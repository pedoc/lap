use super::{assets, settings, types::*};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tauri::{AppHandle, Emitter, Manager};
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelView {
    pub definition: ModelDefinition,
    pub installed: bool,
    pub parameters: Vec<ParameterSpec>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceView {
    pub instance: ModelInstance,
    pub values: BTreeMap<String, Value>,
    pub credential_stored: bool,
    pub contract_tested: bool,
    pub sessions: Vec<super::runtime::SessionReport>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationView {
    pub library_id: String,
    pub models: Vec<ModelView>,
    pub instances: Vec<InstanceView>,
    pub bindings: settings::Binding,
    pub semantic_profile: String,
    pub face_profile: String,
    pub runtime: super::runtime::RuntimeInfo,
}
pub fn view() -> Result<ConfigurationView, String> {
    let config = settings::snapshot()?;
    let library_id = crate::t_config::current_library_id()?;
    let models = config
        .catalog()
        .into_iter()
        .map(|d| ModelView {
            installed: assets::installed(&d),
            parameters: parameters(d.task, d.adapter),
            definition: d,
        })
        .collect();
    let instances = config
        .instances
        .iter()
        .map(|i| {
            let resolved = config.resolve(&i.id)?;
            let credential_stored = if resolved.definition.adapter == Adapter::JinaEmbeddings {
                super::remote::credential(&i.id, &i.credential_revision)
                    .ok()
                    .is_some_and(|entry| entry.get_password().is_ok())
            } else {
                false
            };
            let contract_tested = config
                .tested_contracts
                .get(&i.id)
                .is_some_and(|key| *key == resolved.contract_key());
            let sessions = super::runtime::reports(&resolved);
            Ok(InstanceView {
                instance: i.clone(),
                values: resolved.values,
                credential_stored,
                contract_tested,
                sessions,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bindings = config
        .bindings
        .get(&library_id)
        .cloned()
        .unwrap_or_default();
    let semantic_profile = config.resolve(&bindings.semantic)?.profile();
    let face_profile = config.resolve(&bindings.face)?.profile();
    Ok(ConfigurationView {
        library_id,
        models,
        instances,
        bindings,
        semantic_profile,
        face_profile,
        runtime: super::runtime::info(),
    })
}
fn changed(app: &AppHandle) -> Result<(), String> {
    let payload = view()?;
    app.emit("ai-configuration-changed", payload)
        .map_err(|e| e.to_string())
}
fn require_idle<T>(
    app: &AppHandle,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _library_guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
        .try_write()
        .map_err(|_| "An inference or library operation is running; try again after it finishes")?;
    let face = app.state::<crate::t_face::FaceIndexingStatus>().0.clone();
    let face_guard = face.lock().map_err(|e| e.to_string())?;
    let indexing = app.state::<crate::t_cmds::IndexCancellation>().0.clone();
    let index_guard = indexing.lock().map_err(|e| e.to_string())?;
    let similar = app.state::<crate::t_similar::SimilarState>();
    let similar_guard = similar.status.lock().map_err(|e| e.to_string())?;
    if *face_guard || !index_guard.is_empty() || similar_guard.is_scanning {
        return Err(
            "Stop library/face/similarity indexing before changing AI configuration".into(),
        );
    }
    drop(similar_guard);
    drop(index_guard);
    drop(face_guard);
    operation()
}
#[tauri::command]
pub async fn get_ai_configuration() -> Result<ConfigurationView, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let _configuration = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        view()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn save_ai_instance(
    app_handle: AppHandle,
    mut instance: ModelInstance,
    api_key: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        require_idle(&app_handle, || {
            let config = settings::snapshot()?;
            let definition = config
                .catalog()
                .into_iter()
                .find(|d| d.id == instance.model_id)
                .ok_or("Unknown model")?;
            let credential_changed = api_key.as_ref().is_some_and(|k| !k.is_empty());
            instance.credential_revision = if credential_changed {
                uuid::Uuid::new_v4().to_string()
            } else {
                config
                    .instances
                    .iter()
                    .find(|i| i.id == instance.id)
                    .map(|i| i.credential_revision.clone())
                    .unwrap_or_default()
            };
            let instance_id = instance.id.clone();
            let resolved = ResolvedModel::new(definition, instance.clone())?;
            if let Some(existing) = config.instances.iter().find(|i| i.id == instance.id) {
                let previous = reqwest::Url::parse(&existing.endpoint).ok();
                let next = reqwest::Url::parse(&instance.endpoint).ok();
                if previous.as_ref().map(|u| u.origin()) != next.as_ref().map(|u| u.origin())
                    && super::remote::credential(&instance.id, &existing.credential_revision)
                        .ok()
                        .is_some_and(|entry| entry.get_password().is_ok())
                    && api_key.as_ref().is_none_or(|key| key.is_empty())
                {
                    return Err(
                        "Changing the provider origin requires entering an API key explicitly"
                            .into(),
                    );
                }
            }
            if let Some(key) = api_key.filter(|k| !k.is_empty()) {
                if key.len() > 16384 {
                    return Err("API key exceeds the supported length".into());
                }
                if resolved.definition.adapter != Adapter::JinaEmbeddings {
                    return Err("This local model does not use API credentials".into());
                }
                super::remote::credential(&instance.id, &instance.credential_revision)?
                    .set_password(&key)
                    .map_err(
                        |_| "Could not store API key securely in the system credential store",
                    )?;
            }
            let revision = instance.credential_revision.clone();
            if let Err(error) = settings::save_instance(instance) {
                if credential_changed {
                    if let Ok(entry) = super::remote::credential(&instance_id, &revision) {
                        let _ = entry.delete_credential();
                    }
                }
                return Err(error);
            }
            if credential_changed {
                if let Some(old) = config.instances.iter().find(|i| i.id == instance_id) {
                    if let Ok(entry) = super::remote::credential(&old.id, &old.credential_revision)
                    {
                        let _ = entry.delete_credential();
                    }
                }
            }
            for task in [Task::Semantic, Task::Face] {
                let current = settings::active(task)?;
                super::profiles::ensure(task, &current.profile())?;
            }
            changed(&app_handle)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn activate_ai_instance(
    app_handle: AppHandle,
    task: Task,
    instance_id: String,
    library_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        require_idle(&app_handle, || {
            if crate::t_config::current_library_id()? != library_id {
                return Err("Library changed; reload AI settings".into());
            }
            let model = settings::snapshot()?.resolve(&instance_id)?;
            if model.definition.task != task {
                return Err("Capability mismatch".into());
            }
            if model.definition.adapter == Adapter::JinaEmbeddings {
                if !model.instance.allow_cloud {
                    return Err("Explicit online processing consent is required".into());
                }
                super::remote::credential(&model.instance.id, &model.instance.credential_revision)?
                    .get_password()
                    .map_err(|_| "Save an API key before enabling this instance")?;
                if !settings::contract_tested(&model) {
                    return Err(
                        "Test this API instance's image/text capability before enabling it".into(),
                    );
                }
            } else {
                assets::verify(&model.definition)?;
                if !settings::contract_tested(&model) {
                    super::runtime::with_profiling(|| test_resolved(&model))?;
                    settings::mark_tested(&model.instance.id, &model.contract_key())?;
                }
            }
            settings::bind(task, &instance_id)?;
            super::profiles::ensure(task, &model.profile())?;
            changed(&app_handle)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn import_ai_model(app_handle: AppHandle, manifest: String) -> Result<(), String> {
    if manifest.len() > 2 * 1024 * 1024 {
        return Err("Model manifest exceeds 2 MB".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let definition: ModelDefinition =
            serde_json::from_str(&manifest).map_err(|e| format!("Invalid model manifest: {e}"))?;
        settings::import_model(definition)?;
        changed(&app_handle)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn delete_ai_instance(app_handle: AppHandle, instance_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        require_idle(&app_handle, || {
            let previous = settings::snapshot()?
                .instances
                .into_iter()
                .find(|i| i.id == instance_id);
            settings::delete_instance(&instance_id)?;
            if let Some(previous) = previous {
                if let Ok(entry) =
                    super::remote::credential(&instance_id, &previous.credential_revision)
                {
                    let _ = entry.delete_credential();
                }
            }
            changed(&app_handle)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn download_ai_model(app_handle: AppHandle, model_id: String) -> Result<(), String> {
    let model = settings::snapshot()?
        .catalog()
        .into_iter()
        .find(|m| m.id == model_id)
        .ok_or("Unknown model ID")?;
    assets::download(app_handle.clone(), model).await?;
    changed(&app_handle)
}
#[tauri::command]
pub fn cancel_ai_model_download(model_id: String) -> Result<(), String> {
    assets::cancel(&model_id)
}
#[tauri::command]
pub async fn test_ai_instance(instance_id: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _configuration = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        let model = settings::snapshot()?.resolve(&instance_id)?;
        let result = super::runtime::with_profiling(|| test_resolved(&model))?;
        settings::mark_tested(&instance_id, &model.contract_key())?;
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}
fn test_resolved(model: &ResolvedModel) -> Result<Value, String> {
    let started = std::time::Instant::now();
    let size = match model.definition.task {
        Task::Semantic => {
            let mut backend = crate::t_ai::build_embedder(&model)?;
            let text = backend.encode_text("a photograph of a person")?;
            let image = image::DynamicImage::new_rgb8(224, 224);
            let mut bytes = std::io::Cursor::new(Vec::new());
            image
                .write_to(&mut bytes, image::ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            let vector = backend.encode_image(&bytes.into_inner())?;
            if text.len() != vector.len() {
                return Err("Image and text embedding dimensions differ".into());
            }
            vector.len()
        }
        Task::Face => {
            use super::capabilities::{FaceDetector, FaceEmbedder};
            let mut backend = crate::t_face::FaceEngine::new();
            backend.load_instance(model.clone())?;
            let image = image::DynamicImage::new_rgb8(640, 640);
            backend.detect(&image)?;
            let region = crate::t_face::FaceBox {
                x: 100.,
                y: 100.,
                width: 112.,
                height: 112.,
                confidence: 1.,
                landmarks: Some(
                    crate::ai::alignment::ARCFACE_POINTS
                        .iter()
                        .map(|(x, y)| (x + 100.0, y + 100.0))
                        .collect(),
                ),
            };
            backend.embed_face(&image, &region)?.len()
        }
    };
    Ok(
        json!({"dimension":size,"elapsedMs":started.elapsed().as_millis(),"runtime":runtime_summary(model),"sessions":super::runtime::reports(model),"profile":model.profile()}),
    )
}

fn runtime_summary(model: &ResolvedModel) -> String {
    if model.definition.adapter == Adapter::JinaEmbeddings {
        return "http".into();
    }
    let backends = super::runtime::reports(model)
        .into_iter()
        .map(|r| r.selected)
        .collect::<std::collections::BTreeSet<_>>();
    format!(
        "onnx-{}",
        backends.into_iter().collect::<Vec<_>>().join("+")
    )
}
#[tauri::command]
pub async fn refresh_ai_runtime(app_handle: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        require_idle(&app_handle, || {
            super::runtime::refresh();
            changed(&app_handle)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
