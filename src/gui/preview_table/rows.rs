use super::*;

/// Per-frame viewport and interaction state for drawing the preview rows.
pub(super) struct DrawRowsCtx<'a> {
    pub order: &'a [usize],
    pub total_col_width: f32,
    pub scroll_right_pad: f32,
    pub range: std::ops::Range<usize>,
    pub press_in_preview: bool,
    pub any_drag: bool,
    pub has_modal_or_popup: bool,
    pub row_clicked_this_frame: &'a mut bool,
    pub dir_to_enter: &'a mut Option<String>,
}

/// Draws the visible rows and handles clicks, drags, and box selection.
pub(super) fn draw_rows(
    ui: &mut egui::Ui,
    app: &mut GuiApp,
    ctx: DrawRowsCtx<'_>,
) -> Vec<(usize, egui::Rect)> {
    let mut row_rects: Vec<(usize, egui::Rect)> = Vec::new();
    let visible_start = ctx.range.start;
    ui.set_min_width((ctx.total_col_width + ctx.scroll_right_pad).max(300.0));

    let pointer_over_popup = ui
        .input(|i| i.pointer.interact_pos())
        .and_then(|pos| ui.ctx().layer_id_at(pos))
        .is_some_and(|lid| lid.order >= egui::Order::Foreground);

    // Extract per-frame state to avoid nested borrow conflicts with app.
    let editing_idx = app.editing_idx;
    let edit_pending_focus = Cell::new(app.edit_pending_focus);

    // Clone the Rc (cheap) so `row` borrows from local scope, not `app`.
    // This avoids borrow conflicts with context_menu and click handlers.
    let cached_rows = app.cached_rows.clone();

    let range_start = ctx.range.start;
    for (rel_i, &oi) in ctx.order[ctx.range].iter().enumerate() {
        let row = &cached_rows[oi];
        let idx = row.idx;
        let display_idx = range_start + rel_i;
        let is_sel = app.selection.get(idx).copied().unwrap_or(false);
        let col_order = app.layout.col_order.clone();
        let col_widths = app.layout.col_widths.clone();

        let (row_rect, row_resp) = ui.allocate_exact_size(
            egui::vec2(ctx.total_col_width, ROW_H),
            Sense::click_and_drag(),
        );
        row_rects.push((idx, row_rect));

        // Background: selection / hover.
        // While a box-selection drag is active, skip the hover
        // highlight: the box updates the selection at end of
        // frame, so rows under the pointer would otherwise
        // flash hover color a frame before turning selected.
        let popup_active = app.active_popup;
        let hovered = popup_active.is_none()
            && !pointer_over_popup
            && ctx.press_in_preview
            && !ctx.any_drag
            && app.preview_box_select_anchor.is_none()
            && rect_hovered(ui, row_rect);
        paint_row_bg(ui, row_rect, is_sel, display_idx, hovered);

        // Context menu (takes &mut app — separate from immutable borrows).
        row_resp.context_menu(|ui| {
            // Prevent the empty-space click handler from clearing
            // the selection at end-of-frame — this right-click
            // should not be treated as an empty-space click.
            *ctx.row_clicked_this_frame = true;
            // Right-click on an unselected item: select only it.
            // Right-click on an already-selected item: preserve multi-selection.
            if idx < app.selection.len() && !app.selection[idx] {
                app.selection.iter_mut().for_each(|s| *s = false);
                app.selection[idx] = true;
                app.last_clicked_idx = Some(idx);
                app.selection_anchor = None; // fresh single select → new anchor
                app.selection_generation += 1;
                // Right-click selects a new file → re-arm.
                app.set_processed(false);
            }
            draw_preview_context_menu(ui, app, idx);
        });

        // Render cells
        let editing_this = editing_idx == Some(idx);
        if editing_this {
            app.editing_row_rect = Some(row_rect);
        }

        // Render cells inside horizontal layout constrained to the row rect.
        let mut cell_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );

        // Pre-compute the strikethrough stroke for missing-file rows.
        // Width 1.0, colored to match dimmed text so it doesn't feel harsh.
        let strike = if row.missing {
            let base = cell_ui.style().visuals.text_color();
            egui::Stroke::new(1.0, base.gamma_multiply(0.45))
        } else {
            egui::Stroke::NONE
        };
        // Multiplier applied to every text color for missing rows.
        let missing_dim = if row.missing { 0.45 } else { 1.0 };

        for &col_idx in &col_order {
            let col_id = COL_IDS[col_idx];
            let w = col_widths[col_idx];

            match col_id {
                PreviewSortCol::Name if editing_this => {
                    draw_editing_cell(
                        &mut cell_ui,
                        idx,
                        w,
                        row.is_dir,
                        &mut app.edit_buffer,
                        &edit_pending_focus,
                    );
                }
                PreviewSortCol::Name => {
                    let icon = if row.is_dir { "📁" } else { "📄" };
                    let text_color = cell_ui.style().visuals.text_color().gamma_multiply(missing_dim);
                    let dir_color = Color32::from_rgb(0, 95, 140).gamma_multiply(missing_dim);
                    let icon_color = if row.is_dir { dir_color } else { text_color };

                    // Build a single LayoutJob: icon seg + name content.
                    // This ensures the transition from non-diff to diff never
                    // shifts the text — always the same font, same allocation.
                    let (rect, _) =
                        cell_ui.allocate_exact_size(egui::vec2(w, ROW_H), egui::Sense::hover());
                    let mut cu = cell_ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );

                    // Preview diffs are only shown for selected, non-missing rows;
                    // deselected rows and missing rows render their plain original name.
                    if is_sel && row.changed && !row.missing {
                        let mut all_segs = vec![DiffSeg {
                            text: format!("{} ", icon),
                            fg: Some(icon_color),
                            bg: Color32::TRANSPARENT,
                        }];
                        let orig_segs = match &row.orig_segs {
                            Some(s) => s.clone(),
                            None => diff_colored(&row.name, &row.new_name).0,
                        };
                        all_segs.extend(orig_segs);

                        let mut job = egui::text::LayoutJob::default();
                        let font_id = cu
                            .style()
                            .text_styles
                            .get(&egui::TextStyle::Body)
                            .cloned()
                            .unwrap_or(egui::FontId::proportional(FONT_BODY));
                        for seg in all_segs {
                            job.append(
                                &seg.text,
                                0.0,
                                egui::TextFormat {
                                    font_id: font_id.clone(),
                                    color: seg.fg.unwrap_or(text_color),
                                    background: seg.bg,
                                    strikethrough: strike,
                                    ..Default::default()
                                },
                            );
                        }
                        cu.add(egui::Label::new(job).truncate().selectable(false));
                    } else if row.is_dir {
                        // Two-tone: blue icon, normal name
                        let mut job = egui::text::LayoutJob::default();
                        let font_id = cu
                            .style()
                            .text_styles
                            .get(&egui::TextStyle::Body)
                            .cloned()
                            .unwrap_or(egui::FontId::proportional(FONT_BODY));
                        job.append(
                            &format!("{} ", icon),
                            0.0,
                            egui::TextFormat {
                                font_id: font_id.clone(),
                                color: dir_color,
                                strikethrough: strike,
                                ..Default::default()
                            },
                        );
                        job.append(
                            &row.name,
                            0.0,
                            egui::TextFormat {
                                font_id,
                                color: text_color,
                                strikethrough: strike,
                                ..Default::default()
                            },
                        );
                        cu.add(egui::Label::new(job).truncate().selectable(false));
                    } else {
                        let mut job = egui::text::LayoutJob::default();
                        let font_id = cu
                            .style()
                            .text_styles
                            .get(&egui::TextStyle::Body)
                            .cloned()
                            .unwrap_or(egui::FontId::proportional(FONT_BODY));
                        job.append(
                            &format!("{} {}", icon, row.name),
                            0.0,
                            egui::TextFormat {
                                font_id,
                                color: text_color,
                                strikethrough: strike,
                                ..Default::default()
                            },
                        );
                        cu.add(egui::Label::new(job).truncate().selectable(false));
                    }
                }
                PreviewSortCol::Size => {
                    let text_color = cell_ui.style().visuals.text_color().gamma_multiply(0.6 * missing_dim);
                    let txt = if row.is_dir {
                        String::new()
                    } else {
                        human_size(row.size)
                    };
                    draw_label_cell(&mut cell_ui, w, &txt, text_color);
                }
                PreviewSortCol::Date => {
                    let text_color = cell_ui.style().visuals.text_color().gamma_multiply(0.6 * missing_dim);
                    let txt = if row.date > 0 {
                        format_date(row.date)
                    } else {
                        String::new()
                    };
                    draw_label_cell(&mut cell_ui, w, &txt, text_color);
                }
                PreviewSortCol::NewName => {
                    // Missing rows always show the plain current name — a rename
                    // preview for a deleted file is meaningless, and we apply
                    // the same strikethrough style as the Name column.
                    if is_sel && row.changed && !row.missing {
                        let new_segs_vec = match &row.new_segs {
                            Some(s) => s.clone(),
                            None => diff_colored(&row.name, &row.new_name).1,
                        };
                        render_segs(&mut cell_ui, &new_segs_vec, w);
                    } else {
                        let label = if is_sel && !row.missing { &row.new_name } else { &row.name };
                        let text_color = cell_ui.style().visuals.text_color();
                        let is_dark = cell_ui.style().visuals.dark_mode;
                        let base_color = if is_sel || !is_dark {
                            text_color
                        } else {
                            text_color.gamma_multiply(0.4)
                        };
                        let color = base_color.gamma_multiply(missing_dim);
                        if row.missing {
                            // Render with strikethrough — draw_label_cell doesn't
                            // support it, so build the LayoutJob directly.
                            let (rect, _) = cell_ui
                                .allocate_exact_size(egui::vec2(w, ROW_H), egui::Sense::hover());
                            let mut cu = cell_ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(rect)
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                            );
                            let font_id = cu
                                .style()
                                .text_styles
                                .get(&egui::TextStyle::Body)
                                .cloned()
                                .unwrap_or(egui::FontId::proportional(FONT_BODY));
                            let mut job = egui::text::LayoutJob::default();
                            job.append(
                                label,
                                0.0,
                                egui::TextFormat {
                                    font_id,
                                    color,
                                    strikethrough: strike,
                                    ..Default::default()
                                },
                            );
                            cu.add(egui::Label::new(job).truncate().selectable(false));
                        } else {
                            draw_label_cell(&mut cell_ui, w, label, color);
                        }
                    }
                }
                PreviewSortCol::Type => {
                    let text_color = cell_ui.style().visuals.text_color().gamma_multiply(0.6 * missing_dim);
                    // SSoT with the rename pipeline (normal + compound
                    // extensions) and allocation-free: only the extension text
                    // is needed, so borrow it rather than splitting the name.
                    let label = if row.is_dir {
                        "Folder"
                    } else {
                        awara::extension_str(&row.name).unwrap_or("File")
                    };
                    draw_label_cell(&mut cell_ui, w, label, text_color);
                }
                PreviewSortCol::Status => {
                    let (txt, color) = if row.missing {
                        ("Missing".to_string(), Color32::from_rgb(180, 100, 0).gamma_multiply(missing_dim))
                    } else {
                        match row.status {
                            Some(0) => ("OK".to_string(), GREEN),
                            Some(1) => {
                                let err = row.error_msg.as_deref().unwrap_or("Error");
                                (err.to_string(), Color32::from_rgb(160, 0, 0))
                            }
                            Some(2) => ("Skip".to_string(), Color32::from_rgb(255, 176, 0)),
                            _ => (String::new(), Color32::GRAY),
                        }
                    };
                    let (rect, _) =
                        cell_ui.allocate_exact_size(egui::vec2(w, ROW_H), egui::Sense::hover());
                    let mut cu = cell_ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    cu.add(
                        egui::Label::new(egui::RichText::new(&txt).color(color))
                            .truncate()
                            .selectable(false),
                    );
                }
            }
        }

        // Row click interaction (primary button only — right-click is context menu).
        // Skip if a context menu is open or an action is pending.
        if row_resp.clicked_by(egui::PointerButton::Primary)
            && idx < app.selection.len()
            && !app.has_any_modal()
            && !ui.ctx().any_popup_open()
            && app.pending_context_action.is_none()
            && ctx.press_in_preview
        {
            handle_row_click(app, idx, ctx.row_clicked_this_frame, ui);
        }

        // Drag-to-reorder start — preserve selection if dragging already-selected item.
        if row_resp.drag_started()
            && idx < app.selection.len()
            && !app.has_any_modal()
            && !ui.ctx().any_popup_open()
            && app.pending_context_action.is_none()
            && ctx.press_in_preview
        {
            app.reorder_drag_idx = Some(idx);
            app.last_clicked_idx = Some(idx);
            *ctx.row_clicked_this_frame = true;
            // If the dragged item is not selected, select only it.
            if !app.selection[idx] {
                for i in 0..app.selection.len() {
                    app.selection[i] = i == idx;
                }
                app.selection_anchor = None; // fresh single select → new anchor
                app.selection_generation += 1;
                // Drag-selecting a new file → re-arm.
                app.set_processed(false);
            }
        }

        // Double-click on directory → enter
        if row_resp.double_clicked() && row.is_dir && !app.has_any_modal() {
            *ctx.dir_to_enter = Some(row.name.clone());
        }
    }

    // Write back Cell-wrapped state.
    app.edit_pending_focus = edit_pending_focus.into_inner();

    // Box selection
    let additive_box = ui.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd || i.modifiers.shift);
    let mut box_ctx = crate::gui::helpers::BoxSelectionCtx {
        visible_start,
        display_order: ctx.order,
        sel: &mut app.selection,
        anchor: &mut app.preview_box_select_anchor,
    };
    let box_result = handle_box_selection_by_display_order(
        ui,
        &row_rects,
        &mut box_ctx,
        ctx.press_in_preview
            && !ctx.any_drag
            && !ui.ctx().any_popup_open()
            && !app.context_menu_action_taken
            && !ctx.has_modal_or_popup,
        additive_box,
    );
    app.selection_dragging = box_result;
    if box_result {
        app.selection_generation += 1;
        app.preview_active = true;
    }
    row_rects
}

/// Draws the drop indicator while dragging, then applies the reorder on release.
pub(super) fn handle_reorder_drag(
    ui: &mut egui::Ui,
    app: &mut GuiApp,
    row_rects: &[(usize, egui::Rect)],
    total_col_width: f32,
) {
    if let Some(dragged_idx) = app.reorder_drag_idx {
        let display_order = app.get_display_order();
        if !row_rects.is_empty() {
            let first_file = row_rects[0].0;
            let range_start = display_order
                .iter()
                .position(|&i| i == first_file)
                .unwrap_or(0);

            if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                if ui.input(|i| i.pointer.primary_down()) {
                    let target = compute_drop_target(pos.y, row_rects, range_start);
                    let drop_y = if target <= range_start {
                        row_rects[0].1.top()
                    } else if target >= range_start + row_rects.len() {
                        row_rects.last().unwrap().1.bottom()
                    } else {
                        row_rects[target - range_start].1.top()
                    };
                    draw_drop_indicator(ui, drop_y, total_col_width);
                    ui.ctx().request_repaint();
                } else {
                    let target =
                        compute_drop_target(pos.y, row_rects, range_start).min(display_order.len());
                    app.move_group_to_display_pos(dragged_idx, target);
                    app.reorder_drag_idx = None;
                }
            } else if !ui.input(|i| i.pointer.primary_down()) {
                app.reorder_drag_idx = None;
            }
        } else if !ui.input(|i| i.pointer.primary_down()) {
            app.reorder_drag_idx = None;
        }
    }
}
