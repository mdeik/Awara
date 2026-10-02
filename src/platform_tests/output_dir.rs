use super::*;

#[test]
fn test_apply_output_dir_flat() {
    let mut op = RenameOp {
        original_path: PathBuf::from("/home/user/src/doc.txt"),
        new_path: PathBuf::from("/home/user/src/report.txt"),
        original_name: "doc.txt".into(),
        new_name: "report.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0u8,
    };
    apply_output_dir(&mut op, Path::new("/tmp/out"), false, None);
    assert_eq!(op.new_path, PathBuf::from("/tmp/out/report.txt"));
}

/// The output-dir transform is the one place that records relocation. A target
/// placed in a different directory is flagged as a transfer; an output location
/// that resolves back to the source's own directory is not (it is an in-place
/// rename, so it stays undoable).
#[test]
fn test_apply_output_dir_records_relocation() {
    let mk = |orig: &Path, name: &str| RenameOp {
        original_path: orig.to_path_buf(),
        new_path: orig.to_path_buf(),
        original_name: orig.file_name().unwrap().to_string_lossy().to_string(),
        new_name: name.into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0u8,
    };

    let src = PathBuf::from("/home/user/src/doc.txt");

    // Placed elsewhere: relocated, hence a location transfer and not undoable.
    let mut moved = mk(&src, "report.txt");
    apply_output_dir(&mut moved, Path::new("/tmp/out"), false, None);
    assert_eq!(moved.new_path, PathBuf::from("/tmp/out/report.txt"));
    assert!(moved.relocated, "different directory => relocated");
    assert!(moved.is_location_transfer());
    assert!(!moved.is_undoable(), "a transfer is not undoable");

    // Output location is the source's own directory: the name changes but the
    // target stays put, so this is an in-place rename, not a transfer.
    let mut in_place = mk(&src, "report.txt");
    apply_output_dir(&mut in_place, Path::new("/home/user/src"), false, None);
    assert_eq!(
        in_place.new_path,
        PathBuf::from("/home/user/src/report.txt")
    );
    assert!(!in_place.relocated, "same directory => not a transfer");
    assert!(in_place.is_undoable());
}

#[test]
fn test_apply_output_dir_keep_structure() {
    let mut op = RenameOp {
        original_path: PathBuf::from("/home/user/src/sub/report.txt"),
        new_path: PathBuf::from("/home/user/src/sub/report.txt"),
        original_name: "report.txt".into(),
        new_name: "final.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0u8,
    };
    apply_output_dir(
        &mut op,
        Path::new("/tmp/out"),
        true,
        Some(Path::new("/home/user/src")),
    );
    assert_eq!(op.new_path, PathBuf::from("/tmp/out/sub/final.txt"));
}

#[test]
fn test_common_base_dir() {
    let files = vec![
        PathBuf::from("/home/user/src/sub/a.txt"),
        PathBuf::from("/home/user/src/other/b.txt"),
        PathBuf::from("/home/user/src/c.txt"),
    ];
    assert_eq!(
        common_base_dir(&files),
        Some(PathBuf::from("/home/user/src"))
    );

    // Single file → its parent directory.
    let one = vec![PathBuf::from("/home/user/src/sub/a.txt")];
    assert_eq!(
        common_base_dir(&one),
        Some(PathBuf::from("/home/user/src/sub"))
    );

    // Relative paths share the empty prefix (structure preserved relative
    // to the working directory).
    let rel = vec![PathBuf::from("a/x.txt"), PathBuf::from("b/y.txt")];
    assert_eq!(common_base_dir(&rel), Some(PathBuf::from("")));

    // Mixed relative and absolute paths also collapse to the empty path.
    let mixed = vec![PathBuf::from("a/x.txt"), PathBuf::from("/b/y.txt")];
    assert_eq!(common_base_dir(&mixed), Some(PathBuf::from("")));

    // Empty input.
    assert_eq!(common_base_dir(&[]), None);
}

#[test]
fn test_apply_output_dir_guards_absolute_rel() {
    // An absolute relative-path (mixed relative/absolute inputs with an
    // empty base) must flatten instead of escaping the output dir.
    let mut op = RenameOp {
        original_path: PathBuf::from("/b/y.txt"),
        new_path: PathBuf::from("/b/y.txt"),
        original_name: "y.txt".into(),
        new_name: "y2.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0u8,
    };
    apply_output_dir(&mut op, Path::new("/tmp/out"), true, Some(Path::new("")));
    assert_eq!(op.new_path, PathBuf::from("/tmp/out/y2.txt"));

    // Windows: a drive-prefixed path without a root (`C:b`) must also
    // flatten, since joining it would replace the output dir.
    #[cfg(windows)]
    {
        let mut op3 = RenameOp {
            original_path: PathBuf::from("C:b/y.txt"),
            new_path: PathBuf::from("C:b/y.txt"),
            original_name: "y.txt".into(),
            new_name: "y3.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        };
        apply_output_dir(&mut op3, Path::new("/tmp/out"), true, Some(Path::new("")));
        assert_eq!(op3.new_path, PathBuf::from("/tmp/out/y3.txt"));
    }

    // Relative inputs still keep their structure with the empty base.
    let mut op2 = RenameOp {
        original_path: PathBuf::from("a/x.txt"),
        new_path: PathBuf::from("a/x.txt"),
        original_name: "x.txt".into(),
        new_name: "x2.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0u8,
    };
    apply_output_dir(&mut op2, Path::new("/tmp/out"), true, Some(Path::new("")));
    assert_eq!(op2.new_path, PathBuf::from("/tmp/out/a/x2.txt"));
}

#[test]
fn test_resolve_output_dir() {
    let base = Path::new("/home/user/proj");
    // Relative paths resolve against the current folder.
    assert_eq!(
        resolve_output_dir("out", base),
        PathBuf::from("/home/user/proj/out")
    );
    assert_eq!(
        resolve_output_dir("./sub", base),
        PathBuf::from("/home/user/proj/sub")
    );
    assert_eq!(
        resolve_output_dir(".", base),
        PathBuf::from("/home/user/proj")
    );
    assert_eq!(resolve_output_dir("..", base), PathBuf::from("/home/user"));
    // Absolute paths are used verbatim.
    assert_eq!(
        resolve_output_dir("/abs/dir", base),
        PathBuf::from("/abs/dir")
    );
}

#[test]
fn test_execute_keep_structure_output_dir() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let sub = src.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let f1 = sub.join("a.txt");
    let f2 = src.join("b.txt");
    std::fs::write(&f1, b"a").unwrap();
    std::fs::write(&f2, b"b").unwrap();
    let out = dir.path().join("out");

    let base = common_base_dir(&[f1.clone(), f2.clone()]).unwrap();
    assert_eq!(base, src);

    let mut config = RenameConfig::default();
    config.add.add_prefix = Some("x_".into());
    let mut options = RenameOptions {
        ..Default::default()
    };
    let paths: Vec<String> = [f1.clone(), f2.clone()]
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    let plan_opts = crate::PlanOptions {
        dirname_level: 1,
        output_dir: Some(out.as_path()),
        keep_structure: true,
        base_dir: Some(base.as_path()),
        copy_mode: true,
        parallel: true,
    };
    let plan = crate::plan_renames(&paths, &config, &plan_opts);
    let resolution = crate::Resolution::default_for(CollisionStrategy::Skip);
    let result = crate::execute_plan(plan, &config, &mut options, &resolution);
    assert!(
        result.failed_ops.is_empty(),
        "errors: {:?}",
        result.failed_ops
    );
    assert!(out.join("sub/x_a.txt").exists());
    assert!(out.join("x_b.txt").exists());
}

/// Two files from different folders renaming to the same name must collide
/// at the output-dir target and honor the collision strategy (docs: this
/// option "will honor the Overwrite Target Files menu option") — never
/// silently overwrite.
#[test]
fn test_execute_output_dir_collision_skip() {
    let dir = TempDir::new().unwrap();
    let folder_a = dir.path().join("FolderA");
    let folder_b = dir.path().join("FolderB");
    std::fs::create_dir_all(&folder_a).unwrap();
    std::fs::create_dir_all(&folder_b).unwrap();
    let f1 = folder_a.join("same.txt");
    let f2 = folder_b.join("same.txt");
    std::fs::write(&f1, b"one").unwrap();
    std::fs::write(&f2, b"two").unwrap();
    let out = dir.path().join("out");

    let mut config = RenameConfig::default();
    config.replace.replace = Some("same".into());
    config.replace.with = Some("renamed".into());
    let paths: Vec<String> = [f1.clone(), f2.clone()]
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    let mut options = RenameOptions {
        ..Default::default()
    };
    let plan_opts = crate::PlanOptions {
        output_dir: Some(out.as_path()),
        copy_mode: true,
        parallel: true,
        ..Default::default()
    };
    let plan = crate::plan_renames(&paths, &config, &plan_opts);
    let resolution = crate::Resolution::default_for(CollisionStrategy::Skip);
    let result = crate::execute_plan(plan, &config, &mut options, &resolution);
    assert_eq!(result.successful_ops.len(), 1);
    assert_eq!(result.skipped_ops.len(), 1);
    // Exactly one file landed in the output dir — no silent overwrite.
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
    assert!(out.join("renamed.txt").exists());
}

#[test]
fn test_execute_output_dir_collision_overwrite() {
    let dir = TempDir::new().unwrap();
    let folder_a = dir.path().join("FolderA");
    let folder_b = dir.path().join("FolderB");
    std::fs::create_dir_all(&folder_a).unwrap();
    std::fs::create_dir_all(&folder_b).unwrap();
    let f1 = folder_a.join("same.txt");
    let f2 = folder_b.join("same.txt");
    std::fs::write(&f1, b"one").unwrap();
    std::fs::write(&f2, b"two").unwrap();
    let out = dir.path().join("out");

    let mut config = RenameConfig::default();
    config.replace.replace = Some("same".into());
    config.replace.with = Some("renamed".into());
    let paths: Vec<String> = [f1.clone(), f2.clone()]
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    let mut options = RenameOptions {
        ..Default::default()
    };
    let plan_opts = crate::PlanOptions {
        output_dir: Some(out.as_path()),
        copy_mode: true,
        parallel: true,
        ..Default::default()
    };
    let plan = crate::plan_renames(&paths, &config, &plan_opts);
    let resolution = crate::Resolution::default_for(CollisionStrategy::Overwrite);
    let result = crate::execute_plan(plan, &config, &mut options, &resolution);
    assert_eq!(result.successful_ops.len(), 2);
    // Last one wins, deterministically.
    assert_eq!(
        std::fs::read_to_string(out.join("renamed.txt")).unwrap(),
        "two"
    );
}

#[test]
fn test_detect_collision_in_output_dir() {
    let dir = TempDir::new().unwrap();
    // A file already exists at the output location.
    let out_dir = dir.path().join("out");
    std::fs::create_dir(&out_dir).unwrap();
    let existing = out_dir.join("target.txt");
    std::fs::write(&existing, b"exists").unwrap();

    // Build ops whose new_path is already set to the output target.
    let ops = vec![
        RenameOp {
            original_path: dir.path().join("a.txt"),
            new_path: out_dir.join("target.txt"),
            original_name: "a.txt".into(),
            new_name: "target.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        },
        RenameOp {
            original_path: dir.path().join("b.txt"),
            new_path: out_dir.join("target.txt"),
            original_name: "b.txt".into(),
            new_name: "target.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        },
    ];
    // Apply output-dir transformation as the preview would.
    let mut adjusted = ops.clone();
    for op in &mut adjusted {
        apply_output_dir(op, &out_dir, false, None);
    }

    let collisions = detect_collisions(&adjusted);
    // Both OnDisk (file exists in out dir) and InBatch (two files target same name).
    assert_eq!(collisions.len(), 2, "both OnDisk and InBatch detected");
    assert!(
        collisions
            .iter()
            .any(|c| matches!(c.reason, CollisionReason::OnDisk)),
        "should detect file already exists in output dir"
    );
    assert!(
        collisions
            .iter()
            .any(|c| matches!(c.reason, CollisionReason::InBatch(_))),
        "should detect in-batch conflict in output dir"
    );
}

#[test]
#[cfg(unix)]
fn test_get_drive_roots_unix_returns_root() {
    let roots = get_drive_roots();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0], Path::new("/"));
}

#[test]
#[cfg(windows)]
fn test_get_drive_roots_windows_has_c_drive() {
    let roots = get_drive_roots();
    assert!(!roots.is_empty(), "should find at least one drive");
    assert!(
        roots.iter().any(|p| p.to_string_lossy() == r"C:\"),
        "should include C: drive"
    );
}

#[test]
fn test_timestamp_incr_per_file() {
    let dir = TempDir::new().unwrap();

    // Create three files with the same original mtime.
    let files: Vec<_> = (0..3)
        .map(|i| {
            let p = dir.path().join(format!("f{i}.txt"));
            fs::write(&p, b"data").unwrap();
            // Set all to the same base time: 2020-01-01 12:00:00 local
            let base = TimestampSpec::Fixed("2020-01-01 12:00:00".to_string())
                .resolve(&p, "modified")
                .unwrap();
            let ft = filetime::FileTime::from_system_time(base);
            filetime::set_file_mtime(&p, ft).unwrap();
            p
        })
        .collect();

    // Apply with incr 3600s (1 hour) per file
    let config = RenameConfig {
        special: SpecialSection {
            set_modified: Some(TimestampSpec::Fixed("2020-01-01 12:00:00".to_string())),
            timestamp_incr_secs: 3600,
            ..Default::default()
        },
        ..Default::default()
    };

    for (i, f) in files.iter().enumerate() {
        apply_timestamps_to_file(f, &config, i);
    }

    // Read back the mtimes and check they differ by incr.
    let times: Vec<_> = files
        .iter()
        .map(|f| {
            std::fs::metadata(f)
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        })
        .collect();

    assert!(
        times[1] > times[0],
        "file 1 should be later than file 0: {} vs {}",
        times[1],
        times[0]
    );
    assert!(
        times[2] > times[1],
        "file 2 should be later than file 1: {} vs {}",
        times[2],
        times[1]
    );
    // Each step should be roughly 3600s
    let diff1 = times[1] - times[0];
    let diff2 = times[2] - times[1];
    assert!(
        (3599..=3601).contains(&diff1),
        "diff 0→1 should be ~3600s, got {diff1}"
    );
    assert!(
        (3599..=3601).contains(&diff2),
        "diff 1→2 should be ~3600s, got {diff2}"
    );

    // Also verify file 0 has the base time (no offset)
    let expected_base =
        crate::core::rename::local_datetime_to_utc_secs(2020, 1, 1, 12, 0, 0).unwrap();
    let actual_base = times[0] as i64;
    assert!(
        (actual_base - expected_base).abs() <= 2,
        "file 0 should be ~{expected_base}, got {actual_base}"
    );
}
