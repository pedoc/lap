//! Versioned, bounded CPU tensor exchange. No Rust/ORT allocation crosses a DLL boundary.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const ABI: u32 = 1;
pub const MAX_BYTES: usize = 512 * 1024 * 1024;
const MAX_HEADER: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tensor {
    pub name: String,
    pub dtype: String,
    pub shape: Vec<i64>,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub bytes: usize,
}
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub op: String,
    #[serde(default)]
    pub session: u64,
    #[serde(default)]
    pub runtime: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub device: i32,
    #[serde(default)]
    pub threads: usize,
    #[serde(default)]
    pub memory_limit: usize,
    #[serde(default)]
    pub max_workspace: bool,
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub tensors: Vec<Tensor>,
    #[serde(default)]
    pub inputs: Vec<Tensor>,
    #[serde(default)]
    pub outputs: Vec<Tensor>,
    #[serde(default)]
    pub custom: BTreeMap<String, String>,
    #[serde(default)]
    pub operators: BTreeMap<String, u64>,
    #[serde(default)]
    pub error: Option<String>,
}
pub fn element_size(dtype: &str) -> Result<usize, String> {
    match dtype {
        "f32" | "i32" => Ok(4),
        "i64" | "f64" => Ok(8),
        "u8" | "bool" => Ok(1),
        _ => Err(format!("Unsupported CUDA bridge tensor type: {dtype}")),
    }
}
pub fn tensor_bytes(tensor: &Tensor) -> Result<usize, String> {
    if tensor.name.is_empty() || tensor.name.len() > 1024 || tensor.shape.len() > 16 {
        return Err("Invalid tensor identity or rank".into());
    }
    let elements = tensor.shape.iter().try_fold(1usize, |n, &dim| {
        if dim < 0 || dim > MAX_BYTES as i64 {
            return Err("Runtime tensor dimensions must not be negative");
        }
        n.checked_mul(dim as usize)
            .filter(|n| *n <= MAX_BYTES)
            .ok_or("Tensor shape exceeds runtime limit")
    })?;
    elements
        .checked_mul(element_size(&tensor.dtype)?)
        .filter(|n| *n <= MAX_BYTES)
        .ok_or_else(|| "Tensor exceeds runtime limit".into())
}
fn validate(message: &Message, payload: &[u8]) -> Result<(), String> {
    if payload.len() > MAX_BYTES || message.tensors.len() > 64 {
        return Err("CUDA tensor packet exceeds limit".into());
    }
    let mut end = 0;
    let mut names = std::collections::BTreeSet::new();
    for tensor in &message.tensors {
        if !names.insert(&tensor.name)
            || tensor.offset != end
            || tensor.bytes != tensor_bytes(tensor)?
        {
            return Err("Invalid tensor buffer layout".into());
        }
        end = end
            .checked_add(tensor.bytes)
            .ok_or("Tensor buffer overflow")?;
        if end > payload.len() {
            return Err("Truncated tensor data".into());
        }
    }
    if end != payload.len() {
        return Err("Unexpected tensor payload".into());
    }
    Ok(())
}
pub fn encode(message: &Message, payload: &[u8]) -> Result<Vec<u8>, String> {
    validate(message, payload)?;
    let json = serde_json::to_vec(message).map_err(|e| e.to_string())?;
    if json.len() > MAX_HEADER {
        return Err("CUDA runtime metadata exceeds limit".into());
    }
    let mut output = Vec::with_capacity(12 + json.len() + payload.len());
    output.extend_from_slice(b"LAPCUDA1");
    output.extend_from_slice(&(json.len() as u32).to_le_bytes());
    output.extend_from_slice(&json);
    output.extend_from_slice(payload);
    Ok(output)
}
pub fn decode(packet: &[u8]) -> Result<(Message, &[u8]), String> {
    if packet.len() < 12
        || packet.len() > MAX_BYTES + MAX_HEADER + 12
        || &packet[..8] != b"LAPCUDA1"
    {
        return Err("Invalid CUDA runtime protocol".into());
    }
    let size = u32::from_le_bytes(packet[8..12].try_into().map_err(|_| "Invalid header")?) as usize;
    if size > MAX_HEADER || size > packet.len() - 12 {
        return Err("Invalid CUDA metadata length".into());
    }
    let message: Message =
        serde_json::from_slice(&packet[12..12 + size]).map_err(|e| e.to_string())?;
    let payload = &packet[12 + size..];
    validate(&message, payload)?;
    Ok((message, payload))
}
#[repr(C)]
pub struct Buffer {
    pub data: *mut u8,
    pub len: usize,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packets_round_trip_without_inventing_tensor_bytes() {
        let message = Message {
            op: "run".into(),
            tensors: vec![Tensor {
                name: "x".into(),
                dtype: "f32".into(),
                shape: vec![1, 2],
                bytes: 8,
                ..Default::default()
            }],
            ..Default::default()
        };
        let packet = encode(&message, &[0; 8]).unwrap();
        let (m, b) = decode(&packet).unwrap();
        assert_eq!(m.tensors[0].shape, vec![1, 2]);
        assert_eq!(b.len(), 8);
    }
    #[test]
    fn malformed_shapes_types_duplicate_names_and_buffers_are_rejected() {
        for shape in [vec![-1], vec![i64::MAX, 2], vec![1; 17]] {
            assert!(
                tensor_bytes(&Tensor {
                    name: "x".into(),
                    dtype: "f32".into(),
                    shape,
                    ..Default::default()
                })
                .is_err()
            );
        }
        assert!(decode(b"bad").is_err());
        assert!(encode(&Message::default(), &[0]).is_err());
        let t = Tensor {
            name: "x".into(),
            dtype: "f32".into(),
            shape: vec![1],
            bytes: 4,
            ..Default::default()
        };
        assert!(
            encode(
                &Message {
                    tensors: vec![t.clone(), Tensor { offset: 4, ..t }],
                    ..Default::default()
                },
                &[0; 8]
            )
            .is_err()
        );
    }
}
