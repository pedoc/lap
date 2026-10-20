/**
 * Database storage, backup, and restore operations.
 * Handles custom DB storage directory, migration, backup/restore of library databases.
 * project: Lap
 * author:  julyx10
 * date:    2026-01-15
 */
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;
use uuid::Uuid;

use crate::t_config::{self, AppConfig, Library};

static DB_MIGRATION_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

// Admission and draining share one lock so a connection cannot slip past migration.
static ACTIVE_CONNECTIONS: Mutex<usize> = Mutex::new(0);
static CONNECTIONS_RETURNED: Condvar = Condvar::new();

pub(crate) struct DbConnectionLease;

impl DbConnectionLease {
    pub(crate) fn acquire() -> Result<Self, String> {
        let mut active = ACTIVE_CONNECTIONS.lock().map_err(|e| e.to_string())?;
        if is_db_migration_in_progress() {
            return Err("Database storage migration is in progress.".into());
        }
        *active += 1;
        Ok(Self)
    }
}

impl Drop for DbConnectionLease {
    fn drop(&mut self) {
        let mut active = ACTIVE_CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
        *active -= 1;
        CONNECTIONS_RETURNED.notify_all();
    }
}

struct DbMigrationGuard;

impl DbMigrationGuard {
    fn acquire() -> Result<Self, String> {
        DB_MIGRATION_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "Database storage migration is already in progress.".to_string())?;
        let guard = Self;
        let active = ACTIVE_CONNECTIONS.lock().map_err(|e| e.to_string())?;
        let (active, _) = CONNECTIONS_RETURNED
            .wait_timeout_while(active, Duration::from_secs(5), |count| *count > 0)
            .map_err(|e| e.to_string())?;
        if *active > 0 {
            return Err("Database is busy. Please retry after current operations finish.".into());
        }
        crate::t_sqlite::clear_conn_pool();
        Ok(guard)
    }
}

impl Drop for DbMigrationGuard {
    fn drop(&mut self) {
        DB_MIGRATION_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

fn get_db_storage_dir_from_config(config: &AppConfig) -> Result<PathBuf, String> {
    if let Some(dir) = config.db_storage_dir.as_deref() {
        let path = PathBuf::from(dir);
        fs::create_dir_all(&path)
            .map_err(|e| format!("Failed to create database storage directory: {}", e))?;
        Ok(path)
    } else {
        t_config::get_libraries_dir()
    }
}

fn get_library_db_path_from_config(config: &AppConfig, library_id: &str) -> Result<String, String> {
    let db_dir = get_db_storage_dir_from_config(config)?;
    Ok(db_dir
        .join(format!("{}.db", library_id))
        .to_string_lossy()
        .into_owned())
}

pub fn get_db_storage_dir() -> Result<String, String> {
    let config = t_config::load_app_config()?;
    get_db_storage_dir_from_config(&config).map(|p| p.to_string_lossy().into_owned())
}

pub fn is_using_custom_db_storage() -> Result<bool, String> {
    let config = t_config::load_app_config()?;
    Ok(config.db_storage_dir.is_some())
}

pub fn is_db_migration_in_progress() -> bool {
    DB_MIGRATION_IN_PROGRESS.load(Ordering::SeqCst)
}

/// Get the database file path for a library
pub fn get_library_db_path(library_id: &str) -> Result<String, String> {
    if is_db_migration_in_progress() {
        return Err("Database storage migration is in progress.".to_string());
    }
    let config = t_config::load_app_config()?;
    get_library_db_path_from_config(&config, library_id)
}

/// Get the current library's database file path
pub fn get_current_db_path() -> Result<String, String> {
    if is_db_migration_in_progress() {
        return Err("Database storage migration is in progress.".to_string());
    }
    let config = t_config::load_app_config()?;
    get_library_db_path_from_config(&config, &config.current_library_id)
}

fn checkpoint_db(path: &Path) -> Result<(), String> {
    // Windows maps a file-as-parent to "not found" in try_exists. Distinguish
    // malformed paths from genuinely absent databases before treating them as safe.
    let mut ancestor=path.parent();
    while let Some(parent)=ancestor {
        if parent.try_exists().map_err(|e|format!("Failed to inspect database parent '{}': {e}",parent.display()))? {
            if !parent.is_dir(){return Err(format!("Database parent is not a directory: {}",parent.display()));}
            break;
        }
        ancestor=parent.parent();
    }
    if !path
        .try_exists()
        .map_err(|e| format!("Failed to inspect database '{}': {}", path.display(), e))?
    {
        return Ok(());
    }

    let conn = rusqlite::Connection::open(path)
        .map_err(|e| format!("Failed to open database for checkpoint: {}", e))?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| format!("Failed to set SQLite busy timeout: {}", e))?;

    let run_checkpoint = |mode: &str| -> Result<(), String> {
        let pragma = format!("PRAGMA wal_checkpoint({})", mode);
        conn.query_row(&pragma, [], |row| row.get::<_, i64>(0))
            .map_err(|e| format!("Failed to checkpoint database with mode {}: {}", mode, e))
            .and_then(|busy| {
                if busy == 0 {
                    Ok(())
                } else {
                    Err("Database checkpoint is busy. Please retry later.".into())
                }
            })
    };

    if let Err(truncate_err) = run_checkpoint("TRUNCATE") {
        eprintln!("{}", truncate_err);
        run_checkpoint("RESTART")?;
    }

    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbStorageChangeResult {
    pub path: String,
    pub cleanup_warnings: Vec<String>,
}

pub fn change_db_storage_dir(new_dir: &str) -> Result<DbStorageChangeResult, String> {
    migrate_db_storage(Some(new_dir))
}

pub fn reset_db_storage_dir() -> Result<DbStorageChangeResult, String> {
    migrate_db_storage(None)
}

fn migrate_db_storage(new_dir: Option<&str>) -> Result<DbStorageChangeResult, String> {
    let _migration_guard = DbMigrationGuard::acquire()?;
    let mut config = t_config::load_app_config()?;
    let target_dir = match new_dir {
        Some(dir) => PathBuf::from(dir),
        None => t_config::get_libraries_dir()?,
    };
    fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create target database directory: {}", e))?;
    let current_dir = get_db_storage_dir_from_config(&config)?;
    let current_dir = fs::canonicalize(&current_dir).map_err(|e| e.to_string())?;
    let target_dir = fs::canonicalize(&target_dir).map_err(|e| e.to_string())?;
    let path = target_dir.to_string_lossy().into_owned();

    let mut migrated_libraries = Vec::new();
    if current_dir != target_dir {
        for library in &config.libraries {
            let source = current_dir.join(format!("{}.db", library.id));
            if !source
                .try_exists()
                .map_err(|e| format!("Failed to inspect database '{}': {}", source.display(), e))?
            {
                continue;
            }
            checkpoint_db(&source)?;
            copy_library_database(&source, &target_dir, &library.id)?;
            migrated_libraries.push(library.clone());
        }
    }
    config.db_storage_dir = new_dir.map(|_| path.clone());
    t_config::save_app_config(&config)?;

    let cleanup_warnings = if current_dir != target_dir {
        cleanup_old_databases(&current_dir, &migrated_libraries)
    } else {
        Vec::new()
    };
    Ok(DbStorageChangeResult {
        path,
        cleanup_warnings,
    })
}

fn copy_library_database(source: &Path, target_dir: &Path, library_id: &str) -> Result<(), String> {
    let target = target_dir.join(format!("{}.db", library_id));
    // Finish copying before replacing the destination; keep the source on every failure.
    let staged = target_dir.join(format!(".{}-{}.tmp", library_id, Uuid::new_v4()));
    let result = (|| {
        fs::copy(source, &staged)
            .map_err(|e| format!("Failed to copy database '{}': {}", source.display(), e))?;
        fs::OpenOptions::new()
            .write(true)
            .open(&staged)
            .and_then(|file| file.sync_all())
            .map_err(|e| format!("Failed to flush database copy: {}", e))?;
        for suffix in [".db-wal", ".db-shm", ".db"] {
            let stale = target_dir.join(format!("{}{}", library_id, suffix));
            match fs::remove_file(&stale) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("Failed to replace '{}': {}", stale.display(), e)),
            }
        }
        fs::rename(&staged, &target)
            .map_err(|e| format!("Failed to install database '{}': {}", target.display(), e))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

fn cleanup_old_databases(directory: &Path, libraries: &[Library]) -> Vec<String> {
    let mut warnings = Vec::new();
    for library in libraries {
        for suffix in [".db", ".db-wal", ".db-shm"] {
            let source = directory.join(format!("{}{}", library.id, suffix));
            if let Err(e) = fs::remove_file(&source) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    let warning = format!("{}: {}", source.display(), e);
                    eprintln!(
                        "Database migrated, but old file cleanup failed: {}",
                        warning
                    );
                    warnings.push(warning);
                }
            }
        }
    }
    warnings
}

// ============================================================================
// Backup / Restore
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbStorageInfo {
    pub library_id: String,
    pub library_name: String,
    pub db_file_size: i64,
}

mod backup;
pub use backup::{
    BackupMetaData, BackupResult, RestoreResult, RestoreSelection, apply_pending_settings,
    backup_databases, parse_backup_file, restore_databases, restored_ui_preferences,
};

pub fn get_db_storage_info() -> Result<Vec<DbStorageInfo>, String> {
    let config = t_config::load_app_config()?;

    let mut results = Vec::new();
    for lib in &config.libraries {
        let db_path = get_library_db_path_from_config(&config, &lib.id)?;
        let db_file_size = {
            let p = Path::new(&db_path);
            if p.exists() {
                fs::metadata(p).ok().map(|m| m.len() as i64).unwrap_or(0)
            } else {
                0
            }
        };

        results.push(DbStorageInfo {
            library_id: lib.id.clone(),
            library_name: lib.name.clone(),
            db_file_size,
        });
    }

    Ok(results)
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("lap-storage-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn migration_waits_for_connections_and_reopens_admission() {
        let lease = DbConnectionLease::acquire().unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _guard = DbMigrationGuard::acquire().unwrap();
            ready_tx.send(()).unwrap();
            finish_rx.recv().unwrap();
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !is_db_migration_in_progress() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(DbConnectionLease::acquire().is_err());
        assert!(ready_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(lease);
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(DbConnectionLease::acquire().is_err());
        finish_tx.send(()).unwrap();
        worker.join().unwrap();
        let lease = DbConnectionLease::acquire().unwrap();
        // A timed-out migration must also reopen admission without touching storage.
        assert!(DbMigrationGuard::acquire().is_err());
        assert!(!is_db_migration_in_progress());
        drop(lease);
        assert!(DbConnectionLease::acquire().is_ok());
    }

    #[test]
    fn checkpoint_rejects_busy_database_then_copies_committed_data() {
        let source_dir = TestDir::new();
        let target_dir = TestDir::new();
        let source = source_dir.0.join("default.db");
        let writer = rusqlite::Connection::open(&source).unwrap();
        writer
            .execute_batch(
                "PRAGMA journal_mode=WAL; CREATE TABLE test(value); INSERT INTO test VALUES (1);",
            )
            .unwrap();
        let reader = rusqlite::Connection::open(&source).unwrap();
        reader.execute_batch("BEGIN; SELECT * FROM test;").unwrap();
        writer.execute("INSERT INTO test VALUES (2)", []).unwrap();
        assert!(checkpoint_db(&source).is_err());
        reader.execute_batch("ROLLBACK").unwrap();
        checkpoint_db(&source).unwrap();
        fs::write(target_dir.0.join("default.db-wal"), "stale").unwrap();
        copy_library_database(&source, &target_dir.0, "default").unwrap();
        assert!(source.exists());
        assert!(!target_dir.0.join("default.db-wal").exists());
        let copy = rusqlite::Connection::open(target_dir.0.join("default.db")).unwrap();
        assert_eq!(
            copy.query_row("SELECT COUNT(*) FROM test", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

    #[test]
    fn copy_failure_keeps_source_and_cleanup_failure_is_reported() {
        let source_dir = TestDir::new();
        let target_dir = TestDir::new();
        let source = source_dir.0.join("default.db");
        fs::write(&source, "source data").unwrap();
        // A metadata error (a file used as a directory) must not mean "missing".
        assert!(checkpoint_db(&source.join("default.db")).is_err());
        fs::create_dir(target_dir.0.join("default.db-wal")).unwrap();
        assert!(copy_library_database(&source, &target_dir.0, "default").is_err());
        assert_eq!(fs::read_to_string(&source).unwrap(), "source data");
        assert_eq!(fs::read_dir(&target_dir.0).unwrap().count(), 1);
        let libraries = AppConfig::default().libraries;
        let warnings = cleanup_old_databases(&target_dir.0, &libraries);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("default.db-wal"));
        assert!(cleanup_old_databases(&source_dir.0, &libraries).is_empty());
        assert!(!source.exists());
    }
}
