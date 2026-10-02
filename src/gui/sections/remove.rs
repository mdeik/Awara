use super::*;

pub(super) fn draw_section_remove(
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
        Remove,
        "Remove",
        |ui, _en| {
            // Rows 1-3: 4-column grid — labels in own cols
            egui::Grid::new("remove_grid")
                .min_col_width(0.0)
                .show(ui, |ui| {
                    // Row 1: First label | field | Last label | field
                    drag_label_tip(ui, "First:", &mut c.remove.remove_first, d, 999, "Remove first N characters from the filename");
                    drag_field_tip(ui, &mut c.remove.remove_first, d, 999, "Remove first N characters from the filename");
                    drag_label_tip(ui, "Last:", &mut c.remove.remove_last, d, 999, "Remove last N characters from the filename");
                    drag_field_tip(ui, &mut c.remove.remove_last, d, 999, "Remove last N characters from the filename");
                    ui.end_row();

                    // Row 2: From label | field | To label | field
                    drag_label_tip(ui, "From:", &mut c.remove.remove_from, d, 999, "Remove from character position N (use with Remove To)");
                    drag_from_field_tip(ui, &mut c.remove.remove_from, &mut c.remove.remove_to, d, 999, "Remove from character position N (use with Remove To)");
                    drag_label_tip(ui, "To:", &mut c.remove.remove_to, d, 999, "Remove up to character position N (use with Remove From); 0 removes to the end of the name");
                    drag_field_tip(ui, &mut c.remove.remove_to, d, 999, "Remove up to character position N (use with Remove From); 0 removes to the end of the name");
                    ui.end_row();

                    // Row 3: Chars label | field | Words label | field
                    ui.label(RichText::new("Chars:").size(FONT_LABEL));
                    {
                        let mut rc_buf = c.remove.remove_chars.clone().unwrap_or_default();
                        if fixed_text(ui, [44.0, 20.0], &mut rc_buf)
                            .on_hover_text("Remove specific characters from the filename")
                            .changed()
                        {
                            c.remove.remove_chars = if rc_buf.is_empty() {
                                None
                            } else {
                                Some(rc_buf)
                            };
                            *d = true;
                        }
                    }
                    ui.label(RichText::new("Words:").size(FONT_LABEL));
                    {
                        let mut rw_buf = c.remove.remove_words.clone().unwrap_or_default();
                        if fixed_text(ui, [44.0, 20.0], &mut rw_buf)
                            .on_hover_text("Remove specific words from the filename")
                            .changed()
                        {
                            c.remove.remove_words = if rw_buf.is_empty() {
                                None
                            } else {
                                Some(rw_buf)
                            };
                            *d = true;
                        }
                    }
                    ui.end_row();

                });

            // Row 5: Crop in its own 3-column grid
            egui::Grid::new("remove_crop_grid")
                .min_col_width(0.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Crop:").size(FONT_LABEL));
                    {
                        const CROP_MODES: [(&str, Option<CropMode>); 4] = [
                            ("None", None),
                            ("Before", Some(CropMode::Before)),
                            ("After", Some(CropMode::After)),
                            ("Special", Some(CropMode::Special)),
                        ];
                        combo_enum_tip(
                            ui,
                            "crop",
                            &mut c.remove.crop_mode,
                            &CROP_MODES,
                            d,
                            "Crop text before, after, or keep only a matching substring",
                        );
                    }
                    {
                        let mut rc_crop = c.remove.remove_crop.clone().unwrap_or_default();
                        if fixed_text(ui, [80.0, 20.0], &mut rc_crop)
                            .on_hover_text("Text to match when cropping (before:TEXT, after:TEXT, or special:TEXT)")
                            .changed()
                        {
                            c.remove.remove_crop = if rc_crop.is_empty() {
                                None
                            } else {
                                Some(rc_crop)
                            };
                            *d = true;
                        }
                    }
                    ui.end_row();
                });

            ui.horizontal(|ui| {
                cb_tip(
                    ui,
                    d,
                    &mut c.remove.remove_digits,
                    "Digits",
                    "Remove all digit characters from the filename",
                );
                cb_tip(
                    ui,
                    d,
                    &mut c.remove.remove_high,
                    "High-ASCII",
                    "Remove non-ASCII (high) characters from the filename",
                );
                cb_tip(
                    ui,
                    d,
                    &mut c.remove.trim,
                    "Trim",
                    "Trim leading and trailing whitespace from the filename",
                );
            });
            ui.horizontal(|ui| {
                cb_tip(
                    ui,
                    d,
                    &mut c.remove.double_spaces,
                    "Dbl Spc",
                    "Replace multiple consecutive spaces with a single space",
                );
                cb_tip(
                    ui,
                    d,
                    &mut c.remove.remove_accents,
                    "Accents",
                    "Remove accent marks from characters (e.g. café -> cafe)",
                );
                cb_tip(
                    ui,
                    d,
                    &mut c.remove.remove_all_chars,
                    "Chars",
                    "Remove all characters in the name part, leaving extension intact",
                );
            });
            ui.horizontal(|ui| {
                cb_tip(
                    ui,
                    d,
                    &mut c.remove.remove_symbols,
                    "Sym.",
                    "Remove symbols/non-alphanumeric characters (keeps spaces)",
                );
                ui.label(RichText::new("Lead Dots:").size(FONT_LABEL));
                const LEAD_DOTS: [(&str, Option<LeadDotsMode>); 4] = [
                    ("None", None),
                    (".", Some(LeadDotsMode::Single)),
                    ("..", Some(LeadDotsMode::Double)),
                    ("Both", Some(LeadDotsMode::Both)),
                ];
                combo_enum_tip(
                    ui,
                    "lead_dots",
                    &mut c.remove.remove_lead_dots,
                    &LEAD_DOTS,
                    d,
                    "Remove leading dots from filenames: single (.), double (..), or both",
                );
            });
        },
        {
            reset_section(c, SectionId::Remove);
        }
    );
}
