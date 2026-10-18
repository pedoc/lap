//! Minimal ONNX metadata rewrite for DirectML compatibility.
//!
//! PyTorch exports `Reshape` nodes with `allowzero=1` even when the target
//! shape contains no zero dimensions. ONNX Runtime's DirectML EP rejects that
//! combination while creating a session. The attribute is a varint in the
//! protobuf, so it can be flipped to the default `0` without reserializing the
//! model or touching the checksum-pinned artifact on disk.

const MAX_PATCHES: usize = 4096;

fn read_varint(bytes: &[u8], position: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*position)?;
        *position += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn fields<'a>(
    bytes: &'a [u8],
    mut visit: impl FnMut(u64, u8, usize, usize, &'a [u8]) -> Option<()>,
) -> Option<()> {
    let mut position = 0usize;
    while position < bytes.len() {
        let key = read_varint(bytes, &mut position)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        let start;
        let length;
        match wire {
            0 => {
                start = position;
                read_varint(bytes, &mut position)?;
                length = position - start;
            }
            1 => {
                start = position;
                position = position.checked_add(8)?;
                if position > bytes.len() {
                    return None;
                }
                length = 8;
            }
            2 => {
                length = usize::try_from(read_varint(bytes, &mut position)?).ok()?;
                start = position;
                position = position.checked_add(length)?;
                if position > bytes.len() {
                    return None;
                }
            }
            5 => {
                start = position;
                position = position.checked_add(4)?;
                if position > bytes.len() {
                    return None;
                }
                length = 4;
            }
            _ => return None,
        }
        visit(field, wire, start, length, &bytes[start..position])?;
    }
    Some(())
}

fn patch_offsets(model: &[u8]) -> Result<Vec<usize>, ()> {
    let mut graph = None;
    fields(model, |field, wire, start, length, _| {
        if field == 7 && wire == 2 {
            graph = Some((start, length));
        }
        Some(())
    })
    .ok_or(())?;
    let (graph_start, graph_length) = graph.ok_or(())?;
    let graph_bytes = model
        .get(graph_start..graph_start + graph_length)
        .ok_or(())?;
    let mut nodes = Vec::new();
    fields(graph_bytes, |field, wire, start, length, _| {
        if field == 1 && wire == 2 {
            nodes.push((graph_start + start, length));
        }
        Some(())
    })
    .ok_or(())?;

    let mut patches = Vec::new();
    for (node_start, node_length) in nodes {
        let node = model.get(node_start..node_start + node_length).ok_or(())?;
        let mut operation = None;
        let mut attributes = Vec::new();
        fields(node, |field, wire, start, length, _| {
            if field == 4 && wire == 2 {
                operation = Some(&node[start..start + length]);
            }
            if field == 5 && wire == 2 {
                attributes.push((node_start + start, length));
            }
            Some(())
        })
        .ok_or(())?;
        if operation != Some(b"Reshape".as_slice()) {
            continue;
        }
        for (attribute_start, attribute_length) in attributes {
            let attribute = model
                .get(attribute_start..attribute_start + attribute_length)
                .ok_or(())?;
            let mut name = None;
            let mut value = None;
            fields(attribute, |field, wire, start, length, raw| {
                if field == 1 && wire == 2 {
                    name = Some(&attribute[start..start + length]);
                }
                if field == 3 && wire == 0 {
                    value = Some((attribute_start + start, raw));
                }
                Some(())
            })
            .ok_or(())?;
            if name != Some(b"allowzero".as_slice()) {
                continue;
            }
            if let Some((offset, raw)) = value {
                if raw.len() == 1 && raw[0] == 1 {
                    patches.push(offset);
                }
            }
        }
        if patches.len() > MAX_PATCHES {
            return Err(());
        }
    }
    Ok(patches)
}

/// Only call for the checksum-pinned export selected by the runtime compatibility guard.
/// Its constant target shapes contain no zero dimensions. Malformed input is unchanged.
pub(super) fn disable_reshape_allowzero(model: &mut [u8]) -> usize {
    let Ok(patches) = patch_offsets(model) else {
        return 0;
    };
    for offset in &patches {
        model[*offset] = 0;
    }
    patches.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint(mut value: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            bytes.push(byte);
            if value == 0 {
                return bytes;
            }
        }
    }
    fn bytes_field(field: u64, payload: &[u8]) -> Vec<u8> {
        let mut bytes = varint((field << 3) | 2);
        bytes.extend(varint(payload.len() as u64));
        bytes.extend_from_slice(payload);
        bytes
    }
    fn varint_field(field: u64, value: u64) -> Vec<u8> {
        let mut bytes = varint(field << 3);
        bytes.extend(varint(value));
        bytes
    }
    fn node(operation: &str, allowzero: u64) -> Vec<u8> {
        let mut attribute = bytes_field(1, b"allowzero");
        attribute.extend(varint_field(3, allowzero));
        let mut node = bytes_field(4, operation.as_bytes());
        node.extend(bytes_field(5, &attribute));
        node
    }
    fn model(nodes: &[Vec<u8>]) -> Vec<u8> {
        let mut graph = Vec::new();
        for node in nodes {
            graph.extend(bytes_field(1, node));
        }
        bytes_field(7, &graph)
    }

    #[test]
    fn only_reshape_allowzero_is_flipped() {
        let mut source = model(&[node("Reshape", 1), node("Conv", 1), node("Reshape", 0)]);
        assert_eq!(disable_reshape_allowzero(&mut source), 1);
        let mut again = model(&[node("Reshape", 1), node("Conv", 1)]);
        assert_eq!(disable_reshape_allowzero(&mut again), 1);
        assert_eq!(disable_reshape_allowzero(&mut again), 0);
    }

    #[test]
    fn malformed_input_is_left_untouched() {
        let mut source = vec![0x3a, 0xff];
        let before = source.clone();
        assert_eq!(disable_reshape_allowzero(&mut source), 0);
        assert_eq!(source, before);
    }
}
