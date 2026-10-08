/**
 * Face Recognition module
 * Handles face detection (RetinaFace) and embedding (MobileFaceNet) using ONNX Runtime.
 */
use crate::{t_cluster, t_sqlite};
use image::DynamicImage;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

// cancellation token for face indexing
#[derive(Clone)]
pub struct FaceIndexCancellation(pub Arc<Mutex<bool>>);

// detailed status for face indexing
#[derive(Clone)]
pub struct FaceIndexingStatus(pub Arc<Mutex<bool>>);

// face indexing progress
#[derive(Clone, serde::Serialize)]
pub struct FaceIndexProgress {
    pub current: usize,
    pub total: usize,
    pub faces_found: usize,
    pub phase: String,
}

#[derive(Clone)]
pub struct FaceIndexProgressState(pub Arc<Mutex<FaceIndexProgress>>);

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
    pub fn new() -> Self { Self { model: None, backend: None, runtime_generation: crate::ai::runtime::generation() } }
    pub fn load_models(&mut self, _app: &AppHandle) -> Result<(), String> {
        let model = crate::ai::settings::active(crate::ai::types::Task::Face)?;
        crate::ai::profiles::ensure(crate::ai::types::Task::Face, &model.profile())?;
        if self.is_loaded() { return Ok(()); }
        self.load_instance(model)
    }
    pub fn load_instance(&mut self, model: crate::ai::types::ResolvedModel) -> Result<(), String> {
        let backend = crate::ai::adapters::face_backend(&model)?;
        self.backend = Some(backend); self.model = Some(model); self.runtime_generation = crate::ai::runtime::generation(); Ok(())
    }
    pub fn is_loaded(&self) -> bool {
        self.runtime_generation == crate::ai::runtime::generation() && self.backend.is_some() && self.model.as_ref()
            .zip(crate::ai::settings::active(crate::ai::types::Task::Face).ok().as_ref())
            .is_some_and(|(loaded,active)| loaded.session_key() == active.session_key())
    }
    fn process(&mut self,image:&DynamicImage)->Result<(Vec<FaceData>,(u32,u32)),String>{
        let result=self.backend.as_mut().ok_or("No face model loaded")?.process(image);
        if let Err(ref error)=result {
            let model=self.model.clone().ok_or("No face model loaded")?;
            if (crate::ai::runtime::is_auto(&model,"detector")||crate::ai::runtime::is_auto(&model,"embedding")) && crate::ai::runtime::used_acceleration(&model) {
                crate::ai::runtime::force_cpu(&model,error);self.backend=None;self.load_instance(model)?;
                return self.backend.as_mut().ok_or("No face model loaded")?.process(image);
            }
        }result
    }
    pub fn process_image(&mut self,path:&str)->Result<(Vec<FaceData>,(u32,u32)),String> {
        let image=image::open(path).map_err(|e|e.to_string())?;
        self.process(&image)
    }
    pub fn process_image_from_bytes(&mut self,bytes:&[u8])->Result<(Vec<FaceData>,(u32,u32)),String> {
        let image=image::load_from_memory(bytes).map_err(|e|e.to_string())?;
        self.process(&image)
    }
}

#[derive(Clone)]
pub struct FaceState(pub std::sync::Arc<Mutex<FaceEngine>>);

pub fn run_face_indexing(
    app_handle: AppHandle,
    face_state: FaceState,
    cancel_token_struct: FaceIndexCancellation,
    status_token_struct: FaceIndexingStatus,
    progress_token_struct: FaceIndexProgressState,
    cluster_epsilon: Option<f32>,
) -> Result<(), String> {
    let cancel_token = cancel_token_struct.0.clone();
    let status_token = status_token_struct.0.clone();
    let progress_token = progress_token_struct.0.clone();
    let model = crate::ai::settings::active(crate::ai::types::Task::Face)?;
    let epsilon = cluster_epsilon.unwrap_or(model.number("cluster_distance") as f32);
    let library_id = crate::t_config::current_library_id()?;

    // Check if already running
    {
        let mut running = status_token.lock().unwrap();
        if *running {
            return Err("Face indexing is already running".to_string());
        }
        *running = true;
    }

    // Reset cancellation flag
    *cancel_token.lock().unwrap() = false;

    // Reset progress
    {
        let mut progress = progress_token.lock().unwrap();
        progress.current = 0;
        progress.total = 0;
        progress.faces_found = 0;
        progress.phase = "indexing".to_string();
    }

    tauri::async_runtime::spawn_blocking(move || {
        // 1. Initialization
        let reset_status = || {
            if let Ok(mut running) = status_token.lock() {
                *running = false;
            }
        };

        let _library_guard = match crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read() {
            Ok(guard) => guard,
            Err(_) => { reset_status(); return; }
        };
        // Load models if not already loaded
        {
            let mut engine = face_state.0.lock().unwrap();
            {
                if let Err(e) = engine.load_models(&app_handle) {
                    eprintln!("Failed to load face models: {}", e);
                    let _ = app_handle.emit(
                        "face_index_finished",
                        serde_json::json!({
                            "total_faces": 0,
                            "total_persons": 0,
                            "cancelled": false,
                            "error": e.to_string()
                        }),
                    );
                    reset_status();
                    return;
                }
            }
        }

        // 2. Preparation (Get files and stats)
        let (processed_count, existing_faces_count) = match t_sqlite::Face::get_stats() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to get stats: {}", e);
                (0, 0)
            }
        };

        let files = match t_sqlite::Face::get_unprocessed_image_files() {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to get unprocessed files: {}", e);
                let _ = app_handle.emit(
                    "face_index_finished",
                    serde_json::json!({
                        "total_faces": 0,
                        "total_persons": 0,
                        "cancelled": false,
                        "error": e
                    }),
                );
                reset_status();
                return;
            }
        };

        let total_files = processed_count + files.len();
        let mut total_faces = existing_faces_count;
        let mut current = processed_count;

        // Init progress
        {
            let mut progress = progress_token.lock().unwrap();
            progress.total = total_files;
            progress.current = current;
            progress.faces_found = total_faces;
            progress.phase = "indexing".to_string();
        }

        let _ = app_handle.emit(
            "face_index_progress",
            serde_json::json!({
                "current": current,
                "total": total_files,
                "faces_found": total_faces,
                "phase": "indexing"
            }),
        );

        // 3. Image Processing Loop
        let mut cancelled = false;
        let db_conn = match t_sqlite::open_conn() {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Failed to open DB connection for face indexing: {}", e);
                let _ = app_handle.emit(
                    "face_index_finished",
                    serde_json::json!({
                        "total_faces": 0,
                        "total_persons": 0,
                        "cancelled": false,
                        "error": e
                    }),
                );
                reset_status();
                return;
            }
        };

        for (file_id, file_path, width, height, modified_at, size) in files {
            if *cancel_token.lock().unwrap() || crate::t_config::current_library_id().ok().as_deref() != Some(library_id.as_str()) {
                cancelled = true;
                break;
            }

            current += 1;

            let mut engine = face_state.0.lock().unwrap();

            // Optimization: Try to use thumbnail first
            // We need to know if we used a thumbnail to scale the bbox
            let (process_result, used_thumb) = match t_sqlite::AThumb::fetch(file_id) {
                Ok(Some(thumb)) if thumb.thumb_data.is_some() => {
                    let thumb_bytes = thumb.thumb_data.as_ref().unwrap();
                    match engine.process_image_from_bytes(thumb_bytes) {
                        Ok(res) => (Ok(res), true),
                        Err(_) => (engine.process_image(&file_path), false),
                    }
                }
                _ => (engine.process_image(&file_path), false),
            };

            match process_result {
                Ok((mut faces, (proc_w, proc_h))) => {
                    // If we used a thumbnail, scale bbox to original size
                    if used_thumb {
                        let scale_x = width as f32 / proc_w as f32;
                        let scale_y = height as f32 / proc_h as f32;

                        for face in &mut faces {
                            face.bbox.x *= scale_x;
                            face.bbox.y *= scale_y;
                            face.bbox.width *= scale_x;
                            face.bbox.height *= scale_y;
                            if let Some(points)=face.bbox.landmarks.as_mut(){for point in points{point.0*=scale_x;point.1*=scale_y;}}
                        }
                    }

                    let records=faces.into_iter().map(|face| (
                        serde_json::to_string(&face.bbox).unwrap_or_default(), face.embedding
                    )).collect::<Vec<_>>();
                    match t_sqlite::Face::save_scanned_with_conn(&db_conn, file_id, &records, Some((modified_at,size))) {
                        Ok(count) => total_faces += count,
                        Err(error) => eprintln!("Failed to commit face results for {file_id}: {error}"),
                    }
                }
                Err(e) => {
                    eprintln!("Failed to process image {}: {}", file_path, e);
                    // Do not retry files the decoder cannot read on every resume.
                    // Status 2 means the image was processed without a face result.
                    if let Err(mark_error) = t_sqlite::Face::save_scanned_with_conn(&db_conn, file_id, &[], Some((modified_at,size))) {
                        eprintln!("Failed to mark unreadable file {} as skipped: {}", file_id, mark_error);
                    }
                }
            }

            // Periodic progress update (every 10 files or at end)
            if current % 10 == 0 || current == total_files {
                {
                    let mut progress = progress_token.lock().unwrap();
                    progress.current = current;
                    progress.faces_found = total_faces;
                }

                let _ = app_handle.emit(
                    "face_index_progress",
                    serde_json::json!({
                        "current": current,
                        "total": total_files,
                        "faces_found": total_faces,
                        "phase": "indexing"
                    }),
                );
            }
        }

        if cancelled {
            let _ = app_handle.emit(
                "face_index_finished",
                serde_json::json!({
                    "total_faces": total_faces,
                    "total_persons": 0,
                    "cancelled": true
                }),
            );
            reset_status();
            return;
        }

        // 4. Clustering
        {
            let mut progress = progress_token.lock().unwrap();
            progress.phase = "clustering".to_string();
        }

        let _ = app_handle.emit(
            "face_index_progress",
            serde_json::json!({
                "current": total_files,
                "total": total_files,
                "faces_found": total_faces,
                "phase": "clustering"
            }),
        );

        let cancel_token_cluster = cancel_token.clone();
        let total_persons = match t_cluster::cluster_faces(
            epsilon,
            |progress| {
                let _ = app_handle.emit(
                    "cluster_progress",
                    serde_json::json!({
                        "phase": progress.phase,
                        "current": progress.current,
                        "total": progress.total,
                    }),
                );
            },
            || {
                // Check if user has cancelled
                *cancel_token_cluster.lock().unwrap()
            },
        ) {
            Ok(count) => count,
            Err(e) => {
                eprintln!("Clustering failed: {}", e);
                0
            }
        };
        let cancelled_during_cluster = *cancel_token.lock().unwrap();

        // 5. Finished
        let _ = app_handle.emit(
            "face_index_finished",
            serde_json::json!({
                "total_faces": total_faces,
                "total_persons": total_persons,
                "cancelled": cancelled_during_cluster
            }),
        );
        reset_status();
    });

    Ok(())
}

impl crate::ai::capabilities::FaceDetector for FaceEngine {
    fn detect(&mut self, image: &DynamicImage) -> Result<Vec<FaceBox>, String> { self.backend.as_mut().ok_or("No face model loaded")?.detect(image) }
}
impl crate::ai::capabilities::FaceEmbedder for FaceEngine {
    fn embed_face(&mut self, image: &DynamicImage, face: &FaceBox) -> Result<Vec<f32>, String> { self.backend.as_mut().ok_or("No face model loaded")?.embed_face(image, face) }
}
