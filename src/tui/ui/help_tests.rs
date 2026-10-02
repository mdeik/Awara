use super::*;
use crate::tui::ui::help::build_help_items;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

fn entry(command: &'static str, category: &'static str) -> HelpEntry {
    HelpEntry {
        command,
        description: "",
        usage: "",
        example: "",
        category,
    }
}

/// Render the built items into a buffer and return the visible lines
/// (right-trimmed), mirroring what the help navigator actually draws.
fn render_items(items: Vec<ListItem>) -> Vec<String> {
    let width = 60;
    let height = 12;
    let buf = render_buffer(items);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// Render the built items into a buffer so tests can inspect cell styles.
/// This mirrors the help navigator: the List has no highlight_style —
/// selection styling lives in the item lines themselves.
fn render_buffer(items: Vec<ListItem>) -> Buffer {
    let width = 60;
    let height = 12;
    let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
    List::new(items).render(buf.area, &mut buf);
    buf
}

/// The first command of each section must render below its header, never
/// in place of it (regression: Case section appeared to have no commands).
#[test]
fn test_first_command_of_section_renders_below_header() {
    let e0 = entry("--case-name <MODE>", "Case");
    let e1 = entry("--case-exception <WORD>", "Case");
    let e2 = entry("--remove-first <N>", "Remove");
    let items = build_help_items(&[(0usize, &e0), (1, &e1), (2, &e2)], None);
    let lines = render_items(items);

    assert!(
        lines[0].contains("── Case ──"),
        "header line shown: {:?}",
        lines[0]
    );
    assert!(
        lines[1].contains("--case-name"),
        "first command still visible below header: {:?}",
        lines[1]
    );
    assert!(lines[2].contains("--case-exception"));
    assert!(lines[3].contains("── Remove ──"));
    assert!(lines[4].contains("--remove-first"));
}

/// A section with a single command (the worst case for the original bug)
/// must still show that command under its header.
#[test]
fn test_single_command_section_still_shows_command() {
    let e0 = entry("--extension-mode <MODE>", "Extension");
    let items = build_help_items(&[(0usize, &e0)], None);
    let lines = render_items(items);

    assert!(lines[0].contains("── Extension ──"));
    assert!(
        lines[1].contains("--extension-mode"),
        "only command must render below its header: {:?}",
        lines[1]
    );
}

/// Selection is a position in the filtered view, NOT an entry index. The
/// rendered items here carry entry indices 25..27; selecting position 1
/// must highlight the middle item (entry 26). Under the old entry-index
/// semantics this selection would match nothing (entry index 1 isn't in
/// the slice) and no item would get the ▸ marker.
#[test]
fn test_selection_is_position_not_entry_index() {
    let e25 = entry("--case-name <MODE>", "Case");
    let e26 = entry("--case-exception <WORD>", "Case");
    let e27 = entry("--remove-first <N>", "Remove");
    let items = build_help_items(&[(25usize, &e25), (26, &e26), (27, &e27)], Some(1));
    let lines = render_items(items);

    assert!(lines[0].contains("── Case ──"));
    assert!(
        lines[1].starts_with("  "),
        "position 0 unselected: {:?}",
        lines[1]
    );
    assert!(
        lines[2].contains("▸ --case-exception"),
        "position 1 selected: {:?}",
        lines[2]
    );
    assert!(lines[3].contains("── Remove ──"));
    assert!(
        lines[4].starts_with("  "),
        "position 2 unselected: {:?}",
        lines[4]
    );
}

/// Selecting the first command of a section must highlight the command
/// only — the section header above it keeps its Cyan style (regression:
/// the List's highlight_style used to recolor the whole item row, turning
/// the header yellow along with the command).
#[test]
fn test_selected_header_keeps_cyan_style() {
    let e0 = entry("--case-name <MODE>", "Case");
    let e1 = entry("--case-exception <WORD>", "Case");
    let items = build_help_items(&[(0usize, &e0), (1, &e1)], Some(0));
    let buf = render_buffer(items);

    assert_eq!(
        buf[(2, 0)].fg,
        Color::Cyan,
        "header line must keep its own color when the item below is selected"
    );
    assert_eq!(
        buf[(2, 1)].fg,
        Color::Yellow,
        "selected command line gets the highlight color"
    );
}

/// Item order mirrors the filtered view order exactly — each rendered item
/// keeps its position in the entries slice passed in.
#[test]
fn test_item_order_matches_entry_order() {
    let e0 = entry("A", "One");
    let e1 = entry("B", "Two");
    let e2 = entry("C", "Two");
    let items = build_help_items(&[(0usize, &e0), (1, &e1), (2, &e2)], Some(1));
    let lines = render_items(items);

    assert!(lines[0].contains("── One ──"));
    assert!(lines[1].contains("A"));
    assert!(lines[2].contains("── Two ──"));
    assert!(
        lines[3].contains("▸ B"),
        "selected item keeps its prefix: {:?}",
        lines[3]
    );
    assert!(lines[4].starts_with("  C"));
}
