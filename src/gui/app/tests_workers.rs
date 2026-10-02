use super::tests_util::*;
use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_preview_thread_abandons_stale_large_request() {
    let dir = TempDir::new().unwrap();

    // Create 60 temp files for the "stale" request — enough to still be
    // computing when the fresh request lands, without blowing the 5s
    // deadline on Windows (Defender scans every open).
    let mut stale_files: Vec<PathBuf> = Vec::new();
    for i in 0..60 {
        let f = dir.path().join(format!("stale_{}.txt", i));
        File::create(&f).unwrap();
        stale_files.push(f);
    }

    let mut app = app_with_live_threads();
    app.all_files = stale_files.clone();
    app.file_names = stale_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    app.selection = vec![true; 60];
    app.config.add.add_prefix = Some("old_".to_string());

    // Send the first (stale) preview request
    app.preview_dirty = true;
    app.update_preview();
    assert!(app.preview_pending, "stale request should be pending");

    // Immediately replace with 10 fresh files with a different prefix
    let mut fresh_files: Vec<PathBuf> = Vec::new();
    for i in 0..10 {
        let f = dir.path().join(format!("fresh_{}.txt", i));
        File::create(&f).unwrap();
        fresh_files.push(f);
    }
    app.all_files = fresh_files.clone();
    app.file_names = fresh_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    app.selection = vec![true; 10];
    app.config.add.add_prefix = Some("new_".to_string());

    app.preview_dirty = true;
    app.update_preview();

    // Poll check_scan_results until preview arrives (or timeout)
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.check_scan_results();
        if !app.preview_pending && !app.preview.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert!(
        !app.preview_pending,
        "preview never completed within timeout"
    );
    assert!(!app.preview.is_empty(), "preview should have results");
    // Verify the result is from the FRESH request (new_ prefix, 10 files)
    assert_eq!(
        app.preview.len(),
        10,
        "should have 10 items (fresh request), not 200 (stale)"
    );
    assert_eq!(
        app.preview[0].new_name(),
        "new_fresh_0.txt",
        "stale request should have been abandoned; got stale prefix"
    );
}

#[test]
fn test_preview_thread_handles_multiple_superseding_requests() {
    let dir = TempDir::new().unwrap();

    // 60 files keeps filesystem work small (each rename stats and opens
    // every file for its EXIF date — slow under Windows Defender) while
    // still giving the worker a long enough compute for the later
    // requests to supersede it (switching happens at drain points).
    let files: Vec<PathBuf> = (0..60)
        .map(|i| {
            let f = dir.path().join(format!("f_{}.txt", i));
            File::create(&f).unwrap();
            f
        })
        .collect();

    let mut app = app_with_live_threads();
    app.all_files = files.clone();
    app.file_names = files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    app.selection = vec![true; 60];

    // Send request 1: prefix "v1_"
    app.config.add.add_prefix = Some("v1_".to_string());
    app.preview_dirty = true;
    app.update_preview();

    // Immediately supersede with v2_
    app.config.add.add_prefix = Some("v2_".to_string());
    app.preview_dirty = true;
    app.update_preview();

    // Supersede again with v3_ (final)
    app.config.add.add_prefix = Some("v3_".to_string());
    app.preview_dirty = true;
    app.update_preview();

    // Wait for the final result
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.check_scan_results();
        if !app.preview_pending && !app.preview.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert!(
        !app.preview_pending,
        "preview never completed within timeout"
    );
    // Only v3_ should survive
    assert_eq!(
        app.preview[0].new_name(),
        "v3_f_0.txt",
        "only the final request should be processed"
    );
}

#[test]
fn test_row_cache_thread_abandons_stale_request() {
    let mut app = app_with_live_threads();

    // Create preview items for 600 stale files
    let stale_items: Vec<PreviewItem> = (0..600)
        .map(|i| {
            preview_row(
                RenameOp {
                    original_path: PathBuf::from(format!("/tmp/orig_{}.txt", i)),
                    new_path: PathBuf::from(format!("/tmp/new_{}.txt", i)),
                    original_name: format!("orig_{}.txt", i),
                    new_name: format!("new_{}.txt", i),
                    was_copy: false,
                    relocated: false,
                    original_permissions: None,
                    applied_attributes: String::new(),
                    applied_timestamps_mask: 0u8,
                },
                true,
                i,
            )
        })
        .collect();

    app.all_files = (0..600)
        .map(|i| PathBuf::from(format!("/tmp/orig_{}.txt", i)))
        .collect();
    app.file_names = app
        .all_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    app.file_sizes = vec![0u64; 600];
    app.file_dates = vec![0i64; 600];
    app.file_is_dir = vec![false; 600];
    app.preview = Arc::new(stale_items);

    // Request row cache build for the stale data
    app.cached_rows = std::rc::Rc::new(Vec::new());
    app.rows_cache_pending = false;
    app.rebuild_rows_cache();
    assert!(app.rows_cache_pending, "stale row cache should be pending");

    // Immediately replace with 5 fresh items and rebuild
    let fresh_items: Vec<PreviewItem> = (0..5)
        .map(|i| {
            preview_row(
                RenameOp {
                    original_path: PathBuf::from(format!("/tmp/only_{}.txt", i)),
                    new_path: PathBuf::from(format!("/tmp/only_{}.txt", i)),
                    original_name: format!("only_{}.txt", i),
                    new_name: format!("only_{}.txt", i),
                    was_copy: false,
                    relocated: false,
                    original_permissions: None,
                    applied_attributes: String::new(),
                    applied_timestamps_mask: 0u8,
                },
                true,
                i,
            )
        })
        .collect();

    app.all_files = (0..5)
        .map(|i| PathBuf::from(format!("/tmp/only_{}.txt", i)))
        .collect();
    app.file_names = app
        .all_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    app.file_sizes = vec![0u64; 5];
    app.file_dates = vec![0i64; 5];
    app.file_is_dir = vec![false; 5];
    app.preview = Arc::new(fresh_items);

    app.rows_cache_pending = false;
    app.rebuild_rows_cache();

    // Poll until the result arrives
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.check_scan_results();
        if !app.rows_cache_pending && !app.cached_rows.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert!(
        !app.rows_cache_pending,
        "row cache never completed within timeout"
    );
    assert!(!app.cached_rows.is_empty(), "cached rows should have data");
    // Should have 5 rows (the fresh request), not 600 (the stale one)
    assert_eq!(
        app.cached_rows.len(),
        5,
        "stale row cache request should have been abandoned; got stale data"
    );
}

// ── Selection-dispatch throttle (box-drag coalescing) ──

#[test]
fn test_selection_dispatch_throttle_decisions() {
    let t0 = std::time::Instant::now();
    let fresh = t0 + std::time::Duration::from_millis(10);
    let stale = t0 + SELECTION_DISPATCH_THROTTLE + std::time::Duration::from_millis(1);

    // Never defer when not dragging.
    assert!(!defer_selection_dispatch(false, Some(t0), fresh));
    assert!(!defer_selection_dispatch(false, None, t0));
    // No previous dispatch → dispatch immediately even while dragging.
    assert!(!defer_selection_dispatch(true, None, t0));
    // Dragging with a fresh dispatch → defer; after the window → dispatch.
    assert!(defer_selection_dispatch(true, Some(t0), fresh));
    assert!(!defer_selection_dispatch(true, Some(t0), stale));

    // Flush: nothing pending → never flush.
    assert!(!should_flush_selection_dispatch(false, false, Some(t0), t0));
    assert!(!should_flush_selection_dispatch(
        false,
        true,
        Some(t0),
        fresh
    ));
    // Pending + drag released → flush immediately.
    assert!(should_flush_selection_dispatch(
        true,
        false,
        Some(t0),
        fresh
    ));
    // Pending + still dragging + window passed → flush.
    assert!(should_flush_selection_dispatch(true, true, Some(t0), stale));
    // Pending + still dragging + fresh → wait.
    assert!(!should_flush_selection_dispatch(
        true,
        true,
        Some(t0),
        fresh
    ));
    // Pending, no recorded dispatch → flush (nothing to wait for).
    assert!(should_flush_selection_dispatch(true, true, None, t0));
    assert!(should_flush_selection_dispatch(true, false, None, t0));
}
#[test]
fn test_rows_cache_incremental_matches_full_rebuild() {
    let files: Vec<String> = (0..50).map(|i| format!("/tmp/f{}.txt", i)).collect();
    let sel1: Vec<bool> = (0..50).map(|i| i % 3 == 0).collect();
    let sel2: Vec<bool> = (0..50).map(|i| i % 2 == 0).collect();
    let fp = test_fp(50);

    // Full build for the base (selection sel1).
    let base_req = rows_cache_test_request(files.clone(), sel1.clone(), false, fp.clone());
    let base_rows = build_rows_cache(&base_req, None);

    // Incremental rebuild for sel2 on top of the base.
    let inc_req = rows_cache_test_request(files.clone(), sel2.clone(), true, fp.clone());
    let inc_rows = build_rows_cache(&inc_req, Some((&base_rows, &sel1, &fp)));

    // Full rebuild for sel2 — ground truth.
    let full_req = rows_cache_test_request(files, sel2, false, fp);
    let full_rows = build_rows_cache(&full_req, None);

    assert_eq!(row_sig(&inc_rows), row_sig(&full_rows));
}

/// Rows whose selection state is unchanged must be carried over verbatim;
/// only flipped rows are recomputed.
#[test]
fn test_rows_cache_incremental_carries_unchanged_rows() {
    let files = vec![
        "/tmp/a.txt".to_string(),
        "/tmp/b.txt".to_string(),
        "/tmp/c.txt".to_string(),
        "/tmp/d.txt".to_string(),
    ];
    let sel1 = vec![true, false, true, true];
    let sel2 = vec![true, true, true, false];
    let fp = test_fp(4);

    let base_rows = sentinel_rows(4);
    let req = rows_cache_test_request(files, sel2, true, fp.clone());
    let rows = build_rows_cache(&req, Some((&base_rows, &sel1, &fp)));

    // Selection unchanged at 0 (selected) and 2 (selected) → carried.
    assert_eq!(rows[0].name, "BASE_0");
    assert_eq!(rows[0].new_name, "SENTINEL_0");
    assert_eq!(rows[2].name, "BASE_2");
    assert_eq!(rows[2].new_name, "SENTINEL_2");

    // Selection flipped at 1 (F→T) and 3 (T→F) → recomputed from real data.
    assert_eq!(rows[1].name, "b.txt");
    assert_ne!(rows[1].new_name, "SENTINEL_1");
    assert_eq!(rows[3].name, "d.txt");
    assert_eq!(rows[3].new_name, "d.txt"); // deselected → plain name, no diff
    assert!(!rows[3].changed);
}

/// A dataset fingerprint mismatch must force a full rebuild — nothing is
/// carried over.
#[test]
fn test_rows_cache_incremental_falls_back_on_fp_mismatch() {
    let files = vec![
        "/tmp/a.txt".to_string(),
        "/tmp/b.txt".to_string(),
        "/tmp/c.txt".to_string(),
        "/tmp/d.txt".to_string(),
    ];
    let sel1 = vec![true, false, true, false];
    let sel2 = vec![true, true, false, false];
    let fp = test_fp(4);
    let mut other_fp = fp.clone();
    other_fp.rows_dataset_gen += 1; // dataset changed (e.g. an F2 rename)

    let base_rows = sentinel_rows(4);
    let req = rows_cache_test_request(files, sel2, true, other_fp);
    let rows = build_rows_cache(&req, Some((&base_rows, &sel1, &fp)));

    // Full rebuild — even unchanged-selection rows are recomputed.
    assert_eq!(rows[0].name, "a.txt");
    assert_ne!(rows[0].new_name, "SENTINEL_0");
    assert_eq!(rows[2].name, "c.txt");
    assert_ne!(rows[2].new_name, "SENTINEL_2");
}

/// A base whose length doesn't match the file list must force a full rebuild.
#[test]
fn test_rows_cache_incremental_falls_back_on_len_mismatch() {
    let files = vec![
        "/tmp/a.txt".to_string(),
        "/tmp/b.txt".to_string(),
        "/tmp/c.txt".to_string(),
        "/tmp/d.txt".to_string(),
    ];
    let sel1 = vec![true, false, true, false];
    let sel2 = vec![true, true, false, false];
    let fp = test_fp(4);

    let short_base = sentinel_rows(2);
    let req = rows_cache_test_request(files, sel2, true, fp.clone());
    let rows = build_rows_cache(&req, Some((&short_base, &sel1, &fp)));

    assert_eq!(rows[0].name, "a.txt");
    assert_ne!(rows[0].new_name, "SENTINEL_0");
}

/// `rows_dataset_gen` bumps only on full rebuilds, keeping the fingerprint
/// stable across pure selection changes.
#[test]
fn test_rows_dataset_gen_bumps_only_on_full_rebuild() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("/tmp/a.txt")];
    app.file_names = vec!["/tmp/a.txt".to_string()];
    app.selection = vec![true];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.file_is_dir = vec![false];

    assert_eq!(app.rows_dataset_gen, 0);
    app.rebuild_rows_cache(); // full
    assert_eq!(app.rows_dataset_gen, 1);
    app.rebuild_rows_cache_selection_only();
    assert_eq!(app.rows_dataset_gen, 1, "selection-only must not bump");
    app.rebuild_rows_cache();
    assert_eq!(app.rows_dataset_gen, 2);
}

/// End-to-end through the live worker: a selection-only rebuild after a
/// full build reflects the new selection (deselected rows revert to their
/// plain names).
#[test]
fn test_selection_only_rebuild_updates_rows_via_worker() {
    let mut app = app_with_live_threads();
    let dir = TempDir::new().unwrap();
    let files: Vec<String> = (0..20)
        .map(|i| {
            let p = dir.path().join(format!("f{}.txt", i));
            std::fs::write(&p, b"x").unwrap();
            p.to_string_lossy().to_string()
        })
        .collect();
    app.all_files = files.iter().map(PathBuf::from).collect();
    app.file_names = files.clone();
    app.file_sizes = vec![1; 20];
    app.file_dates = vec![0; 20];
    app.file_is_dir = vec![false; 20];
    app.config.add.add_suffix = Some("_X".to_string());
    app.selection = (0..20).map(|i| i < 10).collect();
    // Identity preview ops — selected rows without a real op get their
    // rename computed by `build_row` (calculate_rename).
    app.preview = Arc::new(
        (0..20)
            .map(|i| {
                let path = PathBuf::from(&files[i]);
                let name = format!("f{}.txt", i);
                preview_row(
                    RenameOp {
                        original_path: path.clone(),
                        new_path: path,
                        original_name: name.clone(),
                        new_name: name,
                        was_copy: false,
                        relocated: false,
                        original_permissions: None,
                        applied_attributes: String::new(),
                        applied_timestamps_mask: 0,
                    },
                    false,
                    i,
                )
            })
            .collect(),
    );

    // Full build with the initial selection (0..10 selected → renamed).
    app.rebuild_rows_cache();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.check_scan_results();
        if !app.rows_cache_pending && !app.cached_rows.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(app.cached_rows.len(), 20, "initial build never completed");
    assert_eq!(app.cached_rows[1].new_name, "f1_X.txt");

    // Selection change (even indices) → selection-only rebuild.
    app.selection = (0..20).map(|i| i % 2 == 0).collect();
    app.rebuild_rows_cache_selection_only();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.check_scan_results();
        if !app.rows_cache_pending {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    // Row 1: deselected now → plain name, no diff.
    assert_eq!(app.cached_rows[1].new_name, "f1.txt");
    assert!(!app.cached_rows[1].changed);
    // Row 2: still selected → renamed.
    assert_eq!(app.cached_rows[2].new_name, "f2_X.txt");
    // Row 10: newly selected → renamed.
    assert_eq!(app.cached_rows[10].new_name, "f10_X.txt");
}

/// In-place file mutations (an F2 rename keeps the file count, so no scan
/// generation changes) must invalidate the incremental base: the next
/// selection-only rebuild falls back to a full rebuild and shows the new
/// name instead of carrying a stale row.
#[test]
fn test_post_names_changed_invalidates_incremental_base() {
    let mut app = app_with_live_threads();
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, b"x").unwrap();
    app.all_files = vec![a.clone()];
    app.file_names = vec![a.to_string_lossy().to_string()];
    app.file_sizes = vec![1];
    app.file_dates = vec![0];
    app.file_is_dir = vec![false];
    app.selection = vec![true];
    app.preview = Arc::new(vec![preview_row(
        RenameOp {
            original_path: a.clone(),
            new_path: a.clone(),
            original_name: "a.txt".into(),
            new_name: "a.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0,
        },
        false,
        0,
    )]);

    // Initial full build — let it land so the worker has an incremental base.
    app.rebuild_rows_cache();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.check_scan_results();
        if !app.rows_cache_pending && !app.cached_rows.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(app.cached_rows[0].name, "a.txt");

    // Simulate an F2 rename: disk + in-place file list mutation.
    std::fs::rename(&a, &b).unwrap();
    app.all_files[0] = b.clone();
    app.file_names[0] = b.to_string_lossy().to_string();
    app.post_names_changed();

    // Selection-only rebuild (no preview recompute in between). The
    // fingerprint bump must force a full rebuild so the row shows the new
    // name rather than the carried-over stale row.
    app.rebuild_rows_cache_selection_only();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.check_scan_results();
        if !app.rows_cache_pending {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(app.cached_rows[0].name, "b.txt");
    assert_eq!(app.cached_rows[0].new_name, "b.txt");
}
