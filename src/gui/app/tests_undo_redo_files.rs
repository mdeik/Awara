use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_undo_redo_single_f2_rename() {
    // F2: a.txt → b.txt → undo → redo
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    File::create(&a).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = vec![a.clone()];
    app.file_names = vec![a.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.selection = vec![true];

    // Simulate F2 rename: a.txt → b.txt
    let b = dir.path().join("b.txt");
    std::fs::rename(&a, &b).unwrap();
    app.all_files[0] = b.clone();
    app.file_names[0] = b.to_string_lossy().to_string();
    app.cwd_commits_mut().push(Commit {
        timestamp: std::time::Instant::now(),
        label: "Inline — test".into(),
        ops: vec![RenameOp {
            original_path: a.clone(),
            new_path: b.clone(),
            original_name: "a.txt".into(),
            new_name: "b.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        }],
    });

    // find_undo_op should find the op
    assert!(app.find_undo_op(0).is_some(), "should find undo op");
    assert!(
        app.find_redo_op(0).is_none(),
        "no redo after forward rename"
    );

    // Undo: b.txt → a.txt
    app.undo_file(0);
    assert!(a.exists(), "a.txt should exist after undo");
    assert!(!b.exists(), "b.txt should not exist after undo");
    assert_eq!(app.all_files[0], a, "all_files should point to a.txt");

    // After undo: find_undo_op should Return None (nothing more to undo)
    assert!(app.find_undo_op(0).is_none(), "no undo after single undo");
    // find_redo_op should find the op (can redo)
    assert!(
        app.find_redo_op(0).is_some(),
        "should find redo op after undo"
    );

    // Redo: a.txt → b.txt
    app.redo_file(0);
    assert!(!a.exists(), "a.txt should not exist after redo");
    assert!(b.exists(), "b.txt should exist after redo");
    assert_eq!(app.all_files[0], b, "all_files should point to b.txt");

    // After redo: find_undo_op should find the op again
    assert!(
        app.find_undo_op(0).is_some(),
        "should find undo op after redo"
    );
    // A Revert commit was created by the first undo; after redo the file is
    // back at b, so redo's original_path = b (from the Revert commit) matches.
    // The toggle is now recorded and redo is still available to cycle back.
    assert!(
        app.find_redo_op(0).is_some(),
        "redo should still be available after redo (cycling through Revert commit)"
    );

    // Toggle back via Ctrl+Z: head is Revert, so no new commit.
    app.undo_file(0);
    assert!(a.exists(), "a.txt after second undo");

    // Only one Revert commit should exist in the history.
    let revert_count = app
        .cwd_commits()
        .iter()
        .filter(|c| c.label.starts_with("Revert"))
        .count();
    assert_eq!(revert_count, 1, "only one Revert commit should be created");
}

/// An effects-only apply (attributes with the name unchanged) is revertible:
/// undo restores the pre-effect snapshot, redo re-applies the attribute.
#[test]
fn test_undo_redo_metadata_only_commit() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    File::create(&a).unwrap();

    // Forward: attribute applied in place; snapshot captured before.
    let snap = awara::PermSnapshot::capture(&a).unwrap();
    let mut perms = std::fs::metadata(&a).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&a, perms).unwrap();
    assert!(std::fs::metadata(&a).unwrap().permissions().readonly());

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = vec![a.clone()];
    app.file_names = vec![a.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.file_is_dir = vec![false];
    app.selection = vec![true];
    app.cwd_commits_mut().push(Commit {
        timestamp: std::time::Instant::now(),
        label: "Apply — test".into(),
        ops: vec![RenameOp {
            original_path: a.clone(),
            new_path: a.clone(),
            original_name: "a.txt".into(),
            new_name: "a.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: Some(snap),
            applied_attributes: "readonly".into(),
            applied_timestamps_mask: 0u8,
        }],
    });

    assert!(app.find_undo_op(0).is_some(), "effects-only op is undoable");
    app.undo_file(0);
    assert!(
        !std::fs::metadata(&a).unwrap().permissions().readonly(),
        "undo restores the pre-effect metadata"
    );
    assert!(a.exists(), "the name never changed");

    assert!(app.find_redo_op(0).is_some(), "effects-only op is redoable");
    app.redo_file(0);
    assert!(
        std::fs::metadata(&a).unwrap().permissions().readonly(),
        "redo re-applies the attribute"
    );
}

#[test]
fn test_undo_redo_multi_commit_chain() {
    // Chain: a.txt → b.txt (F2) → c.txt (F2) → undo → undo → redo → redo
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    File::create(&a).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = vec![a.clone()];
    app.file_names = vec![a.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.selection = vec![true];

    // Commit 1: a.txt → b.txt
    let b = dir.path().join("b.txt");
    std::fs::rename(&a, &b).unwrap();
    app.all_files[0] = b.clone();
    app.file_names[0] = b.to_string_lossy().to_string();
    app.cwd_commits_mut().push(Commit {
        timestamp: std::time::Instant::now(),
        label: "F2 1".into(),
        ops: vec![RenameOp {
            original_path: a.clone(),
            new_path: b.clone(),
            original_name: "a.txt".into(),
            new_name: "b.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        }],
    });

    // Commit 2: b.txt → c.txt
    let c = dir.path().join("c.txt");
    std::fs::rename(&b, &c).unwrap();
    app.all_files[0] = c.clone();
    app.file_names[0] = c.to_string_lossy().to_string();
    app.cwd_commits_mut().push(Commit {
        timestamp: std::time::Instant::now(),
        label: "F2 2".into(),
        ops: vec![RenameOp {
            original_path: b.clone(),
            new_path: c.clone(),
            original_name: "b.txt".into(),
            new_name: "c.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        }],
    });

    // Should find undo op for c.txt (most recent = Commit 2)
    assert!(app.find_undo_op(0).is_some(), "should find undo op for c");

    // Undo 1: c.txt → b.txt
    app.undo_file(0);
    assert!(b.exists(), "b.txt should exist after first undo");
    assert!(!c.exists(), "c.txt should not exist after first undo");
    assert_eq!(app.all_files[0], b, "all_files should be b.txt");

    // Should now find undo op for b.txt (Commit 1: a→b)
    let undo_op = app.find_undo_op(0);
    assert!(undo_op.is_some(), "should find undo op for b");

    // Undo 2: b.txt → a.txt
    app.undo_file(0);
    assert!(a.exists(), "a.txt should exist after second undo");
    assert!(!b.exists(), "b.txt should not exist after second undo");
    assert_eq!(app.all_files[0], a, "all_files should be a.txt");

    // Nothing more to undo
    assert!(app.find_undo_op(0).is_none(), "nothing more to undo");

    // Redo 1: a.txt → b.txt
    assert!(app.find_redo_op(0).is_some(), "should find redo op for a");
    app.redo_file(0);
    assert!(b.exists(), "b.txt should exist after first redo");
    assert!(!a.exists(), "a.txt should not exist after first redo");
    assert_eq!(app.all_files[0], b, "all_files should be b.txt");

    // Redo 2: b.txt → c.txt
    assert!(app.find_redo_op(0).is_some(), "should find redo op for b");
    app.redo_file(0);
    assert!(c.exists(), "c.txt should exist after second redo");
    assert!(!b.exists(), "b.txt should not exist after second redo");
    assert_eq!(app.all_files[0], c, "all_files should be c.txt");

    // Should be back at c
    assert!(
        app.find_undo_op(0).is_some(),
        "should find undo op back at c"
    );
    // The Revert commit provides a redo path even back at head (cycling).
    assert!(
        app.find_redo_op(0).is_some(),
        "redo available via Revert commit for cycling"
    );

    // Only one Revert commit was created (on first undo from head).
    let revert_count = app
        .cwd_commits()
        .iter()
        .filter(|c| c.label.starts_with("Revert"))
        .count();
    assert_eq!(revert_count, 1, "only one Revert commit for the chain");
}

#[test]
fn test_undo_redo_batch_rename() {
    // Batch rename multiple files, then undo/redo individual files
    let dir = TempDir::new().unwrap();

    let files = ["a.txt", "b.txt", "c.txt"];
    let paths: Vec<PathBuf> = files.iter().map(|f| dir.path().join(f)).collect();
    for p in &paths {
        File::create(p).unwrap();
    }

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = paths.clone();
    app.file_names = paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    app.file_sizes = vec![0; 3];
    app.selection = vec![true; 3];

    // Simulate a batch rename: a→x, b→y, c→z
    let x = dir.path().join("x.txt");
    let y = dir.path().join("y.txt");
    let z = dir.path().join("z.txt");
    std::fs::rename(&paths[0], &x).unwrap();
    std::fs::rename(&paths[1], &y).unwrap();
    std::fs::rename(&paths[2], &z).unwrap();

    app.all_files = vec![x.clone(), y.clone(), z.clone()];
    app.file_names = vec![
        x.to_string_lossy().to_string(),
        y.to_string_lossy().to_string(),
        z.to_string_lossy().to_string(),
    ];

    app.cwd_commits_mut().push(Commit {
        timestamp: std::time::Instant::now(),
        label: "Batch — test".into(),
        ops: vec![
            RenameOp {
                original_path: paths[0].clone(),
                new_path: x.clone(),
                original_name: "a.txt".into(),
                new_name: "x.txt".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0u8,
            },
            RenameOp {
                original_path: paths[1].clone(),
                new_path: y.clone(),
                original_name: "b.txt".into(),
                new_name: "y.txt".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0u8,
            },
            RenameOp {
                original_path: paths[2].clone(),
                new_path: z.clone(),
                original_name: "c.txt".into(),
                new_name: "z.txt".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0u8,
            },
        ],
    });

    // Undo first file: x.txt → a.txt
    assert!(app.find_undo_op(0).is_some(), "undo op for idx 0");
    app.undo_file(0);
    assert!(paths[0].exists(), "a.txt restored");
    assert!(!x.exists(), "x.txt gone");
    assert_eq!(app.all_files[0], paths[0]);

    // Undo second file: y.txt → b.txt
    assert!(app.find_undo_op(1).is_some(), "undo op for idx 1");
    app.undo_file(1);
    assert!(paths[1].exists(), "b.txt restored");
    assert!(!y.exists(), "y.txt gone");

    // Third file still at z.txt
    assert!(z.exists(), "z.txt still exists");
    assert!(app.find_undo_op(2).is_some(), "undo op for idx 2");

    // Redo first file: a.txt → x.txt
    assert!(app.find_redo_op(0).is_some(), "redo op for idx 0");
    app.redo_file(0);
    assert!(x.exists(), "x.txt redone");
    assert!(!paths[0].exists(), "a.txt gone after redo");

    // Redo second file: b.txt → y.txt
    assert!(app.find_redo_op(1).is_some(), "redo op for idx 1");
    app.redo_file(1);
    assert!(y.exists(), "y.txt redone");
    assert!(!paths[1].exists(), "b.txt gone after redo");

    // Can undo first file again
    assert!(app.find_undo_op(0).is_some(), "can undo again");
    app.undo_file(0);
    assert!(paths[0].exists(), "a.txt restored again");
    assert!(!x.exists(), "x.txt gone after second undo");
}

#[test]
fn test_undo_redo_skip_when_nothing_available() {
    // Files without any commit history should be skipped silently
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("no_history.txt"),
        PathBuf::from("also_no_history.txt"),
    ];
    app.file_names = vec!["no_history.txt".into(), "also_no_history.txt".into()];
    app.selection = vec![true, true];

    // No commits → nothing to undo or redo
    assert!(app.find_undo_op(0).is_none());
    assert!(app.find_undo_op(1).is_none());
    assert!(app.find_redo_op(0).is_none());
    assert!(app.find_redo_op(1).is_none());

    // undo_file/redo_file should not panic
    app.undo_file(0);
    app.redo_file(0);
}

#[test]
fn test_undo_redo_was_copy_mode() {
    // Copy mode: a.txt is preserved, b.txt is the copy.
    // Undo should delete the copy without touching the original.
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    File::create(&a).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = vec![a.clone()];
    app.file_names = vec![a.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.selection = vec![true];

    // Simulate copy: a.txt stays, b.txt is created as copy
    let b = dir.path().join("b.txt");
    std::fs::copy(&a, &b).unwrap();
    // In copy mode, BOTH a.txt and b.txt exist
    app.cwd_commits_mut().push(Commit {
        timestamp: std::time::Instant::now(),
        label: "Copy — test".into(),
        ops: vec![RenameOp {
            original_path: a.clone(),
            new_path: b.clone(),
            original_name: "a.txt".into(),
            new_name: "b.txt".into(),
            was_copy: true,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        }],
    });

    // After copy, all_files points to the copy: all_files[0] = b.txt
    app.all_files[0] = b.clone();
    app.file_names[0] = b.to_string_lossy().to_string();

    // Undo should delete the copy (b.txt) but keep the original (a.txt)
    assert!(
        app.find_undo_op(0).is_some(),
        "should find undo op for copy"
    );
    assert!(a.exists(), "original exists before undo");
    assert!(b.exists(), "copy exists before undo");

    app.undo_file(0);

    assert!(a.exists(), "original should still exist after undo");
    assert!(!b.exists(), "copy should be deleted after undo");
    assert_eq!(app.all_files[0], a, "all_files should point to original");
}

#[test]
fn test_undo_redo_selected_files_skips_non_applicable() {
    // Two files, one with history and one without.
    // Undo should only affect the one with history.
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    File::create(&a).unwrap();
    File::create(&b).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = vec![a.clone(), b.clone()];
    app.file_names = vec![
        a.to_string_lossy().to_string(),
        b.to_string_lossy().to_string(),
    ];
    app.file_sizes = vec![0, 0];
    app.selection = vec![true, true];

    // Rename a.txt → renamed.txt, b.txt stays as is
    let renamed = dir.path().join("renamed.txt");
    std::fs::rename(&a, &renamed).unwrap();
    app.all_files[0] = renamed.clone();
    app.file_names[0] = renamed.to_string_lossy().to_string();

    app.cwd_commits_mut().push(Commit {
        timestamp: std::time::Instant::now(),
        label: "Batch — test".into(),
        ops: vec![RenameOp {
            original_path: a.clone(),
            new_path: renamed.clone(),
            original_name: "a.txt".into(),
            new_name: "renamed.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        }],
    });

    // idx 0 (renamed.txt) has undo op; idx 1 (b.txt) does not
    assert!(app.find_undo_op(0).is_some());
    assert!(app.find_undo_op(1).is_none());

    // Undo both — only idx 0 should be affected
    app.undo_file(0);
    app.undo_file(1); // should be a no-op

    assert!(a.exists(), "a.txt restored");
    assert!(!renamed.exists(), "renamed.txt gone");
    assert!(b.exists(), "b.txt unchanged");
    assert_eq!(app.all_files[0], a);
    assert_eq!(app.all_files[1], b);
}
