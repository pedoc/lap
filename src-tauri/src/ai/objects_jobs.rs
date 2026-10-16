//! Explicit resumable object indexing. Models are global; observations are library-local.
use super::{media, objects_store as store, types::Task, vision_jobs};
use rusqlite::{Connection, ToSql, params_from_iter};
use serde::Serialize;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub library_id: String,
    pub job_id: String,
    pub running: bool,
    pub stopping: bool,
    pub cancelled: bool,
    pub total: usize,
    pub completed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped: usize,
    pub phase: String,
    pub scope: String,
    pub scope_name: Option<String>,
    pub error: Option<String>,
}
struct Job {
    cancel: AtomicBool,
    status: Mutex<Status>,
}
static JOB: Mutex<Option<Arc<Job>>> = Mutex::new(None);
pub fn is_running() -> bool {
    JOB.lock()
        .map(|slot| {
            slot.as_ref()
                .is_some_and(|job| job.status.lock().is_ok_and(|status| status.running))
        })
        .unwrap_or(true)
}
pub fn status(library: &str) -> Status {
    JOB.lock()
        .ok()
        .and_then(|slot| {
            slot.as_ref().and_then(|job| {
                job.status
                    .lock()
                    .ok()
                    .filter(|status| status.library_id == library)
                    .map(|status| status.clone())
            })
        })
        .unwrap_or_else(|| Status {
            library_id: library.into(),
            ..Default::default()
        })
}
fn check(job: &Job, library: &str) -> Result<(), String> {
    media::check_library(library)?;
    if job.cancel.load(Ordering::SeqCst) {
        return Err("Object recognition stopped; completed results are saved".into());
    }
    Ok(())
}
fn targets(
    c: &Connection,
    profile: &str,
    force: bool,
    retry: bool,
    ids: Option<&[i64]>,
    folder: Option<(i64, String)>,
) -> Result<Vec<i64>, String> {
    let mut sql = format!("SELECT a.id {}", media::base());
    let mut values: Vec<Box<dyn ToSql>> = Vec::new();
    if !force {
        sql.push_str(" AND ");
        sql.push_str(&store::state_condition(
            "?",
            if retry { "failed" } else { "pending" },
        )?);
        values.push(Box::new(profile.to_owned()));
    }
    if let Some(ids) = ids {
        sql.push_str(" AND a.id IN(SELECT value FROM json_each(?))");
        values.push(Box::new(
            serde_json::to_string(ids).map_err(|e| e.to_string())?,
        ));
    }
    if let Some((album, path)) = folder {
        let escaped = path
            .trim_end_matches(['/', '\\'])
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        sql.push_str(" AND b.album_id=? AND(b.path=? OR b.path LIKE ? ESCAPE '\\' OR b.path LIKE ? ESCAPE '\\')");
        values.extend([
            Box::new(album) as Box<dyn ToSql>,
            Box::new(path),
            Box::new(format!("{escaped}/%")),
            Box::new(format!("{escaped}\\\\%")),
        ]);
    }
    sql.push_str(" ORDER BY a.id");
    let mut statement = c.prepare(&sql).map_err(|e| e.to_string())?;
    statement
        .query_map(params_from_iter(values.iter()), |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn start_object_index(
    app_handle: AppHandle,
    library_id: String,
    force: bool,
    retry_failed: Option<bool>,
    file_ids: Option<Vec<i64>>,
    folder_id: Option<i64>,
) -> Result<Status, String> {
    let retry = retry_failed.unwrap_or(false);
    if force && retry || folder_id.is_some() && file_ids.is_some() {
        return Err("Choose one object indexing mode and scope".into());
    }
    if file_ids
        .as_ref()
        .is_some_and(|ids| ids.is_empty() || ids.len() > 10000 || ids.iter().any(|id| *id <= 0))
    {
        return Err("Select 1–10000 indexed images for object detection".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        media::check_library(&library_id)?;
        if *app_handle
            .state::<crate::t_face::FaceIndexingStatus>()
            .0
            .lock()
            .map_err(|e| e.to_string())?
            || !app_handle
                .state::<crate::t_cmds::IndexCancellation>()
                .0
                .lock()
                .map_err(|e| e.to_string())?
                .is_empty()
            || app_handle
                .state::<crate::t_similar::SimilarState>()
                .status
                .lock()
                .map_err(|e| e.to_string())?
                .is_scanning
        {
            return Err(
                "Wait for library, face or similarity indexing before detecting objects".into(),
            );
        }
        if super::ocr::is_running() && !super::ocr::is_automatic_running() {
            return Err(
                "Stop manual OCR or region extraction before starting object recognition".into(),
            );
        }
        let model = super::settings::active(Task::Detection)?;
        super::assets::verify(&model.definition)?;
        let c = crate::t_sqlite::open_conn()?;
        store::ensure_schema(&c)?;
        store::ensure_model(&c, &model)?;
        let folder = folder_id
            .map(|id| {
                c.query_row(
                    "SELECT album_id,path FROM afolders WHERE id=?1",
                    [id],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
                )
                .map_err(|e| e.to_string())
            })
            .transpose()?;
        let scope = if file_ids.is_some() {
            "selected"
        } else if folder.is_some() {
            "folder"
        } else {
            "library"
        };
        let initial = Status {
            library_id: library_id.clone(),
            job_id: uuid::Uuid::new_v4().to_string(),
            running: true,
            phase: "preparing".into(),
            scope: scope.into(),
            scope_name: folder.as_ref().map(|(_, path)| path.clone()),
            ..Default::default()
        };
        let mut slot = JOB.lock().map_err(|e| e.to_string())?;
        if slot
            .as_ref()
            .is_some_and(|job| job.status.lock().is_ok_and(|s| s.running))
        {
            return Err("An object recognition job is already running".into());
        }
        let job = Arc::new(Job {
            cancel: AtomicBool::new(false),
            status: Mutex::new(initial.clone()),
        });
        *slot = Some(job.clone());
        drop(slot);
        super::ocr::yield_automatic();
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _lane = loop {
                    check(&job, &library_id)?;
                    match vision_jobs::try_claim(Task::Detection) {
                        Ok(lease) => break lease,
                        Err(_) => {
                            job.status.lock().map_err(|e| e.to_string())?.phase = "waiting".into();
                            std::thread::sleep(Duration::from_millis(50));
                        }
                    }
                };
                let mut c = {
                    let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
                        .read()
                        .map_err(|e| e.to_string())?;
                    check(&job, &library_id)?;
                    crate::t_sqlite::open_conn()?
                };
                let profile = model.profile();
                let ids = targets(&c, &profile, force, retry, file_ids.as_deref(), folder)?;
                {
                    let mut status = job.status.lock().map_err(|e| e.to_string())?;
                    status.total = ids.len();
                    status.phase = "indexing".into();
                }
                if ids.is_empty() {
                    return Ok::<(), String>(());
                }
                let mut backend = super::adapters::create_object_backend(&model)?;
                for id in ids {
                    check(&job, &library_id)?;
                    let source = {
                        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
                            .read()
                            .map_err(|e| e.to_string())?;
                        check(&job, &library_id)?;
                        media::source_optional_on(&c, id)?
                    };
                    let Some(source) = source else {
                        let mut status = job.status.lock().map_err(|e| e.to_string())?;
                        status.completed += 1;
                        status.skipped += 1;
                        continue;
                    };
                    let mut result = source.load().and_then(|image| {
                        backend.detect_objects(&image, &|| check(&job, &library_id))
                    });
                    if let Err(error) = source.verify() {
                        result = Err(error);
                    }
                    check(&job, &library_id)?;
                    let written = {
                        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
                            .read()
                            .map_err(|e| e.to_string())?;
                        check(&job, &library_id)?;
                        if let Err(error) = &mut result {
                            *error = error.replace(&source.path, "[photo]");
                        }
                        store::store(&mut c, id, &profile, source.modified, source.size, &result)?
                    };
                    {
                        let mut status = job.status.lock().map_err(|e| e.to_string())?;
                        status.completed += 1;
                        if !written {
                            status.skipped += 1;
                        } else if result.is_ok() {
                            status.succeeded += 1;
                        } else {
                            status.failed += 1;
                        }
                    }
                    let _ = app_handle.emit(
                        "object-index-updated",
                        serde_json::json!({"libraryId":library_id,"fileId":id}),
                    );
                }
                Ok(())
            }));
            if let Ok(mut status) = job.status.lock() {
                status.running = false;
                status.stopping = false;
                status.cancelled =
                    job.cancel.load(Ordering::SeqCst) || media::check_library(&library_id).is_err();
                status.error = if status.cancelled {
                    None
                } else {
                    match result {
                        Ok(Ok(())) => None,
                        Ok(Err(error)) => Some(error),
                        Err(_) => Some(
                            "Object worker stopped unexpectedly; completed results are saved"
                                .into(),
                        ),
                    }
                };
            }
            let _ = app_handle.emit(
                "object-index-updated",
                serde_json::json!({"libraryId":library_id,"finished":true}),
            );
        });
        Ok(initial)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn cancel_object_index(library_id: String, job_id: String) -> Result<(), String> {
    media::check_library(&library_id)?;
    let job = JOB
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("No object indexing job")?;
    let mut status = job.status.lock().map_err(|e| e.to_string())?;
    if status.library_id != library_id || status.job_id != job_id {
        return Err("Stale object indexing job".into());
    }
    job.cancel.store(true, Ordering::SeqCst);
    status.stopping = status.running;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_scope_and_retry_modes_never_expand_into_other_images() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT,file_types INTEGER,small_image_filter INTEGER,excluded_folders TEXT);CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,path TEXT,is_excluded_from_search INTEGER DEFAULT 0);CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,file_type INTEGER,width INTEGER,height INTEGER,live_photo_video_id INTEGER,modified_at INTEGER,size INTEGER);CREATE TABLE media_states(file_id INTEGER PRIMARY KEY,hidden INTEGER DEFAULT 0,deleted_at INTEGER);INSERT INTO albums VALUES(1,'/photos',7,0,'[]');INSERT INTO afolders VALUES(1,1,'/photos/trip_%',0),(2,1,'/photos/trip_%/child',0),(3,1,'/photos/trip_%2',0);INSERT INTO afiles VALUES(1,1,1,100,100,NULL,10,20),(2,2,1,100,100,NULL,10,20),(3,3,1,100,100,NULL,10,20);").unwrap();
        store::ensure_schema(&c).unwrap();
        assert_eq!(
            targets(&c, "p", false, false, Some(&[2]), None).unwrap(),
            vec![2]
        );
        assert_eq!(
            targets(
                &c,
                "p",
                false,
                false,
                None,
                Some((1, "/photos/trip_%".into()))
            )
            .unwrap(),
            vec![1, 2]
        );
        c.execute(
            "INSERT INTO object_index VALUES(1,'p',10,20,NULL,'error',0,0)",
            [],
        )
        .unwrap();
        assert_eq!(targets(&c, "p", false, true, None, None).unwrap(), vec![1]);
        assert_eq!(
            targets(&c, "p", false, false, None, None).unwrap(),
            vec![2, 3]
        );
        assert_eq!(
            targets(&c, "p", true, false, Some(&[1]), None).unwrap(),
            vec![1]
        );
    }
}
