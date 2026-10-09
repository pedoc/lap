use super::{assets, settings, types::*};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelView {
    pub definition: ModelDefinition,
    pub configuration: ModelConfiguration,
    pub installed: bool,
    pub builtin: bool,
    pub parameters: Vec<ParameterSpec>,
    pub values: BTreeMap<String, Value>,
    pub profile: String,
    pub credential_stored: bool,
    pub contract_tested: bool,
    pub sessions: Vec<super::runtime::SessionReport>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationView {
    pub models: Vec<ModelView>,
    pub selection: settings::Selection,
    pub semantic_profile: String,
    pub face_profile: String,
    pub runtime: super::runtime::RuntimeInfo,
}
pub fn view() -> Result<ConfigurationView, String> {
    let config = settings::snapshot()?;
    let builtins = settings::builtins();
    let models = config
        .catalog()
        .into_iter()
        .map(|definition| {
            let resolved = config.resolve(&definition.id)?;
            let credential_stored = definition.adapter == Adapter::JinaEmbeddings
                && super::remote::credential(
                    &definition.id,
                    &resolved.configuration.credential_revision,
                )
                .ok()
                .is_some_and(|entry| entry.get_password().is_ok());
            let contract_tested = config
                .tested_contracts
                .get(&definition.id)
                .is_some_and(|key| *key == resolved.contract_key());
            let sessions = super::runtime::reports(&resolved);
            Ok(ModelView {
                installed: assets::installed(&definition),
                builtin: builtins.iter().any(|d| d.id == definition.id),
                parameters: parameters(definition.task, definition.adapter),
                definition,
                profile: resolved.profile(),
                configuration: resolved.configuration,
                values: resolved.values,
                credential_stored,
                contract_tested,
                sessions,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ConfigurationView {
        semantic_profile: config.active(Task::Semantic)?.profile(),
        face_profile: config.active(Task::Face)?.profile(),
        selection: config.selection,
        models,
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
pub async fn save_ai_model(
    app_handle: AppHandle,
    model_id: String,
    mut configuration: ModelConfiguration,
    api_key: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        require_idle(&app_handle, || {
            let config = settings::snapshot()?;
            let definition = config
                .catalog()
                .into_iter()
                .find(|d| d.id == model_id)
                .ok_or("Unknown model")?;
            let previous = config
                .models
                .get(&model_id)
                .ok_or("Unknown model configuration")?;
            let credential_changed = api_key.as_ref().is_some_and(|key| !key.is_empty());
            configuration.credential_revision = if credential_changed {
                uuid::Uuid::new_v4().to_string()
            } else {
                previous.credential_revision.clone()
            };
            let resolved = ResolvedModel::new(definition, configuration.clone())?;
            let previous_origin = reqwest::Url::parse(&previous.endpoint)
                .ok()
                .map(|u| u.origin());
            let next_origin = reqwest::Url::parse(&configuration.endpoint)
                .ok()
                .map(|u| u.origin());
            if previous_origin != next_origin
                && super::remote::credential(&model_id, &previous.credential_revision)
                    .ok()
                    .is_some_and(|entry| entry.get_password().is_ok())
                && !credential_changed
            {
                return Err(
                    "Changing the provider origin requires entering an API key explicitly".into(),
                );
            }
            if let Some(key) = api_key.filter(|key| !key.is_empty()) {
                if key.len() > 16384 {
                    return Err("API key exceeds the supported length".into());
                }
                if resolved.definition.adapter != Adapter::JinaEmbeddings {
                    return Err("This local model does not use API credentials".into());
                }
                super::remote::credential(&model_id, &configuration.credential_revision)?
                    .set_password(&key)
                    .map_err(
                        |_| "Could not store API key securely in the system credential store",
                    )?;
            }
            let revision = configuration.credential_revision.clone();
            if let Err(error) = settings::save_model(&model_id, configuration) {
                if credential_changed {
                    if let Ok(entry) = super::remote::credential(&model_id, &revision) {
                        let _ = entry.delete_credential();
                    }
                }
                return Err(error);
            }
            if credential_changed {
                if let Ok(entry) =
                    super::remote::credential(&model_id, &previous.credential_revision)
                {
                    let _ = entry.delete_credential();
                }
            }
            for task in [Task::Semantic, Task::Face] {
                if config.selected(task) == model_id {
                    super::profiles::ensure(task, &resolved.profile())?;
                }
            }
            changed(&app_handle)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn activate_ai_model(
    app_handle: AppHandle,
    task: Task,
    model_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        require_idle(&app_handle, || {
            let model = settings::snapshot()?.resolve(&model_id)?;
            if model.definition.task != task {
                return Err("Capability mismatch".into());
            }
            if model.definition.adapter == Adapter::JinaEmbeddings {
                if !model.configuration.allow_cloud {
                    return Err("Explicit online processing consent is required".into());
                }
                super::remote::credential(&model_id, &model.configuration.credential_revision)?
                    .get_password()
                    .map_err(|_| "Save an API key before enabling this model")?;
                if !settings::contract_tested(&model) {
                    return Err(
                        "Test this online model's image/text capability before enabling it".into(),
                    );
                }
            } else {
                assets::verify(&model.definition)?;
                if !settings::contract_tested(&model) {
                    super::runtime::with_profiling(|| test_resolved(&model))?;
                    settings::mark_tested(&model_id, &model.contract_key())?;
                }
            }
            settings::select(task, &model_id)?;
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
pub async fn delete_ai_model(app_handle: AppHandle, model_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        require_idle(&app_handle, || {
            let previous = settings::snapshot()?.models.get(&model_id).cloned();
            settings::delete_model(&model_id)?;
            if let Some(previous) = previous {
                if let Ok(entry) =
                    super::remote::credential(&model_id, &previous.credential_revision)
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
pub async fn open_ai_model_directory(model_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let model = settings::snapshot()?
            .catalog()
            .into_iter()
            .find(|m| m.id == model_id)
            .ok_or("Unknown model ID")?;
        let directory = assets::prepare_directory(&model)?;
        let path = directory
            .to_str()
            .ok_or("Model directory is not valid UTF-8")?;
        crate::t_utils::reveal_path(path)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn cancel_ai_model_download(model_id: String) -> Result<(), String> {
    assets::cancel(&model_id)
}
#[tauri::command]
pub async fn test_ai_model(model_id: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _configuration = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        let model = settings::snapshot()?.resolve(&model_id)?;
        let result = super::runtime::with_profiling(|| test_resolved(&model))?;
        settings::mark_tested(&model_id, &model.contract_key())?;
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
            backend.load_model(model.clone())?;
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
