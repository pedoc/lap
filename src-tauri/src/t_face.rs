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
    pub library_id: String,
    pub scope: String,
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
        self.load_model(model)
    }
    pub fn load_model(&mut self, model: crate::ai::types::ResolvedModel) -> Result<(), String> {
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
                crate::ai::runtime::force_cpu(&model,error);self.backend=None;self.load_model(model)?;
                return self.backend.as_mut().ok_or("No face model loaded")?.process(image);
            }
        }result
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn process_image(&mut self,path:&str)->Result<(Vec<FaceData>,(u32,u32)),String> {
        let bytes=std::fs::read(path).map_err(|e|e.to_string())?;
        self.process_image_from_bytes(&bytes)
    }
    pub fn process_image_from_bytes(&mut self,bytes:&[u8])->Result<(Vec<FaceData>,(u32,u32)),String> {
        let image=crate::ai::face_jobs::decode_image(bytes)?;
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
    scope: crate::ai::face_jobs::FaceScope,
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
        progress.library_id = library_id.clone();
        progress.scope = scope.key().into();
    }

    tauri::async_runtime::spawn_blocking(move || {
        struct RunningGuard(Arc<Mutex<bool>>);
        impl Drop for RunningGuard { fn drop(&mut self) { if let Ok(mut running)=self.0.lock(){*running=false;} } }
        let _running = RunningGuard(status_token.clone());
        let emit = |event: &str, mut payload: serde_json::Value| {
            payload["library_id"] = serde_json::json!(library_id);
            payload["scope"] = serde_json::json!(scope.key());
            app_handle.emit(event, payload)
        };
        // 1. Initialization

        let _library_guard = match crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read() {
            Ok(guard) => guard,
            Err(_) => {  return; }
        };
        if crate::t_config::current_library_id().ok().as_deref()!=Some(library_id.as_str()) || *cancel_token.lock().unwrap() {
            let _=emit("face_index_finished",serde_json::json!({"cancelled":true,"total_faces":0,"total_persons":0}));return;
        }
        let prepared=(|| -> Result<_,String> {
            crate::ai::profiles::ensure(crate::ai::types::Task::Face,&model.profile())?;
            let conn=t_sqlite::open_conn()?;
            let (files,cached)=crate::ai::face_jobs::selected_images(&conn,&scope)?;
            // A bad/offline selection is rejected before modifying any of its face records.
            if scope.file_ids.is_some() && files.iter().any(|image|!std::path::Path::new(&image.path).is_file()) {
                return Err("Selected images are unavailable; no images were processed".into());
            }
            if !files.is_empty() { face_state.0.lock().map_err(|e|e.to_string())?.load_models(&app_handle)?; }
            Ok((files,cached))
        })();
        let (files,cached)=match prepared { Ok(value)=>value,Err(error)=> {
            let _=emit("face_index_finished",serde_json::json!({"total_faces":0,"total_persons":0,"cancelled":false,"error":error}));return;
        }};
        let total_files=files.len()+cached;
        let mut total_faces=0;
        let mut current=cached;
        let mut failed=0usize;

        // Init progress
        {
            let mut progress = progress_token.lock().unwrap();
            progress.total = total_files;
            progress.current = current;
            progress.faces_found = total_faces;
            progress.phase = "indexing".to_string();
        }

        let _ = emit(
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
                let _ = emit(
                    "face_index_finished",
                    serde_json::json!({
                        "total_faces": 0,
                        "total_persons": 0,
                        "cancelled": false,
                        "error": e
                    }),
                );

                return;
            }
        };

        for source in files {
            let (file_id,file_path,width,height,modified_at,size)=(source.id,source.path.clone(),source.width,source.height,source.modified_at,source.size);
            if *cancel_token.lock().unwrap() || crate::t_config::current_library_id().ok().as_deref() != Some(library_id.as_str()) {
                cancelled = true;
                break;
            }

            current += 1;

            let mut engine = face_state.0.lock().unwrap();

            // Prefer the original/high-resolution preview: small thumbnails lose faces in group photos.
            let before=std::fs::metadata(&file_path).ok().map(|m|(m.len(),m.modified().ok()));
            let bytes=tauri::async_runtime::block_on(crate::t_image::get_file_image_bytes_cached(
                &file_path,crate::t_raw_display::RawDisplayOptions::rendered_bright()));
            let process_result=bytes.and_then(|bytes|engine.process_image_from_bytes(&bytes));
            let after=std::fs::metadata(&file_path).ok().map(|m|(m.len(),m.modified().ok()));
            drop(engine);
            if before.is_none() || before!=after || before.as_ref().is_some_and(|s|s.0!=size as u64) {
                failed+=1;
                continue;
            }
            let used_thumb=true; // Scale any backend preview's coordinates to catalog dimensions.
            match process_result {
                Ok((mut faces, (proc_w, proc_h))) => {
                    if proc_w==0 || proc_h==0 || faces.iter().any(|face| ![face.bbox.x,face.bbox.y,face.bbox.width,face.bbox.height,face.bbox.confidence].iter().all(|v|v.is_finite()) || face.bbox.width<=0. || face.bbox.height<=0.) {
                        failed+=1; continue;
                    }
                    // Scale original/high-resolution preview coordinates to catalog dimensions.
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

                    let records=faces.into_iter().map(|face| serde_json::to_string(&face.bbox).map(|bbox|(bbox,face.embedding))).collect::<Result<Vec<_>,_>>();
                    let Ok(records)=records else { failed+=1; continue; };
                    let committed=if scope.force {
                        crate::ai::face_jobs::replace_scanned(&db_conn,&source,&records)
                    } else { t_sqlite::Face::save_scanned_with_conn(&db_conn,file_id,&records,Some((modified_at,size))) };
                    match committed {
                        Ok(count) => { total_faces+=count; let _=emit("face-data-changed",serde_json::json!({"file_id":file_id})); },
                        Err(error) => { failed+=1; eprintln!("Failed to commit face results for {file_id}: {error}"); },
                    }
                }
                Err(e) => {
                    eprintln!("Failed to process image {}: {}", file_path, e);
                    // Decode/inference failures must not erase previous faces or mark a failed image as face-free.
                    failed+=1;
                }
            }

            // Each committed image becomes visible immediately.
            {
                {
                    let mut progress = progress_token.lock().unwrap();
                    progress.current = current;
                    progress.faces_found = total_faces;
                }

                let _ = emit(
                    "face_index_progress",
                    serde_json::json!({
                        "current": current,
                        "total": total_files,
                        "faces_found": total_faces,
                        "phase": "indexing", "failed":failed, "cached":cached
                    }),
                );
            }
        }

        if cancelled {
            let _ = emit(
                "face_index_finished",
                serde_json::json!({
                    "total_faces": total_faces,
                    "total_persons": 0,
                    "cancelled": true
                }),
            );

            return;
        }

        // 4. Clustering
        {
            let mut progress = progress_token.lock().unwrap();
            progress.phase = "clustering".to_string();
        }

        let _ = emit(
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
            scope.file_ids.as_deref(),
            |progress| {
                let _ = emit(
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
                let _=emit("face_index_finished",serde_json::json!({"total_faces":total_faces,"total_persons":0,"cancelled":false,"failed":failed,"cached":cached,"error":e}));
                return;
            }
        };
        let cancelled_during_cluster = *cancel_token.lock().unwrap();

        let _=emit("face-data-changed",serde_json::json!({"file_id":null}));
        // 5. Finished
        let _ = emit(
            "face_index_finished",
            serde_json::json!({
                "total_faces": total_faces,
                "total_persons": total_persons,
                "cancelled": cancelled_during_cluster, "failed":failed, "cached":cached
            }),
        );

    });

    Ok(())
}

impl crate::ai::capabilities::FaceDetector for FaceEngine {
    fn detect(&mut self, image: &DynamicImage) -> Result<Vec<FaceBox>, String> { self.backend.as_mut().ok_or("No face model loaded")?.detect(image) }
}
impl crate::ai::capabilities::FaceEmbedder for FaceEngine {
    fn embed_face(&mut self, image: &DynamicImage, face: &FaceBox) -> Result<Vec<f32>, String> { self.backend.as_mut().ok_or("No face model loaded")?.embed_face(image, face) }
}
