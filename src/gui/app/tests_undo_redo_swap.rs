use super::*;
use tempfile::TempDir;

#[test]
fn test_undo_files_batch_restores_multiple_and_rekeys_dirs() {
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().to_path_buf();
    let a = parent.join("a");
    let b = parent.join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    std::fs::create_dir(a.join("x")).unwrap();
    std::fs::create_dir(b.join("y")).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = parent.clone();
    app.all_files = vec![a.clone(), b.clone()];
    app.file_names = vec![
        a.to_string_lossy().to_string(),
        b.to_string_lossy().to_string(),
    ];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.selection = vec![true, true];

    // Tree caches for both dirs (expanded + scanned)
    for (d, child) in [(&a, "x"), (&b, "y")] {
        app.tree_expanded.insert(d.clone());
        app.tree_scanned.insert(d.clone());
        app.tree_children.insert(d.clone(), vec![d.join(child)]);
        app.tree_has_subdirs.insert(d.clone(), true);
    }

    // F2 rename a → a2, b → b2
    app.editing_idx = Some(0);
    app.edit_buffer = "a2".into();
    app.commit_edit();
    app.editing_idx = Some(1);
    app.edit_buffer = "b2".into();
    app.commit_edit();

    let a2 = parent.join("a2");
    let b2 = parent.join("b2");
    assert!(a2.is_dir() && b2.is_dir());

    // Undo both at once — one batch, both restored, tree re-keyed
    let n = app.undo_files(&[0, 1]);
    assert_eq!(n, 2, "both undos succeed");
    assert!(a.is_dir() && b.is_dir());
    assert!(!a2.exists() && !b2.exists());
    assert!(app.tree_children.contains_key(&a));
    assert!(app.tree_children.contains_key(&b));
    assert_eq!(app.tree_has_subdirs.get(&a), Some(&true));
    assert!(!app.tree_children.contains_key(&a2));
    assert!(!app.tree_children.contains_key(&b2));

    // ONE Revert commit for the whole batch, with both inverse ops
    let commits = app.commits_by_dir.get(&parent).unwrap();
    let revert: Vec<&Commit> = commits
        .iter()
        .filter(|c| c.label.starts_with("Revert"))
        .collect();
    assert_eq!(revert.len(), 1, "single batch Revert commit");
    assert_eq!(revert[0].ops.len(), 2, "both inverse ops in one commit");

    // Redo both at once
    let n = app.redo_files(&[0, 1]);
    assert_eq!(n, 2, "both redos succeed");
    assert!(a2.is_dir() && b2.is_dir());
    assert!(!a.exists() && !b.exists());
    assert!(app.tree_children.contains_key(&a2));
    assert!(app.tree_children.contains_key(&b2));
    assert!(!app.tree_children.contains_key(&a));
}

#[test]
fn test_undo_files_swap_pair_resolves_with_temp_deferral() {
    // A swap pair (a↔b) is fully undone via temp-name deferral — the file
    // parked at a temp name is completed to its final target after the
    // conflicting op vacates the path, so nothing is clobbered.
    let tmp = TempDir::new().unwrap();
    let a = tmp.path().join("a.txt");
    let b = tmp.path().join("b.txt");
    // Post-apply state: a's content is at b, b's content is at a.
    std::fs::write(&b, b"content_a").unwrap();
    std::fs::write(&a, b"content_b").unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![b.clone(), a.clone()];
    app.file_names = vec![
        b.to_string_lossy().to_string(),
        a.to_string_lossy().to_string(),
    ];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.selection = vec![true, true];

    let mk = |orig: &Path, new: &Path, orig_name: &str, new_name: &str| RenameOp {
        original_path: orig.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: orig_name.into(),
        new_name: new_name.into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    app.commits_by_dir.insert(
        tmp.path().to_path_buf(),
        vec![Commit {
            timestamp: std::time::Instant::now(),
            label: "Apply".into(),
            ops: vec![mk(&a, &b, "a.txt", "b.txt"), mk(&b, &a, "b.txt", "a.txt")],
        }],
    );

    let n = app.undo_files(&[0, 1]);
    assert_eq!(n, 2, "swap resolved via temp deferral");
    assert_eq!(
        std::fs::read(&a).unwrap(),
        b"content_a",
        "a's content restored"
    );
    assert_eq!(
        std::fs::read(&b).unwrap(),
        b"content_b",
        "b's content restored"
    );
    assert_eq!(app.all_files[0], a);
    assert_eq!(app.all_files[1], b);
}

#[test]
fn test_redo_files_swap_pair() {
    // Redo of a swap pair must re-apply the swap chain-safely, restoring
    // the post-apply state.
    let tmp = TempDir::new().unwrap();
    let a = tmp.path().join("a.txt");
    let b = tmp.path().join("b.txt");
    std::fs::write(&b, b"content_a").unwrap();
    std::fs::write(&a, b"content_b").unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![b.clone(), a.clone()];
    app.file_names = vec![
        b.to_string_lossy().to_string(),
        a.to_string_lossy().to_string(),
    ];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.selection = vec![true, true];

    let mk = |orig: &Path, new: &Path, orig_name: &str, new_name: &str| RenameOp {
        original_path: orig.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: orig_name.into(),
        new_name: new_name.into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    app.commits_by_dir.insert(
        tmp.path().to_path_buf(),
        vec![Commit {
            timestamp: std::time::Instant::now(),
            label: "Apply".into(),
            ops: vec![mk(&a, &b, "a.txt", "b.txt"), mk(&b, &a, "b.txt", "a.txt")],
        }],
    );

    assert_eq!(app.undo_files(&[0, 1]), 2, "undo resolves");
    assert_eq!(std::fs::read(&a).unwrap(), b"content_a");
    assert_eq!(std::fs::read(&b).unwrap(), b"content_b");

    // Redo re-applies the swap: a's content back to b, b's back to a.
    assert_eq!(app.redo_files(&[0, 1]), 2, "redo resolves");
    assert_eq!(
        std::fs::read(&b).unwrap(),
        b"content_a",
        "a's content at b again"
    );
    assert_eq!(
        std::fs::read(&a).unwrap(),
        b"content_b",
        "b's content at a again"
    );
}

/// After an undo, the display names, fuzzy search index, and cached sort
/// order must reflect the restored names immediately (no rescan needed).
#[test]
fn test_undo_refreshes_fuzzy_index_and_sort_cache() {
    let tmp = TempDir::new().unwrap();
    let a = tmp.path().join("a.txt");
    let renamed = tmp.path().join("zzz.txt");
    std::fs::write(&renamed, b"data").unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![renamed.clone()];
    app.file_names = vec![renamed.to_string_lossy().to_string()];
    app.file_display_names = vec!["zzz.txt".into()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];
    app.fuzzy_index = FuzzyIndex::from_items(&["zzz.txt".to_string()]);

    app.commits_by_dir.insert(
        tmp.path().to_path_buf(),
        vec![Commit {
            timestamp: std::time::Instant::now(),
            label: "Apply".into(),
            ops: vec![RenameOp {
                original_path: a.clone(),
                new_path: renamed.clone(),
                original_name: "a.txt".into(),
                new_name: "zzz.txt".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0,
            }],
        }],
    );

    app.undo_files(&[0]);

    assert!(a.exists(), "restored on disk");
    assert_eq!(
        app.file_display_names,
        vec!["a.txt"],
        "display names refreshed"
    );
    assert!(
        app.fuzzy_index.search("a.txt").iter().any(|m| m.index == 0),
        "fuzzy index finds the restored name"
    );
    assert!(
        app.display_order_dirty.get(),
        "sort cache invalidated so the table re-sorts"
    );
}

#[test]
fn test_undo_files_genuine_collision_fails_without_clobber() {
    // Target re-created externally (not part of the batch): the undo must
    // skip (reported as failed) and leave both files untouched.
    let tmp = TempDir::new().unwrap();
    let a = tmp.path().join("a.txt");
    let b = tmp.path().join("b.txt");
    std::fs::write(&b, b"content_a").unwrap(); // a→b was applied
    std::fs::write(&a, b"unrelated").unwrap(); // a re-created externally

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![b.clone()];
    app.file_names = vec![b.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];

    app.commits_by_dir.insert(
        tmp.path().to_path_buf(),
        vec![Commit {
            timestamp: std::time::Instant::now(),
            label: "Apply".into(),
            ops: vec![RenameOp {
                original_path: a.clone(),
                new_path: b.clone(),
                original_name: "a.txt".into(),
                new_name: "b.txt".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0,
            }],
        }],
    );

    let n = app.undo_files(&[0]);
    assert_eq!(n, 0, "genuine collision is not clobbered");
    assert_eq!(std::fs::read(&a).unwrap(), b"unrelated", "target untouched");
    assert_eq!(std::fs::read(&b).unwrap(), b"content_a", "source untouched");
    assert!(
        app.status_message.contains("0 reverted, 1 failed"),
        "failure reported: {}",
        app.status_message
    );
}

#[test]
fn test_undo_files_skips_items_without_history() {
    let tmp = TempDir::new().unwrap();
    let a = tmp.path().join("a.txt");
    let renamed = tmp.path().join("renamed.txt");
    let b = tmp.path().join("b.txt");
    std::fs::write(&renamed, b"").unwrap();
    std::fs::write(&b, b"").unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![renamed.clone(), b.clone()];
    app.file_names = vec![
        renamed.to_string_lossy().to_string(),
        b.to_string_lossy().to_string(),
    ];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.selection = vec![true, true];

    // History only for idx 0 (a.txt → renamed.txt)
    app.commits_by_dir.insert(
        tmp.path().to_path_buf(),
        vec![Commit {
            timestamp: std::time::Instant::now(),
            label: "Apply".into(),
            ops: vec![RenameOp {
                original_path: a.clone(),
                new_path: renamed.clone(),
                original_name: "a.txt".into(),
                new_name: "renamed.txt".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0,
            }],
        }],
    );

    let n = app.undo_files(&[0, 1]);
    assert_eq!(n, 1, "only the item with history is undone");
    assert!(a.exists(), "a.txt restored");
    assert!(!renamed.exists(), "renamed.txt gone");
    assert_eq!(app.all_files[0], a);
    assert_eq!(app.all_files[1], b, "no-history item untouched");
    // Single successful undo → detailed message (not the batch summary)
    assert!(
        app.status_message.starts_with("Undid rename: "),
        "status reported: {}",
        app.status_message
    );
}

#[test]
fn test_undo_redo_no_current_file_skips_silently() {
    // undo_file/redo_file with an out-of-bounds index should not panic
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("file.txt")];
    app.file_names = vec!["file.txt".into()];

    app.undo_file(999); // out of bounds
    app.redo_file(999); // out of bounds

    app.undo_file(0); // exists but no commits
    app.redo_file(0);
}
