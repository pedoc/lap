use super::*;

const ORIGINALS: [(&str, &[u8]); 7] = [
    ("photo.RAW", b"raw original"),
    ("photo.JPG", b"jpeg original"),
    ("photo.MOV", b"movie original"),
    ("photo.AAE", b"shared edits"),
    ("photo.RAW.AAE", b"raw edits"),
    ("photo.JPG.aae", b"jpeg edits"),
    ("photo.MOV.AAE", b"movie edits"),
];

fn originals(directory: &Path) -> Vec<AFile> {
    fs::create_dir_all(directory).unwrap();
    for (name, bytes) in ORIGINALS {
        fs::write(directory.join(name), bytes).unwrap();
    }
    [(2, "photo.JPG"), (3, "photo.MOV")]
        .into_iter()
        .map(|(id, name)| AFile {
            id: Some(id),
            file_path: Some(directory.join(name).to_string_lossy().into_owned()),
            ..Default::default()
        })
        .collect()
}

#[test]
fn rename_maps_every_members_full_name_aae_and_shared_edits_once() {
    let directory = tempfile::tempdir().unwrap();
    let components = originals(directory.path());
    let primary = directory.path().join("photo.RAW");
    let plans = build_rename_plan(primary.to_str().unwrap(), "renamed.NEF", &components).unwrap();
    assert!(preflight_rename_plan(
        primary.to_str().unwrap(),
        "renamed.NEF",
        &plans
    ));
    assert_eq!(plans.len(), 6);
    assert_eq!(
        plans
            .iter()
            .filter(|plan| plan.old_path.ends_with("photo.AAE"))
            .count(),
        1
    );
    for plan in &plans {
        fs::rename(&plan.old_path, &plan.new_path).unwrap();
    }
    fs::rename(&primary, directory.path().join("renamed.NEF")).unwrap();
    for (name, bytes) in [
        ("renamed.NEF", b"raw original".as_slice()),
        ("renamed.JPG", b"jpeg original".as_slice()),
        ("renamed.MOV", b"movie original".as_slice()),
        ("renamed.AAE", b"shared edits".as_slice()),
        ("renamed.NEF.AAE", b"raw edits".as_slice()),
        ("renamed.JPG.aae", b"jpeg edits".as_slice()),
        ("renamed.MOV.AAE", b"movie edits".as_slice()),
    ] {
        assert_eq!(fs::read(directory.path().join(name)).unwrap(), bytes);
    }
    assert!(
        ORIGINALS
            .iter()
            .all(|(name, _)| !directory.path().join(name).exists())
    );
    assert_eq!(
        plans.iter().filter(|plan| plan.file_id.is_some()).count(),
        2
    );
}

#[test]
fn copy_and_move_plans_keep_companion_edit_bytes_and_full_names() {
    for moving in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        let target = directory.path().join("target");
        fs::create_dir(&target).unwrap();
        let components = originals(&source);
        let primary = source.join("photo.RAW");
        let sources = sidecar_transfer_sources(primary.to_str().unwrap(), &components);
        let target_primary = target.join("photo(1).RAW");
        let plans = build_apple_sidecar_transfer_plan_from_sources(
            primary.to_str().unwrap(),
            &target_primary,
            &sources,
        )
        .unwrap();
        assert_eq!(plans.len(), 6);
        assert!(!group_target_conflicts(&target_primary, &plans));
        for (old, new) in plans
            .iter()
            .map(|plan| (&plan.old_path, &plan.new_path))
            .chain(std::iter::once((&primary, &target_primary)))
        {
            if moving {
                fs::rename(old, new).unwrap();
            } else {
                fs::copy(old, new).unwrap();
            }
        }
        for (name, bytes) in ORIGINALS {
            let expected = name.replacen("photo", "photo(1)", 1);
            assert_eq!(fs::read(target.join(expected)).unwrap(), bytes);
            assert_eq!(source.join(name).exists(), !moving);
        }
    }
}

#[test]
fn companion_aae_collisions_are_visible_before_any_file_changes() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let target = directory.path().join("target");
    fs::create_dir(&target).unwrap();
    let components = originals(&source);
    let primary = source.join("photo.RAW");
    let sources = sidecar_transfer_sources(primary.to_str().unwrap(), &components);
    let target_primary = target.join("photo(1).RAW");
    let plans = build_apple_sidecar_transfer_plan_from_sources(
        primary.to_str().unwrap(),
        &target_primary,
        &sources,
    )
    .unwrap();
    let protected = target.join("photo(1).JPG.aae");
    fs::write(&protected, b"existing destination edits").unwrap();
    assert!(group_target_conflicts(&target_primary, &plans));
    let rename_plans =
        build_rename_plan(primary.to_str().unwrap(), "renamed.RAW", &components).unwrap();
    fs::write(source.join("renamed.JPG.aae"), b"existing rename edits").unwrap();
    assert!(!preflight_rename_plan(
        primary.to_str().unwrap(),
        "renamed.RAW",
        &rename_plans
    ));
    assert_eq!(fs::read(&protected).unwrap(), b"existing destination edits");
    assert_eq!(
        fs::read(source.join("renamed.JPG.aae")).unwrap(),
        b"existing rename edits"
    );
    for (name, bytes) in ORIGINALS {
        assert_eq!(fs::read(source.join(name)).unwrap(), bytes);
    }
}

#[test]
fn unpaired_primary_keeps_both_existing_aae_naming_conventions() {
    let directory = tempfile::tempdir().unwrap();
    let primary = directory.path().join("single.HEIC");
    fs::write(&primary, b"single original").unwrap();
    fs::write(directory.path().join("single.AAE"), b"short edits").unwrap();
    fs::write(directory.path().join("single.HEIC.AAE"), b"full edits").unwrap();
    let plans = build_rename_plan(primary.to_str().unwrap(), "edited.JPG", &[]).unwrap();
    let mut names = plans
        .iter()
        .map(|plan| plan.new_name.as_str())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["edited.AAE", "edited.JPG.AAE"]);
}

#[test]
fn distinct_owners_short_name_edits_cannot_collapse_into_one_target() {
    let directory = tempfile::tempdir().unwrap();
    let primary = directory.path().join("primary.RAW");
    let companion = directory.path().join("different.JPG");
    for path in [
        &primary,
        &companion,
        &directory.path().join("primary.AAE"),
        &directory.path().join("different.AAE"),
    ] {
        fs::write(path, b"preserve every file").unwrap();
    }
    let components = [AFile {
        id: Some(2),
        file_path: Some(companion.to_string_lossy().into_owned()),
        ..Default::default()
    }];
    let target = directory.path().join("new.RAW");
    let sources = sidecar_transfer_sources(primary.to_str().unwrap(), &components);
    assert!(
        build_apple_sidecar_transfer_plan_from_sources(
            primary.to_str().unwrap(),
            &target,
            &sources
        )
        .is_err()
    );
    let plans = build_rename_plan(primary.to_str().unwrap(), "new.RAW", &components).unwrap();
    assert!(!preflight_rename_plan(
        primary.to_str().unwrap(),
        "new.RAW",
        &plans
    ));
    assert_eq!(
        fs::read(directory.path().join("primary.AAE")).unwrap(),
        b"preserve every file"
    );
    assert_eq!(
        fs::read(directory.path().join("different.AAE")).unwrap(),
        b"preserve every file"
    );
}
