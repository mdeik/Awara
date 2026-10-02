use super::*;

/// Arrow-key / page-key navigation for the revert list (runs before the
/// dialog is drawn, so the selection moves even without a click).
pub(crate) fn handle_revert_keys(ctx: &egui::Context, state: &mut RevertDialogState) {
    let shift_down = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowDown));
    let shift_up = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowUp));
    let arrow_down = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
    let arrow_up = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
    let page_down = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown));
    let page_up = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp));
    let home_key = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Home));
    let end_key = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::End));

    if !state.entries.is_empty()
        && (shift_down
            || shift_up
            || arrow_down
            || arrow_up
            || page_down
            || page_up
            || home_key
            || end_key)
    {
        let display_order = &state.display_order;
        let cur_pos = state
            .last_clicked_idx
            .and_then(|a| display_order.iter().position(|&i| i == a))
            .unwrap_or(0);

        let target_pos = if shift_down || arrow_down {
            (cur_pos + 1).min(display_order.len().saturating_sub(1))
        } else if (shift_up || arrow_up) && cur_pos > 0 {
            cur_pos - 1
        } else if page_down {
            (cur_pos + 25).min(display_order.len().saturating_sub(1))
        } else if page_up {
            cur_pos.saturating_sub(25)
        } else if home_key {
            0
        } else if end_key {
            display_order.len().saturating_sub(1)
        } else {
            cur_pos
        };

        if target_pos != cur_pos {
            let target = display_order[target_pos];
            let shift = shift_down || shift_up;
            // Ranges are computed in display order inside apply_revert_selection.
            apply_revert_selection(
                &mut state.selection,
                display_order,
                &mut state.selection_anchor,
                &mut state.last_clicked_idx,
                target,
                false,
                shift,
            );
            state.cached_selected_count = None; // invalidate on selection change
            state.scroll_to_idx = Some(target);
            state
                .scroll_sequence_start
                .get_or_insert_with(std::time::Instant::now);
            state.scroll_last_event = Some(std::time::Instant::now());
        }
    }
}

/// Dialog-wide keyboard shortcuts: Escape closes, Ctrl+A selects all
/// multi-step entries, Ctrl+D clears the selection, Ctrl+I inverts it.
/// Returns true when the dialog should close (Escape).
pub(crate) fn handle_revert_shortcuts(ctx: &egui::Context, state: &mut RevertDialogState) -> bool {
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        return true;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::A)) {
        for (j, e) in state.entries.iter().enumerate() {
            if e.has_change_at(state.active_commit_idx) {
                state.selection[j] = true;
            }
        }
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::D)) {
        state.selection.fill(false);
        state.last_clicked_idx = None;
        state.selection_anchor = None;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::I)) {
        for j in 0..state.entries.len() {
            if state.entries[j].has_change_at(state.active_commit_idx) {
                state.selection[j] = !state.selection[j];
            }
        }
    }
    false
}

/// Runs the deferred right-click action (copy / open) and shows the
/// file-properties popup when requested.
pub(crate) fn handle_revert_context_action(ctx: &egui::Context, state: &mut RevertDialogState) {
    if let Some((idx, action)) = state.pending_context_action.take()
        && let Some(entry) = state.entries.get(idx)
    {
        match action.as_str() {
            "copy_filename" => {
                copy_text(&entry.current_name, ctx);
            }
            "copy_full_path" => {
                copy_text(&entry.current_path.to_string_lossy(), ctx);
            }
            "copy_dir_path" => {
                if let Some(parent) = entry.current_path.parent() {
                    copy_text(&parent.to_string_lossy(), ctx);
                }
            }
            "open_file" => {
                open_in_os(&entry.current_path);
            }
            "open_folder" => {
                open_parent_in_os(&entry.current_path);
            }
            "properties" => {
                state.show_properties_idx = Some(idx);
            }
            _ => {}
        }
    }

    // ── Properties popup ──
    if let Some(prop_idx) = state.show_properties_idx {
        if let Some(entry) = state.entries.get(prop_idx) {
            let path = entry.current_path.clone();
            let name = entry.current_name.clone();
            draw_properties_popup(ctx, "revert_properties", &path, &name, None, || {
                state.show_properties_idx = None
            });
        } else {
            // Entry vanished — close popup
            state.show_properties_idx = None;
        }
    }
}
