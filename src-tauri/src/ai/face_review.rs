//! Library-scoped review of disposable observations and durable human annotations.
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

// This workspace deliberately covers the whole current library, not the selected album/person.
const ITEMS: &str = "WITH items AS (
 SELECT f.id face_id,n.id annotation_id,a.id file_id,a.name file_name,f.person_id,p.name person_name,
 f.bbox,a.width,a.height,a.modified_at,a.size,
 CASE WHEN n.kind IS NOT NULL THEN n.kind WHEN f.person_id IS NULL THEN 'unknown' ELSE 'suggested' END state
 FROM faces f JOIN afiles a ON a.id=f.file_id LEFT JOIN persons p ON p.id=f.person_id
 LEFT JOIN face_annotations n ON n.id=f.annotation_id
 WHERE n.id IS NULL OR (n.source_modified_at IS a.modified_at AND n.source_size=a.size)
 UNION ALL
 SELECT NULL,n.id,a.id,a.name,n.person_id,p.name,
 json_object('x',json_extract(n.region,'$.x')*a.width,'y',json_extract(n.region,'$.y')*a.height,
 'width',json_extract(n.region,'$.width')*a.width,'height',json_extract(n.region,'$.height')*a.height,'confidence',0,'landmarks',NULL),
 a.width,a.height,a.modified_at,a.size,
 CASE WHEN n.source_modified_at IS a.modified_at AND n.source_size=a.size THEN n.kind ELSE 'stale' END
 FROM face_annotations n JOIN afiles a ON a.id=n.file_id LEFT JOIN persons p ON p.id=n.person_id
 WHERE NOT EXISTS(SELECT 1 FROM faces f WHERE f.annotation_id=n.id)
 OR NOT(n.source_modified_at IS a.modified_at AND n.source_size=a.size)
) ";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewItem {
    pub face_id: Option<i64>,
    pub annotation_id: Option<i64>,
    pub file_id: i64,
    pub file_name: String,
    pub person_id: Option<i64>,
    pub person_name: Option<String>,
    pub bbox: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub modified_at: Option<i64>,
    pub size: i64,
    pub state: String,
}
impl ReviewItem {
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            face_id: row.get(0)?,
            annotation_id: row.get(1)?,
            file_id: row.get(2)?,
            file_name: row.get(3)?,
            person_id: row.get(4)?,
            person_name: row.get(5)?,
            bbox: row.get(6)?,
            width: row.get(7)?,
            height: row.get(8)?,
            modified_at: row.get(9)?,
            size: row.get(10)?,
            state: row.get(11)?,
        })
    }
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageRequest {
    pub library_id: String,
    pub filter: String,
    pub person_id: Option<i64>,
    pub offset: usize,
    pub limit: usize,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPage {
    pub items: Vec<ReviewItem>,
    pub total: usize,
    pub counts: BTreeMap<String, usize>,
}
fn condition(filter: &str) -> Result<&'static str, String> {
    match filter {
        "all" => Ok("state IN ('suggested','unknown','unassigned','confirmed')"),
        "suggested" => Ok("state='suggested'"),
        "unknown" => Ok("state IN ('unknown','unassigned')"),
        "confirmed" => Ok("state='confirmed'"),
        "ignored" => Ok("state IN ('ignored','not_face')"),
        "stale" => Ok("state='stale'"),
        _ => Err("Unsupported face review filter".into()),
    }
}
pub fn page(conn: &Connection, request: &PageRequest) -> Result<ReviewPage, String> {
    let filter = condition(&request.filter)?;
    let mut counts = BTreeMap::new();
    {
        let mut statement = conn
            .prepare(&format!(
                "{ITEMS} SELECT state,COUNT(*) FROM items WHERE (?1 IS NULL OR person_id=?1) GROUP BY state"
            ))
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([request.person_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (state, count) = row.map_err(|e| e.to_string())?;
            counts.insert(state, count as usize);
        }
    }
    let total: i64 = conn
        .query_row(
            &format!(
                "{ITEMS} SELECT COUNT(*) FROM items WHERE {filter} AND (?1 IS NULL OR person_id=?1)"
            ),
            [request.person_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let offset = i64::try_from(request.offset).map_err(|_| "Invalid page offset")?;
    let mut statement=conn.prepare(&format!("{ITEMS} SELECT * FROM items WHERE {filter} AND (?1 IS NULL OR person_id=?1) ORDER BY file_id,COALESCE(face_id,annotation_id),annotation_id LIMIT ?2 OFFSET ?3")).map_err(|e|e.to_string())?;
    let items = statement
        .query_map(
            params![
                request.person_id,
                request.limit.clamp(1, 100) as i64,
                offset
            ],
            ReviewItem::read,
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(ReviewPage {
        items,
        total: total as usize,
        counts,
    })
}
fn current(conn: &Connection, expected: &ReviewItem) -> Result<ReviewItem, String> {
    if expected.file_id <= 0 || (expected.face_id.is_none() && expected.annotation_id.is_none()) {
        return Err("Invalid face review selection".into());
    }
    let current=conn.query_row(&format!("{ITEMS} SELECT * FROM items WHERE file_id=?1 AND face_id IS ?2 AND annotation_id IS ?3"),params![expected.file_id,expected.face_id,expected.annotation_id],ReviewItem::read).optional().map_err(|e|e.to_string())?.ok_or("Face review result changed; refresh the workspace")?;
    if &current != expected {
        return Err("Face, person or source changed; refresh the workspace before editing".into());
    }
    Ok(current)
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAction {
    Confirm,
    Reject,
    Ignore,
    NotFace,
    Restore,
    AssignExisting,
    AssignNew,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionRequest {
    pub library_id: String,
    pub profile: String,
    pub action: ReviewAction,
    pub items: Vec<ReviewItem>,
    pub name: Option<String>,
    pub target_person_id: Option<i64>,
    pub expected_target_name: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResult {
    pub count: usize,
    pub person_id: Option<i64>,
    pub name: Option<String>,
    pub previous_person_ids: Vec<i64>,
    pub file_ids: Vec<i64>,
    pub membership_changed: bool,
}
pub fn apply(conn: &Connection, request: &ActionRequest) -> Result<ReviewResult, String> {
    if request.items.is_empty() || request.items.len() > 100 {
        return Err("Select between 1 and 100 faces to review".into());
    }
    let mut keys = HashSet::new();
    if request
        .items
        .iter()
        .any(|item| !keys.insert((item.file_id, item.face_id, item.annotation_id)))
    {
        return Err("Duplicate face review selection".into());
    }
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        // Validate every snapshot before creating a new identity or editing any annotations.
        let items = request
            .items
            .iter()
            .map(|item| current(conn, item))
            .collect::<Result<Vec<_>, _>>()?;
        for item in &items {
            if item.state == "stale" {
                return Err("Source changed; old regions cannot be applied to new content. Detect and annotate the current image instead".into());
            }
            let allowed = match request.action {
                ReviewAction::Restore => ["ignored", "not_face"].contains(&item.state.as_str()),
                ReviewAction::Confirm | ReviewAction::Reject => {
                    item.state == "suggested" && item.person_id.is_some()
                }
                _ => {
                    item.face_id.is_some()
                        && ["unknown", "unassigned", "suggested", "confirmed"]
                            .contains(&item.state.as_str())
                }
            };
            if !allowed {
                return Err("This action is not supported for the selected face states; refresh or restore first".into());
            }
        }
        let (target, name) = match request.action {
            ReviewAction::AssignNew => {
                let name = super::face_names::valid_name(request.name.as_deref().unwrap_or(""))?;
                (
                    Some(super::face_names::create_person_in_transaction(
                        conn,
                        name,
                        items.first().and_then(|item| item.face_id),
                    )?),
                    Some(name.to_string()),
                )
            }
            ReviewAction::AssignExisting => {
                let id = request
                    .target_person_id
                    .filter(|id| *id > 0)
                    .ok_or("Choose a target person")?;
                let name: Option<String> = conn
                    .query_row("SELECT name FROM persons WHERE id=?1", [id], |row| {
                        row.get(0)
                    })
                    .optional()
                    .map_err(|e| e.to_string())?
                    .ok_or("Target person no longer exists")?;
                if name != request.expected_target_name {
                    return Err("Target person changed; choose it again".into());
                }
                (Some(id), name)
            }
            _ => (None, None),
        };
        let membership_changed = match request.action {
            ReviewAction::Confirm | ReviewAction::Restore => false,
            _ => items.iter().any(|item| item.person_id != target),
        };
        let previous_person_ids = items
            .iter()
            .filter_map(|item| item.person_id)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let file_ids = items
            .iter()
            .map(|item| item.file_id)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        for item in items {
            if request.action == ReviewAction::Restore {
                let id = item.annotation_id.ok_or("Missing durable annotation")?;
                conn.execute("UPDATE face_annotations SET kind='unassigned',person_id=NULL,updated_at=CAST(strftime('%s','now') AS INTEGER) WHERE id=?1",[id]).map_err(|e|e.to_string())?;
                conn.execute(
                    "UPDATE faces SET person_id=NULL,embedding=NULL WHERE annotation_id=?1",
                    [id],
                )
                .map_err(|e| e.to_string())?;
                super::face_annotations::restore_file(conn, item.file_id)?;
            } else {
                let mode = match request.action {
                    ReviewAction::Confirm => super::face_names::EditMode::Confirm,
                    ReviewAction::Reject => super::face_names::EditMode::Unassign,
                    ReviewAction::Ignore => super::face_names::EditMode::Ignore,
                    ReviewAction::NotFace => super::face_names::EditMode::NotFace,
                    ReviewAction::AssignExisting | ReviewAction::AssignNew => {
                        super::face_names::EditMode::AssignExisting
                    }
                    ReviewAction::Restore => return Err("Unsupported review action".into()),
                };
                let edit = super::face_names::FaceNameRequest {
                    library_id: request.library_id.clone(),
                    profile: request.profile.clone(),
                    file_id: item.file_id,
                    face_id: item
                        .face_id
                        .ok_or("Re-detect or restore the image before editing")?,
                    expected_person_id: item.person_id,
                    expected_name: item.person_name,
                    mode,
                    name: None,
                    target_person_id: target,
                };
                super::face_names::edit_in_transaction(conn, &edit)?;
            }
        }
        Ok(ReviewResult {
            count: request.items.len(),
            person_id: target,
            name,
            previous_person_ids,
            file_ids,
            membership_changed,
        })
    })();
    match result {
        Ok(result) => {
            conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            Ok(result)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}
/// Assignment search also includes human-created identities which currently have no observations.
pub fn people(conn: &Connection, search: &str) -> Result<Vec<serde_json::Value>, String> {
    let pattern = format!(
        "%{}%",
        search
            .trim()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let lookup = search
        .trim()
        .strip_prefix('#')
        .and_then(|id| id.parse::<i64>().ok())
        .filter(|id| *id > 0);
    let mut statement=conn.prepare("SELECT id,name FROM persons WHERE COALESCE(name,'') LIKE ?1 ESCAPE '\\' COLLATE NOCASE OR id=?2 ORDER BY CASE WHEN id=?2 THEN 0 ELSE 1 END,manual DESC,CASE WHEN name IS NULL OR name='' THEN 1 ELSE 0 END,name COLLATE NOCASE,id LIMIT 50").map_err(|e|e.to_string())?;
    statement.query_map(params![pattern,lookup],|row|Ok(serde_json::json!({"id":row.get::<_,i64>(0)?,"name":row.get::<_,Option<String>>(1)?}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}
/// Cached preview only: bounded memory, no original-file writes or model invocation.
pub fn thumbnail(conn: &Connection, expected: &ReviewItem) -> Result<Option<String>, String> {
    use base64::Engine;
    let item = current(conn, expected)?;
    if item.state == "stale" {
        return Ok(None);
    }
    let Some(data) =
        crate::t_sqlite::AThumb::fetch(item.file_id)?.and_then(|thumb| thumb.thumb_data)
    else {
        return Ok(None);
    };
    let image = image::load_from_memory(&data).map_err(|e| e.to_string())?;
    let bbox: crate::t_face::FaceBox =
        serde_json::from_str(&item.bbox).map_err(|_| "Invalid face region")?;
    let (Some(width), Some(height)) = (item.width, item.height) else {
        return Ok(None);
    };
    if width <= 0
        || height <= 0
        || ![bbox.x, bbox.y, bbox.width, bbox.height]
            .iter()
            .all(|v| v.is_finite())
        || bbox.width <= 0.
        || bbox.height <= 0.
    {
        return Ok(None);
    }
    let iw = image.width();
    let ih = image.height();
    let sx = iw as f32 / width as f32;
    let sy = ih as f32 / height as f32;
    let x = ((bbox.x - bbox.width * 0.15) * sx).max(0.).min(iw as f32) as u32;
    let y = ((bbox.y - bbox.height * 0.15) * sy).max(0.).min(ih as f32) as u32;
    let right = ((bbox.x + bbox.width * 1.15) * sx)
        .max(0.)
        .min(iw as f32)
        .ceil() as u32;
    let bottom = ((bbox.y + bbox.height * 1.15) * sy)
        .max(0.)
        .min(ih as f32)
        .ceil() as u32;
    if right <= x || bottom <= y {
        return Ok(None);
    }
    let crop = image.crop_imm(x, y, right - x, bottom - y).resize(
        160,
        160,
        image::imageops::FilterType::Triangle,
    );
    let mut output = std::io::Cursor::new(Vec::new());
    crop.write_to(&mut output, image::ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;
    Ok(Some(
        base64::engine::general_purpose::STANDARD.encode(output.into_inner()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE afiles(id INTEGER PRIMARY KEY,name TEXT,width INTEGER,height INTEGER,modified_at INTEGER,size INTEGER,has_faces INTEGER); CREATE TABLE persons(id INTEGER PRIMARY KEY,name TEXT,created_at INTEGER,cover_face_id INTEGER,thumbnail BLOB); CREATE TABLE faces(id INTEGER PRIMARY KEY AUTOINCREMENT,file_id INTEGER,person_id INTEGER,bbox TEXT,embedding BLOB,created_at INTEGER); INSERT INTO afiles VALUES(1,'a.jpg',100,100,10,100,1),(2,'b.jpg',100,100,10,100,1); INSERT INTO persons VALUES(7,'Alice',0,11,NULL),(8,'Bob',0,12,NULL);").unwrap();
        super::super::face_annotations::ensure_schema(&c).unwrap();
        c.execute(
            "INSERT INTO face_annotation_meta VALUES('imported-v1','1')",
            [],
        )
        .unwrap();
        let bbox=serde_json::json!({"x":10,"y":10,"width":30,"height":30,"confidence":1,"landmarks":null}).to_string();
        c.execute("INSERT INTO faces(id,file_id,person_id,bbox,embedding,created_at) VALUES(11,1,7,?1,X'01020304',0),(12,2,8,?1,X'01020304',0),(13,1,NULL,?1,NULL,0)",[bbox]).unwrap();
        c
    }
    fn list(c: &Connection, filter: &str) -> ReviewPage {
        page(
            c,
            &PageRequest {
                library_id: "lib".into(),
                filter: filter.into(),
                person_id: None,
                offset: 0,
                limit: 36,
            },
        )
        .unwrap()
    }
    fn request(action: ReviewAction, items: Vec<ReviewItem>) -> ActionRequest {
        ActionRequest {
            library_id: "lib".into(),
            profile: "p".into(),
            action,
            items,
            name: None,
            target_person_id: None,
            expected_target_name: None,
        }
    }
    #[test]
    fn unknown_and_suggested_have_separate_paginated_entry_points() {
        let c = db();
        let result = list(&c, "unknown");
        assert_eq!(result.total, 1);
        assert_eq!(result.counts["suggested"], 2);
        let result = page(
            &c,
            &PageRequest {
                library_id: "lib".into(),
                filter: "suggested".into(),
                person_id: None,
                offset: 1,
                limit: 1,
            },
        )
        .unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].face_id, Some(12));
        assert!(
            page(
                &c,
                &PageRequest {
                    library_id: "lib".into(),
                    filter: "bad' OR 1=1".into(),
                    person_id: None,
                    offset: 0,
                    limit: 1
                }
            )
            .is_err()
        );
    }
    #[test]
    fn confirmation_and_rejection_are_durable_and_batch_atomic() {
        let c = db();
        let mut items = list(&c, "suggested").items;
        items[1].person_name = Some("stale".into());
        assert!(apply(&c, &request(ReviewAction::Confirm, items)).is_err());
        assert_eq!(list(&c, "confirmed").total, 0);
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM face_annotations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let items = list(&c, "suggested").items;
        apply(&c, &request(ReviewAction::Confirm, vec![items[0].clone()])).unwrap();
        apply(&c, &request(ReviewAction::Reject, vec![items[1].clone()])).unwrap();
        assert_eq!(list(&c, "confirmed").total, 1);
        let unknown = list(&c, "unknown");
        assert_eq!(unknown.total, 2);
        assert_eq!(unknown.counts["unassigned"], 1);
        assert!(apply(&c, &request(ReviewAction::Confirm, unknown.items)).is_err());
    }
    #[test]
    fn ignored_regions_are_recoverable_after_model_reset_without_old_vectors() {
        let c = db();
        let item = list(&c, "suggested").items.remove(0);
        apply(&c, &request(ReviewAction::NotFace, vec![item])).unwrap();
        assert_eq!(list(&c, "ignored").total, 1);
        super::super::face_annotations::reset_model_results(&c).unwrap();
        let item = list(&c, "ignored").items.remove(0);
        assert!(item.face_id.is_none());
        apply(&c, &request(ReviewAction::Restore, vec![item])).unwrap();
        assert_eq!(list(&c, "ignored").total, 0);
        let item = list(&c, "unknown").items.remove(0);
        assert_eq!(item.state, "unassigned");
        assert!(item.person_id.is_none());
        let bytes: Option<Vec<u8>> = c
            .query_row(
                "SELECT embedding FROM faces WHERE id=?1",
                [item.face_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(bytes.is_none());
    }
    #[test]
    fn changed_source_annotations_remain_visible_but_cannot_be_restored_or_edited() {
        let c = db();
        let item = list(&c, "suggested").items.remove(0);
        apply(&c, &request(ReviewAction::Ignore, vec![item])).unwrap();
        c.execute("UPDATE afiles SET modified_at=20 WHERE id=1", [])
            .unwrap();
        assert_eq!(list(&c, "ignored").total, 0);
        let item = list(&c, "stale").items.remove(0);
        assert!(item.face_id.is_none());
        assert!(apply(&c, &request(ReviewAction::Restore, vec![item])).is_err());
        assert_eq!(list(&c, "stale").total, 1);
    }
    #[test]
    fn reassignment_search_includes_empty_manual_people_and_escapes_wildcards() {
        let c = db();
        c.execute(
            "INSERT INTO persons(id,name,manual) VALUES(99,'孤立人物',1),(100,'100%_name',1)",
            [],
        )
        .unwrap();
        assert_eq!(people(&c, "孤立").unwrap()[0]["id"], 99);
        let result = people(&c, "%").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["id"], 100);
    }
    #[test]
    fn invalid_duplicate_or_stale_snapshots_cannot_modify_any_faces() {
        let c = db();
        let item = list(&c, "suggested").items.remove(0);
        assert!(apply(&c, &request(ReviewAction::Ignore, vec![])).is_err());
        assert!(
            apply(
                &c,
                &request(ReviewAction::Ignore, vec![item.clone(), item.clone()])
            )
            .is_err()
        );
        let mut forged = item.clone();
        forged.file_id = 99;
        assert!(apply(&c, &request(ReviewAction::Ignore, vec![forged])).is_err());
        c.execute("UPDATE faces SET bbox='{}' WHERE id=11", [])
            .unwrap();
        assert!(apply(&c, &request(ReviewAction::Ignore, vec![item])).is_err());
        assert_eq!(list(&c, "suggested").total, 2);
    }
    #[test]
    fn person_delete_failure_rolls_back_both_observations_and_annotations() {
        let c = db();
        super::super::face_annotations::capture_face(&c, 11, "confirmed").unwrap();
        c.execute_batch("CREATE TRIGGER protect_person BEFORE DELETE ON persons BEGIN SELECT RAISE(ABORT,'blocked'); END;").unwrap();
        assert!(super::super::face_names::delete_person(&c, 7).is_err());
        assert_eq!(list(&c, "confirmed").items[0].person_id, Some(7));
        c.execute_batch("DROP TRIGGER protect_person").unwrap();
        assert_eq!(super::super::face_names::delete_person(&c, 7).unwrap(), 1);
        assert_eq!(list(&c, "unknown").counts["unassigned"], 1);
    }
    #[test]
    fn batch_reassignment_changes_only_selected_faces_and_survives_model_reset() {
        let c = db();
        let mut r = request(ReviewAction::AssignExisting, list(&c, "suggested").items);
        r.items.retain(|item| item.face_id == Some(11));
        r.target_person_id = Some(8);
        r.expected_target_name = Some("Bob".into());
        let result = apply(&c, &r).unwrap();
        assert_eq!(result.person_id, Some(8));
        assert!(result.membership_changed);
        assert_eq!(
            c.query_row("SELECT person_id FROM faces WHERE id=13", [], |row| row
                .get::<_, Option<
                i64,
            >>(
                0
            ))
            .unwrap(),
            None
        );
        assert_eq!(
            c.query_row("SELECT person_id FROM faces WHERE id=12", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            8
        );
        super::super::face_annotations::reset_model_results(&c).unwrap();
        assert_eq!(list(&c, "confirmed").items[0].person_id, Some(8));
    }
    #[test]
    fn batch_split_creates_one_person_for_all_selected_faces_and_keeps_old_identities() {
        let c = db();
        let mut r = request(ReviewAction::AssignNew, list(&c, "suggested").items);
        r.name = Some("Carol".into());
        let result = apply(&c, &r).unwrap();
        let id = result.person_id.unwrap();
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM faces WHERE person_id=?1",
                [id],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM persons WHERE id IN (7,8)",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        assert_eq!(list(&c, "confirmed").total, 2);
    }
    #[test]
    fn stale_targets_duplicate_names_and_failed_split_transactions_leave_no_partial_people() {
        let c = db();
        let items = list(&c, "suggested").items;
        let mut r = request(ReviewAction::AssignExisting, items.clone());
        r.target_person_id = Some(8);
        r.expected_target_name = Some("Old Bob".into());
        assert!(apply(&c, &r).is_err());
        let mut r = request(ReviewAction::AssignNew, items);
        r.name = Some("Alice".into());
        assert!(apply(&c, &r).is_err());
        r.name = Some("Carol".into());
        c.execute("UPDATE afiles SET height=0 WHERE id=2", [])
            .unwrap();
        // Refresh snapshots so failure occurs during capture, after a new person/first annotation was written.
        r.items = list(&c, "suggested").items;
        assert!(apply(&c, &r).is_err());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM persons", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(list(&c, "confirmed").total, 0);
        assert_eq!(list(&c, "suggested").total, 2);
    }
    #[test]
    fn person_scoped_review_never_returns_other_people_or_unassigned_faces() {
        let c = db();
        let result = page(
            &c,
            &PageRequest {
                library_id: "lib".into(),
                filter: "all".into(),
                person_id: Some(7),
                offset: 0,
                limit: 36,
            },
        )
        .unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.items[0].face_id, Some(11));
        assert_eq!(result.counts["suggested"], 1);
        assert!(!result.counts.contains_key("unknown"));
    }
    #[test]
    fn explicit_person_id_search_can_reach_unnamed_people_beyond_the_first_page() {
        let c = db();
        for id in 100..200 {
            c.execute(
                "INSERT INTO persons(id,name,manual) VALUES(?1,NULL,0)",
                [id],
            )
            .unwrap();
        }
        let found = people(&c, "#199").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0]["id"], 199);
    }
}
