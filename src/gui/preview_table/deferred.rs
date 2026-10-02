use super::*;

/// Handles actions that run after the table is drawn.
pub(super) fn apply_deferred_actions(
    ui: &mut egui::Ui,
    app: &mut GuiApp,
    row_clicked_this_frame: bool,
    h_scroll_inner_rect: egui::Rect,
    sort_request: &mut Option<PreviewSortCol>,
    dir_to_enter: Option<String>,
) {
    // ── Apply deferred mutations ──
    app.apply_context_action();
    if let Some(col) = sort_request.take() {
        app.sort_preview_by(col);
        // Sort preference is always persisted (like column sizing).
        app.save_settings();
    }

    // ── Selection-change side effects ──
    // Does numbering depend on which items are selected? (SSoT: GuiApp helper,
    // which also respects the Numbering section's enabled state.)
    let has_numbering = app.numbering_active();

    let selection_changed = app.selection_generation != app.last_seen_selection_gen;
    if selection_changed {
        app.last_seen_selection_gen = app.selection_generation;
        app.cached_selected_count.set(None);

        let now = std::time::Instant::now();
        if defer_selection_dispatch(app.selection_dragging, app.last_selection_dispatch, now) {
            // Active box-select drag: coalesce the background rebuild so a full
            // rebuild isn't dispatched on every mouse-move frame. The change is
            // guaranteed to be dispatched by the flush below once the drag ends
            // or the throttle window passes.
            app.selection_dispatch_pending = true;
            let remaining = SELECTION_DISPATCH_THROTTLE
                - now.saturating_duration_since(app.last_selection_dispatch.unwrap());
            ui.ctx().request_repaint_after(remaining);
        } else {
            app.selection_dispatch_pending = false;
            app.last_selection_dispatch = Some(now);
            if has_numbering {
                // With numbering, the new name of every item changes based on
                // which items are selected (the numbering sequence shifts), so
                // the full preview must be recomputed.
                app.preview_dirty = true;
            } else {
                // Without numbering, the rename for each item is independent of
                // selection — the preview's existing `op.new_name` is still
                // correct. Only rows whose selection state flipped need to be
                // rebuilt; the background thread carries the rest over
                // (selection-only incremental build).
                app.rebuild_rows_cache_selection_only();
            }
        }
    }

    // Flush a deferred selection change once the drag ends (or the throttle
    // window passes even while still dragging). Runs every frame, so a release
    // without a further selection bump still dispatches the final state.
    if should_flush_selection_dispatch(
        app.selection_dispatch_pending,
        app.selection_dragging,
        app.last_selection_dispatch,
        std::time::Instant::now(),
    ) {
        app.selection_dispatch_pending = false;
        app.last_selection_dispatch = Some(std::time::Instant::now());
        if has_numbering {
            app.preview_dirty = true;
            // The preview dispatch happens in `update()` on the *next* frame —
            // make sure one is scheduled so the numbering preview reflects the
            // final selection.
            ui.ctx().request_repaint();
        } else {
            app.rebuild_rows_cache_selection_only();
        }
    }

    if let Some(dir_name) = dir_to_enter {
        let path = app.cwd.join(&dir_name);
        if path.is_dir() {
            app.navigating_to_dir = true;
            app.enter_dir(&path);
        }
    }

    // ── Click on empty space → deselect / commit edit ──
    // Skip if a context menu action was just triggered this frame — the click
    // that closes the menu should not also clear the selection.
    if !row_clicked_this_frame
        && !app.context_menu_action_taken
        && ui.input(|i| i.pointer.any_click())
        && !app.has_any_modal()
        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
    {
        // Don't process clicks inside a context menu / popup.
        let on_popup = ui
            .ctx()
            .layer_id_at(pos)
            .is_some_and(|lid| lid.order >= egui::Order::Foreground);
        if on_popup {
            // click is on a popup — let the popup handle it
        } else if h_scroll_inner_rect.contains(pos) {
            let on_edit = app.editing_row_rect.is_some_and(|r| r.contains(pos));
            if !on_edit && app.editing_idx.is_some() {
                app.commit_edit();
            }
            app.preview_active = true;
            app.selection.fill(false);
            app.selection_generation += 1;
            app.last_clicked_idx = None;
            app.selection_anchor = None;
        } else {
            if app.editing_idx.is_some() {
                app.commit_edit();
            }
            app.preview_active = false;
        }
    }
}
