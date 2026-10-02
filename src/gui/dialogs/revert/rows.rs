use super::*;

/// Draws the scrollable revert list: sortable columns, rows, box
/// selection, and edge-scroll while dragging.
pub(crate) fn draw_revert_rows(
    ui: &mut egui::Ui,
    app: &mut GuiApp,
    state: &mut RevertDialogState,
    active_idx: usize,
    pending_action: &mut Option<(usize, String)>,
) {
    use crate::gui::table_shared::{paint_row_bg, rect_hovered};
    let n_entries = state.entries.len();
    let display_order = &state.display_order;
    let mut row_rects: Vec<(usize, egui::Rect)> = Vec::new();
    let spacing = ui.style().spacing.item_spacing.x;
    let total_w = state.col_widths.iter().sum::<f32>()
        + spacing * (state.col_widths.len().saturating_sub(1) as f32);
    let mut scroll_area = ScrollArea::vertical()
        .id_salt("rev_rows_scroll")
        .scroll_source(ScrollSource {
            drag: DragScroll::Never,
            ..Default::default()
        })
        .auto_shrink([false; 2]);

    // Keyboard-triggered scroll for arrow-key navigation.
    if let Some(target_idx) = state.scroll_to_idx.take() {
        let target_top = target_idx as f32 * ROW_H;
        let target_bot = target_top + ROW_H;
        let margin = ROW_H * 2.0;

        let scroll_id = ui.make_persistent_id(egui::Id::new("rev_rows_scroll"));
        let cur_offset = ui
            .ctx()
            .data(|d| d.get_temp::<f32>(scroll_id))
            .unwrap_or(0.0);
        let viewport_h = ui.available_height();

        let is_first = target_idx == 0;
        let is_last = target_idx + 1 >= n_entries;

        if is_first {
            scroll_area = scroll_area.vertical_scroll_offset(0.0);
            state.scroll_sequence_start = None;
            state.scroll_last_event = None;
        } else if is_last {
            let max_offset = (n_entries as f32 * ROW_H - viewport_h).max(0.0);
            scroll_area = scroll_area.vertical_scroll_offset(max_offset);
            state.scroll_sequence_start = None;
            state.scroll_last_event = None;
        } else {
            let above_viewport = target_bot < cur_offset + margin;
            let below_viewport = target_top > cur_offset + viewport_h - margin;

            if above_viewport || below_viewport {
                let offset = target_top - viewport_h * 0.33;
                let max_offset = (n_entries as f32 * ROW_H - viewport_h).max(0.0);
                scroll_area = scroll_area.vertical_scroll_offset(offset.clamp(0.0, max_offset));
            }
            state.scroll_sequence_start = None;
            state.scroll_last_event = None;
        }
    }

    let rev_scroll_resp = scroll_area.show_rows(ui, ROW_H, n_entries, |ui, range| {
        ui.set_min_width(total_w);
        let visible_start = range.start;
        for (rel_i, &oi) in display_order[range].iter().enumerate() {
            let display_idx = visible_start + rel_i;
            let entry = &state.entries[oi];
            let sel = state.selection[oi];
            let ok = entry.revertable;
            let can_select = ok && entry.has_change_at(active_idx);

            let rb = if active_idx < entry.target_by_commit.len() {
                &entry.target_by_commit[active_idx]
            } else {
                &entry.current_name
            };
            let orig = entry
                .lineage
                .first()
                .map(|(_, n, _)| n.as_str())
                .unwrap_or_default();

            let (row_rect, resp) =
                ui.allocate_exact_size(Vec2::new(total_w, ROW_H), Sense::click_and_drag());
            row_rects.push((oi, row_rect));

            // Skip the hover highlight while a box-select
            // drag is active so rows under the pointer don't
            // flash hover color a frame before turning
            // selected (box selection applies at end of frame).
            let hovered = !sel && state.box_select_anchor.is_none() && rect_hovered(ui, row_rect);
            paint_row_bg(ui, row_rect, sel, display_idx, hovered);

            // ── Render cells ──
            let is_dir = entry.current_path.is_dir();
            let icon = if is_dir { "📁 " } else { "📄 " };
            let dir_color = Color32::from_rgb(0, 95, 140);
            let icon_color = if is_dir {
                dir_color
            } else {
                ui.style().visuals.text_color()
            };
            let rb_differs = rb != &entry.current_name;

            let mut row_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(row_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );

            for ci in 0..state.col_widths.len() {
                let w = state.col_widths[ci];

                let (cell_rect, _) =
                    row_ui.allocate_exact_size(egui::Vec2::new(w, ROW_H), egui::Sense::hover());
                let mut cell_ui = row_ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(cell_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );

                match ci {
                    0 => {
                        let color = if ok {
                            ui.style().visuals.text_color()
                        } else {
                            Color32::GRAY
                        };
                        if rb_differs {
                            let (orig_segs, _) = diff_colored(&entry.current_name, rb);
                            let mut all_segs = vec![DiffSeg {
                                text: icon.to_string(),
                                fg: Some(icon_color),
                                bg: Color32::TRANSPARENT,
                            }];
                            all_segs.extend(orig_segs);
                            let mut job = egui::text::LayoutJob::default();
                            let font_id = egui::FontId::proportional(FONT_LABEL);
                            for seg in &all_segs {
                                job.append(
                                    &seg.text,
                                    0.0,
                                    egui::TextFormat {
                                        font_id: font_id.clone(),
                                        color: seg.fg.unwrap_or(color),
                                        background: seg.bg,
                                        ..Default::default()
                                    },
                                );
                            }
                            cell_ui.add(egui::Label::new(job).truncate().selectable(false));
                        } else {
                            let name = entry.current_name.as_str();
                            let mut job = egui::text::LayoutJob::default();
                            let font_id = egui::FontId::proportional(FONT_LABEL);
                            job.append(
                                icon,
                                0.0,
                                egui::TextFormat {
                                    font_id: font_id.clone(),
                                    color: icon_color,
                                    ..Default::default()
                                },
                            );
                            job.append(
                                name,
                                0.0,
                                egui::TextFormat {
                                    font_id,
                                    color,
                                    ..Default::default()
                                },
                            );
                            cell_ui.add(egui::Label::new(job).truncate().selectable(false));
                        }
                    }
                    1 => {
                        if rb_differs {
                            let (_, new_segs) = diff_colored(&entry.current_name, rb);
                            let mut job = egui::text::LayoutJob::default();
                            let font_id = egui::FontId::proportional(FONT_LABEL);
                            for seg in &new_segs {
                                job.append(
                                    &seg.text,
                                    0.0,
                                    egui::TextFormat {
                                        font_id: font_id.clone(),
                                        color: seg.fg.unwrap_or(ui.style().visuals.text_color()),
                                        background: seg.bg,
                                        ..Default::default()
                                    },
                                );
                            }
                            cell_ui.add(egui::Label::new(job).truncate().selectable(false));
                        } else if entry.has_change_at(active_idx) {
                            // Effects-only rollback: the name is unchanged, only
                            // attributes/timestamps will be restored.
                            cell_ui.add(
                                egui::Label::new(RichText::new("(metadata)").color(Color32::GRAY))
                                    .truncate()
                                    .selectable(false),
                            );
                        } else {
                            cell_ui.add(
                                egui::Label::new(RichText::new(rb.as_str()).color(Color32::GRAY))
                                    .truncate()
                                    .selectable(false),
                            );
                        }
                    }
                    2 => {
                        cell_ui.add(
                            egui::Label::new(RichText::new(orig).color(Color32::GRAY))
                                .truncate()
                                .selectable(false),
                        );
                    }
                    3 => {
                        let attr = GuiApp::revert_attr_summary(entry, active_idx);
                        let attr = if attr.is_empty() {
                            "no change"
                        } else {
                            attr.as_str()
                        };
                        cell_ui.add(
                            egui::Label::new(RichText::new(attr).color(Color32::GRAY))
                                .truncate()
                                .selectable(false),
                        );
                    }
                    4 => {
                        let ts = GuiApp::revert_ts_summary(entry, active_idx);
                        let ts = if ts.is_empty() {
                            "no change"
                        } else {
                            ts.as_str()
                        };
                        cell_ui.add(
                            egui::Label::new(RichText::new(ts).color(Color32::GRAY))
                                .truncate()
                                .selectable(false),
                        );
                    }
                    _ => {
                        // Mirror preview table: empty until an operation runs.
                        let path_str = entry.current_path.to_string_lossy().to_string();
                        let (txt, color) = match app.revert_status_map.get(&path_str) {
                            Some((0, _)) => ("OK".to_string(), GREEN),
                            Some((1, msg)) => (
                                msg.as_deref().unwrap_or("Error").to_string(),
                                Color32::from_rgb(160, 0, 0),
                            ),
                            Some((2, _)) => ("Skip".to_string(), Color32::from_rgb(255, 176, 0)),
                            _ => (String::new(), Color32::GRAY),
                        };

                        cell_ui.add(
                            egui::Label::new(RichText::new(&txt).color(color))
                                .truncate()
                                .selectable(false),
                        );
                    }
                }
            }

            // Context menu
            let entry_path = entry.current_path.clone();
            let entry_name = entry.current_name.clone();
            resp.context_menu(|ui| {
                // Select this item first if not already selected.
                if !state.selection[oi] {
                    state.selection.iter_mut().for_each(|s| *s = false);
                    state.selection[oi] = true;
                    state.last_clicked_idx = Some(oi);
                    state.selection_anchor = None; // fresh single select
                }
                draw_file_context_menu(ui, oi, &entry_path, &entry_name, pending_action);
            });

            if resp.clicked() && can_select {
                let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
                let shift = ui.input(|i| i.modifiers.shift);
                // Ranges are computed in display order inside
                // apply_revert_selection (the table is sortable).
                apply_revert_selection(
                    &mut state.selection,
                    display_order,
                    &mut state.selection_anchor,
                    &mut state.last_clicked_idx,
                    oi,
                    ctrl,
                    shift,
                );
                state.cached_selected_count = None; // invalidate on click
                state.scroll_to_idx = Some(oi);
            }
        }
        ui.add_space(14.0);

        // ── Box selection ──
        let additive_box =
            ui.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd || i.modifiers.shift);
        let sel_before = state.selection.clone();
        let mut box_ctx = crate::gui::helpers::BoxSelectionCtx {
            visible_start,
            display_order,
            sel: &mut state.selection,
            anchor: &mut state.box_select_anchor,
        };
        // Skip while a context menu is open (or an action just
        // fired) so the click that dismisses the menu doesn't
        // also clear the selection — same guard as the preview
        // table.
        let empty_click_allowed =
            !ui.ctx().any_popup_open() && state.pending_context_action.is_none();
        handle_box_selection_by_display_order(
            ui,
            &row_rects,
            &mut box_ctx,
            empty_click_allowed,
            additive_box,
        );
        if state.selection != sel_before {
            state.cached_selected_count = None; // invalidate after box select
            // An empty-space click cleared everything — drop the
            // shift-click anchor so the next click starts a fresh
            // range instead of extending a stale one.
            if state.selection.iter().all(|&s| !s) {
                state.last_clicked_idx = None;
                state.selection_anchor = None;
            }
        }
    });

    // ── Edge-scroll (with acceleration) for box selection ──
    if ui.input(|i| i.pointer.primary_down()) && ui.input(|i| i.pointer.press_origin()).is_some() {
        let edge_px = 30.0;
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let clip = ui.clip_rect();
            let scroll_delta = if pos.y < clip.top() + edge_px && pos.y >= clip.top() - 10.0 {
                -1.0
            } else if pos.y > clip.bottom() - edge_px && pos.y <= clip.bottom() + 10.0 {
                1.0
            } else {
                0.0
            };

            if scroll_delta == 0.0 {
                state.edge_drag_start = None;
            } else {
                let viewport_h = clip.height();
                let max_offset = (rev_scroll_resp.content_size.y - viewport_h).max(0.0);
                let scroll_id = ui.make_persistent_id(egui::Id::new("rev_rows_scroll"));
                let current_offset = ui.ctx().memory_mut(|mem| {
                    let st = mem
                        .data
                        .get_temp_mut_or_default::<egui::scroll_area::State>(scroll_id);
                    st.offset.y
                });

                let at_boundary = (scroll_delta > 0.0 && current_offset >= max_offset)
                    || (scroll_delta < 0.0 && current_offset <= 0.0);

                if at_boundary {
                    state.edge_drag_start = None;
                } else {
                    let elapsed = state
                        .edge_drag_start
                        .map(|s| s.elapsed().as_secs_f32())
                        .unwrap_or(0.0);
                    if state.edge_drag_start.is_none() {
                        state.edge_drag_start = Some(std::time::Instant::now());
                    }

                    let initial_speed = 0.3;
                    let speed_rows = if elapsed < 0.4 {
                        initial_speed
                    } else {
                        (initial_speed + (elapsed - 0.4) * 0.8).min(5.0)
                    };

                    let new_offset =
                        (current_offset + speed_rows * ROW_H * scroll_delta).clamp(0.0, max_offset);

                    ui.ctx().memory_mut(|mem| {
                        let st = mem
                            .data
                            .get_temp_mut_or_default::<egui::scroll_area::State>(scroll_id);
                        st.offset.y = new_offset;
                    });
                    ui.ctx().request_repaint();
                }
            }
        }
    } else {
        state.edge_drag_start = None;
    }
}
