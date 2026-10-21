//! Local viewing counters, independent of original media and AI-derived indexes.
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, Serialize)]
pub struct ViewChange {
    pub library_id: String,
    pub file_id: i64,
    pub view_count: i64,
    pub last_viewed_at: i64,
}

pub(crate) fn record_on(
    conn: &Connection,
    library: &str,
    file_id: i64,
    at: i64,
) -> Result<ViewChange, String> {
    if file_id <= 0 || at <= 0 {
        return Err("VIEW_FILE_UNAVAILABLE".into());
    }
    let sql = format!(
        "UPDATE afiles SET view_count=COALESCE(view_count,0)+1,
        last_viewed_at=MAX(COALESCE(last_viewed_at,0),?2)
        WHERE id=?1 AND {} RETURNING view_count,last_viewed_at",
        crate::t_media_lifecycle::retained_predicate("afiles")
    );
    conn.query_row(&sql, params![file_id, at], |row| {
        Ok(ViewChange {
            library_id: library.to_owned(),
            file_id,
            view_count: row.get("view_count")?,
            last_viewed_at: row.get("last_viewed_at")?,
        })
    })
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "VIEW_FILE_UNAVAILABLE".into())
}

#[tauri::command]
pub async fn record_media_view(
    app: AppHandle,
    library_id: String,
    file_id: i64,
) -> Result<ViewChange, String> {
    let change = tauri::async_runtime::spawn_blocking(move || {
        crate::t_cmds::with_current_library(&library_id, || {
            let conn = crate::t_sqlite::open_conn()?;
            record_on(&conn, &library_id, file_id, chrono::Utc::now().timestamp())
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    // A failed notification must not turn a committed visit into a retryable failure.
    if let Err(error) = app.emit("media-view-changed", &change) {
        eprintln!("Viewing statistics notification failed: {error}");
    }
    Ok(change)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(conn: &Connection) {
        conn.execute_batch("CREATE TABLE afiles(id INTEGER PRIMARY KEY,name TEXT,title TEXT,size INTEGER,modified_at INTEGER,
            view_count INTEGER NOT NULL DEFAULT 0,last_viewed_at INTEGER);
            CREATE TABLE media_states(file_id INTEGER PRIMARY KEY,hidden INTEGER DEFAULT 0,deleted_at INTEGER);
            INSERT INTO afiles(id,name,title,size,modified_at) VALUES(1,'original.jpg','Title',100,200);").unwrap();
    }
    #[test]
    fn counters_are_atomic_monotonic_and_do_not_change_source_metadata() {
        let conn = Connection::open_in_memory().unwrap();
        fixture(&conn);
        assert_eq!(record_on(&conn, "lib", 1, 1000).unwrap().view_count, 1);
        let next = record_on(&conn, "lib", 1, 500).unwrap();
        assert_eq!((next.view_count, next.last_viewed_at), (2, 1000));
        assert_eq!(
            conn.query_row(
                "SELECT name,title,size,modified_at FROM afiles WHERE id=1",
                [],
                |r| Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?
                ))
            )
            .unwrap(),
            ("original.jpg".into(), "Title".into(), 100, 200)
        );
        assert!(record_on(&conn, "lib", 999, 1000).is_err());
        assert!(record_on(&conn, "lib", 0, 1000).is_err());
    }
    #[test]
    fn deleted_files_cannot_receive_new_visits_but_restoration_retains_counts() {
        let conn = Connection::open_in_memory().unwrap();
        fixture(&conn);
        record_on(&conn, "lib", 1, 1000).unwrap();
        conn.execute_batch("INSERT INTO media_states VALUES(1,0,123);")
            .unwrap();
        assert!(record_on(&conn, "lib", 1, 2000).is_err());
        conn.execute_batch("UPDATE media_states SET deleted_at=NULL,hidden=1;")
            .unwrap();
        assert_eq!(record_on(&conn, "lib", 1, 2000).unwrap().view_count, 2);
    }
    #[test]
    fn concurrent_windows_do_not_lose_increments() {
        let path =
            std::env::temp_dir().join(format!("lap-view-test-{}.sqlite", uuid::Uuid::new_v4()));
        let conn = Connection::open(&path).unwrap();
        fixture(&conn);
        drop(conn);
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let conn = Connection::open(path).unwrap();
                    conn.busy_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                    for _ in 0..20 {
                        record_on(&conn, "lib", 1, 1000).unwrap();
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let conn = Connection::open(&path).unwrap();
        assert_eq!(
            conn.query_row("SELECT view_count FROM afiles WHERE id=1", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            40
        );
        drop(conn);
        std::fs::remove_file(path).unwrap();
    }
}
