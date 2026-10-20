//! Compiled separately in the host and bridge; each owns its own ORT values.
use lap_ai_runtime_wire::{MAX_BYTES, Tensor as Descriptor};
use ort::{
    tensor::{Shape, SymbolicDimensions, TensorElementType},
    value::{DynValue, Tensor, ValueType},
};
pub fn dtype(value: &ValueType) -> Result<String, String> {
    let ty = value
        .tensor_type()
        .ok_or("CUDA bridge supports tensor I/O only")?;
    Ok(match ty {
        TensorElementType::Float32 => "f32",
        TensorElementType::Float64 => "f64",
        TensorElementType::Int64 => "i64",
        TensorElementType::Int32 => "i32",
        TensorElementType::Uint8 => "u8",
        TensorElementType::Bool => "bool",
        _ => return Err(format!("Unsupported tensor type: {ty:?}")),
    }
    .into())
}
pub fn describe(name: &str, value: &ValueType) -> Result<Descriptor, String> {
    let shape = value
        .tensor_shape()
        .ok_or("CUDA bridge supports tensor I/O only")?
        .to_vec();
    if name.is_empty() || name.len() > 1024 || shape.len() > 16 {
        return Err("Unsupported CUDA tensor identity or rank".into());
    }
    Ok(Descriptor {
        name: name.into(),
        dtype: dtype(value)?,
        shape,
        ..Default::default()
    })
}
#[allow(dead_code)] // Also compiled into the host, where this metadata conversion is used.
pub fn value_type(description: &Descriptor) -> Result<ValueType, String> {
    let ty = match description.dtype.as_str() {
        "f32" => TensorElementType::Float32,
        "f64" => TensorElementType::Float64,
        "i64" => TensorElementType::Int64,
        "i32" => TensorElementType::Int32,
        "u8" => TensorElementType::Uint8,
        "bool" => TensorElementType::Bool,
        _ => return Err("Unsupported CUDA I/O type".into()),
    };
    Ok(ValueType::Tensor {
        ty,
        shape: Shape::new(description.shape.iter().copied()),
        dimension_symbols: SymbolicDimensions::new(vec![String::new(); description.shape.len()]),
    })
}
pub fn append(name: &str, value: &DynValue, payload: &mut Vec<u8>) -> Result<Descriptor, String> {
    let mut d = describe(name, value.dtype())?;
    d.offset = payload.len();
    d.bytes = lap_ai_runtime_wire::tensor_bytes(&d)?;
    if payload
        .len()
        .checked_add(d.bytes)
        .is_none_or(|n| n > MAX_BYTES)
    {
        return Err("CUDA tensor payload exceeds limit".into());
    }
    macro_rules! numeric {
        ($ty:ty) => {{
            let (_, data) = value
                .try_extract_tensor::<$ty>()
                .map_err(|e| e.to_string())?;
            for &v in data {
                payload.extend_from_slice(&v.to_le_bytes());
            }
        }};
    }
    match d.dtype.as_str() {
        "f32" => numeric!(f32),
        "f64" => numeric!(f64),
        "i64" => numeric!(i64),
        "i32" => numeric!(i32),
        "u8" => {
            let (_, data) = value
                .try_extract_tensor::<u8>()
                .map_err(|e| e.to_string())?;
            payload.extend_from_slice(data);
        }
        "bool" => {
            let (_, data) = value
                .try_extract_tensor::<bool>()
                .map_err(|e| e.to_string())?;
            payload.extend(data.iter().map(|v| u8::from(*v)));
        }
        _ => return Err("Unsupported CUDA tensor type".into()),
    }
    Ok(d)
}
pub fn materialize(d: &Descriptor, payload: &[u8]) -> Result<DynValue, String> {
    let end = d
        .offset
        .checked_add(d.bytes)
        .ok_or("Tensor offset overflow")?;
    let data = payload.get(d.offset..end).ok_or("Truncated CUDA tensor")?;
    macro_rules! numeric {
        ($ty:ty,$n:expr) => {{
            let values: Vec<$ty> = data
                .chunks_exact($n)
                .map(|chunk| {
                    <$ty>::from_le_bytes(chunk.try_into().expect("validated element width"))
                })
                .collect();
            Tensor::from_array((d.shape.clone(), values))
                .map(|v| v.into_dyn())
                .map_err(|e| e.to_string())
        }};
    }
    match d.dtype.as_str() {
        "f32" => numeric!(f32, 4),
        "f64" => numeric!(f64, 8),
        "i64" => numeric!(i64, 8),
        "i32" => numeric!(i32, 4),
        "u8" => Tensor::from_array((d.shape.clone(), data.to_vec()))
            .map(|v| v.into_dyn())
            .map_err(|e| e.to_string()),
        "bool" => {
            if data.iter().any(|b| *b > 1) {
                return Err("Invalid boolean tensor".into());
            }
            Tensor::from_array((
                d.shape.clone(),
                data.iter().map(|b| *b != 0).collect::<Vec<_>>(),
            ))
            .map(|v| v.into_dyn())
            .map_err(|e| e.to_string())
        }
        _ => Err("Unsupported CUDA tensor type".into()),
    }
}
