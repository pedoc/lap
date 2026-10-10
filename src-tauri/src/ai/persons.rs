//! Explicit human person merges; observations/vectors remain in their current model space.
use rusqlite::{Connection, OptionalExtension, params_from_iter};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonSnapshot {
    pub id: i64,
    pub name: Option<String>,
    pub face_count: usize,
    pub annotation_count: usize,
    pub fingerprint: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MergePreview {
    pub target: PersonSnapshot,
    pub sources: Vec<PersonSnapshot>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewRequest {
    pub library_id: String,
    pub profile: String,
    pub target_person_id: i64,
    pub source_person_ids: Vec<i64>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MergeRequest {
    pub library_id: String,
    pub profile: String,
    pub preview: MergePreview,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    pub person_id: i64,
    pub name: Option<String>,
    pub source_person_ids: Vec<i64>,
    pub moved_faces: usize,
    pub moved_annotations: usize,
}
fn sources(ids: &[i64], target: i64) -> Result<Vec<i64>, String> {
    if target <= 0
        || ids.is_empty()
        || ids.len() > 100
        || ids.iter().any(|id| *id <= 0 || *id == target)
    {
        return Err("Choose 1–100 source people and a different target person".into());
    }
    let unique = ids.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() != ids.len() {
        return Err("Duplicate source person selection".into());
    }
    Ok(unique.into_iter().collect())
}
fn snapshot(conn: &Connection, id: i64) -> Result<PersonSnapshot, String> {
    let (name, manual, cover): (Option<String>, i64, Option<i64>) = conn
        .query_row(
            "SELECT name,manual,cover_face_id FROM persons WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("Person no longer exists; refresh the merge preview")?;
    let mut hash = Sha256::new();
    hash.update(
        serde_json::to_vec(&serde_json::json!([id, name, manual, cover]))
            .map_err(|e| e.to_string())?,
    );
    let mut face_count = 0;
    {
        let mut statement=conn.prepare("SELECT f.id,f.file_id,f.bbox,f.annotation_id,a.width,a.height,a.modified_at,a.size FROM faces f JOIN afiles a ON a.id=f.file_id WHERE f.person_id=?1 ORDER BY f.id").map_err(|e|e.to_string())?;
        let rows = statement
            .query_map([id], |row| {
                Ok(serde_json::json!([
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, i64>(7)?
                ]))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            hash.update(
                serde_json::to_vec(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?,
            );
            hash.update(b"\n");
            face_count += 1;
        }
    }
    let mut annotation_count = 0;
    {
        let mut statement=conn.prepare("SELECT id,file_id,kind,region,source_modified_at,source_size,updated_at FROM face_annotations WHERE person_id=?1 ORDER BY id").map_err(|e|e.to_string())?;
        let rows = statement
            .query_map([id], |row| {
                Ok(serde_json::json!([
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?
                ]))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            hash.update(
                serde_json::to_vec(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?,
            );
            hash.update(b"\n");
            annotation_count += 1;
        }
    }
    Ok(PersonSnapshot {
        id,
        name,
        face_count,
        annotation_count,
        fingerprint: format!("{:x}", hash.finalize()),
    })
}
pub fn preview(conn: &Connection, target: i64, ids: &[i64]) -> Result<MergePreview, String> {
    let ids = sources(ids, target)?;
    Ok(MergePreview {
        target: snapshot(conn, target)?,
        sources: ids
            .into_iter()
            .map(|id| snapshot(conn, id))
            .collect::<Result<Vec<_>, _>>()?,
    })
}
pub fn merge(conn: &Connection, expected: &MergePreview) -> Result<MergeResult, String> {
    let target = expected.target.id;
    let ids = sources(
        &expected
            .sources
            .iter()
            .map(|person| person.id)
            .collect::<Vec<_>>(),
        target,
    )?;
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        if preview(conn, target, &ids)? != *expected {
            return Err(
                "People, faces or source files changed; reload the merge preview before confirming"
                    .into(),
            );
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        // Existing stale annotations are transferred as history, never recaptured against new source bytes.
        let faces = {
            let mut statement=conn.prepare(&format!("SELECT f.id FROM faces f JOIN afiles a ON a.id=f.file_id LEFT JOIN face_annotations n ON n.id=f.annotation_id WHERE f.person_id IN ({placeholders}) AND (n.id IS NULL OR (n.kind='confirmed' AND n.source_modified_at IS a.modified_at AND n.source_size=a.size)) ORDER BY f.id")).map_err(|e|e.to_string())?;
            statement
                .query_map(params_from_iter(&ids), |row| row.get::<_, i64>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };
        let arguments = std::iter::once(target)
            .chain(ids.iter().copied())
            .collect::<Vec<_>>();
        conn.execute(
            &format!("UPDATE faces SET person_id=? WHERE person_id IN ({placeholders})"),
            params_from_iter(&arguments),
        )
        .map_err(|e| e.to_string())?;
        conn.execute(&format!("UPDATE face_annotations SET person_id=?,updated_at=CAST(strftime('%s','now') AS INTEGER) WHERE person_id IN ({placeholders})"),params_from_iter(&arguments)).map_err(|e|e.to_string())?;
        for face in faces {
            super::face_annotations::capture_face(conn, face, "confirmed")?;
        }
        conn.execute("UPDATE persons SET manual=1,thumbnail=NULL,cover_face_id=COALESCE((SELECT id FROM faces WHERE person_id=?1 AND id=persons.cover_face_id),(SELECT id FROM faces WHERE person_id=?1 ORDER BY id LIMIT 1)) WHERE id=?1",[target]).map_err(|e|e.to_string())?;
        conn.execute(
            &format!("DELETE FROM persons WHERE id IN ({placeholders})"),
            params_from_iter(&ids),
        )
        .map_err(|e| e.to_string())?;
        Ok(MergeResult {
            person_id: target,
            name: expected.target.name.clone(),
            source_person_ids: ids,
            moved_faces: expected
                .sources
                .iter()
                .map(|person| person.face_count)
                .sum(),
            moved_annotations: expected
                .sources
                .iter()
                .map(|person| person.annotation_count)
                .sum(),
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
#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE afiles(id INTEGER PRIMARY KEY,width INTEGER,height INTEGER,modified_at INTEGER,size INTEGER,has_faces INTEGER); CREATE TABLE persons(id INTEGER PRIMARY KEY AUTOINCREMENT,name TEXT,created_at INTEGER,cover_face_id INTEGER,thumbnail BLOB); CREATE TABLE faces(id INTEGER PRIMARY KEY AUTOINCREMENT,file_id INTEGER,person_id INTEGER,bbox TEXT,embedding BLOB,created_at INTEGER); INSERT INTO afiles VALUES(1,100,100,10,100,1),(2,100,100,10,100,1); INSERT INTO persons VALUES(1,'Alice',0,11,X'01'),(2,'Bob',0,12,X'02'),(3,'Untouched',0,NULL,NULL);").unwrap();
        super::super::face_annotations::ensure_schema(&c).unwrap();
        c.execute(
            "INSERT INTO face_annotation_meta VALUES('imported-v1','1')",
            [],
        )
        .unwrap();
        let bbox=serde_json::json!({"x":10,"y":10,"width":20,"height":20,"confidence":1,"landmarks":null}).to_string();
        c.execute("INSERT INTO faces(id,file_id,person_id,bbox,embedding,created_at) VALUES(11,1,1,?1,X'01020304',0),(12,2,2,?1,X'05060708',0),(13,2,1,?1,X'090a0b0c',0)",[bbox]).unwrap();
        c
    }
    #[test]
    fn merge_preserves_target_name_vectors_regions_cover_and_unrelated_people() {
        let c = db();
        let expected = preview(&c, 2, &[1]).unwrap();
        let result = merge(&c, &expected).unwrap();
        assert_eq!(result.moved_faces, 2);
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM persons", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            c.query_row("SELECT name FROM persons WHERE id=2", [], |row| row
                .get::<_, String>(0))
                .unwrap(),
            "Bob"
        );
        assert_eq!(
            c.query_row("SELECT cover_face_id FROM persons WHERE id=2", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            12
        );
        assert_eq!(
            c.query_row("SELECT hex(embedding) FROM faces WHERE id=11", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap(),
            "01020304"
        );
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM face_annotations WHERE person_id=2 AND kind='confirmed'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        super::super::face_annotations::reset_model_results(&c).unwrap();
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM faces WHERE person_id=2 AND embedding IS NULL",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
    }
    #[test]
    fn stale_name_membership_or_source_previews_never_silently_merge() {
        for change in [
            "UPDATE persons SET name='Changed' WHERE id=1",
            "UPDATE faces SET person_id=3 WHERE id=13",
            "UPDATE afiles SET size=200 WHERE id=1",
        ] {
            let c = db();
            let expected = preview(&c, 2, &[1]).unwrap();
            c.execute(change, []).unwrap();
            assert!(merge(&c, &expected).is_err());
            assert_eq!(
                c.query_row("SELECT COUNT(*) FROM persons", [], |row| row
                    .get::<_, i64>(0))
                    .unwrap(),
                3
            );
        }
    }
    #[test]
    fn failed_annotation_capture_rolls_back_all_transfers_and_person_deletes() {
        let c = db();
        c.execute("UPDATE faces SET bbox='{}' WHERE id=13", [])
            .unwrap();
        let expected = preview(&c, 2, &[1]).unwrap();
        assert!(merge(&c, &expected).is_err());
        assert_eq!(
            c.query_row("SELECT person_id FROM faces WHERE id=11", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM face_annotations", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            c.query_row("SELECT manual FROM persons WHERE id=2", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn merge_transfers_stale_annotations_without_claiming_current_source_confirmation() {
        let c = db();
        super::super::face_annotations::capture_face(&c, 11, "confirmed").unwrap();
        c.execute("UPDATE afiles SET modified_at=20 WHERE id=1", [])
            .unwrap();
        merge(&c, &preview(&c, 2, &[1]).unwrap()).unwrap();
        assert_eq!(
            c.query_row(
                "SELECT source_modified_at FROM face_annotations WHERE file_id=1",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            10
        );
        assert!(
            super::super::face_annotations::active_for_file(&c, 1)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            c.query_row(
                "SELECT person_id FROM face_annotations WHERE file_id=1",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
    }
    #[test]
    fn invalid_missing_self_or_duplicate_selections_are_rejected() {
        let c = db();
        for ids in [vec![], vec![2], vec![1, 1], vec![99], vec![-1]] {
            assert!(preview(&c, 2, &ids).is_err());
        }
        assert!(preview(&c, 99, &[1]).is_err());
    }
    #[test]
    fn empty_manual_identities_can_be_merged_without_fabricated_faces() {
        let c = db();
        c.execute(
            "INSERT INTO persons(id,name,manual) VALUES(99,'Empty',1)",
            [],
        )
        .unwrap();
        let result = merge(&c, &preview(&c, 2, &[99]).unwrap()).unwrap();
        assert_eq!(result.moved_faces, 0);
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM faces", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
}
