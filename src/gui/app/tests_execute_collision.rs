use super::collision::CollisionEntry;
use super::tests_util::*;
use super::*;
use tempfile::TempDir;

#[test]
fn test_detect_in_batch_collision() {
    use awara::detect_collisions;
    let ops = vec![
        RenameOp {
            original_path: PathBuf::from("/tmp/x.txt"),
            new_path: PathBuf::from("/tmp/a.txt"),
            original_name: "x.txt".into(),
            new_name: "a.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        },
        RenameOp {
            original_path: PathBuf::from("/tmp/y.txt"),
            new_path: PathBuf::from("/tmp/a.txt"),
            original_name: "y.txt".into(),
            new_name: "a.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        },
    ];
    let collisions = detect_collisions(&ops);
    assert_eq!(collisions.len(), 1, "one InBatch collision");
    assert!(
        matches!(collisions[0].reason, awara::CollisionReason::InBatch(0)),
        "second op collides with first"
    );
}

#[test]
fn test_detect_chain_collision_excluded() {
    use awara::detect_collisions;
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    let c = dir.path().join("c.txt");
    std::fs::write(&b, b"").unwrap(); // b.txt exists on disk

    let ops = vec![
        RenameOp {
            original_path: a.clone(),
            new_path: b.clone(),
            original_name: "a.txt".into(),
            new_name: "b.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        },
        RenameOp {
            original_path: b.clone(),
            new_path: c.clone(),
            original_name: "b.txt".into(),
            new_name: "c.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        },
    ];
    // b.txt exists on disk but is also the source of another op — chain, not collision.
    let collisions = detect_collisions(&ops);
    assert_eq!(collisions.len(), 0, "chain conflict filtered out");
    std::fs::remove_file(&b).unwrap();
}
#[test]
fn test_live_reason_ondisk_stays_ondisk() {
    // An original OnDisk collision never changes, regardless of prior decisions.
    let mut state = CollisionDialogState {
        entries: vec![make_entry(
            "/tmp/x.txt",
            "x.txt",
            "a.txt",
            CollisionReason::OnDisk,
            None,
        )],
        current: 0,
        all_skip: false,
        is_f2: false,
        init_focus: true,
    };
    state.recompute_live_reasons();
    assert_eq!(
        state.entries[0].live_reason,
        LiveCollisionReason::OnDisk,
        "OnDisk should stay OnDisk regardless of decisions"
    );
}

/// Dialog decisions map back onto the correct plan ops by source path, so a
/// skipped/replaced op keeps its index (and therefore its numbering slot).
#[test]
fn test_dialog_decisions_map_to_plan_indices() {
    let mk = |orig: &str, new: &str| RenameOp {
        original_path: PathBuf::from(orig),
        new_path: PathBuf::from(new),
        original_name: PathBuf::from(orig)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        new_name: PathBuf::from(new)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0u8,
    };
    let plan = awara::RenamePlan::from_ops(vec![
        mk("/tmp/a.txt", "/tmp/x.txt"),
        mk("/tmp/b.txt", "/tmp/x.txt"),
        mk("/tmp/c.txt", "/tmp/y.txt"),
    ]);
    let collisions = vec![awara::Collision {
        op_index: 1,
        reason: CollisionReason::InBatch(0),
        target_is_dir: false,
        source_is_dir: false,
    }];

    let mut state = CollisionDialogState::from_plan(&plan, &collisions);
    assert_eq!(state.entries.len(), 1);
    assert_eq!(state.entries[0].source_path, "/tmp/b.txt");

    state.entries[0].decided_skip = true;
    assert_eq!(
        state.decisions(&plan),
        vec![
            awara::OpDecision::Default,
            awara::OpDecision::Skip,
            awara::OpDecision::Default
        ]
    );

    state.entries[0].decided_skip = false;
    state.entries[0].decided_replace = true;
    assert_eq!(state.decisions(&plan)[1], awara::OpDecision::Overwrite);
}

#[test]
fn test_live_reason_inbatch_stays_inbatch_when_prior_undecided() {
    // InBatch stays InBatch when the prior entry hasn't been decided yet.
    let mut state = CollisionDialogState {
        entries: vec![
            make_entry(
                "/tmp/x.txt",
                "x.txt",
                "a.txt",
                CollisionReason::OnDisk,
                None,
            ),
            make_entry(
                "/tmp/y.txt",
                "y.txt",
                "a.txt",
                CollisionReason::InBatch(0),
                Some("/tmp/x.txt".to_string()),
            ),
        ],
        current: 1,
        all_skip: false,
        is_f2: false,
        init_focus: true,
    };
    state.recompute_live_reasons();
    assert_eq!(
        state.entries[1].live_reason,
        LiveCollisionReason::InBatch,
        "InBatch should stay InBatch when prior entry is undecided"
    );
}

#[test]
fn test_live_reason_inbatch_stays_inbatch_when_prior_replaced() {
    // InBatch stays InBatch when the prior entry was Replaced (the file
    // at target was written by our own batch, not a pre-existing user file).
    let mut state = CollisionDialogState {
        entries: vec![
            make_entry(
                "/tmp/x.txt",
                "x.txt",
                "a.txt",
                CollisionReason::OnDisk,
                None,
            ),
            make_entry(
                "/tmp/y.txt",
                "y.txt",
                "a.txt",
                CollisionReason::InBatch(0),
                Some("/tmp/x.txt".to_string()),
            ),
        ],
        current: 1,
        all_skip: false,
        is_f2: false,
        init_focus: true,
    };
    state.entries[0].decided_replace = true;
    state.recompute_live_reasons();
    assert_eq!(
        state.entries[1].live_reason,
        LiveCollisionReason::InBatch,
        "InBatch should stay InBatch when prior entry was Replaced"
    );
}

#[test]
fn test_live_reason_inbatch_becomes_ondisk_when_prior_skipped_and_file_exists() {
    // InBatch becomes OnDisk when the prior entry was skipped AND the
    // target file still exists on disk.
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"existing content").unwrap();

    let mut state = CollisionDialogState {
        entries: vec![
            make_entry(
                "/tmp/x.txt",
                "x.txt",
                "a.txt",
                CollisionReason::OnDisk,
                None,
            ),
            // The second entry's target_path must be a real path under `dir`
            // because recompute_live_reasons checks whether it exists on disk.
            CollisionEntry {
                source_path: dir.path().join("y.txt").to_string_lossy().to_string(),
                source_display: "y.txt".to_string(),
                target_display: "a.txt".to_string(),
                target_path: dir.path().join("a.txt"),
                target_is_dir: false,
                source_is_dir: false,
                reason: CollisionReason::InBatch(0),
                decided_replace: false,
                decided_skip: false,
                conflict_source_path: Some("/tmp/x.txt".to_string()),
                live_reason: LiveCollisionReason::InBatch,
            },
        ],
        current: 1,
        all_skip: false,
        is_f2: false,
        init_focus: true,
    };
    state.entries[0].decided_skip = true;
    state.recompute_live_reasons();
    assert_eq!(
        state.entries[1].live_reason,
        LiveCollisionReason::OnDisk,
        "InBatch should become OnDisk when prior was skipped and file exists"
    );
    std::fs::remove_file(&target).unwrap();
}

#[test]
fn test_live_reason_inbatch_stays_inbatch_when_prior_skipped_but_file_gone() {
    // InBatch stays InBatch when the prior entry was skipped but the
    // target file no longer exists (e.g. deleted externally).
    let dir = TempDir::new().unwrap();
    // Do NOT create the target file — it doesn't exist.

    let mut state = CollisionDialogState {
        entries: vec![
            make_entry(
                "/tmp/x.txt",
                "x.txt",
                "a.txt",
                CollisionReason::OnDisk,
                None,
            ),
            CollisionEntry {
                source_path: dir.path().join("y.txt").to_string_lossy().to_string(),
                source_display: "y.txt".to_string(),
                target_display: "a.txt".to_string(),
                target_path: dir.path().join("a.txt"),
                target_is_dir: false,
                source_is_dir: false,
                reason: CollisionReason::InBatch(0),
                decided_replace: false,
                decided_skip: false,
                conflict_source_path: Some("/tmp/x.txt".to_string()),
                live_reason: LiveCollisionReason::InBatch,
            },
        ],
        current: 1,
        all_skip: false,
        is_f2: false,
        init_focus: true,
    };
    state.entries[0].decided_skip = true;
    state.recompute_live_reasons();
    assert_eq!(
        state.entries[1].live_reason,
        LiveCollisionReason::InBatch,
        "InBatch should stay InBatch when prior was skipped but target is gone"
    );
}

#[test]
fn test_live_reason_chain_inbatch_all_stay_inbatch() {
    // Three entries all targeting the same path, sequential decisions.
    // None should show the OnDisk message since all replacements stay within the batch.
    let mut state = CollisionDialogState {
        entries: vec![
            make_entry(
                "/tmp/a.txt",
                "a.txt",
                "out.txt",
                CollisionReason::OnDisk,
                None,
            ),
            make_entry(
                "/tmp/b.txt",
                "b.txt",
                "out.txt",
                CollisionReason::InBatch(0),
                Some("/tmp/a.txt".to_string()),
            ),
            make_entry(
                "/tmp/c.txt",
                "c.txt",
                "out.txt",
                CollisionReason::InBatch(0),
                Some("/tmp/a.txt".to_string()),
            ),
        ],
        current: 2,
        all_skip: false,
        is_f2: false,
        init_focus: true,
    };
    // Scenario: entry 0 was Replaced (the file at target is our own batch output).
    state.entries[0].decided_replace = true;
    state.recompute_live_reasons();

    // Entry 1 (InBatch referring to entry 0) is still InBatch because the
    // file at target was created by our own batch replacement.
    assert_eq!(
        state.entries[1].live_reason,
        LiveCollisionReason::InBatch,
        "entry 1 should stay InBatch when entry 0 was Replaced"
    );
    // Entry 2 (InBatch also referring to entry 0) is also InBatch for the same reason.
    assert_eq!(
        state.entries[2].live_reason,
        LiveCollisionReason::InBatch,
        "entry 2 should stay InBatch when entry 0 was Replaced"
    );
}

#[test]
fn test_live_reason_f2_always_ondisk() {
    // F2 renames always produce CollisionReason::OnDisk, which always
    // stays LiveCollisionReason::OnDisk.
    let mut state = CollisionDialogState {
        entries: vec![make_entry(
            "/tmp/x.txt",
            "x.txt",
            "new_name.txt",
            CollisionReason::OnDisk,
            None,
        )],
        current: 0,
        all_skip: false,
        is_f2: true,
        init_focus: true,
    };
    state.recompute_live_reasons();
    assert_eq!(
        state.entries[0].live_reason,
        LiveCollisionReason::OnDisk,
        "F2 OnDisk should always stay OnDisk"
    );
}
#[test]
fn test_trash_surgical_keeps_other_dir_commits() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();

    // Two files: one we'll trash, one we'll keep.
    let trash_file = dir.path().join("trash_me.txt");
    let keep_file = dir.path().join("keep_me.txt");
    std::fs::write(&trash_file, b"").unwrap();
    std::fs::write(&keep_file, b"").unwrap();

    app.cwd = dir.path().to_path_buf();
    app.cwd_input = dir.path().to_string_lossy().to_string();
    app.scan_dir();
    // Poll until scan finishes.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.check_scan_results();
        if !app.scanning {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!app.all_files.is_empty(), "scan should have found files");

    // Add a commit for the keep-file in a separate directory.
    let other_dir = PathBuf::from("/tmp/other");
    app.commits_by_dir
        .entry(other_dir.clone())
        .or_default()
        .push(Commit {
            timestamp: std::time::Instant::now(),
            label: "other".into(),
            ops: vec![RenameOp {
                original_path: PathBuf::from("/tmp/other/old.txt"),
                new_path: PathBuf::from("/tmp/other/new.txt"),
                original_name: "old.txt".into(),
                new_name: "new.txt".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0u8,
            }],
        });

    // Select and trash the first file.
    app.selection = vec![true, false];
    app.trash_selected_multi().unwrap();

    // The other directory's commit should survive.
    assert!(
        app.commits_by_dir.contains_key(&other_dir),
        "other dir commits preserved after trash"
    );
    assert_eq!(
        app.commits_by_dir[&other_dir][0].ops.len(),
        1,
        "other dir op preserved"
    );
}

#[test]
fn test_execute_renames_error_stored_in_status_map() {
    // Verify that a failed rename populates status_map with (status=1, Some(error)).
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();

    // Create a file with a short name
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"hello").unwrap();

    app.cwd = dir.path().to_path_buf();
    app.cwd_input = app.cwd.to_string_lossy().to_string();
    app.all_files = vec![p.clone()];
    app.file_sizes = vec![5];
    app.file_dates = vec![0];
    app.file_is_dir = vec![false];
    app.selection = vec![true];
    app.ensure_file_names_synced();

    // Set up the Name section to produce a filename that's too long
    app.section_enabled[SectionId::Name as usize] = true;
    app.config.name.name_mode = Some("fixed".into());
    let long_name = "s".repeat(800);
    app.config.name.name_value = Some(long_name);
    app.build_command_order();

    // Execute — this should fail because the filename is too long
    app.preview_dirty = true;
    app.execute_renames();

    // Must call update() logic to trigger preview update and row cache rebuild
    // The rename was synchronous (execute_plan_inner runs inline), so
    // status_map is already populated.
    let path_str = p.to_string_lossy().to_string();

    // status_map should have the original path with (1, Some(error_msg))
    let entry = app.status_map.get(&path_str);
    assert!(
        entry.is_some(),
        "failed file should have an entry in status_map"
    );
    let (status, err) = entry.unwrap();
    assert_eq!(
        *status, 1u8,
        "failed file should have status=1 in status_map"
    );
    let err_msg = err
        .as_ref()
        .expect("failed file should have an error message in status_map");
    assert!(
        !err_msg.is_empty(),
        "error message should not be empty, got: {:?}",
        err_msg
    );
    // The error should mention something relevant
    let lower = err_msg.to_lowercase();
    assert!(
        lower.contains("too long")
            || lower.contains("enoent")
            || lower.contains("no such file")
            || lower.contains("permission")
            || lower.contains("eacces")
            || lower.contains("denied")
            || lower.contains("syntax is incorrect"),
        "error message should describe the failure, got: {}",
        err_msg
    );
}

#[test]
fn test_execute_renames_error_propagates_to_row_cache() {
    // Full integration: error message flows from execution → status_map
    // → RowsCacheRequest → Row.error_msg.
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();

    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"hello").unwrap();

    app.cwd = dir.path().to_path_buf();
    app.cwd_input = app.cwd.to_string_lossy().to_string();
    app.all_files = vec![p.clone()];
    app.file_sizes = vec![5];
    app.file_dates = vec![0];
    app.file_is_dir = vec![false];
    app.selection = vec![true];
    app.ensure_file_names_synced();

    // Set up Name section to produce an excessively long filename
    app.section_enabled[SectionId::Name as usize] = true;
    app.config.name.name_mode = Some("fixed".into());
    app.config.name.name_value = Some("s".repeat(800));
    app.build_command_order();

    app.preview_dirty = true;
    app.execute_renames();

    // Manually rebuild row cache (preview is not running in this test)
    // Use the synchronous fallback.
    app.ensure_file_names_synced();
    let status_map = app.ensure_status_map().clone();
    let request = RowsCacheRequest {
        r#gen: 0,
        files: app.file_names.clone(),
        sizes: app.file_sizes.clone(),
        dates: app.file_dates.clone(),
        is_dirs: app.file_is_dir.clone(),
        preview: app.preview.clone(),
        status_map,
        selection: app.selection.clone(),
        config: app.config.clone(),
        section_enabled: app.section_enabled,
        processed: app.processed,
        selection_only: false,
        fp: app.rows_dataset_fp(),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let rows = build_rows_cache(&request, None);
    let row = rows.iter().find(|r| r.idx == 0).expect("row should exist");

    assert_eq!(row.status, Some(1u8), "row status should be failed");
    let err_msg = row
        .error_msg
        .as_deref()
        .expect("row.error_msg should contain the OS error");
    assert!(!err_msg.is_empty(), "error message must not be empty");
    let lower = err_msg.to_lowercase();
    assert!(
        lower.contains("too long")
            || lower.contains("enoent")
            || lower.contains("no such file")
            || lower.contains("permission")
            || lower.contains("eacces")
            || lower.contains("denied")
            || lower.contains("syntax is incorrect"),
        "error message should describe the failure, got: {}",
        err_msg
    );
}

#[test]
fn test_execute_renames_success_status_in_map() {
    // Verify successful renames still show status=0 (no regression)
    let (mut app, _dir) = selection_test_app(&["old_a.txt"]);
    app.preview_dirty = true;
    app.execute_renames();

    // After success, all_files[0] has been updated to the new path
    // The original path should have status=0 in status_map
    // Actually after success, file_names[0] is the NEW name.
    // The original name should also be in status_map from the commit.
    // We can check the commit directly:
    let ops = &app.cwd_commits().last().unwrap().ops;
    assert_eq!(ops.len(), 1);
    let orig = ops[0].original_path.to_string_lossy().to_string();
    assert_eq!(
        app.status_map.get(&orig),
        Some(&(0u8, None)),
        "original path should have status=0 (success)"
    );
    let new = ops[0].new_path.to_string_lossy().to_string();
    assert_eq!(
        app.status_map.get(&new),
        Some(&(0u8, None)),
        "new path should have status=0 (success)"
    );
    // No errors for all-success execution
    assert!(
        app.status_map.values().all(|(s, _)| *s == 0u8),
        "all status_map entries should have status=0 for all-success execution"
    );
}

/// The collision dialog resolves a *frozen* plan. A background scan that
/// finishes while the dialog is open replaces the file list, but must not swap
/// the plan the user is resolving — execution still renames exactly the files
/// they applied to (absolute paths). This is safe because a scan never deletes
/// or replaces files on disk, and the post-apply list updates are keyed by full
/// path, so the newly scanned, unrelated files are never touched.
#[test]
fn test_frozen_plan_executes_original_files_after_scan_completion() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    let existing_target = dir.path().join("xa.txt");
    std::fs::write(&a, b"").unwrap();
    std::fs::write(&b, b"").unwrap();
    // The target of `a.txt` already exists → OnDisk collision opens the dialog.
    std::fs::write(&existing_target, b"").unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = vec![a.clone(), b.clone()];
    app.file_names = vec![
        a.to_string_lossy().to_string(),
        b.to_string_lossy().to_string(),
    ];
    app.file_display_names = vec!["a.txt".into(), "b.txt".into()];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.file_is_dir = vec![false, false];
    app.selection = vec![true, true];
    app.config.add.add_prefix = Some("x".into());

    app.try_execute_renames();
    assert!(app.collision_dialog.is_some(), "collision opens the dialog");
    let frozen_sources: Vec<String> = app
        .pending_plan
        .as_ref()
        .expect("pending plan")
        .ops()
        .iter()
        .map(|op| op.original_path.to_string_lossy().to_string())
        .collect();
    assert!(frozen_sources.contains(&a.to_string_lossy().to_string()));
    assert!(frozen_sources.contains(&b.to_string_lossy().to_string()));

    // A background scan finishes while the dialog is open and replaces the list
    // with an unrelated file.
    let other = dir.path().join("other.txt");
    std::fs::write(&other, b"").unwrap();
    app.scanning = true;
    app.scan_result_tx
        .send(ScanResult {
            files: vec![other.clone()],
            sizes: vec![0],
            dates: vec![0],
            is_dirs: vec![false],
            r#gen: app.scan_generation,
            is_partial: false,
        })
        .unwrap();
    app.check_scan_results();
    assert_eq!(app.all_files, vec![other.clone()], "scan result applied");

    // The dialog and its plan survive the scan.
    assert!(app.collision_dialog.is_some());
    assert!(app.pending_plan.is_some());

    // Resolving it renames the files the user applied to, not the scanned file.
    let plan = app.pending_plan.take().expect("frozen plan");
    app.collision_dialog = None;
    let resolution = awara::Resolution::default_for(CollisionStrategy::Overwrite);
    app.execute_plan_inner(plan, &resolution, "Apply", false);

    assert!(dir.path().join("xa.txt").exists(), "a.txt -> xa.txt");
    assert!(dir.path().join("xb.txt").exists(), "b.txt -> xb.txt");
    assert!(other.exists(), "the newly scanned file must be untouched");
}

/// A folder on **either** side of a collision must never be resolved as
/// Replace: folders cannot be overwritten or merged, so the decision degrades to
/// Skip even when the user (or "Replace All") marked it replace. A file→file
/// collision is unaffected.
#[test]
fn test_dialog_refuses_replace_when_folder_involved() {
    let mk = |orig: &str, new: &str| RenameOp {
        original_path: PathBuf::from(orig),
        new_path: PathBuf::from(new),
        original_name: PathBuf::from(orig)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        new_name: PathBuf::from(new)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0u8,
    };
    // The paths are logical only — the folder flags come from the collision,
    // so no filesystem setup is needed.
    let plan = awara::RenamePlan::from_ops(vec![
        mk("/tmp/src.txt", "/tmp/folder"),
        mk("/tmp/other.txt", "/tmp/file.txt"),
        mk("/tmp/folderSrc", "/tmp/other.txt"),
    ]);
    let collisions = vec![
        // Folder *target*.
        awara::Collision {
            op_index: 0,
            reason: CollisionReason::OnDisk,
            target_is_dir: true,
            source_is_dir: false,
        },
        // File → file: replace is allowed.
        awara::Collision {
            op_index: 1,
            reason: CollisionReason::OnDisk,
            target_is_dir: false,
            source_is_dir: false,
        },
        // Folder *source* over an existing file target.
        awara::Collision {
            op_index: 2,
            reason: CollisionReason::OnDisk,
            target_is_dir: false,
            source_is_dir: true,
        },
    ];

    let mut state = CollisionDialogState::from_plan(&plan, &collisions);
    assert!(
        state.entries[0].target_is_dir,
        "folder target must be flagged"
    );
    assert!(!state.entries[1].target_is_dir && !state.entries[1].source_is_dir);
    assert!(
        state.entries[2].source_is_dir,
        "folder source must be flagged"
    );

    // The user chooses Replace on every entry.
    for e in &mut state.entries {
        e.decided_replace = true;
    }

    let decisions = state.decisions(&plan);
    assert_eq!(
        decisions[0],
        awara::OpDecision::Skip,
        "Replace on a folder target must degrade to Skip"
    );
    assert_eq!(
        decisions[1],
        awara::OpDecision::Overwrite,
        "a file→file collision still overwrites"
    );
    assert_eq!(
        decisions[2],
        awara::OpDecision::Skip,
        "Replace with a folder source must degrade to Skip"
    );
}

/// The dialog takes each entry's target path straight from the plan op (the
/// same `op.new_path` execution uses), never re-deriving it from the source
/// parent + display name.
#[test]
fn test_from_plan_stores_target_path_from_op() {
    let mk = |orig: &str, new: &str| RenameOp {
        original_path: PathBuf::from(orig),
        new_path: PathBuf::from(new),
        original_name: PathBuf::from(orig)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        new_name: PathBuf::from(new)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0u8,
    };
    // The target is in an output directory — deliberately NOT beside the source,
    // so a re-derivation from source parent + name would give the wrong path.
    let plan = awara::RenamePlan::from_ops(vec![mk("/tmp/src.txt", "/tmp/out/renamed.txt")]);
    let collisions = vec![awara::Collision {
        op_index: 0,
        reason: CollisionReason::OnDisk,
        target_is_dir: false,
        source_is_dir: false,
    }];

    let state = CollisionDialogState::from_plan(&plan, &collisions);
    assert_eq!(
        state.entries[0].target_path,
        PathBuf::from("/tmp/out/renamed.txt"),
        "target path must come from the plan op, not be re-derived"
    );
}

/// F2 conflicts resolve the directory flag from the real target via the shared
/// no-follow helper.
#[test]
fn test_from_f2_marks_directory_target() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src.txt");
    std::fs::write(&src, b"").unwrap();

    let folder = dir.path().join("folder");
    std::fs::create_dir(&folder).unwrap();
    let dir_state = CollisionDialogState::from_f2(
        0,
        "folder".to_string(),
        "src.txt".to_string(),
        src.to_string_lossy().to_string(),
    );
    assert!(dir_state.entries[0].target_is_dir, "folder target flagged");
    assert_eq!(dir_state.entries[0].target_path, folder);

    let file_target = dir.path().join("other.txt");
    std::fs::write(&file_target, b"").unwrap();
    let file_state = CollisionDialogState::from_f2(
        0,
        "other.txt".to_string(),
        "src.txt".to_string(),
        src.to_string_lossy().to_string(),
    );
    assert!(!file_state.entries[0].target_is_dir);
}
