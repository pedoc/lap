//! Chat adapters for existing jobs and editable catalogue suggestions.
use super::{
    chat_tools::{ToolContext, ToolOutput, tool},
    config::Capability,
    store::{self, AnalysisText, MediaSource, sql_error},
    workflow::{self, JobOptions, JobView},
};
use genai::chat::{Tool, ToolCall};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

pub(super) fn definitions() -> Vec<Tool> {
    let ids =
        json!({"type":"array","items":{"type":"integer","minimum":1},"minItems":1,"maxItems":1000});
    vec![
        tool(
            "start_media_task",
            "仅在用户需要后台媒体理解或向量索引时启动任务，不修改标题、说明或标签。operations可组合understand/index_text/index_visual。没有有效分析时先understand再index_text。聊天中已有的标题/说明/标签建议直接用propose_metadata提交，不需要先启动理解任务。返回后台任务ID，不等待GPU任务完成。",
            json!({"type":"object","properties":{"file_ids":ids,"operations":{"type":"array","items":{"type":"string","enum":["understand","index_text","index_visual"]},"minItems":1,"maxItems":3}},"required":["file_ids","operations"],"additionalProperties":false}),
        ),
        tool(
            "get_media_task",
            "读取当前图库中已知媒体AI任务的状态、完成/失败数量和有限失败项。后台进度由应用展示，不要高频轮询。",
            json!({"type":"object","properties":{"task_id":{"type":"string","maxLength":128}},"required":["task_id"],"additionalProperties":false}),
        ),
        tool(
            "propose_metadata",
            "将聊天生成或用户指定的标题/说明/标签提交为可编辑建议预览，不直接写回。请在proposals中为每个file_id提供fields内所有字段的值，无需已有媒体理解记录或额外任务；已有分析仅在未提供完整建议时作为可选来源。缺少建议返回proposal_missing，应根据本轮对话、画面或资料补齐proposals。用户在应用中确认后才会采用，收到pending_confirmation不能声称已写入。",
            json!({"type":"object","properties":{"file_ids":{"type":"array","items":{"type":"integer","minimum":1},"minItems":1,"maxItems":16},"fields":{"type":"array","items":{"type":"string","enum":["title","description","tags"]},"minItems":1,"maxItems":3},"proposals":{"type":"array","maxItems":16,"items":{"type":"object","properties":{"file_id":{"type":"integer","minimum":1},"title":{"type":"string","maxLength":256},"description":{"type":"string","maxLength":8000},"tags":{"type":"array","maxItems":20,"items":{"type":"string","maxLength":64}}},"required":["file_id"],"additionalProperties":false}}},"required":["file_ids","fields"],"additionalProperties":false}),
        ),
    ]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartArgs {
    file_ids: Vec<i64>,
    operations: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskArgs {
    task_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalArgs {
    file_ids: Vec<i64>,
    fields: Vec<String>,
    #[serde(default)]
    proposals: Vec<ProposalEdit>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalEdit {
    file_id: i64,
    title: Option<String>,
    description: Option<String>,
    tags: Option<Vec<String>>,
}

fn parse<T: serde::de::DeserializeOwned>(call: &ToolCall) -> Result<T, String> {
    serde_json::from_value(call.fn_arguments.clone())
        .map_err(|_| "工具参数格式无效或含未知字段".into())
}

fn sources_on(
    c: &Connection,
    ids: &[i64],
    scope: Option<&[i64]>,
    limit: usize,
) -> Result<Vec<MediaSource>, String> {
    if ids.is_empty() || ids.len() > limit || ids.iter().any(|id| *id <= 0) {
        return Err(format!("请明确提供 1–{limit} 个有效媒体 ID"));
    }
    let ids: BTreeSet<_> = ids.iter().copied().collect();
    if scope.is_some_and(|allowed| ids.iter().any(|id| !allowed.contains(id))) {
        return Err("工具请求超出本轮允许的媒体范围".into());
    }
    ids.into_iter().map(|id| store::source(c, id)).collect()
}

fn job_options(
    args: &StartArgs,
    sources: &[MediaSource],
    ctx: &ToolContext,
) -> Result<JobOptions, String> {
    if args.operations.is_empty()
        || args.operations.len() > 3
        || args
            .operations
            .iter()
            .any(|op| !["understand", "index_text", "index_visual"].contains(&op.as_str()))
    {
        return Err("任务类型仅支持 understand、index_text、index_visual".into());
    }
    let understand = args.operations.iter().any(|op| op == "understand");
    let text = args.operations.iter().any(|op| op == "index_text");
    let visual = args.operations.iter().any(|op| op == "index_visual");
    let can_use_chat_model = sources.iter().all(|src| {
        ctx.model.supports(if src.kind == 2 {
            Capability::VideoFrames
        } else {
            Capability::Image
        })
    });
    if understand || visual {
        for source in sources {
            source.verify().map_err(|_| {
                format!(
                    "媒体 {} 的原文件不可用或已变化，请下载或重新扫描后重试。",
                    source.file_id
                )
            })?;
        }
    }
    Ok(JobOptions {
        model_id: (understand && can_use_chat_model).then(|| ctx.model.model.id.clone()),
        index_only: !understand && text,
        index: text,
        visual_index: visual,
        visual_only: !understand && !text && visual,
        ..Default::default()
    })
}

fn task_projection(view: &JobView) -> Value {
    json!({"id":view.id,"kind":"media_ai","state":view.state,"total":view.total,"completed":view.completed,"failed":view.failed,"pending":view.pending,"createdAt":view.created_at,
        "items":view.items.iter().map(|item| json!({"fileId":item.file_id,"state":item.state,"message":if item.message.is_empty(){""}else{"处理详情请在任务面板查看"}})).collect::<Vec<_>>()})
}

fn task_on(c: &Connection, id: &str, scope: Option<&[i64]>) -> Result<JobView, String> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err("任务 ID 无效".into());
    }
    let exists: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM media_ai_job_items WHERE job_id=?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if !exists {
        return Err("任务不可用或不属于当前图库".into());
    }
    let unavailable: bool = c.query_row(&format!("SELECT EXISTS(SELECT 1 FROM media_ai_job_items i WHERE i.job_id=?1 AND NOT EXISTS(SELECT 1 {} AND a.id=i.file_id))", store::point_base()), [id], |row| row.get(0)).map_err(sql_error)?;
    if unavailable {
        return Err("任务包含当前不可访问的媒体".into());
    }
    if let Some(scope) = scope {
        let allowed: BTreeSet<_> = scope.iter().copied().collect();
        let mut query = c
            .prepare("SELECT file_id FROM media_ai_job_items WHERE job_id=?1")
            .map_err(sql_error)?;
        for row in query
            .query_map([id], |row| row.get::<_, i64>(0))
            .map_err(sql_error)?
        {
            if !allowed.contains(&row.map_err(sql_error)?) {
                return Err("任务超出本轮允许的媒体范围".into());
            }
        }
    }
    workflow::job_view(c, id)
}

fn proposal_on(
    c: &Connection,
    args: &ProposalArgs,
    ctx: &ToolContext,
    call_id: &str,
) -> Result<ToolOutput, String> {
    let sources = sources_on(c, &args.file_ids, ctx.scope_file_ids.as_deref(), 16)?;
    if args.fields.is_empty()
        || args.fields.len() > 3
        || args
            .fields
            .iter()
            .any(|field| !["title", "description", "tags"].contains(&field.as_str()))
        || args.proposals.len() > 16
    {
        return Err("建议字段仅支持 title、description、tags".into());
    }
    let mut seen = BTreeSet::new();
    for edit in &args.proposals {
        if !args.file_ids.contains(&edit.file_id) || !seen.insert(edit.file_id) {
            return Err("编辑建议含范围外或重复的媒体".into());
        }
        if edit.title.is_some() && !args.fields.iter().any(|f| f == "title")
            || edit.description.is_some() && !args.fields.iter().any(|f| f == "description")
            || edit.tags.is_some() && !args.fields.iter().any(|f| f == "tags")
        {
            return Err("编辑建议超出本次指定的字段".into());
        }
    }
    let mut files = Vec::new();
    let mut missing = Vec::new();
    let selected = store::Adoption {
        title: args.fields.iter().any(|field| field == "title"),
        description: args.fields.iter().any(|field| field == "description"),
        tags: args.fields.iter().any(|field| field == "tags"),
        ..Default::default()
    };
    for source in sources {
        let edit = args
            .proposals
            .iter()
            .find(|edit| edit.file_id == source.file_id);
        let direct = edit.is_some_and(|edit| {
            (!selected.title || edit.title.is_some())
                && (!selected.description || edit.description.is_some())
                && (!selected.tags || edit.tags.is_some())
        });
        let analyses = if direct {
            vec![]
        } else {
            store::analyses(c, &source)?
        };
        let analysis = analyses.first();
        if !direct && analysis.is_none() {
            missing.push(source.file_id);
            continue;
        }
        let mut proposal = analysis
            .map(|analysis| analysis.text.clone())
            .unwrap_or(AnalysisText {
                title: String::new(),
                description: String::new(),
                tags: vec![],
            });
        if let Some(edit) = edit {
            if let Some(value) = &edit.title {
                proposal.title = value.clone();
            }
            if let Some(value) = &edit.description {
                proposal.description = value.clone();
            }
            if let Some(value) = &edit.tags {
                proposal.tags = value.clone();
            }
        }
        let proposal = if direct {
            store::validate_proposal(&proposal, &selected)?
        } else {
            AnalysisText::parse(&serde_json::to_string(&proposal).map_err(|_| "建议无法编码")?)?
        };
        let name = Path::new(&source.path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "媒体".into());
        files.push(json!({"fileId":source.file_id,"name":name,"fileType":source.kind,"width":source.width,"height":source.height,"duration":source.duration,"source":source,"analysisId":analysis.map(|analysis| &analysis.id),"proposal":proposal,
            "analyses":analyses.iter().take(1).map(|analysis|json!({"id":analysis.id,"segment":analysis.segment,"start":analysis.start,"end":analysis.end,"title":analysis.text.title,"description":analysis.text.description,"tags":analysis.text.tags,"dismissed":analysis.dismissed,"createdAt":analysis.created_at})).collect::<Vec<_>>() }));
    }
    if !missing.is_empty() {
        return Ok(ToolOutput {
            json: json!({"status":"proposal_missing","fileIds":missing,"fields":args.fields,"message":"请在proposals中为每项媒体补齐fields指定的建议内容，可直接使用本轮对话、画面和已提供资料，无需先启动媒体理解任务。"}),
            references: vec![],
            parts: vec![],
            local: None,
        });
    }
    let id = format!("{}:{call_id}", ctx.request_id);
    let fields = json!({"title":args.fields.iter().any(|field|field=="title"),"description":args.fields.iter().any(|field|field=="description"),"tags":args.fields.iter().any(|field|field=="tags")});
    Ok(ToolOutput {
        json: json!({"status":"pending_confirmation","proposal":{"id":id,"count":files.len(),"fields":fields},"message":"建议已准备，用户在应用中预览并确认后才会写入。"}),
        references: vec![],
        parts: vec![],
        local: Some(json!({"proposal":{"id":id,"fields":fields,"files":files}})),
    })
}

pub(super) async fn execute(
    library: &str,
    call: &ToolCall,
    ctx: &ToolContext,
) -> Result<ToolOutput, String> {
    match call.fn_name.as_str() {
        "start_media_task" => {
            let args: StartArgs = parse(call)?;
            let sources = store::db(library, |c| {
                sources_on(c, &args.file_ids, ctx.scope_file_ids.as_deref(), 1000)
            })?;
            let options = job_options(&args, &sources, ctx)?;
            if options.index_only {
                let missing = store::db(library, |c| {
                    sources.iter().try_fold(Vec::new(), |mut missing, source| {
                        if store::analyses(c, source)?.is_empty() {
                            missing.push(source.file_id);
                        }
                        Ok::<_, String>(missing)
                    })
                })?;
                if !missing.is_empty() {
                    return Ok(ToolOutput {
                        json: json!({"status":"analysis_missing","fileIds":missing,"message":"文本索引需要有效分析；请同时选择 understand 与 index_text。"}),
                        references: vec![],
                        parts: vec![],
                        local: None,
                    });
                }
            }
            let app = ctx.app.clone().ok_or("当前窗口无法启动媒体任务")?;
            let id = workflow::media_ai_start_job(
                app,
                library.into(),
                Some(sources.iter().map(|src| src.file_id).collect()),
                options,
            )
            .await?;
            let view = store::db(library, |c| task_on(c, &id, ctx.scope_file_ids.as_deref()))?;
            Ok(ToolOutput {
                json: json!({"status":"started","task":task_projection(&view)}),
                references: vec![],
                parts: vec![],
                local: Some(json!({"task":view})),
            })
        }
        "get_media_task" => {
            let args: TaskArgs = parse(call)?;
            let view = store::db(library, |c| {
                task_on(c, &args.task_id, ctx.scope_file_ids.as_deref())
            })?;
            Ok(ToolOutput {
                json: json!({"status":"ok","task":task_projection(&view)}),
                references: vec![],
                parts: vec![],
                local: Some(json!({"task":view})),
            })
        }
        "propose_metadata" => {
            let args: ProposalArgs = parse(call)?;
            store::db(library, |c| proposal_on(c, &args, ctx, &call.call_id))
        }
        _ => Err("未知的媒体工作流工具".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> ToolContext {
        ToolContext {
            library_id: "library".into(),
            model: store::tests::model("chat"),
            scope_file_ids: Some(vec![1]),
            app: None,
            request_id: "request".into(),
            prefetch_fields: vec!["file".into(), "analysis".into()],
        }
    }
    #[test]
    fn job_operations_do_not_adopt_and_embedding_only_does_not_require_original_pixels() {
        let (c, _dir) = store::tests::fixture();
        let mut source = store::source(&c, 1).unwrap();
        source.cloud_only = true;
        let ctx = context();
        let opts = job_options(
            &StartArgs {
                file_ids: vec![1],
                operations: vec!["index_text".into()],
            },
            &[source.clone()],
            &ctx,
        )
        .unwrap();
        assert!(opts.index_only && opts.index && !opts.visual_only);
        assert!(!opts.adopt.title && !opts.adopt.description && !opts.adopt.tags);
        assert!(
            job_options(
                &StartArgs {
                    file_ids: vec![1],
                    operations: vec!["understand".into()]
                },
                &[source],
                &ctx
            )
            .is_err()
        );
        assert!(
            job_options(
                &StartArgs {
                    file_ids: vec![1],
                    operations: vec!["delete".into()]
                },
                &[],
                &ctx
            )
            .is_err()
        );
    }
    #[test]
    fn proposals_are_editable_local_snapshots_and_only_user_adoption_changes_catalogue() {
        let (mut c, _dir) = store::tests::fixture();
        let source = store::source(&c, 1).unwrap();
        let text = AnalysisText {
            title: "已有建议".into(),
            description: "已有说明".into(),
            tags: vec!["湖泊".into()],
        };
        store::put_analysis(
            &mut c,
            &source,
            0,
            0.,
            0.,
            text,
            &store::tests::model("analysis"),
        )
        .unwrap();
        let args = ProposalArgs {
            file_ids: vec![1],
            fields: vec!["title".into()],
            proposals: vec![ProposalEdit {
                file_id: 1,
                title: Some("编辑后的标题".into()),
                description: None,
                tags: None,
            }],
        };
        let out = proposal_on(&c, &args, &context(), "call-1").unwrap();
        assert_eq!(out.json["status"], "pending_confirmation");
        assert!(!out.json.to_string().contains(&source.path));
        assert!(!out.json.to_string().contains("编辑后的标题"));
        let local = out.local.unwrap();
        assert_eq!(
            local["proposal"]["files"][0]["proposal"]["title"],
            "编辑后的标题"
        );
        assert_eq!(store::source(&c, 1).unwrap().title, source.title);
        let proposal =
            serde_json::from_value(local["proposal"]["files"][0]["proposal"].clone()).unwrap();
        assert!(local["proposal"]["files"][0]["analysisId"].is_null());
        c.execute("UPDATE afiles SET title='人工更新' WHERE id=1", [])
            .unwrap();
        assert!(
            store::adopt_proposal(
                &mut c,
                &source,
                &store::Adoption {
                    proposal: Some(proposal),
                    title: true,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(
            store::source(&c, 1).unwrap().title.as_deref(),
            Some("人工更新")
        );
    }
    #[test]
    fn explicit_chat_proposals_do_not_need_analysis_or_start_background_jobs() {
        let (mut c, _dir) = store::tests::fixture();
        let source = store::source(&c, 1).unwrap();
        let args = ProposalArgs {
            file_ids: vec![1],
            fields: vec!["title".into(), "tags".into()],
            proposals: vec![ProposalEdit {
                file_id: 1,
                title: Some("七个人，一段青春".into()),
                description: None,
                tags: Some(vec!["集体合影".into(), "青春纪念".into()]),
            }],
        };
        let out = proposal_on(&c, &args, &context(), "chat-direct").unwrap();
        assert_eq!(out.json["status"], "pending_confirmation");
        let local = out.local.unwrap();
        let file = &local["proposal"]["files"][0];
        assert!(file["analysisId"].is_null());
        assert_eq!(file["proposal"]["description"], "");
        assert_eq!(store::source(&c, 1).unwrap(), source);
        assert!(store::analyses(&c, &source).unwrap().is_empty());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM media_ai_jobs", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        store::adopt_proposal(
            &mut c,
            &source,
            &store::Adoption {
                title: true,
                tags: true,
                create_tags: true,
                proposal: Some(serde_json::from_value(file["proposal"].clone()).unwrap()),
                ..Default::default()
            },
        )
        .unwrap();
        let current = store::source(&c, 1).unwrap();
        assert_eq!(current.title.as_deref(), Some("七个人，一段青春"));
        assert_eq!(current.comments, source.comments);
        assert_eq!(current.tag_ids.len(), 2);
        assert!(store::analyses(&c, &current).unwrap().is_empty());
    }
    #[test]
    fn analysis_is_an_optional_fallback_and_incomplete_chat_values_request_content() {
        let (mut c, _dir) = store::tests::fixture();
        let source = store::source(&c, 1).unwrap();
        let mut args = ProposalArgs {
            file_ids: vec![1],
            fields: vec!["title".into(), "tags".into()],
            proposals: vec![ProposalEdit {
                file_id: 1,
                title: Some("聊天标题".into()),
                description: None,
                tags: None,
            }],
        };
        let missing = proposal_on(&c, &args, &context(), "partial").unwrap();
        assert_eq!(missing.json["status"], "proposal_missing");
        assert!(
            missing.json["message"]
                .as_str()
                .unwrap()
                .contains("无需先启动媒体理解")
        );
        store::put_analysis(
            &mut c,
            &source,
            0,
            0.,
            0.,
            AnalysisText {
                title: "旧标题".into(),
                description: "已有说明".into(),
                tags: vec!["湖泊".into()],
            },
            &store::tests::model("analysis"),
        )
        .unwrap();
        let out = proposal_on(&c, &args, &context(), "fallback").unwrap();
        let file = &out.local.as_ref().unwrap()["proposal"]["files"][0];
        assert!(file["analysisId"].as_str().is_some());
        assert_eq!(file["proposal"]["title"], "聊天标题");
        assert_eq!(file["proposal"]["tags"], json!(["湖泊"]));
        args.proposals[0].tags = Some(vec!["新建议".into()]);
        let direct = proposal_on(&c, &args, &context(), "complete").unwrap();
        assert!(direct.local.unwrap()["proposal"]["files"][0]["analysisId"].is_null());
    }
    #[test]
    fn proposals_request_explicit_values_without_requiring_analysis_and_reject_scope_and_unrequested_fields()
     {
        let (c, _dir) = store::tests::fixture();
        let mut args = ProposalArgs {
            file_ids: vec![1],
            fields: vec!["tags".into()],
            proposals: vec![],
        };
        assert_eq!(
            proposal_on(&c, &args, &context(), "call").unwrap().json["status"],
            "proposal_missing"
        );
        args.proposals.push(ProposalEdit {
            file_id: 1,
            title: Some("outside fields".into()),
            description: None,
            tags: None,
        });
        assert!(proposal_on(&c, &args, &context(), "call").is_err());
        args.proposals.clear();
        args.file_ids = vec![2];
        assert!(proposal_on(&c, &args, &context(), "call").is_err());
        assert!(sources_on(&c, &[], None, 16).is_err());
    }
    #[test]
    fn task_details_check_scope_and_never_send_paths_in_errors() {
        let (c, _dir) = store::tests::fixture();
        let id = uuid::Uuid::new_v4().to_string();
        c.execute("INSERT INTO media_ai_jobs(id,state,snapshot,created_at,message) VALUES(?1,'paused','{}',1,'C:/private/original.jpg failed')",[&id]).unwrap();
        c.execute("INSERT INTO media_ai_job_items(job_id,file_id,source,state,message) VALUES(?1,1,'{}','failed','C:/private/original.jpg failed')",[&id]).unwrap();
        let view = task_on(&c, &id, Some(&[1])).unwrap();
        assert_eq!(view.failed, 1);
        assert!(!task_projection(&view).to_string().contains("C:/private"));
        assert!(task_on(&c, &id, Some(&[2])).is_err());
    }
}
