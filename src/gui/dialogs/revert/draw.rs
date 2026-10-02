use super::*;

/// `pub(crate)` so `mod.rs` can re-export it for `windows.rs`.
pub(crate) fn draw_revert_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    let mut state = match app.revert_dialog.take() {
        Some(s) => s,
        None => return,
    };
    if state.entries.is_empty() {
        // Nothing to show — close the dialog
        app.revert_status_map.clear();
        app.revert_dialog = None;
        app.active_popup = None;
        return;
    }

    let mut close_requested = false;
    let mut apply_requested = false;
    // Local variable for deferred context action — avoids borrowing `state` inside egui closures.
    let mut pending_action: Option<(usize, String)> = None;

    // ── Revert dialog arrow-key + page-key navigation (before Modal) ──
    handle_revert_keys(ctx, &mut state);

    // Collect all commits across all directories for the sidebar.
    let all_commits: Vec<String> = app.all_commits().map(|c| c.label.clone()).collect();
    let mut active_idx = state.active_commit_idx;

    // Modal dimmer background
    egui::Area::new(egui::Id::new("revert_modal_dimmer"))
        .interactable(true)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let screen_rect = ctx.content_rect();
            ui.painter()
                .rect_filled(screen_rect, 0.0, egui::Color32::from_black_alpha(140));
        });

    let mut is_open = true;
    egui::Window::new("Revert Changes")
        .id(egui::Id::new("revert_popup"))
        .collapsible(false)
        .resizable(true)
        .movable(true)
        .default_size(egui::vec2(960.0, 560.0))
        .min_size(egui::vec2(600.0, 350.0))
        .default_pos(ctx.content_rect().center())
        .pivot(egui::Align2::CENTER_CENTER)
        .open(&mut is_open)
        .show(ctx, |ui| {
            // Absorb drag interactions in the window body so dragging a selection
            // box or empty space inside the table never moves the window.
            // Moving the window is reserved exclusively for dragging the title bar.
            ui.interact(
                ui.max_rect(),
                ui.id().with("revert_body_drag_absorber"),
                egui::Sense::click_and_drag(),
            );

            // Toolbar
            ui.horizontal(|ui| {
                // O(1) cached count — recompute only when cache is stale.
                let n = state
                    .cached_selected_count
                    .get_or_insert_with(|| state.selection.iter().filter(|&&s| s).count());
                ui.label(format!("Objects: {}", state.entries.len()));
                ui.separator();
                ui.label(format!("Selected: {}", n));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Done").clicked() {
                        close_requested = true;
                    }
                    ui.separator();
                    let all = state.selection.iter().all(|&s| s);
                    if ui
                        .button(if all { "Deselect All" } else { "Select All" })
                        .clicked()
                    {
                        let v = !all;
                        for (j, e) in state.entries.iter().enumerate() {
                            if e.has_change_at(active_idx) {
                                state.selection[j] = v;
                            }
                        }
                        state.cached_selected_count = None; // invalidate
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            state.selection.iter().any(|&s| s) && !state.executing,
                            egui::Button::new("Apply Revert"),
                        )
                        .clicked()
                    {
                        apply_requested = true;
                    }
                });
            });
            ui.separator();

            // Split: commit sidebar (right) + table (left)
            egui::Panel::right("revert_commit_panel")
                .resizable(true)
                .default_size(150.0)
                .min_size(100.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Commits").strong().size(FONT_LABEL));
                    ui.separator();
                    ScrollArea::vertical()
                        .scroll_source(ScrollSource {
                            drag: DragScroll::Never,
                            ..Default::default()
                        })
                        .id_salt("rev_commit")
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            // Show newest commits at top (reverse iteration)
                            let dark_mode = ui.style().visuals.dark_mode;
                            for (di, label) in all_commits.iter().rev().enumerate() {
                                let ci = all_commits.len() - 1 - di;
                                let is_active = active_idx == ci;

                                // Reserve space and paint selection background first, so it
                                // appears behind the label text.
                                let available = ui.available_size();
                                let label = egui::Label::new(RichText::new(format!("{} ", label)));
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(available.x, ui.style().spacing.interact_size.y),
                                    egui::Sense::click(),
                                );

                                if is_active {
                                    let bg = crate::gui::types::selection_bg(dark_mode);
                                    let mut bg_rect = rect;
                                    bg_rect.max.x = ui.max_rect().max.x;
                                    ui.painter().rect_filled(bg_rect, 0.0, bg);
                                }

                                let resp = ui.put(rect, label.sense(egui::Sense::click()));
                                if resp.clicked() {
                                    active_idx = ci;
                                }
                            }
                        });
                });

            // Table
            egui::CentralPanel::default()
                .frame(egui::Frame::new())
                .show(ui, |ui| {
                    ui.style_mut().spacing.item_spacing.y = 0.0;
                    let spacing = ui.style().spacing.item_spacing.x;

                    // ── Headers with drag-to-resize handles ──
                    ui.horizontal(|ui| {
                        ui.set_min_height(HEADER_H);
                        let header_labels = [
                            "Current Name",
                            "Rollback To",
                            "Original Name",
                            "Attributes",
                            "Timestamps",
                            "Status",
                        ];
                        for (ci, label) in header_labels.iter().enumerate() {
                            if ci >= state.col_widths.len() {
                                break;
                            }
                            let result = draw_table_header_cell(
                                ui,
                                label,
                                state.col_widths[ci],
                                state.sort_col == ci,
                                state.sort_asc,
                                "rev",
                                ci,
                            );
                            if result.resize_delta != 0.0 {
                                state.col_widths[ci] =
                                    (state.col_widths[ci] + result.resize_delta).max(COL_MIN_WIDTH);
                            }
                            if result.drag_stopped {
                                app.revert_col_widths = state.col_widths.clone();
                                app.save_settings();
                            }
                            if result.clicked {
                                if state.sort_col == ci {
                                    state.sort_asc = !state.sort_asc;
                                } else {
                                    state.sort_col = ci;
                                    state.sort_asc = true;
                                }
                                state.display_order_dirty = true;
                            }
                        }
                    });

                    // Separator below header — match total_w accounting for gaps between cells
                    let sep_w = state.col_widths.iter().sum::<f32>()
                        + spacing * state.col_widths.len().saturating_sub(1) as f32;
                    draw_header_separator(ui, sep_w);

                    // ── Data rows (scrollable, virtualized) ──
                    // Recompute sort keys and display order when sort or active commit changes.
                    let commit_changed = active_idx != state.active_commit_idx;
                    if commit_changed {
                        state.display_order_dirty = true;
                    }
                    if state.display_order_dirty {
                        state.sort_keys = state
                            .entries
                            .iter()
                            .map(|e| match state.sort_col {
                                0 => e.current_name.clone(),
                                1 => {
                                    if active_idx < e.target_by_commit.len() {
                                        e.target_by_commit[active_idx].clone()
                                    } else {
                                        e.current_name.clone()
                                    }
                                }
                                2 => e
                                    .lineage
                                    .first()
                                    .map(|(_, n, _)| n.clone())
                                    .unwrap_or_default(),
                                3 => GuiApp::revert_attr_summary(e, active_idx),
                                4 => GuiApp::revert_ts_summary(e, active_idx),
                                _ => {
                                    // Sort by operation result mirroring preview table.
                                    let path_str = e.current_path.to_string_lossy().to_string();
                                    // We can't borrow app here during sort_keys rebuild,
                                    // so use an empty string (blank = not yet run).
                                    // The rendered cell reads live from app.status_map.
                                    path_str
                                }
                            })
                            .collect();
                        state.display_order = (0..state.entries.len()).collect();
                        state.display_order.sort_by(|&a, &b| {
                            let ka = &state.sort_keys[a];
                            let kb = &state.sort_keys[b];
                            if state.sort_asc {
                                ka.cmp(kb)
                            } else {
                                kb.cmp(ka)
                            }
                        });
                        state.display_order_dirty = false;
                    }
                    draw_revert_rows(ui, app, &mut state, active_idx, &mut pending_action);
                }); // close the table panel
        });

    if !is_open {
        close_requested = true;
    }

    // ── Keyboard shortcuts: Escape, Ctrl+A, Ctrl+D, Ctrl+I ──
    if handle_revert_shortcuts(ctx, &mut state) {
        close_requested = true;
    }

    state.active_commit_idx = active_idx;
    state.pending_context_action = pending_action;

    // ── Process deferred context action (copy / open) ──
    handle_revert_context_action(ctx, &mut state);

    // Keep the runtime copy of the revert-dialog widths in sync so they can be
    // persisted even when the dialog is closed later.
    app.revert_col_widths = state.col_widths.clone();

    if apply_requested {
        // Execute renames, then rebuild dialog to show updated state
        app.revert_dialog = Some(state);
        app.confirm_revert();
        app.build_revert_dialog_state();
        // After a revert, pre-select all remaining revertable entries
        if let Some(new_state) = &mut app.revert_dialog {
            let active = new_state.active_commit_idx;
            for (j, e) in new_state.entries.iter().enumerate() {
                if e.has_change_at(active) {
                    new_state.selection[j] = true;
                }
            }
        }
    } else if close_requested {
        app.revert_status_map.clear();
        app.revert_dialog = None;
        app.active_popup = None;
    } else {
        app.revert_dialog = Some(state);
    }
}
