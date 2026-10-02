//! Regression tests for the preview/apply ordering bugs.
//!
//! Two distinct defects, both rooted in the display order being derived from
//! live state instead of being an explicit, stored value:
//!
//! 1. Changing the sort did not invalidate the preview, so numbering stayed
//!    keyed to the *old* order while Apply renumbered using the *new* one.
//! 2. Completing a preview recomputed the display order, so the table re-sorted
//!    itself under the user while they were editing rename options.
//!
//! The fix makes the order a stored value: `get_display_order()` is its only
//! producer, both the preview and Apply materialize it, and only explicit events
//! (sort click, manual reposition, rescan, filter) invalidate it. Apply always
//! plans from that snapshot, so there is no second, separately-derived plan that
//! could disagree with the preview.

use super::*;
use awara::NumberingMode;
use std::fs;
use std::fs::File;
use tempfile::TempDir;

/// Two files in index order [alpha, zebra] with size order [zebra, alpha], so
/// name order and size order disagree.
fn app_name_and_size_order_disagree() -> (GuiApp, TempDir) {
    let dir = TempDir::new().unwrap();
    for n in ["alpha.txt", "zebra.txt"] {
        File::create(dir.path().join(n)).unwrap();
    }

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = vec![dir.path().join("alpha.txt"), dir.path().join("zebra.txt")];
    app.file_names = vec![
        dir.path().join("alpha.txt").to_string_lossy().to_string(),
        dir.path().join("zebra.txt").to_string_lossy().to_string(),
    ];
    app.file_display_names = vec!["alpha.txt".to_string(), "zebra.txt".to_string()];
    app.file_sizes = vec![100, 1];
    app.file_dates = vec![0, 0];
    app.file_is_dir = vec![false, false];
    app.selection = vec![true, true];
    app.section_enabled = DEFAULT_SECTION_ENABLED;
    (app, dir)
}

fn new_name_of(app: &GuiApp, file_idx: usize) -> String {
    app.preview[file_idx].new_name().to_string()
}

/// Numbering that follows the display order: `1_`, `2_`, … in sorted order.
fn enable_prefix_numbering(app: &mut GuiApp) {
    app.config.numbering.numbering_mode = Some(NumberingMode::Prefix);
    app.config.numbering.numbering_start = 1;
    app.config.numbering.numbering_increment = 1;
    app.config.numbering.numbering_pad = 0;
    app.config.numbering.numbering_sep = Some("_".to_string());
}

/// Bug 1: with numbering active, re-sorting changes every generated name, so
/// the preview must be recomputed. Before the fix `sort_preview_by` did not
/// arm the preview and `update_preview`'s dispatch gate ignored the order
/// entirely, leaving the table showing names from the previous sort order.
#[test]
fn test_sort_change_renumbers_preview_when_numbering_active() {
    let (mut app, _dir) = app_name_and_size_order_disagree();
    enable_prefix_numbering(&mut app);

    // Sort by size → plan order [zebra, alpha] → zebra gets 1, alpha gets 2.
    app.sort_preview_by(PreviewSortCol::Size);
    app.preview_dirty = true;
    app.update_preview();
    assert_eq!(
        new_name_of(&app, 0),
        "2_alpha.txt",
        "size order: alpha is #2"
    );
    assert_eq!(
        new_name_of(&app, 1),
        "1_zebra.txt",
        "size order: zebra is #1"
    );

    // The user now clicks the Name column. Numbering follows display order, so
    // alpha must become #1 and zebra #2.
    app.sort_preview_by(PreviewSortCol::Name);
    app.update_preview();

    assert_eq!(
        new_name_of(&app, 0),
        "1_alpha.txt",
        "after re-sorting by name, alpha must be renumbered to #1"
    );
    assert_eq!(
        new_name_of(&app, 1),
        "2_zebra.txt",
        "after re-sorting by name, zebra must be renumbered to #2"
    );
}

/// Bug 2: a completed preview must not rebuild the display order. The order is
/// the same object the table draws with, so `Rc::ptr_eq` is a direct assertion
/// that the rows did not move.
#[test]
fn test_preview_recompute_does_not_rebuild_display_order() {
    let (mut app, _dir) = app_name_and_size_order_disagree();
    app.sort_preview_by(PreviewSortCol::Size);

    // Materialize the display order the table is drawing with.
    let before = app.get_display_order();
    assert_eq!(*before, vec![1, 0], "size order puts zebra (idx 1) first");

    // The user edits an option; the preview recomputes with new names.
    app.config.remove.remove_first = Some(2);
    app.preview_dirty = true;
    app.preview_generation += 1;
    app.compute_preview_sync(app.preview_generation);

    let after = app.get_display_order();
    assert!(
        std::rc::Rc::ptr_eq(&before, &after),
        "preview completion must not recompute the display order (table must not re-sort under the user)"
    );
    assert_eq!(*after, vec![1, 0], "display order unchanged");
}

/// The ordering guarantee, end to end: the names the preview displays are
/// exactly the names Apply produces.
///
/// Preview and Apply each derive a plan, so this is the property that can
/// actually break (and did: the preview renumbered in display order while Apply
/// numbered in file-index order). Numbering is active and the sort disagrees
/// with index order, which is the only case where the two can differ.
#[test]
fn test_apply_executes_the_names_the_preview_shows() {
    let (mut app, dir) = app_name_and_size_order_disagree();
    enable_prefix_numbering(&mut app);

    app.sort_preview_by(PreviewSortCol::Size);
    app.preview_dirty = true;
    app.update_preview();

    // Displayed names, in file-index order.
    let shown: Vec<String> = (0..app.all_files.len())
        .map(|i| new_name_of(&app, i))
        .collect();
    assert_eq!(shown, ["2_alpha.txt", "1_zebra.txt"]);

    app.execute_renames();

    let mut on_disk: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    on_disk.sort();
    let mut expected = shown.clone();
    expected.sort();
    assert_eq!(
        on_disk, expected,
        "Apply must produce exactly the names the preview showed"
    );
}

/// Apply must use the *live* selection, not a snapshot taken when the preview was
/// planned. Deselecting without bumping `selection_generation` is the adversarial
/// case: a generation-based shortcut would rename a file the user just
/// deselected, and the counter is a convention that any new path could forget.
#[test]
fn test_apply_respects_selection_changed_without_a_generation_bump() {
    let dir = TempDir::new().unwrap();
    for n in ["a.txt", "b.txt"] {
        File::create(dir.path().join(n)).unwrap();
    }

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.cwd_input = app.cwd.to_string_lossy().to_string();
    app.all_files = vec![dir.path().join("a.txt"), dir.path().join("b.txt")];
    app.file_names = vec![
        dir.path().join("a.txt").to_string_lossy().to_string(),
        dir.path().join("b.txt").to_string_lossy().to_string(),
    ];
    app.file_display_names = vec!["a.txt".to_string(), "b.txt".to_string()];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.file_is_dir = vec![false, false];
    app.selection = vec![true, true];
    // Both files match, so both show a rename.
    app.config.add.add_prefix = Some("p_".into());

    app.preview_dirty = true;
    app.update_preview();
    assert_eq!(new_name_of(&app, 0), "p_a.txt");
    assert_eq!(new_name_of(&app, 1), "p_b.txt");

    // Deselect b.txt WITHOUT bumping `selection_generation` — the whole point of
    // the gap: the counter is a convention, not a guarantee.
    app.selection[1] = false;

    app.execute_renames();

    assert!(
        dir.path().join("p_a.txt").exists(),
        "the still-selected file must be renamed"
    );
    assert!(
        !dir.path().join("p_b.txt").exists(),
        "a deselected file must not be renamed"
    );
    assert!(
        dir.path().join("b.txt").exists(),
        "the deselected file must be left alone"
    );
}

/// Gap B: the preview dispatch gate claimed to cover "what the planner reads",
/// but left out `scan_generation` and `cwd`, so a change to either silently
/// skipped the recompute. Every plan input must be in the gate.
#[test]
fn test_dispatch_gate_covers_every_plan_input() {
    fn assert_redispatches(label: &str, mutate: impl FnOnce(&mut GuiApp)) {
        let (mut app, _dir) = app_name_and_size_order_disagree();
        app.preview_dirty = true;
        app.update_preview();
        let before = app.preview_generation;

        mutate(&mut app);
        app.preview_dirty = true;
        app.update_preview();

        assert!(
            app.preview_generation > before,
            "{label}: changing a plan input must redispatch the preview"
        );
    }

    assert_redispatches("config", |a| a.config.replace.replace = Some("z".into()));
    assert_redispatches("section_enabled", |a| {
        a.section_enabled[SectionId::Replace as usize] = false
    });
    assert_redispatches("selection", |a| a.selection_generation += 1);
    assert_redispatches("processed", |a| a.processed_generation += 1);
    assert_redispatches("file_count", |a| {
        a.all_files.push(PathBuf::from("/tmp/awara-extra.txt"))
    });
    assert_redispatches("scan_generation", |a| a.scan_generation += 1);
    assert_redispatches("cwd", |a| a.cwd = PathBuf::from("/tmp/awara-other"));
}

/// The one documented asymmetry: the display order changes generated names only
/// when numbering follows it. So a reorder must force a recompute with numbering
/// and must not without it (the rows are still valid, and re-planning on Apply
/// picks up the new collision order anyway).
#[test]
fn test_sort_redispatches_preview_only_with_numbering() {
    // Without numbering: no recompute.
    let (mut app, _dir) = app_name_and_size_order_disagree();
    app.preview_dirty = true;
    app.update_preview();
    let gen_before = app.preview_generation;

    app.sort_preview_by(PreviewSortCol::Size);
    app.update_preview();
    assert_eq!(
        app.preview_generation, gen_before,
        "without numbering a reorder cannot change a name, so no recompute"
    );

    // With numbering: it must recompute.
    let (mut app, _dir) = app_name_and_size_order_disagree();
    enable_prefix_numbering(&mut app);
    app.preview_dirty = true;
    app.update_preview();
    let gen_before = app.preview_generation;

    app.sort_preview_by(PreviewSortCol::Size);
    app.update_preview();
    assert!(
        app.preview_generation > gen_before,
        "with numbering a reorder renumbers every file, so it must recompute"
    );
}

/// Apply renames files in place: the file *set* keeps its indices, so the frozen
/// display order is still valid and must stay frozen. Rebuilding it there
/// re-sorted the table by the new names the moment the user clicked Apply.
#[test]
fn test_apply_does_not_resort_the_table() {
    let dir = TempDir::new().unwrap();
    for n in ["b.txt", "c.txt"] {
        File::create(dir.path().join(n)).unwrap();
    }
    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.cwd_input = app.cwd.to_string_lossy().to_string();
    app.all_files = vec![dir.path().join("b.txt"), dir.path().join("c.txt")];
    app.file_names = app
        .all_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    app.file_display_names = vec!["b.txt".to_string(), "c.txt".to_string()];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.file_is_dir = vec![false, false];
    app.selection = vec![true, true];
    // b.txt → z.txt, so a re-sort by name would put the files in the other order.
    app.config.replace.replace = Some("b".into());
    app.config.replace.with = Some("z".into());

    let before = app.get_display_order();
    assert_eq!(*before, vec![0, 1], "index order is name order to start");

    app.execute_renames();
    assert!(
        dir.path().join("z.txt").exists(),
        "the rename must have happened"
    );

    let after = app.get_display_order();
    assert!(
        std::rc::Rc::ptr_eq(&before, &after),
        "Apply must not rebuild the display order"
    );
    assert_eq!(*after, vec![0, 1], "rows must not move under the user");
}

/// Undo/redo restore names, so they are in-place mutations too: the order is the
/// planner's input and stays frozen until the user sorts explicitly.
#[test]
fn test_undo_does_not_resort_the_table() {
    let dir = TempDir::new().unwrap();
    for n in ["a.txt", "b.txt"] {
        File::create(dir.path().join(n)).unwrap();
    }
    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = vec![dir.path().join("a.txt"), dir.path().join("b.txt")];
    app.file_names = app
        .all_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    app.file_display_names = vec!["a.txt".to_string(), "b.txt".to_string()];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.file_is_dir = vec![false, false];
    app.selection = vec![true, true];
    // A recorded rename z.txt → a.txt, so undoing it restores a name that sorts
    // last: a rebuild would move the row.
    app.cwd_commits_mut().push(Commit {
        timestamp: std::time::Instant::now(),
        label: "Inline — test".into(),
        ops: vec![RenameOp {
            original_path: dir.path().join("z.txt"),
            new_path: dir.path().join("a.txt"),
            original_name: "z.txt".into(),
            new_name: "a.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        }],
    });

    let before = app.get_display_order();
    assert_eq!(*before, vec![0, 1]);

    app.undo_file(0);
    assert!(
        dir.path().join("z.txt").exists(),
        "the undo must have happened"
    );

    let after = app.get_display_order();
    assert!(
        std::rc::Rc::ptr_eq(&before, &after),
        "undo must not rebuild the display order"
    );
    assert_eq!(*after, vec![0, 1], "rows must not move under the user");
}

/// Every in-place name mutation funnels through `post_names_changed` — Apply, F2
/// inline rename, undo/redo, and a revert-dialog confirm — so the frozen order
/// has to survive there, not at one call site.
#[test]
fn test_post_names_changed_keeps_the_frozen_order() {
    let (mut app, dir) = app_name_and_size_order_disagree();
    let before = app.get_display_order();
    assert_eq!(*before, vec![0, 1], "name order: alpha, zebra");

    // Rename alpha.txt in place, the way F2/undo/redo/revert leave the list: only
    // the names change. A rebuild would sort to [zebra, zz_alpha].
    std::fs::rename(
        dir.path().join("alpha.txt"),
        dir.path().join("zz_alpha.txt"),
    )
    .unwrap();
    app.all_files[0] = dir.path().join("zz_alpha.txt");
    app.file_names[0] = app.all_files[0].to_string_lossy().to_string();
    app.post_names_changed();

    let after = app.get_display_order();
    assert!(
        std::rc::Rc::ptr_eq(&before, &after),
        "a rename must not rebuild the display order"
    );
    assert_eq!(*after, vec![0, 1], "rows must not move under the user");
}

/// Removals are the other half of "sorting is explicit": the listing can lose
/// entries in place — an apply that overwrote one, a trash — and that must prune
/// the frozen order rather than let it rebuild into a re-sort.
///
/// It must also keep *every* index-keyed vector in step. The removal sites had
/// drifted: trash skipped `file_is_dir` and `file_display_names`, so after a trash
/// the Type column and the Name sort read shifted entries, and the selection
/// anchors kept pointing at whatever had moved into their index.
#[test]
fn test_remove_file_indices_prunes_and_keeps_vectors_aligned() {
    let dir = TempDir::new().unwrap();
    let names = ["a.txt", "b.txt", "c.txt", "d.txt"];
    for n in names {
        File::create(dir.path().join(n)).unwrap();
    }
    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = names.iter().map(|n| dir.path().join(n)).collect();
    app.file_names = app
        .all_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    app.file_display_names = names.iter().map(|n| n.to_string()).collect();
    app.file_sizes = vec![0; 4];
    app.file_dates = vec![0; 4];
    app.file_is_dir = vec![false; 4];
    app.selection = vec![true, false, false, true];
    // A manual order [c, a, d, b], so a prune cannot be confused with a re-sort by
    // name (which would give [a, b, ...]).
    app.custom_order = Some(vec![2, 0, 3, 1]);
    app.last_clicked_idx = Some(3); // d
    app.selection_anchor = Some(1); // b
    let gen_before = app.order_generation;
    assert_eq!(*app.get_display_order(), vec![2, 0, 3, 1]);

    // Two entries leave the listing, as trashing a.txt and c.txt would.
    let removed = app.remove_file_indices(&[0, 2]);
    assert_eq!(
        removed,
        vec![
            dir.path().join("a.txt").to_string_lossy().to_string(),
            dir.path().join("c.txt").to_string_lossy().to_string()
        ],
        "the removed paths are reported, for the caller's commit cleanup"
    );
    assert!(
        !app.display_order_dirty.get(),
        "the order is pruned in place, not flagged for a rebuild (which would re-sort)"
    );
    assert_eq!(app.file_display_names, vec!["b.txt", "d.txt"]);
    assert_eq!(
        app.selection,
        vec![false, true],
        "b keeps its state, d keeps its"
    );
    assert_eq!(app.file_sizes.len(), 2);
    assert_eq!(app.file_dates.len(), 2);
    assert_eq!(
        app.file_is_dir.len(),
        2,
        "every per-file vector shrinks together"
    );
    assert_eq!(
        *app.get_display_order(),
        vec![1, 0],
        "pruned: d (now 1) then b (now 0), the sequence the user set"
    );
    assert!(
        app.order_generation > gen_before,
        "a pruned order is a new planner input"
    );
    assert_eq!(app.last_clicked_idx, Some(1), "d moved 3 → 1");
    assert_eq!(app.selection_anchor, Some(0), "b moved 1 → 0");

    // Remove b (now index 0): the anchor naming it is dropped rather than left
    // pointing at whatever moved into its place; the other anchor shifts.
    app.remove_file_indices(&[0]);
    assert_eq!(app.selection_anchor, None, "a removed anchor is cleared");
    assert_eq!(app.last_clicked_idx, Some(0), "d is the only file left");
    assert_eq!(*app.get_display_order(), vec![0]);
}

/// The one apply that changes the file *set*: resolving a collision with Overwrite
/// drops the replaced entry from the listing. Even then the table must not
/// re-sort — the order is pruned to cover what is left.
#[test]
fn test_apply_overwrite_does_not_resort_the_table() {
    let dir = TempDir::new().unwrap();
    for n in ["a.txt", "b.txt", "c.txt"] {
        File::create(dir.path().join(n)).unwrap();
    }
    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = vec![
        dir.path().join("a.txt"),
        dir.path().join("b.txt"),
        dir.path().join("c.txt"),
    ];
    app.file_names = app
        .all_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    app.file_display_names = vec![
        "a.txt".to_string(),
        "b.txt".to_string(),
        "c.txt".to_string(),
    ];
    app.file_sizes = vec![0, 0, 0];
    app.file_dates = vec![0, 0, 0];
    app.file_is_dir = vec![false, false, false];
    app.selection = vec![true, true, true];
    assert_eq!(*app.get_display_order(), vec![0, 1, 2]);

    // c.txt is renamed onto a.txt, so a.txt's entry leaves the listing.
    let plan = awara::RenamePlan::from_ops(vec![RenameOp {
        original_path: dir.path().join("c.txt"),
        new_path: dir.path().join("a.txt"),
        original_name: "c.txt".to_string(),
        new_name: "a.txt".to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    }]);
    let resolution = awara::Resolution::default_for(CollisionStrategy::Overwrite);
    app.execute_plan_inner(plan, &resolution, "Apply", false);

    assert_eq!(
        app.all_files.len(),
        2,
        "the overwritten entry left the listing"
    );
    assert_eq!(app.file_display_names, vec!["b.txt", "a.txt"]);
    assert_eq!(
        *app.get_display_order(),
        vec![0, 1],
        "pruned: b, then the file renamed onto a.txt"
    );
}

/// Structural safety: the stored order must always cover exactly the current
/// file list, or the table would index out of bounds / show stale rows. Any
/// change to the file set must refresh it.
#[test]
fn test_display_order_tracks_file_set_changes() {
    let (mut app, _dir) = app_name_and_size_order_disagree();
    let before = app.get_display_order();
    assert_eq!(before.len(), 2);

    // A rescan replaces the file set (simulated the way the scan path does).
    app.all_files = vec![app.all_files[0].clone()];
    app.file_names = vec![app.file_names[0].clone()];
    app.file_display_names = vec![app.file_display_names[0].clone()];
    app.file_sizes = vec![100];
    app.file_dates = vec![0];
    app.file_is_dir = vec![false];
    app.selection = vec![true];
    app.scan_generation += 1;
    app.invalidate_sorted_cache();

    let after = app.get_display_order();
    assert_eq!(
        after.len(),
        1,
        "display order must cover exactly the current file set"
    );
    assert!(
        after.iter().all(|&i| i < app.all_files.len()),
        "every index must be in range"
    );
}

/// The Type sort uses the shared extension rule (normal + compound) and treats
/// folders as extensionless, so it lines up with the Type column's "Folder"
/// label instead of sorting a dotted folder by its pseudo-extension.
#[test]
fn test_type_sort_uses_shared_extension_rule() {
    let dir = TempDir::new().unwrap();
    let names = ["a.txt", "b.tar.gz", "c.zip"];
    for n in names {
        File::create(dir.path().join(n)).unwrap();
    }
    fs::create_dir(dir.path().join("d.folder")).unwrap();
    fs::create_dir(dir.path().join("a.folder")).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    // Index order: two folders, then three files.
    let order = ["d.folder", "a.folder", "a.txt", "b.tar.gz", "c.zip"];
    app.all_files = order.iter().map(|n| dir.path().join(n)).collect();
    app.file_display_names = order.iter().map(|n| n.to_string()).collect();
    app.file_sizes = vec![0; order.len()];
    app.file_dates = vec![0; order.len()];
    app.file_is_dir = vec![true, true, false, false, false];
    app.preview_sort_col = PreviewSortCol::Type;
    app.preview_sort_asc = true;

    let sorted = app.sorted_indices();
    let sorted_names: Vec<&str> = sorted
        .iter()
        .map(|&i| app.file_display_names[i].as_str())
        .collect();
    assert_eq!(
        sorted_names,
        vec!["d.folder", "a.folder", "b.tar.gz", "a.txt", "c.zip"],
        "folders first (extensionless, stable), then files by normal/compound extension"
    );
}
