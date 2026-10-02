use crate::tui::app::{App, AppMode, HelpEntry, StatusSeverity};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Wrap},
};
// TUI drawing, split by panel.
mod config; // the config/options panel
mod diff; // diff highlighting between old and new names
mod help; // the help panel
mod navigator; // the file navigator panel
mod status; // the status bar and console

use config::draw_config;
pub use diff::diff_strings;
pub(super) use diff::human_size;
pub(super) use help::{draw_help_config, draw_help_navigator, draw_help_preview};
use navigator::draw_navigator;
use status::{draw_console, draw_status};

#[cfg(test)]
mod help_tests;
#[cfg(test)]
mod tests;

pub fn draw(f: &mut Frame, app: &mut App) {
    // Help list selection is a position in the filtered view; rebuild the view
    // every frame so filter changes (F cycle / Esc reset) can't desync it.
    // Must run before the panes: draw_help_preview draws before draw_help_navigator.
    app.help_visible = app.compute_help_visible();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(10),   // Preview (Flexible)
            Constraint::Min(10),   // Navigator + Config
            Constraint::Max(2),    // Status
            Constraint::Length(3), // Console
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_preview(f, app, chunks[1]);

    let mid_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[2]);

    draw_navigator(f, app, mid_chunks[0]);
    draw_config(f, app, mid_chunks[1]);

    draw_status(f, app, chunks[3]);
    draw_console(f, app, chunks[4]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let mode_str = match app.mode {
        AppMode::Navigator => "NAVIGATOR",
        AppMode::Preview => "PREVIEW",
        AppMode::Config => "CONFIG",
        AppMode::Console => "CONSOLE",
        AppMode::Help => "HELP",
    };

    let dry_run_str = if app.dry_run { " [DRY-RUN]" } else { "" };

    let text = vec![Line::from(vec![
        Span::raw("Awara | "),
        Span::styled(
            format!("Path: {} | ", app.cwd.display()),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            format!("Mode: {} | ", mode_str),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(format!("Selected: {}", app.selection.len())),
        if app.dry_run {
            Span::styled(dry_run_str.to_string(), Style::default().fg(Color::Red))
        } else {
            Span::raw(dry_run_str)
        },
    ])];

    let p = Paragraph::new(text);
    f.render_widget(p, area);
}

fn draw_preview(f: &mut Frame, app: &mut App, area: Rect) {
    if app.mode == AppMode::Help {
        draw_help_preview(f, app, area);
        return;
    }

    let is_active = app.mode == AppMode::Preview;
    let highlight_style = if is_active {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let border_style = if is_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let header = Row::new(vec!["Original", "→", "New", "Type", "Status"])
        .style(border_style)
        .height(1)
        .bottom_margin(1);

    let rows = app.preview_items.iter().map(|item| {
        let row_style = match item.status.as_str() {
            "Collision" => Style::default().fg(Color::Red),
            "Unchanged" => Style::default().fg(Color::DarkGray),
            _ => Style::default(),
        };

        let orig_spans = &item.orig_spans;
        let new_spans = &item.new_spans;

        let type_cell =
            Cell::from(if item.is_dir { "Folder" } else { "File" }).style(if item.is_dir {
                Style::default().fg(Color::Blue)
            } else {
                Style::default()
            });

        Row::new(vec![
            Cell::from(Line::from(orig_spans.clone())),
            Cell::from("→"),
            Cell::from(Line::from(new_spans.clone())),
            type_cell,
            Cell::from(item.status.clone()),
        ])
        .style(row_style)
    });

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(40),
            Constraint::Length(3),
            Constraint::Percentage(40),
            Constraint::Length(8),
            Constraint::Percentage(12),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Preview")
            .border_style(border_style),
    )
    .row_highlight_style(highlight_style);

    f.render_stateful_widget(table, area, &mut app.preview_state);
}
