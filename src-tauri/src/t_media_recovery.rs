//! Independent recovery journals. They describe software state, never write media metadata.
use crate::t_media_lifecycle::{Manifest, TrashItem};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
const VERSION: u32 = 1;
const LIMIT: u64 = 4 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogFile {
    id: i64,
    name: String,
    folder: String,
    hidden: bool,
    identity: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Journal {
    version: u32,
    revision: u64,
    record: TrashItem,
    files: Vec<CatalogFile>,
    #[serde(default)]
    restored_files: Vec<CatalogFile>,
}
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub registered: usize,
    pub reattached: usize,
    pub issues: Vec<String>,
}
fn exists(conn: &Connection, table: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}
fn read_files(conn: &Connection, manifest: &Manifest) -> Result<Vec<CatalogFile>, String> {
    let mut files = Vec::new();
    for member in &manifest.members {
        let Some(id) = member.file_id else { continue };
        let mut stmt=conn.prepare("SELECT a.*,b.path AS recovery_folder,COALESCE(ms.hidden,0) AS recovery_hidden FROM afiles a JOIN afolders b ON b.id=a.folder_id LEFT JOIN media_states ms ON ms.file_id=a.id WHERE a.id=?1").map_err(|e|e.to_string())?;
        let columns = stmt
            .column_names()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>();
        let row = stmt
            .query_row([id], |r| {
                let mut identity = BTreeMap::new();
                for (i, name) in columns.iter().enumerate() {
                    if ![
                        "size",
                        "modified_at",
                        "created_at",
                        "inode",
                        "content_identifier",
                        "folder_id",
                    ]
                    .contains(&name.as_str())
                    {
                        continue;
                    }
                    let value = match r.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                        rusqlite::types::ValueRef::Integer(v) => serde_json::json!(v),
                        rusqlite::types::ValueRef::Real(v) => serde_json::json!(v),
                        rusqlite::types::ValueRef::Text(v) => {
                            serde_json::json!(String::from_utf8_lossy(v))
                        }
                        _ => continue,
                    };
                    identity.insert(name.clone(), value);
                }
                Ok(CatalogFile {
                    id,
                    name: member.original_name.clone(),
                    folder: r.get("recovery_folder")?,
                    hidden: r.get("recovery_hidden")?,
                    identity,
                })
            })
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(row) = row {
            files.push(row);
        }
    }
    Ok(files)
}
fn candidates(directory: &Path) -> Result<Vec<(u64, PathBuf)>, String> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("recovery-") || !name.ends_with(".json") {
            continue;
        }
        let revision = name
            .trim_start_matches("recovery-")
            .split('-')
            .next()
            .and_then(|v| v.parse::<u64>().ok());
        if let Some(revision) = revision {
            if entry.file_type().map_err(|e| e.to_string())?.is_file() {
                result.push((revision, entry.path()));
            }
        }
    }
    result.sort_by_key(|(r, _)| *r);
    Ok(result)
}
fn latest(directory: &Path, id: &str) -> Result<Option<Journal>, String> {
    let Some((revision, path)) = candidates(directory)?.pop() else {
        return Ok(None);
    };
    let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() || meta.len() > LIMIT {
        return Err("Invalid recovery journal file".into());
    }
    let journal: Journal = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Recovery journal is damaged: {e}"))?;
    if journal.version != VERSION || journal.revision != revision || journal.record.id != id {
        return Err("Recovery journal version/identity mismatch".into());
    }
    crate::t_media_lifecycle::validate_root(&journal.record.manifest, id)?;
    Ok(Some(journal))
}
pub(crate) fn checkpoint(
    conn: &Connection,
    record: &TrashItem,
    status: Option<&str>,
) -> Result<(), String> {
    let directory = crate::t_media_lifecycle::validate_root(&record.manifest, &record.id)?;
    if !directory.exists() {
        return Err("Recovery directory is missing; media operation stopped".into());
    }
    let previous = latest(&directory, &record.id)?;
    let revision = previous.as_ref().map(|j| j.revision + 1).unwrap_or(1);
    let mut files = read_files(conn, &record.manifest)?;
    if files.is_empty() {
        files = previous
            .as_ref()
            .map(|j| j.files.clone())
            .unwrap_or_default();
    }
    let mut record = record.clone();
    if let Some(status) = status {
        record.status = status.into();
        record.error = None;
    }
    let restored_files = if record.status == "restored" {
        let mut final_manifest = record.manifest.clone();
        if let Some(plan) = &record.manifest.restore_plan {
            for (member, target) in final_manifest.members.iter_mut().zip(&plan.targets) {
                member.original_name = Path::new(&target.path)
                    .file_name()
                    .ok_or("Restored filename missing")?
                    .to_string_lossy()
                    .into_owned();
            }
        }
        let final_files = read_files(conn, &final_manifest)?;
        // Preserve original catalogue identity so an older database can reconcile a completed relocation.
        if let Some(previous) = &previous {
            files = previous.files.clone();
        }
        final_files
    } else {
        Vec::new()
    };
    let journal = Journal {
        version: VERSION,
        revision,
        record,
        files,
        restored_files,
    };
    let bytes = serde_json::to_vec_pretty(&journal).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Recovery journal exceeds the safety limit".into());
    }
    let token = uuid::Uuid::new_v4();
    let temp = directory.join(format!(".journal-{token}.tmp"));
    let target = directory.join(format!("recovery-{revision:020}-{token}.json"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(&temp, &target).map_err(|e| e.to_string())?;
    Ok(())
}
pub(crate) fn checkpoint_current(conn: &Connection, id: &str) -> Result<(), String> {
    let record = crate::t_media_lifecycle::load(conn, id)?;
    checkpoint(conn, &record, None)
}
fn matches(conn: &Connection, file: &CatalogFile, group: &str) -> Result<bool, String> {
    let manifest = Manifest {
        restore_plan: None,
        root: String::new(),
        primary_id: file.id,
        members: vec![crate::t_media_lifecycle::Member {
            retain_original: false,
            source_inode: None,
            file_id: Some(file.id),
            original_name: file.name.clone(),
            original_path: String::new(),
            stored_path: String::new(),
            size: 0,
            hash: String::new(),
        }],
    };
    let current = read_files(conn, &manifest)?.into_iter().next();
    let Some(current) = current else {
        return Ok(false);
    };
    if current.folder != file.folder || current.identity != file.identity {
        return Ok(false);
    }
    let name: String = conn
        .query_row("SELECT name FROM afiles WHERE id=?1", [file.id], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    Ok(name == file.name || name == format!(".lap-deleted-{group}-{}", file.id))
}
fn remember(
    conn: &Connection,
    journal: &Journal,
    status: &str,
    matched: &[CatalogFile],
) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO media_trash_groups(id,created_at,status,manifest,error) VALUES(?1,?2,?3,?4,?5)",params![journal.record.id,journal.record.created_at,status,serde_json::to_string(&journal.record.manifest).map_err(|e|e.to_string())?,if status=="review_required"{Some("Recovery journal and restored database differ. Metadata was not overwritten; inspect the original/recovery files before retrying.")}else{None}]).map_err(|e|e.to_string())?;
    for file in matched {
        tx.execute("INSERT INTO media_states(file_id,hidden,deleted_at,trash_group,original_name) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(file_id) DO UPDATE SET hidden=excluded.hidden,deleted_at=excluded.deleted_at,trash_group=excluded.trash_group,original_name=excluded.original_name",params![file.id,file.hidden,journal.record.created_at,journal.record.id,file.name]).map_err(|e|e.to_string())?;
        tx.execute(
            "UPDATE afiles SET name=?1 WHERE id=?2",
            params![
                format!(".lap-deleted-{}-{}", journal.record.id, file.id),
                file.id
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
// A completed relocation may refer to an album absent from an older database.
// Report it and protect scan cleanup instead of aborting the entire recovery listing.
fn reconcile_completed(
    conn: &Connection,
    journal: &Journal,
    album: i64,
    id: &str,
    report: &mut Report,
    repair: bool,
) -> Result<(), String> {
    let known: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM media_trash_groups WHERE id=?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let result = (|| {
        if let Some(plan) = &journal.record.manifest.restore_plan {
            crate::t_media_restore::validate_targets(conn, &journal.record.manifest, plan)?;
        }
        if !repair {
            return Ok(());
        }
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        crate::t_media_restore::apply_catalog(&tx, &journal.record)?;
        tx.commit().map_err(|e| e.to_string())
    })();
    match result {
        Ok(()) => {
            if repair && known {
                report.reattached += 1;
            }
        }
        Err(error) => {
            let reason = format!(
                "{id}: completed restoration cannot be reconciled: {error}. Verify the destination album and database identity; no media was changed"
            );
            if repair {
                conn.execute(
                    "INSERT OR REPLACE INTO media_recovery_issues VALUES(?1,?2,?3)",
                    params![album, id, reason],
                )
                .map_err(|e| e.to_string())?;
            }
            report.issues.push(reason);
        }
    }
    Ok(())
}

/// Only a validated final identity counts as an idempotent retry success.
pub(crate) fn completed_operation(
    conn: &Connection,
    id: &str,
    purge: bool,
) -> Result<bool, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "Invalid recovery group ID")?;
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM media_trash_groups WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists {
        return Ok(false);
    }
    let mut statement = conn
        .prepare("SELECT path FROM albums")
        .map_err(|e| e.to_string())?;
    let roots = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for root in roots {
        crate::t_media_jobs::check()?;
        let Ok(root) = Path::new(&root).canonicalize() else {
            continue;
        };
        let folder = root.join(".lap-trash").join(id);
        if !folder.try_exists().map_err(|e| e.to_string())? {
            continue;
        }
        let Some(journal) = latest(&folder, id)? else {
            continue;
        };
        if Path::new(&journal.record.manifest.root) != root
            || crate::t_media_lifecycle::validate_root(&journal.record.manifest, id).is_err()
        {
            return Ok(false);
        }
        if purge && journal.record.status == "purged" {
            for member in &journal.record.manifest.members {
                if Path::new(&member.stored_path)
                    .try_exists()
                    .map_err(|e| e.to_string())?
                {
                    return Ok(false);
                }
                if let Some(file) = member.file_id {
                    let exists: bool = conn
                        .query_row(
                            "SELECT EXISTS(SELECT 1 FROM afiles WHERE id=?1)",
                            [file],
                            |r| r.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    if exists {
                        return Ok(false);
                    }
                }
            }
            return Ok(true);
        }
        if !purge
            && journal.record.status == "restored"
            && crate::t_media_restore::completed_files_valid(&journal.record)
        {
            let files = if journal.restored_files.is_empty() {
                &journal.files
            } else {
                &journal.restored_files
            };
            if files.is_empty() {
                return Ok(false);
            }
            for file in files {
                if !matches(conn, file, id)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
    }
    Ok(false)
}

/// Pagination is a bounded SQL read; startup/manual checks and operations verify the files.
pub(crate) fn listing_report(conn: &Connection) -> Result<Report, String> {
    let mut report = Report::default();
    if exists(conn, "media_recovery_report")? {
        let payload: Option<String> = conn
            .query_row(
                "SELECT payload FROM media_recovery_report WHERE id=1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(payload) = payload {
            report = serde_json::from_str(&payload).unwrap_or_else(|_| Report {
                issues: vec![
                    "The last recovery report is unreadable; run recovery inspection again".into(),
                ],
                ..Default::default()
            });
        }
    }
    report.registered = conn
        .query_row("SELECT COUNT(*) FROM media_trash_groups", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(report)
}

pub fn inspect(conn: &Connection, repair: bool) -> Result<Report, String> {
    if !exists(conn, "media_trash_groups")? {
        return Ok(Report::default());
    }
    if repair {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS media_recovery_issues(album_id INTEGER,group_id TEXT,reason TEXT,PRIMARY KEY(album_id,group_id)); DELETE FROM media_recovery_issues;").map_err(|e|e.to_string())?;
    }
    let mut report = Report::default();
    let roots = {
        let mut stmt = conn
            .prepare("SELECT id,path FROM albums")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    let mut scanned = std::collections::HashSet::new();
    for (album, root) in roots {
        crate::t_media_jobs::check()?;
        let Ok(root) = Path::new(&root).canonicalize() else {
            report.issues.push(format!("Album is offline: {root}"));
            continue;
        };
        if !scanned.insert(root.clone()) {
            continue;
        }
        let trash = root.join(".lap-trash");
        if !trash.exists() {
            continue;
        }
        if trash.canonicalize().map_err(|e| e.to_string())? != trash {
            report.issues.push(format!(
                "Recovery directory is redirected: {}",
                trash.display()
            ));
            continue;
        }
        let entries = fs::read_dir(&trash)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let entry_count = entries.len();
        for (index, entry) in entries.into_iter().enumerate() {
            crate::t_media_jobs::check()?;
            crate::t_media_jobs::member(index + 1, entry_count);
            let id = entry.file_name().to_string_lossy().into_owned();
            if uuid::Uuid::parse_str(&id).is_err()
                || !entry.file_type().map_err(|e| e.to_string())?.is_dir()
            {
                continue;
            }
            let known: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM media_trash_groups WHERE id=?1)",
                    [&id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            match latest(&entry.path(), &id) {
                Ok(Some(journal)) => {
                    if PathBuf::from(&journal.record.manifest.root) != root {
                        report
                            .issues
                            .push(format!("{id}: journal belongs to another root"));
                        continue;
                    }
                    if known {
                        report.registered += 1;
                        let stored = crate::t_media_lifecycle::load(conn, &id)?;
                        let mut identity_ok = !journal.files.is_empty();
                        for file in &journal.files {
                            identity_ok &= matches(conn, file, &id)?;
                        }
                        if !identity_ok && !journal.restored_files.is_empty() {
                            identity_ok = true;
                            for file in &journal.restored_files {
                                identity_ok &= matches(conn, file, &id)?;
                            }
                        }
                        if !identity_ok || stored.status == "review_required" {
                            if repair {
                                conn.execute(
                                    "INSERT OR REPLACE INTO media_recovery_issues VALUES(?1,?2,?3)",
                                    params![
                                        album,
                                        id,
                                        "Registered recovery metadata requires review"
                                    ],
                                )
                                .map_err(|e| e.to_string())?;
                            }
                            report.issues.push(format!("{id}: registered catalogue identity differs from independent journal"));
                            continue;
                        }
                        if journal.record.status == "restoring"
                            && journal.record.manifest.restore_plan.is_some()
                        {
                            let mut old = stored.manifest.clone();
                            old.restore_plan = None;
                            let mut new = journal.record.manifest.clone();
                            new.restore_plan = None;
                            if serde_json::to_value(&old).map_err(|e| e.to_string())?
                                != serde_json::to_value(&new).map_err(|e| e.to_string())?
                            {
                                report.issues.push(format!(
                                    "{id}: recovery source identity differs; plan was not imported"
                                ));
                                continue;
                            }
                            crate::t_media_restore::validate_structure(
                                &journal.record.manifest,
                                journal
                                    .record
                                    .manifest
                                    .restore_plan
                                    .as_ref()
                                    .ok_or("Restore plan missing")?,
                            )?;
                            if repair {
                                conn.execute("UPDATE media_trash_groups SET status='restoring',manifest=?1 WHERE id=?2",params![serde_json::to_string(&journal.record.manifest).map_err(|e|e.to_string())?,id]).map_err(|e|e.to_string())?;
                            }
                        }
                        if journal.record.status == "restored" && stored.status != "restored" {
                            let verified =
                                crate::t_media_restore::completed_files_valid(&journal.record);
                            if verified {
                                reconcile_completed(
                                    conn,
                                    &journal,
                                    album,
                                    &id,
                                    &mut report,
                                    repair,
                                )?;
                            } else if !verified {
                                report.issues.push(format!(
                                    "{id}: completion journal and media files differ"
                                ));
                            }
                        } else if journal.record.status == "purged" && stored.status != "purging" {
                            if repair {
                                conn.execute("UPDATE media_trash_groups SET status='purging',error='Media already permanently removed; only stale catalogue cleanup remains' WHERE id=?1",[&id]).map_err(|e|e.to_string())?;
                            }
                            report.issues.push(format!(
                                "{id}: permanent removal completed before this database snapshot"
                            ));
                        }
                        continue;
                    }

                    if journal.record.status == "restored" {
                        let mut identity_ok = !journal.files.is_empty();
                        for file in &journal.files {
                            identity_ok &= matches(conn, file, &id)?;
                        }
                        if !identity_ok && !journal.restored_files.is_empty() {
                            identity_ok = true;
                            for file in &journal.restored_files {
                                identity_ok &= matches(conn, file, &id)?;
                            }
                        }
                        if crate::t_media_restore::completed_files_valid(&journal.record)
                            && identity_ok
                        {
                            reconcile_completed(conn, &journal, album, &id, &mut report, repair)?;
                            continue;
                        }
                        report.issues.push(format!(
                            "{id}: restored media or catalogue identity differs"
                        ));
                        continue;
                    }
                    let mut matched = Vec::new();
                    for file in &journal.files {
                        if matches(conn, file, &id)? {
                            matched.push(file.clone());
                        }
                    }
                    let active = matches!(
                        journal.record.status.as_str(),
                        "moving" | "trashed" | "restoring" | "purging"
                    );
                    let complete = matched.len() == journal.files.len() && !matched.is_empty();
                    let state = if active && complete {
                        journal.record.status.as_str()
                    } else {
                        "review_required"
                    };
                    if repair {
                        remember(conn, &journal, state, &matched)?;
                        report.reattached += 1;
                    }
                    if state == "review_required" {
                        if repair {
                            conn.execute(
                                "INSERT OR REPLACE INTO media_recovery_issues VALUES(?1,?2,?3)",
                                params![album, id, "Recovery files and catalogue identity differ"],
                            )
                            .map_err(|e| e.to_string())?;
                        }
                    }
                    report.issues.push(format!(
                        "{id}: recovery files were absent from this database; {}",
                        if repair && state != "review_required" {
                            "registration restored without touching media files"
                        } else {
                            "metadata requires review; no original was modified"
                        }
                    ));
                }
                Ok(None) => {
                    if known {
                        report.registered += 1;
                        if repair {
                            checkpoint_current(conn, &id)?;
                        }
                        report.issues.push(format!(
                            "{id}: legacy recovery entry received an independent journal"
                        ));
                    } else {
                        let reason = format!(
                            "{id}: unregistered recovery directory has no readable journal"
                        );
                        if repair {
                            conn.execute(
                                "INSERT OR REPLACE INTO media_recovery_issues VALUES(?1,?2,?3)",
                                params![album, id, reason],
                            )
                            .map_err(|e| e.to_string())?;
                        }
                        report.issues.push(reason);
                    }
                }
                Err(error) => {
                    let reason = format!("{id}: {error}");
                    if repair {
                        conn.execute(
                            "INSERT OR REPLACE INTO media_recovery_issues VALUES(?1,?2,?3)",
                            params![album, id, reason],
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    report.issues.push(reason);
                }
            }
        }
    }
    crate::t_media_jobs::check()?;
    if repair {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS media_recovery_report(id INTEGER PRIMARY KEY,payload TEXT NOT NULL)").map_err(|e|e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO media_recovery_report VALUES(1,?1)",
            [serde_json::to_string(&report).map_err(|e| e.to_string())?],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(report)
}

/// Cleanup is refused while an independent journal cannot safely be associated with its catalogue.
pub fn assert_cleanup_safe(conn: &Connection, album: i64) -> Result<(), String> {
    let _operation = crate::t_media_lifecycle::try_mutation_guard()?;
    if !exists(conn, "media_recovery_issues")? {
        return Ok(());
    }
    let issue: Option<String> = conn
        .query_row(
            "SELECT reason FROM media_recovery_issues WHERE album_id=?1 LIMIT 1",
            [album],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(issue) = issue {
        return Err(format!(
            "Recovery reconciliation is required before scan cleanup: {issue}"
        ));
    }
    Ok(())
}

pub fn backup_journals(conn: &Connection) -> Result<Vec<(String, String)>, String> {
    if !exists(conn, "media_trash_groups")? {
        return Ok(Vec::new());
    }
    let ids = {
        let mut stmt = conn
            .prepare("SELECT id FROM media_trash_groups ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    let mut output = Vec::new();
    for id in ids {
        let record = crate::t_media_lifecycle::load(conn, &id)?;
        let journal = Journal {
            version: VERSION,
            revision: 0,
            files: read_files(conn, &record.manifest)?,
            restored_files: Vec::new(),
            record,
        };
        output.push((
            id,
            serde_json::to_string_pretty(&journal).map_err(|e| e.to_string())?,
        ));
    }
    Ok(output)
}
