//! Scoped, cancellable rebuilding of derived image thumbnails. Never modifies original media.
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{
    Arc, LazyLock, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Scope {
    Files { file_ids: Vec<i64> },
    Folder { folder_id: i64, recursive: bool },
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub library_id: String,
    pub job_id: String,
    pub scope: Scope,
    pub thumbnail_size: u32,
    pub raw_display_options: crate::t_raw_display::RawDisplayOptions,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub library_id: String,
    pub job_id: String,
    pub total: usize,
    pub completed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped: usize,
    pub cancelled: bool,
    pub finished: bool,
    pub errors: Vec<String>,
}
struct Active {
    id: String,
    cancel: Arc<AtomicBool>,
}
#[derive(Default)]
struct Jobs {
    active: HashMap<String, Active>,
    early_cancel: HashSet<(String, String)>,
}
static ACTIVE: LazyLock<Mutex<Jobs>> = LazyLock::new(|| Mutex::new(Jobs::default()));
struct JobGuard {
    library: String,
    id: String,
}
impl Drop for JobGuard {
    fn drop(&mut self) {
        if let Ok(mut jobs) = ACTIVE.lock() {
            if jobs
                .active
                .get(&self.library)
                .is_some_and(|job| job.id == self.id)
            {
                jobs.active.remove(&self.library);
            }
        }
    }
}
pub fn cancel(library: &str, id: &str) -> Result<(), String> {
    if id.len() > 64 {
        return Err("Invalid job ID".into());
    }
    let mut jobs = ACTIVE.lock().map_err(|e| e.to_string())?;
    if let Some(job) = jobs.active.get(library).filter(|job| job.id == id) {
        job.cancel.store(true, Ordering::Release);
    } else {
        if jobs.early_cancel.len() >= 256 {
            if let Some(key) = jobs.early_cancel.iter().next().cloned() {
                jobs.early_cancel.remove(&key);
            }
        }
        jobs.early_cancel
            .insert((library.to_string(), id.to_string()));
    }
    Ok(())
}
pub(crate) fn image_ids(conn: &Connection, scope: &Scope) -> Result<Vec<i64>, String> {
    match scope {
        Scope::Files { file_ids } => {
            if file_ids.is_empty() || file_ids.len() > 10000 || file_ids.iter().any(|id| *id <= 0) {
                return Err(
                    "Select between 1 and 10000 images; empty selections never rebuild a library"
                        .into(),
                );
            }
            let ids = file_ids
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let mut found = Vec::new();
            for chunk in ids.chunks(400) {
                let placeholders = vec!["?"; chunk.len()].join(",");
                let mut statement=conn.prepare(&format!("SELECT a.id FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE a.id IN ({placeholders}) AND a.file_type IN (1,3) ORDER BY a.id")).map_err(|e|e.to_string())?;
                found.extend(
                    statement
                        .query_map(params_from_iter(chunk), |row| row.get::<_, i64>(0))
                        .map_err(|e| e.to_string())?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| e.to_string())?,
                );
            }
            if found.len() != ids.len() {
                return Err(
                    "Selection contains missing or non-image files; no thumbnails were rebuilt"
                        .into(),
                );
            }
            Ok(found)
        }
        Scope::Folder {
            folder_id,
            recursive,
        } => {
            if *folder_id <= 0 {
                return Err("Invalid folder ID".into());
            }
            let (album, path): (i64, String) = conn
                .query_row(
                    "SELECT album_id,path FROM afolders WHERE id=?1",
                    [folder_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or("Folder no longer exists")?;
            let prefix = format!(
                "{}{}",
                path.trim_end_matches(['\\', '/']),
                std::path::MAIN_SEPARATOR
            );
            let mut statement=conn.prepare("SELECT a.id FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE a.file_type IN (1,3) AND b.album_id=?1 AND (b.id=?2 OR (?3 AND instr(b.path,?4)=1)) ORDER BY a.id").map_err(|e|e.to_string())?;
            statement
                .query_map(params![album, folder_id, recursive, prefix], |row| {
                    row.get(0)
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())
        }
    }
}
pub fn run(request: Request, app: tauri::AppHandle) -> Result<Progress, String> {
    use tauri::Emitter;
    if !(16..=4096).contains(&request.thumbnail_size)
        || request.job_id.is_empty()
        || request.job_id.len() > 64
        || !request
            .job_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Invalid thumbnail job options".into());
    }
    let token = Arc::new(AtomicBool::new(false));
    {
        let mut jobs = ACTIVE.lock().map_err(|e| e.to_string())?;
        if jobs
            .early_cancel
            .remove(&(request.library_id.clone(), request.job_id.clone()))
        {
            return Ok(Progress {
                library_id: request.library_id,
                job_id: request.job_id,
                cancelled: true,
                finished: true,
                ..Progress::default()
            });
        }

        if jobs.active.contains_key(&request.library_id) {
            return Err("A thumbnail regeneration job is already running for this library".into());
        }
        jobs.active.insert(
            request.library_id.clone(),
            Active {
                id: request.job_id.clone(),
                cancel: token.clone(),
            },
        );
    }
    let _job = JobGuard {
        library: request.library_id.clone(),
        id: request.job_id.clone(),
    };
    let ids = {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != request.library_id {
            return Err("Library changed; no thumbnails were rebuilt".into());
        }
        let conn = crate::t_sqlite::open_conn()?;
        image_ids(&conn, &request.scope)?
    };
    let mut progress = Progress {
        library_id: request.library_id.clone(),
        job_id: request.job_id.clone(),
        total: ids.len(),
        ..Progress::default()
    };
    let _ = app.emit("thumbnail-regeneration-progress", &progress);
    for id in ids {
        if token.load(Ordering::Acquire) {
            progress.cancelled = true;
            break;
        }
        // Release between images: switching libraries cancels remaining work without writing into another DB.
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != request.library_id {
            progress.cancelled = true;
            break;
        }
        let result = crate::t_sqlite::AThumb::rebuild_image_thumbnail(
            id,
            request.thumbnail_size,
            request.raw_display_options,
        );
        match result {
            Ok(true) => {
                progress.succeeded += 1;
                if let Ok(Some(file)) = crate::t_sqlite::AFile::get_file_info(id) {
                    let _=app.emit("thumbnail_ready",serde_json::json!({"library_id":request.library_id,"album_id":file.album_id,"file_ids":[id],"invalidate":true,"thumbnail_only":true}));
                }
            }
            Ok(false) => progress.skipped += 1,
            Err(error) => {
                progress.failed += 1;
                if progress.errors.len() < 5 {
                    progress.errors.push(format!("Image #{id}: {error}"));
                }
            }
        }
        progress.completed += 1;
        let _ = app.emit("thumbnail-regeneration-progress", &progress);
    }
    progress.finished = true;
    let _ = app.emit("thumbnail-regeneration-progress", &progress);
    Ok(progress)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        let sep = std::path::MAIN_SEPARATOR;
        c.execute_batch("CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,path TEXT);CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,file_type INTEGER);INSERT INTO afiles VALUES(1,1,1),(2,2,3),(3,3,1),(4,4,1),(5,1,2);").unwrap();
        for (id, album, path) in [
            (1, 1, format!("photos{sep}target")),
            (2, 1, format!("photos{sep}target{sep}child")),
            (3, 1, format!("photos{sep}target-extra")),
            (4, 2, format!("photos{sep}target{sep}other-album")),
        ] {
            c.execute(
                "INSERT INTO afolders VALUES(?1,?2,?3)",
                params![id, album, path],
            )
            .unwrap();
        }
        c
    }
    #[test]
    fn invalid_empty_mixed_or_missing_selections_do_not_expand_scope() {
        let c = db();
        for ids in [vec![], vec![0], vec![1, 5], vec![1, 999]] {
            assert!(image_ids(&c, &Scope::Files { file_ids: ids }).is_err());
        }
    }
    #[test]
    fn file_selections_are_deduplicated_and_keep_non_targets_untouched() {
        let c = db();
        assert_eq!(
            image_ids(
                &c,
                &Scope::Files {
                    file_ids: vec![2, 1, 1]
                }
            )
            .unwrap(),
            vec![1, 2]
        );
    }
    #[test]
    fn recursive_folder_scope_respects_path_boundaries_and_album() {
        let c = db();
        assert_eq!(
            image_ids(
                &c,
                &Scope::Folder {
                    folder_id: 1,
                    recursive: true
                }
            )
            .unwrap(),
            vec![1, 2]
        );
        assert_eq!(
            image_ids(
                &c,
                &Scope::Folder {
                    folder_id: 1,
                    recursive: false
                }
            )
            .unwrap(),
            vec![1]
        );
        assert!(
            image_ids(
                &c,
                &Scope::Folder {
                    folder_id: 999,
                    recursive: true
                }
            )
            .is_err()
        );
    }
    #[test]
    #[ignore = "Requires explicit LAP_REBUILD_LIVE_FILE_ID and LAP_REBUILD_EXPECTED_FILENAME; rebuilds one user-authorized thumbnail"]
    fn explicitly_authorized_live_heic_rebuild_preserves_original_and_annotations() {
        use crate::t_sqlite::{AFile, AThumb};
        let id: i64 = std::env::var("LAP_REBUILD_LIVE_FILE_ID")
            .expect("explicit file ID")
            .parse()
            .unwrap();
        let expected = std::env::var("LAP_REBUILD_EXPECTED_FILENAME").expect("filename fence");
        let file = AFile::get_file_info(id).unwrap().unwrap();
        assert_eq!(file.name, expected);
        let path = file.file_path.unwrap();
        let original = blake3::hash(&std::fs::read(&path).unwrap());
        let before = {
            let conn = crate::t_sqlite::open_conn().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM face_annotations WHERE file_id=?1",
                [id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        };
        assert!(
            AThumb::rebuild_image_thumbnail(
                id,
                512,
                crate::t_raw_display::RawDisplayOptions::default()
            )
            .unwrap()
        );
        let thumbnail = AThumb::fetch(id).unwrap().unwrap();
        assert_eq!(thumbnail.error_code, 0);
        assert!(thumbnail.thumb_key.unwrap().starts_with("h2"));
        let bytes = thumbnail.thumb_data.unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgb8();
        let reference = image::open(std::env::var("LAP_HEIC_REFERENCE_IMAGE").unwrap())
            .unwrap()
            .resize_exact(
                decoded.width(),
                decoded.height(),
                image::imageops::FilterType::Triangle,
            )
            .to_rgb8();
        let error = decoded
            .as_raw()
            .iter()
            .zip(reference.as_raw())
            .map(|(a, b)| (*a as f64 - *b as f64).abs())
            .sum::<f64>()
            / decoded.as_raw().len() as f64;
        assert!(
            error < 10.,
            "Live cache differs from independent decode: {error}"
        );
        assert_eq!(original, blake3::hash(&std::fs::read(path).unwrap()));
        let conn = crate::t_sqlite::open_conn().unwrap();
        assert_eq!(
            before,
            conn.query_row(
                "SELECT COUNT(*) FROM face_annotations WHERE file_id=?1",
                [id],
                |row| row.get::<_, i64>(0)
            )
            .unwrap()
        );
    }
}
