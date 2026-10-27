use super::protocol;
use crate::t_sqlite::{self, AFile};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Scope {
    Selection { file_ids: Vec<i64> },
    Album { album_id: i64 },
}

pub struct Snapshot {
    pub library: String,
    pub title: String,
    pub file_ids: Vec<i64>,
    ids_json: String,
    sorted_ids: Vec<i64>,
}

impl Snapshot {
    pub fn contains(&self, id: i64) -> bool {
        self.sorted_ids.binary_search(&id).is_ok()
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: i64,
    pub title: String,
    pub video: bool,
    pub mime: &'static str,
    pub size: u64,
    pub width: u64,
    pub height: u64,
    pub duration: u64,
}

pub struct Page {
    pub entries: Vec<Entry>,
    pub total: usize,
}

fn public(bulk: bool) -> String {
    format!(
        "a.file_type IN (1,2,3) AND COALESCE(a.cloud_only,0)=0 AND COALESCE(a.metadata_pending,0)=0 AND {} AND {} AND {}",
        AFile::search_exclusion_condition("b"),
        if bulk {
            AFile::live_photo_companion_exclusion_condition()
        } else {
            AFile::live_photo_companion_point_condition()
        },
        if bulk {
            AFile::album_query_predicate("a")
        } else {
            AFile::album_filter_predicate("a")
        }
    )
}

pub fn is_public_on(conn: &Connection, id: i64) -> Result<bool, String> {
    conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE a.id=?1 AND {})", public(false)), [id], |row| row.get(0)).map_err(|e| e.to_string())
}

pub fn resolve(library: &str, scope: Scope) -> Result<Snapshot, String> {
    crate::t_cmds::with_current_library(library, || {
        resolve_on(&*t_sqlite::open_conn()?, library, scope)
    })
}

pub fn resolve_on(conn: &Connection, library: &str, scope: Scope) -> Result<Snapshot, String> {
    let _snapshot = read_snapshot(conn)?;
    let (title, ids) = match scope {
        Scope::Selection { file_ids } => {
            if file_ids.is_empty() || file_ids.iter().any(|id| *id <= 0) {
                return Err("请选择需要共享的照片或视频".into());
            }
            let mut seen = HashSet::new();
            let ids = file_ids
                .into_iter()
                .filter(|id| seen.insert(*id))
                .collect::<Vec<_>>();
            let json = serde_json::to_string(&ids).map_err(|e| e.to_string())?;
            let sql = format!(
                "SELECT a.id FROM json_each(?1) chosen JOIN afiles a ON a.id=chosen.value JOIN afolders b ON b.id=a.folder_id WHERE {} ORDER BY CAST(chosen.key AS INTEGER)",
                public(true)
            );
            let found = conn
                .prepare(&sql)
                .map_err(|e| e.to_string())?
                .query_map([json], |row| row.get(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<i64>, _>>()
                .map_err(|e| e.to_string())?;
            if found.len() != ids.len() {
                return Err("选中内容包含隐藏、已删除、云端或已排除的媒体，请重新选择".into());
            }
            ("所选内容".to_owned(), found)
        }
        Scope::Album { album_id } => {
            let title = conn
                .query_row("SELECT name FROM albums WHERE id=?1", [album_id], |row| {
                    row.get::<_, String>(0)
                })
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or("相册已不存在")?;
            let sql = format!(
                "SELECT a.id FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE b.album_id=?1 AND {} ORDER BY b.path COLLATE NOCASE,a.name COLLATE NOCASE,a.id",
                public(true)
            );
            let ids = conn
                .prepare(&sql)
                .map_err(|e| e.to_string())?
                .query_map([album_id], |row| row.get(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<i64>, _>>()
                .map_err(|e| e.to_string())?;
            (title, ids)
        }
    };
    if ids.is_empty() {
        return Err("此范围内没有可共享的本地照片或视频".into());
    }
    let ids_json = serde_json::to_string(&ids).map_err(|e| e.to_string())?;
    let mut sorted_ids = ids.clone();
    sorted_ids.sort_unstable();
    Ok(Snapshot {
        library: library.into(),
        title,
        file_ids: ids,
        ids_json,
        sorted_ids,
    })
}

pub fn page(
    snapshot: &Snapshot,
    start: u32,
    count: u32,
    sort: &str,
    item: Option<i64>,
) -> Result<Page, String> {
    crate::t_cmds::with_current_library(&snapshot.library, || {
        page_on(&*t_sqlite::open_conn()?, snapshot, start, count, sort, item)
    })
}

const COLUMNS: &str = "a.id,COALESCE(NULLIF(a.title,''),a.name),a.name,a.file_type,COALESCE(a.size,0),COALESCE(a.width,0),COALESCE(a.height,0),COALESCE(a.duration,0)";

fn order(sort: &str) -> Result<&'static str, String> {
    match sort {
        "" => Ok("CAST(chosen.key AS INTEGER)"),
        "+dc:title" | "dc:title" => Ok("COALESCE(NULLIF(a.title,''),a.name) COLLATE NOCASE,a.id"),
        "-dc:title" => Ok("COALESCE(NULLIF(a.title,''),a.name) COLLATE NOCASE DESC,a.id DESC"),
        _ => Err("Unsupported sort criteria".into()),
    }
}

fn row_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    let name: String = row.get(2)?;
    let video = row.get::<_, i64>(3)? == 2;
    Ok(Entry {
        id: row.get(0)?,
        title: row.get(1)?,
        video,
        mime: if video {
            protocol::video_mime(std::path::Path::new(&name)).unwrap_or("application/octet-stream")
        } else {
            "image/jpeg"
        },
        size: row.get::<_, i64>(4)?.max(0) as u64,
        width: row.get::<_, i64>(5)?.max(0) as u64,
        height: row.get::<_, i64>(6)?.max(0) as u64,
        duration: row.get::<_, i64>(7)?.max(0) as u64,
    })
}

fn query_from() -> String {
    format!(
        "FROM json_each(?1) chosen JOIN afiles a ON a.id=chosen.value JOIN afolders b ON b.id=a.folder_id WHERE {}",
        public(true)
    )
}

fn read_snapshot(conn: &Connection) -> Result<Option<rusqlite::Transaction<'_>>, String> {
    if conn.is_autocommit() {
        conn.unchecked_transaction()
            .map(Some)
            .map_err(|e| e.to_string())
    } else {
        Ok(None)
    }
}

pub fn page_on(
    conn: &Connection,
    snapshot: &Snapshot,
    start: u32,
    count: u32,
    sort: &str,
    item: Option<i64>,
) -> Result<Page, String> {
    let _snapshot = read_snapshot(conn)?;
    if item.is_some_and(|id| !snapshot.contains(id)) {
        return Err("No such object".into());
    }
    let from = query_from();
    let total = conn
        .query_row(
            &format!("SELECT COUNT(*) {from}"),
            [&snapshot.ids_json],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())? as usize;
    let sql = format!(
        "SELECT {COLUMNS} {from} AND (?2 IS NULL OR a.id=?2) ORDER BY {} LIMIT ?3 OFFSET ?4",
        order(sort)?
    );
    let limit = if count == 0 { -1 } else { i64::from(count) };
    let entries = conn
        .prepare(&sql)
        .map_err(|e| e.to_string())?
        .query_map(params![snapshot.ids_json, item, limit, start], row_entry)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(Page { entries, total })
}

pub fn stream_page(
    snapshot: &Snapshot,
    start: u32,
    count: u32,
    sort: &str,
    emit: &mut dyn FnMut(Entry) -> Result<(), String>,
) -> Result<(usize, usize), String> {
    crate::t_cmds::with_current_library(&snapshot.library, || {
        stream_page_on(&*t_sqlite::open_conn()?, snapshot, start, count, sort, emit)
    })
}

pub fn stream_page_on(
    conn: &Connection,
    snapshot: &Snapshot,
    start: u32,
    count: u32,
    sort: &str,
    emit: &mut dyn FnMut(Entry) -> Result<(), String>,
) -> Result<(usize, usize), String> {
    let _snapshot = read_snapshot(conn)?;
    let from = query_from();
    let total = conn
        .query_row(
            &format!("SELECT COUNT(*) {from}"),
            [&snapshot.ids_json],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())? as usize;
    let sql = format!(
        "SELECT {COLUMNS} {from} ORDER BY {} LIMIT ?2 OFFSET ?3",
        order(sort)?
    );
    let limit = if count == 0 { -1 } else { i64::from(count) };
    let mut statement = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(params![snapshot.ids_json, limit, start], row_entry)
        .map_err(|e| e.to_string())?;
    let mut returned = 0;
    for row in rows {
        emit(row.map_err(|e| e.to_string())?)?;
        returned += 1;
    }
    Ok((returned, total))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub fn database(count: i64) -> Connection {
        let conn = crate::ai::media::tests::db(count);
        conn.execute_batch("ALTER TABLE albums ADD COLUMN name TEXT DEFAULT '旅行相册'; ALTER TABLE afiles ADD COLUMN title TEXT; ALTER TABLE afiles ADD COLUMN duration INTEGER; UPDATE afiles SET live_photo_video_id=NULL;").unwrap();
        conn
    }

    #[test]
    fn whole_album_includes_nested_folders_and_excludes_other_albums_and_private_media() {
        let conn = database(10);
        conn.execute_batch("INSERT INTO afolders VALUES(2,1,'/photos/nested',0); UPDATE afiles SET folder_id=2 WHERE id=2; INSERT INTO albums VALUES(2,'/other',7,0,'[]','其他相册'); INSERT INTO afolders VALUES(3,2,'/other',0); UPDATE afiles SET folder_id=3 WHERE id=3; INSERT INTO media_states VALUES(4,1,NULL),(5,0,100); UPDATE afiles SET cloud_only=1 WHERE id=6; UPDATE afiles SET metadata_pending=1 WHERE id=7;").unwrap();
        let request: Scope =
            serde_json::from_value(serde_json::json!({"kind":"album","albumId":1})).unwrap();
        let snapshot = resolve_on(&conn, "test", request).unwrap();
        assert_eq!(snapshot.title, "旅行相册");
        assert!(snapshot.contains(1) && snapshot.contains(2));
        for id in [3, 4, 5, 6, 7] {
            assert!(!snapshot.contains(id), "{id}");
        }
        assert!(!is_public_on(&conn, 4).unwrap());
        assert!(
            resolve_on(
                &conn,
                "test",
                Scope::Selection {
                    file_ids: vec![1, 4]
                }
            )
            .is_err()
        );
    }

    #[test]
    fn browse_pages_preserve_selection_order_and_recheck_hidden_items_without_expanding_the_scope()
    {
        let conn = database(1000);
        let request: Scope =
            serde_json::from_value(serde_json::json!({"kind":"selection","fileIds":[9,2,7,9]}))
                .unwrap();
        let snapshot = resolve_on(&conn, "test", request).unwrap();
        assert_eq!(snapshot.file_ids, vec![9, 2, 7]);
        assert_eq!(
            page_on(&conn, &snapshot, 1, 1, "", None).unwrap().entries[0].id,
            2
        );
        assert!(page_on(&conn, &snapshot, 0, 1, "", Some(10)).is_err());
        conn.execute("INSERT INTO media_states VALUES(2,1,NULL)", [])
            .unwrap();
        let page = page_on(&conn, &snapshot, 0, 0, "", None).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(
            page.entries
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            vec![9, 7]
        );
        let album = resolve_on(&conn, "test", Scope::Album { album_id: 1 }).unwrap();
        let page = page_on(&conn, &album, 0, 0, "", None).unwrap();
        assert_eq!(page.total, 999);
        assert_eq!(page.entries.len(), 999);
        assert_eq!(
            page_on(&conn, &album, 200, 200, "", None)
                .unwrap()
                .entries
                .len(),
            200
        );
    }
}
