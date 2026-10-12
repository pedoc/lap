//! Confirm one real detector candidate without changing model settings or replacing other faces.
use super::{face_annotations, face_jobs};
use crate::t_face::FaceBox;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub library_id: String,
    pub profile: String,
    pub file_id: i64,
    pub source_revision: String,
    pub source_size: [i64; 2],
    pub decoded_size: [u32; 2],
    pub region: face_annotations::Region,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultView {
    pub file_id: i64,
    pub face_id: i64,
    pub person_id: Option<i64>,
    pub stored_regions: usize,
    pub manual_regions: usize,
    pub added: bool,
    pub restored_embedding: bool,
}
pub fn source_revision(file_id: i64, size: u64, modified: Option<SystemTime>) -> Option<String> {
    let nanos = modified?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    Some(
        blake3::hash(format!("{file_id}:{size}:{nanos}").as_bytes())
            .to_hex()
            .to_string(),
    )
}
pub fn validate(request: &Request) -> Result<(), String> {
    let r = &request.region;
    if request.file_id <= 0
        || request.library_id.is_empty()
        || request.profile.is_empty()
        || request.source_revision.len() != 64
        || !request
            .source_revision
            .bytes()
            .all(|c| c.is_ascii_hexdigit())
        || request.source_size.iter().any(|size| *size <= 0)
        || request.decoded_size.contains(&0)
        || ![r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite())
        || r.x < 0.
        || r.y < 0.
        || r.width <= 0.
        || r.height <= 0.
        || r.x + r.width > 1.00001
        || r.y + r.height > 1.00001
    {
        return Err(
            "FACE_CONFIRM_STALE: Report is incomplete or stale; re-detect this image first".into(),
        );
    }
    Ok(())
}
/// A client region is a selector, not permission to insert an arbitrary rectangle.
pub fn matching_detection(
    candidates: Vec<FaceBox>,
    region: &face_annotations::Region,
    decoded: [u32; 2],
    threshold: f32,
) -> Result<FaceBox, String> {
    let selected = FaceBox {
        x: region.x * decoded[0] as f32,
        y: region.y * decoded[1] as f32,
        width: region.width * decoded[0] as f32,
        height: region.height * decoded[1] as f32,
        confidence: 1.,
        landmarks: None,
    };
    candidates.into_iter().filter(|face|face.confidence.is_finite()&&face.confidence>=threshold)
        .filter_map(|face| {
            let clipped=face_annotations::normalized_box(&face,decoded[0] as i64,decoded[1] as i64).ok()?;
            let clipped=FaceBox{x:clipped.x*decoded[0] as f32,y:clipped.y*decoded[1] as f32,width:clipped.width*decoded[0] as f32,height:clipped.height*decoded[1] as f32,confidence:face.confidence,landmarks:None};
            let overlap=face_jobs::overlap(&selected,&clipped);
            (overlap>=0.9).then_some((face,overlap))
        }).max_by(|a,b|a.1.total_cmp(&b.1)).map(|item|item.0)
        .ok_or_else(||"FACE_CONFIRM_STALE: The current detector did not reproduce this candidate; re-detect first".into())
}
pub fn source_box(mut bbox: FaceBox, decoded: [u32; 2], source: [i64; 2]) -> FaceBox {
    let sx = source[0] as f32 / decoded[0] as f32;
    let sy = source[1] as f32 / decoded[1] as f32;
    bbox.x *= sx;
    bbox.y *= sy;
    bbox.width *= sx;
    bbox.height *= sy;
    if let Some(points) = bbox.landmarks.as_mut() {
        for point in points {
            point.0 *= sx;
            point.1 *= sy;
        }
    }
    bbox
}
fn same_region(a: &FaceBox, b: &FaceBox) -> bool {
    let overlap = face_jobs::overlap(a, b);
    overlap >= 0.9
        || (overlap >= 0.5
            && ((a.x + a.width * 0.5) - (b.x + b.width * 0.5)).abs() <= a.width.min(b.width) * 0.1
            && ((a.y + a.height * 0.5) - (b.y + b.height * 0.5)).abs()
                <= a.height.min(b.height) * 0.1)
}
/// Narrow additive transaction; never removes/relabels existing detections or bypasses a human exclusion.
pub fn commit(
    conn: &Connection,
    source: &face_jobs::SourceImage,
    bbox: &FaceBox,
    vector: &[f32],
) -> Result<ResultView, String> {
    let norm = vector.iter().map(|value| value * value).sum::<f32>();
    if !norm.is_finite()
        || vector.is_empty()
        || vector.iter().any(|value| !value.is_finite())
        || vector.iter().map(|value| value * value).sum::<f32>() <= f32::EPSILON
    {
        return Err("Invalid face embedding".into());
    }
    face_annotations::import_existing(conn)?;
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let actual = tx
        .query_row(
            "SELECT modified_at,size,width,height FROM afiles WHERE id=?1",
            [source.id],
            |r| {
                Ok((
                    r.get::<_, Option<i64>>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if actual != Some((source.modified_at, source.size, source.width, source.height)) {
        return Err("FACE_CONFIRM_STALE: Image metadata changed; re-detect first".into());
    }
    face_annotations::normalized_box(bbox, source.width, source.height)?;
    let annotations = face_annotations::active_for_file(&tx, source.id)?;
    if annotations.iter().any(|a| {
        ["ignored", "not_face"].contains(&a.kind.as_str())
            && face_jobs::overlap(
                bbox,
                &face_annotations::pixel_box(a, source.width, source.height),
            ) >= 0.5
    }) {
        return Err("FACE_CONFIRM_EXCLUDED: This region is manually ignored or marked not a face; undo that decision first".into());
    }
    let existing = {
        let mut stmt=tx.prepare("SELECT f.id,f.person_id,f.bbox,f.embedding IS NOT NULL FROM faces f LEFT JOIN face_annotations n ON n.id=f.annotation_id WHERE f.file_id=?1 AND (n.id IS NULL OR (n.kind IN ('confirmed','unassigned') AND n.source_modified_at IS ?2 AND n.source_size=?3))").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map(params![source.id, source.modified_at, source.size], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, bool>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    let matched = existing
        .into_iter()
        .filter_map(|(id, person, text, embedding)| {
            let old: FaceBox = serde_json::from_str(&text).ok()?;
            let score = face_jobs::overlap(bbox, &old);
            same_region(bbox, &old).then_some((id, person, embedding, score))
        })
        .max_by(|a, b| a.3.total_cmp(&b.3));
    let (face_id, person_id, added, restored_embedding) =
        if let Some((id, person, has_embedding, _)) = matched {
            if !has_embedding {
                let bytes = vector
                    .iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect::<Vec<_>>();
                tx.execute(
                    "UPDATE faces SET embedding=?1 WHERE id=?2",
                    params![bytes, id],
                )
                .map_err(|e| e.to_string())?;
            }
            (id, person, false, !has_embedding)
        } else {
            // A new unnamed identity is immediately visible in People; the confirmed ROI survives later scans.
            tx.execute(
                "INSERT INTO persons(name,created_at) VALUES(NULL,unixepoch())",
                [],
            )
            .map_err(|e| e.to_string())?;
            let person = tx.last_insert_rowid();
            let text = serde_json::to_string(bbox).map_err(|e| e.to_string())?;
            let face = crate::t_sqlite::Face::add_with_conn(&tx, source.id, &text, vector)?;
            tx.execute(
                "UPDATE faces SET person_id=?1 WHERE id=?2",
                params![person, face],
            )
            .map_err(|e| e.to_string())?;
            face_annotations::capture_face(&tx, face, "confirmed")?;
            (face, Some(person), true, false)
        };
    crate::t_sqlite::Face::mark_scanned_with_conn(&tx, source.id, 1)?;
    let (stored,manual): (i64,i64)=tx.query_row("SELECT COUNT(*),COALESCE(SUM(f.annotation_id IS NOT NULL),0) FROM faces f LEFT JOIN face_annotations n ON n.id=f.annotation_id WHERE f.file_id=?1 AND (n.id IS NULL OR (n.kind IN ('confirmed','unassigned') AND n.source_modified_at IS ?2 AND n.source_size=?3))",params![source.id,source.modified_at,source.size],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(ResultView {
        file_id: source.id,
        face_id,
        person_id,
        stored_regions: stored as usize,
        manual_regions: manual as usize,
        added,
        restored_embedding,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bbox(x: f32) -> FaceBox {
        FaceBox {
            x,
            y: 10.,
            width: 30.,
            height: 30.,
            confidence: 0.9,
            landmarks: Some(vec![(x + 5., 15.); 5]),
        }
    }
    fn fixture() -> (Connection, face_jobs::SourceImage) {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS media_states(file_id INTEGER PRIMARY KEY,hidden INTEGER NOT NULL DEFAULT 0,deleted_at INTEGER,trash_group TEXT,original_name TEXT); CREATE TABLE afiles(id INTEGER PRIMARY KEY,width INTEGER,height INTEGER,modified_at INTEGER,size INTEGER,has_faces INTEGER,name TEXT); INSERT INTO afiles VALUES(1,100,100,10,100,2,'original.jpg'),(2,100,100,10,100,1,'other.jpg'); CREATE TABLE persons(id INTEGER PRIMARY KEY AUTOINCREMENT,name TEXT,cover_face_id INTEGER,thumbnail BLOB,created_at INTEGER); CREATE TABLE faces(id INTEGER PRIMARY KEY AUTOINCREMENT,file_id INTEGER,bbox TEXT,embedding BLOB,person_id INTEGER,created_at INTEGER); INSERT INTO persons VALUES(7,'Alice',11,NULL,0);").unwrap();
        let json = serde_json::to_string(&bbox(60.)).unwrap();
        c.execute(
            "INSERT INTO faces VALUES(11,2,?1,X'0000803f00000000',7,0)",
            [json],
        )
        .unwrap();
        face_annotations::import_existing(&c).unwrap();
        (
            c,
            face_jobs::SourceImage {
                id: 1,
                path: "unused-original.jpg".into(),
                width: 100,
                height: 100,
                modified_at: Some(10),
                size: 100,
                scanned: true,
            },
        )
    }
    #[test]
    fn selected_real_candidate_keeps_detector_threshold_landmarks_and_source_geometry() {
        let region = face_annotations::normalized_box(&bbox(10.), 100, 100).unwrap();
        assert!(
            matching_detection(
                vec![FaceBox {
                    confidence: 0.64,
                    ..bbox(10.)
                }],
                &region,
                [100, 100],
                0.65
            )
            .is_err()
        );
        assert!(matching_detection(vec![bbox(60.)], &region, [100, 100], 0.65).is_err());
        let face =
            matching_detection(vec![bbox(60.), bbox(10.)], &region, [100, 100], 0.65).unwrap();
        assert!(face.landmarks.is_some());
        let face = source_box(face, [100, 100], [200, 300]);
        assert_eq!(
            (face.x, face.y, face.width, face.height),
            (20., 30., 60., 90.)
        );
        assert_eq!(face.landmarks.unwrap()[0], (30., 45.));
    }
    #[test]
    fn additive_confirmation_is_idempotent_survives_rescan_and_preserves_other_people() {
        let (c, source) = fixture();
        let first = commit(&c, &source, &bbox(10.), &[1., 0.]).unwrap();
        assert!(first.added);
        assert_eq!((first.stored_regions, first.manual_regions), (1, 1));
        let twice = commit(&c, &source, &bbox(10.), &[1., 0.]).unwrap();
        assert!(!twice.added);
        assert_eq!(twice.face_id, first.face_id);
        face_jobs::replace_scanned_reported(&c, &source, &[]).unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM faces WHERE file_id=1", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM faces WHERE file_id=2 AND person_id=7",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            c.query_row("SELECT name FROM persons WHERE id=7", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Alice"
        );
        assert_eq!(
            c.query_row("SELECT name FROM afiles WHERE id=1", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "original.jpg"
        );
        assert_eq!(
            c.query_row(
                "SELECT manual FROM persons WHERE id=?1",
                [first.person_id.unwrap()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
    #[test]
    fn overlapping_but_distinct_faces_are_not_merged() {
        let (c, source) = fixture();
        let first = commit(&c, &source, &bbox(10.), &[1., 0.]).unwrap();
        let second = commit(&c, &source, &bbox(17.5), &[0., 1.]).unwrap();
        assert!(second.added);
        assert_ne!(first.face_id, second.face_id);
        assert_eq!(second.stored_regions, 2);
    }
    #[test]
    fn named_identity_and_region_are_preserved_and_missing_embedding_is_restored() {
        let (c, source) = fixture();
        let first = commit(&c, &source, &bbox(10.), &[1., 0.]).unwrap();
        c.execute(
            "UPDATE persons SET name='User name' WHERE id=?1",
            [first.person_id.unwrap()],
        )
        .unwrap();
        c.execute(
            "UPDATE faces SET embedding=NULL WHERE id=?1",
            [first.face_id],
        )
        .unwrap();
        let restored = commit(&c, &source, &bbox(10.), &[1., 0.]).unwrap();
        assert!(restored.restored_embedding);
        assert!(!restored.added);
        assert_eq!(restored.face_id, first.face_id);
        assert_eq!(
            c.query_row(
                "SELECT name FROM persons WHERE id=?1",
                [first.person_id.unwrap()],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "User name"
        );
    }
    #[test]
    fn human_exclusions_and_source_changes_cannot_be_bypassed() {
        for kind in ["ignored", "not_face"] {
            let (c, source) = fixture();
            let first = commit(&c, &source, &bbox(10.), &[1., 0.]).unwrap();
            face_annotations::capture_face(&c, first.face_id, kind).unwrap();
            assert!(
                commit(&c, &source, &bbox(10.), &[1., 0.])
                    .unwrap_err()
                    .starts_with("FACE_CONFIRM_EXCLUDED:")
            );
            assert_eq!(
                c.query_row("SELECT COUNT(*) FROM faces WHERE file_id=1", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
        let (c, mut source) = fixture();
        source.size = 101;
        assert!(commit(&c, &source, &bbox(10.), &[1., 0.]).is_err());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM faces WHERE file_id=1", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn failures_roll_back_new_people_and_faces_and_invalid_vectors_are_rejected() {
        let (c, source) = fixture();
        for v in [vec![], vec![0., 0.], vec![f32::NAN, 1.], vec![f32::MAX, 1.]] {
            assert!(commit(&c, &source, &bbox(10.), &v).is_err());
        }
        c.execute_batch("CREATE TRIGGER reject_face BEFORE INSERT ON faces BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
        assert!(commit(&c, &source, &bbox(10.), &[1., 0.]).is_err());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM persons", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM faces", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn requests_require_complete_snapshot_and_normalized_selector() {
        let mut request = Request {
            library_id: "lib".into(),
            profile: "profile".into(),
            file_id: 1,
            source_revision: "a".repeat(64),
            source_size: [100, 100],
            decoded_size: [100, 100],
            region: face_annotations::Region {
                x: 0.1,
                y: 0.1,
                width: 0.3,
                height: 0.3,
            },
        };
        assert!(validate(&request).is_ok());
        request.region.width = 2.;
        assert!(validate(&request).is_err());
        request.region.width = 0.3;
        request.source_revision.clear();
        assert!(validate(&request).is_err());
        let now = Some(SystemTime::now());
        assert_ne!(source_revision(1, 100, now), source_revision(2, 100, now));
        assert_ne!(source_revision(1, 100, now), source_revision(1, 101, now));
        assert!(source_revision(1, 100, None).is_none());
    }
}
