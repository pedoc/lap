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
/// Opening an empty directory does not mark a model as installed.
pub fn prepare_directory(model: &ModelDefinition) -> Result<PathBuf, String> {
    let path = directory(model)?;
    prepare_directory_at(model, &path)?;
    Ok(path)
}
fn prepare_directory_at(model: &ModelDefinition, path: &std::path::Path) -> Result<(), String> {
    model.validate()?;
    if model.adapter == Adapter::JinaEmbeddings {
        return Err("Online API models do not have a local model directory".into());
    }
    std::fs::create_dir_all(path).map_err(|e| format!("Could not create model directory: {e}"))
}
pub fn artifact_path(model: &ModelDefinition, role: &str) -> Result<PathBuf, String> {
    if !model.files.iter().any(|f| f.role == role) {
        return Err(format!("Unknown artifact role: {role}"));
    }
    Ok(directory(model)?.join(artifact_filename(role)))
}
fn artifact_filename(role: &str) -> String {
    match role {
        "tokenizer" => "tokenizer.json".into(),
        "charset" => "charset.txt".into(),
        _ => format!("{role}.onnx"),
    }
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
const DOWNLOAD_USER_AGENT: &str = concat!("Lap/", env!("CARGO_PKG_VERSION"));
fn artifact_request(client: &reqwest::Client, url: &str) -> reqwest::RequestBuilder {
    client
        .get(url)
        .header(reqwest::header::USER_AGENT, DOWNLOAD_USER_AGENT)
        .timeout(Duration::from_secs(1800))
}
fn artifact_network_error(error: reqwest::Error, artifact: &super::types::Artifact) -> String {
    let host = reqwest::Url::parse(&artifact.url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "download source".into());
    format!(
        "{} from {}: {}",
        artifact.role,
        host,
        crate::t_network::request_error(error, &artifact.url)
    )
}
pub async fn download(app: AppHandle, model: ModelDefinition) -> Result<(), String> {
    model.validate()?;
    let client = crate::t_network::client()?;
    download_with_client(model, client, move |payload| {
        let _ = app.emit("ai-model-download-progress", payload);
    })
    .await
}
async fn download_with_client(
    model: ModelDefinition,
    client: reqwest::Client,
    emit: impl Fn(serde_json::Value) + Send + Sync,
) -> Result<(), String> {
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
    let total: u64 = model.files.iter().map(|f| f.size).sum();
    let mut received = 0u64;
    let mut emitted = Instant::now();
    for (index, artifact) in model.files.iter().enumerate() {
        if *cancelled.borrow() {
            return Err("Download canceled".into());
        }
        emit(
            json!({"modelId":model.id,"file":artifact.role,"phase":"connecting","progress":if total>0 {received as f64/total as f64*100.}else{0.},"received":received,"total":total}),
        );
        let mut response = tokio::select! {
            result=artifact_request(&client,&artifact.url).send()=>result.map_err(|e|artifact_network_error(e,artifact))?.error_for_status().map_err(|e|artifact_network_error(e,artifact))?,
            _=cancelled.changed()=>return Err("Download canceled".into()),
        };
        emit(
            json!({"modelId":model.id,"file":artifact.role,"phase":"downloading","progress":if total>0 {received as f64/total as f64*100.}else{0.},"received":received,"total":total}),
        );
        let expected = response.content_length();
        let path = staging.join(artifact_filename(&artifact.role));
        let mut file = tokio::fs::File::create(&path)
            .await
            .map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut written = 0u64;
        loop {
            let chunk = tokio::select! {
                result=tokio::time::timeout(Duration::from_secs(30),response.chunk())=>result.map_err(|_|format!("{}: Download stalled for 30 seconds",artifact.role))?.map_err(|e|artifact_network_error(e,artifact))?,
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
                emit(
                    json!({"modelId":model.id,"file":artifact.role,"phase":"downloading","progress":progress,"received":received,"total":total}),
                );
                emitted = Instant::now();
            }
        }
        emit(
            json!({"modelId":model.id,"file":artifact.role,"phase":"verifying","progress":if total>0 {(received as f64/total as f64*100.).min(99.)}else{0.},"received":received,"total":total}),
        );
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
    emit(
        json!({"modelId":model.id,"phase":"installing","progress":99,"received":received,"total":total}),
    );
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
    emit(
        json!({"modelId":model.id,"phase":"complete","progress":100,"received":received,"total":total}),
    );
    Ok(())
}

#[cfg(test)]
mod directory_tests {
    use super::*;
    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!("lap-model-directory-test-{}", uuid::Uuid::new_v4()))
    }
    #[test]
    fn missing_local_directory_is_created_without_an_installation_receipt() {
        let model = crate::ai::settings::builtins()
            .into_iter()
            .find(|m| m.id == "buffalo-s")
            .unwrap();
        let path = temp_path();
        assert!(!path.exists());
        prepare_directory_at(&model, &path).unwrap();
        assert!(path.is_dir());
        assert!(!path.join("installed.json").exists());
        prepare_directory_at(&model, &path).unwrap();
        std::fs::remove_dir(path).unwrap();
    }
    #[test]
    fn online_models_do_not_create_local_directories() {
        let model = crate::ai::settings::builtins()
            .into_iter()
            .find(|m| m.adapter == Adapter::JinaEmbeddings)
            .unwrap();
        let path = temp_path();
        assert!(prepare_directory_at(&model, &path).is_err());
        assert!(!path.exists());
    }
    #[test]
    fn invalid_model_definitions_do_not_create_directories() {
        let mut model = crate::ai::settings::builtins()
            .into_iter()
            .find(|m| m.id == "buffalo-s")
            .unwrap();
        model.id = "../outside".into();
        let path = temp_path();
        assert!(prepare_directory_at(&model, &path).is_err());
        assert!(!path.exists());
    }
}

#[cfg(test)]
mod ocr_artifact_tests {
    #[test]
    fn dictionary_download_and_runtime_use_the_same_name() {
        assert_eq!(super::artifact_filename("charset"), "charset.txt");
        assert_eq!(super::artifact_filename("tokenizer"), "tokenizer.json");
        assert_eq!(
            super::artifact_filename("ocr_detector"),
            "ocr_detector.onnx"
        );
    }
}
#[cfg(test)]
mod download_transport_tests {
    use super::*;
    use std::io::{Read, Write};
    fn read_headers(stream: &mut std::net::TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 1024];
        while !bytes.ends_with(b"\r\n\r\n") {
            let n = stream.read(&mut buffer).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buffer[..n]);
            assert!(bytes.len() < 16384);
        }
        String::from_utf8(bytes).unwrap()
    }
    #[tokio::test]
    async fn branded_user_agent_survives_cross_origin_download_redirects() {
        let first = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let second = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let initial = format!("http://{}/model", first.local_addr().unwrap());
        let next = format!("http://{}/weights", second.local_addr().unwrap());
        let a = std::thread::spawn(move || {
            let (mut stream, _) = first.accept().unwrap();
            let headers = read_headers(&mut stream);
            assert!(headers.to_ascii_lowercase().contains(&format!(
                "user-agent: {}",
                DOWNLOAD_USER_AGENT.to_ascii_lowercase()
            )));
            write!(stream,"HTTP/1.1 302 Found\r\nLocation: {next}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let b = std::thread::spawn(move || {
            let (mut stream, _) = second.accept().unwrap();
            let headers = read_headers(&mut stream);
            assert!(headers.to_ascii_lowercase().contains(&format!(
                "user-agent: {}",
                DOWNLOAD_USER_AGENT.to_ascii_lowercase()
            )));
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nweights",
                )
                .unwrap();
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let response = artifact_request(&client, &initial)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .unwrap();
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"weights");
        a.join().unwrap();
        b.join().unwrap();
    }
    #[test]
    fn download_requests_identify_the_application_without_altering_the_source() {
        let client = reqwest::Client::new();
        let url = "https://example.invalid/model.onnx";
        let request = artifact_request(&client, url).build().unwrap();
        assert_eq!(
            request.headers()[reqwest::header::USER_AGENT],
            DOWNLOAD_USER_AGENT
        );
        assert_eq!(request.url().as_str(), url);
    }
    #[tokio::test]
    #[ignore = "Requires live model source, configured proxy if needed, and a workspace-only LAP_AI_TEST_MODEL_ROOT"]
    async fn actual_model_download_installs_with_the_application_transport() {
        let root = PathBuf::from(
            std::env::var_os("LAP_AI_TEST_MODEL_ROOT").expect("explicit workspace model root"),
        );
        let workspace_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .canonicalize()
            .unwrap();
        assert!(
            root.canonicalize().unwrap().starts_with(workspace_target),
            "Never write test models outside the workspace target directory"
        );
        let id = std::env::var("LAP_MODEL_DOWNLOAD_ID")
            .unwrap_or_else(|_| "rapidocr-pp-ocrv5-server".into());
        let model = crate::ai::settings::builtins()
            .into_iter()
            .find(|m| m.id == id)
            .unwrap();
        let proxy = std::env::var("LAP_MODEL_DOWNLOAD_PROXY").ok();
        let client = crate::t_network::client_with_proxy(proxy.as_deref()).unwrap();
        let events = std::sync::Mutex::new(Vec::new());
        download_with_client(model.clone(), client, |event| {
            events.lock().unwrap().push(event);
        })
        .await
        .unwrap();
        assert!(installed(&model));
        verify(&model).unwrap();
        let events = events.lock().unwrap();
        for phase in [
            "connecting",
            "downloading",
            "verifying",
            "installing",
            "complete",
        ] {
            assert!(
                events.iter().any(|e| e["phase"] == phase),
                "missing phase: {phase}"
            );
        }
        assert_eq!(events.last().unwrap()["progress"], 100);
        assert_eq!(
            events.last().unwrap()["received"].as_u64().unwrap(),
            model.files.iter().map(|f| f.size).sum::<u64>()
        );
        println!(
            "{}: downloaded and installed {} checksum-verified files ({} bytes); phase events={}",
            model.id,
            model.files.len(),
            model.files.iter().map(|f| f.size).sum::<u64>(),
            events.len()
        );
    }
}
