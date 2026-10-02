use super::*;
/// Draw a sortable, resizable header cell. Returns the interaction result.
pub(crate) struct HeaderDraw {
    /// The cell was clicked — caller should toggle sort.
    pub(crate) clicked: bool,
    /// Horizontal drag delta from the resize handle (0.0 if not being dragged).
    pub(crate) resize_delta: f32,
    /// The resize handle was released this frame — caller should persist widths.
    pub(crate) drag_stopped: bool,
}

pub(crate) fn draw_table_header_cell(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    is_active: bool,
    sort_asc: bool,
    ns: &'static str,
    col_idx: usize,
) -> HeaderDraw {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, HEADER_H), egui::Sense::hover());

    // ── Label with sort indicator ──
    {
        let text = if is_active {
            if sort_asc {
                format!("{} \u{25B2}", label)
            } else {
                format!("{} \u{25BC}", label)
            }
        } else {
            label.to_string()
        };
        let mut child_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(
                    egui::pos2(rect.left(), rect.top()),
                    egui::pos2(rect.right() - 6.0, rect.bottom()),
                ))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        child_ui.add(
            egui::Label::new(egui::RichText::new(text).size(FONT_BODY))
                .truncate()
                .selectable(false),
        );
    }

    // ── Separator + 6px drag handle ──
    let sep_color = ui.style().visuals.widgets.noninteractive.bg_stroke.color;
    let sep_x = rect.right();
    ui.painter().line_segment(
        [
            egui::pos2(sep_x, rect.top()),
            egui::pos2(sep_x, rect.bottom()),
        ],
        egui::Stroke::new(1.0_f32, sep_color),
    );
    let handle_rect = egui::Rect::from_min_size(
        egui::pos2(sep_x - 3.0, rect.top()),
        egui::vec2(6.0, HEADER_H),
    );
    let dr = ui.interact(
        handle_rect,
        egui::Id::new((ns, "resize", col_idx)),
        egui::Sense::drag(),
    );
    let resize_delta = if dr.dragged() { dr.drag_delta().x } else { 0.0 };
    if dr.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
    }

    // ── Sort click ──
    let sort_resp = ui.interact(
        rect,
        egui::Id::new((ns, "sort", col_idx)),
        egui::Sense::click(),
    );

    HeaderDraw {
        clicked: sort_resp.clicked(),
        resize_delta,
        drag_stopped: dr.drag_stopped(),
    }
}

/// Describes the selection change from a single row click.
#[derive(Clone)]
pub(crate) enum ClickAction {
    /// Select only this index.
    Single(usize),
    /// Toggle this index only.
    Toggle(usize),
    /// Select a range from start to end (inclusive).
    Range { start: usize, end: usize },
}

/// Compute the intended selection change from a row click.
pub(crate) fn compute_click_action(
    idx: usize,
    ctrl: bool,
    shift: bool,
    last_clicked: Option<usize>,
) -> ClickAction {
    if shift {
        if let Some(anchor) = last_clicked {
            let start = anchor.min(idx);
            let end = anchor.max(idx);
            ClickAction::Range { start, end }
        } else {
            ClickAction::Toggle(idx)
        }
    } else if ctrl {
        ClickAction::Toggle(idx)
    } else {
        ClickAction::Single(idx)
    }
}

/// After an Apply, selection actions that include a file in a new selection
/// re-arm the rename guard; a pure deselect (toggle-off) does not.
/// `sel` must reflect the selection state *after* the action was applied.
pub(crate) fn action_rearms(action: &ClickAction, sel: &[bool]) -> bool {
    match action {
        ClickAction::Single(_) => true,
        ClickAction::Range { .. } => true,
        ClickAction::Toggle(i) => sel.get(*i).copied().unwrap_or(false),
    }
}

/// Select file indices whose display positions fall in [lo, hi] (inclusive).
/// SSoT for display-order-aware range selection, used by shift-click in
/// preview_table.rs and (potentially) shift+arrow in app.rs.
pub(crate) fn select_display_range(sel: &mut [bool], order: &[usize], lo: usize, hi: usize) {
    for s in sel.iter_mut() {
        *s = false;
    }
    for &file_idx in order.iter().take(hi + 1).skip(lo) {
        if let Some(s) = sel.get_mut(file_idx) {
            *s = true;
        }
    }
}
pub(crate) fn apply_click_action(sel: &mut [bool], action: ClickAction) -> Option<usize> {
    match action {
        ClickAction::Single(i) => {
            for (j, s) in sel.iter_mut().enumerate() {
                *s = j == i;
            }
            Some(i)
        }
        ClickAction::Toggle(i) => {
            if let Some(s) = sel.get_mut(i) {
                *s = !*s;
            }
            Some(i)
        }
        ClickAction::Range { start, end } => {
            for (j, s) in sel.iter_mut().enumerate() {
                *s = j >= start && j <= end;
            }
            Some(start)
        }
    }
}

/// Handle click-and-drag box selection — draws a semi-transparent selection
/// rectangle and selects every row it intersects.
///
/// `extra_condition` is an additional guard (e.g. checking `press_in_preview`).
/// When `additive` is true, the box adds rows to the existing selection instead
/// of replacing it (used for Ctrl/Shift + drag-box).
/// `row_filter` is called for each intersected row; only rows where it returns
/// true are selected (e.g. checking `revertable` in the revert dialog).
/// Returns `true` if the box selection was active this frame, or an
/// empty-space click cleared the selection.
/// Box selection using **display-order positions** instead of screen-space rects.
/// The `anchor` is captured ONCE when the drag starts (from press_origin), not recomputed
/// every frame — this is critical for edge-scrolling: as the viewport scrolls, the screen-space
/// `press_origin` maps to different content rows, so we must freeze the anchor at drag-start.
/// Context for box selection by display order.
///
/// Bundles the state needed to map screen coordinates to display positions
/// and mutate the selection array and anchor.
pub(crate) struct BoxSelectionCtx<'a> {
    pub visible_start: usize,
    pub display_order: &'a [usize],
    pub sel: &'a mut [bool],
    /// Anchor may fall outside `[0, len)` when the drag starts in the empty
    /// space above/below the list — the selection range is intersected with
    /// the real row range below, so out-of-bounds anchors select nothing.
    pub anchor: &'a mut Option<isize>,
}

/// `extra_condition` gates the selection (e.g. press-in-preview guard).
pub(crate) fn handle_box_selection_by_display_order(
    ui: &egui::Ui,
    row_rects: &[(usize, egui::Rect)],
    ctx: &mut BoxSelectionCtx<'_>,
    extra_condition: bool,
    additive: bool,
) -> bool {
    if !ui.input(|i| i.pointer.primary_down()) {
        // Drag ended — clear anchor.
        *ctx.anchor = None;
        return false;
    }
    let inside_vscroll = ui
        .input(|i| i.pointer.press_origin())
        .is_some_and(|origin| ui.clip_rect().contains(origin));
    if !inside_vscroll {
        *ctx.anchor = None;
        return false;
    }
    let (Some(origin), Some(current)) = (
        ui.input(|i| i.pointer.press_origin()),
        ui.input(|i| i.pointer.interact_pos()),
    ) else {
        return false;
    };

    let on_row = row_rects.iter().any(|(_, r)| r.contains(origin));
    if on_row || row_rects.is_empty() || !extra_condition {
        *ctx.anchor = None;
        return false;
    }

    // When the origin is outside the rendered rows (above first or below last),
    // only proceed on an actual drag (origin ≠ current), not a plain click.
    let first_top = row_rects[0].1.top();
    let last_bot = row_rects.last().unwrap().1.bottom();
    if origin.y < first_top || origin.y >= last_bot {
        let dy = (current.y - origin.y).abs();
        if dy < 4.0 {
            // A plain click (not a drag) in the empty space above/below the rows:
            // deselect, mirroring the "outside the table columns" branch below.
            // The click must not drag-select anything, but it should clear the
            // selection — clicking empty space = deselect.
            *ctx.anchor = None;
            for s in ctx.sel.iter_mut() {
                *s = false;
            }
            return true;
        }
        // Drag from empty space into rows — allow box selection to proceed.
    }

    let sel_box = egui::Rect::from_two_pos(origin, current);

    // Convert screen-space Y → display-order position. Unclamped: when the
    // pointer is in the empty space above the first or below the last row,
    // the position falls outside `[0, len)` instead of snapping to the edge
    // row (which would select a row the box never touches).
    let screen_y_to_display = |screen_y: f32| -> isize {
        let first_top = row_rects[0].1.top();
        let rel_y = screen_y - first_top;
        let row_off = (rel_y / ROW_H).floor() as isize;
        ctx.visible_start as isize + row_off
    };

    // Check whether the selection box overlaps the table's X range.
    // When dragging outside the columns, clear the selection but still draw the box.
    let table_left = row_rects[0].1.left();
    let table_right = row_rects[0].1.right();
    let box_x_min = sel_box.min.x.min(sel_box.max.x);
    let box_x_max = sel_box.min.x.max(sel_box.max.x);
    let x_overlaps = box_x_min < table_right && box_x_max > table_left;

    if x_overlaps {
        // Capture the anchor ONCE at the start of the drag.
        if ctx.anchor.is_none() {
            *ctx.anchor = Some(screen_y_to_display(origin.y));
        }
        let anchor_pos = ctx.anchor.unwrap();
        let current_pos = screen_y_to_display(current.y);

        let (lo, hi) = if anchor_pos <= current_pos {
            (anchor_pos, current_pos)
        } else {
            (current_pos, anchor_pos)
        };

        if !additive {
            for s in ctx.sel.iter_mut() {
                *s = false;
            }
        }

        // Intersect the range with the real row positions. If the box is
        // entirely above the first or below the last row, the intersection is
        // empty and nothing is selected — matching the drawn highlight.
        let len = ctx.display_order.len() as isize;
        let sel_lo = lo.max(0);
        let sel_hi = hi.min(len - 1);
        if sel_lo <= sel_hi {
            // Select the range using display_order to map display positions → file indices.
            for dp in sel_lo..=sel_hi {
                if let Some(&file_idx) = ctx.display_order.get(dp as usize)
                    && let Some(s) = ctx.sel.get_mut(file_idx)
                {
                    *s = true;
                }
            }
        }
    } else {
        // Outside table columns — clear selections and reset anchor.
        *ctx.anchor = None;
        if !additive {
            for s in ctx.sel.iter_mut() {
                *s = false;
            }
        }
    }

    // Always draw the visual selection box regardless of X overlap.
    let dm = ui.style().visuals.dark_mode;
    let accent = selection_bg(dm);
    let box_color = if dm {
        // Dark mode: lighter version of selection bg so it's visible
        Color32::from_rgba_premultiplied(60, 66, 78, 80)
    } else {
        // Light mode: a clearly visible blue that doesn't wash out to white
        Color32::from_rgba_premultiplied(201, 215, 255, 235)
    };
    ui.painter().rect_filled(sel_box, 0.0, box_color);
    ui.painter().rect_stroke(
        sel_box,
        0.0,
        egui::Stroke::new(1.5_f32, accent),
        egui::StrokeKind::Middle,
    );

    ui.ctx().request_repaint();
    true
}
