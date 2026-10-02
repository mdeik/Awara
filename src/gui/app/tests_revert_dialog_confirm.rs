use super::*;
use std::fs::File;
use tempfile::TempDir;

/// The dialog's target computation must roll back each entry of a
/// single-commit chain (a→b, b→c) ONE step (c→b, b→a) — not both to the
/// origin — so a batch revert resolves through the chain-safe executor.
#[test]
fn test_confirm_revert_resolves_chain_via_dialog() {
    let tmp = TempDir::new().unwrap();
    let a = tmp.path().join("a.txt");
    let b = tmp.path().join("b.txt");
    let c = tmp.path().join("c.txt");
    // Post-apply state of the forward chain a→b, b→c:
    std::fs::write(&b, b"content_a").unwrap();
    std::fs::write(&c, b"content_b").unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![b.clone(), c.clone()];
    app.file_names = vec![
        b.to_string_lossy().to_string(),
        c.to_string_lossy().to_string(),
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
            ops: vec![mk(&a, &b, "a.txt", "b.txt"), mk(&b, &c, "b.txt", "c.txt")],
        }],
    );

    app.build_revert_dialog_state();
    assert!(app.revert_dialog.is_some());
    let dialog = app.revert_dialog.as_mut().unwrap();
    assert_eq!(dialog.entries.len(), 2, "both chain files in the dialog");
    // Entries are in display (name) order: b.txt, c.txt
    assert_eq!(
        dialog.entries[0].target_by_commit[0], "a.txt",
        "b rolls back one step to a"
    );
    assert_eq!(
        dialog.entries[1].target_by_commit[0], "b.txt",
        "c rolls back one step to b, not the origin"
    );
    dialog.selection[0] = true;
    dialog.selection[1] = true;
    app.confirm_revert();

    assert_eq!(std::fs::read(&a).unwrap(), b"content_a", "a restored");
    assert_eq!(std::fs::read(&b).unwrap(), b"content_b", "b restored");
    assert!(!c.exists(), "c vacated");
    assert!(
        app.status_message.contains("2 reverted, 0 errors"),
        "status: {}",
        app.status_message
    );
}

/// A swap pair (a↔b) in one commit: the dialog cross-targets the entries
/// (a→b, b→a) so the revert resolves via temp deferral instead of both
/// being no-ops.
#[test]
fn test_confirm_revert_resolves_swap_via_dialog() {
    let tmp = TempDir::new().unwrap();
    let a = tmp.path().join("a.txt");
    let b = tmp.path().join("b.txt");
    // Post-apply state of the forward swap a→b, b→a:
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

    app.build_revert_dialog_state();
    assert!(app.revert_dialog.is_some());
    let dialog = app.revert_dialog.as_mut().unwrap();
    assert_eq!(dialog.entries.len(), 2, "both swap files in the dialog");
    // Insertion order (no sort configured): b.txt first, a.txt second —
    // targets cross
    assert_eq!(
        dialog.entries[0].target_by_commit[0], "a.txt",
        "b rolls back to a"
    );
    assert_eq!(
        dialog.entries[1].target_by_commit[0], "b.txt",
        "a rolls back to b"
    );
    dialog.selection[0] = true;
    dialog.selection[1] = true;
    app.confirm_revert();

    assert_eq!(std::fs::read(&a).unwrap(), b"content_a", "a restored");
    assert_eq!(std::fs::read(&b).unwrap(), b"content_b", "b restored");
    assert!(
        app.status_message.contains("2 reverted, 0 errors"),
        "status: {}",
        app.status_message
    );
}

/// Reverting a copy-mode entry deletes the copy (source preserved) through
/// the executor's copy branch.
#[test]
fn test_confirm_revert_copy_mode_deletes_copy() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("a.txt");
    let copy = tmp.path().join("b.txt");
    std::fs::write(&src, b"data").unwrap();
    std::fs::write(&copy, b"data").unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![copy.clone()];
    app.file_names = vec![copy.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];

    // forward_info is derived from the active commit's ops (new_path →
    // was_copy/perms).
    app.commits_by_dir.insert(
        tmp.path().to_path_buf(),
        vec![Commit {
            timestamp: std::time::Instant::now(),
            label: "Apply".into(),
            ops: vec![RenameOp {
                original_path: src.clone(),
                new_path: copy.clone(),
                original_name: "a.txt".into(),
                new_name: "b.txt".into(),
                was_copy: true,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0,
            }],
        }],
    );

    use crate::gui::types::{RevertDialogState, RevertEntry};
    app.revert_dialog = Some(RevertDialogState {
        entries: vec![RevertEntry {
            current_path: copy.clone(),
            current_name: "b.txt".into(),
            lineage: vec![
                (src.clone(), "a.txt".into(), None),
                (copy.clone(), "b.txt".into(), Some(0)),
            ],
            target_by_commit: vec!["a.txt".into()],
            effects_by_commit: vec![None],
            revertable: true,
            block_reason: None,
        }],
        selection: vec![true],
        active_commit_idx: 0,
        executing: false,
        col_widths: vec![100.0, 100.0, 100.0],
        pending_context_action: None,
        last_clicked_idx: None,
        selection_anchor: None,
        sort_col: 0,
        sort_asc: true,
        show_properties_idx: None,
        display_order: vec![0],
        display_order_dirty: false,
        sort_keys: vec!["b.txt".into()],
        scroll_to_idx: None,
        scroll_sequence_start: None,
        scroll_last_event: None,
        edge_drag_start: None,
        box_select_anchor: None,
        cached_selected_count: None,
    });

    app.confirm_revert();

    assert!(!copy.exists(), "copy deleted");
    assert!(src.exists(), "source intact");
    assert_eq!(app.all_files[0], src, "listing points at the source");
    assert!(
        app.status_message.contains("1 reverted, 0 errors"),
        "status: {}",
        app.status_message
    );
}

/// Multi-commit chain (a→b in c0, b→c in c1): target_by_commit must give
/// the per-commit rollback names — revert everything → a, revert only the
/// last commit → b.
#[test]
fn test_revert_dialog_targets_multi_commit_chain() {
    let tmp = TempDir::new().unwrap();
    let a = tmp.path().join("a.txt");
    let b = tmp.path().join("b.txt");
    let c = tmp.path().join("c.txt");
    std::fs::write(&c, b"data").unwrap(); // current state after both renames

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![c.clone()];
    app.file_names = vec![c.to_string_lossy().to_string()];
    app.file_sizes = vec![0];
    app.file_dates = vec![0];
    app.selection = vec![true];

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
        vec![
            Commit {
                timestamp: std::time::Instant::now(),
                label: "C0".into(),
                ops: vec![mk(&a, &b, "a.txt", "b.txt")],
            },
            Commit {
                timestamp: std::time::Instant::now(),
                label: "C1".into(),
                ops: vec![mk(&b, &c, "b.txt", "c.txt")],
            },
        ],
    );

    app.build_revert_dialog_state();
    let dialog = app.revert_dialog.as_mut().unwrap();
    assert_eq!(dialog.entries.len(), 1);
    assert_eq!(
        dialog.entries[0].target_by_commit,
        vec!["a.txt".to_string(), "b.txt".to_string()],
        "per-commit rollback names"
    );
    assert_eq!(dialog.active_commit_idx, 1, "active commit is the latest");
}

/// After a partial apply (some files error, dialog rebuilds), errors from the
/// completed batch must still be visible in revert_status_map — i.e. build_revert_dialog_state
/// must NOT clear revert_status_map when called from the apply path.
#[test]
fn test_revert_partial_apply_errors_persist_in_rebuilt_dialog() {
    let tmp = TempDir::new().unwrap();
    let file_a = tmp.path().join("alpha.txt");
    let file_b = tmp.path().join("beta.txt");
    File::create(&file_a).unwrap();
    File::create(&file_b).unwrap();

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = tmp.path().to_path_buf();
    app.all_files = vec![file_a.clone(), file_b.clone()];
    app.file_names = vec![
        file_a.to_string_lossy().to_string(),
        file_b.to_string_lossy().to_string(),
    ];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.selection = vec![true, true];

    // Rename both in a single batch: alpha.txt -> alpha_new.txt, beta.txt -> beta_new.txt
    app.config.add.add_suffix = Some("_new".into());
    app.section_enabled[SectionId::Add as usize] = true;
    app.preview_dirty = true;
    app.execute_renames();

    let alpha_new = tmp.path().join("alpha_new.txt");
    let beta_new = tmp.path().join("beta_new.txt");
    assert!(alpha_new.exists(), "alpha_new should exist");
    assert!(beta_new.exists(), "beta_new should exist");

    // Now both alpha_new and beta_new are in the table.
    // Block alpha_new's revert by pre-creating alpha.txt again.
    File::create(&file_a).unwrap(); // alpha.txt exists -> collision for alpha_new revert

    app.all_files = vec![alpha_new.clone(), beta_new.clone()];
    app.file_names = vec![
        alpha_new.to_string_lossy().to_string(),
        beta_new.to_string_lossy().to_string(),
    ];
    app.file_sizes = vec![0, 0];
    app.file_dates = vec![0, 0];
    app.selection = vec![true, true];

    // Open dialog fresh — revert_status_map should be cleared.
    app.revert_status_map.clear();
    app.build_revert_dialog_state();
    assert!(app.revert_dialog.is_some());

    // Select both entries and apply.
    {
        let dialog = app.revert_dialog.as_mut().unwrap();
        for s in dialog.selection.iter_mut() {
            *s = true;
        }
    }
    app.confirm_revert();

    // beta_new should have reverted OK; alpha_new should have errored.
    let alpha_new_str = alpha_new.to_string_lossy().to_string();
    let beta_str = file_b.to_string_lossy().to_string();

    // Error for alpha_new should be in revert_status_map.
    let alpha_status = app.revert_status_map.get(&alpha_new_str);
    assert!(
        alpha_status.is_some(),
        "alpha error should be in revert_status_map"
    );
    assert_eq!(
        alpha_status.unwrap().0,
        1u8,
        "alpha should be code 1 (error)"
    );

    // Now simulate the apply_requested rebuild (what dialogs.rs does).
    // build_revert_dialog_state must NOT clear revert_status_map here.
    app.build_revert_dialog_state();

    // Error for alpha_new must still be visible after the rebuild.
    let alpha_status_after = app.revert_status_map.get(&alpha_new_str);
    assert!(
        alpha_status_after.is_some(),
        "alpha error must survive build_revert_dialog_state during apply_requested rebuild"
    );
    assert_eq!(alpha_status_after.unwrap().0, 1u8);

    // OK for beta_new must also survive.
    let beta_ok = app.revert_status_map.get(&beta_str);
    assert!(
        beta_ok.is_some(),
        "beta OK must survive build_revert_dialog_state during apply_requested rebuild"
    );
    assert_eq!(beta_ok.unwrap().0, 0u8);
}
