use super::*;

/// The Case Name combo: `(label, value)` in combo order, `None` being "Same".
const CASE_MODES: [(&str, Option<CaseMode>); 7] = [
    ("Same", None),
    ("Upper", Some(CaseMode::Upper)),
    ("Lower", Some(CaseMode::Lower)),
    ("Title", Some(CaseMode::Title)),
    ("Title Enhanced", Some(CaseMode::TitleEnhanced)),
    ("Sentence", Some(CaseMode::Sentence)),
    ("Invert", Some(CaseMode::Invert)),
];

pub(super) fn draw_section_case(
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
        Case,
        "Case",
        |ui, _en| {
            egui::Grid::new("case_grid")
                .min_col_width(0.0)
                .show(ui, |ui| {
                    // Row 1: Name label | combo
                    ui.label(RichText::new("Name:").size(FONT_LABEL));
                    combo_enum_tip(ui, "cname", &mut c.case.case_name, &CASE_MODES, d, "Case conversion for the filename part: lower, upper, title, sentence, or invert");
                    ui.end_row();
                    // Row 2: Exception label | text field
                    ui.label(RichText::new("Excep.:").size(FONT_LABEL));
                    {
                        let excep_enabled = c.case.case_name.is_some();
                        let mut buf = c.case.case_exception.clone().unwrap_or_default();
                        let resp = fixed_text_enabled(ui, [143.0, 22.0], &mut buf, excep_enabled)
                            .on_hover_text("Word to preserve as-is during case conversion (e.g. 'iOS' stays 'iOS')");
                        if excep_enabled && resp.changed() {
                            c.case.case_exception = if buf.is_empty() { None } else { Some(buf) };
                            *d = true;
                        } else if !excep_enabled {
                            // Ensure field is cleared when disabled (name selector = Same)
                            c.case.case_exception = None;
                        }
                    }
                    ui.end_row();
                });
        },
        {
            reset_section(c, SectionId::Case);
        }
    );
}
