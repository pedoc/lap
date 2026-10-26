use crate::ai::{
    types::{ResolvedModel, Task},
    vision_jobs::{self, Priority},
};
/**
 * Face Recognition module
 * Handles face detection (RetinaFace) and embedding (MobileFaceNet) using ONNX Runtime.
 */
use crate::{t_cluster, t_sqlite};
use image::DynamicImage;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{AppHandle, Emitter};

// cancellation token for face indexing
#[derive(Clone)]
pub struct FaceIndexCancellation(pub Arc<Mutex<bool>>);

// detailed status for face indexing
#[derive(Clone)]
pub struct FaceIndexingStatus(pub Arc<Mutex<bool>>);

// face indexing progress
#[derive(Clone, Debug, serde::Serialize)]
pub struct FaceIndexProgress {
    pub current: usize,
    pub total: usize,
    pub faces_found: usize,
    pub phase: String,
    pub library_id: String,
    pub scope: String,
    pub job_id: String,
    pub queued: bool,
    pub priority: String,
    pub queued_jobs: usize,
    pub priority_queued_jobs: usize,
}
impl Default for FaceIndexProgress {
    fn default() -> Self {
        Self {
            current: 0,
            total: 0,
            faces_found: 0,
            phase: "idle".into(),
            library_id: String::new(),
            scope: "library".into(),
            job_id: String::new(),
            queued: false,
            priority: "normal".into(),
            queued_jobs: 0,
            priority_queued_jobs: 0,
        }
    }
}

#[derive(Clone)]
pub struct FaceIndexProgressState(pub Arc<Mutex<FaceIndexProgress>>);

struct FaceJob {
    id: String,
    library_id: String,
    model: ResolvedModel,
    scope: crate::ai::face_jobs::FaceScope,
    priority: Priority,
    registration: vision_jobs::RequestRegistration,
    cancel: Arc<AtomicBool>,
    finished: AtomicBool,
    progress: Mutex<FaceIndexProgress>,
}
impl FaceJob {
    fn check(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::Acquire) {
            return Err("Face indexing cancelled".into());
        }
        vision_jobs::ensure_admission()?;
        if crate::t_config::current_library_id()? != self.library_id {
            return Err("Library changed; face results were discarded".into());
        }
        let model = crate::ai::settings::active(Task::Face)?;
        self.check_model(&model)
    }
    fn check_model(&self, model: &ResolvedModel) -> Result<(), String> {
        if model.session_key() != self.model.session_key() {
            return Err("Face model or configuration changed; face results were discarded".into());
        }
        Ok(())
    }
    fn finish(&self) {
        self.finished.store(true, Ordering::Release);
        self.registration.release();
    }
}
struct RunningGuard(Arc<FaceJob>);
impl Drop for RunningGuard {
    fn drop(&mut self) {
        let unfinished = !self.0.finished.swap(true, Ordering::AcqRel);
        self.0.registration.release();
        if unfinished {
            self.0
                .progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .phase = "failed".into();
            project_jobs();
        }
    }
}
#[derive(Default)]
struct FaceJobs {
    entries: VecDeque<Arc<FaceJob>>,
    mirrors: Option<(FaceIndexingStatus, FaceIndexProgressState)>,
}
impl FaceJobs {
    fn register(
        &mut self,
        library_id: String,
        model: ResolvedModel,
        scope: crate::ai::face_jobs::FaceScope,
    ) -> Result<Arc<FaceJob>, String> {
        vision_jobs::ensure_admission()?;
        let priority = if scope.file_ids.is_some() {
            Priority::Interactive
        } else {
            Priority::Normal
        };
        let live = self
            .entries
            .iter()
            .filter(|job| !job.finished.load(Ordering::Acquire))
            .collect::<Vec<_>>();
        if live.len() >= 32 {
            return Err("The face queue is full; wait for a queued job to finish".into());
        }
        if priority == Priority::Normal
            && live
                .iter()
                .any(|job| job.library_id == library_id && job.priority == Priority::Normal)
        {
            return Err("Face library indexing is already running or queued".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let registration = vision_jobs::register_request(Task::Face, priority, &id)?;
        let progress = FaceIndexProgress {
            library_id: library_id.clone(),
            scope: scope.key().into(),
            job_id: id.clone(),
            phase: "waiting".into(),
            queued: true,
            priority: priority.key().into(),
            ..Default::default()
        };
        let job = Arc::new(FaceJob {
            id,
            library_id,
            model,
            scope,
            priority,
            registration,
            cancel: Arc::new(AtomicBool::new(false)),
            finished: AtomicBool::new(false),
            progress: Mutex::new(progress),
        });
        self.entries.push_back(job.clone());
        Ok(job)
    }
    fn selected(&self, active: Option<&str>) -> Option<&Arc<FaceJob>> {
        let live = self
            .entries
            .iter()
            .filter(|job| !job.finished.load(Ordering::Acquire));
        live.clone()
            .find(|job| Some(job.id.as_str()) == active)
            .or_else(|| {
                live.clone()
                    .find(|job| job.priority == Priority::Interactive)
            })
            .or_else(|| live.clone().next())
    }
    fn progress_for(&self, job: &FaceJob, active: Option<&str>) -> FaceIndexProgress {
        let active = active.or_else(|| self.selected(None).map(|job| job.id.as_str()));
        let mut progress = job
            .progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let live = self
            .entries
            .iter()
            .filter(|entry| !entry.finished.load(Ordering::Acquire));
        progress.queued_jobs = live
            .clone()
            .filter(|entry| Some(entry.id.as_str()) != active)
            .count();
        progress.priority_queued_jobs = live
            .filter(|entry| {
                Some(entry.id.as_str()) != active && entry.priority == Priority::Interactive
            })
            .count();
        progress.queued = !job.finished.load(Ordering::Acquire) && Some(job.id.as_str()) != active;
        if progress.queued {
            progress.phase = "waiting".into();
        } else if progress.phase == "waiting" {
            progress.phase = "loading".into();
        }
        progress
    }
    fn snapshot(&self, active: Option<&str>) -> (bool, Option<FaceIndexProgress>) {
        let selected = self.selected(active);
        (
            selected.is_some(),
            selected.map(|job| self.progress_for(job, active)),
        )
    }
    fn cancel(&self, job_id: Option<&str>, library_id: Option<&str>) -> Result<(), String> {
        if let Some(id) = job_id {
            let job = self
                .entries
                .iter()
                .find(|job| job.id == id)
                .ok_or("Face job was not found")?;
            if library_id.is_some_and(|library| library != job.library_id) {
                return Err("Face job belongs to another library".into());
            }
            job.cancel.store(true, Ordering::Release);
        } else {
            for job in self
                .entries
                .iter()
                .filter(|job| library_id.is_none_or(|library| library == job.library_id))
            {
                job.cancel.store(true, Ordering::Release);
            }
        }
        Ok(())
    }
    fn project(&mut self, active: Option<&str>) {
        let mut remove = self
            .entries
            .iter()
            .filter(|job| job.finished.load(Ordering::Acquire))
            .count()
            .saturating_sub(16);
        self.entries.retain(|job| {
            if remove > 0 && job.finished.load(Ordering::Acquire) {
                remove -= 1;
                false
            } else {
                true
            }
        });
        let (running, progress) = self.snapshot(active);
        if let Some((status, mirror)) = &self.mirrors {
            *status.0.lock().unwrap_or_else(|e| e.into_inner()) = running;
            if let Some(progress) = progress {
                *mirror.0.lock().unwrap_or_else(|e| e.into_inner()) = progress;
            }
        }
    }
}
static FACE_JOBS: LazyLock<Mutex<FaceJobs>> = LazyLock::new(|| Mutex::new(FaceJobs::default()));
fn face_lane_owner() -> Option<String> {
    vision_jobs::active_or_next_request(Task::Face)
}
pub fn indexing_snapshot() -> Result<(bool, Option<FaceIndexProgress>), String> {
    let active = face_lane_owner();
    Ok(FACE_JOBS
        .lock()
        .map_err(|e| e.to_string())?
        .snapshot(active.as_deref()))
}
pub fn cancel_jobs(job_id: Option<&str>, library_id: Option<&str>) -> Result<(), String> {
    FACE_JOBS
        .lock()
        .map_err(|e| e.to_string())?
        .cancel(job_id, library_id)
}
pub fn cancel_all(library: &str) {
    let _ = cancel_jobs(None, Some(library));
}
fn project_jobs() {
    let active = face_lane_owner();
    FACE_JOBS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .project(active.as_deref());
}
fn update_progress(job: &FaceJob, update: impl FnOnce(&mut FaceIndexProgress)) {
    update(&mut job.progress.lock().unwrap_or_else(|e| e.into_inner()));
    project_jobs();
}
fn emit_job(app: &AppHandle, job: &FaceJob, event: &str, mut payload: serde_json::Value) {
    let active = face_lane_owner();
    let progress = FACE_JOBS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .progress_for(job, active.as_deref());
    payload["library_id"] = serde_json::json!(job.library_id);
    payload["scope"] = serde_json::json!(job.scope.key());
    payload["job_id"] = serde_json::json!(job.id);
    payload["priority"] = serde_json::json!(progress.priority);
    payload["queued"] = serde_json::json!(progress.queued);
    payload["queued_jobs"] = serde_json::json!(progress.queued_jobs);
    payload["priority_queued_jobs"] = serde_json::json!(progress.priority_queued_jobs);
    if let Some(diagnostics) = payload
        .get_mut("diagnostics")
        .filter(|value| value.is_object())
    {
        diagnostics["job_id"] = serde_json::json!(job.id);
    }
    let _ = app.emit(event, payload);
}
fn emit_progress(app: &AppHandle, job: &FaceJob, failed: usize, cached: usize) {
    let mut payload = serde_json::to_value(
        job.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone(),
    )
    .unwrap_or_default();
    payload["failed"] = serde_json::json!(failed);
    payload["cached"] = serde_json::json!(cached);
    emit_job(app, job, "face_index_progress", payload);
}

// face stats
#[derive(Clone, serde::Serialize)]
pub struct FaceStats {
    pub total: usize,
    pub processed: usize,
    pub unprocessed: usize,
    pub faces: usize,
}

/// Detected face bounding box and landmarks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub confidence: f32,
    pub landmarks: Option<Vec<(f32, f32)>>, // 5 facial landmarks
}

/// Face with embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceData {
    pub bbox: FaceBox,
    pub embedding: Vec<f32>,
}

pub struct FaceEngine {
    model: Option<crate::ai::types::ResolvedModel>,
    backend: Option<Box<dyn crate::ai::capabilities::FacePipeline>>,
    runtime_generation: u64,
}
impl FaceEngine {
    pub fn new() -> Self {
        Self {
            model: None,
            backend: None,
            runtime_generation: crate::ai::runtime::generation(),
        }
    }
    pub fn load_models(&mut self, _app: &AppHandle) -> Result<(), String> {
        let model = crate::ai::settings::active(crate::ai::types::Task::Face)?;
        crate::ai::profiles::ensure(crate::ai::types::Task::Face, &model.profile())?;
        self.ensure_model(&model)
    }
    fn ensure_model(&mut self, model: &ResolvedModel) -> Result<(), String> {
        if self.runtime_generation == crate::ai::runtime::generation()
            && self.backend.is_some()
            && self
                .model
                .as_ref()
                .is_some_and(|loaded| loaded.session_key() == model.session_key())
        {
            return Ok(());
        }
        self.load_model(model.clone())
    }
    pub fn load_model(&mut self, model: crate::ai::types::ResolvedModel) -> Result<(), String> {
        let _lane = crate::ai::vision_jobs::claim(crate::ai::types::Task::Face, || Ok(()))?;
        let backend = crate::ai::adapters::face_backend(&model)?;
        self.backend = Some(backend);
        self.model = Some(model);
        self.runtime_generation = crate::ai::runtime::generation();
        Ok(())
    }
    pub fn is_loaded(&self) -> bool {
        self.runtime_generation == crate::ai::runtime::generation()
            && self.backend.is_some()
            && self
                .model
                .as_ref()
                .zip(
                    crate::ai::settings::active(crate::ai::types::Task::Face)
                        .ok()
                        .as_ref(),
                )
                .is_some_and(|(loaded, active)| loaded.session_key() == active.session_key())
    }
    fn process(&mut self, image: &DynamicImage) -> Result<(Vec<FaceData>, (u32, u32)), String> {
        let _lane = crate::ai::vision_jobs::claim(crate::ai::types::Task::Face, || Ok(()))?;
        for attempt in 0..3 {
            let result = self
                .backend
                .as_mut()
                .ok_or("No face model loaded")?
                .process(image);
            if let Err(ref error) = result {
                let model = self.model.clone().ok_or("No face model loaded")?;
                let role = self
                    .backend
                    .as_ref()
                    .and_then(|b| b.diagnostics())
                    .map(|r| r.stage);
                let role = role
                    .as_deref()
                    .filter(|role| matches!(*role, "detector" | "embedding"));
                if attempt < 2 && crate::ai::runtime::recover_acceleration(&model, role, error) {
                    self.backend = None;
                    self.load_model(model)?;
                    continue;
                }
            }
            return result;
        }
        Err("Face backend retries exhausted".into())
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn process_image(&mut self, path: &str) -> Result<(Vec<FaceData>, (u32, u32)), String> {
        if crate::t_utils::is_cloud_only_path(path) {
            return Err("Original is online-only; download it locally before face analysis".into());
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        self.process_image_from_bytes(&bytes)
    }
    pub fn process_image_reported(
        &mut self,
        bytes: &[u8],
    ) -> (
        Result<(Vec<FaceData>, (u32, u32)), String>,
        crate::ai::face_diagnostics::PipelineReport,
    ) {
        let image = match crate::ai::face_jobs::decode_image(bytes) {
            Ok(image) => image,
            Err(error) => {
                return (
                    Err(error),
                    crate::ai::face_diagnostics::PipelineReport {
                        stage: "decode_image".into(),
                        ..Default::default()
                    },
                );
            }
        };
        let result = self.process(&image); // Keep the existing auto-provider retry behavior unchanged.
        let report = self
            .backend
            .as_ref()
            .and_then(|backend| backend.diagnostics())
            .unwrap_or(crate::ai::face_diagnostics::PipelineReport {
                stage: "inference".into(),
                decoded_size: Some([image.width(), image.height()]),
                ..Default::default()
            });
        (result, report)
    }
    pub fn process_image_from_bytes(
        &mut self,
        bytes: &[u8],
    ) -> Result<(Vec<FaceData>, (u32, u32)), String> {
        let image = crate::ai::face_jobs::decode_image(bytes)?;
        self.process(&image)
    }
}

#[derive(Clone)]
pub struct FaceState(pub std::sync::Arc<Mutex<FaceEngine>>);

pub fn run_face_indexing(
    app_handle: AppHandle,
    face_state: FaceState,
    _cancel_token_struct: FaceIndexCancellation,
    status_token_struct: FaceIndexingStatus,
    progress_token_struct: FaceIndexProgressState,
    cluster_epsilon: Option<f32>,
    scope: crate::ai::face_jobs::FaceScope,
) -> Result<FaceIndexProgress, String> {
    vision_jobs::ensure_admission()?;
    let model = crate::ai::settings::active(Task::Face)?;
    let epsilon = cluster_epsilon.unwrap_or(model.number("cluster_distance") as f32);
    let library_id = crate::t_config::current_library_id()?;
    let (job, receipt) = {
        let mut jobs = FACE_JOBS.lock().map_err(|e| e.to_string())?;
        let job = jobs.register(library_id, model, scope)?;
        jobs.mirrors = Some((status_token_struct, progress_token_struct));
        let active = face_lane_owner();
        jobs.project(active.as_deref());
        let receipt = jobs.progress_for(&job, active.as_deref());
        (job, receipt)
    };
    emit_progress(&app_handle, &job, 0, 0);
    tauri::async_runtime::spawn_blocking(move || {
        let _running = RunningGuard(job.clone());
        let mut diagnostics = crate::ai::face_diagnostics::JobReport::new(&job.model);
        let mut totals = FaceTotals::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_face_job(
                &app_handle,
                &face_state,
                &job,
                epsilon,
                &mut diagnostics,
                &mut totals,
            )
        }))
        .unwrap_or_else(|_| {
            Err("Face indexing worker failed; previous committed results were retained".into())
        });
        let error = result.err();
        let cancelled = job.check().is_err();
        if totals.current == 0 {
            diagnostics.startup_error = error.clone();
        }
        diagnostics.update_devices(&job.model);
        update_progress(&job, |progress| {
            progress.phase = if cancelled {
                "cancelled"
            } else if error.is_some() {
                "failed"
            } else {
                "done"
            }
            .into();
            progress.queued = false;
        });
        job.finish();
        project_jobs();
        emit_job(
            &app_handle,
            &job,
            "face_index_finished",
            serde_json::json!({
                "total_faces": totals.faces, "total_persons": totals.persons, "cancelled": cancelled,
                "failed": totals.failed, "cached": totals.cached, "error": error, "diagnostics": diagnostics
            }),
        );
    });
    Ok(receipt)
}
#[derive(Default)]
struct FaceTotals {
    current: usize,
    total: usize,
    faces: usize,
    persons: usize,
    failed: usize,
    cached: usize,
}
fn claim_face_job(
    app: &AppHandle,
    job: &FaceJob,
    phase: &str,
    totals: &FaceTotals,
) -> Result<vision_jobs::Lease, String> {
    update_progress(job, |progress| {
        progress.phase = "waiting".into();
        progress.queued = true;
    });
    emit_progress(app, job, totals.failed, totals.cached);
    let lease = vision_jobs::claim_job(Task::Face, job.priority, &job.id, || job.check())?;
    job.check()?;
    update_progress(job, |progress| {
        progress.phase = phase.into();
        progress.queued = false;
    });
    emit_progress(app, job, totals.failed, totals.cached);
    Ok(lease)
}
fn publish_people(app: &AppHandle, job: &FaceJob, result: &t_cluster::ClusterResult) {
    if !result.person_ids.is_empty() {
        emit_job(
            app,
            job,
            "face-person-changed",
            serde_json::json!({"mode":"incremental","previousPersonIds":result.person_ids,"fileIds":result.file_ids}),
        );
        emit_job(
            app,
            job,
            "face-data-changed",
            serde_json::json!({"fileIds":result.file_ids}),
        );
    }
}
fn cluster_face_job(
    app: &AppHandle,
    job: &FaceJob,
    epsilon: f32,
    totals: &FaceTotals,
) -> Result<t_cluster::ClusterResult, String> {
    let _lane = claim_face_job(app, job, "clustering", totals)?;
    let result = t_cluster::cluster_faces(
        epsilon,
        job.scope.file_ids.as_deref(),
        |progress| {
            emit_job(
                app,
                job,
                "cluster_progress",
                serde_json::json!({"phase":progress.phase,"current":progress.current,"total":progress.total}),
            );
        },
        || job.check().is_err(),
    )?;
    job.check()?;
    Ok(result)
}
fn run_face_job(
    app: &AppHandle,
    face_state: &FaceState,
    job: &FaceJob,
    epsilon: f32,
    diagnostics: &mut crate::ai::face_diagnostics::JobReport,
    totals: &mut FaceTotals,
) -> Result<(), String> {
    job.check()?;
    let _library_guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
        .read()
        .map_err(|e| e.to_string())?;
    job.check()?;
    let (files, cached, conn) = {
        let _lane = claim_face_job(app, job, "loading", totals)?;
        crate::ai::profiles::ensure(Task::Face, &job.model.profile())?;
        let conn = t_sqlite::open_conn()?;
        let (files, cached) = crate::ai::face_jobs::selected_images(&conn, &job.scope)?;
        if job.scope.file_ids.is_some() {
            let missing = files
                .iter()
                .filter(|source| !std::path::Path::new(&source.path).is_file())
                .take(10)
                .map(|source| {
                    std::path::Path::new(&source.path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| format!("Image #{}", source.id))
                })
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                return Err(format!(
                    "Selected originals are unavailable; no images were processed: {}",
                    missing.join(", ")
                ));
            }
        }
        if !files.is_empty() {
            face_state
                .0
                .lock()
                .map_err(|e| e.to_string())?
                .ensure_model(&job.model)?;
        }
        (files, cached, conn)
    };
    totals.total = files.len() + cached;
    totals.cached = cached;
    totals.current = cached;
    diagnostics.image_count = totals.total;
    diagnostics.cached_images = cached;
    diagnostics.update_devices(&job.model);
    update_progress(job, |progress| {
        progress.total = totals.total;
        progress.current = totals.current;
        progress.faces_found = totals.faces;
    });
    let mut grouping = crate::ai::face_incremental::GroupingCadence::default();
    let mut last_grouping = std::time::Instant::now();
    for source in files {
        let lane = claim_face_job(app, job, "indexing", totals)?;
        job.check()?;
        totals.current += 1;
        use crate::ai::face_diagnostics::{ImageReport, PipelineReport};
        let filename = std::path::Path::new(&source.path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("Image #{}", source.id));
        let mut detail = ImageReport {
            file_id: source.id,
            file_name: filename.clone(),
            source_size: [source.width, source.height],
            format: std::path::Path::new(&source.path)
                .extension()
                .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default(),
            pipeline: PipelineReport {
                stage: "source_validation".into(),
                ..Default::default()
            },
            ..ImageReport::default()
        };
        match crate::ai::face_jobs::needs_inference(&conn, &source, job.scope.force) {
            Ok(false) => {
                totals.cached += 1;
                diagnostics.cached_images += 1;
            }
            needs_inference => {
                let result = needs_inference.and_then(|_| {
                    process_face_source(face_state, job, &conn, &source, &mut detail)
                });
                job.check()?;
                match result {
                    Ok(report) => {
                        grouping.committed(report.stored_regions);
                        totals.faces += report.stored_regions;
                        emit_job(
                            app,
                            job,
                            "face-data-changed",
                            serde_json::json!({"file_id":source.id}),
                        );
                    }
                    Err(error) => {
                        totals.failed += 1;
                        detail.error = Some(error.replace(&source.path, &filename));
                        detail.outcome = "failed".into();
                    }
                }
                diagnostics.add(detail);
            }
        }
        diagnostics.update_devices(&job.model);
        // Give queued interactive work a chance before grouping or the next background image.
        drop(lane);
        update_progress(job, |progress| {
            progress.current = totals.current;
            progress.faces_found = totals.faces;
        });
        emit_progress(app, job, totals.failed, totals.cached);
        if grouping.due(last_grouping.elapsed()) {
            let result = cluster_face_job(app, job, epsilon, totals)?;
            totals.persons = result.total_persons;
            publish_people(app, job, &result);
            grouping.published();
            last_grouping = std::time::Instant::now();
        }
    }
    let result = cluster_face_job(app, job, epsilon, totals)?;
    totals.persons = result.total_persons;
    publish_people(app, job, &result);
    emit_job(
        app,
        job,
        "face-data-changed",
        serde_json::json!({"file_id":null}),
    );
    Ok(())
}
fn process_face_source(
    face_state: &FaceState,
    job: &FaceJob,
    conn: &rusqlite::Connection,
    source: &crate::ai::face_jobs::SourceImage,
    detail: &mut crate::ai::face_diagnostics::ImageReport,
) -> Result<crate::ai::face_jobs::CommitReport, String> {
    detail.pipeline.stage = "source_validation".into();
    let before = std::fs::metadata(&source.path).map_err(|e| e.to_string())?;
    let before = (before.len(), before.modified().ok());
    if before.0 != source.size as u64 {
        return Err("Source changed before inference; old face records were retained".into());
    }
    detail.source_revision =
        crate::ai::face_confirmation::source_revision(source.id, before.0, before.1);
    detail.pipeline.stage = "read_image".into();
    let bytes = tauri::async_runtime::block_on(crate::t_image::get_file_image_bytes_cached(
        &source.path,
        crate::t_raw_display::RawDisplayOptions::rendered_bright(),
    ))?;
    job.check()?;
    let (result, report) = {
        let mut engine = face_state.0.lock().map_err(|e| e.to_string())?;
        engine.ensure_model(&job.model)?;
        engine.process_image_reported(&bytes)
    };
    detail.pipeline = report;
    let (mut faces, (proc_w, proc_h)) = result?;
    job.check()?;
    let after = std::fs::metadata(&source.path)
        .ok()
        .map(|metadata| (metadata.len(), metadata.modified().ok()));
    if after != Some(before) {
        detail.pipeline.stage = "source_validation".into();
        return Err(
            "Source missing or changed during inference; old face records were retained".into(),
        );
    }
    if proc_w == 0
        || proc_h == 0
        || faces.iter().any(|face| {
            ![
                face.bbox.x,
                face.bbox.y,
                face.bbox.width,
                face.bbox.height,
                face.bbox.confidence,
            ]
            .iter()
            .all(|value| value.is_finite())
                || face.bbox.width <= 0.
                || face.bbox.height <= 0.
        })
    {
        detail.pipeline.stage = "output_validation".into();
        return Err("Model returned invalid image dimensions or face regions".into());
    }
    let scale_x = source.width as f32 / proc_w as f32;
    let scale_y = source.height as f32 / proc_h as f32;
    for face in &mut faces {
        face.bbox.x *= scale_x;
        face.bbox.y *= scale_y;
        face.bbox.width *= scale_x;
        face.bbox.height *= scale_y;
        if let Some(points) = face.bbox.landmarks.as_mut() {
            for point in points {
                point.0 *= scale_x;
                point.1 *= scale_y;
            }
        }
    }
    let records = faces
        .into_iter()
        .map(|face| serde_json::to_string(&face.bbox).map(|bbox| (bbox, face.embedding)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    job.check()?;
    let report = crate::ai::face_jobs::replace_scanned_reported(conn, source, &records).map_err(
        |error| {
            detail.pipeline.stage = "database_commit".into();
            error
        },
    )?;
    detail.stored_regions = report.stored_regions;
    detail.manual_regions = report.manual_regions;
    detail.annotation_suppressed = report.suppressed_detections;
    detail.outcome = if detail.annotation_suppressed > 0 && detail.stored_regions == 0 {
        "annotation_suppressed"
    } else if detail.pipeline.detected_faces == 0 {
        "no_detection"
    } else if detail.pipeline.accepted_faces == 0 && detail.pipeline.quality_filtered > 0 {
        "quality_filtered"
    } else {
        "stored"
    }
    .into();
    Ok(report)
}

impl crate::ai::capabilities::FaceDetector for FaceEngine {
    fn detect(&mut self, image: &DynamicImage) -> Result<Vec<FaceBox>, String> {
        let _lane = crate::ai::vision_jobs::claim(crate::ai::types::Task::Face, || Ok(()))?;
        for attempt in 0..3 {
            let result = self
                .backend
                .as_mut()
                .ok_or("No face model loaded")?
                .detect(image);
            if let Err(ref error) = result {
                let model = self.model.clone().ok_or("No face model loaded")?;
                if attempt < 2
                    && crate::ai::runtime::recover_acceleration(&model, Some("detector"), error)
                {
                    self.backend = None;
                    self.load_model(model)?;
                    continue;
                }
            }
            return result;
        }
        Err("Face detector retries exhausted".into())
    }
}
impl crate::ai::capabilities::FaceEmbedder for FaceEngine {
    fn embed_face(&mut self, image: &DynamicImage, face: &FaceBox) -> Result<Vec<f32>, String> {
        let _lane = crate::ai::vision_jobs::claim(crate::ai::types::Task::Face, || Ok(()))?;
        for attempt in 0..3 {
            let result = self
                .backend
                .as_mut()
                .ok_or("No face model loaded")?
                .embed_face(image, face);
            if let Err(ref error) = result {
                let model = self.model.clone().ok_or("No face model loaded")?;
                if attempt < 2
                    && crate::ai::runtime::recover_acceleration(&model, Some("embedding"), error)
                {
                    self.backend = None;
                    self.load_model(model)?;
                    continue;
                }
            }
            return result;
        }
        Err("Face embedding retries exhausted".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::face_jobs::FaceScope;
    static FACE_TEST_LOCK: Mutex<()> = Mutex::new(());
    fn serialize_face_tests() -> std::sync::MutexGuard<'static, ()> {
        FACE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn model() -> ResolvedModel {
        crate::ai::settings::Configuration::default()
            .resolve("buffalo-s")
            .unwrap()
    }
    fn selection() -> FaceScope {
        FaceScope::new(Some(vec![1]), false).unwrap()
    }
    #[test]
    fn selection_jobs_are_accepted_and_status_prefers_the_actual_owner_then_interactive_fifo() {
        let _serial = serialize_face_tests();
        let mut jobs = FaceJobs::default();
        let background = jobs
            .register(
                "library".into(),
                model(),
                FaceScope::new(None, false).unwrap(),
            )
            .unwrap();
        let starting = jobs.progress_for(&background, None);
        assert!(!starting.queued);
        assert_eq!(starting.phase, "loading");
        assert_eq!(
            (starting.queued_jobs, starting.priority_queued_jobs),
            (0, 0)
        );
        assert!(
            jobs.register(
                "library".into(),
                model(),
                FaceScope::new(None, false).unwrap()
            )
            .is_err()
        );
        let first = jobs
            .register("library".into(), model(), selection())
            .unwrap();
        let second = jobs
            .register("library".into(), model(), selection())
            .unwrap();
        let waiting = jobs.snapshot(None).1.unwrap();
        assert_eq!(waiting.job_id, first.id);
        assert!(!waiting.queued);
        assert_eq!((waiting.queued_jobs, waiting.priority_queued_jobs), (2, 1));
        let running = jobs.snapshot(Some(&background.id)).1.unwrap();
        assert_eq!(running.job_id, background.id);
        assert!(!running.queued);
        assert_eq!((running.queued_jobs, running.priority_queued_jobs), (2, 2));
        assert!(jobs.progress_for(&first, Some(&background.id)).queued);
        let external_owner = jobs.snapshot(Some("")).1.unwrap();
        assert!(external_owner.queued);
        assert_eq!(
            (
                external_owner.queued_jobs,
                external_owner.priority_queued_jobs
            ),
            (3, 2)
        );
        let running = jobs.snapshot(Some(&second.id)).1.unwrap();
        assert_eq!(running.job_id, second.id);
        assert_eq!((running.queued_jobs, running.priority_queued_jobs), (2, 1));
        first.finish();
        assert_eq!(jobs.snapshot(None).1.unwrap().job_id, second.id);
        second.finish();
        assert_eq!(jobs.snapshot(None).1.unwrap().job_id, background.id);
    }
    #[test]
    fn targeted_and_library_cancellation_leave_other_jobs_untouched() {
        let _serial = serialize_face_tests();
        let mut jobs = FaceJobs::default();
        let first = jobs.register("a".into(), model(), selection()).unwrap();
        let second = jobs.register("a".into(), model(), selection()).unwrap();
        let other = jobs.register("b".into(), model(), selection()).unwrap();
        assert!(jobs.cancel(Some(&first.id), Some("b")).is_err());
        assert!(!first.cancel.load(Ordering::Acquire));
        jobs.cancel(Some(&first.id), Some("a")).unwrap();
        assert!(first.cancel.load(Ordering::Acquire));
        assert!(!second.cancel.load(Ordering::Acquire));
        assert!(!other.cancel.load(Ordering::Acquire));
        assert!(first.check().unwrap_err().contains("cancelled"));
        assert!(jobs.snapshot(None).0);
        jobs.cancel(None, Some("a")).unwrap();
        assert!(second.cancel.load(Ordering::Acquire));
        assert!(!other.cancel.load(Ordering::Acquire));
        jobs.cancel(None, None).unwrap();
        assert!(other.cancel.load(Ordering::Acquire));
        assert!(jobs.cancel(Some("missing"), None).is_err());
    }
    #[test]
    fn one_finished_job_keeps_the_compatibility_flag_and_other_progress_alive() {
        let _serial = serialize_face_tests();
        let status = FaceIndexingStatus(Arc::new(Mutex::new(false)));
        let mirror = FaceIndexProgressState(Arc::new(Mutex::new(FaceIndexProgress::default())));
        let mut jobs = FaceJobs {
            mirrors: Some((status.clone(), mirror.clone())),
            ..Default::default()
        };
        let first = jobs.register("a".into(), model(), selection()).unwrap();
        let second = jobs.register("a".into(), model(), selection()).unwrap();
        jobs.project(Some(&first.id));
        assert!(*status.0.lock().unwrap());
        first.finish();
        jobs.project(None);
        assert!(*status.0.lock().unwrap());
        assert_eq!(mirror.0.lock().unwrap().job_id, second.id);
        second.finish();
        jobs.project(None);
        assert!(!*status.0.lock().unwrap());
        assert!(jobs.snapshot(None).1.is_none());
    }
    #[test]
    fn live_jobs_and_completed_receipts_are_bounded_without_evicting_live_work() {
        let _serial = serialize_face_tests();
        let mut jobs = FaceJobs::default();
        let mut live = Vec::new();
        for _ in 0..32 {
            live.push(jobs.register("a".into(), model(), selection()).unwrap());
        }
        assert!(jobs.register("a".into(), model(), selection()).is_err());
        for job in live.iter().take(31) {
            job.finish();
        }
        jobs.project(None);
        assert_eq!(jobs.entries.len(), 17);
        assert_eq!(jobs.snapshot(None).1.unwrap().job_id, live[31].id);
        let queued = jobs.register("a".into(), model(), selection()).unwrap();
        assert!(jobs.entries.iter().any(|job| job.id == queued.id));
    }
    #[test]
    fn captured_face_model_rejects_both_session_and_profile_changes() {
        let _serial = serialize_face_tests();
        let mut jobs = FaceJobs::default();
        let job = jobs.register("a".into(), model(), selection()).unwrap();
        let mut changed = job.model.clone();
        assert!(job.check_model(&changed).is_ok());
        changed
            .values
            .insert("device".into(), serde_json::json!("cpu"));
        assert_eq!(changed.profile(), job.model.profile());
        assert!(job.check_model(&changed).is_err());
        let mut changed = job.model.clone();
        changed
            .values
            .insert("blur_threshold".into(), serde_json::json!(123.));
        assert_ne!(changed.profile(), job.model.profile());
        assert!(job.check_model(&changed).is_err());
    }
    #[test]
    fn finished_receipts_release_priority_before_the_last_job_reference_is_dropped() {
        let _serial = serialize_face_tests();
        let mut jobs = FaceJobs::default();
        let selected = jobs.register("a".into(), model(), selection()).unwrap();
        let retained = selected.clone();
        selected.finish();
        selected.registration.release();
        assert!(jobs.entries.iter().any(|job| job.id == selected.id));
        assert!(Arc::strong_count(&retained) >= 3);
        let background = jobs
            .register("a".into(), model(), FaceScope::new(None, false).unwrap())
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let _lane = vision_jobs::claim_job(Task::Face, Priority::Normal, &background.id, || {
            if std::time::Instant::now() > deadline {
                return Err("Completed history retained its priority registration".into());
            }
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn cancellation_and_panic_guards_release_priority_without_erasing_history() {
        let _serial = serialize_face_tests();
        let mut jobs = FaceJobs::default();
        let cancelled = jobs.register("a".into(), model(), selection()).unwrap();
        jobs.cancel(Some(&cancelled.id), Some("a")).unwrap();
        drop(RunningGuard(cancelled.clone()));
        assert!(cancelled.finished.load(Ordering::Acquire));
        let failed = jobs.register("a".into(), model(), selection()).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _running = RunningGuard(failed.clone());
            panic!("fake face worker failure");
        }));
        assert!(result.is_err());
        assert!(failed.finished.load(Ordering::Acquire));
        assert_eq!(jobs.entries.len(), 2);
        let background = jobs
            .register("a".into(), model(), FaceScope::new(None, false).unwrap())
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let _lane = vision_jobs::claim_job(Task::Face, Priority::Normal, &background.id, || {
            if std::time::Instant::now() > deadline {
                return Err("An exited face worker retained priority".into());
            }
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn registered_next_request_drives_receipts_between_image_leases() {
        let _serial = serialize_face_tests();
        let mut jobs = FaceJobs::default();
        let background = jobs
            .register("a".into(), model(), FaceScope::new(None, false).unwrap())
            .unwrap();
        let first = jobs.register("a".into(), model(), selection()).unwrap();
        let second = jobs.register("a".into(), model(), selection()).unwrap();
        let next = face_lane_owner();
        assert_eq!(next.as_deref(), Some(first.id.as_str()));
        assert!(!jobs.progress_for(&first, next.as_deref()).queued);
        assert!(jobs.progress_for(&second, next.as_deref()).queued);
        assert!(jobs.progress_for(&background, next.as_deref()).queued);
        first.finish();
        let next = face_lane_owner();
        assert_eq!(next.as_deref(), Some(second.id.as_str()));
        assert_eq!(jobs.snapshot(next.as_deref()).1.unwrap().job_id, second.id);
        second.finish();
        let next = face_lane_owner();
        assert_eq!(next.as_deref(), Some(background.id.as_str()));
        assert!(!jobs.progress_for(&background, next.as_deref()).queued);
    }
    #[test]
    fn queued_face_selection_runs_at_the_next_image_boundary_before_background_work() {
        let _serial = serialize_face_tests();
        use std::{
            cell::Cell,
            sync::{Barrier, mpsc},
            thread,
            time::Duration,
        };
        let wait = Duration::from_secs(5);
        let mut jobs = FaceJobs::default();
        let background = jobs
            .register("a".into(), model(), FaceScope::new(None, false).unwrap())
            .unwrap();
        let current =
            vision_jobs::claim_job(Task::Face, Priority::Normal, &background.id, || Ok(()))
                .unwrap();
        let selection = jobs.register("a".into(), model(), selection()).unwrap();
        let fake_engine = Arc::new(Mutex::new(Vec::new()));
        let (queued, queue_arrivals) = mpsc::channel();
        let (entered, arrivals) = mpsc::channel();
        let (between_images, gap) = mpsc::channel();
        let mut workers = Vec::new();
        let mut releases = Vec::new();
        for job in [background.clone(), selection.clone()] {
            let start = Arc::new(Barrier::new(2));
            let worker_start = start.clone();
            let fake_engine = fake_engine.clone();
            let queued = queued.clone();
            let entered = entered.clone();
            let between_images = between_images.clone();
            let (release, resume) = mpsc::channel();
            releases.push(release);
            workers.push(thread::spawn(move || {
                let _running = RunningGuard(job.clone());
                worker_start.wait();
                let images = if job.priority == Priority::Interactive {
                    2
                } else {
                    1
                };
                for image in 0..images {
                    let checks = Cell::new(0);
                    let lease = vision_jobs::claim_job(Task::Face, job.priority, &job.id, || {
                        checks.set(checks.get() + 1);
                        if checks.get() == 2 {
                            queued.send(()).map_err(|e| e.to_string())?;
                        }
                        if job.cancel.load(Ordering::Acquire) {
                            return Err("cancelled".into());
                        }
                        Ok(())
                    })?;
                    fake_engine
                        .lock()
                        .map_err(|e| e.to_string())?
                        .push((job.id.clone(), image));
                    entered
                        .send((job.id.clone(), image))
                        .map_err(|e| e.to_string())?;
                    resume.recv_timeout(wait).map_err(|e| e.to_string())?;
                    drop(lease);
                    if job.priority == Priority::Interactive && image == 0 {
                        between_images.send(()).map_err(|e| e.to_string())?;
                        resume.recv_timeout(wait).map_err(|e| e.to_string())?;
                    }
                }
                Ok::<_, String>(())
            }));
            start.wait();
            queue_arrivals.recv_timeout(wait).unwrap();
        }
        assert_eq!(
            jobs.snapshot(vision_jobs::active_or_next_request(Task::Face).as_deref())
                .1
                .unwrap()
                .job_id,
            background.id
        );
        drop(current);
        let first = arrivals.recv_timeout(wait);
        let active = jobs
            .snapshot(vision_jobs::active_or_next_request(Task::Face).as_deref())
            .1
            .unwrap();
        let _ = releases[1].send(());
        let gap_reached = gap.recv_timeout(wait);
        let background_waits = matches!(arrivals.try_recv(), Err(mpsc::TryRecvError::Empty));
        let _ = releases[1].send(());
        let second = arrivals.recv_timeout(wait);
        let _ = releases[1].send(());
        let third = arrivals.recv_timeout(wait);
        let _ = releases[0].send(());
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
        assert_eq!(first.unwrap(), (selection.id.clone(), 0));
        assert_eq!(active.job_id, selection.id);
        assert!(!active.queued);
        assert!(gap_reached.is_ok());
        assert!(
            background_waits,
            "Background inference ran between selected images"
        );
        assert_eq!(second.unwrap(), (selection.id.clone(), 1));
        assert_eq!(third.unwrap(), (background.id.clone(), 0));
        assert_eq!(
            *fake_engine.lock().unwrap(),
            vec![
                (selection.id.clone(), 0),
                (selection.id.clone(), 1),
                (background.id.clone(), 0)
            ]
        );
    }
}
