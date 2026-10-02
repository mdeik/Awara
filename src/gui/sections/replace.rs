use super::*;

pub(super) fn draw_section_replace(
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
        Replace,
        "Replace",
        |ui, _en| {
            egui::Grid::new("replace_grid")
                .min_col_width(0.0)
                .show(ui, |ui| {
                    // Row 1: Replace label | field
                    ui.label(RichText::new("Replace:").size(FONT_LABEL));
                    {
                        let mut buf = c.replace.replace.clone().unwrap_or_default();
                        if fixed_text(ui, [135.0, 22.0], &mut buf)
                            .on_hover_text("String to find and replace in filenames")
                            .changed()
                        {
                            c.replace.replace = if buf.is_empty() { None } else { Some(buf) };
                            *d = true;
                        }
                    }
                    ui.end_row();
                    // Row 2: With label | field
                    ui.label(RichText::new("With:").size(FONT_LABEL));
                    {
                        let mut buf = c.replace.with.clone().unwrap_or_default();
                        if fixed_text(ui, [135.0, 22.0], &mut buf)
                            .on_hover_text("Replacement string")
                            .changed()
                        {
                            c.replace.with = if buf.is_empty() { None } else { Some(buf) };
                            *d = true;
                        }
                    }
                    ui.end_row();
                });
            ui.horizontal(|ui| {
                cb_tip(
                    ui,
                    d,
                    &mut c.replace.replace_case_sensitive,
                    "Match Case",
                    "Enable case-sensitive matching for the replace operation",
                );
                cb_tip(
                    ui,
                    d,
                    &mut c.replace.replace_first,
                    "First",
                    "Replace only the first occurrence of the search string",
                );
            });
        },
        {
            reset_section(c, SectionId::Replace);
        }
    );
}
