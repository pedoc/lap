//! Read-only assistant tools. Model-visible values are explicit projections, never catalogue rows.
use super::{chat, config, store, visual, workflow};
use crate::ai::{face_annotations, objects_store, ocr_annotations, settings, types::Task};
use genai::chat::{ContentPart, Tool, ToolCall};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::HashSet;

#[path = "chat_tools_search.rs"]
mod search;

pub struct ToolContext {
    pub library_id: String,
    pub request_id: String,
    pub app: Option<tauri::AppHandle>,
    pub model: config::Resolved,
    pub scope_file_ids: Option<Vec<i64>>,
    pub allowed_fields: Vec<String>,
}
pub struct ToolOutput {
    pub json: Value,
    pub references: Vec<chat::Reference>,
    pub parts: Vec<ContentPart>,
    pub local: Option<Value>,
}
impl ToolOutput {
    pub fn value(json: Value) -> Self {
        Self {
            json,
            references: Vec::new(),
            parts: Vec::new(),
            local: None,
        }
    }
}
pub fn tool(name: &str, description: &str, schema: Value) -> Tool {
    Tool::new(name)
        .with_description(description)
        .with_schema(schema)
}
pub fn definitions() -> Vec<Tool> {
    let mut out = vec![
        tool(
            "get_media_info",
            "Read existing catalogue data for visible media. Request only needed fields. Does not run recognition. People names distinguish confirmed/suggested/unknown; regions and index state include coverage. Local paths and embeddings are never returned.",
            json!({"type":"object","properties":{"file_ids":{"type":"array","items":{"type":"integer","minimum":1},"minItems":1,"maxItems":16},"fields":{"type":"array","items":{"type":"string","enum":["file","tags","people","objects","ocr","exif","location","analysis"]},"uniqueItems":true}},"required":["file_ids"],"additionalProperties":false}),
        ),
        tool(
            "get_media_content",
            "Read authorized pixels of one visible image or a video interval of at most 60 seconds (three sampled frames). A normalized x,y,width,height region requires explicit coordinate_space. decoded_image means EXIF-oriented decoded pixels. Rotated legacy face_catalog/source_image and OCR document regions require full-image review instead of implicit conversion. Cloud-only originals are never downloaded.",
            json!({"type":"object","properties":{"file_id":{"type":"integer","minimum":1},"start":{"type":"number","minimum":0},"end":{"type":"number","minimum":0},"region":{"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4},"coordinate_space":{"type":"string","enum":["decoded_image","source_image","face_catalog","ocr_document"]}},"required":["file_id"],"additionalProperties":false}),
        ),
        search::definition(),
    ];
    out.extend(super::chat_tools_workflow::definitions());
    out
}
pub async fn execute(
    library: &str,
    call: &ToolCall,
    ctx: &ToolContext,
) -> Result<ToolOutput, String> {
    if library != ctx.library_id {
        return Err("wrong_library: 工具图库与本轮上下文不一致。".into());
    }
    match call.fn_name.as_str() {
        "get_media_info" => info(library, &call.fn_arguments, ctx),
        "get_media_content" => content(library, &call.fn_arguments, ctx).await,
        "search_media" => search::execute(library, &call.fn_arguments, ctx).await,
        _ => super::chat_tools_workflow::execute(library, call, ctx).await,
    }
}
pub(super) fn permitted(ctx: &ToolContext, field: &str) -> bool {
    ctx.allowed_fields.iter().any(|value| value == field)
}
pub(super) fn denied(reason: &str) -> ToolOutput {
    ToolOutput::value(json!({"status":"denied","reason":reason}))
}
pub(super) fn check_scope(ids: &[i64], ctx: &ToolContext) -> Result<(), String> {
    if ids.is_empty() || ids.len() > 16 || ids.iter().any(|id| *id <= 0) {
        return Err("媒体 ID 数量无效，每次允许 1–16 项。".into());
    }
    if ctx
        .scope_file_ids
        .as_ref()
        .is_some_and(|scope| ids.iter().any(|id| !scope.contains(id)))
    {
        return Err("scope_denied: 媒体不在本轮允许的范围内。".into());
    }
    Ok(())
}
pub(super) fn sources(
    library: &str,
    ids: &[i64],
    ctx: &ToolContext,
) -> Result<Vec<store::MediaSource>, String> {
    if library != ctx.library_id {
        return Err("wrong_library: 图库不一致。".into());
    }
    check_scope(ids, ctx)?;
    store::db(library, |c| {
        ids.iter().map(|id| store::source(c, *id)).collect()
    })
}
pub(super) fn reference_on(
    c: &Connection,
    src: store::MediaSource,
    start: f64,
    end: f64,
) -> Result<chat::Reference, String> {
    let evidence: Vec<_> = store::analyses(c, &src)?
        .into_iter()
        .filter(|a| src.kind != 2 || a.end == 0.0 || a.start < end && a.end > start)
        .map(|a| chat::Evidence {
            analysis_id: a.id,
            start: a.start,
            end: a.end,
            title: a.text.title,
            description: a.text.description,
            tags: a.text.tags,
        })
        .collect();
    Ok(chat::Reference {
        file_id: src.file_id,
        name: std::path::Path::new(&src.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        file_type: src.kind,
        source_stamp: src.stamp(),
        start,
        end,
        analysis_ids: evidence.iter().map(|e| e.analysis_id.clone()).collect(),
        source: src,
        evidence,
    })
}
pub(super) fn table(c: &Connection, name: &str) -> Result<bool, String> {
    c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |r| r.get(0),
    )
    .map_err(store::sql_error)
}
fn columns(c: &Connection, name: &str) -> Result<HashSet<String>, String> {
    let mut statement = c
        .prepare(&format!("PRAGMA table_info({name})"))
        .map_err(store::sql_error)?;
    statement
        .query_map([], |r| r.get(1))
        .map_err(store::sql_error)?
        .collect::<Result<_, _>>()
        .map_err(store::sql_error)
}
pub(super) fn limited(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}
fn catalogue(
    c: &Connection,
    id: i64,
    fields: &[(&str, &str)],
    available: &HashSet<String>,
) -> Result<Value, String> {
    let sql = format!(
        "SELECT {} FROM afiles WHERE id=?1",
        fields
            .iter()
            .map(|(column, _)| if available.contains(*column) {
                column.to_string()
            } else {
                "NULL".into()
            })
            .collect::<Vec<_>>()
            .join(",")
    );
    c.query_row(&sql, [id], |row| {
        let mut object = Map::new();
        let mut truncated = false;
        for (index, (_, key)) in fields.iter().enumerate() {
            use rusqlite::types::ValueRef;
            let value = match row.get_ref(index)? {
                ValueRef::Null | ValueRef::Blob(_) => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(n) => json!(n),
                ValueRef::Text(s) => {
                    let text = String::from_utf8_lossy(s);
                    truncated |= text.chars().count() > 4000;
                    json!(limited(&text, 4000))
                }
            };
            object.insert((*key).into(), value);
        }
        object.insert("truncated".into(), json!(truncated));
        Ok(Value::Object(object))
    })
    .map_err(store::sql_error)
}
const FILE: &[(&str, &str)] = &[
    ("name", "name"),
    ("title", "title"),
    ("comments", "description"),
    ("file_type", "fileType"),
    ("size", "size"),
    ("width", "width"),
    ("height", "height"),
    ("duration", "duration"),
    ("taken_date", "takenDate"),
    ("rating", "rating"),
];
const EXIF: &[(&str, &str)] = &[
    ("e_make", "make"),
    ("e_model", "model"),
    ("e_lens_make", "lensMake"),
    ("e_lens_model", "lensModel"),
    ("e_date_time", "dateTime"),
    ("e_exposure_bias", "exposureBias"),
    ("e_exposure_time", "exposureTime"),
    ("e_f_number", "fNumber"),
    ("e_focal_length", "focalLength"),
    ("e_iso_speed", "iso"),
    ("e_flash", "flash"),
    ("e_orientation", "orientation"),
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InfoArgs {
    file_ids: Vec<i64>,
    fields: Option<Vec<String>>,
}
fn info(library: &str, args: &Value, ctx: &ToolContext) -> Result<ToolOutput, String> {
    let args: InfoArgs =
        serde_json::from_value(args.clone()).map_err(|_| "get_media_info 参数无效".to_string())?;
    check_scope(&args.file_ids, ctx)?;
    let fields = args.fields.unwrap_or_else(|| ctx.allowed_fields.clone());
    if fields.iter().any(|f| {
        ![
            "file", "tags", "people", "objects", "ocr", "exif", "location", "analysis",
        ]
        .contains(&f.as_str())
            || !permitted(ctx, f)
    }) {
        return Ok(denied("请求了本轮未授权的资料类别。"));
    }
    let configuration = config::load()?;
    store::db(library, |c| {
        info_on(c, &args.file_ids, &fields, &configuration)
    })
}
fn info_on(
    c: &Connection,
    ids: &[i64],
    fields: &[String],
    configuration: &config::Configuration,
) -> Result<ToolOutput, String> {
    let available = columns(c, "afiles")?;
    let mut items = Vec::new();
    let mut refs = Vec::new();
    let mut truncated = false;
    for id in ids {
        let src = store::source(c, *id)?;
        let mut item = json!({"fileId":id,"sourceStamp":src.stamp(),"indexStatus":index_status(c,&src,configuration)?});
        for field in fields {
            let value=match field.as_str() {
            "file"=>catalogue(c,*id,FILE,&available)?,
            "exif"=>catalogue(c,*id,EXIF,&available)?,
            "tags"=>tags(c,*id)?, "people"=>people(c,&src)?, "objects"=>objects(c,*id)?, "ocr"=>ocr(c,*id)?,
            "location"=>location(c,*id,&available)?,
            "analysis"=>json!(store::analyses(c,&src)?.into_iter().take(12).map(|a|json!({"analysisId":a.id,"start":a.start,"end":a.end,"title":limited(&a.text.title,256),"description":limited(&a.text.description,3000),"tags":a.text.tags,"createdAt":a.created_at})).collect::<Vec<_>>()),
            _=>return Err("未知资料字段".into()),
        };
            item[field] = value;
        }
        if serde_json::to_vec(&items)
            .map_err(|_| "工具结果编码失败")?
            .len()
            + serde_json::to_vec(&item)
                .map_err(|_| "工具结果编码失败")?
                .len()
            > 64 * 1024
        {
            truncated = true;
            break;
        }
        let end = if src.kind == 2 {
            (src.duration as f64).min(60.0).max(0.0)
        } else {
            0.0
        };
        refs.push(reference_on(c, src, 0.0, end)?);
        items.push(item);
    }
    Ok(ToolOutput {
        json: json!({"status":"ok","items":items,"truncated":truncated}),
        references: refs,
        parts: Vec::new(),
        local: None,
    })
}
fn tags(c: &Connection, id: i64) -> Result<Value, String> {
    let mut s=c.prepare("SELECT t.id,t.name FROM atags t JOIN afile_tags ft ON ft.tag_id=t.id WHERE ft.file_id=?1 ORDER BY t.id LIMIT 101").map_err(store::sql_error)?;
    let mut rows = s
        .query_map([id], |r| {
            Ok(json!({"id":r.get::<_,i64>(0)?,"name":limited(&r.get::<_,String>(1)?,128)}))
        })
        .map_err(store::sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(store::sql_error)?;
    let truncated = rows.len() > 100;
    rows.truncate(100);
    Ok(json!({"items":rows,"truncated":truncated}))
}
fn person_name(c: &Connection, id: Option<i64>) -> Result<Option<String>, String> {
    let Some(id) = id else { return Ok(None) };
    c.query_row(
        "SELECT name FROM persons WHERE id=?1 AND COALESCE(hidden,0)=0",
        [id],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|v| {
        v.flatten()
            .filter(|s| !s.trim().is_empty())
            .map(|s| limited(&s, 128))
    })
    .map_err(store::sql_error)
}
fn people(c: &Connection, src: &store::MediaSource) -> Result<Value, String> {
    people_on(c, src, &crate::t_search::Profiles::current().face)
}
fn people_on(c: &Connection, src: &store::MediaSource, profile: &str) -> Result<Value, String> {
    if src.kind == 2 {
        return Ok(
            json!({"status":"unsupported","reason":"当前人物记录是图片记录，不能证明视频人物覆盖。"}),
        );
    }
    if !table(c, "faces")? || !table(c, "persons")? {
        return Ok(json!({"status":"not_indexed","items":[]}));
    }
    let mut items = Vec::new();
    if table(c, "face_annotations")? {
        for a in face_annotations::active_for_file(c, src.file_id)? {
            if !["confirmed", "unassigned"].contains(&a.kind.as_str()) {
                continue;
            }
            let name = person_name(c, a.person_id)?;
            let state = if name.is_some() && a.kind == "confirmed" {
                "confirmed"
            } else {
                "unknown"
            };
            items.push(json!({"annotationId":a.id,"personId":if name.is_some(){a.person_id}else{None},"name":name,"state":state,"region":a.region,"source":"manual"}));
        }
    }
    let scanned = if columns(c, "afiles")?.contains("has_faces") {
        c.query_row(
            "SELECT COALESCE(has_faces,0) FROM afiles WHERE id=?1",
            [src.file_id],
            |r| r.get::<_, i64>(0),
        )
        .map_err(store::sql_error)?
    } else {
        0
    };
    let compatible = table(c, "ai_index_profiles")?
        && c.query_row(
            "SELECT EXISTS(SELECT 1 FROM ai_index_profiles WHERE task='face' AND fingerprint=?1)",
            [profile],
            |r| r.get::<_, bool>(0),
        )
        .map_err(store::sql_error)?;
    if scanned != 0 && compatible {
        let face_columns = columns(c, "faces")?;
        let filter = if face_columns.contains("annotation_id") {
            " AND annotation_id IS NULL"
        } else {
            ""
        };
        let mut s = c
            .prepare(&format!(
                "SELECT id,bbox,person_id FROM faces WHERE file_id=?1{filter} ORDER BY id LIMIT 101"
            ))
            .map_err(store::sql_error)?;
        let rows = s
            .query_map([src.file_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                ))
            })
            .map_err(store::sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(store::sql_error)?;
        for (id, bbox, person) in rows {
            let bounds = serde_json::from_str::<crate::t_face::FaceBox>(&bbox)
                .map_err(|_| "人脸区域数据无效")?;
            let region =
                face_annotations::normalized_box(&bounds, src.width as i64, src.height as i64)?;
            let name = person_name(c, person)?;
            items.push(json!({"faceId":id,"personId":if name.is_some(){person}else{None},"state":if name.is_some(){"suggested"}else{"unknown"},"name":name,"region":region,"source":"automatic"}));
        }
    }
    let truncated = items.len() > 100;
    items.truncate(100);
    Ok(
        json!({"status":if items.is_empty()&&!compatible&&scanned!=0{"stale"}else if scanned==0&&items.is_empty(){"not_indexed"}else{"ready"},"items":items,"truncated":truncated,"modelCompatible":compatible,"scanComplete":scanned!=0&&compatible,"coverage":if scanned!=0&&compatible{"indexed_image"}else{"manual_regions_only"},"coordinateSpace":{"units":"normalized","basis":"face_catalog","width":src.width,"height":src.height,"exifOrientation":src.orientation,"cropSupported":src.orientation==1}}),
    )
}
fn objects(c: &Connection, id: i64) -> Result<Value, String> {
    let profile = settings::active(Task::Detection)
        .map(|m| m.profile())
        .unwrap_or_default();
    objects_on(c, id, &profile)
}
fn objects_on(c: &Connection, id: i64, profile: &str) -> Result<Value, String> {
    if !table(c, "object_index")? || !table(c, "object_annotations")? {
        return Ok(json!({"status":"not_indexed"}));
    }
    let data = objects_store::photo_on(c, id, profile)?;
    let manual = data.annotation.is_some();
    let revision = data.annotation.as_ref().map(|a| a.revision.clone());
    let Some(mut document) = data.document else {
        return Ok(json!({"status":if data.error.is_some(){"failed"}else{"not_indexed"}}));
    };
    let count = document.objects.len();
    document.objects.truncate(100);
    let items:Vec<_>=document.objects.iter().map(|object|json!({"id":object.id,"class":{"key":object.class.key,"name":limited(&object.class.name,128)},"score":object.score,"reviewed":object.reviewed,"bounds":object.bounds})).collect();
    Ok(
        json!({"status":"ready","source":if manual{"manual"}else{"automatic"},"revision":revision,"coordinateSpace":{"units":"normalized","basis":"decoded_image","width":document.image_width,"height":document.image_height},"items":items,"truncated":document.truncated||count>100}),
    )
}
fn ocr(c: &Connection, id: i64) -> Result<Value, String> {
    let profile = settings::active(Task::Ocr)
        .map(|m| m.profile())
        .unwrap_or_default();
    ocr_on(c, id, &profile)
}
fn ocr_on(c: &Connection, id: i64, profile: &str) -> Result<Value, String> {
    if !table(c, "ocr_index")? || !table(c, "ocr_annotations")? {
        return Ok(json!({"status":"not_indexed"}));
    }
    let data = ocr_annotations::view_on(c, id, profile)?;
    let manual = data.annotation.is_some();
    let revision = data.annotation.as_ref().map(|a| a.revision.clone());
    let Some(document) = data.document else {
        return Ok(json!({"status":if data.error.is_some(){"failed"}else{"not_indexed"}}));
    };
    let lines:Vec<_>=document.lines.iter().take(100).map(|line|json!({"text":limited(&line.text,1000),"confidence":line.confidence,"bounds":line.bounds})).collect();
    Ok(
        json!({"status":"ready","source":if manual{"manual"}else{"automatic"},"revision":revision,"text":limited(&document.text,12000),"items":lines,"truncated":document.lines.len()>100||document.text.chars().count()>12000,"coordinateSpace":{"units":"normalized","basis":"ocr_document","width":document.image_width,"height":document.image_height,"rotationDegrees":document.rotation_degrees,"deskewDegrees":document.deskew_degrees}}),
    )
}
fn location(c: &Connection, id: i64, available: &HashSet<String>) -> Result<Value, String> {
    let mut out = catalogue(
        c,
        id,
        &[
            ("gps_latitude", "latitude"),
            ("gps_longitude", "longitude"),
            ("gps_altitude", "altitude"),
            ("geo_name", "name"),
            ("geo_admin1", "admin1"),
            ("geo_admin2", "admin2"),
            ("geo_cc", "countryCode"),
        ],
        available,
    )?;
    if table(c, "file_locations")?
        && available.contains("gps_latitude")
        && available.contains("gps_longitude")
    {
        let text: Option<String> = c
            .query_row(
                &format!(
                    "SELECT {} FROM afiles a WHERE a.id=?1",
                    crate::t_map::address_store::address_json("a")
                ),
                [id],
                |r| r.get(0),
            )
            .map_err(store::sql_error)?;
        if let Some(address) = text.and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
            out["address"] = address
                .get("address")
                .map(|v| json!(limited(v.as_str().unwrap_or_default(), 1000)))
                .unwrap_or(Value::Null);
            out["hierarchy"]=json!(address.get("hierarchy").and_then(Value::as_array).into_iter().flatten().take(10).map(|v|json!({"name":v.get("name").and_then(Value::as_str).map(|s|limited(s,128)),"kind":v.get("kind").and_then(Value::as_str)})).collect::<Vec<_>>());
        }
    }
    Ok(out)
}
fn index_status(
    c: &Connection,
    src: &store::MediaSource,
    configuration: &config::Configuration,
) -> Result<Value, String> {
    let analyses = store::analyses(c, src)?;
    let text = configuration.role("embedding").ok();
    let media = configuration.role("multimodalEmbedding").ok();
    let text_count = if let Some(profile) = text.as_ref().and_then(|m| m.embedding_profile().ok()) {
        let mut count = 0;
        for a in &analyses {
            count += c
                .query_row(
                    "SELECT COUNT(*) FROM media_ai_vectors WHERE analysis_id=?1 AND profile_id=?2",
                    params![a.id, profile],
                    |r| r.get::<_, i64>(0),
                )
                .map_err(store::sql_error)?;
        }
        count
    } else {
        0
    };
    let (media_count, complete) =
        if let Some(model) = media.as_ref().filter(|m| m.multimodal_profile().is_ok()) {
            let vectors = visual::vectors(c, src, model)?;
            (vectors.len(), visual::complete(src, &vectors))
        } else {
            (0, false)
        };
    Ok(
        json!({"basis":"current_default_roles","analysisCount":analyses.len(),"text":{"configured":text.is_some(),"indexedSegments":text_count},"media":{"configured":media.is_some(),"indexedSegments":media_count,"complete":complete}}),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContentArgs {
    file_id: i64,
    start: Option<f64>,
    end: Option<f64>,
    region: Option<[f64; 4]>,
    coordinate_space: Option<String>,
}
fn check_region(region: [f64; 4], space: Option<&str>, orientation: i32) -> Result<(), String> {
    if region.iter().any(|v| !v.is_finite())
        || region[0] < 0.0
        || region[1] < 0.0
        || region[2] <= 0.0
        || region[3] <= 0.0
        || region[0] + region[2] > 1.0
        || region[1] + region[3] > 1.0
    {
        return Err("图片区域必须是有效的归一化x/y/width/height。".into());
    }
    match space {
        Some("decoded_image")=>Ok(()),
        Some("source_image"|"face_catalog") if orientation==1=>Ok(()),
        Some("source_image"|"face_catalog"|"ocr_document")=>Err("此标注坐标与解码画面方向不能直接混用，请先读取整图复核，或提供已转换的decoded_image区域。".into()),
        _=>Err("region必须显式声明coordinate_space。".into()),
    }
}
fn encode_region(src: store::MediaSource, region: [f64; 4]) -> Result<String, String> {
    use base64::Engine;
    let image = src
        .image()
        .map_err(|_| "原图无法读取或来源版本变化。".to_string())?;
    let width = image.width();
    let height = image.height();
    let x = (region[0] * width as f64).floor() as u32;
    let y = (region[1] * height as f64).floor() as u32;
    let w = ((region[2] * width as f64).ceil() as u32)
        .min(width - x)
        .max(1);
    let h = ((region[3] * height as f64).ceil() as u32)
        .min(height - y)
        .max(1);
    let crop = image.crop_imm(x, y, w, h);
    let crop = if crop.width() > 1280 || crop.height() > 1280 {
        crop.thumbnail(1280, 1280)
    } else {
        crop
    };
    let image = image::DynamicImage::ImageRgb8(crop.to_rgb8());
    let mut buffer = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, image::ImageFormat::Jpeg)
        .map_err(|_| "区域图片编码失败")?;
    Ok(base64::engine::general_purpose::STANDARD.encode(buffer.into_inner()))
}
async fn content(library: &str, args: &Value, ctx: &ToolContext) -> Result<ToolOutput, String> {
    let args: ContentArgs = serde_json::from_value(args.clone())
        .map_err(|_| "get_media_content 参数无效".to_string())?;
    let src = sources(library, &[args.file_id], ctx)?
        .into_iter()
        .next()
        .ok_or("媒体不存在")?;
    let cap = if src.kind == 2 {
        config::Capability::VideoFrames
    } else {
        config::Capability::Image
    };
    chat::check_model(&config::load()?, &ctx.model, &[cap])?;
    src.verify()
        .map_err(|_| "原文件不可用、仅在云端或来源版本已变化；不会自动下载。".to_string())?;
    let (start, end) = if src.kind == 2 {
        if args.region.is_some() {
            return Err("视频区域裁剪尚不支持，请读取有界采样片段。".into());
        }
        let duration = if src.duration > 0 {
            src.duration as f64
        } else {
            crate::t_video::get_video_duration(&src.path)
                .await
                .map_err(|_| "无法读取视频时长，请检查原文件可用性。".to_string())?
                as f64
        };
        let start = args.start.unwrap_or(0.0);
        let end = args.end.unwrap_or((start + 60.0).min(duration));
        if !start.is_finite()
            || !end.is_finite()
            || start < 0.0
            || end <= start
            || end > duration
            || end - start > 60.0
        {
            return Err("视频区间无效；每次最多读取60秒。".into());
        }
        (start, end)
    } else {
        if args.start.unwrap_or(0.0) != 0.0 || args.end.unwrap_or(0.0) != 0.0 {
            return Err("图片不支持视频时间参数。".into());
        }
        (0.0, 0.0)
    };
    let parts = if let Some(region) = args.region {
        check_region(region, args.coordinate_space.as_deref(), src.orientation)?;
        let image_source = src.clone();
        let data =
            tauri::async_runtime::spawn_blocking(move || encode_region(image_source, region))
                .await
                .map_err(|_| "区域图片准备失败")??;
        vec![ContentPart::from_binary_base64("image/jpeg", data, None)]
    } else {
        workflow::parts(&src, start, end, String::new())
            .await
            .map_err(|_| "媒体画面准备失败，请检查原文件与采样区间。".to_string())?
    };
    chat::check_model(&config::load()?, &ctx.model, &[cap])?;
    let reference = store::db(library, |c| {
        let current = store::source(c, src.file_id)?;
        if current.stamp() != src.stamp() {
            return Err("媒体已变化，请重新引用。".into());
        }
        reference_on(c, src.clone(), start, end)
    })?;
    Ok(ToolOutput {
        json: json!({"status":"ok","items":[{"fileId":src.file_id,"start":start,"end":end,"region":args.region,"coordinateSpace":args.coordinate_space,"frameCount":if src.kind==2{3}else{1},"coverage":if src.kind==2{"sampled_frames_only"}else{"image"}}]}),
        references: vec![reference],
        parts,
        local: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn context() -> ToolContext {
        ToolContext {
            library_id: "library-a".into(),
            request_id: "request-a".into(),
            app: None,
            model: store::tests::model("chat"),
            scope_file_ids: Some(vec![1, 2, 3]),
            allowed_fields: vec![
                "file", "tags", "people", "objects", "ocr", "exif", "analysis",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
    fn call(name: &str, args: Value) -> ToolCall {
        ToolCall {
            call_id: "call-a".into(),
            fn_name: name.into(),
            fn_arguments: args,
            thought_signatures: None,
        }
    }
    fn people_schema(c: &Connection) {
        c.execute_batch("ALTER TABLE afiles ADD COLUMN has_faces INTEGER DEFAULT 0;CREATE TABLE persons(id INTEGER PRIMARY KEY,name TEXT,hidden INTEGER DEFAULT 0);CREATE TABLE faces(id INTEGER PRIMARY KEY,file_id INTEGER,bbox TEXT,embedding BLOB,person_id INTEGER,created_at INTEGER,annotation_id INTEGER);CREATE TABLE ai_index_profiles(task TEXT PRIMARY KEY,fingerprint TEXT);INSERT INTO ai_index_profiles VALUES('face','face-profile');").unwrap();
        face_annotations::ensure_schema(c).unwrap();
    }
    #[tokio::test]
    async fn assistant_tools_deny_wrong_library_scope_fields_and_unbounded_search_before_io() {
        let ctx = context();
        assert!(
            execute(
                "library-b",
                &call("get_media_info", json!({"file_ids":[1]})),
                &ctx
            )
            .await
            .err()
            .unwrap()
            .contains("wrong_library")
        );
        assert!(check_scope(&[99], &ctx).is_err());
        let output = execute(
            "library-a",
            &call(
                "get_media_info",
                json!({"file_ids":[1],"fields":["location"]}),
            ),
            &ctx,
        )
        .await
        .unwrap();
        assert_eq!(output.json["status"], "denied");
        assert!(
            execute(
                "library-a",
                &call("get_media_info", json!({"file_ids":[1],"path":"secret"})),
                &ctx
            )
            .await
            .is_err()
        );
        assert!(
            execute("library-a", &call("search_media", json!({"limit":0})), &ctx)
                .await
                .is_err()
        );
        assert!(
            execute(
                "library-a",
                &call("search_media", json!({"limit":-1})),
                &ctx
            )
            .await
            .is_err()
        );
        let mut none = context();
        none.allowed_fields.clear();
        let output = execute(
            "library-a",
            &call(
                "search_media",
                json!({"limit":2,"mode":"structured","person_ids":[1]}),
            ),
            &none,
        )
        .await
        .unwrap();
        assert_eq!(output.json["status"], "denied");
    }
    #[test]
    fn assistant_info_is_a_whitelist_and_unknown_faces_are_not_named() {
        let (c, _dir) = store::tests::fixture();
        people_schema(&c);
        c.execute_batch("ALTER TABLE afiles ADD COLUMN e_make TEXT;ALTER TABLE afiles ADD COLUMN gps_latitude REAL;ALTER TABLE afiles ADD COLUMN gps_longitude REAL;UPDATE afiles SET e_make='Fixture Camera',gps_latitude=12.5,gps_longitude=34.5,has_faces=1 WHERE id=1;INSERT INTO persons VALUES(7,'Visible Name',0,0,NULL);INSERT INTO afile_tags VALUES(1,1);").unwrap();
        let src = store::source(&c, 1).unwrap();
        c.execute("INSERT INTO face_annotations(file_id,person_id,kind,region,source_modified_at,source_size,created_at,updated_at) VALUES(1,7,'confirmed',?1,?2,?3,0,0)",params![r#"{"x":0.1,"y":0.2,"width":0.2,"height":0.3}"#,src.modified,src.size]).unwrap();
        c.execute(
            "INSERT INTO faces VALUES(1,1,?1,X'01020304',NULL,0,NULL)",
            [r#"{"x":200,"y":200,"width":100,"height":100,"confidence":0.9}"#],
        )
        .unwrap();
        let mut output = info_on(
            &c,
            &[1],
            &["file", "tags", "exif"].map(str::to_owned),
            &config::Configuration::default(),
        )
        .unwrap();
        output.json["items"][0]["people"] = people_on(&c, &src, "face-profile").unwrap();
        let encoded = output.json.to_string();
        assert!(!encoded.contains(&src.path));
        assert!(!encoded.contains("embedding"));
        assert!(!encoded.contains("latitude"));
        assert_eq!(output.json["items"][0]["exif"]["make"], "Fixture Camera");
        let faces = output.json["items"][0]["people"]["items"]
            .as_array()
            .unwrap();
        assert!(
            faces
                .iter()
                .any(|f| f["state"] == "confirmed" && f["name"] == "Visible Name")
        );
        assert!(
            faces
                .iter()
                .any(|f| f["state"] == "unknown" && f["name"].is_null())
        );
        assert_eq!(
            output.references[0].source.path, src.path,
            "full source stays local only"
        );
        let mismatched = people_on(&c, &src, "another-profile").unwrap();
        assert_eq!(
            mismatched["items"].as_array().unwrap().len(),
            1,
            "only confirmed manual face survives model change"
        );
        c.execute_batch("UPDATE afiles SET modified_at=modified_at+1,has_faces=0 WHERE id=1")
            .unwrap();
        let current = store::source(&c, 1).unwrap();
        assert_eq!(
            people_on(&c, &current, "face-profile").unwrap()["status"],
            "not_indexed"
        );
        c.execute_batch("INSERT INTO media_states(file_id,hidden) VALUES(1,1)")
            .unwrap();
        assert!(
            info_on(
                &c,
                &[1],
                &["file".into()],
                &config::Configuration::default()
            )
            .is_err()
        );
    }
    #[test]
    fn assistant_effective_object_and_ocr_human_edits_override_models() {
        let (c, _dir) = store::tests::fixture();
        objects_store::ensure_schema(&c).unwrap();
        crate::ai::ocr::ensure_schema(&c).unwrap();
        let src = store::source(&c, 1).unwrap();
        let dog = json!({"imageWidth":1000,"imageHeight":1000,"objects":[{"id":"object-1","class":{"key":"test:dog","sourceId":"dog","name":"dog"},"score":0.8,"reviewed":false,"bounds":[0.1,0.2,0.3,0.4]}],"truncated":false});
        let mut cat = dog.clone();
        cat["objects"][0]["class"] = json!({"key":"test:cat","sourceId":"cat","name":"cat"});
        cat["objects"][0]["reviewed"] = json!(true);
        c.execute(
            "INSERT INTO object_index VALUES(1,'profile',?1,?2,?3,NULL,1,1)",
            params![src.modified, src.size, dog.to_string()],
        )
        .unwrap();
        c.execute("INSERT INTO object_annotations VALUES(1,?1,?2,'profile','Fixture',?3,'{}',?4,1,1,'revision-1')",params![src.modified,src.size,dog.to_string(),cat.to_string()]).unwrap();
        let result = objects_on(&c, 1, "profile").unwrap();
        assert_eq!(result["source"], "manual");
        assert_eq!(result["items"][0]["class"]["name"], "cat");
        assert_eq!(result["revision"], "revision-1");
        let document = json!({"text":"automatic","lines":[{"text":"automatic","confidence":0.8,"bounds":[0.1,0.2,0.3,0.1]}],"imageWidth":1000,"imageHeight":1000});
        c.execute("INSERT INTO ocr_index(file_id,profile,modified_at,size,document,text,error) VALUES(1,'ocr-profile',?1,?2,?3,'automatic',NULL)",params![src.modified,src.size,document.to_string()]).unwrap();
        c.execute("INSERT INTO ocr_annotations VALUES(1,?1,?2,'ocr-profile','Fixture',?3,?4,?3,'human','ocr-revision')",params![src.modified,src.size,document.to_string(),json!([{"text":"human","ignored":false}]).to_string()]).unwrap();
        let result = ocr_on(&c, 1, "ocr-profile").unwrap();
        assert_eq!(result["source"], "manual");
        assert_eq!(result["text"], "human");
        assert_eq!(result["items"][0]["text"], "human");
    }
    #[test]
    fn explicit_decoded_regions_handle_rotated_rgba_and_reject_ambiguous_face_coordinates() {
        use base64::Engine;
        assert!(check_region([0.0, 0.0, 0.5, 0.5], None, 1).is_err());
        assert!(check_region([0.0, 0.0, 0.5, 0.5], Some("face_catalog"), 6).is_err());
        assert!(check_region([0.0, 0.0, 0.5, 0.5], Some("ocr_document"), 1).is_err());
        assert!(check_region([0.0, 0.0, 0.5, 0.5], Some("decoded_image"), 6).is_ok());
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("synthetic-rgba.png");
        let image = image::ImageBuffer::from_fn(8, 4, |x, _| {
            if x < 4 {
                image::Rgba([240u8, 0, 0, 128])
            } else {
                image::Rgba([0u8, 0, 240, 128])
            }
        });
        image.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let info = crate::t_utils::FileInfo::new(path.to_str().unwrap()).unwrap();
        let source = store::MediaSource {
            file_id: 1,
            path: path.to_string_lossy().into_owned(),
            modified: info.modified,
            size: info.file_size,
            orientation: 6,
            kind: 1,
            width: 8,
            height: 4,
            duration: 0,
            title: None,
            comments: None,
            tag_ids: Vec::new(),
            cloud_only: false,
        };
        let decoded = source.image().unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4, 8));
        let jpeg = encode_region(source, [0.0, 0.5, 1.0, 0.5]).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(jpeg)
            .unwrap();
        let crop = image::load_from_memory(&bytes).unwrap().to_rgb8();
        assert_eq!((crop.width(), crop.height()), (4, 4));
        assert!(
            crop.pixels().all(|p| p[2] > 180 && p[0] < 60),
            "decoded lower half is blue after EXIF 90-degree rotation"
        );
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
}
