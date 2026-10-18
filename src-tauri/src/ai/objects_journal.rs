//! Durable object-task intent and exact item ledger. No original media is modified.
use super::objects_jobs::Status;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub force: bool,
    pub retry_failed: bool,
    pub file_ids: Option<Vec<i64>>,
    pub folder_id: Option<i64>,
    pub batch_size: Option<usize>,
    pub model_values: std::collections::BTreeMap<String, serde_json::Value>,
    pub model_version: String,
    pub selected_sources: std::collections::BTreeMap<i64, String>,
}
impl Request {
    pub fn validate(&self) -> Result<(), String> {
        if self.force && self.retry_failed
            || self.file_ids.is_some() && self.folder_id.is_some()
            || self.folder_id.is_some_and(|id| id <= 0)
            || self
                .batch_size
                .is_some_and(|size| !(1..=100).contains(&size))
            || self.file_ids.as_ref().is_some_and(|ids| {
                ids.is_empty() || ids.len() > 10000 || ids.iter().any(|id| *id <= 0)
            })
        {
            return Err(
                "Choose one valid object indexing mode and scope (1–10000 explicit photos)".into(),
            );
        }
        Ok(())
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub status: Status,
    pub model_id: String,
    pub model_name: String,
    pub profile: String,
    pub compatible: bool,
    pub planned: bool,
    pub model_values: std::collections::BTreeMap<String, serde_json::Value>,
    pub model_version: String,
    pub remaining: i64,
    pub resumable: bool,
    pub retryable: bool,
    pub resumed_by: Option<String>,
    pub retried_by: Option<String>,
    pub created_at: i64,
}
pub struct Recovery {
    pub request: Request,
    pub ids: Option<Vec<i64>>,
    pub scope: String,
    pub scope_name: Option<String>,
    pub parent_id: String,
    pub failed: bool,
}
struct Record {
    status: Status,
    request: Request,
    profile: String,
    model_id: String,
    model_name: String,
    planned: bool,
    resumed_by: Option<String>,
    retried_by: Option<String>,
    created_at: i64,
}
pub fn ensure_schema(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS object_jobs(job_id TEXT PRIMARY KEY,library_id TEXT NOT NULL,profile TEXT NOT NULL,model_id TEXT NOT NULL,model_name TEXT NOT NULL,request_json TEXT NOT NULL,status_json TEXT NOT NULL,planned INTEGER NOT NULL DEFAULT 0,resumed_by TEXT,retried_by TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);
 CREATE INDEX IF NOT EXISTS object_jobs_library ON object_jobs(library_id,created_at);
 CREATE TABLE IF NOT EXISTS object_job_items(job_id TEXT NOT NULL,file_id INTEGER NOT NULL,state TEXT NOT NULL DEFAULT 'pending',source_key TEXT,PRIMARY KEY(job_id,file_id));
 CREATE INDEX IF NOT EXISTS object_job_pending ON object_job_items(job_id,state,file_id);") .map_err(|e|e.to_string())?;
    let has_key = {
        let mut stmt = c
            .prepare("PRAGMA table_info(object_job_items)")
            .map_err(|e| e.to_string())?;
        let names = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        names.iter().any(|n| n == "source_key")
    };
    if !has_key {
        c.execute(
            "ALTER TABLE object_job_items ADD COLUMN source_key TEXT",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn load(c: &Connection, id: &str, library: &str) -> Result<Record, String> {
    let row=c.query_row("SELECT profile,model_id,model_name,request_json,status_json,planned,resumed_by,retried_by,created_at FROM object_jobs WHERE job_id=?1 AND library_id=?2",params![id,library],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,bool>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,Option<String>>(7)?,r.get::<_,i64>(8)?))).optional().map_err(|e|e.to_string())?.ok_or("Object task was not found in this library")?;
    let request: Request =
        serde_json::from_str(&row.3).map_err(|e| format!("Stored task scope is invalid: {e}"))?;
    request.validate()?;
    let mut status: Status =
        serde_json::from_str(&row.4).map_err(|e| format!("Stored task status is invalid: {e}"))?;
    status.job_id = id.into();
    status.library_id = library.into();
    Ok(Record {
        profile: row.0,
        model_id: row.1,
        model_name: row.2,
        request,
        status,
        planned: row.5,
        resumed_by: row.6,
        retried_by: row.7,
        created_at: row.8,
    })
}
pub fn begin(
    c: &mut Connection,
    status: &Status,
    request: &Request,
    profile: &str,
    model_id: &str,
    model_name: &str,
    parent: Option<&Recovery>,
) -> Result<(), String> {
    request.validate()?;
    let tx = c.transaction().map_err(|e| e.to_string())?;
    if let Some(parent) = parent {
        let column = if parent.failed {
            "retried_by"
        } else {
            "resumed_by"
        };
        let changed=tx.execute(&format!("UPDATE object_jobs SET {column}=?1 WHERE job_id=?2 AND library_id=?3 AND profile=?4 AND {column} IS NULL"),params![status.job_id,parent.parent_id,status.library_id,profile]).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("This task has already been continued; use its latest child task".into());
        }
    }
    let now = chrono::Utc::now().timestamp_millis();
    tx.execute("INSERT INTO object_jobs(job_id,library_id,profile,model_id,model_name,request_json,status_json,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8)",params![status.job_id,status.library_id,profile,model_id,model_name,serde_json::to_string(request).map_err(|e|e.to_string())?,serde_json::to_string(status).map_err(|e|e.to_string())?,now]).map_err(|e|e.to_string())?;
    if let Some(parent) = parent.filter(|parent| parent.ids.is_some()) {
        let copied=tx.execute("INSERT INTO object_job_items(job_id,file_id,state,source_key) SELECT ?1,file_id,'pending',source_key FROM object_job_items WHERE job_id=?2 AND state=?3",params![status.job_id,parent.parent_id,if parent.failed{"failed"}else{"pending"}]).map_err(|e|e.to_string())?;
        if copied != parent.ids.as_ref().map(Vec::len).unwrap_or(0) || copied == 0 {
            return Err("The saved task item list changed; reload before continuing".into());
        }
        tx.execute(
            "UPDATE object_jobs SET planned=1 WHERE job_id=?1",
            [&status.job_id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
#[cfg(test)]
pub fn plan(
    c: &mut Connection,
    id: &str,
    ids: &[i64],
    check: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    plan_sources(c, id, ids, check, |_, file| {
        Ok(blake3::hash(file.to_string().as_bytes())
            .to_hex()
            .to_string())
    })
}
pub fn plan_sources(
    c: &mut Connection,
    id: &str,
    ids: &[i64],
    check: impl Fn() -> Result<(), String>,
    source: impl Fn(&Connection, i64) -> Result<String, String>,
) -> Result<(), String> {
    let tx = c.transaction().map_err(|e| e.to_string())?;
    {
        let mut insert = tx
            .prepare("INSERT INTO object_job_items(job_id,file_id,source_key) VALUES(?1,?2,?3)")
            .map_err(|e| e.to_string())?;
        for (index, file) in ids.iter().enumerate() {
            if index % 256 == 0 {
                check()?;
            }
            if *file <= 0 {
                return Err("Invalid planned photo ID".into());
            }
            let key = source(&tx, *file)?;
            insert
                .execute(params![id, file, key])
                .map_err(|e| e.to_string())?;
        }
    }
    check()?;
    if tx
        .execute(
            "UPDATE object_jobs SET planned=1 WHERE job_id=?1 AND planned=0",
            [id],
        )
        .map_err(|e| e.to_string())?
        != 1
    {
        return Err("Task plan is missing or already prepared".into());
    }
    tx.commit().map_err(|e| e.to_string())
}
pub fn expected_source(c: &Connection, job: &str, file: i64) -> Result<String, String> {
    c.query_row("SELECT source_key FROM object_job_items WHERE job_id=?1 AND file_id=?2",params![job,file],|r|r.get::<_,Option<String>>(0)).optional().map_err(|e|e.to_string())?.flatten().filter(|key|key.len()==64&&key.bytes().all(|c|c.is_ascii_hexdigit())).ok_or("Saved task source identity is missing; start a new task instead of replaying unverified IDs".into())
}
pub fn source_matches(c: &Connection, job: &str, file: i64) -> Result<bool, String> {
    let expected = expected_source(c, job, file)?;
    Ok(super::media::source_optional_on(c, file)?
        .map(|source| source.stamp_key())
        .transpose()?
        .as_deref()
        == Some(expected.as_str()))
}
/// Called inside the same transaction as object_index; a crash cannot leave a successful photo pending.
pub fn mark(c: &Connection, job: &str, file: i64, state: &str) -> Result<(), String> {
    if !matches!(state, "succeeded" | "failed" | "skipped") {
        return Err("Invalid task item outcome".into());
    }
    let changed=c.execute("UPDATE object_job_items SET state=?3 WHERE job_id=?1 AND file_id=?2 AND state='pending'",params![job,file,state]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("Task item is missing or was already committed".into());
    }
    Ok(())
}
pub fn save_status(c: &Connection, status: &Status) -> Result<(), String> {
    c.execute(
        "UPDATE object_jobs SET status_json=?2,updated_at=?3 WHERE job_id=?1",
        params![
            status.job_id,
            serde_json::to_string(status).map_err(|e| e.to_string())?,
            chrono::Utc::now().timestamp_millis()
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn item_ids(c: &Connection, id: &str, state: &str) -> Result<Vec<i64>, String> {
    let mut stmt = c
        .prepare(
            "SELECT file_id FROM object_job_items WHERE job_id=?1 AND state=?2 ORDER BY file_id",
        )
        .map_err(|e| e.to_string())?;
    stmt.query_map(params![id, state], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
pub fn recover(
    c: &Connection,
    id: &str,
    library: &str,
    profile: &str,
    failed: bool,
) -> Result<Recovery, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "Invalid object task ID")?;
    let record = load(c, id, library)?;
    if record.profile != profile {
        return Err(format!(
            "Task used {} with a different model/parameter profile. Switch back to its saved configuration, or start a new task; no model was changed automatically",
            record.model_name
        ));
    }
    if (failed && record.retried_by.is_some()) || (!failed && record.resumed_by.is_some()) {
        return Err("This task has already been continued; use its latest child task".into());
    }
    if failed && !record.planned {
        return Err("This task has not planned any failed photos".into());
    }
    if record.planned {
        let missing:i64=c.query_row("SELECT COUNT(*) FROM object_job_items WHERE job_id=?1 AND state=?2 AND source_key IS NULL",params![id,if failed{"failed"}else{"pending"}],|r|r.get(0)).map_err(|e|e.to_string())?;
        if missing > 0 {
            return Err("This older task lacks source identity snapshots; start a new task instead of replaying potentially reused file IDs".into());
        }
    }
    let ids = if record.planned {
        Some(item_ids(c, id, if failed { "failed" } else { "pending" })?)
    } else {
        None
    };
    if ids.as_ref().is_some_and(Vec::is_empty) {
        return Err("No matching unfinished photos remain in this task".into());
    }
    Ok(Recovery {
        request: record.request,
        ids,
        scope: record.status.scope,
        scope_name: record.status.scope_name,
        parent_id: id.into(),
        failed,
    })
}
pub fn list(
    c: &Connection,
    library: &str,
    profile: &str,
    live: &Status,
) -> Result<Vec<Entry>, String> {
    let mut statement=c.prepare("SELECT job_id FROM object_jobs WHERE library_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 20").map_err(|e|e.to_string())?;
    let ids = statement
        .query_map([library], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for id in ids {
        let record = load(c, &id, library)?;
        let mut status = record.status;
        let mut pending = 0;
        if record.planned {
            status.total = 0;
            status.completed = 0;
            status.succeeded = 0;
            status.failed = 0;
            status.skipped = 0;
            let mut counts = c
                .prepare(
                    "SELECT state,COUNT(*) FROM object_job_items WHERE job_id=?1 GROUP BY state",
                )
                .map_err(|e| e.to_string())?;
            let rows = counts
                .query_map([&id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, usize>(1)?))
                })
                .map_err(|e| e.to_string())?;
            for row in rows {
                let (state, n) = row.map_err(|e| e.to_string())?;
                status.total += n;
                match state.as_str() {
                    "pending" => pending = n,
                    "succeeded" => status.succeeded = n,
                    "failed" => status.failed = n,
                    "skipped" => status.skipped = n,
                    _ => return Err("Unknown persisted object task item state".into()),
                }
            }
            status.completed = status.succeeded + status.failed + status.skipped;
        }
        let known = live.job_id == id;
        let active = known && live.running;
        status.historical = !known;
        if known {
            status.phase = live.phase.clone();
            status.running = live.running;
            status.stopping = live.stopping;
            status.cancelled = live.cancelled;
            status.error = live.error.clone();
        } else {
            if status.running {
                status.phase = "interrupted".into();
                status.cancelled = false;
            }
            status.running = false;
            status.stopping = false;
        }
        let compatible = record.profile == profile;
        let resumable = !active && record.resumed_by.is_none() && (!record.planned || pending > 0);
        let retryable = !active && record.retried_by.is_none() && status.failed > 0;
        entries.push(Entry {
            remaining: pending as i64,
            status,
            model_id: record.model_id,
            model_name: record.model_name,
            profile: record.profile,
            compatible,
            resumable,
            retryable,
            resumed_by: record.resumed_by,
            retried_by: record.retried_by,
            created_at: record.created_at,
            planned: record.planned,
            model_values: record.request.model_values,
            model_version: record.request.model_version,
        });
    }
    Ok(entries)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        ensure_schema(&c).unwrap();
        c
    }
    fn status() -> Status {
        Status {
            library_id: "A".into(),
            job_id: uuid::Uuid::new_v4().to_string(),
            running: true,
            phase: "indexing".into(),
            scope: "selected".into(),
            ..Default::default()
        }
    }
    #[test]
    fn restart_recovers_exact_pending_ids_and_retries_failed_without_expanding_scope() {
        let mut c = db();
        let root = status();
        let request = Request {
            file_ids: Some(vec![1, 2, 3]),
            ..Default::default()
        };
        begin(&mut c, &root, &request, "p", "model", "Model", None).unwrap();
        plan(&mut c, &root.job_id, &[1, 2, 3], || Ok(())).unwrap();
        mark(&c, &root.job_id, 1, "succeeded").unwrap();
        mark(&c, &root.job_id, 2, "failed").unwrap();
        let entries = list(&c, "A", "p", &Status::default()).unwrap();
        assert_eq!(entries[0].status.phase, "interrupted");
        assert_eq!(entries[0].status.completed, 2);
        assert_eq!(entries[0].remaining, 1);
        assert!(entries[0].resumable && entries[0].retryable);
        let pending = recover(&c, &root.job_id, "A", "p", false).unwrap();
        assert_eq!(pending.ids, Some(vec![3]));
        let child = status();
        begin(
            &mut c,
            &child,
            &pending.request,
            "p",
            "model",
            "Model",
            Some(&pending),
        )
        .unwrap();
        assert!(recover(&c, &root.job_id, "A", "p", false).is_err());
        assert_eq!(
            recover(&c, &child.job_id, "A", "p", false).unwrap().ids,
            Some(vec![3])
        );
        let failed = recover(&c, &root.job_id, "A", "p", true).unwrap();
        assert_eq!(failed.ids, Some(vec![2]));
        assert!(recover(&c, &root.job_id, "B", "p", true).is_err());
        assert!(recover(&c, &root.job_id, "A", "other", true).is_err());
    }
    #[test]
    fn planning_interruption_preserves_the_original_intent_and_never_partially_commits_a_plan() {
        let mut c = db();
        let root = status();
        let request = Request {
            folder_id: Some(12),
            force: true,
            ..Default::default()
        };
        begin(&mut c, &root, &request, "p", "m", "Model", None).unwrap();
        assert!(plan(&mut c, &root.job_id, &[1, 2], || Err("stopped".into())).is_err());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM object_job_items", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let recovery = recover(&c, &root.job_id, "A", "p", false).unwrap();
        assert!(recovery.ids.is_none());
        assert_eq!(recovery.request.folder_id, Some(12));
        assert!(recovery.request.force);
    }
    #[test]
    fn unplanned_automatic_batches_keep_their_folder_and_limit_for_manual_recovery() {
        let mut c = db();
        let mut root = status();
        root.automatic = true;
        root.scope = "folder".into();
        root.scope_name = Some("/photos/trip".into());
        let request = Request {
            folder_id: Some(12),
            batch_size: Some(20),
            ..Default::default()
        };
        begin(&mut c, &root, &request, "p", "m", "Model", None).unwrap();
        let recovery = recover(&c, &root.job_id, "A", "p", false).unwrap();
        assert_eq!(recovery.scope, "folder");
        assert_eq!(recovery.scope_name.as_deref(), Some("/photos/trip"));
        assert_eq!(recovery.request.folder_id, Some(12));
        assert_eq!(recovery.request.batch_size, Some(20));
        assert!(recovery.ids.is_none());
        assert!(!recovery.request.force && !recovery.request.retry_failed);
        let mut invalid = request;
        invalid.batch_size = Some(0);
        assert!(invalid.validate().is_err());
    }
    #[test]
    fn selected_intent_retains_source_identity_even_before_a_plan_exists() {
        let mut c = db();
        let root = status();
        let mut request = Request {
            file_ids: Some(vec![7]),
            ..Default::default()
        };
        request
            .selected_sources
            .insert(7, blake3::hash(b"original source").to_hex().to_string());
        begin(&mut c, &root, &request, "p", "m", "Model", None).unwrap();
        let recovered = recover(&c, &root.job_id, "A", "p", false).unwrap();
        assert!(recovered.ids.is_none());
        assert_eq!(recovered.request.selected_sources, request.selected_sources);
    }
    #[test]
    fn live_status_is_not_misreported_as_an_interruption_and_terminal_items_cannot_commit_twice() {
        let mut c = db();
        let root = status();
        begin(&mut c, &root, &Request::default(), "p", "m", "M", None).unwrap();
        plan(&mut c, &root.job_id, &[9], || Ok(())).unwrap();
        assert!(list(&c, "A", "p", &root).unwrap()[0].status.running);
        mark(&c, &root.job_id, 9, "skipped").unwrap();
        assert!(mark(&c, &root.job_id, 9, "succeeded").is_err());
        let entries = list(&c, "A", "p", &Status::default()).unwrap();
        assert!(!entries[0].resumable);
        assert_eq!(entries[0].status.completed, 1);
        assert!(list(&c, "B", "p", &Status::default()).unwrap().is_empty());
    }
}
