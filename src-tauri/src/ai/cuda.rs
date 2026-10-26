//! A small optional DLL isolates the CUDA ORT API from the host's CPU/DirectML API.
use lap_ai_runtime_wire::{ABI, Buffer, Message};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
#[path = "../../../crates/cuda-bridge/src/tensors.rs"]
pub mod tensors;
type Call = unsafe extern "C" fn(*const u8, usize) -> Buffer;
type Free = unsafe extern "C" fn(Buffer);
struct Bridge {
    _library: libloading::Library,
    call: Call,
    free: Free,
}
static BRIDGE: Mutex<Option<Arc<Bridge>>> = Mutex::new(None);
pub fn bridge_path() -> Option<PathBuf> {
    let parent = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let mut candidates = vec![
        parent.join("resources/lap_cuda_bridge.dll"),
        parent.join("lap_cuda_bridge.dll"),
    ];
    #[cfg(debug_assertions)]
    candidates
        .push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/lap_cuda_bridge.dll"));
    candidates.into_iter().find(|path| path.is_file())
}
fn bridge() -> Result<Arc<Bridge>, String> {
    let mut state = BRIDGE.lock().map_err(|e| e.to_string())?;
    if let Some(bridge) = state.as_ref() {
        return Ok(bridge.clone());
    }
    let file =
        bridge_path().ok_or("This package is missing its CUDA bridge; rebuild/reinstall Lap")?;
    let library = unsafe {
        libloading::os::windows::Library::load_with_flags(&file, 0x00000100 | 0x00001000)
    }
    .map_err(|e| format!("Cannot load bundled CUDA bridge: {e}"))?;
    let library: libloading::Library = library.into();
    let version = unsafe { library.get::<unsafe extern "C" fn() -> u32>(b"lap_cuda_abi\0") }
        .map_err(|e| e.to_string())?;
    if unsafe { version() } != ABI {
        return Err("CUDA bridge version mismatch; reinstall Lap".into());
    }
    let call = *unsafe { library.get::<Call>(b"lap_cuda_call\0") }.map_err(|e| e.to_string())?;
    let free = *unsafe { library.get::<Free>(b"lap_cuda_free\0") }.map_err(|e| e.to_string())?;
    let bridge = Arc::new(Bridge {
        _library: library,
        call,
        free,
    });
    *state = Some(bridge.clone());
    Ok(bridge)
}
impl Bridge {
    fn invoke(&self, message: Message, payload: &[u8]) -> Result<(Message, Vec<u8>), String> {
        let request = lap_ai_runtime_wire::encode(&message, payload)?;
        let result = unsafe { (self.call)(request.as_ptr(), request.len()) };
        if result.data.is_null() {
            return Err("CUDA bridge returned an empty response".into());
        }
        struct Reply<'a> {
            buffer: Buffer,
            bridge: &'a Bridge,
        }
        impl Drop for Reply<'_> {
            fn drop(&mut self) {
                unsafe {
                    (self.bridge.free)(Buffer {
                        data: self.buffer.data,
                        len: self.buffer.len,
                    })
                };
            }
        }
        let reply = Reply {
            buffer: result,
            bridge: self,
        };
        if reply.buffer.len > lap_ai_runtime_wire::MAX_BYTES + 8 * 1024 * 1024 + 12 {
            return Err("CUDA response exceeds limit".into());
        }
        let bytes = unsafe { std::slice::from_raw_parts(reply.buffer.data, reply.buffer.len) };
        let (message, data) = lap_ai_runtime_wire::decode(bytes)?;
        if let Some(error) = message.error {
            return Err(error);
        }
        Ok((message, data.to_vec()))
    }
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: i32,
    pub name: String,
    pub memory_bytes: u64,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Driver {
    pub available: bool,
    pub version: i32,
    pub devices: Vec<Device>,
    pub reason: String,
}
pub fn driver() -> Driver {
    fn detect() -> Result<Driver, String> {
        let library =
            unsafe { libloading::os::windows::Library::load_with_flags("nvcuda.dll", 0x00000800) }
                .map_err(|e| format!("NVIDIA CUDA driver is unavailable: {e}"))?;
        macro_rules! symbol {
            ($name:literal,$ty:ty) => {
                unsafe { library.get::<$ty>(concat!($name, "\0").as_bytes()) }
                    .map_err(|e| e.to_string())?
            };
        }
        let init = symbol!("cuInit", unsafe extern "system" fn(u32) -> i32);
        let code = unsafe { init(0) };
        if code != 0 {
            return Err(format!(
                "NVIDIA CUDA driver initialization failed (code {code})"
            ));
        }
        let count_fn = symbol!(
            "cuDeviceGetCount",
            unsafe extern "system" fn(*mut i32) -> i32
        );
        let mut count = 0;
        if unsafe { count_fn(&mut count) } != 0 || count <= 0 {
            return Err("No CUDA-capable NVIDIA device is available".into());
        }
        let name_fn = symbol!(
            "cuDeviceGetName",
            unsafe extern "system" fn(*mut std::ffi::c_char, i32, i32) -> i32
        );
        let device_fn = symbol!(
            "cuDeviceGet",
            unsafe extern "system" fn(*mut i32, i32) -> i32
        );
        let memory_fn = symbol!(
            "cuDeviceTotalMem_v2",
            unsafe extern "system" fn(*mut usize, i32) -> i32
        );
        let version_fn = symbol!(
            "cuDriverGetVersion",
            unsafe extern "system" fn(*mut i32) -> i32
        );
        let mut version = 0;
        unsafe { version_fn(&mut version) };
        let mut devices = Vec::new();
        for id in 0..count.min(32) {
            let mut device = 0;
            if unsafe { device_fn(&mut device, id) } != 0 {
                continue;
            }
            let mut name = [0i8; 256];
            let mut memory = 0;
            if unsafe { name_fn(name.as_mut_ptr(), 256, device) } == 0
                && unsafe { memory_fn(&mut memory, device) } == 0
            {
                name[255] = 0;
                let name = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }
                    .to_string_lossy()
                    .into_owned();
                devices.push(Device {
                    id,
                    name,
                    memory_bytes: memory as u64,
                });
            }
        }
        if devices.is_empty() {
            return Err("NVIDIA CUDA device enumeration failed".into());
        }
        Ok(Driver {
            available: !devices.is_empty(),
            version,
            devices,
            reason: String::new(),
        })
    }
    detect().unwrap_or_else(|reason| Driver {
        available: false,
        version: 0,
        devices: Vec::new(),
        reason,
    })
}
pub fn probe() -> Result<BTreeMap<String, u64>, String> {
    let root = super::cuda_assets::installed_root()?;
    let bridge = bridge()?;
    bridge.invoke(
        Message {
            op: "init".into(),
            runtime: root.to_string_lossy().into_owned(),
            ..Default::default()
        },
        &[],
    )?;
    let (response, _) = bridge.invoke(
        Message {
            op: "probe".into(),
            device: 0,
            ..Default::default()
        },
        &[],
    )?;
    Ok(response.operators)
}
pub struct CudaSession {
    bridge: Arc<Bridge>,
    id: u64,
    pub inputs: Vec<ort::session::Input>,
    pub outputs: Vec<ort::session::Output>,
    custom: BTreeMap<String, String>,
}
impl CudaSession {
    pub fn open(
        model: &super::types::ResolvedModel,
        role: &str,
        device: i32,
        trace: &Option<PathBuf>,
    ) -> Result<Self, String> {
        let root = super::cuda_assets::installed_root()?;
        let bridge = bridge()?;
        bridge.invoke(
            Message {
                op: "init".into(),
                runtime: root.to_string_lossy().into_owned(),
                ..Default::default()
            },
            &[],
        )?;
        let path = super::assets::artifact_path(&model.definition, role)?;
        let (response, _) = bridge.invoke(
            Message {
                op: "open".into(),
                model: path.to_string_lossy().into_owned(),
                device,
                threads: (model.number("threads") as usize).clamp(1, 128),
                memory_limit: model
                    .values
                    .get("cuda_memory_limit_mb")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.)
                    .clamp(0., 65536.) as usize
                    * 1024
                    * 1024,
                max_workspace: model
                    .values
                    .get("cuda_max_workspace")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                profile: trace
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                ..Default::default()
            },
            &[],
        )?;
        let inputs = response
            .inputs
            .iter()
            .map(|d| {
                tensors::value_type(d).map(|input_type| ort::session::Input {
                    name: d.name.clone(),
                    input_type,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let outputs = response
            .outputs
            .iter()
            .map(|d| {
                tensors::value_type(d).map(|output_type| ort::session::Output {
                    name: d.name.clone(),
                    output_type,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            bridge,
            id: response.session,
            inputs,
            outputs,
            custom: response.custom,
        })
    }
    pub fn custom(&self, key: &str) -> Option<String> {
        self.custom.get(key).cloned()
    }
    pub fn run<'i, 'v: 'i, const N: usize>(
        &self,
        inputs: ort::session::SessionInputs<'i, 'v, N>,
    ) -> Result<Vec<(String, ort::value::DynValue)>, String> {
        let mut payload = Vec::new();
        let mut tensors = Vec::new();
        match inputs {
            ort::session::SessionInputs::ValueMap(values) => {
                for (name, value) in values {
                    tensors.push(self::tensors::append(&name, &value, &mut payload)?);
                }
            }
            ort::session::SessionInputs::ValueSlice(values) => {
                if values.len() != self.inputs.len() {
                    return Err("CUDA input count mismatch".into());
                }
                for (value, input) in values.iter().zip(&self.inputs) {
                    tensors.push(self::tensors::append(&input.name, value, &mut payload)?);
                }
            }
            ort::session::SessionInputs::ValueArray(values) => {
                if values.len() != self.inputs.len() {
                    return Err("CUDA input count mismatch".into());
                }
                for (value, input) in values.iter().zip(&self.inputs) {
                    tensors.push(self::tensors::append(&input.name, value, &mut payload)?);
                }
            }
        }
        let (response, data) = self.bridge.invoke(
            Message {
                op: "run".into(),
                session: self.id,
                tensors,
                ..Default::default()
            },
            &payload,
        )?;
        response
            .tensors
            .iter()
            .map(|d| self::tensors::materialize(d, &data).map(|value| (d.name.clone(), value)))
            .collect()
    }
    pub fn close(&mut self) -> BTreeMap<String, u64> {
        if self.id == 0 {
            return BTreeMap::new();
        }
        let id = std::mem::take(&mut self.id);
        self.bridge
            .invoke(
                Message {
                    op: "close".into(),
                    session: id,
                    ..Default::default()
                },
                &[],
            )
            .map(|(m, _)| m.operators)
            .unwrap_or_default()
    }
}
impl Drop for CudaSession {
    fn drop(&mut self) {
        self.close();
    }
}
