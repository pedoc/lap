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

use super::hardware::{GpuAdapter, GpuDevice};
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
    pub adapters: Vec<GpuAdapter>,
    pub physical_detection_complete: bool,
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
    let inventory = super::hardware::discover();
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
                && !inventory.adapters.is_empty()
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
    let info = RuntimeInfo {
        providers,
        devices: inventory.devices,
        adapters: inventory.adapters,
        physical_detection_complete: inventory.physical_detection_complete,
    };
    *state = Some(info.clone());
    info
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
        if role == "detector" && model.definition.task == super::types::Task::Face {
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
        } else if model.definition.task == super::types::Task::Ocr {
            for (name, size) in ocr_dimension_overrides(model, role) {
                builder = builder
                    .with_dimension_override(name, size)
                    .map_err(|e| e.to_string())?;
            }
        } else if model.definition.task == super::types::Task::Detection {
            let size = model
                .definition
                .object_input_size()
                .ok_or("Unknown object input contract")? as i64;
            for (name, size) in [
                ("batch_size", 1),
                ("batch", 1),
                ("num_channels", 3),
                ("height", size),
                ("width", size),
            ] {
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
    let artifact = super::assets::artifact_path(&model.definition, role)?;
    if provider == "directml" && needs_reshape_compatibility(model, role) {
        let mut bytes = std::fs::read(&artifact).map_err(|e| e.to_string())?;
        if super::onnx_patch::disable_reshape_allowzero(&mut bytes) > 0 {
            return builder
                .commit_from_memory(&bytes)
                .map_err(|e| format!("Cannot create {provider} session for {role}: {e}"));
        }
    }
    builder
        .commit_from_file(artifact)
        .map_err(|e| format!("Cannot create {provider} session for {role}: {e}"))
}
fn needs_reshape_compatibility(model: &ResolvedModel, role: &str) -> bool {
    model.definition.adapter == super::types::Adapter::RfDetrCocoOnnx
        && role == "detector"
        && model.definition.files.iter().any(|artifact| {
            artifact.role == role
                && artifact.sha256.eq_ignore_ascii_case(
                    "cf0d64793e5ead6c21260d92f447e95737e6ea76b3f4b3da302aabdf67da9aff",
                )
        })
}
fn virtual_adapter(name: &str) -> bool {
    let name = name.to_lowercase();
    [
        "todesk",
        "parsec",
        "sunshine",
        "virtual",
        "idd",
        "remote",
        "basic display",
        "基本显示",
        "虚拟",
    ]
    .iter()
    .any(|needle| name.contains(needle))
}
fn directml_devices(runtime: &RuntimeInfo, configured: i32) -> Vec<i32> {
    if configured >= 0 {
        return runtime
            .adapters
            .iter()
            .any(|adapter| adapter.id == configured)
            .then_some(configured)
            .into_iter()
            .collect();
    }
    let mut candidates = Vec::new();
    let mut push = |id: i32| {
        if !candidates.contains(&id) {
            candidates.push(id);
        }
    };
    let mut devices = runtime.devices.iter().collect::<Vec<_>>();
    devices.sort_by(|a, b| {
        virtual_adapter(&a.name)
            .cmp(&virtual_adapter(&b.name))
            .then_with(|| {
                b.memory_bytes
                    .unwrap_or(0)
                    .cmp(&a.memory_bytes.unwrap_or(0))
            })
            .then_with(|| a.id.cmp(&b.id))
    });
    for device in devices {
        push(device.id);
        for id in &device.adapter_ids {
            push(*id);
        }
    }
    let mut adapters = runtime.adapters.iter().collect::<Vec<_>>();
    adapters.sort_by(|a, b| {
        virtual_adapter(&a.name)
            .cmp(&virtual_adapter(&b.name))
            .then_with(|| b.memory_bytes.cmp(&a.memory_bytes))
            .then_with(|| a.id.cmp(&b.id))
    });
    for adapter in adapters {
        push(adapter.id);
    }
    candidates
}

fn provider_candidates(desired: &str, forced_cpu: bool) -> Vec<&str> {
    match (desired, forced_cpu) {
        ("auto", false) => vec!["cuda", "directml", "coreml", "cpu"],
        ("auto", true) => vec!["cpu"],
        _ => vec![desired],
    }
}
fn ocr_dimension_overrides(model: &ResolvedModel, role: &str) -> Vec<(&'static str, i64)> {
    let (height, width) = match role {
        "ocr_detector" => {
            let size = model.number("ocr_detection_size") as i64;
            (size, size)
        }
        "recognizer" => (48, model.number("ocr_recognition_width") as i64),
        _ => return Vec::new(),
    };
    let mut values = vec![
        ("batch", 1),
        ("batch_size", 1),
        ("height", height),
        ("width", width),
        ("?", height),
    ];
    // Observed symbolic input dimensions in the checksum-pinned official exports.
    // v4 uses p2o.*, v5 uses DynamicDimension.*. Other contracts safely fail/fallback.
    for names in [
        [
            "p2o.DynamicDimension.0",
            "p2o.DynamicDimension.1",
            "p2o.DynamicDimension.2",
        ],
        [
            "DynamicDimension.0",
            "DynamicDimension.1",
            "DynamicDimension.2",
        ],
    ] {
        values.push((names[0], 1));
        values.push((names[1], if role == "recognizer" { width } else { height }));
        if role == "ocr_detector" {
            values.push((names[2], width));
        }
    }
    values
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
    if desired == "auto" {
        if let Some(reason) = &forced {
            failures.push(reason.clone());
        }
    }
    let candidates = provider_candidates(&desired, forced.is_some());
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
        let devices: Vec<Option<i32>> = match provider {
            "directml" => {
                let ids = directml_devices(&runtime, configured);
                if ids.is_empty() {
                    if desired == "auto" {
                        failures.push("Requested DirectML adapter is unavailable".into());
                        continue;
                    }
                    return Err("Requested DirectML adapter is unavailable".into());
                }
                ids.into_iter().map(Some).collect()
            }
            "cuda" => vec![Some(configured.max(0))],
            _ => vec![None],
        };
        let explicit_device = provider != "directml" || configured >= 0;
        for device in devices {
            match build(model, role, provider, device, &trace) {
                Ok(session) => {
                    if desired == "auto" && provider == "cpu" && failures.is_empty() {
                        failures.push("No available GPU execution provider; using CPU".into());
                    }
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
                    let message = match device {
                        Some(id) => {
                            let name = runtime
                                .adapters
                                .iter()
                                .find(|adapter| adapter.id == id)
                                .map(|adapter| adapter.name.as_str())
                                .unwrap_or("unknown adapter");
                            format!("{provider} device {id} ({name}): {error}")
                        }
                        None => error,
                    };
                    if desired != "auto" && explicit_device {
                        return Err(message);
                    }
                    failures.push(message);
                }
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
    fn reshape_compatibility_only_applies_to_the_verified_export() {
        let mut model = crate::ai::settings::Configuration::default()
            .resolve("rf-detr-large")
            .unwrap();
        assert!(needs_reshape_compatibility(&model, "detector"));
        assert!(!needs_reshape_compatibility(&model, "vision"));
        model.definition.id = "custom-rf-detr".into();
        assert!(needs_reshape_compatibility(&model, "detector"));
        model.definition.files[0].sha256 = "a".repeat(64);
        assert!(!needs_reshape_compatibility(&model, "detector"));
        model.definition.files[0].sha256 =
            "cf0d64793e5ead6c21260d92f447e95737e6ea76b3f4b3da302aabdf67da9aff".into();
        model.definition.adapter = super::super::types::Adapter::RtDetrOnnx;
        assert!(!needs_reshape_compatibility(&model, "detector"));
    }
    #[test]
    fn automatic_provider_order_prefers_acceleration_and_forcing_respects_explicit_gpu() {
        assert_eq!(
            provider_candidates("auto", false),
            vec!["cuda", "directml", "coreml", "cpu"]
        );
        assert_eq!(provider_candidates("auto", true), vec!["cpu"]);
        assert_eq!(provider_candidates("directml", true), vec!["directml"]);
        assert_eq!(provider_candidates("cpu", false), vec!["cpu"]);
    }
    #[test]
    fn directml_ocr_shapes_match_verified_v4_and_v5_exports() {
        let model = crate::ai::settings::Configuration::default()
            .resolve("rapidocr-pp-ocrv4")
            .unwrap();
        for (role, h, w) in [("ocr_detector", 960, 960), ("recognizer", 48, 1024)] {
            let shape = ocr_dimension_overrides(&model, role)
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>();
            assert_eq!(shape["?"], h);
            assert_eq!(shape["height"], h);
            assert_eq!(shape["width"], w);
            assert_eq!(shape["p2o.DynamicDimension.0"], 1);
            assert_eq!(shape["DynamicDimension.0"], 1);
            assert_eq!(
                shape["p2o.DynamicDimension.1"],
                if role == "recognizer" { w } else { h }
            );
        }
    }
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
    fn logical_alias_fixture() -> RuntimeInfo {
        RuntimeInfo {
            providers: Vec::new(),
            devices: vec![GpuDevice {
                id: 0,
                name: "GPU".into(),
                memory_bytes: Some(4096),
                physical_id: "physical-a".into(),
                adapter_ids: vec![0, 1],
            }],
            adapters: vec![0, 1]
                .into_iter()
                .map(|id| GpuAdapter {
                    id,
                    name: "GPU".into(),
                    memory_bytes: 4096,
                    physical_ids: vec!["physical-a".into()],
                })
                .collect(),
            physical_detection_complete: true,
        }
    }
    #[test]
    fn physical_grouping_keeps_explicit_provider_adapter_ids_valid() {
        let runtime = logical_alias_fixture();
        assert_eq!(directml_devices(&runtime, -1), vec![0, 1]);
        assert_eq!(directml_devices(&runtime, 1), vec![1]);
        assert!(directml_devices(&runtime, 7).is_empty());
    }
    #[test]
    fn failed_physical_detection_does_not_disable_usable_logical_adapters() {
        let mut runtime = logical_alias_fixture();
        runtime.devices.clear();
        runtime.physical_detection_complete = false;
        assert_eq!(directml_devices(&runtime, -1), vec![0, 1]);
        assert_eq!(directml_devices(&runtime, 1), vec![1]);
    }
    #[test]
    fn automatic_selection_uses_representative_ids_not_physical_display_ordinals() {
        let mut runtime = logical_alias_fixture();
        runtime.devices.push(GpuDevice {
            id: 5,
            name: "GPU".into(),
            memory_bytes: Some(8192),
            physical_id: "physical-b".into(),
            adapter_ids: vec![5],
        });
        assert_eq!(directml_devices(&runtime, -1), vec![5, 0, 1]);
    }
    #[test]
    fn virtual_display_adapters_are_tried_after_hardware() {
        let mut runtime = logical_alias_fixture();
        runtime.devices.push(GpuDevice {
            id: 3,
            name: "ToDesk Virtual Display Adapter".into(),
            memory_bytes: Some(16384),
            physical_id: "physical-virtual".into(),
            adapter_ids: vec![3],
        });
        runtime.adapters.push(GpuAdapter {
            id: 3,
            name: "ToDesk Virtual Display Adapter".into(),
            memory_bytes: 16384,
            physical_ids: vec!["physical-virtual".into()],
        });
        assert_eq!(directml_devices(&runtime, -1), vec![0, 1, 3]);
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
