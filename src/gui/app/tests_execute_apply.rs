use super::tests_util::*;
use super::*;
use awara::NumberingMode;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_numbering_skips_deselected_files() {
    // Numbering should count only selected files in display order.
    // A deselected file in the middle must not consume a numbering slot.
    let dir = TempDir::new().unwrap();
    for name in &["a.txt", "b.txt", "c.txt"] {
        File::create(dir.path().join(name)).unwrap();
    }
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        dir.path().join("a.txt"),
        dir.path().join("b.txt"),
        dir.path().join("c.txt"),
    ];
    app.file_sizes = vec![0; 3];
    app.file_dates = vec![0; 3];
    app.section_enabled[SectionId::Numbering as usize] = true;
    app.config.numbering.numbering_mode = Some(NumberingMode::Prefix);
    app.config.numbering.numbering_start = 1;
    app.config.numbering.numbering_increment = 1;
    app.config.numbering.numbering_pad = 0;
    app.config.numbering.numbering_sep = Some("_".to_string());

    // Select first and last, deselect middle
    app.selection = vec![true, false, true];

    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);

    assert_eq!(app.preview.len(), 3, "preview for all 3 files");

    // a.txt — first selected in display order → num_idx 0 → "1_a.txt"
    assert!(app.preview[0].selected);
    assert_eq!(app.preview[0].new_name(), "1_a.txt");

    // b.txt — deselected → unchanged
    assert!(!app.preview[1].selected);
    assert_eq!(app.preview[1].new_name(), "b.txt");

    // c.txt — second selected in display order → num_idx 1 → "2_c.txt"
    assert!(app.preview[2].selected);
    assert_eq!(app.preview[2].new_name(), "2_c.txt");
}

#[test]
fn test_numbering_starts_at_first_selected_when_first_file_deselected() {
    // When the first file(s) in the list are deselected, numbering must
    // still start at 1 for the first *selected* file.
    let dir = TempDir::new().unwrap();
    for name in &["x.txt", "y.txt", "z.txt"] {
        File::create(dir.path().join(name)).unwrap();
    }
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        dir.path().join("x.txt"),
        dir.path().join("y.txt"),
        dir.path().join("z.txt"),
    ];
    app.file_sizes = vec![0; 3];
    app.file_dates = vec![0; 3];
    app.section_enabled[SectionId::Numbering as usize] = true;
    app.config.numbering.numbering_mode = Some(NumberingMode::Prefix);
    app.config.numbering.numbering_start = 1;
    app.config.numbering.numbering_increment = 1;
    app.config.numbering.numbering_pad = 0;
    app.config.numbering.numbering_sep = Some("_".to_string());

    // Deselect x.txt, select y.txt and z.txt
    app.selection = vec![false, true, true];

    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);

    assert_eq!(app.preview.len(), 3);

    // x.txt — deselected → unchanged
    assert!(!app.preview[0].selected);
    assert_eq!(app.preview[0].new_name(), "x.txt");

    // y.txt — first selected → num_idx 0 → "1_y.txt"
    assert!(app.preview[1].selected);
    assert_eq!(app.preview[1].new_name(), "1_y.txt");

    // z.txt — second selected → num_idx 1 → "2_z.txt"
    assert!(app.preview[2].selected);
    assert_eq!(app.preview[2].new_name(), "2_z.txt");
}

#[test]
fn test_numbering_restart_folder_in_preview() {
    // numbering_restart_folder should reset numbering per folder in preview
    let dir = TempDir::new().unwrap();
    let dir_a = dir.path().join("a");
    let dir_b = dir.path().join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();
    for name in &["f1.txt", "f2.txt"] {
        File::create(dir_a.join(name)).unwrap();
        File::create(dir_b.join(name)).unwrap();
    }

    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        dir_a.join("f1.txt"),
        dir_a.join("f2.txt"),
        dir_b.join("f1.txt"),
        dir_b.join("f2.txt"),
    ];
    app.file_sizes = vec![0; 4];
    app.file_dates = vec![0; 4];
    app.section_enabled[SectionId::Numbering as usize] = true;
    app.config.numbering.numbering_mode = Some(NumberingMode::Prefix);
    app.config.numbering.numbering_start = 1;
    app.config.numbering.numbering_increment = 1;
    app.config.numbering.numbering_pad = 0;
    app.config.numbering.numbering_sep = Some("_".to_string());
    app.config.numbering.numbering_restart_folder = true;
    app.selection = vec![true; 4];

    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);

    assert_eq!(app.preview.len(), 4);

    // Folder a: f1.txt → 1_f1.txt, f2.txt → 2_f2.txt
    // Folder b: f1.txt → 1_f1.txt, f2.txt → 2_f2.txt
    assert_eq!(app.preview[0].new_name(), "1_f1.txt");
    assert_eq!(app.preview[1].new_name(), "2_f2.txt");
    assert_eq!(app.preview[2].new_name(), "1_f1.txt");
    assert_eq!(app.preview[3].new_name(), "2_f2.txt");
}

/// Numbering (including per-folder restart counters) is assigned in
/// DISPLAY order, matching `execute_renames` (which numbers candidates in
/// display order). When the display order differs from the file index
/// order, the preview must follow display order.
#[test]
fn test_numbering_restart_folder_follows_display_order() {
    let dir = TempDir::new().unwrap();
    let dir_a = dir.path().join("a");
    let dir_b = dir.path().join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();
    // Index order: a/f2, a/f1, b/f2, b/f1.
    let files = vec![
        dir_a.join("f2.txt"),
        dir_a.join("f1.txt"),
        dir_b.join("f2.txt"),
        dir_b.join("f1.txt"),
    ];
    for f in &files {
        File::create(f).unwrap();
    }

    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = files.clone();
    app.file_names = files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    app.file_display_names = files
        .iter()
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        })
        .collect();
    app.file_sizes = vec![0; 4];
    app.file_dates = vec![0; 4];
    app.file_is_dir = vec![false; 4];
    app.section_enabled[SectionId::Numbering as usize] = true;
    app.config.numbering.numbering_mode = Some(NumberingMode::Prefix);
    app.config.numbering.numbering_start = 1;
    app.config.numbering.numbering_increment = 1;
    app.config.numbering.numbering_pad = 0;
    app.config.numbering.numbering_sep = Some("_".to_string());
    app.config.numbering.numbering_restart_folder = true;
    app.selection = vec![true; 4];

    // Display order (sorted by path): a/f1 (idx 1), a/f2 (idx 0),
    // b/f1 (idx 3), b/f2 (idx 2) → num_order = [1, 0, 3, 2].
    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);

    assert_eq!(app.preview.len(), 4);
    // Display order assigns per folder: a/f1→1, a/f2→2, b/f1→1, b/f2→2.
    assert_eq!(app.preview[0].new_name(), "2_f2.txt"); // a/f2 (index order 0)
    assert_eq!(app.preview[1].new_name(), "1_f1.txt"); // a/f1
    assert_eq!(app.preview[2].new_name(), "2_f2.txt"); // b/f2
    assert_eq!(app.preview[3].new_name(), "1_f1.txt"); // b/f1

    // Cross-check against execute_renames, the execution SSoT: numbering
    // there is assigned in display order too, so every preview new_name
    // must match the executed rename for the same file.
    let display_order_paths: Vec<String> = [1usize, 0, 3, 2]
        .iter()
        .map(|&i| app.file_names[i].clone())
        .collect();
    let mut options = RenameOptions::default();
    let result = awara::execute_renames(&display_order_paths, &app.config, &mut options);
    assert_eq!(result.successful_ops.len(), 4);
    let exec_by_path: HashMap<&str, &str> = result
        .successful_ops
        .iter()
        .map(|op| (op.original_path.to_str().unwrap(), op.new_name.as_str()))
        .collect();
    for (i, f) in app.file_names.iter().enumerate() {
        let expected = exec_by_path[f.as_str()];
        assert_eq!(
            app.preview[i].new_name(),
            expected,
            "preview vs execute numbering mismatch at index {}",
            i
        );
    }
}

#[test]
fn test_update_preview_respects_selection() {
    let files = ["old_file_a.txt", "old_file_b.txt", "old_file_c.txt"];
    let (mut app, _dir) = selection_test_app(&files);

    // Deselect the middle file
    app.selection[1] = false;
    app.preview_dirty = true;
    app.update_preview();

    assert_eq!(app.preview.len(), 3, "should have preview for all files");

    // File 0: selected → should have new name
    assert!(app.preview[0].selected);
    assert_ne!(
        app.preview[0].new_name(),
        app.preview[0].original_name,
        "selected item should be renamed"
    );
    assert!(
        app.preview[0].new_name().contains("new"),
        "selected item new_name should contain replacement"
    );

    // File 1: deselected → should be unchanged
    assert!(!app.preview[1].selected);
    assert_eq!(
        app.preview[1].new_name(),
        app.preview[1].original_name,
        "deselected item should NOT be renamed"
    );

    // File 2: selected → should have new name
    assert!(app.preview[2].selected);
    assert_ne!(
        app.preview[2].new_name(),
        app.preview[2].original_name,
        "selected item should be renamed"
    );
}

/// GUI preview: the Regex section must update the New Name for a pattern that
/// contains a literal space, under the default config (which has v2 on).
#[test]
fn test_preview_regex_section_literal_space() {
    let files = ["Example Movie - 01 (Sample Cut).mkv"];
    let (mut app, _dir) = selection_test_app(&files);
    // Exercise the Regex section instead of the default Replace op.
    app.config.replace.replace = None;
    app.config.replace.with = None;
    app.config.regex.regex_match = Some(r"- \d+".into());
    app.config.regex.regex_replace = Some("- 001".into());
    app.section_enabled[SectionId::Regex as usize] = true;

    app.preview_dirty = true;
    app.update_preview();

    assert_eq!(app.preview.len(), 1);
    assert_eq!(
        app.preview[0].new_name(),
        "Example Movie - 001 (Sample Cut).mkv"
    );
}

/// GUI preview: a lone Match pattern (empty "With") deletes the matched text,
/// instead of being a silent no-op.
#[test]
fn test_preview_regex_empty_with_deletes_match() {
    let files = ["track01.mp3"];
    let (mut app, _dir) = selection_test_app(&files);
    app.config.replace.replace = None;
    app.config.replace.with = None;
    app.config.regex.regex_match = Some(r"\d+".into());
    app.config.regex.regex_replace = None;
    app.section_enabled[SectionId::Regex as usize] = true;

    app.preview_dirty = true;
    app.update_preview();

    assert_eq!(app.preview.len(), 1);
    assert_eq!(app.preview[0].new_name(), "track.mp3");
}

#[test]
fn test_execute_renames_respects_selection() {
    // Files contain "old" so the replace config replaces them
    let files = ["old_a.txt", "old_b.txt", "old_c.txt"];
    let (mut app, _dir) = selection_test_app(&files);

    // Deselect the middle file
    app.selection[1] = false;

    // Run execute
    app.preview_dirty = true;
    app.execute_renames();

    // After execution, undo_stack should only contain ops for
    // selected files (deselected middle file is excluded).
    assert!(
        !app.cwd_commits().is_empty(),
        "cwd_commits should be non-empty"
    );
    let ops = &app.cwd_commits().last().unwrap().ops;
    assert_eq!(ops.len(), 2, "only 2 selected files should be renamed");
    assert!(ops.iter().any(|op| op.original_name == "old_a.txt"));
    assert!(ops.iter().any(|op| op.original_name == "old_c.txt"));
    assert!(!ops.iter().any(|op| op.original_name == "old_b.txt"));
}
#[test]
fn test_action_rearms_selection_rules() {
    // Single and Range selections always re-arm; a toggle re-arms only
    // when the file ends up selected (pure deselect does not).
    use crate::gui::helpers::{ClickAction, action_rearms};
    assert!(action_rearms(&ClickAction::Single(0), &[false]));
    assert!(action_rearms(
        &ClickAction::Range { start: 0, end: 2 },
        &[false, true, true]
    ));
    assert!(action_rearms(&ClickAction::Toggle(0), &[true, false]));
    assert!(
        !action_rearms(&ClickAction::Toggle(0), &[false, false]),
        "toggle-off must not re-arm"
    );
}

#[test]
fn test_numbering_after_partial_reselect_applies_to_selection() {
    // Apply numbering to the whole batch → processed. Reselect a subset
    // (re-arms via the selection action) → numbering applies to the
    // reselected subset with batch-consistent positions.
    let (mut app, _dir) = selection_test_app(&["a.txt", "b.txt", "c.txt"]);
    app.section_enabled[SectionId::Numbering as usize] = true;
    app.config.numbering.numbering_mode = Some(NumberingMode::Prefix);
    app.config.numbering.numbering_start = 1;
    app.config.numbering.numbering_increment = 1;
    app.config.numbering.numbering_pad = 0;
    app.config.numbering.numbering_sep = Some("_".to_string());
    app.preview_dirty = true;
    app.execute_renames();
    assert!(app.processed);

    // Reselect a subset: b.txt and c.txt (like a shift-click range,
    // which re-arms the guard). Display order after the apply is
    // 1_a.txt(0), 2_b.txt(1), 3_c.txt(2).
    app.selection = vec![false, true, true];
    app.set_processed(false);
    app.selection_generation += 1;
    app.preview_dirty = true;
    app.update_preview();

    // Numbering positions span the full selection: b is position 0 of the
    // reselected set → prefix "1_"; c is position 1 → prefix "2_". The
    // op operates on the current names (2_b.txt / 3_c.txt), matching what
    // the preview shows before applying.
    let b = app.preview.iter().find(|p| p.index == 1).unwrap();
    assert!(b.selected);
    assert_eq!(b.new_name(), "1_2_b.txt");
    let c = app.preview.iter().find(|p| p.index == 2).unwrap();
    assert_eq!(c.new_name(), "2_3_c.txt");
    let a = app.preview.iter().find(|p| p.index == 0).unwrap();
    assert!(!a.selected, "deselected file stays out of the numbering");
}

#[cfg(windows)]
#[test]
fn test_execute_records_physical_name_on_windows() {
    // Windows strips trailing spaces from created names, so after apply the
    // in-memory file list must reflect the on-disk (trimmed) name.
    let dir = TempDir::new().unwrap();
    File::create(dir.path().join("oldname.txt")).unwrap();
    let mut app = GuiApp::new_with_config_for_test();
    // Point cwd at the tempdir so the renamed file stays in the listing
    // (scope decisions compare the new path's parent against cwd).
    app.cwd = dir.path().to_path_buf();
    app.cwd_input = dir.path().to_string_lossy().to_string();
    app.all_files = vec![dir.path().join("oldname.txt")];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];
    app.section_enabled[SectionId::Replace as usize] = true;
    app.config.copy_to.copy_mode = false;
    app.config.replace.replace = Some("oldname".into());
    app.config.replace.with = Some("name ".into());
    app.preview_dirty = true;
    app.execute_renames();

    // On disk the target "name .txt" is created as "name.txt"...
    let disk: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(disk, vec!["name.txt"]);
    // ...and the in-memory list matches disk, not the logical preview name.
    assert_eq!(
        app.file_names,
        vec![dir.path().join("name.txt").to_string_lossy().to_string()]
    );
}

#[test]
fn test_execute_renames_invalidates_tree_cache_for_dir_rename() {
    let tmp = TempDir::new().unwrap();
    // Create: root/subdir/child.txt
    let subdir = tmp.path().join("subdir");
    std::fs::create_dir(&subdir).unwrap();
    let child = subdir.join("child.txt");
    File::create(&child).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![subdir.clone()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];

    // Simulate tree cache as if user navigated here
    app.tree_expanded.insert(subdir.clone());
    app.tree_scanned.insert(subdir.clone());
    app.tree_children
        .insert(subdir.clone(), vec![child.clone()]);
    app.tree_scanned.insert(child.clone());

    // Also cache the parent (will be invalidated for re-scan)
    app.tree_scanned.insert(tmp.path().to_path_buf());
    app.tree_children
        .insert(tmp.path().to_path_buf(), vec![subdir.clone()]);

    app.cwd = tmp.path().to_path_buf();

    // Set up replace: "subdir" -> "newdir"
    app.config.replace.replace = Some("subdir".into());
    app.config.replace.with = Some("newdir".into());
    app.section_enabled[SectionId::Replace as usize] = true;
    // Disable MoveCopy and CopyTo — their defaults would interfere with rename tests
    app.section_enabled[SectionId::MoveCopy as usize] = false;
    app.config.copy_to.copy_mode = false;

    app.preview_dirty = true;
    app.execute_renames();

    let new_subdir = tmp.path().join("newdir");
    let new_child = new_subdir.join("child.txt");

    // Debug: verify the rename actually happened on disk
    assert!(
        new_subdir.is_dir(),
        "directory should exist at new path: {:?}",
        new_subdir
    );
    assert!(
        !subdir.exists(),
        "old directory should be gone: {:?}",
        subdir
    );

    // Verify selection preserved
    assert_eq!(app.selection, vec![true], "selection should be preserved");

    // Verify undo_stack has the commit (status symbols survive)
    assert!(
        !app.cwd_commits().is_empty(),
        "cwd_commits should have commits"
    );
    let ops = &app.cwd_commits().last().unwrap().ops;
    assert_eq!(ops.len(), 1, "should have 1 successful op");
    assert_eq!(
        ops[0].new_path, new_subdir,
        "op.new_path should be the renamed dir"
    );

    // Verify old tree cache entries removed
    assert!(
        !app.tree_children.contains_key(&subdir),
        "old dir key removed from tree_children"
    );
    assert!(
        !app.tree_scanned.contains(&subdir),
        "old dir key removed from tree_scanned"
    );
    assert!(
        !app.tree_expanded.contains(&subdir),
        "old dir key removed from tree_expanded"
    );

    // Verify new tree cache entries present
    assert!(
        app.tree_children.contains_key(&new_subdir),
        "new dir key in tree_children"
    );
    assert!(
        app.tree_scanned.contains(&new_subdir),
        "new dir key in tree_scanned"
    );
    assert!(
        app.tree_expanded.contains(&new_subdir),
        "new dir key in tree_expanded"
    );

    // Verify children transferred
    assert_eq!(
        app.tree_children[&new_subdir],
        vec![new_child],
        "child paths re-keyed under new dir"
    );

    // Verify parent invalidated for re-scan
    assert!(
        !app.tree_scanned.contains(tmp.path()),
        "parent dir should be invalidated for re-scan"
    );

    // Parent's stale children list kept until ensure_tree_scanned re-reads
    assert_eq!(
        app.tree_children[tmp.path()],
        vec![subdir],
        "parent children list is stale until re-scan"
    );

    // all_files updated in-place
    assert_eq!(app.all_files, vec![new_subdir], "all_files paths updated");
}

// ──────────────────────────────────────────
// Copy / Move to Location is not undoable
// ──────────────────────────────────────────

/// A Copy / Move to Location apply relocates entries rather than renaming them
/// in place, so it must not create an undo/redo/revert commit.
#[test]
fn test_copy_to_apply_creates_no_commit() {
    let (mut app, dir) = selection_test_app(&["a.txt"]);
    app.cwd = dir.path().to_path_buf();
    let out = dir.path().join("out");
    app.config.copy_to.output_dir = Some(out.to_string_lossy().to_string());
    app.config.copy_to.copy_mode = true;
    app.preview_dirty = true;
    app.execute_renames();

    assert!(
        out.join("a.txt").exists(),
        "the copy landed in the output dir"
    );
    assert!(
        app.cwd_commits().is_empty(),
        "a Copy/Move to Location apply must not create a commit"
    );
}

/// Move mode is a location transfer too (the op reports `was_copy = false`), so it
/// must also stay out of the history.
#[test]
fn test_copy_to_move_apply_creates_no_commit() {
    let (mut app, dir) = selection_test_app(&["a.txt"]);
    app.cwd = dir.path().to_path_buf();
    let out = dir.path().join("out");
    app.config.copy_to.output_dir = Some(out.to_string_lossy().to_string());
    app.config.copy_to.copy_mode = false;
    app.preview_dirty = true;
    app.execute_renames();

    assert!(
        out.join("a.txt").exists(),
        "the file moved to the output dir"
    );
    assert!(
        !dir.path().join("a.txt").exists(),
        "source vacated by the move"
    );
    assert!(
        app.cwd_commits().is_empty(),
        "a move to location must not create a commit"
    );
}

/// A Copy / Move to Location apply leaves existing history untouched — it does
/// not add a commit, so undo/redo/revert continue to see only the real renames.
#[test]
fn test_copy_to_apply_preserves_existing_commits() {
    let (mut app, dir) = selection_test_app(&["a.txt"]);
    app.cwd = dir.path().to_path_buf();
    app.cwd_commits_mut().push(Commit {
        timestamp: std::time::Instant::now(),
        label: "Seed".into(),
        ops: vec![],
    });

    let out = dir.path().join("out");
    app.config.copy_to.output_dir = Some(out.to_string_lossy().to_string());
    app.preview_dirty = true;
    app.execute_renames();

    assert!(out.join("a.txt").exists(), "the transfer happened");
    assert_eq!(app.cwd_commits().len(), 1, "no commit was added");
    assert_eq!(app.cwd_commits()[0].label, "Seed");
}

/// In-place renames of a subdirectory and its child in the same apply must both
/// be recorded for undo/redo/revert — they are not Copy/Move to Location
/// transfers, even though the child sits inside the renamed parent.
#[test]
fn test_subdir_and_child_renames_are_recorded() {
    let tmp = TempDir::new().unwrap();
    let subdir = tmp.path().join("subdir");
    std::fs::create_dir(&subdir).unwrap();
    let child = subdir.join("a.txt");
    File::create(&child).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![child.clone(), subdir.clone()];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.selection = vec![true, true];
    app.cwd = tmp.path().to_path_buf();
    // Prefix both: "subdir" -> "x_subdir" and "a.txt" -> "x_a.txt" (in place).
    app.config.add.add_prefix = Some("x_".into());
    app.section_enabled[SectionId::Add as usize] = true;
    app.config.replace.replace = None;
    app.config.replace.with = None;
    app.preview_dirty = true;
    app.execute_renames();

    assert!(tmp.path().join("x_subdir").is_dir(), "dir renamed in place");
    assert!(
        tmp.path().join("x_subdir").join("x_a.txt").is_file(),
        "child renamed inside the renamed dir"
    );

    let commits = app.cwd_commits();
    assert_eq!(commits.len(), 1, "the in-place rename committed");
    assert_eq!(
        commits[0].ops.len(),
        2,
        "the subdir and its child are both undoable"
    );
}

// ──────────────────────────────────────────
// Path-producing renames (computed name carries a separator)
// ──────────────────────────────────────────

/// A rename whose computed name contains a path separator creates a
/// subdirectory and moves the entry into it. It is a structural transfer like
/// Copy / Move to Location, so it must not create an undo/redo/revert commit.
#[test]
fn test_path_producing_rename_creates_no_commit() {
    let (mut app, dir) = selection_test_app(&["old_a.txt"]);
    app.cwd = dir.path().to_path_buf();
    app.config.replace.replace = Some("old".into());
    app.config.replace.with = Some("sub/new".into());
    app.skip_dir_creation_warning = true;
    app.preview_dirty = true;
    app.execute_renames();

    assert!(
        dir.path().join("sub").is_dir(),
        "the subdirectory was created"
    );
    assert!(
        dir.path().join("sub/new_a.txt").exists(),
        "the entry moved into the subpath"
    );
    // Surgical listing update (no manual refresh): the moved file leaves the
    // pane and the new folder appears in it.
    let sub = dir.path().join("sub");
    assert!(
        !app.all_files.iter().any(|p| p.ends_with("old_a.txt")),
        "the moved file is gone from the listing: {:?}",
        app.all_files
    );
    assert!(
        app.all_files.contains(&sub),
        "the new folder appears in the listing: {:?}",
        app.all_files
    );
    assert!(
        app.cwd_commits().is_empty(),
        "a path-producing rename must not create a commit"
    );
}

/// The new subdirectory must appear without a manual refresh: the cwd (its
/// grandparent) is invalidated so the tree re-reads its children.
#[test]
fn test_path_producing_rename_invalidates_tree_for_new_parent() {
    let (mut app, dir) = selection_test_app(&["old_a.txt"]);
    app.cwd = dir.path().to_path_buf();
    app.cwd_input = app.cwd.to_string_lossy().to_string();
    // Simulate the cwd having already been scanned by the tree.
    app.tree_scanned.insert(app.cwd.clone());
    app.tree_children.insert(app.cwd.clone(), vec![]);

    app.config.replace.replace = Some("old".into());
    app.config.replace.with = Some("sub/new".into());
    app.skip_dir_creation_warning = true;
    app.preview_dirty = true;
    app.execute_renames();

    assert!(
        !app.tree_scanned.contains(&app.cwd),
        "cwd must be re-read so the new subfolder shows"
    );
}

/// With subfolders on, the moved entry is still under the cwd, so its row stays
/// (re-pointed at the new location) while the new folder is still added.
#[test]
fn test_path_producing_rename_keeps_row_when_recursive() {
    let (mut app, dir) = selection_test_app(&["old_a.txt"]);
    app.cwd = dir.path().to_path_buf();
    app.config.filters.filter_subfolders = true;
    app.config.replace.replace = Some("old".into());
    app.config.replace.with = Some("sub/new".into());
    app.skip_dir_creation_warning = true;
    app.preview_dirty = true;
    app.execute_renames();

    let moved = dir.path().join("sub/new_a.txt");
    assert!(
        app.all_files.contains(&moved),
        "recursive listing keeps the moved entry: {:?}",
        app.all_files
    );
    assert!(
        app.all_files.contains(&dir.path().join("sub")),
        "the new folder is added too: {:?}",
        app.all_files
    );
}

/// The raw (pre-filter) scan is kept in step, so changing a non-structural
/// filter after a `folder/file` relocation neither resurrects the moved entry
/// nor drops the new folder.
#[test]
fn test_path_producing_rename_keeps_raw_scan_consistent() {
    let (mut app, dir) = selection_test_app(&["old_a.txt"]);
    // Mirror a real scan: raw holds the scanned entry the filtered list came from.
    app.raw_all_files = vec![dir.path().join("old_a.txt")];
    app.raw_file_sizes = vec![0];
    app.raw_file_dates = vec![0];
    app.raw_file_is_dir = vec![false];

    app.config.replace.replace = Some("old".into());
    app.config.replace.with = Some("sub/new".into());
    app.skip_dir_creation_warning = true;
    app.preview_dirty = true;
    app.execute_renames();

    // A non-structural filter change rebuilds the listing from raw.
    app.apply_filters_in_memory();

    let sub = dir.path().join("sub");
    assert!(
        !app.all_files.iter().any(|p| p.ends_with("old_a.txt")),
        "the moved entry is not resurrected: {:?}",
        app.all_files
    );
    assert!(
        app.all_files.contains(&sub),
        "the new folder survives a filter rebuild: {:?}",
        app.all_files
    );
}

/// A new folder the in-memory filters would exclude is not inserted into the
/// pane — it still lands in the raw scan, so clearing the filter reveals it.
#[test]
fn test_path_producing_rename_respects_mask_for_new_folder() {
    let (mut app, dir) = selection_test_app(&["old_a.txt"]);
    // Mirror a scan so the raw listing is live (a no-op append would otherwise
    // be invisible here).
    app.raw_all_files = vec![dir.path().join("old_a.txt")];
    app.raw_file_sizes = vec![0];
    app.raw_file_dates = vec![0];
    app.raw_file_is_dir = vec![false];
    // Mask matching only the renamed file, so the new `sub` folder is excluded.
    app.config.filters.filter_pattern = Some("old_*".into());
    app.config.replace.replace = Some("old".into());
    app.config.replace.with = Some("sub/new".into());
    app.skip_dir_creation_warning = true;
    app.preview_dirty = true;
    app.execute_renames();

    assert!(
        dir.path().join("sub").is_dir(),
        "the folder was created on disk"
    );
    assert!(
        !app.all_files.contains(&dir.path().join("sub")),
        "the filtered-out folder is not shown: {:?}",
        app.all_files
    );
    assert!(
        app.raw_all_files.contains(&dir.path().join("sub")),
        "but it is present in the raw scan: {:?}",
        app.raw_all_files
    );
}

/// A move to a location outside the folder drops the row as well — the entry is
/// no longer in the cwd, so the pane matches a refresh.
#[test]
fn test_move_to_location_removes_row() {
    let (mut app, dir) = selection_test_app(&["a.txt"]);
    app.cwd = dir.path().to_path_buf();
    let out = dir.path().join("out");
    app.config.copy_to.output_dir = Some(out.to_string_lossy().to_string());
    app.config.copy_to.copy_mode = false; // move, not copy
    app.preview_dirty = true;
    app.execute_renames();

    assert!(out.join("a.txt").exists(), "the file moved to the location");
    assert!(
        !app.all_files
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "a.txt")),
        "the moved file leaves the listing: {:?}",
        app.all_files
    );
    assert!(
        app.all_files.contains(&out),
        "the destination folder is added: {:?}",
        app.all_files
    );
}

// ──────────────────────────────────────────
// Apply gates: navigational names block, folder creation warns
// ──────────────────────────────────────────

/// A computed name that navigates the path blocks Apply (SSoT:
/// `path_navigation_error`) and schedules nothing.
#[test]
fn test_invalid_new_name_blocks_apply() {
    let (mut app, dir) = selection_test_app(&["old_a.txt"]);
    app.config.replace.replace = Some("old".into());
    app.config.replace.with = Some("../escaped".into());
    app.preview_dirty = true;
    app.execute_renames();

    assert!(app.show_invalid_name_warning, "the modal is shown");
    assert!(!app.invalid_name_entries.is_empty());
    assert!(!app.processed, "the processed guard was not disarmed");
    assert!(dir.path().join("old_a.txt").exists(), "nothing renamed");
    assert!(app.cwd_commits().is_empty(), "nothing committed");
}

/// A navigational name that would otherwise be a no-op (`./a.txt`) still blocks
/// Apply — it must not fall through to "No files to rename".
#[test]
fn test_navigational_noop_blocks_apply() {
    let (mut app, dir) = selection_test_app(&["a.txt"]);
    app.section_enabled[SectionId::Replace as usize] = false;
    app.config.add.add_prefix = Some("./".into());
    app.section_enabled[SectionId::Add as usize] = true;
    app.preview_dirty = true;
    app.execute_renames();

    assert!(
        app.show_invalid_name_warning,
        "the invalid name is surfaced, not 'No files to rename'"
    );
    assert!(!app.invalid_name_entries.is_empty());
    assert!(!app.status_message.contains("No files to rename"));
    assert!(dir.path().join("a.txt").exists(), "nothing renamed");
}

/// F2 (inline) guards navigational names too, including a path-equal one
/// (`./a.txt`) that the no-op shortcut would otherwise swallow silently.
#[test]
fn test_f2_navigational_name_blocks_including_path_equal() {
    let (mut app, dir) = selection_test_app(&["a.txt"]);
    app.editing_idx = Some(0);
    app.edit_buffer = "./a.txt".into();
    app.commit_edit();

    assert!(
        app.show_invalid_name_warning,
        "explained, not silently dropped"
    );
    assert!(!app.invalid_name_entries.is_empty());
    assert!(app.editing_idx.is_none(), "edit closed");
    assert!(dir.path().join("a.txt").exists(), "nothing renamed");
}

/// F2 creating subfolders warns first; nothing runs until confirm, then it
/// executes inline.
#[test]
fn test_f2_folder_creation_warning_gates_then_confirms() {
    let (mut app, dir) = selection_test_app(&["a.txt"]);
    app.editing_idx = Some(0);
    app.edit_buffer = "sub/a.txt".into();
    app.commit_edit();

    assert!(
        app.show_dir_creation_warning,
        "warned before creating folders"
    );
    assert!(app.pending_dir_plan.is_some());
    assert!(!dir.path().join("sub").exists(), "not executed yet");

    app.confirm_dir_creation_warning();
    assert!(!app.show_dir_creation_warning);
    assert!(
        dir.path().join("sub/a.txt").exists(),
        "executed inline on confirm"
    );
}

/// F2 folder creation is skipped when "Don't ask again" is set.
#[test]
fn test_f2_folder_creation_skipped_when_disabled() {
    let (mut app, dir) = selection_test_app(&["a.txt"]);
    app.skip_dir_creation_warning = true;
    app.editing_idx = Some(0);
    app.edit_buffer = "sub/a.txt".into();
    app.commit_edit();

    assert!(!app.show_dir_creation_warning);
    assert!(dir.path().join("sub/a.txt").exists());
}

/// When the F2 folder-creating rename also collides, confirming the warning
/// resumes into the **F2** collision dialog (so execution stays inline).
#[test]
fn test_f2_folder_creation_warning_preserves_collision_dialog() {
    let (mut app, dir) = selection_test_app(&["a.txt"]);
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub/a.txt"), b"x").unwrap();
    app.editing_idx = Some(0);
    app.edit_buffer = "sub/a.txt".into();
    app.commit_edit();

    assert!(app.show_dir_creation_warning, "warned first");
    app.confirm_dir_creation_warning();

    let state = app.collision_dialog.as_ref().expect("collision dialog");
    assert!(state.is_f2, "F2 dialog keeps execution inline");
}

/// A rename that will create folders warns first; nothing runs until confirm.
#[test]
fn test_dir_creation_warning_gates_then_confirms() {
    let (mut app, dir) = selection_test_app(&["old_a.txt"]);
    app.config.replace.replace = Some("old".into());
    app.config.replace.with = Some("sub/new".into());
    app.preview_dirty = true;
    app.execute_renames();

    assert!(app.show_dir_creation_warning, "warning shown");
    assert_eq!(app.pending_dir_count, 1);
    assert!(app.pending_dir_plan.is_some());
    assert!(!dir.path().join("sub").exists(), "not executed yet");

    app.confirm_dir_creation_warning();
    assert!(!app.show_dir_creation_warning);
    assert!(
        dir.path().join("sub/new_a.txt").exists(),
        "executed after confirm"
    );
}

/// Cancelling the warning runs nothing and leaves the guard armed.
#[test]
fn test_dir_creation_warning_cancel_does_nothing() {
    let (mut app, dir) = selection_test_app(&["old_a.txt"]);
    app.config.replace.replace = Some("old".into());
    app.config.replace.with = Some("sub/new".into());
    app.preview_dirty = true;
    app.execute_renames();

    app.cancel_dir_creation_warning();
    assert!(!app.show_dir_creation_warning);
    assert!(app.pending_dir_plan.is_none());
    assert!(!dir.path().join("sub").exists(), "nothing ran");
    assert!(!app.processed, "the processed guard stayed armed");
}

/// "Don't ask again" bypasses the warning and executes immediately.
#[test]
fn test_dir_creation_warning_skipped_when_disabled() {
    let (mut app, dir) = selection_test_app(&["old_a.txt"]);
    app.skip_dir_creation_warning = true;
    app.config.replace.replace = Some("old".into());
    app.config.replace.with = Some("sub/new".into());
    app.preview_dirty = true;
    app.execute_renames();

    assert!(!app.show_dir_creation_warning, "warning bypassed");
    assert!(
        dir.path().join("sub/new_a.txt").exists(),
        "executed directly"
    );
}
