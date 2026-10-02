use super::*;
use tempfile::TempDir;

#[test]
fn test_probe_tree_arrow_caches_answer() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let files_only = TempDir::new().unwrap();
    std::fs::write(files_only.path().join("a.txt"), b"").unwrap();

    // First call probes synchronously and returns + caches the answer
    let has = app.probe_tree_arrow(dir.path());
    assert!(has, "folder with a subdir probes true");
    assert_eq!(app.tree_has_subdirs.get(dir.path()), Some(&true));

    let has = app.probe_tree_arrow(files_only.path());
    assert!(!has, "file-only folder probes false");
    assert_eq!(app.tree_has_subdirs.get(files_only.path()), Some(&false));
}

/// Once the preview scan observes the current directory's contents, stale
/// expand-arrow answers for dirs under the cwd must be dropped (and
/// re-probed flash-free on the next frame), so a dir whose subdirectories
/// were removed loses its arrow — and dirs outside the cwd keep their
/// cached answers.
#[test]
fn test_scan_completion_reprobes_arrows_under_cwd() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path().join("cwd");
    let d = cwd.join("d");
    let sub = d.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let outside = tmp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = cwd.clone();
    // Stale cached arrow answers: d had subdirs (now removed externally),
    // and an unrelated dir outside the cwd also has a cached answer.
    app.tree_has_subdirs.insert(d.clone(), true);
    app.tree_has_subdirs.insert(outside.clone(), true);

    // The external removal happened before the scan.
    std::fs::remove_dir_all(&sub).unwrap();

    // Complete a scan of the cwd.
    app.scanning = true;
    app.scan_result_tx
        .send(ScanResult {
            files: vec![d.clone()],
            sizes: vec![0],
            dates: vec![0],
            is_dirs: vec![true],
            r#gen: app.scan_generation,
            is_partial: false,
        })
        .unwrap();
    app.check_scan_results();

    // The stale answer under the cwd was dropped; the outside one kept.
    assert!(
        !app.tree_has_subdirs.contains_key(&d),
        "stale arrow answer under cwd dropped"
    );
    assert_eq!(
        app.tree_has_subdirs.get(&outside),
        Some(&true),
        "arrow answer outside the cwd kept"
    );
    // Re-probing now reports no subdirs, so the arrow hides.
    assert!(!app.probe_tree_arrow(&d));
}

/// An externally renamed dir must behave like remove + add: the tree's
/// children list refresh drops the old row and shows the new name, and the
/// scan-completion arrow refresh drops the old path's arrow answer while
/// the new path probes fresh (its subdirectories moved with it).
#[test]
fn test_external_dir_rename_refreshes_tree_rows_and_arrows() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path().join("cwd");
    let b = cwd.join("b");
    let c = cwd.join("c");
    std::fs::create_dir_all(b.join("sub")).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = cwd.clone();
    app.cwd_input = cwd.to_string_lossy().to_string();
    // Simulate the tree having seen /cwd/b expanded with a cached arrow.
    app.tree_children.insert(cwd.clone(), vec![b.clone()]);
    app.tree_scanned.insert(cwd.clone());
    app.tree_expanded.insert(cwd.clone());
    app.tree_has_subdirs.insert(b.clone(), true);
    // Swap in a live tree result channel so we can inject the re-scan.
    let (tree_result_tx, new_tree_scan_rx) = mpsc::channel();
    app.tree_scan_rx = new_tree_scan_rx;

    // External rename b → c.
    std::fs::rename(&b, &c).unwrap();

    // What scan_dir's expanded-node invalidation + the next frame's
    // ensure_tree_scanned would do: re-read the cwd's children.
    app.invalidate_tree_scan(&cwd);
    app.ensure_tree_scanned(&cwd);
    let generation = app.pending_tree_scan_generation_for_test(&cwd);
    tree_result_tx
        .send((cwd.clone(), generation, vec![c.clone()]))
        .unwrap();
    app.check_scan_results();
    assert_eq!(
        app.tree_children.get(&cwd),
        Some(&vec![c.clone()]),
        "row refreshed to the new name"
    );
    assert!(app.tree_scanned.contains(&cwd));

    // The preview scan completes → stale arrow answer under the old path
    // is dropped, and the renamed dir probes fresh under the new path.
    app.scanning = true;
    app.scan_result_tx
        .send(ScanResult {
            files: vec![c.clone()],
            sizes: vec![0],
            dates: vec![0],
            is_dirs: vec![true],
            r#gen: app.scan_generation,
            is_partial: false,
        })
        .unwrap();
    app.check_scan_results();
    assert!(
        !app.tree_has_subdirs.contains_key(&b),
        "old path arrow answer dropped"
    );
    assert!(
        app.probe_tree_arrow(&c),
        "renamed dir keeps its arrow (subdirs moved with it)"
    );
}

#[test]
fn test_scan_dir_preserves_probe_cache_unless_hidden_changes() {
    // A real cwd is required so scan_dir() reaches the probe-invalidation
    // logic (a missing cwd short-circuits before it).
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("leaf");
    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.cwd_input = tmp.path().to_string_lossy().to_string();
    app.config.filters.filter_hidden = false;
    app.tree_probe_hidden = Some(false);
    app.tree_has_subdirs.insert(dir.clone(), false);

    // Navigation with the same hidden filter keeps the probe answers — no
    // re-probe flash on every click.
    app.scan_dir();
    assert_eq!(
        app.tree_has_subdirs.get(&dir),
        Some(&false),
        "probe cache survives navigation"
    );

    // Toggling the hidden filter invalidates them so arrows are re-decided
    app.config.filters.filter_hidden = true;
    app.scan_dir();
    assert!(
        app.tree_has_subdirs.is_empty(),
        "probe cache cleared when hidden filter changes"
    );
    assert_eq!(app.tree_probe_hidden, Some(true));
}

/// F5's subtree invalidation must drop expand-arrow answers for collapsed
/// descendants too — not just `dir` itself. A collapsed child whose
/// subdirectories were removed externally would otherwise keep its stale
/// arrow until manually expanded (or until the cwd file scan happens to
/// finish).
#[test]
fn test_invalidate_tree_subtree_drops_stale_arrow_for_collapsed_child() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path().join("cwd");
    let sub = cwd.join("sub");
    std::fs::create_dir_all(sub.join("gone")).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = cwd.clone();
    // `sub` is collapsed and its cached probe says it has subdirectories.
    app.tree_has_subdirs.insert(sub.clone(), true);
    app.tree_expanded.insert(cwd.clone());
    app.tree_children.insert(cwd.clone(), vec![sub.clone()]);
    app.tree_scanned.insert(cwd.clone());
    // Swap in a live tree result channel so we can inject the re-scan.
    let (tree_result_tx, new_tree_scan_rx) = mpsc::channel();
    app.tree_scan_rx = new_tree_scan_rx;

    // External change: sub loses its subdirectories.
    std::fs::remove_dir_all(sub.join("gone")).unwrap();

    // F5: invalidate the cwd subtree, then re-read its children.
    app.invalidate_tree_subtree(&cwd);
    app.ensure_tree_scanned(&cwd);
    let generation = app.pending_tree_scan_generation_for_test(&cwd);
    tree_result_tx
        .send((cwd.clone(), generation, vec![sub.clone()]))
        .unwrap();
    app.check_scan_results();

    assert_eq!(
        app.tree_children.get(&cwd),
        Some(&vec![sub.clone()]),
        "sub still listed under cwd after the re-scan"
    );
    assert!(
        !app.tree_has_subdirs.contains_key(&sub),
        "subtree refresh must drop stale arrow answers for collapsed children"
    );
}

#[test]
fn test_refresh_requests_scroll_to_cwd() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path().join("cwd");
    std::fs::create_dir_all(&cwd).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = cwd.clone();
    app.tree_scroll_target = None;

    app.refresh();

    assert_eq!(
        app.tree_scroll_target.as_deref(),
        Some(cwd.as_path()),
        "refresh must request the tree to scroll back to the cwd node"
    );
}

#[test]
fn test_refresh_drops_stale_arrow_answers() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path().join("cwd");
    let sub = cwd.join("sub");
    std::fs::create_dir_all(sub.join("gone")).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = cwd.clone();
    // `sub` is collapsed and its cached probe says it has subdirectories.
    app.tree_has_subdirs.insert(sub.clone(), true);
    app.tree_expanded.insert(cwd.clone());
    app.tree_children.insert(cwd.clone(), vec![sub.clone()]);
    app.tree_scanned.insert(cwd.clone());

    // External change: sub loses its subdirectories.
    std::fs::remove_dir_all(sub.join("gone")).unwrap();

    app.refresh();

    assert!(
        app.tree_has_subdirs.is_empty(),
        "full refresh must clear the probe cache so arrows re-derive from disk"
    );
}

#[test]
fn test_refresh_missing_cwd_scrolls_to_nearest_existing_parent() {
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().join("parent");
    let missing = parent.join("gone");
    std::fs::create_dir_all(&parent).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = missing.clone();
    app.tree_scroll_target = None;

    app.refresh();

    assert_eq!(
        app.tree_scroll_target.as_deref(),
        Some(parent.as_path()),
        "missing cwd: viewbox should target the nearest existing parent"
    );
    assert_eq!(
        app.cwd, missing,
        "scroll target must not change the cwd — viewbox only"
    );
    assert!(
        app.tree_expanded.contains(tmp.path()),
        "ancestors of the scroll target must be expanded so its node renders"
    );
}

#[test]
fn test_toggle_tree_expand_collapse_records_has_subdirs() {
    let mut app = GuiApp::new_with_config_for_test();

    // Expanded dir with subdirectories → collapse records `true` so the
    // arrow stays visible without re-probing
    let dir = PathBuf::from("/tmp");
    app.tree_children
        .insert(dir.clone(), vec![PathBuf::from("/tmp/a")]);
    app.tree_scanned.insert(dir.clone());
    app.tree_expanded.insert(dir.clone());
    app.toggle_tree_expand(&dir);
    assert!(!app.tree_expanded.contains(&dir));
    assert!(!app.tree_children.contains_key(&dir));
    assert_eq!(app.tree_has_subdirs.get(&dir), Some(&true));

    // Expanded dir with no subdirectories → collapse records `false` so
    // the arrow disappears
    let empty = PathBuf::from("/tmp/empty");
    app.tree_children.insert(empty.clone(), vec![]);
    app.tree_scanned.insert(empty.clone());
    app.tree_expanded.insert(empty.clone());
    app.toggle_tree_expand(&empty);
    assert_eq!(app.tree_has_subdirs.get(&empty), Some(&false));
}

#[test]
fn test_dir_has_subdirs() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("file.txt"), b"").unwrap();
    assert!(dir_has_subdirs(dir.path(), true));
    assert!(dir_has_subdirs(dir.path(), false));

    // A folder whose only entries are files has no subdirectories
    let files_only = TempDir::new().unwrap();
    std::fs::write(files_only.path().join("a.txt"), b"").unwrap();
    std::fs::write(files_only.path().join("b.txt"), b"").unwrap();
    assert!(!dir_has_subdirs(files_only.path(), true));
    assert!(!dir_has_subdirs(files_only.path(), false));

    #[cfg(unix)]
    {
        // A hidden subfolder counts only when show_hidden is set
        let only_hidden = TempDir::new().unwrap();
        std::fs::create_dir(only_hidden.path().join(".hidden")).unwrap();
        assert!(dir_has_subdirs(only_hidden.path(), true));
        assert!(!dir_has_subdirs(only_hidden.path(), false));

        // A symlink to a directory is skipped (mirrors scan_tree_children)
        let link = files_only.path().join("link");
        std::os::unix::fs::symlink(dir.path(), &link).unwrap();
        assert!(!dir_has_subdirs(files_only.path(), true));
    }
}
#[test]
#[cfg(unix)]
fn test_scan_tree_children_empty_root_unix_returns_root() {
    let children = scan_tree_children(&PathBuf::new(), false);
    assert_eq!(children.len(), 1);
    assert_eq!(children[0], Path::new("/"));
}

#[test]
#[cfg(windows)]
fn test_scan_tree_children_empty_root_windows_has_c_drive() {
    let children = scan_tree_children(&PathBuf::new(), false);
    assert!(!children.is_empty(), "should find at least one drive");
    assert!(
        children.iter().any(|p| p.to_string_lossy() == r"C:\"),
        "should include C: drive"
    );
}

/// Tree children are sorted with the same natural comparison as the main
/// file table: numeric runs compare numerically, so `b2` precedes `b10`.
#[test]
fn test_scan_tree_children_sorts_naturally() {
    let tmp = TempDir::new().unwrap();
    for name in ["b10", "b2", "b1", "b3"] {
        std::fs::create_dir(tmp.path().join(name)).unwrap();
    }
    let children = scan_tree_children(tmp.path(), true);
    let names: Vec<String> = children
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["b1", "b2", "b3", "b10"]);
}

/// The tree matches Windows Explorer's order: punctuation-leading names
/// ("(ABC)…", "[…]") come before plain-letter names ("Example.Show…"),
/// and numeric runs compare numerically ("S01" before "S02").
#[test]
fn test_scan_tree_children_matches_os_order() {
    let tmp = TempDir::new().unwrap();
    for name in [
        "Example.Show.S01.1080p",
        "[Alpha Group] Sample Release",
        "(ABC) Sample Show",
        "Example.Show.S02.1080p",
        "[Zeta Fansub]_Sample_Movie_DVDrip",
    ] {
        std::fs::create_dir(tmp.path().join(name)).unwrap();
    }
    let children = scan_tree_children(tmp.path(), true);
    let names: Vec<String> = children
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        names,
        vec![
            "(ABC) Sample Show",
            "[Alpha Group] Sample Release",
            "[Zeta Fansub]_Sample_Movie_DVDrip",
            "Example.Show.S01.1080p",
            "Example.Show.S02.1080p",
        ]
    );
}

/// The tree navigator must degrade gracefully when a directory can't be
/// read: the scan yields no children (so the node resolves instead of
/// showing a stuck "…") and the arrow probe reports no subdirectories, so
/// the expand arrow stays hidden. The subdirectory makes a successful read
/// yield a non-empty result, so a bypassed permission check fails loudly
/// (rather than passing vacuously). When tests run as root — common on CI —
/// mode bits are bypassed entirely; the test then re-runs itself as the
/// unprivileged "nobody" user so the degradation path is still exercised.
#[test]
#[cfg(unix)]
fn test_tree_scan_and_probe_unreadable_dir() {
    let dir = TempDir::new().unwrap();
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::create_dir(locked.join("sub")).unwrap();
    std::fs::write(locked.join("file.txt"), b"data").unwrap();
    // Deny read+execute so read_dir fails for non-root users.
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    // Still readable? Mode bits are being bypassed: root (the CI case)
    // ignores them, and a few filesystems don't implement them at all.
    // Restore permissions, then re-run just this test as "nobody"
    // (uid 65534, the standard unprivileged account) so the assertions
    // below run for real; if dropping privileges isn't possible, skip.
    if std::fs::read_dir(&locked).is_ok() {
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        use std::os::unix::process::CommandExt;
        let rerun = if std::env::var_os("AWARA_TEST_AS_NOBODY").is_none() {
            std::env::current_exe().ok().map(|exe| {
                std::process::Command::new(exe)
                    // Must match this test's full path so the child runs
                    // only this test (and only once — the env var above
                    // stops the child from re-spawning itself).
                    .arg("gui::app::tests_tree_probe::test_tree_scan_and_probe_unreadable_dir")
                    .env("AWARA_TEST_AS_NOBODY", "1")
                    // tempfile honors TMPDIR; /tmp is world-writable, so
                    // the nobody child can always create its temp dirs.
                    .env("TMPDIR", "/tmp")
                    .uid(65534)
                    .gid(65534)
                    .status()
            })
        } else {
            None
        };
        match rerun {
            Some(Ok(status)) => {
                assert!(status.success(), "re-run as nobody failed: {status}")
            }
            _ => eprintln!(
                "skipping: locked dir readable despite 0o000 (root or \
                     mode-ignoring fs) and cannot re-run as nobody"
            ),
        }
        return;
    }

    // Scan degrades to no children; the probe reports no subdirectories.
    assert!(
        scan_tree_children(&locked, true).is_empty(),
        "scan degrades to no children"
    );
    assert!(!dir_has_subdirs(&locked, true), "probe reports no subdirs");

    // The GUI probe caches the negative answer, so the arrow stays hidden.
    let mut app = GuiApp::new_with_config_for_test();
    assert!(!app.probe_tree_arrow(&locked));
    assert_eq!(app.tree_has_subdirs.get(&locked), Some(&false));

    // Restore permissions so TempDir can clean up.
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
}
#[test]
fn test_commit_edit_and_undo_rekey_tree_on_dir_rename() {
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().to_path_buf();
    let subdir = parent.join("subdir");
    std::fs::create_dir(&subdir).unwrap();
    let nested = subdir.join("nested");
    std::fs::create_dir(&nested).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = parent.clone();
    app.all_files = vec![subdir.clone()];
    app.file_names = vec![subdir.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];

    // Simulate the tree having the dir expanded + scanned
    app.tree_expanded.insert(subdir.clone());
    app.tree_scanned.insert(subdir.clone());
    app.tree_children
        .insert(subdir.clone(), vec![nested.clone()]);
    app.tree_has_subdirs.insert(subdir.clone(), true);

    // F2 inline rename subdir → renamed
    app.editing_idx = Some(0);
    app.edit_buffer = "renamed".into();
    app.commit_edit();

    let renamed = parent.join("renamed");
    assert!(renamed.is_dir(), "F2 rename happened on disk");
    assert!(
        app.tree_children.contains_key(&renamed),
        "F2 dir rename re-keys tree children"
    );
    assert!(app.tree_scanned.contains(&renamed));
    assert!(app.tree_expanded.contains(&renamed));
    assert_eq!(app.tree_has_subdirs.get(&renamed), Some(&true));
    assert!(!app.tree_children.contains_key(&subdir));

    // History recorded while inside the renamed dir must follow it around
    app.commits_by_dir.insert(
        renamed.clone(),
        vec![Commit {
            timestamp: std::time::Instant::now(),
            label: "Inner".into(),
            ops: vec![RenameOp {
                original_path: renamed.join("a.txt"),
                new_path: renamed.join("b.txt"),
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

    // Undo the rename — every consumer re-keys back to the original path
    app.undo_file(0);
    assert!(subdir.is_dir(), "undo restored the dir");
    assert!(!renamed.exists());
    assert!(
        app.tree_children.contains_key(&subdir),
        "dir undo re-keys tree children back"
    );
    assert!(app.tree_scanned.contains(&subdir));
    assert!(app.tree_expanded.contains(&subdir));
    assert_eq!(app.tree_has_subdirs.get(&subdir), Some(&true));
    assert!(!app.tree_children.contains_key(&renamed));

    // Descendant history re-keyed back, old key gone
    let inner = app.commits_by_dir.get(&subdir);
    assert!(inner.is_some(), "inner history re-keyed to restored dir");
    assert_eq!(inner.unwrap()[0].ops[0].original_path, subdir.join("a.txt"));
    assert_eq!(inner.unwrap()[0].ops[0].new_path, subdir.join("b.txt"));
    assert!(
        !app.commits_by_dir.contains_key(&renamed),
        "stale renamed history key removed"
    );

    // The undo record itself is not corrupted: the F2 op keeps exact paths
    let f2_ops = &app.commits_by_dir.get(&parent).unwrap()[0].ops;
    assert!(
        f2_ops
            .iter()
            .any(|op| op.original_path == subdir && op.new_path == renamed),
        "F2 op survives undo with exact paths"
    );

    // Redo — re-keys everything forward again
    app.redo_file(0);
    assert!(renamed.is_dir(), "redo re-applied the rename");
    assert!(!subdir.exists());
    assert!(app.tree_children.contains_key(&renamed));
    assert!(app.tree_scanned.contains(&renamed));
    assert!(app.tree_expanded.contains(&renamed));
    assert_eq!(app.tree_has_subdirs.get(&renamed), Some(&true));
    assert!(!app.tree_children.contains_key(&subdir));
    let inner = app.commits_by_dir.get(&renamed);
    assert!(inner.is_some(), "inner history follows the redo");
    assert_eq!(
        inner.unwrap()[0].ops[0].original_path,
        renamed.join("a.txt")
    );
    assert!(!app.commits_by_dir.contains_key(&subdir));
}
