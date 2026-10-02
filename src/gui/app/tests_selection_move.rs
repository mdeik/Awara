use super::tests_util::*;
use super::*;

#[test]
fn test_shift_click_anchor_is_first_click_not_last() {
    // The shift-click anchor must be the FIRST click of the current
    // selection, not the most recent click. Regression test: previously
    // each shift-click re-anchored on the last click, so a second
    // shift-click extended the range from the wrong end.
    use crate::gui::preview_table::apply_row_click;

    let (mut app, _dir) = selection_test_app(&["a.txt", "b.txt", "c.txt", "d.txt"]);
    app.selection.fill(false);
    app.file_is_dir = vec![false; 4];
    app.file_display_names = vec![
        "a.txt".into(),
        "b.txt".into(),
        "c.txt".into(),
        "d.txt".into(),
    ];

    // Click b → anchor not yet pinned, single selection.
    apply_row_click(&mut app, 1, false, false);
    assert_eq!(app.selection, vec![false, true, false, false]);
    assert_eq!(app.selection_anchor, None);

    // Shift-click d → range from b (pinned anchor) to d.
    apply_row_click(&mut app, 3, false, true);
    assert_eq!(app.selection, vec![false, true, true, true]);
    assert_eq!(app.selection_anchor, Some(1));

    // Shift-click c → range must still extend from b, NOT from d
    // (the previously clicked item).
    apply_row_click(&mut app, 2, false, true);
    assert_eq!(app.selection, vec![false, true, true, false]);
    assert_eq!(app.selection_anchor, Some(1));

    // A plain click resets the anchor; the next shift-click extends from it.
    apply_row_click(&mut app, 3, false, false);
    assert_eq!(app.selection_anchor, None);
    apply_row_click(&mut app, 0, false, true); // shift-click a
    assert_eq!(app.selection, vec![true, true, true, true]); // a..d
    assert_eq!(app.selection_anchor, Some(3));
}

#[test]
fn test_shift_click_anchor_first_click_no_prior_selection() {
    // First action is a shift-click with nothing selected: toggles like a
    // plain click and pins the anchor for the next shift-click.
    use crate::gui::preview_table::apply_row_click;

    let (mut app, _dir) = selection_test_app(&["a.txt", "b.txt", "c.txt"]);
    app.selection.fill(false);
    app.file_is_dir = vec![false; 3];
    app.file_display_names = vec!["a.txt".into(), "b.txt".into(), "c.txt".into()];

    apply_row_click(&mut app, 2, false, true); // shift-click c
    assert_eq!(app.selection, vec![false, false, true]);

    apply_row_click(&mut app, 0, false, true); // shift-click a
    assert_eq!(app.selection, vec![true, true, true]);
}
#[test]
fn test_custom_order_initially_none() {
    let app = GuiApp::new_with_config_for_test();
    assert!(
        app.custom_order.is_none(),
        "custom_order should start as None"
    );
    assert!(app.pending_context_action.is_none());
    assert!(app.show_properties_idx.is_none());
}

#[test]
fn test_get_display_order_equals_sorted_indices_by_default() {
    let mut app = GuiApp::new_with_config_for_test();
    let display = app.get_display_order();
    let sorted = app.sorted_indices();
    assert_eq!(display, sorted, "default order should match sorted");
}

#[test]
fn test_sorted_indices_returns_custom_order_when_set() {
    let mut app = GuiApp::new_with_config_for_test();
    // A manual order only covers the file set it was built from, so give the
    // app a matching file list.
    app.all_files = (0..6).map(|i| PathBuf::from(format!("{i}.txt"))).collect();
    app.file_display_names = (0..6).map(|i| format!("{i}.txt")).collect();
    let custom = vec![5, 4, 3, 2, 1, 0];
    app.custom_order = Some(custom.clone());
    assert_eq!(app.sorted_indices(), std::rc::Rc::new(custom));
}

#[test]
fn test_stale_custom_order_is_dropped() {
    // A manual reposition is a permutation of the file list it was built from.
    // If the list shrinks (filter, trash, rescan) the stored order would index
    // out of range, so it must be discarded rather than used.
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")];
    app.file_display_names = vec!["a.txt".to_string(), "b.txt".to_string()];
    app.custom_order = Some(vec![5, 4, 3]);

    let order = app.sorted_indices();
    assert!(
        order.iter().all(|&i| i < app.all_files.len()),
        "order must stay in range of the current file set"
    );
    assert!(
        app.custom_order.is_none(),
        "a stale manual order must be discarded"
    );
}

#[test]
fn test_sort_preview_by_clears_custom_order() {
    let mut app = GuiApp::new_with_config_for_test();
    app.custom_order = Some(vec![5, 4, 3, 2, 1, 0]);
    assert!(app.custom_order.is_some());
    app.sort_preview_by(PreviewSortCol::Size);
    assert!(
        app.custom_order.is_none(),
        "sort_preview_by should clear custom_order"
    );
}

#[test]
fn test_reset_custom_order_clears() {
    let mut app = GuiApp::new_with_config_for_test();
    app.custom_order = Some(vec![2, 1, 0]);
    app.reset_custom_order();
    assert!(app.custom_order.is_none());
}

#[test]
fn test_move_selected_up_on_first_item_no_op() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![true, false, false]; // first selected
    app.custom_order = None;

    app.move_selected_up(); // should be a no-op (already at top)
    assert!(
        app.custom_order.is_none(),
        "no-op should not set custom_order"
    );
}

#[test]
fn test_move_selected_down_on_last_item_no_op() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![false, false, true]; // last selected
    app.custom_order = None;

    app.move_selected_down(); // should be a no-op (already at bottom)
    assert!(
        app.custom_order.is_none(),
        "no-op should not set custom_order"
    );
}

#[test]
fn test_move_selected_up_swaps_with_previous() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.file_sizes = vec![1, 2, 3];
    app.selection = vec![false, true, false]; // second selected
    app.custom_order = None;

    app.move_selected_up();
    let order = app.custom_order.expect("custom_order should be set");
    // file idx 1 (b.txt) should now be at display position 0
    assert_eq!(order[0], 1, "b.txt should move up");
    assert_eq!(order[1], 0, "a.txt should move down");
    assert_eq!(order[2], 2, "c.txt should stay");
}

#[test]
fn test_move_selected_down_swaps_with_next() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![false, true, false]; // second selected
    app.custom_order = None;

    app.move_selected_down();
    let order = app.custom_order.expect("custom_order should be set");
    assert_eq!(order[0], 0, "a.txt stays");
    assert_eq!(order[1], 2, "c.txt moves up");
    assert_eq!(order[2], 1, "b.txt moves down");
}

#[test]
fn test_move_selected_to_top() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![false, false, true]; // third selected
    app.custom_order = None;

    app.move_selected_to_top();
    let order = app.custom_order.expect("custom_order should be set");
    assert_eq!(order[0], 2, "c.txt should be at top");
    assert_eq!(order[1], 0, "a.txt second");
    assert_eq!(order[2], 1, "b.txt third");
}

#[test]
fn test_move_selected_to_bottom() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![true, false, false]; // first selected
    app.custom_order = None;

    app.move_selected_to_bottom();
    let order = app.custom_order.expect("custom_order should be set");
    assert_eq!(order[0], 1, "b.txt should be at top");
    assert_eq!(order[1], 2, "c.txt second");
    assert_eq!(order[2], 0, "a.txt should be at bottom");
}

#[test]
fn test_move_selected_up_no_selection_no_op() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")];
    app.selection = vec![false, false]; // nothing selected
    app.custom_order = None;

    app.move_selected_up();
    assert!(app.custom_order.is_none(), "no-op with no selection");
}

#[test]
fn test_apply_context_action_properties_multi() {
    let mut app = GuiApp::new_with_config_for_test();
    assert!(app.show_properties_multi_data.is_none());
    app.all_files = vec![
        PathBuf::from("/tmp/foo.txt"),
        PathBuf::from("/tmp/bar.txt"),
        PathBuf::from("/tmp/baz.txt"),
    ];
    app.file_sizes = vec![100, 200, 300];
    app.pending_context_action = Some(ContextAction::Properties);
    app.pending_sel_indices = vec![0, 2];
    app.apply_context_action();
    let data = app
        .show_properties_multi_data
        .expect("properties data should be set");
    assert_eq!(data.count, 2);
    assert_eq!(data.total_size, 400);
    assert_eq!(data.names, vec!["foo.txt", "baz.txt"]);
    assert!(
        app.pending_context_action.is_none(),
        "action should be consumed"
    );
}

#[test]
fn test_apply_context_action_reset_order() {
    let mut app = GuiApp::new_with_config_for_test();
    app.custom_order = Some(vec![2, 1, 0]);
    app.pending_context_action = Some(ContextAction::ResetOrder);
    app.apply_context_action();
    assert!(app.custom_order.is_none());
}

#[test]
fn test_move_selected_up_block_contiguous() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
        PathBuf::from("d.txt"),
        PathBuf::from("e.txt"),
    ];
    app.selection = vec![false, true, true, true, false];
    app.custom_order = None;

    app.move_selected_up();
    let order = app.custom_order.expect("custom_order should be set");
    assert_eq!(order[0], 1, "b should be at top");
    assert_eq!(order[1], 2, "c second");
    assert_eq!(order[2], 3, "d third");
    assert_eq!(order[3], 0, "a moved below block");
    assert_eq!(order[4], 4, "e stays last");
}

#[test]
fn test_move_selected_up_block_already_at_top_no_op() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![true, true, false];
    app.custom_order = None;

    app.move_selected_up();
    assert!(app.custom_order.is_none(), "no-op at top");
}

#[test]
fn test_move_selected_down_block_contiguous() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
        PathBuf::from("d.txt"),
        PathBuf::from("e.txt"),
    ];
    app.selection = vec![false, true, true, true, false];
    app.custom_order = None;

    app.move_selected_down();
    let order = app.custom_order.expect("custom_order should be set");
    assert_eq!(order[0], 0, "a stays first");
    assert_eq!(order[1], 4, "e moved above block");
    assert_eq!(order[2], 1, "b now at position 2");
    assert_eq!(order[3], 2, "c at position 3");
    assert_eq!(order[4], 3, "d at position 4");
}

#[test]
fn test_move_selected_down_block_at_bottom_no_op() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![false, true, true];
    app.custom_order = None;

    app.move_selected_down();
    assert!(app.custom_order.is_none(), "no-op at bottom");
}

#[test]
fn test_move_selected_to_top_group() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
        PathBuf::from("d.txt"),
        PathBuf::from("e.txt"),
    ];
    // All selected items move to top, preserving their relative order.
    app.selection = vec![false, false, true, false, true];
    app.custom_order = None;

    app.move_selected_to_top();
    let order = app.custom_order.expect("custom_order should be set");
    assert_eq!(order[0], 2, "c first (selected)");
    assert_eq!(order[1], 4, "e second (selected)");
    assert_eq!(order[2], 0, "a third");
    assert_eq!(order[3], 1, "b fourth");
    assert_eq!(order[4], 3, "d fifth");
}

#[test]
fn test_move_selected_to_bottom_group() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
        PathBuf::from("d.txt"),
        PathBuf::from("e.txt"),
    ];
    // All selected items move to bottom, preserving their relative order.
    app.selection = vec![true, false, false, true, false];
    app.custom_order = None;

    app.move_selected_to_bottom();
    let order = app.custom_order.expect("custom_order should be set");
    assert_eq!(order[0], 1, "b first");
    assert_eq!(order[1], 2, "c second");
    assert_eq!(order[2], 4, "e third");
    assert_eq!(order[3], 0, "a fourth (selected)");
    assert_eq!(order[4], 3, "d fifth (selected)");
}

#[test]
fn test_apply_context_action_move_up_down_to_top_bottom() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![false, true, false];
    app.custom_order = None;

    app.pending_context_action = Some(ContextAction::MoveUp);
    app.apply_context_action();
    let order = app.custom_order.expect("MoveUp should set order");
    assert_eq!(order[0], 1, "b.txt moved up via MoveUp");

    app.custom_order = None;
    app.pending_context_action = Some(ContextAction::MoveDown);
    app.apply_context_action();
    let order = app.custom_order.expect("MoveDown should set order");
    assert_eq!(order[2], 1, "b.txt moved down via MoveDown");

    app.custom_order = None;
    app.pending_context_action = Some(ContextAction::MoveToTop);
    app.apply_context_action();
    let order = app.custom_order.expect("MoveToTop should set order");
    assert_eq!(order[0], 1, "b.txt moved to top via MoveToTop");

    app.custom_order = None;
    app.pending_context_action = Some(ContextAction::MoveToBottom);
    app.apply_context_action();
    let order = app.custom_order.expect("MoveToBottom should set order");
    assert_eq!(order[2], 1, "b.txt moved to bottom via MoveToBottom");
}

#[test]
fn test_apply_context_action_undo_redo_skips_missing() {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = vec![
        PathBuf::from("a.txt"),
        PathBuf::from("b.txt"),
        PathBuf::from("c.txt"),
    ];
    app.selection = vec![true, true, true];

    app.pending_context_action = Some(ContextAction::UndoFiles);
    app.apply_context_action();
    assert!(app.pending_context_action.is_none());

    app.pending_context_action = Some(ContextAction::RedoFiles);
    app.apply_context_action();
    assert!(app.pending_context_action.is_none());
}
