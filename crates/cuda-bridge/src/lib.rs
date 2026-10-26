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
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
type SessionEntry<T> = Arc<Mutex<Option<T>>>;
type SessionMap<T> = Mutex<BTreeMap<u64, SessionEntry<T>>>;
static SESSIONS: SessionMap<Session> = Mutex::new(BTreeMap::new());
static NEXT: AtomicU64 = AtomicU64::new(1);
static RUNTIME: OnceLock<PathBuf> = OnceLock::new();
static LIBRARIES: Mutex<Vec<libloading::Library>> = Mutex::new(Vec::new());
fn session_entry<T>(sessions: &SessionMap<T>, id: u64) -> Result<SessionEntry<T>, String> {
    sessions
        .lock()
        .map_err(|e| e.to_string())?
        .get(&id)
        .cloned()
        .ok_or_else(|| "CUDA session was closed".into())
}
fn with_session<T, R>(
    entry: &SessionEntry<T>,
    operation: impl FnOnce(&mut T) -> Result<R, String>,
) -> Result<R, String> {
    let mut session = entry.lock().map_err(|e| e.to_string())?;
    operation(session.as_mut().ok_or("CUDA session was closed")?)
}
fn close_session<T>(sessions: &SessionMap<T>, id: u64) -> Result<Option<T>, String> {
    let entry = sessions.lock().map_err(|e| e.to_string())?.remove(&id);
    match entry {
        Some(entry) => {
            // A failed run may poison its own session, but cleanup must still release it.
            let mut session = entry.lock().unwrap_or_else(|e| e.into_inner());
            Ok(session.take())
        }
        None => Ok(None),
    }
}
fn initialize_once<T>(
    runtime: &OnceLock<PathBuf>,
    libraries: &Mutex<T>,
    path: PathBuf,
    load: impl FnOnce(&mut T, &Path) -> Result<(), String>,
) -> Result<(), String> {
    let mut libraries = libraries.lock().map_err(|e| e.to_string())?;
    if let Some(current) = runtime.get() {
        return if current == &path {
            Ok(())
        } else {
            Err("CUDA runtime changed; restart Lap before loading another runtime".into())
        };
    }
    load(&mut libraries, &path)?;
    runtime
        .set(path)
        .map_err(|_| "CUDA runtime was initialized concurrently".to_string())
}
fn initialize(root: &str) -> Result<(), String> {
    if !Path::new(root).is_absolute() {
        return Err("CUDA runtime requires an absolute component directory".into());
    }
    let path = Path::new(root)
        .canonicalize()
        .map_err(|e| format!("CUDA runtime directory: {e}"))?;
    initialize_once(&RUNTIME, &LIBRARIES, path, |libraries, path| {
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
        Ok(())
    })
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
                .insert(id, Arc::new(Mutex::new(Some(session))));
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
            let entry = session_entry(&SESSIONS, message.session)?;
            with_session(&entry, |session| {
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
            })
        }
        "close" => {
            let mut operators = BTreeMap::new();
            if let Some(mut session) = close_session(&SESSIONS, message.session)? {
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
    use std::{
        sync::{Barrier, TryLockError, mpsc},
        thread,
        time::{Duration, Instant},
    };
    const WAIT: Duration = Duration::from_secs(5);
    fn fake_sessions() -> Arc<SessionMap<usize>> {
        Arc::new(Mutex::new(BTreeMap::from([
            (1, Arc::new(Mutex::new(Some(0)))),
            (2, Arc::new(Mutex::new(Some(0)))),
        ])))
    }
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
    #[test]
    fn different_sessions_enter_inference_together() {
        let sessions = fake_sessions();
        let (entered, arrivals) = mpsc::channel();
        let mut releases = Vec::new();
        let mut workers = Vec::new();
        for id in [1, 2] {
            let sessions = sessions.clone();
            let entered = entered.clone();
            let (release, resume) = mpsc::channel();
            releases.push(release);
            workers.push(thread::spawn(move || {
                let entry = session_entry(&sessions, id)?;
                with_session(&entry, |session| {
                    entered.send(id).map_err(|e| e.to_string())?;
                    resume.recv_timeout(WAIT).map_err(|e| e.to_string())?;
                    *session += 1;
                    Ok(())
                })
            }));
        }
        let first = arrivals.recv_timeout(WAIT);
        let second = arrivals.recv_timeout(WAIT);
        for release in releases {
            let _ = release.send(());
        }
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
        let mut entered = [first.unwrap(), second.unwrap()];
        entered.sort();
        assert_eq!(entered, [1, 2]);
    }
    #[test]
    fn the_same_session_keeps_one_inference_owner() {
        let sessions = fake_sessions();
        let entry = session_entry(&sessions, 1).unwrap();
        let first_entry = entry.clone();
        let (entered, arrival) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let first = thread::spawn(move || {
            with_session(&first_entry, |session| {
                entered.send(()).map_err(|e| e.to_string())?;
                resume.recv_timeout(WAIT).map_err(|e| e.to_string())?;
                *session += 1;
                Ok(())
            })
        });
        arrival.recv_timeout(WAIT).unwrap();
        let (attempted, attempting) = mpsc::channel();
        let second_entry = entry.clone();
        let second = thread::spawn(move || {
            attempted.send(()).unwrap();
            with_session(&second_entry, |session| {
                *session += 1;
                Ok(())
            })
        });
        attempting.recv_timeout(WAIT).unwrap();
        let locked = matches!(entry.try_lock(), Err(TryLockError::WouldBlock));
        let _ = release.send(());
        first.join().unwrap().unwrap();
        second.join().unwrap().unwrap();
        assert!(locked);
        assert_eq!(with_session(&entry, |session| Ok(*session)).unwrap(), 2);
    }
    #[test]
    fn closing_waits_for_its_run_without_blocking_other_sessions() {
        let sessions = fake_sessions();
        let stale = session_entry(&sessions, 1).unwrap();
        let running = stale.clone();
        let (entered, arrival) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let run = thread::spawn(move || {
            with_session(&running, |session| {
                entered.send(()).map_err(|e| e.to_string())?;
                resume.recv_timeout(WAIT).map_err(|e| e.to_string())?;
                *session += 1;
                Ok(())
            })
        });
        arrival.recv_timeout(WAIT).unwrap();
        let closing = sessions.clone();
        let (closed, closing_result) = mpsc::channel();
        let close = thread::spawn(move || {
            closed.send(close_session(&closing, 1)).unwrap();
        });
        let other_sessions = sessions.clone();
        let (finished, other_result) = mpsc::channel();
        let other = thread::spawn(move || {
            let result = (|| {
                let deadline = Instant::now() + WAIT;
                while session_entry(&other_sessions, 1).is_ok() {
                    if Instant::now() > deadline {
                        return Err("Session was not removed before waiting for its run".into());
                    }
                    thread::yield_now();
                }
                with_session(&session_entry(&other_sessions, 2)?, |session| {
                    *session += 1;
                    Ok(*session)
                })
            })();
            finished.send(result).unwrap();
        });
        let other_finished = other_result.recv_timeout(WAIT);
        let still_running = matches!(closing_result.try_recv(), Err(mpsc::TryRecvError::Empty));
        let _ = release.send(());
        run.join().unwrap().unwrap();
        close.join().unwrap();
        other.join().unwrap();
        assert_eq!(other_finished.unwrap().unwrap(), 1);
        assert!(still_running);
        assert_eq!(closing_result.recv_timeout(WAIT).unwrap().unwrap(), Some(1));
        assert!(
            with_session(&stale, |_| Ok(()))
                .unwrap_err()
                .contains("closed")
        );
        assert_eq!(close_session(&sessions, 1).unwrap(), None);
    }
    #[test]
    fn failed_inference_only_poisons_its_session_and_cleanup_still_releases_it() {
        let sessions = fake_sessions();
        let entry = session_entry(&sessions, 1).unwrap();
        let failure = std::panic::catch_unwind(|| {
            let _: Result<(), String> = with_session(&entry, |_| panic!("fake inference failed"));
        });
        assert!(failure.is_err());
        assert!(with_session(&entry, |_| Ok(())).is_err());
        assert_eq!(
            with_session(&session_entry(&sessions, 2).unwrap(), |session| Ok(
                *session
            ))
            .unwrap(),
            0
        );
        assert_eq!(close_session(&sessions, 1).unwrap(), Some(0));
        assert!(entry.lock().unwrap_err().into_inner().is_none());
    }
    #[test]
    fn concurrent_initialization_loads_one_runtime_and_rejects_changes() {
        let runtime = Arc::new(OnceLock::new());
        let libraries = Arc::new(Mutex::new(Vec::new()));
        let start = Arc::new(Barrier::new(3));
        let path = std::env::temp_dir().join("lap-cuda-initialization-test");
        let mut workers = Vec::new();
        for _ in 0..2 {
            let runtime = runtime.clone();
            let libraries = libraries.clone();
            let start = start.clone();
            let path = path.clone();
            workers.push(thread::spawn(move || {
                start.wait();
                initialize_once(&runtime, &libraries, path, |libraries, _| {
                    libraries.push(());
                    Ok(())
                })
            }));
        }
        start.wait();
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
        assert_eq!(runtime.get(), Some(&path));
        assert_eq!(libraries.lock().unwrap().len(), 1);
        assert!(
            initialize_once(&runtime, &libraries, path.join("other"), |_, _| {
                panic!("An initialized runtime must not be loaded again")
            })
            .unwrap_err()
            .contains("runtime changed")
        );
    }
}
