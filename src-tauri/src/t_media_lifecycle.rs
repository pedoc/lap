//! Photo visibility and recoverable file deletion. No automatic expiry or original-file overwrite.
use crate::t_sqlite::{self, AFile};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::Emitter;

static OPERATIONS: Mutex<()> = Mutex::new(());
pub fn is_recovery_path(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(".lap-trash")
    })
}
pub const STATE_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS media_states(file_id INTEGER PRIMARY KEY,hidden INTEGER NOT NULL DEFAULT 0,deleted_at INTEGER,trash_group TEXT,original_name TEXT,visibility_group TEXT,FOREIGN KEY(file_id) REFERENCES afiles(id) ON DELETE CASCADE); CREATE INDEX IF NOT EXISTS media_states_deleted ON media_states(deleted_at);";
pub fn setup(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(STATE_SCHEMA)
        .map_err(|e| e.to_string())?;
    let group_exists = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(media_states)")
            .map_err(|e| e.to_string())?;
        let cols = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        cols.iter().any(|c| c == "visibility_group")
    };
    if !group_exists {
        conn.execute(
            "ALTER TABLE media_states ADD COLUMN visibility_group TEXT",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    conn.execute_batch("CREATE TABLE IF NOT EXISTS media_trash_groups(id TEXT PRIMARY KEY,created_at INTEGER NOT NULL,status TEXT NOT NULL,manifest TEXT NOT NULL,error TEXT); CREATE INDEX IF NOT EXISTS media_states_group ON media_states(trash_group);").map_err(|e|e.to_string())
}
pub fn normal_predicate(alias: &str) -> String {
    format!(
        "NOT EXISTS (SELECT 1 FROM media_states ms WHERE ms.file_id={alias}.id AND (ms.hidden=1 OR ms.deleted_at IS NOT NULL))"
    )
}
pub fn retained_predicate(alias: &str) -> String {
    format!(
        "NOT EXISTS (SELECT 1 FROM media_states ms WHERE ms.file_id={alias}.id AND ms.deleted_at IS NOT NULL)"
    )
}
pub fn path_reserved(conn: &Connection, folder: i64, name: &str) -> Result<bool, String> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states ms JOIN afiles a ON a.id=ms.file_id JOIN media_trash_groups g ON g.id=ms.trash_group WHERE a.folder_id=?1 AND ms.original_name=?2 AND g.status!='trashed')",params![folder,name],|r|r.get(0)).map_err(|e|e.to_string())
}
pub fn folder_has_private(conn: &Connection, path: &str) -> Result<bool, String> {
    let mut stmt=conn.prepare("SELECT f.path FROM media_states ms JOIN afiles a ON a.id=ms.file_id JOIN afolders f ON f.id=a.folder_id WHERE ms.hidden=1 OR ms.deleted_at IS NOT NULL").map_err(|e|e.to_string())?;
    for row in stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
    {
        if Path::new(&row.map_err(|e| e.to_string())?).starts_with(path) {
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn folder_has_deleted(conn: &Connection, path: &str) -> Result<bool, String> {
    if is_recovery_path(Path::new(path)) {
        return Ok(true);
    }
    let mut stmt=conn.prepare("SELECT f.path FROM media_states ms JOIN afiles a ON a.id=ms.file_id JOIN afolders f ON f.id=a.folder_id WHERE ms.deleted_at IS NOT NULL").map_err(|e|e.to_string())?;
    let paths = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    for folder in paths {
        if Path::new(&folder.map_err(|e| e.to_string())?).starts_with(path) {
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn has_deleted_in_album(conn: &Connection, album: i64) -> Result<bool, String> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states ms JOIN afiles a ON a.id=ms.file_id JOIN afolders f ON f.id=a.folder_id WHERE f.album_id=?1 AND ms.deleted_at IS NOT NULL)",[album],|r|r.get(0)).map_err(|e|e.to_string())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub file_id: Option<i64>,
    pub original_name: String,
    pub original_path: String,
    pub stored_path: String,
    pub size: u64,
    pub hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub root: String,
    pub primary_id: i64,
    pub members: Vec<Member>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashItem {
    pub id: String,
    pub created_at: i64,
    pub status: String,
    pub error: Option<String>,
    pub manifest: Manifest,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub media: std::collections::HashMap<String, i64>,
    pub hidden: i64,
    pub deleted: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashPage {
    pub items: Vec<TrashItem>,
    pub total: i64,
    pub recovery: crate::t_media_recovery::Report,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiddenPage {
    pub files: Vec<AFile>,
    pub total: i64,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub succeeded: Vec<String>,
    pub errors: Vec<(String, String)>,
}

pub(crate) fn fingerprint(path: &Path) -> Result<(u64, String), String> {
    let meta =
        fs::symlink_metadata(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(format!(
            "Only ordinary files are supported: {}",
            path.display()
        ));
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut size = 0;
    loop {
        let count = file.read(&mut buf).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        size += count as u64;
        hash.update(&buf[..count]);
    }
    if size != meta.len() {
        return Err(format!("File changed while reading: {}", path.display()));
    }
    Ok((size, format!("{:x}", hash.finalize())))
}
pub(crate) fn verify(path: &Path, member: &Member) -> Result<(), String> {
    let (size, hash) = fingerprint(path)?;
    if size != member.size || hash != member.hash {
        return Err(format!(
            "File content changed; operation stopped: {}",
            path.display()
        ));
    }
    Ok(())
}
fn no_replace_move(source: &Path, target: &Path, member: &Member) -> Result<(), String> {
    if target.try_exists().map_err(|e| e.to_string())? {
        return Err(format!(
            "Destination already exists; nothing was overwritten: {}",
            target.display()
        ));
    }
    verify(source, member)?;
    // A hard link is efficient on NTFS/APFS/ext4 and atomically refuses an occupied destination.
    // Filesystems without links fall back to a create_new copy, never to an overwriting rename.
    let linked = fs::hard_link(source, target).is_ok();
    if !linked {
        let mut input = fs::File::open(source).map_err(|e| e.to_string())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .map_err(|e| format!("Cannot reserve destination {}: {e}", target.display()))?;
        let source_time = input
            .metadata()
            .map_err(|e| e.to_string())?
            .modified()
            .map_err(|e| e.to_string())?;
        let copy = (|| {
            std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
            output
                .set_times(fs::FileTimes::new().set_modified(source_time))
                .map_err(|e| e.to_string())?;
            output.flush().map_err(|e| e.to_string())?;
            output.sync_all().map_err(|e| e.to_string())?;
            verify(target, member)?;
            verify(source, member)
        })();
        if let Err(error) = copy {
            drop(output);
            let _ = fs::remove_file(target);
            return Err(error);
        }
    }
    verify(target, member)?;
    // Source removal is the final step, after durable verified storage exists.
    let resolved = source.canonicalize().map_err(|e| e.to_string())?;
    if resolved != source {
        return Err("Source was redirected while moving; safe copy retained".into());
    }
    fs::remove_file(&resolved).map_err(|e| {
        format!("Copy is safe in the recovery area, but original removal failed: {e}")
    })?;
    Ok(())
}
// Plan paths only after batch deduplication. Existing recovery records are never renumbered.
fn assign_storage_paths(manifest: &mut Manifest, id: &str) {
    let directory = Path::new(&manifest.root).join(".lap-trash").join(id);
    for (index, member) in manifest.members.iter_mut().enumerate() {
        member.stored_path = directory
            .join(format!("{index}-{}", member.original_name))
            .to_string_lossy()
            .into_owned();
    }
}
pub(crate) fn validate_root(manifest: &Manifest, id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "Invalid recovery group ID")?;
    let root = Path::new(&manifest.root)
        .canonicalize()
        .map_err(|e| format!("Original disk or album is unavailable: {e}"))?;
    if root != PathBuf::from(&manifest.root) {
        return Err("Album root changed; recovery stopped".into());
    }
    let dir = root.join(".lap-trash").join(id);
    // Validate existing parents against the recorded canonical root, including junction/symlink escapes.
    for parent in [root.join(".lap-trash"), dir.clone()] {
        if parent.try_exists().map_err(|e| e.to_string())?
            && parent.canonicalize().map_err(|e| e.to_string())? != parent
        {
            return Err("Recovery directory is redirected; operation stopped".into());
        }
    }
    for (index, member) in manifest.members.iter().enumerate() {
        let expected = dir.join(format!("{index}-{}", member.original_name));
        if PathBuf::from(&member.stored_path) != expected {
            return Err("Invalid registered recovery path".into());
        }
        let original = Path::new(&member.original_path);
        if !original.starts_with(&root)
            || original.file_name().and_then(|v| v.to_str()) != Some(member.original_name.as_str())
        {
            return Err("Original path is outside its recorded album".into());
        }
        if let Some(parent) = original.parent() {
            let mut existing = parent;
            while !existing.exists() {
                existing = existing.parent().ok_or("Original parent is unavailable")?;
            }
            if !existing
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(&root)
            {
                return Err("Original parent is redirected outside the album".into());
            }
        }
    }
    Ok(dir)
}
pub(crate) fn load(conn: &Connection, id: &str) -> Result<TrashItem, String> {
    conn.query_row(
        "SELECT id,created_at,status,error,manifest FROM media_trash_groups WHERE id=?1",
        [id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
            ))
        },
    )
    .map_err(|e| e.to_string())
    .and_then(|(id, created_at, status, error, json)| {
        Ok(TrashItem {
            id,
            created_at,
            status,
            error,
            manifest: serde_json::from_str(&json).map_err(|e| e.to_string())?,
        })
    })
}
fn set_error(conn: &Connection, id: &str, error: &str) {
    let _ = conn.execute(
        "UPDATE media_trash_groups SET error=?1 WHERE id=?2",
        params![error, id],
    );
    let _ = crate::t_media_recovery::checkpoint_current(conn, id);
}
fn component_ids(primary: i64) -> Result<Vec<i64>, String> {
    let conn = t_sqlite::open_conn()?;
    let mut ids = vec![primary];
    let owners = {
        let mut stmt = conn
            .prepare("SELECT id FROM afiles WHERE live_photo_video_id=?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([primary], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    for owner in owners.into_iter().chain(std::iter::once(primary)) {
        if !ids.contains(&owner) {
            ids.push(owner);
        }
        for file in AFile::live_photo_component_files(owner)? {
            if let Some(id) = file.id {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    let related = {
        let mut stmt=conn.prepare("SELECT file_id FROM media_states WHERE visibility_group IN (SELECT visibility_group FROM media_states WHERE file_id=?1)").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map([primary], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    for id in related {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}
pub fn hide(ids: Vec<i64>, hidden: bool) -> Result<(), String> {
    let _guard = OPERATIONS.lock().map_err(|e| e.to_string())?;
    let conn = t_sqlite::open_conn()?;
    let mut groups = std::collections::HashMap::new();
    for id in ids {
        if groups.contains_key(&id) {
            continue;
        }
        let group = uuid::Uuid::new_v4().to_string();
        for member in component_ids(id)? {
            let deleted:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND deleted_at IS NOT NULL)",[member],|r|r.get(0)).map_err(|e|e.to_string())?;
            if deleted {
                return Err("Restore deleted photos before changing visibility".into());
            }
            if AFile::get_file_info(member)?.is_none() {
                return Err("Photo no longer exists in this library".into());
            }
            groups.insert(member, group.clone());
        }
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    if hidden {
        invalidate_covers(&tx, &groups.keys().copied().collect::<Vec<_>>())?;
    }
    for (id, group) in groups {
        tx.execute("INSERT INTO media_states(file_id,hidden,visibility_group) VALUES(?1,?2,?3) ON CONFLICT(file_id) DO UPDATE SET hidden=excluded.hidden,visibility_group=excluded.visibility_group",params![id,hidden,group]).map_err(|e|e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
fn prepare_manifest(conn: &Connection, primary: i64, id: &str) -> Result<Manifest, String> {
    let root:String=conn.query_row("SELECT c.path FROM afiles a JOIN afolders b ON b.id=a.folder_id JOIN albums c ON c.id=b.album_id WHERE a.id=?1",[primary],|r|r.get(0)).map_err(|e|e.to_string())?;
    let root = Path::new(&root).canonicalize().map_err(|e| e.to_string())?;
    let mut members = Vec::new();
    let mut seen = HashSet::new();
    for file_id in component_ids(primary)? {
        let deleted:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND deleted_at IS NOT NULL)",[file_id],|r|r.get(0)).map_err(|e|e.to_string())?;
        if deleted {
            return Err("Photo is already in Recently Deleted".into());
        }
        let file = AFile::get_file_info(file_id)?.ok_or("Photo no longer exists")?;
        let original = file.file_path.ok_or("Original file path is unavailable")?;
        let lexical = Path::new(&original);
        if fs::symlink_metadata(lexical)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err(
                "Symbolic-link photos cannot be moved into the managed recovery area".into(),
            );
        }
        let original = lexical.canonicalize().map_err(|e| e.to_string())?;
        if !original.starts_with(&root) {
            return Err("Photo is outside its registered album".into());
        }
        let mut targets = vec![(Some(file_id), original.clone())];
        targets.extend(
            crate::t_apple_sidecar::apple_aae_sidecar_paths(&original.to_string_lossy())
                .into_iter()
                .map(|p| (None, p)),
        );
        for (file_id, path) in targets {
            if fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
            {
                return Err("Recovery does not follow symbolic-link sidecars".into());
            }
            let path = path.canonicalize().map_err(|e| e.to_string())?;
            if !path.starts_with(&root) {
                return Err("Sidecar is outside its album".into());
            }
            if !seen.insert(path.clone()) {
                continue;
            }
            let original_name = path
                .file_name()
                .and_then(|v| v.to_str())
                .ok_or("Invalid filename")?
                .to_string();
            let (size, hash) = fingerprint(&path)?;
            let stored = root.join(".lap-trash").join(id).join(format!(
                "{}-{}",
                members.len(),
                original_name
            ));
            members.push(Member {
                file_id,
                original_name,
                original_path: path.to_string_lossy().into_owned(),
                stored_path: stored.to_string_lossy().into_owned(),
                size,
                hash,
            });
        }
    }
    Ok(Manifest {
        root: root.to_string_lossy().into_owned(),
        primary_id: primary,
        members,
    })
}
fn invalidate_covers(conn: &Connection, ids: &[i64]) -> Result<(), String> {
    for id in ids {
        conn.execute(
            "UPDATE albums SET cover_file_id=NULL WHERE cover_file_id=?1",
            [id],
        )
        .map_err(|e| e.to_string())?;
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='persons')",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists {
            conn.execute("UPDATE persons SET thumbnail=NULL WHERE id IN (SELECT person_id FROM faces WHERE file_id=?1)",[id]).map_err(|e|e.to_string())?;
            conn.execute("UPDATE persons SET thumbnail=NULL,cover_face_id=NULL WHERE cover_face_id IN (SELECT id FROM faces WHERE file_id=?1)",[id]).map_err(|e|e.to_string())?;
        }
    }
    Ok(())
}
fn register_intent(conn: &Connection, id: &str, manifest: &Manifest) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let time = chrono::Utc::now().timestamp();
    tx.execute(
        "INSERT INTO media_trash_groups VALUES(?1,?2,'moving',?3,NULL)",
        params![
            id,
            time,
            serde_json::to_string(manifest).map_err(|e| e.to_string())?
        ],
    )
    .map_err(|e| e.to_string())?;
    for member in &manifest.members {
        if let Some(file_id) = member.file_id {
            tx.execute("INSERT INTO media_states(file_id,deleted_at,trash_group,original_name) VALUES(?1,?2,?3,?4) ON CONFLICT(file_id) DO UPDATE SET deleted_at=excluded.deleted_at,trash_group=excluded.trash_group,original_name=excluded.original_name",params![file_id,time,id,member.original_name]).map_err(|e|e.to_string())?;
            tx.execute(
                "UPDATE afiles SET name=?1 WHERE id=?2",
                params![format!(".lap-deleted-{id}-{file_id}"), file_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    invalidate_covers(
        &tx,
        &manifest
            .members
            .iter()
            .filter_map(|m| m.file_id)
            .collect::<Vec<_>>(),
    )?;
    tx.commit().map_err(|e| e.to_string())?;
    crate::t_media_recovery::checkpoint_current(conn, id)
}
pub fn validate_original(id: i64, path: &str, allow_hidden: bool) -> Result<(), String> {
    let conn = t_sqlite::open_conn()?;
    let blocked:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND (hidden=1 OR deleted_at IS NOT NULL))",[id],|r|r.get(0)).map_err(|e|e.to_string())?;
    let deleted: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND deleted_at IS NOT NULL)",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if (blocked && !allow_hidden) || deleted || is_recovery_path(Path::new(path)) {
        return Err(
            "Use Hidden/Recently Deleted actions for this photo; its original was not deleted"
                .into(),
        );
    }
    let file = AFile::get_file_info(id)?.ok_or("Photo no longer exists in this library")?;
    let registered = PathBuf::from(file.file_path.ok_or("Original path is missing")?)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if Path::new(path).canonicalize().map_err(|e| e.to_string())? != registered {
        return Err(
            "Frontend file path is stale or belongs to another photo/library; deletion stopped"
                .into(),
        );
    }
    Ok(())
}
pub fn move_to_recently_deleted(
    files: Vec<crate::t_cmds::BatchDeleteFile>,
    expected_library: Option<&str>,
    allow_hidden: bool,
) -> Result<crate::t_cmds::BatchDeleteResult, String> {
    let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
        .write()
        .map_err(|e| e.to_string())?;
    if let Some(library) = expected_library {
        if crate::t_config::current_library_id()? != library {
            return Err("Library changed; deletion was not performed".into());
        }
    }
    let _guard = OPERATIONS.lock().map_err(|e| e.to_string())?;
    let conn = t_sqlite::open_conn()?;
    for file in &files {
        validate_original(file.file_id, &file.file_path, allow_hidden)?;
    }
    let mut plans = Vec::new();
    let mut seen = HashSet::new();
    let mut paths = HashSet::new();
    // Preflight all selected originals and associated files before any filesystem mutation.
    for primary in files.iter().map(|f| f.file_id) {
        if seen.contains(&primary) {
            continue;
        }
        let id = uuid::Uuid::new_v4().to_string();
        let mut manifest = prepare_manifest(&conn, primary, &id)?;
        manifest
            .members
            .retain(|m| paths.insert(m.original_path.clone()));
        assign_storage_paths(&mut manifest, &id);
        for m in &manifest.members {
            if let Some(file_id) = m.file_id {
                seen.insert(file_id);
            }
        }
        plans.push((id, manifest));
    }
    let mut result = crate::t_cmds::BatchDeleteResult {
        failed_count: 0,
        deleted_file_ids: Vec::new(),
        trash_failed_file_ids: Vec::new(),
    };
    for (id, manifest) in plans {
        let prepared = (|| {
            let dir = validate_root(&manifest, &id)?;
            fs::create_dir_all(&dir).map_err(|e| {
                format!("Cannot create recovery directory; original files are unchanged: {e}")
            })?;
            validate_root(&manifest, &id)?;
            register_intent(&conn, &id, &manifest)
        })();
        if let Err(error) = prepared {
            eprintln!("Recently Deleted: {error}");
            result.failed_count += 1;
            result.trash_failed_file_ids.push(manifest.primary_id);
            continue;
        }
        result
            .deleted_file_ids
            .extend(manifest.members.iter().filter_map(|m| m.file_id));
        let moved = (|| {
            for member in &manifest.members {
                no_replace_move(
                    Path::new(&member.original_path),
                    Path::new(&member.stored_path),
                    member,
                )?;
            }
            conn.execute(
                "UPDATE media_trash_groups SET status='trashed',error=NULL WHERE id=?1",
                [&id],
            )
            .map_err(|e| e.to_string())?;
            crate::t_media_recovery::checkpoint_current(&conn, &id)?;
            Ok::<_, String>(())
        })();
        if let Err(error) = moved {
            set_error(&conn, &id, &error);
            result.failed_count += 1;
        }
    }
    Ok(result)
}
fn original_catalog_conflict(conn: &Connection, member: &Member) -> Result<bool, String> {
    let Some(id) = member.file_id else {
        return Ok(false);
    };
    let comparison = if cfg!(windows) {
        "a.name=?2 COLLATE NOCASE"
    } else {
        "a.name=?2"
    };
    conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM afiles a JOIN afiles old ON old.folder_id=a.folder_id WHERE old.id=?1 AND {comparison} AND a.id!=?1)"),params![id,member.original_name],|r|r.get(0)).map_err(|e|e.to_string())
}
fn restore_on(conn: &Connection, item: &TrashItem) -> Result<(), String> {
    let dir = validate_root(&item.manifest, &item.id)?;
    if item.status == "review_required" {
        return Err(
            "Recovery metadata differs from this database; automatic file operations are disabled"
                .into(),
        );
    }
    if item.status == "purging" {
        return Err("Permanent deletion already started; remaining files cannot be treated as a complete photo group".into());
    }
    for member in &item.manifest.members {
        if original_catalog_conflict(conn, member)? {
            return Err(format!(
                "Original location has another catalogued photo; move it away before restoring: {}",
                member.original_path
            ));
        }
        let original = Path::new(&member.original_path);
        let stored = Path::new(&member.stored_path);
        if original.exists() {
            verify(original, member)?;
        } else {
            verify(stored, member)?;
        }
    }
    conn.execute(
        "UPDATE media_trash_groups SET status='restoring',error=NULL WHERE id=?1",
        [&item.id],
    )
    .map_err(|e| e.to_string())?;
    crate::t_media_recovery::checkpoint_current(conn, &item.id)?;
    for member in &item.manifest.members {
        let original = Path::new(&member.original_path);
        let stored = Path::new(&member.stored_path);
        if original.exists() {
            verify(original, member)?;
            if stored.exists() {
                verify(stored, member)?;
                if !stored.starts_with(&dir) {
                    return Err("Invalid recovery path".into());
                }
                validate_root(&item.manifest, &item.id)?;
                let resolved = stored.canonicalize().map_err(|e| e.to_string())?;
                if !resolved.starts_with(&dir) {
                    return Err("Recovery file escaped its registered directory".into());
                }
                fs::remove_file(resolved).map_err(|e| e.to_string())?;
            }
        } else {
            fs::create_dir_all(original.parent().ok_or("Original parent missing")?)
                .map_err(|e| e.to_string())?;
            validate_root(&item.manifest, &item.id)?;
            no_replace_move(stored, original, member)?;
        }
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for member in &item.manifest.members {
        if let Some(id) = member.file_id {
            let changed = tx
                .execute(
                    "UPDATE afiles SET name=?1,last_scan_time=?2 WHERE id=?3",
                    params![
                        member.original_name,
                        chrono::Utc::now().timestamp_millis(),
                        id
                    ],
                )
                .map_err(|e| e.to_string())?;
            if changed != 1 {
                return Err("Original catalog record is missing; files were recovered but metadata needs attention".into());
            }
            tx.execute("UPDATE media_states SET deleted_at=NULL,trash_group=NULL,original_name=NULL WHERE file_id=?1",[id]).map_err(|e|e.to_string())?;
        }
    }
    tx.execute("DELETE FROM media_trash_groups WHERE id=?1", [&item.id])
        .map_err(|e| e.to_string())?;
    crate::t_media_recovery::checkpoint(&tx, item, Some("restored"))?;
    tx.commit().map_err(|e| e.to_string())
}
fn purge_on(conn: &Connection, item: &TrashItem) -> Result<(), String> {
    let dir = validate_root(&item.manifest, &item.id)?;
    if !matches!(item.status.as_str(), "trashed" | "purging") {
        return Err(
            "This move/recovery is incomplete. Restore it before permanent deletion.".into(),
        );
    }
    for member in &item.manifest.members {
        let stored = Path::new(&member.stored_path);
        if stored.exists() {
            verify(stored, member)?;
        } else if item.status != "purging" {
            return Err("A registered recovery file is missing. Nothing else was deleted.".into());
        }
    }
    conn.execute(
        "UPDATE media_trash_groups SET status='purging',error=NULL WHERE id=?1",
        [&item.id],
    )
    .map_err(|e| e.to_string())?;
    crate::t_media_recovery::checkpoint_current(conn, &item.id)?;
    for member in &item.manifest.members {
        let stored = Path::new(&member.stored_path);
        if stored.exists() {
            if !stored.starts_with(&dir) {
                return Err("Invalid recovery target".into());
            }
            verify(stored, member)?;
            validate_root(&item.manifest, &item.id)?;
            let resolved = stored.canonicalize().map_err(|e| e.to_string())?;
            if !resolved.starts_with(&dir) {
                return Err("Recovery file escaped its registered directory".into());
            }
            fs::remove_file(resolved).map_err(|e| e.to_string())?;
        }
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for member in &item.manifest.members {
        if let Some(id) = member.file_id {
            tx.execute("DELETE FROM afiles WHERE id=?1", [id])
                .map_err(|e| e.to_string())?;
        }
    }
    tx.execute("DELETE FROM media_trash_groups WHERE id=?1", [&item.id])
        .map_err(|e| e.to_string())?;
    crate::t_media_recovery::checkpoint(&tx, item, Some("purged"))?;
    tx.commit().map_err(|e| e.to_string())
}
pub fn operate(ids: Vec<String>, purge: bool) -> Result<OperationResult, String> {
    let _guard = OPERATIONS.lock().map_err(|e| e.to_string())?;
    let conn = t_sqlite::open_conn()?;
    let mut result = OperationResult::default();
    for id in ids.into_iter().collect::<HashSet<_>>() {
        let operation = load(&conn, &id).and_then(|item| {
            if purge {
                purge_on(&conn, &item)
            } else {
                restore_on(&conn, &item)
            }
        });
        match operation {
            Ok(()) => result.succeeded.push(id),
            Err(error) => {
                set_error(&conn, &id, &error);
                result.errors.push((id, error));
            }
        }
    }
    Ok(result)
}
pub fn overview() -> Result<Overview, String> {
    let conn = t_sqlite::open_conn()?;
    let mut media = std::collections::HashMap::new();
    for (kind, condition) in [
        ("image", "a.file_type IN (1,3)"),
        ("video", "a.file_type=2"),
        ("raw", "a.file_type=3"),
        ("live", "a.media_subtype IN ('live_photo','motion_photo')"),
        ("raw_pair", "a.media_subtype='raw_jpeg_pair'"),
        ("gif", "LOWER(a.name) LIKE '%.gif'"),
    ] {
        let count=conn.query_row(&format!("SELECT COUNT(*) FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE {condition} AND {} AND {}{}",AFile::search_exclusion_condition("b"),AFile::live_photo_companion_exclusion_condition(),AFile::album_filter_sql("a")),[],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?;
        media.insert(kind.to_string(), count);
    }
    let hidden=conn.query_row("SELECT COUNT(*) FROM media_states WHERE hidden=1 AND deleted_at IS NULL AND file_id NOT IN (SELECT live_photo_video_id FROM afiles WHERE live_photo_video_id IS NOT NULL)",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    let deleted = conn
        .query_row("SELECT COUNT(*) FROM media_trash_groups", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(Overview {
        media,
        hidden,
        deleted,
    })
}
pub fn hidden_page(offset: i64, limit: i64) -> Result<HiddenPage, String> {
    let conn = t_sqlite::open_conn()?;
    let total=conn.query_row("SELECT COUNT(*) FROM media_states WHERE hidden=1 AND deleted_at IS NULL AND file_id NOT IN (SELECT live_photo_video_id FROM afiles WHERE live_photo_video_id IS NOT NULL)",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    let ids = {
        let mut stmt=conn.prepare("SELECT file_id FROM media_states WHERE hidden=1 AND deleted_at IS NULL AND file_id NOT IN (SELECT live_photo_video_id FROM afiles WHERE live_photo_video_id IS NOT NULL) ORDER BY file_id DESC LIMIT ?1 OFFSET ?2").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map(params![limit.clamp(1, 100), offset.max(0)], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    Ok(HiddenPage {
        files: AFile::get_files_by_ids_including_private(&ids)?,
        total,
    })
}
pub fn trash_page(offset: i64, limit: i64) -> Result<TrashPage, String> {
    let conn = t_sqlite::open_conn()?;
    let recovery = crate::t_media_recovery::inspect(&conn, true)?;
    let total = conn
        .query_row("SELECT COUNT(*) FROM media_trash_groups", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let ids = {
        let mut stmt = conn
            .prepare(
                "SELECT id FROM media_trash_groups ORDER BY created_at DESC,id LIMIT ?1 OFFSET ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit.clamp(1, 100), offset.max(0)], |r| {
                r.get::<_, String>(0)
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    Ok(TrashPage {
        recovery,
        items: ids
            .iter()
            .map(|id| load(&conn, id))
            .collect::<Result<Vec<_>, _>>()?,
        total,
    })
}
fn event(app: &tauri::AppHandle, library: &str) {
    let _ = app.emit(
        "media-lifecycle-changed",
        serde_json::json!({"library_id":library}),
    );
    let _=app.emit("face-person-changed",serde_json::json!({"library_id":library,"mode":"visibility_changed","membershipChanged":true}));
}
#[tauri::command]
pub async fn get_media_overview(library_id: String) -> Result<Overview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        overview()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn get_hidden_media(
    library_id: String,
    offset: i64,
    limit: i64,
) -> Result<HiddenPage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        hidden_page(offset, limit)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn get_deleted_media(
    library_id: String,
    offset: i64,
    limit: i64,
) -> Result<TrashPage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        trash_page(offset, limit)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn set_media_hidden(
    app: tauri::AppHandle,
    library_id: String,
    file_ids: Vec<i64>,
    hidden: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .write()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        let mut affected = Vec::new();
        for id in &file_ids {
            affected.extend(component_ids(*id)?);
        }
        hide(file_ids, hidden)?;
        let _ = app.emit(
            "media-lifecycle-changed",
            serde_json::json!({"library_id":library_id,"file_ids":affected}),
        );
        if hidden {
            let _ = app.emit(
                "files-deleted",
                serde_json::json!({"library_id":library_id,"fileIds":affected,"source":"hidden"}),
            );
        }
        let _=app.emit("face-person-changed",serde_json::json!({"library_id":library_id,"mode":"visibility_changed","membershipChanged":true}));
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn restore_deleted_media(
    app: tauri::AppHandle,
    library_id: String,
    group_ids: Vec<String>,
) -> Result<OperationResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .write()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        let result = operate(group_ids, false)?;
        event(&app, &library_id);
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn purge_deleted_media(
    app: tauri::AppHandle,
    library_id: String,
    group_ids: Vec<String>,
) -> Result<OperationResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .write()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        let result = operate(group_ids, true)?;
        event(&app, &library_id);
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_private_media_preview(
    library_id: String,
    file_id: i64,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move||{let _library=crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read().map_err(|e|e.to_string())?;if crate::t_config::current_library_id()?!=library_id{return Err("Library changed".into());}let conn=t_sqlite::open_conn()?;
    let private:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND (hidden=1 OR deleted_at IS NOT NULL))",[file_id],|r|r.get(0)).map_err(|e|e.to_string())?;
    if !private{return Err("Photo is no longer in this private view".into());}
    Ok(crate::t_sqlite::AThumb::fetch(file_id)?.and_then(|t|t.thumb_data_base64).map(|bytes|format!("data:image/{};base64,{}",if bytes.starts_with("iVBORw0KGgo"){"png"}else{"jpeg"},bytes)))
    }).await.map_err(|e|e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        root: PathBuf,
        base: PathBuf,
        conn: Connection,
    }
    impl Fixture {
        fn new() -> Self {
            let base =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("target/media-lifecycle-fixtures");
            fs::create_dir_all(&base).unwrap();
            let root = base.join(format!("test-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            let root = root.canonicalize().unwrap();
            let conn = Connection::open_in_memory().unwrap();
            conn.execute_batch("PRAGMA foreign_keys=ON;
             CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT,cover_file_id INTEGER);
             CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,path TEXT);
             CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,name TEXT,last_scan_time INTEGER,rating INTEGER,comments TEXT,UNIQUE(folder_id,name));
             CREATE TABLE athumbs(file_id INTEGER PRIMARY KEY,thumb_data BLOB,FOREIGN KEY(file_id) REFERENCES afiles(id) ON DELETE CASCADE);
             CREATE TABLE persons(id INTEGER PRIMARY KEY,name TEXT,thumbnail BLOB,cover_face_id INTEGER);
             CREATE TABLE faces(id INTEGER PRIMARY KEY,file_id INTEGER,person_id INTEGER,FOREIGN KEY(file_id) REFERENCES afiles(id) ON DELETE CASCADE);
             CREATE TABLE afile_tags(file_id INTEGER,tag_id INTEGER,FOREIGN KEY(file_id) REFERENCES afiles(id) ON DELETE CASCADE);
             CREATE TABLE manual_annotations(file_id INTEGER,name TEXT,FOREIGN KEY(file_id) REFERENCES afiles(id) ON DELETE CASCADE);
             INSERT INTO albums VALUES(1,'',1);INSERT INTO afolders VALUES(1,1,'');
             INSERT INTO afiles VALUES(1,1,'photo.jpg',0,5,'keep my comment'),(2,1,'video.mov',0,3,'video');
             INSERT INTO athumbs VALUES(1,X'010203');INSERT INTO persons VALUES(9,'Alice',X'010203',7);INSERT INTO faces VALUES(7,1,9);
             INSERT INTO afile_tags VALUES(1,42);INSERT INTO manual_annotations VALUES(1,'Alice');").unwrap();
            setup(&conn).unwrap();
            conn.execute(
                "UPDATE albums SET path=?1",
                [root.to_string_lossy().as_ref()],
            )
            .unwrap();
            conn.execute(
                "UPDATE afolders SET path=?1",
                [root.to_string_lossy().as_ref()],
            )
            .unwrap();
            fs::write(root.join("photo.jpg"), b"original photo").unwrap();
            fs::write(root.join("video.mov"), b"paired video").unwrap();
            fs::write(root.join("photo.AAE"), b"editing sidecar").unwrap();
            Self { root, base, conn }
        }
        fn plan(&self) -> (String, Manifest) {
            let id = uuid::Uuid::new_v4().to_string();
            let mut members = Vec::new();
            for (file_id, name) in [
                (Some(1), "photo.jpg"),
                (Some(2), "video.mov"),
                (None, "photo.AAE"),
            ] {
                let original = self.root.join(name);
                let (size, hash) = fingerprint(&original).unwrap();
                members.push(Member {
                    file_id,
                    original_name: name.into(),
                    original_path: original.to_string_lossy().into_owned(),
                    stored_path: self
                        .root
                        .join(".lap-trash")
                        .join(&id)
                        .join(format!("{}-{name}", members.len()))
                        .to_string_lossy()
                        .into_owned(),
                    size,
                    hash,
                });
            }
            (
                id,
                Manifest {
                    root: self.root.to_string_lossy().into_owned(),
                    primary_id: 1,
                    members,
                },
            )
        }
        fn stage(&self, id: &str, manifest: &Manifest) {
            fs::create_dir_all(validate_root(manifest, id).unwrap()).unwrap();
            register_intent(&self.conn, id, manifest).unwrap();
        }
        fn trash(&self, id: &str, manifest: &Manifest) {
            self.stage(id, manifest);
            for member in &manifest.members {
                no_replace_move(
                    Path::new(&member.original_path),
                    Path::new(&member.stored_path),
                    member,
                )
                .unwrap();
            }
            self.conn
                .execute(
                    "UPDATE media_trash_groups SET status='trashed' WHERE id=?1",
                    [id],
                )
                .unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Recursive cleanup is restricted to an owned UUID test directory under this workspace's target.
            if let (Ok(base), Ok(root)) = (self.base.canonicalize(), self.root.canonicalize()) {
                if root.parent() == Some(base.as_path())
                    && root
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("test-"))
                {
                    let _ = fs::remove_dir_all(root);
                }
            }
        }
    }
    #[test]
    fn media_lifecycle_shared_middle_sidecar_is_renumbered_before_registration() {
        let f = Fixture::new();
        let (id, mut manifest) = f.plan();
        manifest.members.swap(1, 2);
        assign_storage_paths(&mut manifest, &id);
        let shared = manifest.members[1].original_path.clone();
        let mut seen = HashSet::from([shared]);
        manifest
            .members
            .retain(|member| seen.insert(member.original_path.clone()));
        assert!(validate_root(&manifest, &id).is_err());
        assign_storage_paths(&mut manifest, &id);
        assert!(validate_root(&manifest, &id).is_ok());
        assert!(manifest.members[1].stored_path.ends_with("1-video.mov"));
        f.trash(&id, &manifest);
        restore_on(&f.conn, &load(&f.conn, &id).unwrap()).unwrap();
        assert_eq!(
            fs::read(f.root.join("photo.jpg")).unwrap(),
            b"original photo"
        );
        assert_eq!(fs::read(f.root.join("video.mov")).unwrap(), b"paired video");
        assert_eq!(
            fs::read(f.root.join("photo.AAE")).unwrap(),
            b"editing sidecar"
        );
    }
    #[test]
    fn media_lifecycle_empty_and_unchanged_plans_have_stable_storage_names() {
        let f = Fixture::new();
        let (id, mut manifest) = f.plan();
        let original = manifest
            .members
            .iter()
            .map(|m| m.stored_path.clone())
            .collect::<Vec<_>>();
        assign_storage_paths(&mut manifest, &id);
        assert_eq!(
            original,
            manifest
                .members
                .iter()
                .map(|m| m.stored_path.clone())
                .collect::<Vec<_>>()
        );
        manifest.members.clear();
        assign_storage_paths(&mut manifest, &id);
        assert!(manifest.members.is_empty());
    }

    #[test]
    fn media_recovery_older_database_reattaches_pending_files_without_moving_media() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.trash(&id, &m);
        f.conn.execute_batch("DELETE FROM media_states;DELETE FROM media_trash_groups;UPDATE afiles SET name=CASE id WHEN 1 THEN 'photo.jpg' ELSE 'video.mov' END;").unwrap();
        let report = crate::t_media_recovery::inspect(&f.conn, true).unwrap();
        assert_eq!(report.reattached, 1);
        assert!(!f.root.join("photo.jpg").exists());
        assert!(Path::new(&m.members[0].stored_path).exists());
        restore_on(&f.conn, &load(&f.conn, &id).unwrap()).unwrap();
        assert_eq!(
            fs::read(f.root.join("photo.jpg")).unwrap(),
            b"original photo"
        );
        assert_eq!(
            f.conn
                .query_row("SELECT rating FROM afiles WHERE id=1", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            5
        );
    }
    #[test]
    fn media_recovery_read_only_preflight_does_not_change_catalogue_or_media() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.trash(&id, &m);
        let path = f.root.join("readonly.db");
        f.conn
            .execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])
            .unwrap();
        let readonly =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let report = crate::t_media_recovery::inspect(&readonly, false).unwrap();
        assert_eq!(report.registered, 1);
        assert!(Path::new(&m.members[0].stored_path).exists());
        let exported = crate::t_media_recovery::backup_journals(&readonly).unwrap();
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].0, id);
    }
    #[test]
    fn media_recovery_corrupt_journal_prevents_destructive_scan_cleanup() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.trash(&id, &m);
        let dir = validate_root(&m, &id).unwrap();
        let journal = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|v| v == "json"))
            .unwrap();
        fs::write(journal, b"invalid-json").unwrap();
        let report = crate::t_media_recovery::inspect(&f.conn, true).unwrap();
        assert!(!report.issues.is_empty());
        assert!(crate::t_media_recovery::assert_cleanup_safe(&f.conn, 1).is_err());
        assert!(Path::new(&m.members[0].stored_path).exists());
    }
    #[test]
    fn media_recovery_unrelated_catalogue_ids_are_not_rebound_or_overwritten() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.trash(&id, &m);
        f.conn.execute_batch("DELETE FROM media_states;DELETE FROM media_trash_groups;UPDATE afiles SET name='different-photo.jpg' WHERE id=1;").unwrap();
        let report = crate::t_media_recovery::inspect(&f.conn, true).unwrap();
        assert!(!report.issues.is_empty());
        assert_eq!(load(&f.conn, &id).unwrap().status, "review_required");
        assert_eq!(
            f.conn
                .query_row("SELECT name FROM afiles WHERE id=1", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "different-photo.jpg"
        );
        assert!(restore_on(&f.conn, &load(&f.conn, &id).unwrap()).is_err());
    }
    #[test]
    fn media_recovery_completed_restore_reconciles_stale_database_without_file_io() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.trash(&id, &m);
        restore_on(&f.conn, &load(&f.conn, &id).unwrap()).unwrap();
        register_intent(&f.conn, &id, &m).unwrap();
        let old = load(&f.conn, &id).unwrap();
        crate::t_media_recovery::checkpoint(&f.conn, &old, Some("restored")).unwrap();
        let report = crate::t_media_recovery::inspect(&f.conn, true).unwrap();
        assert_eq!(report.reattached, 1);
        assert!(load(&f.conn, &id).is_err());
        assert_eq!(
            fs::read(f.root.join("photo.jpg")).unwrap(),
            b"original photo"
        );
    }

    #[test]
    fn media_lifecycle_real_query_predicates_hide_private_media_but_scan_preserves_hidden() {
        let f = Fixture::new();
        f.conn.execute_batch("ALTER TABLE albums ADD COLUMN file_types INTEGER DEFAULT 7;ALTER TABLE albums ADD COLUMN small_image_filter INTEGER DEFAULT 0;ALTER TABLE albums ADD COLUMN excluded_folders TEXT DEFAULT '[]';ALTER TABLE afiles ADD COLUMN file_type INTEGER DEFAULT 1;ALTER TABLE afiles ADD COLUMN width INTEGER DEFAULT 100;ALTER TABLE afiles ADD COLUMN height INTEGER DEFAULT 100;").unwrap();
        f.conn
            .execute("INSERT INTO media_states(file_id,hidden) VALUES(1,1)", [])
            .unwrap();
        let count = |p: String| {
            f.conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM afiles a WHERE {p}"),
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
        };
        assert_eq!(count(AFile::album_query_predicate("a")), 1);
        assert_eq!(count(AFile::album_scan_predicate("a")), 2);
        let (id, m) = f.plan();
        f.trash(&id, &m);
        assert_eq!(count(AFile::album_query_predicate("a")), 0);
        assert_eq!(count(AFile::album_scan_predicate("a")), 0);
        restore_on(&f.conn, &load(&f.conn, &id).unwrap()).unwrap();
        assert_eq!(count(AFile::album_query_predicate("a")), 1);
        assert_eq!(count(AFile::album_scan_predicate("a")), 2);
    }

    #[test]
    fn media_lifecycle_restore_uses_scan_clock_units_and_hidden_metadata_is_not_swept() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        let scan_started = chrono::Utc::now().timestamp_millis();
        f.trash(&id, &m);
        restore_on(&f.conn, &load(&f.conn, &id).unwrap()).unwrap();
        let stamp = f
            .conn
            .query_row("SELECT last_scan_time FROM afiles WHERE id=1", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap();
        assert!(stamp >= scan_started);
        f.conn.execute("INSERT INTO media_states(file_id,hidden) VALUES(1,1) ON CONFLICT(file_id) DO UPDATE SET hidden=1",[]).unwrap();
        assert!(folder_has_private(&f.conn, &f.root.to_string_lossy()).unwrap());
        assert!(!folder_has_deleted(&f.conn, &f.root.to_string_lossy()).unwrap());
        f.conn
            .execute(
                &format!(
                    "DELETE FROM afiles AS a WHERE last_scan_time<?1 AND {}",
                    normal_predicate("a")
                ),
                [i64::MAX],
            )
            .unwrap();
        assert_eq!(
            f.conn
                .query_row(
                    "SELECT COUNT(*) FROM manual_annotations WHERE file_id=1",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }

    #[test]
    #[cfg(windows)]
    fn media_lifecycle_windows_case_only_catalogue_conflicts_are_not_overwritten() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.trash(&id, &m);
        fs::write(f.root.join("PHOTO.JPG"), b"original photo").unwrap();
        f.conn
            .execute(
                "INSERT INTO afiles VALUES(3,1,'PHOTO.JPG',0,2,'new user metadata')",
                [],
            )
            .unwrap();
        assert!(restore_on(&f.conn, &load(&f.conn, &id).unwrap()).is_err());
        assert_eq!(
            f.conn
                .query_row("SELECT comments FROM afiles WHERE id=3", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "new user metadata"
        );
    }

    #[test]
    fn media_lifecycle_restore_retains_ids_ratings_tags_faces_annotations_and_bytes() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.trash(&id, &m);
        assert!(!f.root.join("photo.jpg").exists());
        assert_eq!(
            f.conn
                .query_row("SELECT rating FROM afiles WHERE id=1", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            5
        );
        restore_on(&f.conn, &load(&f.conn, &id).unwrap()).unwrap();
        assert_eq!(
            fs::read(f.root.join("photo.jpg")).unwrap(),
            b"original photo"
        );
        assert_eq!(fs::read(f.root.join("video.mov")).unwrap(), b"paired video");
        assert_eq!(
            fs::read(f.root.join("photo.AAE")).unwrap(),
            b"editing sidecar"
        );
        let row: (String, i64, String) = f
            .conn
            .query_row(
                "SELECT name,rating,comments FROM afiles WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, ("photo.jpg".into(), 5, "keep my comment".into()));
        for table in ["athumbs", "faces", "afile_tags", "manual_annotations"] {
            assert_eq!(
                f.conn
                    .query_row(
                        &format!("SELECT COUNT(*) FROM {table} WHERE file_id=1"),
                        [],
                        |r| r.get::<_, i64>(0)
                    )
                    .unwrap(),
                1
            );
        }
        assert_eq!(
            f.conn
                .query_row("SELECT name FROM persons WHERE id=9", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "Alice"
        );
    }
    #[test]
    fn media_lifecycle_restore_never_overwrites_a_conflicting_original() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.trash(&id, &m);
        fs::write(f.root.join("photo.jpg"), b"new unrelated user file").unwrap();
        assert!(restore_on(&f.conn, &load(&f.conn, &id).unwrap()).is_err());
        assert_eq!(
            fs::read(f.root.join("photo.jpg")).unwrap(),
            b"new unrelated user file"
        );
        verify(Path::new(&m.members[0].stored_path), &m.members[0]).unwrap();
    }
    #[test]
    fn media_lifecycle_interrupted_move_restores_original_and_moved_members() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.stage(&id, &m);
        no_replace_move(
            Path::new(&m.members[0].original_path),
            Path::new(&m.members[0].stored_path),
            &m.members[0],
        )
        .unwrap();
        restore_on(&f.conn, &load(&f.conn, &id).unwrap()).unwrap();
        for member in &m.members {
            verify(Path::new(&member.original_path), member).unwrap();
        }
    }
    #[test]
    fn media_lifecycle_interrupted_restore_can_be_retried() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.trash(&id, &m);
        f.conn
            .execute(
                "UPDATE media_trash_groups SET status='restoring' WHERE id=?1",
                [&id],
            )
            .unwrap();
        no_replace_move(
            Path::new(&m.members[0].stored_path),
            Path::new(&m.members[0].original_path),
            &m.members[0],
        )
        .unwrap();
        restore_on(&f.conn, &load(&f.conn, &id).unwrap()).unwrap();
        assert!(f.root.join("photo.jpg").exists());
    }
    #[test]
    fn media_lifecycle_permanent_cleanup_never_touches_new_originals_or_neighbors() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.trash(&id, &m);
        fs::write(f.root.join("photo.jpg"), b"new user file").unwrap();
        fs::write(f.root.join("neighbor.jpg"), b"neighbor").unwrap();
        purge_on(&f.conn, &load(&f.conn, &id).unwrap()).unwrap();
        assert_eq!(
            fs::read(f.root.join("photo.jpg")).unwrap(),
            b"new user file"
        );
        assert_eq!(fs::read(f.root.join("neighbor.jpg")).unwrap(), b"neighbor");
        assert_eq!(
            f.conn
                .query_row("SELECT COUNT(*) FROM afiles", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            f.conn
                .query_row("SELECT COUNT(*) FROM faces", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn media_lifecycle_interrupted_purge_finishes_without_restoring_partial_groups() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.trash(&id, &m);
        f.conn
            .execute(
                "UPDATE media_trash_groups SET status='purging' WHERE id=?1",
                [&id],
            )
            .unwrap();
        fs::remove_file(&m.members[0].stored_path).unwrap();
        let item = load(&f.conn, &id).unwrap();
        assert!(restore_on(&f.conn, &item).is_err());
        purge_on(&f.conn, &item).unwrap();
        assert!(!Path::new(&m.members[1].stored_path).exists());
    }
    #[test]
    fn media_lifecycle_pending_moves_cannot_be_permanently_deleted() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.stage(&id, &m);
        assert!(purge_on(&f.conn, &load(&f.conn, &id).unwrap()).is_err());
        assert!(f.root.join("photo.jpg").exists());
    }
    #[test]
    fn media_lifecycle_tampered_paths_and_copies_are_rejected_without_deletion() {
        let f = Fixture::new();
        let (id, mut m) = f.plan();
        m.members[0].stored_path = f.root.join("photo.jpg").to_string_lossy().into_owned();
        assert!(validate_root(&m, &id).is_err());
        let (id, m) = f.plan();
        f.trash(&id, &m);
        fs::write(&m.members[0].stored_path, b"changed recovery data").unwrap();
        assert!(purge_on(&f.conn, &load(&f.conn, &id).unwrap()).is_err());
        assert!(Path::new(&m.members[1].stored_path).exists());
    }
    #[test]
    fn media_lifecycle_hidden_and_deleted_are_distinct_and_restore_keeps_hidden_state() {
        let f = Fixture::new();
        f.conn
            .execute("INSERT INTO media_states(file_id,hidden) VALUES(1,1)", [])
            .unwrap();
        let visible = |predicate: String| {
            f.conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM afiles a WHERE a.id=1 AND {predicate}"),
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
        };
        assert_eq!(visible(normal_predicate("a")), 0);
        assert_eq!(visible(retained_predicate("a")), 1);
        let (id, m) = f.plan();
        f.trash(&id, &m);
        assert_eq!(visible(retained_predicate("a")), 0);
        restore_on(&f.conn, &load(&f.conn, &id).unwrap()).unwrap();
        assert_eq!(visible(normal_predicate("a")), 0);
        assert_eq!(visible(retained_predicate("a")), 1);
    }
    #[test]
    fn media_lifecycle_reserved_names_and_folder_guards_protect_interrupted_operations() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.stage(&id, &m);
        assert!(path_reserved(&f.conn, 1, "photo.jpg").unwrap());
        assert!(folder_has_deleted(&f.conn, &f.root.to_string_lossy()).unwrap());
        assert!(has_deleted_in_album(&f.conn, 1).unwrap());
        assert!(!path_reserved(&f.conn, 1, "new.jpg").unwrap());
        assert!(is_recovery_path(&f.root.join(".lap-trash/test/file.jpg")));
    }
    #[test]
    fn media_lifecycle_same_bytes_with_a_new_catalogue_record_still_conflict() {
        let f = Fixture::new();
        let (id, m) = f.plan();
        f.trash(&id, &m);
        fs::write(f.root.join("photo.jpg"), b"original photo").unwrap();
        f.conn
            .execute(
                "INSERT INTO afiles VALUES(3,1,'photo.jpg',0,1,'new metadata')",
                [],
            )
            .unwrap();
        assert!(restore_on(&f.conn, &load(&f.conn, &id).unwrap()).is_err());
        assert_eq!(
            f.conn
                .query_row("SELECT rating FROM afiles WHERE id=3", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}

#[tauri::command]
pub async fn delete_hidden_media(
    app: tauri::AppHandle,
    library_id: String,
    file_ids: Vec<i64>,
) -> Result<crate::t_cmds::BatchDeleteResult, String> {
    tauri::async_runtime::spawn_blocking(move||{
        let files={let _scope=crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read().map_err(|e|e.to_string())?;if crate::t_config::current_library_id()?!=library_id{return Err("Library changed".into());}let conn=t_sqlite::open_conn()?;let mut files=Vec::new();
            for id in file_ids{let hidden:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND hidden=1 AND deleted_at IS NULL)",[id],|r|r.get(0)).map_err(|e|e.to_string())?;if !hidden{return Err("This selection is no longer in Hidden".into());}let file=AFile::get_file_info(id)?.ok_or("Photo no longer exists")?;files.push(crate::t_cmds::BatchDeleteFile{file_id:id,file_path:file.file_path.ok_or("Original path missing")?});}files};
        let result=move_to_recently_deleted(files,Some(&library_id),true)?;event(&app,&library_id);Ok(result)
    }).await.map_err(|e|e.to_string())?
}
