use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_rekey_dir_prefix_updates_status_map() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("mydir");
    std::fs::create_dir(&dir).unwrap();
    let child = dir.join("file.txt");
    File::create(&child).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    // Manually seed status_map with an entry under the old dir
    let child_str = child.to_string_lossy().to_string();
    app.status_map.insert(child_str.clone(), (0u8, None));

    let new_dir = tmp.path().join("mydir_renamed");
    app.rekey_dir_prefix(&dir, &new_dir);

    // Old key must be gone
    assert!(
        !app.status_map.contains_key(&child_str),
        "old status_map key should be removed after rekey"
    );
    // New key must be present with the same value
    let new_child_str = new_dir.join("file.txt").to_string_lossy().to_string();
    let val = app.status_map.get(&new_child_str);
    assert!(val.is_some(), "re-keyed status_map entry should exist");
    assert_eq!(val.unwrap().0, 0u8);
}

/// rekey_dir_prefix must update all keys in revert_status_map when a parent
/// directory is renamed so revert dialog status column stays correct.
#[test]
fn test_rekey_dir_prefix_updates_revert_status_map() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("parent");
    std::fs::create_dir(&dir).unwrap();
    let child = dir.join("doc.txt");
    File::create(&child).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    // Seed revert_status_map with an error entry under the old dir
    let child_str = child.to_string_lossy().to_string();
    app.revert_status_map
        .insert(child_str.clone(), (1u8, Some("disk full".to_string())));

    let new_dir = tmp.path().join("parent_v2");
    app.rekey_dir_prefix(&dir, &new_dir);

    // Old key must be gone
    assert!(
        !app.revert_status_map.contains_key(&child_str),
        "old revert_status_map key should be removed after rekey"
    );
    // New key must be present with intact error info
    let new_child_str = new_dir.join("doc.txt").to_string_lossy().to_string();
    let val = app.revert_status_map.get(&new_child_str);
    assert!(
        val.is_some(),
        "re-keyed revert_status_map entry should exist"
    );
    assert_eq!(val.unwrap().0, 1u8);
    assert_eq!(val.unwrap().1.as_deref(), Some("disk full"));
}

/// rekey_dir_prefix must comprehensively update both main table structures
/// (all_files, file_names, status_map) and revert structures (commits_by_dir,
/// redo_stack, revert_status_map, active revert_dialog entries).
#[test]
fn test_rekey_dir_prefix_updates_both_main_and_revert_tables() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("source_dir");
    std::fs::create_dir(&dir).unwrap();
    let child = dir.join("item.txt");
    File::create(&child).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();

    // 1. Setup Main table state
    app.all_files = vec![child.clone()];
    app.file_names = vec![child.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.file_is_dir = vec![false];
    app.selection = vec![true];
    app.status_map
        .insert(child.to_string_lossy().to_string(), (0u8, None));

    // 2. Setup Revert table & history state
    let op = RenameOp {
        original_path: dir.join("old_item.txt"),
        new_path: child.clone(),
        original_name: "old_item.txt".into(),
        new_name: "item.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    app.commits_by_dir.insert(
        dir.clone(),
        vec![Commit {
            timestamp: std::time::Instant::now(),
            label: "Rename 1".into(),
            ops: vec![op],
        }],
    );
    app.revert_status_map
        .insert(child.to_string_lossy().to_string(), (0u8, None));

    // 3. Setup active revert_dialog
    app.build_revert_dialog_state();
    assert!(app.revert_dialog.is_some());

    // 4. Perform directory rekey
    let new_dir = tmp.path().join("target_dir");
    app.rekey_dir_prefix(&dir, &new_dir);

    let new_child = new_dir.join("item.txt");
    let new_child_str = new_child.to_string_lossy().to_string();
    let old_child_str = child.to_string_lossy().to_string();

    // Verify Main Table
    assert_eq!(
        app.all_files[0], new_child,
        "all_files path must be re-keyed"
    );
    assert_eq!(
        app.file_names[0], new_child_str,
        "file_names cache must be re-keyed"
    );
    assert!(
        !app.status_map.contains_key(&old_child_str),
        "old path in status_map removed"
    );
    assert!(
        app.status_map.contains_key(&new_child_str),
        "new path in status_map present"
    );

    // Verify Revert History
    assert!(
        !app.commits_by_dir.contains_key(&dir),
        "old dir key in commits_by_dir removed"
    );
    assert!(
        app.commits_by_dir.contains_key(&new_dir),
        "new dir key in commits_by_dir present"
    );
    let commit_ops = &app.commits_by_dir.get(&new_dir).unwrap()[0].ops;
    assert_eq!(
        commit_ops[0].new_path, new_child,
        "RenameOp new_path re-keyed"
    );
    assert_eq!(
        commit_ops[0].original_path,
        new_dir.join("old_item.txt"),
        "RenameOp original_path re-keyed"
    );

    // Verify Revert Status Map
    assert!(
        !app.revert_status_map.contains_key(&old_child_str),
        "old path in revert_status_map removed"
    );
    assert!(
        app.revert_status_map.contains_key(&new_child_str),
        "new path in revert_status_map present"
    );

    // Verify Active Revert Dialog entries
    let dialog = app.revert_dialog.as_ref().unwrap();
    assert_eq!(
        dialog.entries[0].current_path, new_child,
        "dialog entry current_path re-keyed"
    );
    assert_eq!(
        dialog.entries[0].lineage[0].0,
        new_dir.join("old_item.txt"),
        "dialog entry lineage re-keyed"
    );
}
#[test]
fn test_rekey_dir_updates_all_consumers() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("src");
    std::fs::create_dir(&dir).unwrap();
    let child = dir.join("item.txt");
    File::create(&child).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();

    // Main table + status
    app.all_files = vec![child.clone()];
    app.file_names = vec![child.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];
    app.status_map
        .insert(child.to_string_lossy().to_string(), (0u8, None));

    // History + revert table
    app.commits_by_dir.insert(
        dir.clone(),
        vec![Commit {
            timestamp: std::time::Instant::now(),
            label: "Rename 1".into(),
            ops: vec![RenameOp {
                original_path: dir.join("old.txt"),
                new_path: child.clone(),
                original_name: "old.txt".into(),
                new_name: "item.txt".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0,
            }],
        }],
    );
    app.revert_status_map
        .insert(child.to_string_lossy().to_string(), (0u8, None));

    // File tree caches
    app.tree_children.insert(dir.clone(), vec![child.clone()]);
    app.tree_scanned.insert(dir.clone());
    app.tree_expanded.insert(dir.clone());
    app.tree_has_subdirs.insert(dir.clone(), true);

    let new_dir = tmp.path().join("dst");
    app.rekey_dir(&dir, &new_dir);

    let new_child = new_dir.join("item.txt");
    let old_child_str = child.to_string_lossy().to_string();
    let new_child_str = new_child.to_string_lossy().to_string();

    // File list / status maps (rekey_dir_prefix side)
    assert_eq!(app.all_files[0], new_child, "all_files re-keyed");
    assert_eq!(app.file_names[0], new_child_str, "file_names re-keyed");
    assert!(!app.status_map.contains_key(&old_child_str));
    assert!(app.status_map.contains_key(&new_child_str));
    assert!(app.commits_by_dir.contains_key(&new_dir));
    assert!(!app.commits_by_dir.contains_key(&dir));
    assert!(
        app.revert_status_map.contains_key(&new_child_str),
        "revert_status_map re-keyed"
    );

    // File tree (rekey_tree_dir side)
    assert_eq!(
        app.tree_children.get(&new_dir),
        Some(&vec![new_child]),
        "tree children re-keyed"
    );
    assert!(app.tree_scanned.contains(&new_dir));
    assert!(app.tree_expanded.contains(&new_dir));
    assert_eq!(app.tree_has_subdirs.get(&new_dir), Some(&true));
    assert!(!app.tree_children.contains_key(&dir));
    assert!(!app.tree_scanned.contains(&dir));
    assert!(!app.tree_expanded.contains(&dir));
    assert!(!app.tree_has_subdirs.contains_key(&dir));
}

#[test]
fn test_rekey_dir_prefix_updates_redo_stack() {
    // rekey_dir_prefix step 2: redo_stack ops under the renamed directory
    // must follow it to the new path.
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("parent");
    std::fs::create_dir(&dir).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.redo_stack.push(Commit {
        timestamp: std::time::Instant::now(),
        label: "Redo".into(),
        ops: vec![RenameOp {
            original_path: dir.join("a.txt"),
            new_path: dir.join("b.txt"),
            original_name: "a.txt".into(),
            new_name: "b.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0,
        }],
    });

    let new_dir = tmp.path().join("renamed");
    app.rekey_dir_prefix(&dir, &new_dir);

    let redo = &app.redo_stack[0];
    assert_eq!(redo.ops[0].original_path, new_dir.join("a.txt"));
    assert_eq!(redo.ops[0].new_path, new_dir.join("b.txt"));
    assert!(
        redo.ops
            .iter()
            .all(|o| !o.original_path.starts_with(&dir) && !o.new_path.starts_with(&dir)),
        "no stale paths under the old dir"
    );
}
#[test]
fn test_commit_history_capping_fifo() {
    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = PathBuf::from("/test/dir");

    // Push 60 commits (exceeding MAX_COMMITS_PER_DIR = 50)
    for i in 0..60 {
        app.push_cwd_commit(Commit {
            timestamp: std::time::Instant::now(),
            label: format!("Commit {}", i),
            ops: vec![],
        });
    }

    let commits = app.cwd_commits();
    assert_eq!(commits.len(), MAX_COMMITS_PER_DIR);
    // The oldest 10 commits (0..10) should have been pruned FIFO; first should be "Commit 10"
    assert_eq!(commits[0].label, "Commit 10");
    assert_eq!(commits.last().unwrap().label, "Commit 59");
}

#[test]
fn test_commit_history_dir_lru_eviction() {
    let mut app = GuiApp::new_with_config_for_test();

    // Populate 25 distinct directories with commits (exceeding MAX_COMMITS_DIRS = 20)
    for i in 0..25 {
        let dir = PathBuf::from(format!("/test/dir_{}", i));
        app.cwd = dir.clone();
        app.push_cwd_commit(Commit {
            timestamp: std::time::Instant::now(),
            label: format!("Dir {} commit", i),
            ops: vec![],
        });
    }

    // commits_by_dir should be capped at MAX_COMMITS_DIRS (20)
    assert_eq!(app.commits_by_dir.len(), MAX_COMMITS_DIRS);

    // Oldest 5 directories (/test/dir_0 through /test/dir_4) should be evicted
    for i in 0..5 {
        let old_dir = PathBuf::from(format!("/test/dir_{}", i));
        assert!(
            !app.commits_by_dir.contains_key(&old_dir),
            "Directory {:?} should have been evicted by LRU",
            old_dir
        );
    }

    // Most recent directories (/test/dir_5 through /test/dir_24) should still be present
    for i in 5..25 {
        let dir = PathBuf::from(format!("/test/dir_{}", i));
        assert!(
            app.commits_by_dir.contains_key(&dir),
            "Directory {:?} should be retained in commits_by_dir",
            dir
        );
    }
}
#[test]
fn test_commit_dir_lru_preservation_across_rekey() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir_a = PathBuf::from("/test/dir_a");
    let dir_b = PathBuf::from("/test/dir_b");
    let dir_a_renamed = PathBuf::from("/test/dir_a_renamed");

    // Push commits into dir_a and dir_b
    app.cwd = dir_a.clone();
    app.push_cwd_commit(Commit {
        timestamp: std::time::Instant::now(),
        label: "Commit A".to_string(),
        ops: vec![],
    });

    app.cwd = dir_b.clone();
    app.push_cwd_commit(Commit {
        timestamp: std::time::Instant::now(),
        label: "Commit B".to_string(),
        ops: vec![],
    });

    assert_eq!(app.commit_dir_lru, vec![dir_a.clone(), dir_b.clone()]);

    // Rekey dir_a to dir_a_renamed
    app.rekey_dir_prefix(&dir_a, &dir_a_renamed);

    // dir_a should be replaced by dir_a_renamed with no duplicate entries or lingering dir_a
    assert!(!app.commit_dir_lru.contains(&dir_a));
    assert!(app.commit_dir_lru.contains(&dir_a_renamed));
    assert_eq!(
        app.commit_dir_lru,
        vec![dir_a_renamed.clone(), dir_b.clone()]
    );

    // Access/push commit to dir_a_renamed -> moves to most-recent position (back)
    app.cwd = dir_a_renamed.clone();
    app.push_cwd_commit(Commit {
        timestamp: std::time::Instant::now(),
        label: "Commit A2".to_string(),
        ops: vec![],
    });

    assert_eq!(
        app.commit_dir_lru,
        vec![dir_b.clone(), dir_a_renamed.clone()]
    );
    assert_eq!(app.commits_by_dir.len(), 2);
}
