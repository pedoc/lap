//! Explicit resumable object indexing. Models are global; observations are library-local.
use super::{media, objects_store as store, types::Task, vision_jobs};
use rusqlite::{Connection, ToSql, params_from_iter};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
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
    pub queued: bool,
    pub priority: String,
    pub queued_jobs: usize,
    pub priority_queued_jobs: usize,
    pub stopping: bool,
    pub cancelled: bool,
    pub total: usize,
    pub prepared: usize,
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
    profile: String,
    registration: Option<vision_jobs::RequestRegistration>,
}
static JOBS: Mutex<VecDeque<Arc<Job>>> = Mutex::new(VecDeque::new());
type Backend = Box<dyn super::capabilities::ObjectDetector>;
static BACKEND: Mutex<Option<(String, u64, Backend)>> = Mutex::new(None);
// Callers hold the detection lane before taking the shared model cache.
fn with_backend<T>(
    model: &super::types::ResolvedModel,
    job: &Job,
    app: &AppHandle,
    run: impl FnOnce(&mut dyn super::capabilities::ObjectDetector) -> Result<T, String>,
) -> Result<T, String> {
    let mut cache = match BACKEND.lock() {
        Ok(cache) => cache,
        Err(error) => {
            let mut cache = error.into_inner();
            *cache = None;
            BACKEND.clear_poison();
            cache
        }
    };
    let key = model.session_key();
    let generation = super::runtime::generation();
    if !cache
        .as_ref()
        .is_some_and(|(loaded, version, _)| loaded == &key && *version == generation)
    {
        *cache = None;
        job.status.lock().map_err(|e| e.to_string())?.phase = "loading".into();
        publish_progress(app, job);
        *cache = Some((
            key,
            generation,
            super::adapters::create_object_backend(model)?,
        ));
    }
    job.status.lock().map_err(|e| e.to_string())?.phase = "indexing".into();
    publish_progress(app, job);
    run(cache
        .as_mut()
        .ok_or("Object backend is unavailable")?
        .2
        .as_mut())
}
fn release_backend_if_idle() {
    if let Ok(mut cache) = BACKEND.lock() {
        if !is_running() {
            *cache = None;
        }
    }
}
pub fn is_running() -> bool {
    JOBS.lock()
        .map(|jobs| {
            jobs.iter()
                .any(|job| job.status.lock().is_ok_and(|status| status.running))
        })
        .unwrap_or(true)
}
pub fn yield_automatic() {
    let jobs = JOBS
        .lock()
        .map(|jobs| jobs.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    for job in jobs {
        if let Ok(mut status) = job.status.lock() {
            if status.running && status.automatic {
                job.cancel.store(true, Ordering::SeqCst);
                status.stopping = true;
            }
        }
    }
}
pub fn cancel_all(library: &str) {
    if let Ok(jobs) = JOBS.lock() {
        for job in jobs.iter() {
            if let Ok(mut status) = job.status.lock() {
                if status.library_id == library && status.running {
                    job.cancel.store(true, Ordering::SeqCst);
                    status.stopping = true;
                }
            }
        }
    }
}
pub fn status(library: &str) -> Status {
    let active = vision_jobs::active_or_next_request(Task::Detection);
    project_statuses(job_statuses(library), active.as_deref()).unwrap_or_else(|| Status {
        library_id: library.into(),
        ..Default::default()
    })
}
fn job_statuses(library: &str) -> Vec<Status> {
    JOBS.lock()
        .map(|jobs| {
            jobs.iter()
                .filter_map(|job| {
                    job.status
                        .lock()
                        .ok()
                        .filter(|s| s.library_id == library)
                        .map(|s| s.clone())
                })
                .collect()
        })
        .unwrap_or_default()
}
fn project_statuses(mut rows: Vec<Status>, active: Option<&str>) -> Option<Status> {
    let owner = active.map(str::to_owned).or_else(|| {
        rows.iter()
            .find(|s| s.running && s.priority == "interactive")
            .or_else(|| rows.iter().find(|s| s.running))
            .map(|s| s.job_id.clone())
    });
    let active = owner.as_deref();
    let queued = rows
        .iter()
        .filter(|s| s.running && Some(s.job_id.as_str()) != active)
        .count();
    let priority_queued = rows
        .iter()
        .filter(|s| s.running && s.priority == "interactive" && Some(s.job_id.as_str()) != active)
        .count();
    for status in &mut rows {
        status.queued = status.running && Some(status.job_id.as_str()) != active;
        status.queued_jobs = queued;
        status.priority_queued_jobs = priority_queued;
    }
    rows.iter()
        .find(|s| s.running && Some(s.job_id.as_str()) == active)
        .or_else(|| {
            rows.iter()
                .find(|s| s.running && s.priority == "interactive")
        })
        .or_else(|| rows.iter().find(|s| s.running))
        .or_else(|| rows.last())
        .cloned()
}
fn receipt(library: &str, id: &str) -> Result<Status, String> {
    let active = vision_jobs::active_or_next_request(Task::Detection);
    let rows = job_statuses(library);
    project_receipt(rows, active.as_deref(), id)
}
fn project_receipt(rows: Vec<Status>, active: Option<&str>, id: &str) -> Result<Status, String> {
    let projection = project_statuses(rows.clone(), active);
    let mut row = rows
        .into_iter()
        .find(|s| s.job_id == id)
        .ok_or("Object job is unavailable")?;
    row.queued = row.running
        && !projection
            .as_ref()
            .is_some_and(|status| !status.queued && status.job_id == id);
    if let Some(status) = projection {
        row.queued_jobs = status.queued_jobs;
        row.priority_queued_jobs = status.priority_queued_jobs;
    }
    Ok(row)
}
pub(crate) fn registered_status(library: &str, id: &str) -> Option<Status> {
    receipt(library, id).ok()
}
fn finish_job(job: &Arc<Job>) {
    if let Some(registration) = &job.registration {
        registration.release();
    }
    if let Ok(mut jobs) = JOBS.lock() {
        if let Some(index) = jobs.iter().position(|item| Arc::ptr_eq(item, job)) {
            if let Some(job) = jobs.remove(index) {
                jobs.push_back(job);
            }
        }
        let mut finished = jobs
            .iter()
            .filter(|job| job.status.lock().is_ok_and(|s| !s.running))
            .count();
        jobs.retain(|job| {
            if finished > 32 && job.status.lock().is_ok_and(|s| !s.running) {
                finished -= 1;
                false
            } else {
                true
            }
        });
    }
}
fn check(job: &Job, library: &str) -> Result<(), String> {
    media::check_library(library)?;
    if job.cancel.load(Ordering::SeqCst) {
        return Err("Object recognition stopped; completed results are saved".into());
    }
    if !job.profile.is_empty() && super::settings::active(Task::Detection)?.profile() != job.profile
    {
        return Err(
            "Object model changed while the request was queued; retry with the current model"
                .into(),
        );
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
    let mut sql = format!(
        "SELECT a.id {} AND {}",
        media::base(),
        media::local_originals()
    );
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
pub(crate) fn has_local_pending(
    c: &Connection,
    profile: &str,
    folder_id: Option<i64>,
) -> Result<bool, String> {
    let folder = folder_id
        .map(|id| {
            c.query_row(
                "SELECT album_id,path FROM afolders WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| format!("Object automation folder is unavailable: {e}"))
        })
        .transpose()?;
    Ok(!targets(c, profile, false, false, None, folder, Some(1))?.is_empty())
}
fn persist(c: &Connection, job: &Job) -> Result<(), String> {
    let status = job.status.lock().map_err(|e| e.to_string())?.clone();
    super::objects_journal::save_status(c, &status)
}
fn publish_progress(app: &AppHandle, job: &Job) {
    let library = job
        .status
        .lock()
        .ok()
        .map(|status| status.library_id.clone());
    if let Some(library) = library {
        let _ = app.emit("object-job-progress", status(&library));
    }
}

fn prepare_sources(
    c: &mut Connection,
    job: &Job,
    ids: &[i64],
    selected: &std::collections::BTreeMap<i64, String>,
    check: impl Fn() -> Result<(), String>,
    mut notify: impl FnMut(&Status),
) -> Result<(), String> {
    let job_id = {
        let mut status = job.status.lock().map_err(|e| e.to_string())?;
        status.total = ids.len();
        status.prepared = 0;
        status.phase = "preparing".into();
        status.job_id.clone()
    };
    let mut last_emit: Option<std::time::Instant> = None;
    super::objects_journal::plan_sources(
        c,
        &job_id,
        ids,
        check,
        |c, id| {
            if let Some(key) = selected.get(&id) {
                return Ok(key.clone());
            }
            media::source_optional_on(c, id)?
                .map(|source| source.stamp_key())
                .unwrap_or_else(|| {
                    Ok(
                        blake3::hash(format!("unavailable:{job_id}:{id}").as_bytes())
                            .to_hex()
                            .to_string(),
                    )
                })
        },
        |prepared| {
            let snapshot = {
                let mut status = job.status.lock().map_err(|e| e.to_string())?;
                status.prepared = prepared;
                status.clone()
            };
            if prepared == ids.len()
                || last_emit
                    .is_none_or(|last| last.elapsed() >= std::time::Duration::from_millis(150))
            {
                notify(&snapshot);
                last_emit = Some(std::time::Instant::now());
            }
            Ok(())
        },
    )
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
    vision_jobs::ensure_admission()?;
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
        vision_jobs::ensure_admission()?;
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        media::check_library(&library_id)?;
        let policy = automation_revision.as_ref()
            .map(|revision| automation::authorized(revision, &library_id)).transpose()?;
        if !app_handle
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
                "Wait for library or similarity indexing before detecting objects".into(),
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
                    vision_jobs::ensure_admission()?;
                    media::check_library(&library_id)?;
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
        let interactive = !automatic && request.file_ids.is_some();
        let priority = if interactive { vision_jobs::Priority::Interactive } else { vision_jobs::Priority::Normal };
        let initial = Status {
            library_id: library_id.clone(),
            job_id: uuid::Uuid::new_v4().to_string(),
            running: true,
            queued: true,
            priority: if interactive { "interactive" } else { "normal" }.into(),
            phase: "selecting".into(),
            scope,
            scope_name,
            automatic,
            automation_revision: automation_revision.clone(),
            ..Default::default()
        };
        let mut jobs = JOBS.lock().map_err(|e| e.to_string())?;
        let active = jobs.iter().filter_map(|j|j.status.lock().ok().map(|s|s.clone())).filter(|s|s.running).collect::<Vec<_>>();
        if active.len() >= 32 { return Err("Too many queued object requests; wait for a task to finish".into()); }
        if !interactive && active.iter().any(|s|s.library_id == library_id && s.priority == "normal") { return Err("An object background task is already queued or running".into()); }
        if let Some(revision) = &automation_revision {
            automation::authorized(revision, &library_id)?;
        }
        let registration = vision_jobs::register_request(Task::Detection,priority,&initial.job_id)?;
        let job = Arc::new(Job {
            cancel: AtomicBool::new(false),
            status: Mutex::new(initial.clone()),
            profile: model.profile(),
            registration: Some(registration),
        });
        jobs.push_back(job.clone());
        drop(jobs);
        // Reserve admission first, but never wait on SQLite while holding the job registry.
        if let Err(error) = journal::begin(
            &mut c,
            &initial,
            &request,
            &model.profile(),
            &model.definition.id,
            &model.definition.name,
            recovery.as_ref(),
        ) {
            {
                let mut status = job.status.lock().unwrap_or_else(|error|error.into_inner());
                status.running = false;
                status.queued = false;
                status.phase = "failed".into();
                status.error = Some(error.clone());
            }
            job.status.clear_poison();
            finish_job(&job);
            publish_progress(&app_handle,&job);
            return Err(error);
        }
        let initial = receipt(&library_id, &initial.job_id)?;
        publish_progress(&app_handle,&job);
        let _ = app_handle.emit(
            "object-index-updated",
            serde_json::json!({"libraryId":library_id,"started":true}),
        );
        let job_id = initial.job_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut c = {
                    vision_jobs::ensure_admission()?;
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
                check(&job, &library_id)?;
                if !replayed {
                    prepare_sources(
                        &mut c,
                        &job,
                        &ids,
                        &request.selected_sources,
                        || check(&job, &library_id),
                        |_| {
                            publish_progress(&app_handle,&job);
                        },
                    )?;
                }
                {
                    let mut status = job.status.lock().map_err(|e| e.to_string())?;
                    status.total = ids.len();
                    status.prepared = ids.len();
                    status.phase = "waiting".into();
                }
                publish_progress(&app_handle, &job);
                persist(&c, &job)?;
                if ids.is_empty() {
                    return Ok::<(), String>(());
                }
                job.status.lock().map_err(|e| e.to_string())?.execution =
                    serde_json::json!(super::runtime::reports(&model));
                for id in ids {
                    check(&job, &library_id)?;
                    job.status.lock().map_err(|e| e.to_string())?.phase = "waiting".into();
                    let _lane = vision_jobs::claim_job(Task::Detection, priority, &job_id, || check(&job, &library_id))?;
                    if !request.force && targets(&c,&profile,false,request.retry_failed,Some(&[id]),None,None)?.is_empty() {
                        journal::mark(&c,&job_id,id,"skipped")?;
                        {
                            let mut status = job.status.lock().map_err(|e|e.to_string())?;
                            status.completed += 1;
                            status.skipped += 1;
                        }
                        persist(&c,&job)?;
                        continue;
                    }
                    let source = {
                        vision_jobs::ensure_admission()?;
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
                    job.status.lock().map_err(|e| e.to_string())?.phase = "indexing".into();
                    publish_progress(&app_handle, &job);
                    let mut result = source.load().and_then(|image| {
                        with_backend(&model, &job, &app_handle, |backend| backend.detect_objects(&image, &|| check(&job, &library_id)))
                    });
                    if let Err(error) = source.verify() {
                        result = Err(error);
                    }
                    check(&job, &library_id)?;
                    if result.is_err() && crate::t_cmds::with_current_library(&library_id, || {
                        media::defer_cloud_original(&c, id, &source.path)
                    })? {
                        journal::mark(&c, &job_id, id, "skipped")?;
                        let mut status = job.status.lock().map_err(|e| e.to_string())?;
                        status.completed += 1;
                        status.skipped += 1;
                        drop(status);
                        persist(&c, &job)?;
                        publish_progress(&app_handle, &job);
                        continue;
                    }
                    let written = {
                        vision_jobs::ensure_admission()?;
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
            {
                let mut status = job.status.lock().unwrap_or_else(|error|error.into_inner());
                status.running = false;
                status.queued = false;
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
            job.status.clear_poison();
            finish_job(&job);
            release_backend_if_idle();
            publish_progress(&app_handle, &job);
            // Only touch the current library. An old task's last item ledger remains recoverable after switching back.
            if let Ok(_guard) = vision_jobs::ensure_admission().and_then(|_|crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read().map_err(|error|error.to_string())) {
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
    let job = JOBS
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|job| {
            job.status
                .lock()
                .is_ok_and(|s| s.library_id == library_id && s.job_id == job_id)
        })
        .cloned()
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
    fn idle_receipts_start_immediately_and_only_same_type_owners_queue_them() {
        let selected = Status {
            job_id: "selected".into(),
            running: true,
            queued: true,
            priority: "interactive".into(),
            ..Default::default()
        };
        // An idle detection lane is unaffected by active OCR or face work.
        let idle = project_receipt(vec![selected.clone()], None, "selected").unwrap();
        assert!(!idle.queued);
        assert_eq!((idle.queued_jobs, idle.priority_queued_jobs), (0, 0));
        let unknown = project_receipt(vec![selected.clone()], Some(""), "selected").unwrap();
        assert!(unknown.queued);
        assert_eq!((unknown.queued_jobs, unknown.priority_queued_jobs), (1, 1));
        assert!(
            project_receipt(
                vec![selected.clone()],
                Some("earlier-preparing-request"),
                "selected"
            )
            .unwrap()
            .queued
        );
        let background = Status {
            job_id: "background".into(),
            running: true,
            priority: "normal".into(),
            ..Default::default()
        };
        let queued =
            project_receipt(vec![background, selected], Some("background"), "selected").unwrap();
        assert!(queued.queued);
        assert_eq!((queued.queued_jobs, queued.priority_queued_jobs), (1, 1));
    }
    #[test]
    fn projection_keeps_active_progress_and_restores_the_background_after_selected_jobs_finish() {
        let rows = vec![
            Status {
                job_id: "background".into(),
                running: true,
                priority: "normal".into(),
                completed: 7,
                ..Default::default()
            },
            Status {
                job_id: "selected-1".into(),
                running: true,
                priority: "interactive".into(),
                ..Default::default()
            },
            Status {
                job_id: "selected-2".into(),
                running: true,
                priority: "interactive".into(),
                ..Default::default()
            },
        ];
        let current = project_statuses(rows.clone(), Some("background")).unwrap();
        assert_eq!(current.job_id, "background");
        assert_eq!(current.completed, 7);
        assert!(!current.queued);
        assert_eq!((current.queued_jobs, current.priority_queued_jobs), (2, 2));
        assert_eq!(
            project_statuses(rows.clone(), None).unwrap().job_id,
            "selected-1"
        );
        let current = project_statuses(rows.clone(), Some("selected-2")).unwrap();
        assert_eq!(current.job_id, "selected-2");
        assert_eq!((current.queued_jobs, current.priority_queued_jobs), (2, 1));
        let mut restored = rows;
        restored[1].running = false;
        restored[2].running = false;
        assert_eq!(
            project_statuses(restored.clone(), None).unwrap().job_id,
            "background"
        );
        restored[0].running = false;
        let done = project_statuses(restored, None).unwrap();
        assert_eq!(done.job_id, "selected-2");
        assert!(!done.queued);
        assert_eq!(done.queued_jobs, 0);
    }
    #[test]
    fn background_snapshot_rechecks_successful_selected_work_without_expanding_its_plan() {
        let c = media::tests::db(2);
        store::ensure_schema(&c).unwrap();
        assert_eq!(
            targets(&c, "p", false, false, Some(&[1]), None, None).unwrap(),
            vec![1]
        );
        c.execute(
            "INSERT INTO object_index VALUES(1,'p',10,20,'{}',NULL,0,1)",
            [],
        )
        .unwrap();
        assert!(
            targets(&c, "p", false, false, Some(&[1]), None, None)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            targets(&c, "p", true, false, Some(&[1]), None, None).unwrap(),
            vec![1]
        );
        c.execute("UPDATE afiles SET modified_at=11 WHERE id=1", [])
            .unwrap();
        assert_eq!(
            targets(&c, "p", false, false, Some(&[1]), None, None).unwrap(),
            vec![1]
        );
    }
    #[test]
    fn large_library_preparation_registers_the_real_plan_and_reports_preparation_separately() {
        let mut c = media::tests::db(11470);
        store::ensure_schema(&c).unwrap();
        let initial = Status {
            library_id: "fixture".into(),
            job_id: uuid::Uuid::new_v4().to_string(),
            running: true,
            phase: "selecting".into(),
            scope: "library".into(),
            ..Default::default()
        };
        super::super::objects_journal::begin(
            &mut c,
            &initial,
            &Default::default(),
            "fixture-profile",
            "fixture-model",
            "Fixture",
            None,
        )
        .unwrap();
        let ids = targets(&c, "fixture-profile", false, false, None, None, None).unwrap();
        assert_eq!(ids.len(), 11470);
        let job = Job {
            registration: None,
            profile: String::new(),
            cancel: AtomicBool::new(false),
            status: Mutex::new(initial),
        };
        let mut observed = Vec::new();
        let started = std::time::Instant::now();
        prepare_sources(
            &mut c,
            &job,
            &ids,
            &Default::default(),
            || Ok(()),
            |status| {
                assert!(
                    job.status.try_lock().is_ok(),
                    "notifications must not hold the task mutex"
                );
                assert_eq!(status.phase, "preparing");
                assert_eq!(
                    status.completed, 0,
                    "preparing a source is not successful recognition"
                );
                observed.push(status.prepared);
            },
        )
        .unwrap();
        assert_eq!(observed.first(), Some(&0));
        assert_eq!(observed.last(), Some(&ids.len()));
        assert!(observed.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM object_job_items WHERE length(source_key)=64",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            11470
        );
        assert!(
            c.query_row("SELECT planned FROM object_jobs", [], |r| r
                .get::<_, bool>(0))
                .unwrap()
        );
        eprintln!(
            "Prepared {} real task ledger items in {:?}",
            ids.len(),
            started.elapsed()
        );
    }

    #[test]
    fn automatic_checks_cancel_changed_policies_but_do_not_authorize_manual_jobs() {
        let job = Job {
            registration: None,
            profile: String::new(),
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
            registration: None,
            profile: String::new(),
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
        c.execute_batch("ALTER TABLE afiles ADD COLUMN cloud_only INTEGER NOT NULL DEFAULT 0; ALTER TABLE afiles ADD COLUMN metadata_pending INTEGER NOT NULL DEFAULT 0;").unwrap();
        c.execute_batch("UPDATE afiles SET cloud_only=1 WHERE id=1; UPDATE afiles SET metadata_pending=1 WHERE id=2;").unwrap();
        assert_eq!(
            targets(&c, "p", true, false, None, None, None).unwrap(),
            vec![3]
        );
        assert!(!has_local_pending(&c, "p", Some(1)).unwrap());
        assert!(has_local_pending(&c, "p", Some(3)).unwrap());
        assert_eq!(
            c.query_row(
                &format!("SELECT COUNT(*) {} AND a.id=1", media::base()),
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1,
            "cached observations stay browsable"
        );
        c.execute_batch("UPDATE afiles SET cloud_only=0,metadata_pending=0;")
            .unwrap();
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
