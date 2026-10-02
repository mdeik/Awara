use super::tests_util::preview_row;
use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_parallel_compute_preview_equivalence() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();

    let mut files = Vec::new();
    for i in 0..100 {
        let p = dir.path().join(format!("file_{:03}.txt", i));
        File::create(&p).unwrap();
        files.push(p.to_string_lossy().to_string());
    }

    app.file_names = files.clone();
    app.selection = vec![true; 100];
    app.config.replace.replace = Some("file_".into());
    app.config.replace.with = Some("document_".into());

    let cancel_flag = std::sync::atomic::AtomicBool::new(false);
    let params = PreviewParams {
        files: &app.file_names,
        selection: &app.selection,
        config: &app.config,
        section_enabled: &app.section_enabled,
        num_order: &(0..100).collect::<Vec<_>>(),
        cwd: dir.path(),
        processed: false,
    };

    let items = compute_preview(params, &cancel_flag).expect("preview computation succeeded");
    assert_eq!(items.len(), 100);
    assert_eq!(items[0].new_name(), "document_000.txt");
    assert_eq!(items[99].new_name(), "document_099.txt");
}

#[test]
fn test_large_dataset_performance_smoke() {
    let mut app = GuiApp::new_with_config_for_test();
    let num_files = 160_000;
    app.selection = vec![false; num_files];

    let start = std::time::Instant::now();
    // Select all
    app.selection.fill(true);
    app.selection_generation += 1;
    app.cached_selected_count.set(None);

    let count = app.selected_count();
    let elapsed = start.elapsed();

    assert_eq!(count, num_files);
    assert!(
        elapsed < std::time::Duration::from_millis(100),
        "160,000 item selection count took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_parallel_sorting_correctness() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();

    let mut all_files = Vec::new();
    let mut display_names = Vec::new();
    for name in &["zebra.txt", "alpha.txt", "charlie.txt", "bravo.txt"] {
        let p = dir.path().join(name);
        File::create(&p).unwrap();
        all_files.push(p);
        display_names.push((*name).to_string());
    }

    app.all_files = all_files;
    app.file_display_names = display_names;
    app.file_sizes = vec![100, 400, 200, 300];
    app.file_dates = vec![10, 40, 20, 30];
    app.file_is_dir = vec![false, false, false, false];

    // Test Name ASC sorting
    app.preview_sort_col = PreviewSortCol::Name;
    app.preview_sort_asc = true;
    let sorted = app.sorted_indices();
    assert_eq!(*sorted, vec![1, 3, 2, 0]); // alpha (1), bravo (3), charlie (2), zebra (0)

    // Test Size ASC sorting
    app.invalidate_sorted_cache();
    app.preview_sort_col = PreviewSortCol::Size;
    let sorted = app.sorted_indices();
    assert_eq!(*sorted, vec![0, 2, 3, 1]); // 100, 200, 300, 400
}
#[test]
fn test_preview_output_equality_skips_redispatch() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("/test/abc.txt")];
    app.file_names = vec!["/test/abc.txt".to_string()];
    app.file_display_names = vec!["abc.txt".to_string()];
    app.selection = vec![true];

    // 1. First dispatch with remove_first = 10 (exceeding filename length 7)
    app.config.remove.remove_first = Some(10);
    app.preview_dirty = true;
    app.update_preview();
    assert_eq!(app.preview_generation, 1);
    assert!(!app.preview_dirty);

    // 2. Second dispatch with remove_first = 20 (also exceeding filename length 7)
    // Normalized config clamps both to max length 7 -> semantic equality matches
    app.config.remove.remove_first = Some(20);
    app.preview_dirty = true;
    app.update_preview();

    // Preview generation should NOT increment, and preview_dirty should be cleared immediately
    assert_eq!(
        app.preview_generation, 1,
        "Redundant generation should not be dispatched"
    );
    assert!(!app.preview_dirty);

    // 3. Changing to a meaningful difference (remove_first = 2 < 7) must dispatch
    app.config.remove.remove_first = Some(2);
    app.preview_dirty = true;
    app.update_preview();
    assert_eq!(
        app.preview_generation, 2,
        "Meaningful change must dispatch generation 2"
    );
}

#[test]
fn test_preview_same_as_current_skips_row_cache_rebuild() {
    let (preview_tx, preview_rx) = std::sync::mpsc::channel();
    let mut app = GuiApp::new_with_config_for_test();
    app.preview_rx = preview_rx;

    app.all_files = vec![PathBuf::from("/test/abc.txt")];
    app.file_names = vec!["/test/abc.txt".to_string()];
    app.file_display_names = vec!["abc.txt".to_string()];
    app.selection = vec![true];

    // Seed initial preview
    let initial_items = vec![preview_row(
        RenameOp {
            original_path: PathBuf::from("/test/abc.txt"),
            new_path: PathBuf::from("/test/abc.txt"),
            original_name: "abc.txt".to_string(),
            new_name: "abc.txt".to_string(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0,
        },
        true,
        0,
    )];
    app.preview = Arc::new(initial_items);

    let initial_rows_gen = app.rows_cache_gen;

    // Simulate incoming preview result with identical contents
    app.preview_generation = 5;
    app.preview_pending = true;
    let identical_items = vec![preview_row(
        RenameOp {
            original_path: PathBuf::from("/test/abc.txt"),
            new_path: PathBuf::from("/test/abc.txt"),
            original_name: "abc.txt".to_string(),
            new_name: "abc.txt".to_string(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0,
        },
        true,
        0,
    )];
    preview_tx.send((5, identical_items)).unwrap();

    // Process incoming preview results
    app.check_scan_results();

    // rows_cache_gen should NOT increment because same_as_current skipped rebuild_rows_cache()
    assert_eq!(
        app.rows_cache_gen, initial_rows_gen,
        "Identical preview output must skip rebuild_rows_cache"
    );
    assert!(!app.preview_pending);
}

#[test]
fn test_preview_selection_only_change_rebuilds_rows_cache() {
    // Regression: a preview recompute that differs only in selection-ness
    // (identity → identity — e.g. select a row, then reset a section so
    // the rename disappears) must rebuild the row cache. Otherwise the
    // rows keep showing the stale new name from the previous rebuild.
    let (preview_tx, preview_rx) = std::sync::mpsc::channel();
    let mut app = GuiApp::new_with_config_for_test();
    app.preview_rx = preview_rx;

    app.all_files = vec![PathBuf::from("/test/a.txt")];
    app.file_names = vec!["/test/a.txt".to_string()];
    app.file_display_names = vec!["a.txt".to_string()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.file_is_dir = vec![false];
    app.selection = vec![true];

    // Stale preview: identity op computed while the row was deselected.
    app.preview = Arc::new(vec![preview_row(
        RenameOp {
            original_path: PathBuf::from("/test/a.txt"),
            new_path: PathBuf::from("/test/a.txt"),
            original_name: "a.txt".to_string(),
            new_name: "a.txt".to_string(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0,
        },
        false,
        0,
    )]);
    // Seed an empty (clearly stale) row cache.
    app.cached_rows = std::rc::Rc::new(Vec::new());
    let rows_gen_before = app.rows_cache_gen;

    // Incoming result: identical identity op, but the row is selected.
    app.preview_generation = 9;
    app.preview_pending = true;
    preview_tx
        .send((
            9,
            vec![preview_row(
                RenameOp {
                    original_path: PathBuf::from("/test/a.txt"),
                    new_path: PathBuf::from("/test/a.txt"),
                    original_name: "a.txt".to_string(),
                    new_name: "a.txt".to_string(),
                    was_copy: false,
                    relocated: false,
                    original_permissions: None,
                    applied_attributes: String::new(),
                    applied_timestamps_mask: 0,
                },
                true,
                0,
            )],
        ))
        .unwrap();

    app.check_scan_results();

    assert!(
        app.rows_cache_gen > rows_gen_before,
        "selection-only preview change must rebuild the row cache"
    );
    assert_eq!(app.cached_rows.len(), 1);
    assert_eq!(
        app.cached_rows[0].new_name, "a.txt",
        "stale new name must be cleared"
    );
    assert!(!app.cached_rows[0].changed);
}

#[test]
fn test_build_rows_cache_precomputes_diff_segments() {
    let cancel = Arc::new(AtomicBool::new(false));
    let preview = Arc::new(vec![
        // 0: Changed (prefix added)
        preview_row(
            RenameOp {
                original_path: PathBuf::from("/test/abc.txt"),
                new_path: PathBuf::from("/test/pre_abc.txt"),
                original_name: "abc.txt".to_string(),
                new_name: "pre_abc.txt".to_string(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0,
            },
            true,
            0,
        ),
        // 1: Changed (middle replacement)
        preview_row(
            RenameOp {
                original_path: PathBuf::from("/test/photo_2020_raw.jpg"),
                new_path: PathBuf::from("/test/photo_2026_raw.jpg"),
                original_name: "photo_2020_raw.jpg".to_string(),
                new_name: "photo_2026_raw.jpg".to_string(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0,
            },
            true,
            1,
        ),
        // 2: Unchanged
        preview_row(
            RenameOp {
                original_path: PathBuf::from("/test/doc.pdf"),
                new_path: PathBuf::from("/test/doc.pdf"),
                original_name: "doc.pdf".to_string(),
                new_name: "doc.pdf".to_string(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0,
            },
            true,
            2,
        ),
    ]);

    let req = RowsCacheRequest {
        r#gen: 1,
        files: vec![
            "/test/abc.txt".to_string(),
            "/test/photo_2020_raw.jpg".to_string(),
            "/test/doc.pdf".to_string(),
        ],
        sizes: vec![100, 200, 300],
        dates: vec![0, 0, 0],
        is_dirs: vec![false, false, false],
        preview,
        status_map: HashMap::new(),
        selection: vec![true, true, true],
        config: RenameConfig::default(),
        section_enabled: [true; SectionId::COUNT],
        processed: false,
        selection_only: false,
        fp: RowsDatasetFp {
            preview_generation: 0,
            scan_generation: 0,
            processed_generation: 0,
            rows_dataset_gen: 0,
            file_count: 3,
            working: cleared_working_config(&RenameConfig::default(), &[true; SectionId::COUNT]),
        },
        cancel: Arc::clone(&cancel),
    };

    let rows = build_rows_cache(&req, None);

    // Row 0: changed, diff segments precomputed
    assert_eq!(rows[0].name, "abc.txt");
    assert_eq!(rows[0].new_name, "pre_abc.txt");
    assert!(rows[0].changed);
    assert!(rows[0].orig_segs.is_some());
    assert!(rows[0].new_segs.is_some());

    // Row 1: changed, diff segments precomputed
    assert_eq!(rows[1].name, "photo_2020_raw.jpg");
    assert_eq!(rows[1].new_name, "photo_2026_raw.jpg");
    assert!(rows[1].changed);
    assert!(rows[1].orig_segs.is_some());
    assert!(rows[1].new_segs.is_some());

    // Row 2: unchanged, diff segments are None
    assert_eq!(rows[2].name, "doc.pdf");
    assert_eq!(rows[2].new_name, "doc.pdf");
    assert!(!rows[2].changed);
    assert!(rows[2].orig_segs.is_none());
    assert!(rows[2].new_segs.is_none());
}
