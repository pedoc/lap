use super::types::{Adapter, ModelDefinition};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
static DOWNLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static CANCEL: Mutex<Option<(String, tokio::sync::watch::Sender<bool>)>> = Mutex::new(None);
pub fn directory(model: &ModelDefinition) -> Result<PathBuf, String> {
    model.validate()?;
    #[cfg(test)]
    if let Some(root) = std::env::var_os("LAP_AI_TEST_MODEL_ROOT") {
        return Ok(PathBuf::from(root).join(&model.id));
    }
    Ok(crate::t_config::get_app_data_dir()?
        .join("models")
        .join(&model.id)
        .join(model.digest()))
}
pub fn artifact_path(model: &ModelDefinition, role: &str) -> Result<PathBuf, String> {
    if !model.files.iter().any(|f| f.role == role) {
        return Err(format!("Unknown artifact role: {role}"));
    }
    Ok(directory(model)?.join(if role == "tokenizer" {
        "tokenizer.json".to_string()
    } else {
        format!("{role}.onnx")
    }))
}
pub fn installed(model: &ModelDefinition) -> bool {
    if model.adapter == Adapter::JinaEmbeddings {
        return true;
    }
    let Ok(dir) = directory(model) else {
        return false;
    };
    let receipt = std::fs::read_to_string(dir.join("installed.json")).ok();
    if receipt.as_deref() != Some(model.digest().as_str()) {
        return false;
    }
    model.files.iter().all(|f| {
        artifact_path(model, &f.role)
            .ok()
            .and_then(|p| std::fs::metadata(p).ok())
            .is_some_and(|m| m.is_file() && m.len() > 0 && (f.size == 0 || m.len() == f.size))
    })
}
pub fn verify(model: &ModelDefinition) -> Result<(), String> {
    if !installed(model) {
        return Err(format!(
            "{} is not installed. Download it in Settings → AI models.",
            model.name
        ));
    }
    for f in &model.files {
        use std::io::Read;
        let mut file =
            std::fs::File::open(artifact_path(model, &f.role)?).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        if format!("{:x}", hash.finalize()) != f.sha256.to_lowercase() {
            return Err(format!(
                "{} failed integrity verification; reinstall this model",
                f.role
            ));
        }
    }
    Ok(())
}
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct DownloadCancellation;
impl Drop for DownloadCancellation {
    fn drop(&mut self) {
        if let Ok(mut c) = CANCEL.lock() {
            *c = None;
        }
    }
}
pub fn cancel(model_id: &str) -> Result<(), String> {
    if let Some((id, tx)) = CANCEL.lock().map_err(|e| e.to_string())?.as_ref() {
        if id != model_id {
            return Err("Another model is downloading; cancellation target does not match".into());
        }
        let _ = tx.send(true);
    }
    Ok(())
}
pub async fn download(app: AppHandle, model: ModelDefinition) -> Result<(), String> {
    model.validate()?;
    if model.adapter == Adapter::JinaEmbeddings {
        return Err("Online API models do not have local artifacts".into());
    }
    let _lock = DOWNLOAD_LOCK
        .try_lock()
        .map_err(|_| "Another model download is already running")?;
    let (tx, mut cancelled) = tokio::sync::watch::channel(false);
    *CANCEL.lock().map_err(|e| e.to_string())? = Some((model.id.clone(), tx));
    let _cancel_guard = DownloadCancellation;
    let target = directory(&model)?;
    let parent = target.parent().ok_or("Invalid model directory")?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| e.to_string())?;
    let staging = parent.join(format!(".download-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir(&staging)
        .await
        .map_err(|e| e.to_string())?;
    let _stage_guard = Stage(staging.clone());
    let client = crate::t_network::client()?;
    let total: u64 = model.files.iter().map(|f| f.size).sum();
    let mut received = 0u64;
    let mut emitted = Instant::now();
    for (index, artifact) in model.files.iter().enumerate() {
        if *cancelled.borrow() {
            return Err("Download canceled".into());
        }
        let mut response = tokio::select! {
            result=client.get(&artifact.url).timeout(Duration::from_secs(1800)).send()=>result.map_err(|e|e.without_url().to_string())?.error_for_status().map_err(|e|e.without_url().to_string())?,
            _=cancelled.changed()=>return Err("Download canceled".into()),
        };
        let expected = response.content_length();
        let path = staging.join(if artifact.role == "tokenizer" {
            "tokenizer.json".to_string()
        } else {
            format!("{}.onnx", artifact.role)
        });
        let mut file = tokio::fs::File::create(&path)
            .await
            .map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut written = 0u64;
        loop {
            let chunk = tokio::select! {
                result=tokio::time::timeout(Duration::from_secs(30),response.chunk())=>result.map_err(|_|"Download stalled for 30 seconds")?.map_err(|e|e.without_url().to_string())?,
                _=cancelled.changed()=>return Err("Download canceled".into()),
            };
            let Some(chunk) = chunk else {
                break;
            };
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            hash.update(&chunk);
            written += chunk.len() as u64;
            received += chunk.len() as u64;
            if emitted.elapsed() > Duration::from_millis(200) {
                let progress = if total > 0 {
                    (received as f64 / total as f64 * 100.).min(99.)
                } else {
                    index as f64 / model.files.len() as f64 * 100.
                };
                let _=app.emit("ai-model-download-progress",json!({"modelId":model.id,"file":artifact.role,"progress":progress,"received":received,"total":total}));
                emitted = Instant::now();
            }
        }
        file.sync_all().await.map_err(|e| e.to_string())?;
        drop(file);
        if written == 0
            || expected.is_some_and(|n| n != written)
            || (artifact.size > 0 && written != artifact.size)
            || format!("{:x}", hash.finalize()) != artifact.sha256.to_lowercase()
        {
            return Err(format!(
                "{} is incomplete or failed SHA256 verification",
                artifact.role
            ));
        }
    }
    if *cancelled.borrow() {
        return Err("Download canceled".into());
    }
    tokio::fs::write(staging.join("installed.json"), model.digest())
        .await
        .map_err(|e| e.to_string())?;
    // Both directories are derived exclusively from validated IDs and content hashes.
    let backup = parent.join(format!(".backup-{}", uuid::Uuid::new_v4()));
    let had_existing = target.exists();
    if had_existing {
        tokio::fs::rename(&target, &backup)
            .await
            .map_err(|e| e.to_string())?;
    }
    if let Err(error) = tokio::fs::rename(&staging, &target).await {
        if had_existing {
            let _ = tokio::fs::rename(&backup, &target).await;
        }
        return Err(error.to_string());
    }
    if had_existing {
        let _ = tokio::fs::remove_dir_all(backup).await;
    }
    let _ = app.emit(
        "ai-model-download-progress",
        json!({"modelId":model.id,"progress":100,"received":received,"total":total}),
    );
    Ok(())
}
