use super::{ToolContext, ToolOutput, check_scope, limited, reference_on, tool};
use crate::ai::{
    media,
    services::{client, config, store, workflow},
};
use chrono::{Local, NaiveDate, TimeZone};
use genai::chat::Tool;
use rusqlite::{Connection, params_from_iter, types::Value as SqlValue};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn definition() -> Tool {
    tool(
        "search_media",
        "Search the allowed visible media scope. Use structured for exact catalogue conditions, semantic for scene meaning, or combined for semantic ranking inside exact filters. Analysis and media indexes are separate. Positive limit is mandatory; missing indexes are not no-match results. Ambiguous person/tag names return candidates for clarification.",
        json!({"type":"object","properties":{
        "query":{"type":"string","maxLength":2000},"mode":{"type":"string","enum":["structured","semantic","combined"]},"source":{"type":"string","enum":["analysis","media"]},"file_name":{"type":"string","maxLength":256},"person_ids":{"type":"array","items":{"type":"integer","minimum":1},"maxItems":8},"person_names":{"type":"array","items":{"type":"string"},"maxItems":8},"tag_ids":{"type":"array","items":{"type":"integer","minimum":1},"maxItems":8},"tag_names":{"type":"array","items":{"type":"string"},"maxItems":8},"date_from":{"type":"string","description":"Inclusive local calendar date YYYY-MM-DD; defaults to taken_date capture time."},"date_to":{"type":"string","description":"Inclusive local calendar date YYYY-MM-DD for the chosen date_field."},"date_field":{"type":"string","enum":["taken","created","modified"],"description":"taken=capture date (default), created=file creation date, modified=file modification date; not import time."},"album_id":{"type":"integer","minimum":1},"file_type":{"type":"integer","minimum":0,"maximum":7,"description":"Type bit mask: image 1, video 2, RAW 4; zero means all."},"limit":{"type":"integer","minimum":1,"maximum":50},"offset":{"type":"integer","minimum":0,"maximum":199}
    },"required":["limit"],"additionalProperties":false}),
    )
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Args {
    query: String,
    mode: String,
    source: String,
    file_name: String,
    person_ids: Vec<i64>,
    person_names: Vec<String>,
    tag_ids: Vec<i64>,
    tag_names: Vec<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    date_field: String,
    album_id: Option<i64>,
    file_type: i64,
    limit: i64,
    offset: usize,
}
struct Filter {
    condition: String,
    values: Vec<SqlValue>,
}
fn like(text: &str) -> String {
    format!(
        "%{}%",
        text.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}
fn scope(ctx: &ToolContext) -> Result<Filter, String> {
    let mut out = Filter {
        condition: "1".into(),
        values: Vec::new(),
    };
    if let Some(ids) = &ctx.scope_file_ids {
        if ids.iter().any(|id| *id <= 0) {
            return Err("scope_denied: 本轮媒体范围无效。".into());
        }
        out.condition = "a.id IN (SELECT CAST(value AS INTEGER) FROM json_each(?))".into();
        out.values.push(SqlValue::Text(
            serde_json::to_string(ids).map_err(|_| "范围编码失败")?,
        ));
    }
    Ok(out)
}
fn add(filter: &mut Filter, condition: &str, values: impl IntoIterator<Item = SqlValue>) {
    filter.condition.push_str(" AND (");
    filter.condition.push_str(condition);
    filter.condition.push(')');
    filter.values.extend(values)
}
fn date(value: &str, next: bool) -> Result<i64, String> {
    let mut day =
        NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| "日期必须为YYYY-MM-DD")?;
    if next {
        day = day.succ_opt().ok_or("日期超出范围")?;
    }
    Local
        .from_local_datetime(&day.and_hms_opt(0, 0, 0).ok_or("日期无效")?)
        .earliest()
        .map(|d| d.timestamp())
        .ok_or_else(|| "当地日期无效".into())
}
fn people_condition(ids: &str) -> String {
    format!(
        "EXISTS(SELECT 1 FROM ({}) person_link JOIN persons visible_person ON visible_person.id=person_link.person_id WHERE person_link.file_id=a.id AND person_link.person_id IN ({ids}) AND COALESCE(visible_person.hidden,0)=0)",
        crate::t_search::person_links("?")
    )
}
fn resolve_names(
    c: &Connection,
    names: &[String],
    people: bool,
    ctx: &ToolContext,
    face_profile: &str,
) -> Result<Result<Vec<i64>, ToolOutput>, String> {
    let mut ids = Vec::new();
    for name in names {
        if name.trim().is_empty() || name.chars().count() > 128 {
            return Err("名称为空或过长".into());
        }
        let restriction = scope(ctx)?;
        let (from, association, hidden) = if people {
            (
                "persons p",
                people_condition("p.id"),
                " AND COALESCE(p.hidden,0)=0",
            )
        } else {
            (
                "atags p",
                "EXISTS(SELECT 1 FROM afile_tags ft WHERE ft.file_id=a.id AND ft.tag_id=p.id)"
                    .into(),
                "",
            )
        };
        let sql = format!(
            "SELECT p.id,p.name FROM {from} WHERE p.name=? COLLATE NOCASE{hidden} AND EXISTS(SELECT 1 {} AND ({}) AND ({association})) ORDER BY p.id LIMIT 6",
            store::point_base(),
            restriction.condition
        );
        let mut values = vec![SqlValue::Text(name.clone())];
        values.extend(restriction.values);
        if people {
            values.push(SqlValue::Text(face_profile.into()));
        }
        let mut statement = c.prepare(&sql).map_err(store::sql_error)?;
        let choices = statement
            .query_map(params_from_iter(values), |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(store::sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(store::sql_error)?;
        if choices.len() != 1 {
            return Ok(Err(ToolOutput::value(
                json!({"status":if choices.is_empty(){"not_found"}else{"ambiguous"},"entity":if people{"person"}else{"tag"},"name":name,"choices":choices.into_iter().map(|(id,name)|json!({"id":id,"name":limited(&name,128)})).collect::<Vec<_>>()}),
            )));
        }
        ids.push(choices[0].0);
    }
    Ok(Ok(ids))
}
fn filters(
    c: &Connection,
    args: &Args,
    ctx: &ToolContext,
) -> Result<Result<Filter, ToolOutput>, String> {
    filters_on(c, args, ctx, &crate::t_search::Profiles::current().face)
}
fn filters_on(
    c: &Connection,
    args: &Args,
    ctx: &ToolContext,
    face_profile: &str,
) -> Result<Result<Filter, ToolOutput>, String> {
    let mut filter = scope(ctx)?;
    if let Some(id) = args.album_id {
        if id <= 0 {
            return Err("相册 ID 无效".into());
        }
        add(&mut filter, "b.album_id=?", [SqlValue::Integer(id)]);
    }
    if !(0..=7).contains(&args.file_type) {
        return Err("媒体类型无效".into());
    }
    if args.file_type > 0 {
        add(
            &mut filter,
            "(? & CASE a.file_type WHEN 1 THEN 1 WHEN 2 THEN 2 WHEN 3 THEN 4 ELSE 0 END)<>0",
            [SqlValue::Integer(args.file_type)],
        );
    }
    if !args.file_name.is_empty() {
        add(
            &mut filter,
            "a.name LIKE ? ESCAPE '\\' COLLATE NOCASE",
            [SqlValue::Text(like(&args.file_name))],
        );
    }
    if args.mode == "structured" && !args.query.trim().is_empty() {
        let term = SqlValue::Text(like(&args.query));
        add(
            &mut filter,
            "(a.name LIKE ? ESCAPE '\\' COLLATE NOCASE OR COALESCE(a.title,'') LIKE ? ESCAPE '\\' COLLATE NOCASE OR COALESCE(a.comments,'') LIKE ? ESCAPE '\\' COLLATE NOCASE)",
            [term.clone(), term.clone(), term],
        );
    }
    let date_column = match args.date_field.as_str() {
        "" | "taken" => "a.taken_date",
        "created" => "a.created_at",
        "modified" => "a.modified_at",
        _ => return Err("日期字段只能是taken/created/modified。".into()),
    };
    if let Some(value) = &args.date_from {
        add(
            &mut filter,
            &format!("{date_column}>=?"),
            [SqlValue::Integer(date(value, false)?)],
        );
    }
    if let Some(value) = &args.date_to {
        add(
            &mut filter,
            &format!("{date_column}<?"),
            [SqlValue::Integer(date(value, true)?)],
        );
    }
    if let (Some(from), Some(to)) = (&args.date_from, &args.date_to) {
        if from > to {
            return Err("日期范围倒置".into());
        }
    }
    let mut people = args.person_ids.clone();
    match resolve_names(c, &args.person_names, true, ctx, face_profile)? {
        Ok(ids) => people.extend(ids),
        Err(output) => return Ok(Err(output)),
    }
    let mut tags = args.tag_ids.clone();
    match resolve_names(c, &args.tag_names, false, ctx, face_profile)? {
        Ok(ids) => tags.extend(ids),
        Err(output) => return Ok(Err(output)),
    }
    if people.len() > 8 || tags.len() > 8 || people.iter().chain(&tags).any(|id| *id <= 0) {
        return Err("人物或标签 ID 数量无效".into());
    }
    for id in people {
        let literal = id.to_string();
        add(
            &mut filter,
            &people_condition(&literal),
            [SqlValue::Text(face_profile.into())],
        );
    }
    for id in tags {
        add(
            &mut filter,
            "EXISTS(SELECT 1 FROM afile_tags ft WHERE ft.file_id=a.id AND ft.tag_id=?)",
            [SqlValue::Integer(id)],
        );
    }
    Ok(Ok(filter))
}
fn row_sql(source: &str, filter: &Filter) -> String {
    let valid = "json_extract(v.source,'$.kind')=a.file_type AND json_extract(v.source,'$.path')=b.path||'/'||a.name AND json_extract(v.source,'$.size')=a.size AND json_extract(v.source,'$.modified') IS a.modified_at AND json_extract(v.source,'$.orientation')=COALESCE(a.e_orientation,1)";
    if source == "media" {
        format!(
            "SELECT a.id AS file_id,v.id AS evidence_id,COALESCE(a.title,a.name),COALESCE(a.comments,''),v.start,v.end,v.vector FROM media_ai_visual_vectors v JOIN afiles a ON a.id=v.file_id JOIN afolders b ON b.id=a.folder_id WHERE v.profile_id=? AND a.file_type IN(1,2,3) AND {} AND ({}) AND {valid}",
            media::visible(),
            filter.condition
        )
    } else {
        let valid = valid.replace("v.source", "x.source");
        format!(
            "SELECT a.id AS file_id,x.id AS evidence_id,x.title,x.description,x.start,x.end,v.vector FROM media_ai_vectors v JOIN media_ai_analyses x ON x.id=v.analysis_id JOIN afiles a ON a.id=x.file_id JOIN afolders b ON b.id=a.folder_id WHERE v.profile_id=? AND a.file_type IN(1,2,3) AND {} AND ({}) AND {valid} AND NOT EXISTS(SELECT 1 FROM media_ai_analyses newer WHERE newer.file_id=x.file_id AND newer.stamp=x.stamp AND newer.segment=x.segment AND newer.created_at>x.created_at)",
            media::visible(),
            filter.condition
        )
    }
}
fn profile(model: &config::Resolved, source: &str) -> Result<String, String> {
    if source == "media" {
        model.multimodal_profile()
    } else {
        model.embedding_profile()
    }
}
fn default_source(configuration: &config::Configuration) -> &'static str {
    if configuration.role("embedding").is_ok() {
        "analysis"
    } else {
        "media"
    }
}
fn ranked(
    c: &Connection,
    source: &str,
    model: &config::Resolved,
    filter: &Filter,
    query: &[f32],
    limit: usize,
) -> Result<Vec<store::Hit>, String> {
    let mut values = vec![SqlValue::Text(profile(model, source)?)];
    values.extend(filter.values.clone());
    let mut statement = c
        .prepare(&row_sql(source, filter))
        .map_err(store::sql_error)?;
    let rows = statement
        .query_map(params_from_iter(values), |r| {
            Ok((
                store::Hit {
                    file_id: r.get(0)?,
                    analysis_id: r.get(1)?,
                    title: r.get(2)?,
                    description: r.get(3)?,
                    start: r.get(4)?,
                    end: r.get(5)?,
                    score: 0.0,
                },
                r.get::<_, Vec<u8>>(6)?,
            ))
        })
        .map_err(store::sql_error)?;
    store::rank_vectors(rows, query, limit, store::SearchUnit::Segments)
}
fn hit_current(
    c: &Connection,
    source: &str,
    semantic: bool,
    filter: &Filter,
    hit: &store::Hit,
    src: &store::MediaSource,
) -> Result<bool, String> {
    let mut values = vec![SqlValue::Integer(hit.file_id)];
    values.extend(filter.values.clone());
    let matches: bool = c
        .query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 {} AND a.id=? AND ({}))",
                store::point_base(),
                filter.condition
            ),
            params_from_iter(values),
            |r| r.get(0),
        )
        .map_err(store::sql_error)?;
    if !matches || !semantic {
        return Ok(matches);
    }
    if source == "analysis" {
        Ok(store::analyses(c, src)?
            .iter()
            .any(|a| a.id == hit.analysis_id))
    } else {
        c.query_row("SELECT EXISTS(SELECT 1 FROM media_ai_visual_vectors WHERE id=?1 AND file_id=?2 AND stamp=?3)",rusqlite::params![hit.analysis_id,hit.file_id,src.stamp()],|r|r.get(0)).map_err(store::sql_error)
    }
}
fn parse_args(value: &Value) -> Result<Args, String> {
    let mut args: Args =
        serde_json::from_value(value.clone()).map_err(|_| "search_media 参数无效".to_string())?;
    if args.limit < 1
        || args.limit > 50
        || args.offset >= 200
        || args.offset + args.limit as usize > 200
    {
        return Err("检索必须使用1–50的正数limit，且分页窗口不超过200项。".into());
    }
    if args.mode.is_empty() {
        args.mode = if args.query.trim().is_empty() {
            "structured"
        } else {
            "semantic"
        }
        .into();
    }
    if !["structured", "semantic", "combined"].contains(&args.mode.as_str())
        || !["", "analysis", "media"].contains(&args.source.as_str())
        || args.query.chars().count() > 2000
    {
        return Err("检索方式、索引来源或查询内容无效".into());
    }
    if args.mode != "structured" && args.query.trim().is_empty() {
        return Err("语义检索需要非空query。".into());
    }
    if args.file_name.chars().count() > 256
        || args.person_ids.len() > 8
        || args.person_names.len() > 8
        || args.tag_ids.len() > 8
        || args.tag_names.len() > 8
    {
        return Err("名称条件过长或条件数量超过8项。".into());
    }
    Ok(args)
}
pub(super) async fn execute(
    library: &str,
    value: &Value,
    ctx: &ToolContext,
) -> Result<ToolOutput, String> {
    let mut args = parse_args(value)?;
    let configuration = if args.mode != "structured" {
        Some(config::load()?)
    } else {
        None
    };
    if args.source.is_empty() {
        args.source = configuration
            .as_ref()
            .map(default_source)
            .unwrap_or("analysis")
            .into();
    }
    let filter = match store::db(library, |c| filters(c, &args, ctx))? {
        Ok(filter) => filter,
        Err(output) => return Ok(output),
    };
    let end = args.offset + args.limit as usize;
    let (hits, coverage) = if args.mode == "structured" {
        let hits = store::db(library, |c| {
            let mut values = filter.values.clone();
            values.push(SqlValue::Integer((end + 1) as i64));
            let mut s=c.prepare(&format!("SELECT a.id,a.name,COALESCE(a.title,''),COALESCE(a.comments,'') {} AND ({}) ORDER BY COALESCE(a.taken_date,0) DESC,a.id LIMIT ?",store::point_base(),filter.condition)).map_err(store::sql_error)?;
            s.query_map(params_from_iter(values), |r| {
                Ok(store::Hit {
                    file_id: r.get(0)?,
                    analysis_id: String::new(),
                    title: r.get(2)?,
                    description: r.get(3)?,
                    start: 0.0,
                    end: 0.0,
                    score: 0.0,
                })
            })
            .map_err(store::sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(store::sql_error)
        })?;
        (hits, Value::Null)
    } else {
        let configuration = configuration.as_ref().ok_or("缺少语义检索配置")?;
        let model = configuration.role(if args.source == "media" {
            "multimodalEmbedding"
        } else {
            "embedding"
        })?;
        let capability = if args.source == "media" {
            config::Capability::EmbedMultimodal
        } else {
            config::Capability::EmbedText
        };
        workflow::check_policy(&configuration, &model, capability, false)?;
        let coverage = store::db(library, |c| {
            let total: i64 = c
                .query_row(
                    &format!(
                        "SELECT COUNT(*) {} AND ({})",
                        store::point_base(),
                        filter.condition
                    ),
                    params_from_iter(filter.values.clone()),
                    |r| r.get(0),
                )
                .map_err(store::sql_error)?;
            let mut values = vec![SqlValue::Text(profile(&model, &args.source)?)];
            values.extend(filter.values.clone());
            let indexed: i64 = c
                .query_row(
                    &format!(
                        "SELECT COUNT(DISTINCT file_id) FROM ({})",
                        row_sql(&args.source, &filter)
                    ),
                    params_from_iter(values),
                    |r| r.get(0),
                )
                .map_err(store::sql_error)?;
            Ok((total, indexed))
        })?;
        if coverage.0 > 0 && coverage.1 == 0 {
            return Ok(ToolOutput::value(
                json!({"status":"index_missing","source":args.source,"items":[],"coverage":{"inScope":coverage.0,"indexed":0},"reason":"当前范围没有与此模型配置匹配的有效索引；这不表示没有匹配媒体。"}),
            ));
        }
        let vector = if coverage.0 == 0 {
            Vec::new()
        } else {
            let _permit = workflow::CALLS
                .acquire()
                .await
                .map_err(|_| "检索队列不可用")?;
            store::db(library, |_| Ok(()))?;
            workflow::check_policy(&config::load()?, &model, capability, false)?;
            if args.source == "media" {
                client::embed_multimodal_query(&model, args.query.clone()).await?
            } else {
                client::embed(&model, args.query.clone(), true).await?
            }
        };
        workflow::check_policy(&config::load()?, &model, capability, false)?;
        let hits = if vector.is_empty() {
            Vec::new()
        } else {
            let ranked_library = library.to_owned();
            let ranked_source = args.source.clone();
            let ranked_model = model.clone();
            let ranked_filter = Filter {
                condition: filter.condition.clone(),
                values: filter.values.clone(),
            };
            tauri::async_runtime::spawn_blocking(move || {
                store::db(&ranked_library, |c| {
                    ranked(
                        c,
                        &ranked_source,
                        &ranked_model,
                        &ranked_filter,
                        &vector,
                        (end + 1).min(200),
                    )
                })
            })
            .await
            .map_err(|_| "向量检索执行失败")??
        };
        workflow::check_policy(&config::load()?, &model, capability, false)?;
        (
            hits,
            json!({"inScope":coverage.0,"indexed":coverage.1,"partial":coverage.1<coverage.0}),
        )
    };
    let has_more = hits.len() > end;
    let capped = end >= 200 && hits.len() >= 200;
    let selected: Vec<_> = hits
        .into_iter()
        .skip(args.offset)
        .take(args.limit as usize)
        .collect();
    let mut refs = Vec::new();
    let mut items = Vec::new();
    store::db(library, |c| {
        for hit in selected {
            check_scope(&[hit.file_id], ctx)?;
            let src = store::source(c, hit.file_id)?;
            if !hit_current(
                c,
                &args.source,
                args.mode != "structured",
                &filter,
                &hit,
                &src,
            )? {
                return Err("检索来源或筛选条件已变化，请重新检索。".into());
            }
            let mut item = json!({"fileId":hit.file_id,"start":hit.start,"end":hit.end});
            if args.mode != "structured" {
                item["score"] = json!(hit.score)
            }
            item["name"] = json!(
                std::path::Path::new(&src.path)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            );
            if args.mode != "structured" && args.source == "analysis" {
                item["title"] = json!(limited(&hit.title, 256));
                item["description"] = json!(limited(&hit.description, 2000));
            }
            refs.push(reference_on(c, src, hit.start, hit.end)?);
            items.push(item);
        }
        Ok(())
    })?;
    Ok(ToolOutput {
        json: json!({"status":"ok","mode":args.mode,"source":args.source,"items":items,"hasMore":has_more,"nextOffset":if has_more{Some(end)}else{None},"truncated":capped,"coverage":coverage}),
        references: refs,
        parts: Vec::new(),
        local: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::services::{
        config::{Capability, EmbeddingProtocol, ProviderKind},
        visual,
    };
    fn args(value: Value) -> Args {
        serde_json::from_value(value).unwrap()
    }
    fn filter(c: &Connection, args: &Args, ctx: &ToolContext) -> Filter {
        match filters_on(c, args, ctx, "face-profile").unwrap() {
            Ok(v) => v,
            Err(_) => panic!("unexpected ambiguity"),
        }
    }
    fn ids(c: &Connection, filter: &Filter) -> Vec<i64> {
        let mut statement = c
            .prepare(&format!(
                "SELECT a.id {} AND ({}) ORDER BY a.id",
                store::point_base(),
                filter.condition
            ))
            .unwrap();
        statement
            .query_map(params_from_iter(filter.values.clone()), |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }
    fn seed(c: &Connection, id: i64, model: &config::Resolved, vector: Vec<f32>) {
        let source = store::source(c, id).unwrap();
        let analysis = store::Analysis {
            id: format!("analysis-{id}"),
            file_id: id,
            stamp: source.stamp(),
            segment: 0,
            start: 0.0,
            end: 0.0,
            text: store::AnalysisText {
                title: "Fixture".into(),
                description: "Synthetic description".into(),
                tags: Vec::new(),
            },
            model: store::tests::model("generator"),
            created_at: 1,
            dismissed: Vec::new(),
            indexed: false,
        };
        c.execute("INSERT OR IGNORE INTO media_ai_analyses(id,file_id,stamp,source,segment,start,end,title,description,tags,model,created_at) VALUES(?1,?2,?3,?4,0,0,0,'Fixture','Synthetic description','[]',?5,1)",rusqlite::params![analysis.id,id,source.stamp(),serde_json::to_string(&source).unwrap(),serde_json::to_string(&analysis.model).unwrap()]).unwrap();
        store::put_vector(c, &analysis, model, vector).unwrap();
    }
    #[test]
    fn semantic_filters_and_scope_apply_before_ranking_and_profiles_never_mix() {
        let (c, _dir) = store::tests::fixture();
        c.execute_batch("ALTER TABLE afiles ADD COLUMN taken_date INTEGER;INSERT INTO afile_tags VALUES(1,1),(2,2);").unwrap();
        let model = store::tests::model("embedding-a");
        let other = store::tests::model("embedding-b");
        seed(&c, 1, &model, vec![0.0, 1.0]);
        seed(&c, 2, &model, vec![1.0, 0.0]);
        seed(&c, 3, &other, vec![1.0, 0.0]);
        let mut ctx = super::super::tests::context();
        ctx.scope_file_ids = None;
        let restricted = filter(
            &c,
            &args(
                json!({"limit":1,"mode":"combined","source":"analysis","query":"lake","tag_ids":[1]}),
            ),
            &ctx,
        );
        let hits = ranked(&c, "analysis", &model, &restricted, &[1.0, 0.0], 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].file_id, 1,
            "global top file 2 cannot hide filtered file 1"
        );
        ctx.scope_file_ids = Some(vec![1]);
        let scoped = filter(
            &c,
            &args(json!({"limit":1,"mode":"semantic","query":"lake"})),
            &ctx,
        );
        assert_eq!(
            ranked(&c, "analysis", &model, &scoped, &[1.0, 0.0], 1).unwrap()[0].file_id,
            1
        );
        ctx.scope_file_ids = None;
        let all = filter(&c, &args(json!({"limit":10})), &ctx);
        assert_eq!(
            ranked(&c, "analysis", &other, &all, &[1.0, 0.0], 10)
                .unwrap()
                .iter()
                .map(|h| h.file_id)
                .collect::<Vec<_>>(),
            vec![3]
        );
        c.execute_batch("INSERT INTO media_states(file_id,hidden) VALUES(1,1);UPDATE afiles SET modified_at=modified_at+1 WHERE id=2;").unwrap();
        assert!(
            ranked(&c, "analysis", &model, &all, &[1.0, 0.0], 10)
                .unwrap()
                .is_empty(),
            "hidden and replaced sources are excluded inside ranking"
        );
    }
    #[test]
    fn original_image_ranking_uses_its_own_profile_and_exact_scope() {
        let (mut c, _dir) = store::tests::fixture();
        let source = store::source(&c, 1).unwrap();
        let mut model = store::tests::model("visual-a");
        model.provider.kind = ProviderKind::Embedding;
        model.provider.embedding_protocol = EmbeddingProtocol::VllmMultimodal;
        model.model.capabilities = [Capability::EmbedText, Capability::EmbedMultimodal]
            .into_iter()
            .collect();
        let mut other = model.clone();
        other.model.remote_id = "visual-b".into();
        visual::put_vector(&mut c, &source, &model, 0, 0.0, 0.0, 0.0, vec![0.0, 1.0]).unwrap();
        visual::put_vector(&mut c, &source, &other, 0, 0.0, 0.0, 0.0, vec![1.0, 0.0]).unwrap();
        let ctx = super::super::tests::context();
        let scoped = filter(&c, &args(json!({"limit":1})), &ctx);
        assert!(
            ranked(&c, "media", &model, &scoped, &[1.0, 0.0], 1).unwrap()[0]
                .score
                .abs()
                < 0.001
        );
        assert!(ranked(&c, "media", &other, &scoped, &[1.0, 0.0], 1).unwrap()[0].score > 0.99);
        let mut none = ctx;
        none.scope_file_ids = Some(vec![2]);
        let scoped = filter(&c, &args(json!({"limit":1})), &none);
        assert!(
            ranked(&c, "media", &model, &scoped, &[1.0, 0.0], 1)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn person_name_resolution_and_filters_share_current_profile_and_manual_precedence() {
        let (c, _dir) = store::tests::fixture();
        c.execute_batch("ALTER TABLE afiles ADD COLUMN taken_date INTEGER;ALTER TABLE afiles ADD COLUMN has_faces INTEGER DEFAULT 1;CREATE TABLE persons(id INTEGER PRIMARY KEY,name TEXT,hidden INTEGER);CREATE TABLE faces(id INTEGER PRIMARY KEY,file_id INTEGER,person_id INTEGER,annotation_id INTEGER);CREATE TABLE ai_index_profiles(task TEXT PRIMARY KEY,fingerprint TEXT);INSERT INTO ai_index_profiles VALUES('face','face-profile');CREATE TABLE face_annotations(id INTEGER PRIMARY KEY,file_id INTEGER,person_id INTEGER,kind TEXT,source_modified_at INTEGER,source_size INTEGER);INSERT INTO persons VALUES(1,'Twin',0),(2,'Twin',0),(3,'Hidden',1),(4,'Stale',0);INSERT INTO faces VALUES(1,1,1,NULL),(2,3,3,NULL);INSERT INTO face_annotations SELECT 1,id,2,'confirmed',modified_at,size FROM afiles WHERE id=2;INSERT INTO face_annotations SELECT 2,id,4,'confirmed',modified_at-1,size FROM afiles WHERE id=3;").unwrap();
        let mut ctx = super::super::tests::context();
        ctx.prefetch_fields.clear();
        let names = vec!["Twin".into()];
        let ambiguous = match resolve_names(&c, &names, true, &ctx, "face-profile").unwrap() {
            Err(output) => output,
            Ok(_) => panic!("expected ambiguity"),
        };
        assert_eq!(ambiguous.json["status"], "ambiguous");
        assert_eq!(ambiguous.json["choices"].as_array().unwrap().len(), 2);
        assert_eq!(
            resolve_names(&c, &names, true, &ctx, "different-profile")
                .unwrap()
                .ok()
                .unwrap(),
            vec![2],
            "manual confirmation survives a model change"
        );
        for name in ["Hidden", "Stale"] {
            let output =
                match resolve_names(&c, &[name.into()], true, &ctx, "face-profile").unwrap() {
                    Err(output) => output,
                    Ok(_) => panic!("name should be excluded"),
                };
            assert_eq!(output.json["status"], "not_found");
        }
        let auto = match filters_on(
            &c,
            &parse_args(&json!({"limit":2,"mode":"structured","person_ids":[1]})).unwrap(),
            &ctx,
            "different-profile",
        )
        .unwrap()
        {
            Ok(v) => v,
            Err(_) => panic!(),
        };
        assert!(ids(&c, &auto).is_empty());
        let manual = match filters_on(
            &c,
            &parse_args(&json!({"limit":2,"mode":"structured","person_ids":[2]})).unwrap(),
            &ctx,
            "different-profile",
        )
        .unwrap()
        {
            Ok(v) => v,
            Err(_) => panic!(),
        };
        assert_eq!(ids(&c, &manual), vec![2]);
        let mut scoped = ctx;
        scoped.scope_file_ids = Some(vec![1]);
        assert_eq!(
            resolve_names(&c, &names, true, &scoped, "face-profile")
                .unwrap()
                .ok()
                .unwrap(),
            vec![1],
            "ambiguity cannot disclose people outside scope"
        );
    }
    #[test]
    fn exact_filename_filters_escape_like_wildcards() {
        let (c, _dir) = store::tests::fixture();
        c.execute_batch("UPDATE afiles SET name='100% photo.jpg' WHERE id=1;UPDATE afiles SET name='100x photo.jpg' WHERE id=2;").unwrap();
        let mut ctx = super::super::tests::context();
        ctx.prefetch_fields.clear();
        let selected = filter(
            &c,
            &parse_args(&json!({"limit":2,"mode":"structured","file_name":"100%"})).unwrap(),
            &ctx,
        );
        assert_eq!(ids(&c, &selected), vec![1]);
    }
    #[test]
    fn omitted_source_uses_text_only_embedding_configuration_without_multimodal_role() {
        let mut embedding = store::tests::model("text-only");
        embedding.provider.kind = ProviderKind::Embedding;
        embedding.model.capabilities = [Capability::EmbedText].into_iter().collect();
        let mut configuration = config::Configuration::default();
        configuration.providers.push(embedding.provider.clone());
        configuration.models.push(embedding.model.clone());
        configuration.roles.embedding = Some(embedding.model.id.clone());
        assert!(configuration.role("multimodalEmbedding").is_err());
        assert_eq!(default_source(&configuration), "analysis");
        configuration.roles.embedding = None;
        assert_eq!(default_source(&configuration), "media");
    }
    #[test]
    fn late_rank_results_are_rechecked_after_source_or_filter_changes() {
        let (c, _dir) = store::tests::fixture();
        c.execute_batch(
            "ALTER TABLE afiles ADD COLUMN taken_date INTEGER;INSERT INTO afile_tags VALUES(1,1);",
        )
        .unwrap();
        let model = store::tests::model("embedding");
        seed(&c, 1, &model, vec![1.0, 0.0]);
        let ctx = super::super::tests::context();
        let filter = filter(&c, &args(json!({"limit":1,"tag_ids":[1]})), &ctx);
        let hit = ranked(&c, "analysis", &model, &filter, &[1.0, 0.0], 1)
            .unwrap()
            .remove(0);
        let source = store::source(&c, 1).unwrap();
        assert!(hit_current(&c, "analysis", true, &filter, &hit, &source).unwrap());
        c.execute_batch("DELETE FROM afile_tags WHERE file_id=1")
            .unwrap();
        assert!(!hit_current(&c, "analysis", true, &filter, &hit, &source).unwrap());
        c.execute_batch("INSERT INTO afile_tags VALUES(1,1);UPDATE afiles SET modified_at=modified_at+1 WHERE id=1;").unwrap();
        let changed = store::source(&c, 1).unwrap();
        assert!(!hit_current(&c, "analysis", true, &filter, &hit, &changed).unwrap());
    }
}
