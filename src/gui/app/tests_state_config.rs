use super::*;
use awara::{CaseMode, ExtensionMode, NumberingMode};

#[test]
fn test_remove_section_zero_values_not_modified() {
    let mut cfg = RenameConfig::default();
    cfg.remove.remove_to = Some(0);
    cfg.remove.remove_first = Some(0);
    cfg.remove.remove_last = Some(0);
    cfg.remove.remove_from = Some(0);
    assert!(!section_is_modified(&cfg, SectionId::Remove));
    // From set (with To still at 0) is a real removal → modified.
    cfg.remove.remove_from = Some(-3);
    assert!(section_is_modified(&cfg, SectionId::Remove));
    // Any non-zero To is a real value → modified.
    let mut cfg = RenameConfig::default();
    cfg.remove.remove_to = Some(2);
    assert!(section_is_modified(&cfg, SectionId::Remove));
}

/// Filter length/level fields at 0 are the default → not modified.
#[test]
fn test_filters_section_zero_lengths_not_modified() {
    let mut cfg = RenameConfig::default();
    cfg.filters.min_name_len = Some(0);
    cfg.filters.max_name_len = Some(0);
    cfg.filters.filter_level = Some(0);
    cfg.filters.min_path_len = Some(0);
    cfg.filters.max_path_len = Some(0);
    assert!(!section_is_modified(&cfg, SectionId::Filters));
    cfg.filters.min_name_len = Some(3);
    assert!(section_is_modified(&cfg, SectionId::Filters));
}

/// Empty text fields count as unset, so a cleared text box never lights
/// the modified border (mirrors the numeric 0 handling).
#[test]
fn test_sections_empty_text_not_modified() {
    let mut cfg = RenameConfig::default();
    cfg.replace.replace = Some(String::new());
    cfg.regex.regex_match = Some(String::new());
    cfg.add.add_prefix = Some(String::new());
    cfg.remove.remove_chars = Some(String::new());
    cfg.case.case_exception = Some(String::new());
    cfg.filters.exclude_regex = Some(String::new());
    cfg.copy_to.output_dir = Some(String::new());
    assert!(!section_is_modified(&cfg, SectionId::Replace));
    assert!(!section_is_modified(&cfg, SectionId::Regex));
    assert!(!section_is_modified(&cfg, SectionId::Add));
    assert!(!section_is_modified(&cfg, SectionId::Remove));
    assert!(!section_is_modified(&cfg, SectionId::Case));
    assert!(!section_is_modified(&cfg, SectionId::Filters));
    assert!(!section_is_modified(&cfg, SectionId::CopyTo));
    // A non-empty value marks the section modified again.
    cfg.replace.replace = Some("abc".into());
    assert!(section_is_modified(&cfg, SectionId::Replace));
}
#[test]
fn test_gui_new_defaults() {
    let gs = GuiApp::new_with_config_for_test();
    assert_eq!(gs.config.filters.filter_pattern, Some("*".to_string()));
    assert!(gs.undo_file.is_empty());
}

#[test]
fn test_build_command_order_disabled_sections() {
    let mut app = GuiApp::new_with_config_for_test();
    // Set some fields on the replace section
    app.config.replace.replace = Some("foo".into());
    app.config.replace.with = Some("bar".into());
    app.config.replace.replace_case_sensitive = true;
    app.config.replace.replace_first = true;

    // Disable the Replace section — build_command_order preserves values
    app.section_enabled[SectionId::Replace as usize] = false;
    app.build_command_order();

    // Values are preserved (clearing happens on a clone elsewhere)
    assert_eq!(app.config.replace.replace, Some("foo".into()));
    assert_eq!(app.config.replace.with, Some("bar".into()));
    assert!(app.config.replace.replace_case_sensitive);
    assert!(app.config.replace.replace_first);
}

#[test]
fn test_build_command_order_disabled_sections_all() {
    let mut app = GuiApp::new_with_config_for_test();
    // Set a value on every config field, then disable all sections
    app.config.regex.regex_match = Some("test".into());
    app.config.regex.regex_replace = Some("test".into());
    app.config.replace.replace = Some("foo".into());
    app.config.replace.with = Some("bar".into());
    app.config.remove.remove_first = Some(3);
    app.config.remove.remove_last = Some(2);
    app.config.add.add_prefix = Some("pre".into());
    app.config.numbering.numbering_mode = Some(NumberingMode::Prefix);
    app.config.numbering.numbering_start = 5;
    app.config.case.case_name = Some(CaseMode::Lower);
    app.config.case.case_exception = Some("ex".into());
    app.config.extension.extension_mode = Some(ExtensionMode::Upper);
    app.config.extension.extension_replace = Some("ext".into());
    app.config.name.name_value = Some("name".into());
    app.config.name.name_mode = Some("fixed".into());
    app.config.auto_date.add_date = Some("YYYY".into());
    app.config.append_folder.add_dirname = true;
    app.config.append_folder.add_dirname_sep = Some("_".into());
    app.config.filters.exclude_regex = Some(".*".into());
    app.config.special.set_attributes = Some("readonly".into());
    app.config.stop_on_error = true;

    // Disable all sections
    for en in app.section_enabled.iter_mut() {
        *en = false;
    }
    app.build_command_order();

    // All values preserved (clearing happens on a clone elsewhere)
    assert_eq!(app.config.replace.replace, Some("foo".into()));
    assert_eq!(app.config.remove.remove_first, Some(3));
    assert_eq!(
        app.config.numbering.numbering_mode,
        Some(NumberingMode::Prefix)
    );
    assert_eq!(app.config.numbering.numbering_start, 5);
    assert_eq!(app.config.special.set_attributes, Some("readonly".into()));
    assert!(app.config.stop_on_error);
}

#[test]
fn test_build_command_order_filter_fields_preserved_when_disabled() {
    let mut app = GuiApp::new_with_config_for_test();
    app.section_enabled[SectionId::Filters as usize] = true;

    // Set filter fields
    app.config.filters.filter_match_case = true;
    app.config.filters.filter_use_regex = true;
    app.config.filters.filter_files = false;
    app.config.filters.filter_folders = false;
    app.config.filters.filter_hidden = true;
    app.config.filters.filter_subfolders = true;
    app.config.filters.filter_level = Some(5);
    app.config.filters.min_name_len = Some(3);
    app.config.filters.max_name_len = Some(10);
    app.config.filters.min_path_len = Some(5);
    app.config.filters.max_path_len = Some(50);
    app.config.filters.filter_attr = Some("hidden".into());

    // Disable Filters section
    app.section_enabled[SectionId::Filters as usize] = false;
    app.build_command_order();

    // Values should be preserved (clearing happens on a clone)
    assert!(app.config.filters.filter_match_case);
    assert!(app.config.filters.filter_use_regex);
    assert!(!app.config.filters.filter_files);
    assert!(!app.config.filters.filter_folders);
    assert!(app.config.filters.filter_hidden);
    assert!(app.config.filters.filter_subfolders);
    assert_eq!(app.config.filters.filter_level, Some(5));
    assert_eq!(app.config.filters.min_name_len, Some(3));
    assert_eq!(app.config.filters.max_name_len, Some(10));
    assert_eq!(app.config.filters.min_path_len, Some(5));
    assert_eq!(app.config.filters.max_path_len, Some(50));
    assert_eq!(app.config.filters.filter_attr, Some("hidden".into()));
}
#[test]
fn test_apply_timestamps_ui_no_change() {
    let mut app = GuiApp::new_with_config_for_test();
    // All modes start as "no_change" → all config fields remain None
    app.apply_timestamps_ui();
    assert_eq!(app.config.special.set_created, None);
    assert_eq!(app.config.special.set_modified, None);
    assert_eq!(app.config.special.set_accessed, None);
}

#[test]
fn test_apply_timestamps_ui_created_current() {
    let mut app = GuiApp::new_with_config_for_test();
    app.timestamps.created.mode = "current".into();
    app.apply_timestamps_ui();
    assert_eq!(app.config.special.set_created, Some(TimestampSpec::Current));
}

#[test]
fn test_apply_timestamps_ui_created_fixed() {
    let mut app = GuiApp::new_with_config_for_test();
    app.timestamps.created.mode = "fixed".into();
    app.timestamps.created.fixed_date = "2025-06-15".into();
    // The editor is a 12-hour clock: 2:30 PM = 14:30.
    app.timestamps.created.fixed_time = "02:30:00".into();
    app.timestamps.created.fixed_pm = true;
    app.apply_timestamps_ui();
    assert_eq!(
        app.config.special.set_created,
        Some(TimestampSpec::Fixed("2025-06-15 14:30:00".into()))
    );
}

#[test]
fn test_apply_timestamps_ui_created_copy_from_modified() {
    let mut app = GuiApp::new_with_config_for_test();
    app.timestamps.created.mode = "modified".into();
    app.apply_timestamps_ui();
    assert_eq!(
        app.config.special.set_created,
        Some(TimestampSpec::CopyFrom("modified".into()))
    );
}

#[test]
fn test_apply_timestamps_ui_created_taken() {
    let mut app = GuiApp::new_with_config_for_test();
    app.timestamps.created.mode = "taken".into();
    app.apply_timestamps_ui();
    assert_eq!(app.config.special.set_created, Some(TimestampSpec::Taken));
    // "taken" mode no longer writes to insert_meta — that's an AutoDate concern
    assert_eq!(app.config.auto_date.insert_meta, None);
}

#[test]
fn test_apply_timestamps_ui_modified_copy_from_created() {
    let mut app = GuiApp::new_with_config_for_test();
    app.timestamps.modified.mode = "created".into();
    app.apply_timestamps_ui();
    assert_eq!(
        app.config.special.set_modified,
        Some(TimestampSpec::CopyFrom("created".into()))
    );
}

#[test]
fn test_apply_timestamps_ui_accessed_copy_from_created() {
    let mut app = GuiApp::new_with_config_for_test();
    app.timestamps.accessed.mode = "created".into();
    app.apply_timestamps_ui();
    assert_eq!(
        app.config.special.set_accessed,
        Some(TimestampSpec::CopyFrom("created".into()))
    );
}

#[test]
fn test_apply_timestamps_ui_modified_current() {
    let mut app = GuiApp::new_with_config_for_test();
    app.timestamps.modified.mode = "current".into();
    app.apply_timestamps_ui();
    assert_eq!(
        app.config.special.set_modified,
        Some(TimestampSpec::Current)
    );
}

#[test]
fn test_apply_timestamps_ui_modified_fixed_date_only() {
    let mut app = GuiApp::new_with_config_for_test();
    app.timestamps.modified.mode = "fixed".into();
    app.timestamps.modified.fixed_date = "2025-01-01".into();
    app.timestamps.modified.fixed_time = "".into();
    app.apply_timestamps_ui();
    assert_eq!(
        app.config.special.set_modified,
        Some(TimestampSpec::Fixed("2025-01-01".into()))
    );
}

#[test]
fn test_apply_timestamps_ui_fixed_invalid_date_is_not_persisted() {
    let mut app = GuiApp::new_with_config_for_test();
    app.timestamps.modified.mode = "fixed".into();
    app.timestamps.modified.fixed_date = "not-a-date".into();
    app.timestamps.modified.fixed_time = "".into();
    // Invalid input leaves the existing value untouched (the editor also shows
    // an inline error and disables Confirm, so it can't be applied normally).
    app.config.special.set_modified = Some(TimestampSpec::Fixed("2025-01-01".into()));
    app.apply_timestamps_ui();
    assert_eq!(
        app.config.special.set_modified,
        Some(TimestampSpec::Fixed("2025-01-01".into()))
    );
    assert!(!app.timestamps_valid());
}

#[test]
fn test_timestamps_valid_detects_bad_fixed_values() {
    let mut app = GuiApp::new_with_config_for_test();
    assert!(app.timestamps_valid(), "default state should be valid");

    app.timestamps.created.mode = "fixed".into();
    app.timestamps.created.fixed_date = "2025-06-15".into();
    app.timestamps.created.fixed_time = "02:30:00".into();
    app.timestamps.created.fixed_pm = true;
    assert!(app.timestamps_valid());

    // Out of 12-hour range in the editor field.
    app.timestamps.created.fixed_time = "15:30:00".into();
    assert!(!app.timestamps_valid());
    app.timestamps.created.fixed_time = "02:30:00".into();

    app.timestamps.created.fixed_date = "2025-13-40".into();
    assert!(!app.timestamps_valid());

    app.timestamps.created.mode = "current".into();
    assert!(app.timestamps_valid(), "non-fixed modes ignore the fields");
}

#[test]
fn test_apply_timestamps_ui_accessed_current() {
    let mut app = GuiApp::new_with_config_for_test();
    app.timestamps.accessed.mode = "current".into();
    app.apply_timestamps_ui();
    assert_eq!(
        app.config.special.set_accessed,
        Some(TimestampSpec::Current)
    );
}
#[test]
fn test_build_command_order_name_section_preserves_fields() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.name.name_value = Some("custom_name".into());
    app.config.name.name_mode = Some("fixed".into());

    app.section_enabled[SectionId::Name as usize] = false;
    app.build_command_order();
    // Values preserved — clearing happens on a clone elsewhere
    assert_eq!(app.config.name.name_value, Some("custom_name".into()));
    assert_eq!(app.config.name.name_mode, Some("fixed".into()));
}

// ── Numbering flush_strings tests ──

#[test]
fn test_build_command_order_numbering_produces_item() {
    let mut app = GuiApp::new_with_config_for_test();
    app.section_enabled[SectionId::Numbering as usize] = true;
    app.config.numbering.numbering_mode = Some(NumberingMode::Prefix);
    app.config.numbering.numbering_start = 10;
    app.config.numbering.numbering_increment = 2;
    app.config.numbering.numbering_pad = 4;
    app.build_command_order();
    let has_numbering = app
        .config
        .command_order
        .iter()
        .any(|item| matches!(item, RenameItem::Numbering(..)));
    assert!(has_numbering);
}

/// `build_command_order` emits items in the configured section order for
/// arbitrary operations, not just numbering.
#[test]
fn test_build_command_order_respects_section_order() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.add.add_prefix = Some("A".into());
    app.config.remove.remove_first = Some(2);
    app.config.numbering.numbering_mode = Some(NumberingMode::Prefix);

    app.config.section_order = vec!["Numbering".into(), "Remove".into(), "Add".into()];
    app.build_command_order();

    let kinds: Vec<&str> = app
        .config
        .command_order
        .iter()
        .map(|item| match item {
            RenameItem::Numbering(..) => "numbering",
            RenameItem::RemoveFirst(_) => "remove",
            RenameItem::Prefix(_) => "add",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["numbering", "remove", "add"]);
}

/// Moving a section that is *modified* but contributes no pipeline items (here a
/// Case section with only an exception set — no case mode) must not recompute.
#[test]
fn test_reorder_modified_but_itemless_section_does_not_dirty_preview() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.add.add_prefix = Some("A".into());
    // Case is "modified" but emits no item without a case mode.
    app.config.case.case_exception = Some("ex".into());
    assert!(section_is_modified(&app.config, SectionId::Case));

    app.config.section_order = vec!["Case".into(), "Add".into()];
    app.build_command_order();
    let baseline = app.config.command_order.clone();

    app.preview_dirty = false;
    app.config.section_order = vec!["Add".into(), "Case".into()];
    app.build_command_order();

    assert_eq!(app.config.command_order, baseline);
    assert!(
        !app.preview_dirty,
        "a modified but itemless section's position must not force a recompute"
    );
}

/// Moving an active section above/below a *disabled* section leaves the
/// effective pipeline unchanged (disabled sections emit no items), so the
/// preview must not be recomputed.
#[test]
fn test_reorder_active_past_disabled_does_not_dirty_preview() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.add.add_prefix = Some("A".into());
    app.config.replace.replace = Some("x".into());
    app.section_enabled[SectionId::Replace as usize] = false;

    app.config.section_order = vec!["Replace".into(), "Add".into()];
    app.build_command_order();
    let baseline = app.config.command_order.clone();

    app.preview_dirty = false;
    app.config.section_order = vec!["Add".into(), "Replace".into()];
    app.build_command_order();

    assert_eq!(app.config.command_order, baseline);
    assert!(
        !app.preview_dirty,
        "moving an active section past a disabled one must not recompute"
    );
}

/// Moving an active section above/below an *empty* section leaves the effective
/// pipeline unchanged (empty sections emit no items), so the preview must not
/// be recomputed.
#[test]
fn test_reorder_active_past_empty_does_not_dirty_preview() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.add.add_prefix = Some("A".into());
    // Replace has no values set, so it emits nothing.

    app.config.section_order = vec!["Replace".into(), "Add".into()];
    app.build_command_order();
    let baseline = app.config.command_order.clone();

    app.preview_dirty = false;
    app.config.section_order = vec!["Add".into(), "Replace".into()];
    app.build_command_order();

    assert_eq!(app.config.command_order, baseline);
    assert!(
        !app.preview_dirty,
        "moving an active section past an empty one must not recompute"
    );
}

/// A partial stored `section_order` is completed with every default section at
/// its default position.
#[test]
fn test_partial_section_order_is_completed_with_defaults() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.add.add_prefix = Some("A".into());
    app.config.section_order = vec!["Add".into()];
    app.build_command_order();

    let labels: Vec<&str> = SectionId::default_order()
        .iter()
        .map(|s| s.label())
        .collect();
    for label in &labels {
        assert!(
            app.config.section_order.iter().any(|s| s == label),
            "missing {label} after normalization"
        );
    }
    assert_eq!(app.config.section_order.len(), labels.len());
    assert!(matches!(
        app.config.command_order.first(),
        Some(RenameItem::Prefix(_))
    ));
}

/// A configured section that is missing from a partial order is slotted back in
/// at its default position, so it still runs.
#[test]
fn test_partial_section_order_reenables_missing_sections() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.add.add_prefix = Some("A".into());
    app.config.numbering.numbering_mode = Some(NumberingMode::Prefix);
    app.config.section_order = vec!["Add".into()];
    app.build_command_order();

    let kinds: Vec<&str> = app
        .config
        .command_order
        .iter()
        .map(|item| match item {
            RenameItem::Prefix(_) => "add",
            RenameItem::Numbering(..) => "numbering",
            _ => "other",
        })
        .collect();
    // Numbering defaults after Add and must not be silently dropped.
    assert_eq!(kinds, vec!["add", "numbering"]);
}

/// `numbering_active` requires both a configured numbering mode and an enabled
/// Numbering section.
#[test]
fn test_numbering_active_respects_section_enabled() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.numbering.numbering_mode = Some(NumberingMode::Prefix);
    assert!(app.numbering_active());

    app.section_enabled[SectionId::Numbering as usize] = false;
    assert!(!app.numbering_active());
}

/// Regression: the Order Editor writes `SectionId::label()` values
/// ("Append Folder Name"), which must be recognized by `emit_section_items`.
/// Previously the emitted label was "Append Folder", so reordering silently
/// dropped the Append Folder operation.
#[test]
fn test_build_command_order_append_folder_after_reorder() {
    let mut app = GuiApp::new_with_config_for_test();
    app.section_enabled[SectionId::AppendFolder as usize] = true;
    app.config.append_folder.add_dirname = true;
    app.config.section_order = vec!["Append Folder Name".into(), "Add".into()];
    app.build_command_order();
    assert!(
        app.config
            .command_order
            .iter()
            .any(|item| matches!(item, RenameItem::Dirname(..))),
        "Append Folder must still emit an item after a reorder"
    );
}

/// A reorder that leaves the compiled pipeline unchanged (here: all sections
/// are empty) must not mark the preview dirty.
#[test]
fn test_reorder_with_unchanged_pipeline_does_not_dirty_preview() {
    let mut app = GuiApp::new_with_config_for_test();
    app.build_command_order();
    assert!(app.config.command_order.is_empty());

    app.preview_dirty = false;
    app.config.section_order = vec!["Numbering".into(), "Add".into()];
    app.build_command_order();
    assert!(
        !app.preview_dirty,
        "an order change with an identical command_order must not recompute the preview"
    );
}

/// A reorder that changes the compiled pipeline must mark the preview dirty.
#[test]
fn test_reorder_changing_pipeline_dirties_preview() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.add.add_prefix = Some("A".into());
    app.config.numbering.numbering_mode = Some(NumberingMode::Prefix);

    app.config.section_order = vec!["Add".into(), "Numbering".into()];
    app.build_command_order();

    app.preview_dirty = false;
    app.config.section_order = vec!["Numbering".into(), "Add".into()];
    app.build_command_order();
    assert!(
        app.preview_dirty,
        "a pipeline-changing reorder must recompute"
    );
    assert!(matches!(
        app.config.command_order.first(),
        Some(RenameItem::Numbering(..))
    ));
}
#[test]
fn test_init_timestamps_ui_from_created_current() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.special.set_created = Some(TimestampSpec::Current);
    app.init_timestamps_ui();
    assert_eq!(app.timestamps.created.mode, "current");
}

#[test]
fn test_init_timestamps_ui_from_created_fixed() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.special.set_created = Some(TimestampSpec::Fixed("2025-06-15 14:30:00".into()));
    app.init_timestamps_ui();
    assert_eq!(app.timestamps.created.mode, "fixed");
    assert_eq!(app.timestamps.created.fixed_date, "2025-06-15");
    // 24-hour "14:30:00" is shown as 12-hour "02:30:00 PM".
    assert_eq!(app.timestamps.created.fixed_time, "02:30:00");
    assert!(app.timestamps.created.fixed_pm);
}

/// `fixed_pm` follows the stored time-of-day (AM/midnight/noon), so the editor's
/// AM/PM buttons reflect the actual time rather than always showing PM.
#[test]
fn test_init_timestamps_ui_fixed_am_pm_mapping() {
    for (stored, time12, pm) in [
        ("2025-06-15 09:30:00", "09:30:00", false), // AM
        ("2025-06-15 00:30:00", "12:30:00", false), // midnight
        ("2025-06-15 12:30:00", "12:30:00", true),  // noon
        ("2025-06-15 14:30:00", "02:30:00", true),
        ("2025-06-15 23:05:00", "11:05:00", true),
    ] {
        let mut app = GuiApp::new_with_config_for_test();
        app.config.special.set_created = Some(TimestampSpec::Fixed(stored.into()));
        app.init_timestamps_ui();
        assert_eq!(
            app.timestamps.created.fixed_time, time12,
            "12-hour time for {stored}"
        );
        assert_eq!(
            app.timestamps.created.fixed_pm, pm,
            "AM/PM flag for {stored}"
        );
    }
}

/// 12-hour editor time + AM/PM converts to a 24-hour `Fixed` string.
#[test]
fn test_apply_timestamps_ui_fixed_12h_am_pm() {
    let mut app = GuiApp::new_with_config_for_test();
    app.timestamps.created.mode = "fixed".into();
    app.timestamps.created.fixed_date = "2025-06-15".into();
    app.timestamps.created.fixed_time = "02:30:00".into();
    app.timestamps.created.fixed_pm = true;
    app.apply_timestamps_ui();
    assert_eq!(
        app.config.special.set_created,
        Some(TimestampSpec::Fixed("2025-06-15 14:30:00".into()))
    );

    app.timestamps.created.fixed_pm = false;
    app.apply_timestamps_ui();
    assert_eq!(
        app.config.special.set_created,
        Some(TimestampSpec::Fixed("2025-06-15 02:30:00".into()))
    );

    // Midnight and noon are the tricky 12-hour cases.
    app.timestamps.created.fixed_time = "12:00:00".into();
    app.timestamps.created.fixed_pm = false;
    app.apply_timestamps_ui();
    assert_eq!(
        app.config.special.set_created,
        Some(TimestampSpec::Fixed("2025-06-15 00:00:00".into()))
    );
    app.timestamps.created.fixed_pm = true;
    app.apply_timestamps_ui();
    assert_eq!(
        app.config.special.set_created,
        Some(TimestampSpec::Fixed("2025-06-15 12:00:00".into()))
    );
}

#[test]
fn test_init_timestamps_ui_from_created_copy() {
    let mut app = GuiApp::new_with_config_for_test();
    app.config.special.set_created = Some(TimestampSpec::CopyFrom("modified".into()));
    app.init_timestamps_ui();
    assert_eq!(app.timestamps.created.mode, "modified");
}

/// The timestamp editor's "now" prefill must be local wall-clock time, not UTC:
/// parsing it back as local yields (approximately) the current instant.
#[test]
fn test_now_datetime_string_is_local() {
    let s = GuiApp::now_datetime_string();
    let parsed = awara::parse_local_datetime(&s)
        .unwrap_or_else(|| panic!("could not parse prefilled time {s:?}"));
    let now = std::time::SystemTime::now();
    let diff = parsed
        .duration_since(now)
        .or_else(|_| now.duration_since(parsed))
        .unwrap();
    assert!(
        diff.as_secs() < 2,
        "prefill should be local now (diff {diff:?})"
    );
}
