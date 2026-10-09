//! Scope validation and atomic re-detection; never turns an empty selection into a library scan.
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use std::collections::{BTreeSet, HashSet};

#[derive(Clone)]
pub struct FaceScope {
    pub file_ids: Option<Vec<i64>>,
    pub force: bool,
}
impl FaceScope {
    pub fn new(file_ids: Option<Vec<i64>>, force: bool) -> Result<Self, String> {
        let file_ids = match file_ids {
            None if force => return Err("Re-detection requires an explicit image selection".into()),
            None => None,
            Some(ids) => {
                if ids.is_empty() || ids.len() > 10000 || ids.iter().any(|id| *id <= 0) {
                    return Err("Select between 1 and 10000 valid images".into());
                }
                Some(
                    ids.into_iter()
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                )
            }
        };
        Ok(Self { file_ids, force })
    }
    pub fn key(&self) -> &'static str {
        if self.file_ids.is_some() {
            "selection"
        } else {
            "library"
        }
    }
}
#[derive(Clone)]
pub struct SourceImage {
    pub id: i64,
    pub path: String,
    pub width: i64,
    pub height: i64,
    pub modified_at: Option<i64>,
    pub size: i64,
    pub scanned: bool,
}
pub fn selected_images(
    conn: &Connection,
    scope: &FaceScope,
) -> Result<(Vec<SourceImage>, usize), String> {
    let base = "SELECT a.id,f.path || '/' || a.name,a.width,a.height,a.modified_at,a.size,COALESCE(a.has_faces,0) FROM afiles a JOIN afolders f ON a.folder_id=f.id WHERE a.file_type IN (1,3) AND a.width>0 AND a.height>0";
    let mut images = Vec::new();
    let mut collect = |query: &str, ids: &[i64]| -> Result<(), String> {
        let mut statement = conn.prepare(query).map_err(|e| e.to_string())?;
        let rows = statement
            .query_map(params_from_iter(ids), |row| {
                Ok(SourceImage {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    width: row.get(2)?,
                    height: row.get(3)?,
                    modified_at: row.get(4)?,
                    size: row.get(5)?,
                    scanned: row.get::<_, i64>(6)? != 0,
                })
            })
            .map_err(|e| e.to_string())?;
        images.extend(
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?,
        );
        Ok(())
    };
    if let Some(ids) = &scope.file_ids {
        for chunk in ids.chunks(400) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            collect(
                &format!("{base} AND a.id IN ({placeholders}) ORDER BY a.id"),
                chunk,
            )?;
        }
        if images.len() != ids.len() {
            return Err("Selection contains missing, unsupported or unindexed images; no images were processed".into());
        }
    } else {
        collect(
            &format!("{base} AND COALESCE(a.has_faces,0)=0 ORDER BY a.id"),
            &[],
        )?;
    }
    let cached = if scope.force {
        0
    } else {
        images.iter().filter(|image| image.scanned).count()
    };
    images.retain(|image| scope.force || !image.scanned);
    Ok((images, cached))
}
/// Match the browser/preview orientation, rather than detecting unrotated EXIF JPEG pixels.
pub fn decode_image(bytes: &[u8]) -> Result<image::DynamicImage, String> {
    use image::ImageDecoder;
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image = image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    Ok(image)
}
fn overlap(a: &crate::t_face::FaceBox, b: &crate::t_face::FaceBox) -> f32 {
    let area = |face: &crate::t_face::FaceBox| face.width.max(0.) * face.height.max(0.);
    let intersection = ((a.x + a.width).min(b.x + b.width) - a.x.max(b.x)).max(0.)
        * ((a.y + a.height).min(b.y + b.height) - a.y.max(b.y)).max(0.);
    let union = area(a) + area(b) - intersection;
    if union.is_finite() && union > 0. {
        intersection / union
    } else {
        0.
    }
}
/// Preserve face IDs/person assignments for overlapping detections from the same compatible index.
pub fn replace_scanned(
    conn: &Connection,
    image: &SourceImage,
    records: &[(String, Vec<f32>)],
) -> Result<usize, String> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let actual = conn
            .query_row(
                "SELECT modified_at,size FROM afiles WHERE id=?1",
                [image.id],
                |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if actual != Some((image.modified_at, image.size)) {
            return Err("Source image changed during face inference; result discarded".into());
        }
        let old = {
            let mut statement = conn
                .prepare("SELECT id,bbox FROM faces WHERE file_id=?1 ORDER BY id")
                .map_err(|e| e.to_string())?;
            statement
                .query_map([image.id], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };
        let mut retained = HashSet::new();
        for (bbox, vector) in records {
            let new: crate::t_face::FaceBox =
                serde_json::from_str(bbox).map_err(|_| "Invalid face bounding box")?;
            let matched = old
                .iter()
                .filter(|(id, _)| !retained.contains(id))
                .filter_map(|(id, text)| {
                    let old: crate::t_face::FaceBox = serde_json::from_str(text).ok()?;
                    let score = overlap(&old, &new);
                    (score >= 0.5).then_some((*id, score))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((id, _)) = matched {
                let bytes = vector
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect::<Vec<_>>();
                conn.execute(
                    "UPDATE faces SET bbox=?1,embedding=?2 WHERE id=?3",
                    params![bbox, bytes, id],
                )
                .map_err(|e| e.to_string())?;
                retained.insert(id);
            } else {
                crate::t_sqlite::Face::add_with_conn(conn, image.id, bbox, vector)?;
            }
        }
        for (id, _) in old {
            if !retained.contains(&id) {
                conn.execute(
                    "UPDATE persons SET cover_face_id=NULL,thumbnail=NULL WHERE cover_face_id=?1",
                    [id],
                )
                .map_err(|e| e.to_string())?;
                conn.execute("DELETE FROM faces WHERE id=?1", [id])
                    .map_err(|e| e.to_string())?;
            }
        }
        crate::t_sqlite::Face::mark_scanned_with_conn(
            conn,
            image.id,
            if records.is_empty() { 2 } else { 1 },
        )?;
        Ok(records.len())
    })();
    match result {
        Ok(count) => {
            conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            Ok(count)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE afolders(id INTEGER PRIMARY KEY,path TEXT); CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,name TEXT,file_type INTEGER,width INTEGER,height INTEGER,modified_at INTEGER,size INTEGER,has_faces INTEGER); CREATE TABLE faces(id INTEGER PRIMARY KEY,file_id INTEGER,bbox TEXT,embedding BLOB,person_id INTEGER,created_at INTEGER); CREATE TABLE persons(id INTEGER PRIMARY KEY,name TEXT,cover_face_id INTEGER,thumbnail BLOB); INSERT INTO afolders VALUES(1,'photos'); INSERT INTO afiles VALUES(1,1,'a.jpg',1,100,100,10,100,0),(2,1,'b.jpg',1,100,100,10,100,1),(3,1,'clip.mp4',2,100,100,10,100,0),(4,1,'raw.dng',3,100,100,10,100,0); INSERT INTO persons VALUES(7,'Alice',11,NULL);").unwrap();
        c
    }
    fn bbox(x: f32) -> String {
        serde_json::json!({"x":x,"y":0,"width":20,"height":20,"confidence":1,"landmarks":null})
            .to_string()
    }
    #[test]
    fn empty_invalid_or_unbounded_selection_never_becomes_library_scan() {
        assert!(FaceScope::new(Some(vec![]), false).is_err());
        assert!(FaceScope::new(Some(vec![0]), false).is_err());
        assert!(FaceScope::new(None, true).is_err());
        assert!(FaceScope::new(Some(vec![1; 10001]), false).is_err());
    }
    #[test]
    fn selection_is_deduplicated_and_does_not_include_other_images() {
        let c = db();
        let scope = FaceScope::new(Some(vec![1, 1]), true).unwrap();
        let (images, cached) = selected_images(&c, &scope).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].id, 1);
        assert_eq!(cached, 0);
    }
    #[test]
    fn normal_detection_reuses_cached_results_and_raw_images_are_supported() {
        let c = db();
        let scope = FaceScope::new(Some(vec![2, 4]), false).unwrap();
        let (images, cached) = selected_images(&c, &scope).unwrap();
        assert_eq!(cached, 1);
        assert_eq!(images[0].id, 4);
    }
    #[test]
    fn mixed_or_missing_selections_are_rejected_as_a_whole() {
        let c = db();
        for ids in [vec![1, 3], vec![1, 99]] {
            assert!(selected_images(&c, &FaceScope::new(Some(ids), true).unwrap()).is_err());
        }
    }
    #[test]
    fn redetection_preserves_face_id_person_name_and_other_images() {
        let c = db();
        c.execute(
            "INSERT INTO faces VALUES(11,1,?1,X'01020304',7,0)",
            [bbox(0.)],
        )
        .unwrap();
        c.execute(
            "INSERT INTO faces VALUES(12,2,?1,X'01020304',7,0)",
            [bbox(0.)],
        )
        .unwrap();
        let image = selected_images(&c, &FaceScope::new(Some(vec![1]), true).unwrap())
            .unwrap()
            .0
            .remove(0);
        replace_scanned(
            &c,
            &image,
            &[(bbox(1.), vec![1., 0.]), (bbox(60.), vec![0., 1.])],
        )
        .unwrap();
        assert_eq!(
            c.query_row("SELECT person_id FROM faces WHERE id=11", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            7
        );
        assert_eq!(
            c.query_row("SELECT name FROM persons WHERE id=7", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Alice"
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM faces WHERE file_id=2", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn failed_or_stale_redetection_is_atomic() {
        let c = db();
        c.execute(
            "INSERT INTO faces VALUES(11,1,?1,X'01020304',7,0)",
            [bbox(0.)],
        )
        .unwrap();
        let mut image = selected_images(&c, &FaceScope::new(Some(vec![1]), true).unwrap())
            .unwrap()
            .0
            .remove(0);
        assert!(replace_scanned(&c, &image, &[("invalid".into(), vec![1.])]).is_err());
        image.size = 999;
        assert!(replace_scanned(&c, &image, &[]).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM faces", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn jpeg_exif_orientation_is_applied_before_detection() {
        let mut jpeg = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(8, 4)
            .write_to(&mut jpeg, image::ImageFormat::Jpeg)
            .unwrap();
        let jpeg = jpeg.into_inner();
        let payload = b"Exif\0\0II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut rotated = jpeg[..2].to_vec();
        rotated.extend_from_slice(&[0xff, 0xe1]);
        rotated.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        rotated.extend_from_slice(payload);
        rotated.extend_from_slice(&jpeg[2..]);
        let decoded = decode_image(&rotated).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4, 8));
    }
    #[test]
    fn malformed_images_do_not_become_successful_empty_detections() {
        assert!(decode_image(b"invalid image").is_err());
    }
    #[test]
    #[ignore = "Requires checksum-pinned Buffalo-L and public face-fixture.png"]
    fn real_cpu_detection_respects_selection_and_preserves_named_face_on_redetection() {
        use crate::ai::types::Task;
        let root = std::path::PathBuf::from(
            std::env::var_os("LAP_AI_TEST_MODEL_ROOT").expect("model fixtures"),
        );
        let fixture = root.join("face-fixture.png");
        let bytes = std::fs::read(&fixture).unwrap();
        let image = decode_image(&bytes).unwrap();
        let mut model = crate::ai::settings::Configuration::default()
            .resolve("buffalo-l")
            .unwrap();
        assert_eq!(model.definition.task, Task::Face);
        model
            .values
            .insert("device".into(), serde_json::json!("cpu"));
        let mut engine = crate::t_face::FaceEngine::new();
        engine.load_model(model).unwrap();
        let (faces, _) = engine.process_image_from_bytes(&bytes).unwrap();
        assert!(!faces.is_empty());
        let records = faces
            .into_iter()
            .map(|face| (serde_json::to_string(&face.bbox).unwrap(), face.embedding))
            .collect::<Vec<_>>();
        let c = db();
        c.execute(
            "UPDATE afiles SET width=?1,height=?2,size=?3",
            params![image.width(), image.height(), bytes.len()],
        )
        .unwrap();
        c.execute(
            "INSERT INTO faces VALUES(11,2,?1,X'01020304',7,0)",
            [bbox(0.)],
        )
        .unwrap();
        let selected = selected_images(&c, &FaceScope::new(Some(vec![1]), true).unwrap())
            .unwrap()
            .0
            .remove(0);
        replace_scanned(&c, &selected, &records).unwrap();
        let id = c
            .query_row(
                "SELECT id FROM faces WHERE file_id=1 ORDER BY id LIMIT 1",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        c.execute("UPDATE faces SET person_id=7 WHERE id=?1", [id])
            .unwrap();
        replace_scanned(&c, &selected, &records).unwrap();
        assert_eq!(
            c.query_row("SELECT person_id FROM faces WHERE id=?1", [id], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            7
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM faces WHERE file_id=2", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row("SELECT name FROM persons WHERE id=7", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Alice"
        );
        println!("Scoped real CPU detection and named-face re-detection passed");
    }
}
