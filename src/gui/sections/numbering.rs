use super::*;

pub(super) fn draw_section_numbering(
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
        Numbering,
        "Numbering",
        |ui, _en| {
            // Rows 1-3: 4-column grid — labels in own cols for Start & Pad rows
            egui::Grid::new("num_grid")
                .min_col_width(0.0)
                .show(ui, |ui| {
                    // Row 1: Mode label | combo | At label | At field
                    ui.label(RichText::new("Mode:").size(FONT_LABEL));
                    {
                        const NUMBERING_MODES: [(&str, Option<NumberingMode>); 5] = [
                            ("None", None),
                            ("Prefix", Some(NumberingMode::Prefix)),
                            ("Suffix", Some(NumberingMode::Suffix)),
                            ("Both", Some(NumberingMode::Both)),
                            ("Insert", Some(NumberingMode::Insert)),
                        ];
                        combo_enum_tip(
                            ui,
                            "nmode",
                            &mut c.numbering.numbering_mode,
                            &NUMBERING_MODES,
                            d,
                            "Numbering mode: Prefix (001_name), Suffix (name_001), or Insert at a position",
                        );
                    }
                    ui.label(RichText::new("At:").size(FONT_LABEL));
                    {
                        let mut at = c.numbering.numbering_at.unwrap_or(0);
                        if ui
                            .add_sized(
                                egui::vec2(44.0, 22.0),
                                int_drag(&mut at)
                                    .range(NUMBERING_AT.0..=NUMBERING_AT.1),
                            )
                            .on_hover_text("Character position to insert the number (only for Insert mode, negative counts from end)")
                            .changed()
                        {
                            c.numbering.numbering_at = Some(at);
                            *d = true;
                        }
                    }
                    ui.end_row();

                    // Row 2: Start label | field | Incr label | field
                    ui.label(RichText::new("Start:").size(FONT_LABEL));
                    if ui
                        .add_sized(
                            egui::vec2(44.0, 22.0),
                            int_drag(&mut c.numbering.numbering_start)
                                .range(NUMBERING_START.0..=NUMBERING_START.1),
                        )
                        .on_hover_text("Starting number for the sequence (default: 1)")
                        .changed()
                    {
                        *d = true;
                    }
                    ui.label(RichText::new("Incr:").size(FONT_LABEL));
                    if ui
                        .add_sized(
                            egui::vec2(44.0, 22.0),
                            int_drag(&mut c.numbering.numbering_increment)
                                .range(NUMBERING_INCREMENT.0..=NUMBERING_INCREMENT.1),
                        )
                        .on_hover_text("Step between consecutive numbers (default: 1, negative for descending)")
                        .changed()
                    {
                        *d = true;
                    }
                    ui.end_row();

                    // Row 3: Pad label | field | Sep label | field
                    ui.label(RichText::new("Pad:").size(FONT_LABEL));
                    if ui
                        .add_sized(
                            egui::vec2(44.0, 22.0),
                            int_drag(&mut c.numbering.numbering_pad)
                                .range(NUMBERING_PAD.0..=NUMBERING_PAD.1),
                        )
                        .on_hover_text("Zero-pad numbers to this width (e.g. pad=3 gives 001, 002)")
                        .changed()
                    {
                        *d = true;
                    }
                    ui.label(RichText::new("Sep:").size(FONT_LABEL));
                    {
                        let mut sep_buf = c.numbering.numbering_sep.clone().unwrap_or_default();
                        if fixed_text(ui, [44.0, 22.0], &mut sep_buf)
                            .on_hover_text("Separator between the number and the filename (e.g. '_' for 001_name)")
                            .changed()
                        {
                            c.numbering.numbering_sep = if sep_buf.is_empty() { None } else { Some(sep_buf) };
                            *d = true;
                        }
                    }
                    ui.end_row();
                });

            ui.horizontal(|ui| {
                let mut brk = c.numbering.numbering_break.unwrap_or(0);
                ui.label(RichText::new("Break:").size(FONT_LABEL));
                if ui
                    .add_sized(
                        egui::vec2(44.0, 22.0),
                        int_drag(&mut brk).range(NUMBERING_BREAK.0..=NUMBERING_BREAK.1),
                    )
                    .on_hover_text("Reset numbering every N files")
                    .changed()
                {
                    c.numbering.numbering_break = Some(brk);
                    *d = true;
                }
                if ui
                    .add(egui::Checkbox::new(
                        &mut c.numbering.numbering_restart_folder,
                        "Per Folder",
                    ))
                    .on_hover_text("Restart numbering for each folder when renaming recursively")
                    .changed()
                {
                    *d = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Base:").size(FONT_LABEL));
                combo_opt_tip(
                    ui,
                    "ntype",
                    &mut c.numbering.numbering_type,
                    NUMBERING_BASES,
                    d,
                    "Numbering base/type: Decimal, Binary, Hex, A-Z letters, or Roman numerals",
                );
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Case:").size(FONT_LABEL));
                combo_opt_tip(
                    ui,
                    "ncase",
                    &mut c.numbering.numbering_case,
                    &[("Upper", "upper"), ("Lower", "lower")],
                    d,
                    "Letter case for alpha/Roman numbering (lower or upper)",
                );
            });
            // numbering_source intentionally omitted — CLI-only (Name/Date/Size sorting overlaps with scan order)
        },
        {
            reset_section(c, SectionId::Numbering);
        }
    );
}
