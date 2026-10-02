use super::tests_util::preview_row;
use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_apply_context_action_trash_with_skip_confirmation() {
    let tmp = TempDir::new().unwrap();
    let f1 = tmp.path().join("m1.txt");
    let f2 = tmp.path().join("m2.txt");
    File::create(&f1).unwrap();
    File::create(&f2).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![f1.clone(), f2.clone()];
    app.file_sizes = vec![10, 20];
    app.file_dates = vec![100, 200];
    app.selection = vec![true, true];
    app.skip_trash_confirmation = true;

    app.pending_context_action = Some(ContextAction::Trash);
    app.apply_context_action();

    assert_eq!(app.all_files.len(), 0);
    assert!(app.pending_context_action.is_none());
}
#[test]
fn test_trash_selected_removes_file() {
    let tmp = TempDir::new().unwrap();
    let f = tmp.path().join("a.txt");
    File::create(&f).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![f.clone(), tmp.path().join("b.txt")];
    app.file_sizes = vec![10, 20];
    app.file_dates = vec![100, 200];
    app.selection = vec![true, false];
    // Simulate existing preview / cached rows
    app.preview = Arc::new(vec![
        preview_row(
            awara::RenameOp {
                original_path: f.clone(),
                new_path: f.clone(),
                original_name: "a.txt".into(),
                new_name: "a_renamed.txt".into(),
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
                original_path: tmp.path().join("b.txt"),
                new_path: tmp.path().join("b.txt"),
                original_name: "b.txt".into(),
                new_name: "b.txt".into(),
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0u8,
            },
            false,
            1,
        ),
    ]);
    app.commits_by_dir
        .entry(app.cwd.clone())
        .or_default()
        .push(Commit {
            timestamp: std::time::Instant::now(),
            label: "test".into(),
            ops: vec![],
        });

    app.trash_selected().unwrap();

    assert_eq!(app.all_files.len(), 1, "one file should remain");
    assert!(
        app.commits_by_dir.values().all(|c| c.is_empty()),
        "undo_stack cleared after trash"
    );
    assert!(
        app.preview.iter().all(|pr| pr.index == 0),
        "surviving preview index adjusted"
    );
    assert_eq!(app.file_sizes, vec![20], "sizes trimmed");
    assert_eq!(app.file_dates, vec![200], "dates trimmed");
    assert_eq!(app.selection, vec![false], "selection trimmed");
    assert!(app.cached_rows.len() == 1, "cache rebuilt");
}

#[test]
fn test_trash_selected_removes_dir_and_descendants() {
    let tmp = TempDir::new().unwrap();
    let subdir = tmp.path().join("sub");
    std::fs::create_dir(&subdir).unwrap();
    let child = subdir.join("child.txt");
    File::create(&child).unwrap();
    let other = tmp.path().join("keep.txt");
    File::create(&other).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    // Order: subdir, child, other
    app.all_files = vec![subdir.clone(), child.clone(), other.clone()];
    app.file_sizes = vec![0, 5, 10];
    app.file_dates = vec![0, 0, 0];
    app.selection = vec![true, false, false];

    // Simulate tree cache
    app.tree_expanded.insert(subdir.clone());
    app.tree_scanned.insert(subdir.clone());
    app.tree_children
        .insert(subdir.clone(), vec![child.clone()]);
    app.tree_scanned.insert(child.clone());
    app.tree_scanned.insert(tmp.path().to_path_buf());
    app.tree_children.insert(
        tmp.path().to_path_buf(),
        vec![subdir.clone(), other.clone()],
    );

    app.preview_dirty = true;

    app.trash_selected().unwrap();

    // all_files should only have 'keep.txt' remaining
    assert_eq!(
        app.all_files,
        vec![other.clone()],
        "subdir and child removed"
    );
    assert_eq!(app.file_sizes, vec![10], "sizes correct");
    assert_eq!(app.selection, vec![false], "selection trimmed");

    // Tree cache: subdir and child removed
    assert!(!app.tree_children.contains_key(&subdir));
    assert!(!app.tree_scanned.contains(&subdir));
    assert!(!app.tree_expanded.contains(&subdir));
    assert!(!app.tree_scanned.contains(&child));

    // Parent's children list is pruned synchronously — the removed dir is gone
    // from the rendered layout before any viewbox scroll this frame, instead of
    // a frame or more later when the async re-scan lands (which would shift the
    // rows under the scroll target).
    assert!(
        !app.tree_children[tmp.path()].contains(&subdir),
        "removed dir dropped from the parent's child list immediately"
    );
    assert!(
        app.tree_children[tmp.path()].contains(&other),
        "surviving sibling kept"
    );
    // The parent is still invalidated for a re-scan (arrow + external changes).
    assert!(!app.tree_scanned.contains(tmp.path()));
}

#[test]
fn test_trash_path_removes_dir_and_descendants() {
    let tmp = TempDir::new().unwrap();
    let subdir = tmp.path().join("sub");
    std::fs::create_dir(&subdir).unwrap();
    let child = subdir.join("child.txt");
    File::create(&child).unwrap();
    let other = tmp.path().join("keep.txt");
    File::create(&other).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![subdir.clone(), child.clone(), other.clone()];
    app.file_sizes = vec![0, 5, 10];
    app.file_dates = vec![0, 0, 0];

    // Simulate tree cache for the subtree and its parent.
    app.tree_expanded.insert(subdir.clone());
    app.tree_scanned.insert(subdir.clone());
    app.tree_children
        .insert(subdir.clone(), vec![child.clone()]);
    app.tree_scanned.insert(child.clone());
    app.tree_scanned.insert(tmp.path().to_path_buf());
    app.tree_children.insert(
        tmp.path().to_path_buf(),
        vec![subdir.clone(), other.clone()],
    );

    app.trash_path(&subdir).unwrap();

    assert_eq!(app.all_files, vec![other], "subdir and child removed");
    assert_eq!(app.file_sizes, vec![10], "sizes trimmed");
    assert!(!subdir.exists(), "directory moved to trash");

    // Tree cache: subdir and its descendants evicted.
    assert!(!app.tree_children.contains_key(&subdir));
    assert!(!app.tree_scanned.contains(&subdir));
    assert!(!app.tree_expanded.contains(&subdir));
    assert!(!app.tree_scanned.contains(&child));
    // The removed row is pruned from the parent's child list synchronously,
    // before any viewbox scroll — not later, via the async re-scan.
    assert!(
        !app.tree_children[tmp.path()].contains(&subdir),
        "removed dir dropped from the parent's child list immediately"
    );
    // Parent still invalidated so it re-reads its children on the next frame.
    assert!(!app.tree_scanned.contains(tmp.path()));
}

/// Removing several directories in one batch must drop every removed row from
/// the parent's cached child list in the same call — not one frame later when
/// the async re-scan lands — so a viewbox scroll computed this frame never
/// targets a layout that still contains the removed rows (which is what
/// misaligned the tree when multiple directories were removed). The batched
/// path must also still recognise directories after the trash has moved them,
/// so their scanned descendants go too.
#[test]
fn test_trash_selected_multi_prunes_all_tree_rows_synchronously() {
    let tmp = TempDir::new().unwrap();
    let a = tmp.path().join("a");
    let a_child = a.join("child.txt");
    let b = tmp.path().join("b");
    let keep = tmp.path().join("keep");
    std::fs::create_dir(&a).unwrap();
    File::create(&a_child).unwrap();
    std::fs::create_dir(&b).unwrap();
    std::fs::create_dir(&keep).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![a.clone(), a_child.clone(), b.clone(), keep.clone()];
    app.file_sizes = vec![0, 0, 0, 0];
    app.file_dates = vec![0, 0, 0, 0];
    app.selection = vec![true, false, true, false];

    // The tree shows the cwd expanded with all three directories cached.
    app.tree_expanded.insert(tmp.path().to_path_buf());
    app.tree_scanned.insert(tmp.path().to_path_buf());
    app.tree_children.insert(
        tmp.path().to_path_buf(),
        vec![a.clone(), b.clone(), keep.clone()],
    );

    app.trash_selected_multi().unwrap();

    // The scanned descendant of a trashed directory is dropped from the listing;
    // this only works if the directory is still recognised after the trash.
    assert_eq!(
        app.all_files,
        vec![keep.clone()],
        "dirs and their descendants removed"
    );

    let children = app.tree_children.get(tmp.path()).unwrap();
    assert!(!children.contains(&a), "removed dir a pruned immediately");
    assert!(!children.contains(&b), "removed dir b pruned immediately");
    assert!(children.contains(&keep), "surviving dir kept");
    // The parent is still invalidated for the follow-up re-scan.
    assert!(!app.tree_scanned.contains(tmp.path()));
}

/// A tree scan that was already in flight when a directory was trashed must be
/// discarded when it lands. Its result was read from disk before the removal
/// and would otherwise re-insert the removed row — and mark the parent scanned,
/// so no fresh scan would run to correct it.
#[test]
fn test_stale_tree_scan_result_cannot_resurrect_trashed_dir() {
    let tmp = TempDir::new().unwrap();
    let gone = tmp.path().join("gone");
    let keep = tmp.path().join("keep");
    std::fs::create_dir(&gone).unwrap();
    std::fs::create_dir(&keep).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();

    // The parent is expanded with a cached child list and a re-scan in flight
    // (read from disk before the trash below).
    let parent = tmp.path().to_path_buf();
    app.tree_expanded.insert(parent.clone());
    app.tree_children
        .insert(parent.clone(), vec![gone.clone(), keep.clone()]);
    let stale_generation = app.begin_tree_scan_for_test(&parent);

    // Deliver the stale scan through an injected channel.
    let (tree_result_tx, new_tree_scan_rx) = mpsc::channel();
    app.tree_scan_rx = new_tree_scan_rx;

    // Trashing the dir prunes its row synchronously.
    app.trash_path(&gone).unwrap();
    assert!(!app.tree_children[&parent].contains(&gone));

    // The in-flight scan — which still listed `gone` — now lands.
    tree_result_tx
        .send((
            parent.clone(),
            stale_generation,
            vec![gone.clone(), keep.clone()],
        ))
        .unwrap();
    app.check_scan_results();

    assert!(
        !app.tree_children[&parent].contains(&gone),
        "a stale scan result must not resurrect the trashed dir"
    );
    assert!(app.tree_children[&parent].contains(&keep), "survivor kept");
    assert!(
        !app.tree_scanned.contains(&parent),
        "the discarded result must not mark the parent scanned"
    );
}

#[test]
fn test_trash_path_target_not_in_listing() {
    let tmp = TempDir::new().unwrap();
    let outside = tmp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    let keep = tmp.path().join("keep.txt");
    File::create(&keep).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![keep.clone()];
    app.file_sizes = vec![10];
    app.file_dates = vec![0];

    app.trash_path(&outside).unwrap();

    assert_eq!(app.all_files, vec![keep], "unrelated listing untouched");
    assert!(!outside.exists(), "directory moved to trash");
}

#[test]
fn test_trash_path_navigates_to_parent_when_cwd_inside() {
    let tmp = TempDir::new().unwrap();
    let subdir = tmp.path().join("sub");
    std::fs::create_dir(&subdir).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = subdir.clone();
    app.cwd_input = subdir.to_string_lossy().to_string();

    app.trash_path(&subdir).unwrap();

    assert_eq!(
        app.cwd,
        tmp.path().to_path_buf(),
        "cwd falls back to the trashed folder's parent"
    );
    assert!(app.scanning, "fallback navigation re-scans the parent");
}

#[test]
fn test_trash_path_refuses_filesystem_root() {
    let mut app = GuiApp::new_with_config_for_test();
    let err = app
        .trash_path(Path::new("/"))
        .expect_err("trashing a filesystem root must be refused");
    assert!(err.contains("filesystem root"), "unexpected error: {err}");
}

/// Selecting a file that has already been deleted outside the GUI and then
/// choosing "Move to Trash" must succeed (no error) and remove the entry from
/// the list, just as if the trash had actually been called.
///
/// This covers the `if path.exists()` guard in `trash_selected_multi`: without
/// it the trash crate would return an error because the path doesn't exist.
#[test]
fn test_trash_selected_silently_removes_already_missing_file() {
    let tmp = TempDir::new().unwrap();
    let keep = tmp.path().join("keep.txt");
    File::create(&keep).unwrap();
    // `gone` is created then immediately removed to simulate an external delete.
    let gone = tmp.path().join("gone.txt");
    File::create(&gone).unwrap();
    std::fs::remove_file(&gone).unwrap();
    assert!(!gone.exists(), "pre-condition: file must not exist");

    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![gone.clone(), keep.clone()];
    app.file_sizes = vec![0, 10];
    app.file_dates = vec![0, 100];
    app.selection = vec![true, false]; // select only the already-missing file

    let result = app.trash_selected_multi();
    assert!(
        result.is_ok(),
        "trash of a missing file must not return an error: {:?}",
        result
    );
    assert_eq!(
        app.all_files,
        vec![keep],
        "missing file entry must be removed from the list"
    );
    assert_eq!(app.selection, vec![false], "selection trimmed to one entry");
    assert_eq!(app.file_sizes, vec![10], "sizes trimmed");
    assert_eq!(app.file_dates, vec![100], "dates trimmed");
}
