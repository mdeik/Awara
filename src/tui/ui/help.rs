use super::*;

pub(crate) fn draw_help_navigator(f: &mut Frame, app: &mut App, area: Rect) {
    let border_style = if app.mode == AppMode::Help {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let filter_name = app.help_filter_name();
    let title = format!(" Help: Commands [F:{}] ", filter_name);

    // Map the filtered entry indices to (entry_index, entry) pairs. Selection
    // is a position in this filtered view (never an entry index), so it always
    // stays in bounds and matches the filter by construction.
    let filtered_items = app
        .help_visible
        .iter()
        .filter_map(|&i| app.help_entries.get(i).map(|entry| (i, entry)))
        .collect::<Vec<_>>();

    // Keep the selection inside the filtered view (e.g. after the filter changes)
    let sel = app.help_state.selected().unwrap_or(0);
    if filtered_items.is_empty() {
        app.help_state.select(None);
    } else if sel >= filtered_items.len() {
        app.help_state.select(Some(0));
    }

    let items = build_help_items(&filtered_items, app.help_state.selected());

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border_style),
    );

    f.render_stateful_widget(list, area, &mut app.help_state);
}

pub(crate) fn build_help_items<'a>(
    entries: &[(usize, &'a HelpEntry)],
    selected: Option<usize>,
) -> Vec<ListItem<'a>> {
    let mut items = Vec::with_capacity(entries.len());
    let mut last_cat: &str = "";

    for (pos, &(_, entry)) in entries.iter().enumerate() {
        let is_selected = selected == Some(pos);
        let prefix = if is_selected { "▸ " } else { "  " };

        let command_line =
            Line::from(format!("{}{}", prefix, entry.command)).style(if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            });

        if entry.category != last_cat {
            last_cat = entry.category;
            let header_line = Line::from(format!("── {} ──", entry.category)).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
            items.push(ListItem::new(vec![header_line, command_line]));
        } else {
            items.push(ListItem::new(command_line));
        }
    }

    items
}

pub(crate) fn draw_help_preview(f: &mut Frame, app: &App, area: Rect) {
    let border_style = Style::default().fg(Color::Yellow);

    let example_text = if let Some(entry) = app.help_selected_entry() {
        if entry.usage.is_empty() {
            entry.example.to_string()
        } else {
            format!(
                "Usage:\n  {}\n\nExample:\n  {}",
                entry.usage,
                entry.example.replace("\n", "\n  ")
            )
        }
    } else {
        String::new()
    };

    let paragraph = Paragraph::new(example_text)
        .style(Style::default().fg(Color::Green))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Example ")
                .border_style(border_style),
        );

    f.render_widget(paragraph, area);
}

pub(crate) fn draw_help_config(f: &mut Frame, app: &App, area: Rect) {
    let border_style = Style::default().fg(Color::Yellow);

    let help_text = if let Some(entry) = app.help_selected_entry() {
        entry.description.to_string()
    } else {
        "Select a command from the list to see details.".to_string()
    };

    let paragraph = Paragraph::new(help_text)
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Description ")
                .border_style(border_style),
        );

    f.render_widget(paragraph, area);
}
