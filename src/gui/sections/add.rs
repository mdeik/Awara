use super::*;

pub(super) fn draw_section_add(
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
        Add,
        "Add",
        |ui, _en| {
            egui::Grid::new("add_grid")
                .min_col_width(0.0)
                .show(ui, |ui| {
                    // Row 1: Prefix label | field
                    ui.label(RichText::new("Prefix:").size(FONT_LABEL));
                    {
                        let mut buf = c.add.add_prefix.clone().unwrap_or_default();
                        if fixed_text(ui, [135.0, 22.0], &mut buf)
                            .on_hover_text("Add text at the beginning of the filename")
                            .changed()
                        {
                            c.add.add_prefix = if buf.is_empty() { None } else { Some(buf) };
                            *d = true;
                        }
                    }
                    ui.end_row();
                    // Row 2: Insert label | field
                    ui.label(RichText::new("Insert:").size(FONT_LABEL));
                    {
                        let mut buf = c.add.add_insert.clone().unwrap_or_default();
                        if fixed_text(ui, [135.0, 22.0], &mut buf)
                            .on_hover_text("Text to insert at a specific position in the filename")
                            .changed()
                        {
                            c.add.add_insert = if buf.is_empty() { None } else { Some(buf) };
                            *d = true;
                        }
                    }
                    ui.end_row();
                    // Row 3: at label | DragValue
                    ui.label(RichText::new("at:").size(FONT_LABEL));
                    {
                        let mut v = c.add.add_at.unwrap_or(0);
                        if ui
                            .add_sized(
                                egui::vec2(44.0, 22.0),
                                int_drag(&mut v)
                                    .range(ADD_AT.0..=ADD_AT.1),
                            )
                            .on_hover_text(
                                "Character position to insert at (0 = start of filename; negative counts from the end)",
                            )
                            .changed()
                        {
                            // 0 is the default insert position; it is
                            // stored as-is and treated as unset by change
                            // detection (awara::opt_num_default).
                            c.add.add_at = Some(v);
                            *d = true;
                        }
                    }
                    ui.end_row();
                    // Row 4: Suffix label | field
                    ui.label(RichText::new("Suffix:").size(FONT_LABEL));
                    {
                        let mut buf = c.add.add_suffix.clone().unwrap_or_default();
                        if fixed_text(ui, [135.0, 22.0], &mut buf)
                            .on_hover_text("Add text at the end of the filename (before the extension)")
                            .changed()
                        {
                            c.add.add_suffix = if buf.is_empty() { None } else { Some(buf) };
                            *d = true;
                        }
                    }
                    ui.end_row();
                });
            ui.horizontal(|ui| {
                cb_tip(
                    ui,
                    d,
                    &mut c.add.add_word_space,
                    "Word Space",
                    "Add spaces between words in CamelCase (e.g. MyFile -> My File)",
                );
            });
        },
        {
            reset_section(c, SectionId::Add);
        }
    );
}
