//! Read-only, local-only access to registered private media. Never trust a UI path.
use crate::{t_media_lifecycle as lifecycle, t_raw_display::RawDisplayOptions};
use base64::{Engine, engine::general_purpose::STANDARD};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::Manager;

#[derive(Debug)]
struct Source {
    path: PathBuf,
    name: String,
    file_type: i64,
    rotate: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateMedia {
    name: String,
    path: String,
    image_data: Option<String>,
    file_type: i64,
    rotate: i32,
    companion_type: Option<i64>,
    warning: Option<String>,
    decode_error: Option<String>,
}

fn resolve(
    conn: &Connection,
    file_id: i64,
    mode: &str,
    group_id: Option<&str>,
) -> Result<Source, String> {
    let state: Option<(bool, Option<i64>, Option<String>)> = conn
        .query_row(
            "SELECT hidden,deleted_at,trash_group FROM media_states WHERE file_id=?1",
            [file_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let (hidden, deleted, group) = state.ok_or("Media is no longer in this private view")?;
    let (path, name) = match mode {
        "hidden" if hidden && deleted.is_none() && group_id.is_none() => {
            let (folder, name, root): (String,String,String) = conn.query_row(
                "SELECT f.path,a.name,c.path FROM afiles a JOIN afolders f ON f.id=a.folder_id JOIN albums c ON c.id=f.album_id WHERE a.id=?1", [file_id],
                |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
            ).map_err(|e|e.to_string())?;
            if Path::new(&name).file_name().and_then(|n| n.to_str()) != Some(name.as_str()) {
                return Err("Invalid registered media name".into());
            }
            let root = Path::new(&root)
                .canonicalize()
                .map_err(|e| format!("Album disk unavailable: {e}"))?;
            let path = Path::new(&folder)
                .join(&name)
                .canonicalize()
                .map_err(|e| format!("Original media unavailable: {e}"))?;
            if !path.starts_with(root) || !path.is_file() || lifecycle::is_recovery_path(&path) {
                return Err("Original media is outside its registered album".into());
            }
            (path, name)
        }
        "deleted" if deleted.is_some() && group_id.is_some() && group.as_deref() == group_id => {
            let id = group_id.ok_or("Recovery group required")?;
            let item = lifecycle::load(conn, id)?;
            // Interrupted, mismatched or partially purged groups must be reviewed, not guessed.
            if item.status != "trashed" {
                return Err(format!(
                    "Recovery group requires review ({}). No original was read",
                    item.status
                ));
            }
            lifecycle::validate_root(&item.manifest, id)?;
            let member = item
                .manifest
                .members
                .iter()
                .find(|m| m.file_id == Some(file_id))
                .ok_or("Media is not registered in this recovery group")?;
            let path = Path::new(&member.stored_path)
                .canonicalize()
                .map_err(|e| format!("Recovery media unavailable: {e}"))?;
            if path != Path::new(&member.stored_path) {
                return Err("Recovery media path was redirected".into());
            }
            lifecycle::verify(&path, member)?;
            (path, member.original_name.clone())
        }
        _ => return Err("Media is no longer in the requested private view".into()),
    };
    let file_type = crate::t_utils::get_file_type(&name).ok_or("Unsupported media format")?;
    let rotate = conn
        .query_row(
            "SELECT COALESCE(rotate,0) FROM afiles WHERE id=?1",
            [file_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(Source {
        path,
        name,
        file_type,
        rotate,
    })
}

// An explicit allowlisted decoder path: no HTTP client, online HEIC fallback, AI or indexing.
fn local_image(source: &Source, options: RawDisplayOptions) -> Result<Option<Vec<u8>>, String> {
    let path = source
        .path
        .to_str()
        .ok_or("Media path is not valid Unicode")?;
    if source.file_type == 2 {
        return Ok(None);
    }
    let decoded = if source.file_type == 3 {
        crate::t_image::get_raw_preview_image(path, options)?
    } else if crate::t_image::is_heic_path(path) {
        crate::t_heif::get_heif_preview(path, 1, 4096)?
    } else if crate::t_jxl::is_jxl_path(path) {
        crate::t_jxl::get_jxl_preview_image(path, 4096)?
    } else if crate::t_image::is_ffmpeg_backed_image_path(path) {
        crate::t_video::get_video_thumbnail_sync(path, 4096, None, None)?
    } else if crate::t_libraw::is_tiff_path(path) {
        let img = image::open(path).map_err(|e| format!("Local TIFF decode failed: {e}"))?;
        Some(crate::t_image::resize_dynamic_image_to_jpeg(
            img,
            crate::t_image::get_image_orientation(path),
            4096,
        )?)
    } else {
        // Browser reads the validated original, retaining full resolution/animation and EXIF orientation.
        return Ok(None);
    };
    decoded.map(Some).ok_or_else(||"Local decoder unavailable or format unsupported. No media was uploaded; install the local decoder/FFmpeg or use the cached thumbnail".into())
}

fn read_on(
    conn: &Connection,
    file_id: i64,
    mode: &str,
    group_id: Option<&str>,
    companion: bool,
    raw_options: RawDisplayOptions,
) -> Result<PrivateMedia, String> {
    let primary = resolve(conn, file_id, mode, group_id)?;
    if mode == "deleted"
        && lifecycle::load(conn, group_id.ok_or("Recovery group required")?)?
            .manifest
            .primary_id
            != file_id
    {
        return Err("Only a registered primary item can be opened".into());
    }
    let pair_id: Option<i64> = conn.query_row(
        "SELECT live_photo_video_id FROM afiles WHERE id=?1 AND media_subtype IN ('live_photo','raw_jpeg_pair')", [file_id], |r|r.get(0),
    ).optional().map_err(|e|e.to_string())?.flatten();
    let pair = pair_id.map(|id| resolve(conn, id, mode, group_id));
    let warning = pair.as_ref().and_then(|p| p.as_ref().err()).cloned();
    let companion_type = pair
        .as_ref()
        .and_then(|p| p.as_ref().ok())
        .map(|p| p.file_type);
    let source = if companion {
        pair.ok_or("No registered companion media")??
    } else {
        primary
    };
    let (data, decode_error) = match local_image(&source, raw_options) {
        Ok(bytes) => (bytes, None),
        Err(error) => (None, Some(error)),
    };
    Ok(PrivateMedia {
        name: source.name,
        path: source.path.to_string_lossy().into_owned(),
        image_data: data.map(|bytes| {
            let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                "png"
            } else {
                "jpeg"
            };
            format!("data:image/{mime};base64,{}", STANDARD.encode(bytes))
        }),
        file_type: source.file_type,
        rotate: source.rotate,
        companion_type,
        warning,
        decode_error,
    })
}

#[tauri::command]
pub async fn read_private_media(
    app: tauri::AppHandle,
    library_id: String,
    file_id: i64,
    mode: String,
    group_id: Option<String>,
    companion: bool,
    raw_options: Option<RawDisplayOptions>,
) -> Result<PrivateMedia, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        let conn = crate::t_sqlite::open_conn()?;
        let media = read_on(
            &conn,
            file_id,
            &mode,
            group_id.as_deref(),
            companion,
            raw_options.unwrap_or_default(),
        )?;
        // Explicit exact-file grant also covers dot-prefixed recovery directories on Unix.
        if media.decode_error.is_none() && media.image_data.is_none() {
            app.state::<tauri::Scopes>()
                .allow_file(&media.path)
                .map_err(|e| format!("Private media asset access failed: {e}"))?;
        }
        Ok(media)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::t_media_lifecycle::{Manifest, Member};
    struct Fixture {
        conn: Connection,
        base: PathBuf,
        root: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/private-media-fixtures");
            std::fs::create_dir_all(&base).unwrap();
            let root = base.join(format!("test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&root).unwrap();
            let root = root.canonicalize().unwrap();
            let conn = Connection::open_in_memory().unwrap();
            conn.execute_batch("CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT);
                CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,path TEXT);
                CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,name TEXT,rotate INTEGER,media_subtype TEXT,live_photo_video_id INTEGER);
                INSERT INTO albums VALUES(1,''); INSERT INTO afolders VALUES(1,1,'');
                INSERT INTO afiles VALUES(1,1,'photo.jpg',0,'live_photo',2),(2,1,'video.mov',0,NULL,NULL);").unwrap();
            lifecycle::setup(&conn).unwrap();
            conn.execute("UPDATE albums SET path=?1", [root.to_str().unwrap()])
                .unwrap();
            conn.execute("UPDATE afolders SET path=?1", [root.to_str().unwrap()])
                .unwrap();
            image::RgbImage::new(20, 10)
                .save(root.join("photo.jpg"))
                .unwrap();
            std::fs::write(root.join("video.mov"), b"private video").unwrap();
            Self { conn, base, root }
        }
        fn hidden(&self) {
            self.conn
                .execute_batch("INSERT INTO media_states(file_id,hidden) VALUES(1,1),(2,1)")
                .unwrap();
        }
        fn trashed(&self) -> String {
            let id = uuid::Uuid::new_v4().to_string();
            let dir = self.root.join(".lap-trash").join(&id);
            std::fs::create_dir_all(&dir).unwrap();
            let mut members = Vec::new();
            for (index, (file_id, name)) in [(1, "photo.jpg"), (2, "video.mov")].iter().enumerate()
            {
                let original = self.root.join(name);
                let stored = dir.join(format!("{index}-{name}"));
                let (size, hash) = lifecycle::fingerprint(&original).unwrap();
                std::fs::rename(&original, &stored).unwrap();
                members.push(Member {
                    file_id: Some(*file_id),
                    original_name: name.to_string(),
                    original_path: original.to_string_lossy().into_owned(),
                    stored_path: stored.to_string_lossy().into_owned(),
                    size,
                    hash,
                });
                self.conn.execute("INSERT INTO media_states(file_id,hidden,deleted_at,trash_group) VALUES(?1,1,1,?2)",rusqlite::params![file_id,id]).unwrap();
                self.conn
                    .execute(
                        "UPDATE afiles SET name=?1 WHERE id=?2",
                        rusqlite::params![format!(".lap-deleted-{id}-{file_id}"), file_id],
                    )
                    .unwrap();
            }
            let manifest = Manifest {
                root: self.root.to_string_lossy().into_owned(),
                primary_id: 1,
                members,
            };
            self.conn.execute("INSERT INTO media_trash_groups(id,created_at,status,manifest) VALUES(?1,1,'trashed',?2)",rusqlite::params![id,serde_json::to_string(&manifest).unwrap()]).unwrap();
            id
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let (Ok(base), Ok(root)) = (self.base.canonicalize(), self.root.canonicalize()) {
                if root.parent() == Some(base.as_path())
                    && root
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("test-"))
                {
                    let _ = std::fs::remove_dir_all(root);
                }
            }
        }
    }
    #[test]
    fn hidden_read_is_scoped_and_does_not_mutate_media_or_visibility() {
        let f = Fixture::new();
        f.hidden();
        let before = std::fs::read(f.root.join("photo.jpg")).unwrap();
        let source = resolve(&f.conn, 1, "hidden", None).unwrap();
        assert_eq!(source.path, f.root.join("photo.jpg"));
        assert_eq!(source.file_type, 1);
        assert!(
            local_image(&source, RawDisplayOptions::default())
                .unwrap()
                .is_none()
        );
        assert_eq!(std::fs::read(&source.path).unwrap(), before);
        assert!(resolve(&f.conn, 1, "deleted", None).is_err());
        assert!(resolve(&f.conn, 1, "hidden", Some("fake")).is_err());
        f.conn
            .execute("UPDATE media_states SET hidden=0 WHERE file_id=1", [])
            .unwrap();
        assert!(resolve(&f.conn, 1, "hidden", None).is_err());
        assert!(resolve(&f.conn, 999, "hidden", None).is_err());
    }
    #[test]
    fn deleted_resolves_registered_storage_not_placeholder_or_replacement() {
        let f = Fixture::new();
        let id = f.trashed();
        std::fs::write(f.root.join("photo.jpg"), b"replacement must never be read").unwrap();
        let source = resolve(&f.conn, 1, "deleted", Some(&id)).unwrap();
        assert_eq!(
            source.path,
            f.root.join(".lap-trash").join(&id).join("0-photo.jpg")
        );
        assert_eq!(source.name, "photo.jpg");
        assert_eq!(
            resolve(&f.conn, 2, "deleted", Some(&id)).unwrap().file_type,
            2
        );
        assert!(resolve(&f.conn, 1, "hidden", None).is_err());
        assert!(
            resolve(
                &f.conn,
                1,
                "deleted",
                Some(&uuid::Uuid::new_v4().to_string())
            )
            .is_err()
        );
        assert!(resolve(&f.conn, 1, "deleted", None).is_err());
        std::fs::write(&source.path, b"corrupt").unwrap();
        assert!(
            resolve(&f.conn, 1, "deleted", Some(&id))
                .unwrap_err()
                .contains("changed")
        );
    }
    #[test]
    fn incomplete_recovery_is_not_guessed_or_fallen_back_to_original() {
        let f = Fixture::new();
        let id = f.trashed();
        for status in ["moving", "review_required", "purging", "restoring"] {
            f.conn
                .execute(
                    "UPDATE media_trash_groups SET status=?1 WHERE id=?2",
                    rusqlite::params![status, id],
                )
                .unwrap();
            assert!(
                resolve(&f.conn, 1, "deleted", Some(&id))
                    .unwrap_err()
                    .contains("review")
            );
        }
        f.conn
            .execute(
                "UPDATE media_trash_groups SET status='trashed' WHERE id=?1",
                [&id],
            )
            .unwrap();
        let source = resolve(&f.conn, 1, "deleted", Some(&id)).unwrap();
        std::fs::remove_file(source.path).unwrap();
        assert!(
            resolve(&f.conn, 1, "deleted", Some(&id))
                .unwrap_err()
                .contains("unavailable")
        );
    }
    #[test]
    fn hidden_path_escape_and_unregistered_recovery_members_are_rejected() {
        let f = Fixture::new();
        f.hidden();
        f.conn
            .execute("UPDATE afiles SET name='../outside.jpg' WHERE id=1", [])
            .unwrap();
        assert!(resolve(&f.conn, 1, "hidden", None).is_err());
        let f = Fixture::new();
        let id = f.trashed();
        let mut item = lifecycle::load(&f.conn, &id).unwrap();
        item.manifest.members.remove(1);
        f.conn
            .execute(
                "UPDATE media_trash_groups SET manifest=?1 WHERE id=?2",
                rusqlite::params![serde_json::to_string(&item.manifest).unwrap(), id],
            )
            .unwrap();
        assert!(resolve(&f.conn, 2, "deleted", Some(&id)).is_err());
    }
    #[test]
    fn local_tiff_decodes_without_writes_and_invalid_raw_reports_local_failure() {
        let f = Fixture::new();
        let path = f.root.join("photo.tiff");
        image::RgbImage::new(20, 10).save(&path).unwrap();
        let before = std::fs::read(&path).unwrap();
        let source = Source {
            path: path.clone(),
            name: "photo.tiff".into(),
            file_type: 1,
            rotate: 0,
        };
        let bytes = local_image(&source, RawDisplayOptions::default())
            .unwrap()
            .unwrap();
        assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 20);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let path = f.root.join("invalid.dng");
        std::fs::write(&path, b"not a RAW file").unwrap();
        let source = Source {
            path,
            name: "invalid.dng".into(),
            file_type: 3,
            rotate: 0,
        };
        assert!(local_image(&source, RawDisplayOptions::default()).is_err());
    }
    #[test]
    fn companion_reads_are_registered_and_private_and_secondary_cannot_impersonate_primary() {
        let f = Fixture::new();
        let id = f.trashed();
        let primary = read_on(
            &f.conn,
            1,
            "deleted",
            Some(&id),
            false,
            RawDisplayOptions::default(),
        )
        .unwrap();
        assert_eq!(primary.companion_type, Some(2));
        let video = read_on(
            &f.conn,
            1,
            "deleted",
            Some(&id),
            true,
            RawDisplayOptions::default(),
        )
        .unwrap();
        assert_eq!(video.file_type, 2);
        assert!(video.path.ends_with("1-video.mov"));
        assert!(
            read_on(
                &f.conn,
                2,
                "deleted",
                Some(&id),
                false,
                RawDisplayOptions::default()
            )
            .unwrap_err()
            .contains("primary")
        );
        f.conn
            .execute(
                "UPDATE media_states SET trash_group='other' WHERE file_id=2",
                [],
            )
            .unwrap();
        let primary = read_on(
            &f.conn,
            1,
            "deleted",
            Some(&id),
            false,
            RawDisplayOptions::default(),
        )
        .unwrap();
        assert!(primary.companion_type.is_none());
        assert!(primary.warning.is_some());
        assert!(
            read_on(
                &f.conn,
                1,
                "deleted",
                Some(&id),
                true,
                RawDisplayOptions::default()
            )
            .is_err()
        );
    }
    #[test]
    fn decode_errors_preserve_raw_identity_for_policy_switch_and_do_not_change_state() {
        let f = Fixture::new();
        f.hidden();
        std::fs::write(f.root.join("invalid.dng"), b"unsupported RAW").unwrap();
        f.conn.execute("UPDATE afiles SET name='invalid.dng',media_subtype=NULL,live_photo_video_id=NULL WHERE id=1",[]).unwrap();
        for mode in [
            crate::t_raw_display::RawPreviewMode::Embedded,
            crate::t_raw_display::RawPreviewMode::Rendered,
        ] {
            let result = read_on(
                &f.conn,
                1,
                "hidden",
                None,
                false,
                RawDisplayOptions {
                    mode,
                    ..RawDisplayOptions::default()
                },
            )
            .unwrap();
            assert_eq!(result.file_type, 3);
            assert!(result.decode_error.is_some());
            assert!(result.image_data.is_none());
        }
        let hidden: bool = f
            .conn
            .query_row("SELECT hidden FROM media_states WHERE file_id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(hidden);
        assert_eq!(
            std::fs::read(f.root.join("invalid.dng")).unwrap(),
            b"unsupported RAW"
        );
    }
}
