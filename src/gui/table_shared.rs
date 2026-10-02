use crate::gui::types::selection_bg;
use eframe::egui::{self, Color32};

/// Shared table separator below the header row.
pub(crate) fn draw_header_separator(ui: &mut egui::Ui, width: f32) {
    let sep_color = ui.style().visuals.widgets.noninteractive.bg_stroke.color;
    let sr = ui.available_rect_before_wrap();
    let sy = sr.top();
    ui.painter().line_segment(
        [egui::pos2(sr.left(), sy), egui::pos2(sr.left() + width, sy)],
        egui::Stroke::new(2.0_f32, sep_color),
    );
    ui.add_space(4.0);
}

/// Paint row background: selection colour, zebra striping, and hover highlight.
pub(crate) fn paint_row_bg(
    ui: &egui::Ui,
    rect: egui::Rect,
    selected: bool,
    row_idx: usize,
    hovered: bool,
) {
    if selected {
        let sel_color = selection_bg(ui.style().visuals.dark_mode);
        ui.painter().rect_filled(rect, 0.0, sel_color);
    } else if row_idx % 2 == 1 {
        let mut bg = ui.style().visuals.panel_fill;
        if ui.style().visuals.dark_mode {
            bg = Color32::from_rgb(
                bg.r().saturating_sub(8),
                bg.g().saturating_sub(8),
                bg.b().saturating_sub(6),
            );
        } else {
            bg = Color32::from_rgb(
                bg.r().saturating_add(8),
                bg.g().saturating_add(8),
                bg.b().saturating_add(6),
            );
        }
        ui.painter().rect_filled(rect, 0.0, bg);
    }
    if !selected && hovered {
        ui.painter()
            .rect_filled(rect, 0.0, ui.style().visuals.widgets.hovered.bg_fill);
    }
}

/// Helper: check if the cursor interacts with the given rect (for hover detection).
pub(crate) fn rect_hovered(ui: &egui::Ui, rect: egui::Rect) -> bool {
    rect.contains(ui.input(|i| i.pointer.interact_pos()).unwrap_or_default())
}
