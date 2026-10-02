use super::tests_util::*;
use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_revert_dir_via_confirm() {
    // Test that confirm_revert handles directory renames correctly.
    let tmp = TempDir::new().unwrap();
    let subdir = tmp.path().join("subdir");
    std::fs::create_dir(&subdir).unwrap();
    let child = subdir.join("child.txt");
    File::create(&child).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![subdir.clone()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];

    app.config.replace.replace = Some("subdir".into());
    app.config.replace.with = Some("newdir".into());
    app.section_enabled[SectionId::Replace as usize] = true;
    app.section_enabled[SectionId::MoveCopy as usize] = false;
    app.config.copy_to.copy_mode = false;

    app.cwd = tmp.path().to_path_buf();
    app.preview_dirty = true;
    app.execute_renames();

    let new_subdir = tmp.path().join("newdir");
    assert!(new_subdir.is_dir(), "rename should have happened");

    // Simulate tree cache as if user navigated here after rename
    app.tree_expanded.insert(new_subdir.clone());
    app.tree_scanned.insert(new_subdir.clone());
    app.tree_children
        .insert(new_subdir.clone(), vec![new_subdir.join("child.txt")]);
    app.tree_scanned.insert(new_subdir.join("child.txt"));

    // Build revert dialog and confirm
    app.build_revert_dialog_state();
    assert!(app.revert_dialog.is_some(), "revert dialog should open");

    // Select the first (and only) entry
    app.revert_dialog.as_mut().unwrap().selection[0] = true;
    app.confirm_revert();

    assert!(subdir.is_dir(), "original directory should be restored");
    assert!(!new_subdir.exists(), "renamed directory should be gone");

    // Tree cache should be updated
    assert!(
        !app.tree_children.contains_key(&new_subdir),
        "new dir key removed from tree_children"
    );
    assert!(
        app.tree_children.contains_key(&subdir),
        "original dir key in tree_children"
    );
    assert_eq!(app.all_files, vec![subdir], "all_files paths restored");
}

#[test]
fn test_revert_multiple_files_partial() {
    // Test partially reverting only some files from a commit.
    let files = ["old_a.txt", "old_b.txt", "old_c.txt"];
    let (mut app, dir) = selection_test_app(&files);

    app.preview_dirty = true;
    app.execute_renames();

    assert_eq!(app.cwd_commits().len(), 1, "one commit after batch");
    assert_eq!(app.cwd_commits()[0].ops.len(), 3, "3 files in commit");

    // All files now have "new" prefix
    for f in &files {
        let new_name = f.replace("old", "new");
        assert!(dir.path().join(&new_name).exists());
    }

    // Revert dialog with all 3 selected
    app.selection = vec![true; 3];
    app.build_revert_dialog_state();
    assert_eq!(app.revert_dialog.as_ref().unwrap().entries.len(), 3);

    // Select all 3 rows
    let state = app.revert_dialog.as_mut().unwrap();
    for i in 0..3 {
        state.selection[i] = true;
    }
    app.confirm_revert();

    // Verify files restored
    for f in &files {
        assert!(dir.path().join(f).exists(), "{} should be restored", f);
    }

    // Both commits are preserved: the original forward commit and the revert commit.
    assert_eq!(app.cwd_commits().len(), 2, "original + revert commit");
    assert!(
        app.cwd_commits()[1].label.starts_with("Revert"),
        "newest commit is Revert"
    );
}

#[test]
fn test_revert_renames_no_result() {
    let mut app = GuiApp::new_with_config_for_test();
    app.commits_by_dir.clear();
    // Should not panic and should set an error status
    app.revert_renames();
    assert!(
        app.status_kind == StatusKind::Error,
        "revert with no result should set error status"
    );
    assert!(
        app.status_message.contains("No rename history"),
        "status should mention no rename history, got: {}",
        app.status_message
    );
}

#[test]
fn test_revert_renames_empty_result() {
    let mut app = GuiApp::new_with_config_for_test();
    app.commits_by_dir
        .entry(app.cwd.clone())
        .or_default()
        .push(Commit {
            timestamp: std::time::Instant::now(),
            label: "empty".into(),
            ops: vec![],
        });
    app.revert_renames();
    assert!(
        app.status_kind == StatusKind::Error,
        "revert with empty commits should set error status"
    );
}
#[test]
fn test_lineage_single_commit() {
    // A file renamed once should have lineage: [original, current]
    let (mut app, _dir) = selection_test_app(&["old_a.txt"]);
    app.preview_dirty = true;
    app.execute_renames();

    // Build dialog to trigger lineage computation
    app.selection = vec![true];
    app.build_revert_dialog_state();
    let state = app.revert_dialog.as_ref().unwrap();
    assert_eq!(state.entries.len(), 1);
    let entry = &state.entries[0];
    assert!(entry.revertable);
    assert!(entry.lineage.len() >= 2, "should have original + current");
    // First entry (original) should have None producer
    assert!(entry.lineage[0].2.is_none(), "original has no producer");
    // Last entry (current) should have Some(0) producer
    assert_eq!(
        entry.lineage.last().unwrap().2,
        Some(0),
        "current produced by commit 0"
    );
    // target_by_commit[0] (before commit 0) should be original name
    assert_eq!(
        entry.target_by_commit[0], "old_a.txt",
        "before commit 0: original name"
    );
}

#[test]
fn test_lineage_multiple_commits() {
    // File renamed across 2 sequential commits.
    let (mut app, dir) = selection_test_app(&["file.txt"]);

    // Commit 0: file.txt → renamed.txt
    app.section_enabled[SectionId::Replace as usize] = true;
    app.config.replace.replace = Some("file".into());
    app.config.replace.with = Some("renamed".into());
    app.preview_dirty = true;
    app.execute_renames();
    assert!(dir.path().join("renamed.txt").exists());

    // Commit 1: renamed.txt → final.txt
    app.config.replace.replace = Some("renamed".into());
    app.config.replace.with = Some("final".into());
    app.preview_dirty = true;
    app.execute_renames();
    assert!(dir.path().join("final.txt").exists());

    app.selection = vec![true];
    app.build_revert_dialog_state();
    let state = app.revert_dialog.as_ref().unwrap();
    let entry = &state.entries[0];

    // Lineage: [("file.txt", None), ("renamed.txt", Some(0)), ("final.txt", Some(1))]
    assert_eq!(entry.lineage.len(), 3);
    assert_eq!(entry.lineage[0].1, "file.txt");
    assert!(entry.lineage[0].2.is_none());
    assert_eq!(entry.lineage[1].1, "renamed.txt");
    assert_eq!(entry.lineage[1].2, Some(0));
    assert_eq!(entry.lineage[2].1, "final.txt");
    assert_eq!(entry.lineage[2].2, Some(1));

    // target_by_commit
    assert_eq!(
        entry.target_by_commit[0], "file.txt",
        "before commit 0: original"
    );
    assert_eq!(
        entry.target_by_commit[1], "renamed.txt",
        "before commit 1: after commit 0"
    );
}

#[test]
fn test_revert_undo_stack_structure() {
    // After batch rename, undo_stack should have 1 Commit with the right ops.
    let files = ["old_a.txt", "old_b.txt"];
    let (mut app, _dir) = selection_test_app(&files);

    app.preview_dirty = true;
    app.execute_renames();

    assert_eq!(app.cwd_commits().len(), 1);
    let commit = &app.cwd_commits()[0];
    assert!(commit.label.starts_with("Apply"));
    assert_eq!(commit.ops.len(), 2);
    assert_eq!(commit.ops[0].original_name, "old_a.txt");
    assert_eq!(commit.ops[0].new_name, "new_a.txt");
    assert_eq!(commit.ops[1].original_name, "old_b.txt");
    assert_eq!(commit.ops[1].new_name, "new_b.txt");
}

#[test]
fn test_revert_all_files_in_preview_appear_in_dialog() {
    let files = ["old_a.txt", "old_b.txt", "old_c.txt"];
    let (mut app, _dir) = selection_test_app(&files);
    app.preview_dirty = true;
    app.execute_renames();

    // Selection is ignored — all files in preview appear
    app.selection = vec![true, false, true];
    app.build_revert_dialog_state();
    let state = app.revert_dialog.as_ref().unwrap();
    assert_eq!(
        state.entries.len(),
        3,
        "all 3 files should appear regardless of selection"
    );
    assert!(state.entries.iter().all(|e| e.revertable));
}

#[test]
fn test_revert_shift_click_range_uses_display_order() {
    // Shift-click ranges in the revert dialog must follow the table's
    // display order (the table is sortable), not raw entry indices.
    use crate::gui::dialogs::apply_revert_selection;

    let files = ["old_a.txt", "old_b.txt", "old_c.txt", "old_d.txt"];
    let (mut app, _dir) = selection_test_app(&files);
    app.preview_dirty = true;
    app.execute_renames();
    app.build_revert_dialog_state();
    let state = app.revert_dialog.as_mut().unwrap();

    // Simulate a sort: display order b(1), d(3), a(0), c(2).
    state.display_order = vec![1, 3, 0, 2];
    state.selection.fill(false);
    state.last_clicked_idx = None;
    state.selection_anchor = None;

    // Click the row at display position 0 (entry 1 = b).
    let first = state.display_order[0];
    apply_revert_selection(
        &mut state.selection,
        &state.display_order,
        &mut state.selection_anchor,
        &mut state.last_clicked_idx,
        first,
        false,
        false,
    );
    assert_eq!(state.selection, vec![false, true, false, false]);
    assert_eq!(state.selection_anchor, None);

    // Shift-click the row at display position 2 (entry 0 = a). The range
    // must cover display positions 0..=2 → entries {1, 3, 0} = b, d, a —
    // NOT the entry-index span 0..=1 (which would miss d).
    let second = state.display_order[2];
    apply_revert_selection(
        &mut state.selection,
        &state.display_order,
        &mut state.selection_anchor,
        &mut state.last_clicked_idx,
        second,
        false,
        true,
    );
    assert_eq!(state.selection, vec![true, true, false, true]);
    assert_eq!(state.selection_anchor, Some(first));

    // Shift-click display position 3 (entry 2 = c): the anchor stays
    // pinned to the first click (display position 0) → whole table.
    let third = state.display_order[3];
    apply_revert_selection(
        &mut state.selection,
        &state.display_order,
        &mut state.selection_anchor,
        &mut state.last_clicked_idx,
        third,
        false,
        true,
    );
    assert_eq!(state.selection, vec![true, true, true, true]);
    assert_eq!(state.selection_anchor, Some(first));

    // A plain click resets the anchor and moves the active edge.
    apply_revert_selection(
        &mut state.selection,
        &state.display_order,
        &mut state.selection_anchor,
        &mut state.last_clicked_idx,
        third,
        false,
        false,
    );
    assert_eq!(state.selection_anchor, None);
    assert_eq!(state.last_clicked_idx, Some(third));
}

#[test]
fn test_revert_file_missing_omitted() {
    let files = ["old_a.txt"];
    let (mut app, dir) = selection_test_app(&files);
    app.preview_dirty = true;
    app.execute_renames();

    // Delete the file from disk
    let new_path = dir.path().join("new_a.txt");
    std::fs::remove_file(&new_path).unwrap();

    app.build_revert_dialog_state();
    // Missing file is not revertable, so dialog should not be created
    assert!(app.revert_dialog.is_none());
}

#[test]
fn test_confirm_revert_no_ops_selected() {
    // Opening dialog and closing without selecting anything should be a no-op.
    let (mut app, _dir) = selection_test_app(&["old_a.txt"]);
    app.preview_dirty = true;
    app.execute_renames();

    let prev_undo_len = app.cwd_commits().len();
    app.build_revert_dialog_state();
    // Don't select anything, just confirm
    app.revert_dialog.as_mut().unwrap().selection = vec![false];
    app.confirm_revert();

    assert_eq!(
        app.cwd_commits().len(),
        prev_undo_len,
        "cwd_commits unchanged when nothing reverted"
    );
}

#[test]
fn test_confirm_revert_partial_commit_splitting() {
    // Revert 1 of 2 files from a commit. The unaffected file should remain
    // in its original commit, and a new revert commit should be added.
    let files = ["old_a.txt", "old_b.txt"];
    let (mut app, dir) = selection_test_app(&files);
    app.preview_dirty = true;
    app.execute_renames();
    assert!(dir.path().join("new_a.txt").exists());
    assert!(dir.path().join("new_b.txt").exists());

    app.selection = vec![true, true];
    app.build_revert_dialog_state();
    // Only select entry 0 (old_a), entry 1 stays unchecked
    {
        let state = app.revert_dialog.as_mut().unwrap();
        state.selection[0] = true;
        state.selection[1] = false;
    }
    app.confirm_revert();

    // old_a should be restored
    assert!(
        dir.path().join("old_a.txt").exists(),
        "old_a.txt should be restored"
    );
    assert!(
        !dir.path().join("new_a.txt").exists(),
        "new_a.txt should no longer exist"
    );
    // old_b should remain renamed
    assert!(
        dir.path().join("new_b.txt").exists(),
        "new_b.txt should remain"
    );
    assert!(
        !dir.path().join("old_b.txt").exists(),
        "old_b.txt should still be absent"
    );

    // undo_stack: original commit (with only old_b's op) + revert commit = 2
    assert_eq!(
        app.cwd_commits().len(),
        2,
        "cwd_commits: 1 original (old_b) + 1 revert = 2"
    );
    assert!(
        app.cwd_commits()[0].label.starts_with("Apply"),
        "original commit preserved (first)"
    );
    assert!(
        app.cwd_commits()[1].label.starts_with("Revert"),
        "revert commit is newest (last)"
    );
}

#[test]
fn test_revert_branch_clears_redo() {
    let (mut app, _dir) = selection_test_app(&["old_a.txt"]);
    app.preview_dirty = true;
    app.execute_renames();

    // Populate redo_stack manually (simulating future Ctrl+Z)
    app.redo_stack.push(app.cwd_commits()[0].clone());
    assert!(!app.redo_stack.is_empty());

    // A new batch rename clears redo. The operation changes between
    // applies (the real-user flow that re-arms the processed guard):
    // old_a.txt → new_a.txt → final_a.txt.
    app.config.replace.replace = Some("new".into());
    app.config.replace.with = Some("final".into());
    app.preview_dirty = true;
    app.execute_renames();
    assert!(app.redo_stack.is_empty(), "redo cleared after new rename");
    assert!(
        _dir.path().join("final_a.txt").exists(),
        "second apply should rename new_a.txt → final_a.txt"
    );
}

#[test]
fn test_revert_status_in_rebuild_rows_cache() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let p1 = dir.path().join("a.txt");
    let p2 = dir.path().join("b.txt");
    File::create(&p1).unwrap();
    File::create(&p2).unwrap();

    app.all_files = vec![p1.clone(), p2.clone()];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];

    // Push a commit with a.txt (marks a.txt as "changed")
    app.commits_by_dir
        .entry(app.cwd.clone())
        .or_default()
        .push(Commit {
            timestamp: std::time::Instant::now(),
            label: "test".into(),
            ops: vec![awara::RenameOp {
                original_path: p1.clone(),
                new_path: PathBuf::from("a_renamed.txt"),
                original_name: "a.txt".into(),
                new_name: "a_renamed.txt".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0u8,
            }],
        });

    // Must also populate status_map directly (ensure_status_map no longer rebuilds).
    let p1_str = p1.to_string_lossy().to_string();
    app.status_map.insert(p1_str, (0u8, None));

    app.rebuild_rows_cache();

    assert_eq!(app.cached_rows[0].status, Some(0));
    assert!(app.cached_rows[1].status.is_none());
}

#[test]
fn test_revert_dialog_no_history_files_excluded() {
    // File not in any commit should be excluded from dialog
    let (mut app, _dir) = selection_test_app(&["old_a.txt", "untouched.txt"]);
    app.preview_dirty = true;
    app.execute_renames();

    // Select both files
    app.selection = vec![true, true];
    app.build_revert_dialog_state();
    let state = app.revert_dialog.as_ref().unwrap();
    // Both should appear but only old_a.txt has lineage
    for entry in &state.entries {
        if entry.current_name == "untouched.txt" {
            assert!(
                !entry.revertable,
                "untouched file should show as not revertable"
            );
            assert!(entry.block_reason.is_some());
        }
    }
}

#[test]
fn test_revert_cross_directory() {
    // Rename files in a subdirectory, then revert from the parent.
    // This exercises the cross-directory commit lookup path.
    let dir = TempDir::new().unwrap();
    let subdir = dir.path().join("subdir");
    std::fs::create_dir_all(&subdir).unwrap();
    let file_a = subdir.join("old_a.txt");
    let file_b = subdir.join("old_b.txt");
    File::create(&file_a).unwrap();
    File::create(&file_b).unwrap();

    // ── 1. Rename files inside subdirectory ──
    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = subdir.clone();
    app.cwd_input = subdir.to_string_lossy().to_string();
    app.all_files = vec![file_a.clone(), file_b.clone()];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.selection = vec![true, true];
    app.config.replace.replace = Some("old".into());
    app.config.replace.with = Some("new".into());
    app.section_enabled[SectionId::Replace as usize] = true;
    app.section_enabled[SectionId::MoveCopy as usize] = false;
    app.config.copy_to.copy_mode = false;
    app.preview_dirty = true;
    app.execute_renames();

    let new_a = subdir.join("new_a.txt");
    let new_b = subdir.join("new_b.txt");
    assert!(
        new_a.is_dir() || new_a.exists(),
        "old_a should be renamed to new_a"
    );
    assert!(
        new_b.is_dir() || new_b.exists(),
        "old_b should be renamed to new_b"
    );
    // Commits are stored under subdir key
    assert!(
        app.commits_by_dir.contains_key(&subdir),
        "commits stored under subdir key"
    );

    // ── 2. Navigate to parent directory ──
    let parent = dir.path().to_path_buf();
    app.cwd = parent.clone();
    app.cwd_input = parent.to_string_lossy().to_string();
    // Simulate subdirs enabled: put the renamed files into all_files
    app.all_files = vec![new_a.clone(), new_b.clone()];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    // cwd_commits() is empty — only all_commits() finds the subdir commits
    assert!(
        app.cwd_commits().is_empty(),
        "cwd_commits empty after switching to parent"
    );
    assert!(
        app.all_commits().next().is_some(),
        "all_commits finds commits from subdir"
    );

    // ── 3. Build revert dialog (cross-directory) ──
    app.build_revert_dialog_state();
    let state = app.revert_dialog.as_ref().unwrap();
    assert_eq!(state.entries.len(), 2, "both files appear in dialog");
    assert!(
        state.entries.iter().all(|e| e.revertable),
        "all entries should be revertable"
    );
    // The sidebar labels come from all_commits
    assert_eq!(app.all_commits().count(), 1, "one commit across all dirs");

    // ── 4. Apply revert ──
    app.revert_dialog.as_mut().unwrap().selection = vec![true, true];
    app.confirm_revert();

    // Files renamed back to original names
    assert!(file_a.exists(), "file_a restored");
    assert!(file_b.exists(), "file_b restored");
    assert!(!new_a.exists(), "new_a removed");
    assert!(!new_b.exists(), "new_b removed");

    // Revert commit stored in parent (cwd) since revert was applied from parent.
    // Original subdir commit is preserved separately (no commit splitting).
    let parent_commits = app.commits_by_dir.get(&parent);
    assert!(parent_commits.is_some(), "parent has commit history");
    if let Some(commits) = parent_commits {
        assert_eq!(commits.len(), 1, "one Revert commit in parent");
        assert!(
            commits[0].label.starts_with("Revert"),
            "parent commit is labeled as Revert"
        );
        assert_eq!(commits[0].ops.len(), 2, "two ops in the Revert commit");
    }
    // Subdir's original commit is preserved intact.
    assert!(
        app.commits_by_dir.contains_key(&subdir),
        "subdir original commit preserved"
    );
}

/// A commit that only changed attributes/timestamps (name unchanged) must be
/// listed *and* actionable in the revert dialog — matching the batch Undo path.
#[test]
fn test_revert_restores_metadata_only_change() {
    use awara::TimestampSpec;
    let (mut app, dir) = selection_test_app(&["file.txt"]);

    // No rename — only a metadata effect.
    app.config.replace.replace = None;
    app.config.replace.with = None;
    app.config.special.set_modified = Some(TimestampSpec::Fixed("2020-01-02 03:04:05".into()));

    let path = dir.path().join("file.txt");
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();

    app.preview_dirty = true;
    app.execute_renames();

    // The effect was applied and recorded as a no-op op.
    let after = std::fs::metadata(&path).unwrap().modified().unwrap();
    let expected = TimestampSpec::Fixed("2020-01-02 03:04:05".into())
        .resolve(&path, "modified")
        .unwrap();
    assert_eq!(after, expected, "metadata effect applied");
    assert_ne!(after, before, "effect actually changed the mtime");

    assert_eq!(app.cwd_commits().len(), 1, "one commit");
    let ops = &app.cwd_commits()[0].ops;
    assert_eq!(ops.len(), 1);
    assert!(
        ops[0].is_noop(),
        "effects-only op leaves the name unchanged"
    );
    assert_eq!(
        ops[0].applied_timestamps_mask & awara::TS_MODIFIED,
        awara::TS_MODIFIED,
        "modified bit set"
    );

    // Dialog lists the metadata-only entry, marks it actionable, and its column
    // reflects the active commit's timestamp effect.
    app.build_revert_dialog_state();
    let state = app.revert_dialog.as_ref().unwrap();
    assert_eq!(state.entries.len(), 1, "metadata-only file is listed");
    let active = state.active_commit_idx;
    assert!(
        state.entries[0].has_change_at(active),
        "metadata-only row is actionable"
    );
    assert_eq!(
        GuiApp::revert_ts_summary(&state.entries[0], active),
        "modified"
    );

    // Revert restores the pre-effect metadata and records an inverse commit.
    app.revert_dialog.as_mut().unwrap().selection[0] = true;
    app.confirm_revert();

    let restored = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(restored, before, "pre-effect mtime restored");
    assert!(path.exists(), "file still exists");
    assert_eq!(app.cwd_commits().len(), 2, "forward + revert commit");
    assert!(app.cwd_commits()[1].label.starts_with("Revert"));

    // The revert commit records the applied effects on a no-op op so a later
    // redo can replay them via `reapply_metadata`.
    let revert_op = &app.cwd_commits()[1].ops[0];
    assert!(revert_op.is_noop(), "metadata revert op keeps the name");
    assert_eq!(
        revert_op.applied_timestamps_mask & awara::TS_MODIFIED,
        awara::TS_MODIFIED,
        "redo can resend it"
    );
}

/// A commit that didn't touch a file makes that file non-actionable at that
/// commit — the old `lineage.len() > 1` gate made everything selectable.
#[test]
fn test_revert_entry_not_actionable_for_untouched_commit() {
    use crate::gui::types::Commit;
    let (mut app, dir) = selection_test_app(&["old_a.txt"]);
    app.preview_dirty = true;
    app.execute_renames(); // commit 0: old_a -> new_a

    // Commit 1 touches a different file only.
    app.push_cwd_commit(Commit {
        timestamp: std::time::Instant::now(),
        label: "Batch — other".into(),
        ops: vec![awara::RenameOp {
            original_path: dir.path().join("other.txt"),
            new_path: dir.path().join("other2.txt"),
            original_name: "other.txt".into(),
            new_name: "other2.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0,
        }],
    });

    app.build_revert_dialog_state();
    let state = app.revert_dialog.as_ref().unwrap();
    let active = state.active_commit_idx;
    assert_eq!(active, 1, "newest commit is active");
    assert!(
        state.entries[0].has_change_at(0),
        "commit 0 renamed the file"
    );
    assert!(
        !state.entries[0].has_change_at(active),
        "commit 1 didn't touch the file"
    );
}
