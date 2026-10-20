use super::*;

struct Fixture {
    root: Workspace,
    app: PathBuf,
    databases: PathBuf,
    config: AppConfig,
    config_path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = Workspace::new(&std::env::temp_dir()).unwrap();
        let app = root.0.join("app");
        let databases = root.0.join("databases");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(&databases).unwrap();
        let mut config = AppConfig::default();
        config.db_storage_dir = Some(databases.to_string_lossy().into_owned());
        config.libraries[0].name = "旅行/照片".into();
        config.libraries[0].state.album.selected = true;
        config.libraries[0].hidden = true;
        let config_path = app.join("app-config.json");
        fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        let conn = Connection::open(databases.join("default.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT);
            CREATE TABLE afolders(id INTEGER PRIMARY KEY,path TEXT);
            CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,name TEXT);
            INSERT INTO afiles VALUES(1,1,'photo.jpg');",
        )
        .unwrap();
        Self {
            root,
            app,
            databases,
            config,
            config_path,
        }
    }
    fn archive(&self) -> PathBuf {
        self.root.0.join("backup.zip")
    }
    fn backup(&self) -> BackupResult {
        create_backup(
            &self.config,
            &self.app,
            &["default".into()],
            &self.archive(),
            Some(serde_json::json!({"settings":{"language":"zh"}})),
        )
        .unwrap()
    }
    fn selection(&self) -> Vec<RestoreSelection> {
        vec![RestoreSelection {
            library_name: "旅行/照片".into(),
            entry: None,
            _should_rename: false,
        }]
    }
    fn restore(&self, settings: bool) -> Result<RestoreResult, String> {
        restore_to(
            &self.archive(),
            &self.selection(),
            settings,
            self.config.clone(),
            &self.config_path,
            &self.app,
            false,
        )
    }
    fn saved(&self) -> AppConfig {
        serde_json::from_slice(&fs::read(&self.config_path).unwrap()).unwrap()
    }
}

fn rewrite_zip(path: &Path, edit: impl FnOnce(&mut BTreeMap<String, Vec<u8>>)) {
    let mut archive = ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut contents = BTreeMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        contents.insert(entry.name().into(), bytes);
    }
    drop(archive);
    edit(&mut contents);
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in contents {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn round_trip_includes_wal_and_preserves_existing_libraries_and_saved_state() {
    let fixture = Fixture::new();
    // Keep WAL uncheckpointed while backup takes the SQLite snapshot.
    let conn = Connection::open(fixture.databases.join("default.db")).unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; INSERT INTO afiles VALUES(2,1,'wal.jpg');")
        .unwrap();
    let result = fixture.backup();
    assert_eq!(result.library_count, 1);
    assert!(result.file_size > 0);
    let meta = parse_backup_file(fixture.archive().to_str().unwrap()).unwrap();
    assert_eq!(meta.format_version, 2);
    assert!(meta.config_files.contains(&"ui-preferences.json".into()));
    assert!(!meta.includes_media);
    let result = fixture.restore(false).unwrap();
    assert_eq!(result.restored_count, 1);
    assert_eq!(result.restored_names, ["旅行/照片 (1)"]);
    assert!(!result.restart_required);
    let config = fixture.saved();
    assert_eq!(config.libraries.len(), 2);
    assert_eq!(config.current_library_id, "default");
    assert_eq!(config.db_storage_dir, fixture.config.db_storage_dir);
    assert!(config.libraries[1].state.album.selected);
    assert!(config.libraries[1].hidden);
    assert_ne!(config.libraries[1].id, "default");
    let restored = Connection::open(
        fixture
            .databases
            .join(format!("{}.db", config.libraries[1].id)),
    )
    .unwrap();
    assert_eq!(
        restored
            .query_row("SELECT COUNT(*) FROM afiles", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM afiles", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert!(!fixture.app.join(PENDING).exists());
}

#[test]
fn settings_are_opt_in_staged_until_startup_and_remap_automation_scope() {
    let mut fixture = Fixture::new();
    fixture.config.debug = true;
    fs::write(
        fixture.app.join("ai-config.json"),
        serde_json::to_vec(&crate::ai::settings::Configuration::default()).unwrap(),
    )
    .unwrap();
    fs::write(
        fixture.app.join("private-lock.json"),
        b"{\"version\":1,\"idleMinutes\":5}",
    )
    .unwrap();
    fs::write(fixture.app.join("ocr-automation.json"), br#"{"enabled":true,"paused":false,"scope":{"libraryId":"default","folderId":1,"label":"folder"}}"#).unwrap();
    fixture.backup();
    fixture.config.debug = false;
    fs::write(fixture.app.join("ocr-automation.json"), b"{}").unwrap();
    let result = fixture.restore(true).unwrap();
    assert!(result.restart_required);
    assert_eq!(
        fs::read(fixture.app.join("ocr-automation.json")).unwrap(),
        b"{}"
    );
    assert!(!fixture.saved().debug);
    assert!(fixture.restore(true).unwrap_err().contains("Restart Lap"));
    let saved = fixture.saved();
    apply_pending_to(&fixture.app, saved.clone(), &fixture.config_path).unwrap();
    assert!(!fixture.app.join(PENDING).exists());
    let applied = fixture.saved();
    assert!(applied.debug);
    assert_eq!(applied.current_library_id, "default");
    assert_eq!(applied.libraries.len(), 2);
    assert_eq!(applied.db_storage_dir, fixture.config.db_storage_dir);
    let policy: Value =
        serde_json::from_slice(&fs::read(fixture.app.join("ocr-automation.json")).unwrap())
            .unwrap();
    assert_eq!(policy["scope"]["libraryId"], saved.libraries[1].id);
    assert_eq!(policy["paused"], true);
    let ui: Value = serde_json::from_slice(&fs::read(fixture.app.join(UI_READY)).unwrap()).unwrap();
    assert_eq!(ui["preferences"]["settings"]["language"], "zh");
    // A second startup must not reapply a settings restore.
    apply_pending_to(&fixture.app, applied, &fixture.config_path).unwrap();
}

#[test]
fn checksum_failure_leaves_catalogue_databases_and_settings_untouched() {
    let fixture = Fixture::new();
    fixture.backup();
    let before = fs::read(&fixture.config_path).unwrap();
    rewrite_zip(&fixture.archive(), |files| {
        files.get_mut("libraries/0.db").unwrap()[200] ^= 1;
    });
    assert!(
        fixture
            .restore(true)
            .unwrap_err()
            .contains("checksum mismatch")
    );
    assert_eq!(fs::read(&fixture.config_path).unwrap(), before);
    assert_eq!(fs::read_dir(&fixture.databases).unwrap().count(), 1);
    assert!(!fixture.app.join(PENDING).exists());
}

#[test]
fn unknown_versions_missing_databases_and_path_traversal_are_rejected() {
    let fixture = Fixture::new();
    fixture.backup();
    rewrite_zip(&fixture.archive(), |files| {
        let mut meta: Value = serde_json::from_slice(&files["backup-info.json"]).unwrap();
        meta["formatVersion"] = 999.into();
        files.insert(
            "backup-info.json".into(),
            serde_json::to_vec(&meta).unwrap(),
        );
    });
    assert!(
        fixture
            .restore(false)
            .unwrap_err()
            .contains("Unsupported backup format")
    );
    fixture.backup();
    rewrite_zip(&fixture.archive(), |files| {
        files.remove("libraries/0.db");
    });
    assert!(fixture.restore(false).is_err());
    fixture.backup();
    rewrite_zip(&fixture.archive(), |files| {
        files.insert("../outside.db".into(), vec![]);
    });
    assert!(fixture.restore(false).unwrap_err().contains("Unsafe"));
    assert_eq!(fixture.saved().libraries.len(), 1);
}

#[test]
fn legacy_archive_restores_without_configuration_or_sha256() {
    let fixture = Fixture::new();
    let mut zip = ZipWriter::new(File::create(fixture.archive()).unwrap());
    zip.start_file("backup-info.json", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(r#"{"createdAt":1,"libraries":[{"name":"旅行/照片","dbSize":0}]}"#.as_bytes())
        .unwrap();
    zip.start_file("旅行_照片.db", SimpleFileOptions::default())
        .unwrap();
    std::io::copy(
        &mut File::open(fixture.databases.join("default.db")).unwrap(),
        &mut zip,
    )
    .unwrap();
    zip.finish().unwrap();
    let result = fixture.restore(false).unwrap();
    assert_eq!(result.restored_count, 1);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.contains("Legacy backup"))
    );
}

#[test]
fn failed_backup_preserves_previous_archive_and_missing_library_is_not_silently_skipped() {
    let fixture = Fixture::new();
    fixture.backup();
    let original = fs::read(fixture.archive()).unwrap();
    fs::remove_file(fixture.databases.join("default.db")).unwrap();
    let result = create_backup(
        &fixture.config,
        &fixture.app,
        &["default".into()],
        &fixture.archive(),
        None,
    );
    assert!(result.is_err());
    assert_eq!(fs::read(fixture.archive()).unwrap(), original);
    assert_eq!(fs::read_dir(&fixture.root.0).unwrap().count(), 3);
}

#[test]
fn restore_commit_failure_removes_new_databases_and_pending_settings() {
    let fixture = Fixture::new();
    fixture.backup();
    let original = fs::read(&fixture.config_path).unwrap();
    let bad_config_path = fixture.app.join("directory-instead-of-config");
    fs::create_dir(&bad_config_path).unwrap();
    let result = restore_to(
        &fixture.archive(),
        &fixture.selection(),
        true,
        fixture.config.clone(),
        &bad_config_path,
        &fixture.app,
        false,
    );
    assert!(result.is_err());
    assert_eq!(fs::read_dir(&fixture.databases).unwrap().count(), 1);
    assert!(!fixture.app.join(PENDING).exists());
    assert_eq!(fs::read(&fixture.config_path).unwrap(), original);
}

#[test]
fn startup_settings_failure_rolls_back_every_installed_file_and_retains_pending_restore() {
    let fixture = Fixture::new();
    fixture.backup();
    fixture.restore(true).unwrap();
    let original = fs::read(&fixture.config_path).unwrap();
    let blocked = fixture.app.join(UI_READY);
    fs::create_dir(&blocked).unwrap();
    assert!(apply_pending_to(&fixture.app, fixture.saved(), &fixture.config_path).is_err());
    assert_eq!(fs::read(&fixture.config_path).unwrap(), original);
    assert!(fixture.app.join(PENDING).exists());
    assert!(blocked.is_dir());
}

#[test]
fn invalid_settings_can_be_skipped_but_are_never_installed() {
    let fixture = Fixture::new();
    fs::write(fixture.app.join("ai-config.json"), b"{\"version\":999}").unwrap();
    fixture.backup();
    assert!(fixture.restore(true).is_err());
    assert_eq!(fixture.saved().libraries.len(), 1);
    assert!(fixture.restore(false).is_ok());
    assert_eq!(
        fs::read(fixture.app.join("ai-config.json")).unwrap(),
        b"{\"version\":999}"
    );
}

#[test]
fn database_integrity_schema_and_future_versions_are_checked() {
    let fixture = Fixture::new();
    let path = fixture.databases.join("default.db");
    validate_database(&path).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA user_version=999").unwrap();
    assert!(
        validate_database(&path)
            .unwrap_err()
            .contains("newer version")
    );
    conn.execute_batch("PRAGMA user_version=0; DROP TABLE afiles;")
        .unwrap();
    assert!(
        validate_database(&path)
            .unwrap_err()
            .contains("Invalid Lap database")
    );
    drop(conn);
    fs::write(&path, b"not sqlite").unwrap();
    assert!(validate_database(&path).is_err());
}

#[test]
fn filename_collisions_are_independent_and_duplicate_selections_are_rejected() {
    let mut fixture = Fixture::new();
    let mut second = fixture.config.libraries[0].clone();
    second.id = "second".into();
    second.name = "旅行_照片".into();
    fixture.config.libraries.push(second);
    fs::copy(
        fixture.databases.join("default.db"),
        fixture.databases.join("second.db"),
    )
    .unwrap();
    create_backup(
        &fixture.config,
        &fixture.app,
        &["default".into(), "second".into()],
        &fixture.archive(),
        None,
    )
    .unwrap();
    let meta = parse_backup_file(fixture.archive().to_str().unwrap()).unwrap();
    assert_ne!(meta.libraries[0].entry, meta.libraries[1].entry);
    let mut selections = fixture.selection();
    selections.extend(fixture.selection());
    assert!(
        restore_to(
            &fixture.archive(),
            &selections,
            false,
            fixture.config.clone(),
            &fixture.config_path,
            &fixture.app,
            false,
        )
        .is_err()
    );
}

#[test]
fn file_replacement_failure_never_removes_the_live_original() {
    let workspace = Workspace::new(&std::env::temp_dir()).unwrap();
    let target = workspace.0.join("original.json");
    fs::write(&target, b"original").unwrap();
    let mut transaction = FileTransaction::default();
    let error = transaction
        .install(&workspace.0.join("missing-source"), &target, true)
        .unwrap_err();
    assert_eq!(fs::read(&target).unwrap(), b"original");
    transaction.fail(error);
    assert_eq!(fs::read(&target).unwrap(), b"original");
    assert_eq!(fs::read_dir(&workspace.0).unwrap().count(), 1);
}

#[test]
fn partial_file_commit_restores_previous_content_and_removes_fresh_destinations() {
    let workspace = Workspace::new(&std::env::temp_dir()).unwrap();
    let target = workspace.0.join("original.json");
    let source = workspace.0.join("new.json");
    fs::write(&target, b"original").unwrap();
    fs::write(&source, b"new").unwrap();
    let mut transaction = FileTransaction::default();
    transaction.install(&source, &target, true).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert!(
        transaction
            .install(
                &workspace.0.join("missing"),
                &workspace.0.join("fresh.db"),
                false
            )
            .is_err()
    );
    assert!(transaction.rollback().is_empty());
    assert_eq!(fs::read(&target).unwrap(), b"original");
    assert!(!workspace.0.join("fresh.db").exists());
}
