//! Evidence-based, independent media labels. Never use names or aspect ratios.
use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};
use rusqlite::{Connection, OptionalExtension, params};
use std::{collections::BTreeMap, io::Read, path::Path};

pub const KINDS: [&str; 4] = ["animated", "screenshot", "burst", "panorama"];
const VERSION: i64 = 1;
const LIMIT: u64 = 8 * 1024 * 1024;
const GPANO: &[u8] = b"http://ns.google.com/photos/1.0/panorama/";

pub fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS media_traits (
        file_id INTEGER PRIMARY KEY REFERENCES afiles(id) ON DELETE CASCADE,
        modified_at INTEGER, size INTEGER, version INTEGER NOT NULL, evidence TEXT NOT NULL
    );
    CREATE TRIGGER IF NOT EXISTS media_traits_delete AFTER DELETE ON afiles BEGIN
        DELETE FROM media_traits WHERE file_id=OLD.id;
    END;",
    )
    .map_err(|e| e.to_string())
}

pub fn predicate(kind: &str) -> Result<String, String> {
    if !KINDS.contains(&kind) {
        return Err("Unknown media classification".into());
    }
    Ok(format!(
        "EXISTS (SELECT 1 FROM media_traits mt WHERE mt.file_id=a.id AND mt.version={VERSION} AND mt.modified_at IS a.modified_at AND mt.size IS a.size AND json_extract(mt.evidence,'$.{kind}') IS NOT NULL)"
    ))
}

pub fn refresh(
    conn: &Connection,
    id: i64,
    path: &Path,
    file_type: i64,
    modified: Option<i64>,
    size: Option<i64>,
    software: Option<&str>,
) -> Result<(), String> {
    let current = conn.query_row("SELECT 1 FROM media_traits WHERE file_id=?1 AND modified_at IS ?2 AND size IS ?3 AND version=?4", params![id,modified,size,VERSION], |_| Ok(())).optional().map_err(|e| e.to_string())?.is_some();
    if current {
        return Ok(());
    }
    let mut evidence = BTreeMap::new();
    if matches!(file_type, 1 | 3) {
        let mut bytes = Vec::new();
        // Read errors remain unindexed and retry on the next scan.
        let Ok(mut file) = std::fs::File::open(path) else {
            return Ok(());
        };
        if file
            .by_ref()
            .take(128 * 1024)
            .read_to_end(&mut bytes)
            .is_err()
        {
            return Ok(());
        }
        if (bytes.starts_with(b"GIF8") || bytes.get(8..12) == Some(b"WEBP"))
            && file
                .take(LIMIT - bytes.len() as u64)
                .read_to_end(&mut bytes)
                .is_err()
        {
            return Ok(());
        }
        evidence = classify(&bytes, software);
        if let Ok(exif) = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&bytes))
        {
            if let Some(field) = exif.get_field(exif::Tag::MakerNote, exif::In::PRIMARY) {
                if let exif::Value::Undefined(data, _) = &field.value {
                    if apple_burst(data) {
                        evidence.insert("burst".into(), "Apple MakerNote BurstUUID".into());
                    }
                }
            }
        }
    }
    let json = serde_json::to_string(&evidence).map_err(|e| e.to_string())?;
    // Do not publish evidence if another scan changed the row while reading.
    conn.execute("INSERT INTO media_traits(file_id,modified_at,size,version,evidence)
        SELECT id,?2,?3,?4,?5 FROM afiles WHERE id=?1 AND modified_at IS ?2 AND size IS ?3
        ON CONFLICT(file_id) DO UPDATE SET modified_at=excluded.modified_at,size=excluded.size,version=excluded.version,evidence=excluded.evidence", params![id,modified,size,VERSION,json]).map_err(|e|e.to_string())?;
    Ok(())
}

fn classify(bytes: &[u8], software: Option<&str>) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    if animated(bytes) {
        result.insert(
            "animated".into(),
            "Multiple frames in GIF / APNG / WebP container".into(),
        );
    }
    // Exact producer values only; generic image editors and filenames are not evidence.
    if let Some(name) = software.filter(|name| {
        matches!(
            name.trim(),
            "gnome-screenshot" | "Spectacle" | "Snipping Tool" | "Snip & Sketch" | "Flameshot"
        )
    }) {
        result.insert("screenshot".into(), format!("Embedded Software: {name}"));
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        for (kind, chunk) in png_chunks(bytes) {
            let value = if kind == b"tEXt" {
                chunk.strip_prefix(b"Software\0")
            } else if kind == b"iTXt" {
                chunk.strip_prefix(b"Software\0\0\0\0\0")
            } else {
                None
            };
            if let Some(value) = value.and_then(|v| std::str::from_utf8(v).ok()) {
                if matches!(
                    value.trim(),
                    "gnome-screenshot"
                        | "Spectacle"
                        | "Snipping Tool"
                        | "Snip & Sketch"
                        | "Flameshot"
                ) {
                    result.insert("screenshot".into(), format!("PNG Software: {value}"));
                }
            }
        }
    }
    for packet in xmp_packets(bytes) {
        if panorama(packet) {
            result.insert(
                "panorama".into(),
                "GPano ProjectionType=equirectangular; UsePanoramaViewer=True".into(),
            );
        }
    }
    result
}

fn xmp_packets(bytes: &[u8]) -> Vec<&[u8]> {
    let mut packets = Vec::new();
    if bytes.starts_with(b"\xff\xd8") {
        let mut pos = 2;
        while bytes.get(pos) == Some(&0xff) {
            while bytes.get(pos) == Some(&0xff) {
                pos += 1;
            }
            let Some(&marker) = bytes.get(pos) else {
                break;
            };
            pos += 1;
            if matches!(marker, 0xda | 0xd9) {
                break;
            }
            let Some(len) = bytes
                .get(pos..pos + 2)
                .map(|v| u16::from_be_bytes([v[0], v[1]]) as usize)
            else {
                break;
            };
            if len < 2 {
                break;
            }
            let Some(segment) = bytes.get(pos + 2..pos + len) else {
                break;
            };
            if marker == 0xe1 {
                if let Some(xml) = segment.strip_prefix(b"http://ns.adobe.com/xap/1.0/\0") {
                    packets.push(xml);
                }
            }
            pos += len;
        }
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        for (kind, chunk) in png_chunks(bytes) {
            if kind == b"iTXt" {
                if let Some(value) = chunk.strip_prefix(b"XML:com.adobe.xmp\0\0\0\0\0") {
                    packets.push(value);
                }
            }
        }
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        let mut pos = 12;
        while let Some(header) = bytes.get(pos..pos + 8) {
            let len = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
            let Some(end) = pos.checked_add(8).and_then(|p| p.checked_add(len)) else {
                break;
            };
            let Some(chunk) = bytes.get(pos + 8..end) else {
                break;
            };
            if &header[..4] == b"XMP " {
                packets.push(chunk);
            }
            pos = end + (len % 2);
        }
    }
    packets
}
fn png_chunks(bytes: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut out = Vec::new();
    let mut pos: usize = 8;
    while let Some(header) = bytes.get(pos..pos + 8) {
        let len = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let Some(end) = pos.checked_add(8).and_then(|p| p.checked_add(len)) else {
            break;
        };
        let Some(chunk) = bytes.get(pos + 8..end) else {
            break;
        };
        if &header[4..] == b"IDAT" {
            break;
        }
        out.push((&header[4..], chunk));
        pos = end + 4;
    }
    out
}
fn panorama(xml: &[u8]) -> bool {
    let mut reader = NsReader::from_reader(xml);
    let mut projection = false;
    let mut viewer = false;
    let mut active = Vec::new();
    loop {
        match reader.read_resolved_event() {
            Ok((ns, Event::Start(e) | Event::Empty(e))) => {
                active = if matches!(ns, ResolveResult::Bound(n) if n.as_ref()==GPANO) {
                    e.local_name().as_ref().to_vec()
                } else {
                    Vec::new()
                };
                for attr in e.attributes() {
                    let Ok(attr) = attr else {
                        return false;
                    };
                    let (ns, local) = reader.resolve_attribute(attr.key);
                    if matches!(ns, ResolveResult::Bound(n) if n.as_ref()==GPANO) {
                        match (local.as_ref(), attr.value.as_ref()) {
                            (b"ProjectionType", b"equirectangular") => projection = true,
                            (b"UsePanoramaViewer", b"True" | b"true" | b"1") => viewer = true,
                            _ => {}
                        }
                    }
                }
            }
            Ok((_, Event::Text(e))) => match (active.as_slice(), &e[..]) {
                (b"ProjectionType", b"equirectangular") => projection = true,
                (b"UsePanoramaViewer", b"True" | b"true" | b"1") => viewer = true,
                _ => {}
            },
            Ok((_, Event::End(_))) => active.clear(),
            Ok((_, Event::Eof)) => return projection && viewer,
            Err(_) => return false,
            _ => {}
        }
    }
}
fn apple_burst(data: &[u8]) -> bool {
    if !data.starts_with(b"Apple iOS\0\0\x01MM") || data.len() < 16 {
        return false;
    }
    let count = u16::from_be_bytes([data[14], data[15]]) as usize;
    for i in 0..count.min(4096) {
        let Some(entry) = data.get(16 + i * 12..28 + i * 12) else {
            return false;
        };
        if entry[..4] != [0, 11, 0, 2] {
            continue;
        } // ASCII BurstUUID, not ContentIdentifier.
        let len = u32::from_be_bytes(entry[4..8].try_into().unwrap()) as usize;
        let offset = u32::from_be_bytes(entry[8..12].try_into().unwrap()) as usize;
        if len != 37 {
            continue;
        }
        if let Some(uuid) = data.get(offset..offset.saturating_add(36)) {
            if uuid.iter().enumerate().all(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    *c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            }) {
                return true;
            }
        }
    }
    false
}
fn animated(data: &[u8]) -> bool {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut pos = 8;
        while let Some(header) = data.get(pos..pos + 8) {
            let len = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
            if &header[4..] == b"acTL" && len == 8 {
                return data
                    .get(pos + 8..pos + 16)
                    .is_some_and(|v| u32::from_be_bytes(v[..4].try_into().unwrap()) > 1);
            }
            if &header[4..] == b"IDAT" {
                return false;
            }
            let Some(next) = pos.checked_add(12).and_then(|p| p.checked_add(len)) else {
                return false;
            };
            pos = next;
        }
    } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        let mut pos = 12;
        let mut frames = 0;
        while let Some(header) = data.get(pos..pos + 8) {
            let len = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
            let Some(end) = pos.checked_add(8).and_then(|p| p.checked_add(len)) else {
                return false;
            };
            if end > data.len() {
                return false;
            }
            if &header[..4] == b"ANMF" && len >= 16 {
                frames += 1;
                if frames >= 2 {
                    return true;
                }
            }
            pos = end + (len % 2);
        }
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        if data.len() < 13 {
            return false;
        }
        let mut pos = 13
            + if data[10] & 0x80 != 0 {
                3 * (1usize << ((data[10] & 7) + 1))
            } else {
                0
            };
        let mut frames = 0;
        while let Some(&marker) = data.get(pos) {
            pos += 1;
            match marker {
                0x2c => {
                    let Some(desc) = data.get(pos..pos + 9) else {
                        return false;
                    };
                    pos += 9;
                    if desc[8] & 0x80 != 0 {
                        pos += 3 * (1usize << ((desc[8] & 7) + 1));
                    }
                    pos += 1; // LZW minimum code size
                    if !skip_blocks(data, &mut pos) {
                        return false;
                    }
                    frames += 1;
                    if frames >= 2 {
                        return true;
                    }
                }
                0x21 => {
                    pos += 1;
                    if !skip_blocks(data, &mut pos) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
    }
    false
}
fn skip_blocks(data: &[u8], pos: &mut usize) -> bool {
    loop {
        let Some(&len) = data.get(*pos) else {
            return false;
        };
        *pos += 1;
        if len == 0 {
            return true;
        }
        *pos += len as usize;
        if *pos > data.len() {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_filename_or_ratio_guessing() {
        assert!(classify(b"Screenshot_2026 panorama burst.gif", Some("Photoshop")).is_empty());
    }
    #[test]
    fn known_capture_producer() {
        assert!(classify(&[], Some("Spectacle")).contains_key("screenshot"));
    }
    #[test]
    fn namespace_aware_panorama() {
        let xml=br#"<x:xmpmeta xmlns:x="adobe:ns:meta/" xmlns:p="http://ns.google.com/photos/1.0/panorama/"><p:meta p:ProjectionType="equirectangular" p:UsePanoramaViewer="True"/></x:xmpmeta>"#;
        assert!(panorama(xml));
        assert!(classify(xml, None).is_empty()); // arbitrary file contents are not metadata
        let mut jpeg = b"\xff\xd8\xff\xe1".to_vec();
        let mut app = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
        app.extend(xml);
        jpeg.extend(((app.len() + 2) as u16).to_be_bytes());
        jpeg.extend(app);
        assert!(classify(&jpeg, None).contains_key("panorama"));
        assert!(!panorama(br#"<m xmlns:p="not-gpano" p:ProjectionType="equirectangular" p:UsePanoramaViewer="True"/>"#));
        assert!(!panorama(br#"<m xmlns:p="http://ns.google.com/photos/1.0/panorama/" p:ProjectionType="equirectangular"/>"#));
    }
    #[test]
    fn static_and_multiframe_containers() {
        let mut gif = b"GIF89a\x01\0\x01\0\0\0\0".to_vec();
        let frame = b"\x2c\0\0\0\0\x01\0\x01\0\0\x02\x01\0\0";
        gif.extend(frame);
        assert!(!animated(&gif));
        gif.extend(frame);
        assert!(animated(&gif));
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x08acTL\0\0\0\x02\0\0\0\0".to_vec();
        assert!(animated(&png));
        png[19] = 1;
        assert!(!animated(&png));
        let mut webp = b"RIFF\0\0\0\0WEBP".to_vec();
        for _ in 0..2 {
            webp.extend(b"ANMF\x10\0\0\0");
            webp.extend([0; 16]);
        }
        assert!(animated(&webp));
        for end in 0..gif.len() {
            let _ = animated(&gif[..end]);
        } // truncated inputs must not panic
    }
    #[test]
    fn apple_burst_uuid_only() {
        let mut data = b"Apple iOS\0\0\x01MM\0\x01\0\x0b\0\x02\0\0\0\x25\0\0\0\x1c".to_vec();
        data.extend(b"12345678-1234-1234-1234-123456789abc\0");
        assert!(apple_burst(&data));
        data[17] = 0x11;
        assert!(!apple_burst(&data));
    }
    #[test]
    fn versioned_independent_labels_and_source_invalidation() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE afiles(id INTEGER PRIMARY KEY,modified_at INTEGER,size INTEGER); INSERT INTO afiles VALUES(1,10,20);").unwrap();
        ensure_schema(&c).unwrap();
        c.execute(
            "INSERT INTO media_traits VALUES(1,10,20,1,?1)",
            [r#"{"burst":"uuid","panorama":"gpano"}"#],
        )
        .unwrap();
        for kind in ["burst", "panorama"] {
            let sql = format!("SELECT {} FROM afiles a", predicate(kind).unwrap());
            assert!(c.query_row(&sql, [], |r| r.get::<_, bool>(0)).unwrap());
        }
        c.execute("UPDATE afiles SET modified_at=11", []).unwrap();
        assert!(
            !c.query_row(
                &format!("SELECT {} FROM afiles a", predicate("burst").unwrap()),
                [],
                |r| r.get::<_, bool>(0)
            )
            .unwrap()
        );
        c.execute("DELETE FROM afiles", []).unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM media_traits", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(predicate("burst');DROP TABLE afiles;--").is_err());
    }
}
