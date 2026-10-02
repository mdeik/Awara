use super::*;

pub(super) fn draw_section_regex(
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
        Regex,
        "RegEx",
        |ui, _en| {
            egui::Grid::new("regex_grid")
                .min_col_width(0.0)
                .show(ui, |ui| {
                    // Row 1: Match label | field
                    ui.label(RichText::new("Match:").size(FONT_LABEL));
                    {
                        let mut buf = c.regex.regex_match.clone().unwrap_or_default();
                        if fixed_text(ui, [135.0, 22.0], &mut buf)
                            .on_hover_text(
                                "Regex pattern to match and replace in filenames (case-sensitive; use (?i) for case-insensitive)",
                            )
                            .changed()
                        {
                            c.regex.regex_match = if buf.is_empty() { None } else { Some(buf) };
                            *d = true;
                        }
                    }
                    ui.end_row();
                    // Row 2: With label | field
                    ui.label(RichText::new("With:").size(FONT_LABEL));
                    {
                        let mut buf = c.regex.regex_replace.clone().unwrap_or_default();
                        if fixed_text(ui, [135.0, 22.0], &mut buf)
                            .on_hover_text(
                                "Replacement string — use $1, $2, etc. for capture groups",
                            )
                            .changed()
                        {
                            c.regex.regex_replace = if buf.is_empty() { None } else { Some(buf) };
                            *d = true;
                        }
                    }
                    ui.end_row();
                });
            ui.horizontal(|ui| {
                cb_tip(
                    ui,
                    d,
                    &mut c.regex.regex_full_name,
                    "Inc. Ext.",
                    "Apply regex to the full filename including extension",
                );
                cb_tip(
                    ui,
                    d,
                    &mut c.regex.regex_simple,
                    "Simple",
                    "Simplified regex mode — no need to escape special characters",
                );
            });
        },
        {
            reset_section(c, SectionId::Regex);
        }
    );
}
