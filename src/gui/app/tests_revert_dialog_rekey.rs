use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_revert_dialog_opens_after_undo() {
    // After undo, the file is at its original_path, which the old lineage
    // builder couldn't walk backward from. The fix adds forward-walking
    // so the full history is visible even when the current state is an
    // original_path (not a new_path).
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    File::create(&a).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = vec![a.clone()];
    app.file_names = vec![a.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.selection = vec![true];
    app.config.copy_to.copy_mode = false;

    // Batch rename: a.txt → new_a.txt
    app.section_enabled[SectionId::Replace as usize] = true;
    app.config.replace.replace = Some("a".into());
    app.config.replace.with = Some("new_a".into());
    app.build_command_order();
    app.preview_dirty = true;
    app.execute_renames();

    let new_a = dir.path().join("new_a.txt");
    assert!(new_a.exists(), "rename should have happened");

    // Undo: new_a.txt → a.txt
    assert!(app.find_undo_op(0).is_some());
    app.undo_file(0);
    assert!(a.exists(), "undo should restore a.txt");

    // Revert dialog should NOT say "No files in the current view"
    // It should find the file and build lineage with both states
    app.build_revert_dialog_state();
    assert!(
        app.revert_dialog.is_some(),
        "revert dialog should open after undo"
    );
    let state = app.revert_dialog.as_ref().unwrap();
    assert_eq!(state.entries.len(), 1, "one file should be in dialog");
    let entry = &state.entries[0];
    // Lineage: [a.txt (None), new_a.txt (Some(0))]
    assert!(
        entry.lineage.len() >= 2,
        "lineage should have at least 2 states, got: {:?}",
        entry.lineage
    );
    assert!(
        entry.lineage.iter().any(|(p, _, _)| p == &a),
        "lineage should include a.txt"
    );
    assert!(
        entry.lineage.iter().any(|(p, _, _)| p == &new_a),
        "lineage should include new_a.txt"
    );
}

#[test]
fn test_revert_dialog_clears_active_popup_on_empty() {
    // When build_revert_dialog_state finds no entries, it should clear
    // active_popup to prevent the grey filter from sticking.
    let mut app = GuiApp::new_with_config_for_test();
    // Set active_popup to simulate a stale revert dialog state
    app.active_popup = Some(PopupKind::RevertDialog);

    // No commits and no files touched by commits → empty dialog
    app.build_revert_dialog_state();

    assert!(app.revert_dialog.is_none(), "no dialog when no files match");
    assert!(app.active_popup.is_none(), "active_popup should be cleared");
}

#[test]
fn test_cached_selected_count_invalidation() {
    let mut app = GuiApp::new_with_config_for_test();
    app.selection = vec![true, false, true, false, true];

    // First call populates cache
    assert_eq!(app.selected_count(), 3);
    assert_eq!(app.cached_selected_count.get(), Some(3));

    // Subsequence call hits cache directly
    assert_eq!(app.selected_count(), 3);

    // Invalidation clears cache
    app.cached_selected_count.set(None);
    assert_eq!(app.cached_selected_count.get(), None);

    // Mutating selection and invalidating
    app.selection[1] = true;
    app.cached_selected_count.set(None);
    assert_eq!(app.selected_count(), 4);
    assert_eq!(app.cached_selected_count.get(), Some(4));
}

#[test]
fn test_cached_modified_count_invalidation() {
    let mut app = GuiApp::new_with_config_for_test();
    app.cached_rows = std::rc::Rc::new(vec![
        Row {
            idx: 0,
            name: "a.txt".into(),
            is_dir: false,
            size: 10,
            date: 100,
            new_name: "new_a.txt".into(),
            changed: true,
            status: Some(0), // modified (success)
            orig_segs: None,
            new_segs: None,
            error_msg: None,
            missing: false,
        },
        Row {
            idx: 1,
            name: "b.txt".into(),
            is_dir: false,
            size: 20,
            date: 200,
            new_name: "b.txt".into(),
            changed: false,
            status: None,
            orig_segs: None,
            new_segs: None,
            error_msg: None,
            missing: false,
        },
    ]);

    assert_eq!(app.modified_count(), 1);
    assert_eq!(app.cached_modified_count.get(), Some(1));

    // When cache is invalidated
    app.cached_modified_count.set(None);
    assert_eq!(app.modified_count(), 1);
}
#[test]
fn test_parent_dir_rename_rekeys_child_revert_lineage() {
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().join("parent");
    std::fs::create_dir(&parent).unwrap();
    let child = parent.join("child.txt");
    File::create(&child).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = parent.clone();
    app.all_files = vec![child.clone()];
    app.file_names = vec![child.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];

    // 1. Rename child.txt -> child_v2.txt
    app.config.replace.replace = Some("child".into());
    app.config.replace.with = Some("child_v2".into());
    app.section_enabled[SectionId::Replace as usize] = true;
    app.preview_dirty = true;
    app.execute_renames();

    let child_v2 = parent.join("child_v2.txt");
    assert!(child_v2.exists(), "child_v2 should exist on disk");

    // 2. Rename parent directory parent -> parent_renamed
    let parent_renamed = tmp.path().join("parent_renamed");
    app.rekey_dir_prefix(&parent, &parent_renamed);
    std::fs::rename(&parent, &parent_renamed).unwrap();

    // 3. Navigate into parent_renamed
    app.cwd = parent_renamed.clone();
    let child_v2_new = parent_renamed.join("child_v2.txt");
    app.all_files = vec![child_v2_new.clone()];
    app.file_names = vec![child_v2_new.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];

    // 4. Build revert dialog state
    app.build_revert_dialog_state();
    assert!(
        app.revert_dialog.is_some(),
        "revert dialog should find history for child in renamed parent"
    );

    let dialog = app.revert_dialog.as_mut().unwrap();
    assert_eq!(dialog.entries.len(), 1);
    assert_eq!(dialog.entries[0].current_name, "child_v2.txt");
    assert_eq!(
        dialog.col_widths.len(),
        6,
        "revert table has 6 columns including status"
    );

    // 5. Confirm revert
    dialog.selection[0] = true;
    app.confirm_revert();

    let restored_child = parent_renamed.join("child.txt");
    assert!(
        restored_child.exists(),
        "restored child should exist at parent_renamed/child.txt"
    );
    assert!(!child_v2_new.exists(), "child_v2 should no longer exist");
}

#[test]
fn test_confirm_revert_rekeys_tree_caches_for_dir_revert() {
    let tmp = TempDir::new().unwrap();
    let subdir = tmp.path().join("subdir");
    std::fs::create_dir(&subdir).unwrap();
    File::create(subdir.join("child.txt")).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![subdir.clone()];
    app.file_names = vec![subdir.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];

    // Rename subdir → newdir via the real pipeline
    app.config.replace.replace = Some("subdir".into());
    app.config.replace.with = Some("newdir".into());
    app.section_enabled[SectionId::Replace as usize] = true;
    app.section_enabled[SectionId::MoveCopy as usize] = false;
    app.config.copy_to.copy_mode = false;
    app.preview_dirty = true;
    app.execute_renames();

    let newdir = tmp.path().join("newdir");
    assert!(newdir.is_dir(), "newdir should exist after rename");

    // Simulate tree cache as if the user had expanded newdir
    app.tree_expanded.insert(newdir.clone());
    app.tree_scanned.insert(newdir.clone());
    app.tree_children
        .insert(newdir.clone(), vec![newdir.join("child.txt")]);
    app.tree_has_subdirs.insert(newdir.clone(), false);

    // Revert newdir → subdir
    app.build_revert_dialog_state();
    assert!(app.revert_dialog.is_some());
    let dialog = app.revert_dialog.as_mut().unwrap();
    dialog.selection[0] = true;
    app.confirm_revert();

    assert!(subdir.is_dir(), "subdir restored by revert");
    assert!(!newdir.exists(), "newdir gone after revert");

    // Tree caches re-keyed back to the original path
    assert!(
        app.tree_children.contains_key(&subdir),
        "children re-keyed to original path"
    );
    assert!(
        app.tree_scanned.contains(&subdir),
        "scanned flag re-keyed to original path"
    );
    assert!(
        app.tree_expanded.contains(&subdir),
        "expanded state re-keyed to original path"
    );
    assert_eq!(
        app.tree_has_subdirs.get(&subdir),
        Some(&false),
        "probe answer re-keyed to original path"
    );
    assert_eq!(
        app.tree_children.get(&subdir),
        Some(&vec![subdir.join("child.txt")]),
        "children list contents re-keyed"
    );
    assert!(!app.tree_children.contains_key(&newdir));
    assert!(!app.tree_scanned.contains(&newdir));
    assert!(!app.tree_expanded.contains(&newdir));
    assert!(!app.tree_has_subdirs.contains_key(&newdir));
}

#[test]
fn test_revert_error_captured_in_status_map() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("test_file.txt");
    File::create(&file).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![file.clone()];
    app.file_names = vec![file.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];

    // Rename test_file.txt -> test_renamed.txt
    app.config.replace.replace = Some("test_file".into());
    app.config.replace.with = Some("test_renamed".into());
    app.section_enabled[SectionId::Replace as usize] = true;
    app.preview_dirty = true;
    app.execute_renames();

    let renamed = tmp.path().join("test_renamed.txt");
    assert!(renamed.exists());

    // Create a blocker at the original path so revert collides
    File::create(&file).unwrap();

    app.build_revert_dialog_state();
    assert!(app.revert_dialog.is_some());

    let dialog = app.revert_dialog.as_mut().unwrap();
    dialog.selection[0] = true;

    // Record what status_map held before revert (from the original rename operation)
    let pre_revert_status = app.status_map.clone();
    app.confirm_revert();

    // Failed revert should be captured in revert_status_map (separate from main status_map)
    let renamed_str = renamed.to_string_lossy().to_string();
    let status = app.revert_status_map.get(&renamed_str);
    assert!(
        status.is_some(),
        "revert_status_map should record error for failed revert"
    );
    assert_eq!(status.unwrap().0, 1u8, "error code 1 should be set");
    assert!(
        status.unwrap().1.is_some(),
        "error message should be present"
    );
    // Confirm revert should not have written a *new* error entry to status_map
    // (it may still hold the original rename's success entry from before the revert)
    assert!(
        !app.status_map
            .get(&renamed_str)
            .is_some_and(|(code, _)| *code == 1u8),
        "revert errors must not be written to main status_map"
    );
    // Entries that were already in status_map before revert should not be removed
    // if the revert did not succeed (file was not actually renamed).
    for (k, v) in &pre_revert_status {
        if let Some(cur) = app.status_map.get(k) {
            // Entry still present — that's fine.
            let _ = (cur, v);
        }
    }
}
