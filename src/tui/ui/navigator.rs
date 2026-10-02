use super::*;

pub(super) fn draw_navigator(f: &mut Frame, app: &mut App, area: Rect) {
    if app.mode == AppMode::Help {
        draw_help_navigator(f, app, area);
        return;
    }

    let items: Vec<ListItem> = app
        .navigator_entries
        .iter()
        .map(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let is_selected = app.selection.contains(path);
            let prefix = if is_selected { "[x] " } else { "[ ] " };

            let size_str = if path.is_file() {
                if let Ok(meta) = path.metadata() {
                    human_size(meta.len())
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            // Pad name so sizes align, keep compact
            let display = if size_str.is_empty() {
                format!("{}{}", prefix, name)
            } else {
                format!("{}{}  {}", prefix, name, size_str)
            };

            let style = if path.is_dir() {
                Style::default().fg(Color::Blue)
            } else {
                Style::default()
            };

            ListItem::new(display).style(style)
        })
        .collect();

    let filter_str = if let Some(pat) = &app.filter_pattern {
        format!(" Filter:{}", pat)
    } else {
        "".to_string()
    };

    let scanning_tag = if app.scanning_in_progress {
        " [SCANNING... Esc to cancel]"
    } else {
        ""
    };

    let depth_str = if let Some(d) = app.max_depth {
        format!(" D:{}", d)
    } else {
        String::new()
    };

    let title = format!(
        "Navigator [R:{} H:{} S:{:?} F:{:?}{}{}]{} ",
        if app.recursive { "ON" } else { "OFF" },
        if app.show_hidden { "ON" } else { "OFF" },
        app.sort_mode,
        app.filter_mode,
        filter_str,
        depth_str,
        scanning_tag
    );

    let highlight_style = if app.mode == AppMode::Navigator {
        Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(Color::Yellow)
    } else {
        Style::default()
    };

    let border_style = if app.mode == AppMode::Navigator {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .highlight_style(highlight_style)
        .highlight_symbol("");

    f.render_stateful_widget(list, area, &mut app.navigator_state);
}
