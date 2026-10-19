//! Human-owned text snapshots survive model/index changes and never modify media.
use super::{
    adapters::ocr::{Document, TextLine},
    types::Task,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LineEdit {
    pub text: String,
    #[serde(default)]
    pub ignored: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[f32; 4]>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub base_profile: String,
    pub model_name: String,
    pub source: Document,
    pub lines: Vec<LineEdit>,
    pub revision: String,
}
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PhotoText {
    pub indexed: bool,
    pub document: Option<Document>,
    pub error: Option<String>,
    pub annotation: Option<Annotation>,
    pub model_document: Option<Document>,
    pub source_key: Option<String>,
    pub profile: String,
    pub manual_source_key: Option<String>,
    pub source_version: Option<String>,
}
pub fn ensure_schema(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS ocr_annotations(file_id INTEGER PRIMARY KEY REFERENCES afiles(id) ON DELETE CASCADE,modified_at INTEGER,size INTEGER NOT NULL,base_profile TEXT NOT NULL,model_name TEXT NOT NULL,source TEXT NOT NULL,edits TEXT NOT NULL,document TEXT NOT NULL,text TEXT NOT NULL,revision TEXT NOT NULL); CREATE TRIGGER IF NOT EXISTS ocr_annotations_delete AFTER DELETE ON afiles BEGIN DELETE FROM ocr_annotations WHERE file_id=OLD.id; END;").map_err(|e|e.to_string())
}
fn stamp(c: &Connection, id: i64) -> Result<Option<(Option<i64>, i64)>, String> {
    c.query_row(
        &format!(
            "SELECT a.modified_at,a.size {} AND a.id=?1",
            super::ocr::base()
        ),
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
    .map_err(|e| e.to_string())
}
fn annotation_on(
    c: &Connection,
    id: i64,
    modified: Option<i64>,
    size: i64,
) -> Result<Option<Annotation>, String> {
    let row=c.query_row("SELECT base_profile,model_name,source,edits,revision FROM ocr_annotations WHERE file_id=?1 AND modified_at IS ?2 AND size=?3",params![id,modified,size],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?))).optional().map_err(|e|e.to_string())?;
    row.map(|(base_profile, model_name, source, lines, revision)| {
        Ok(Annotation {
            base_profile,
            model_name,
            source: serde_json::from_str(&source).map_err(|e| e.to_string())?,
            lines: serde_json::from_str(&lines).map_err(|e| e.to_string())?,
            revision,
        })
    })
    .transpose()
}
fn key(doc: &Document) -> Result<String, String> {
    Ok(
        blake3::hash(&serde_json::to_vec(doc).map_err(|e| e.to_string())?)
            .to_hex()
            .to_string(),
    )
}
fn apply(source: &Document, edits: &[LineEdit]) -> Result<Document, String> {
    if edits.len() < source.lines.len()
        || edits.len() > 2000
        || edits.iter().any(|line| line.text.len() > 65536)
        || edits.iter().map(|line| line.text.len()).sum::<usize>() > 1024 * 1024
    {
        return Err("Text edits do not match the source lines or exceed size limits".into());
    }
    let mut document = source.clone();
    document.lines.clear();
    for (index, edit) in edits.iter().enumerate() {
        let mut line = if let Some(original) = source.lines.get(index) {
            if edit.bounds.is_some_and(|bounds| bounds != original.bounds) {
                return Err("Original OCR line geometry cannot be replaced".into());
            }
            original.clone()
        } else {
            let bounds = edit
                .bounds
                .ok_or("A manually added line needs a text box")?;
            super::ocr_extract::crop_pixels(
                bounds.map(f64::from),
                source.image_width,
                source.image_height,
            )?;
            // Manual text is human-owned, not a fabricated high-confidence model result.
            TextLine {
                text: String::new(),
                confidence: 0.,
                bounds,
            }
        };
        if !edit.ignored && !edit.text.trim().is_empty() {
            line.text = edit.text.clone();
            document.lines.push(line);
        }
    }
    document.text = document
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if document.text.len() > 1024 * 1024 {
        return Err("OCR text exceeds 1 MiB limit".into());
    }
    Ok(document)
}
pub fn view_on(c: &Connection, id: i64, profile: &str) -> Result<PhotoText, String> {
    let Some((modified, size)) = stamp(c, id)? else {
        return Ok(PhotoText::default());
    };
    let raw=c.query_row("SELECT document,error FROM ocr_index WHERE file_id=?1 AND profile=?2 AND modified_at IS ?3 AND size=?4",params![id,profile,modified,size],|r|Ok((r.get::<_,Option<String>>(0)?,r.get::<_,Option<String>>(1)?))).optional().map_err(|e|e.to_string())?;
    let (raw, error) = raw.unwrap_or_default();
    let model_document: Option<Document> = raw
        .map(|doc| serde_json::from_str(&doc).map_err(|e| e.to_string()))
        .transpose()?;
    let annotation = annotation_on(c, id, modified, size)?;
    let source = annotation
        .as_ref()
        .map(|a| &a.source)
        .or(model_document.as_ref());
    let source_key = source.map(key).transpose()?;
    let document = if let Some(annotation) = &annotation {
        Some(apply(&annotation.source, &annotation.lines)?)
    } else {
        model_document.clone()
    };
    Ok(PhotoText {
        indexed: document.is_some(),
        document,
        error: if annotation.is_some() { None } else { error },
        annotation,
        model_document,
        source_key,
        profile: profile.into(),
        manual_source_key: None,
        source_version: None,
    })
}
// Correlated union keeps one parameter for the profile and one for a search term.
// Human text shadows model output only for the exact same source version.
pub fn effective_rows(profile: &str) -> String {
    let valid = "ed.file_id=a.id AND ed.modified_at IS a.modified_at AND ed.size=a.size";
    format!(
        "SELECT oi.text,oi.error FROM ocr_index oi WHERE oi.file_id=a.id AND oi.profile={profile} AND oi.modified_at IS a.modified_at AND oi.size=a.size AND NOT EXISTS(SELECT 1 FROM ocr_annotations ed WHERE {valid}) UNION ALL SELECT ed.text,NULL AS error FROM ocr_annotations ed WHERE {valid}"
    )
}
fn save_on(
    c: &mut Connection,
    id: i64,
    profile: &str,
    model_name: &str,
    expected_revision: &str,
    source_key: &str,
    edits: &[LineEdit],
) -> Result<(), String> {
    save_with_source_on(
        c,
        id,
        profile,
        model_name,
        expected_revision,
        source_key,
        edits,
        None,
    )
}
fn save_with_source_on(
    c: &mut Connection,
    id: i64,
    profile: &str,
    model_name: &str,
    expected_revision: &str,
    source_key: &str,
    edits: &[LineEdit],
    manual_source: Option<Document>,
) -> Result<(), String> {
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let view = view_on(&tx, id, profile)?;
    if view
        .annotation
        .as_ref()
        .map(|a| a.revision.as_str())
        .unwrap_or("")
        != expected_revision
        || view
            .source_key
            .clone()
            .or(manual_source.as_ref().map(key).transpose()?)
            .as_deref()
            != Some(source_key)
    {
        return Err("Recognition or manual text changed; reload before saving your draft".into());
    }
    let source = view
        .annotation
        .as_ref()
        .map(|a| a.source.clone())
        .or(view.model_document)
        .or(manual_source)
        .ok_or("Recognize this photo before editing its text")?;
    let document = apply(&source, edits)?;
    let (modified, size) = stamp(&tx, id)?.ok_or("Photo is no longer visible")?;
    let base_profile = view
        .annotation
        .as_ref()
        .map(|a| a.base_profile.as_str())
        .unwrap_or(profile);
    let model_name = view
        .annotation
        .as_ref()
        .map(|a| a.model_name.as_str())
        .unwrap_or(model_name);
    tx.execute("INSERT INTO ocr_annotations(file_id,modified_at,size,base_profile,model_name,source,edits,document,text,revision) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(file_id) DO UPDATE SET modified_at=excluded.modified_at,size=excluded.size,base_profile=excluded.base_profile,model_name=excluded.model_name,source=excluded.source,edits=excluded.edits,document=excluded.document,text=excluded.text,revision=excluded.revision",params![id,modified,size,base_profile,model_name,serde_json::to_string(&source).map_err(|e|e.to_string())?,serde_json::to_string(edits).map_err(|e|e.to_string())?,serde_json::to_string(&document).map_err(|e|e.to_string())?,document.text,uuid::Uuid::new_v4().to_string()]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn save_ocr_text(
    app_handle: AppHandle,
    library_id: String,
    file_id: i64,
    profile: String,
    expected_revision: String,
    source_key: String,
    lines: Vec<LineEdit>,
    manual_source_key: Option<String>,
) -> Result<(), String> {
    if file_id <= 0 || source_key.len() != 64 || lines.len() > 2000 {
        return Err("Invalid OCR edit request".into());
    }
    let event_library = library_id.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        super::ocr::check_library(&library_id)?;
        let model = super::settings::active(Task::Ocr)?;
        if model.profile() != profile {
            return Err("OCR model changed; reload before saving".into());
        }
        let manual_source = if let Some(expected) = manual_source_key {
            let source = super::media::source_on(&*crate::t_sqlite::open_conn()?, file_id)?;
            Some(empty_manual_source(&source, &expected)?)
        } else {
            None
        };
        if manual_source.is_none() {
            return save_on(
                &mut *crate::t_sqlite::open_conn()?,
                file_id,
                &profile,
                &model.definition.name,
                &expected_revision,
                &source_key,
                &lines,
            );
        }
        save_with_source_on(
            &mut *crate::t_sqlite::open_conn()?,
            file_id,
            &profile,
            &model.definition.name,
            &expected_revision,
            &source_key,
            &lines,
            manual_source,
        )
    })
    .await
    .map_err(|e| e.to_string())??;
    app_handle
        .emit(
            "ocr-annotations-changed",
            serde_json::json!({"libraryId":event_library,"fileId":file_id}),
        )
        .map_err(|e| e.to_string())
}
/// Prepare a read-only, source-checked empty snapshot. Adding manual text does
/// not require an installed model or write an index until explicitly saved.
#[tauri::command]
pub async fn prepare_ocr_manual(
    library_id: String,
    file_id: i64,
    profile: String,
    preview_aspect: f64,
) -> Result<PhotoText, String> {
    if file_id <= 0 || !preview_aspect.is_finite() || preview_aspect <= 0. {
        return Err("Invalid manual OCR request".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read().map_err(|e|e.to_string())?;
        super::ocr::check_library(&library_id)?;
        if super::settings::active(Task::Ocr)?.profile() != profile { return Err("OCR model changed; reload before editing".into()); }
        let source = super::media::source_on(&*crate::t_sqlite::open_conn()?, file_id)?;
        source.verify()?;
        let mut result = view_on(&*crate::t_sqlite::open_conn()?, file_id, &profile)?;
        let version = source.stamp_key()?;
        if result.document.is_none() {
            let empty = empty_manual_source(&source, &version)?;
            result.source_key = Some(key(&empty)?);
            result.model_document = Some(empty.clone());
            result.document = Some(empty);
            result.error = None;
            result.manual_source_key = Some(version.clone());
        }
        let document = result.document.as_ref().ok_or("No OCR source dimensions")?;
        if document.image_height == 0 || ((document.image_width as f64 / document.image_height as f64) / preview_aspect - 1.).abs() > 0.02 {
            return Err("Preview proportions differ from the OCR source; switch to a matching preview before adding text".into());
        }
        result.source_version = Some(version);
        Ok(result)
    }).await.map_err(|e|e.to_string())?
}
fn empty_manual_source(source: &super::media::Source, expected: &str) -> Result<Document, String> {
    if source.stamp_key()? != expected {
        return Err("Photo changed; reload before saving manual text".into());
    }
    let image = source.load()?;
    source.verify()?;
    Ok(Document {
        image_width: image.width(),
        image_height: image.height(),
        ..Default::default()
    })
}
#[tauri::command]
pub async fn reset_ocr_text(
    app_handle: AppHandle,
    library_id: String,
    file_id: i64,
    expected_revision: String,
) -> Result<(), String> {
    let event_library = library_id.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        super::ocr::check_library(&library_id)?;
        let c = crate::t_sqlite::open_conn()?;
        if stamp(&c, file_id)?.is_none() {
            return Err("Photo is no longer visible".into());
        }
        let changed = c
            .execute(
                "DELETE FROM ocr_annotations WHERE file_id=?1 AND revision=?2",
                params![file_id, expected_revision],
            )
            .map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("Manual text changed; reload before restoring model output".into());
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;
    app_handle
        .emit(
            "ocr-annotations-changed",
            serde_json::json!({"libraryId":event_library,"fileId":file_id}),
        )
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::super::adapters::ocr::TextLine;
    use super::*;
    fn manual_line(text: &str, bounds: [f32; 4]) -> LineEdit {
        LineEdit {
            text: text.into(),
            ignored: false,
            bounds: Some(bounds),
        }
    }
    #[test]
    fn missed_text_can_be_added_before_recognition_and_is_searchable_across_models() {
        let mut c = db();
        let empty = Document {
            image_width: 100,
            image_height: 100,
            ..Default::default()
        };
        assert!(!view_on(&c, 1, "a").unwrap().indexed);
        let edits = vec![manual_line("漏检 invoice", [0.1, 0.2, 0.7, 0.2])];
        save_with_source_on(
            &mut c,
            1,
            "a",
            "A",
            "",
            &key(&empty).unwrap(),
            &edits,
            Some(empty),
        )
        .unwrap();
        let result = view_on(&c, 1, "b").unwrap();
        assert!(result.indexed);
        let doc = result.document.unwrap();
        assert_eq!(doc.text, "漏检 invoice");
        assert_eq!(doc.lines[0].confidence, 0.);
        assert_eq!(doc.lines[0].bounds, [0.1, 0.2, 0.7, 0.2]);
        let annotation = result.annotation.unwrap();
        assert!(annotation.source.lines.is_empty());
        let sql = format!(
            "SELECT COUNT(*) {} AND {}",
            super::super::ocr::base(),
            super::super::ocr::search_predicate()
        );
        assert_eq!(
            c.query_row(&sql, ["%invoice%"], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        c.execute("INSERT INTO media_states VALUES(1,1,NULL)", [])
            .unwrap();
        assert!(!view_on(&c, 1, "b").unwrap().indexed);
        assert!(
            save_on(
                &mut c,
                1,
                "b",
                "B",
                &annotation.revision,
                result.source_key.as_ref().unwrap(),
                &annotation.lines
            )
            .is_err()
        );
        assert_eq!(
            c.query_row(&sql, ["%invoice%"], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn appended_lines_preserve_the_immutable_source_and_can_be_edited_or_ignored() {
        let mut c = db();
        seed(&c, "a", "original");
        let original = view_on(&c, 1, "a").unwrap();
        let mut edits = vec![
            LineEdit {
                text: "original".into(),
                ignored: false,
                bounds: None,
            },
            manual_line("region", [0.2, 0.3, 0.4, 0.1]),
        ];
        save_on(
            &mut c,
            1,
            "a",
            "A",
            "",
            original.source_key.as_ref().unwrap(),
            &edits,
        )
        .unwrap();
        let saved = view_on(&c, 1, "a").unwrap();
        assert_eq!(saved.document.unwrap().text, "original\nregion");
        assert_eq!(saved.annotation.as_ref().unwrap().source.lines.len(), 1);
        assert_eq!(saved.source_key, original.source_key);
        edits[1].text = "corrected region".into();
        save_on(
            &mut c,
            1,
            "a",
            "A",
            &saved.annotation.unwrap().revision,
            saved.source_key.as_ref().unwrap(),
            &edits,
        )
        .unwrap();
        let saved = view_on(&c, 1, "a").unwrap();
        edits[1].ignored = true;
        save_on(
            &mut c,
            1,
            "a",
            "A",
            &saved.annotation.unwrap().revision,
            saved.source_key.as_ref().unwrap(),
            &edits,
        )
        .unwrap();
        assert_eq!(
            view_on(&c, 1, "a").unwrap().document.unwrap().text,
            "original"
        );
    }
    #[test]
    fn manual_boxes_are_bounded_and_cannot_replace_original_geometry() {
        let empty = Document {
            image_width: 100,
            image_height: 100,
            ..Default::default()
        };
        for bounds in [
            [f32::NAN, 0., 1., 1.],
            [-0.1, 0., 1., 1.],
            [0.9, 0., 0.2, 1.],
            [0., 0., 0., 1.],
        ] {
            assert!(apply(&empty, &[manual_line("bad", bounds)]).is_err());
        }
        assert!(
            apply(
                &empty,
                &[LineEdit {
                    text: "no box".into(),
                    ignored: false,
                    bounds: None
                }]
            )
            .is_err()
        );
        let source = Document {
            lines: vec![TextLine {
                text: "source".into(),
                confidence: 0.8,
                bounds: [0., 0., 1., 1.],
            }],
            ..empty.clone()
        };
        assert!(apply(&source, &[manual_line("bad", [0., 0., 0.5, 0.5])]).is_err());
        assert!(apply(&source, &[]).is_err());
        let mut large = vec![manual_line(&"a".repeat(65536), [0., 0., 1., 1.]); 16];
        assert!(
            apply(&empty, &large).is_err(),
            "newline bytes also count toward the document limit"
        );
        large.push(manual_line("x", [0., 0., 1., 1.]));
        assert!(apply(&empty, &large).is_err());
    }
    #[test]
    fn empty_manual_source_uses_oriented_read_only_dimensions_and_checks_source_version() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("objects-validation")
            .join(format!("ocr-manual-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("fixture.png");
        image::DynamicImage::new_rgb8(100, 60).save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let info = crate::t_utils::FileInfo::new(path.to_str().unwrap()).unwrap();
        let source = super::super::media::Source {
            path: path.to_string_lossy().into_owned(),
            modified: info.modified,
            size: info.file_size,
            width: 100,
            height: 60,
            orientation: 6,
        };
        assert!(empty_manual_source(&source, "stale").is_err());
        let doc = empty_manual_source(&source, &source.stamp_key().unwrap()).unwrap();
        assert_eq!((doc.image_width, doc.image_height), (60, 100));
        assert!(doc.lines.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::write(&path, b"changed").unwrap();
        assert!(empty_manual_source(&source, &source.stamp_key().unwrap()).is_err());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT,file_types INTEGER,small_image_filter INTEGER,excluded_folders TEXT);CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,path TEXT,is_excluded_from_search INTEGER DEFAULT 0);CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,file_type INTEGER,width INTEGER,height INTEGER,live_photo_video_id INTEGER,modified_at INTEGER,size INTEGER);CREATE TABLE media_states(file_id INTEGER PRIMARY KEY,hidden INTEGER DEFAULT 0,deleted_at INTEGER);CREATE TABLE ai_index_profiles(task TEXT PRIMARY KEY,fingerprint TEXT NOT NULL);INSERT INTO albums VALUES(1,'/photos',7,0,'[]');INSERT INTO afolders VALUES(1,1,'/photos',0);INSERT INTO afiles VALUES(1,1,1,100,100,NULL,10,20);INSERT INTO ai_index_profiles VALUES('ocr','a');").unwrap();
        super::super::ocr::ensure_schema(&c).unwrap();
        c
    }
    fn seed(c: &Connection, profile: &str, text: &str) {
        let doc = Document {
            text: text.into(),
            lines: vec![TextLine {
                text: text.into(),
                confidence: 0.8,
                bounds: [0., 0., 1., 1.],
            }],
            image_width: 100,
            image_height: 100,
            ..Default::default()
        };
        c.execute(
            "INSERT OR REPLACE INTO ocr_index VALUES(1,?1,10,20,?2,?3,NULL)",
            params![profile, text, serde_json::to_string(&doc).unwrap()],
        )
        .unwrap();
    }
    #[test]
    fn human_edits_shadow_search_and_survive_model_switches_and_reruns() {
        let mut c = db();
        seed(&c, "a", "wrong");
        let view = view_on(&c, 1, "a").unwrap();
        save_on(
            &mut c,
            1,
            "a",
            "Model A",
            "",
            view.source_key.as_ref().unwrap(),
            &[LineEdit {
                text: "correct 中文".into(),
                ignored: false,
                bounds: None,
            }],
        )
        .unwrap();
        seed(&c, "a", "rerun");
        seed(&c, "b", "different splitting");
        let view = view_on(&c, 1, "b").unwrap();
        assert_eq!(view.document.unwrap().text, "correct 中文");
        assert_eq!(view.annotation.unwrap().source.text, "wrong");
        let sql = format!(
            "SELECT COUNT(*) {} AND {}",
            super::super::ocr::base(),
            super::super::ocr::search_predicate()
        );
        assert_eq!(
            c.query_row(&sql, ["%correct%"], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row(&sql, ["%rerun%"], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn source_changes_hidden_media_and_stale_revisions_cannot_overwrite_edits() {
        let mut c = db();
        seed(&c, "a", "old");
        let original = view_on(&c, 1, "a").unwrap();
        let edits = [LineEdit {
            text: "new".into(),
            ignored: false,
            bounds: None,
        }];
        save_on(
            &mut c,
            1,
            "a",
            "A",
            "",
            original.source_key.as_ref().unwrap(),
            &edits,
        )
        .unwrap();
        assert!(
            save_on(
                &mut c,
                1,
                "a",
                "A",
                "",
                original.source_key.as_ref().unwrap(),
                &edits
            )
            .is_err()
        );
        c.execute("UPDATE afiles SET size=21", []).unwrap();
        assert!(!view_on(&c, 1, "a").unwrap().indexed);
        assert!(
            save_on(
                &mut c,
                1,
                "a",
                "A",
                "",
                original.source_key.as_ref().unwrap(),
                &edits
            )
            .is_err()
        );
        c.execute("INSERT INTO media_states VALUES(1,1,NULL)", [])
            .unwrap();
        assert!(!view_on(&c, 1, "a").unwrap().indexed);
    }
    #[test]
    fn ignored_text_is_removed_but_original_snapshot_can_be_restored() {
        let mut c = db();
        seed(&c, "a", "texture false positive");
        let old = view_on(&c, 1, "a").unwrap();
        save_on(
            &mut c,
            1,
            "a",
            "A",
            "",
            old.source_key.as_ref().unwrap(),
            &[LineEdit {
                text: "texture false positive".into(),
                ignored: true,
                bounds: None,
            }],
        )
        .unwrap();
        let edited = view_on(&c, 1, "a").unwrap();
        assert_eq!(edited.document.unwrap().text, "");
        assert_eq!(edited.annotation.unwrap().source.lines.len(), 1);
        c.execute("DELETE FROM ocr_annotations", []).unwrap();
        assert_eq!(
            view_on(&c, 1, "a").unwrap().document.unwrap().text,
            "texture false positive"
        );
    }
}
