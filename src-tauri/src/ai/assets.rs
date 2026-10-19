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
static CANCEL: Mutex<Option<(String, String, tokio::sync::watch::Sender<bool>)>> = Mutex::new(None);
#[derive(Default)]
struct DownloadState {
    revision: u64,
    snapshot: Option<serde_json::Value>,
}
static DOWNLOAD_STATE: Mutex<DownloadState> = Mutex::new(DownloadState {
    revision: 0,
    snapshot: None,
});
impl DownloadState {
    fn update(
        &mut self,
        job_id: &str,
        model_id: &str,
        payload: serde_json::Value,
    ) -> serde_json::Value {
        let mut next = if self.snapshot.as_ref().is_some_and(|s| s["jobId"] == job_id) {
            self.snapshot.clone().unwrap_or_default()
        } else {
            json!({"state":"downloading","phase":"connecting","progress":0,"received":0,"total":0,"file":"","message":""})
        };
        if let (Some(target), Some(fields)) = (next.as_object_mut(), payload.as_object()) {
            target.extend(fields.clone());
        }
        self.revision += 1;
        next["modelId"] = json!(model_id);
        next["jobId"] = json!(job_id);
        next["revision"] = json!(self.revision);
        self.snapshot = Some(next.clone());
        next
    }
}
pub fn download_snapshot() -> Result<Option<serde_json::Value>, String> {
    Ok(DOWNLOAD_STATE
        .lock()
        .map_err(|e| e.to_string())?
        .snapshot
        .clone())
}
fn publish_download(
    job_id: &str,
    model_id: &str,
    payload: serde_json::Value,
    emit: &impl Fn(serde_json::Value),
) {
    if let Ok(mut state) = DOWNLOAD_STATE.lock() {
        let next = state.update(job_id, model_id, payload);
        drop(state);
        emit(next);
    }
}
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
pub fn cancel(model_id: &str, job_id: Option<&str>) -> Result<(), String> {
    if let Some((id, job, tx)) = CANCEL.lock().map_err(|e| e.to_string())?.as_ref() {
        if id != model_id || job_id.is_some_and(|expected| expected != job) {
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
/// An interrupted stream can resume only when the server acknowledges the exact byte range.
/// SHA-256 remains mandatory; partial data is confined to the staging directory.
async fn stream_artifact(
    client: &reqwest::Client,
    artifact: &super::types::Artifact,
    path: &std::path::Path,
    cancelled: &mut tokio::sync::watch::Receiver<bool>,
    progress: impl Fn(&str, u64, Option<&str>, bool),
) -> Result<u64, String> {
    const ATTEMPTS: usize = 3;
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut written = 0u64;
    let mut expected = (artifact.size > 0).then_some(artifact.size);
    let mut failure = String::new();
    for attempt in 0..ATTEMPTS {
        if *cancelled.borrow() {
            return Err("Download canceled".into());
        }
        let notice = (attempt > 0).then(|| {
            format!(
                "Connection interrupted at {written} bytes; retry {attempt}/{}: {failure}",
                ATTEMPTS - 1
            )
        });
        progress("connecting", written, notice.as_deref(), false);
        if *cancelled.borrow() {
            return Err("Download canceled".into());
        }
        let mut request = artifact_request(client, &artifact.url);
        if written > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={written}-"));
        }
        let result = tokio::select! {
            result=tokio::time::timeout(Duration::from_secs(30),request.send())=>result.map_err(|_|"Response headers stalled for 30 seconds".into()).and_then(|r|r.map_err(|e|artifact_network_error(e,artifact))),
            _=cancelled.changed()=>return Err("Download canceled".into()),
        };
        let mut response = match result {
            Ok(response) => response,
            Err(error) => {
                failure = error;
                if attempt + 1 < ATTEMPTS {
                    retry_delay(cancelled, attempt).await?;
                }
                continue;
            }
        };
        let status = response.status();
        if !matches!(status.as_u16(), 200 | 206) {
            let retryable = status.is_server_error() || matches!(status.as_u16(), 408 | 429);
            let retry_after = response
                .headers()
                .contains_key(reqwest::header::RETRY_AFTER);
            failure = response
                .error_for_status()
                .err()
                .map(|e| artifact_network_error(e, artifact))
                .unwrap_or_else(|| format!("Unexpected HTTP status {status}"));
            // Do not hammer rate-limited sources or silently ignore a long Retry-After.
            if !retryable || retry_after {
                return Err(failure);
            }
            if attempt + 1 < ATTEMPTS {
                retry_delay(cancelled, attempt).await?;
            }
            continue;
        }
        if status.as_u16() == 206 {
            let range = response
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .ok_or("Missing Content-Range for resumed model download")?;
            let (start, end, total) = parse_content_range(range)?;
            if start != written
                || expected.is_some_and(|n| n != total)
                || response
                    .content_length()
                    .is_some_and(|n| n != end - start + 1)
            {
                return Err("Server returned an inconsistent model download byte range".into());
            }
            expected = Some(total);
        } else {
            if written > 0 {
                file.set_len(0).await.map_err(|e| e.to_string())?;
                use tokio::io::AsyncSeekExt;
                file.rewind().await.map_err(|e| e.to_string())?;
                written = 0;
                hash = Sha256::new();
                progress(
                    "downloading",
                    0,
                    Some("Server does not support resuming; restarting this file safely"),
                    true,
                );
            }
            if let Some(length) = response.content_length() {
                if expected.is_some_and(|n| n != length) {
                    return Err("Model source Content-Length does not match its pinned size".into());
                }
                expected = Some(length);
            }
        }
        progress("downloading", written, None, false);
        let mut emitted = Instant::now();
        let mut broken = None;
        loop {
            if *cancelled.borrow() {
                return Err("Download canceled".into());
            }
            let chunk = tokio::select! {
                result=tokio::time::timeout(Duration::from_secs(30),response.chunk())=>result.map_err(|_|"Download stalled for 30 seconds".into()).and_then(|r|r.map_err(|e|artifact_network_error(e,artifact))),
                _=cancelled.changed()=>return Err("Download canceled".into()),
            };
            let chunk = match chunk {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(error) => {
                    broken = Some(error);
                    break;
                }
            };
            let next = written
                .checked_add(chunk.len() as u64)
                .ok_or("Model download size overflow")?;
            if expected.is_some_and(|n| next > n) {
                return Err("Model download exceeded its pinned size".into());
            }
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            hash.update(&chunk);
            written = next;
            if emitted.elapsed() > Duration::from_millis(200) {
                progress("downloading", written, None, false);
                emitted = Instant::now();
            }
        }
        if broken.is_none() && expected.is_some_and(|n| n != written) {
            broken = Some("Model response ended before its declared length".into());
        }
        if let Some(error) = broken {
            // A fully received, checksum-valid payload is safe even if the connection then breaks.
            if !expected.is_some_and(|n| n == written)
                || format!("{:x}", hash.clone().finalize()) != artifact.sha256.to_lowercase()
            {
                failure = error;
                if attempt + 1 < ATTEMPTS {
                    retry_delay(cancelled, attempt).await?;
                }
                continue;
            }
        }
        if written == 0
            || expected.is_some_and(|n| n != written)
            || format!("{:x}", hash.finalize()) != artifact.sha256.to_lowercase()
        {
            return Err(format!(
                "{} is incomplete or failed SHA256 verification",
                artifact.role
            ));
        }
        if *cancelled.borrow() {
            return Err("Download canceled".into());
        }
        progress("verifying", written, None, false);
        file.sync_all().await.map_err(|e| e.to_string())?;
        return Ok(written);
    }
    Err(format!(
        "{} download failed after {ATTEMPTS} attempts ({written} bytes received): {failure}",
        artifact.role
    ))
}
fn parse_content_range(value: &str) -> Result<(u64, u64, u64), String> {
    let (unit, value) = value
        .split_once(' ')
        .ok_or("Invalid model Content-Range unit")?;
    if !unit.eq_ignore_ascii_case("bytes") {
        return Err("Invalid model Content-Range unit".into());
    }
    let (range, total) = value
        .split_once('/')
        .ok_or("Invalid model Content-Range total")?;
    let (start, end) = range
        .split_once('-')
        .ok_or("Invalid model Content-Range bounds")?;
    let start = start
        .parse::<u64>()
        .map_err(|_| "Invalid model byte range start")?;
    let end = end
        .parse::<u64>()
        .map_err(|_| "Invalid model byte range end")?;
    let total = total
        .parse::<u64>()
        .map_err(|_| "Invalid model byte range size")?;
    if start > end || end >= total || total == 0 {
        return Err("Invalid model Content-Range bounds".into());
    }
    Ok((start, end, total))
}
async fn retry_delay(
    cancelled: &mut tokio::sync::watch::Receiver<bool>,
    attempt: usize,
) -> Result<(), String> {
    if *cancelled.borrow() {
        return Err("Download canceled".into());
    }
    tokio::select! {
        _=tokio::time::sleep(Duration::from_millis(500*(attempt as u64+1)))=>Ok(()),
        _=cancelled.changed()=>Err("Download canceled".into()),
    }
}
pub async fn download(
    app: AppHandle,
    model: ModelDefinition,
    request_id: Option<String>,
) -> Result<(), String> {
    model.validate()?;
    let client = crate::t_network::client()?;
    download_with_id(model, client, request_id, move |payload| {
        let _ = app.emit("ai-model-download-progress", payload);
    })
    .await
}
#[cfg(test)]
async fn download_with_client(
    model: ModelDefinition,
    client: reqwest::Client,
    emit: impl Fn(serde_json::Value) + Send + Sync,
) -> Result<(), String> {
    download_with_id(model, client, None, emit).await
}
async fn download_with_id(
    model: ModelDefinition,
    client: reqwest::Client,
    request_id: Option<String>,
    emit: impl Fn(serde_json::Value) + Send + Sync,
) -> Result<(), String> {
    model.validate()?;
    if model.adapter == Adapter::JinaEmbeddings {
        return Err("Online API models do not have local artifacts".into());
    }
    let _lock = DOWNLOAD_LOCK
        .try_lock()
        .map_err(|_| "Another model download is already running")?;
    let job_id = match request_id {
        Some(id) => uuid::Uuid::parse_str(&id)
            .map_err(|_| "Invalid model download request ID")?
            .to_string(),
        None => uuid::Uuid::new_v4().to_string(),
    };
    let (tx, mut cancelled) = tokio::sync::watch::channel(false);
    *CANCEL.lock().map_err(|e| e.to_string())? = Some((model.id.clone(), job_id.clone(), tx));
    let _cancel_guard = DownloadCancellation;
    publish_download(&job_id, &model.id, json!({}), &emit);
    let result = install_with_client(&model, &client, &mut cancelled, |payload| {
        publish_download(&job_id, &model.id, payload, &emit);
    })
    .await;
    // Publish the terminal snapshot before releasing DOWNLOAD_LOCK; an older job must
    // never overwrite a newer job's snapshot when a settings window is recreated.
    let terminal = match &result {
        Ok(()) => json!({"state":"completed","phase":"complete","progress":100,"message":""}),
        Err(error) if matches!(error.as_str(), "Download canceled" | "Download cancelled") => {
            json!({"state":"cancelled","message":""})
        }
        Err(error) => json!({"state":"failed","message":error}),
    };
    publish_download(&job_id, &model.id, terminal, &emit);
    result
}
async fn install_with_client(
    model: &ModelDefinition,
    client: &reqwest::Client,
    cancelled: &mut tokio::sync::watch::Receiver<bool>,
    emit: impl Fn(serde_json::Value) + Send + Sync,
) -> Result<(), String> {
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
    let total: u64 = model.files.iter().try_fold(0u64, |bytes, file| {
        bytes
            .checked_add(file.size)
            .ok_or("Model download total size overflow")
    })?;
    let mut received = 0u64;
    for (index, artifact) in model.files.iter().enumerate() {
        let completed = received;
        let path = staging.join(artifact_filename(&artifact.role));
        received+=stream_artifact(client,artifact,&path,cancelled,|phase,written,message,restart| {
            let bytes=completed+written;
            let progress=if total>0 {(bytes as f64/total as f64*100.).min(99.)}else{index as f64/model.files.len() as f64*100.};
            emit(json!({"modelId":model.id,"file":artifact.role,"phase":phase,"progress":progress,"received":bytes,"total":total,"message":message.unwrap_or(""),"restart":restart}));
        }).await?;
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
    #[test]
    fn download_cancellation_rejects_an_old_job_for_the_same_model() {
        let _lock = DOWNLOAD_LOCK.try_lock().unwrap();
        let (tx, rx) = tokio::sync::watch::channel(false);
        *CANCEL.lock().unwrap() = Some(("model".into(), "new-job".into(), tx));
        let _guard = DownloadCancellation;
        assert!(cancel("model", Some("old-job")).is_err());
        assert!(cancel("other-model", Some("new-job")).is_err());
        assert!(!*rx.borrow());
        cancel("model", Some("new-job")).unwrap();
        assert!(*rx.borrow());
    }
    #[test]
    fn download_snapshots_preserve_terminal_details_and_reset_for_a_new_job() {
        let mut state = DownloadState::default();
        let first = state.update(
            "job-a",
            "model-a",
            json!({"progress":42,"received":42,"total":100,"phase":"downloading"}),
        );
        let failed = state.update(
            "job-a",
            "model-a",
            json!({"state":"failed","message":"HTTP 403"}),
        );
        assert!(failed["revision"].as_u64().unwrap() > first["revision"].as_u64().unwrap());
        assert_eq!(failed["received"], 42);
        assert_eq!(failed["state"], "failed");
        assert_eq!(state.snapshot, Some(failed.clone()));
        let next = state.update("job-b", "model-a", json!({}));
        assert_eq!(next["progress"], 0);
        assert_eq!(next["message"], "");
        assert_eq!(next["state"], "downloading");
        assert!(next["revision"].as_u64().unwrap() > failed["revision"].as_u64().unwrap());
    }
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

#[cfg(test)]
mod resumable_download_tests {
    use super::*;
    use std::io::{Read, Write};
    fn server(responses: Vec<String>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model.onnx", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let thread = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let deadline = Instant::now() + Duration::from_secs(8);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if Instant::now() >= deadline {
                                return requests;
                            }
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("test server: {error}"),
                    }
                };
                // Windows accepted sockets may inherit FIONBIO from the listener.
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 2048];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut buffer).unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..n]);
                    assert!(request.len() < 16384);
                }
                requests.push(String::from_utf8_lossy(&request).to_lowercase());
                socket.write_all(response.as_bytes()).unwrap();
            }
            requests
        });
        (url, thread)
    }
    fn artifact(url: String) -> super::super::types::Artifact {
        super::super::types::Artifact {
            role: "detector".into(),
            url,
            sha256: format!("{:x}", Sha256::digest(b"weights")),
            size: 7,
        }
    }
    fn path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("model-download-test-{}.part", uuid::Uuid::new_v4()))
    }
    fn full(body: &str) -> String {
        format!("HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\n{body}")
    }
    #[tokio::test]
    async fn interrupted_body_resumes_the_exact_range_without_restarting_or_skipping_checksum() {
        for unknown_size in [false, true] {
            let (url,server)=server(vec![full("wei"),"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Range: bytes 3-6/7\r\nConnection: close\r\n\r\nghts".into()]);
            let mut artifact = artifact(url);
            if unknown_size {
                artifact.size = 0;
            }
            let p = path();
            let (_tx, mut rx) = tokio::sync::watch::channel(false);
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            let bytes = stream_artifact(&client, &artifact, &p, &mut rx, |_, _, _, _| {})
                .await
                .unwrap();
            assert_eq!(bytes, 7);
            assert_eq!(std::fs::read(&p).unwrap(), b"weights");
            let requests = server.join().unwrap();
            assert_eq!(requests.len(), 2);
            assert!(requests[1].contains("range: bytes=3-"));
            assert!(requests[1].contains("user-agent: lap/"));
            std::fs::remove_file(p).unwrap();
        }
    }
    #[tokio::test]
    async fn ignored_range_restarts_safely_and_emits_an_explicit_progress_reset() {
        let (url, server) = server(vec![full("wei"), full("weights")]);
        let p = path();
        let (_tx, mut rx) = tokio::sync::watch::channel(false);
        let restarted = std::sync::atomic::AtomicBool::new(false);
        stream_artifact(
            &reqwest::Client::builder().no_proxy().build().unwrap(),
            &artifact(url),
            &p,
            &mut rx,
            |_, n, _, reset| {
                if reset {
                    assert_eq!(n, 0);
                    restarted.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            },
        )
        .await
        .unwrap();
        assert!(restarted.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(std::fs::read(&p).unwrap(), b"weights");
        assert_eq!(server.join().unwrap().len(), 2);
        std::fs::remove_file(p).unwrap();
    }
    #[tokio::test]
    async fn invalid_resume_ranges_and_wrong_checksums_are_never_accepted() {
        for responses in [
            vec![full("wei"),"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Range: bytes 2-5/7\r\nConnection: close\r\n\r\nghts".into()],
            vec![full("weighTs")],
            vec!["HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()],
            vec!["HTTP/1.1 503 Service Unavailable\r\nRetry-After: Sat, 17 Oct 2026 12:00:00 GMT\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()],
        ] {
            let expected=responses.len();let (url,server)=server(responses);let p=path();let (_tx,mut rx)=tokio::sync::watch::channel(false);
            let result=stream_artifact(&reqwest::Client::builder().no_proxy().build().unwrap(),&artifact(url),&p,&mut rx,|_,_,_,_|{}).await;
            assert!(result.is_err());assert_eq!(server.join().unwrap().len(),expected);std::fs::remove_file(p).unwrap();
        }
        for invalid in ["items 0-6/7", "bytes 3-2/7", "bytes 0-7/7", "bytes 0-0/*"] {
            assert!(parse_content_range(invalid).is_err());
        }
    }
    #[tokio::test]
    async fn cancellation_before_retry_does_not_connect_again_or_mark_a_partial_file_complete() {
        let (url, server) = server(vec![full("wei")]);
        let p = path();
        let (tx, mut rx) = tokio::sync::watch::channel(false);
        let result = stream_artifact(
            &reqwest::Client::builder().no_proxy().build().unwrap(),
            &artifact(url),
            &p,
            &mut rx,
            |phase, _, message, _| {
                if phase == "connecting" && message.is_some() {
                    tx.send(true).unwrap();
                }
            },
        )
        .await;
        assert_eq!(result.unwrap_err(), "Download canceled");
        assert_eq!(server.join().unwrap().len(), 1);
        assert_eq!(std::fs::read(&p).unwrap(), b"wei");
        std::fs::remove_file(p).unwrap();
    }
}
