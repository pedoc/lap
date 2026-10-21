//! Targeted retries never run a mark-and-sweep or rewrite the original scan statistics.
use super::*;
use crate::{t_raw_display::RawDisplayOptions, t_sqlite::Album, t_utils};
use tauri::Manager;

pub(super) fn decorate_folder_on(c: &Connection, folder: &mut Folder) -> Result<(), String> {
    let prefix = format!("{}/", folder.path_key.trim_end_matches('/'));
    folder.retry = c.query_row("SELECT COALESCE(SUM(q.state='resolved'),0) AS resolved,COALESCE(SUM(q.state IN('queued','running')),0) AS running FROM scan_error_retries q JOIN scan_errors e ON e.run_id=q.run_id AND e.ordinal=q.ordinal WHERE e.run_id=?1 AND (e.path_key=?2 OR substr(e.path_key,1,length(?3))=?3)",params![folder.run_id,folder.path_key,prefix],|r|Ok(RetrySummary{resolved:r.get("resolved")?,running:r.get("running")?})).map_err(|e|e.to_string())?;
    Ok(())
}

pub(super) fn errors_on(
    c: &Connection,
    id: &str,
    folder: Option<&str>,
) -> Result<Vec<ErrorDetail>, String> {
    let key = folder.map(path_key);
    let prefix = key
        .as_ref()
        .map(|k| format!("{}/", k.trim_end_matches('/')));
    let mut stmt = c.prepare("SELECT e.ordinal,e.path,e.stage,e.message,COALESCE(q.attempts,0) AS attempts,COALESCE(q.state,'untried') AS retry_state,q.last_attempt_at,q.resolved_at,q.last_error FROM scan_errors e LEFT JOIN scan_error_retries q ON q.run_id=e.run_id AND q.ordinal=e.ordinal WHERE e.run_id=?1 AND (?2 IS NULL OR e.path_key=?2 OR substr(e.path_key,1,length(?3))=?3) ORDER BY e.ordinal LIMIT 100").map_err(|e| e.to_string())?;
    stmt.query_map(params![id, key, prefix], |r| {
        let stage: String = r.get("stage")?;
        let path: String = r.get("path")?;
        Ok(ErrorDetail {
            ordinal: r.get("ordinal")?,
            retryable: matches!(stage.as_str(), "metadata" | "preview" | "search")
                && t_utils::get_file_type(&path).is_some(),
            path,
            stage,
            message: r.get("message")?,
            attempts: r.get("attempts")?,
            retry_state: r.get("retry_state")?,
            last_attempt_at: r.get("last_attempt_at")?,
            resolved_at: r.get("resolved_at")?,
            retry_error: r.get("last_error")?,
        })
    })
    .map_err(|e| e.to_string())?
    .collect::<Result<_, _>>()
    .map_err(|e| e.to_string())
}
fn run_on(c: &Connection, library: &str, id: &str) -> Result<Run, String> {
    c.query_row(
        &format!("SELECT {RUN_COLUMNS} FROM scan_runs r WHERE r.id=?1"),
        [id],
        |r| read_run(r, library),
    )
    .map_err(|e| e.to_string())
}
fn bump(c: &Connection, id: &str) -> Result<(), String> {
    c.execute("UPDATE scan_runs SET revision=revision+1 WHERE id=?1", [id])
        .map(|_| ())
        .map_err(|e| e.to_string())
}
pub(super) fn recover_on(c: &Connection) -> Result<(), String> {
    let tx = c.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("UPDATE scan_runs SET revision=revision+1 WHERE id IN(SELECT run_id FROM scan_error_retries WHERE state IN('queued','running'))",[]).map_err(|e|e.to_string())?;
    tx.execute("UPDATE scan_error_retries SET state='interrupted',last_error='Retry interrupted before completion; it can be retried again' WHERE state IN('queued','running')",[]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
fn claim_on(
    c: &Connection,
    library: &str,
    id: &str,
    ordinals: Option<&[i64]>,
) -> Result<Vec<ErrorDetail>, String> {
    if ordinals.is_some_and(|v| v.is_empty() || v.len() > ERROR_LIMIT) {
        return Err("Select 1 to 100 saved errors to retry".into());
    }
    let tx = c.unchecked_transaction().map_err(|e| e.to_string())?;
    let run = run_on(&tx, library, id)?;
    let latest: String = tx
        .query_row(
            "SELECT id FROM scan_runs WHERE album_id=?1 ORDER BY seq DESC LIMIT 1",
            [run.album_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if latest != id
        || run.finished_at.is_none()
        || !matches!(
            run.status.as_str(),
            "warnings" | "failed" | "paused" | "interrupted" | "completed"
        )
    {
        return Err("Only saved errors from the latest ended scan can be retried".into());
    }
    if run.retry.running > 0 {
        return Err("These scan errors are already being retried".into());
    }
    let errors = errors_on(&tx, id, None)?;
    if let Some(ids) = ordinals {
        if ids.iter().any(|id| {
            !errors
                .iter()
                .any(|e| e.ordinal == *id && e.retryable && e.retry_state != "resolved")
        }) {
            return Err(
                "The selected error no longer needs a file retry; refresh the report".into(),
            );
        }
    }
    let targets: Vec<_> = errors
        .into_iter()
        .filter(|e| {
            e.retryable
                && e.retry_state != "resolved"
                && ordinals.is_none_or(|ids| ids.contains(&e.ordinal))
        })
        .collect();
    if targets.is_empty() {
        return Err(
            "No retryable saved file errors; directory and cleanup errors require a full scan"
                .into(),
        );
    }
    for e in &targets {
        tx.execute("INSERT INTO scan_error_retries(run_id,ordinal,state) VALUES(?1,?2,'queued') ON CONFLICT(run_id,ordinal) DO UPDATE SET state='queued',resolved_at=NULL",params![id,e.ordinal]).map_err(|e|e.to_string())?;
    }
    bump(&tx, id)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(targets)
}
fn start_on(c: &Connection, id: &str, ordinal: i64) -> Result<(), String> {
    let tx = c.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("UPDATE scan_error_retries SET state='running',attempts=attempts+1,last_attempt_at=?3,last_error=NULL WHERE run_id=?1 AND ordinal=?2 AND state='queued'",params![id,ordinal,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
    bump(&tx, id)?;
    tx.commit().map_err(|e| e.to_string())
}
fn finish_on(
    c: &Connection,
    id: &str,
    ordinal: i64,
    result: &Result<i64, String>,
) -> Result<(), String> {
    let tx = c.unchecked_transaction().map_err(|e| e.to_string())?;
    let error = result.as_ref().err();
    tx.execute("UPDATE scan_error_retries SET state=?3,resolved_at=?4,last_error=?5 WHERE run_id=?1 AND ordinal=?2 AND state='running'",params![id,ordinal,if error.is_some(){"failed"}else{"resolved"},if error.is_none(){Some(chrono::Utc::now().timestamp_millis())}else{None},error]).map_err(|e|e.to_string())?;
    bump(&tx, id)?;
    tx.commit().map_err(|e| e.to_string())
}
fn interrupt_on(c: &Connection, id: &str) -> Result<(), String> {
    let tx = c.unchecked_transaction().map_err(|e| e.to_string())?;
    if tx.execute("UPDATE scan_error_retries SET state='interrupted',last_error='Retry stopped before completion; retry again when ready' WHERE run_id=?1 AND state IN('queued','running')",[id]).map_err(|e|e.to_string())?>0 { bump(&tx,id)?; }
    tx.commit().map_err(|e| e.to_string())
}
struct RetryTask {
    conn: Connection,
    app: AppHandle,
    library: String,
    run_id: String,
    lease: Option<Arc<t_utils::AlbumScanGuard>>,
    _db_lease: crate::t_storage::DbConnectionLease,
}
impl RetryTask {
    fn notify(&self) {
        match run_on(&self.conn, &self.library, &self.run_id) {
            Ok(run) => {
                let _ = self.app.emit(
                    "scan-status-changed",
                    Event {
                        run,
                        folders: vec![],
                    },
                );
            }
            Err(e) => eprintln!("Cannot refresh scan retry report: {e}"),
        }
    }
}
impl Drop for RetryTask {
    fn drop(&mut self) {
        if let Err(e) = interrupt_on(&self.conn, &self.run_id) {
            eprintln!("Cannot save interrupted scan retry: {e}");
        }
        self.notify();
        drop(self.lease.take());
        let _ = self.app.emit("trigger-next-album", ());
    }
}
pub(super) async fn execute(
    app: AppHandle,
    library: String,
    run_id: String,
    ordinals: Option<Vec<i64>>,
    thumbnail_size: u32,
    raw: RawDisplayOptions,
    group_raw_jpeg_pairs: bool,
) -> Result<Run, String> {
    if !(32..=4096).contains(&thumbnail_size) {
        return Err("Invalid preview size".into());
    }
    tauri::async_runtime::spawn_blocking(move||{
        let token=app.state::<crate::t_cmds::IndexCancellation>().0.clone();
        let (task,album,targets)=crate::t_cmds::with_current_library(&library,||{
            let _db_lease=crate::t_storage::DbConnectionLease::acquire()?;
            let conn=Connection::open(crate::t_storage::get_current_db_path()?).map_err(|e|e.to_string())?;
            conn.busy_timeout(Duration::from_secs(5)).map_err(|e|e.to_string())?;
            let run=run_on(&conn,&library,&run_id)?;
            let lease=Arc::new(t_utils::AlbumScanGuard::acquire(run.album_id,token.clone())?);
            let album=Album::get_album_by_id(run.album_id)?;
            if path_key(&album.path)!=path_key(&run.root_path) {return Err("The album directory changed; run a full scan instead".into());}
            let targets=claim_on(&conn,&library,&run_id,ordinals.as_deref())?;
            Ok((RetryTask{conn,app:app.clone(),library:library.clone(),run_id:run_id.clone(),lease:Some(lease),_db_lease},album,targets))
        })?;
        task.notify();
        let cancelled=|| token.lock().map(|flags|flags.get(&album.id.unwrap_or(0))!=Some(&false)).unwrap_or(true);
        let mut changed=Vec::new();
        for error in targets {
            if cancelled(){break;}
            start_on(&task.conn,&run_id,error.ordinal)?;
            task.notify();
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||t_utils::retry_scan_file(&app,&album,&error.path,&error.stage,thumbnail_size,raw,group_raw_jpeg_pairs,&cancelled)))
                .unwrap_or_else(|_|Err("Media decoder failed unexpectedly during retry".into()));
            if cancelled() && result.is_err() {interrupt_on(&task.conn,&run_id)?;task.notify();break;}
            finish_on(&task.conn,&run_id,error.ordinal,&result)?;
            if let Ok(file_id)=result {changed.push(file_id);}
            task.notify();
        }
        interrupt_on(&task.conn,&run_id)?;
        if !changed.is_empty() {
            Album::recount_album(album.id.unwrap_or(0))?;
            Album::auto_set_cover(album.id.unwrap_or(0))?;
            let updated=Album::get_album_by_id(album.id.unwrap_or(0))?;
            let _=app.emit("albums-refreshed",serde_json::json!({"libraryId":library,"albums":[updated],"refreshFolders":true,"refreshCounts":true}));
            let _=app.emit("scan-retry-finished",serde_json::json!({"library_id":library,"album_id":album.id,"file_ids":changed}));
        }
        task.notify();
        run_on(&task.conn,&library,&run_id)
    }).await.map_err(|e|format!("Scan retry worker failed: {e}"))?
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(count: u64) -> (Connection, Run) {
        let c = Connection::open_in_memory().unwrap();
        ensure_schema(&c).unwrap();
        let run = Run {
            seq: 0,
            id: "scan-one".into(),
            library_id: "test".into(),
            album_id: 1,
            album_name: "Photos".into(),
            root_path: "/photos".into(),
            started_at: 100,
            updated_at: 200,
            finished_at: Some(200),
            status: "warnings".into(),
            phase: "complete".into(),
            revision: 1,
            counts: Counts {
                total: 10,
                discovered: 10,
                indexed: 8,
                metadata_failed: 2,
                ..Default::default()
            },
            current_path: "/photos/last.jpg".into(),
            retry: RetrySummary::default(),
            error_count: count,
            last_error: Some("Original decoder failure".into()),
        };
        let errors = (0..count)
            .map(|i| ScanError {
                path: format!("/photos/{i}.jpg"),
                stage: "metadata".into(),
                message: "Original decoder failure".into(),
            })
            .collect::<Vec<_>>();
        persist(&c, &run, &[], &errors).unwrap();
        (c, run)
    }
    #[test]
    fn successful_retry_resolves_only_its_error_and_preserves_original_history() {
        let (c, source) = fixture(2);
        let before = c
            .query_row(
                "SELECT status,counts_json,finished_at,error_count FROM scan_runs",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(claim_on(&c, "test", &source.id, None).unwrap().len(), 2);
        assert_eq!(run_on(&c, "test", &source.id).unwrap().retry.running, 2);
        start_on(&c, &source.id, 0).unwrap();
        finish_on(&c, &source.id, 0, &Ok(17)).unwrap();
        let now = run_on(&c, "test", &source.id).unwrap();
        assert_eq!(now.retry.resolved, 1);
        assert_eq!(now.retry.running, 1);
        assert!(now.revision > source.revision);
        let after = c
            .query_row(
                "SELECT status,counts_json,finished_at,error_count FROM scan_runs",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(before, after);
        let errors = errors_on(&c, &source.id, None).unwrap();
        assert_eq!(errors[0].message, "Original decoder failure");
        assert_eq!(errors[0].retry_state, "resolved");
        assert!(errors[0].resolved_at.is_some());
        assert_eq!(errors[1].attempts, 0);
    }
    #[test]
    fn failed_retries_keep_the_new_reason_and_allow_another_attempt_without_touching_other_errors()
    {
        let (c, r) = fixture(2);
        claim_on(&c, "test", &r.id, Some(&[0])).unwrap();
        start_on(&c, &r.id, 0).unwrap();
        finish_on(&c, &r.id, 0, &Err("Original file remains corrupt".into())).unwrap();
        let e = errors_on(&c, &r.id, None).unwrap();
        assert_eq!(e[0].attempts, 1);
        assert_eq!(
            e[0].retry_error.as_deref(),
            Some("Original file remains corrupt")
        );
        assert_eq!(e[1].retry_state, "untried");
        claim_on(&c, "test", &r.id, Some(&[0, 0])).unwrap();
        start_on(&c, &r.id, 0).unwrap();
        finish_on(&c, &r.id, 0, &Ok(17)).unwrap();
        let e = errors_on(&c, &r.id, None).unwrap();
        assert_eq!(e[0].attempts, 2);
        assert_eq!(e[0].retry_state, "resolved");
        assert!(e[0].retry_error.is_none());
        assert!(claim_on(&c, "test", &r.id, Some(&[0])).is_err());
    }
    #[test]
    fn a_retry_claim_rejects_duplicate_running_jobs_invalid_ids_and_outdated_reports() {
        let (c, r) = fixture(2);
        assert!(claim_on(&c, "test", &r.id, Some(&[99])).is_err());
        assert!(claim_on(&c, "test", &r.id, Some(&[])).is_err());
        assert_eq!(run_on(&c, "test", &r.id).unwrap().retry.running, 0);
        claim_on(&c, "test", &r.id, Some(&[0])).unwrap();
        assert!(claim_on(&c, "test", &r.id, Some(&[1])).is_err());
        interrupt_on(&c, &r.id).unwrap();
        let mut later = r.clone();
        later.id = "other-album".into();
        later.album_id = 2;
        persist(&c, &later, &[], &[]).unwrap();
        assert_eq!(claim_on(&c, "test", &r.id, Some(&[0])).unwrap().len(), 1);
        interrupt_on(&c, &r.id).unwrap();
        later.id = "new-scan".into();
        later.album_id = 1;
        persist(&c, &later, &[], &[]).unwrap();
        assert!(claim_on(&c, "test", &r.id, None).is_err());
        later.status = "running".into();
        later.finished_at = None;
        later.revision += 1;
        persist(&c, &later, &[], &[]).unwrap();
        assert!(claim_on(&c, "test", &later.id, None).is_err());
    }
    #[test]
    fn an_interrupted_retry_is_neither_running_nor_resolved_and_remains_retryable() {
        let (c, r) = fixture(2);
        claim_on(&c, "test", &r.id, None).unwrap();
        start_on(&c, &r.id, 0).unwrap();
        recover_on(&c).unwrap();
        let report = run_on(&c, "test", &r.id).unwrap();
        assert_eq!(report.retry.running, 0);
        assert_eq!(report.retry.resolved, 0);
        assert_eq!(report.status, "warnings");
        let errors = errors_on(&c, &r.id, None).unwrap();
        assert_eq!(errors[0].retry_state, "interrupted");
        assert_eq!(errors[0].attempts, 1);
        assert_eq!(errors[1].attempts, 0);
        assert_eq!(claim_on(&c, "test", &r.id, None).unwrap().len(), 2);
    }
    #[test]
    fn saved_error_limit_does_not_make_unrecorded_errors_look_resolved() {
        let (c, r) = fixture(150);
        let plan = claim_on(&c, "test", &r.id, None).unwrap();
        assert_eq!(plan.len(), 100);
        for e in plan {
            start_on(&c, &r.id, e.ordinal).unwrap();
            finish_on(&c, &r.id, e.ordinal, &Ok(17)).unwrap();
        }
        let report = run_on(&c, "test", &r.id).unwrap();
        assert_eq!(report.retry.resolved, 100);
        assert_eq!(report.error_count, 150);
        assert_eq!(report.retry.running, 0);
        assert!(claim_on(&c, "test", &r.id, None).is_err());
    }
    #[test]
    fn directory_scopes_and_non_file_errors_cannot_expand_a_targeted_retry() {
        let (c, r) = fixture(2);
        c.execute(
            "UPDATE scan_errors SET path='/photos/A/one.jpg',path_key=?1 WHERE ordinal=0",
            [path_key("/photos/A/one.jpg")],
        )
        .unwrap();
        c.execute("UPDATE scan_errors SET path='/photos/AB',path_key=?1,stage='directory' WHERE ordinal=1",[path_key("/photos/AB")]).unwrap();
        let selected = errors_on(&c, &r.id, Some("/photos/A")).unwrap();
        assert_eq!(selected.len(), 1);
        assert!(selected[0].retryable);
        assert!(!errors_on(&c, &r.id, None).unwrap()[1].retryable);
        assert!(claim_on(&c, "test", &r.id, Some(&[1])).is_err());
        assert_eq!(claim_on(&c, "test", &r.id, None).unwrap().len(), 1);
    }
    #[test]
    fn history_retention_also_removes_retry_rows() {
        let (c, r) = fixture(1);
        claim_on(&c, "test", &r.id, None).unwrap();
        interrupt_on(&c, &r.id).unwrap();
        for i in 0..HISTORY_LIMIT {
            let mut later = r.clone();
            later.id = format!("later-{i}");
            persist(&c, &later, &[], &[]).unwrap();
        }
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM scan_error_retries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
