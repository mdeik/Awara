use super::tests_util::*;
use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_preview_updated_after_selection_change() {
    // Verify that the detection in draw_file_and_preview works:
    // after selection changes, preview_dirty is set, so update_preview
    // will recompute with the new selection.
    let files = ["old_a.txt", "old_b.txt"];
    let (mut app, _dir) = selection_test_app(&files);

    app.preview_dirty = true;
    app.update_preview();
    assert!(!app.preview_dirty, "preview should be clean after update");

    // Both selected → both show new names
    assert!(app.preview.iter().all(|p| p.selected));

    // Simulate what happens after a frame: user deselects item 0 via checkbox.
    // When draw_file_and_preview runs, the deferred check detects the mismatch
    // and sets preview_dirty = true.
    app.selection[0] = false;

    // This is what the deferred code at the end of draw_file_and_preview does:
    let selection_changed = app
        .preview
        .iter()
        .any(|pr| app.selection.get(pr.index).copied().unwrap_or(true) != pr.selected);
    assert!(selection_changed);
    // Bump selection_generation just as the real toggle helpers do.
    app.selection_generation += 1;
    app.preview_dirty = true;

    // Re-run preview
    app.update_preview();

    // Now item 0 should be unchanged
    assert!(!app.preview[0].selected);
    assert_eq!(
        app.preview[0].new_name(),
        app.preview[0].original_name,
        "item 0 should be unchanged after deselection"
    );
    // Item 1 still selected → renamed
    assert!(app.preview[1].selected);
    assert_ne!(
        app.preview[1].new_name(),
        app.preview[1].original_name,
        "item 1 should still be renamed"
    );
}
#[test]
fn test_rebuild_rows_cache_basic() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let p1 = dir.path().join("report.txt");
    let p2 = dir.path().join("data.csv");
    File::create(&p1).unwrap();
    File::create(&p2).unwrap();

    app.all_files = vec![p1.clone(), p2.clone()];
    app.file_sizes = vec![100, 200];
    app.file_dates = vec![5000, 6000];
    app.selection = vec![true, true];

    // Set up preview items with a change
    app.preview = Arc::new(vec![
        preview_row(
            awara::RenameOp {
                original_path: p1.clone(),
                new_path: dir.path().join("renamed_report.txt"),
                original_name: "report.txt".into(),
                new_name: "renamed_report.txt".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0u8,
            },
            true,
            0,
        ),
        preview_row(
            awara::RenameOp {
                original_path: p2.clone(),
                new_path: p2.clone(),
                original_name: "data.csv".into(),
                new_name: "data.csv".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0u8,
            },
            true,
            1,
        ),
    ]);

    app.rebuild_rows_cache();

    assert_eq!(app.cached_rows.len(), 2);
    assert_eq!(app.cached_rows[0].name, "report.txt");
    assert_eq!(app.cached_rows[0].new_name, "renamed_report.txt");
    assert!(app.cached_rows[0].changed);
    assert_eq!(app.cached_rows[0].size, 100);
    assert_eq!(app.cached_rows[0].date, 5000);
    assert!(app.cached_rows[0].status.is_none()); // no cwd_commits entries

    assert_eq!(app.cached_rows[1].name, "data.csv");
    assert_eq!(app.cached_rows[1].new_name, "data.csv");
    assert!(!app.cached_rows[1].changed);
    assert_eq!(app.cached_rows[1].size, 200);
}

#[test]
fn test_rebuild_rows_cache_with_status() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let p1 = dir.path().join("a.txt");
    let p2 = dir.path().join("b.txt");
    let p3 = dir.path().join("c.txt");
    File::create(&p1).unwrap();
    File::create(&p2).unwrap();
    File::create(&p3).unwrap();

    app.all_files = vec![p1.clone(), p2.clone(), p3.clone()];
    app.file_sizes = vec![0, 0, 0];
    app.file_dates = vec![0, 0, 0];

    // Set undo_stack so rebuild_rows_cache picks up status (0=changed) for files
    // that appear in commit ops.
    app.commits_by_dir
        .entry(app.cwd.clone())
        .or_default()
        .push(Commit {
            timestamp: std::time::Instant::now(),
            label: "test".into(),
            ops: vec![
                awara::RenameOp {
                    original_path: p1.clone(),
                    new_path: PathBuf::from("a_renamed.txt"),
                    original_name: "a.txt".into(),
                    new_name: "a_renamed.txt".into(),
                    was_copy: false,
                    relocated: false,
                    original_permissions: None,
                    applied_attributes: String::new(),
                    applied_timestamps_mask: 0u8,
                },
                awara::RenameOp {
                    original_path: p2.clone(),
                    new_path: PathBuf::from("b_renamed.txt"),
                    original_name: "b.txt".into(),
                    new_name: "b_renamed.txt".into(),
                    was_copy: false,
                    relocated: false,
                    original_permissions: None,
                    applied_attributes: String::new(),
                    applied_timestamps_mask: 0u8,
                },
            ],
        });

    // Must also populate status_map directly (ensure_status_map no longer rebuilds).
    let p1_str = p1.to_string_lossy().to_string();
    let p2_str = p2.to_string_lossy().to_string();
    app.status_map.insert(p1_str, (0u8, None));
    app.status_map.insert(p2_str, (0u8, None));

    app.rebuild_rows_cache();

    assert_eq!(app.cached_rows.len(), 3);
    // Status map keyed by path string -> 0 for any path that was in any commit op
    assert_eq!(
        app.cached_rows[0].status,
        Some(0),
        "a.txt should have status (in commit)"
    );
    assert_eq!(
        app.cached_rows[1].status,
        Some(0),
        "b.txt should have status (in commit)"
    );
    assert!(
        app.cached_rows[2].status.is_none(),
        "c.txt should have no status (not in commit)"
    );
}

/// The row cache's lazy recompute must use the planner's no-op rule
/// (`RenameOp::is_noop`, which normalizes `.` segments and trailing separators),
/// not a textual name comparison — otherwise a row shows a name the plan
/// dropped, and it can disagree with the preview when the operation is toggled.
#[test]
fn test_row_cache_matches_planner_for_path_equal_name() {
    // No extension, so an added suffix lands at the very end: `a` -> `a/`, which
    // is textually different but path-equal (trailing separator).
    let (mut app, _dir) = selection_test_app(&["a"]);
    app.config.replace.replace = None;
    app.config.replace.with = None;
    app.config.add.add_suffix = Some("/".into());
    app.section_enabled[SectionId::Add as usize] = true;
    app.preview_dirty = true;
    app.update_preview();
    app.rebuild_rows_cache_sync();

    assert_eq!(app.preview.len(), 1);
    assert!(
        !app.preview[0].is_changed(),
        "the plan drops a path-equal name"
    );
    assert!(
        !app.cached_rows[0].changed,
        "row must not show a name the plan dropped: {:?}",
        app.cached_rows[0].new_name
    );
}

/// `Row::missing` must be `true` for a path that no longer exists on disk and
/// `false` for one that does. This flag is set in `build_row` via `Path::exists`
/// so the render site can cross out the row without a separate scan pass.
#[test]
fn test_row_missing_flag_reflects_disk_state() {
    let tmp = TempDir::new().unwrap();
    let present = tmp.path().join("here.txt");
    let absent = tmp.path().join("gone.txt");
    File::create(&present).unwrap();
    // `absent` is intentionally never created.

    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![present.clone(), absent.clone()];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.selection = vec![false, false];

    app.rebuild_rows_cache_sync();

    assert_eq!(app.cached_rows.len(), 2);
    assert!(
        !app.cached_rows[0].missing,
        "existing file must not be flagged as missing"
    );
    assert!(
        app.cached_rows[1].missing,
        "file deleted outside the GUI must be flagged as missing"
    );
}
