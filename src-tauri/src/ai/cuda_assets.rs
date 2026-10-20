//! Checksum-pinned optional CUDA component. Never installs drivers or changes PATH.
use super::types::Artifact;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{AppHandle, Emitter};
#[derive(Clone, Deserialize)]
struct Package {
    role: String,
    package: String,
    version: String,
    prefix: String,
    url: String,
    sha256: String,
    size: u64,
}
#[derive(Deserialize)]
struct Catalog {
    id: String,
    packages: Vec<Package>,
}
fn catalog() -> Result<Catalog, String> {
    let catalog: Catalog =
        serde_json::from_str(include_str!("cuda-runtime.json")).map_err(|e| e.to_string())?;
    if !valid_name(&catalog.id) || catalog.packages.is_empty() {
        return Err("Invalid CUDA runtime catalog".into());
    }
    Ok(catalog)
}
fn digest() -> String {
    format!("{:x}", Sha256::digest(include_bytes!("cuda-runtime.json")))
}
fn base() -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(path) = std::env::var_os("LAP_CUDA_TEST_ROOT") {
        let root = PathBuf::from(path)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !root.starts_with(target) {
            return Err("CUDA fixtures must remain inside the workspace target directory".into());
        }
        return Ok(root);
    }
    Ok(crate::t_config::get_app_data_dir()?.join("runtimes/cuda"))
}
fn root() -> Result<PathBuf, String> {
    Ok(base()?.join(digest()))
}
const REQUIRED: &[&str] = &[
    "onnxruntime.dll",
    "onnxruntime_providers_shared.dll",
    "onnxruntime_providers_cuda.dll",
    "cublas64_12.dll",
    "cublasLt64_12.dll",
    "cudart64_12.dll",
    "cufft64_11.dll",
    "curand64_10.dll",
    "cudnn64_9.dll",
    "nvrtc64_120_0.dll",
    "nvrtc-builtins64_128.dll",
    "cudnn_graph64_9.dll",
    "cudnn_ops64_9.dll",
    "cudnn_adv64_9.dll",
    "cudnn_cnn64_9.dll",
    "cudnn_engines_precompiled64_9.dll",
    "cudnn_engines_runtime_compiled64_9.dll",
    "cudnn_heuristic64_9.dll",
];
#[derive(Serialize, Deserialize)]
struct Receipt {
    digest: String,
    files: Vec<(String, u64, String)>,
}
fn installed_at(path: &Path) -> bool {
    let receipt = std::fs::read(path.join("installed.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Receipt>(&b).ok());
    receipt.is_some_and(|receipt| {
        receipt.digest == digest()
            && REQUIRED
                .iter()
                .all(|name| receipt.files.iter().any(|(n, _, _)| n == name))
            && receipt.files.iter().all(|(name, size, sha)| {
                valid_name(name)
                    && sha.len() == 64
                    && std::fs::metadata(path.join(name))
                        .is_ok_and(|m| m.is_file() && m.len() == *size)
            })
    })
}
pub fn installed_root() -> Result<PathBuf, String> {
    let root = root()?;
    if !installed_at(&root) {
        return Err(
            "CUDA runtime is not installed. Download it in Settings → AI models → CUDA runtime"
                .into(),
        );
    }
    // Recheck code integrity on first load or after any file/receipt change.
    // UI status uses cheap metadata checks; large runtime libraries are not
    // rehashed on every configuration refresh.
    static VERIFIED: Mutex<Option<String>> = Mutex::new(None);
    let bytes = std::fs::read(root.join("installed.json")).map_err(|e| e.to_string())?;
    let receipt: Receipt = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let mut key = format!("{}:{:x}", root.display(), Sha256::digest(&bytes));
    for (name, _, _) in &receipt.files {
        let metadata = std::fs::metadata(root.join(name)).map_err(|e| e.to_string())?;
        key.push_str(&format!(
            "{name}:{}:{:?}",
            metadata.len(),
            metadata.modified().ok()
        ));
    }
    let mut verified = VERIFIED.lock().map_err(|e| e.to_string())?;
    if verified.as_deref() != Some(&key) {
        for (name, _, sha) in receipt.files {
            if checksum(&root.join(&name))? != sha {
                return Err(format!(
                    "CUDA library failed integrity verification: {name}. Reinstall the CUDA component"
                ));
            }
        }
        *verified = Some(key);
    }
    Ok(root)
}
static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static CANCEL: Mutex<Option<(String, tokio::sync::watch::Sender<bool>)>> = Mutex::new(None);
struct DownloadState {
    revision: u64,
    snapshot: Option<Value>,
}
static DOWNLOAD: Mutex<DownloadState> = Mutex::new(DownloadState {
    revision: 0,
    snapshot: None,
});
static PROBE: Mutex<Option<Result<std::collections::BTreeMap<String, u64>, String>>> =
    Mutex::new(None);
static CHECKING: AtomicBool = AtomicBool::new(false);
struct CheckingGuard;
impl CheckingGuard {
    fn begin() -> Result<Self, String> {
        CHECKING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "A CUDA verification is already running".to_string())?;
        Ok(Self)
    }
}
impl Drop for CheckingGuard {
    fn drop(&mut self) {
        CHECKING.store(false, Ordering::SeqCst);
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub supported: bool,
    pub state: String,
    pub reason: String,
    pub installed: bool,
    pub checking: bool,
    pub total_bytes: u64,
    pub download: Option<Value>,
    pub devices: Value,
    pub operators: std::collections::BTreeMap<String, u64>,
}
fn ensure_space(path: &Path, needed: u64) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetDiskFreeSpaceExW(
                path: *const u16,
                available: *mut u64,
                total: *mut u64,
                free: *mut u64,
            ) -> i32;
        }
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut available = 0;
        if unsafe {
            GetDiskFreeSpaceExW(
                path.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(format!(
                "Cannot check CUDA installation disk space: {}",
                std::io::Error::last_os_error()
            ));
        }
        if available < needed {
            return Err(format!(
                "Not enough free space for CUDA runtime: approximately {} MiB required, {} MiB available",
                needed / 1048576,
                available / 1048576
            ));
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (path, needed);
    Ok(())
}
pub fn status() -> Status {
    let supported = cfg!(all(target_os = "windows", target_arch = "x86_64"));
    let installed = root().is_ok_and(|p| installed_at(&p));
    let total_bytes = catalog()
        .map(|c| c.packages.iter().map(|p| p.size).sum())
        .unwrap_or(0);
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    let (state, reason, devices);
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    let (state, reason, devices) = (
        "unsupported",
        "The downloadable CUDA component supports Windows x64".to_string(),
        json!([]),
    );
    let mut operators = std::collections::BTreeMap::new();
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        let driver = super::cuda::driver();
        devices = json!(driver.devices);
        if !driver.available {
            state = "driver_missing";
            reason = driver.reason;
        } else if super::cuda::bridge_path().is_none() {
            state = "bridge_missing";
            reason = "This Lap package is missing its optional CUDA bridge; rebuild/reinstall Lap"
                .into();
        } else if !installed {
            state = "not_installed";
            reason =
                "Download the optional CUDA 12 / cuDNN 9 runtime; no full CUDA Toolkit is required"
                    .into();
        } else {
            let probe = PROBE.lock().ok().and_then(|p| p.clone());
            match probe {
                Some(Err(error)) => {
                    state = "failed";
                    reason = error;
                }
                Some(Ok(report)) => {
                    state = "ready";
                    reason = String::new();
                    operators = report;
                }
                None => {
                    state = "installed";
                    reason =
                        "Runtime files installed; run the CUDA check or test your chosen model"
                            .into();
                }
            }
        }
    }
    let checking = CHECKING.load(Ordering::SeqCst);
    Status {
        supported,
        state: if checking {
            "checking".into()
        } else {
            state.into()
        },
        reason,
        installed,
        checking,
        total_bytes,
        download: DOWNLOAD.lock().ok().and_then(|s| s.snapshot.clone()),
        devices,
        operators,
    }
}
fn publish(app: &AppHandle, job: &str, payload: Value) {
    if let Ok(mut state) = DOWNLOAD.lock() {
        let mut next = if state.snapshot.as_ref().is_some_and(|p| p["jobId"] == job) {
            state.snapshot.clone().unwrap_or_default()
        } else {
            json!({"modelId":"cuda-runtime","state":"downloading","phase":"connecting","progress":0,"received":0,"total":0,"message":""})
        };
        if let (Some(dst), Some(src)) = (next.as_object_mut(), payload.as_object()) {
            dst.extend(src.clone());
        }
        state.revision += 1;
        next["jobId"] = json!(job);
        next["revision"] = json!(state.revision);
        state.snapshot = Some(next.clone());
        drop(state);
        let _ = app.emit("ai-cuda-runtime-progress", next);
    }
}
fn cancelled(receiver: &tokio::sync::watch::Receiver<bool>) -> Result<(), String> {
    if *receiver.borrow() {
        Err("Download canceled".into())
    } else {
        Ok(())
    }
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() < 180
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        && name != "."
        && name != ".."
}
fn checksum(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn extract(
    path: &Path,
    target: &Path,
    package: &Package,
    seen: &mut BTreeSet<String>,
    receiver: &tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if zip.len() > 20000 {
        return Err("CUDA archive contains too many files".into());
    }
    let mut total = 0u64;
    for index in 0..zip.len() {
        cancelled(receiver)?;
        let mut file = zip.by_index(index).map_err(|e| e.to_string())?;
        let source = file.name().to_string();
        if file.enclosed_name().is_none()
            || source.contains('\\')
            || source.contains(':')
            || file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err("Unsafe path in CUDA archive".into());
        }
        if file.is_dir() {
            continue;
        }
        let Some(name) = source.strip_prefix(&package.prefix) else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with(".dll") {
            continue;
        }
        if package.role == "onnxruntime" && !REQUIRED[..3].contains(&name) {
            continue;
        }
        if !valid_name(name) || !seen.insert(name.to_ascii_lowercase()) {
            return Err("Unsafe or duplicate CUDA library name".into());
        }
        total = total
            .checked_add(file.size())
            .ok_or("CUDA archive size overflow")?;
        if file.size() > 1024 * 1024 * 1024 || total > 2 * 1024 * 1024 * 1024 {
            return Err("CUDA archive exceeds extraction limit".into());
        }
        let mut out = std::fs::File::create(target.join(name)).map_err(|e| e.to_string())?;
        let mut written = 0u64;
        let mut buffer = [0u8; 1024 * 64];
        loop {
            cancelled(receiver)?;
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            written += n as u64;
            if written > file.size() {
                return Err("CUDA archive expanded beyond declared size".into());
            }
            out.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        }
        if written != file.size() {
            return Err("CUDA library was truncated".into());
        }
    }
    // Keep package identities and license files from the original, verified archives.
    std::fs::create_dir_all(target.join("licenses")).map_err(|e| e.to_string())?;
    for index in 0..zip.len() {
        let mut file = zip.by_index(index).map_err(|e| e.to_string())?;
        let name = file.name().to_ascii_lowercase();
        if !file.is_dir()
            && (name.contains("license") || name.ends_with("notice"))
            && file.size() < 1024 * 1024
        {
            let mut text = Vec::new();
            file.read_to_end(&mut text).map_err(|e| e.to_string())?;
            std::fs::write(
                target
                    .join("licenses")
                    .join(format!("{}-{index}.txt", package.role)),
                text,
            )
            .map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(
        target
            .join("licenses")
            .join(format!("{}.txt", package.role)),
        format!(
            "{} {}\nSHA-256: {}\n",
            package.package, package.version, package.sha256
        ),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct CancelGuard;
impl Drop for CancelGuard {
    fn drop(&mut self) {
        if let Ok(mut c) = CANCEL.lock() {
            *c = None;
        }
    }
}
async fn install(
    app: &AppHandle,
    job: &str,
    receiver: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    let spec = catalog()?;
    let base = base()?;
    tokio::fs::create_dir_all(&base)
        .await
        .map_err(|e| e.to_string())?;
    let destination = root()?;
    let existing = destination.clone();
    if tauri::async_runtime::spawn_blocking(move || {
        installed_at(&existing) && installed_root().is_ok()
    })
    .await
    .map_err(|e| e.to_string())?
    {
        return Ok(());
    }
    let stage = base.join(format!(".install-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir(&stage)
        .await
        .map_err(|e| e.to_string())?;
    let _stage = Stage(stage.clone());
    let cache = base.join("downloads");
    tokio::fs::create_dir_all(&cache)
        .await
        .map_err(|e| e.to_string())?;
    let client = crate::t_network::client()?;
    let total: u64 = spec.packages.iter().map(|p| p.size).sum();
    let cached_bytes: u64 = spec
        .packages
        .iter()
        .filter_map(|p| {
            std::fs::metadata(cache.join(format!("{}.whl", p.sha256)))
                .ok()
                .map(|m| m.len().min(p.size))
        })
        .sum();
    ensure_space(&base, total.saturating_mul(3).saturating_sub(cached_bytes))?;
    let mut received = 0;
    let mut seen = BTreeSet::new();
    for package in &spec.packages {
        cancelled(receiver)?;
        if !package.url.starts_with("https://files.pythonhosted.org/")
            || package.sha256.len() != 64
            || !package.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("Invalid pinned CUDA package source".into());
        }
        let path = cache.join(format!("{}.whl", package.sha256));
        let existing = path.clone();
        let expected = package.clone();
        let cached = tauri::async_runtime::spawn_blocking(move || {
            std::fs::metadata(&existing).is_ok_and(|m| m.len() == expected.size)
                && checksum(&existing).is_ok_and(|sha| sha == expected.sha256)
        })
        .await
        .map_err(|e| e.to_string())?;
        if !cached {
            let artifact = Artifact {
                role: package.role.clone(),
                url: package.url.clone(),
                sha256: package.sha256.clone(),
                size: package.size,
            };
            super::assets::stream_artifact(&client,&artifact,&path,receiver,|phase,bytes,message,restart|{let done=received+bytes;publish(app,job,json!({"phase":phase,"progress":(done as f64/total as f64*90.).min(90.),"received":done,"total":total,"file":package.role,"message":message.unwrap_or(""),"restart":restart}));}).await?;
        }
        received += package.size;
        publish(
            app,
            job,
            json!({"phase":"installing","progress":received as f64/total as f64*90.,"received":received,"total":total,"file":package.role,"message":""}),
        );
        let package = package.clone();
        let directory = stage.clone();
        let watcher = receiver.clone();
        seen = tauri::async_runtime::spawn_blocking(move || {
            let mut seen = seen;
            extract(&path, &directory, &package, &mut seen, &watcher)?;
            Ok::<_, String>(seen)
        })
        .await
        .map_err(|e| e.to_string())??;
    }
    cancelled(receiver)?;
    for file in REQUIRED {
        if !stage.join(file).is_file() {
            return Err(format!("CUDA package is missing required library: {file}"));
        }
    }
    let files = std::fs::read_dir(&stage)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .map(|e| {
            e.metadata()
                .map(|m| (e.file_name().to_string_lossy().into_owned(), m.len()))
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    publish(
        app,
        job,
        json!({"phase":"verifying","progress":95,"received":received,"total":total,"file":"CUDA runtime","message":""}),
    );
    let directory = stage.clone();
    let watcher = receiver.clone();
    let files = tauri::async_runtime::spawn_blocking(move || {
        files
            .into_iter()
            .map(|(name, size)| {
                cancelled(&watcher)?;
                checksum(&directory.join(&name)).map(|sha| (name, size, sha))
            })
            .collect::<Result<Vec<_>, String>>()
    })
    .await
    .map_err(|e| e.to_string())??;
    cancelled(receiver)?;
    std::fs::write(
        stage.join("installed.json"),
        serde_json::to_vec(&Receipt {
            digest: digest(),
            files,
        })
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if destination.exists() {
        // Only incomplete, content-addressed component directories are replaced.
        let backup = base.join(format!(".old-{}", uuid::Uuid::new_v4()));
        tokio::fs::rename(&destination, &backup)
            .await
            .map_err(|e| e.to_string())?;
        if let Err(error) = tokio::fs::rename(&stage, &destination).await {
            let _ = tokio::fs::rename(&backup, &destination).await;
            return Err(error.to_string());
        }
        let _ = tokio::fs::remove_dir_all(backup).await;
    } else {
        tokio::fs::rename(&stage, &destination)
            .await
            .map_err(|e| e.to_string())?;
    }
    // Completed archives are no longer needed; cancelled attempts retain verified packages.
    for package in spec.packages {
        let _ = tokio::fs::remove_file(cache.join(format!("{}.whl", package.sha256))).await;
    }
    Ok(())
}
#[tauri::command]
pub async fn get_cuda_runtime_status() -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(status)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn cancel_cuda_runtime_download(job_id: String) -> Result<(), String> {
    if let Some((id, sender)) = CANCEL.lock().map_err(|e| e.to_string())?.as_ref() {
        if id != &job_id {
            return Err("A different CUDA runtime download is active".into());
        }
        let _ = sender.send(true);
    }
    Ok(())
}
#[tauri::command]
pub async fn download_cuda_runtime(
    app_handle: AppHandle,
    request_id: String,
) -> Result<Status, String> {
    if !status().supported {
        return Err("CUDA runtime download supports Windows x64 only".into());
    }
    uuid::Uuid::parse_str(&request_id).map_err(|_| "Invalid CUDA download request")?;
    let _lock = LOCK
        .try_lock()
        .map_err(|_| "A CUDA runtime download or verification is already running")?;
    let (sender, mut receiver) = tokio::sync::watch::channel(false);
    *CANCEL.lock().map_err(|e| e.to_string())? = Some((request_id.clone(), sender));
    let _cancel = CancelGuard;
    publish(&app_handle, &request_id, json!({}));
    let result = install(&app_handle, &request_id, &mut receiver).await;
    // Keep the lock through the terminal event so an old task cannot overwrite a new task.
    let terminal = match &result {
        Ok(()) => json!({"state":"completed","phase":"complete","progress":100,"message":""}),
        Err(e) if e == "Download canceled" => json!({"state":"cancelled","message":""}),
        Err(e) => json!({"state":"failed","message":e}),
    };
    publish(&app_handle, &request_id, terminal);
    if result.is_ok() {
        if let Ok(mut probe) = PROBE.lock() {
            *probe = None;
        }
    }
    super::runtime::refresh();
    let _ = super::commands::changed(&app_handle);
    result?;
    Ok(status())
}
#[tauri::command]
pub async fn test_cuda_runtime(app_handle: AppHandle) -> Result<Status, String> {
    let lock = LOCK
        .try_lock()
        .map_err(|_| "A CUDA runtime download or verification is already running")?;
    let checking = CheckingGuard::begin()?;
    let _ = super::commands::changed(&app_handle);
    tauri::async_runtime::spawn_blocking(move || {
        // Keep ownership in the worker: closing Settings must not release the
        // busy state or installation lock while CUDA libraries are loading.
        let mut probed = false;
        let result = (|| {
            let _lane = super::vision_jobs::try_claim(super::types::Task::Detection)?;
            probed = true;
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            let result = super::cuda::probe();
            #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
            let result: Result<std::collections::BTreeMap<String, u64>, String> =
                Err("CUDA component is unsupported on this platform".into());
            *PROBE.lock().map_err(|e| e.to_string())? = Some(result.clone());
            result
        })();
        drop(checking);
        if probed {
            super::runtime::refresh();
        }
        let _ = super::commands::changed(&app_handle);
        drop(lock);
        result?;
        Ok(status())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn open_cuda_runtime_directory() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let base = base()?;
        std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
        crate::t_utils::reveal_path(base.to_str().ok_or("CUDA directory is not UTF-8")?)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verification_busy_state_rejects_duplicates_and_clears_on_worker_exit() {
        let checking = CheckingGuard::begin().unwrap();
        assert!(CHECKING.load(Ordering::SeqCst));
        assert!(CheckingGuard::begin().is_err());
        drop(checking);
        assert!(!CHECKING.load(Ordering::SeqCst));
    }
    fn fixture() -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/objects-validation")
            .join(format!("cuda-assets-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn package() -> Package {
        Package {
            role: "test".into(),
            package: "fixture".into(),
            version: "1".into(),
            prefix: "nvidia/test/bin/".into(),
            url: "https://files.pythonhosted.org/fixture".into(),
            sha256: "0".repeat(64),
            size: 1,
        }
    }
    fn archive(path: &Path, names: &[&str]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for name in names {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"fixture dll").unwrap();
        }
        zip.finish().unwrap();
    }
    #[test]
    fn catalog_is_pinned_and_contains_the_complete_gpu_runtime() {
        let c = catalog().unwrap();
        assert!(!c.id.is_empty());
        assert!(c.packages.len() >= 7);
        for p in c.packages {
            assert!(p.url.starts_with("https://files.pythonhosted.org/"));
            assert_eq!(p.sha256.len(), 64);
            assert!(p.size > 0);
            assert!(valid_name(&p.role));
        }
    }
    #[test]
    fn cuda_options_reload_sessions_without_invalidating_model_results() {
        let config = crate::ai::settings::Configuration::default();
        let original = config.resolve("rapidocr-pp-ocrv5").unwrap();
        let mut changed = original.clone();
        changed
            .values
            .insert("cuda_memory_limit_mb".into(), json!(512));
        changed.values.insert("cuda_device_id".into(), json!(0));
        changed
            .values
            .insert("cuda_max_workspace".into(), json!(true));
        assert_eq!(original.profile(), changed.profile());
        assert_ne!(original.session_key(), changed.session_key());
        for p in crate::ai::types::parameters(changed.definition.task, changed.definition.adapter)
            .into_iter()
            .filter(|p| p.key.starts_with("cuda_"))
        {
            assert!(!p.affects_index);
            assert!(p.reload_session);
        }
    }
    #[test]
    fn verified_archives_only_extract_safe_dlls_and_reject_case_collisions() {
        let path = fixture();
        let out = path.join("out");
        std::fs::create_dir(&out).unwrap();
        let zip = path.join("runtime.whl");
        archive(&zip, &["nvidia/test/bin/test.dll", "other/code.exe"]);
        let (_, rx) = tokio::sync::watch::channel(false);
        let mut seen = BTreeSet::new();
        extract(&zip, &out, &package(), &mut seen, &rx).unwrap();
        assert!(out.join("test.dll").is_file());
        assert!(!out.join("code.exe").exists());
        archive(&zip, &["nvidia/test/bin/TEST.DLL"]);
        assert!(extract(&zip, &out, &package(), &mut seen, &rx).is_err());
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn traversal_and_cancelled_extraction_never_write_outside_staging() {
        let path = fixture();
        let out = path.join("out");
        std::fs::create_dir(&out).unwrap();
        let zip = path.join("bad.whl");
        archive(&zip, &["../escaped.dll"]);
        let (tx, rx) = tokio::sync::watch::channel(false);
        assert!(extract(&zip, &out, &package(), &mut BTreeSet::new(), &rx).is_err());
        assert!(!path.join("escaped.dll").exists());
        archive(&zip, &["nvidia/test/bin/test.dll"]);
        tx.send(true).unwrap();
        assert!(extract(&zip, &out, &package(), &mut BTreeSet::new(), &rx).is_err());
        assert!(!out.join("test.dll").exists());
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn incomplete_or_changed_components_do_not_become_available() {
        let path = fixture();
        let mut files = Vec::new();
        for name in REQUIRED {
            std::fs::write(path.join(name), b"fixture").unwrap();
            files.push((name.to_string(), 7, checksum(&path.join(name)).unwrap()));
        }
        std::fs::write(
            path.join("installed.json"),
            serde_json::to_vec(&Receipt {
                digest: digest(),
                files,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(installed_at(&path));
        std::fs::write(path.join(REQUIRED[0]), b"truncated").unwrap();
        assert!(!installed_at(&path));
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    #[ignore = "Requires checksum-pinned official runtime archives in workspace-only LAP_CUDA_ARCHIVE_ROOT"]
    fn actual_official_archives_extract_with_the_production_installer() {
        let source = PathBuf::from(
            std::env::var_os("LAP_CUDA_ARCHIVE_ROOT").expect("explicit archive fixture directory"),
        )
        .canonicalize()
        .unwrap();
        assert!(
            source.starts_with(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("target")
                    .canonicalize()
                    .unwrap()
            )
        );
        let target = fixture();
        let (_, receiver) = tokio::sync::watch::channel(false);
        let mut seen = BTreeSet::new();
        for package in catalog().unwrap().packages {
            let name = package.url.rsplit('/').next().unwrap();
            let archive = source.join(name);
            assert_eq!(checksum(&archive).unwrap(), package.sha256);
            extract(&archive, &target, &package, &mut seen, &receiver).unwrap();
        }
        for name in REQUIRED {
            assert!(target.join(name).is_file(), "missing {name}");
        }
        assert!(target.join("licenses").read_dir().unwrap().count() >= 7);
        assert!(!target.join("onnxruntime_providers_tensorrt.dll").exists());
        println!(
            "Production extractor verified {} CUDA libraries with license files",
            seen.len()
        );
        std::fs::remove_dir_all(target).unwrap();
    }
    #[test]
    #[ignore = "Requires the checksum-verified CUDA fixture component in LAP_CUDA_TEST_ROOT and a CUDA GPU"]
    fn actual_cuda_models_use_the_optional_component_without_changing_profiles() {
        use crate::ai::{capabilities::ObjectDetector, types::Task};
        assert!(std::env::var_os("LAP_CUDA_TEST_ROOT").is_some());
        let report = super::super::cuda::probe().unwrap();
        assert!(report.get("CUDAExecutionProvider").copied().unwrap_or(0) > 0);
        crate::ai::runtime::refresh();
        let config = crate::ai::settings::Configuration::default();
        let model_root = PathBuf::from(
            std::env::var_os("LAP_AI_TEST_MODEL_ROOT").expect("explicit fixture model root"),
        )
        .canonicalize()
        .unwrap();
        assert!(
            model_root.starts_with(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("target")
                    .canonicalize()
                    .unwrap()
            )
        );
        for id in ["rapidocr-pp-ocrv5", "yolox-s"] {
            let mut model = config.resolve(id).unwrap();
            let directory = crate::ai::assets::directory(&model.definition).unwrap();
            std::fs::write(directory.join("installed.json"), model.definition.digest()).unwrap();
            let original = model.profile();
            model.values.insert("device".into(), json!("cuda"));
            assert_eq!(model.profile(), original);
            crate::ai::runtime::with_profiling(|| match model.definition.task {
                Task::Ocr => {
                    let mut backend = crate::ai::adapters::ocr::PaddleOcr::new(&model).unwrap();
                    backend.probe().unwrap();
                    let picture = if let Some(path) = std::env::var_os("LAP_CUDA_OCR_FIXTURE") {
                        let path = PathBuf::from(path).canonicalize().unwrap();
                        assert!(
                            path.starts_with(
                                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                    .join("target")
                                    .canonicalize()
                                    .unwrap()
                            )
                        );
                        image::open(path).unwrap()
                    } else {
                        image::DynamicImage::new_rgb8(640, 480)
                    };
                    let doc = backend.recognize(&picture, &|| Ok(())).unwrap();
                    assert_eq!(
                        (doc.image_width, doc.image_height),
                        (picture.width(), picture.height())
                    );
                    if std::env::var_os("LAP_CUDA_OCR_FIXTURE").is_some() {
                        println!("CUDA OCR: {}", doc.text);
                        assert!(doc.text.contains("LAP"));
                    }
                }
                Task::Detection => {
                    let mut backend =
                        crate::ai::adapters::objects::OnnxObjects::new(&model).unwrap();
                    backend
                        .detect_objects(&image::DynamicImage::new_rgb8(640, 480), &|| Ok(()))
                        .unwrap();
                }
                _ => unreachable!(),
            });
            let reports = crate::ai::runtime::reports(&model);
            println!("{id}: {}", serde_json::to_string(&reports).unwrap());
            assert!(reports.iter().all(|r| r.selected == "cuda"));
            assert!(reports.iter().any(|r| {
                r.operators
                    .get("CUDAExecutionProvider")
                    .copied()
                    .unwrap_or(0)
                    > 0
            }));
        }
    }
}
