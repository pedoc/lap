//! Optional CUDA runtime. Loaded only on demand; its ORT API never replaces the
//! host's statically linked CPU/DirectML runtime. The ABI exchanges owned bytes.
mod probe_model;
mod tensors;
use lap_ai_runtime_wire::{ABI, Buffer, MAX_BYTES, Message};
use ort::{
    execution_providers::{ArenaExtendStrategy, CUDAExecutionProvider},
    session::{Session, builder::GraphOptimizationLevel},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
static SESSIONS: Mutex<BTreeMap<u64, Session>> = Mutex::new(BTreeMap::new());
static NEXT: AtomicU64 = AtomicU64::new(1);
static RUNTIME: OnceLock<PathBuf> = OnceLock::new();
static LIBRARIES: Mutex<Vec<libloading::Library>> = Mutex::new(Vec::new());
fn initialize(root: &str) -> Result<(), String> {
    if !Path::new(root).is_absolute() {
        return Err("CUDA runtime requires an absolute component directory".into());
    }
    let path = Path::new(root)
        .canonicalize()
        .map_err(|e| format!("CUDA runtime directory: {e}"))?;
    if let Some(current) = RUNTIME.get() {
        return if current == &path {
            Ok(())
        } else {
            Err("CUDA runtime changed; restart Lap before loading another runtime".into())
        };
    }
    let mut libraries = LIBRARIES.lock().map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    for name in [
        "cudart64_12.dll",
        "cublasLt64_12.dll",
        "cublas64_12.dll",
        "cufft64_11.dll",
        "curand64_10.dll",
        "nvrtc-builtins64_128.dll",
        "nvrtc64_120_0.dll",
        "cudnn_graph64_9.dll",
        "cudnn_ops64_9.dll",
        "cudnn_adv64_9.dll",
        "cudnn_cnn64_9.dll",
        "cudnn_engines_precompiled64_9.dll",
        "cudnn_engines_runtime_compiled64_9.dll",
        "cudnn_heuristic64_9.dll",
        "cudnn64_9.dll",
        "onnxruntime.dll",
    ] {
        let file = path.join(name);
        if !file.is_file() {
            return Err(format!("CUDA dependency is missing: {name}"));
        }
        // Resolve dependencies from this immutable component directory, not CWD/PATH.
        let library = unsafe {
            libloading::os::windows::Library::load_with_flags(&file, 0x00000100 | 0x00001000)
        }
        .map_err(|e| format!("Cannot load CUDA dependency {name}: {e}"))?;
        libraries.push(library.into());
    }
    ort::init_from(path.join("onnxruntime.dll").to_string_lossy())
        .with_name("Lap CUDA")
        .commit()
        .map_err(|e| e.to_string())?;
    RUNTIME
        .set(path)
        .map_err(|_| "CUDA runtime was initialized concurrently".to_string())?;
    Ok(())
}
fn builder(
    device: i32,
    threads: usize,
    profile: &str,
    memory_limit: usize,
    max_workspace: bool,
) -> Result<ort::session::builder::SessionBuilder, String> {
    if RUNTIME.get().is_none() {
        return Err("Initialize the optional CUDA runtime first".into());
    }
    if device < 0 || threads > 128 {
        return Err("Invalid CUDA device or thread count".into());
    }
    let mut provider = CUDAExecutionProvider::default()
        .with_device_id(device)
        .with_arena_extend_strategy(ArenaExtendStrategy::SameAsRequested)
        .with_conv_max_workspace(max_workspace)
        .with_conv_algorithm_search(
            ort::execution_providers::cuda::CuDNNConvAlgorithmSearch::Heuristic,
        );
    if memory_limit > 0 {
        provider = provider.with_memory_limit(memory_limit);
    }
    let mut b = Session::builder()
        .map_err(|e| e.to_string())?
        .with_intra_threads(threads.max(1))
        .map_err(|e| e.to_string())?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| e.to_string())?
        .with_execution_providers([provider.build().error_on_failure()])
        .map_err(|e| e.to_string())?;
    if !profile.is_empty() {
        b = b.with_profiling(profile).map_err(|e| e.to_string())?;
    }
    Ok(b)
}
fn operators(session: &mut Session) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    if let Ok(file) = session.end_profiling() {
        if let Ok(bytes) = std::fs::read(&file) {
            if let Ok(events) = serde_json::from_slice::<Vec<serde_json::Value>>(&bytes) {
                for event in events {
                    if event["cat"] == "Node" {
                        if let Some(provider) =
                            event.pointer("/args/provider").and_then(|p| p.as_str())
                        {
                            *counts.entry(provider.into()).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
        let path = Path::new(&file);
        if path.file_name().is_some_and(|name| {
            name.to_string_lossy().starts_with("lap-ai-profile-")
                || name.to_string_lossy().starts_with("lap-cuda-probe-")
        }) {
            let _ = std::fs::remove_file(path);
        }
    }
    counts
}
fn execute(message: Message, payload: &[u8]) -> Result<(Message, Vec<u8>), String> {
    if matches!(message.op.as_str(), "run" | "open" | "probe") && RUNTIME.get().is_none() {
        return Err("Initialize the optional CUDA runtime first".into());
    }
    match message.op.as_str() {
        "init" => {
            initialize(&message.runtime)?;
            Ok((
                Message {
                    op: "ready".into(),
                    ..Default::default()
                },
                Vec::new(),
            ))
        }
        "probe" => {
            let prefix = std::env::temp_dir().join(format!(
                "lap-cuda-probe-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            let mut session = builder(message.device, 2, &prefix.to_string_lossy(), 0, false)?
                .commit_from_memory(probe_model::MATMUL)
                .map_err(|e| e.to_string())?;
            {
                let a = ort::value::Tensor::from_array(([32, 32], vec![1f32; 1024]))
                    .map_err(|e| e.to_string())?;
                let b = ort::value::Tensor::from_array(([32, 32], vec![1f32; 1024]))
                    .map_err(|e| e.to_string())?;
                let result = session
                    .run(ort::inputs!["A"=>a,"B"=>b])
                    .map_err(|e| e.to_string())?;
                let (_, data) = result[0]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| e.to_string())?;
                if data.len() != 1024 || data.iter().any(|n| (*n - 32.).abs() > 1e-3) {
                    return Err("CUDA probe returned incorrect matrix results".into());
                }
            }
            let operators = operators(&mut session);
            if operators.get("CUDAExecutionProvider").copied().unwrap_or(0) == 0 {
                return Err("Probe did not execute any operator on CUDA".into());
            }
            Ok((
                Message {
                    op: "verified".into(),
                    operators,
                    ..Default::default()
                },
                Vec::new(),
            ))
        }
        "open" => {
            if !Path::new(&message.model).is_absolute() {
                return Err("CUDA model path must be absolute".into());
            }
            let session = builder(
                message.device,
                message.threads,
                &message.profile,
                message.memory_limit,
                message.max_workspace,
            )?
            .commit_from_file(&message.model)
            .map_err(|e| e.to_string())?;
            let inputs = session
                .inputs
                .iter()
                .map(|i| tensors::describe(&i.name, &i.input_type))
                .collect::<Result<Vec<_>, _>>()?;
            let outputs = session
                .outputs
                .iter()
                .map(|o| tensors::describe(&o.name, &o.output_type))
                .collect::<Result<Vec<_>, _>>()?;
            let mut custom = BTreeMap::new();
            if inputs.len() > 64 || outputs.len() > 64 {
                return Err("Model has too many I/O tensors for CUDA bridge".into());
            }
            if let Some(value) = session
                .metadata()
                .map_err(|e| e.to_string())?
                .custom("character")
                .map_err(|e| e.to_string())?
            {
                if value.len() > 4 * 1024 * 1024 {
                    return Err("Model character metadata exceeds limit".into());
                }
                custom.insert("character".into(), value);
            }
            let id = NEXT.fetch_add(1, Ordering::SeqCst);
            SESSIONS
                .lock()
                .map_err(|e| e.to_string())?
                .insert(id, session);
            Ok((
                Message {
                    op: "opened".into(),
                    session: id,
                    inputs,
                    outputs,
                    custom,
                    ..Default::default()
                },
                Vec::new(),
            ))
        }
        "run" => {
            let inputs = message
                .tensors
                .iter()
                .map(|d| tensors::materialize(d, payload).map(|value| (d.name.clone(), value)))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sessions = SESSIONS.lock().map_err(|e| e.to_string())?;
            let session = sessions
                .get_mut(&message.session)
                .ok_or("CUDA session was closed")?;
            let output = session.run(inputs).map_err(|e| e.to_string())?;
            let mut data = Vec::new();
            let mut descriptors = Vec::new();
            for (name, value) in output.iter() {
                descriptors.push(tensors::append(name, &value, &mut data)?);
            }
            Ok((
                Message {
                    op: "result".into(),
                    tensors: descriptors,
                    ..Default::default()
                },
                data,
            ))
        }
        "close" => {
            let mut operators = BTreeMap::new();
            if let Some(mut session) = SESSIONS
                .lock()
                .map_err(|e| e.to_string())?
                .remove(&message.session)
            {
                operators = self::operators(&mut session);
            }
            Ok((
                Message {
                    op: "closed".into(),
                    operators,
                    ..Default::default()
                },
                Vec::new(),
            ))
        }
        _ => Err("Unknown CUDA runtime request".into()),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn lap_cuda_abi() -> u32 {
    ABI
}
/// # Safety
/// The input must refer to `len` readable bytes for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lap_cuda_call(data: *const u8, len: usize) -> Buffer {
    let result = std::panic::catch_unwind(|| {
        if data.is_null() || len > MAX_BYTES + 8 * 1024 * 1024 + 12 {
            return Err("Invalid CUDA ABI input".into());
        }
        let input = unsafe { std::slice::from_raw_parts(data, len) };
        let (message, payload) = lap_ai_runtime_wire::decode(input)?;
        execute(message, payload)
    })
    .unwrap_or_else(|_| Err("CUDA runtime panicked; the session must be recreated".into()));
    let (message, payload) = result.unwrap_or_else(|error| {
        (
            Message {
                op: "error".into(),
                error: Some(error.chars().take(8192).collect()),
                ..Default::default()
            },
            Vec::new(),
        )
    });
    let bytes = lap_ai_runtime_wire::encode(&message, &payload)
        .unwrap_or_else(|_| {
            lap_ai_runtime_wire::encode(
                &Message {
                    op: "error".into(),
                    error: Some("CUDA response exceeds limit".into()),
                    ..Default::default()
                },
                &[],
            )
            .unwrap_or_default()
        })
        .into_boxed_slice();
    let len = bytes.len();
    Buffer {
        data: Box::into_raw(bytes) as *mut u8,
        len,
    }
}
/// # Safety
/// Only buffers returned by `lap_cuda_call` may be passed, exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lap_cuda_free(buffer: Buffer) {
    if !buffer.data.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(buffer.data, buffer.len)) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(message: Message) -> Message {
        let packet = lap_ai_runtime_wire::encode(&message, &[]).unwrap();
        let buffer = unsafe { lap_cuda_call(packet.as_ptr(), packet.len()) };
        let bytes = unsafe { std::slice::from_raw_parts(buffer.data, buffer.len) }.to_vec();
        unsafe { lap_cuda_free(buffer) };
        lap_ai_runtime_wire::decode(&bytes).unwrap().0
    }
    #[test]
    fn abi_rejects_malformed_messages_without_loading_any_runtime() {
        let buffer = unsafe { lap_cuda_call(std::ptr::null(), 0) };
        let bytes = unsafe { std::slice::from_raw_parts(buffer.data, buffer.len) }.to_vec();
        unsafe { lap_cuda_free(buffer) };
        assert!(
            lap_ai_runtime_wire::decode(&bytes)
                .unwrap()
                .0
                .error
                .is_some()
        );
        assert_eq!(lap_cuda_abi(), 1);
    }
    #[test]
    fn invalid_session_calls_cannot_trigger_a_cwd_runtime_search() {
        let message = request(Message {
            op: "run".into(),
            session: 7,
            ..Default::default()
        });
        assert!(message.error.unwrap().contains("Initialize"));
        let message = request(Message {
            op: "init".into(),
            runtime: "relative".into(),
            ..Default::default()
        });
        assert!(message.error.unwrap().contains("absolute"));
    }
}
