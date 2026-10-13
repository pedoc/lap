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
    let journal = Journal {
        version: VERSION,
        revision,
        record,
        files,
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
        root: String::new(),
        primary_id: file.id,
        members: vec![crate::t_media_lifecycle::Member {
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
        for entry in fs::read_dir(&trash).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
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
                        if journal.record.status == "restored" && stored.status != "restored" {
                            let verified = journal.record.manifest.members.iter().all(|m| {
                                !Path::new(&m.stored_path).exists()
                                    && crate::t_media_lifecycle::verify(
                                        Path::new(&m.original_path),
                                        m,
                                    )
                                    .is_ok()
                            });
                            if repair && verified {
                                let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
                                for member in &journal.record.manifest.members {
                                    if let Some(file_id) = member.file_id {
                                        tx.execute("UPDATE afiles SET name=?1,last_scan_time=?2 WHERE id=?3 AND EXISTS(SELECT 1 FROM media_states WHERE file_id=?3 AND trash_group=?4)",params![member.original_name,chrono::Utc::now().timestamp_millis(),file_id,id]).map_err(|e|e.to_string())?;
                                        tx.execute("UPDATE media_states SET deleted_at=NULL,trash_group=NULL,original_name=NULL WHERE file_id=?1 AND trash_group=?2",params![file_id,id]).map_err(|e|e.to_string())?;
                                    }
                                }
                                tx.execute("DELETE FROM media_trash_groups WHERE id=?1", [&id])
                                    .map_err(|e| e.to_string())?;
                                tx.commit().map_err(|e| e.to_string())?;
                                report.reattached += 1;
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
                        if journal
                            .record
                            .manifest
                            .members
                            .iter()
                            .all(|m| Path::new(&m.original_path).exists())
                        {
                            continue;
                        }
                        report
                            .issues
                            .push(format!("{id}: restored originals are missing"));
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
    Ok(report)
}

/// Cleanup is refused while an independent journal cannot safely be associated with its catalogue.
pub fn assert_cleanup_safe(conn: &Connection, album: i64) -> Result<(), String> {
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
            record,
        };
        output.push((
            id,
            serde_json::to_string_pretty(&journal).map_err(|e| e.to_string())?,
        ));
    }
    Ok(output)
}
