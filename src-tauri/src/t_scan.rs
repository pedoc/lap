//! Durable reports for full album scans. Directory checks/watchers are not full scans.
use once_cell::sync::Lazy;
use rusqlite::{Connection, Row, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

mod retry;

pub const HISTORY_LIMIT: usize = 20;
pub const ERROR_LIMIT: usize = 100;
pub fn ensure_schema(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS scan_runs(
 seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,album_id INTEGER NOT NULL,album_name TEXT NOT NULL,root_path TEXT NOT NULL,
 started_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,finished_at INTEGER,status TEXT NOT NULL,phase TEXT NOT NULL,
 revision INTEGER NOT NULL,counts_json TEXT NOT NULL,current_path TEXT NOT NULL,error_count INTEGER NOT NULL,last_error TEXT);
 CREATE INDEX IF NOT EXISTS scan_runs_album ON scan_runs(album_id,seq DESC);
 CREATE TABLE IF NOT EXISTS scan_folders(run_id TEXT NOT NULL,path_key TEXT NOT NULL,snapshot_json TEXT NOT NULL,PRIMARY KEY(run_id,path_key));
 CREATE INDEX IF NOT EXISTS scan_folders_path ON scan_folders(path_key,run_id);
 CREATE TABLE IF NOT EXISTS scan_error_retries(run_id TEXT NOT NULL,ordinal INTEGER NOT NULL,attempts INTEGER NOT NULL DEFAULT 0,state TEXT NOT NULL,last_attempt_at INTEGER,resolved_at INTEGER,last_error TEXT,PRIMARY KEY(run_id,ordinal));
 CREATE TABLE IF NOT EXISTS scan_errors(run_id TEXT NOT NULL,ordinal INTEGER NOT NULL,path_key TEXT NOT NULL,path TEXT NOT NULL,stage TEXT NOT NULL,message TEXT NOT NULL,PRIMARY KEY(run_id,ordinal));
 ").map_err(|e|e.to_string())?;
    let albums: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='albums')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if albums {
        c.execute_batch(
            "DROP TRIGGER IF EXISTS scan_reports_album_delete; CREATE TRIGGER scan_reports_album_delete AFTER DELETE ON albums BEGIN
        DELETE FROM scan_error_retries WHERE run_id IN(SELECT id FROM scan_runs WHERE album_id=OLD.id);
        DELETE FROM scan_errors WHERE run_id IN(SELECT id FROM scan_runs WHERE album_id=OLD.id);
        DELETE FROM scan_folders WHERE run_id IN(SELECT id FROM scan_runs WHERE album_id=OLD.id);
        DELETE FROM scan_runs WHERE album_id=OLD.id;
    END;",
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Counts {
    pub total: u64,
    pub discovered: u64,
    pub indexed: u64,
    pub previews_ready: u64,
    pub search_total: u64,
    pub search_ready: u64,
    pub images: u64,
    pub skipped: u64,
    pub metadata_failed: u64,
    pub preview_failed: u64,
    pub search_failed: u64,
    pub deferred: u64,
    pub pending: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Run {
    pub seq: i64,
    pub id: String,
    pub library_id: String,
    pub album_id: i64,
    pub album_name: String,
    pub root_path: String,
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
    pub status: String,
    pub phase: String,
    pub revision: i64,
    pub counts: Counts,
    pub current_path: String,
    #[serde(default)]
    pub retry: RetrySummary,
    pub error_count: u64,
    pub last_error: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RetrySummary {
    pub resolved: u64,
    pub running: u64,
}
#[derive(Serialize)]
pub struct ErrorDetail {
    pub ordinal: i64,
    pub path: String,
    pub stage: String,
    pub message: String,
    pub attempts: u64,
    pub retry_state: String,
    pub last_attempt_at: Option<i64>,
    pub resolved_at: Option<i64>,
    pub retry_error: Option<String>,
    pub retryable: bool,
}
const RUN_COLUMNS: &str = "r.*, (SELECT COUNT(*) FROM scan_error_retries q WHERE q.run_id=r.id AND q.state='resolved') AS resolved_errors, (SELECT COUNT(*) FROM scan_error_retries q WHERE q.run_id=r.id AND q.state IN ('queued','running')) AS retrying_errors";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Folder {
    pub run_id: String,
    pub path: String,
    pub path_key: String,
    pub counts: Counts,
    pub entered: bool,
    pub enumerated: bool,
    #[serde(default)]
    pub retry: RetrySummary,
}
#[derive(Clone, Debug, Serialize)]
pub struct ScanError {
    pub path: String,
    pub stage: String,
    pub message: String,
}
#[derive(Clone, Serialize)]
pub struct Event {
    pub run: Run,
    pub folders: Vec<Folder>,
}
#[derive(Serialize)]
pub struct FolderBatch {
    pub runs: Vec<Run>,
    pub folders: Vec<Folder>,
}
#[derive(Serialize)]
pub struct Details {
    pub run: Run,
    pub folders: Vec<Folder>,
    pub folder_count: i64,
    pub errors: Vec<ErrorDetail>,
}

pub fn path_key_for(path: &str, windows: bool) -> String {
    let mut p = if windows {
        path.replace('\\', "/")
    } else {
        path.to_owned()
    };
    if windows {
        if p.to_ascii_lowercase().starts_with("//?/unc/") {
            p = format!("//{}", &p[8..]);
        } else if p.starts_with("//?/") {
            p = p[4..].to_owned();
        }
        p = p.to_lowercase();
    }
    while p.ends_with('/') && p.len() > 1 && !(windows && p.len() == 3 && p.as_bytes()[1] == b':') {
        p.pop();
    }
    p
}
pub fn path_key(path: &str) -> String {
    path_key_for(path, cfg!(windows))
}
fn json<T: serde::de::DeserializeOwned>(row: &Row, name: &str) -> rusqlite::Result<T> {
    let s: String = row.get(name)?;
    serde_json::from_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            row.as_ref().column_index(name).unwrap_or(0),
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })
}
fn read_run(row: &Row, library: &str) -> rusqlite::Result<Run> {
    Ok(Run {
        seq: row.get("seq")?,
        id: row.get("id")?,
        library_id: library.into(),
        album_id: row.get("album_id")?,
        album_name: row.get("album_name")?,
        root_path: row.get("root_path")?,
        started_at: row.get("started_at")?,
        updated_at: row.get("updated_at")?,
        finished_at: row.get("finished_at")?,
        status: row.get("status")?,
        phase: row.get("phase")?,
        revision: row.get("revision")?,
        counts: json(row, "counts_json")?,
        current_path: row.get("current_path")?,
        retry: RetrySummary {
            resolved: row.get("resolved_errors")?,
            running: row.get("retrying_errors")?,
        },
        error_count: row.get("error_count")?,
        last_error: row.get("last_error")?,
    })
}
pub fn recover_interrupted(c: &Connection) -> Result<(), String> {
    retry::recover_on(c)?;
    c.execute("UPDATE scan_runs SET status='interrupted',finished_at=updated_at,updated_at=MAX(updated_at,?1),revision=revision+1 WHERE status IN ('running','pausing')",[chrono::Utc::now().timestamp_millis()]).map(|_|()).map_err(|e|e.to_string())
}
fn persist(
    c: &Connection,
    run: &Run,
    folders: &[Folder],
    errors: &[ScanError],
) -> Result<(), String> {
    let tx = c.unchecked_transaction().map_err(|e| e.to_string())?;
    if run.revision == 0
        && tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM scan_runs WHERE id=?1)",
                [&run.id],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| e.to_string())?
    {
        return Err("Scan request id already exists".into());
    }
    tx.execute("INSERT INTO scan_runs(id,album_id,album_name,root_path,started_at,updated_at,finished_at,status,phase,revision,counts_json,current_path,error_count,last_error) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
 ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at,finished_at=excluded.finished_at,status=excluded.status,phase=excluded.phase,revision=excluded.revision,counts_json=excluded.counts_json,current_path=excluded.current_path,error_count=excluded.error_count,last_error=excluded.last_error",
 params![run.id,run.album_id,run.album_name,run.root_path,run.started_at,run.updated_at,run.finished_at,run.status,run.phase,run.revision,serde_json::to_string(&run.counts).map_err(|e|e.to_string())?,run.current_path,run.error_count,run.last_error]).map_err(|e|e.to_string())?;
    for folder in folders {
        tx.execute("INSERT INTO scan_folders(run_id,path_key,snapshot_json) VALUES(?1,?2,?3) ON CONFLICT(run_id,path_key) DO UPDATE SET snapshot_json=excluded.snapshot_json",params![run.id,folder.path_key,serde_json::to_string(folder).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    }
    for (index, error) in errors.iter().take(ERROR_LIMIT).enumerate() {
        tx.execute("INSERT OR IGNORE INTO scan_errors(run_id,ordinal,path_key,path,stage,message) VALUES(?1,?2,?3,?4,?5,?6)",params![run.id,index as i64,path_key(&error.path),error.path,error.stage,error.message]).map_err(|e|e.to_string())?;
    }
    if run.finished_at.is_some() {
        let old: Vec<String> = {
            let mut s=tx.prepare("SELECT id FROM scan_runs WHERE album_id=?1 AND finished_at IS NOT NULL ORDER BY seq DESC LIMIT -1 OFFSET ?2").map_err(|e|e.to_string())?;
            s.query_map(params![run.album_id, HISTORY_LIMIT as i64], |r| r.get(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?
        };
        for id in old {
            tx.execute("DELETE FROM scan_error_retries WHERE run_id=?1", [&id])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM scan_errors WHERE run_id=?1", [&id])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM scan_folders WHERE run_id=?1", [&id])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM scan_runs WHERE id=?1", [&id])
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}
struct Journal {
    conn: Connection,
    run: Run,
    folders: BTreeMap<String, Folder>,
    dirty: BTreeSet<String>,
    errors: Vec<ScanError>,
    last_flush: Instant,
    search_enabled: bool,
}
pub struct Session {
    inner: Mutex<Journal>,
    app: AppHandle,
    _lease: Arc<crate::t_utils::AlbumScanGuard>,
    _db_lease: crate::t_storage::DbConnectionLease,
}
static ACTIVE: Lazy<Mutex<HashMap<i64, Weak<Session>>>> = Lazy::new(|| Mutex::new(HashMap::new()));
impl Session {
    pub fn begin(
        app: &AppHandle,
        library: &str,
        album: &crate::t_sqlite::Album,
        id: String,
        lease: Arc<crate::t_utils::AlbumScanGuard>,
    ) -> Result<Arc<Self>, String> {
        if uuid::Uuid::parse_str(&id).is_err() {
            return Err("Invalid scan job id".into());
        }
        let db_lease = crate::t_storage::DbConnectionLease::acquire()?;
        let c = Connection::open(crate::t_storage::get_current_db_path()?)
            .map_err(|e| e.to_string())?;
        c.busy_timeout(Duration::from_millis(500))
            .map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().timestamp_millis();
        let mut run = Run {
            seq: 0,
            id,
            library_id: library.into(),
            album_id: album.id.unwrap_or(0),
            album_name: album.name.clone(),
            root_path: album.path.clone(),
            started_at: now,
            updated_at: now,
            finished_at: None,
            status: "running".into(),
            phase: "counting".into(),
            revision: 0,
            counts: Counts::default(),
            current_path: album.path.clone(),
            retry: RetrySummary::default(),
            error_count: 0,
            last_error: None,
        };
        if c.query_row(
            "SELECT EXISTS(SELECT 1 FROM scan_runs WHERE id=?1)",
            [&run.id],
            |r| r.get::<_, bool>(0),
        )
        .map_err(|e| e.to_string())?
        {
            return Err("Scan request id already exists".into());
        }
        persist(&c, &run, &[], &[])?;
        run.seq = c.last_insert_rowid();
        let s = Arc::new(Self {
            inner: Mutex::new(Journal {
                conn: c,
                run,
                folders: BTreeMap::new(),
                dirty: BTreeSet::new(),
                errors: Vec::new(),
                last_flush: Instant::now() - Duration::from_secs(2),
                search_enabled: crate::ai::settings::background_indexing_enabled(),
            }),
            app: app.clone(),
            _lease: lease,
            _db_lease: db_lease,
        });
        ACTIVE
            .lock()
            .map_err(|e| e.to_string())?
            .insert(album.id.unwrap_or(0), Arc::downgrade(&s));
        s.phase("counting");
        Ok(s)
    }
    pub fn snapshot(&self) -> Run {
        self.inner.lock().unwrap().run.clone()
    }
    fn mutate(&self, force: bool, f: impl FnOnce(&mut Journal)) {
        let event = {
            let mut j = self.inner.lock().unwrap();
            if j.run.finished_at.is_some() {
                return;
            }
            f(&mut j);
            if !force && j.last_flush.elapsed() < Duration::from_secs(1) {
                return;
            }
            j.run.updated_at = chrono::Utc::now().timestamp_millis().max(j.run.updated_at);
            j.run.revision += 1;
            let root = path_key(&j.run.root_path);
            if let Some(folder) = j.folders.get(&root) {
                j.run.counts = folder.counts.clone();
            }
            let folders: Vec<Folder> = j
                .dirty
                .iter()
                .filter_map(|key| j.folders.get(key).cloned())
                .collect();
            if let Err(e) = persist(&j.conn, &j.run, &folders, &j.errors) {
                eprintln!("Scan checkpoint failed: {e}");
            } else {
                j.dirty.clear();
            }
            j.last_flush = Instant::now();
            Event {
                run: j.run.clone(),
                folders,
            }
        };
        let _ = self.app.emit("scan-status-changed", event);
    }
    fn folders(j: &mut Journal, path: &str, is_dir: bool, mut f: impl FnMut(&mut Folder, bool)) {
        let root = Path::new(&j.run.root_path);
        let target = Path::new(path);
        let base = if is_dir {
            target
        } else {
            target.parent().unwrap_or(root)
        };
        for ancestor in base.ancestors() {
            if !ancestor.starts_with(root) {
                break;
            }
            let display = ancestor.to_string_lossy().to_string();
            let key = path_key(&display);
            let id = j.run.id.clone();
            let search = j.search_enabled;
            let folder = j.folders.entry(key.clone()).or_insert_with(|| Folder {
                run_id: id,
                path: display,
                path_key: key.clone(),
                counts: Counts::default(),
                retry: RetrySummary::default(),
                entered: false,
                enumerated: false,
            });
            f(folder, search);
            j.dirty.insert(key);
            if ancestor == root {
                break;
            }
        }
    }
    pub fn expected(&self, path: &str, kind: Option<i64>) {
        self.mutate(false, |j| {
            j.run.current_path = path.into();
            Self::folders(j, path, kind.is_none(), |folder, search| {
                if let Some(kind) = kind {
                    folder.counts.total += 1;
                    if search && matches!(kind, 1 | 3) {
                        folder.counts.search_total += 1;
                    }
                }
            });
        });
    }
    pub fn enter(&self, path: &str) {
        self.mutate(false, |j| {
            j.run.current_path = path.into();
            let key = path_key(path);
            Self::folders(j, path, true, |f, _| {
                if f.path_key == key {
                    f.entered = true;
                }
            });
        });
    }
    pub fn enumerated(&self, path: &str) {
        self.mutate(false, |j| {
            if let Some(f) = j.folders.get_mut(&path_key(path)) {
                f.enumerated = true;
                f.counts.total = f.counts.discovered;
                if j.search_enabled {
                    f.counts.search_total = f.counts.images;
                }
                j.dirty.insert(f.path_key.clone());
            }
        });
    }
    pub fn metadata(
        &self,
        path: &str,
        kind: i64,
        ok: bool,
        preview: bool,
        search: bool,
        task: bool,
    ) {
        self.mutate(false, |j| {
            j.run.current_path = path.into();
            Self::folders(j, path, false, |f, enabled| {
                let c = &mut f.counts;
                c.discovered += 1;
                c.total = c.total.max(c.discovered);
                if matches!(kind, 1 | 3) {
                    c.images += 1;
                    if enabled {
                        c.search_total = c.search_total.max(c.images);
                    }
                }
                if ok {
                    c.indexed += 1;
                    if preview {
                        c.previews_ready += 1;
                    }
                    if enabled && search {
                        c.search_ready += 1;
                    }
                    if task {
                        c.pending += 1;
                    } else if !preview {
                        c.deferred += 1;
                    }
                }
            });
        });
    }
    pub fn excluded(&self, path: &str, kind: i64) {
        self.mutate(false, |j| {
            Self::folders(j, path, false, |f, search| {
                f.counts.total = f.counts.total.saturating_sub(1);
                if search && matches!(kind, 1 | 3) {
                    f.counts.search_total = f.counts.search_total.saturating_sub(1);
                }
            })
        });
    }
    pub fn skipped(&self, path: &str) {
        self.mutate(false, |j| {
            Self::folders(j, path, false, |f, _| f.counts.skipped += 1)
        });
    }
    pub fn stage_ready(&self, path: &str, stage: &str) {
        self.mutate(false, |j| {
            Self::folders(j, path, false, |f, _| {
                if stage == "preview" {
                    f.counts.previews_ready += 1;
                } else if stage == "search" {
                    f.counts.search_ready += 1;
                }
            })
        });
    }
    pub fn task_done(&self, path: &str) {
        self.mutate(false, |j| {
            Self::folders(j, path, false, |f, _| {
                f.counts.pending = f.counts.pending.saturating_sub(1)
            })
        });
    }
    pub fn error(&self, path: &str, stage: &str, message: &str) {
        self.mutate(false, |j| {
            j.run.error_count += 1;
            j.run.last_error = Some(format!("{path}: {message}"));
            if j.errors.len() < ERROR_LIMIT {
                j.errors.push(ScanError {
                    path: path.into(),
                    stage: stage.into(),
                    message: message.into(),
                });
            }
            Self::folders(j, path, stage == "directory", |f, _| match stage {
                "metadata" => f.counts.metadata_failed += 1,
                "preview" => f.counts.preview_failed += 1,
                "search" => f.counts.search_failed += 1,
                _ => {}
            });
        });
    }
    pub fn phase(&self, phase: &str) {
        let changed = self.inner.lock().unwrap().run.phase != phase;
        self.mutate(changed, |j| j.run.phase = phase.into());
    }
    pub fn pause_requested(&self) {
        self.mutate(true, |j| j.run.status = "pausing".into());
    }
    pub fn finish(&self, status: &str) -> Result<Run, String> {
        let event = {
            let mut j = self.inner.lock().map_err(|e| e.to_string())?;
            let root = path_key(&j.run.root_path);
            if let Some(f) = j.folders.get(&root) {
                j.run.counts = f.counts.clone();
            }
            j.run.status =
                if status == "completed" && (j.run.error_count > 0 || j.run.counts.deferred > 0) {
                    "warnings"
                } else {
                    status
                }
                .into();
            j.run.phase = if status == "completed" {
                "complete"
            } else {
                j.run.phase.as_str()
            }
            .into();
            j.run.updated_at = chrono::Utc::now().timestamp_millis().max(j.run.updated_at);
            j.run.finished_at = Some(j.run.updated_at);
            j.run.revision += 1;
            let folders: Vec<Folder> = j
                .dirty
                .iter()
                .filter_map(|k| j.folders.get(k).cloned())
                .collect();
            persist(&j.conn, &j.run, &folders, &j.errors)?;
            j.dirty.clear();
            Event {
                run: j.run.clone(),
                folders,
            }
        };
        let _ = self.app.emit("scan-status-changed", event.clone());
        Ok(event.run)
    }
}
pub fn active_session(album: i64) -> Option<Arc<Session>> {
    ACTIVE.lock().ok()?.get(&album)?.upgrade()
}
pub struct TaskReportGuard {
    pub session: Arc<Session>,
    pub path: String,
}
impl Drop for TaskReportGuard {
    fn drop(&mut self) {
        self.session.task_done(&self.path);
    }
}

pub fn overview_on(c: &Connection, library: &str) -> Result<Vec<Run>, String> {
    let mut s=c.prepare(&format!("SELECT {RUN_COLUMNS} FROM scan_runs r WHERE r.seq=(SELECT MAX(x.seq) FROM scan_runs x WHERE x.album_id=r.album_id) ORDER BY r.seq DESC LIMIT 1000")).map_err(|e|e.to_string())?;
    s.query_map([], |r| read_run(r, library))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())
}
pub fn history_on(
    c: &Connection,
    library: &str,
    album: i64,
    folder: Option<&str>,
) -> Result<Vec<Run>, String> {
    let key = folder.map(path_key);
    let mut s=c.prepare(&format!("SELECT {RUN_COLUMNS} FROM scan_runs r WHERE r.album_id=?1 AND (?2 IS NULL OR EXISTS(SELECT 1 FROM scan_folders f WHERE f.run_id=r.id AND f.path_key=?2)) ORDER BY r.seq DESC LIMIT 20")).map_err(|e|e.to_string())?;
    s.query_map(params![album, key], |r| read_run(r, library))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())
}
pub fn details_on(
    c: &Connection,
    library: &str,
    id: &str,
    folder: Option<&str>,
    offset: u32,
) -> Result<Details, String> {
    let run = c
        .query_row(
            &format!("SELECT {RUN_COLUMNS} FROM scan_runs r WHERE r.id=?1"),
            [id],
            |r| read_run(r, library),
        )
        .map_err(|e| e.to_string())?;
    let key = folder.map(path_key);
    let prefix = key
        .as_ref()
        .map(|k| format!("{}/", k.trim_end_matches('/')));
    let predicate = "run_id=?1 AND (?2 IS NULL OR path_key=?2 OR substr(path_key,1,length(?3))=?3)";
    let count = c
        .query_row(
            &format!("SELECT COUNT(*) FROM scan_folders WHERE {predicate}"),
            params![id, key, prefix],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut s=c.prepare(&format!("SELECT snapshot_json FROM scan_folders WHERE {predicate} ORDER BY length(path_key),path_key LIMIT 100 OFFSET ?4")).map_err(|e|e.to_string())?;
    let mut folders: Vec<Folder> = s
        .query_map(params![id, key, prefix, offset], |r| {
            json(r, "snapshot_json")
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    for folder in &mut folders {
        retry::decorate_folder_on(c, folder)?;
    }
    let errors = retry::errors_on(c, id, folder)?;
    Ok(Details {
        run,
        folders,
        folder_count: count,
        errors,
    })
}
pub fn folders_on(
    c: &Connection,
    library: &str,
    album: i64,
    paths: &[String],
) -> Result<FolderBatch, String> {
    if paths.len() > 200 {
        return Err("At most 200 directory paths per request".into());
    }
    let mut runs = BTreeMap::new();
    let mut folders = Vec::new();
    let mut stmt=c.prepare(&format!("SELECT {RUN_COLUMNS},f.snapshot_json FROM scan_folders f JOIN scan_runs r ON r.id=f.run_id WHERE r.album_id=?1 AND f.path_key=?2 ORDER BY r.seq DESC LIMIT 1")).map_err(|e|e.to_string())?;
    for path in paths {
        let mut rows = stmt
            .query(params![album, path_key(path)])
            .map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let run = read_run(row, library).map_err(|e| e.to_string())?;
            let mut folder: Folder = json(row, "snapshot_json").map_err(|e| e.to_string())?;
            retry::decorate_folder_on(c, &mut folder)?;
            folders.push(folder);
            runs.insert(run.id.clone(), run);
        }
    }
    Ok(FolderBatch {
        runs: runs.into_values().collect(),
        folders,
    })
}
fn reported_run(mut run: Run) -> Run {
    if matches!(run.status.as_str(), "running" | "pausing")
        && !crate::t_utils::album_scan_active(run.album_id)
    {
        run.status = "interrupted".into();
        run.finished_at = Some(run.updated_at);
        run.revision += 1;
    }
    run
}
#[tauri::command]
pub async fn get_scan_overview(library_id: String) -> Result<Vec<Run>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::t_cmds::with_current_library(&library_id, || {
            overview_on(&*crate::t_sqlite::open_conn()?, &library_id)
                .map(|rows| rows.into_iter().map(reported_run).collect())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn get_scan_history(
    library_id: String,
    album_id: i64,
    folder_path: Option<String>,
) -> Result<Vec<Run>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::t_cmds::with_current_library(&library_id, || {
            history_on(
                &*crate::t_sqlite::open_conn()?,
                &library_id,
                album_id,
                folder_path.as_deref(),
            )
            .map(|rows| rows.into_iter().map(reported_run).collect())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn get_scan_details(
    library_id: String,
    run_id: String,
    folder_path: Option<String>,
    offset: u32,
) -> Result<Details, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::t_cmds::with_current_library(&library_id, || {
            details_on(
                &*crate::t_sqlite::open_conn()?,
                &library_id,
                &run_id,
                folder_path.as_deref(),
                offset,
            )
            .map(|mut details| {
                details.run = reported_run(details.run);
                details
            })
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn retry_scan_errors(
    app: AppHandle,
    library_id: String,
    run_id: String,
    ordinals: Option<Vec<i64>>,
    thumbnail_size: u32,
    raw_display_options: crate::t_raw_display::RawDisplayOptions,
    group_raw_jpeg_pairs: bool,
) -> Result<Run, String> {
    retry::execute(
        app,
        library_id,
        run_id,
        ordinals,
        thumbnail_size,
        raw_display_options,
        group_raw_jpeg_pairs,
    )
    .await
}
#[tauri::command]
pub async fn get_scan_folder_states(
    library_id: String,
    album_id: i64,
    paths: Vec<String>,
) -> Result<FolderBatch, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::t_cmds::with_current_library(&library_id, || {
            folders_on(
                &*crate::t_sqlite::open_conn()?,
                &library_id,
                album_id,
                &paths,
            )
            .map(|mut batch| {
                batch.runs = batch.runs.into_iter().map(reported_run).collect();
                batch
            })
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(id: usize) -> Run {
        Run {
            seq: id as i64 + 1,
            id: format!("run-{id}"),
            library_id: "test".into(),
            album_id: 1,
            album_name: "Pictures".into(),
            root_path: "/photos".into(),
            started_at: 1000,
            updated_at: 1000,
            finished_at: Some(1000),
            status: "completed".into(),
            phase: "complete".into(),
            revision: 1,
            counts: Counts::default(),
            current_path: "/photos".into(),
            retry: RetrySummary::default(),
            error_count: 0,
            last_error: None,
        }
    }
    #[test]
    fn path_keys_preserve_component_and_platform_boundaries() {
        assert_eq!(path_key_for(r"C:\Photos\A\", true), "c:/photos/a");
        assert_eq!(
            path_key_for(r"\\?\UNC\server\share\x", true),
            "//server/share/x"
        );
        assert_eq!(path_key_for(r"C:\", true), "c:/");
        assert_ne!(path_key_for("/A", false), path_key_for("/a", false));
        assert_eq!(path_key_for(r"/a\b", false), r"/a\b");
    }
    #[test]
    fn bounded_history_and_interruption_do_not_fabricate_completion() {
        let c = Connection::open_in_memory().unwrap();
        ensure_schema(&c).unwrap();
        for i in 0..25 {
            persist(&c, &run(i), &[], &[]).unwrap();
        }
        assert_eq!(history_on(&c, "test", 1, None).unwrap().len(), 20);
        let mut r = run(26);
        r.status = "running".into();
        r.finished_at = None;
        persist(&c, &r, &[], &[]).unwrap();
        recover_interrupted(&c).unwrap();
        let r = &overview_on(&c, "test").unwrap()[0];
        assert_eq!(r.status, "interrupted");
        assert!(r.finished_at.is_some());
    }
    #[test]
    fn folder_history_uses_exact_paths_and_scoped_details_do_not_include_siblings() {
        let c = Connection::open_in_memory().unwrap();
        ensure_schema(&c).unwrap();
        let r = run(1);
        let folders = vec!["/photos/A", "/photos/AB", "/photos/A/sub"]
            .into_iter()
            .map(|path| Folder {
                run_id: r.id.clone(),
                path: path.into(),
                path_key: path_key(path),
                counts: Counts::default(),
                retry: RetrySummary::default(),
                entered: true,
                enumerated: true,
            })
            .collect::<Vec<_>>();
        persist(
            &c,
            &r,
            &folders,
            &[ScanError {
                path: "/photos/AB/file.jpg".into(),
                stage: "metadata".into(),
                message: "bad".into(),
            }],
        )
        .unwrap();
        let d = details_on(&c, "test", &r.id, Some("/photos/A"), 0).unwrap();
        assert_eq!(d.folders.len(), 2);
        assert!(d.errors.is_empty());
        assert!(
            history_on(&c, "test", 1, Some("/photos/Z"))
                .unwrap()
                .is_empty()
        );
        let b = folders_on(&c, "test", 1, &["/photos/A".into()]).unwrap();
        assert_eq!(b.folders.len(), 1);
    }
}

#[cfg(test)]
mod report_contract_tests {
    use super::*;
    #[test]
    fn a_reused_start_id_cannot_overwrite_another_album() {
        let c = Connection::open_in_memory().unwrap();
        ensure_schema(&c).unwrap();
        let mut r = Run {
            seq: 0,
            id: "same-request".into(),
            library_id: "test".into(),
            album_id: 1,
            album_name: "Photos".into(),
            root_path: "/photos".into(),
            started_at: 1,
            updated_at: 1,
            finished_at: None,
            status: "running".into(),
            phase: "counting".into(),
            revision: 0,
            counts: Counts::default(),
            current_path: "/photos".into(),
            retry: RetrySummary::default(),
            error_count: 0,
            last_error: None,
        };
        persist(&c, &r, &[], &[]).unwrap();
        r.album_id = 2;
        assert!(persist(&c, &r, &[], &[]).is_err());
        assert_eq!(
            c.query_row("SELECT album_id FROM scan_runs", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        r.album_id = 1;
        r.revision = 1;
        r.error_count = 150;
        r.status = "warnings".into();
        r.finished_at = Some(2);
        let errors = (0..150)
            .map(|i| ScanError {
                path: format!("/photos/{i}.jpg"),
                stage: "preview".into(),
                message: "Decoder failed".into(),
            })
            .collect::<Vec<_>>();
        persist(&c, &r, &[], &errors).unwrap();
        let details = details_on(&c, "test", &r.id, None, 0).unwrap();
        assert_eq!(details.errors.len(), 100);
        assert_eq!(details.run.error_count, 150);
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    #[test]
    fn removing_an_album_removes_its_reports_and_error_paths() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE albums(id INTEGER PRIMARY KEY);INSERT INTO albums VALUES(1);",
        )
        .unwrap();
        ensure_schema(&c).unwrap();
        let r = Run {
            seq: 0,
            id: "owned".into(),
            library_id: "test".into(),
            album_id: 1,
            album_name: "Photos".into(),
            root_path: "/photos".into(),
            started_at: 1,
            updated_at: 1,
            finished_at: Some(1),
            status: "completed".into(),
            phase: "complete".into(),
            revision: 1,
            counts: Counts::default(),
            current_path: "/photos".into(),
            retry: RetrySummary::default(),
            error_count: 1,
            last_error: Some("bad".into()),
        };
        let f = Folder {
            run_id: r.id.clone(),
            path: "/photos".into(),
            path_key: path_key("/photos"),
            counts: Counts::default(),
            retry: RetrySummary::default(),
            entered: true,
            enumerated: true,
        };
        persist(
            &c,
            &r,
            &[f],
            &[ScanError {
                path: "/photos/a.jpg".into(),
                stage: "preview".into(),
                message: "bad".into(),
            }],
        )
        .unwrap();
        c.execute(
            "INSERT INTO scan_error_retries(run_id,ordinal,state) VALUES('owned',0,'failed')",
            [],
        )
        .unwrap();
        c.execute("DELETE FROM albums WHERE id=1", []).unwrap();
        for table in [
            "scan_runs",
            "scan_folders",
            "scan_errors",
            "scan_error_retries",
        ] {
            assert_eq!(
                c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
}

fn validate_directory(path: &str) -> Result<(), String> {
    let meta =
        std::fs::metadata(path).map_err(|e| format!("Cannot access directory '{path}': {e}"))?;
    if !meta.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }
    std::fs::read_dir(path).map_err(|e| format!("Cannot read directory '{path}': {e}"))?;
    Ok(())
}
#[tauri::command]
pub async fn open_scan_directory(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        validate_directory(&path)?;
        opener::open(&path).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod directory_open_tests {
    use super::*;
    #[test]
    fn directory_open_accepts_only_accessible_directories_not_urls_or_programs() {
        assert!(validate_directory("https://example.invalid").is_err());
        let path =
            std::env::temp_dir().join(format!("lap-open-directory-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"fixture").unwrap();
        assert!(validate_directory(path.to_str().unwrap()).is_err());
        std::fs::remove_file(path).unwrap();
        assert!(validate_directory(std::env::temp_dir().to_str().unwrap()).is_ok());
    }
}
