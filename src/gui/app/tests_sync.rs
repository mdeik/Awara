use super::tests_util::preview_row;
use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_compute_preview_sync_numbering_respects_section_order() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let f1 = dir.path().join("a.txt");
    let f2 = dir.path().join("b.txt");
    File::create(&f1).unwrap();
    File::create(&f2).unwrap();

    app.all_files = vec![f1, f2];
    app.selection = vec![true, true];
    app.config.add.add_prefix = Some("A".to_string());
    app.config.numbering.numbering_mode = Some(awara::NumberingMode::Prefix);
    app.config.numbering.numbering_start = 1;
    app.config.numbering.numbering_increment = 1;

    // Numbering before Add: number first, then the prefix is prepended.
    app.config.section_order = vec!["Numbering".into(), "Add".into()];
    app.build_command_order();
    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);
    assert_eq!(app.preview[0].new_name(), "A1a.txt");
    assert_eq!(app.preview[1].new_name(), "A2b.txt");

    // Add before Numbering: prefix first, then the number is prepended.
    app.config.section_order = vec!["Add".into(), "Numbering".into()];
    app.build_command_order();
    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);
    assert_eq!(app.preview[0].new_name(), "1Aa.txt");
    assert_eq!(app.preview[1].new_name(), "2Ab.txt");
}

#[test]
fn test_compute_preview_sync_numbering_break_resets() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    for n in ["a1.txt", "a2.txt", "b1.txt", "b2.txt"] {
        File::create(dir.path().join(n)).unwrap();
    }
    app.all_files = ["a1.txt", "a2.txt", "b1.txt", "b2.txt"]
        .iter()
        .map(|n| dir.path().join(n))
        .collect();
    app.selection = vec![true; 4];
    app.config.numbering.numbering_mode = Some(awara::NumberingMode::Prefix);
    app.config.numbering.numbering_start = 1;
    app.config.numbering.numbering_increment = 1;
    app.config.numbering.numbering_sep = Some("_".into());
    app.config.numbering.numbering_break = Some(2);
    app.build_command_order();

    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);

    let names: Vec<&str> = app.preview.iter().map(|p| p.new_name()).collect();
    assert_eq!(names, vec!["1_a1.txt", "2_a2.txt", "1_b1.txt", "2_b2.txt"]);
}

#[test]
fn test_compute_preview_sync_no_numbering() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let f1 = dir.path().join("report.txt");
    let f2 = dir.path().join("data.csv");
    File::create(&f1).unwrap();
    File::create(&f2).unwrap();

    app.all_files = vec![f1, f2];
    app.selection = vec![true, true];
    app.config.replace.replace = Some("report".to_string());
    app.config.replace.with = Some("draft".to_string());

    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);

    assert!(!app.preview_dirty);
    assert!(!app.preview_pending);
    assert_eq!(app.preview_last_gen, app.preview_generation);
    assert_eq!(app.preview.len(), 2);
    // First file: "report.txt" → "draft.txt"
    assert!(app.preview[0].selected);
    assert_eq!(app.preview[0].new_name(), "draft.txt");
    // Second file: unchanged since "data" doesn't match "report"
    assert!(app.preview[1].selected);
    assert_eq!(app.preview[1].new_name(), "data.csv");
}

/// A mixed batch (changed / unchanged interleaved) keeps every row aligned:
/// unchanged files get a synthesized identity row, changed ones get their op.
#[test]
fn test_compute_preview_sync_mixed_changed_and_unchanged() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    for n in ["a1.txt", "b.txt", "a2.txt", "c.txt"] {
        File::create(dir.path().join(n)).unwrap();
    }
    app.all_files = ["a1.txt", "b.txt", "a2.txt", "c.txt"]
        .iter()
        .map(|n| dir.path().join(n))
        .collect();
    app.selection = vec![true; 4];
    app.config.replace.replace = Some("a".into());
    app.config.replace.with = Some("z".into());

    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);

    let names: Vec<&str> = app.preview.iter().map(|p| p.new_name()).collect();
    assert_eq!(names, ["z1.txt", "b.txt", "z2.txt", "c.txt"]);
}

#[test]
fn test_compute_preview_sync_respects_selection() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let f1 = dir.path().join("a.txt");
    let f2 = dir.path().join("b.txt");
    File::create(&f1).unwrap();
    File::create(&f2).unwrap();

    app.all_files = vec![f1, f2];
    app.selection = vec![true, false]; // only first selected
    app.config.add.add_prefix = Some("new_".to_string());

    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);

    assert_eq!(app.preview.len(), 2);
    // Selected: renamed
    assert!(app.preview[0].selected);
    assert_eq!(app.preview[0].new_name(), "new_a.txt");
    // Unselected: unchanged
    assert!(!app.preview[1].selected);
    assert_eq!(app.preview[1].new_name(), app.preview[1].original_name);
}

/// Deselecting a file must clear its preview in the row cache even when the
/// preview ops still hold the rename (selection changes without numbering
/// only rebuild the row cache, not the preview) — otherwise the diff
/// lingers on the deselected row.
#[test]
fn test_deselected_row_clears_preview_in_row_cache() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let f1 = dir.path().join("a.txt");
    let f2 = dir.path().join("b.txt");
    File::create(&f1).unwrap();
    File::create(&f2).unwrap();

    app.all_files = vec![f1, f2];
    app.selection = vec![true, true];
    app.config.add.add_prefix = Some("new_".to_string());

    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);

    // Both selected → both rows show a preview.
    assert_eq!(app.cached_rows.len(), 2);
    assert!(app.cached_rows[0].changed);
    assert_eq!(app.cached_rows[0].new_name, "new_a.txt");

    // Deselect the first file and rebuild the row cache without recomputing
    // the preview (the non-numbering selection-change path).
    app.selection = vec![false, true];
    app.rebuild_rows_cache_sync();

    assert!(
        !app.cached_rows[0].changed,
        "deselected row must not linger"
    );
    assert_eq!(app.cached_rows[0].new_name, "a.txt");
    assert!(app.cached_rows[0].orig_segs.is_none());
    assert!(app.cached_rows[0].new_segs.is_none());
    // The still-selected row keeps its preview.
    assert!(app.cached_rows[1].changed);
    assert_eq!(app.cached_rows[1].new_name, "new_b.txt");
}

#[test]
fn test_update_preview_falls_back_to_sync_when_thread_dead() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let f1 = dir.path().join("test.txt");
    File::create(&f1).unwrap();

    app.all_files = vec![f1];
    app.selection = vec![true];
    app.config.add.add_prefix = Some("pre_".to_string());
    app.preview_dirty = true;

    // preview_tx is a channel with no receiver (test constructor drops it)
    // So send() will fail and update_preview should fall back to sync
    app.update_preview();

    // Should have computed preview synchronously
    assert!(!app.preview_dirty);
    assert!(!app.preview_pending);
    assert_eq!(app.preview.len(), 1);
    assert_eq!(app.preview[0].new_name(), "pre_test.txt");
}

#[test]
fn test_check_scan_results_drains_preview_with_generation() {
    let mut app = GuiApp::new_with_config_for_test();
    // Swap in a live preview result channel
    let (preview_result_tx, new_preview_rx) = mpsc::channel();
    app.preview_rx = new_preview_rx;

    let expected_gen = 5u64;
    let preview_items = vec![preview_row(
        awara::RenameOp {
            original_path: PathBuf::from("a.txt"),
            new_path: PathBuf::from("b.txt"),
            original_name: "a.txt".into(),
            new_name: "b.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        },
        true,
        0,
    )];

    app.preview_pending = true;
    app.preview_generation = expected_gen;

    preview_result_tx
        .send((expected_gen, preview_items.clone()))
        .unwrap();

    app.check_scan_results();

    assert_eq!(app.preview.len(), 1);
    assert_eq!(app.preview[0].new_name(), "b.txt");
    assert_eq!(app.preview_last_gen, expected_gen);
    assert!(!app.preview_pending);
}

#[test]
fn test_check_scan_results_discards_stale_preview() {
    let mut app = GuiApp::new_with_config_for_test();
    let (preview_result_tx, new_preview_rx) = mpsc::channel();
    app.preview_rx = new_preview_rx;

    let stale_gen = 3u64;
    let current_gen = 5u64;
    let stale_items = vec![preview_row(
        awara::RenameOp {
            original_path: PathBuf::from("old.txt"),
            new_path: PathBuf::from("stale.txt"),
            original_name: "old.txt".into(),
            new_name: "stale.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        },
        true,
        0,
    )];

    // Current generation is higher than the result we're about to send
    app.preview_pending = true;
    app.preview_generation = current_gen;

    preview_result_tx.send((stale_gen, stale_items)).unwrap();

    app.check_scan_results();

    // Stale result should be discarded — preview stays empty
    assert!(app.preview.is_empty());
    assert_ne!(app.preview_last_gen, stale_gen);
    // Still pending since we haven't received the current gen result
    assert!(app.preview_pending);
}

#[test]
fn test_preview_generation_increments_on_each_update() {
    let mut app = GuiApp::new_with_config_for_test();
    let gen_before = app.preview_generation;

    app.preview_dirty = true;
    app.all_files = vec![PathBuf::from("a.txt")];
    app.file_names = vec!["/test/a.txt".to_string()];
    app.file_display_names = vec!["a.txt".to_string()];
    app.selection = vec![true];
    app.selection_generation += 1;

    app.update_preview();
    assert_eq!(
        app.preview_generation,
        gen_before + 1,
        "generation should increment on first meaningful dispatch"
    );

    // Make a semantically distinct change so the guard does not suppress it.
    app.config.remove.remove_first = Some(1);
    app.preview_dirty = true;
    app.update_preview();
    assert_eq!(
        app.preview_generation,
        gen_before + 2,
        "generation should increment again when config changes"
    );
}

// ── maybe_shrink tests ──

#[test]
fn test_maybe_shrink_large_capacity_with_small_len() {
    let mut v: Vec<u64> = Vec::with_capacity(20000);
    v.push(1); // len=1, capacity=20000 → len*10 < capacity → shrink
    maybe_shrink(&mut v);
    assert_eq!(v.len(), 1);
    // After shrink_to_fit, capacity should equal len
    assert_eq!(v.capacity(), 1);
}

#[test]
fn test_maybe_shrink_below_threshold() {
    let mut v: Vec<u64> = Vec::with_capacity(1000);
    // capacity=1000 < 16384 → no shrink
    v.push(1);
    maybe_shrink(&mut v);
    assert_eq!(v.capacity(), 1000);
}

#[test]
fn test_maybe_shrink_ok_ratio() {
    let mut v: Vec<u64> = Vec::with_capacity(20000);
    for i in 0..3000 {
        v.push(i);
    }
    // len=3000, capacity=20000, len*10=30000 > capacity → no shrink
    let cap_before = v.capacity();
    maybe_shrink(&mut v);
    assert_eq!(v.capacity(), cap_before);
}

#[test]
fn test_maybe_shrink_empty_vec() {
    let mut v: Vec<u64> = Vec::with_capacity(20000);
    // len=0, capacity=20000, len*10=0 < capacity → would shrink
    // But empty Vecs are fine to shrink
    maybe_shrink(&mut v);
    assert!(v.capacity() < 20000 || v.capacity() == 0);
}
#[test]
fn test_compiled_config_from_ref_matches_new() {
    let config = awara::RenameConfig {
        regex: awara::RegexSection {
            regex_match: Some(r"(\w+)_(\d+)".to_string()),
            regex_replace: Some(r"$2-$1".to_string()),
            ..Default::default()
        },
        remove: awara::RemoveSection {
            remove_words: Some("bad ugly".to_string()),
            double_spaces: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let via_new = CompiledConfig::new(config.clone());
    let via_from_ref = CompiledConfig::from_ref(&config);

    // Same regexes should be compiled
    assert_eq!(via_new.re_match.is_some(), via_from_ref.re_match.is_some());
    assert_eq!(via_new.re_words.len(), via_from_ref.re_words.len());
    assert_eq!(
        via_new.re_double_space.is_some(),
        via_from_ref.re_double_space.is_some()
    );
    // Config should be deep-copied equivalently
    assert_eq!(
        via_new.config.regex.regex_match,
        via_from_ref.config.regex.regex_match
    );
    assert_eq!(
        via_new.config.remove.remove_words,
        via_from_ref.config.remove.remove_words
    );
}

/// Rows are built from the plan by a forward cursor over the selected files, so
/// the row↔op mapping must stay exact even when actionable and non-actionable
/// files are interleaved (a file can be selected and still have no op because
/// the operation leaves its name unchanged).
///
/// This is the property the cursor rewrite could break with an off-by-one: a
/// shifted cursor would hand one file's new name to its neighbour.
#[test]
fn test_rows_align_with_plan_ops_when_interleaved() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    // Only the x* files match the replacement; the b* files stay unchanged.
    let names = ["x1.txt", "b1.txt", "x2.txt", "b2.txt", "x3.txt"];
    for n in names {
        File::create(dir.path().join(n)).unwrap();
    }

    app.all_files = names.iter().map(|n| dir.path().join(n)).collect();
    app.file_names = names
        .iter()
        .map(|n| dir.path().join(n).to_string_lossy().to_string())
        .collect();
    app.file_display_names = names.iter().map(|n| n.to_string()).collect();
    app.file_sizes = vec![0; names.len()];
    app.file_dates = vec![0; names.len()];
    app.file_is_dir = vec![false; names.len()];
    app.selection = vec![true; names.len()];
    app.config.replace.replace = Some("x".into());
    app.config.replace.with = Some("Y".into());
    app.build_command_order();

    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);

    // Three x* files change, in plan order; the b* files are identity rows.
    // The rows are the only product of the preview now, so the plan-derived
    // assertion became a row assertion.
    let changed_names: Vec<String> = app
        .preview
        .iter()
        .filter(|p| p.is_changed())
        .map(|p| p.new_name().to_string())
        .collect();
    assert_eq!(changed_names, ["Y1.txt", "Y2.txt", "Y3.txt"]);

    let row_names: Vec<&str> = app.preview.iter().map(|p| p.new_name()).collect();
    assert_eq!(
        row_names,
        ["Y1.txt", "b1.txt", "Y2.txt", "b2.txt", "Y3.txt"]
    );

    // Rows without a rename carry no new name at all; rows with one do.
    let changed: Vec<bool> = app.preview.iter().map(|p| p.is_changed()).collect();
    assert_eq!(changed, [true, false, true, false, true]);
    for (i, row) in app.preview.iter().enumerate() {
        assert_eq!(
            row.new_name.is_some(),
            changed[i],
            "row {i} should carry a new name exactly when it changed"
        );
        assert_eq!(row.index, i, "rows stay in file-index order");
    }
}

/// A row is the hot path for large listings — there is one per file, an identity
/// row for every unselected file — so its inline size is a deliberate budget:
/// two names, a selection flag and an index.
///
/// This guards the shape rather than a timing: rows used to carry a whole
/// `RenameOp` (~136 bytes incl. two `PathBuf`s, a permission snapshot and
/// attributes), and the plan was then deep-cloned into every row. Dropping that
/// took measured allocations for a 2000-file preview from 6.5 to 3.5 per file
/// with nothing selected (6.8 → 3.8 with 1% selected). The type having no op
/// field is the real guarantee; this catches the shape drifting back.
#[test]
fn test_row_stays_name_sized() {
    let size = std::mem::size_of::<PreviewItem>();
    assert!(
        size <= 72,
        "PreviewItem grew to {size} bytes; rows must stay name-sized, not op-sized"
    );
}
