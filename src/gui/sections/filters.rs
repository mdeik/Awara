use super::*;

/// Deferred scan/refilter requests raised while the Filters section is drawn.
/// `auto_scan` gates whether an edit triggers the request immediately.
pub(super) struct FilterScanCtl<'a> {
    pub auto_scan: bool,
    pub scan_clicked: &'a mut bool,
    pub refilter_clicked: &'a mut bool,
}

/// Draws the Filters section.
pub(super) fn draw_section_filters(
    ui: &mut egui::Ui,
    en: &mut SectionEnabled,
    d: &mut bool,
    sec_order: &dyn Fn(SectionId) -> String,
    c: &mut RenameConfig,
    ctl: FilterScanCtl<'_>,
) {
    sec_group!(
        ui,
        en,
        d,
        sec_order,
        c,
        Filters,
        "Filters",
        |ui, _en| {
            // 6-column grid: rows are [mask_label mask_field files+folder+hidden subfolders name_label name_fields]
            //                            [_ match_case+regex exclude_label_field level_label_field path_label path_fields]
            egui::Grid::new("filter_grid")
                .min_col_width(0.0)
                .show(ui, |ui| {
                    // Row 1: mask_label | mask_field | files+folder+hidden | subfolders | name_label | name_fields
                    ui.label(RichText::new("Mask:").size(FONT_LABEL));

                    let mut mask_buf = c.filters.filter_pattern.clone().unwrap_or_default();
                    if fixed_text(ui, [160.0, 20.0], &mut mask_buf)
                        .on_hover_text(
                            "Glob pattern to filter visible files (e.g. '*.txt', 'photo_*')",
                        )
                        .changed()
                    {
                        c.filters.filter_pattern = if mask_buf.is_empty() {
                            None
                        } else {
                            Some(mask_buf.clone())
                        };
                        *d = true;
                        if ctl.auto_scan {
                            *ctl.refilter_clicked = true;
                        }
                    }

                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Checkbox::new(&mut c.filters.filter_files, "Files"))
                            .on_hover_text("Show files in the file list")
                            .changed()
                        {
                            *d = true;
                            if ctl.auto_scan && *d {
                                *ctl.refilter_clicked = true;
                            }
                        }
                        if ui
                            .add(egui::Checkbox::new(
                                &mut c.filters.filter_folders,
                                "Folders",
                            ))
                            .on_hover_text("Show folders in the file list")
                            .changed()
                        {
                            *d = true;
                            if ctl.auto_scan {
                                *ctl.refilter_clicked = true;
                            }
                        }
                        if ui
                            .add(egui::Checkbox::new(&mut c.filters.filter_hidden, "Hidden"))
                            .on_hover_text("Include hidden files (dotfiles) in the listing")
                            .changed()
                        {
                            *d = true;
                            if ctl.auto_scan {
                                *ctl.scan_clicked = true;
                            }
                        }
                    });

                    if ui
                        .add(egui::Checkbox::new(
                            &mut c.filters.filter_subfolders,
                            "Subfolders",
                        ))
                        .on_hover_text("Recursively include subfolder contents")
                        .changed()
                    {
                        *d = true;
                        if ctl.auto_scan {
                            *ctl.scan_clicked = true;
                        }
                    }

                    ui.label(RichText::new("Name:").size(FONT_LABEL));

                    ui.horizontal(|ui| {
                        let mut v = c.filters.min_name_len.unwrap_or(0);
                        if ui
                            .add_sized(
                                egui::vec2(36.0, 20.0),
                                int_drag(&mut v).range(NAME_LEN.0..=NAME_LEN.1),
                            )
                            .on_hover_text("Minimum filename length to include")
                            .changed()
                        {
                            c.filters.min_name_len = Some(v);
                            *d = true;
                            if ctl.auto_scan {
                                *ctl.refilter_clicked = true;
                            }
                        }
                        ui.label(RichText::new("-").size(FONT_LABEL));
                        let mut v = c.filters.max_name_len.unwrap_or(0);
                        if ui
                            .add_sized(
                                egui::vec2(36.0, 20.0),
                                int_drag(&mut v).range(NAME_LEN.0..=NAME_LEN.1),
                            )
                            .on_hover_text("Maximum filename length to include")
                            .changed()
                        {
                            c.filters.max_name_len = Some(v);
                            *d = true;
                            if ctl.auto_scan {
                                *ctl.refilter_clicked = true;
                            }
                        }
                    });
                    ui.end_row();

                    // Row 2: _ | match_case+regex | exclude_label_field | level_label_field | path_label | path_fields
                    ui.label("");

                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Checkbox::new(
                                &mut c.filters.filter_match_case,
                                "Match Case",
                            ))
                            .on_hover_text(
                                "Case-sensitive matching for glob masks (regex masks are always case-sensitive; use (?i))",
                            )
                            .changed()
                        {
                            *d = true;
                            if ctl.auto_scan {
                                *ctl.refilter_clicked = true;
                            }
                        }
                        if ui
                            .add(egui::Checkbox::new(
                                &mut c.filters.filter_use_regex,
                                "Regex",
                            ))
                            .on_hover_text("Treat filter patterns as regular expressions")
                            .changed()
                        {
                            *d = true;
                            if ctl.auto_scan {
                                *ctl.refilter_clicked = true;
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Exclude:").size(FONT_LABEL));
                        let mut excl_buf = c.filters.exclude_regex.clone().unwrap_or_default();
                        if fixed_text(ui, [160.0, 20.0], &mut excl_buf)
                            .on_hover_text(
                                "Regex pattern to exclude matching files, e.g. '\\.bak$' (case-sensitive; use (?i) for case-insensitive)",
                            )
                            .changed()
                        {
                            c.filters.exclude_regex = if excl_buf.is_empty() {
                                None
                            } else {
                                Some(excl_buf)
                            };
                            *d = true;
                            if ctl.auto_scan {
                                *ctl.refilter_clicked = true;
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Level:").size(FONT_LABEL));
                        let mut v = c.filters.filter_level.unwrap_or(0);
                        if ui
                            .add_sized(
                                egui::vec2(36.0, 20.0),
                                int_drag(&mut v).range(FILTER_LEVEL.0..=FILTER_LEVEL.1),
                            )
                            .on_hover_text("Maximum nesting depth when recursing")
                            .changed()
                        {
                            c.filters.filter_level = Some(v);
                            *d = true;
                            if ctl.auto_scan {
                                *ctl.scan_clicked = true;
                            }
                        }
                    });

                    ui.label(RichText::new("Path:").size(FONT_LABEL));

                    ui.horizontal(|ui| {
                        let mut v = c.filters.min_path_len.unwrap_or(0);
                        if ui
                            .add_sized(
                                egui::vec2(36.0, 20.0),
                                int_drag(&mut v).range(PATH_LEN.0..=PATH_LEN.1),
                            )
                            .on_hover_text("Minimum full path length to include")
                            .changed()
                        {
                            c.filters.min_path_len = Some(v);
                            *d = true;
                            if ctl.auto_scan {
                                *ctl.refilter_clicked = true;
                            }
                        }
                        ui.label(RichText::new("-").size(FONT_LABEL));
                        let mut v = c.filters.max_path_len.unwrap_or(0);
                        if ui
                            .add_sized(
                                egui::vec2(36.0, 20.0),
                                int_drag(&mut v).range(PATH_LEN.0..=PATH_LEN.1),
                            )
                            .on_hover_text("Maximum full path length to include")
                            .changed()
                        {
                            c.filters.max_path_len = Some(v);
                            *d = true;
                            if ctl.auto_scan {
                                *ctl.refilter_clicked = true;
                            }
                        }
                    });
                    ui.end_row();
                });
        },
        {
            reset_section(c, SectionId::Filters);
            *ctl.scan_clicked = true;
        }
    );
}
