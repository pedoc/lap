//! Versioned, streaming catalogue backups. Media files are never modified here.
use super::{DbMigrationGuard, get_db_storage_dir_from_config, get_library_db_path_from_config};
use crate::t_config::{self, AppConfig, Library};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use uuid::Uuid;
use zip::{ZipWriter, read::ZipArchive, write::SimpleFileOptions};

const VERSION: u32 = 2;
const JSON_LIMIT: u64 = 8 * 1024 * 1024;
const PENDING: &str = "pending-settings-restore.json";
const UI_READY: &str = "restored-ui-preferences.json";
static SETTINGS_RESTORE_ERROR: Mutex<Option<String>> = Mutex::new(None);
const CONFIG_FILES: &[&str] = &[
    "app-config.json",
    "ai-config.json",
    "ai-services.json",
    "ocr-automation.json",
    "objects-automation.json",
    "private-lock.json",
    "ui-preferences.json",
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub file_path: String,
    pub file_size: i64,
    pub library_count: usize,
    pub library_names: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupMetaLibrary {
    pub name: String,
    pub db_size: i64,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub entry: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupMetaData {
    #[serde(default = "legacy_version")]
    pub format_version: u32,
    #[serde(default)]
    pub app_version: String,
    pub created_at: i64,
    pub libraries: Vec<BackupMetaLibrary>,
    #[serde(default)]
    pub includes_media: bool,
    #[serde(default)]
    pub recovery_reports: Vec<String>,
    #[serde(default)]
    pub files: BTreeMap<String, FileDigest>,
    #[serde(default)]
    pub config_files: Vec<String>,
}
fn legacy_version() -> u32 {
    1
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileDigest {
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSelection {
    pub library_name: String,
    #[serde(default)]
    pub entry: Option<String>,
    // Kept for older clients. All name conflicts are resolved on the backend.
    #[serde(default, rename = "shouldRename")]
    pub _should_rename: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub restored_count: usize,
    pub restored_names: Vec<String>,
    pub warnings: Vec<String>,
    pub restart_required: bool,
}

/// Only this newly-created directory is removed on drop. Rollback copies live
/// beside their targets, so a failed rollback never loses the original files.
struct Workspace(PathBuf);
impl Workspace {
    fn new(parent: &Path) -> Result<Self, String> {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let path = parent.join(format!(".lap-backup-{}", Uuid::new_v4()));
        fs::create_dir(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct InstalledFile {
    target: PathBuf,
    previous: Option<PathBuf>,
    installed: bool,
}
#[derive(Default)]
struct FileTransaction {
    installed: Vec<InstalledFile>,
}
impl FileTransaction {
    fn install(&mut self, source: &Path, target: &Path, replace: bool) -> Result<(), String> {
        let previous = match fs::symlink_metadata(target) {
            Ok(meta) => {
                if !replace || !meta.file_type().is_file() {
                    return Err(format!("Refusing to replace {}", target.display()));
                }
                let previous = target.with_file_name(format!(".lap-rollback-{}", Uuid::new_v4()));
                // Keep the live file in place until the replacement is ready.
                // A hard link avoids copying large existing ZIPs on NTFS/APFS;
                // other filesystems use an ordinary, flushed rollback copy.
                if fs::hard_link(target, &previous).is_err() {
                    fs::copy(target, &previous).map_err(|e| e.to_string())?;
                    if let Err(error) = sync_file(&previous) {
                        let _ = fs::remove_file(&previous);
                        return Err(error);
                    }
                }
                Some(previous)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        self.installed.push(InstalledFile {
            target: target.into(),
            previous,
            installed: false,
        });
        if self
            .installed
            .last()
            .is_some_and(|file| file.previous.is_none())
        {
            // Reserve a fresh UUID destination with create_new, so even a
            // collision can never overwrite an existing library database.
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)
                .map_err(|e| e.to_string())?;
            if let Some(file) = self.installed.last_mut() {
                file.installed = true;
            }
        }
        fs::rename(source, target)
            .map_err(|e| format!("Cannot install {}: {e}", target.display()))?;
        if let Some(file) = self.installed.last_mut() {
            file.installed = true;
        }
        Ok(())
    }
    fn rollback(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        for file in self.installed.drain(..).rev() {
            if let Some(previous) = file.previous {
                let result = if file.installed {
                    fs::rename(&previous, &file.target)
                } else {
                    fs::remove_file(&previous)
                };
                if let Err(error) = result {
                    warnings.push(format!(
                        "Original retained at {}: {error}",
                        previous.display()
                    ));
                }
            } else if file.installed {
                if let Err(error) = fs::remove_file(&file.target) {
                    warnings.push(format!(
                        "Cannot roll back {}: {error}",
                        file.target.display()
                    ));
                }
            }
        }
        warnings
    }
    fn commit(mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        for file in self.installed.drain(..) {
            if let Some(previous) = file.previous {
                if let Err(error) = fs::remove_file(&previous) {
                    warnings.push(format!(
                        "Old file retained at {}: {error}",
                        previous.display()
                    ));
                }
            }
        }
        warnings
    }
    fn fail(&mut self, error: String) -> String {
        let warnings = self.rollback();
        if warnings.is_empty() {
            error
        } else {
            format!("{error}\n{}", warnings.join("\n"))
        }
    }
}
impl Drop for FileTransaction {
    fn drop(&mut self) {
        for warning in self.rollback() {
            eprintln!("{warning}");
        }
    }
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}

fn sync_file(path: &Path) -> Result<(), String> {
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())
}

fn transfer(mut input: impl Read, mut output: impl Write) -> Result<FileDigest, String> {
    let mut buffer = [0u8; 64 * 1024];
    let mut hash = Sha256::new();
    let mut size = 0;
    loop {
        let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        hash.update(&buffer[..count]);
        size += count as u64;
    }
    Ok(FileDigest {
        size,
        sha256: format!("{:x}", hash.finalize()),
    })
}

fn read_json(archive: &mut ZipArchive<File>, name: &str) -> Result<Value, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|e| format!("Missing {name}: {e}"))?;
    if entry.size() > JSON_LIMIT {
        return Err(format!("{name} exceeds the configuration size limit"));
    }
    let mut bytes = Vec::new();
    (&mut entry)
        .take(JSON_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > JSON_LIMIT {
        return Err(format!("{name} is too large"));
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid {name}: {e}"))
}

fn safe_entry(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['\\', ':', '\0'])
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn metadata(archive: &mut ZipArchive<File>) -> Result<BackupMetaData, String> {
    if archive.len() > 100_000 {
        return Err("Too many files in backup".into());
    }
    let mut names = HashSet::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|e| e.to_string())?;
        if !safe_entry(entry.name()) || !names.insert(entry.name().to_string()) {
            return Err("Unsafe or duplicate backup entry".into());
        }
    }
    let mut meta: BackupMetaData = serde_json::from_value(read_json(archive, "backup-info.json")?)
        .map_err(|e| format!("Invalid backup metadata: {e}"))?;
    if !(1..=VERSION).contains(&meta.format_version) || meta.includes_media {
        return Err("Unsupported backup format".into());
    }
    if meta.libraries.is_empty() || meta.libraries.len() > 10_000 {
        return Err("Invalid library list".into());
    }
    let mut libraries = HashSet::new();
    for lib in &mut meta.libraries {
        if meta.format_version == 1 {
            lib.entry = format!("{}.db", sanitize_filename(&lib.name));
        }
        if lib.name.trim().is_empty()
            || lib.db_size < 0
            || !safe_entry(&lib.entry)
            || !libraries.insert(lib.entry.clone())
            || !names.contains(&lib.entry)
        {
            return Err(format!(
                "Missing or ambiguous library database: {}",
                lib.name
            ));
        }
    }
    if meta.format_version == VERSION {
        if meta.files.len() + 1 != names.len()
            || !meta.files.keys().all(|name| names.contains(name))
            || meta.files.contains_key("backup-info.json")
        {
            return Err("Backup file manifest does not match the archive".into());
        }
        for (name, digest) in &meta.files {
            let entry = archive.by_name(name).map_err(|e| e.to_string())?;
            if digest.size != entry.size()
                || digest.sha256.len() != 64
                || !digest.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err(format!("Invalid backup checksum metadata: {name}"));
            }
        }
        for lib in &meta.libraries {
            if meta.files.get(&lib.entry).map(|file| file.size) != Some(lib.db_size as u64) {
                return Err(format!("Invalid database size: {}", lib.name));
            }
        }
    }
    // Derive this from actual entries; never trust a UI availability flag.
    meta.config_files = CONFIG_FILES
        .iter()
        .filter(|name| names.contains(**name))
        .map(|s| s.to_string())
        .collect();
    Ok(meta)
}

fn open_archive(path: &Path) -> Result<(ZipArchive<File>, BackupMetaData), String> {
    let file = File::open(path).map_err(|e| format!("Cannot open backup: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid backup ZIP: {e}"))?;
    let meta = metadata(&mut archive)?;
    Ok((archive, meta))
}

pub fn parse_backup_file(path: &str) -> Result<BackupMetaData, String> {
    open_archive(Path::new(path)).map(|(_, meta)| meta)
}

fn validate_database(path: &Path) -> Result<(), String> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let integrity: String = conn
        .query_row("PRAGMA integrity_check(1)", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if integrity != "ok" {
        return Err(format!("Database integrity check failed: {integrity}"));
    }
    let version: i32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version > crate::t_migration::latest_schema_version() {
        return Err("This database requires a newer version of Lap".into());
    }
    for sql in [
        "SELECT id,path FROM albums LIMIT 0",
        "SELECT id,path FROM afolders LIMIT 0",
        "SELECT id,folder_id,name FROM afiles LIMIT 0",
    ] {
        conn.prepare(sql)
            .map_err(|e| format!("Invalid Lap database: {e}"))?;
    }
    Ok(())
}

fn add_entry(
    zip: &mut ZipWriter<File>,
    files: &mut BTreeMap<String, FileDigest>,
    name: &str,
    input: impl Read,
) -> Result<(), String> {
    zip.start_file(
        name,
        SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(true),
    )
    .map_err(|e| e.to_string())?;
    let digest = transfer(input, zip)?;
    if files.insert(name.into(), digest).is_some() {
        return Err("Duplicate backup entry".into());
    }
    Ok(())
}

pub fn backup_databases(
    library_ids: &[String],
    dest_path: &str,
    preferences: Option<Value>,
) -> Result<BackupResult, String> {
    let _guard = DbMigrationGuard::acquire()?;
    create_backup(
        &t_config::load_app_config()?,
        &t_config::get_app_data_dir()?,
        library_ids,
        Path::new(dest_path),
        preferences,
    )
}

fn create_backup(
    config: &AppConfig,
    app_dir: &Path,
    ids: &[String],
    destination: &Path,
    preferences: Option<Value>,
) -> Result<BackupResult, String> {
    if !destination
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        return Err("Choose a .zip file for the backup".into());
    }
    let mut selected = Vec::new();
    let mut unique = HashSet::new();
    for id in ids {
        if !unique.insert(id) {
            continue;
        }
        selected.push(
            config
                .libraries
                .iter()
                .find(|lib| &lib.id == id)
                .ok_or_else(|| format!("Unknown library: {id}"))?,
        );
    }
    if selected.is_empty() {
        return Err("No libraries selected for backup".into());
    }
    let workspace = Workspace::new(
        destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let staged = workspace.0.join("backup.zip");
    let mut zip = ZipWriter::new(File::create(&staged).map_err(|e| e.to_string())?);
    let mut meta = BackupMetaData {
        format_version: VERSION,
        app_version: env!("CARGO_PKG_VERSION").into(),
        created_at: chrono::Utc::now().timestamp(),
        libraries: Vec::new(),
        includes_media: false,
        recovery_reports: Vec::new(),
        files: BTreeMap::new(),
        config_files: Vec::new(),
    };
    for (index, library) in selected.iter().enumerate() {
        let source = get_library_db_path_from_config(config, &library.id)?;
        // VACUUM INTO produces a consistent standalone snapshot including WAL
        // commits without changing or truncating the original database.
        let conn = Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| format!("Cannot back up {}: {e}", library.name))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let snapshot = workspace.0.join("snapshot.db");
        conn.execute("VACUUM INTO ?1", [snapshot.to_string_lossy().as_ref()])
            .map_err(|e| e.to_string())?;
        drop(conn);
        validate_database(&snapshot)?;
        let conn = Connection::open_with_flags(&snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
        let report = crate::t_media_recovery::inspect(&conn, false)?;
        meta.recovery_reports.extend(
            report
                .issues
                .into_iter()
                .map(|issue| format!("{}: {issue}", library.name)),
        );
        for (journal_id, journal) in crate::t_media_recovery::backup_journals(&conn)? {
            if !safe_entry(&journal_id) || journal_id.contains('/') {
                return Err("Invalid recovery journal identity".into());
            }
            add_entry(
                &mut zip,
                &mut meta.files,
                &format!("recovery/{index}/{journal_id}.json"),
                journal.as_bytes(),
            )?;
        }
        drop(conn);
        let entry = format!("libraries/{index}.db");
        add_entry(
            &mut zip,
            &mut meta.files,
            &entry,
            File::open(&snapshot).map_err(|e| e.to_string())?,
        )?;
        meta.libraries.push(BackupMetaLibrary {
            name: library.name.clone(),
            id: library.id.clone(),
            db_size: meta.files[&entry].size as i64,
            entry,
        });
        fs::remove_file(snapshot).map_err(|e| e.to_string())?;
    }
    for name in CONFIG_FILES {
        let bytes = if *name == "app-config.json" {
            // Exclude references to databases the user did not select.
            let mut snapshot = config.clone();
            snapshot.libraries = selected.iter().map(|lib| (*lib).clone()).collect();
            serde_json::to_vec_pretty(&snapshot).map_err(|e| e.to_string())?
        } else if *name == "ui-preferences.json" {
            let Some(value) = preferences.as_ref() else {
                continue;
            };
            serde_json::to_vec(value).map_err(|e| e.to_string())?
        } else {
            match fs::read(app_dir.join(name)) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(format!("Cannot read {name}: {e}")),
            }
        };
        if bytes.len() as u64 > JSON_LIMIT {
            return Err(format!("{name} is too large"));
        }
        // Keep a backup of the actual settings. Full semantic validation is
        // applied only when the user opts into restoring these settings.
        serde_json::from_slice::<Value>(&bytes).map_err(|e| format!("Invalid {name}: {e}"))?;
        add_entry(&mut zip, &mut meta.files, name, bytes.as_slice())?;
        meta.config_files.push(name.to_string());
    }
    zip.start_file(
        "backup-info.json",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
    )
    .map_err(|e| e.to_string())?;
    zip.write_all(&serde_json::to_vec_pretty(&meta).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let file = zip.finish().map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    drop(file);
    // Reopen the completed ZIP before publishing it. A failed backup leaves a
    // previously existing destination intact.
    open_archive(&staged)?;
    let mut transaction = FileTransaction::default();
    if let Err(error) = transaction.install(&staged, destination, true) {
        return Err(transaction.fail(error));
    }
    let mut warnings = meta.recovery_reports;
    warnings.extend(transaction.commit());
    Ok(BackupResult {
        file_path: destination.to_string_lossy().into_owned(),
        file_size: size as i64,
        library_count: selected.len(),
        library_names: selected.iter().map(|lib| lib.name.clone()).collect(),
        warnings,
    })
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingSettings {
    version: u32,
    files: BTreeMap<String, Value>,
}

fn validate_setting(name: &str, value: &Value) -> Result<(), String> {
    let source = serde_json::to_string(value).map_err(|e| e.to_string())?;
    if source.len() as u64 > JSON_LIMIT || !value.is_object() {
        return Err(format!("Invalid {name}"));
    }
    match name {
        "app-config.json" => {
            let config: AppConfig =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            if let Some(settings) = config.map_services {
                settings.normalize()?;
            }
            Ok(())
        }
        "ai-config.json" => crate::ai::settings::validate_backup(&source),
        "ai-services.json" => {
            serde_json::from_value::<crate::ai::services::config::Configuration>(value.clone())
                .map_err(|e| e.to_string())?
                .validate()
        }
        "private-lock.json" => crate::t_private_lock::validate_backup(&source),
        "ocr-automation.json" => crate::ai::ocr_auto::validate(
            &serde_json::from_value(value.clone()).map_err(|e| e.to_string())?,
        ),
        "objects-automation.json" => crate::ai::objects_automation::validate(
            &serde_json::from_value(value.clone()).map_err(|e| e.to_string())?,
        ),
        "ui-preferences.json" => Ok(()),
        _ => Err("Unsupported configuration file".into()),
    }
}

fn restore_settings_payload(
    archive: &mut ZipArchive<File>,
    meta: &BackupMetaData,
    library_ids: &BTreeMap<String, String>,
) -> Result<PendingSettings, String> {
    let mut files = BTreeMap::new();
    for name in &meta.config_files {
        let mut value = read_json(archive, name)?;
        if name.ends_with("-automation.json") {
            // Restored libraries receive fresh IDs. A scope referencing an
            // unselected library must never accidentally target another one.
            if let Some(old) = value.pointer("/scope/libraryId").and_then(Value::as_str) {
                if let Some(id) = library_ids.get(old) {
                    value["scope"]["libraryId"] = id.clone().into();
                } else {
                    value["scope"] = Value::Null;
                    value["enabled"] = false.into();
                }
            }
            value["paused"] = true.into();
            value["revision"] = Uuid::new_v4().to_string().into();
            value["lastError"] = Value::Null;
        }
        validate_setting(name, &value)?;
        files.insert(name.clone(), value);
    }
    if files.is_empty() {
        return Err("This backup contains no application settings".into());
    }
    Ok(PendingSettings { version: 1, files })
}

pub fn restore_databases(
    backup_path: &str,
    selections: &[RestoreSelection],
    restore_settings: bool,
) -> Result<RestoreResult, String> {
    let _guard = DbMigrationGuard::acquire()?;
    let app_dir = t_config::get_app_data_dir()?;
    restore_to(
        Path::new(backup_path),
        selections,
        restore_settings,
        t_config::load_app_config()?,
        &t_config::get_config_file_path()?,
        &app_dir,
        true,
    )
}

fn restore_to(
    backup: &Path,
    selections: &[RestoreSelection],
    restore_settings: bool,
    config: AppConfig,
    config_path: &Path,
    app_dir: &Path,
    lock_configuration: bool,
) -> Result<RestoreResult, String> {
    if selections.is_empty() {
        return Err("No libraries selected for restore".into());
    }
    if restore_settings
        && app_dir
            .join(PENDING)
            .try_exists()
            .map_err(|e| e.to_string())?
    {
        return Err("Restart Lap to apply the previous settings restore first".into());
    }
    let (mut archive, meta) = open_archive(backup)?;
    let target_dir = get_db_storage_dir_from_config(&config)?;
    let workspace = Workspace::new(&target_dir)?;
    let mut selected = BTreeMap::new();
    for selection in selections {
        let matches: Vec<_> = meta
            .libraries
            .iter()
            .filter(|lib| {
                lib.name == selection.library_name
                    && selection
                        .entry
                        .as_ref()
                        .is_none_or(|entry| entry == &lib.entry)
            })
            .collect();
        if matches.len() != 1 || selected.contains_key(&matches[0].entry) {
            return Err("Invalid or ambiguous restore selection".into());
        }
        selected.insert(matches[0].entry.clone(), matches[0]);
    }
    // Stream every payload to EOF to check ZIP CRC and, for v2, SHA-256. Only
    // selected databases are written to disk. Memory stays bounded by a buffer.
    let mut databases = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if name == "backup-info.json" {
            continue;
        }
        let digest = if selected.contains_key(&name) {
            let path = workspace.0.join(format!("{}.db", Uuid::new_v4()));
            let mut output = File::create(&path).map_err(|e| e.to_string())?;
            let digest = transfer(&mut entry, &mut output)?;
            output.sync_all().map_err(|e| e.to_string())?;
            databases.insert(name.clone(), path);
            digest
        } else {
            transfer(&mut entry, std::io::sink())?
        };
        if meta.format_version == VERSION && meta.files.get(&name) != Some(&digest) {
            return Err(format!("Backup checksum mismatch: {name}"));
        }
    }
    let archived_config: Option<AppConfig> =
        if meta.config_files.iter().any(|n| n == "app-config.json") {
            Some(
                serde_json::from_value(read_json(&mut archive, "app-config.json")?)
                    .map_err(|e| format!("Invalid app-config.json: {e}"))?,
            )
        } else {
            None
        };
    let mut new_libraries = Vec::new();
    let mut warnings = meta.recovery_reports.clone();
    if meta.format_version == 1 {
        warnings.push("Legacy backup: SHA-256 checksums are unavailable; ZIP CRC and database integrity were checked.".into());
    }
    let mut library_ids = BTreeMap::new();
    let mut staged = Vec::new();
    for (entry, lib) in selected {
        let path = databases
            .get(&entry)
            .ok_or("Selected database is missing")?;
        validate_database(path).map_err(|e| format!("{}: {e}", lib.name))?;
        {
            let conn = Connection::open(path).map_err(|e| e.to_string())?;
            crate::t_media_lifecycle::setup(&conn)?;
            let report = crate::t_media_recovery::inspect(&conn, true)?;
            warnings.extend(
                report
                    .issues
                    .into_iter()
                    .map(|issue| format!("{}: {issue}", lib.name)),
            );
            conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|e| e.to_string())?;
            conn.query_row("PRAGMA journal_mode=DELETE", [], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| e.to_string())?;
        }
        sync_file(path)?;
        let original = archived_config.as_ref().and_then(|cfg| {
            cfg.libraries.iter().find(|old| {
                if lib.id.is_empty() {
                    old.name == lib.name
                } else {
                    old.id == lib.id
                }
            })
        });
        let id = Uuid::new_v4().to_string();
        if let Some(old) = original {
            library_ids.insert(old.id.clone(), id.clone());
        }
        let mut state = original.map(|lib| lib.state.clone()).unwrap_or_default();
        // A snapshot must not resume an interrupted indexing job automatically.
        state.index.status = 0;
        state.index.album_queue.clear();
        new_libraries.push(Library {
            id: id.clone(),
            name: lib.name.clone(),
            created_at: original
                .map_or_else(|| chrono::Utc::now().timestamp(), |lib| lib.created_at),
            state,
            hidden: original.is_some_and(|lib| lib.hidden),
        });
        staged.push((path.clone(), target_dir.join(format!("{id}.db"))));
    }
    let pending = if restore_settings {
        Some(restore_settings_payload(&mut archive, &meta, &library_ids)?)
    } else {
        None
    };
    // All parsing, validation and database repairs finish before any live file
    // is installed. The catalogue is installed last and rollback removes new DBs.
    let app_workspace = Workspace::new(app_dir)?;
    let pending_path = app_workspace.0.join(PENDING);
    if let Some(pending) = &pending {
        write_synced(
            &pending_path,
            &serde_json::to_vec(pending).map_err(|e| e.to_string())?,
        )?;
    }
    let publish = |mut current: AppConfig, config_path: &Path| -> Result<_, String> {
        if current.db_storage_dir != config.db_storage_dir {
            return Err("Database storage location changed during restore; please retry".into());
        }
        // Only this short commit holds the config lock. UI state saves during
        // decompression are preserved, and no synchronous IPC waits on a long
        // checksum or database integrity scan.
        let mut names: HashSet<_> = current
            .libraries
            .iter()
            .map(|lib| lib.name.clone())
            .collect();
        let mut restored_names = Vec::new();
        for library in &new_libraries {
            let mut library = library.clone();
            library.name = unique_name(&library.name, &names);
            names.insert(library.name.clone());
            restored_names.push(library.name.clone());
            current.libraries.push(library);
        }
        let local_config = app_workspace.0.join("app-config.json");
        write_synced(
            &local_config,
            &serde_json::to_vec_pretty(&current).map_err(|e| e.to_string())?,
        )?;
        let mut transaction = FileTransaction::default();
        let result = (|| {
            for (source, target) in &staged {
                transaction.install(source, target, false)?;
            }
            if pending.is_some() {
                transaction.install(&pending_path, &app_dir.join(PENDING), false)?;
            }
            transaction.install(&local_config, config_path, true)
        })();
        if let Err(error) = result {
            return Err(transaction.fail(error));
        }
        Ok((restored_names, transaction.commit()))
    };
    let (restored_names, cleanup_warnings) = if lock_configuration {
        t_config::with_locked_app_config(|current, path| publish(current, &path))?
    } else {
        publish(config.clone(), config_path)?
    };
    warnings.extend(cleanup_warnings);
    Ok(RestoreResult {
        restored_count: restored_names.len(),
        restored_names,
        warnings,
        restart_required: pending.is_some(),
    })
}

fn merge_global_settings(current: &mut AppConfig, archived: AppConfig) {
    current.debug = archived.debug;
    current.network_proxy_url = archived.network_proxy_url;
    current.map_services = archived.map_services;
    // Database location, active library and existing catalogue stay local.
}

/// Called before creating any windows, AI sessions or automation workers. Files
/// are staged during restore so cached settings cannot overwrite the restored
/// configuration while the old session is still running.
pub fn apply_pending_settings() -> Result<(), String> {
    let app_dir = t_config::get_app_data_dir()?;
    let result = t_config::with_locked_app_config(|config, config_path| {
        apply_pending_to(&app_dir, config, &config_path)
    });
    if let Ok(mut error) = SETTINGS_RESTORE_ERROR.lock() {
        *error = result.as_ref().err().cloned();
    }
    result
}

fn apply_pending_to(
    app_dir: &Path,
    mut config: AppConfig,
    config_path: &Path,
) -> Result<(), String> {
    let path = app_dir.join(PENDING);
    if !path.try_exists().map_err(|e| e.to_string())? {
        return Ok(());
    }
    let file = File::open(&path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > JSON_LIMIT * CONFIG_FILES.len() as u64 {
        return Err("Pending settings restore is too large".into());
    }
    let pending: PendingSettings = serde_json::from_reader(file).map_err(|e| e.to_string())?;
    if pending.version != 1 || pending.files.len() > CONFIG_FILES.len() {
        return Err("Unsupported pending settings restore".into());
    }
    for (name, value) in &pending.files {
        validate_setting(name, value)?;
    }
    let workspace = Workspace::new(app_dir)?;
    let mut staged = Vec::new();
    for (name, value) in pending.files {
        if name == "app-config.json" {
            merge_global_settings(
                &mut config,
                serde_json::from_value(value).map_err(|e| e.to_string())?,
            );
        } else {
            let target = if name == "ui-preferences.json" {
                UI_READY
            } else {
                &name
            };
            let value = if name == "ui-preferences.json" {
                serde_json::json!({"revision":Uuid::new_v4().to_string(), "preferences":value})
            } else {
                value
            };
            let source = workspace.0.join(target);
            write_synced(
                &source,
                &serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
            )?;
            staged.push((source, app_dir.join(target)));
        }
    }
    let source = workspace.0.join("app-config.json");
    write_synced(
        &source,
        &serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?,
    )?;
    staged.push((source, config_path.into()));
    let mut transaction = FileTransaction::default();
    let result = (|| {
        for (source, target) in staged {
            transaction.install(&source, &target, true)?;
        }
        // Consuming the pending marker is also rollback-safe.
        fs::rename(&path, workspace.0.join("applied-settings.json")).map_err(|e| e.to_string())
    })();
    if let Err(error) = result {
        return Err(transaction.fail(error));
    }
    for warning in transaction.commit() {
        eprintln!("{warning}");
    }
    Ok(())
}

pub fn restored_ui_preferences() -> Result<Option<Value>, String> {
    if let Some(error) = SETTINGS_RESTORE_ERROR
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
    {
        return Err(format!("Settings restore failed: {error}"));
    }
    let path = t_config::get_app_data_dir()?.join(UI_READY);
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if file.metadata().map_err(|e| e.to_string())?.len() > JSON_LIMIT {
        return Err("Restored UI preferences are too large".into());
    }
    serde_json::from_reader(file)
        .map(Some)
        .map_err(|e| e.to_string())
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_ .".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}
fn unique_name(name: &str, existing: &HashSet<String>) -> String {
    if !existing.contains(name) {
        return name.into();
    }
    for index in 1..=10_000 {
        let candidate = format!("{name} ({index})");
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
    format!("{name} ({})", Uuid::new_v4())
}

#[cfg(test)]
mod tests;
