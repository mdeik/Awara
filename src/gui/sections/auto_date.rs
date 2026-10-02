use super::*;

pub(super) fn draw_section_auto_date(
    ui: &mut egui::Ui,
    en: &mut SectionEnabled,
    d: &mut bool,
    sec_order: &dyn Fn(SectionId) -> String,
    c: &mut RenameConfig,
    e: &mut GuiEditState,
) {
    sec_group!(
        ui,
        en,
        d,
        sec_order,
        c,
        AutoDate,
        "Auto Date",
        |ui, _en| {
            // Seed the widget state from the config, so the combos show what the
            // config will actually apply. Each field is seeded only while it is
            // unset, so a persisted edit state (or one the user is mid-edit in)
            // wins; `None` means "not seeded yet" for exactly that reason.
            if e.auto_date_type.is_none()
                || e.auto_date_format_key.is_none()
                || e.auto_date_sep.is_none()
                || e.auto_date_custom.is_none()
            {
                let seed = auto_date_edit_seed(c);
                e.auto_date_type.get_or_insert(seed.date_type);
                e.auto_date_format_key.get_or_insert(seed.format_key);
                e.auto_date_sep.get_or_insert(seed.sep);
                e.auto_date_custom.get_or_insert(seed.custom);
            }
            // The Fmt key the handlers write is the one the combo displays, so the
            // label and the value it stores cannot disagree.
            let fmt_idx = fmt_index(e.auto_date_format_key.as_deref());
            let fmt_key = fmt_combo_entry(fmt_idx).0;

            // Mode combo: determined from date_position and whether any date field is set
            let has_date = c.auto_date.add_date.is_some()
                || c.auto_date.add_file_date.is_some()
                || c.auto_date.insert_meta.is_some();
            let mode_idx = match c.auto_date.date_position {
                DatePosition::Suffix if has_date => 2,
                _ if has_date => 1,
                _ => 0,
            };
            // Type combo: determined directly from auto_date_type
            let type_idx = e
                .auto_date_type
                .as_deref()
                .and_then(auto_date_type_index)
                .unwrap_or(0);

            // Live mutable copies for the text fields — the widget buffers.
            let mut cur_seg = e.auto_date_sep.clone().unwrap_or_default();
            let mut cur_sep = c
                .auto_date
                .auto_date_sep_filename
                .clone()
                .unwrap_or_default();
            let mut cur_custom = e.auto_date_custom.clone().unwrap_or_default();

            // Helper to compose the format string and write to the right config field
            let recompose = |c: &mut RenameConfig,
                             e: &mut GuiEditState,
                             fmt_key: &str,
                             seg_str: &str,
                             sep_str: &str,
                             custom_str: &str| {
                let fmt = build_date_fmt(fmt_key, seg_str, custom_str);
                let fmt = if fmt.is_empty() && fmt_key != CUSTOM_FMT.0 {
                    if c.auto_date.auto_date_century {
                        "DDMMYYYY".into()
                    } else {
                        "DDMMYY".into()
                    }
                } else {
                    fmt
                };
                c.auto_date.auto_date_sep_filename = if sep_str.is_empty() {
                    None
                } else {
                    Some(sep_str.to_string())
                };
                c.auto_date.auto_date_type = e.auto_date_type.clone();
                // A key that names an Insert Meta tag is stored as one — including
                // `exif-date` and `modified`, which a `taken_` prefix test used to
                // miss and turn into a bogus Add Date format string.
                match e.auto_date_type.as_deref() {
                    Some(t) if meta_key_is_insert_tag(t) => {
                        c.auto_date.add_date = None;
                        c.auto_date.add_file_date = None;
                        c.auto_date.insert_meta = Some(t.to_string());
                        c.auto_date.insert_meta_fmt = Some(fmt);
                    }
                    _ => {
                        c.auto_date.add_date = Some(fmt);
                        c.auto_date.add_file_date = None;
                        c.auto_date.insert_meta = None;
                    }
                }
            };

            ui.horizontal(|ui| {
                ui.label(RichText::new("Mode:").size(FONT_LABEL));
                let md_names = ["None", "Prefix", "Suffix"];
                let mw = dyn_combo_width(ui, &md_names);
                egui::ComboBox::from_id_salt("adm")
                    .selected_text(md_names[mode_idx])
                    .width(mw)
                    .show_ui(ui, |ui| {
                        for (i, n) in md_names.iter().enumerate() {
                            if ui.selectable_label(mode_idx == i, *n).clicked() {
                                match i {
                                    0 => {
                                        c.auto_date.add_date = None;
                                        c.auto_date.add_file_date = None;
                                        c.auto_date.insert_meta = None;
                                        c.auto_date.date_position = DatePosition::None;
                                    }
                                    1 => {
                                        c.auto_date.date_position = DatePosition::Prefix;
                                        if c.auto_date.add_date.is_none()
                                            && c.auto_date.add_file_date.is_none()
                                            && c.auto_date.insert_meta.is_none()
                                        {
                                            let fmt = build_date_fmt(fmt_key, "", "");
                                            let fmt = if fmt.is_empty() {
                                                "DDMMYYYY".to_string()
                                            } else {
                                                fmt
                                            };
                                            c.auto_date.add_date = Some(fmt);
                                            e.auto_date_type = Some(DEFAULT_DATE_TYPE.to_string());
                                        }
                                    }
                                    2 => {
                                        c.auto_date.date_position = DatePosition::Suffix;
                                        if c.auto_date.add_date.is_none()
                                            && c.auto_date.add_file_date.is_none()
                                            && c.auto_date.insert_meta.is_none()
                                        {
                                            let fmt = build_date_fmt(fmt_key, "", "");
                                            let fmt = if fmt.is_empty() {
                                                "DDMMYYYY".to_string()
                                            } else {
                                                fmt
                                            };
                                            c.auto_date.add_date = Some(fmt);
                                            e.auto_date_type = Some(DEFAULT_DATE_TYPE.to_string());
                                        }
                                    }
                                    _ => {}
                                }
                                *d = true;
                            }
                        }
                    })
                    .response
                    .on_hover_text(
                        "Whether to insert the date as a prefix or suffix to the filename",
                    );
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Type:").size(FONT_LABEL));
                let cur_type = type_idx;
                egui::ComboBox::from_id_salt("adt")
                    .selected_text(AUTO_DATE_TYPES[cur_type].1)
                    .width(dyn_combo_width_iter(
                        ui,
                        AUTO_DATE_TYPES.iter().map(|(_, label, _)| *label),
                    ))
                    .show_ui(ui, |ui| {
                        for (i, (key, label, _)) in AUTO_DATE_TYPES.iter().enumerate() {
                            if ui.selectable_label(cur_type == i, *label).clicked() {
                                e.auto_date_type = Some((*key).to_string());
                                c.auto_date.date_position = if mode_idx == 2 { DatePosition::Suffix } else { DatePosition::Prefix };
                                if has_date {
                                    recompose(c, e, fmt_key, &cur_seg, &cur_sep, &cur_custom);
                                }
                                *d = true;
                            }
                        }
                    })
                    .response
                    .on_hover_text("Which timestamp to use: Creation/Modified/Accessed (current time or new file time), Taken (EXIF photo date), or Current (wall clock)");
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Fmt:").size(FONT_LABEL));
                let cur_fmt = fmt_idx;
                egui::ComboBox::from_id_salt("adf")
                    .selected_text(fmt_combo_entry(cur_fmt).1)
                    .width(dyn_combo_width_iter(
                        ui,
                        (0..FMT_COMBO_LEN).map(|i| fmt_combo_entry(i).1),
                    ))
                    .show_ui(ui, |ui| {
                        for i in 0..FMT_COMBO_LEN {
                            let (new_key, label) = fmt_combo_entry(i);
                            if ui.selectable_label(cur_fmt == i, label).clicked() {
                                e.auto_date_format_key = Some(new_key.to_string());
                                if has_date {
                                    recompose(c, e, new_key, &cur_seg, &cur_sep, &cur_custom);
                                }
                                *d = true;
                            }
                        }
                    })
                    .response
                    .on_hover_text("Date format: Year-Month-Day, Day-Month-Year, Month-Day-Year, with optional time components, or Custom");
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sep:").size(FONT_LABEL));
                if fixed_text(ui, [44.0, 22.0], &mut cur_sep)
                    .on_hover_text("Separator between the filename and the date string (e.g. '_' for '2025-01-15_myfile')")
                    .changed()
                {
                    c.auto_date.auto_date_sep_filename = if cur_sep.is_empty() { None } else { Some(cur_sep.clone()) };
                    *d = true;
                }
                ui.label(RichText::new("Seg:").size(FONT_LABEL));
                if fixed_text(ui, [44.0, 22.0], &mut cur_seg)
                    .on_hover_text("Separator/segment delimiter between date components (e.g. '-' for YYYY-MM-DD)")
                    .changed()
                {
                    e.auto_date_sep = Some(cur_seg.clone());
                    if has_date {
                        recompose(c, e, fmt_key, &cur_seg, &cur_sep, &cur_custom);
                    }
                    *d = true;
                }
            });
            // Row 5: Custom label | field (disabled when Fmt ≠ Custom)
            ui.horizontal(|ui| {
                        ui.label(RichText::new("Custom:").size(FONT_LABEL));
                        let is_custom = fmt_idx == 9;
                        let resp = fixed_text_enabled(ui, [105.0, 22.0], &mut cur_custom, is_custom)
                            .on_hover_text("Custom date format string (used when Format is set to 'Custom'). Use tokens like YYYY, MM, DD, hh, mm, ss");
                        if resp.changed() && is_custom {
                            e.auto_date_custom = Some(cur_custom.clone());
                            let cur_fmt_key = CUSTOM_FMT.0;
                            if has_date {
                                recompose(c, e, cur_fmt_key, &cur_seg, &cur_sep, &cur_custom);
                            }
                            *d = true;
                        }
                    });
            // Row 6: Century checkbox + Offset field together
            ui.horizontal(|ui| {
                        ui.label(RichText::new("Cent.:").size(FONT_LABEL));
                        if ui
                            .add_sized(
                                egui::vec2(22.0, 22.0),
                                egui::Checkbox::new(&mut c.auto_date.auto_date_century, ""),
                            )
                            .on_hover_text("Include century (YYYY vs YY) in the date — e.g. 2025 vs 25")
                            .changed()
                        {
                            if has_date {
                                recompose(c, e, fmt_key, &cur_seg, &cur_sep, &cur_custom);
                            }
                            *d = true;
                        }
                        ui.label(RichText::new("Off.:").size(FONT_LABEL));
                        let mut v = c.auto_date.auto_date_offset.unwrap_or(0);
                        if ui
                            .add_sized(
                                egui::vec2(48.0, 22.0),
                                int_drag(&mut v)
                                    .range(AUTO_DATE_OFFSET.0..=AUTO_DATE_OFFSET.1)
                                    .suffix("h"),
                            )
                            .on_hover_text("Extra offset in hours applied on top of your local time (0 = your local time)")
                            .changed()
                        {
                            c.auto_date.auto_date_offset = Some(v);
                            *d = true;
                        }
                    });
        },
        {
            reset_section(c, SectionId::AutoDate);
            reset_auto_date_edit_state(e);
        }
    );
}
