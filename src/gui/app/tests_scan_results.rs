use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_check_scan_results_wires_data() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let file1 = dir.path().join("a.txt");
    let file2 = dir.path().join("b.txt");
    File::create(&file1).unwrap();
    File::create(&file2).unwrap();

    // Put the app in scanning state so check_scan_results processes results
    app.scanning = true;

    // Send a result through the channel
    app.scan_result_tx
        .send(ScanResult {
            files: vec![file1.clone(), file2.clone()],
            sizes: vec![10, 20],
            dates: vec![1000, 2000],
            is_dirs: vec![false, false],
            r#gen: 0,
            is_partial: false,
        })
        .unwrap();

    app.check_scan_results();

    assert!(!app.scanning, "scanning should be false after processing");
    assert_eq!(app.all_files.len(), 2, "should have 2 files");
    assert_eq!(app.file_sizes, vec![10, 20]);
    assert_eq!(app.file_dates, vec![1000, 2000]);
    assert_eq!(app.selection.len(), 2, "selection should match file count");
    assert!(
        app.selection.iter().all(|&s| !s),
        "no files should be selected by default"
    );
    assert!(app.preview_dirty, "preview should be marked dirty");
}

#[test]
fn test_check_scan_results_not_scanning() {
    let mut app = GuiApp::new_with_config_for_test();
    app.scanning = false;
    let all_files_before = app.all_files.len();
    // Should be a no-op
    app.check_scan_results();
    assert_eq!(
        app.all_files.len(),
        all_files_before,
        "check_scan_results should be no-op when not scanning"
    );
}

#[test]
fn test_scan_completion_restores_previous_status() {
    // A scan swaps the status bar to the scanning message and completes by
    // restoring whatever status was showing before the scan started.
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("a.txt");
    File::create(&file).unwrap();
    app.cwd = dir.path().to_path_buf();
    app.cwd_input = dir.path().to_string_lossy().to_string();

    app.set_status("keep me", StatusKind::Persistent);
    app.scan_dir();
    assert!(app.scanning);
    assert_eq!(
        app.status_message, SCANNING_STATUS,
        "status bar should show the scanning message"
    );
    assert_eq!(app.status_kind, StatusKind::Persistent);

    // Inject the final result rather than waiting on the background thread.
    app.scan_result_tx
        .send(ScanResult {
            files: vec![file],
            sizes: vec![0],
            dates: vec![0],
            is_dirs: vec![false],
            r#gen: app.scan_generation,
            is_partial: false,
        })
        .unwrap();
    app.check_scan_results();

    assert!(!app.scanning, "scanning should be false after processing");
    assert_eq!(
        app.status_message, "keep me",
        "previous status should be restored after the scan"
    );
    assert_eq!(app.status_kind, StatusKind::Persistent);
    assert!(
        app.status_before_scan.is_none(),
        "snapshot should be consumed by the restore"
    );
}

#[test]
fn test_scan_completion_keeps_newer_status_set_mid_scan() {
    // A status set while the scan is in flight (e.g. "All settings reset")
    // takes precedence over the pre-scan snapshot.
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("a.txt");
    File::create(&file).unwrap();
    app.cwd = dir.path().to_path_buf();

    app.set_status("keep me", StatusKind::Persistent);
    app.scan_dir();
    app.set_status("All settings reset", StatusKind::Temporary);

    app.scan_result_tx
        .send(ScanResult {
            files: vec![file],
            sizes: vec![0],
            dates: vec![0],
            is_dirs: vec![false],
            r#gen: app.scan_generation,
            is_partial: false,
        })
        .unwrap();
    app.check_scan_results();

    assert_eq!(
        app.status_message, "All settings reset",
        "mid-scan status must not be clobbered by the restore"
    );
    assert_eq!(app.status_kind, StatusKind::Temporary);
    assert!(app.status_before_scan.is_none());
}

#[test]
fn test_cancel_scan_restores_previous_status() {
    let mut app = GuiApp::new_with_config_for_test();
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("a.txt");
    File::create(&file).unwrap();
    app.cwd = dir.path().to_path_buf();

    app.set_status("keep me", StatusKind::Persistent);
    app.scan_dir();
    assert_eq!(app.status_message, SCANNING_STATUS);

    app.cancel_scan();
    assert!(!app.scanning);
    assert_eq!(
        app.status_message, "keep me",
        "cancelling a scan restores the previous status"
    );
    assert!(app.status_before_scan.is_none());
}
#[test]
fn test_check_scan_results_tree_does_not_affect_scanning_state() {
    let mut app = GuiApp::new_with_config_for_test();
    let (tree_result_tx, new_tree_scan_rx) = mpsc::channel();
    app.tree_scan_rx = new_tree_scan_rx;

    // Simulate being in the middle of a file scan
    app.scanning = true;

    // Send a tree result
    let dir = PathBuf::from("/tmp");
    let generation = app.begin_tree_scan_for_test(&dir);
    tree_result_tx.send((dir, generation, vec![])).unwrap();

    app.check_scan_results();

    // File scan state should be unchanged
    assert!(app.scanning, "file scan should still be in progress");
}

#[test]
fn test_check_scan_results_preview_does_not_affect_scanning_state() {
    let mut app = GuiApp::new_with_config_for_test();
    let (preview_result_tx, new_preview_rx) = mpsc::channel();
    app.preview_rx = new_preview_rx;

    app.scanning = true;
    app.preview_pending = true;
    app.preview_generation = 1;

    preview_result_tx.send((1, vec![])).unwrap();

    app.check_scan_results();

    assert!(app.scanning, "file scan should still be in progress");
}
#[test]
fn test_pending_preselection_marks_matching_files() {
    let mut app = GuiApp::new_with_config_for_test();

    // Simulate a completed scan result with some files
    let (tx, rx) = mpsc::channel();
    app.scan_result_rx = rx;
    app.scanning = true;

    app.all_files = vec![
        "/tmp/photo.jpg".into(),
        "/tmp/document.txt".into(),
        "/tmp/song.mp3".into(),
    ];

    app.pending_preselection = Some(vec!["photo.jpg".into(), "song.mp3".into()]);

    // Send the scan result through the channel
    tx.send(ScanResult {
        files: app.all_files.clone(),
        sizes: vec![100, 200, 300],
        dates: vec![1000, 2000, 3000],
        is_dirs: vec![false, false, false],
        r#gen: 0,
        is_partial: false,
    })
    .unwrap();

    app.check_scan_results();

    assert!(!app.scanning);
    assert_eq!(app.selection, vec![true, false, true]);
    assert!(
        app.pending_preselection.is_none(),
        "pending_preselection should be consumed"
    );
}

#[test]
fn test_pending_preselection_empty_list_selects_nothing() {
    let mut app = GuiApp::new_with_config_for_test();

    let (tx, rx) = mpsc::channel();
    app.scan_result_rx = rx;
    app.scanning = true;

    app.all_files = vec!["/tmp/photo.jpg".into(), "/tmp/document.txt".into()];

    app.pending_preselection = Some(vec![]);

    tx.send(ScanResult {
        files: app.all_files.clone(),
        sizes: vec![100, 200],
        dates: vec![1000, 2000],
        is_dirs: vec![false, false],
        r#gen: 0,
        is_partial: false,
    })
    .unwrap();

    app.check_scan_results();

    assert_eq!(app.selection, vec![false, false]);
}

#[test]
fn test_pending_preselection_none_is_noop() {
    let mut app = GuiApp::new_with_config_for_test();

    let (tx, rx) = mpsc::channel();
    app.scan_result_rx = rx;
    app.scanning = true;

    app.pending_preselection = None;

    tx.send(ScanResult {
        files: vec!["/tmp/file.txt".into()],
        sizes: vec![100],
        dates: vec![1000],
        is_dirs: vec![false],
        r#gen: 0,
        is_partial: false,
    })
    .unwrap();

    app.check_scan_results();

    assert_eq!(app.selection, vec![false]);
}

#[test]
fn test_pending_preselection_ignores_paths_not_in_scan() {
    let mut app = GuiApp::new_with_config_for_test();
    let (tx, rx) = mpsc::channel();
    app.scan_result_rx = rx;
    app.scanning = true;

    app.all_files = vec!["/tmp/a.txt".into(), "/tmp/b.txt".into()];

    // Pre-selection includes a file that won't be in the scan results
    app.pending_preselection = Some(vec!["a.txt".into(), "missing.txt".into()]);

    tx.send(ScanResult {
        files: app.all_files.clone(),
        sizes: vec![100, 200],
        dates: vec![1000, 2000],
        is_dirs: vec![false, false],
        r#gen: 0,
        is_partial: false,
    })
    .unwrap();

    app.check_scan_results();

    // a.txt matched, missing.txt didn't — b.txt stays unselected
    assert_eq!(app.selection, vec![true, false]);
}

#[test]
fn test_pending_preselection_selects_dirs_by_basename() {
    let mut app = GuiApp::new_with_config_for_test();
    let (tx, rx) = mpsc::channel();
    app.scan_result_rx = rx;
    app.scanning = true;

    app.all_files = vec!["/tmp/MyFolder".into(), "/tmp/photo.jpg".into()];

    app.pending_preselection = Some(vec!["MyFolder".into()]);

    tx.send(ScanResult {
        files: app.all_files.clone(),
        sizes: vec![0, 100],
        dates: vec![1000, 2000],
        is_dirs: vec![true, false],
        r#gen: 0,
        is_partial: false,
    })
    .unwrap();

    app.check_scan_results();

    assert_eq!(app.selection, vec![true, false]);
}

#[test]
fn test_pending_preselection_cross_directory_files_ignored() {
    // Simulates the (theoretical) case where files from different
    // directories are passed. This can't happen via OS context menus
    // (file managers only allow selection within one directory), but
    // could happen via manual `awara --gui /dir1/a.txt /dir2/b.txt`.
    // Expected: first file's parent is scanned, cross-dir files ignored.
    let mut app = GuiApp::new_with_config_for_test();

    let (tx, rx) = mpsc::channel();
    app.scan_result_rx = rx;
    app.scanning = true;

    // Scenario: user passed files from two different directories.
    // `run_with_files` picks the first file's parent (here /dir1) as cwd
    // and scans it. Only /dir1/a.txt is in scan results.
    app.pending_preselection = Some(vec![
        "a.txt".into(), // exists in scanned dir
        "b.txt".into(), // from a different dir, not in scan
    ]);

    app.all_files = vec!["/dir1/a.txt".into(), "/dir1/c.txt".into()];

    tx.send(ScanResult {
        files: app.all_files.clone(),
        sizes: vec![100, 200],
        dates: vec![1000, 2000],
        is_dirs: vec![false, false],
        r#gen: 0,
        is_partial: false,
    })
    .unwrap();

    app.check_scan_results();

    // a.txt matched → selected; b.txt not in scan → ignored;
    // c.txt in dir but not preselected → unselected.
    assert_eq!(app.selection, vec![true, false]);
}

#[test]
fn test_pending_preselection_case_insensitive_windows() {
    // Windows/macOS pass the exact filename from the context menu,
    // but the file system is case-insensitive. Our matching should
    // handle PHOTO.JPG → photo.jpg.
    let mut app = GuiApp::new_with_config_for_test();
    let (tx, rx) = mpsc::channel();
    app.scan_result_rx = rx;
    app.scanning = true;

    app.pending_preselection = Some(vec!["PHOTO.JPG".into(), "DOCUMENT.TXT".into()]);
    app.all_files = vec!["/dir/photo.jpg".into(), "/dir/document.txt".into()];

    tx.send(ScanResult {
        files: app.all_files.clone(),
        sizes: vec![100, 200],
        dates: vec![1000, 2000],
        is_dirs: vec![false, false],
        r#gen: 0,
        is_partial: false,
    })
    .unwrap();

    app.check_scan_results();
    assert_eq!(app.selection, vec![true, true]);
}

#[test]
fn test_pending_preselection_mixed_case_partial() {
    let mut app = GuiApp::new_with_config_for_test();
    let (tx, rx) = mpsc::channel();
    app.scan_result_rx = rx;
    app.scanning = true;

    // One matches case-insensitively, one doesn't exist
    app.pending_preselection = Some(vec!["My File.TXT".into(), "NOPE.txt".into()]);
    app.all_files = vec!["/dir/my file.txt".into(), "/dir/other.txt".into()];

    tx.send(ScanResult {
        files: app.all_files.clone(),
        sizes: vec![100, 200],
        dates: vec![1000, 2000],
        is_dirs: vec![false, false],
        r#gen: 0,
        is_partial: false,
    })
    .unwrap();

    app.check_scan_results();
    assert_eq!(app.selection, vec![true, false]);
}

#[test]
fn test_pending_preselection_unicode_filenames() {
    // Files with non-ASCII characters (common in real-world usage)
    let mut app = GuiApp::new_with_config_for_test();
    let (tx, rx) = mpsc::channel();
    app.scan_result_rx = rx;
    app.scanning = true;

    app.pending_preselection = Some(vec!["café.jpg".into(), "résumé.pdf".into()]);
    app.all_files = vec!["/dir/café.jpg".into(), "/dir/plain.txt".into()];

    tx.send(ScanResult {
        files: app.all_files.clone(),
        sizes: vec![100, 200],
        dates: vec![1000, 2000],
        is_dirs: vec![false, false],
        r#gen: 0,
        is_partial: false,
    })
    .unwrap();

    app.check_scan_results();
    // café.jpg matches; résumé.pdf doesn't exist in scan; plain.txt not preselected
    assert_eq!(app.selection, vec![true, false]);
}

#[test]
fn test_pending_preselection_hidden_files() {
    // Files starting with a dot should match by full basename
    let mut app = GuiApp::new_with_config_for_test();
    let (tx, rx) = mpsc::channel();
    app.scan_result_rx = rx;
    app.scanning = true;

    app.pending_preselection = Some(vec![".gitignore".into(), ".config".into()]);
    app.all_files = vec![
        "/proj/.gitignore".into(),
        "/proj/.config".into(),
        "/proj/readme.md".into(),
    ];

    tx.send(ScanResult {
        files: app.all_files.clone(),
        sizes: vec![10, 200, 50],
        dates: vec![1000, 2000, 3000],
        is_dirs: vec![false, true, false],
        r#gen: 0,
        is_partial: false,
    })
    .unwrap();

    app.check_scan_results();
    assert_eq!(app.selection, vec![true, true, false]);
}

#[test]
fn test_pending_preselection_filenames_with_spaces() {
    // Files with spaces should match by full basename
    let mut app = GuiApp::new_with_config_for_test();
    let (tx, rx) = mpsc::channel();
    app.scan_result_rx = rx;
    app.scanning = true;

    app.pending_preselection = Some(vec!["my vacation photo.jpg".into()]);
    app.all_files = vec![
        "/dir/my vacation photo.jpg".into(),
        "/dir/summer.jpg".into(),
    ];

    tx.send(ScanResult {
        files: app.all_files.clone(),
        sizes: vec![100, 200],
        dates: vec![1000, 2000],
        is_dirs: vec![false, false],
        r#gen: 0,
        is_partial: false,
    })
    .unwrap();

    app.check_scan_results();
    assert_eq!(app.selection, vec![true, false]);
}
