use super::*;

pub(crate) fn render_segs(ui: &mut egui::Ui, segs: &[DiffSeg], w: f32) {
    let mut job = egui::text::LayoutJob::default();
    let font_id = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Body)
        .cloned()
        .unwrap_or(egui::FontId::proportional(FONT_BODY));
    let default_fg = ui.style().visuals.text_color();
    for seg in segs {
        job.append(
            &seg.text,
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color: seg.fg.unwrap_or(default_fg),
                background: seg.bg,
                ..Default::default()
            },
        );
    }
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, ROW_H), egui::Sense::hover());
    let mut cu = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    cu.add(egui::Label::new(job).truncate().selectable(false));
}
