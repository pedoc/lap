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

pub(super) const CONTEXT_FIELDS: [&str; 8] = [
    "file", "tags", "people", "objects", "ocr", "exif", "analysis", "location",
];
pub struct ToolContext {
    pub library_id: String,
    pub request_id: String,
    pub app: Option<tauri::AppHandle>,
    pub model: config::Resolved,
    pub scope_file_ids: Option<Vec<i64>>,
    pub prefetch_fields: Vec<String>,
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
            "Read existing catalogue data for visible media. Explicit fields can request categories omitted from automatic context; omitted fields default only to the user's automatic-context selection. Request only needed fields. Does not run recognition. People names distinguish confirmed/suggested/unknown; regions and index state include coverage. Position describes the face center in the displayed image only when coordinate alignment is known. Local paths and embeddings are never returned.",
            json!({"type":"object","properties":{"file_ids":{"type":"array","items":{"type":"integer","minimum":1},"minItems":1,"maxItems":16},"fields":{"type":"array","items":{"type":"string","enum":["file","tags","people","objects","ocr","exif","location","analysis"]},"uniqueItems":true}},"required":["file_ids"],"additionalProperties":false}),
        ),
        tool(
            "get_media_content",
            "Read authorized pixels of one visible image or a video interval of at most 60 seconds (three sampled frames), with the permitted catalogue context. A normalized x,y,width,height region requires explicit coordinate_space. decoded_image means EXIF-oriented decoded pixels. Rotated legacy face_catalog/source_image and OCR document regions require full-image review instead of implicit conversion. Catalogue regions describe the full image, not a cropped view. Cloud-only originals are never downloaded.",
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
fn info_fields(requested: Option<Vec<String>>, ctx: &ToolContext) -> Result<Vec<String>, String> {
    let fields = requested.unwrap_or_else(|| ctx.prefetch_fields.clone());
    if fields.len() > CONTEXT_FIELDS.len()
        || fields
            .iter()
            .any(|field| !CONTEXT_FIELDS.contains(&field.as_str()))
    {
        return Err("get_media_info 资料类别无效。".into());
    }
    Ok(fields)
}
fn info(library: &str, args: &Value, ctx: &ToolContext) -> Result<ToolOutput, String> {
    let args: InfoArgs =
        serde_json::from_value(args.clone()).map_err(|_| "get_media_info 参数无效".to_string())?;
    check_scope(&args.file_ids, ctx)?;
    let fields = info_fields(args.fields, ctx)?;
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
    let item_budget = (64 * 1024 - 512) / ids.len().max(1);
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
        truncated |= fit_info_item(&mut item, item_budget)?;
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
fn reduce_info_value(value: &mut Value) -> bool {
    match value {
        Value::String(text) if text.chars().count() > 128 => {
            *text = limited(text, (text.chars().count() / 2).max(128));
            true
        }
        Value::Array(items) if items.iter().all(Value::is_number) => false,
        Value::Array(items) if items.len() > 1 => {
            items.truncate(items.len().div_ceil(2));
            true
        }
        Value::Array(items) => items.iter_mut().any(reduce_info_value),
        Value::Object(fields) => fields.values_mut().any(reduce_info_value),
        _ => false,
    }
}
fn fit_info_item(item: &mut Value, budget: usize) -> Result<bool, String> {
    let mut truncated = Vec::new();
    let target = budget.saturating_sub(256);
    // Long prose must not evict the whole image, its people, or every later reference.
    for field in [
        "analysis", "ocr", "exif", "tags", "file", "location", "objects", "people",
    ] {
        let mut changed = false;
        while serde_json::to_vec(item)
            .map_err(|_| "工具结果编码失败")?
            .len()
            > target
        {
            let Some(value) = item.get_mut(field) else {
                break;
            };
            if !reduce_info_value(value) {
                if !["people", "objects"].contains(&field) {
                    *value = if value.is_array() {
                        json!([])
                    } else {
                        json!({"status":"omitted","reason":"context_budget"})
                    };
                    changed = true;
                }
                break;
            }
            changed = true;
        }
        if changed {
            truncated.push(field);
            item["truncatedFields"] = json!(truncated);
        }
    }
    if serde_json::to_vec(item)
        .map_err(|_| "工具结果编码失败")?
        .len()
        > budget
    {
        return Err("媒体资料摘要仍超过大小上限，请减少引用数量或资料类别。".into());
    }
    Ok(!truncated.is_empty())
}
pub(super) fn context_on(
    c: &Connection,
    references: &[chat::Reference],
    fields: &[String],
    configuration: &config::Configuration,
) -> Result<Value, String> {
    for reference in references {
        if store::source(c, reference.file_id)?.stamp() != reference.source_stamp {
            return Err("媒体已变化，请重新引用。".into());
        }
    }
    if fields.is_empty() {
        return Ok(json!({"status":"ok","items":[],"truncated":false}));
    }
    let ids: Vec<_> = references
        .iter()
        .map(|reference| reference.file_id)
        .collect();
    let mut output = info_on(c, &ids, fields, configuration)?.json;
    for item in output["items"].as_array_mut().ok_or("媒体资料无法编码")? {
        let index = references
            .iter()
            .position(|reference| item["fileId"] == reference.file_id)
            .ok_or("媒体资料与引用不匹配")?;
        if item["sourceStamp"] != references[index].source_stamp {
            return Err("媒体已变化，请重新引用。".into());
        }
    }
    Ok(output)
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
fn face_position(region: &face_annotations::Region, orientation: i32) -> Option<Value> {
    if orientation != 1
        || ![region.x, region.y, region.width, region.height]
            .iter()
            .all(|value| value.is_finite())
        || region.x < 0.0
        || region.y < 0.0
        || region.width <= 0.0
        || region.height <= 0.0
        || region.x + region.width > 1.00001
        || region.y + region.height > 1.00001
    {
        return None;
    }
    let x = region.x + region.width / 2.0;
    let y = region.y + region.height / 2.0;
    let horizontal = if x < 1.0 / 3.0 {
        "left"
    } else if x > 2.0 / 3.0 {
        "right"
    } else {
        "center"
    };
    let vertical = if y < 1.0 / 3.0 {
        "top"
    } else if y > 2.0 / 3.0 {
        "bottom"
    } else {
        "middle"
    };
    Some(
        json!({"horizontal":horizontal,"vertical":vertical,"center":[x,y],"basis":"decoded_image","subject":"face_center"}),
    )
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
            items.push(json!({"annotationId":a.id,"personId":if name.is_some(){a.person_id}else{None},"name":name,"state":state,"position":face_position(&a.region,src.orientation),"region":a.region,"source":"manual"}));
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
            items.push(json!({"faceId":id,"personId":if name.is_some(){person}else{None},"state":if name.is_some(){"suggested"}else{"unknown"},"name":name,"position":face_position(&region,src.orientation),"region":region,"source":"automatic"}));
        }
    }
    let truncated = items.len() > 100;
    items.truncate(100);
    Ok(
        json!({"status":if items.is_empty()&&!compatible&&scanned!=0{"stale"}else if scanned==0&&items.is_empty(){"not_indexed"}else{"ready"},"regionType":"face","items":items,"truncated":truncated,"modelCompatible":compatible,"scanComplete":scanned!=0&&compatible,"coverage":if scanned!=0&&compatible{"indexed_image"}else{"manual_regions_only"},"positionStatus":if src.orientation==1{"display_aligned"}else{"coordinate_alignment_unknown"},"coordinateSpace":{"units":"normalized","basis":"face_catalog","origin":"top_left","regionFormat":"x_y_width_height","width":src.width,"height":src.height,"exifOrientation":src.orientation,"cropSupported":src.orientation==1}}),
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
        json!({"status":"ready","regionType":"object","source":if manual{"manual"}else{"automatic"},"revision":revision,"coordinateSpace":{"units":"normalized","basis":"decoded_image","origin":"top_left","regionFormat":"x_y_width_height","width":document.image_width,"height":document.image_height},"items":items,"truncated":document.truncated||count>100}),
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
        json!({"status":"ready","regionType":"text_line","source":if manual{"manual"}else{"automatic"},"revision":revision,"text":limited(&document.text,12000),"items":lines,"truncated":document.lines.len()>100||document.text.chars().count()>12000,"coordinateSpace":{"units":"normalized","basis":"ocr_document","origin":"top_left","regionFormat":"x_y_width_height","width":document.image_width,"height":document.image_height},"processing":{"rotationDegrees":document.rotation_degrees,"deskewDegrees":document.deskew_degrees,"boundsMapping":"already_mapped_to_input"}}),
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
    let configuration = config::load()?;
    store::db(library, |c| {
        let reference = reference_on(c, src.clone(), start, end)?;
        content_output_on(
            c,
            &args,
            reference,
            parts,
            &ctx.prefetch_fields,
            &configuration,
        )
    })
}
fn content_output_on(
    c: &Connection,
    args: &ContentArgs,
    reference: chat::Reference,
    parts: Vec<ContentPart>,
    fields: &[String],
    configuration: &config::Configuration,
) -> Result<ToolOutput, String> {
    let catalogue = context_on(c, std::slice::from_ref(&reference), fields, configuration)?;
    Ok(ToolOutput {
        json: json!({"status":"ok","items":[{"fileId":reference.file_id,"start":reference.start,"end":reference.end,"region":args.region,"coordinateSpace":args.coordinate_space,"frameCount":if reference.file_type==2{3}else{1},"coverage":if reference.file_type==2{"sampled_frames_only"}else{"image"}}],"catalogue":catalogue}),
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
            prefetch_fields: CONTEXT_FIELDS.into_iter().map(str::to_owned).collect(),
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
    async fn assistant_tools_reject_wrong_library_scope_unknown_fields_and_unbounded_search_before_io()
     {
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
        let error = execute(
            "library-a",
            &call("get_media_info", json!({"file_ids":[1],"fields":["path"]})),
            &ctx,
        )
        .await
        .err()
        .unwrap();
        assert!(error.contains("资料类别无效"));
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
    }
    #[test]
    fn explicit_tool_fields_are_independent_of_automatic_context_selection() {
        let (c, _dir) = store::tests::fixture();
        c.execute_batch("ALTER TABLE afiles ADD COLUMN gps_latitude REAL;ALTER TABLE afiles ADD COLUMN gps_longitude REAL;UPDATE afiles SET gps_latitude=12.5,gps_longitude=34.5 WHERE id=1;INSERT INTO afile_tags VALUES(1,1);").unwrap();
        let mut ctx = context();
        ctx.prefetch_fields.clear();
        assert!(info_fields(None, &ctx).unwrap().is_empty());
        let explicit = info_fields(Some(vec!["location".into(), "tags".into()]), &ctx).unwrap();
        let configuration = config::Configuration::default();
        let output = info_on(&c, &[1], &explicit, &configuration).unwrap();
        assert_eq!(output.json["items"][0]["location"]["latitude"], 12.5);
        assert_eq!(output.json["items"][0]["tags"]["items"][0]["id"], 1);
        let automatic =
            context_on(&c, &output.references, &ctx.prefetch_fields, &configuration).unwrap();
        assert!(automatic["items"].as_array().unwrap().is_empty());
        assert!(info_fields(Some(vec!["path".into()]), &ctx).is_err());
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
        let named = faces.iter().find(|f| f["name"] == "Visible Name").unwrap();
        assert_eq!(output.json["items"][0]["people"]["regionType"], "face");
        assert_eq!(named["position"]["horizontal"], "left");
        assert_eq!(named["position"]["subject"], "face_center");
        assert_eq!(named["region"]["width"], json!(0.2f32));
        let args: ContentArgs = serde_json::from_value(json!({"file_id":1})).unwrap();
        let pixels = vec![ContentPart::from_binary_base64(
            "image/jpeg",
            "synthetic-pixels",
            None,
        )];
        let content = content_output_on(
            &c,
            &args,
            output.references[0].clone(),
            pixels,
            &["people".into()],
            &config::Configuration::default(),
        )
        .unwrap();
        assert_eq!(
            content.json["catalogue"]["items"][0]["people"]["items"][0]["name"],
            "Visible Name"
        );
        assert_eq!(
            content.json["catalogue"]["items"][0]["people"]["items"][0]["position"]["horizontal"],
            "left"
        );
        assert!(content.json["catalogue"]["items"][0].get("file").is_none());
        assert_eq!(content.parts.len(), 1);
        assert!(!content.json.to_string().contains(&src.path));
        let plain = content_output_on(
            &c,
            &args,
            output.references[0].clone(),
            Vec::new(),
            &[],
            &config::Configuration::default(),
        )
        .unwrap();
        assert!(
            plain.json["catalogue"]["items"]
                .as_array()
                .unwrap()
                .is_empty()
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
    fn assistant_face_positions_do_not_guess_rotated_or_invalid_coordinates() {
        let mut region = face_annotations::Region {
            x: 0.7,
            y: 0.1,
            width: 0.1,
            height: 0.1,
        };
        let position = face_position(&region, 1).unwrap();
        assert_eq!(position["horizontal"], "right");
        assert_eq!(position["vertical"], "top");
        for orientation in 2..=8 {
            assert!(face_position(&region, orientation).is_none());
        }
        region.width = f32::NAN;
        assert!(face_position(&region, 1).is_none());
        region.width = 0.5;
        assert!(face_position(&region, 1).is_none());
    }
    #[test]
    fn assistant_context_budget_keeps_people_regions_and_every_media_summary() {
        let face = json!({"name":"Fixture Person","state":"confirmed","region":{"x":0.1,"y":0.2,"width":0.2,"height":0.3},"position":{"horizontal":"left","center":[0.2,0.35]}});
        let mut output = json!({"status":"ok","items":[],"truncated":false});
        for id in 1..=16 {
            let mut item = json!({"fileId":id,"sourceStamp":"fixture","people":{"items":[face.clone()]},"objects":{"items":[{"id":"object-1","bounds":[0.1,0.2,0.3,0.4],"class":{"name":"person"}}]},"ocr":{"text":"长文字".repeat(12000),"items":[{"text":"长文字".repeat(1000),"bounds":[0.1,0.2,0.3,0.4]}]},"analysis":(0..12).map(|index|json!({"analysisId":format!("a-{index}"),"description":"长说明".repeat(3000)})).collect::<Vec<_>>()});
            assert!(fit_info_item(&mut item, (64 * 1024 - 512) / 16).unwrap());
            assert_eq!(item["people"]["items"][0], face);
            assert_eq!(
                item["objects"]["items"][0]["bounds"],
                json!([0.1, 0.2, 0.3, 0.4])
            );
            assert!(
                item["truncatedFields"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("analysis"))
            );
            output["items"].as_array_mut().unwrap().push(item);
        }
        assert_eq!(output["items"].as_array().unwrap().len(), 16);
        assert!(serde_json::to_vec(&output).unwrap().len() <= 64 * 1024);
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
        assert_eq!(result["regionType"], "object");
        assert_eq!(result["coordinateSpace"]["basis"], "decoded_image");
        assert_eq!(result["coordinateSpace"]["origin"], "top_left");
        assert_eq!(
            result["coordinateSpace"]["regionFormat"],
            "x_y_width_height"
        );
        assert_eq!(result["items"][0]["class"]["name"], "cat");
        assert_eq!(result["revision"], "revision-1");
        let document = json!({"text":"automatic","lines":[{"text":"automatic","confidence":0.8,"bounds":[0.1,0.2,0.3,0.1]}],"imageWidth":1000,"imageHeight":1000,"rotationDegrees":90,"deskewDegrees":2.5});
        c.execute("INSERT INTO ocr_index(file_id,profile,modified_at,size,document,text,error) VALUES(1,'ocr-profile',?1,?2,?3,'automatic',NULL)",params![src.modified,src.size,document.to_string()]).unwrap();
        c.execute("INSERT INTO ocr_annotations VALUES(1,?1,?2,'ocr-profile','Fixture',?3,?4,?3,'human','ocr-revision')",params![src.modified,src.size,document.to_string(),json!([{"text":"human","ignored":false}]).to_string()]).unwrap();
        let result = ocr_on(&c, 1, "ocr-profile").unwrap();
        assert_eq!(result["source"], "manual");
        assert_eq!(result["regionType"], "text_line");
        assert_eq!(result["coordinateSpace"]["basis"], "ocr_document");
        assert_eq!(result["coordinateSpace"]["origin"], "top_left");
        assert_eq!(
            result["coordinateSpace"]["regionFormat"],
            "x_y_width_height"
        );
        assert_eq!(
            result["processing"]["boundsMapping"],
            "already_mapped_to_input"
        );
        assert_eq!(result["processing"]["rotationDegrees"], 90);
        assert_eq!(result["processing"]["deskewDegrees"], 2.5);
        assert_eq!(
            result["items"][0]["bounds"],
            json!([0.1f32, 0.2f32, 0.3f32, 0.1f32])
        );
        assert!(result["coordinateSpace"].get("rotationDegrees").is_none());
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
