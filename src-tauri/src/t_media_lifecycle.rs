//! Photo visibility and recoverable file deletion. No automatic expiry or original-file overwrite.
use crate::t_sqlite::{self, AFile};
use rusqlite::{Connection, OptionalExtension, params};
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
    let path: String = conn
        .query_row("SELECT path FROM afolders WHERE id=?1", [folder], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    if crate::t_media_restore::planned_path_reserved(conn, &Path::new(&path).join(name))? {
        return Ok(true);
    }
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
    if crate::t_media_restore::planned_path_reserved(conn, Path::new(path))? {
        return Ok(true);
    }
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
    let root: String = conn
        .query_row("SELECT path FROM albums WHERE id=?1", [album], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if crate::t_media_restore::planned_path_reserved(conn, Path::new(&root))? {
        return Ok(true);
    }
    conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states ms JOIN afiles a ON a.id=ms.file_id JOIN afolders f ON f.id=a.folder_id WHERE f.album_id=?1 AND ms.deleted_at IS NOT NULL)",[album],|r|r.get(0)).map_err(|e|e.to_string())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub file_id: Option<i64>,
    #[serde(default)]
    pub source_inode: Option<u64>,
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
    #[serde(default)]
    pub restore_plan: Option<crate::t_media_restore::RestorePlan>,
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
        crate::t_media_jobs::check()?;
        let count = file.read(&mut buf).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        size += count as u64;
        hash.update(&buf[..count]);
        crate::t_media_jobs::bytes("verifying", size, meta.len());
    }
    crate::t_media_jobs::check()?;
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
pub(crate) fn source_inode(path: &Path) -> Option<u64> {
    crate::t_utils::FileInfo::new(&path.to_string_lossy())
        .ok()
        .map(|f| f.inode)
        .filter(|id| *id != 0)
}
fn no_replace_move(source: &Path, target: &Path, member: &Member) -> Result<(), String> {
    crate::t_media_jobs::check()?;
    if target.try_exists().map_err(|e| e.to_string())? {
        verify(target, member)?;
        if !source.try_exists().map_err(|e| e.to_string())? {
            return Ok(());
        }
        // A crash after promotion/hard-link creation may leave both names. Only
        // the recorded original inode can be unlinked; a new same-byte file is not ours.
        if member.source_inode.is_none() || source_inode(source) != member.source_inode {
            return Err("Both recovery and original copies exist, but the original identity cannot be proven. No original was removed; review this group before retrying".into());
        }
    } else {
        verify(source, member)?;
        if fs::hard_link(source, target).is_err() {
            let parent = target.parent().ok_or("Recovery parent missing")?;
            if parent.canonicalize().map_err(|e| e.to_string())? != parent {
                return Err("Recovery parent was redirected".into());
            }
            let temporary = parent.join(format!(".lap-move-{}.tmp", uuid::Uuid::new_v4()));
            let mut input = fs::File::open(source).map_err(|e| e.to_string())?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|e| e.to_string())?;
            let identity = source_inode(&temporary);
            let copied = (|| {
                crate::t_media_jobs::copy(&mut input, &mut output, member.size)?;
                let modified = input
                    .metadata()
                    .and_then(|m| m.modified())
                    .map_err(|e| e.to_string())?;
                output
                    .set_times(fs::FileTimes::new().set_modified(modified))
                    .map_err(|e| e.to_string())?;
                output
                    .flush()
                    .and_then(|_| output.sync_all())
                    .map_err(|e| e.to_string())?;
                verify(&temporary, member)?;
                verify(source, member)?;
                crate::t_media_jobs::check()?;
                if parent.canonicalize().map_err(|e| e.to_string())? != parent {
                    return Err("Recovery parent was redirected".into());
                }
                crate::t_media_restore::promote_no_replace(&temporary, target)
            })();
            drop(output);
            if copied.is_err()
                && identity.is_some()
                && source_inode(&temporary) == identity
                && temporary.canonicalize().is_ok_and(|p| p == temporary)
            {
                let _ = fs::remove_file(&temporary);
            }
            copied?;
        }
    }
    verify(target, member)?;
    verify(source, member)?;
    if member.source_inode.is_some() && source_inode(source) != member.source_inode {
        return Err("Original identity changed; safe recovery copy retained".into());
    }
    let resolved = source.canonicalize().map_err(|e| e.to_string())?;
    if resolved != source {
        return Err("Source was redirected; safe recovery copy retained".into());
    }
    crate::t_media_jobs::check()?;
    fs::remove_file(&resolved)
        .map_err(|e| format!("Copy is safe in recovery, but original removal failed: {e}"))?;
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
    // Error text is advisory. Do not publish a possibly stale database snapshot over
    // a newer recovery journal whose progress was persisted before a failed DB write.
}
fn component_ids(primary: i64) -> Result<Vec<i64>, String> {
    {
        let conn = t_sqlite::open_conn()?;
        component_ids_on(&conn, primary)
    }
}
fn component_ids_on(conn: &Connection, primary: i64) -> Result<Vec<i64>, String> {
    let mut ids = vec![primary];
    let mut statement = conn
        .prepare("SELECT id FROM afiles WHERE live_photo_video_id=?1")
        .map_err(|e| e.to_string())?;
    let owners = statement
        .query_map([primary], |r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for owner in owners.into_iter().chain(std::iter::once(primary)) {
        if !ids.contains(&owner) {
            ids.push(owner);
        }
        let companion: Option<i64> = conn
            .query_row(
                "SELECT live_photo_video_id FROM afiles WHERE id=?1",
                [owner],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .flatten();
        if let Some(id) = companion.filter(|id| *id > 0) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    let mut statement=conn.prepare("SELECT file_id FROM media_states WHERE visibility_group IN (SELECT visibility_group FROM media_states WHERE file_id=?1)").map_err(|e|e.to_string())?;
    let related = statement
        .query_map([primary], |r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for id in related {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}
fn original_path_on(conn: &Connection, id: i64) -> Result<PathBuf, String> {
    let (folder, name): (String, String) = conn
        .query_row(
            "SELECT f.path,a.name FROM afiles a JOIN afolders f ON f.id=a.folder_id WHERE a.id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("Photo is no longer registered: {e}"))?;
    Ok(Path::new(&folder).join(name))
}
thread_local! {static OPERATION_DEPTH:std::cell::Cell<usize>=const {std::cell::Cell::new(0)};}
pub(crate) struct OperationGuard {
    _mutex: Option<std::sync::MutexGuard<'static, ()>>,
}
impl Drop for OperationGuard {
    fn drop(&mut self) {
        OPERATION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}
pub(crate) fn try_mutation_guard() -> Result<OperationGuard, String> {
    if OPERATION_DEPTH.with(|depth| depth.get() > 0) {
        OPERATION_DEPTH.with(|depth| depth.set(depth.get() + 1));
        return Ok(OperationGuard { _mutex: None });
    }
    let guard = OPERATIONS
        .try_lock()
        .map_err(|_| "A media operation is running; stop it before changing files or folders")?;
    OPERATION_DEPTH.with(|depth| depth.set(1));
    Ok(OperationGuard {
        _mutex: Some(guard),
    })
}
fn operation_guard() -> Result<OperationGuard, String> {
    if OPERATION_DEPTH.with(|depth| depth.get() > 0) {
        return try_mutation_guard();
    }
    loop {
        crate::t_media_jobs::check()?;
        match OPERATIONS.try_lock() {
            Ok(guard) => {
                OPERATION_DEPTH.with(|depth| depth.set(1));
                return Ok(OperationGuard {
                    _mutex: Some(guard),
                });
            }
            Err(std::sync::TryLockError::Poisoned(error)) => return Err(error.to_string()),
            Err(_) => {
                crate::t_media_jobs::stage("waiting");
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}
pub fn hide(ids: Vec<i64>, hidden: bool) -> Result<(), String> {
    let _guard = try_mutation_guard()?;
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
    for file_id in component_ids_on(conn, primary)? {
        let deleted:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND deleted_at IS NOT NULL)",[file_id],|r|r.get(0)).map_err(|e|e.to_string())?;
        if deleted {
            return Err("Photo is already in Recently Deleted".into());
        }
        let original = original_path_on(conn, file_id)?;
        let lexical = original.as_path();
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
            let identity = source_inode(&path);
            let (size, hash) = fingerprint(&path)?;
            if identity.is_some() && source_inode(&path) != identity {
                return Err("Original changed during preflight".into());
            }
            let stored = root.join(".lap-trash").join(id).join(format!(
                "{}-{}",
                members.len(),
                original_name
            ));
            members.push(Member {
                source_inode: identity,
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
        restore_plan: None,
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
    crate::t_media_jobs::admitted(id);
    crate::t_media_recovery::checkpoint_current(conn, id)
}
pub fn validate_original(id: i64, path: &str, allow_hidden: bool) -> Result<(), String> {
    {
        let conn = t_sqlite::open_conn()?;
        validate_original_on(&conn, id, path, allow_hidden)
    }
}
fn validate_original_on(
    conn: &Connection,
    id: i64,
    path: &str,
    allow_hidden: bool,
) -> Result<(), String> {
    let (hidden,deleted):(bool,bool)=conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND hidden=1),EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND deleted_at IS NOT NULL)",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;
    if (hidden && !allow_hidden) || deleted || is_recovery_path(Path::new(path)) {
        return Err(
            "Use Hidden/Recently Deleted actions for this photo; original was not deleted".into(),
        );
    }
    let registered = original_path_on(conn, id)?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if path.is_empty() {
        if !(allow_hidden && hidden) {
            return Err("Original path is required".into());
        }
    } else if Path::new(path).canonicalize().map_err(|e| e.to_string())? != registered {
        return Err("Original path is stale or belongs to another photo/library".into());
    }
    Ok(())
}
pub fn move_to_recently_deleted(
    files: Vec<crate::t_cmds::BatchDeleteFile>,
    expected_library: Option<&str>,
    allow_hidden: bool,
) -> Result<crate::t_cmds::BatchDeleteResult, String> {
    let conn = {
        let _scope = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if let Some(library) = expected_library {
            if crate::t_config::current_library_id()? != library {
                return Err("Library changed; deletion was not performed".into());
            }
        }
        t_sqlite::open_conn()?
    };
    move_job_on(
        &conn,
        files
            .into_iter()
            .map(|f| crate::t_media_jobs::Item::File {
                file_id: f.file_id,
                file_path: f.file_path,
            })
            .collect(),
        allow_hidden,
    )
}
pub(crate) fn move_job_on(
    conn: &Connection,
    items: Vec<crate::t_media_jobs::Item>,
    allow_hidden: bool,
) -> Result<crate::t_cmds::BatchDeleteResult, String> {
    use crate::t_media_jobs::{self as job, Item};
    let _guard = operation_guard()?;
    let mut plans = Vec::new();
    let mut seen = HashSet::new();
    let mut paths = HashSet::new();
    let mut result = crate::t_cmds::BatchDeleteResult {
        failed_count: 0,
        deleted_file_ids: Vec::new(),
        trash_failed_file_ids: Vec::new(),
    };
    // Resumed groups own their existing companions, including shared AAE paths.
    for item in &items {
        job::check()?;
        if let Item::Group { group_id } = item {
            if let Ok(group) = load(conn, group_id) {
                if matches!(group.status.as_str(), "moving" | "trashed")
                    && validate_root(&group.manifest, group_id).is_ok()
                {
                    for member in &group.manifest.members {
                        paths.insert(member.original_path.clone());
                        if let Some(id) = member.file_id {
                            seen.insert(id);
                        }
                    }
                }
            }
        }
    }
    for item in items {
        if job::check().is_err() {
            break;
        }
        job::select(item.clone());
        let prepared = (|| match &item {
            Item::Inspection => Err("Inspection is not a media move".into()),
            Item::Group { group_id } => {
                let stored = match load(conn, group_id) {
                    Ok(stored) => stored,
                    Err(error) => {
                        if crate::t_media_recovery::completed_operation(conn, group_id, false)?
                            || crate::t_media_recovery::completed_operation(conn, group_id, true)?
                        {
                            job::skipped(&item);
                            return Ok((
                                String::new(),
                                Manifest {
                                    root: String::new(),
                                    primary_id: 0,
                                    members: Vec::new(),
                                    restore_plan: None,
                                },
                                true,
                            ));
                        }
                        return Err(error);
                    }
                };
                if stored.status == "trashed" {
                    for member in &stored.manifest.members {
                        verify(Path::new(&member.stored_path), member)?;
                    }
                    job::complete();
                    return Ok((String::new(), stored.manifest, true));
                }
                if stored.status != "moving" {
                    return Err("Only an interrupted move can resume moving into recovery".into());
                }
                Ok((group_id.clone(), stored.manifest, true))
            }
            Item::File { file_id, file_path } => {
                if seen.contains(file_id) {
                    job::skipped(&item);
                    return Ok((
                        String::new(),
                        Manifest {
                            root: String::new(),
                            primary_id: *file_id,
                            members: Vec::new(),
                            restore_plan: None,
                        },
                        false,
                    ));
                }
                validate_original_on(conn, *file_id, file_path, allow_hidden)?;
                let id = uuid::Uuid::new_v4().to_string();
                let mut manifest = prepare_manifest(conn, *file_id, &id)?;
                manifest
                    .members
                    .retain(|m| paths.insert(m.original_path.clone()));
                assign_storage_paths(&mut manifest, &id);
                for m in &manifest.members {
                    if let Some(id) = m.file_id {
                        seen.insert(id);
                    }
                }
                Ok((id, manifest, false))
            }
        })();
        match prepared {
            Ok((id, manifest, registered)) => {
                if !id.is_empty() {
                    plans.push((item, id, manifest, registered));
                }
            }
            Err(error) => {
                if job::check().is_err() {
                    break;
                }
                job::failed(error);
                result.failed_count += 1;
                if let Item::File { file_id, .. } = item {
                    result.trash_failed_file_ids.push(file_id);
                }
            }
        }
    }
    for (item, id, manifest, registered) in plans {
        if job::check().is_err() {
            break;
        }
        job::select(item.clone());
        let prepared = (|| {
            let dir = validate_root(&manifest, &id)?;
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            validate_root(&manifest, &id)?;
            if !registered {
                // Serialize only catalogue admission with interactive file mutations,
                // never the expensive hashing/copying of original media.
                let _admission = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
                    .read()
                    .map_err(|e| e.to_string())?;
                job::check()?;
                for member in &manifest.members {
                    if let Some(file_id) = member.file_id {
                        validate_original_on(conn, file_id, &member.original_path, allow_hidden)?;
                    }
                    if member.source_inode.is_some()
                        && source_inode(Path::new(&member.original_path)) != member.source_inode
                    {
                        return Err(
                            "Original identity changed after preflight; nothing was moved".into(),
                        );
                    }
                }
                register_intent(conn, &id, &manifest)?;
            } else {
                crate::t_media_recovery::checkpoint_current(conn, &id)?;
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = prepared {
            if load(conn, &id).is_ok() {
                set_error(conn, &id, &error);
            } else if !registered {
                if let Ok(directory) = validate_root(&manifest, &id) {
                    let _ = fs::remove_dir(directory);
                }
            }
            job::failed(error);
            result.failed_count += 1;
            result.trash_failed_file_ids.push(manifest.primary_id);
            continue;
        }
        result
            .deleted_file_ids
            .extend(manifest.members.iter().filter_map(|m| m.file_id));
        let moved = (|| {
            for (index, member) in manifest.members.iter().enumerate() {
                job::check()?;
                job::member(index + 1, manifest.members.len());
                validate_root(&manifest, &id)?;
                no_replace_move(
                    Path::new(&member.original_path),
                    Path::new(&member.stored_path),
                    member,
                )?;
            }
            job::stage("catalogue");
            conn.execute(
                "UPDATE media_trash_groups SET status='trashed',error=NULL WHERE id=?1",
                [&id],
            )
            .map_err(|e| e.to_string())?;
            crate::t_media_recovery::checkpoint_current(conn, &id)?;
            Ok::<_, String>(())
        })();
        match moved {
            Ok(()) => job::complete(),
            Err(error) => {
                set_error(conn, &id, &error);
                job::failed(error);
                result.failed_count += 1;
            }
        }
    }
    Ok(result)
}
#[cfg(test)]
fn restore_on(conn: &Connection, item: &TrashItem) -> Result<(), String> {
    crate::t_media_restore::restore(
        conn,
        item,
        &crate::t_media_restore::RestoreOptions::default(),
    )
}

fn purge_on(conn: &Connection, item: &TrashItem) -> Result<(), String> {
    let dir = validate_root(&item.manifest, &item.id)?;
    if !matches!(item.status.as_str(), "trashed" | "purging") {
        return Err(
            "This move/recovery is incomplete. Restore it before permanent deletion.".into(),
        );
    }
    for (index, member) in item.manifest.members.iter().enumerate() {
        crate::t_media_jobs::member(index + 1, item.manifest.members.len());
        crate::t_media_jobs::check()?;
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
    for (index, member) in item.manifest.members.iter().enumerate() {
        crate::t_media_jobs::member(index + 1, item.manifest.members.len());
        crate::t_media_jobs::check()?;
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
            crate::t_media_jobs::stage("cleaning");
            crate::t_media_jobs::check()?;
            fs::remove_file(resolved).map_err(|e| e.to_string())?;
        }
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for (index, member) in item.manifest.members.iter().enumerate() {
        crate::t_media_jobs::member(index + 1, item.manifest.members.len());
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


pub(crate) fn inspect_job_on(conn: &Connection) -> Result<(), String> {
    let _guard = operation_guard()?;
    crate::t_media_jobs::select(crate::t_media_jobs::Item::Inspection);
    crate::t_media_jobs::stage("inspecting");
    crate::t_media_recovery::inspect(conn, true)?;
    crate::t_media_jobs::complete();
    Ok(())
}
pub(crate) fn operate_on(
    conn: &Connection,
    ids: Vec<String>,
    purge: bool,
    options: &crate::t_media_restore::RestoreOptions,
) -> Result<OperationResult, String> {
    use crate::t_media_jobs as job;
    let _guard = operation_guard()?;
    let mut result = OperationResult::default();
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        if job::check().is_err() {
            break;
        }
        job::select(job::Item::group(&id));
        let operation = (|| {
            let item = match load(conn, &id) {
                Ok(item) => item,
                Err(_) => {
                    if crate::t_media_recovery::completed_operation(conn, &id, purge)? {
                        return Ok(());
                    }
                    crate::t_media_recovery::inspect(conn, true)?;
                    if crate::t_media_recovery::completed_operation(conn, &id, purge)? {
                        return Ok(());
                    }
                    load(conn, &id)?
                }
            };
            if purge {
                purge_on(conn, &item)
            } else {
                crate::t_media_restore::restore(conn, &item, options)
            }
        })();
        match operation {
            Ok(()) => {
                result.succeeded.push(id);
                job::complete();
            }
            Err(error) => {
                set_error(conn, &id, &error);
                job::failed(error.clone());
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
    let snapshot = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let conn = &snapshot;
    let recovery = crate::t_media_recovery::listing_report(&conn)?;
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
pub(crate) fn event(app: &tauri::AppHandle, library: &str) {
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
    window: tauri::WebviewWindow,
    library_id: String,
    offset: i64,
    limit: i64,
    session_token: Option<String>,
) -> Result<HiddenPage, String> {
    let owner=window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        let permit=crate::t_private_lock::authorize(&library_id,session_token.as_deref(),&owner)?;
        let result=hidden_page(offset, limit);
        crate::t_private_lock::recheck(&permit)?;
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn get_deleted_media(
    window: tauri::WebviewWindow,
    library_id: String,
    offset: i64,
    limit: i64,
    session_token: Option<String>,
) -> Result<TrashPage, String> {
    let owner=window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        let permit=crate::t_private_lock::authorize(&library_id,session_token.as_deref(),&owner)?;
        let result=trash_page(offset, limit);
        crate::t_private_lock::recheck(&permit)?;
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn set_media_hidden(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    library_id: String,
    file_ids: Vec<i64>,
    hidden: bool,
    session_token: Option<String>,
) -> Result<(), String> {
    let owner=window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .write()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        let permit=if !hidden {Some(crate::t_private_lock::authorize(&library_id,session_token.as_deref(),&owner)?)} else {None};
        let mut affected = Vec::new();
        for id in &file_ids {
            affected.extend(component_ids(*id)?);
        }
        if let Some(permit)=&permit {crate::t_private_lock::recheck(permit)?;}
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
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    library_id: String,
    group_ids: Vec<String>,
    session_token: Option<String>,
    options: Option<crate::t_media_restore::RestoreOptions>,
) -> Result<OperationResult, String> {
    let owner = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        let permit =
            crate::t_private_lock::authorize(&library_id, session_token.as_deref(), &owner)?;
        let conn = t_sqlite::open_conn()?;
        drop(_library);
        let result = operate_on(&conn, group_ids, false, &options.unwrap_or_default())?;
        crate::t_private_lock::recheck(&permit)?;
        event(&app, &library_id);
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn purge_deleted_media(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    library_id: String,
    group_ids: Vec<String>,
    session_token: Option<String>,
) -> Result<OperationResult, String> {
    let owner = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        let permit =
            crate::t_private_lock::authorize(&library_id, session_token.as_deref(), &owner)?;
        let conn = t_sqlite::open_conn()?;
        drop(_library);
        let result = operate_on(
            &conn,
            group_ids,
            true,
            &crate::t_media_restore::RestoreOptions::default(),
        )?;
        crate::t_private_lock::recheck(&permit)?;
        event(&app, &library_id);
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_private_media_preview(
    window: tauri::WebviewWindow,
    library_id: String,
    file_id: i64,
    session_token: Option<String>,
) -> Result<Option<String>, String> {
    let owner=window.label().to_string();
    tauri::async_runtime::spawn_blocking(move||{let _library=crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read().map_err(|e|e.to_string())?;if crate::t_config::current_library_id()?!=library_id{return Err("Library changed".into());}let permit=crate::t_private_lock::authorize(&library_id,session_token.as_deref(),&owner)?;let conn=t_sqlite::open_conn()?;
    let private:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND (hidden=1 OR deleted_at IS NOT NULL))",[file_id],|r|r.get(0)).map_err(|e|e.to_string())?;
    if !private{return Err("Photo is no longer in this private view".into());}
    let result=crate::t_sqlite::AThumb::fetch(file_id)?.and_then(|t|t.thumb_data_base64).map(|bytes|format!("data:image/{};base64,{}",if bytes.starts_with("iVBORw0KGgo"){"png"}else{"jpeg"},bytes));crate::t_private_lock::recheck(&permit)?;Ok(result)
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
            conn.execute_batch("ALTER TABLE afolders ADD COLUMN name TEXT DEFAULT '';ALTER TABLE afolders ADD COLUMN created_at INTEGER;ALTER TABLE afolders ADD COLUMN modified_at INTEGER;ALTER TABLE afolders ADD COLUMN is_favorite INTEGER DEFAULT 0;ALTER TABLE afolders ADD COLUMN is_excluded_from_search INTEGER DEFAULT 0;ALTER TABLE afolders ADD COLUMN has_subfolders INTEGER;ALTER TABLE afolders ADD COLUMN inode INTEGER;").unwrap();
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
                    source_inode: None,
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
                    restore_plan: None,
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
    fn media_recovery_error_reporting_does_not_downgrade_a_journal_ahead_of_the_database() {
        let f = Fixture::new();
        let (id, manifest) = f.plan();
        f.trash(&id, &manifest);
        f.conn.execute_batch(
            "CREATE TRIGGER fail_restore_plan BEFORE UPDATE OF manifest,status ON media_trash_groups BEGIN SELECT RAISE(ABORT,'simulated database write failure'); END;",
        ).unwrap();
        let error = restore_on(&f.conn, &load(&f.conn, &id).unwrap()).unwrap_err();
        assert!(load(&f.conn, &id).unwrap().manifest.restore_plan.is_none());
        set_error(&f.conn, &id, &error);
        f.conn
            .execute_batch("DROP TRIGGER fail_restore_plan;")
            .unwrap();
        crate::t_media_recovery::inspect(&f.conn, true).unwrap();
        let recovered = load(&f.conn, &id).unwrap();
        assert_eq!(recovered.status, "restoring");
        assert!(recovered.manifest.restore_plan.is_some());
        restore_on(&f.conn, &recovered).unwrap();
        for member in &manifest.members {
            verify(Path::new(&member.original_path), member).unwrap();
        }
    }
    #[test]
    fn media_job_cancelled_preflight_releases_the_library_lock_and_preserves_originals() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let f = Fixture::new();
        f.conn.execute_batch("ALTER TABLE afiles ADD COLUMN live_photo_video_id INTEGER; UPDATE afiles SET live_photo_video_id=2 WHERE id=1;").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let item = crate::t_media_jobs::Item::File {
            file_id: 1,
            file_path: f.root.join("photo.jpg").to_string_lossy().into_owned(),
        };
        let control =
            crate::t_media_jobs::test_control(vec![item.clone()], cancel, move |summary| {
                assert!(crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.try_write().is_ok());
                if summary.stage == "planning" {
                    stop.store(true, Ordering::Release);
                }
            });
        crate::t_media_jobs::with_control(control, || move_job_on(&f.conn, vec![item], false))
            .unwrap();
        assert_eq!(
            fs::read(f.root.join("photo.jpg")).unwrap(),
            b"original photo"
        );
        assert_eq!(
            f.conn
                .query_row("SELECT COUNT(*) FROM media_trash_groups", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn media_job_resumes_a_hard_link_without_removing_a_new_same_byte_original() {
        let f = Fixture::new();
        let (id, mut manifest) = f.plan();
        for member in &mut manifest.members {
            member.source_inode = source_inode(Path::new(&member.original_path));
        }
        f.stage(&id, &manifest);
        let first = &manifest.members[0];
        fs::hard_link(&first.original_path, &first.stored_path).unwrap();
        move_job_on(&f.conn, vec![crate::t_media_jobs::Item::group(&id)], true).unwrap();
        for member in &manifest.members {
            verify(Path::new(&member.stored_path), member).unwrap();
            assert!(!Path::new(&member.original_path).exists());
        }
        let f = Fixture::new();
        let (id, mut manifest) = f.plan();
        for member in &mut manifest.members {
            member.source_inode = source_inode(Path::new(&member.original_path));
        }
        f.stage(&id, &manifest);
        let first = &manifest.members[0];
        fs::hard_link(&first.original_path, &first.stored_path).unwrap();
        fs::remove_file(&first.original_path).unwrap();
        fs::write(&first.original_path, b"original photo").unwrap();
        let result =
            move_job_on(&f.conn, vec![crate::t_media_jobs::Item::group(&id)], true).unwrap();
        assert_eq!(result.failed_count, 1);
        assert_eq!(fs::read(&first.original_path).unwrap(), b"original photo");
        assert_eq!(load(&f.conn, &id).unwrap().status, "moving");
    }
    #[test]
    fn media_job_cancelled_cleanup_is_retryable_but_never_restores_a_purging_group() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let f = Fixture::new();
        let (id, manifest) = f.plan();
        f.trash(&id, &manifest);
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let control = crate::t_media_jobs::test_control(
            vec![crate::t_media_jobs::Item::group(&id)],
            cancel,
            move |summary| {
                if summary.stage == "cleaning" {
                    stop.store(true, Ordering::Release);
                }
            },
        );
        crate::t_media_jobs::with_control(control, || {
            operate_on(&f.conn, vec![id.clone()], true, &Default::default())
        })
        .unwrap();
        assert_eq!(load(&f.conn, &id).unwrap().status, "purging");
        assert!(restore_on(&f.conn, &load(&f.conn, &id).unwrap()).is_err());
        for member in &manifest.members {
            verify(Path::new(&member.stored_path), member).unwrap();
        }
        assert!(
            operate_on(&f.conn, vec![id.clone()], true, &Default::default())
                .unwrap()
                .errors
                .is_empty()
        );
        assert!(load(&f.conn, &id).is_err());
    }
    #[test]
    fn media_job_admission_failure_keeps_the_registered_group_in_its_retry_ledger() {
        use std::sync::{Arc, atomic::AtomicBool};
        let f = Fixture::new();
        f.conn.execute_batch("ALTER TABLE afiles ADD COLUMN live_photo_video_id INTEGER; UPDATE afiles SET live_photo_video_id=2 WHERE id=1; CREATE TRIGGER fail_journal_after_commit AFTER INSERT ON media_trash_groups BEGIN UPDATE media_trash_groups SET manifest=json_set(manifest,'$.root','missing-test-root') WHERE id=NEW.id; END;").unwrap();
        let item = crate::t_media_jobs::Item::File {
            file_id: 1,
            file_path: f.root.join("photo.jpg").to_string_lossy().into_owned(),
        };
        let control = crate::t_media_jobs::test_control(
            vec![item.clone()],
            Arc::new(AtomicBool::new(false)),
            |_| {},
        );
        let result = crate::t_media_jobs::with_control(control.clone(), || {
            move_job_on(&f.conn, vec![item], false)
        })
        .unwrap();
        assert_eq!(result.failed_count, 1);
        let remaining = crate::t_media_jobs::test_remaining(&control);
        assert_eq!(remaining.len(), 1);
        let crate::t_media_jobs::Item::Group { group_id } = &remaining[0] else {
            panic!("committed admission must retry a group, not an already-private original");
        };
        let group_id = group_id.clone();
        assert_eq!(
            fs::read(f.root.join("photo.jpg")).unwrap(),
            b"original photo"
        );
        let mut pending = load(&f.conn, &group_id).unwrap();
        pending.manifest.root = f.root.to_string_lossy().into_owned();
        f.conn
            .execute_batch("DROP TRIGGER fail_journal_after_commit;")
            .unwrap();
        f.conn
            .execute(
                "UPDATE media_trash_groups SET manifest=?1 WHERE id=?2",
                params![serde_json::to_string(&pending.manifest).unwrap(), group_id],
            )
            .unwrap();
        let resumed = move_job_on(&f.conn, remaining, true).unwrap();
        assert_eq!(resumed.failed_count, 0);
        assert!(!f.root.join("photo.jpg").exists());
        assert_eq!(load(&f.conn, &group_id).unwrap().status, "trashed");
    }
    #[test]
    fn media_job_mutation_admission_is_reentrant_only_on_its_owner_thread() {
        let outer = try_mutation_guard().unwrap();
        let inner = try_mutation_guard().unwrap();
        assert!(
            std::thread::spawn(|| try_mutation_guard().is_err())
                .join()
                .unwrap()
        );
        drop(inner);
        drop(outer);
        assert!(
            std::thread::spawn(|| try_mutation_guard().is_ok())
                .join()
                .unwrap()
        );
    }
    #[test]
    fn media_job_foreground_report_reads_do_not_hash_files_or_replay_recovery() {
        use std::sync::{Arc, atomic::AtomicBool};
        let f = Fixture::new();
        let (id, manifest) = f.plan();
        f.stage(&id, &manifest);
        crate::t_media_recovery::inspect(&f.conn, true).unwrap();
        let control = crate::t_media_jobs::test_control(
            vec![crate::t_media_jobs::Item::Inspection],
            Arc::new(AtomicBool::new(true)),
            |_| {},
        );
        let report = crate::t_media_jobs::with_control(control.clone(), || {
            crate::t_media_recovery::listing_report(&f.conn)
        })
        .unwrap();
        assert_eq!(report.registered, 1);
        assert!(
            crate::t_media_jobs::with_control(control, || crate::t_media_recovery::inspect(
                &f.conn, true
            ))
            .is_err()
        );
        assert_eq!(
            fs::read(f.root.join("photo.jpg")).unwrap(),
            b"original photo"
        );
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
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    library_id: String,
    file_ids: Vec<i64>,
    session_token: Option<String>,
) -> Result<crate::t_cmds::BatchDeleteResult, String> {
    let owner=window.label().to_string();
    tauri::async_runtime::spawn_blocking(move||{
        let files={let _scope=crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read().map_err(|e|e.to_string())?;if crate::t_config::current_library_id()?!=library_id{return Err("Library changed".into());}let permit=crate::t_private_lock::authorize(&library_id,session_token.as_deref(),&owner)?;let conn=t_sqlite::open_conn()?;let mut files=Vec::new();
            for id in file_ids{let hidden:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND hidden=1 AND deleted_at IS NULL)",[id],|r|r.get(0)).map_err(|e|e.to_string())?;if !hidden{return Err("This selection is no longer in Hidden".into());}let file=AFile::get_file_info(id)?.ok_or("Photo no longer exists")?;files.push(crate::t_cmds::BatchDeleteFile{file_id:id,file_path:file.file_path.ok_or("Original path missing")?});}crate::t_private_lock::recheck(&permit)?;files};
        let result=move_to_recently_deleted(files,Some(&library_id),true)?;event(&app,&library_id);Ok(result)
    }).await.map_err(|e|e.to_string())?
}

/// Ordinary thumbnail / preview protocols never serve hidden or deleted records.
pub(crate) fn is_private_file(file_id:i64)->Result<bool,String> {
    t_sqlite::open_conn()?.query_row("SELECT EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND (hidden=1 OR deleted_at IS NOT NULL))",[file_id],|r|r.get(0)).map_err(|e|e.to_string())
}

#[tauri::command]
pub async fn preview_deleted_media_restore(
    window: tauri::WebviewWindow,
    library_id: String,
    group_ids: Vec<String>,
    options: crate::t_media_restore::RestoreOptions,
    session_token: Option<String>,
) -> Result<crate::t_media_restore::PreviewResult, String> {
    let owner = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let _scope = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        let permit =
            crate::t_private_lock::authorize(&library_id, session_token.as_deref(), &owner)?;
        let conn = t_sqlite::open_conn()?;
        let result = crate::t_media_restore::preview(&conn, &group_ids, &options);
        crate::t_private_lock::recheck(&permit)?;
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}
