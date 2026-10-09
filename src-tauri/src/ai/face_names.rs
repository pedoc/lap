//! Transactional face naming distinguishes global person rename from one-face reassignment.
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EditMode {
    Rename,
    AssignExisting,
    AssignNew,
    Unassign,
    Confirm,
    Ignore,
    NotFace,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FaceNameRequest {
    pub library_id: String,
    pub profile: String,
    pub file_id: i64,
    pub face_id: i64,
    pub expected_person_id: Option<i64>,
    pub expected_name: Option<String>,
    pub mode: EditMode,
    pub name: Option<String>,
    pub target_person_id: Option<i64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceNameChange {
    pub face_id: i64,
    pub file_id: i64,
    pub person_id: Option<i64>,
    pub previous_person_id: Option<i64>,
    pub name: Option<String>,
    pub mode: EditMode,
}
pub fn valid_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 128 || name.chars().any(char::is_control) {
        return Err("Person names must contain 1–128 characters without control characters".into());
    }
    Ok(name)
}
fn unique_name(conn: &Connection, name: &str, except: Option<i64>) -> Result<(), String> {
    let exists: bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM persons WHERE name=?1 COLLATE NOCASE AND (?2 IS NULL OR id<>?2))",params![name,except],|row|row.get(0)).map_err(|e|e.to_string())?;
    if exists {
        return Err("A person with this name already exists. Change this face's assignment instead of renaming or creating a duplicate person".into());
    }
    Ok(())
}
fn rename_in_transaction(conn: &Connection, person_id: i64, name: &str) -> Result<usize, String> {
    let name = valid_name(name)?;
    unique_name(conn, name, Some(person_id))?;
    let affected = conn
        .execute(
            "UPDATE persons SET name=?1,manual=1 WHERE id=?2",
            params![name, person_id],
        )
        .map_err(|e| e.to_string())?;
    if affected != 1 {
        return Err("Person no longer exists".into());
    }
    Ok(affected)
}
pub fn rename_person(conn: &Connection, person_id: i64, name: &str) -> Result<usize, String> {
    super::face_annotations::import_existing(conn)?;
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let count = rename_in_transaction(conn, person_id, name)?;
        let face = conn
            .query_row(
                "SELECT id FROM faces WHERE person_id=?1 ORDER BY id LIMIT 1",
                [person_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(face) = face {
            super::face_annotations::capture_face(conn, face, "confirmed")?;
        }
        Ok(count)
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
pub fn edit(conn: &Connection, request: &FaceNameRequest) -> Result<FaceNameChange, String> {
    if request.face_id <= 0 || request.file_id <= 0 {
        return Err("Invalid face or image ID".into());
    }
    super::face_annotations::import_existing(conn)?;
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = edit_in_transaction(conn, request);
    match result {
        Ok(change) => {
            conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            Ok(change)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}
pub(crate) fn edit_in_transaction(
    conn: &Connection,
    request: &FaceNameRequest,
) -> Result<FaceNameChange, String> {
    let current=conn.query_row("SELECT f.person_id,p.name FROM faces f LEFT JOIN persons p ON p.id=f.person_id LEFT JOIN face_annotations n ON n.id=f.annotation_id JOIN afiles a ON a.id=f.file_id WHERE f.id=?1 AND f.file_id=?2 AND (n.id IS NULL OR (n.kind IN ('confirmed','unassigned') AND n.source_modified_at IS a.modified_at AND n.source_size=a.size))",params![request.face_id,request.file_id],|row|Ok((row.get::<_,Option<i64>>(0)?,row.get::<_,Option<String>>(1)?))).optional().map_err(|e|e.to_string())?.ok_or("Face no longer exists in this image; reload the face labels")?;
    if current.0 != request.expected_person_id || current.1 != request.expected_name {
        return Err("This face's person or name changed while editing; reopen the editor".into());
    }
    let (person_id, name) = match request.mode {
        EditMode::Rename => {
            let person = current
                .0
                .ok_or("An unassigned face must be assigned to a person before renaming")?;
            let name = valid_name(request.name.as_deref().unwrap_or(""))?;
            rename_in_transaction(conn, person, name)?;
            (Some(person), Some(name.to_string()))
        }
        EditMode::AssignExisting => {
            let target = request
                .target_person_id
                .filter(|id| *id > 0)
                .ok_or("Choose an existing person")?;
            let name = conn
                .query_row("SELECT name FROM persons WHERE id=?1", [target], |row| {
                    row.get::<_, Option<String>>(0)
                })
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or("Target person no longer exists")?;
            (Some(target), name)
        }
        EditMode::AssignNew => {
            let name = valid_name(request.name.as_deref().unwrap_or(""))?;
            unique_name(conn, name, None)?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0);
            conn.execute(
                "INSERT INTO persons(name,created_at,cover_face_id) VALUES(?1,?2,?3)",
                params![name, now, request.face_id],
            )
            .map_err(|e| e.to_string())?;
            (Some(conn.last_insert_rowid()), Some(name.to_string()))
        }
        EditMode::Unassign | EditMode::Ignore | EditMode::NotFace => (None, None),
        EditMode::Confirm => {
            let person = current
                .0
                .ok_or("Assign a person before confirming this face")?;
            (Some(person), current.1.clone())
        }
    };
    if request.mode != EditMode::Rename {
        conn.execute(
            "UPDATE faces SET person_id=?1 WHERE id=?2 AND file_id=?3",
            params![person_id, request.face_id, request.file_id],
        )
        .map_err(|e| e.to_string())?;
        if current.0 != person_id {
            if let Some(previous) = current.0 {
                conn.execute("UPDATE persons SET cover_face_id=NULL,thumbnail=NULL WHERE id=?1 AND cover_face_id=?2",params![previous,request.face_id]).map_err(|e|e.to_string())?;
            }
            if let Some(person) = person_id {
                conn.execute("UPDATE persons SET cover_face_id=COALESCE(cover_face_id,?2),thumbnail=NULL WHERE id=?1",params![person,request.face_id]).map_err(|e|e.to_string())?;
            }
        }
    }
    super::face_annotations::capture_face(
        conn,
        request.face_id,
        match request.mode {
            EditMode::Ignore => "ignored",
            EditMode::NotFace => "not_face",
            _ if person_id.is_some() => "confirmed",
            _ => "unassigned",
        },
    )?;
    Ok(FaceNameChange {
        face_id: request.face_id,
        file_id: request.file_id,
        person_id,
        previous_person_id: current.0,
        name,
        mode: request.mode,
    })
}
/// Explicit person deletion is one transaction, including durable annotation updates.
pub fn delete_person(conn: &Connection, person_id: i64) -> Result<usize, String> {
    super::face_annotations::import_existing(conn)?;
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        conn.execute(
            "UPDATE face_annotations SET person_id=NULL,kind='unassigned' WHERE person_id=?1",
            [person_id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE faces SET person_id=NULL WHERE person_id=?1",
            [person_id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM persons WHERE id=?1", [person_id])
            .map_err(|e| e.to_string())
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
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE afiles(id INTEGER PRIMARY KEY,width INTEGER,height INTEGER,modified_at INTEGER,size INTEGER); INSERT INTO afiles VALUES(1,100,100,10,100),(2,100,100,10,100); CREATE TABLE persons(id INTEGER PRIMARY KEY,name TEXT,created_at INTEGER,cover_face_id INTEGER,thumbnail BLOB); CREATE TABLE faces(id INTEGER PRIMARY KEY,file_id INTEGER,person_id INTEGER,bbox TEXT); INSERT INTO persons VALUES(7,'Alice',0,11,X'0102'),(8,'Bob',0,13,NULL); INSERT INTO faces VALUES(11,1,7,'box'),(12,2,7,'box'),(13,2,8,'box'),(14,1,NULL,'box');").unwrap();
        let bbox=serde_json::json!({"x":10,"y":10,"width":20,"height":20,"confidence":1,"landmarks":null}).to_string();
        conn.execute("UPDATE faces SET bbox=?1", [bbox]).unwrap();
        conn
    }
    fn request(mode: EditMode) -> FaceNameRequest {
        FaceNameRequest {
            library_id: "lib".into(),
            profile: "profile".into(),
            file_id: 1,
            face_id: 11,
            expected_person_id: Some(7),
            expected_name: Some("Alice".into()),
            mode,
            name: Some("Alice New".into()),
            target_person_id: Some(8),
        }
    }
    #[test]
    fn rename_changes_the_person_name_not_face_membership() {
        let c = db();
        let result = edit(&c, &request(EditMode::Rename)).unwrap();
        assert_eq!(result.person_id, Some(7));
        assert_eq!(
            c.query_row("SELECT name FROM persons WHERE id=7", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Alice New"
        );
        assert_eq!(
            c.query_row("SELECT person_id FROM faces WHERE id=12", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            7
        );
    }
    #[test]
    fn correction_moves_only_one_face_and_keeps_other_people_names() {
        let c = db();
        edit(&c, &request(EditMode::AssignExisting)).unwrap();
        assert_eq!(
            c.query_row("SELECT person_id FROM faces WHERE id=11", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            8
        );
        assert_eq!(
            c.query_row("SELECT person_id FROM faces WHERE id=12", [], |r| r
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
        assert!(
            c.query_row("SELECT cover_face_id FROM persons WHERE id=7", [], |r| {
                r.get::<_, Option<i64>>(0)
            })
            .unwrap()
            .is_none()
        );
    }
    #[test]
    fn unassigned_face_can_create_and_join_a_named_person() {
        let c = db();
        let mut r = request(EditMode::AssignNew);
        r.face_id = 14;
        r.expected_person_id = None;
        r.expected_name = None;
        r.name = Some(" Carol ".into());
        let result = edit(&c, &r).unwrap();
        assert_eq!(result.name.as_deref(), Some("Carol"));
        assert_eq!(
            c.query_row("SELECT person_id FROM faces WHERE id=14", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            result.person_id.unwrap()
        );
    }
    #[test]
    fn stale_face_or_name_edits_do_not_overwrite_newer_changes() {
        let c = db();
        let mut r = request(EditMode::Rename);
        r.expected_name = Some("stale".into());
        assert!(edit(&c, &r).is_err());
        r.expected_name = Some("Alice".into());
        r.file_id = 99;
        assert!(edit(&c, &r).is_err());
        assert_eq!(
            c.query_row("SELECT name FROM persons WHERE id=7", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Alice"
        );
    }
    #[test]
    fn duplicate_names_never_silently_merge_people() {
        let c = db();
        let mut r = request(EditMode::Rename);
        r.name = Some("bob".into());
        assert!(edit(&c, &r).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM persons", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn invalid_target_or_name_rolls_back_without_orphan_people() {
        let c = db();
        let mut r = request(EditMode::AssignExisting);
        r.target_person_id = Some(99);
        assert!(edit(&c, &r).is_err());
        r.mode = EditMode::AssignNew;
        r.name = Some(" ".into());
        assert!(edit(&c, &r).is_err());
        assert_eq!(
            c.query_row("SELECT person_id FROM faces WHERE id=11", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            7
        );
    }
    #[test]
    fn unassign_keeps_the_detected_region_and_existing_person_record() {
        let c = db();
        edit(&c, &request(EditMode::Unassign)).unwrap();
        assert!(
            c.query_row("SELECT person_id FROM faces WHERE id=11", [], |r| r
                .get::<_, Option<i64>>(0))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM faces", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM persons", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn names_are_trimmed_and_control_characters_or_excessive_lengths_are_rejected() {
        assert_eq!(valid_name(" 张三 ").unwrap(), "张三");
        assert!(valid_name("a\nb").is_err());
        assert!(valid_name(&"字".repeat(129)).is_err());
    }
}
