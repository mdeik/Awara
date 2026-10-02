use super::*;

pub(super) fn draw_config(f: &mut Frame, app: &mut App, area: Rect) {
    if app.mode == AppMode::Help {
        draw_help_config(f, app, area);
        return;
    }

    let config_items = app.get_config_items();
    let items: Vec<ListItem> = config_items
        .iter()
        .enumerate()
        .map(|(i, (label, value, _))| {
            let _is_selected = Some(i) == app.config_state.selected();
            let is_dragged = Some(i) == app.config_drag_index;

            let is_editing = app
                .editing_value
                .as_ref()
                .and_then(|(ed_key, _)| app.config_display_order.get(i).map(|dk| dk == ed_key))
                .unwrap_or(false);

            // Determine if this is a sub-option (should be indented under a main item).
            // Sub-options are recognized by key patterns that suggest they're secondary flags
            // belonging to a primary operation.
            let is_sub = is_sub_option(label, i, &config_items);

            let prefix = if is_dragged {
                " "
            } else if is_editing {
                ">  "
            } else if is_sub {
                "   "
            } else {
                "  "
            };

            let style = if is_editing {
                Style::default().fg(Color::Green)
            } else if is_sub {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };

            if is_sub {
                ListItem::new(format!("{}  {}: {}", prefix, label, value)).style(style)
            } else {
                ListItem::new(format!("{} {}: {}", prefix, label, value)).style(style)
            }
        })
        .collect();

    let highlight_style = if app.mode == AppMode::Config {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let border_style = if app.mode == AppMode::Config {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let stacking_str = if app.stacking_mode {
        " [Stacking: ON]"
    } else {
        " [Stacking: OFF]"
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Pending Config{} ", stacking_str))
                .border_style(border_style),
        )
        .highlight_style(highlight_style)
        .highlight_symbol("");

    f.render_stateful_widget(list, area, &mut app.config_state);
}

/// Determine if a config item is a sub-option that should be indented under its parent.
/// Uses naming conventions: sub-options start with the parent's name as a prefix.
/// E.g. "Num Start" is a sub-option of "Numbering".
fn is_sub_option(label: &str, index: usize, all_items: &[(String, String, String)]) -> bool {
    // Stacking mode items are always top-level
    if label.starts_with("step_") || label.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return false;
    }

    // Known parent prefixes — an item is a sub-option if its key starts with
    // a recognized parent prefix and the parent appears before it in the list.
    let parent_prefixes = [
        ("Num ", "Numbering"),
        ("Ext ", "Extension"),
        ("Dirname ", "Add Dirname"),
        ("Replace ", "Replace"),
        ("Regex ", "Regex"),
        ("Remove ", "Remove"),
        ("Filter ", "Filter"),
    ];

    for (prefix, parent_name) in &parent_prefixes {
        if label.starts_with(prefix) {
            // Check if parent appears before this item in the list
            for j in 0..index {
                if all_items.get(j).map(|(n, _, _)| n.as_str()) == Some(parent_name) {
                    return true;
                }
            }
        }
    }

    false
}
