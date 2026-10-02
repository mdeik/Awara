use super::*;

pub(super) fn draw_section_name(
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
        Name,
        "Name",
        |ui, _en| {
            egui::Grid::new("name_grid")
                .min_col_width(0.0)
                .show(ui, |ui| {
                    // Row 1: Name label | combo
                    ui.label(RichText::new("Name:").size(FONT_LABEL));
                    {
                        combo_opt_tip(
                            ui,
                            "name_mode",
                            &mut c.name.name_mode,
                            &[
                                ("Keep", "keep"),
                                ("Remove", "remove"),
                                ("Fixed", "fixed"),
                                ("Reverse", "reverse"),
                                ("Pad Numbers", "pad_numbers"),
                                ("Reformat Date", "reformat_date"),
                            ],
                            d,
                            "Set how the filename is handled: Keep (original), Remove (empty), Fixed (use Value), Reverse (reverse text), Pad Numbers (zero-pad digits), Reformat Date (parse and reformat dates)",
                        );
                    }
                    ui.end_row();
                    // Row 2: Value label | field (enabled for Fixed, Pad Numbers, Reformat Date)
                    ui.label(RichText::new("Value:").size(FONT_LABEL));
                    {
                        let mode = c.name.name_mode.as_deref().unwrap_or("keep");
                        let is_enabled = mode == "fixed" || mode == "pad_numbers" || mode == "reformat_date";
                        let mut buf = c.name.name_value.clone().unwrap_or_default();
                        let tip = match mode {
                            "fixed" => "Custom name value used when Name mode is set to 'Fixed'. Leave empty to clear the stem.",
                            "pad_numbers" => "Pad amount and optional character (e.g. '5' pads with zeros, '5>A' pads with A). Default: 0 (no padding).",
                            "reformat_date" => "Custom date format for reformatting (e.g. '%a-%b-%m-%Y' for day-month-year, or '%d+%m+%Y>%d-%m-%Y' to convert an unusual format).",
                            _ => "",
                        };
                        if fixed_text_enabled(ui, [138.0, 22.0], &mut buf, is_enabled)
                            .on_hover_text(tip)
                            .changed()
                        {
                            c.name.name_value = if buf.is_empty() { None } else { Some(buf) };
                            *d = true;
                        }
                    }
                    ui.end_row();
                });
        },
        {
            reset_section(c, SectionId::Name);
        }
    );
}
