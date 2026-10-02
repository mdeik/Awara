use super::*;

/// Every category must appear in exactly one contiguous run in the help
/// list, so each section header is followed by its own commands.
#[test]
fn test_help_cycle_filter_resets_selection_to_first_visible() {
    let mut app = App::new();
    app.help_state.select(Some(42));
    app.help_cycle_filter();
    let cat = app.help_filter_name();
    assert_ne!(cat, "All", "cycling from All must land on a category");
    app.help_visible = app.compute_help_visible();
    assert_eq!(app.help_state.selected(), Some(0));
    assert!(!app.help_visible.is_empty());
    assert_eq!(app.help_entries[app.help_visible[0]].category, cat);
}

#[test]
fn test_help_visible_matches_filter_category() {
    let mut app = App::new();
    app.help_cycle_filter();
    let cat = app.help_filter_name();
    app.help_visible = app.compute_help_visible();
    assert!(!app.help_visible.is_empty());
    for &i in &app.help_visible {
        assert_eq!(app.help_entries[i].category, cat);
    }
    // Visible indices are unique and in ascending entry order
    let mut sorted = app.help_visible.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted, app.help_visible);
}

#[test]
fn test_help_selected_entry_maps_position_to_entry() {
    let mut app = App::new();
    app.help_visible = app.compute_help_visible(); // All filter
    app.help_state.select(Some(0));
    let e = app.help_selected_entry().expect("entry at position 0");
    assert_eq!(e.command, app.help_entries[app.help_visible[0]].command);
    // A position past the end of the filtered view yields None
    app.help_state.select(Some(app.help_visible.len()));
    assert!(app.help_selected_entry().is_none());
}

/// Every category must appear in exactly one contiguous run in the help
/// list, so each section header is followed by its own commands.
#[test]
fn test_help_categories_are_contiguous() {
    let entries = help_entries();
    let mut seen = std::collections::HashSet::new();
    let mut last_cat: &str = "";
    for e in &entries {
        if e.category != last_cat {
            assert!(
                !seen.contains(e.category),
                "category '{}' appears in multiple sections",
                e.category
            );
            seen.insert(e.category);
            last_cat = e.category;
        }
    }
}

/// The Case section must list every case flag (regression: it appeared
/// empty because the first command of each section was hidden by its
/// section header).
#[test]
fn test_help_case_section_lists_all_case_commands() {
    let entries = help_entries();
    let case: Vec<&str> = entries
        .iter()
        .filter(|e| e.category == "Case")
        .map(|e| e.command)
        .collect();
    assert_eq!(
        case.len(),
        2,
        "Case section should list both case flags, got: {:?}",
        case
    );
    assert!(case.iter().any(|c| c.contains("--case-name")));
    assert!(case.iter().any(|c| c.contains("--case-exception")));
}

/// Every declared console command must appear in the help menu (regression:
/// sort, save-preset, and load-preset used to have no help entries).
#[test]
fn test_help_covers_every_console_command() {
    let entries = help_entries();
    for meta in parser::CONSOLE_COMMANDS {
        assert!(
            entries
                .iter()
                .any(|e| e.category == "TUI Console" && e.command.contains(meta.name)),
            "console command '{}' has no help entry",
            meta.name
        );
    }
}

/// The TUI Console help section is generated from the command registry, so
/// it must match it exactly — no missing, extra, or mistyped entries.
#[test]
fn test_help_console_section_matches_command_registry() {
    let entries = help_entries();
    let console_entries: Vec<&str> = entries
        .iter()
        .filter(|e| e.category == "TUI Console")
        .map(|e| e.command)
        .collect();

    let expected: Vec<String> = parser::CONSOLE_COMMANDS
        .iter()
        .map(|m| m.help_label())
        .collect();

    assert_eq!(
        console_entries.len(),
        expected.len(),
        "console help has {} entries but the registry declares {}",
        console_entries.len(),
        expected.len()
    );
    for exp in &expected {
        assert!(
            console_entries.contains(&exp.as_str()),
            "help is missing the entry for '{}'",
            exp
        );
    }
}

#[test]
fn test_stacking_fallthrough_filter_flags() {
    let mut app = App::new();
    app.stacking_mode = true;
    use crate::tui::parser::parse_command;
    let _ = parse_command("--filter-files --filter-hidden", &mut app.pending_config);
    assert!(app.pending_config.filters.filter_files);
    assert!(app.pending_config.filters.filter_hidden);
    assert!(app.pending_config.command_order.is_empty());
}
