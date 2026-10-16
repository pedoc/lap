//! Human-owned text snapshots survive model/index changes and never modify media.
use super::{adapters::ocr::Document, types::Task};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LineEdit {
    pub text: String,
    #[serde(default)]
    pub ignored: bool,
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
    if edits.len() != source.lines.len()
        || edits.len() > 2000
        || edits.iter().any(|line| line.text.len() > 65536)
        || edits.iter().map(|line| line.text.len()).sum::<usize>() > 1024 * 1024
    {
        return Err("Text edits do not match the source lines or exceed size limits".into());
    }
    let mut document = source.clone();
    document.lines = source
        .lines
        .iter()
        .zip(edits)
        .filter(|(_, edit)| !edit.ignored && !edit.text.trim().is_empty())
        .map(|(line, edit)| {
            let mut line = line.clone();
            line.text = edit.text.clone();
            line
        })
        .collect();
    document.text = document
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
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
        || view.source_key.as_deref() != Some(source_key)
    {
        return Err("Recognition or manual text changed; reload before saving your draft".into());
    }
    let source = view
        .annotation
        .as_ref()
        .map(|a| a.source.clone())
        .or(view.model_document)
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
        save_on(
            &mut *crate::t_sqlite::open_conn()?,
            file_id,
            &profile,
            &model.definition.name,
            &expected_revision,
            &source_key,
            &lines,
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
