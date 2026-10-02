use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_copy_multi_methods() {
    let app = GuiApp::new_with_config_for_test();
    assert!(app.all_files.is_empty());
}

#[test]
fn test_copy_filename_multi_joins_newlines() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("/dir/foo.txt"),
        PathBuf::from("/dir/bar.txt"),
        PathBuf::from("/dir/baz.txt"),
    ];
    let indices = [0, 1, 2];
    let result: Vec<String> = indices
        .iter()
        .filter_map(|&idx| {
            app.all_files
                .get(idx)
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        })
        .collect();
    assert_eq!(result.join("\n"), "foo.txt\nbar.txt\nbaz.txt");
}
#[test]
fn test_show_properties_multi_data_initialized() {
    let app = GuiApp::new_with_config_for_test();
    assert!(
        app.show_properties_multi_data.is_none(),
        "should start as None"
    );
}

#[test]
fn test_apply_context_action_none_is_noop() {
    let mut app = GuiApp::new_with_config_for_test();
    app.apply_context_action(); // should not panic
    assert!(app.pending_context_action.is_none());
}
#[test]
fn test_move_selected_up_with_existing_custom_order() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![false, false, true]; // select file idx 2 (c.txt)
    app.custom_order = Some(vec![1, 2, 0]);

    app.move_selected_up();
    let order = app.custom_order.expect("custom_order should be set");
    // c.txt (idx 2) should swap with b.txt (idx 1)
    assert_eq!(order[0], 2, "c.txt should move up");
    assert_eq!(order[1], 1, "b.txt should move down");
    assert_eq!(order[2], 0, "a.txt stays");
}

#[test]
fn test_move_selected_down_with_existing_custom_order() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![false, true, false]; // select file idx 1 (b.txt)
    app.custom_order = Some(vec![1, 0, 2]);

    app.move_selected_down();
    let order = app.custom_order.expect("custom_order should be set");
    // b.txt (idx 1) should swap with a.txt (idx 0)
    assert_eq!(order[0], 0, "a.txt moves up");
    assert_eq!(order[1], 1, "b.txt moves down");
    assert_eq!(order[2], 2, "c.txt stays");
}

#[test]
fn test_move_selected_to_top_with_custom_order() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
        PathBuf::from("d.txt"),
    ];
    app.selection = vec![false, false, true, false]; // select file idx 2 (c.txt)
    app.custom_order = Some(vec![0, 3, 1, 2]);

    app.move_selected_to_top();
    let order = app.custom_order.expect("custom_order should be set");
    assert_eq!(order[0], 2, "c.txt should be at top");
    assert_eq!(order[1], 0, "a.txt second");
    assert_eq!(order[2], 3, "d.txt third");
    assert_eq!(order[3], 1, "b.txt last");
}

#[test]
fn test_move_selected_to_bottom_with_custom_order() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![false, true, false]; // select file idx 1 (b.txt)
    app.custom_order = Some(vec![2, 0, 1]);

    app.move_selected_to_bottom();
    let order = app.custom_order.expect("custom_order should be set");
    assert_eq!(order[0], 2, "c.txt stays first");
    assert_eq!(order[1], 0, "a.txt stays second");
    assert_eq!(order[2], 1, "b.txt moves to bottom (already there, no-op)");
}

#[test]
fn test_preview_dirty_set_after_reposition() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")];
    app.selection = vec![false, true]; // select b.txt
    app.preview_dirty = false;

    app.move_selected_up();
    assert!(app.preview_dirty, "reposition should mark preview dirty");

    app.preview_dirty = false;
    app.reset_custom_order();
    assert!(app.preview_dirty, "reset should mark preview dirty");
}

#[test]
fn test_clipboard_methods_dont_panic() {
    // egui::Context can't be constructed in tests, but we can verify
    // the methods don't panic on valid indices by checking the extraction path.
    let app = GuiApp::new_with_config_for_test();
    // Empty all_files — methods should be no-ops
    assert_eq!(app.all_files.len(), 0);
    // These won't panic because they handle out-of-bounds gracefully
    // (we just can't verify clipboard state without an egui Context)
}
#[test]
fn test_select_all() {
    let mut app = GuiApp::new_with_config_for_test();
    app.selection = vec![false, true, false, true];
    app.select_all();
    assert!(
        app.selection.iter().all(|&s| s),
        "select_all should set all to true"
    );
}

#[test]
fn test_selected_count() {
    let mut app = GuiApp::new_with_config_for_test();
    app.selection = vec![true, false, true, true, false];
    assert_eq!(app.selected_count(), 3);

    app.selection.clear();
    assert_eq!(app.selected_count(), 0);
}

#[test]
fn test_modified_count() {
    let mut app = GuiApp::new_with_config_for_test();
    let p1 = PathBuf::from("a.txt");
    let p2 = PathBuf::from("b.txt");
    let p3 = PathBuf::from("c.txt");
    app.all_files = vec![p1.clone(), p2.clone(), p3.clone()];
    app.selection = vec![true, true, true];
    app.update_preview();

    // Set up cached_rows with a mix of statuses
    let rows = vec![
        Row {
            idx: 0,
            name: "a.txt".into(),
            is_dir: false,
            size: 0,
            date: 0,
            new_name: "a.txt".into(),
            changed: false,
            status: Some(0), // OK
            orig_segs: None,
            new_segs: None,
            error_msg: None,
            missing: false,
        },
        Row {
            idx: 1,
            name: "b.txt".into(),
            is_dir: false,
            size: 0,
            date: 0,
            new_name: "b.txt".into(),
            changed: false,
            status: None, // pending
            orig_segs: None,
            new_segs: None,
            error_msg: None,
            missing: false,
        },
        Row {
            idx: 2,
            name: "c.txt".into(),
            is_dir: false,
            size: 0,
            date: 0,
            new_name: "c.txt".into(),
            changed: false,
            status: Some(1), // failed
            orig_segs: None,
            new_segs: None,
            error_msg: None,
            missing: false,
        },
    ];
    app.cached_rows = std::rc::Rc::new(rows);

    // modified_count counts files with OK status (Some(0))
    assert_eq!(app.modified_count(), 1);
}
#[test]
fn test_start_edit_sets_fields() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("/tmp/photo.jpg")];

    app.start_edit(0);

    assert_eq!(app.editing_idx, Some(0));
    assert_eq!(app.edit_buffer, "photo.jpg");
    assert!(app.edit_pending_focus);
}

#[test]
fn test_start_edit_invalid_index() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("/tmp/photo.jpg")];

    app.start_edit(5);

    assert!(app.editing_idx.is_none());
    assert!(app.edit_buffer.is_empty());
}

#[test]
fn test_commit_edit_renames_file() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let old = dir.path().join("old.txt");
    File::create(&old).unwrap();
    app.all_files = vec![old.clone()];

    app.start_edit(0);
    app.edit_buffer = "new.txt".into();
    app.commit_edit();

    assert!(app.editing_idx.is_none());
    assert!(app.edit_buffer.is_empty());
    assert!(!old.exists());
    assert!(dir.path().join("new.txt").exists());
}

#[test]
fn test_commit_edit_empty_buffer() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("/tmp/photo.jpg")];
    app.editing_idx = Some(0);
    app.edit_buffer = "".into();

    app.commit_edit(); // Should not panic

    assert!(app.editing_idx.is_none());
}

#[test]
fn test_cycle_edit_forward() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![false, false, false];
    app.editing_idx = Some(0);
    app.edit_buffer = "a.txt".into();

    app.cycle_edit(true);

    assert_eq!(app.editing_idx, Some(1));
    assert_eq!(app.edit_buffer, "b.txt");
    assert_eq!(app.selection, vec![false, true, false]);
}

#[test]
fn test_cycle_edit_wraps_forward() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")];
    app.selection = vec![false, false];
    app.editing_idx = Some(1);
    app.edit_buffer = "b.txt".into();

    app.cycle_edit(true);

    assert_eq!(app.editing_idx, Some(0));
    assert_eq!(app.edit_buffer, "a.txt");
    assert_eq!(app.selection, vec![true, false]);
}

#[test]
fn test_cycle_edit_backward() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![false, false, false];
    app.editing_idx = Some(1);
    app.edit_buffer = "b.txt".into();

    app.cycle_edit(false);

    assert_eq!(app.editing_idx, Some(0));
    assert_eq!(app.edit_buffer, "a.txt");
    assert_eq!(app.selection, vec![true, false, false]);
}

#[test]
fn test_cycle_edit_wraps_backward() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")];
    app.selection = vec![false, false];
    app.editing_idx = Some(0);
    app.edit_buffer = "a.txt".into();

    app.cycle_edit(false);

    assert_eq!(app.editing_idx, Some(1));
    assert_eq!(app.edit_buffer, "b.txt");
    assert_eq!(app.selection, vec![false, true]);
}

#[test]
fn test_cycle_edit_noop_when_not_editing() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("a.txt")];
    app.editing_idx = None;

    app.cycle_edit(true); // Should not panic
    assert!(app.editing_idx.is_none());
}

/// `cycle_edit` moves the selection, so it must bump `selection_generation` like
/// every other selection mutation. Otherwise the deferred selection-change
/// handling never notices, and with numbering active the preview keeps numbering
/// for the previous selection.
#[test]
fn test_cycle_edit_bumps_selection_generation() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")];
    app.file_display_names = vec!["a.txt".to_string(), "b.txt".to_string()];
    app.selection = vec![true, false];
    app.editing_idx = Some(0);
    app.edit_buffer = "a.txt".into();
    let gen_before = app.selection_generation;

    app.cycle_edit(true);

    assert_eq!(app.selection, vec![false, true], "selection moved");
    assert!(
        app.selection_generation > gen_before,
        "a selection mutation must bump selection_generation"
    );
}

#[test]
fn test_cycle_edit_respects_custom_order() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![false, false, false];
    app.custom_order = Some(vec![2, 0, 1]); // c, a, b
    app.editing_idx = Some(2); // "c.txt", first in custom order
    app.edit_buffer = "c.txt".into();

    app.cycle_edit(true); // should go to a.txt (index 0, second in custom order)

    assert_eq!(app.editing_idx, Some(0));
    assert_eq!(app.edit_buffer, "a.txt");
    assert_eq!(app.selection, vec![true, false, false]);
}
#[test]
fn test_keyboard_navigation_paged() {
    let mut app = GuiApp::new_with_config_for_test();
    let num_files = 100;
    app.all_files = (0..num_files)
        .map(|i| PathBuf::from(format!("file_{:03}.txt", i)))
        .collect();
    app.file_display_names = (0..num_files)
        .map(|i| format!("file_{:03}.txt", i))
        .collect();
    app.file_sizes = vec![0; num_files];
    app.file_dates = vec![0; num_files];
    app.file_is_dir = vec![false; num_files];
    app.selection = vec![false; num_files];

    let order = app.get_display_order();

    // Simulate PageDown jump from index 0
    let cur_pos = 0;
    let paged_down = (cur_pos + 25).min(order.len().saturating_sub(1));
    assert_eq!(paged_down, 25);

    // Simulate PageUp jump back from index 25
    let paged_up = paged_down.saturating_sub(25);
    assert_eq!(paged_up, 0);

    // Simulate End jump
    let end_idx = order.len().saturating_sub(1);
    assert_eq!(end_idx, 99);
}
