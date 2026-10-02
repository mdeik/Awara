use super::*;
/// Add a [`TextEdit::singleline`] that occupies exactly `size`, regardless of content.
/// Returns the widget response for chaining (`.on_hover_text()`, `.changed()`, etc.).
pub(crate) fn fixed_text(
    ui: &mut egui::Ui,
    size: impl Into<egui::Vec2>,
    text: &mut String,
) -> egui::Response {
    fixed_text_enabled(ui, size, text, true)
}

/// Like [`fixed_text`] but with an explicit enabled/disabled state.
pub(crate) fn fixed_text_enabled(
    ui: &mut egui::Ui,
    size: impl Into<egui::Vec2>,
    text: &mut String,
    enabled: bool,
) -> egui::Response {
    let size = size.into();
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
    ));
    let inner = child_ui.add_enabled(enabled, egui::TextEdit::singleline(text));
    // Return the allocated rect (so Grid sees the correct column width) but
    // merge the TextEdit's interactivity (changed, hover, etc.) onto it.
    response.union(inner)
}
// ── Field helpers — bridge egui's &mut String requirement to RenameConfig's Option<T> ──

/// Drag resolution for integer fields, in value-units per logical pixel.
///
/// [`egui::DragValue`]'s "smart aim" snaps a dragged value to the roundest
/// number within `speed × aim_radius` of the precise drag position (see
/// `emath::smart_aim`). At `speed(1)` with the default one-point aim radius that
/// window is two units wide, so it can span two integers and pick the rounder
/// one — leaving values like `9` and `24` impossible to drag to (`8` snaps to
/// `10`, `23` to `25`). egui's own default speed for integer types is `0.25`,
/// which shrinks the window to half a unit so every integer stays reachable;
/// exact values can still be typed. Keep this below `0.5`.
pub(crate) const INT_DRAG_SPEED: f64 = 0.25;

/// Build an integer [`egui::DragValue`] with the SSoT drag resolution
/// ([`INT_DRAG_SPEED`]) already applied. Every numeric field should go through
/// this rather than calling `egui::DragValue::new` directly, so the speed has a
/// single source and no field can silently inherit egui's float default of `1`.
/// Chain `.range(...)`, `.suffix(...)`, etc.
pub(crate) fn int_drag<'a, T: egui::emath::Numeric>(value: &'a mut T) -> egui::DragValue<'a> {
    egui::DragValue::new(value).speed(INT_DRAG_SPEED)
}

/// Label-only half of drag_val_tip for use in multi-column layouts.
pub(crate) fn drag_label_tip(
    ui: &mut egui::Ui,
    label: &str,
    _value: &mut Option<isize>,
    _dirty: &mut bool,
    _max: usize,
    _tip: &str,
) {
    ui.label(egui::RichText::new(label).size(FONT_LABEL));
}

/// Field-only half of `drag_val_tip` for use in multi-column layouts.
/// The actual value (including 0) is always stored: a field at 0 is
/// displayed as 0 and treated as "not configured" by change detection
/// (see `awara::opt_num_default`), never as cleared.
pub(crate) fn drag_field_tip(
    ui: &mut egui::Ui,
    value: &mut Option<isize>,
    dirty: &mut bool,
    max: usize,
    tip: &str,
) {
    let max_i = max as isize;
    let mut v = value.unwrap_or(0);
    if ui
        .add_sized(
            egui::vec2(44.0, 22.0),
            int_drag(&mut v).range(-max_i..=max_i),
        )
        .on_hover_text(tip)
        .changed()
    {
        *value = Some(v);
        *dirty = true;
    }
}

/// Core linked From/To rule shared by Remove From/To and Name Segment: when
/// the From position is increased to a non-negative value and To equals the
/// previous From, drag To along so the range keeps its size. Decreasing From
/// never changes To, and negative positions never trigger the linkage.
pub(crate) fn linked_from_to_values(prev_from: isize, new_from: isize, to: &mut isize) {
    if prev_from >= 0 && new_from > prev_from && *to == prev_from {
        *to = new_from;
    }
}

/// Option-wrapped variant of [`linked_from_to_values`] for Remove From/To,
/// where a cleared field displays as 0 (`None` counts as 0 in the
/// comparisons; the stored value never becomes `None` from the linkage).
pub(crate) fn linked_from_to(
    prev_from: Option<isize>,
    new_from: Option<isize>,
    to: &mut Option<isize>,
) {
    let prev = prev_from.unwrap_or(0);
    let cur = new_from.unwrap_or(0);
    let mut t = to.unwrap_or(0);
    linked_from_to_values(prev, cur, &mut t);
    *to = Some(t);
}

/// Field-only half of `drag_val_tip` for the "From" position of Remove
/// From/To; unlike [`drag_field_tip`] it keeps the "To" field linked when
/// the From value is increased (see [`linked_from_to`]).
pub(crate) fn drag_from_field_tip(
    ui: &mut egui::Ui,
    from_value: &mut Option<isize>,
    to_value: &mut Option<isize>,
    dirty: &mut bool,
    max: usize,
    tip: &str,
) {
    let max_i = max as isize;
    let prev_from = *from_value;
    let mut v = from_value.unwrap_or(0);
    if ui
        .add_sized(
            egui::vec2(44.0, 22.0),
            int_drag(&mut v).range(-max_i..=max_i),
        )
        .on_hover_text(tip)
        .changed()
    {
        let new_from = Some(v);
        linked_from_to(prev_from, new_from, to_value);
        *from_value = new_from;
        *dirty = true;
    }
}

/// ComboBox over `(label, value)` string options, with a hover tooltip.
///
/// Takes the table as pairs rather than parallel label/value slices, so a Core
/// SSoT list can be passed straight in (no per-frame copy) and the two halves
/// cannot fall out of step.
pub(crate) fn combo_opt_tip(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut Option<String>,
    options: &[(&str, &str)],
    dirty: &mut bool,
    tip: &str,
) {
    let current = value.as_deref().unwrap_or("");
    let idx = options.iter().position(|(_, v)| *v == current).unwrap_or(0);
    let w = dyn_combo_width_iter(ui, options.iter().map(|(label, _)| *label));
    egui::ComboBox::from_id_salt(id)
        .selected_text(options[idx].0)
        .width(w)
        .show_ui(ui, |ui| {
            for (i, (label, val)) in options.iter().enumerate() {
                if ui.selectable_label(idx == i, *label).clicked() {
                    *value = Some((*val).to_string());
                    *dirty = true;
                }
            }
        })
        .response
        .on_hover_text(tip);
}

/// The generic sibling of [`combo_opt_tip`]: a combo over `(label, Option<T>)`
/// options, where `None` is the "off" entry and is stored as-is.
///
/// For a config field that holds an enum rather than a string. Two parallel lists
/// (labels plus values) used to be indexed by position here, which silently does
/// the wrong thing if they ever diverge — one table of pairs cannot.
pub(crate) fn combo_enum_tip<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: &str,
    cur: &mut Option<T>,
    options: &[(&str, Option<T>)],
    dirty: &mut bool,
    tip: &str,
) {
    let idx = options.iter().position(|(_, v)| v == cur).unwrap_or(0);
    egui::ComboBox::from_id_salt(id)
        .selected_text(options[idx].0)
        .width(dyn_combo_width_iter(
            ui,
            options.iter().map(|(label, _)| *label),
        ))
        .show_ui(ui, |ui| {
            for (label, val) in options.iter() {
                if ui.selectable_label(*val == *cur, *label).clicked() {
                    *cur = *val;
                    *dirty = true;
                }
            }
        })
        .response
        .on_hover_text(tip);
}

/// Inline checkbox with a hover tooltip.
pub(crate) fn cb_tip(ui: &mut egui::Ui, d: &mut bool, b: &mut bool, l: &str, tip: &str) {
    if ui
        .add(egui::Checkbox::new(b, l))
        .on_hover_text(tip)
        .changed()
    {
        *d = true;
    }
}

/// Measure the width needed for the longest option string in a combo.
/// Uses the actual layout engine for accurate accumulated glyph widths.
///
/// Takes an iterator, so a combo whose options come from a table can be measured
/// without collecting them into a `Vec` first.
pub(crate) fn dyn_combo_width_iter<'a>(
    ui: &mut egui::Ui,
    options: impl Iterator<Item = &'a str>,
) -> f32 {
    let padding = 36.0; // dropdown arrow + frame margins
    let font_id = egui::FontId::proportional(FONT_LABEL);
    // Scale by 1.08 to account for glyph extent beyond advance (kerning, overhangs)
    // that accumulates on longer strings
    let scale = 1.08;
    ui.ctx().fonts_mut(|f| {
        options
            .map(|s| s.chars().map(|c| f.glyph_width(&font_id, c)).sum::<f32>() * scale)
            .fold(0.0, f32::max)
            .ceil()
            + padding
    })
}

/// Measure the width needed for a combo's options, given as a slice.
pub(crate) fn dyn_combo_width(ui: &mut egui::Ui, options: &[&str]) -> f32 {
    dyn_combo_width_iter(ui, options.iter().copied())
}

/// Draw a small triangular arrow button (up or down) using the painter.
/// Returns the click response.
pub(crate) fn arrow_button(ui: &mut egui::Ui, up: bool) -> egui::Response {
    let size = egui::vec2(22.0, 20.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        // Button background
        let bg = if response.hovered() || response.is_pointer_button_down_on() {
            ui.style().visuals.widgets.active.bg_fill
        } else {
            ui.style().visuals.widgets.inactive.bg_fill
        };
        ui.painter().rect_filled(rect, 4.0, bg);
        if response.hovered() {
            ui.painter().rect_stroke(
                rect,
                4.0,
                ui.style().visuals.widgets.hovered.bg_stroke,
                egui::StrokeKind::Middle,
            );
        }

        // Triangle arrow
        let cx = rect.center().x;
        let cy = rect.center().y;
        let s = 5.0;
        let points = if up {
            vec![
                egui::pos2(cx, cy - s),
                egui::pos2(cx - s, cy + s),
                egui::pos2(cx + s, cy + s),
            ]
        } else {
            vec![
                egui::pos2(cx, cy + s),
                egui::pos2(cx - s, cy - s),
                egui::pos2(cx + s, cy - s),
            ]
        };
        let arrow_color = if response.hovered() {
            ui.style().visuals.widgets.active.text_color()
        } else {
            ui.style().visuals.widgets.inactive.text_color()
        };
        ui.painter().add(egui::Shape::convex_polygon(
            points,
            arrow_color,
            egui::Stroke::NONE,
        ));
    }
    response
}

/// Draw a disabled (greyed-out) up/down arrow indicator.
/// Same as [`arrow_button`] but tinted grey and unclickable.
pub(crate) fn arrow_indicator(ui: &mut egui::Ui, up: bool) {
    let size = egui::vec2(22.0, 20.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(
            rect,
            4.0,
            ui.style()
                .visuals
                .widgets
                .inactive
                .bg_fill
                .gamma_multiply(0.4),
        );
        let cx = rect.center().x;
        let cy = rect.center().y;
        let s = 5.0;
        let points = if up {
            vec![
                egui::pos2(cx, cy - s),
                egui::pos2(cx - s, cy + s),
                egui::pos2(cx + s, cy + s),
            ]
        } else {
            vec![
                egui::pos2(cx, cy + s),
                egui::pos2(cx - s, cy - s),
                egui::pos2(cx + s, cy - s),
            ]
        };
        ui.painter().add(egui::Shape::convex_polygon(
            points,
            egui::Color32::GRAY,
            egui::Stroke::NONE,
        ));
    }
}

// ── Section group wrapper ──

pub(crate) fn section_group(
    ui: &mut egui::Ui,
    enabled: &mut bool,
    label: &str,
    modified: bool,
    body: impl FnOnce(&mut egui::Ui, &mut bool),
) -> bool {
    let mut clicked = false;
    let mut frame = egui::Frame::group(ui.style()).inner_margin(Margin::symmetric(6, 4));
    // Slightly wider border than the default 1.0. The width is fixed so the
    // border size (and thus layout) never changes — only the color signals
    // "modified".
    frame.stroke.width = 1.5;
    if modified && *enabled {
        let purple = if ui.style().visuals.dark_mode {
            egui::Color32::from_rgb(93, 75, 95)
        } else {
            egui::Color32::from_rgb(170, 80, 220)
        };
        frame.stroke.color = purple;
    }
    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            ui.add_sized(egui::vec2(14.0, 14.0), egui::Checkbox::new(enabled, ""));
            let resp = ui.add(
                egui::Button::new(egui::RichText::new("R").size(FONT_TINY).strong())
                    .min_size(egui::vec2(18.0, 14.0)),
            );
            if resp.clicked() {
                clicked = true;
            }
            ui.label(egui::RichText::new(label).size(FONT_TITLE).strong());
        });
        ui.add_space(2.0);
        body(ui, enabled);
    });
    clicked
}
// ── Helpers ──

pub(crate) fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub(crate) fn format_date(secs: i64) -> String {
    if secs < 0 {
        return "—".into();
    }
    // SSoT: `awara::format_date` renders an instant in local time using the
    // offset in effect at that instant (not the current offset).
    use std::time::{Duration, UNIX_EPOCH};
    let sys_time = UNIX_EPOCH + Duration::from_secs(secs as u64);
    awara::format_date("YYYY-MM-DD hh:mm:ss", sys_time, true, None)
}
