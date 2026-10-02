use super::tests_util::*;
use super::*;
use tempfile::TempDir;

#[test]
fn test_reset_auto_date_edit_state() {
    let mut e = GuiEditState {
        auto_date_type: Some("creation_curr".into()),
        auto_date_format_key: Some("ymd_hms".into()),
        auto_date_sep: Some("-".into()),
        auto_date_custom: Some("YYYY/MM/DD".into()),
        move_part_action: Some("Copy first n".into()),
        move_part_val1: 5,
        move_part_val2: 3,
        move_part_dest: Some("To end".into()),
        move_part_dest_val: 10,
        move_part_sep: "_".into(),
    };
    reset_auto_date_edit_state(&mut e);
    // Auto-date fields reset
    assert_eq!(e.auto_date_type, None);
    assert_eq!(e.auto_date_format_key, None);
    assert_eq!(e.auto_date_sep, None);
    assert_eq!(e.auto_date_custom, None);
    // Move/copy fields UNTOUCHED
    assert_eq!(e.move_part_action, Some("Copy first n".into()));
    assert_eq!(e.move_part_val1, 5);
}

#[test]
fn test_reset_move_copy_edit_state() {
    let mut e = GuiEditState {
        auto_date_type: Some("creation_curr".into()),
        auto_date_format_key: Some("ymd_hms".into()),
        auto_date_sep: Some("-".into()),
        auto_date_custom: Some("YYYY/MM/DD".into()),
        move_part_action: Some("Copy first n".into()),
        move_part_val1: 5,
        move_part_val2: 3,
        move_part_dest: Some("To end".into()),
        move_part_dest_val: 10,
        move_part_sep: "_".into(),
    };
    reset_move_copy_edit_state(&mut e);
    // Move/copy fields reset to MoveCopyValue defaults
    assert_eq!(e.move_part_action, None);
    assert_eq!(e.move_part_val1, MoveCopyValue::default().start);
    assert_eq!(e.move_part_val2, MoveCopyValue::default().length);
    assert_eq!(e.move_part_dest, None);
    assert_eq!(e.move_part_dest_val, MoveCopyValue::default().destination);
    assert_eq!(e.move_part_sep, "");
    // Auto-date fields UNTOUCHED
    assert_eq!(e.auto_date_type, Some("creation_curr".into()));
    assert_eq!(e.auto_date_format_key, Some("ymd_hms".into()));
}

#[test]
fn test_reset_all_resets_all_fields_to_defaults() {
    let mut app = GuiApp::new_with_config_for_test();
    // Mutate fields that trigger modified state AND fields that don't
    app.config.regex.regex_match = Some("abc".into());
    app.config.regex.regex_full_name = true;
    app.config.replace.replace = Some("old".into());
    app.config.replace.replace_case_sensitive = true;
    app.config.numbering.numbering_start = 99;
    app.config.numbering.numbering_increment = 5;
    app.config.filters.filter_files = false;
    app.config.filters.filter_subfolders = true;
    app.config.copy_to.copy_mode = false;
    app.config.copy_to.keep_structure = true;
    app.config.special.timestamp_incr_secs = 60;
    app.config.section_order = vec!["Replace".into(), "Regex".into()];
    app.section_enabled[0] = false;
    app.section_enabled[2] = false;
    app.edit_state.auto_date_type = Some("modified_curr".into());
    app.edit_state.auto_date_format_key = Some("ymd".into());
    app.edit_state.move_part_action = Some("Move range".into());
    app.edit_state.move_part_val1 = 5;
    app.edit_state.move_part_dest = Some("To pos".into());
    app.edit_state.move_part_dest_val = 12;
    app.edit_state.move_part_sep = "__".into();
    app.undo_file = "custom_undo.json".into();
    app.timestamps.created.mode = "fixed".into();

    app.reset_all();

    assert_eq!(app.config, RenameConfig::default());
    assert_eq!(app.edit_state.auto_date_type, None);
    assert_eq!(app.edit_state.auto_date_format_key, None);
    assert_eq!(app.edit_state.move_part_action, None);
    assert_eq!(
        app.edit_state.move_part_val1,
        MoveCopyValue::default().start
    );
    assert_eq!(app.edit_state.move_part_dest, None);
    assert_eq!(
        app.edit_state.move_part_dest_val,
        MoveCopyValue::default().destination
    );
    assert_eq!(app.edit_state.move_part_sep, "");
    assert_eq!(app.section_enabled, DEFAULT_SECTION_ENABLED);
    assert_eq!(app.timestamps, GuiTimestampsState::default());
    assert_eq!(app.undo_file, "");
    assert!(app.preview_dirty);
    assert_eq!(app.status_message, "All settings reset");
}
#[test]
fn test_compose_move_copy_copy_action() {
    let e = edit_state(Some("Copy first n"), 3, 2, Some("To end"), 0);
    let s = compose_move_copy_section(&e);
    let v = s.copy_part.unwrap();
    assert_eq!((v.start, v.length, v.destination), (1, 3, -1));
    assert_eq!(v.separator, None);
    assert!(s.move_part.is_none(), "copy action must not set move_part");

    let e = edit_state(Some("Copy last n"), 4, 2, Some("To start"), 0);
    let v = compose_move_copy_section(&e).copy_part.unwrap();
    assert_eq!((v.start, v.length, v.destination), (-1, 4, 1));

    let e = edit_state(Some("Copy range"), 2, 5, Some("To pos"), 7);
    let v = compose_move_copy_section(&e).copy_part.unwrap();
    assert_eq!((v.start, v.length, v.destination), (2, 5, 7));
}

#[test]
fn test_compose_move_copy_separator() {
    let mut e = edit_state(Some("Move first n"), 3, 2, Some("To end"), 0);
    e.move_part_sep = "_".into();
    let v = compose_move_copy_section(&e).move_part.unwrap();
    assert_eq!(v.separator.as_deref(), Some("_"));
    // An empty separator means no separator.
    e.move_part_sep = String::new();
    assert_eq!(
        compose_move_copy_section(&e).move_part.unwrap().separator,
        None
    );
}

#[test]
fn test_compose_move_copy_move_action() {
    let e = edit_state(Some("Move first n"), 3, 2, Some("To end"), 0);
    let s = compose_move_copy_section(&e);
    let v = s.move_part.unwrap();
    assert_eq!((v.start, v.length, v.destination), (1, 3, -1));
    assert!(s.copy_part.is_none(), "move action must not set copy_part");

    let e = edit_state(Some("Move range"), 2, 5, Some("To pos"), -2);
    let v = compose_move_copy_section(&e).move_part.unwrap();
    assert_eq!((v.start, v.length, v.destination), (2, 5, -2));
}

#[test]
fn test_compose_move_copy_switch_copy_to_move() {
    // Start as a copy, then switch to a move: the stale copy must be cleared.
    let mut e = edit_state(Some("Copy first n"), 3, 2, Some("To end"), 0);
    let mut config = RenameConfig {
        move_copy: compose_move_copy_section(&e),
        ..Default::default()
    };
    assert!(config.move_copy.copy_part.is_some());

    e.move_part_action = Some("Move first n".into());
    config.move_copy = compose_move_copy_section(&e);
    assert!(
        config.move_copy.move_part.is_some() && config.move_copy.copy_part.is_none(),
        "switching to move must clear the previous copy so the preview stops applying it"
    );
}

#[test]
fn test_compose_move_copy_switch_move_to_copy() {
    let mut e = edit_state(Some("Move range"), 2, 5, Some("To pos"), 4);
    let mut config = RenameConfig {
        move_copy: compose_move_copy_section(&e),
        ..Default::default()
    };
    assert!(config.move_copy.move_part.is_some());

    e.move_part_action = Some("Copy range".into());
    config.move_copy = compose_move_copy_section(&e);
    assert!(
        config.move_copy.copy_part.is_some() && config.move_copy.move_part.is_none(),
        "switching to copy must clear the previous move"
    );
}

#[test]
fn test_compose_move_copy_no_action() {
    let e = edit_state(None, 3, 2, Some("To end"), 0);
    let s = compose_move_copy_section(&e);
    assert!(s.move_part.is_none() && s.copy_part.is_none());
}
#[test]
fn test_section_is_modified_movecopy_dup() {
    let mut config = RenameConfig::default();
    // Default: not modified
    assert!(
        !section_is_modified(&config, SectionId::MoveCopy),
        "default config should not show MoveCopy as modified"
    );

    // Setting move_part should mark as modified
    config.move_copy.move_part = Some(MoveCopyValue {
        start: 1,
        length: 3,
        destination: 5,
        ..Default::default()
    });
    assert!(
        section_is_modified(&config, SectionId::MoveCopy),
        "move_part set should mark MoveCopy as modified"
    );

    config.move_copy.move_part = None;
    assert!(
        !section_is_modified(&config, SectionId::MoveCopy),
        "clearing move_part should unmark MoveCopy"
    );

    // Setting copy_part should also mark as modified
    config.move_copy.copy_part = Some(MoveCopyValue {
        start: 2,
        length: 4,
        destination: 6,
        ..Default::default()
    });
    assert!(
        section_is_modified(&config, SectionId::MoveCopy),
        "copy_part set should mark MoveCopy as modified"
    );
}

#[test]
fn test_section_is_modified_movecopy() {
    let mut config = RenameConfig::default();
    // Default: not modified
    assert!(
        !section_is_modified(&config, SectionId::MoveCopy),
        "default config should not show MoveCopy as modified"
    );

    // Setting move_part should mark as modified
    config.move_copy.move_part = Some(MoveCopyValue {
        start: 1,
        length: 3,
        destination: 5,
        ..Default::default()
    });
    assert!(
        section_is_modified(&config, SectionId::MoveCopy),
        "move_part set should mark MoveCopy as modified"
    );

    config.move_copy.move_part = None;
    assert!(
        !section_is_modified(&config, SectionId::MoveCopy),
        "clearing move_part should unmark MoveCopy"
    );

    // Setting copy_part should also mark as modified
    config.move_copy.copy_part = Some(MoveCopyValue {
        start: 2,
        length: 4,
        destination: 6,
        ..Default::default()
    });
    assert!(
        section_is_modified(&config, SectionId::MoveCopy),
        "copy_part set should mark MoveCopy as modified"
    );
}

/// Position 0 is the default insert position and must not mark the Add
/// section modified, while non-zero/negative positions and insert text do.
#[test]
fn test_section_is_modified_add_zero_position() {
    let mut config = RenameConfig::default();

    // Position 0 alone is the default — not modified.
    config.add.add_at = Some(0);
    assert!(
        !section_is_modified(&config, SectionId::Add),
        "add_at = Some(0) should not mark Add as modified"
    );

    // Non-zero and negative positions are real changes — modified.
    config.add.add_at = Some(3);
    assert!(section_is_modified(&config, SectionId::Add));
    config.add.add_at = Some(-1);
    assert!(section_is_modified(&config, SectionId::Add));
    config.add.add_at = None;
    assert!(!section_is_modified(&config, SectionId::Add));

    // Insert text marks the section modified regardless of position.
    config.add.add_insert = Some("X".into());
    assert!(section_is_modified(&config, SectionId::Add));
}

/// Verify that every section is covered by both `clear_section` and `section_is_modified`.
#[test]
fn test_all_sections_clear_and_modified() {
    let test_sections = [
        SectionId::Regex,
        SectionId::Replace,
        SectionId::Remove,
        SectionId::Add,
        SectionId::Numbering,
        SectionId::Case,
        SectionId::Extension,
        SectionId::Name,
        SectionId::AutoDate,
        SectionId::AppendFolder,
        SectionId::MoveCopy,
        SectionId::Filters,
        SectionId::CopyTo,
        SectionId::NameSegment,
        SectionId::Special,
    ];

    for &section in &test_sections {
        let mut cfg = RenameConfig::default();

        match section {
            SectionId::Regex => cfg.regex.regex_match = Some("X".into()),
            SectionId::Replace => cfg.replace.replace = Some("X".into()),
            SectionId::Remove => cfg.remove.remove_first = Some(42),
            SectionId::Add => cfg.add.add_prefix = Some("X".into()),
            SectionId::Numbering => {
                cfg.numbering.numbering_mode = Some(awara::NumberingMode::Insert)
            }
            SectionId::Case => cfg.case.case_name = Some(awara::CaseMode::Upper),
            SectionId::Extension => {
                cfg.extension.extension_mode = Some(awara::ExtensionMode::Upper)
            }
            SectionId::Name => cfg.name.name_mode = Some("prefix".into()),
            SectionId::AutoDate => cfg.auto_date.add_date = Some("YYYY-MM-DD".into()),
            SectionId::AppendFolder => cfg.append_folder.add_dirname_pos = Some(0),
            SectionId::MoveCopy => {
                cfg.move_copy.move_part = Some(MoveCopyValue {
                    start: 1,
                    length: 2,
                    destination: 3,
                    ..Default::default()
                })
            }
            SectionId::Filters => cfg.filters.filter_files = false,
            SectionId::CopyTo => cfg.copy_to.output_dir = Some("/tmp".into()),
            SectionId::NameSegment => cfg.name_segment.copy_name_segment_from = 3,
            SectionId::Special => cfg.special.set_attributes = Some("readonly".into()),
        }

        assert!(
            section_is_modified(&cfg, section),
            "section {:?} should be modified after mutation",
            section
        );

        cfg.clear_section(section);

        assert!(
            !section_is_modified(&cfg, section),
            "section {:?} should not be modified after clear",
            section
        );
    }
}
#[test]
fn test_persist_edit_state_integrated_with_config() {
    // Verify that edit_state and rename_config round-trip together
    let mut app = GuiApp::new_with_config_for_test();
    app.config.replace.replace = Some("old".into());
    app.config.replace.with = Some("new".into());
    app.config.filters.filter_files = false;
    app.config.special.set_attributes = Some("readonly".into());
    app.edit_state.auto_date_type = Some("modified_curr".into());
    app.edit_state.move_part_action = Some("Move range".into());

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    GuiPersistedState::save_to(&path, &app);

    let loaded = GuiPersistedState::load_from(&path).unwrap();

    // Config fields preserved
    assert_eq!(loaded.doc.rename_config.replace.replace, Some("old".into()));
    assert_eq!(loaded.doc.rename_config.replace.with, Some("new".into()));
    assert!(!loaded.doc.rename_config.filters.filter_files);
    assert_eq!(
        loaded.doc.rename_config.special.set_attributes,
        Some("readonly".into())
    );

    // Edit state fields preserved
    assert_eq!(
        loaded.edit_state.auto_date_type,
        Some("modified_curr".into())
    );
    assert_eq!(
        loaded.edit_state.move_part_action,
        Some("Move range".into())
    );
}
#[test]
fn test_remove_commit_ops_by_paths_surgical() {
    let mut app = GuiApp::new_with_config_for_test();
    // Set up commits in two directories.
    let dir_a = PathBuf::from("/tmp/a");
    let dir_b = PathBuf::from("/tmp/b");

    app.commits_by_dir
        .entry(dir_a.clone())
        .or_default()
        .push(Commit {
            timestamp: std::time::Instant::now(),
            label: "A".into(),
            ops: vec![
                RenameOp {
                    original_path: dir_a.join("old.txt"),
                    new_path: dir_a.join("new.txt"),
                    original_name: "old.txt".into(),
                    new_name: "new.txt".into(),
                    was_copy: false,
                    relocated: false,
                    original_permissions: None,
                    applied_attributes: String::new(),
                    applied_timestamps_mask: 0u8,
                },
                RenameOp {
                    original_path: dir_a.join("keep.txt"),
                    new_path: dir_a.join("kept.txt"),
                    original_name: "keep.txt".into(),
                    new_name: "kept.txt".into(),
                    was_copy: false,
                    relocated: false,
                    original_permissions: None,
                    applied_attributes: String::new(),
                    applied_timestamps_mask: 0u8,
                },
            ],
        });
    app.commits_by_dir
        .entry(dir_b.clone())
        .or_default()
        .push(Commit {
            timestamp: std::time::Instant::now(),
            label: "B".into(),
            ops: vec![RenameOp {
                original_path: dir_b.join("x.txt"),
                new_path: dir_b.join("y.txt"),
                original_name: "x.txt".into(),
                new_name: "y.txt".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0u8,
            }],
        });

    // Remove ops referencing the trashed path.
    let trashed: HashSet<String> = [dir_a.join("new.txt").to_string_lossy().to_string()].into();
    app.remove_commit_ops_by_paths(&trashed);

    // Dir A should keep the "keep.txt→kept.txt" op, lose the "old.txt→new.txt" op.
    let a_ops = &app.commits_by_dir[&dir_a][0].ops;
    assert_eq!(a_ops.len(), 1, "dir A: only keep op should survive");
    assert_eq!(a_ops[0].original_name, "keep.txt");

    // Dir B should be untouched.
    let b_ops = &app.commits_by_dir[&dir_b][0].ops;
    assert_eq!(b_ops.len(), 1, "dir B: untouched");
    assert_eq!(b_ops[0].original_name, "x.txt");
}

#[test]
fn test_sweep_dead_commits_stale() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = PathBuf::from("/tmp/sweep_test");

    // Create a real file that will exist during the sweep.
    let tmp_dir = TempDir::new().unwrap();
    let live_path = tmp_dir.path().join("live.txt");
    std::fs::write(&live_path, b"data").unwrap();

    app.commits_by_dir
        .entry(dir.clone())
        .or_default()
        .push(Commit {
            timestamp: std::time::Instant::now(),
            label: "mixed".into(),
            ops: vec![
                // Dead: neither path exists
                RenameOp {
                    original_path: PathBuf::from("/tmp/ghost_a.txt"),
                    new_path: PathBuf::from("/tmp/ghost_b.txt"),
                    original_name: "ghost_a.txt".into(),
                    new_name: "ghost_b.txt".into(),
                    was_copy: false,
                    relocated: false,
                    original_permissions: None,
                    applied_attributes: String::new(),
                    applied_timestamps_mask: 0u8,
                },
                // Live: original_path exists
                RenameOp {
                    original_path: live_path.clone(),
                    new_path: PathBuf::from("/tmp/moved_away.txt"),
                    original_name: "live.txt".into(),
                    new_name: "moved_away.txt".into(),
                    was_copy: false,
                    relocated: false,
                    original_permissions: None,
                    applied_attributes: String::new(),
                    applied_timestamps_mask: 0u8,
                },
            ],
        });

    app.sweep_dead_commits();

    let ops = &app.commits_by_dir[&dir][0].ops;
    assert_eq!(ops.len(), 1, "only the live op should survive");
    assert_eq!(ops[0].original_name, "live.txt");
}
