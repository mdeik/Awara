use crate::gui::app::{GuiApp, LiveCollisionReason, copy_text, open_in_os, open_parent_in_os};
use crate::gui::helpers::{
    ClickAction, DiffSeg, FONT_LABEL, FONT_TINY, FONT_TITLE, HEADER_H, ROW_H, apply_click_action,
    arrow_button, arrow_indicator, build_fixed_datetime, compute_click_action, diff_colored,
    draw_table_header_cell, fixed_text, format_date, handle_box_selection_by_display_order,
    human_size, int_drag, select_display_range,
};
use crate::gui::table_shared::draw_header_separator;
use crate::gui::types::{COL_MIN_WIDTH, GREEN, PopupKind, SectionId, StatusKind};
use awara::{CollisionReason, CollisionStrategy};
use eframe::egui::{
    self, Color32, Frame, RichText, ScrollArea, Sense, Vec2,
    containers::scroll_area::{DragScroll, ScrollSource},
};

/// Shared width for the file-properties popup.
pub(super) const PROPERTIES_POPUP_WIDTH: f32 = 260.0;

/// Mutable state for one timestamp section (Created / Modified / Accessed).
pub(crate) struct TsSection<'a> {
    pub mode: &'a mut String,
    pub fixed_date: &'a mut String,
    pub fixed_time: &'a mut String,
    pub fixed_pm: &'a mut bool,
    pub incr_by: &'a mut u32,
    pub delta_days: &'a mut u32,
    pub delta_hours: &'a mut u32,
    pub delta_mins: &'a mut u32,
    pub delta_secs: &'a mut u32,
    pub negative: &'a mut bool,
}

/// Helper: draw one timestamp section (Created / Modified / Accessed).
/// `other_label` and `other_val` name the cross-reference radio option
/// (e.g. "Modified"/"modified" for Created, "Created"/"created" for Modified/Accessed).
pub(crate) fn draw_ts_section(
    ui: &mut egui::Ui,
    label: &str,
    s: &mut TsSection,
    other_label: &str,
    other_val: &str,
) {
    Frame::group(ui.style())
        .fill(ui.style().visuals.panel_fill)
        .stroke(ui.style().visuals.widgets.noninteractive.bg_stroke)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(label).strong().size(FONT_TITLE));

            // Row 1: primary mode radio buttons (always visible)
            ui.horizontal(|ui| {
                ui.radio_value(s.mode, "no_change".to_string(), "No Change")
                    .on_hover_text("Leave this timestamp unchanged");
                ui.radio_value(s.mode, "current".to_string(), "Current")
                    .on_hover_text("Set to the current date and time");
                ui.radio_value(s.mode, other_val.to_string(), other_label)
                    .on_hover_text(format!("Copy the {} timestamp to this field", other_label));
                ui.radio_value(s.mode, "taken".to_string(), "Taken (Original / Item Date)")
                    .on_hover_text("Use the original file timestamp (e.g. EXIF date from photos)");
            });

            // Row 2: Fixed — radio always shown; date/time/incr always visible but disabled when not active
            let is_fixed = s.mode.as_str() == "fixed";
            ui.horizontal(|ui| {
                ui.radio_value(s.mode, "fixed".to_string(), "Fixed")
                    .on_hover_text("Set to a specific date and time");
                ui.add_enabled_ui(is_fixed, |ui| {
                    ui.label("Date:");
                    fixed_text(ui, [100.0, 22.0], s.fixed_date)
                        .on_hover_text("Date in YYYY-MM-DD format (e.g. 2025-12-31)");
                    ui.label("Time:");
                    fixed_text(ui, [80.0, 22.0], s.fixed_time)
                        .on_hover_text("Time in 12-hour format (hour 1-12), hh:mm:ss");
                    ui.selectable_value(&mut *s.fixed_pm, false, "AM");
                    ui.selectable_value(&mut *s.fixed_pm, true, "PM");
                    ui.add_sized(
                        [50.0, 22.0],
                        int_drag(s.incr_by)
                            .range(0..=86400)
                            .suffix("s"),
                    )
                    .on_hover_text(
                        "Increment each file's timestamp by this many seconds (e.g. 3600 = +1 hour per file)",
                    );
                    if let Err(err) =
                        build_fixed_datetime(s.fixed_date.as_str(), s.fixed_time, *s.fixed_pm)
                    {
                        ui.label(RichText::new("⚠").color(Color32::from_rgb(220, 80, 80)))
                            .on_hover_text(err);
                    }
                });
            });

            // Row 3: Delta — radio always shown; duration fields always visible but disabled when not active
            let is_delta = s.mode.as_str() == "delta";
            ui.horizontal(|ui| {
                ui.radio_value(s.mode, "delta".to_string(), "Delta")
                    .on_hover_text("Offset from the current date and time");
                ui.add_enabled_ui(is_delta, |ui| {
                    ui.label("Days:");
                    ui.add_sized([40.0, 22.0], int_drag(s.delta_days).range(0..=99999))
                        .on_hover_text("Number of days to add (or subtract if Negative is checked)");
                    ui.label("Hours:");
                    ui.add_sized([40.0, 22.0], int_drag(s.delta_hours).range(0..=23))
                        .on_hover_text("Number of hours to add");
                    ui.label("Mins:");
                    ui.add_sized([40.0, 22.0], int_drag(s.delta_mins).range(0..=59))
                        .on_hover_text("Number of minutes to add");
                    ui.label("Secs:");
                    ui.add_sized([40.0, 22.0], int_drag(s.delta_secs).range(0..=59))
                        .on_hover_text("Number of seconds to add");
                    ui.checkbox(s.negative, "Neg.")
                        .on_hover_text("Subtract the delta instead of adding it");
                });
            });
        });
    ui.add_space(4.0);
}

/// Shared file-properties grid (Name, Type, Size, Modified, Path).
/// `new_name` is shown as a "New Name" row if present and differs from `name`.
fn draw_properties_grid(
    ui: &mut egui::Ui,
    path: &std::path::Path,
    name: &str,
    new_name: Option<&str>,
) {
    let meta = std::fs::metadata(path).ok();
    let path_str = path.to_string_lossy();

    egui::Grid::new("props_grid")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            ui.label(RichText::new("Name:").strong());
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(name);
            });
            ui.end_row();

            if let Some(nn) = new_name
                && nn != name
                && !nn.is_empty()
            {
                ui.label(RichText::new("New Name:").strong());
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new(nn).color(GREEN));
                });
                ui.end_row();
            }

            ui.label(RichText::new("Type:").strong());
            if path.is_dir() {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label("Directory");
                });
            } else {
                // SSoT with the rename pipeline: normal + compound extensions
                // (so `.tar.gz` shows as TAR.GZ). Files only, hence not a dir.
                let ext = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(awara::extension_str)
                    .map(str::to_uppercase)
                    .unwrap_or_else(|| "\u{2014}".into());
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(ext);
                });
            }
            ui.end_row();

            ui.label(RichText::new("Size:").strong());
            if let Some(m) = &meta {
                let hs = human_size(m.len());
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(format!("{hs} ({} bytes)", m.len()));
                });
            } else {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label("\u{2014}");
                });
            }
            ui.end_row();

            if let Some(m) = &meta
                && let Ok(t) = m.modified()
            {
                let dur = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                let secs = dur.as_secs() as i64;
                ui.label(RichText::new("Modified:").strong());
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(format_date(secs));
                });
                ui.end_row();
            }

            ui.label(RichText::new("Path:").strong());
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(ui.available_width());
                ui.add(egui::Label::new(path_str.as_ref()).truncate());
            });
            ui.end_row();
        });
}

/// Full file-properties popup — modal, frame, grid, and action buttons.
pub(crate) fn draw_properties_popup(
    ctx: &egui::Context,
    id: &str,
    path: &std::path::Path,
    name: &str,
    new_name: Option<&str>,
    on_close: impl FnOnce(),
) {
    egui::Modal::new(egui::Id::new(id)).show(ctx, |ui| {
        Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_width(PROPERTIES_POPUP_WIDTH);
            ui.strong("Properties");
            ui.separator();
            draw_properties_grid(ui, path, name, new_name);
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    open_in_os(path);
                }
                if ui.button("Open Containing Folder").clicked() {
                    open_parent_in_os(path);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        on_close();
                    }
                });
            });
        });
    });
}

// ── Collision Resolution Dialog ──

/// `pub(super)` — called by `draw_windows` in `windows.rs`.
pub(super) fn draw_collision_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    let mut state = match app.collision_dialog.take() {
        Some(s) => s,
        None => return,
    };
    if state.entries.is_empty() {
        app.collision_dialog = None;
        app.pending_plan = None;
        return;
    }

    let total = state.entries.len();
    let cur = state.current;

    // All entries already decided? Execute.
    if cur >= total {
        let plan = app.pending_plan.take();
        if state.all_skip {
            let n = state.entries.len();
            app.set_status(
                &format!("Done: 0 renamed, 0 errors, {} skipped", n),
                StatusKind::Persistent,
            );
        } else if let Some(plan) = plan {
            let decisions = state.decisions(&plan);
            let resolution = awara::Resolution::new(CollisionStrategy::Overwrite, Some(decisions));
            let label = if state.is_f2 { "Inline" } else { "Apply" };
            app.execute_plan_inner(plan, &resolution, label, state.is_f2);
        }
        app.collision_dialog = None;
        return;
    }

    // Re-compute live collision reasons so all entries are up to date
    // with the latest user decisions. Both the message and the warning
    // read from `live_reason` on the entry — never derive it ad-hoc.
    state.recompute_live_reasons();

    let entry = &state.entries[cur];

    let mut do_replace = false;
    let mut do_skip = false;
    let mut do_replace_all = false;
    let mut do_skip_all = false;
    let mut do_cancel = false;
    let mut do_f2_exec = false;

    let title = if state.is_f2 {
        "F2 Rename Conflict"
    } else {
        "Rename Conflict"
    };
    // Progress lives in the title so it doesn't compete with it as a second
    // full-size line.
    let title = format!("{} ({} of {})", title, cur + 1, total);

    egui::Modal::new(egui::Id::new("collision_popup")).show(ctx, |ui| {
        Frame::popup(ui.style()).show(ui, |ui| {
            // Fixed width: the dialog is the same size for every entry, and
            // narrower than the old message-driven width.
            ui.set_width(360.0);
            ui.strong(title);
            ui.separator();
            ui.add_space(2.0);

            // Parent path (same for both source and target). Left aligned: a
            // plain label in the vertical layout, truncated to the dialog width.
            let parent_path = std::path::Path::new(&entry.source_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            ui.add(
                egui::Label::new(
                    RichText::new(&parent_path)
                        .size(FONT_TINY)
                        .color(Color32::GRAY),
                )
                .truncate(),
            );
            ui.add_space(2.0);

            // Source → Target: each name left aligned in a fixed-width column
            // (a plain `add_sized` would center it).
            ui.horizontal(|ui| {
                let label_h = 18.0;
                let name_w = 150.0;
                let name_cell = |ui: &mut egui::Ui, text: &str| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(name_w, label_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add(egui::Label::new(RichText::new(text)).truncate());
                        },
                    );
                };
                name_cell(ui, &entry.source_display);
                ui.label(RichText::new(" → ").color(Color32::GRAY));
                name_cell(ui, &entry.target_display);
            });
            ui.add_space(4.0);

            // One message block per entry: reason and consequence are rendered
            // together, at a single size and a single theme-derived color, so
            // there are no competing styles. On-disk conflicts are destructive
            // (error color); in-batch conflicts are informational (warn color).
            let (message, color): (String, Color32) = match entry.live_reason {
                // Name what actually already exists: a folder target for a file
                // source, but a *file* target when the folder is the source.
                LiveCollisionReason::OnDisk if entry.target_is_dir => (
                    "\u{26a0} Folder already exists \u{2014} cannot replace or merge; skipped."
                        .to_string(),
                    ui.visuals().error_fg_color,
                ),
                LiveCollisionReason::OnDisk if entry.source_is_dir => (
                    "\u{26a0} File already exists \u{2014} cannot replace with a folder; skipped."
                        .to_string(),
                    ui.visuals().error_fg_color,
                ),
                LiveCollisionReason::OnDisk => {
                    // "(earlier conflict resolved)" when this was originally an
                    // InBatch collision that became OnDisk after the prior entry
                    // was skipped.
                    let base = if matches!(entry.reason, CollisionReason::InBatch(_)) {
                        "\u{26a0} File already exists (earlier conflict resolved)"
                    } else {
                        "\u{26a0} File already exists"
                    };
                    (
                        format!("{} \u{2014} Replace permanently deletes it.", base),
                        ui.visuals().error_fg_color,
                    )
                }
                LiveCollisionReason::InBatch if entry.target_is_dir || entry.source_is_dir => (
                    "\u{26a0} Another entry in this batch involves a folder; skipped.".to_string(),
                    ui.visuals().warn_fg_color,
                ),
                LiveCollisionReason::InBatch => (
                    "Another file in this batch targets the same name.".to_string(),
                    ui.visuals().warn_fg_color,
                ),
            };
            ui.label(RichText::new(message).color(color));

            ui.add_space(8.0);
            ui.separator();

            // Buttons
            let mut cancel_resp: Option<egui::Response> = None;
            let folder_involved = entry.target_is_dir || entry.source_is_dir;
            ui.horizontal(|ui| {
                // A folder on either side cannot be replaced, so Replace is
                // disabled for it (Skip remains available).
                if ui
                    .add_enabled(!folder_involved, egui::Button::new("Replace"))
                    .clicked()
                {
                    do_replace = true;
                }
                if ui.button("Replace All").clicked() {
                    do_replace_all = true;
                }
                if ui.button("Skip").clicked() {
                    do_skip = true;
                }
                if ui.button("Skip All").clicked() {
                    do_skip_all = true;
                }
                let resp = ui.button("Cancel");
                if resp.clicked() {
                    do_cancel = true;
                }
                cancel_resp = Some(resp);
            });

            if state.init_focus
                && let Some(resp) = cancel_resp
            {
                resp.request_focus();
            }

            // F2: extra Replace button that also executes immediately. Not offered
            // when a folder is involved — it cannot be replaced.
            if state.is_f2
                && cur + 1 >= total
                && !folder_involved
                && ui.button("Replace and Rename").clicked()
            {
                do_f2_exec = true;
            }

            state.init_focus = false;
        });
    });

    // Process actions outside the borrow.
    if do_cancel {
        app.collision_dialog = None;
        app.pending_plan = None;
        return;
    }

    if do_replace_all {
        // Respect prior choices: entries before `cur` keep their decisions.
        // All entries from `cur` onward become Replace — except folder targets,
        // which cannot be overwritten and are left to Skip.
        for e in state.entries.iter_mut().skip(cur) {
            if e.target_is_dir {
                e.decided_skip = true;
                e.decided_replace = false;
                continue;
            }
            e.decided_replace = true;
            e.decided_skip = false;
        }
        state.current = total; // trigger execution next frame
        app.collision_dialog = Some(state);
        return;
    }

    if do_skip_all {
        for e in state.entries.iter_mut().skip(cur) {
            e.decided_skip = true;
            e.decided_replace = false;
        }
        state.current = total;
        app.collision_dialog = Some(state);
        return;
    }

    if do_replace || do_f2_exec {
        if state.entries[cur].target_is_dir {
            // Defense in depth: a folder target can only be skipped.
            state.entries[cur].decided_skip = true;
            state.entries[cur].decided_replace = false;
        } else {
            state.entries[cur].decided_replace = true;
            state.entries[cur].decided_skip = false;
        }
    }

    if do_skip {
        state.entries[cur].decided_skip = true;
        state.entries[cur].decided_replace = false;
    }

    if do_replace || do_skip || do_f2_exec {
        state.current += 1;
    }

    app.collision_dialog = Some(state);
}

/// Shared context menu for a file row — used by both the main table and revert dialog.
/// The deferred action is stored in `deferred_action`.
pub(crate) fn draw_file_context_menu(
    ui: &mut egui::Ui,
    entry_idx: usize,
    _path: &std::path::Path,
    _current_name: &str,
    deferred_action: &mut Option<(usize, String)>,
) {
    ui.set_min_width(200.0);
    ui.menu_button("Copy to Clipboard", |ui| {
        if ui.button("Filename").clicked() {
            *deferred_action = Some((entry_idx, "copy_filename".into()));
            ui.close();
        }
        if ui.button("Full Path").clicked() {
            *deferred_action = Some((entry_idx, "copy_full_path".into()));
            ui.close();
        }
        if ui.button("Directory Path").clicked() {
            *deferred_action = Some((entry_idx, "copy_dir_path".into()));
            ui.close();
        }
    });
    if ui.button("Open Containing Folder").clicked() {
        *deferred_action = Some((entry_idx, "open_folder".into()));
        ui.close();
    }
    if ui.button("Open with Default App").clicked() {
        *deferred_action = Some((entry_idx, "open_file".into()));
        ui.close();
    }
    ui.separator();
    if ui.button("Properties").clicked() {
        *deferred_action = Some((entry_idx, "properties".into()));
        ui.close();
    }
}

/// Apply the selection change for a revert-dialog row click or keyboard
/// navigation (shift-click / shift+arrow ranges, ctrl-click toggles, plain
/// single select). Ranges are computed in DISPLAY order — the table is
/// sortable, so the range between the anchor and the clicked/navigated row
/// must cover the visually contiguous rows, not a raw entry-index span.
/// Takes disjoint fields (not `&mut RevertDialogState`) so it can be called
/// from inside the row-rendering closure, which already borrows
/// `state.display_order`. Extracted from `draw_revert_dialog` so the anchor
/// semantics are unit-testable without an egui context.
pub(crate) fn apply_revert_selection(
    selection: &mut [bool],
    display_order: &[usize],
    selection_anchor: &mut Option<usize>,
    last_clicked_idx: &mut Option<usize>,
    entry_idx: usize,
    ctrl: bool,
    shift: bool,
) {
    // Pin the range anchor on the first shift action of a range; keep it
    // fixed for later shift actions so the range always extends from the
    // first click of the current selection, not the most recent one.
    if shift && selection_anchor.is_none() {
        *selection_anchor = *last_clicked_idx;
    }
    if !shift {
        *selection_anchor = None;
    }
    if shift {
        if let Some(anchor) = *selection_anchor {
            let anchor_pos = display_order.iter().position(|&i| i == anchor);
            let click_pos = display_order.iter().position(|&i| i == entry_idx);
            if let (Some(ap), Some(cp)) = (anchor_pos, click_pos) {
                let (lo, hi) = if ap <= cp { (ap, cp) } else { (cp, ap) };
                select_display_range(selection, display_order, lo, hi);
            } else {
                // Stale anchor (not in display order) — degrade to a toggle.
                let action = ClickAction::Toggle(entry_idx);
                apply_click_action(selection, action);
            }
        } else {
            let action = ClickAction::Toggle(entry_idx);
            apply_click_action(selection, action);
        }
    } else {
        let action = compute_click_action(entry_idx, ctrl, shift, None);
        apply_click_action(selection, action);
    }
    // Active edge is always the clicked/navigated row (Range actions
    // previously clobbered it with the range start).
    *last_clicked_idx = Some(entry_idx);
}

mod revert; // the revert-confirmation dialog
mod windows; // per-frame window and popup drawing

// Re-exported for the other GUI panels.
pub(crate) use revert::draw_revert_dialog;
pub(crate) use windows::draw_windows;
