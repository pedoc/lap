use crate::{t_config, t_network};
use serde::Serialize;
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

static DOWNLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const FFMPEG_RELEASE: &str = "https://github.com/julyx10/lap-binaries/releases/download/ffmpeg-8.1";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceStatus {
    pub ffmpeg_ready: bool,
    pub ffmpeg_files: Vec<String>,
}

fn app_resource_dir(name: &str) -> Result<PathBuf, String> {
    Ok(t_config::get_app_data_dir()?.join("resources").join(name))
}

fn bundled_dir(app: &AppHandle, name: &str) -> Option<PathBuf> {
    app.path().resource_dir().ok().map(|path| path.join(name))
}

fn has_payload(path: &std::path::Path) -> bool {
    fs::metadata(path).map(|metadata| metadata.is_file() && metadata.len() > 0).unwrap_or(false)
}

pub fn ffmpeg_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let managed = app_resource_dir("ffmpeg")?;
    if managed.exists() {
        return Ok(managed);
    }
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/ffmpeg");
        if dev.exists() {
            return Ok(dev);
        }
    }
    Ok(bundled_dir(app, "ffmpeg").unwrap_or(managed))
}

fn ffmpeg_filenames() -> Result<[String; 2], String> {
    let triple = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        (os, arch) => return Err(format!("FFmpeg is not available for {os}/{arch}")),
    };
    let ext = if cfg!(windows) { ".exe" } else { "" };
    Ok([
        format!("ffmpeg-{triple}{ext}"),
        format!("ffprobe-{triple}{ext}"),
    ])
}

pub fn status(app: &AppHandle) -> ResourceStatus {
    let ffmpeg_files = ffmpeg_filenames()
        .ok()
        .map(|names| {
            let dir = ffmpeg_dir(app).ok();
            names
                .into_iter()
                .filter(|name| dir.as_ref().is_some_and(|path| has_payload(&path.join(name))))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ResourceStatus {
        ffmpeg_ready: ffmpeg_filenames()
            .ok()
            .is_some_and(|names| ffmpeg_files.len() == names.len()),
        ffmpeg_files,
    }
}

async fn download_file(
    app: &AppHandle,
    client: &reqwest::Client,
    url: &str,
    destination: &PathBuf,
    label: &str,
    base: u64,
    total: u64,
    completed: usize,
    file_count: usize,
) -> Result<u64, String> {
    if has_payload(destination) {
        return Ok(fs::metadata(destination).map(|m| m.len()).unwrap_or(0));
    }
    let parent = destination.parent().ok_or("Invalid resource path")?;
    if destination.exists() {
        tokio::fs::remove_file(destination).await.map_err(|e| e.to_string())?;
    }
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| e.to_string())?;
    let temp = destination.with_extension(format!("{}.download", std::process::id()));
    let _ = tokio::fs::remove_file(&temp).await;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download {label} failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Download {label} failed: {e}"))?;
    let expected = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&temp)
        .await
        .map_err(|e| e.to_string())?;
    let mut downloaded = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Download {label} failed: {e}"))?
    {
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        let progress = if total > 0 {
            (((base + downloaded) as f64 / total as f64) * 100.0).min(99.0) as u8
        } else {
            (((completed as f64
                + if expected > 0 {
                    downloaded as f64 / expected as f64
                } else {
                    0.0
                })
                / file_count as f64)
                * 100.0)
                .min(99.0) as u8
        };
        let _ = app.emit("app_resources_download_progress", serde_json::json!({"progress": progress, "file": label, "downloadedBytes": base + downloaded, "totalBytes": total}));
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    if downloaded == 0 || (expected > 0 && downloaded != expected) {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(format!("Downloaded {label} is incomplete"));
    }
    tokio::fs::rename(&temp, destination)
        .await
        .map_err(|e| format!("Could not install {label}: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(destination, fs::Permissions::from_mode(0o755));
    }
    Ok(downloaded)
}

pub async fn download(app: AppHandle, kind: String) -> Result<(), String> {
    if kind != "ffmpeg" {
        return Err("Unknown resource type".into());
    }
    let _guard = DOWNLOAD_LOCK.lock().await;
    let client = t_network::client()?;
    let mut downloads = Vec::new();
    let dir = app_resource_dir("ffmpeg")?;
    for name in ffmpeg_filenames()? {
        if !has_payload(&dir.join(&name)) {
            downloads.push((format!("{FFMPEG_RELEASE}/{name}"), dir.join(&name), name));
        }
    }
    if downloads.is_empty() {
        return Ok(());
    }
    let mut total = 0u64;
    for (url, _, _) in &downloads {
        if let Ok(response) = client
            .get(url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send()
            .await
        {
            total += response
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.rsplit_once('/'))
                .and_then(|(_, n)| n.parse().ok())
                .or_else(|| response.content_length())
                .unwrap_or(0);
        }
    }
    let mut completed_bytes = 0;
    let file_count = downloads.len();
    for (index, (url, path, name)) in downloads.into_iter().enumerate() {
        completed_bytes += download_file(
            &app,
            &client,
            &url,
            &path,
            &name,
            completed_bytes,
            total,
            index,
            file_count,
        )
        .await?;
    }
    let _ = app.emit("app_resources_download_progress", serde_json::json!({"progress": 100, "file": "complete", "downloadedBytes": completed_bytes, "totalBytes": total}));
    Ok(())
}
