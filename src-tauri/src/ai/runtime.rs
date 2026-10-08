use super::types::ResolvedModel;
use ort::{
    execution_providers::{CPUExecutionProvider, ExecutionProvider},
    session::{Session, builder::GraphOptimizationLevel},
};
use serde::Serialize;
use std::{
    cell::Cell,
    collections::BTreeMap,
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuDevice {
    pub id: i32,
    pub name: String,
    pub memory_bytes: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub available: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub providers: Vec<ProviderInfo>,
    pub devices: Vec<GpuDevice>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReport {
    pub role: String,
    pub requested: String,
    pub selected: String,
    pub device_id: Option<i32>,
    pub fallback_reason: Option<String>,
    pub operators: BTreeMap<String, u64>,
}
static INFO: Mutex<Option<RuntimeInfo>> = Mutex::new(None);
static REPORTS: Mutex<BTreeMap<(String, String), SessionReport>> = Mutex::new(BTreeMap::new());
static FORCED_CPU: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
static GENERATION: AtomicU64 = AtomicU64::new(0);
thread_local! {static PROFILING:Cell<bool>=const {Cell::new(false)};}
pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}
pub fn refresh() {
    if let Ok(mut info) = INFO.lock() {
        *info = None;
    }
    if let Ok(mut forced) = FORCED_CPU.lock() {
        forced.clear();
    }
    GENERATION.fetch_add(1, Ordering::SeqCst);
}
pub fn info() -> RuntimeInfo {
    let mut state = INFO.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(info) = state.as_ref() {
        return info.clone();
    }
    let devices = gpu_devices();
    let mut providers = vec![ProviderInfo {
        id: "cpu".into(),
        available: CPUExecutionProvider::default()
            .is_available()
            .unwrap_or(false),
        reason: String::new(),
    }];
    let directml = {
        #[cfg(target_os = "windows")]
        {
            ort::execution_providers::DirectMLExecutionProvider::default()
                .is_available()
                .unwrap_or(false)
                && !devices.is_empty()
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    };
    providers.push(ProviderInfo {
        id: "directml".into(),
        available: directml,
        reason: if directml {
            "Windows DirectX 12 hardware; operator coverage is checked by an inference test".into()
        } else {
            "Requires a Windows DirectML runtime and a DirectX 12 hardware adapter".into()
        },
    });
    let coreml = {
        #[cfg(target_os = "macos")]
        {
            ort::execution_providers::CoreMLExecutionProvider::default()
                .is_available()
                .unwrap_or(false)
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    };
    providers.push(ProviderInfo {
        id: "coreml".into(),
        available: coreml,
        reason: "macOS CoreML may use GPU, Neural Engine and CPU; model support varies".into(),
    });
    let cuda = {
        #[cfg(feature = "ai-cuda")]
        {
            ort::execution_providers::CUDAExecutionProvider::default().supported_by_platform()
                && ort::execution_providers::CUDAExecutionProvider::default()
                    .is_available()
                    .unwrap_or(false)
        }
        #[cfg(not(feature = "ai-cuda"))]
        {
            false
        }
    };
    providers.push(ProviderInfo{id:"cuda".into(),available:cuda,reason:if cfg!(feature="ai-cuda"){ "Requires compatible NVIDIA drivers, CUDA 12 and cuDNN libraries; tested when creating a session".into()}else{"CUDA is not built into this package; enable the ai-cuda build feature with matching runtime dependencies".into()}});
    let info = RuntimeInfo { providers, devices };
    *state = Some(info.clone());
    info
}
#[cfg(target_os = "windows")]
fn gpu_devices() -> Vec<GpuDevice> {
    use windows::Win32::Graphics::{
        Direct3D::D3D_FEATURE_LEVEL_11_0,
        Direct3D12::{D3D12CreateDevice, ID3D12Device},
        Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1},
    };
    let mut devices = Vec::new();
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return devices;
        };
        for id in 0..128 {
            let Ok(adapter) = factory.EnumAdapters1(id) else {
                break;
            };
            let Ok(desc) = adapter.GetDesc1() else {
                continue;
            };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let mut device: Option<ID3D12Device> = None;
            if D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device).is_err() {
                continue;
            }
            let end = desc
                .Description
                .iter()
                .position(|v| *v == 0)
                .unwrap_or(desc.Description.len());
            devices.push(GpuDevice {
                id: id as i32,
                name: String::from_utf16_lossy(&desc.Description[..end]),
                memory_bytes: desc.DedicatedVideoMemory as u64,
            });
        }
    }
    devices
}
#[cfg(not(target_os = "windows"))]
fn gpu_devices() -> Vec<GpuDevice> {
    Vec::new()
}
pub fn reports(model: &ResolvedModel) -> Vec<SessionReport> {
    REPORTS
        .lock()
        .map(|reports| {
            reports
                .iter()
                .filter(|((key, _), _)| key == &model.session_key())
                .map(|(_, r)| r.clone())
                .collect()
        })
        .unwrap_or_default()
}
pub fn used_acceleration(model: &ResolvedModel) -> bool {
    reports(model).iter().any(|r| r.selected != "cpu")
}
pub fn force_cpu(model: &ResolvedModel, reason: &str) {
    if let Ok(mut forced) = FORCED_CPU.lock() {
        forced.insert(model.session_key(), reason.to_string());
    }
    GENERATION.fetch_add(1, Ordering::SeqCst);
}
pub fn is_auto(model: &ResolvedModel, role: &str) -> bool {
    requested(model, role) == "auto"
}
fn requested(model: &ResolvedModel, role: &str) -> String {
    model
        .values
        .get(&format!("{role}_device"))
        .and_then(serde_json::Value::as_str)
        .filter(|v| *v != "inherit")
        .or_else(|| {
            model
                .values
                .get("device")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("auto")
        .to_string()
}
pub fn with_profiling<T>(operation: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            PROFILING.with(|mode| mode.set(self.0));
        }
    }
    let previous = PROFILING.with(|mode| mode.replace(true));
    let _reset = Reset(previous);
    operation()
}
pub struct SessionHandle {
    session: Session,
    key: String,
    role: String,
    trace_prefix: Option<PathBuf>,
}
impl Deref for SessionHandle {
    type Target = Session;
    fn deref(&self) -> &Session {
        &self.session
    }
}
impl DerefMut for SessionHandle {
    fn deref_mut(&mut self) -> &mut Session {
        &mut self.session
    }
}
impl Drop for SessionHandle {
    fn drop(&mut self) {
        #[cfg(test)]
        eprintln!("GPU debug: drop {}", self.role);
        if let Some(prefix) = &self.trace_prefix {
            if let Ok(path) = self.session.end_profiling() {
                let counts = std::fs::read(&path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                    .and_then(|events| events.as_array().cloned())
                    .map(|events| {
                        let mut counts = BTreeMap::new();
                        for event in events {
                            if event.get("cat").and_then(|v| v.as_str()) == Some("Node") {
                                if let Some(provider) =
                                    event.pointer("/args/provider").and_then(|v| v.as_str())
                                {
                                    *counts.entry(provider.to_string()).or_insert(0) += 1;
                                }
                            }
                        }
                        counts
                    })
                    .unwrap_or_default();
                if let Ok(mut reports) = REPORTS.lock() {
                    if let Some(report) = reports.get_mut(&(self.key.clone(), self.role.clone())) {
                        report.operators = counts;
                    }
                }
                // Never remove an unrelated file returned by a third-party runtime.
                if path.starts_with(&prefix.to_string_lossy().to_string()) {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }
}
fn build(
    model: &ResolvedModel,
    role: &str,
    provider: &str,
    device: Option<i32>,
    trace: &Option<PathBuf>,
) -> Result<Session, String> {
    #[cfg(test)]
    eprintln!("GPU debug: begin {provider}/{role} device={device:?}");
    let mut builder = Session::builder()
        .map_err(|e| e.to_string())?
        .with_optimization_level(if provider == "cpu" {
            GraphOptimizationLevel::Level3
        } else {
            GraphOptimizationLevel::Level1
        })
        .map_err(|e| e.to_string())?
        .with_intra_threads(model.number("threads") as usize)
        .map_err(|e| e.to_string())?;
    match provider {
        "cpu" => {}
        "directml" => {
            #[cfg(target_os = "windows")]
            {
                builder = builder
                    .with_parallel_execution(false)
                    .map_err(|e| e.to_string())?
                    .with_memory_pattern(false)
                    .map_err(|e| e.to_string())?
                    .with_execution_providers([
                        ort::execution_providers::DirectMLExecutionProvider::default()
                            .with_device_id(device.unwrap_or(0))
                            .build()
                            .error_on_failure(),
                    ])
                    .map_err(|e| e.to_string())?;
            }
            #[cfg(not(target_os = "windows"))]
            {
                return Err("DirectML is unavailable on this platform".into());
            }
        }
        "coreml" => {
            #[cfg(target_os = "macos")]
            {
                builder = builder
                    .with_execution_providers([
                        ort::execution_providers::CoreMLExecutionProvider::default()
                            .with_compute_units(
                                ort::execution_providers::coreml::CoreMLComputeUnits::All,
                            )
                            .build()
                            .error_on_failure(),
                    ])
                    .map_err(|e| e.to_string())?;
            }
            #[cfg(not(target_os = "macos"))]
            {
                return Err("CoreML is unavailable on this platform".into());
            }
        }
        "cuda" => {
            #[cfg(feature = "ai-cuda")]
            {
                builder = builder
                    .with_execution_providers([
                        ort::execution_providers::CUDAExecutionProvider::default()
                            .with_device_id(device.unwrap_or(0))
                            .build()
                            .error_on_failure(),
                    ])
                    .map_err(|e| e.to_string())?;
            }
            #[cfg(not(feature = "ai-cuda"))]
            {
                return Err("CUDA support is not compiled into this build".into());
            }
        }
        _ => return Err("Unknown execution provider".into()),
    }
    #[cfg(test)]
    eprintln!("GPU debug: registered {provider}/{role}");
    if provider == "directml" {
        if role == "detector" {
            if model
                .values
                .get("detector_adaptive_size")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                return Err("DirectML face detection requires fixed input size; disable adaptive detection size or use CPU".into());
            }
            let size = model.number("detector_size") as i64;
            for name in ["?", "height", "width"] {
                builder = builder
                    .with_dimension_override(name, size)
                    .map_err(|e| e.to_string())?;
            }
        } else if role == "vision" {
            for (name, size) in [
                ("batch_size", 1),
                ("num_channels", 3),
                ("height", 224),
                ("width", 224),
            ] {
                builder = builder
                    .with_dimension_override(name, size)
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    if let Some(path) = trace {
        builder = builder.with_profiling(path).map_err(|e| e.to_string())?;
    }
    #[cfg(test)]
    eprintln!("GPU debug: commit {provider}/{role}");
    builder
        .commit_from_file(super::assets::artifact_path(&model.definition, role)?)
        .map_err(|e| format!("Cannot create {provider} session for {role}: {e}"))
}
pub fn local_session(model: &ResolvedModel, role: &str) -> Result<SessionHandle, String> {
    let desired = requested(model, role);
    let runtime = info();
    let forced = FORCED_CPU
        .lock()
        .ok()
        .and_then(|map| map.get(&model.session_key()).cloned());
    let trace = PROFILING
        .with(|mode| mode.get())
        .then(|| std::env::temp_dir().join(format!("lap-ai-profile-{}", uuid::Uuid::new_v4())));
    let mut failures = Vec::new();
    let candidates = if desired == "auto" {
        if let Some(reason) = forced {
            failures.push(reason);
            vec!["cpu"]
        } else {
            vec!["cuda", "directml", "coreml", "cpu"]
        }
    } else {
        vec![desired.as_str()]
    };
    for provider in candidates {
        let Some(info) = runtime.providers.iter().find(|p| p.id == provider) else {
            return Err("Unknown execution provider".into());
        };
        if !info.available {
            if desired != "auto" {
                return Err(info.reason.clone());
            }
            continue;
        }
        let configured = model.number("gpu_device_id") as i32;
        let device = match provider {
            "directml" => {
                let found = if configured < 0 {
                    runtime.devices.iter().max_by(|a, b| {
                        a.memory_bytes
                            .cmp(&b.memory_bytes)
                            .then_with(|| b.id.cmp(&a.id))
                    })
                } else {
                    runtime.devices.iter().find(|d| d.id == configured)
                };
                let Some(found) = found else {
                    if desired == "auto" {
                        failures.push("Requested DirectML adapter is unavailable".into());
                        continue;
                    }
                    return Err("Requested DirectML adapter is unavailable".into());
                };
                Some(found.id)
            }
            "cuda" => Some(configured.max(0)),
            _ => None,
        };
        match build(model, role, provider, device, &trace) {
            Ok(session) => {
                let key = model.session_key();
                let report = SessionReport {
                    role: role.into(),
                    requested: desired.clone(),
                    selected: provider.into(),
                    device_id: device,
                    fallback_reason: (!failures.is_empty()).then(|| failures.join("; ")),
                    operators: BTreeMap::new(),
                };
                if let Ok(mut reports) = REPORTS.lock() {
                    reports.insert((key.clone(), role.into()), report);
                }
                return Ok(SessionHandle {
                    session,
                    key,
                    role: role.into(),
                    trace_prefix: trace,
                });
            }
            Err(error) => {
                if desired != "auto" {
                    return Err(error);
                }
                failures.push(error);
            }
        }
    }
    Err(format!(
        "No usable execution provider: {}",
        failures.join("; ")
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_device_is_not_silently_substituted() {
        let config = super::super::settings::Configuration::default();
        let mut model = config.resolve("clip-b32").unwrap();
        model
            .values
            .insert("device".into(), serde_json::json!("cpu"));
        assert_eq!(requested(&model, "vision"), "cpu");
        model
            .values
            .insert("vision_device".into(), serde_json::json!("directml"));
        assert_eq!(requested(&model, "vision"), "directml");
        assert_eq!(requested(&model, "text"), "cpu");
    }
    #[test]
    fn runtime_discovery_is_explicit() {
        let runtime = info();
        assert!(
            runtime
                .providers
                .iter()
                .any(|p| p.id == "cpu" && p.available)
        );
        println!("runtime: {:?}", runtime);
    }
}
