use super::types::Task;
use rusqlite::{Connection, OptionalExtension, params};
use std::sync::Mutex;
static PROFILE_LOCK: Mutex<()> = Mutex::new(());
// One active derived index per task/library in this early-development version.
// No migration of legacy AI vectors: they are intentionally discarded on first use.
pub fn ensure(task: Task, fingerprint: &str) -> Result<bool, String> {
    let _lock = PROFILE_LOCK.lock().map_err(|e| e.to_string())?;
    let conn = crate::t_sqlite::open_conn()?;
    ensure_on(&conn, task, fingerprint)
}
pub fn ensure_on(conn: &Connection, task: Task, fingerprint: &str) -> Result<bool, String> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS ai_index_profiles(task TEXT PRIMARY KEY, fingerprint TEXT NOT NULL);").map_err(|e|e.to_string())?;
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let old = conn
            .query_row(
                "SELECT fingerprint FROM ai_index_profiles WHERE task=?1",
                [task.key()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if old.as_deref() == Some(fingerprint) {
            return Ok(false);
        }
        match task {
            Task::Semantic => {
                conn.execute("UPDATE afiles SET embeds=NULL", [])
                    .map_err(|e| e.to_string())?;
                let has_table:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='similarity_scans')",[],|r|r.get(0)).map_err(|e|e.to_string())?;
                if has_table {
                    conn.execute("DELETE FROM similarity_scans", [])
                        .map_err(|e| e.to_string())?;
                }
            }
            Task::Face => {
                super::face_annotations::reset_model_results(conn)?;
            }
        }
        conn.execute("INSERT INTO ai_index_profiles(task,fingerprint) VALUES(?1,?2) ON CONFLICT(task) DO UPDATE SET fingerprint=excluded.fingerprint",params![task.key(),fingerprint]).map_err(|e|e.to_string())?;
        Ok(true)
    })();
    match result {
        Ok(changed) => {
            conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            Ok(changed)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE afiles(id INTEGER PRIMARY KEY,embeds BLOB,has_faces INTEGER,name TEXT,width INTEGER,height INTEGER,modified_at INTEGER,size INTEGER); CREATE TABLE faces(id INTEGER PRIMARY KEY,file_id INTEGER,bbox TEXT,embedding BLOB,person_id INTEGER,created_at INTEGER); CREATE TABLE persons(id INTEGER PRIMARY KEY,name TEXT,cover_face_id INTEGER,thumbnail BLOB,created_at INTEGER); INSERT INTO afiles VALUES(1,X'01020304',1,'original.jpg',100,100,10,100); INSERT INTO faces VALUES(1,1,NULL,NULL,1,0); INSERT INTO persons VALUES(1,NULL,NULL,NULL,0);").unwrap();
        c
    }
    #[test]
    fn same_profile_preserves_vectors_and_different_profile_invalidates_only_ai() {
        let c = db();
        ensure_on(&c, Task::Semantic, "a").unwrap();
        c.execute("UPDATE afiles SET embeds=X'01020304'", [])
            .unwrap();
        assert!(!ensure_on(&c, Task::Semantic, "a").unwrap());
        assert!(ensure_on(&c, Task::Semantic, "b").unwrap());
        let (name, emb): (String, Option<Vec<u8>>) = c
            .query_row("SELECT name,embeds FROM afiles", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(name, "original.jpg");
        assert!(emb.is_none());
        assert_eq!(
            c.query_row("SELECT count(*) FROM faces", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn incompatible_face_profiles_preserve_manual_names_and_only_invalidate_vectors() {
        let c = db();
        c.execute("UPDATE persons SET name='Alice'", []).unwrap();
        let bbox=serde_json::json!({"x":10,"y":10,"width":30,"height":30,"confidence":1,"landmarks":null}).to_string();
        c.execute("UPDATE faces SET bbox=?1,embedding=X'01020304'", [bbox])
            .unwrap();
        ensure_on(&c, Task::Face, "model-a").unwrap();
        ensure_on(&c, Task::Face, "model-b").unwrap();
        assert_eq!(
            c.query_row("SELECT name FROM persons", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "Alice"
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM faces WHERE embedding IS NULL AND person_id=1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT fingerprint FROM ai_index_profiles WHERE task='face'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "model-b"
        );
    }
    #[test]
    fn face_profile_does_not_remove_media() {
        let c = db();
        ensure_on(&c, Task::Face, "x").unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM afiles", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM persons", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

pub fn store_vector(
    conn: &Connection,
    library: &str,
    profile: &str,
    file_id: i64,
    vector: &[f32],
    source: (Option<i64>, i64),
) -> Result<(), String> {
    let _lock = PROFILE_LOCK.lock().map_err(|e| e.to_string())?;
    if crate::t_config::current_library_id()? != library
        || super::settings::active(Task::Semantic)?.profile() != profile
    {
        return Err(
            "Library or model changed while inference was running; result discarded".into(),
        );
    }
    let active: String = conn
        .query_row(
            "SELECT fingerprint FROM ai_index_profiles WHERE task='semantic'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if active != profile {
        return Err("Index profile changed during inference".into());
    }
    let bytes = vector
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<_>>();
    let changed = conn
        .execute(
            "UPDATE afiles SET embeds=?1 WHERE id=?2 AND modified_at IS ?3 AND size=?4",
            params![bytes, file_id, source.0, source.1],
        )
        .map_err(|e| e.to_string())?;
    if changed != 1 {
        return Err("Image changed or was removed during inference; result discarded".into());
    }
    Ok(())
}

pub fn ensure_selected() -> Result<(), String> {
    for task in [Task::Semantic, Task::Face] {
        let model = super::settings::active(task)?;
        ensure(task, &model.profile())?;
    }
    Ok(())
}

pub fn invalidate_file(file_id: i64) -> Result<(), String> {
    let _lock = PROFILE_LOCK.lock().map_err(|e| e.to_string())?;
    let conn = crate::t_sqlite::open_conn()?;
    super::face_annotations::import_existing(&conn)?;
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        conn.execute(
            "UPDATE afiles SET embeds=NULL,has_faces=0 WHERE id=?1",
            [file_id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM faces WHERE file_id=?1", [file_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}
