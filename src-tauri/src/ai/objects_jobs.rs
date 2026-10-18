//! Explicit resumable object indexing. Models are global; observations are library-local.
use super::{media, objects_store as store, types::Task, vision_jobs};
use rusqlite::{Connection, ToSql, params_from_iter};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Emitter, Manager};
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
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
    pub historical: bool,
    pub execution: serde_json::Value,
    pub automatic: bool,
    pub automation_revision: Option<String>,
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
pub fn yield_automatic() {
    let job = JOB.lock().ok().and_then(|slot| slot.clone());
    let automatic = job
        .as_ref()
        .and_then(|job| job.status.lock().ok().map(|status| status.automatic))
        .unwrap_or(false);
    if let Some(job) = job.filter(|_| automatic) {
        if let Ok(mut status) = job.status.lock() {
            if status.running {
                job.cancel.store(true, Ordering::SeqCst);
                status.stopping = true;
            }
        }
    }
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
    check_automation(job, library, |revision, library| {
        super::objects_automation::authorized(revision, library).map(|_| ())
    })
}
fn check_automation(
    job: &Job,
    library: &str,
    authorized: impl FnOnce(&str, &str) -> Result<(), String>,
) -> Result<(), String> {
    let revision = {
        let status = job.status.lock().map_err(|error| error.to_string())?;
        status
            .automatic
            .then(|| status.automation_revision.clone().unwrap_or_default())
    };
    if let Some(revision) = revision {
        if authorized(&revision, library).is_err() {
            job.cancel.store(true, Ordering::SeqCst);
            return Err("Automatic object recognition yielded; completed results are saved".into());
        }
    }
    Ok(())
}
fn apply_automation_scope(
    request: &mut super::objects_journal::Request,
    policy: &super::objects_automation::Policy,
) {
    request.folder_id = policy.scope.as_ref().map(|scope| scope.folder_id);
    request.batch_size = Some(policy.batch_size);
}
fn targets(
    c: &Connection,
    profile: &str,
    force: bool,
    retry: bool,
    ids: Option<&[i64]>,
    folder: Option<(i64, String)>,
    limit: Option<usize>,
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
    if let Some(limit) = limit {
        sql.push_str(" LIMIT ?");
        values.push(Box::new(limit as i64));
    }
    let mut statement = c.prepare(&sql).map_err(|e| e.to_string())?;
    statement
        .query_map(params_from_iter(values.iter()), |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
fn persist(c: &Connection, job: &Job) -> Result<(), String> {
    let status = job.status.lock().map_err(|e| e.to_string())?.clone();
    super::objects_journal::save_status(c, &status)
}
#[tauri::command]
pub async fn start_object_index(
    app_handle: AppHandle,
    library_id: String,
    force: bool,
    retry_failed: Option<bool>,
    file_ids: Option<Vec<i64>>,
    folder_id: Option<i64>,
    resume_job_id: Option<String>,
    resume_failed: Option<bool>,
    automation_revision: Option<String>,
) -> Result<Status, String> {
    use super::objects_automation as automation;
    use super::objects_journal as journal;
    let automatic = automation_revision.is_some();
    let request = journal::Request {
        force,
        retry_failed: retry_failed.unwrap_or(false),
        file_ids,
        folder_id,
        ..Default::default()
    };
    request.validate()?;
    if automatic
        && (request.force
            || request.retry_failed
            || request.file_ids.is_some()
            || request.folder_id.is_some()
            || resume_job_id.is_some())
    {
        return Err(
            "Automatic object recognition may only process its configured pending scope".into(),
        );
    }
    let resume_failed = resume_failed.unwrap_or(false);
    if resume_failed && resume_job_id.is_none()
        || resume_job_id.is_some()
            && (request.force
                || request.retry_failed
                || request.file_ids.is_some()
                || request.folder_id.is_some())
    {
        return Err("Choose either a fresh task scope or one saved task to continue".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        media::check_library(&library_id)?;
        let policy = automation_revision.as_ref()
            .map(|revision| automation::authorized(revision, &library_id)).transpose()?;
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
        let mut c = crate::t_sqlite::open_conn()?;
        store::ensure_schema(&c)?;
        store::ensure_model(&c, &model)?;
        let recovery = resume_job_id
            .as_ref()
            .map(|id| journal::recover(&c, id, &library_id, &model.profile(), resume_failed))
            .transpose()?;
        let mut request = recovery
            .as_ref()
            .map(|r| r.request.clone())
            .unwrap_or(request);
        request.model_values = model.values.clone();
        request.model_version = model.definition.version.clone();
        if let Some(policy) = &policy { apply_automation_scope(&mut request, policy); }
        request.validate()?;
        // A completed plan replays exact ledger IDs; a renamed/deleted folder cannot broaden it.
        let replay = recovery.as_ref().and_then(|r| r.ids.clone());
        let folder = if replay.is_none() {
            request.folder_id
                .map(|id| {
                    c.query_row(
                        "SELECT album_id,path FROM afolders WHERE id=?1",
                        [id],
                        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
                    )
                    .map_err(|e| e.to_string())
                })
                .transpose()?
        } else {
            None
        };
        if replay.is_none() {
            if let Some(recovery)=&recovery {
                if recovery.scope=="folder" && folder.as_ref().map(|(_,path)|path.as_str())!=recovery.scope_name.as_deref() {
                    return Err("The saved folder identity changed; start a new explicit scope instead of resuming".into());
                }
                if request.file_ids.as_ref().is_some_and(|ids|ids.iter().any(|id|!request.selected_sources.contains_key(id))) {
                    return Err("This selected task has no early source identity snapshot; start a new selection".into());
                }
            } else if let Some(ids)=&request.file_ids {
                for id in ids {
                    let key=media::source_optional_on(&c,*id)?.map(|source|source.stamp_key()).unwrap_or_else(||Ok(blake3::hash(format!("unavailable-selection:{library_id}:{id}").as_bytes()).to_hex().to_string()))?;
                    request.selected_sources.insert(*id,key);
                }
            }
        }
        let prepared_ids = if automatic {
            let ids = targets(&c, &model.profile(), false, false, None, folder.clone(), request.batch_size)?;
            if ids.is_empty() { return Ok(status(&library_id)); }
            Some(ids)
        } else { None };
        if let Err(error) = super::assets::verify(&model.definition) {
            if automatic {
                let _ = automation::pause_with_error(&error);
                let _ = app_handle.emit("object-automation-changed", ());
            }
            return Err(error);
        }
        let scope = recovery
            .as_ref()
            .map(|r| r.scope.clone())
            .unwrap_or_else(|| {
                if request.file_ids.is_some() {
                    "selected"
                } else if folder.is_some() {
                    "folder"
                } else {
                    "library"
                }
                .into()
            });
        let scope_name = recovery
            .as_ref()
            .and_then(|r| r.scope_name.clone())
            .or_else(|| folder.as_ref().map(|(_, p)| p.clone()));
        let initial = Status {
            library_id: library_id.clone(),
            job_id: uuid::Uuid::new_v4().to_string(),
            running: true,
            phase: "preparing".into(),
            scope,
            scope_name,
            automatic,
            automation_revision: automation_revision.clone(),
            ..Default::default()
        };
        let mut slot = JOB.lock().map_err(|e| e.to_string())?;
        if slot
            .as_ref()
            .is_some_and(|job| job.status.lock().is_ok_and(|s| s.running))
        {
            return Err("An object recognition job is already running".into());
        }
        if let Some(revision) = &automation_revision {
            automation::authorized(revision, &library_id)?;
        }
        journal::begin(
            &mut c,
            &initial,
            &request,
            &model.profile(),
            &model.definition.id,
            &model.definition.name,
            recovery.as_ref(),
        )?;
        let job = Arc::new(Job {
            cancel: AtomicBool::new(false),
            status: Mutex::new(initial.clone()),
        });
        *slot = Some(job.clone());
        drop(slot);
        super::ocr::yield_automatic();
        let _ = app_handle.emit(
            "object-index-updated",
            serde_json::json!({"libraryId":library_id,"started":true}),
        );
        let job_id = initial.job_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut c = {
                    let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
                        .read()
                        .map_err(|e| e.to_string())?;
                    check(&job, &library_id)?;
                    crate::t_sqlite::open_conn()?
                };
                let profile = model.profile();
                let replayed = replay.is_some();
                let ids = match replay {
                    Some(ids) => ids,
                    None => match prepared_ids {
                    Some(ids) => ids,
                    None => targets(
                        &c,
                        &profile,
                        request.force,
                        request.retry_failed,
                        request.file_ids.as_deref(),
                        folder,
                        request.batch_size,
                    )?,
                    },
                };
                if !replayed {
                    journal::plan_sources(
                        &mut c,
                        &job_id,
                        &ids,
                        || check(&job, &library_id),
                        |c, id| {
                            if let Some(key)=request.selected_sources.get(&id){return Ok(key.clone());}
                            media::source_optional_on(c, id)?
                                .map(|source| source.stamp_key())
                                .unwrap_or_else(|| {
                                    Ok(blake3::hash(
                                        format!("unavailable:{job_id}:{id}").as_bytes(),
                                    )
                                    .to_hex()
                                    .to_string())
                                })
                        },
                    )?;
                }
                {
                    let mut status = job.status.lock().map_err(|e| e.to_string())?;
                    status.total = ids.len();
                    status.phase = "waiting".into();
                }
                persist(&c, &job)?;
                if ids.is_empty() {
                    return Ok::<(), String>(());
                }
                // Loading sessions also consumes the shared inference lane, but not for the entire library.
                let mut backend = {
                    let _lane = vision_jobs::claim(Task::Detection, || check(&job, &library_id))?;
                    super::adapters::create_object_backend(&model)?
                };
                job.status.lock().map_err(|e| e.to_string())?.execution =
                    serde_json::json!(super::runtime::reports(&model));
                for id in ids {
                    check(&job, &library_id)?;
                    let source = {
                        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
                            .read()
                            .map_err(|e| e.to_string())?;
                        check(&job, &library_id)?;
                        media::source_optional_on(&c, id)?
                    };
                    let source = match source {
                        Some(source)
                            if source.stamp_key()?
                                == journal::expected_source(&c, &job_id, id)? =>
                        {
                            Some(source)
                        }
                        _ => None,
                    };
                    let Some(source) = source else {
                        journal::mark(&c, &job_id, id, "skipped")?;
                        {
                            let mut status = job.status.lock().map_err(|e| e.to_string())?;
                            status.completed += 1;
                            status.skipped += 1;
                        }
                        persist(&c, &job)?;
                        continue;
                    };
                    job.status.lock().map_err(|e| e.to_string())?.phase = "waiting".into();
                    let _lane = vision_jobs::claim(Task::Detection, || check(&job, &library_id))?;
                    job.status.lock().map_err(|e| e.to_string())?.phase = "indexing".into();
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
                        store::store_for_job(
                            &mut c,
                            id,
                            &profile,
                            source.modified,
                            source.size,
                            &result,
                            &job_id,
                        )?
                    };
                    {
                        let mut status = job.status.lock().map_err(|e| e.to_string())?;
                        status.execution = serde_json::json!(super::runtime::reports(&model));
                        status.completed += 1;
                        if !written {
                            status.skipped += 1;
                        } else if result.is_ok() {
                            status.succeeded += 1;
                        } else {
                            status.failed += 1;
                        }
                    }
                    persist(&c, &job)?;
                    drop(_lane);
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
                        Ok(Err(e)) => Some(e),
                        Err(_) => Some(
                            "Object worker stopped unexpectedly; completed results are saved"
                                .into(),
                        ),
                    }
                };
                status.phase = if status.cancelled {
                    "stopped"
                } else if status.error.is_some() {
                    "failed"
                } else {
                    "finished"
                }
                .into();
            }
            // Only touch the current library. An old task's last item ledger remains recoverable after switching back.
            if let Ok(_guard) = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read() {
                if media::check_library(&library_id).is_ok() {
                    if let Ok(c) = crate::t_sqlite::open_conn() {
                        let _ = persist(&c, &job);
                    }
                }
            }
            let fatal = if job
                .status
                .lock()
                .ok()
                .is_some_and(|status| status.automatic)
            {
                job.status.lock().ok().and_then(|status| status.error.clone())
            } else {
                None
            };
            if let Some(error) = fatal {
                let _ = automation::pause_with_error(&error);
                let _ = app_handle.emit("object-automation-changed", ());
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
    let automatic = status.automatic;
    drop(status);
    if automatic {
        super::objects_automation::pause()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_checks_cancel_changed_policies_but_do_not_authorize_manual_jobs() {
        let job = Job {
            cancel: AtomicBool::new(false),
            status: Mutex::new(Status {
                automatic: true,
                automation_revision: Some("r1".into()),
                ..Default::default()
            }),
        };
        assert!(
            check_automation(&job, "A", |revision, library| {
                assert_eq!(revision, "r1");
                assert_eq!(library, "A");
                Ok(())
            })
            .is_ok()
        );
        assert!(check_automation(&job, "A", |_, _| Err("paused or active".into())).is_err());
        assert!(job.cancel.load(Ordering::SeqCst));
        let manual = Job {
            cancel: AtomicBool::new(false),
            status: Mutex::new(Status::default()),
        };
        assert!(
            check_automation(&manual, "A", |_, _| panic!(
                "manual jobs must not check an idle policy"
            ))
            .is_ok()
        );
    }
    #[test]
    fn automatic_scope_and_batch_limit_are_part_of_the_saved_request() {
        use super::super::objects_automation::{Policy, Scope};
        let policy = Policy {
            batch_size: 7,
            scope: Some(Scope {
                library_id: "A".into(),
                folder_id: 12,
                label: "photos".into(),
            }),
            ..Default::default()
        };
        let mut request = super::super::objects_journal::Request::default();
        apply_automation_scope(&mut request, &policy);
        request.validate().unwrap();
        let restored: super::super::objects_journal::Request =
            serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
        assert_eq!(restored.folder_id, Some(12));
        assert_eq!(restored.batch_size, Some(7));
    }
    #[test]
    fn explicit_scope_and_retry_modes_never_expand_into_other_images() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT,file_types INTEGER,small_image_filter INTEGER,excluded_folders TEXT);CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,path TEXT,is_excluded_from_search INTEGER DEFAULT 0);CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,file_type INTEGER,width INTEGER,height INTEGER,live_photo_video_id INTEGER,modified_at INTEGER,size INTEGER);CREATE TABLE media_states(file_id INTEGER PRIMARY KEY,hidden INTEGER DEFAULT 0,deleted_at INTEGER);INSERT INTO albums VALUES(1,'/photos',7,0,'[]');INSERT INTO afolders VALUES(1,1,'/photos/trip_%',0),(2,1,'/photos/trip_%/child',0),(3,1,'/photos/trip_%2',0);INSERT INTO afiles VALUES(1,1,1,100,100,NULL,10,20),(2,2,1,100,100,NULL,10,20),(3,3,1,100,100,NULL,10,20);").unwrap();
        store::ensure_schema(&c).unwrap();
        assert_eq!(
            targets(&c, "p", false, false, Some(&[2]), None, None).unwrap(),
            vec![2]
        );
        assert_eq!(
            targets(
                &c,
                "p",
                false,
                false,
                None,
                Some((1, "/photos/trip_%".into())),
                None
            )
            .unwrap(),
            vec![1, 2]
        );
        c.execute(
            "INSERT INTO object_index VALUES(1,'p',10,20,NULL,'error',0,0)",
            [],
        )
        .unwrap();
        assert_eq!(
            targets(&c, "p", false, true, None, None, None).unwrap(),
            vec![1]
        );
        assert_eq!(
            targets(&c, "p", false, false, None, None, None).unwrap(),
            vec![2, 3]
        );
        assert_eq!(
            targets(&c, "p", false, false, None, None, Some(1)).unwrap(),
            vec![2]
        );
        assert_eq!(
            targets(&c, "p", true, false, Some(&[1]), None, None).unwrap(),
            vec![1]
        );
    }
}
