//! Model-independent human annotations. Embeddings and automatic assignments remain disposable.
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Region {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
#[derive(Clone, Debug)]
pub struct Annotation {
    pub id: i64,
    pub person_id: Option<i64>,
    pub kind: String,
    pub region: Region,
}
fn column(conn: &Connection, table: &str, name: &str) -> Result<bool, String> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|e| e.to_string())?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(names.iter().any(|candidate| candidate == name))
}
pub fn ensure_schema(conn: &Connection) -> Result<(), String> {
    if !column(conn, "persons", "manual")? {
        conn.execute(
            "ALTER TABLE persons ADD COLUMN manual INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if !column(conn, "faces", "annotation_id")? {
        conn.execute("ALTER TABLE faces ADD COLUMN annotation_id INTEGER", [])
            .map_err(|e| e.to_string())?;
    }
    conn.execute_batch("CREATE TABLE IF NOT EXISTS face_annotations(id INTEGER PRIMARY KEY AUTOINCREMENT,file_id INTEGER NOT NULL,person_id INTEGER,kind TEXT NOT NULL CHECK(kind IN ('confirmed','unassigned','ignored','not_face')),region TEXT NOT NULL,source_modified_at INTEGER,source_size INTEGER NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,FOREIGN KEY(file_id) REFERENCES afiles(id) ON DELETE CASCADE,FOREIGN KEY(person_id) REFERENCES persons(id) ON DELETE SET NULL); CREATE UNIQUE INDEX IF NOT EXISTS idx_faces_annotation ON faces(annotation_id) WHERE annotation_id IS NOT NULL; CREATE INDEX IF NOT EXISTS idx_face_annotations_file ON face_annotations(file_id); CREATE INDEX IF NOT EXISTS idx_face_annotations_person ON face_annotations(person_id); CREATE TABLE IF NOT EXISTS face_annotation_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);").map_err(|e|e.to_string())?;
    Ok(())
}
fn normalized(bbox: &str, width: i64, height: i64) -> Result<Region, String> {
    let bbox: crate::t_face::FaceBox =
        serde_json::from_str(bbox).map_err(|_| "Invalid face region")?;
    if width <= 0
        || height <= 0
        || ![bbox.x, bbox.y, bbox.width, bbox.height]
            .iter()
            .all(|v| v.is_finite())
        || bbox.width <= 0.
        || bbox.height <= 0.
    {
        return Err("Invalid source dimensions or face region".into());
    }
    let x = bbox.x.max(0.) / width as f32;
    let y = bbox.y.max(0.) / height as f32;
    let right = (bbox.x + bbox.width).min(width as f32) / width as f32;
    let bottom = (bbox.y + bbox.height).min(height as f32) / height as f32;
    if right <= x || bottom <= y {
        return Err("Face region is outside the source image".into());
    }
    Ok(Region {
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}
pub fn capture_face(conn: &Connection, face_id: i64, kind: &str) -> Result<i64, String> {
    if !["confirmed", "unassigned", "ignored", "not_face"].contains(&kind) {
        return Err("Unsupported annotation state".into());
    }
    let (file,person,bbox,annotation,width,height,modified,size)=conn.query_row("SELECT f.file_id,f.person_id,f.bbox,f.annotation_id,a.width,a.height,a.modified_at,a.size FROM faces f JOIN afiles a ON a.id=f.file_id WHERE f.id=?1",[face_id],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,Option<i64>>(1)?,row.get::<_,String>(2)?,row.get::<_,Option<i64>>(3)?,row.get::<_,i64>(4)?,row.get::<_,i64>(5)?,row.get::<_,Option<i64>>(6)?,row.get::<_,i64>(7)?))).map_err(|_|"Face or source image no longer exists")?;
    let person = if kind == "confirmed" { person } else { None };
    if kind == "confirmed" && person.is_none() {
        return Err("Choose a person before confirming a face".into());
    }
    let region =
        serde_json::to_string(&normalized(&bbox, width, height)?).map_err(|e| e.to_string())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_secs() as i64)
        .unwrap_or(0);
    let annotation = if let Some(id) = annotation {
        conn.execute("UPDATE face_annotations SET person_id=?1,kind=?2,region=?3,source_modified_at=?4,source_size=?5,updated_at=?6 WHERE id=?7",params![person,kind,region,modified,size,now,id]).map_err(|e|e.to_string())?;
        id
    } else {
        conn.execute("INSERT INTO face_annotations(file_id,person_id,kind,region,source_modified_at,source_size,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",params![file,person,kind,region,modified,size,now]).map_err(|e|e.to_string())?;
        conn.last_insert_rowid()
    };
    conn.execute(
        "UPDATE faces SET annotation_id=?1 WHERE id=?2",
        params![annotation, face_id],
    )
    .map_err(|e| e.to_string())?;
    if let Some(person) = person {
        conn.execute("UPDATE persons SET manual=1 WHERE id=?1", [person])
            .map_err(|e| e.to_string())?;
    }
    Ok(annotation)
}
/// One-time conservative import: the old schema had no provenance; protect named people's existing associations.
pub fn import_existing(conn: &Connection) -> Result<(), String> {
    ensure_schema(conn)?;
    let imported: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM face_annotation_meta WHERE key='imported-v1')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if imported {
        return Ok(());
    }
    let owns_transaction = conn.is_autocommit();
    if owns_transaction {
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;
    }
    let result = (|| {
        conn.execute(
            "UPDATE persons SET manual=1 WHERE name IS NOT NULL AND TRIM(name)<>''",
            [],
        )
        .map_err(|e| e.to_string())?;
        let rows = {
            let mut statement=conn.prepare("SELECT f.id,f.bbox,a.width,a.height FROM faces f JOIN persons p ON p.id=f.person_id JOIN afiles a ON a.id=f.file_id WHERE p.manual=1 AND f.annotation_id IS NULL AND a.width>0 AND a.height>0 AND f.bbox IS NOT NULL").map_err(|e|e.to_string())?;
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };
        for (id, bbox, width, height) in rows {
            // Legacy malformed regions cannot be recovered, but the person's name remains protected.
            if normalized(&bbox, width, height).is_ok() {
                capture_face(conn, id, "confirmed")?;
            }
        }
        conn.execute(
            "INSERT INTO face_annotation_meta(key,value) VALUES('imported-v1','1')",
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            if owns_transaction {
                conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            }
            Ok(())
        }
        Err(error) => {
            if owns_transaction {
                let _ = conn.execute_batch("ROLLBACK");
            }
            Err(error)
        }
    }
}
pub fn active_for_file(conn: &Connection, file: i64) -> Result<Vec<Annotation>, String> {
    let mut statement=conn.prepare("SELECT n.id,n.file_id,n.person_id,n.kind,n.region FROM face_annotations n JOIN afiles a ON a.id=n.file_id WHERE n.file_id=?1 AND n.source_modified_at IS a.modified_at AND n.source_size=a.size ORDER BY n.id").map_err(|e|e.to_string())?;
    let rows = statement
        .query_map([file], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.map(|row| {
        let (id, _file_id, person_id, kind, region) = row.map_err(|e| e.to_string())?;
        let region =
            serde_json::from_str(&region).map_err(|_| "Invalid stored annotation region")?;
        Ok(Annotation {
            id,
            person_id,
            kind,
            region,
        })
    })
    .collect()
}
pub fn pixel_box(annotation: &Annotation, width: i64, height: i64) -> crate::t_face::FaceBox {
    crate::t_face::FaceBox {
        x: annotation.region.x * width as f32,
        y: annotation.region.y * height as f32,
        width: annotation.region.width * width as f32,
        height: annotation.region.height * height as f32,
        confidence: 0.,
        landmarks: None,
    }
}
pub fn restore_file(conn: &Connection, file: i64) -> Result<(), String> {
    let dimensions = conn
        .query_row(
            "SELECT width,height FROM afiles WHERE id=?1",
            [file],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((Some(width), Some(height))) = dimensions else {
        return Ok(());
    };
    if width <= 0 || height <= 0 {
        return Ok(());
    }
    for annotation in active_for_file(conn, file)? {
        if !["confirmed", "unassigned"].contains(&annotation.kind.as_str()) {
            continue;
        }
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM faces WHERE annotation_id=?1)",
                [annotation.id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists {
            continue;
        }
        let bbox = serde_json::to_string(&pixel_box(&annotation, width, height))
            .map_err(|e| e.to_string())?;
        conn.execute("INSERT INTO faces(file_id,bbox,embedding,person_id,annotation_id,created_at) VALUES(?1,?2,NULL,?3,?4,0)",params![file,bbox,annotation.person_id,annotation.id]).map_err(|e|e.to_string())?;
    }
    Ok(())
}
pub fn reset_model_results(conn: &Connection) -> Result<(), String> {
    import_existing(conn)?;
    conn.execute_batch("DELETE FROM faces; UPDATE persons SET cover_face_id=NULL; DELETE FROM persons WHERE manual=0; UPDATE afiles SET has_faces=0;").map_err(|e|e.to_string())?;
    let files = {
        let mut s = conn
            .prepare("SELECT DISTINCT file_id FROM face_annotations ORDER BY file_id")
            .map_err(|e| e.to_string())?;
        s.query_map([], |row| row.get::<_, i64>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    for file in files {
        restore_file(conn, file)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE afiles(id INTEGER PRIMARY KEY,width INTEGER,height INTEGER,modified_at INTEGER,size INTEGER,has_faces INTEGER); CREATE TABLE persons(id INTEGER PRIMARY KEY,name TEXT,cover_face_id INTEGER,thumbnail BLOB,created_at INTEGER); CREATE TABLE faces(id INTEGER PRIMARY KEY,file_id INTEGER,bbox TEXT,embedding BLOB,person_id INTEGER,created_at INTEGER); INSERT INTO afiles VALUES(1,100,100,10,100,1); INSERT INTO persons VALUES(7,'Alice',11,X'0102',0),(8,NULL,12,NULL,0);").unwrap();
        let bbox=serde_json::json!({"x":10,"y":10,"width":30,"height":30,"confidence":1,"landmarks":null}).to_string();
        c.execute(
            "INSERT INTO faces VALUES(11,1,?1,X'01020304',7,0)",
            [bbox.clone()],
        )
        .unwrap();
        c.execute("INSERT INTO faces VALUES(12,1,?1,X'01020304',8,0)", [bbox])
            .unwrap();
        c
    }
    #[test]
    fn model_reset_keeps_human_identity_regions_and_clears_model_vectors() {
        let c = db();
        c.execute_batch("BEGIN IMMEDIATE").unwrap();
        reset_model_results(&c).unwrap();
        c.execute_batch("COMMIT").unwrap();
        assert_eq!(
            c.query_row("SELECT name FROM persons WHERE id=7", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Alice"
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM persons", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM faces WHERE person_id=7 AND embedding IS NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            c.query_row("SELECT has_faces FROM afiles", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn changed_source_keeps_annotation_but_does_not_apply_it_to_new_content() {
        let c = db();
        import_existing(&c).unwrap();
        c.execute("UPDATE afiles SET modified_at=20", []).unwrap();
        reset_model_results(&c).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM face_annotations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM faces", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            c.query_row("SELECT name FROM persons WHERE id=7", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Alice"
        );
    }
    #[test]
    fn explicit_unassignment_survives_model_changes() {
        let c = db();
        import_existing(&c).unwrap();
        c.execute("UPDATE faces SET person_id=NULL WHERE id=11", [])
            .unwrap();
        capture_face(&c, 11, "unassigned").unwrap();
        reset_model_results(&c).unwrap();
        assert_eq!(
            c.query_row("SELECT person_id FROM faces", [], |r| r
                .get::<_, Option<i64>>(0))
                .unwrap(),
            None
        );
        assert_eq!(active_for_file(&c, 1).unwrap()[0].kind, "unassigned");
    }
    #[test]
    fn import_is_idempotent_and_names_with_no_faces_are_preserved() {
        let c = db();
        import_existing(&c).unwrap();
        import_existing(&c).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM face_annotations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        c.execute(
            "INSERT INTO persons(id,name,created_at,manual) VALUES(9,'Bob',0,1)",
            [],
        )
        .unwrap();
        reset_model_results(&c).unwrap();
        assert_eq!(
            c.query_row("SELECT name FROM persons WHERE id=9", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Bob"
        );
    }
}
