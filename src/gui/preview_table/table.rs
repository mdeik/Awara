use super::deferred::apply_deferred_actions;
use super::header::draw_table_header;
use super::rows::{DrawRowsCtx, draw_rows, handle_reorder_drag};
use super::*;

/// Draws the file/preview table: header, virtualised rows, drag-to-reorder,
/// and the deferred per-frame actions.
pub(crate) fn draw_file_and_preview(app: &mut GuiApp, ui: &mut egui::Ui) {
    const SCROLL_RIGHT_PAD: f32 = 24.0;
    if app.cached_rows.is_empty() && !app.all_files.is_empty() && !app.rows_cache_pending {
        app.rebuild_rows_cache();
    }

    let preview_rect = ui.clip_rect();
    let order = app.get_display_order();
    let col_n = COL_IDS.len();
    if app.layout.col_order.len() != col_n {
        app.layout.col_order = (0..col_n).collect();
    }

    let mut sort_request: Option<PreviewSortCol> = None;
    let mut dir_to_enter: Option<String> = None;
    let mut row_clicked_this_frame = false;

    let spacing_x = ui.style().spacing.item_spacing.x;
    let total_col_width: f32 = app
        .layout
        .col_order
        .iter()
        .map(|&ci| app.layout.col_widths[ci])
        .sum::<f32>()
        + spacing_x * (app.layout.col_order.len().saturating_sub(1) as f32);

    let any_drag = ui.ctx().dragged_id().is_some();
    let press_in_preview = ui
        .input(|i| i.pointer.press_origin())
        .is_none_or(|origin| preview_rect.contains(origin));
    let has_modal_or_popup = app.has_any_modal();

    let h_scroll_resp = ScrollArea::horizontal()
        .id_salt("file_preview_h_scroll")
        .auto_shrink([false; 2])
        .scroll_source(ScrollSource {
            drag: DragScroll::Never,
            ..Default::default()
        })
        .show(ui, |ui| {
            ui.set_min_width((total_col_width + SCROLL_RIGHT_PAD).max(300.0));

            draw_table_header(ui, app, total_col_width, &mut sort_request);
            // Zero out item_spacing so show_rows and the autoscroll logic both
            // use ROW_H for coordinate calculations, matching visual rendering.
            ui.style_mut().spacing.item_spacing.y = 0.0;

            // ── Virtualised data rows ──
            let mut row_rects: Vec<(usize, egui::Rect)> = Vec::new();

            // Build the scroll area, applying keyboard-triggered scroll offset
            // before show_rows computes the visible range.
            let mut scroll_area = ScrollArea::vertical()
                .id_salt("file_preview_v_scroll")
                .scroll_source(ScrollSource {
                    drag: DragScroll::Never,
                    ..Default::default()
                })
                .auto_shrink([false; 2]);

            // show_rows uses ROW_H + item_spacing.y for its internal coordinate
            // system.  Since we zeroed it above, row_h == ROW_H here.
            if let Some(target_idx) = app.scroll_to_idx.take() {
                if let Some(pos) = order.iter().position(|&i| i == target_idx) {
                    let spacing_y = ui.style().spacing.item_spacing.y;
                    let row_h = ROW_H + spacing_y;
                    let viewport_h = ui.available_height();
                    let target_top = pos as f32 * row_h;
                    let target_bot = target_top + row_h;

                    // Read current offset to check visibility.
                    let scroll_id = ui.make_persistent_id(egui::Id::new("file_preview_v_scroll"));
                    let cur_offset = ui.ctx().memory_mut(|mem| {
                        let state = mem
                            .data
                            .get_temp_mut_or_default::<egui::scroll_area::State>(scroll_id);
                        state.offset.y
                    });

                    // Binary margin: quick taps scroll minimally; holding (>0.4s)
                    // triggers aggressive scroll via a huge margin.
                    let hold_secs = app
                        .scroll_sequence_start
                        .map(|s| s.elapsed().as_secs_f32())
                        .unwrap_or(0.0);
                    let extra_margin = if hold_secs > 0.4 { 5000.0 } else { 0.0 };
                    let margin = row_h * (1.0 + extra_margin);

                    let above_viewport = target_bot < cur_offset + margin;
                    let below_viewport = target_top > cur_offset + viewport_h - margin;

                    if above_viewport || below_viewport {
                        let offset = target_top - viewport_h * 0.33;
                        let max_offset =
                            (app.cached_rows.len() as f32 * row_h - viewport_h).max(0.0);
                        let offset = offset.clamp(0.0, max_offset);
                        scroll_area = scroll_area.vertical_scroll_offset(offset);
                    }
                }
            } else {
                // No scroll event this frame.  If the last key press was >500ms ago,
                // the user released the key — reset both timers.
                if let Some(last) = app.scroll_last_event
                    && last.elapsed().as_secs_f32() > 0.5
                {
                    app.scroll_sequence_start = None;
                    app.scroll_last_event = None;
                }
            }

            let scroll_resp =
                scroll_area.show_rows(ui, ROW_H, app.cached_rows.len(), |ui, range| {
                    row_rects = draw_rows(
                        ui,
                        app,
                        DrawRowsCtx {
                            order: &order,
                            total_col_width,
                            scroll_right_pad: SCROLL_RIGHT_PAD,
                            range,
                            press_in_preview,
                            any_drag,
                            has_modal_or_popup,
                            row_clicked_this_frame: &mut row_clicked_this_frame,
                            dir_to_enter: &mut dir_to_enter,
                        },
                    );
                });
            // Edge scroll for box selection
            handle_edge_scroll(
                ui,
                app,
                press_in_preview,
                any_drag,
                scroll_resp.content_size.y,
                ui.make_persistent_id(egui::Id::new("file_preview_v_scroll")),
            );

            handle_reorder_drag(ui, app, &row_rects, total_col_width);

            scroll_resp
        });

    apply_deferred_actions(
        ui,
        app,
        row_clicked_this_frame,
        h_scroll_resp.inner.inner_rect,
        &mut sort_request,
        dir_to_enter,
    );
}
