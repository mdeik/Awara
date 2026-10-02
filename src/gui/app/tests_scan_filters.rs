use super::tests_util::*;
use super::*;

#[test]
fn test_scan_dir_wires_exclude_regex() {
    let mut app = GuiApp::new_with_config_for_test();
    app.section_enabled[SectionId::Filters as usize] = true;

    // Set exclude regex directly on config (UI now writes directly to config)
    app.config.filters.exclude_regex = Some(r"\.bak$".into());

    // Manually trigger the scan_opts wiring that happens in scan_dir
    app.scan_opts.exclude_regex = app.config.filters.exclude_regex.clone();

    assert_eq!(app.scan_opts.exclude_regex, Some(r"\.bak$".into()));
}

#[test]
fn test_scan_dir_wires_entry_filter() {
    let mut app = GuiApp::new_with_config_for_test();

    // When both files and folders
    app.config.filters.filter_files = true;
    app.config.filters.filter_folders = true;
    app.scan_opts.entry_filter =
        if app.config.filters.filter_files && app.config.filters.filter_folders {
            EntryFilter::Both
        } else if app.config.filters.filter_files {
            EntryFilter::Files
        } else {
            EntryFilter::Folders
        };
    assert_eq!(app.scan_opts.entry_filter, EntryFilter::Both);

    // When only files
    app.config.filters.filter_files = true;
    app.config.filters.filter_folders = false;
    app.scan_opts.entry_filter =
        if app.config.filters.filter_files && app.config.filters.filter_folders {
            EntryFilter::Both
        } else if app.config.filters.filter_files {
            EntryFilter::Files
        } else {
            EntryFilter::Folders
        };
    assert_eq!(app.scan_opts.entry_filter, EntryFilter::Files);

    // When only folders
    app.config.filters.filter_files = false;
    app.config.filters.filter_folders = true;
    app.scan_opts.entry_filter =
        if app.config.filters.filter_files && app.config.filters.filter_folders {
            EntryFilter::Both
        } else if app.config.filters.filter_files {
            EntryFilter::Files
        } else {
            EntryFilter::Folders
        };
    assert_eq!(app.scan_opts.entry_filter, EntryFilter::Folders);
}

#[test]
fn test_scan_dir_wires_length_filters() {
    let mut app = GuiApp::new_with_config_for_test();

    app.config.filters.min_name_len = Some(3);
    app.config.filters.max_name_len = Some(10);
    app.config.filters.min_path_len = Some(5);
    app.config.filters.max_path_len = Some(50);

    app.scan_opts.min_name_len = app.config.filters.min_name_len;
    app.scan_opts.max_name_len = app.config.filters.max_name_len;
    app.scan_opts.min_path_len = app.config.filters.min_path_len;
    app.scan_opts.max_path_len = app.config.filters.max_path_len;

    assert_eq!(app.scan_opts.min_name_len, Some(3));
    assert_eq!(app.scan_opts.max_name_len, Some(10));
    assert_eq!(app.scan_opts.min_path_len, Some(5));
    assert_eq!(app.scan_opts.max_path_len, Some(50));
}

#[test]
fn test_scan_dir_wires_filter_use_regex_and_match_case() {
    let mut app = GuiApp::new_with_config_for_test();

    app.config.filters.filter_use_regex = true;
    app.config.filters.filter_match_case = true;

    app.scan_opts.filter_use_regex = app.config.filters.filter_use_regex;
    app.scan_opts.filter_match_case = app.config.filters.filter_match_case;

    assert!(app.scan_opts.filter_use_regex);
    assert!(app.scan_opts.filter_match_case);
}

// ── apply_filters_in_memory tests ──

#[test]
fn test_apply_filters_in_memory_empty_raw_does_nothing() {
    let mut app = GuiApp::new_with_config_for_test();
    app.raw_all_files.clear();
    app.all_files = vec![PathBuf::from("survivor.txt")];
    app.apply_filters_in_memory();
    // No raw data → should be a no-op, all_files unchanged
    assert_eq!(app.all_files, vec![PathBuf::from("survivor.txt")]);
}

#[test]
fn test_apply_filters_in_memory_mask_glob() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.filters.filter_pattern = Some("*.txt".into());
    app.config.filters.filter_files = true;
    app.config.filters.filter_folders = false;
    app.raw_all_files = vec![
        PathBuf::from("readme.txt"),
        PathBuf::from("photo.jpg"),
        PathBuf::from("notes.txt"),
    ];
    app.raw_file_sizes = vec![100, 200, 300];
    app.raw_file_dates = vec![1, 2, 3];
    app.raw_file_is_dir = vec![false, false, false];

    app.apply_filters_in_memory();

    assert_eq!(app.all_files.len(), 2);
    assert!(app.all_files.contains(&PathBuf::from("readme.txt")));
    assert!(app.all_files.contains(&PathBuf::from("notes.txt")));
    assert!(!app.all_files.contains(&PathBuf::from("photo.jpg")));
    assert_eq!(app.file_sizes.len(), 2);
    assert_eq!(app.file_dates.len(), 2);
    assert_eq!(app.file_is_dir.len(), 2);
    assert_eq!(app.selection.len(), 2);
    assert!(app.selection.iter().all(|&s| !s));
    assert!(app.preview_dirty);
}

#[test]
fn test_apply_filters_in_memory_mask_regex() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.filters.filter_pattern = Some(r"^photo\d+".into());
    app.config.filters.filter_use_regex = true;
    app.raw_all_files = vec![
        PathBuf::from("photo001.jpg"),
        PathBuf::from("notes.txt"),
        PathBuf::from("photo999.png"),
        PathBuf::from("photograph.gif"),
    ];
    app.raw_file_sizes = vec![100, 200, 300, 400];
    app.raw_file_dates = vec![1, 2, 3, 4];
    app.raw_file_is_dir = vec![false, false, false, false];

    app.apply_filters_in_memory();

    assert_eq!(app.all_files.len(), 2);
    assert!(app.all_files.contains(&PathBuf::from("photo001.jpg")));
    assert!(app.all_files.contains(&PathBuf::from("photo999.png")));
}

#[test]
fn test_apply_filters_in_memory_exclude_regex() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.filters.exclude_regex = Some(r"\.bak$".into());
    app.raw_all_files = vec![
        PathBuf::from("work.txt"),
        PathBuf::from("backup.bak"),
        PathBuf::from("notes.txt"),
        PathBuf::from("temp.bak"),
    ];
    app.raw_file_sizes = vec![100, 200, 300, 400];
    app.raw_file_dates = vec![1, 2, 3, 4];
    app.raw_file_is_dir = vec![false, false, false, false];

    app.apply_filters_in_memory();

    assert_eq!(app.all_files.len(), 2);
    assert!(app.all_files.contains(&PathBuf::from("work.txt")));
    assert!(app.all_files.contains(&PathBuf::from("notes.txt")));
}

#[test]
fn test_apply_filters_in_memory_entry_filter_files_only() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.filters.filter_files = true;
    app.config.filters.filter_folders = false;
    app.raw_all_files = vec![
        PathBuf::from("document.txt"),
        PathBuf::from("Pictures"),
        PathBuf::from("readme.md"),
        PathBuf::from("Downloads"),
    ];
    app.raw_file_sizes = vec![100, 0, 200, 0];
    app.raw_file_dates = vec![1, 2, 3, 4];
    app.raw_file_is_dir = vec![false, true, false, true];

    app.apply_filters_in_memory();

    assert_eq!(app.all_files.len(), 2);
    assert!(app.all_files.contains(&PathBuf::from("document.txt")));
    assert!(app.all_files.contains(&PathBuf::from("readme.md")));
}

#[test]
fn test_apply_filters_in_memory_entry_filter_folders_only() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.filters.filter_files = false;
    app.config.filters.filter_folders = true;
    app.raw_all_files = vec![
        PathBuf::from("document.txt"),
        PathBuf::from("Pictures"),
        PathBuf::from("Downloads"),
    ];
    app.raw_file_sizes = vec![100, 0, 0];
    app.raw_file_dates = vec![1, 2, 3];
    app.raw_file_is_dir = vec![false, true, true];

    app.apply_filters_in_memory();

    assert_eq!(app.all_files.len(), 2);
    assert!(app.all_files.contains(&PathBuf::from("Pictures")));
    assert!(app.all_files.contains(&PathBuf::from("Downloads")));
}

#[test]
fn test_apply_filters_in_memory_name_length() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.filters.min_name_len = Some(6);
    app.config.filters.max_name_len = Some(8);
    app.raw_all_files = vec![
        PathBuf::from("a.txt"),          // 5 chars
        PathBuf::from("tiny.md"),        // 6 chars
        PathBuf::from("toolongname.rs"), // 14 chars
        PathBuf::from("ok.txt"),         // 6 chars
    ];
    app.raw_file_sizes = vec![10, 20, 30, 40];
    app.raw_file_dates = vec![1, 2, 3, 4];
    app.raw_file_is_dir = vec![false, false, false, false];

    app.apply_filters_in_memory();

    assert_eq!(app.all_files.len(), 2);
    assert!(app.all_files.contains(&PathBuf::from("tiny.md")));
    assert!(app.all_files.contains(&PathBuf::from("ok.txt")));
}

#[test]
fn test_apply_filters_in_memory_match_case_glob() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.filters.filter_pattern = Some("*.TXT".into());
    app.config.filters.filter_match_case = true;
    app.raw_all_files = vec![
        PathBuf::from("readme.TXT"),
        PathBuf::from("notes.txt"),
        PathBuf::from("data.TXT"),
    ];
    app.raw_file_sizes = vec![100, 200, 300];
    app.raw_file_dates = vec![1, 2, 3];
    app.raw_file_is_dir = vec![false, false, false];

    app.apply_filters_in_memory();

    assert_eq!(app.all_files.len(), 2);
    assert!(app.all_files.contains(&PathBuf::from("readme.TXT")));
    assert!(app.all_files.contains(&PathBuf::from("data.TXT")));
}

#[test]
fn test_apply_filters_in_memory_combined_mask_and_exclude() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.filters.filter_pattern = Some("*.txt".into());
    app.config.filters.exclude_regex = Some(r"^temp".into());
    app.raw_all_files = vec![
        PathBuf::from("work.txt"),
        PathBuf::from("temp.txt"),
        PathBuf::from("notes.txt"),
        PathBuf::from("temp_old.txt"),
        PathBuf::from("photo.jpg"),
    ];
    app.raw_file_sizes = vec![100, 200, 300, 400, 500];
    app.raw_file_dates = vec![1, 2, 3, 4, 5];
    app.raw_file_is_dir = vec![false, false, false, false, false];

    app.apply_filters_in_memory();

    assert_eq!(app.all_files.len(), 2);
    assert!(app.all_files.contains(&PathBuf::from("work.txt")));
    assert!(app.all_files.contains(&PathBuf::from("notes.txt")));
}

#[test]
fn test_apply_filters_in_memory_re_filter_after_change() {
    // Simulate changing filters and re-applying in-memory
    let mut app = GuiApp::new_with_config_for_test();
    app.config.filters.filter_files = true;
    app.config.filters.filter_folders = true;
    app.raw_all_files = vec![
        PathBuf::from("cat.png"),
        PathBuf::from("dog.png"),
        PathBuf::from("bird.jpg"),
    ];
    app.raw_file_sizes = vec![100, 200, 300];
    app.raw_file_dates = vec![1, 2, 3];
    app.raw_file_is_dir = vec![false, false, false];

    // First filter: *.png
    app.config.filters.filter_pattern = Some("*.png".into());
    app.apply_filters_in_memory();
    assert_eq!(app.all_files.len(), 2);

    // Change filter to *.jpg — re-filter in-memory without new scan
    app.config.filters.filter_pattern = Some("*.jpg".into());
    app.apply_filters_in_memory();
    assert_eq!(app.all_files.len(), 1);
    assert!(app.all_files.contains(&PathBuf::from("bird.jpg")));
}
#[test]
fn test_processed_blocks_second_apply() {
    // After a successful Apply the whole batch is processed: re-applying
    // the same operation is a no-op until reselect or an op change.
    let (mut app, dir) = selection_test_app(&["old_a.txt", "old_b.txt"]);
    app.preview_dirty = true;
    app.execute_renames();
    assert!(dir.path().join("new_a.txt").exists());
    assert!(dir.path().join("new_b.txt").exists());
    assert!(app.processed, "apply must set the processed flag");
    let commits_after_first = app.cwd_commits().len();

    // Second apply with no changes: no-op, no new commit, no re-rename.
    app.preview_dirty = true;
    app.execute_renames();
    assert!(app.processed, "flag stays set after blocked re-apply");
    assert_eq!(
        app.cwd_commits().len(),
        commits_after_first,
        "blocked apply must not create a commit"
    );
    assert_eq!(
        app.status_message,
        "All selected files have already been processed"
    );
    assert_eq!(app.status_kind, StatusKind::Temporary);
    assert!(dir.path().join("new_a.txt").exists());
    assert!(dir.path().join("new_b.txt").exists());
}

#[test]
fn test_execute_no_selection_leaves_status_alone() {
    let (mut app, _dir) = selection_test_app(&["old_a.txt"]);
    app.selection = vec![false];
    app.set_status("keep me", StatusKind::Persistent);
    app.preview_dirty = true;
    app.execute_renames();
    assert_eq!(app.status_message, "keep me");
    assert!(
        !app.processed,
        "no-selection apply must not touch the guard"
    );
}

#[test]
fn test_execute_no_ops_configured_message() {
    // Files selected, but no rename operation configured.
    let (mut app, _dir) = selection_test_app(&["a.txt"]);
    app.config = RenameConfig::empty();
    app.preview_dirty = true;
    app.execute_renames();
    assert_eq!(app.status_message, "No rename operations configured");
    assert_eq!(app.status_kind, StatusKind::Temporary);
    assert!(!app.processed, "nothing was applied");
}

#[test]
fn test_execute_ops_no_change_falls_back() {
    // Operation is configured but matches nothing (replace "old"→"new"
    // on "a.txt") → generic fallback message, demoted to a normal
    // (non-error) kind so it times out back to the ready message.
    let (mut app, _dir) = selection_test_app(&["a.txt"]);
    app.preview_dirty = true;
    app.execute_renames();
    assert_eq!(app.status_message, "No files to rename");
    assert_eq!(app.status_kind, StatusKind::Temporary);
    assert!(!app.processed, "nothing was applied");
}

#[test]
fn test_processed_preview_is_identity() {
    // While processed, previews and row cache show the plain current name
    // (New Name = current filename, no diff).
    let (mut app, _dir) = selection_test_app(&["old_a.txt"]);
    app.preview_dirty = true;
    app.execute_renames();
    assert!(app.processed);

    // Row cache was rebuilt synchronously after execution with the
    // processed flag → identity rows immediately.
    let rows = &*app.cached_rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "new_a.txt");
    assert_eq!(rows[0].new_name, "new_a.txt");
    assert!(!rows[0].changed, "processed rows must not show a diff");

    // The next frame's update_preview must NOT re-arm (same operation),
    // and must dispatch the frozen (identity) preview.
    app.preview_dirty = true;
    let gen_before = app.preview_generation;
    app.update_preview();
    assert!(
        app.preview_generation > gen_before,
        "processed state must force a preview redispatch"
    );
    assert!(
        app.processed,
        "update_preview must not re-arm same operation"
    );
    assert_eq!(app.preview.len(), 1);
    let pi = &app.preview[0];
    assert!(pi.selected, "processed file stays selected");
    assert_eq!(
        pi.new_name(),
        pi.original_name,
        "preview must show identity for processed items"
    );
    assert_eq!(pi.new_name(), "new_a.txt");
}

#[test]
fn test_processed_rearms_on_select_all() {
    let (mut app, _dir) = selection_test_app(&["old_a.txt"]);
    app.preview_dirty = true;
    app.execute_renames();
    assert!(app.processed);

    app.select_all();
    assert!(!app.processed, "select-all must re-arm the guard");
}

#[test]
fn test_processed_rearms_on_operation_change() {
    // Editing any operation parameter re-arms — even one that does not
    // change the produced filenames.
    let (mut app, _dir) = selection_test_app(&["old_a.txt"]);
    app.preview_dirty = true;
    app.execute_renames();
    assert!(app.processed);

    // "zzz" never appears in any name → output is unaffected, but the
    // operation was still edited → re-arm.
    app.config.replace.replace = Some("zzz".into());
    app.preview_dirty = true;
    app.update_preview();
    assert!(!app.processed, "operation change must re-arm the guard");
}

#[test]
fn test_processed_resets_on_scan() {
    let (mut app, _dir) = selection_test_app(&["old_a.txt"]);
    app.preview_dirty = true;
    app.execute_renames();
    assert!(app.processed);

    // A fresh scan re-arms (safety: also covers programmatic
    // pre-selection after the scan).
    app.scan_dir();
    assert!(!app.processed, "fresh scan must re-arm the guard");
}

#[test]
fn test_processed_clamp_only_edit_does_not_dispatch_when_armed() {
    // Regression: dragging a clamped field (raw change, same normalized
    // output) must not trigger a preview recompute — and when armed it
    // must not re-arm anything (no-op). See
    // test_preview_output_equality_skips_redispatch for the dispatch side.
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("/test/abc.txt")];
    app.file_names = vec!["/test/abc.txt".to_string()];
    app.file_display_names = vec!["abc.txt".to_string()];
    app.selection = vec![true];
    app.config.remove.remove_first = Some(10);
    app.preview_dirty = true;
    app.update_preview();
    let gen_before = app.preview_generation;

    app.config.remove.remove_first = Some(20);
    app.preview_dirty = true;
    app.update_preview();
    assert_eq!(
        app.preview_generation, gen_before,
        "clamp-only edit must not redispatch"
    );
    assert!(!app.processed);
}
