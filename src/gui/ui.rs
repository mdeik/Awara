use crate::gui::app::GuiApp;
use crate::gui::dialogs::draw_windows;
use crate::gui::helpers::{FONT_TINY, FONT_TREE};
use crate::gui::preview_table::draw_file_and_preview;
use crate::gui::sections::draw_rename_options;
use crate::gui::types::{PopupKind, StatusKind, TABLE_INNER_MARGIN, selection_bg};
use awara::tree_root;
use eframe::egui::{
    self, Color32, RichText, ScrollArea,
    containers::scroll_area::{DragScroll, ScrollSource},
};
use std::path::Path;

// ── Main render ──

pub fn render(app: &mut GuiApp, ui: &mut egui::Ui) {
    let has_modal = app.has_any_modal();
    let ctx = ui.ctx().clone();

    // When a modal is open, suppress all scroll events to prevent scroll-area
    // widgets behind the modal from scrolling.  Modal backdrops capture clicks
    // but NOT scroll/hover events — this is the only reliable SSoT fix.
    if has_modal || ctx.any_popup_open() {
        ctx.input_mut(|i| {
            i.smooth_scroll_delta = egui::Vec2::ZERO;
        });
    }

    egui::Panel::top("toolbar").show(ui, |ui| draw_toolbar(app, ui));
    egui::Panel::bottom("statusbar").show(ui, |ui| draw_statusbar(app, ui));

    egui::Panel::top("upper_section")
        .resizable(!has_modal)
        .default_size(400.0)
        .min_size(120.0)
        .show(ui, |ui| {
            // When a modal is open, block all background interaction.
            ui.add_enabled_ui(!has_modal, |ui| {
                draw_path_bar(app, ui);
                ui.separator();
                egui::Panel::left("tree_panel")
                    .resizable(!has_modal)
                    .default_size(160.0)
                    .min_size(110.0)
                    .frame(egui::Frame::new().inner_margin(egui::Margin {
                        left: 0,
                        right: TABLE_INNER_MARGIN,
                        top: 0,
                        bottom: TABLE_INNER_MARGIN,
                    }))
                    .show(ui, |ui| draw_tree_navigator(app, ui));
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().inner_margin(egui::Margin {
                        left: TABLE_INNER_MARGIN,
                        right: 0,
                        top: 0,
                        bottom: TABLE_INNER_MARGIN,
                    }))
                    .show(ui, |ui| draw_file_and_preview(app, ui));
            });
        });

    // Zero the horizontal + bottom margins so the options scrollbars sit
    // flush against the window edges.
    egui::CentralPanel::default()
        .frame(
            egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin {
                left: 0,
                right: 0,
                top: TABLE_INNER_MARGIN,
                bottom: 0,
            }),
        )
        .show(ui, |ui| {
            ui.add_enabled_ui(!has_modal, |ui| {
                draw_rename_options(app, ui);
            });
        });

    draw_windows(app, &ctx);
}

// ── Path bar ──

fn draw_path_bar(app: &mut GuiApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let visuals = ui.style().visuals.clone();
        // Up-button with painted triangle (font-independent)
        let btn_size = egui::vec2(22.0, 22.0);
        let (btn_rect, _) = ui.allocate_exact_size(btn_size, egui::Sense::click());
        let s = 4.0_f32;
        let cx = btn_rect.center().x;
        let cy = btn_rect.center().y;
        let tri = vec![
            egui::pos2(cx, cy - s),
            egui::pos2(cx - s * 0.8, cy + s),
            egui::pos2(cx + s * 0.8, cy + s),
        ];
        if ui
            .interact(btn_rect, egui::Id::new("go_up"), egui::Sense::click())
            .clicked()
        {
            app.go_up();
        }
        if btn_rect.contains(ui.input(|i| i.pointer.interact_pos().unwrap_or_default())) {
            ui.painter()
                .rect_filled(btn_rect, 3.0, visuals.widgets.hovered.bg_fill);
        }
        ui.painter().add(egui::Shape::convex_polygon(
            tri,
            visuals.text_color(),
            egui::Stroke::NONE,
        ));
        let resp = ui.add_sized(
            ui.available_size(),
            egui::TextEdit::singleline(&mut app.cwd_input)
                .font(egui::TextStyle::Monospace)
                .hint_text(if cfg!(windows) {
                    "C:\\path\\to\\directory"
                } else {
                    "/path/to/directory"
                }),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            app.navigate_to_path(&app.cwd_input.clone());
        }
    });
}

// ── Toolbar ──

fn draw_toolbar(app: &mut GuiApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Awara");
        ui.separator();

        // Cancel / Refresh — scan state toggle
        if app.scanning {
            if ui.button("⏹ Cancel").clicked() {
                app.cancel_scan();
                // Feedback for the user-initiated cancel; restores the
                // pre-scan status after its normal timeout.
                app.set_status("Scan cancelled", StatusKind::Temporary);
            }
        } else if ui.button("🔄 Refresh").clicked() {
            app.refresh();
        }

        // Revert button — checks all directories for commit history
        let has_history = app.all_commits().next().is_some();
        if ui
            .add_enabled(
                has_history && !app.executing,
                egui::Button::new("⏪ Revert"),
            )
            .on_hover_text(if has_history {
                "Open the revert dialog to undo previous renames"
            } else {
                "No rename history found in any directory"
            })
            .clicked()
        {
            app.revert_renames();
        }

        // Commit count badge
        if has_history {
            let count = app.all_commits().count();
            let badge = format!(" {} commits ", count);
            ui.label(
                RichText::new(&badge)
                    .size(FONT_TINY)
                    .color(Color32::LIGHT_GRAY),
            );
        }

        ui.separator();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("About").clicked() {
                app.active_popup = Some(PopupKind::About);
            }
            if ui.button("⚙ Settings").clicked() {
                app.active_popup = Some(PopupKind::Settings);
            }
        });
    });
}

// ── Status Bar ──

fn draw_statusbar(app: &mut GuiApp, ui: &mut egui::Ui) {
    let text_color = ui.style().visuals.text_color();
    let color = if app.status_kind == StatusKind::Error {
        Color32::RED
    } else {
        text_color
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(&app.status_message).color(color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(format!("Modified: {}", app.modified_count()));
            ui.separator();
            ui.label(format!("Selected: {}", app.selected_count()));
            ui.separator();
            ui.label(format!("Objects: {}", app.all_files.len()));
        });
    });
}

// ── Tree Navigator ──

fn draw_tree_navigator(app: &mut GuiApp, ui: &mut egui::Ui) {
    ScrollArea::both()
        .id_salt("tree_scroll")
        .scroll_source(ScrollSource {
            drag: DragScroll::Never,
            ..Default::default()
        })
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.set_min_width(400.0);
            let scroll_target = app.tree_scroll_target.clone();
            let target_rect = draw_tree_node(app, ui, &tree_root(), 0, scroll_target.as_deref());
            if let Some(rect) = target_rect {
                // After a full refresh the target node may not be rendered yet
                // (its ancestors are still re-scanning asynchronously) — keep
                // the target pending until the node actually appears so the
                // scroll lands on it instead of being dropped on the
                // collapsed frame.
                ui.scroll_to_rect(rect, Some(egui::Align::Min));
                app.tree_scroll_target = None;
            } else if scroll_target.as_ref().is_some_and(|t| !t.exists()) {
                // The target no longer exists on disk (e.g. the cwd was
                // deleted after the request) — nothing to scroll to, so drop
                // the pending request instead of letting it linger.
                app.tree_scroll_target = None;
            }
        });
}

fn draw_tree_node(
    app: &mut GuiApp,
    ui: &mut egui::Ui,
    dir: &Path,
    depth: usize,
    scroll_target: Option<&Path>,
) -> Option<egui::Rect> {
    let visuals = ui.style().visuals.clone();
    let text_color = visuals.text_color();
    let name = if dir.as_os_str().is_empty() {
        "This PC".to_string()
    } else if dir.as_os_str() == "/" {
        "/".to_string()
    } else if dir.parent().is_none_or(|p| p.as_os_str().is_empty()) {
        // Drive root on Windows (C:\, D:\) or / on Unix (handled above)
        dir.to_string_lossy().to_string()
    } else {
        dir.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    let expanded = app.tree_expanded.contains(dir);
    // The expand arrow is painted only when the folder can actually expand:
    // - the virtual root always can;
    // - an expanded folder always shows the collapse triangle;
    // - a collapsed folder shows the arrow only if it is known to contain
    //   visible subdirectories. Unknown folders are probed synchronously once
    //   (memoized in `tree_has_subdirs`), so the arrow is correct on the very
    //   first frame — leaf folders never flash an arrow.
    let show_arrow = dir.as_os_str().is_empty()
        || expanded
        || match app.tree_has_subdirs.get(dir) {
            Some(&has) => has,
            None => app.probe_tree_arrow(dir),
        };
    let mut target_rect: Option<egui::Rect> = None;
    // Fixed-size box reserved for the expand arrow (or its placeholder) so
    // folder icons line up identically for folders with and without
    // subdirectories.
    let tri_size = 12.0_f32;
    ui.horizontal(|ui| {
        // Tighten the gap between the expand arrow and the folder icon
        // (default horizontal item spacing is 8px).
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.set_min_height(18.0);
        ui.add_space(depth as f32 * 16.0);
        if show_arrow {
            // Painted expand/collapse triangle
            let (tri_rect, _) =
                ui.allocate_exact_size(egui::vec2(tri_size, tri_size), egui::Sense::click());
            let cx = tri_rect.center().x;
            let cy = tri_rect.center().y;
            let s = 3.5_f32;
            let tri_points = if expanded {
                // Down triangle
                vec![
                    egui::pos2(cx, cy + s * 0.6),
                    egui::pos2(cx - s, cy - s * 0.6),
                    egui::pos2(cx + s, cy - s * 0.6),
                ]
            } else {
                // Right triangle
                vec![
                    egui::pos2(cx + s * 0.6, cy),
                    egui::pos2(cx - s * 0.6, cy - s),
                    egui::pos2(cx - s * 0.6, cy + s),
                ]
            };
            if ui
                .interact(
                    tri_rect,
                    egui::Id::new(format!("tree_exp_{:?}", dir)),
                    egui::Sense::click(),
                )
                .clicked()
            {
                app.toggle_tree_expand(dir);
            }
            ui.painter().add(egui::Shape::convex_polygon(
                tri_points,
                text_color,
                egui::Stroke::NONE,
            ));
        } else {
            // Reserve the same box as the arrow so folder icons keep their
            // alignment between folders with and without subdirectories.
            ui.allocate_exact_size(egui::vec2(tri_size, tri_size), egui::Sense::hover());
        }
        // Allocate space for the directory label
        let label_w = ui.available_width().max(50.0);
        let (label_rect, _) =
            ui.allocate_exact_size(egui::vec2(label_w, 18.0), egui::Sense::click());

        // Paint active directory background
        if app.cwd == dir {
            let dm = ui.style().visuals.dark_mode;
            ui.painter().rect_filled(label_rect, 0.0, selection_bg(dm));
        }
        // Scroll anchor for a pending auto-scroll target: the cwd on
        // navigation/refresh, or the nearest existing ancestor when a refresh
        // finds the cwd deleted. A 1px-wide left-edge rect is enough —
        // scroll_to_rect only needs a position.
        if scroll_target == Some(dir) {
            target_rect = Some(egui::Rect::from_min_size(
                egui::pos2((label_rect.left() - 36.0).max(0.0), label_rect.top()),
                egui::vec2(1.0, label_rect.height()),
            ));
        }

        // Paint hover highlight (on top of selection bg if both active)
        let hovered = ui.rect_contains_pointer(label_rect);
        if hovered {
            ui.painter()
                .rect_filled(label_rect, 0.0, visuals.widgets.hovered.bg_fill);
        }

        // Paint directory label — icon with teal color, name in regular text color
        let dir_color = Color32::from_rgb(0, 95, 140);
        let tree_icon = if dir.as_os_str().is_empty() {
            // Virtual root: "This PC" on Windows, "/" on Unix
            "🖥 "
        } else if dir.parent().is_none_or(|p| p.as_os_str().is_empty()) {
            // Direct child of virtual root — drive root on Windows, "/" itself on Unix
            "🖴 "
        } else {
            "📁"
        };
        let mut job = egui::text::LayoutJob::default();
        job.append(
            &format!("{} ", tree_icon),
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(FONT_TREE),
                color: dir_color,
                ..Default::default()
            },
        );
        job.append(
            &name,
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(FONT_TREE),
                color: text_color,
                ..Default::default()
            },
        );
        let galley = ui.painter().layout_job(job);
        let text_pos = egui::pos2(
            label_rect.left(),
            label_rect.center().y - galley.size().y / 2.0,
        );
        ui.painter().galley(text_pos, galley, text_color);

        // Detect click and context menu
        let label_resp = ui.interact(
            label_rect,
            egui::Id::new(format!("tree_lbl_{:?}", dir)),
            egui::Sense::click(),
        );
        if label_resp.clicked() && app.cwd != dir && !dir.as_os_str().is_empty() {
            app.navigate_tree_to(dir);
        }
        label_resp.context_menu(|ui| {
            ui.set_min_width(180.0);
            ui.menu_button("Copy to Clipboard", |ui| {
                if ui.button("Directory Path").clicked() {
                    let text = dir.to_string_lossy().to_string();
                    ui.ctx().copy_text(text);
                    ui.close();
                }
                if ui.button("Directory Name").clicked() {
                    if let Some(name) = dir.file_name() {
                        let text = name.to_string_lossy().to_string();
                        ui.ctx().copy_text(text);
                    }
                    ui.close();
                }
            });
            // Virtual root ("This PC" on Windows) has no real path — skip actions
            // that require one.
            if !dir.as_os_str().is_empty() {
                if ui.button("Open in File Manager").clicked() {
                    GuiApp::open_tree_dir_external(dir);
                    ui.close();
                }
                if ui.button("Navigate Here").clicked() {
                    if app.cwd != dir {
                        app.navigate_tree_to(dir);
                    }
                    ui.close();
                }
                ui.separator();
                if ui.button("Refresh This Subtree").clicked() {
                    app.invalidate_tree_scan(dir);
                    app.tree_has_subdirs.remove(dir);
                    app.ensure_tree_scanned(dir);
                    if app.cwd == dir {
                        app.refresh();
                    }
                    ui.close();
                }
                ui.separator();
                // Never offer to trash a filesystem root (no parent) — the
                // whole volume is not the intent, and some trash backends
                // would attempt it.
                if dir.parent().is_some() && ui.button("Move to Trash").clicked() {
                    if app.skip_trash_confirmation {
                        if let Err(e) = app.trash_path(dir) {
                            app.set_status(&e, StatusKind::Error);
                        }
                    } else {
                        app.trash_target = Some(dir.to_path_buf());
                        app.show_trash_confirmation = true;
                        app.trash_init_focus = true;
                    }
                    ui.close();
                }
                ui.separator();
                if ui.button("Properties").clicked() {
                    app.show_tree_properties = Some(dir.to_path_buf());
                    ui.close();
                }
            }
        });
    });
    if expanded {
        app.ensure_tree_scanned(dir);
        if let Some(children) = app.tree_children.get(dir).cloned() {
            for child in &children {
                if let Some(child_rect) = draw_tree_node(app, ui, child, depth + 1, scroll_target) {
                    target_rect = Some(child_rect);
                }
            }
        } else if app.tree_pending.contains_key(dir) {
            ui.horizontal(|ui| {
                ui.add_space((depth + 1) as f32 * 16.0);
                ui.label("  ...");
            });
        }
    }
    target_rect
}
