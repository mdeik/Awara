use super::*;

pub(super) fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    // Severity-based color
    let color = match app.status_severity {
        StatusSeverity::Error => Color::Red,
        StatusSeverity::Warning => Color::Yellow,
        StatusSeverity::Success => Color::Green,
        StatusSeverity::Info => Color::White,
    };

    // Show keybinding hints when status is idle
    let msg = if app.scanning_in_progress {
        format!("[SCANNING...] {}", app.status_message)
    } else if app.status_message == "Ready" || app.status_message == "Ready. Press ? for help." {
        format!(
            "{} — Keys: Tab=Focus ?=Help R=Recursive H=Hidden F=Filter s=Sort S=Dir | type --help in console",
            app.status_message
        )
    } else {
        app.status_message.clone()
    };

    // Clip to available width so long messages don't get silently truncated
    let max_width = area.width.saturating_sub(1) as usize;
    let display = if msg.len() > max_width {
        format!("{}…", &msg[..max_width.saturating_sub(1)])
    } else {
        msg
    };

    let p = Paragraph::new(display).style(Style::default().fg(color));
    f.render_widget(p, area);
}

pub(super) fn draw_console(f: &mut Frame, app: &App, area: Rect) {
    let border_style = if app.mode == AppMode::Console {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = if app.editing_value.is_some() {
        if let Some((ref key, _)) = app.editing_value {
            format!("Editing: {}", key)
        } else {
            "Console".to_string()
        }
    } else {
        "Console".to_string()
    };

    let input = Paragraph::new(app.input.clone())
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        );
    f.render_widget(input, area);

    // Draw cursor
    if app.mode == AppMode::Console {
        f.set_cursor_position(ratatui::layout::Position::new(
            area.x + app.input_cursor_position as u16 + 1,
            area.y + 1,
        ));
    }
}
