use super::*;

/// Draws the clickable/sortable column-header row for the preview table.
pub(super) fn draw_table_header(
    ui: &mut egui::Ui,
    app: &mut GuiApp,
    total_col_width: f32,
    sort_request: &mut Option<PreviewSortCol>,
) {
    // ── Header ──
    ui.horizontal(|ui| {
        ui.set_min_height(HEADER_H);
        for &col_idx in app.layout.col_order.iter() {
            let col_id = COL_IDS[col_idx];
            let label = COL_LABELS[col_idx];
            let result = draw_table_header_cell(
                ui,
                label,
                app.layout.col_widths[col_idx],
                app.preview_sort_col == col_id,
                app.preview_sort_asc,
                "prev",
                col_idx,
            );
            if result.resize_delta != 0.0 {
                app.layout.col_widths[col_idx] =
                    (app.layout.col_widths[col_idx] + result.resize_delta).max(COL_MIN_WIDTH);
            }
            if result.drag_stopped {
                app.save_settings();
            }
            if result.clicked {
                *sort_request = Some(col_id);
            }
        }
    });

    draw_header_separator(ui, total_col_width);
}
