use super::*;
pub(crate) fn draw_windows(app: &mut GuiApp, ctx: &egui::Context) {
    // ── Collision Dialog (runs before popups, blocks everything when open) ──
    if app.collision_dialog.is_some() {
        draw_collision_dialog(app, ctx);
    }

    if app.active_popup == Some(PopupKind::About) {
        egui::Modal::new(egui::Id::new("about_popup")).show(ctx, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(260.0);
                ui.heading("Awara");
                ui.label(format!("Version: {}", env!("CARGO_PKG_VERSION")));
                ui.label("Author: Matthew Deik".to_string());
                ui.label("");
                ui.label("A powerful cross-platform batch rename utility.");
                ui.label("Renames hundreds of files in seconds with regex,");
                ui.label("numbering, case conversion, filters, and more.");
                ui.add_space(4.0);
                ui.label("Three interfaces for any workflow:");
                ui.colored_label(egui::Color32::LIGHT_BLUE, "🖥  GUI — Visual desktop app");
                ui.colored_label(egui::Color32::LIGHT_BLUE, "📟  TUI — Terminal-native UI");
                ui.colored_label(egui::Color32::LIGHT_BLUE, "⌨  CLI — Scriptable power tool");
                ui.add_space(4.0);
                ui.label("Built with Rust for performance and safety.");
                ui.add_space(4.0);
                ui.separator();
                if ui.button("Close").clicked() {
                    app.active_popup = None;
                }
            });
        });
    }
    // ── Settings Popup ──
    if app.active_popup == Some(PopupKind::Settings) {
        egui::Modal::new(egui::Id::new("settings_popup")).show(ctx, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(300.0);
                ui.heading("Settings");
                ui.separator();
                ui.checkbox(&mut app.auto_refresh, "Auto Refresh");
                ui.checkbox(&mut app.remember_last_dir, "Remember Last Directory");
                ui.checkbox(&mut app.remember_rename_options, "Remember Rename Options");
                ui.checkbox(&mut app.skip_trash_confirmation, "Skip trash confirmation");
                ui.checkbox(
                    &mut app.skip_dir_creation_warning,
                    "Skip folder-creation warning",
                );
                ui.add_space(6.0);
                if ui.button("Reset to defaults").clicked() {
                    let defaults = crate::gui::app::DEFAULT_VIEW_SETTINGS;
                    app.auto_refresh = defaults.auto_refresh;
                    app.remember_last_dir = defaults.remember_last_dir;
                    app.remember_rename_options = defaults.remember_rename_options;
                    app.skip_trash_confirmation = defaults.skip_trash_confirmation;
                    app.skip_dir_creation_warning = defaults.skip_dir_creation_warning;
                }
                ui.separator();
                ui.label("Rename Presets:");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked()
                        && let Ok(json) = awara::RenameDoc::with_disabled(
                            app.config.clone(),
                            awara::disabled_from_enabled(&app.section_enabled),
                        )
                        .to_json_pretty()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_title("Save Preset")
                            .set_file_name("preset.json")
                            .add_filter("JSON", &["json"])
                            .save_file()
                    {
                        match std::fs::write(&path, &json) {
                            Ok(_) => {
                                app.set_status(
                                    &format!("Preset saved to {}", path.display()),
                                    StatusKind::Temporary,
                                );
                                app.active_popup = None;
                            }
                            Err(e) => {
                                app.set_status(
                                    &format!("Failed to save preset: {}", e),
                                    StatusKind::Error,
                                );
                            }
                        }
                    }
                    if ui.button("Load").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_title("Load Preset")
                            .add_filter("JSON", &["json"])
                            .pick_file()
                    {
                        match awara::load_rename_doc_with_warnings(&path) {
                            Ok(loaded) => {
                                app.config = loaded.doc.rename_config;
                                app.section_enabled =
                                    awara::enabled_from_disabled(&loaded.doc.disabled_sections);
                                app.preview_dirty = true;
                                let status = if loaded.warnings.is_empty() {
                                    format!("Preset loaded from {}", path.display())
                                } else {
                                    format!(
                                        "Preset loaded from {} ({} value(s) cleaned)",
                                        path.display(),
                                        loaded.warnings.len()
                                    )
                                };
                                app.set_status(&status, StatusKind::Temporary);
                                app.active_popup = None;
                            }
                            Err(e) => {
                                app.set_status(
                                    &format!("Failed to load preset '{}': {}", path.display(), e),
                                    StatusKind::Error,
                                );
                            }
                        }
                    }
                });
                ui.separator();
                if ui.button("Close").clicked() {
                    crate::gui::persist::AppSettings::save(app);
                    app.active_popup = None;
                }
            });
        });
    }
    // ── Order Editor Popup ──
    if app.active_popup == Some(PopupKind::OrderEditor) {
        egui::Modal::new(egui::Id::new("order_popup")).show(ctx, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);
                ui.strong("Operation Order");
                ui.separator();
                let mut changed = false;
                // Read section order from config (strings → SectionId)
                let mut order: Vec<SectionId> = if app.config.section_order.is_empty() {
                    SectionId::default_order().to_vec()
                } else {
                    app.config
                        .section_order
                        .iter()
                        .filter_map(|name| SectionId::from_label(name))
                        .collect()
                };
                let len = order.len();

                egui::Grid::new("order_grid")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for i in 0..len {
                            let sec = order[i];

                            // Col 1: label
                            ui.label(format!("{}: {}", i + 1, sec.label()));

                            // Col 2: move up / down buttons (always drawn, dimmed at edges)
                            ui.horizontal(|ui| {
                                if i > 0 {
                                    if arrow_button(ui, true).clicked() {
                                        order.swap(i, i - 1);
                                        changed = true;
                                    }
                                } else {
                                    arrow_indicator(ui, true);
                                }
                                if i + 1 < len {
                                    if arrow_button(ui, false).clicked() {
                                        order.swap(i, i + 1);
                                        changed = true;
                                    }
                                } else {
                                    arrow_indicator(ui, false);
                                }
                            });

                            ui.end_row();
                        }
                    });

                if changed {
                    app.config.section_order =
                        order.iter().map(|s| s.label().to_string()).collect();
                    // Rebuild the derived command_order and let it decide whether
                    // the preview actually needs recomputing — a reorder that
                    // leaves the pipeline (e.g. empty sections) unchanged must
                    // not trigger a full preview pass.
                    app.build_command_order();
                }
                ui.separator();
                if ui.button("Reset to Default").clicked() {
                    app.config.section_order.clear(); // empty = default order
                    app.build_command_order();
                }
                ui.separator();
                if ui.button("Close").clicked() {
                    app.active_popup = None;
                }
            });
        });
    }

    // ── Attributes Editor Popup (tri-state per option) ──
    if app.active_popup == Some(PopupKind::AttrEditor) {
        egui::Modal::new(egui::Id::new("attr_popup")).show(ctx, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(220.0);
                ui.strong("File Attributes");
                ui.separator();
                // Single source of truth for the attribute names (see `awara::schema`).
                let attrs = awara::ATTRIBUTE_NAMES;
                let attr_str = app
                    .config
                    .special
                    .set_attributes
                    .clone()
                    .unwrap_or_default();
                let parts: Vec<String> = attr_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut new_parts = parts.clone();
                let mut changed = false;

                let labels = ["Not set", "Clear", "Set"];
                let tooltips = [
                    "Not set (leave as-is)",
                    "Clear (remove attribute)",
                    "Set (add attribute)",
                ];

                egui::Grid::new("attr_grid")
                    .num_columns(4)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        for a in attrs {
                            let has_plus = parts.contains(&format!("+{}", a));
                            let has_minus = parts.contains(&format!("-{}", a));
                            let has_plain = parts.contains(&a.to_string());
                            let state: u8 = if has_plus || has_plain {
                                2
                            } else if has_minus {
                                1
                            } else {
                                0
                            };

                            ui.label(RichText::new(format!("{}:", a)).size(FONT_LABEL));
                            for (si, _) in labels.iter().enumerate() {
                                let is_active = state == si as u8;
                                let btn =
                                    egui::Button::new(RichText::new(labels[si]).size(FONT_LABEL))
                                        .min_size(egui::vec2(50.0, 18.0))
                                        .fill(if is_active {
                                            if ui.style().visuals.dark_mode {
                                                match si {
                                                    2 => egui::Color32::from_rgb(30, 100, 40),
                                                    1 => egui::Color32::from_rgb(120, 40, 30),
                                                    _ => egui::Color32::from_rgb(60, 60, 60),
                                                }
                                            } else {
                                                // Light mode: lighter fills so dark text remains readable
                                                match si {
                                                    2 => egui::Color32::from_rgb(170, 225, 170),
                                                    1 => egui::Color32::from_rgb(225, 170, 170),
                                                    _ => egui::Color32::from_rgb(200, 200, 200),
                                                }
                                            }
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        });
                                let resp = ui.add(btn).on_hover_text(tooltips[si]);
                                if resp.clicked() {
                                    new_parts.retain(|p| {
                                        let bare = p
                                            .strip_prefix('+')
                                            .or_else(|| p.strip_prefix('-'))
                                            .unwrap_or(p);
                                        bare != *a
                                    });
                                    match si {
                                        2 => new_parts.push(a.to_string()),
                                        1 => new_parts.push(format!("-{}", a)),
                                        _ => {}
                                    }
                                    changed = true;
                                }
                            }
                            ui.end_row();
                        }
                    });

                if changed {
                    app.config.special.set_attributes = if new_parts.is_empty() {
                        None
                    } else {
                        Some(new_parts.join(","))
                    };
                    app.preview_dirty = true;
                }
                ui.separator();
                if ui.button("Close").clicked() {
                    app.active_popup = None;
                }
            });
        });
    }

    // ── Timestamps Editor Popup ──
    if app.active_popup == Some(PopupKind::TsEditor) {
        let mut do_ok = false;
        let mut do_cancel = false;

        egui::Modal::new(egui::Id::new("ts_popup")).show(ctx, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(480.0);
                ui.set_max_height(350.0);
                ui.strong("Change File/Folder Timestamps");
                ui.separator();
                ScrollArea::both()
                    .id_salt("ts_scroll")
                    .scroll_source(ScrollSource {
                        drag: DragScroll::Never,
                        ..Default::default()
                    })
                    .show(ui, |ui| {
                        // ── 1. Date Created ──
                        draw_ts_section(
                            ui,
                            "Date Created — set to…",
                            &mut TsSection {
                                mode: &mut app.timestamps.created.mode,
                                fixed_date: &mut app.timestamps.created.fixed_date,
                                fixed_time: &mut app.timestamps.created.fixed_time,
                                fixed_pm: &mut app.timestamps.created.fixed_pm,
                                incr_by: &mut app.timestamps.created.incr_by,
                                delta_days: &mut app.timestamps.created.delta_days,
                                delta_hours: &mut app.timestamps.created.delta_hours,
                                delta_mins: &mut app.timestamps.created.delta_mins,
                                delta_secs: &mut app.timestamps.created.delta_secs,
                                negative: &mut app.timestamps.created.negative,
                            },
                            "Modified",
                            "modified",
                        );

                        // ── 2. Date Modified ──
                        draw_ts_section(
                            ui,
                            "Date Modified — set to…",
                            &mut TsSection {
                                mode: &mut app.timestamps.modified.mode,
                                fixed_date: &mut app.timestamps.modified.fixed_date,
                                fixed_time: &mut app.timestamps.modified.fixed_time,
                                fixed_pm: &mut app.timestamps.modified.fixed_pm,
                                incr_by: &mut app.timestamps.modified.incr_by,
                                delta_days: &mut app.timestamps.modified.delta_days,
                                delta_hours: &mut app.timestamps.modified.delta_hours,
                                delta_mins: &mut app.timestamps.modified.delta_mins,
                                delta_secs: &mut app.timestamps.modified.delta_secs,
                                negative: &mut app.timestamps.modified.negative,
                            },
                            "Created",
                            "created",
                        );

                        // ── 3. Date Accessed ──
                        draw_ts_section(
                            ui,
                            "Date Accessed — set to…",
                            &mut TsSection {
                                mode: &mut app.timestamps.accessed.mode,
                                fixed_date: &mut app.timestamps.accessed.fixed_date,
                                fixed_time: &mut app.timestamps.accessed.fixed_time,
                                fixed_pm: &mut app.timestamps.accessed.fixed_pm,
                                incr_by: &mut app.timestamps.accessed.incr_by,
                                delta_days: &mut app.timestamps.accessed.delta_days,
                                delta_hours: &mut app.timestamps.accessed.delta_hours,
                                delta_mins: &mut app.timestamps.accessed.delta_mins,
                                delta_secs: &mut app.timestamps.accessed.delta_secs,
                                negative: &mut app.timestamps.accessed.negative,
                            },
                            "Created",
                            "created",
                        );

                        // ── Action Buttons ──
                        ui.horizontal_centered(|ui| {
                            if ui
                                .add_enabled(app.timestamps_valid(), egui::Button::new("Confirm"))
                                .on_disabled_hover_text(
                                    "Fix the invalid date/time before confirming",
                                )
                                .clicked()
                            {
                                do_ok = true;
                            }
                            if ui.button("Cancel").clicked() {
                                do_cancel = true;
                            }
                        });
                    });
            });
        });

        // Process OK/Cancel outside the borrow
        if do_ok {
            app.apply_timestamps_ui();
            app.active_popup = None;
        }
        if do_cancel {
            app.active_popup = None;
        }
    }

    // ── Revert Dialog ──
    if app.active_popup == Some(PopupKind::RevertDialog) {
        draw_revert_dialog(app, ctx);
    }

    // ── Properties Popup ──
    if let Some(idx) = app.show_properties_idx {
        let cloned_path = app.all_files.get(idx).cloned();
        if let Some(path) = cloned_path.as_ref() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let new_name = app
                .preview
                .iter()
                .find(|pr| pr.index == idx)
                .map(|pr| pr.new_name().to_string())
                .unwrap_or_default();
            let new_name = if new_name != name && !new_name.is_empty() {
                Some(new_name.as_str())
            } else {
                None
            };

            draw_properties_popup(ctx, "properties_popup", path, &name, new_name, || {
                app.show_properties_idx = None;
            });
        } else {
            // Index no longer valid
            app.show_properties_idx = None;
        }
    }

    // ── Tree Properties Popup ──
    // Set by the tree navigator context menu; the target is a path rather than
    // a listing index because tree entries are not necessarily scanned.
    if let Some(path) = app.show_tree_properties.clone() {
        if path.exists() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            draw_properties_popup(ctx, "tree_properties_popup", &path, &name, None, || {
                app.show_tree_properties = None;
            });
        } else {
            // Directory vanished (e.g. trashed elsewhere) — close the popup.
            app.show_tree_properties = None;
        }
    }

    // ── Multi-file Summary Properties Popup ──
    if let Some(ref data) = app.show_properties_multi_data.clone() {
        let cloned = data.clone();
        egui::Modal::new(egui::Id::new("properties_multi_popup")).show(ctx, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(PROPERTIES_POPUP_WIDTH);
                ui.strong("Properties");
                ui.separator();
                egui::Grid::new("multi_props_grid")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Selected:").strong());
                        ui.label(format!(
                            "{} object{}",
                            cloned.count,
                            if cloned.count == 1 { "" } else { "s" }
                        ));
                        ui.end_row();

                        ui.label(RichText::new("Total Size:").strong());
                        let hs = crate::gui::helpers::human_size(cloned.total_size);
                        ui.label(format!("{hs} ({} bytes)", cloned.total_size));
                        ui.end_row();
                    });
                ui.add_space(8.0);
                ui.separator();
                // Show up to 20 filenames as a compact scrollable list.
                let display_names: Vec<&str> =
                    cloned.names.iter().map(|s| s.as_str()).take(20).collect();
                ui.label(
                    RichText::new(format!("Files ({}/{}):", display_names.len(), cloned.count))
                        .strong(),
                );
                egui::ScrollArea::vertical()
                    .id_salt("multi_props_names")
                    .max_height(200.0)
                    .show(ui, |ui| {
                        // Make labels fill available width
                        ui.set_min_width(ui.available_width());
                        for name in &display_names {
                            ui.add(
                                egui::Label::new(RichText::new(*name).size(FONT_LABEL))
                                    .selectable(false),
                            );
                        }
                        if cloned.names.len() > 20 {
                            ui.label(format!("... and {} more", cloned.names.len() - 20));
                        }
                    });
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            app.show_properties_multi_data = None;
                        }
                    });
                });
            });
        });
    }

    // ── Trash Confirmation Dialog ──
    if app.show_trash_confirmation {
        // A tree-navigator target trashes a single directory; otherwise the
        // confirmation applies to the current file selection.
        let target = app.trash_target.clone();
        let count = if target.is_some() {
            1
        } else {
            app.selected_count()
        };
        egui::Modal::new(egui::Id::new("trash_confirmation")).show(ctx, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(300.0);
                ui.strong("Confirm Trash");
                ui.separator();
                match &target {
                    Some(p) => {
                        let name = p
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| p.to_string_lossy().to_string());
                        ui.label(format!("Would you like to move \"{}\" to Trash?", name));
                    }
                    None => {
                        ui.label(format!(
                            "Would you like to move {} object{} to Trash?",
                            count,
                            if count == 1 { "" } else { "s" }
                        ));
                    }
                }
                ui.add_space(8.0);
                ui.checkbox(&mut app.skip_trash_confirmation, "Don't ask again");
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    let trash_resp = ui.button("Move to Trash");
                    if trash_resp.clicked() {
                        crate::gui::persist::AppSettings::save(app);
                        let result = match &target {
                            Some(p) => app.trash_path(p),
                            None => app.trash_selected_multi(),
                        };
                        if let Err(e) = result {
                            app.set_status(&e, StatusKind::Error);
                        }
                        app.trash_target = None;
                        app.show_trash_confirmation = false;
                    }
                    if app.trash_init_focus {
                        trash_resp.request_focus();
                    }
                    if ui.button("Cancel").clicked() {
                        app.trash_target = None;
                        app.show_trash_confirmation = false;
                    }
                });
                app.trash_init_focus = false;
            });
        });
    }

    // ── Folder-Creation Warning Dialog ──
    if app.show_dir_creation_warning {
        let count = app.pending_dir_count;
        egui::Modal::new(egui::Id::new("dir_creation_warning")).show(ctx, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(320.0);
                ui.strong("Create Folders?");
                ui.separator();
                ui.label(format!(
                    "{} rename{} will create new folder{} from the new name.",
                    count,
                    if count == 1 { "" } else { "s" },
                    if count == 1 { "" } else { "s" },
                ));
                ui.add_space(8.0);
                ui.checkbox(&mut app.skip_dir_creation_warning, "Don't ask again");
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    let ok = ui.button("Continue");
                    if ok.clicked() {
                        crate::gui::persist::AppSettings::save(app);
                        app.confirm_dir_creation_warning();
                    }
                    if app.dir_warning_init_focus {
                        ok.request_focus();
                    }
                    if ui.button("Cancel").clicked() {
                        app.cancel_dir_creation_warning();
                    }
                });
                app.dir_warning_init_focus = false;
            });
        });
    }

    // ── Invalid New Name Dialog (blocks Apply) ──
    if app.show_invalid_name_warning {
        let entries = app.invalid_name_entries.clone();
        egui::Modal::new(egui::Id::new("invalid_name_warning")).show(ctx, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(360.0);
                ui.strong("Invalid New Name");
                ui.separator();
                ui.label(
                    "A rename may only create subfolders — it cannot navigate the path \
                     with an absolute name or a '.' / '..' segment.",
                );
                ui.add_space(6.0);
                for (current, detail) in entries.iter().take(8) {
                    ui.label(format!("• {current} → {detail}"));
                }
                if entries.len() > 8 {
                    ui.label(format!("… and {} more", entries.len() - 8));
                }
                ui.add_space(8.0);
                ui.separator();
                let ok = ui.button("OK");
                if ok.clicked() {
                    app.dismiss_invalid_name_warning();
                }
                if app.invalid_name_init_focus {
                    ok.request_focus();
                }
                app.invalid_name_init_focus = false;
            });
        });
    }
}
