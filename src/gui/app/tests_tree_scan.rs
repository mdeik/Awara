use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_rekey_tree_dir_rekeys_all_maps_including_descendants() {
    let mut app = GuiApp::new_with_config_for_test();
    let old = PathBuf::from("/a");
    let new = PathBuf::from("/b");

    // Cached expanded subtree: /a → /a/x → /a/x/1
    app.tree_children
        .insert(old.clone(), vec![old.join("x"), old.join("y")]);
    app.tree_children
        .insert(old.join("x"), vec![old.join("x").join("1")]);
    app.tree_scanned
        .extend([old.clone(), old.join("x"), old.join("x").join("1")]);
    app.tree_expanded.insert(old.clone());
    app.tree_expanded.insert(old.join("x"));
    app.tree_has_subdirs.insert(old.clone(), true);
    app.tree_has_subdirs.insert(old.join("x"), false);
    // Unrelated subtree must be untouched
    let other = PathBuf::from("/c");
    app.tree_children
        .insert(other.clone(), vec![other.join("z")]);
    app.tree_scanned.insert(other.clone());
    app.tree_expanded.insert(other.clone());
    app.tree_has_subdirs.insert(other.clone(), true);

    app.rekey_tree_dir(&old, &new);

    assert_eq!(
        app.tree_children.get(&new),
        Some(&vec![new.join("x"), new.join("y")]),
        "own children list re-keyed"
    );
    assert_eq!(
        app.tree_children.get(&new.join("x")),
        Some(&vec![new.join("x").join("1")]),
        "descendant children list re-keyed"
    );
    assert!(!app.tree_children.contains_key(&old.join("x")));

    assert!(app.tree_scanned.contains(&new));
    assert!(app.tree_scanned.contains(&new.join("x")));
    assert!(app.tree_scanned.contains(&new.join("x").join("1")));
    assert!(!app.tree_scanned.contains(&old.join("x")));

    assert!(app.tree_expanded.contains(&new));
    assert!(
        app.tree_expanded.contains(&new.join("x")),
        "descendant expanded state re-keyed"
    );
    assert!(!app.tree_expanded.contains(&old.join("x")));

    assert_eq!(app.tree_has_subdirs.get(&new), Some(&true));
    assert_eq!(app.tree_has_subdirs.get(&new.join("x")), Some(&false));
    assert!(!app.tree_has_subdirs.contains_key(&old.join("x")));

    // Unrelated subtree untouched
    assert_eq!(app.tree_children.get(&other), Some(&vec![other.join("z")]));
    assert!(app.tree_scanned.contains(&other));
    assert!(app.tree_expanded.contains(&other));
    assert_eq!(app.tree_has_subdirs.get(&other), Some(&true));
}

#[test]
fn test_rekey_tree_dir_noop_when_same_path() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = PathBuf::from("/a");
    app.tree_children.insert(dir.clone(), vec![dir.join("x")]);
    app.tree_scanned.insert(dir.clone());
    app.tree_expanded.insert(dir.clone());
    app.tree_has_subdirs.insert(dir.clone(), true);

    app.rekey_tree_dir(&dir, &dir);

    assert_eq!(app.tree_children.get(&dir), Some(&vec![dir.join("x")]));
    assert!(app.tree_scanned.contains(&dir));
    assert!(app.tree_expanded.contains(&dir));
    assert_eq!(app.tree_has_subdirs.get(&dir), Some(&true));
}

#[test]
fn test_execute_renames_rekeys_tree_has_subdirs() {
    // Probe answers follow the renamed directory through the real
    // execute_renames path.
    let tmp = TempDir::new().unwrap();
    let subdir = tmp.path().join("subdir");
    std::fs::create_dir(&subdir).unwrap();
    File::create(subdir.join("child.txt")).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![subdir.clone()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];

    app.tree_expanded.insert(subdir.clone());
    app.tree_scanned.insert(subdir.clone());
    app.tree_children.insert(subdir.clone(), vec![]);
    app.tree_has_subdirs.insert(subdir.clone(), false);

    app.cwd = tmp.path().to_path_buf();
    app.config.replace.replace = Some("subdir".into());
    app.config.replace.with = Some("newdir".into());
    app.section_enabled[SectionId::Replace as usize] = true;
    app.section_enabled[SectionId::MoveCopy as usize] = false;
    app.config.copy_to.copy_mode = false;

    app.preview_dirty = true;
    app.execute_renames();

    let new_subdir = tmp.path().join("newdir");
    assert!(new_subdir.is_dir());
    assert_eq!(
        app.tree_has_subdirs.get(&new_subdir),
        Some(&false),
        "probe answer re-keyed to new path"
    );
    assert!(!app.tree_has_subdirs.contains_key(&subdir));
}
#[test]
fn test_ensure_tree_scanned_adds_children() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = PathBuf::new();
    assert!(!app.tree_scanned.contains(&dir));
    assert!(!app.tree_pending.contains_key(&dir));
    app.ensure_tree_scanned(&dir);
    // Now async: marks as pending instead of blocking.
    // Data becomes available after the background thread processes it.
    assert!(
        app.tree_pending.contains_key(&dir),
        "dir should be in pending set"
    );
    // Second call should be a no-op (already pending)
    app.ensure_tree_scanned(&dir);
    assert!(
        app.tree_pending.contains_key(&dir),
        "dir should still be pending"
    );
}

#[test]
fn test_toggle_tree_expand() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = PathBuf::from("/tmp");
    assert!(!app.tree_expanded.contains(&dir));
    app.ensure_tree_scanned(&dir);
    app.toggle_tree_expand(&dir);
    assert!(app.tree_expanded.contains(&dir));
    app.toggle_tree_expand(&dir);
    assert!(!app.tree_expanded.contains(&dir));
}
#[test]
fn test_ensure_tree_scanned_skips_already_scanned() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = PathBuf::new();
    // Mark as already scanned
    app.tree_scanned.insert(dir.clone());
    app.tree_children.insert(dir.clone(), vec![]);

    app.ensure_tree_scanned(&dir);
    // Should NOT add to pending since already scanned
    assert!(!app.tree_pending.contains_key(&dir));
}

#[test]
fn test_ensure_tree_scanned_skips_already_pending() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = PathBuf::new();
    // Mark as already pending (in-flight request)
    app.tree_pending.insert(dir.clone(), 1);

    app.ensure_tree_scanned(&dir);
    // Should still be pending, not double-requested
    assert!(app.tree_pending.contains_key(&dir));
    // Should NOT be marked as scanned yet
    assert!(!app.tree_scanned.contains(&dir));
}

#[test]
fn test_check_scan_results_drains_tree_results() {
    let mut app = GuiApp::new_with_config_for_test();
    // Swap in a live tree result channel so we can inject results
    let (tree_result_tx, new_tree_scan_rx) = mpsc::channel();
    app.tree_scan_rx = new_tree_scan_rx;

    let dir = PathBuf::from("/tmp");
    let children = vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")];
    let generation = app.begin_tree_scan_for_test(&dir);

    // Simulate background thread completing
    tree_result_tx
        .send((dir.clone(), generation, children.clone()))
        .unwrap();

    app.check_scan_results();

    assert!(app.tree_scanned.contains(&dir));
    assert!(!app.tree_pending.contains_key(&dir));
    assert_eq!(app.tree_children.get(&dir), Some(&children));
}

#[test]
fn test_check_scan_results_drains_multiple_tree_results() {
    let mut app = GuiApp::new_with_config_for_test();
    let (tree_result_tx, new_tree_scan_rx) = mpsc::channel();
    app.tree_scan_rx = new_tree_scan_rx;

    let d1 = PathBuf::from("/a");
    let d2 = PathBuf::from("/b");
    let g1 = app.begin_tree_scan_for_test(&d1);
    let g2 = app.begin_tree_scan_for_test(&d2);

    tree_result_tx.send((d1.clone(), g1, vec![])).unwrap();
    tree_result_tx.send((d2.clone(), g2, vec![])).unwrap();

    app.check_scan_results();

    assert!(app.tree_scanned.contains(&d1));
    assert!(app.tree_scanned.contains(&d2));
    assert!(app.tree_pending.is_empty());
}

/// An expanded folder whose re-scan comes back with no subdirectories
/// must auto-collapse: the hollow folder (and its useless collapse
/// triangle) disappears, and caches are evicted so re-expanding re-reads
/// from disk.
#[test]
fn test_empty_tree_scan_auto_collapses_expanded_dir() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path().join("cwd");
    let sub = cwd.join("sub");
    let leaf = sub.join("leaf");
    std::fs::create_dir_all(&leaf).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = cwd.clone();
    // `sub` is expanded with a cached child and an in-flight re-scan.
    app.tree_expanded.insert(sub.clone());
    app.tree_children.insert(sub.clone(), vec![leaf.clone()]);
    app.tree_scanned.insert(sub.clone());
    let generation = app.begin_tree_scan_for_test(&sub);
    // Swap in a live tree result channel so we can inject the re-scan.
    let (tree_result_tx, new_tree_scan_rx) = mpsc::channel();
    app.tree_scan_rx = new_tree_scan_rx;

    // External change: sub loses its only subdirectory.
    std::fs::remove_dir_all(leaf).unwrap();

    // The re-scan completes with no subdirectories.
    tree_result_tx
        .send((sub.clone(), generation, Vec::<PathBuf>::new()))
        .unwrap();
    app.check_scan_results();

    assert!(
        !app.tree_expanded.contains(&sub),
        "expanded folder that became empty must auto-collapse"
    );
    assert_eq!(
        app.tree_has_subdirs.get(&sub),
        Some(&false),
        "empty child list recorded as the arrow answer so the arrow disappears"
    );
    assert!(
        !app.tree_children.contains_key(&sub) && !app.tree_scanned.contains(&sub),
        "cached children and scanned flags evicted so re-expanding re-reads from disk"
    );
}

#[test]
fn test_toggle_tree_expand_evicts_cache() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = PathBuf::from("/tmp");

    // Pre-populate as if previously expanded and scanned
    app.tree_children.insert(dir.clone(), vec![]);
    app.tree_scanned.insert(dir.clone());
    app.tree_expanded.insert(dir.clone());

    // Collapse should evict children and scanned flag
    app.toggle_tree_expand(&dir);
    assert!(!app.tree_expanded.contains(&dir));
    assert!(!app.tree_children.contains_key(&dir));
    assert!(!app.tree_scanned.contains(&dir));
}

#[test]
fn test_toggle_tree_expand_requests_async() {
    let mut app = GuiApp::new_with_config_for_test();
    // Swap in a live tree result channel
    let (tree_result_tx, new_tree_scan_rx) = mpsc::channel();
    app.tree_scan_rx = new_tree_scan_rx;

    let dir = PathBuf::from("/tmp");
    assert!(!app.tree_expanded.contains(&dir));

    // Expand — should mark as pending, not block
    app.toggle_tree_expand(&dir);
    assert!(app.tree_expanded.contains(&dir));
    assert!(app.tree_pending.contains_key(&dir));
    assert!(!app.tree_scanned.contains(&dir)); // not yet

    // Feed the result
    let children = vec![PathBuf::from("/tmp/sub")];
    let generation = app.pending_tree_scan_generation_for_test(&dir);
    tree_result_tx
        .send((dir.clone(), generation, children.clone()))
        .unwrap();
    app.check_scan_results();

    assert!(app.tree_scanned.contains(&dir));
    assert_eq!(app.tree_children.get(&dir), Some(&children));
    assert!(!app.tree_pending.contains_key(&dir));
}
