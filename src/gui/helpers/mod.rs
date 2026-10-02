pub(crate) use crate::gui::types::{
    DIFF_ADD_BG, DIFF_KEEP_BG, DIFF_REMOVE_BG, DiffSeg, selection_bg,
};
use awara::{
    DatePosition, NumberingMode, RenameConfig, RenameItem, resolve_from_to_range,
    resolve_insert_pos,
};
use eframe::egui::{self, Color32, Margin, Vec2};

// ── Constants ──

pub(crate) const ROW_H: f32 = 20.0;
pub(crate) const HEADER_H: f32 = 22.0;

// ── Font sizes ──
// Role-named rather than number-named, so a size change is one edit here
// instead of a sweep across the GUI. These also feed the FontId used by
// text measurement (see `dyn_combo_width_iter`), which must stay in step
// with the labels it measures.
pub(crate) const FONT_TINY: f32 = 10.0; // badges, path hints, compact buttons
pub(crate) const FONT_LABEL: f32 = 11.0; // field/control labels (the workhorse)
pub(crate) const FONT_TITLE: f32 = 11.5; // section group titles
pub(crate) const FONT_BODY: f32 = 13.0; // body text; matches egui's TextStyle::Body default
pub(crate) const FONT_TREE: f32 = 14.0; // directory-tree labels

// ── Macro: wraps section_group call with automatic label + order formatting ──

/// Shorthand for section_group with label formatting and automatic *d = true on reset.
#[macro_export]
macro_rules! sec_group {
    ($ui:expr, $en:expr, $d:expr, $order:expr, $cfg:expr, $id:ident, $label:expr, $body:expr, $reset:expr) => {{
        let idx = SectionId::$id.as_usize();
        let _prev = $en[idx];
        let order_label = $order(SectionId::$id);
        let full_label = if order_label.is_empty() {
            $label.to_string()
        } else {
            format!("{} ({})", $label, order_label)
        };
        if section_group(
            $ui,
            &mut $en[idx],
            &full_label,
            section_is_modified($cfg, SectionId::$id),
            $body,
        ) {
            $reset;
            $en[idx] = true;
            *$d = true;
        }
        if _prev != $en[idx] {
            // Checkbox was toggled — mark preview dirty
            *$d = true;
        }
    }};
}

/// Allocate a column with fixed width weight and run inner content.
#[macro_export]
macro_rules! sec_col {
    ($ui:expr, $gap:expr, $cw:expr, $w:expr, $inner:expr) => {{
        let _w = $cw($w);
        $ui.allocate_ui_with_layout(
            egui::vec2(_w, $ui.available_height()),
            egui::Layout::top_down(egui::Align::LEFT),
            |col_ui: &mut egui::Ui| {
                $inner(col_ui);
            },
        );
        $ui.add_space($gap);
    }};
}

mod date;
mod diff;
mod table;
mod timestamps;
mod widgets;

#[cfg(test)]
mod tests_diff;
#[cfg(test)]
mod tests_widgets;

pub(crate) use date::{
    CUSTOM_FMT, FMT_COMBO_LEN, build_date_fmt, fmt_combo_entry, fmt_index, split_date_fmt,
};
pub(crate) use diff::{FrontEndDiff, diff_colored, render_segs};
pub(crate) use table::{
    BoxSelectionCtx, ClickAction, action_rearms, apply_click_action, compute_click_action,
    draw_table_header_cell, handle_box_selection_by_display_order, select_display_range,
};
pub(crate) use timestamps::{build_fixed_datetime, split_hms};
pub(crate) use widgets::{
    arrow_button, arrow_indicator, cb_tip, combo_enum_tip, combo_opt_tip, drag_field_tip,
    drag_from_field_tip, drag_label_tip, dyn_combo_width, dyn_combo_width_iter, fixed_text,
    fixed_text_enabled, format_date, human_size, int_drag, linked_from_to_values, section_group,
};

// Re-exported for tests only (see `tests_widgets`). Gated on `cfg(test)` so
// these never exist in a normal build and cannot quietly hide a dead import.
#[cfg(test)]
pub(crate) use diff::ExactOp;
#[cfg(test)]
pub(crate) use widgets::{INT_DRAG_SPEED, linked_from_to};
