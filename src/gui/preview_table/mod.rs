use crate::gui::app::{
    GuiApp, SELECTION_DISPATCH_THROTTLE, defer_selection_dispatch, should_flush_selection_dispatch,
};
use crate::gui::helpers::{
    ClickAction, DiffSeg, FONT_BODY, HEADER_H, ROW_H, action_rearms, apply_click_action,
    compute_click_action, diff_colored, draw_table_header_cell, format_date,
    handle_box_selection_by_display_order, human_size, render_segs, select_display_range,
};
use crate::gui::table_shared::{draw_header_separator, paint_row_bg, rect_hovered};
use crate::gui::types::{COL_IDS, COL_LABELS, COL_MIN_WIDTH, ContextAction, GREEN, PreviewSortCol};
use eframe::egui::{
    self, Color32, RichText, ScrollArea, Sense, Vec2,
    containers::scroll_area::{DragScroll, ScrollSource},
    text::CCursorRange,
};
use std::cell::Cell;
mod deferred;
mod header;
mod rows;
mod table;

pub(crate) use table::draw_file_and_preview;

// ── Cell-drawing helpers ───────────────────────────────────────────────────

/// Allocate a cell-sized rect and draw a truncated label inside it.
fn draw_label_cell(ui: &mut egui::Ui, w: f32, text: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, ROW_H), Sense::hover());
    let mut cu = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    cu.add(
        egui::Label::new(RichText::new(text).color(color))
            .truncate()
            .selectable(false),
    );
}

/// Character count to preselect when an F2 inline edit begins: the file stem
/// (name without its extension), so typing replaces the name while the
/// extension is preserved. Shares the core pipeline's split rule via
/// `awara::split_extension_for`, so multi-part extensions like `.tar.gz` are
/// kept whole and dotfiles/extensionless names select in full. Extension
/// identification is files-only, so a directory always selects in full.
fn initial_selection_char_len(name: &str, is_dir: bool) -> usize {
    awara::split_extension_for(name, is_dir).0.chars().count()
}

/// Draw the F2 inline-edit TextEdit widget inside a cell.
fn draw_editing_cell(
    ui: &mut egui::Ui,
    idx: usize,
    w: f32,
    is_dir: bool,
    edit_buffer: &mut String,
    edit_pending_focus: &Cell<bool>,
) {
    let edit_id = ui.make_persistent_id(("f2_name_edit", idx));

    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, ROW_H), Sense::click());
    let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
    ));
    let mut output = egui::TextEdit::singleline(edit_buffer)
        .id(edit_id)
        .font(egui::TextStyle::Monospace)
        .show(&mut cu);

    if edit_pending_focus.get() {
        ui.memory_mut(|mem| mem.request_focus(edit_id));
        // On first focus, preselect the file stem (name without extension) so
        // typing replaces the name but keeps the extension. Directories have
        // no extension, so the whole name is preselected.
        let len = initial_selection_char_len(edit_buffer, is_dir);
        let select_stem =
            CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(len));
        output.state.cursor.set_char_range(Some(select_stem));
        output.state.store(ui.ctx(), edit_id);
        edit_pending_focus.set(false);
    }
}

// ── Row helpers ────────────────────────────────────────────────────────────

/// Populate the context menu for a preview row.
/// Multi-selection–aware: actions that support multiple files show `({count})`
/// and operate on all selected items.
fn draw_preview_context_menu(ui: &mut egui::Ui, app: &mut GuiApp, row_idx: usize) {
    let sel_count = app.selected_count();
    let count_suffix = if sel_count > 1 {
        format!(" ({sel_count})")
    } else {
        String::new()
    };
    ui.set_min_width(240.0);
    ui.menu_button("Copy to Clipboard", |ui| {
        if ui.button(format!("Filename{count_suffix}")).clicked() {
            let sel = app.selected_indices();
            app.copy_filename_multi(&sel, ui.ctx());
            ui.close();
        }
        if ui.button(format!("Full Path{count_suffix}")).clicked() {
            let sel = app.selected_indices();
            app.copy_full_path_multi(&sel, ui.ctx());
            ui.close();
        }
        if ui.button(format!("Directory Path{count_suffix}")).clicked() {
            let sel = app.selected_indices();
            app.copy_dir_path_multi(&sel, ui.ctx());
            ui.close();
        }
        if ui.button(format!("New Name{count_suffix}")).clicked() {
            let sel = app.selected_indices();
            app.copy_new_name_multi(&sel, ui.ctx());
            ui.close();
        }
    });
    if ui
        .button(format!("Open Containing Folder{count_suffix}"))
        .clicked()
    {
        let sel = app.selected_indices();
        app.set_pending_action(ContextAction::OpenFolders, sel);
        ui.close();
    }
    if ui
        .button(format!("Open with Default App{count_suffix}"))
        .clicked()
    {
        let sel = app.selected_indices();
        app.set_pending_action(ContextAction::OpenFiles, sel);
        ui.close();
    }
    ui.separator();
    ui.menu_button("Reposition", |ui| {
        if ui.button(format!("Move Up{count_suffix}")).clicked() {
            let sel = app.selected_indices();
            app.set_pending_action(ContextAction::MoveUp, sel);
            ui.close();
        }
        if ui.button(format!("Move Down{count_suffix}")).clicked() {
            let sel = app.selected_indices();
            app.set_pending_action(ContextAction::MoveDown, sel);
            ui.close();
        }
        ui.separator();
        if ui.button(format!("Move to Top{count_suffix}")).clicked() {
            let sel = app.selected_indices();
            app.set_pending_action(ContextAction::MoveToTop, sel);
            ui.close();
        }
        if ui.button(format!("Move to Bottom{count_suffix}")).clicked() {
            let sel = app.selected_indices();
            app.set_pending_action(ContextAction::MoveToBottom, sel);
            ui.close();
        }
        if app.custom_order.is_some() {
            ui.separator();
            if ui
                .button(format!("Reset to Sorted Order{count_suffix}"))
                .clicked()
            {
                let sel = app.selected_indices();
                app.set_pending_action(ContextAction::ResetOrder, sel);
                ui.close();
            }
        }
    });
    ui.separator();
    // Rename is single-file only — no count suffix.
    if ui.button("Rename").clicked() {
        app.pending_context_action = Some(ContextAction::Rename(row_idx));
        ui.close();
    }
    // Undo: show if ANY selected file has an undo operation (short-circuiting check).
    let any_undo = app
        .selection
        .iter()
        .enumerate()
        .any(|(i, &s)| s && app.find_undo_op(i).is_some());
    if any_undo && ui.button(format!("Undo{count_suffix}")).clicked() {
        let sel = app.selected_indices();
        app.set_pending_action(ContextAction::UndoFiles, sel);
        ui.close();
    }
    // Redo: show if ANY selected file has a redo operation (short-circuiting check).
    let any_redo = app
        .selection
        .iter()
        .enumerate()
        .any(|(i, &s)| s && app.find_redo_op(i).is_some());
    if any_redo && ui.button(format!("Redo{count_suffix}")).clicked() {
        let sel = app.selected_indices();
        app.set_pending_action(ContextAction::RedoFiles, sel);
        ui.close();
    }
    ui.separator();
    if ui.button(format!("Move to Trash{count_suffix}")).clicked() {
        let sel = app.selected_indices();
        app.set_pending_action(ContextAction::Trash, sel);
        ui.close();
    }
    ui.separator();
    if ui.button(format!("Properties{count_suffix}")).clicked() {
        let sel = app.selected_indices();
        app.set_pending_action(ContextAction::Properties, sel);
        ui.close();
    }
}

/// Handle row click with ctrl/shift modifiers.  Takes the ui for input state.
/// Apply the selection change for a row click (shift-click range, ctrl-click
/// toggle, or plain single select). Extracted from `handle_row_click` so the
/// anchor semantics are unit-testable without an egui context.
pub(crate) fn apply_row_click(app: &mut GuiApp, idx: usize, ctrl: bool, shift: bool) {
    if shift {
        // Pin the anchor on the first shift-click of a range; keep it fixed
        // for later shift-clicks so the range always extends from the first
        // click of the current selection, not the most recent click.
        if app.selection_anchor.is_none() {
            app.selection_anchor = app.last_clicked_idx;
        }
        if let Some(anchor) = app.selection_anchor {
            let display_order = app.get_display_order();
            let anchor_pos = display_order.iter().position(|&i| i == anchor);
            let click_pos = display_order.iter().position(|&i| i == idx);
            if let (Some(ap), Some(cp)) = (anchor_pos, click_pos) {
                let (lo, hi) = if ap <= cp { (ap, cp) } else { (cp, ap) };
                select_display_range(&mut app.selection, &display_order, lo, hi);
                app.last_clicked_idx = Some(idx);
                // Files were included in a new (range) selection → re-arm.
                app.set_processed(false);
            } else {
                let action = compute_click_action(idx, ctrl, shift, app.selection_anchor);
                app.last_clicked_idx = apply_click_action(&mut app.selection, action.clone());
                if action_rearms(&action, &app.selection) {
                    app.set_processed(false);
                }
            }
        } else {
            let action = ClickAction::Toggle(idx);
            app.last_clicked_idx = apply_click_action(&mut app.selection, action.clone());
            if action_rearms(&action, &app.selection) {
                app.set_processed(false);
            }
        }
    } else {
        app.selection_anchor = None;
        let action = compute_click_action(idx, ctrl, shift, app.last_clicked_idx);
        app.last_clicked_idx = apply_click_action(&mut app.selection, action.clone());
        // A pure deselect (toggle-off) must not re-arm the rest of the batch.
        if action_rearms(&action, &app.selection) {
            app.set_processed(false);
        }
    }
}

fn handle_row_click(
    app: &mut GuiApp,
    idx: usize,
    row_clicked_this_frame: &mut bool,
    ui: &egui::Ui,
) {
    app.preview_active = true;
    *row_clicked_this_frame = true;

    let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
    let shift = ui.input(|i| i.modifiers.shift);

    apply_row_click(app, idx, ctrl, shift);
    app.selection_generation += 1;

    if app.editing_idx.is_some() && Some(idx) != app.editing_idx {
        app.commit_edit();
    }
}

/// Compute the drop position for drag-to-reorder based on pointer Y.
fn compute_drop_target(
    pointer_y: f32,
    row_rects: &[(usize, egui::Rect)],
    range_start: usize,
) -> usize {
    if row_rects.is_empty() {
        return range_start;
    }
    if pointer_y < row_rects[0].1.top() {
        return range_start;
    }
    if pointer_y >= row_rects.last().unwrap().1.bottom() {
        return range_start + row_rects.len();
    }
    for (i, &(_, r)) in row_rects.iter().enumerate() {
        if pointer_y >= r.top() && pointer_y < r.bottom() {
            let mid = r.top() + r.height() / 2.0;
            return if pointer_y < mid {
                range_start + i
            } else {
                range_start + i + 1
            };
        }
    }
    range_start + row_rects.len()
}

/// Draw a drop-line indicator at `y` for drag-to-reorder.
fn draw_drop_indicator(ui: &mut egui::Ui, y: f32, total_width: f32) {
    let accent = ui.style().visuals.selection.bg_fill;
    let line_left = ui.min_rect().left();
    ui.painter().rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(line_left, y - 1.5),
            egui::pos2(line_left + total_width, y + 1.5),
        ),
        0.0,
        accent,
    );
    ui.painter()
        .circle_filled(egui::pos2(line_left + 6.0, y), 4.0, accent);
}

/// Edge-scroll for box selection.
///
/// ⚠ Reads and writes `egui::scroll_area::State` via the generic data API.
/// This is not a public egui API — if `scroll_area::State` changes in a
/// future version, update the two `mem.data.get_temp_mut_or_default` calls here.
fn handle_edge_scroll(
    ui: &egui::Ui,
    app: &mut GuiApp,
    press_in_preview: bool,
    any_drag: bool,
    visible_rows_height: f32,
    scroll_id: egui::Id,
) {
    if app.has_any_modal() || ui.ctx().any_popup_open() {
        app.preview_edge_drag_start = None;
        return;
    }
    // Only edge-scroll during an active box selection.
    if app.preview_box_select_anchor.is_none() {
        app.preview_edge_drag_start = None;
        return;
    }
    if !ui.input(|i| i.pointer.primary_down()) || !press_in_preview || any_drag {
        app.preview_edge_drag_start = None;
        return;
    }

    let edge_px = 30.0;
    let Some(pos) = ui.input(|i| i.pointer.interact_pos()) else {
        return;
    };

    let clip = ui.clip_rect();
    let scroll_delta = if pos.y < clip.top() + edge_px && pos.y >= clip.top() - 10.0 {
        -1.0
    } else if pos.y > clip.bottom() - edge_px && pos.y <= clip.bottom() + 10.0 {
        1.0
    } else {
        0.0
    };

    if scroll_delta == 0.0 {
        app.preview_edge_drag_start = None;
        return;
    }

    let viewport_h = clip.height();
    let max_offset = (visible_rows_height - viewport_h).max(0.0);

    // Read current scroll offset from the internal state.
    let current_offset = ui.ctx().memory_mut(|mem| {
        let state = mem
            .data
            .get_temp_mut_or_default::<egui::scroll_area::State>(scroll_id);
        let cur = state.offset.y;
        if (cur <= 0.0 && scroll_delta < 0.0) || (cur >= max_offset && scroll_delta > 0.0) {
            app.preview_edge_drag_start = None;
            None
        } else {
            Some(cur)
        }
    });
    let Some(current_offset) = current_offset else {
        return;
    };

    let elapsed = if let Some(start) = app.preview_edge_drag_start {
        start.elapsed().as_secs_f32()
    } else {
        app.preview_edge_drag_start = Some(std::time::Instant::now());
        0.0
    };

    let initial_speed = 0.3;
    let speed_rows = if elapsed < 0.4 {
        initial_speed
    } else {
        (initial_speed + (elapsed - 0.4) * 0.8).min(5.0)
    };

    let new_offset = (current_offset + speed_rows * ROW_H * scroll_delta).clamp(0.0, max_offset);

    // Write the updated offset back.
    ui.ctx().memory_mut(|mem| {
        let state = mem
            .data
            .get_temp_mut_or_default::<egui::scroll_area::State>(scroll_id);
        state.offset.y = new_offset;
    });
    ui.ctx().request_repaint();
}

#[cfg(test)]
mod tests {
    use super::initial_selection_char_len;

    /// Preselect length for a regular file.
    fn file(name: &str) -> usize {
        initial_selection_char_len(name, false)
    }

    #[test]
    fn preselects_stem_not_extension() {
        assert_eq!(file("photo.jpg"), 5);
        assert_eq!(file("a.txt"), 1);
        // Only the final extension is dropped (matches `Path::file_stem`).
        assert_eq!(file("archive.zip"), 7);
    }

    #[test]
    fn compound_extensions_kept_whole() {
        assert_eq!(file("archive.tar.gz"), 7);
        assert_eq!(file("backup.tar.bz2"), 6);
        assert_eq!(file("pkg.tar.xz"), 3);
        assert_eq!(file("log.tar.zst"), 3);
        // Case-insensitive on the whole compound suffix.
        assert_eq!(file("DATA.TAR.GZ"), 4);
        // A bare compound suffix (no stem) is a dotfile; only the leading dot
        // is treated as the extension boundary.
        assert_eq!(file(".tar.gz"), 4);
        // A near-miss still uses the single-extension rule.
        assert_eq!(file("notes.gz"), 5);
    }

    #[test]
    fn directories_select_in_full() {
        // Extension identification is files-only, so a directory selects whole
        // even when its name is dotted or looks compound.
        assert_eq!(initial_selection_char_len("my.folder", true), 9);
        assert_eq!(initial_selection_char_len("archive.tar.gz", true), 14);
        assert_eq!(initial_selection_char_len(".hidden", true), 7);
    }

    #[test]
    fn non_ascii_names_do_not_panic() {
        // The suffix-length byte offset lands inside a multi-byte character:
        // the boundary-safe check must skip it rather than panic.
        assert_eq!(file("\u{65e5}\u{672c}\u{8a9e}abcde"), 8);
        assert_eq!(file("caf\u{e9}.tar.gz"), 4);
    }

    #[test]
    fn dotfiles_and_extensionless_select_whole_name() {
        assert_eq!(file(".bashrc"), 7);
        assert_eq!(file("README"), 6);
        assert_eq!(file(""), 0);
    }

    #[test]
    fn char_count_not_byte_count() {
        // `é` is two bytes but one char: the cursor range is char-based.
        assert_eq!(file("caf\u{e9}.png"), 4);
    }
}
