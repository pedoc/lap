//! Recover registered media without replacing any destination or changing media bytes.
use crate::t_media_lifecycle::{self as lifecycle, Manifest, Member, TrashItem};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RestoreMode {
    #[default]
    Original,
    Rename,
    Directory,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RestoreOptions {
    pub mode: RestoreMode,
    pub directory: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub path: String,
    pub root: String,
    pub album_id: i64,
    pub source: String,
    pub stage: String,
    pub stage_inode: Option<u64>,
    pub staged: bool,
    pub promoted: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePlan {
    pub token: String,
    pub options: RestoreOptions,
    pub targets: Vec<Target>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub group_id: String,
    pub resumed: bool,
    pub targets: Vec<(String, String)>,
}
#[derive(Debug, Default, Serialize)]
pub struct PreviewResult {
    pub plans: Vec<Preview>,
    pub errors: Vec<(String, String)>,
}
fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
fn key(path: &Path) -> String {
    path_string(path).to_lowercase()
}
fn exists(path: &Path) -> Result<bool, String> {
    path.try_exists()
        .map_err(|e| format!("Cannot inspect {}: {e}", path.display()))
}
fn inode(path: &Path) -> Option<u64> {
    crate::t_utils::FileInfo::new(&path_string(path))
        .ok()
        .map(|i| i.inode)
        .filter(|i| *i != 0)
}
fn ensure_under(root: &Path, path: &Path) -> Result<(), String> {
    if !root.is_absolute()
        || !path.is_absolute()
        || !path.starts_with(root)
        || lifecycle::is_recovery_path(path)
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Restore path is outside its registered destination".into());
    }
    let mut parent = path.parent().ok_or("Destination parent missing")?;
    while !parent.exists() {
        parent = parent.parent().ok_or("Destination parent unavailable")?;
    }
    if parent.canonicalize().map_err(|e| e.to_string())? != parent {
        return Err("Destination parent is redirected; recovery stopped".into());
    }
    if root
        .canonicalize()
        .map_err(|e| format!("Destination disk unavailable: {e}"))?
        != root
    {
        return Err("Destination root changed".into());
    }
    Ok(())
}
fn album_for(conn: &Connection, dir: &Path) -> Result<(i64, PathBuf), String> {
    let mut stmt = conn
        .prepare("SELECT id,path FROM albums")
        .map_err(|e| e.to_string())?;
    let roots = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    for row in roots {
        let (id, path) = row.map_err(|e| e.to_string())?;
        // Album paths keep the user's lexical spelling (notably without the
        // Windows extended-length prefix); recovery paths are canonical.
        let Ok(root) = Path::new(&path).canonicalize() else {
            continue;
        };
        if dir.starts_with(&root) {
            ensure_under(&root, &dir.join(".restore-check"))?;
            return Ok((id, root));
        }
    }
    Err("Choose a directory inside an existing album. Add the directory as an album first; no new scan root was created".into())
}
// Map a physical destination back to the album's existing catalogue spelling.
// Never create a second folder tree merely because canonicalize adds \\?\ on Windows.
fn catalog_path(conn: &Connection, path: &Path) -> Result<PathBuf, String> {
    let (album, root) = album_for(conn, path.parent().ok_or("Destination parent missing")?)?;
    let stored: String = conn
        .query_row("SELECT path FROM albums WHERE id=?1", [album], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    let relative = path.strip_prefix(&root).map_err(|e| e.to_string())?;
    if relative.as_os_str().is_empty() {
        Ok(PathBuf::from(stored))
    } else {
        Ok(PathBuf::from(stored).join(relative))
    }
}
// A scan or directory operation can supply an absent or noncanonical path.
fn canonical_probe(path: &Path) -> Result<PathBuf, String> {
    let mut ancestor = path;
    let mut tail = Vec::new();
    while !exists(ancestor)? {
        tail.push(
            ancestor
                .file_name()
                .ok_or("Path has no accessible ancestor")?,
        );
        ancestor = ancestor.parent().ok_or("Path has no accessible ancestor")?;
    }
    let mut result = ancestor.canonicalize().map_err(|e| e.to_string())?;
    for part in tail.into_iter().rev() {
        result.push(part);
    }
    Ok(result)
}
fn catalog_conflict(conn: &Connection, path: &Path, own: Option<i64>) -> Result<bool, String> {
    let path = catalog_path(conn, path)?;
    conn.query_row("SELECT EXISTS(SELECT 1 FROM afiles a JOIN afolders f ON f.id=a.folder_id WHERE f.path=?1 AND a.name=?2 COLLATE NOCASE AND a.id!=?3)",params![path.parent().map(path_string),path.file_name().map(|v|v.to_string_lossy().into_owned()),own.unwrap_or(-1)],|r|r.get(0)).map_err(|e|e.to_string())
}
fn renamed(name: &str, suffix: u32) -> Result<String, String> {
    if suffix == 0 {
        return Ok(name.into());
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|v| v.to_str())
        .ok_or("Invalid original filename")?;
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| format!(".{v}"))
        .unwrap_or_default();
    Ok(format!("{stem} (restored {suffix}){extension}"))
}
pub(crate) fn validate_structure(manifest: &Manifest, plan: &RestorePlan) -> Result<(), String> {
    uuid::Uuid::parse_str(&plan.token).map_err(|_| "Invalid restore plan identity")?;
    if plan.targets.len() != manifest.members.len() {
        return Err("Restore plan does not match its registered media group".into());
    }
    let mut seen = HashSet::new();
    for (i, (m, t)) in manifest.members.iter().zip(&plan.targets).enumerate() {
        let target = Path::new(&t.path);
        let stage = Path::new(&t.stage);
        let root = Path::new(&t.root);
        if !root.is_absolute()
            || !target.is_absolute()
            || !target.starts_with(root)
            || target
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            || lifecycle::is_recovery_path(target)
            || stage.parent() != target.parent()
            || !stage.file_name().is_some_and(|n| {
                n.to_string_lossy()
                    .starts_with(&format!(".lap-restore-{}-{i}-", plan.token))
                    && n.to_string_lossy().ends_with(".tmp")
            })
            || !seen.insert(key(target))
        {
            return Err("Invalid registered restore destination or staging path".into());
        }
        if t.source != m.stored_path && t.source != m.original_path {
            return Err("Restore source is not a registered group member".into());
        }
    }
    Ok(())
}
pub(crate) fn validate_targets(
    conn: &Connection,
    manifest: &Manifest,
    plan: &RestorePlan,
) -> Result<(), String> {
    validate_structure(manifest, plan)?;
    for t in &plan.targets {
        let root: String = conn
            .query_row("SELECT path FROM albums WHERE id=?1", [t.album_id], |r| {
                r.get(0)
            })
            .map_err(|_| "Destination album no longer exists")?;
        let canonical_root = Path::new(&root)
            .canonicalize()
            .map_err(|e| format!("Destination album unavailable: {e}"))?;
        if canonical_root != PathBuf::from(&t.root) {
            return Err("Destination album identity changed".into());
        }
        ensure_under(Path::new(&t.root), Path::new(&t.path))?;
    }
    Ok(())
}
pub(crate) fn plan(
    conn: &Connection,
    item: &TrashItem,
    options: &RestoreOptions,
    reserved: &mut HashSet<String>,
) -> Result<RestorePlan, String> {
    lifecycle::validate_root(&item.manifest, &item.id)?;
    if !matches!(item.status.as_str(), "moving" | "trashed" | "restoring") {
        return Err(format!(
            "Recovery group cannot be restored in state {}",
            item.status
        ));
    }
    if let Some(plan) = &item.manifest.restore_plan {
        validate_targets(conn, &item.manifest, plan)?;
        reserved.extend(plan.targets.iter().map(|t| key(Path::new(&t.path))));
        return Ok(plan.clone());
    }
    let selected = if options.mode == RestoreMode::Directory {
        let input = options
            .directory
            .as_deref()
            .filter(|v| !v.is_empty())
            .ok_or("Choose a restore destination directory")?;
        let path = Path::new(input)
            .canonicalize()
            .map_err(|e| format!("Destination directory unavailable: {e}"))?;
        if !path.is_dir() {
            return Err("Destination is not a directory".into());
        }
        Some(path)
    } else {
        None
    };
    let token = uuid::Uuid::new_v4().to_string();
    for suffix in 0..10000 {
        let mut targets = Vec::new();
        let mut occupied = false;
        let mut internal = HashSet::new();
        for (i, m) in item.manifest.members.iter().enumerate() {
            let original = Path::new(&m.original_path);
            let parent = selected
                .as_deref()
                .unwrap_or(original.parent().ok_or("Original folder missing")?);
            let (album_id, root) = album_for(conn, parent)?;
            let path = parent.join(renamed(&m.original_name, suffix)?);
            if !internal.insert(key(&path)) {
                return Err("Group members have colliding names in this directory. Restore to original folders instead".into());
            }
            let source = if exists(Path::new(&m.stored_path))? {
                m.stored_path.clone()
            } else if matches!(item.status.as_str(), "moving" | "restoring") && exists(original)? {
                m.original_path.clone()
            } else {
                return Err(format!(
                    "Registered recovery source unavailable: {}",
                    m.original_name
                ));
            };
            let already_original = Path::new(&source) == path
                && matches!(item.status.as_str(), "moving" | "restoring");
            if (exists(&path)? && !already_original)
                || catalog_conflict(conn, &path, m.file_id)?
                || planned_path_reserved(conn, &path)?
                || reserved.contains(&key(&path))
            {
                occupied = true;
            }
            targets.push(Target {
                stage: path_string(&parent.join(format!(
                    ".lap-restore-{token}-{i}-{}.tmp",
                    uuid::Uuid::new_v4().simple()
                ))),
                path: path_string(&path),
                root: path_string(&root),
                album_id,
                source,
                stage_inode: None,
                staged: already_original,
                promoted: already_original,
            });
        }
        if !occupied {
            reserved.extend(targets.iter().map(|t| key(Path::new(&t.path))));
            let plan = RestorePlan {
                token,
                options: options.clone(),
                targets,
            };
            validate_targets(conn, &item.manifest, &plan)?;
            return Ok(plan);
        }
        if options.mode == RestoreMode::Original {
            return Err("Original destination is occupied by a file, directory or catalog record. Nothing was overwritten; choose Rename or Directory".into());
        }
    }
    Err("Could not find an available common suffix for the media group".into())
}
fn persist(conn: &Connection, item: &TrashItem) -> Result<(), String> {
    crate::t_media_recovery::checkpoint(conn, item, None)?;
    conn.execute(
        "UPDATE media_trash_groups SET status=?1,manifest=?2,error=NULL WHERE id=?3",
        params![
            item.status,
            serde_json::to_string(&item.manifest).map_err(|e| e.to_string())?,
            item.id
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn own_promoted(member: &Member, t: &Target) -> Result<(), String> {
    lifecycle::verify(Path::new(&t.path), member)?;
    if t.source == t.path || (t.promoted && t.stage_inode.is_none()) {
        return Ok(());
    }
    if t.stage_inode.is_none() || inode(Path::new(&t.path)) != t.stage_inode {
        return Err("Cannot confirm the interrupted destination belongs to this restore. Recovery source was retained; review the destination before retrying".into());
    }
    Ok(())
}
/// Same-directory atomic promotion. No pre-check/overwriting-rename race on any supported OS.
pub(crate) fn promote_no_replace(source: &Path, target: &Path) -> Result<(), String> {
    if source.parent() != target.parent() {
        return Err("Staging file is not in its destination directory".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
        }
        let a: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        // flags=0 deliberately excludes MOVEFILE_REPLACE_EXISTING and cross-volume copy.
        if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 0) } == 0 {
            return Err(format!(
                "Exclusive restore promotion failed; no destination overwritten: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let a =
            std::ffi::CString::new(source.as_os_str().as_bytes()).map_err(|_| "Invalid source")?;
        let b = std::ffi::CString::new(target.as_os_str().as_bytes())
            .map_err(|_| "Invalid destination")?;
        if unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                a.as_ptr(),
                libc::AT_FDCWD,
                b.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } != 0
        {
            return Err(format!(
                "Exclusive restore promotion failed: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        let a =
            std::ffi::CString::new(source.as_os_str().as_bytes()).map_err(|_| "Invalid source")?;
        let b = std::ffi::CString::new(target.as_os_str().as_bytes())
            .map_err(|_| "Invalid destination")?;
        if unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_EXCL) } != 0 {
            return Err(format!(
                "Exclusive restore promotion failed: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        return Err("Exclusive restore promotion is unsupported on this platform".into());
    }
    Ok(())
}
fn stage(conn: &Connection, item: &mut TrashItem, index: usize) -> Result<(), String> {
    let member = item.manifest.members[index].clone();
    let mut t = item
        .manifest
        .restore_plan
        .as_ref()
        .ok_or("Restore plan missing")?
        .targets[index]
        .clone();
    if t.promoted {
        own_promoted(&member, &t)?;
        return Ok(());
    }
    if exists(Path::new(&t.path))? {
        if !exists(Path::new(&t.stage))? && t.staged {
            own_promoted(&member, &t)?;
            t.promoted = true;
            item.manifest
                .restore_plan
                .as_mut()
                .ok_or("Restore plan missing")?
                .targets[index] = t;
            persist(conn, item)?;
            return Ok(());
        }
        return Err(format!(
            "Planned destination is now occupied; no file was overwritten: {}",
            t.path
        ));
    }
    lifecycle::verify(Path::new(&t.source), &member)?;
    if !t.staged {
        // Reclaim only a recorded temporary inode. Unknown temporary files are never deleted.
        if exists(Path::new(&t.stage))?
            && t.stage_inode.is_some()
            && inode(Path::new(&t.stage)) == t.stage_inode
        {
            ensure_under(Path::new(&t.root), Path::new(&t.stage))?;
            if Path::new(&t.stage)
                .canonicalize()
                .map_err(|e| e.to_string())?
                != PathBuf::from(&t.stage)
            {
                return Err("Owned staging path was redirected".into());
            }
            fs::remove_file(&t.stage).map_err(|e| e.to_string())?;
        }
        if exists(Path::new(&t.stage))? {
            let parent = Path::new(&t.path)
                .parent()
                .ok_or("Destination parent missing")?;
            let token = &item
                .manifest
                .restore_plan
                .as_ref()
                .ok_or("Restore plan missing")?
                .token;
            t.stage = path_string(&parent.join(format!(
                ".lap-restore-{token}-{index}-{}.tmp",
                uuid::Uuid::new_v4().simple()
            )));
        }
        ensure_under(Path::new(&t.root), Path::new(&t.path))?;
        fs::create_dir_all(
            Path::new(&t.path)
                .parent()
                .ok_or("Destination parent missing")?,
        )
        .map_err(|e| e.to_string())?;
        validate_targets(
            conn,
            &item.manifest,
            item.manifest
                .restore_plan
                .as_ref()
                .ok_or("Restore plan missing")?,
        )?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&t.stage)
            .map_err(|e| e.to_string())?;
        t.stage_inode = inode(Path::new(&t.stage));
        item.manifest
            .restore_plan
            .as_mut()
            .ok_or("Restore plan missing")?
            .targets[index] = t.clone();
        persist(conn, item)?;
        let mut input = fs::File::open(&t.source).map_err(|e| e.to_string())?;
        let time = input
            .metadata()
            .map_err(|e| e.to_string())?
            .modified()
            .map_err(|e| e.to_string())?;
        crate::t_media_jobs::copy(&mut input, &mut output, member.size)?;
        output
            .set_times(fs::FileTimes::new().set_modified(time))
            .map_err(|e| e.to_string())?;
        output
            .flush()
            .and_then(|_| output.sync_all())
            .map_err(|e| e.to_string())?;
        drop(output);
        lifecycle::verify(Path::new(&t.stage), &member)?;
        lifecycle::verify(Path::new(&t.source), &member)?;
        t.staged = true;
        item.manifest
            .restore_plan
            .as_mut()
            .ok_or("Restore plan missing")?
            .targets[index] = t.clone();
        persist(conn, item)?;
    }
    if t.stage_inode.is_some() && inode(Path::new(&t.stage)) != t.stage_inode {
        return Err("Owned staging file identity changed".into());
    }
    lifecycle::verify(Path::new(&t.stage), &member)?;
    validate_targets(
        conn,
        &item.manifest,
        item.manifest
            .restore_plan
            .as_ref()
            .ok_or("Restore plan missing")?,
    )?;
    if catalog_conflict(conn, Path::new(&t.path), member.file_id)? {
        return Err("Destination catalogue changed before promotion".into());
    }
    crate::t_media_jobs::check()?;
    crate::t_media_jobs::stage("promoting");
    promote_no_replace(Path::new(&t.stage), Path::new(&t.path))?;
    // On filesystems without stable IDs the persisted promoted flag is sufficient for the completed step;
    // an uncheckpointed promotion must be reviewed instead of inferring ownership from identical bytes.
    t.promoted = true;
    item.manifest
        .restore_plan
        .as_mut()
        .ok_or("Restore plan missing")?
        .targets[index] = t;
    persist(conn, item)?;
    Ok(())
}
pub(crate) fn completed_files_valid(item: &TrashItem) -> bool {
    match &item.manifest.restore_plan {
        Some(plan) => {
            validate_structure(&item.manifest, plan).is_ok()
                && item
                    .manifest
                    .members
                    .iter()
                    .zip(&plan.targets)
                    .all(|(m, t)| {
                        t.promoted
                            && !Path::new(&m.stored_path).exists()
                            && lifecycle::verify(Path::new(&t.path), m).is_ok()
                    })
        }
        None => item.manifest.members.iter().all(|m| {
            !Path::new(&m.stored_path).exists()
                && lifecycle::verify(Path::new(&m.original_path), m).is_ok()
        }),
    }
}
pub(crate) fn apply_catalog(conn: &Connection, item: &TrashItem) -> Result<(), String> {
    if let Some(plan) = &item.manifest.restore_plan {
        validate_targets(conn, &item.manifest, plan)?;
        let has_inode: bool = conn
            .prepare("PRAGMA table_info(afiles)")
            .map_err(|e| e.to_string())?
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .any(|v| v.is_ok_and(|n| n == "inode"));
        for (m, t) in item.manifest.members.iter().zip(&plan.targets) {
            if let Some(id) = m.file_id {
                let path = Path::new(&t.path);
                let catalog = catalog_path(conn, path)?;
                let catalog_parent = catalog.parent().ok_or("Destination parent missing")?;
                let folder = crate::t_sqlite::AFolder::add_to_db_with_conn(
                    conn,
                    t.album_id,
                    &path_string(catalog_parent),
                )?
                .id
                .ok_or("Destination folder was not registered")?;
                let name = path
                    .file_name()
                    .ok_or("Destination filename missing")?
                    .to_string_lossy()
                    .into_owned();
                if catalog_conflict(conn, path, Some(id))? {
                    return Err(
                        "Destination catalogue contains another file; nothing was overwritten"
                            .into(),
                    );
                }
                if conn
                    .execute(
                        "UPDATE afiles SET folder_id=?1,name=?2,last_scan_time=?3 WHERE id=?4",
                        params![folder, name, chrono::Utc::now().timestamp_millis(), id],
                    )
                    .map_err(|e| e.to_string())?
                    != 1
                {
                    return Err("Recovered file catalogue record is missing".into());
                }
                if has_inode {
                    conn.execute(
                        "UPDATE afiles SET inode=?1 WHERE id=?2",
                        params![inode(path).map(|v| v as i64), id],
                    )
                    .map_err(|e| e.to_string())?;
                }
                conn.execute(
                    "UPDATE albums SET cover_file_id=NULL WHERE cover_file_id=?1 AND id!=?2",
                    params![id, t.album_id],
                )
                .map_err(|e| e.to_string())?;
                conn.execute("UPDATE media_states SET deleted_at=NULL,trash_group=NULL,original_name=NULL WHERE file_id=?1",[id]).map_err(|e|e.to_string())?;
            }
        }
    } else {
        for m in &item.manifest.members {
            if let Some(id) = m.file_id {
                conn.execute(
                    "UPDATE afiles SET name=?1,last_scan_time=?2 WHERE id=?3",
                    params![m.original_name, chrono::Utc::now().timestamp_millis(), id],
                )
                .map_err(|e| e.to_string())?;
                conn.execute("UPDATE media_states SET deleted_at=NULL,trash_group=NULL,original_name=NULL WHERE file_id=?1",[id]).map_err(|e|e.to_string())?;
            }
        }
    }
    conn.execute("DELETE FROM media_trash_groups WHERE id=?1", [&item.id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub(crate) fn restore(
    conn: &Connection,
    item: &TrashItem,
    options: &RestoreOptions,
) -> Result<(), String> {
    let mut item = item.clone();
    let mut reserved = HashSet::new();
    let plan = plan(conn, &item, options, &mut reserved)?;
    item.manifest.restore_plan = Some(plan);
    item.status = "restoring".into();
    item.error = None;
    persist(conn, &item)?;
    for i in 0..item.manifest.members.len() {
        crate::t_media_jobs::check()?;
        crate::t_media_jobs::member(i + 1, item.manifest.members.len());
        stage(conn, &mut item, i)?;
    }
    let plan = item
        .manifest
        .restore_plan
        .as_ref()
        .ok_or("Restore plan missing")?;
    for (m, t) in item.manifest.members.iter().zip(&plan.targets) {
        lifecycle::verify(Path::new(&t.path), m)?;
        if t.source != t.path && exists(Path::new(&t.source))? {
            lifecycle::verify(Path::new(&t.source), m)?;
            lifecycle::validate_root(&item.manifest, &item.id)?;
            validate_targets(conn, &item.manifest, plan)?;
            let source = Path::new(&t.source)
                .canonicalize()
                .map_err(|e| e.to_string())?;
            if source != PathBuf::from(&t.source) {
                return Err("Recovery source was redirected".into());
            }
            crate::t_media_jobs::check()?;
            crate::t_media_jobs::stage("cleaning");
            fs::remove_file(source).map_err(|e| e.to_string())?;
        }
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    apply_catalog(&tx, &item)?;
    crate::t_media_recovery::checkpoint(&tx, &item, Some("restored"))?;
    tx.commit().map_err(|e| e.to_string())
}
pub(crate) fn planned_path_reserved(conn: &Connection, path: &Path) -> Result<bool, String> {
    let mut stmt = conn
        .prepare("SELECT manifest FROM media_trash_groups WHERE status='restoring'")
        .map_err(|e| e.to_string())?;
    for row in stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
    {
        let manifest: Manifest = serde_json::from_str(&row.map_err(|e| e.to_string())?)
            .map_err(|_| "Registered restore plan damaged")?;
        if let Some(plan) = manifest.restore_plan {
            let probe = canonical_probe(path)?;
            if plan.targets.iter().any(|t| {
                key(Path::new(&t.path)) == key(&probe) || Path::new(&t.path).starts_with(&probe)
            }) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
pub(crate) fn preview(
    conn: &Connection,
    ids: &[String],
    options: &RestoreOptions,
) -> PreviewResult {
    let mut result = PreviewResult::default();
    let mut reserved = HashSet::new();
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            continue;
        }
        match lifecycle::load(conn, id).and_then(|item| {
            plan(conn, &item, options, &mut reserved).map(|p| Preview {
                group_id: id.clone(),
                resumed: item.manifest.restore_plan.is_some(),
                targets: item
                    .manifest
                    .members
                    .iter()
                    .zip(&p.targets)
                    .map(|(m, t)| (m.original_name.clone(), t.path.clone()))
                    .collect(),
            })
        }) {
            Ok(plan) => result.plans.push(plan),
            Err(error) => result.errors.push((id.clone(), error)),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        conn: Connection,
        base: PathBuf,
        root: PathBuf,
        destination: PathBuf,
        id: String,
        manifest: Manifest,
    }
    impl Fixture {
        fn new() -> Self {
            let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/media-restore-fixtures");
            fs::create_dir_all(&base).unwrap();
            let work = base.join(format!("test-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&work).unwrap();
            let root = work.canonicalize().unwrap();
            let destination = root.join("destination");
            fs::create_dir(&destination).unwrap();
            let source = root.join("source");
            fs::create_dir(&source).unwrap();
            let conn = Connection::open_in_memory().unwrap();
            conn.execute_batch("PRAGMA foreign_keys=ON;
              CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT,cover_file_id INTEGER);
              CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,name TEXT,path TEXT,created_at INTEGER,modified_at INTEGER,is_favorite INTEGER,is_excluded_from_search INTEGER DEFAULT 0,has_subfolders INTEGER,inode INTEGER);
              CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,name TEXT,last_scan_time INTEGER,rating INTEGER,comments TEXT,inode INTEGER,media_subtype TEXT,live_photo_video_id INTEGER,UNIQUE(folder_id,name));
              CREATE TABLE manual_annotations(file_id INTEGER,name TEXT);CREATE TABLE faces(id INTEGER PRIMARY KEY,file_id INTEGER,person_id INTEGER);CREATE TABLE afile_tags(file_id INTEGER,tag_id INTEGER);CREATE TABLE acollections_files(collection_id INTEGER,file_id INTEGER);
              INSERT INTO albums VALUES(1,'',1),(2,'',NULL);
              INSERT INTO afolders VALUES(1,1,'source','',0,0,0,0,0,NULL),(2,2,'destination','',0,0,0,0,0,NULL);
              INSERT INTO afiles VALUES(1,1,'photo.dng',0,5,'keep comment',1,'raw_jpeg_pair',2),(2,1,'photo.jpg',0,3,'JPEG',2,NULL,NULL);
              INSERT INTO manual_annotations VALUES(1,'Alice');INSERT INTO faces VALUES(7,1,9);INSERT INTO afile_tags VALUES(1,42);INSERT INTO acollections_files VALUES(8,1);").unwrap();
            conn.execute(
                "UPDATE albums SET path=?1 WHERE id=1",
                [path_string(&source)],
            )
            .unwrap();
            conn.execute(
                "UPDATE albums SET path=?1 WHERE id=2",
                [path_string(&destination)],
            )
            .unwrap();
            conn.execute(
                "UPDATE afolders SET path=?1 WHERE id=1",
                [path_string(&source)],
            )
            .unwrap();
            conn.execute(
                "UPDATE afolders SET path=?1 WHERE id=2",
                [path_string(&destination)],
            )
            .unwrap();
            lifecycle::setup(&conn).unwrap();
            let id = uuid::Uuid::new_v4().to_string();
            let store = source.join(".lap-trash").join(&id);
            fs::create_dir_all(&store).unwrap();
            let mut members = Vec::new();
            for (i, (file_id, name, bytes)) in [
                (Some(1), "photo.dng", b"original RAW".as_slice()),
                (Some(2), "photo.jpg", b"original JPEG".as_slice()),
                (None, "photo.AAE", b"original sidecar".as_slice()),
            ]
            .iter()
            .enumerate()
            {
                let original = source.join(name);
                fs::write(&original, bytes).unwrap();
                let (size, hash) = lifecycle::fingerprint(&original).unwrap();
                let stored = store.join(format!("{i}-{name}"));
                fs::rename(&original, &stored).unwrap();
                members.push(Member {
                    source_inode: None,
                    file_id: *file_id,
                    original_name: name.to_string(),
                    original_path: path_string(&original),
                    stored_path: path_string(&stored),
                    size,
                    hash,
                });
                if let Some(file_id) = file_id {
                    conn.execute("INSERT INTO media_states(file_id,hidden,deleted_at,trash_group,original_name) VALUES(?1,1,1,?2,?3)",params![file_id,id,name]).unwrap();
                    conn.execute(
                        "UPDATE afiles SET name=?1 WHERE id=?2",
                        params![format!(".lap-deleted-{id}-{file_id}"), file_id],
                    )
                    .unwrap();
                }
            }
            let manifest = Manifest {
                root: path_string(&source),
                primary_id: 1,
                members,
                restore_plan: None,
            };
            conn.execute("INSERT INTO media_trash_groups(id,created_at,status,manifest) VALUES(?1,1,'trashed',?2)",params![id,serde_json::to_string(&manifest).unwrap()]).unwrap();
            crate::t_media_recovery::checkpoint_current(&conn, &id).unwrap();
            Self {
                conn,
                base,
                root,
                destination,
                id,
                manifest,
            }
        }
        fn item(&self) -> TrashItem {
            lifecycle::load(&self.conn, &self.id).unwrap()
        }
        fn options(&self, mode: RestoreMode) -> RestoreOptions {
            RestoreOptions {
                mode,
                directory: Some(path_string(&self.destination)),
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let (Ok(base), Ok(root)) = (self.base.canonicalize(), self.root.canonicalize()) {
                if root.parent() == Some(base.as_path())
                    && root
                        .file_name()
                        .is_some_and(|v| v.to_string_lossy().starts_with("test-"))
                {
                    let _ = fs::remove_dir_all(root);
                }
            }
        }
    }
    #[test]
    fn strict_original_treats_even_identical_unregistered_files_as_conflicts() {
        let f = Fixture::new();
        fs::write(&f.manifest.members[0].original_path, b"original RAW").unwrap();
        assert!(restore(&f.conn, &f.item(), &RestoreOptions::default()).is_err());
        for m in &f.manifest.members {
            lifecycle::verify(Path::new(&m.stored_path), m).unwrap();
        }
        assert_eq!(
            fs::read(&f.manifest.members[0].original_path).unwrap(),
            b"original RAW"
        );
    }
    #[test]
    fn rename_uses_one_suffix_for_raw_jpeg_and_sidecars_and_preserves_annotations() {
        let f = Fixture::new();
        let root = Path::new(&f.manifest.root);
        fs::write(root.join("photo.jpg"), b"unrelated new photo").unwrap();
        restore(&f.conn, &f.item(), &f.options(RestoreMode::Rename)).unwrap();
        assert_eq!(
            fs::read(root.join("photo.jpg")).unwrap(),
            b"unrelated new photo"
        );
        for m in &f.manifest.members {
            lifecycle::verify(&root.join(renamed(&m.original_name, 1).unwrap()), m).unwrap();
        }
        let values:(String,i64,String,Option<String>,Option<i64>)=f.conn.query_row("SELECT name,rating,comments,media_subtype,live_photo_video_id FROM afiles WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
        assert_eq!(
            values,
            (
                "photo (restored 1).dng".into(),
                5,
                "keep comment".into(),
                Some("raw_jpeg_pair".into()),
                Some(2)
            )
        );
        assert_eq!(
            f.conn
                .query_row(
                    "SELECT name FROM manual_annotations WHERE file_id=1",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "Alice"
        );
        for table in ["faces", "afile_tags", "acollections_files"] {
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
                .query_row("SELECT hidden FROM media_states WHERE file_id=1", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn directory_restore_relocates_existing_ids_and_retains_original_source_neighbors() {
        let f = Fixture::new();
        fs::write(&f.manifest.members[0].original_path, b"new unrelated RAW").unwrap();
        let sub = f.destination.join("subfolder");
        fs::create_dir(&sub).unwrap();
        let options = RestoreOptions {
            mode: RestoreMode::Directory,
            directory: Some(path_string(&sub)),
        };
        restore(&f.conn, &f.item(), &options).unwrap();
        assert_eq!(
            fs::read(&f.manifest.members[0].original_path).unwrap(),
            b"new unrelated RAW"
        );
        for m in &f.manifest.members {
            lifecycle::verify(&sub.join(&m.original_name), m).unwrap();
        }
        let (album,name):(i64,String)=f.conn.query_row("SELECT f.album_id,a.name FROM afiles a JOIN afolders f ON f.id=a.folder_id WHERE a.id=1",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(album, 2);
        assert_eq!(name, "photo.dng");
        assert_eq!(
            f.conn
                .query_row("SELECT rating FROM afiles WHERE id=1", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            5
        );
    }
    #[test]
    fn resumed_restore_keeps_targets_and_proves_atomic_promotion_without_overwriting() {
        let f = Fixture::new();
        let mut item = f.item();
        let p = plan(
            &f.conn,
            &item,
            &f.options(RestoreMode::Directory),
            &mut HashSet::new(),
        )
        .unwrap();
        item.manifest.restore_plan = Some(p);
        item.status = "restoring".into();
        persist(&f.conn, &item).unwrap();
        stage(&f.conn, &mut item, 0).unwrap();
        let saved = item.manifest.restore_plan.as_ref().unwrap().targets[0]
            .path
            .clone();
        assert!(crate::t_media_restore::planned_path_reserved(&f.conn, Path::new(&saved)).unwrap());
        restore(&f.conn, &f.item(), &f.options(RestoreMode::Rename)).unwrap();
        assert_eq!(
            f.conn
                .query_row("SELECT name FROM afiles WHERE id=1", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "photo.dng"
        );
        assert!(Path::new(&saved).exists());
    }
    #[test]
    fn a_new_conflict_after_planning_stops_without_removing_recovery_sources() {
        let f = Fixture::new();
        let mut item = f.item();
        let p = plan(
            &f.conn,
            &item,
            &f.options(RestoreMode::Directory),
            &mut HashSet::new(),
        )
        .unwrap();
        let target = p.targets[0].path.clone();
        item.manifest.restore_plan = Some(p);
        item.status = "restoring".into();
        persist(&f.conn, &item).unwrap();
        fs::write(&target, b"new user file").unwrap();
        assert!(restore(&f.conn, &f.item(), &f.options(RestoreMode::Directory)).is_err());
        assert_eq!(fs::read(target).unwrap(), b"new user file");
        for m in &f.manifest.members {
            lifecycle::verify(Path::new(&m.stored_path), m).unwrap();
        }
    }
    #[test]
    fn exclusive_promotion_does_not_clobber_a_file_created_in_the_last_moment() {
        let f = Fixture::new();
        let stage = f.destination.join("owned.tmp");
        let target = f.destination.join("new.jpg");
        fs::write(&stage, b"owned data").unwrap();
        fs::write(&target, b"user data").unwrap();
        assert!(promote_no_replace(&stage, &target).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"user data");
        assert_eq!(fs::read(&stage).unwrap(), b"owned data");
    }
    #[test]
    fn corrupt_sources_unregistered_destinations_and_escaping_plans_are_rejected() {
        let f = Fixture::new();
        let outside = f.root.join("unregistered");
        fs::create_dir(&outside).unwrap();
        assert!(
            plan(
                &f.conn,
                &f.item(),
                &RestoreOptions {
                    mode: RestoreMode::Directory,
                    directory: Some(path_string(&outside))
                },
                &mut HashSet::new()
            )
            .is_err()
        );
        let mut p = plan(
            &f.conn,
            &f.item(),
            &f.options(RestoreMode::Directory),
            &mut HashSet::new(),
        )
        .unwrap();
        p.targets[0].path = path_string(&outside.join("unsafe.dng"));
        assert!(validate_structure(&f.manifest, &p).is_err());
        fs::write(&f.manifest.members[0].stored_path, b"corrupt RAW").unwrap();
        assert!(restore(&f.conn, &f.item(), &f.options(RestoreMode::Directory)).is_err());
        assert!(!f.destination.join("photo.dng").exists());
    }
    #[test]
    fn older_database_reconciles_completed_relocation_from_its_independent_journal() {
        let f = Fixture::new();
        restore(&f.conn, &f.item(), &f.options(RestoreMode::Directory)).unwrap();
        f.conn.execute("UPDATE afiles SET folder_id=1,name=CASE id WHEN 1 THEN 'photo.dng' ELSE 'photo.jpg' END,inode=CASE id WHEN 1 THEN 1 ELSE 2 END",[]).unwrap();
        f.conn.execute("UPDATE media_states SET deleted_at=1,trash_group=?1,original_name=CASE file_id WHEN 1 THEN 'photo.dng' ELSE 'photo.jpg' END",[&f.id]).unwrap();
        f.conn.execute("INSERT INTO media_trash_groups(id,created_at,status,manifest) VALUES(?1,1,'trashed',?2)",params![f.id,serde_json::to_string(&f.manifest).unwrap()]).unwrap();
        crate::t_media_recovery::inspect(&f.conn, true).unwrap();
        assert_eq!(f.conn.query_row("SELECT f.album_id FROM afiles a JOIN afolders f ON f.id=a.folder_id WHERE a.id=1",[],|r|r.get::<_,i64>(0)).unwrap(),2);
        assert_eq!(
            f.conn
                .query_row("SELECT COUNT(*) FROM media_trash_groups", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn promotion_after_a_crash_is_recognized_by_the_staging_identity() {
        let f = Fixture::new();
        let mut item = f.item();
        let mut p = plan(
            &f.conn,
            &item,
            &f.options(RestoreMode::Directory),
            &mut HashSet::new(),
        )
        .unwrap();
        let t = &mut p.targets[0];
        fs::copy(&t.source, &t.stage).unwrap();
        t.stage_inode = inode(Path::new(&t.stage));
        t.staged = true;
        assert!(t.stage_inode.is_some());
        item.manifest.restore_plan = Some(p);
        item.status = "restoring".into();
        persist(&f.conn, &item).unwrap();
        let t = &item.manifest.restore_plan.as_ref().unwrap().targets[0];
        promote_no_replace(Path::new(&t.stage), Path::new(&t.path)).unwrap();
        assert!(!f.item().manifest.restore_plan.as_ref().unwrap().targets[0].promoted);
        restore(&f.conn, &f.item(), &f.options(RestoreMode::Directory)).unwrap();
        assert_eq!(
            fs::read(f.destination.join("photo.dng")).unwrap(),
            b"original RAW"
        );
    }
    #[test]
    fn the_independent_journal_recovers_a_plan_written_before_its_database_update() {
        let f = Fixture::new();
        let mut item = f.item();
        let p = plan(
            &f.conn,
            &item,
            &f.options(RestoreMode::Directory),
            &mut HashSet::new(),
        )
        .unwrap();
        let expected = p.targets[0].path.clone();
        item.manifest.restore_plan = Some(p);
        item.status = "restoring".into();
        crate::t_media_recovery::checkpoint(&f.conn, &item, None).unwrap();
        assert!(f.item().manifest.restore_plan.is_none());
        crate::t_media_recovery::inspect(&f.conn, true).unwrap();
        assert_eq!(
            f.item().manifest.restore_plan.as_ref().unwrap().targets[0].path,
            expected
        );
        restore(&f.conn, &f.item(), &f.options(RestoreMode::Rename)).unwrap();
        assert!(Path::new(&expected).exists());
    }
    #[test]
    fn a_partial_owned_stage_can_be_reclaimed_without_deleting_an_unknown_stage_file() {
        let f = Fixture::new();
        let mut item = f.item();
        let mut p = plan(
            &f.conn,
            &item,
            &f.options(RestoreMode::Directory),
            &mut HashSet::new(),
        )
        .unwrap();
        let original_stage = p.targets[0].stage.clone();
        fs::write(&original_stage, b"partial").unwrap();
        p.targets[0].stage_inode = inode(Path::new(&original_stage));
        item.manifest.restore_plan = Some(p);
        item.status = "restoring".into();
        persist(&f.conn, &item).unwrap();
        restore(&f.conn, &f.item(), &f.options(RestoreMode::Directory)).unwrap();
        assert!(!Path::new(&original_stage).exists());
        let f = Fixture::new();
        let mut item = f.item();
        let p = plan(
            &f.conn,
            &item,
            &f.options(RestoreMode::Directory),
            &mut HashSet::new(),
        )
        .unwrap();
        let unknown = p.targets[0].stage.clone();
        fs::write(&unknown, b"not ours").unwrap();
        item.manifest.restore_plan = Some(p);
        item.status = "restoring".into();
        persist(&f.conn, &item).unwrap();
        restore(&f.conn, &f.item(), &f.options(RestoreMode::Directory)).unwrap();
        assert_eq!(fs::read(&unknown).unwrap(), b"not ours");
    }
    #[test]
    fn completed_relocation_to_an_album_missing_from_an_older_database_reports_and_protects() {
        for known in [true, false] {
            let f = Fixture::new();
            restore(&f.conn, &f.item(), &f.options(RestoreMode::Directory)).unwrap();
            f.conn.execute("UPDATE afiles SET folder_id=1,name=CASE id WHEN 1 THEN 'photo.dng' ELSE 'photo.jpg' END,inode=CASE id WHEN 1 THEN 1 ELSE 2 END", []).unwrap();
            if known {
                f.conn.execute("UPDATE media_states SET deleted_at=1,trash_group=?1,original_name=CASE file_id WHEN 1 THEN 'photo.dng' ELSE 'photo.jpg' END", [&f.id]).unwrap();
                f.conn.execute("INSERT INTO media_trash_groups(id,created_at,status,manifest) VALUES(?1,1,'trashed',?2)", params![f.id, serde_json::to_string(&f.manifest).unwrap()]).unwrap();
            }
            f.conn
                .execute("DELETE FROM afolders WHERE album_id=2", [])
                .unwrap();
            f.conn.execute("DELETE FROM albums WHERE id=2", []).unwrap();
            let read_only = crate::t_media_recovery::inspect(&f.conn, false).unwrap();
            assert!(
                read_only
                    .issues
                    .iter()
                    .any(|e| e.contains("Destination album no longer exists"))
            );
            assert_eq!(
                f.conn
                    .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='media_recovery_issues'", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            let report = crate::t_media_recovery::inspect(&f.conn, true).unwrap();
            assert!(
                report
                    .issues
                    .iter()
                    .any(|e| e.contains("Destination album no longer exists"))
            );
            assert!(crate::t_media_recovery::assert_cleanup_safe(&f.conn, 1).is_err());
            assert_eq!(
                f.conn
                    .query_row("SELECT folder_id FROM afiles WHERE id=1", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            for m in &f.manifest.members {
                lifecycle::verify(&f.destination.join(&m.original_name), m).unwrap();
            }
        }
    }
    #[test]
    fn a_new_group_does_not_claim_another_groups_unfinished_destination() {
        let f = Fixture::new();
        let mut pending = f.item();
        pending.manifest.restore_plan = Some(
            plan(
                &f.conn,
                &pending,
                &f.options(RestoreMode::Directory),
                &mut HashSet::new(),
            )
            .unwrap(),
        );
        pending.status = "restoring".into();
        persist(&f.conn, &pending).unwrap();
        assert!(!f.destination.join("photo.dng").exists());
        let second_id = uuid::Uuid::new_v4().to_string();
        let source_folder = Path::new(&f.manifest.root).join("other");
        fs::create_dir(&source_folder).unwrap();
        let mut second = f.item();
        second.status = "trashed".into();
        second.manifest.restore_plan = None;
        second.id = second_id.clone();
        second.manifest.primary_id = 3;
        second.manifest.members.truncate(1);
        let member = &mut second.manifest.members[0];
        member.file_id = Some(3);
        member.original_path = path_string(&source_folder.join(&member.original_name));
        let stored = Path::new(&f.manifest.root)
            .join(".lap-trash")
            .join(&second_id)
            .join(format!("0-{}", member.original_name));
        fs::create_dir_all(stored.parent().unwrap()).unwrap();
        fs::copy(&member.stored_path, &stored).unwrap();
        member.stored_path = path_string(&stored);
        let planned = plan(
            &f.conn,
            &second,
            &f.options(RestoreMode::Directory),
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(
            Path::new(&planned.targets[0].path).file_name().unwrap(),
            "photo (restored 1).dng"
        );
        assert!(!f.destination.join("photo.dng").exists());
    }
    #[test]
    fn noncanonical_album_paths_match_destinations_and_do_not_duplicate_catalogue_folders() {
        for mode in [RestoreMode::Original, RestoreMode::Directory] {
            let f = Fixture::new();
            let lexical = |p: &Path| {
                if cfg!(windows) {
                    path_string(p).trim_start_matches(r"\\?\").to_string()
                } else {
                    path_string(&p.join("."))
                }
            };
            let original = lexical(Path::new(&f.manifest.root));
            let destination = lexical(&f.destination);
            f.conn
                .execute(
                    "UPDATE albums SET path=CASE id WHEN 1 THEN ?1 ELSE ?2 END",
                    params![original, destination],
                )
                .unwrap();
            f.conn
                .execute(
                    "UPDATE afolders SET path=CASE album_id WHEN 1 THEN ?1 ELSE ?2 END",
                    params![original, destination],
                )
                .unwrap();
            f.conn
                .execute("UPDATE afiles SET name='photo.dng' WHERE id=2", [])
                .unwrap();
            assert!(
                plan(
                    &f.conn,
                    &f.item(),
                    &RestoreOptions::default(),
                    &mut HashSet::new()
                )
                .is_err()
            );
            f.conn
                .execute(
                    "UPDATE afiles SET name=?1 WHERE id=2",
                    [format!(".lap-deleted-{}-2", f.id)],
                )
                .unwrap();
            let options = RestoreOptions {
                mode: mode.clone(),
                directory: Some(destination.clone()),
            };
            let previews = preview(&f.conn, &[f.id.clone()], &options);
            assert!(previews.errors.is_empty(), "{:?}", previews.errors);
            let mut pending = f.item();
            pending.manifest.restore_plan =
                Some(plan(&f.conn, &pending, &options, &mut HashSet::new()).unwrap());
            pending.status = "restoring".into();
            persist(&f.conn, &pending).unwrap();
            let output = if mode == RestoreMode::Original {
                original.clone()
            } else {
                destination.clone()
            };
            assert!(planned_path_reserved(&f.conn, &Path::new(&output).join("photo.dng")).unwrap());
            assert!(planned_path_reserved(&f.conn, Path::new(&output)).unwrap());
            restore(&f.conn, &f.item(), &options).unwrap();
            let folder: String = f
                .conn
                .query_row(
                    "SELECT f.path FROM afolders f JOIN afiles a ON a.folder_id=f.id WHERE a.id=1",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(folder, output);
            assert_eq!(
                f.conn
                    .query_row("SELECT COUNT(*) FROM afolders", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                2
            );
            for m in &f.manifest.members {
                lifecycle::verify(&Path::new(&output).join(&m.original_name), m).unwrap();
            }
        }
    }
    #[test]
    fn a_cancelled_background_restore_keeps_sources_and_continues_its_persisted_plan() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let f = Fixture::new();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let control = crate::t_media_jobs::test_control(
            vec![crate::t_media_jobs::Item::group(&f.id)],
            cancel,
            move |summary| {
                if summary.stage == "promoting" {
                    assert!(crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.try_write().is_ok());
                    stop.store(true, Ordering::Release);
                }
            },
        );
        assert!(
            crate::t_media_jobs::with_control(control, || restore(
                &f.conn,
                &f.item(),
                &f.options(RestoreMode::Directory)
            ))
            .is_err()
        );
        let pending = f.item();
        assert_eq!(pending.status, "restoring");
        assert!(pending.manifest.restore_plan.is_some());
        for member in &f.manifest.members {
            lifecycle::verify(Path::new(&member.stored_path), member).unwrap();
        }
        restore(&f.conn, &f.item(), &f.options(RestoreMode::Rename)).unwrap();
        for member in &f.manifest.members {
            lifecycle::verify(&f.destination.join(&member.original_name), member).unwrap();
        }
    }
    #[test]
    fn retrying_completed_recovery_is_idempotent_and_keeps_new_original_neighbors() {
        let f = Fixture::new();
        restore(&f.conn, &f.item(), &f.options(RestoreMode::Directory)).unwrap();
        fs::write(
            &f.manifest.members[0].original_path,
            b"new neighboring original",
        )
        .unwrap();
        let result = crate::t_media_lifecycle::operate_on(
            &f.conn,
            vec![f.id.clone()],
            false,
            &Default::default(),
        )
        .unwrap();
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.succeeded, vec![f.id.clone()]);
        assert_eq!(
            fs::read(&f.manifest.members[0].original_path).unwrap(),
            b"new neighboring original"
        );
        let remaining = crate::t_media_jobs::Item::group(&f.id);
        assert_eq!(
            crate::t_media_lifecycle::move_job_on(&f.conn, vec![remaining], true)
                .unwrap()
                .failed_count,
            0
        );
        assert_eq!(
            fs::read(&f.manifest.members[0].original_path).unwrap(),
            b"new neighboring original"
        );
    }
}
