//! Explicit, resumable local OCR indexing. Derived text never changes originals.
use super::{
    adapters::ocr::{Document, PaddleOcr},
    types::Task,
};
use crate::t_sqlite::AFile;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Emitter};

static JOB: Mutex<Option<Arc<Job>>> = Mutex::new(None);
static INFERENCE_BUSY: AtomicBool = AtomicBool::new(false);
pub(crate) struct InferenceLease;
impl Drop for InferenceLease {
    fn drop(&mut self) {
        INFERENCE_BUSY.store(false, Ordering::SeqCst);
    }
}
pub(crate) fn try_inference() -> Result<InferenceLease, String> {
    INFERENCE_BUSY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .map(|_| InferenceLease)
        .map_err(|_| "OCR is busy; stop the current task before starting another".into())
}
struct Job {
    cancel: AtomicBool,
    status: Mutex<Status>,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub library_id: String,
    pub job_id: String,
    pub running: bool,
    pub stopping: bool,
    pub total: usize,
    pub completed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub error: Option<String>,
    pub cancelled: bool,
    pub scope: String,
    pub scope_name: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub status: Status,
    pub eligible: i64,
    pub indexed: i64,
    pub failed: i64,
    pub with_text: i64,
    pub without_text: i64,
    pub pending: i64,
    pub model_name: String,
    pub model_installed: bool,
    pub profile: String,
    pub sessions: Vec<super::runtime::SessionReport>,
}
pub fn is_running() -> bool {
    INFERENCE_BUSY.load(Ordering::SeqCst)
        || JOB
            .lock()
            .map(|j| {
                j.as_ref()
                    .is_some_and(|j| j.status.lock().is_ok_and(|s| s.running))
            })
            .unwrap_or(true)
}

pub fn ensure_schema(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS ocr_index (
        file_id INTEGER NOT NULL REFERENCES afiles(id) ON DELETE CASCADE,
        profile TEXT NOT NULL, modified_at INTEGER, size INTEGER NOT NULL,
        text TEXT NOT NULL DEFAULT '', document TEXT, error TEXT,
        PRIMARY KEY(file_id,profile)
    ); CREATE INDEX IF NOT EXISTS idx_ocr_profile ON ocr_index(profile);
    CREATE TRIGGER IF NOT EXISTS ocr_index_delete AFTER DELETE ON afiles BEGIN DELETE FROM ocr_index WHERE file_id=OLD.id; END;").map_err(|e|e.to_string())
}
// EXISTS avoids joins changing counts / grouping. The caller must also enforce normal visibility.
pub fn search_predicate() -> &'static str {
    "EXISTS (SELECT 1 FROM ocr_index oi JOIN ai_index_profiles op ON op.task='ocr' AND op.fingerprint=oi.profile WHERE oi.file_id=a.id AND oi.modified_at IS a.modified_at AND oi.size=a.size AND oi.error IS NULL AND oi.text LIKE ? ESCAPE '\\' COLLATE NOCASE)"
}
// Shared by normal photo pagination, counts, grouping and OCR task selection.
// An old model / changed source is pending, not an empty or failed result.
pub fn workspace_condition(state: &str) -> String {
    let valid = "oi.file_id=a.id AND oi.profile=(SELECT fingerprint FROM ai_index_profiles WHERE task='ocr') AND oi.modified_at IS a.modified_at AND oi.size=a.size";
    match state {
        "all" => "1".into(),
        "text" => format!(
            "EXISTS (SELECT 1 FROM ocr_index oi WHERE {valid} AND oi.error IS NULL AND oi.text <> '')"
        ),
        "empty" => format!(
            "EXISTS (SELECT 1 FROM ocr_index oi WHERE {valid} AND oi.error IS NULL AND oi.text = '')"
        ),
        "failed" => {
            format!("EXISTS (SELECT 1 FROM ocr_index oi WHERE {valid} AND oi.error IS NOT NULL)")
        }
        "pending" => format!("NOT EXISTS (SELECT 1 FROM ocr_index oi WHERE {valid})"),
        _ => "0".into(), // Never broaden an unknown filter into the entire library.
    }
}
// Bind the actual global selection, not a possibly stale per-library marker.
// This keeps library switching correct before a new index task has run.
pub fn workspace_query(state: &str, text: &str) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let profile = match super::settings::active(Task::Ocr) {
        Ok(model) => model.profile(),
        Err(_) => return ("0".into(), values),
    };
    let mut condition = workspace_condition(state);
    let marker = "(SELECT fingerprint FROM ai_index_profiles WHERE task='ocr')";
    if condition.contains(marker) {
        condition = condition.replace(marker, "?");
        values.push(Box::new(profile.clone()));
    }
    if !text.is_empty() {
        condition.push_str(" AND EXISTS (SELECT 1 FROM ocr_index oi WHERE oi.file_id=a.id AND oi.profile=? AND oi.modified_at IS a.modified_at AND oi.size=a.size AND oi.error IS NULL AND oi.text LIKE ? ESCAPE '\\' COLLATE NOCASE)");
        values.push(Box::new(profile));
        let text = text
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        values.push(Box::new(format!("%{text}%")));
    }
    (condition, values)
}
fn visible() -> String {
    format!(
        "{} AND {} AND {}",
        AFile::search_exclusion_condition("b"),
        AFile::live_photo_companion_exclusion_condition(),
        AFile::album_query_predicate("a")
    )
}
pub(crate) fn base() -> String {
    format!(
        "FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE a.file_type IN (1,3) AND {}",
        visible()
    )
}
pub(crate) fn check_library(library: &str) -> Result<(), String> {
    if crate::t_config::current_library_id()? != library {
        return Err("Library changed; OCR stopped. Resume in the original library".into());
    }
    Ok(())
}
fn current_status(library: &str) -> Status {
    JOB.lock()
        .ok()
        .and_then(|j| {
            j.as_ref().and_then(|j| {
                j.status
                    .lock()
                    .ok()
                    .filter(|s| s.library_id == library)
                    .map(|s| s.clone())
            })
        })
        .unwrap_or(Status {
            library_id: library.into(),
            ..Default::default()
        })
}
// One aggregate scan per refresh, rather than one whole-library scan per tab.
fn coverage_on(c: &Connection, profile: &str) -> Result<(i64, i64, i64, i64, i64), String> {
    let query = format!("SELECT COUNT(*), COALESCE(SUM(oi.file_id IS NOT NULL AND oi.error IS NULL),0), COALESCE(SUM(oi.error IS NOT NULL),0), COALESCE(SUM(oi.error IS NULL AND oi.text<>''),0), COALESCE(SUM(oi.error IS NULL AND oi.text=''),0)
        FROM afiles a JOIN afolders b ON b.id=a.folder_id LEFT JOIN ocr_index oi ON oi.file_id=a.id AND oi.profile=?1 AND oi.modified_at IS a.modified_at AND oi.size=a.size
        WHERE a.file_type IN (1,3) AND {}", visible());
    c.query_row(&query, [profile], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
    })
    .map_err(|e| e.to_string())
}
fn view_on(c: &Connection, library: &str) -> Result<View, String> {
    let model = super::settings::active(Task::Ocr)?;
    let profile = model.profile();
    let (eligible, indexed, failed, with_text, without_text) = coverage_on(c, &profile)?;
    Ok(View {
        status: current_status(library),
        eligible,
        indexed,
        failed,
        with_text,
        without_text,
        pending: eligible - indexed - failed,
        model_name: model.definition.name.clone(),
        model_installed: super::assets::installed(&model.definition),
        profile,
        sessions: super::runtime::reports(&model),
    })
}
#[tauri::command]
pub async fn get_ocr_status(library_id: String) -> Result<View, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        check_library(&library_id)?;
        let c = crate::t_sqlite::open_conn()?;
        view_on(&c, &library_id)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn cancel_ocr_index(library_id: String, job_id: String) -> Result<(), String> {
    check_library(&library_id)?;
    let job = JOB
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("No OCR job")?;
    let mut status = job.status.lock().map_err(|e| e.to_string())?;
    if status.library_id != library_id || status.job_id != job_id {
        return Err("Stale OCR job".into());
    }
    job.cancel.store(true, Ordering::SeqCst);
    status.stopping = status.running;
    Ok(())
}
#[tauri::command]
pub async fn start_ocr_index(
    app_handle: AppHandle,
    library_id: String,
    force: bool,
    file_ids: Option<Vec<i64>>,
    retry_failed: Option<bool>,
    folder_id: Option<i64>,
) -> Result<Status, String> {
    let retry_failed = retry_failed.unwrap_or(false);
    if force && retry_failed {
        return Err("Choose either rebuild or retry failed photos".into());
    }
    if folder_id.is_some() && file_ids.is_some() {
        return Err("Choose one OCR scope".into());
    }
    if let Some(ids) = &file_ids {
        if ids.is_empty() || ids.len() > 10000 || ids.iter().any(|id| *id <= 0) {
            return Err("Select 1 to 10000 valid photos for OCR".into());
        }
    }
    tauri::async_runtime::spawn_blocking(move||{
        let _guard=crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read().map_err(|e|e.to_string())?;
        check_library(&library_id)?;
        let lease=try_inference()?;
        let model=super::settings::active(Task::Ocr)?;
        super::assets::verify(&model.definition)?;
        let mut slot=JOB.lock().map_err(|e|e.to_string())?;
        if slot.as_ref().is_some_and(|j|j.status.lock().is_ok_and(|s|s.running)) {return Err("An OCR job is already running".into());}
        let folder = folder_id.map(|id| folder_scope(&*crate::t_sqlite::open_conn()?, id)).transpose()?;
        let scope = if file_ids.is_some() { "selected" } else if folder.is_some() { "folder" } else { "library" };
        let initial=Status {scope:scope.into(),scope_name:folder.as_ref().map(|(_,path)|path.clone()),library_id:library_id.clone(),job_id:uuid::Uuid::new_v4().to_string(),running:true,..Default::default()};
        let job=Arc::new(Job {cancel:AtomicBool::new(false),status:Mutex::new(initial.clone())});*slot=Some(job.clone());drop(slot);
        tauri::async_runtime::spawn_blocking(move||{
            let _lease = lease;
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||run(&app_handle,&library_id,&model,force,retry_failed,file_ids,folder, &job)));
            if let Ok(mut status)=job.status.lock() {
                status.running=false;status.stopping=false;
                status.cancelled=job.cancel.load(Ordering::SeqCst);
                status.error=if status.cancelled { None } else { match result {Ok(Ok(()))=>None,Ok(Err(error))=>Some(error),Err(_)=>Some("OCR worker stopped unexpectedly; completed results are saved. Retry the remaining photos".into())} };
            }
            let _=app_handle.emit("ocr-index-updated",serde_json::json!({"libraryId":library_id,"finished":true}));
        });
        Ok(initial)
    }).await.map_err(|e|e.to_string())?
}
fn run(
    app: &AppHandle,
    library: &str,
    model: &super::types::ResolvedModel,
    force: bool,
    retry_failed: bool,
    ids: Option<Vec<i64>>,
    folder: Option<(i64, String)>,
    job: &Job,
) -> Result<(), String> {
    let c = {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        check_library(library)?;
        crate::t_sqlite::open_conn()?
    };
    super::profiles::ensure_on(&c, Task::Ocr, &model.profile())?;
    let profile = model.profile();
    let targets = target_ids_on(&c, &profile, force, retry_failed, ids, folder, &job.cancel)?;
    job.status.lock().map_err(|e| e.to_string())?.total = targets.len();
    if targets.is_empty() {
        return Ok(());
    }
    let mut backend = PaddleOcr::new(model)?;
    for id in targets {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        let check = || {
            check_library(library)?;
            if job.cancel.load(Ordering::SeqCst) {
                return Err(
                    "OCR stopped; completed results are saved. Resume to process the remainder"
                        .into(),
                );
            }
            Ok(())
        };
        check()?;
        let row=c.query_row(&format!("SELECT b.path || '/' || a.name,a.modified_at,a.size,a.width,a.height,COALESCE(a.e_orientation,1) {} AND a.id=?1",base()),[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<i64>>(1)?,r.get::<_,i64>(2)?,r.get::<_,u64>(3)?,r.get::<_,u64>(4)?,r.get::<_,i32>(5)?))).optional().map_err(|e|e.to_string())?;
        let Some((path, modified, size, width, height, orientation)) = row else {
            let mut s = job.status.lock().map_err(|e| e.to_string())?;
            s.completed += 1;
            continue;
        };
        let mut process = (|| {
            if width.saturating_mul(height) > 80_000_000 {
                return Err("Image exceeds 80 megapixel OCR decoding limit".into());
            }
            let before = crate::t_utils::FileInfo::new(&path)?;
            if before.modified != modified || before.file_size != size {
                return Err(
                    "Source file changed since its last scan; rescan this album before OCR".into(),
                );
            }
            let image = tauri::async_runtime::block_on(crate::t_image::load_oriented_image(
                &path,
                orientation,
            ))?;
            check()?;
            super::ocr_orientation::recognize(&mut backend, model, &image, None, &check)
        })();
        match crate::t_utils::FileInfo::new(&path) {
            Ok(after) if after.modified==modified && after.file_size==size => {},
            _ => process=Err("Source changed or became unavailable during OCR; rescan the album and retry this photo".into()),
        }
        check()?; // Cancellation is not an OCR failure; never stamp it as indexed.
        let (text, document, error) = match &process {
            Ok(doc) => (
                doc.text.clone(),
                Some(serde_json::to_string(doc).map_err(|e| e.to_string())?),
                None,
            ),
            Err(error) => (
                String::new(),
                None,
                Some(
                    error
                        .replace(&path, "[photo]")
                        .chars()
                        .take(1500)
                        .collect::<String>(),
                ),
            ),
        };
        // Single guarded SQL statement: source identity / visibility cannot change between check and insert.
        let valid = store_result(
            &c,
            id,
            &profile,
            modified,
            size,
            &text,
            document.as_deref(),
            error.as_deref(),
        )?;
        let mut status = job.status.lock().map_err(|e| e.to_string())?;
        status.completed += 1;
        if valid {
            if process.is_ok() {
                status.succeeded += 1;
            } else {
                status.failed += 1;
            }
        }
        drop(status);
        let _ = app.emit(
            "ocr-index-updated",
            serde_json::json!({"libraryId":library,"fileId":id}),
        );
    }
    Ok(())
}
fn store_result(
    c: &Connection,
    id: i64,
    profile: &str,
    modified: Option<i64>,
    size: i64,
    text: &str,
    document: Option<&str>,
    error: Option<&str>,
) -> Result<bool, String> {
    c.execute(&format!("INSERT INTO ocr_index(file_id,profile,modified_at,size,text,document,error)
        SELECT a.id,?2,?3,?4,?5,?6,?7 {} AND a.id=?1 AND a.modified_at IS ?3 AND a.size=?4
        ON CONFLICT(file_id,profile) DO UPDATE SET modified_at=excluded.modified_at,size=excluded.size,text=excluded.text,document=excluded.document,error=excluded.error",base()),params![id,profile,modified,size,text,document,error]).map(|n|n>0).map_err(|e|e.to_string())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoText {
    pub indexed: bool,
    pub document: Option<Document>,
    pub error: Option<String>,
}
#[tauri::command]
pub async fn get_photo_ocr(library_id: String, file_id: i64) -> Result<PhotoText, String> {
    tauri::async_runtime::spawn_blocking(move||{
        let _guard=crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read().map_err(|e|e.to_string())?;check_library(&library_id)?;
        let c=crate::t_sqlite::open_conn()?;let profile=super::settings::active(Task::Ocr)?.profile();
        let row=c.query_row(&format!("SELECT oi.document,oi.error FROM ocr_index oi JOIN afiles a ON a.id=oi.file_id JOIN afolders b ON b.id=a.folder_id WHERE a.id=?1 AND oi.profile=?2 AND oi.modified_at IS a.modified_at AND oi.size=a.size AND {}",visible()),params![file_id,profile],|r|Ok((r.get::<_,Option<String>>(0)?,r.get::<_,Option<String>>(1)?))).optional().map_err(|e|e.to_string())?;
        let (document,error)=row.unwrap_or_default();Ok(PhotoText {indexed:document.is_some(),document:document.map(|d|serde_json::from_str(&d).map_err(|e|e.to_string())).transpose()?,error})
    }).await.map_err(|e|e.to_string())?
}
fn target_ids_on(
    c: &Connection,
    profile: &str,
    force: bool,
    retry_failed: bool,
    ids: Option<Vec<i64>>,
    folder: Option<(i64, String)>,
    cancel: &AtomicBool,
) -> Result<Vec<i64>, String> {
    // Snapshot IDs only. Page source rows lazily, never retain a whole library's paths / decoded pixels.
    let mut query = format!("SELECT a.id {}", base());
    if retry_failed {
        query.push_str(" AND EXISTS (SELECT 1 FROM ocr_index oi WHERE oi.file_id=a.id AND oi.profile=?1 AND oi.modified_at IS a.modified_at AND oi.size=a.size AND oi.error IS NOT NULL)");
    } else if !force {
        query.push_str(" AND NOT EXISTS (SELECT 1 FROM ocr_index oi WHERE oi.file_id=a.id AND oi.profile=?1 AND oi.modified_at IS a.modified_at AND oi.size=a.size AND oi.error IS NULL)");
    }
    // Use bound path values and separator-aware prefixes; /trip must not match /trip2.
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if !force || retry_failed {
        values.push(Box::new(profile.to_owned()));
    }
    if let Some(ids) = ids {
        query.push_str(" AND a.id IN (SELECT value FROM json_each(?))");
        values.push(Box::new(
            serde_json::to_string(&ids).map_err(|e| e.to_string())?,
        ));
    }
    if let Some((album_id, path)) = folder {
        query.push_str(" AND b.album_id=? AND (b.path=? OR b.path LIKE ? ESCAPE '\\' OR b.path LIKE ? ESCAPE '\\')");
        let escaped = path
            .trim_end_matches(['/', '\\'])
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        values.extend([
            Box::new(album_id) as Box<dyn rusqlite::ToSql>,
            Box::new(path),
            Box::new(format!("{escaped}/%")),
            Box::new(format!("{escaped}\\\\%")),
        ]);
    }
    query.push_str(" ORDER BY a.id");
    let mut statement = c.prepare(&query).map_err(|e| e.to_string())?;
    let mut rows = statement
        .query(rusqlite::params_from_iter(values.iter()))
        .map_err(|e| e.to_string())?;
    let mut targets = Vec::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        if cancel.load(Ordering::SeqCst) {
            return Err("OCR stopped; resume to process the remainder".into());
        }
        let id: i64 = row.get(0).map_err(|e| e.to_string())?;
        targets.push(id);
    }
    drop(rows);
    drop(statement);
    Ok(targets)
}
fn folder_scope(c: &Connection, id: i64) -> Result<(i64, String), String> {
    c.query_row(
        "SELECT album_id, path FROM afolders WHERE id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .map_err(|e| format!("OCR folder is unavailable: {e}"))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub file_id: i64,
    pub state: String,
    pub excerpt: String,
}
fn summaries_on(
    c: &Connection,
    ids: &[i64],
    text: &str,
    profile: &str,
) -> Result<Vec<Summary>, String> {
    let ids = serde_json::to_string(ids).map_err(|e| e.to_string())?;
    let sql = format!(
        "SELECT a.id, oi.text, oi.error FROM afiles a JOIN afolders b ON b.id=a.folder_id LEFT JOIN ocr_index oi ON oi.file_id=a.id AND oi.profile=?2 AND oi.modified_at IS a.modified_at AND oi.size=a.size WHERE a.file_type IN (1,3) AND {} AND a.id IN (SELECT value FROM json_each(?1))",
        visible()
    );
    let mut statement = c.prepare(&sql).map_err(|e| e.to_string())?;
    statement
        .query_map(params![ids, profile], |r| {
            let value: Option<String> = r.get(1)?;
            let error: Option<String> = r.get(2)?;
            let state = if error.is_some() {
                "failed"
            } else if value.is_none() {
                "pending"
            } else if value.as_deref() == Some("") {
                "empty"
            } else {
                "text"
            };
            // Return one bounded line, not a whole document for every thumbnail.
            let value = error.or(value).unwrap_or_default();
            let line = value
                .lines()
                .find(|line| !text.is_empty() && line.to_lowercase().contains(&text.to_lowercase()))
                .or_else(|| value.lines().next())
                .unwrap_or("");
            let chars: Vec<char> = line.chars().collect();
            let start = if text.is_empty() {
                0
            } else {
                let lower = line.to_lowercase();
                lower
                    .find(&text.to_lowercase())
                    .map(|byte| lower[..byte].chars().count().saturating_sub(24))
                    .unwrap_or(0)
                    .min(chars.len())
            };
            let excerpt = format!(
                "{}{}{}",
                if start > 0 { "…" } else { "" },
                chars.iter().skip(start).take(160).collect::<String>(),
                if chars.len() > start + 160 { "…" } else { "" }
            );
            Ok(Summary {
                file_id: r.get(0)?,
                state: state.into(),
                excerpt,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn get_ocr_summaries(
    library_id: String,
    file_ids: Vec<i64>,
    text: String,
) -> Result<Vec<Summary>, String> {
    if file_ids.len() > 1000 || text.len() > 4096 {
        return Err("OCR summary request is too large".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        check_library(&library_id)?;
        summaries_on(
            &*crate::t_sqlite::open_conn()?,
            &file_ids,
            &text,
            &super::settings::active(Task::Ocr)?.profile(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT,file_types INTEGER,small_image_filter INTEGER,excluded_folders TEXT);
            CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,path TEXT,is_excluded_from_search INTEGER DEFAULT 0);
            CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,file_type INTEGER,width INTEGER,height INTEGER,live_photo_video_id INTEGER,modified_at INTEGER,size INTEGER);
            CREATE TABLE media_states(file_id INTEGER PRIMARY KEY,hidden INTEGER DEFAULT 0,deleted_at INTEGER);
            CREATE TABLE ai_index_profiles(task TEXT PRIMARY KEY,fingerprint TEXT NOT NULL);
            INSERT INTO albums VALUES(1,'/photos',7,0,'[]');INSERT INTO afolders VALUES(1,1,'/photos',0);
            INSERT INTO afiles VALUES(1,1,1,100,100,NULL,10,20),(2,1,1,100,100,NULL,10,20);
            INSERT INTO ai_index_profiles VALUES('ocr','model-a');").unwrap();
        ensure_schema(&c).unwrap();
        c
    }
    #[test]
    fn workspace_states_are_disjoint_and_scope_current_model_source_and_visibility() {
        let c = db();
        c.execute_batch("INSERT INTO afiles VALUES(3,1,1,100,100,NULL,10,20),(4,1,1,100,100,NULL,10,20),(5,1,1,100,100,NULL,10,20);").unwrap();
        store_result(
            &c,
            1,
            "model-a",
            Some(10),
            20,
            "Invoice 中文 100%",
            Some("{}"),
            None,
        )
        .unwrap();
        store_result(&c, 2, "model-a", Some(10), 20, "", Some("{}"), None).unwrap();
        store_result(
            &c,
            3,
            "model-a",
            Some(10),
            20,
            "",
            None,
            Some("decode failed"),
        )
        .unwrap();
        store_result(&c, 4, "model-a", Some(10), 20, "stale", Some("{}"), None).unwrap();
        c.execute("UPDATE afiles SET size=21 WHERE id=4", [])
            .unwrap();
        let count = |state: &str| {
            c.query_row(
                &format!(
                    "SELECT COUNT(*) {} AND {}",
                    base(),
                    workspace_condition(state)
                ),
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
        };
        assert_eq!(
            (
                count("all"),
                count("text"),
                count("empty"),
                count("failed"),
                count("pending")
            ),
            (5, 1, 1, 1, 2)
        );
        assert_eq!(count("invalid"), 0);
        assert_eq!(coverage_on(&c, "model-a").unwrap(), (5, 2, 1, 1, 1));
        assert_eq!(coverage_on(&c, "model-b").unwrap(), (5, 0, 0, 0, 0));
        c.execute_batch("INSERT INTO media_states VALUES(1,1,NULL),(2,0,100);")
            .unwrap();
        assert_eq!(count("text"), 0);
        assert_eq!(count("empty"), 0);
        c.execute("UPDATE ai_index_profiles SET fingerprint='model-b'", [])
            .unwrap();
        assert_eq!(count("pending"), 3);
        assert_eq!(count("failed"), 0);
    }
    #[test]
    fn summaries_are_bounded_literal_unicode_and_do_not_leak_hidden_or_stale_text() {
        let c = db();
        let text = format!("<script>\n{}中文 100% _ [a]", "长".repeat(250));
        store_result(&c, 1, "model-a", Some(10), 20, &text, Some("{}"), None).unwrap();
        let values = summaries_on(&c, &[1, 2], "中文", "model-a").unwrap();
        let hit = values.iter().find(|row| row.file_id == 1).unwrap();
        assert_eq!(hit.state, "text");
        assert!(hit.excerpt.contains("中文"));
        assert!(hit.excerpt.chars().count() <= 162);
        assert_eq!(
            values.iter().find(|row| row.file_id == 2).unwrap().state,
            "pending"
        );
        c.execute("INSERT INTO media_states VALUES(1,1,NULL)", [])
            .unwrap();
        assert!(summaries_on(&c, &[1], "", "model-a").unwrap().is_empty());
        c.execute("DELETE FROM media_states", []).unwrap();
        c.execute("UPDATE afiles SET size=21 WHERE id=1", [])
            .unwrap();
        let stale = summaries_on(&c, &[1], "", "model-a").unwrap();
        assert_eq!(stale[0].state, "pending");
        assert!(stale[0].excerpt.is_empty());
    }
    #[test]
    fn retry_failed_is_not_resume_or_rebuild_and_selected_scope_does_not_expand() {
        let c = db();
        let cancel = AtomicBool::new(false);
        c.execute_batch("INSERT INTO afiles VALUES(3,1,1,100,100,NULL,10,20);")
            .unwrap();
        store_result(&c, 1, "model-a", Some(10), 20, "", Some("{}"), None).unwrap();
        store_result(&c, 2, "model-a", Some(10), 20, "", None, Some("failed")).unwrap();
        let ids = |force, retry, selected| {
            target_ids_on(&c, "model-a", force, retry, selected, None, &cancel).unwrap()
        };
        assert_eq!(ids(false, false, None), vec![2, 3]);
        assert_eq!(ids(false, true, None), vec![2]);
        assert_eq!(ids(true, false, None), vec![1, 2, 3]);
        assert!(ids(false, true, Some(vec![1, 3])).is_empty());
        assert_eq!(ids(false, false, Some(vec![3])), vec![3]);
        cancel.store(true, Ordering::SeqCst);
        assert!(target_ids_on(&c, "model-a", false, false, None, None, &cancel).is_err());
    }
    #[test]
    fn folder_scope_respects_album_boundaries_subfolders_and_literal_paths() {
        let c = db();
        let cancel = AtomicBool::new(false);
        c.execute_batch("INSERT INTO afolders VALUES(2,1,'/photos/trip_%',0),(3,1,'/photos/trip_%/child',0),(4,1,'/photos/trip_%2',0),(5,1,'/photos/trip_AX',0),(6,2,'/photos/trip_%/child',0);
            INSERT INTO albums VALUES(2,'/photos',7,0,'[]');
            INSERT INTO afiles VALUES(3,2,1,100,100,NULL,10,20),(4,3,1,100,100,NULL,10,20),(5,4,1,100,100,NULL,10,20),(6,5,1,100,100,NULL,10,20),(7,6,1,100,100,NULL,10,20);").unwrap();
        let folder = folder_scope(&c, 2).unwrap();
        assert_eq!(
            target_ids_on(&c, "model-a", false, false, None, Some(folder), &cancel).unwrap(),
            vec![3, 4]
        );
        assert!(folder_scope(&c, 999).is_err());
    }
    #[test]
    fn source_fingerprint_visibility_and_literal_search_are_enforced() {
        let c = db();
        assert!(
            store_result(
                &c,
                1,
                "model-a",
                Some(10),
                20,
                "中文 100% text",
                Some("{}"),
                None
            )
            .unwrap()
        );
        let query = format!("SELECT COUNT(*) {} AND {}", base(), search_predicate());
        assert_eq!(
            c.query_row(&query, ["%中文%"], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row(&query, ["%100\\%%"], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        c.execute("UPDATE ai_index_profiles SET fingerprint='model-b'", [])
            .unwrap();
        assert_eq!(
            c.query_row(&query, ["%中文%"], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        c.execute("UPDATE ai_index_profiles SET fingerprint='model-a'", [])
            .unwrap();
        c.execute("INSERT INTO media_states VALUES(1,1,NULL)", [])
            .unwrap();
        assert_eq!(
            c.query_row(&query, ["%中文%"], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(
            !store_result(&c, 1, "model-a", Some(10), 20, "private", Some("{}"), None).unwrap()
        );
        c.execute("DELETE FROM media_states", []).unwrap();
        c.execute("UPDATE afiles SET modified_at=11 WHERE id=1", [])
            .unwrap();
        assert_eq!(
            c.query_row(&query, ["%中文%"], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(!store_result(&c, 1, "model-a", Some(10), 20, "stale", Some("{}"), None).unwrap());
        c.execute("DELETE FROM afiles WHERE id=1", []).unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM ocr_index", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn failures_are_retryable_empty_success_is_indexed_and_profiles_can_be_reused() {
        let c = db();
        store_result(&c, 1, "model-a", Some(10), 20, "", None, Some("offline")).unwrap();
        store_result(
            &c,
            2,
            "model-a",
            Some(10),
            20,
            "",
            Some(r#"{"text":"","lines":[]}"#),
            None,
        )
        .unwrap();
        let pending = format!(
            "SELECT a.id {} AND NOT EXISTS(SELECT 1 FROM ocr_index oi WHERE oi.file_id=a.id AND oi.profile=?1 AND oi.modified_at IS a.modified_at AND oi.size=a.size AND oi.error IS NULL)",
            base()
        );
        let ids = c
            .prepare(&pending)
            .unwrap()
            .query_map(["model-a"], |r| r.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(ids, vec![1]);
        super::super::profiles::ensure_on(&c, Task::Ocr, "model-b").unwrap();
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM ocr_index WHERE profile='model-a'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        super::super::profiles::ensure_on(&c, Task::Ocr, "model-a").unwrap();
        assert!(
            store_result(
                &c,
                1,
                "model-a",
                Some(10),
                20,
                "now available",
                Some("{}"),
                None
            )
            .unwrap()
        );
        assert_eq!(
            c.prepare(&pending)
                .unwrap()
                .query_map(["model-a"], |r| r.get::<_, i64>(0))
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn excluded_folders_deleted_and_companions_cannot_be_indexed() {
        let c = db();
        c.execute("UPDATE afolders SET is_excluded_from_search=1", [])
            .unwrap();
        assert!(!store_result(&c, 1, "a", Some(10), 20, "text", Some("{}"), None).unwrap());
        c.execute("UPDATE afolders SET is_excluded_from_search=0", [])
            .unwrap();
        c.execute("INSERT INTO media_states VALUES(1,0,100)", [])
            .unwrap();
        assert!(!store_result(&c, 1, "a", Some(10), 20, "text", Some("{}"), None).unwrap());
        // Companion exclusion applies while its primary remains in the retained library.
        c.execute("DELETE FROM media_states", []).unwrap();
        c.execute("UPDATE afiles SET live_photo_video_id=2 WHERE id=1", [])
            .unwrap();
        assert!(!store_result(&c, 2, "a", Some(10), 20, "text", Some("{}"), None).unwrap());
    }
}
