//! User-authored catalogue titles. Never rename or read the original media.
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const MAX_TITLE_CHARS: usize = 256;

pub(crate) fn normalize(value: Option<&str>) -> Result<Option<String>, String> {
    let text = value.unwrap_or_default().trim();
    if text.chars().any(char::is_control) {
        return Err("TITLE_INVALID_CHARACTERS".into());
    }
    if text.chars().count() > MAX_TITLE_CHARS {
        return Err("TITLE_TOO_LONG".into());
    }
    Ok((!text.is_empty()).then(|| text.to_owned()))
}

pub(crate) fn set_on(
    conn: &Connection,
    file_id: i64,
    title: Option<&str>,
    expected_title: Option<&str>,
) -> Result<Option<String>, String> {
    if file_id <= 0 {
        return Err("TITLE_FILE_NOT_FOUND".into());
    }
    let title = normalize(title)?;
    let sort_key = title
        .as_ref()
        .map(|text| crate::t_utils::natural_sort_key(&text.to_lowercase()));
    let changed = conn
        .execute(
            "UPDATE afiles SET title=?1, title_sort_key=?2 WHERE id=?3 AND title IS ?4",
            params![title, sort_key, file_id, expected_title],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM afiles WHERE id=?1)",
                [file_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        return Err(if exists {
            "TITLE_CONFLICT"
        } else {
            "TITLE_FILE_NOT_FOUND"
        }
        .into());
    }
    Ok(title)
}

#[derive(Clone, Serialize)]
pub struct TitleChange {
    pub library_id: String,
    pub file_id: i64,
    pub title: Option<String>,
}

#[tauri::command]
pub fn get_media_title(library_id: String, file_id: i64) -> Result<TitleChange, String> {
    crate::t_cmds::with_current_library(&library_id, || {
        let conn = crate::t_sqlite::open_conn()?;
        let title: Option<Option<String>> = conn
            .query_row("SELECT title FROM afiles WHERE id=?1", [file_id], |row| {
                row.get("title")
            })
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(TitleChange {
            library_id: library_id.clone(),
            file_id,
            title: title.ok_or("TITLE_FILE_NOT_FOUND")?,
        })
    })
}

#[tauri::command]
pub fn set_media_title(
    app: AppHandle,
    library_id: String,
    file_id: i64,
    title: Option<String>,
    expected_title: Option<String>,
) -> Result<TitleChange, String> {
    crate::t_cmds::with_current_library(&library_id, || {
        let conn = crate::t_sqlite::open_conn()?;
        let title = set_on(&conn, file_id, title.as_deref(), expected_title.as_deref())?;
        let change = TitleChange {
            library_id: library_id.clone(),
            file_id,
            title,
        };
        // Persistence has succeeded even if a closing window cannot receive the notification.
        if let Err(error) = app.emit("media-title-changed", &change) {
            eprintln!("Title notification failed: {error}");
        }
        Ok(change)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE afiles(id INTEGER PRIMARY KEY,name TEXT,title TEXT,
            comments TEXT,size INTEGER,modified_at INTEGER,cloud_only INTEGER);
            INSERT INTO afiles VALUES(1,'DSC_0123.JPG',NULL,'Note',100,200,1),
                (2,'second.jpg',NULL,NULL,300,400,0);
            ALTER TABLE afiles ADD COLUMN title_sort_key TEXT;",
        )
        .unwrap();
        conn
    }

    #[test]
    fn validates_unicode_without_filename_restrictions() {
        assert_eq!(
            normalize(Some("  西湖 / 日落: 2026 🌅  "))
                .unwrap()
                .as_deref(),
            Some("西湖 / 日落: 2026 🌅")
        );
        assert_eq!(normalize(Some(" \t ")).unwrap(), None);
        assert!(normalize(Some(&"🌅".repeat(256))).is_ok());
        assert!(normalize(Some(&"图".repeat(257))).is_err());
        assert!(normalize(Some("line\nbreak")).is_err());
        assert!(normalize(Some("hidden\0control")).is_err());
    }

    #[test]
    fn title_edits_preserve_original_identity_and_other_metadata_even_offline() {
        let conn = fixture();
        set_on(&conn, 1, Some("日落"), None).unwrap();
        let values = conn
            .query_row(
                "SELECT name,comments,size,modified_at,cloud_only FROM afiles WHERE id=1",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>("name")?,
                        r.get::<_, String>("comments")?,
                        r.get::<_, i64>("size")?,
                        r.get::<_, i64>("modified_at")?,
                        r.get::<_, i64>("cloud_only")?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(values, ("DSC_0123.JPG".into(), "Note".into(), 100, 200, 1));
        set_on(&conn, 2, Some("日落"), None).unwrap(); // duplicate titles are valid
        assert_eq!(set_on(&conn, 1, Some("  "), Some("日落")).unwrap(), None);
    }

    #[test]
    fn stale_edits_never_overwrite_another_window_and_missing_ids_fail() {
        let conn = fixture();
        set_on(&conn, 1, Some("First"), None).unwrap();
        assert_eq!(
            set_on(&conn, 1, Some("Stale"), None).unwrap_err(),
            "TITLE_CONFLICT"
        );
        assert_eq!(
            set_on(&conn, 99, Some("Missing"), None).unwrap_err(),
            "TITLE_FILE_NOT_FOUND"
        );
        assert_eq!(
            conn.query_row("SELECT title FROM afiles WHERE id=1", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "First"
        );
    }
}
