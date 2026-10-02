use super::*;
use awara::RenameItem;
use clap::Parser;

/// Helper: parse CLI flags and return RenameItems (SSoT via args.rs)
pub(super) fn parse_to_items(cmd: &str) -> Vec<RenameItem> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let mut args = vec!["awara"];
    args.extend(parts);
    if let Ok(cli_args) = crate::args::Args::try_parse_from(args) {
        let c = cli_args.into_config();
        return awara::config_to_items(&c);
    }
    Vec::new()
}

/// Helper: verify parse_to_items produces items for a rename-operation flag
pub(super) fn assert_parse_items(cmd: &str, expected_count: usize) {
    let items = parse_to_items(cmd);
    assert_eq!(
        items.len(),
        expected_count,
        "parse_to_items({:?}) should produce {} item(s), got {}: {:?}",
        cmd,
        expected_count,
        items.len(),
        items
    );
}

/// Helper: verify parse_to_items produces NO items for a non-operation flag
pub(super) fn assert_parse_no_items(cmd: &str) {
    let items = parse_to_items(cmd);
    assert!(
        items.is_empty(),
        "parse_to_items({:?}) should produce NO items, got: {:?}",
        cmd,
        items
    );
}

#[test]
fn test_sync_config_order_stacking_deterministic() {
    let mut app = App::new();
    app.stacking_mode = true;

    // Add 12 items to command_order (enough to test numerical > lexicographic)
    use awara::RenameItem;
    for i in 1..=12 {
        app.pending_config
            .command_order
            .push(RenameItem::RemoveFirst(i));
    }

    // Clear display order so it gets fully rebuilt
    app.config_display_order.clear();
    app.sync_config_order();

    // Verify numerical order: step_0, step_1, ..., step_9, step_10, step_11
    // NOT lexicographic: step_0, step_1, step_10, step_11, step_2, ...
    let expected: Vec<String> = (0..12).map(|i| format!("step_{}", i)).collect();
    assert_eq!(
        app.config_display_order, expected,
        "Stacking items should be in numerical step order, not lexicographic"
    );

    // Verify stability: calling sync_config_order again does not reorder
    let order_before = app.config_display_order.clone();
    app.sync_config_order();
    assert_eq!(
        app.config_display_order, order_before,
        "sync_config_order should be stable — items should not reorder on subsequent calls"
    );
}

#[test]
fn test_sync_config_order_normal_deterministic() {
    let mut app = App::new();
    app.stacking_mode = false;

    // Set some named fields in a non-canonical order internally
    app.pending_config.remove.remove_first = Some(3);
    app.pending_config.add.add_prefix = Some("pre_".to_string());
    app.pending_config.case.case_name = Some(awara::CaseMode::Title);
    app.pending_config.numbering.numbering_mode = Some(awara::NumberingMode::Suffix);
    app.pending_config.remove.remove_symbols = true;

    // Clear display order so it gets fully rebuilt
    app.config_display_order.clear();
    app.sync_config_order();

    // Verify canonical order: Prefix should come first, Remove First earlier than Remove Symbols,
    // Case Name before Numbering, etc.
    let order = app.config_display_order.clone();
    assert_eq!(
        order.first().map(|s| s.as_str()),
        Some("Prefix"),
        "Prefix should be first in canonical order"
    );

    let prefix_pos = order.iter().position(|k| k == "Prefix").unwrap();
    let remove_first_pos = order.iter().position(|k| k == "Remove First").unwrap();
    let remove_symbols_pos = order.iter().position(|k| k == "Remove Symbols").unwrap();
    let case_name_pos = order.iter().position(|k| k == "Case Name").unwrap();
    let numbering_pos = order.iter().position(|k| k == "Numbering").unwrap();

    assert!(
        prefix_pos < remove_first_pos,
        "Prefix should come before Remove First"
    );
    assert!(
        remove_first_pos < remove_symbols_pos,
        "Remove First should come before Remove Symbols"
    );
    assert!(
        remove_symbols_pos < case_name_pos,
        "Remove Symbols should come before Case Name"
    );
    assert!(
        case_name_pos < numbering_pos,
        "Case Name should come before Numbering"
    );

    // Verify stability
    let order_before = order.clone();
    app.sync_config_order();
    assert_eq!(
        app.config_display_order, order_before,
        "sync_config_order should be stable in normal mode"
    );
}

#[test]
fn test_toggle_stacking_preserves_order() {
    let mut app = App::new();
    app.stacking_mode = false;

    // Start with some named fields in normal mode
    app.pending_config.add.add_prefix = Some("pre_".to_string());
    app.pending_config.add.add_suffix = Some("_suf".to_string());
    app.pending_config.remove.remove_first = Some(2);
    app.pending_config.remove.remove_symbols = true;
    app.pending_config.case.case_name = Some(awara::CaseMode::Title);

    // Also set non-operation fields that aren't RenameItem variants
    app.pending_config.copy_to.output_dir = Some("./out".to_string());
    app.pending_config.copy_to.copy_mode = true;
    app.pending_config.stop_on_error = true;
    app.pending_config.filters.min_name_len = Some(3);
    app.pending_config.filters.max_name_len = Some(50);
    app.pending_config.filters.min_path_len = Some(5);
    app.pending_config.filters.max_path_len = Some(200);

    // Switch to stacking mode ON
    app.toggle_stacking_mode();
    assert!(app.stacking_mode, "Should now be in stacking mode");

    // Verify command_order has all items
    assert_eq!(
        app.pending_config.command_order.len(),
        5,
        "All 5 named fields should be converted to command items"
    );

    // Named fields should be cleared
    assert!(app.pending_config.add.add_prefix.is_none());
    assert!(app.pending_config.remove.remove_first.is_none());

    // Non-operation fields should survive the toggle
    assert_eq!(
        app.pending_config.copy_to.output_dir,
        Some("./out".to_string()),
        "output_dir should survive stacking toggle ON"
    );
    assert!(
        app.pending_config.copy_to.copy_mode,
        "copy_mode should survive stacking toggle ON"
    );
    assert!(
        app.pending_config.stop_on_error,
        "stop_on_error should survive stacking toggle ON"
    );
    assert_eq!(
        app.pending_config.filters.min_name_len,
        Some(3),
        "min_name_len should survive stacking toggle ON"
    );
    assert_eq!(
        app.pending_config.filters.max_name_len,
        Some(50),
        "max_name_len should survive stacking toggle ON"
    );
    assert_eq!(
        app.pending_config.filters.min_path_len,
        Some(5),
        "min_path_len should survive stacking toggle ON"
    );
    assert_eq!(
        app.pending_config.filters.max_path_len,
        Some(200),
        "max_path_len should survive stacking toggle ON"
    );

    // Verify display order is deterministic (step_0..step_4)
    let expected_stack: Vec<String> = (0..5).map(|i| format!("step_{}", i)).collect();
    assert_eq!(
        app.config_display_order, expected_stack,
        "Stacking mode should show items in step order"
    );

    // Switch back to normal mode OFF
    app.toggle_stacking_mode();
    assert!(!app.stacking_mode, "Should now be in normal mode");

    // Verify named fields are restored
    assert_eq!(app.pending_config.add.add_prefix, Some("pre_".to_string()));
    assert_eq!(app.pending_config.add.add_suffix, Some("_suf".to_string()));
    assert_eq!(app.pending_config.remove.remove_first, Some(2));
    assert!(app.pending_config.remove.remove_symbols);
    assert_eq!(
        app.pending_config.case.case_name,
        Some(awara::CaseMode::Title)
    );

    // Non-operation fields should survive the round-trip
    assert_eq!(
        app.pending_config.copy_to.output_dir,
        Some("./out".to_string()),
        "output_dir should survive stacking round-trip"
    );
    assert!(
        app.pending_config.copy_to.copy_mode,
        "copy_mode should survive stacking round-trip"
    );
    assert!(
        app.pending_config.stop_on_error,
        "stop_on_error should survive stacking round-trip"
    );
    assert_eq!(
        app.pending_config.filters.min_name_len,
        Some(3),
        "min_name_len should survive stacking round-trip"
    );
    assert_eq!(
        app.pending_config.filters.max_name_len,
        Some(50),
        "max_name_len should survive stacking round-trip"
    );
    assert_eq!(
        app.pending_config.filters.min_path_len,
        Some(5),
        "min_path_len should survive stacking round-trip"
    );
    assert_eq!(
        app.pending_config.filters.max_path_len,
        Some(200),
        "max_path_len should survive stacking round-trip"
    );

    // Verify display order is deterministic canonical order
    let order = &app.config_display_order;
    assert!(
        !order.is_empty(),
        "Normal mode should have items in display order"
    );
    assert_eq!(
        order.first().map(|s| s.as_str()),
        Some("Prefix"),
        "Prefix should be first after toggling back"
    );
    assert_eq!(
        order.get(1).map(|s| s.as_str()),
        Some("Suffix"),
        "Suffix should be second after toggling back"
    );
}

#[test]
fn test_describe_command() {
    // describe_command now returns the raw command to avoid duplicating flag-name mappings
    assert_eq!(
        App::describe_command("--add-prefix test"),
        "--add-prefix test"
    );
    assert_eq!(App::describe_command("unknown command"), "unknown command");
    assert_eq!(
        App::describe_command("Error: unknown command"),
        "Error: unknown command"
    );
    assert_eq!(App::describe_command(""), "");
}

// ── Stacking interaction: parse_command_to_items ──

#[test]
fn test_parse_to_items_replace() {
    assert_parse_items("--replace foo --with bar", 1);
}

#[test]
fn test_parse_to_items_regex() {
    assert_parse_items("--regex-match IMG_(\\d+) --regex-replace vacation_$1", 1);
}

#[test]
fn test_parse_to_items_prefix() {
    assert_parse_items("--add-prefix test_", 1);
}

#[test]
fn test_parse_to_items_suffix() {
    assert_parse_items("--add-suffix _final", 1);
}

#[test]
fn test_parse_to_items_numbering() {
    assert_parse_items("--numbering-mode prefix --numbering-pad 3", 1);
}

#[test]
fn test_parse_to_items_case() {
    // case_name is converted by config_to_items; case_ext removed
    assert_parse_items("--case-name lower", 1);
}

#[test]
fn test_parse_to_items_remove_first() {
    assert_parse_items("--remove-first 3", 1);
}

#[test]
fn test_parse_to_items_remove_digits() {
    assert_parse_items("--remove-digits", 1);
}

#[test]
fn test_parse_to_items_extension() {
    assert_parse_items("--extension-mode lower --extension .txt", 1);
}

#[test]
fn test_parse_to_items_add_dirname() {
    assert_parse_items("--add-dirname --dirname-sep -", 1);
}

#[test]
fn test_parse_to_items_move_part() {
    assert_parse_items("--move-part 1:3:5", 1);
}

#[test]
fn test_parse_to_items_copy_part() {
    assert_parse_items("--copy-part 2:4:6", 1);
}

#[test]
fn test_parse_to_items_add_date() {
    assert_parse_items("--add-date YYYY-MM-DD", 1);
}

#[test]
fn test_parse_to_items_insert_meta() {
    assert_parse_items("--insert-meta taken_original", 1);
    assert_parse_items("--insert-meta modified", 1);
    // Only the tags the engine implements parse: the `exif:<Tag>` family is not
    // one of them, so it is rejected here instead of reaching the config as a
    // silent no-op.
    let err =
        crate::args::Args::try_parse_from(["awara", "--insert-meta", "exif:DateTimeOriginal"])
            .expect_err("an unimplemented tag must not parse");
    assert!(err.to_string().contains("unknown metadata tag"), "{err}");
}

#[test]
fn test_parse_to_items_crop() {
    assert_parse_items("--crop before:prefix", 1);
}

#[test]
fn test_parse_to_items_remove_lead_dots() {
    assert_parse_items("--remove-lead-dots single", 1);
}

#[test]
fn test_parse_to_items_name_segment() {
    // name_segment_from/to are now converted by config_to_items (DEFAULT_ORDER alignment)
    assert_parse_items("--name-segment-from 1 --name-segment-to 4", 1);
    // to=0 means "through the end" and the display cmd round-trips.
    assert_parse_items("--name-segment-from 3 --name-segment-to 0", 1);
}

#[test]
fn test_parse_to_items_combined() {
    assert_parse_items(
        "--add-prefix pre_ --add-suffix _suf --remove-first 2 --case-name title",
        4,
    );
}

// ── Display cmd round-trips: the stacked-item command strings emitted by
// `rename_item_display` must parse back into the same item. ──

#[test]
fn test_display_cmd_roundtrip_remove_from_to_negative() {
    let items = parse_to_items("--remove-from=-3 --remove-to=-1");
    assert_eq!(
        items,
        vec![RenameItem::RemoveFromTo(-3, -1)],
        "negative from/to must round-trip"
    );
    // The space form works for positive values too.
    assert_eq!(
        parse_to_items("--remove-from 3 --remove-to 5"),
        vec![RenameItem::RemoveFromTo(3, 5)]
    );
}

#[test]
fn test_display_cmd_roundtrip_name_segment() {
    let items = parse_to_items("--name-segment-from=-3 --name-segment-to=-1");
    assert_eq!(items, vec![RenameItem::NameSegment(-3, -1)]);
}

#[test]
fn test_display_cmd_roundtrip_numbering() {
    let items = parse_to_items(
        "--numbering-mode suffix --numbering-start 2 --numbering-increment=-1 \
             --numbering-pad 3 --numbering-sep _ --numbering-type hex --numbering-case lower",
    );
    assert_eq!(
        items,
        vec![RenameItem::Numbering(
            awara::NumberingMode::Suffix,
            2,
            -1,
            3,
            Some("_".into()),
            None,
            Some("hex".into()),
            Some("lower".into()),
            None,
        )]
    );
}

#[test]
fn test_display_cmd_roundtrip_extension() {
    let items = parse_to_items("--extension-mode lower --extension txt");
    assert_eq!(
        items,
        vec![RenameItem::Extension(
            awara::ExtensionMode::Lower,
            Some("txt".into()),
            None,
            false,
        )]
    );
    let items = parse_to_items("--extension-mode upper --extension-add _bak --extension-remove");
    assert_eq!(
        items,
        vec![RenameItem::Extension(
            awara::ExtensionMode::Upper,
            None,
            Some("_bak".into()),
            true,
        )]
    );
}

#[test]
fn test_display_cmd_roundtrip_dirname() {
    let items = parse_to_items("--add-dirname --dirname-sep _ --dirname-pos=-1");
    assert_eq!(
        items,
        vec![RenameItem::Dirname(true, Some("_".into()), Some(-1))]
    );
}

#[test]
fn test_display_cmd_roundtrip_case_title_enhanced() {
    // `TitleEnhanced` kebab-cases to `title-enhanced`, not `titleenhanced`.
    let items = parse_to_items("--case-name title-enhanced");
    assert_eq!(
        items,
        vec![RenameItem::CaseName(awara::CaseMode::TitleEnhanced)]
    );
}

// ── Stacking interaction: non-operation commands produce NO items ──

#[test]
fn test_parse_to_items_no_output_dir() {
    assert_parse_no_items("--output-dir ./renamed");
}

#[test]
fn test_parse_to_items_no_copy_mode() {
    assert_parse_no_items("--copy-mode");
}

#[test]
fn test_parse_to_items_no_stop_on_error() {
    assert_parse_no_items("--stop-on-error");
}

#[test]
fn test_parse_to_items_no_preserve_timestamps() {
    assert_parse_no_items("--preserve-timestamps");
}

#[test]
fn test_parse_to_items_no_length_filters() {
    assert_parse_no_items("--min-name-len 3 --max-name-len 50");
}

#[test]
fn test_parse_to_items_no_filter_flags() {
    assert_parse_no_items("--filter-files --filter-hidden --filter-subfolders");
}

#[test]
fn test_parse_to_items_no_set_attributes() {
    assert_parse_no_items("--set-attributes readonly");
}

#[test]
fn test_parse_to_items_no_set_timestamps() {
    assert_parse_no_items("--set-created current");
}

#[test]
fn test_parse_to_items_no_keep_structure() {
    assert_parse_no_items("--keep-structure");
}

#[test]
fn test_copy_defaults_to_true_with_output_dir() {
    // Docs: "Copy Not Move ... Enabled by default" — copy is the default
    // whenever an output directory is set.
    let parts = vec!["awara", "--output-dir", "./out"];
    let cli_args = crate::args::Args::try_parse_from(parts).unwrap();
    let c = cli_args.into_config();
    assert!(c.copy_to.copy_mode, "copy should be the default");
}

#[test]
fn test_move_flag_disables_copy_mode() {
    let parts = vec!["awara", "--move", "--output-dir", "./out"];
    let cli_args = crate::args::Args::try_parse_from(parts).unwrap();
    let c = cli_args.into_config();
    assert!(
        !c.copy_to.copy_mode,
        "--move should force move instead of copy"
    );
}

#[test]
fn test_parse_to_items_no_exclude_regex() {
    assert_parse_no_items("--exclude-regex \\.bak$");
}

// ── Stacking mode: non-op commands still work via fallthrough to parse_command ──

#[test]
fn test_stacking_fallthrough_output_dir() {
    let mut app = App::new();
    app.stacking_mode = true;
    // Enter a non-rename command in stacking mode — it should fall through to parse_command
    use crate::tui::parser::parse_command;
    let _ = parse_command("--output-dir ./renamed", &mut app.pending_config);
    assert_eq!(
        app.pending_config.copy_to.output_dir,
        Some("./renamed".to_string()),
        "non-operation commands should still work via fallthrough in stacking mode"
    );
    // No items should have been added to command_order
    assert!(
        app.pending_config.command_order.is_empty(),
        "non-operation commands should not add to command_order"
    );
}

#[test]
fn test_stacking_fallthrough_stop_on_error() {
    let mut app = App::new();
    app.stacking_mode = true;
    use crate::tui::parser::parse_command;
    let _ = parse_command("--stop-on-error", &mut app.pending_config);
    assert!(
        app.pending_config.stop_on_error,
        "stop_on_error should work via fallthrough in stacking mode"
    );
    assert!(app.pending_config.command_order.is_empty());
}

#[test]
fn test_stacking_fallthrough_length_filters() {
    let mut app = App::new();
    app.stacking_mode = true;
    use crate::tui::parser::parse_command;
    let _ = parse_command(
        "--min-name-len 3 --max-path-len 200",
        &mut app.pending_config,
    );
    assert_eq!(app.pending_config.filters.min_name_len, Some(3));
    assert_eq!(app.pending_config.filters.max_path_len, Some(200));
    assert!(app.pending_config.command_order.is_empty());
}

#[test]
fn test_parse_rejects_invalid_timestamp_and_accepts_ampm() {
    // Invalid fixed timestamps are rejected by the shared clap parser (SSoT),
    // so the TUI never silently persists an unusable value.
    let bad = vec!["awara", "--set-modified", "garbage"];
    assert!(crate::args::Args::try_parse_from(bad).is_err());

    // AM/PM is accepted and normalized to canonical 24-hour form.
    let ok = vec!["awara", "--set-modified", "2024-01-15 02:30:00 PM"];
    let args = crate::args::Args::try_parse_from(ok).unwrap();
    assert_eq!(
        args.set_modified,
        Some(awara::TimestampSpec::Fixed("2024-01-15 14:30:00".into()))
    );
}

#[test]
fn test_parse_rejects_invalid_option_values() {
    for args in [
        vec!["awara", "--set-attributes", "bogus"],
        vec!["awara", "--numbering-type", "hexadecimal"],
        vec!["awara", "--numbering-case", "mixed"],
        vec!["awara", "--crop", "no-prefix"],
        vec!["awara", "--filter-attr", "bogus"],
    ] {
        assert!(
            crate::args::Args::try_parse_from(args.clone()).is_err(),
            "should reject {args:?}"
        );
    }

    // Valid values still parse, normalized to their canonical form.
    let ok = crate::args::Args::try_parse_from(vec![
        "awara",
        "--numbering-type",
        "HEX",
        "--numbering-case",
        "UPPER",
        "--set-attributes",
        "readonly,HIDDEN",
        "--crop",
        "before:x",
        "--filter-attr",
        "HIDDEN",
    ])
    .unwrap();
    assert_eq!(ok.numbering_type.as_deref(), Some("hex"));
    assert_eq!(ok.numbering_case.as_deref(), Some("upper"));
    assert_eq!(ok.set_attributes.as_deref(), Some("readonly,hidden"));
    assert_eq!(ok.filter_attr.as_deref(), Some("hidden"));
}
