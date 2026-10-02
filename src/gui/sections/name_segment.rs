use super::*;

pub(super) fn draw_section_name_segment(
    ui: &mut egui::Ui,
    en: &mut SectionEnabled,
    d: &mut bool,
    sec_order: &dyn Fn(SectionId) -> String,
    c: &mut RenameConfig,
) {
    sec_group!(
        ui,
        en,
        d,
        sec_order,
        c,
        NameSegment,
        "Name Segment",
        |ui, _en| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("From:").size(FONT_LABEL));
                let prev_from = c.name_segment.copy_name_segment_from;
                let mut from = prev_from;
                if ui
                    .add_sized(
                        egui::vec2(44.0, 22.0),
                        int_drag(&mut from)
                            .range(NAME_SEGMENT.0..=NAME_SEGMENT.1),
                    )
                    .on_hover_text("Start character index (1-based) to extract. E.g. 1 = first character. Negative counts from end (-1 = last).")
                    .changed()
                {
                    // Same From/To linkage as Remove: increasing
                    // From while To equals the old From drags To
                    // along; decreasing never changes To.
                    linked_from_to_values(
                        prev_from,
                        from,
                        &mut c.name_segment.copy_name_segment_to,
                    );
                    c.name_segment.copy_name_segment_from = from;
                    *d = true;
                }
                ui.label(RichText::new("to:").size(FONT_LABEL));
                if ui.add_sized(
                    egui::vec2(44.0, 22.0),
                    int_drag(&mut c.name_segment.copy_name_segment_to)
                        .range(NAME_SEGMENT.0..=NAME_SEGMENT.1),
                )
                .on_hover_text("End character index (1-based, inclusive) for the name segment. E.g. From:1 To:5 = first 5 chars. Negative counts from end.")
                .changed()
                {
                    *d = true;
                }
            });
        },
        {
            reset_section(c, SectionId::NameSegment);
        }
    );
}
