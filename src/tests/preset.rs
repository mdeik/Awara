use super::*;

#[test]
fn test_numbering_bases_constant() {
    // NUMBERING_BASES is the SSoT for numbering type dropdowns
    assert!(!NUMBERING_BASES.is_empty(), "should have at least one base");
    // Should contain decimal and roman
    assert!(
        NUMBERING_BASES.iter().any(|(_, v)| *v == "10"),
        "should contain decimal (base 10)"
    );
    assert!(
        NUMBERING_BASES.iter().any(|(_, v)| *v == "roman"),
        "should contain roman"
    );
    assert!(
        NUMBERING_BASES.iter().any(|(_, v)| *v == "36"),
        "should contain A-Z"
    );
    // First entry should be binary (ordering invariant)
    assert_eq!(NUMBERING_BASES[0].1, "2", "first should be binary");
    // Verify label-value pairs are non-empty
    for (label, value) in NUMBERING_BASES {
        assert!(!label.is_empty(), "label should not be empty");
        assert!(!value.is_empty(), "value should not be empty");
    }
}

#[test]
fn test_nat_cmp_plain_text() {
    // Plain text: same as lexical
    assert_eq!(nat_cmp("apple", "banana"), std::cmp::Ordering::Less);
    assert_eq!(nat_cmp("banana", "apple"), std::cmp::Ordering::Greater);
    assert_eq!(nat_cmp("apple", "apple"), std::cmp::Ordering::Equal);
}

#[test]
fn test_nat_cmp_numeric() {
    // The core case: img2 < img10 < img100
    assert_eq!(nat_cmp("img2", "img10"), std::cmp::Ordering::Less);
    assert_eq!(nat_cmp("img10", "img100"), std::cmp::Ordering::Less);
    assert_eq!(nat_cmp("img100", "img2"), std::cmp::Ordering::Greater);
    assert_eq!(nat_cmp("img2", "img2"), std::cmp::Ordering::Equal);
}

#[test]
fn test_nat_cmp_leading_zeros() {
    // Leading zeros treated as same numeric value
    assert_eq!(nat_cmp("img01", "img1"), std::cmp::Ordering::Equal);
    assert_eq!(nat_cmp("img001", "img1"), std::cmp::Ordering::Equal);
    assert_eq!(nat_cmp("img0", "img00"), std::cmp::Ordering::Equal);
}

#[test]
fn test_nat_cmp_mixed_segments() {
    // Multiple numeric segments
    assert_eq!(nat_cmp("v2.0", "v10.0"), std::cmp::Ordering::Less);
    assert_eq!(nat_cmp("v2.10", "v2.2"), std::cmp::Ordering::Greater);
}

#[test]
fn test_nat_cmp_prefix_matters() {
    // Non-numeric prefix determines ordering when numbers equal
    assert_eq!(nat_cmp("a10", "b2"), std::cmp::Ordering::Less);
    assert_eq!(nat_cmp("b2", "a10"), std::cmp::Ordering::Greater);
}

#[test]
fn test_nat_cmp_empty_strings() {
    assert_eq!(nat_cmp("", ""), std::cmp::Ordering::Equal);
    assert_eq!(nat_cmp("", "a"), std::cmp::Ordering::Less);
    assert_eq!(nat_cmp("a", ""), std::cmp::Ordering::Greater);
}

#[test]
fn test_nat_cmp_numbers_only() {
    assert_eq!(nat_cmp("5", "10"), std::cmp::Ordering::Less);
    assert_eq!(nat_cmp("10", "5"), std::cmp::Ordering::Greater);
    assert_eq!(nat_cmp("42", "42"), std::cmp::Ordering::Equal);
}

/// Ordering ignores case, matching OS file managers (Explorer's
/// `StrCmpLogicalW` uses a case-insensitive char compare).
#[test]
fn test_nat_cmp_case_insensitive() {
    assert_eq!(nat_cmp("Zebra", "apple"), std::cmp::Ordering::Greater);
    assert_eq!(nat_cmp("apple", "Zebra"), std::cmp::Ordering::Less);
    assert_eq!(nat_cmp("apple", "Apple"), std::cmp::Ordering::Equal);
    assert_eq!(
        nat_cmp("SAMPLE.SHOW", "sample.show"),
        std::cmp::Ordering::Equal
    );
}

/// Non-alphanumeric characters sort before letters, like Explorer:
/// '[' (0x5B) and '(' (0x28) weight below 'b' (0x62) case-insensitively,
/// so bracketed/grouped fan-sub names come before plain letter names.
#[test]
fn test_nat_cmp_punctuation_before_letters() {
    assert_eq!(
        nat_cmp("[Alpha Group] Sample Release", "Example.Show"),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        nat_cmp("[Zeta Fansub]_Sample_Movie_DVDrip", "Example.Show.OVA"),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        nat_cmp("(ABC) Sample Show", "[Alpha Group] Sample Release"),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        nat_cmp(
            "[Yotta] Sample Generic Title",
            "[Zeta Fansub]_Sample_Movie_DVDrip"
        ),
        std::cmp::Ordering::Less
    );
}

/// Digits in names still compare numerically regardless of case folding
/// (the OS numeric-run rule is unchanged).
#[test]
fn test_nat_cmp_numeric_with_case_folding() {
    assert_eq!(
        nat_cmp(
            "[Judas] Kyou Kara Maou! (Season 3 + OVAs)",
            "[Judas] Kyou Kara Maou! (Seasons 1-2)"
        ),
        std::cmp::Ordering::Less,
        "space sorts before 's': Season 3 before Seasons 1-2, like Explorer"
    );
}

#[test]
fn test_sanitize_file_name_control_chars() {
    // Null byte replaced on all platforms
    assert_eq!(sanitize_file_name("bad\0file.txt"), "bad_file.txt");
    // Newline and tab replaced on all platforms
    assert_eq!(sanitize_file_name("bad\nfile.txt"), "bad_file.txt");
    assert_eq!(sanitize_file_name("bad\rfile.txt"), "bad_file.txt");
    assert_eq!(sanitize_file_name("bad\tfile.txt"), "bad_file.txt");
}

#[test]
fn test_sanitize_file_name_mixed_control_chars() {
    let result = sanitize_file_name("a\0b\nc.txt");
    assert_eq!(result, "a_b_c.txt");
}

#[test]
fn test_sanitize_file_name_trailing_spaces_dots() {
    // Trailing spaces/dots are preserved — Windows strips them
    // automatically when the file is created.
    assert_eq!(sanitize_file_name("file.txt "), "file.txt ");
    assert_eq!(sanitize_file_name("file.txt."), "file.txt.");
    assert_eq!(sanitize_file_name("file.  "), "file.  ");
}

#[test]
fn test_sanitize_file_name_empty_becomes_unnamed() {
    assert_eq!(sanitize_file_name(""), "_unnamed");
    assert_eq!(sanitize_file_name("."), "_unnamed");
    assert_eq!(sanitize_file_name("  "), "  ");
}

/// The one validity rule for a computed name: subfolders are allowed, path
/// navigation is not. (SSoT: `path_navigation_error`.)
#[test]
fn test_path_navigation_error_table() {
    for ok in ["foo.txt", "sub/foo.txt", "sub/deep/foo.txt"] {
        assert_eq!(path_navigation_error(ok), None, "{ok} should be allowed");
    }
    for (name, want) in [
        ("./foo.txt", PathNavigation::CurDir),
        (".//foo.txt", PathNavigation::CurDir),
        ("sub/../foo.txt", PathNavigation::ParentDir),
        ("../foo.txt", PathNavigation::ParentDir),
        ("/foo.txt", PathNavigation::Rooted),
        ("//foo.txt", PathNavigation::Rooted),
    ] {
        assert_eq!(path_navigation_error(name), Some(want), "{name}");
    }
}

// ── Preset serialization skip tests ──

#[test]
fn test_preset_skips_default_sections() {
    // A fully default config should serialize to minimal JSON
    let cfg = RenameConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();

    // Default sections should be absent (is_default skips them)
    assert!(
        !json.contains("\"add\""),
        "default add section should be skipped"
    );
    assert!(
        !json.contains("\"case\""),
        "default case section should be skipped"
    );
    assert!(
        !json.contains("\"remove\""),
        "default remove section should be skipped"
    );
    assert!(
        !json.contains("\"extension\""),
        "default extension section should be skipped"
    );
    assert!(
        !json.contains("\"name\""),
        "default name section should be skipped"
    );
    assert!(
        !json.contains("\"auto_date\""),
        "default auto_date section should be skipped"
    );
    assert!(
        !json.contains("\"append_folder\""),
        "default append_folder section should be skipped"
    );
    assert!(
        !json.contains("\"move_copy\""),
        "default move_copy section should be skipped"
    );
    assert!(
        !json.contains("\"name_segment\""),
        "default name_segment section should be skipped"
    );
    assert!(
        !json.contains("\"special\""),
        "default special section should be skipped"
    );
    assert!(
        !json.contains("\"stop_on_error\""),
        "default stop_on_error should be skipped"
    );
    assert!(
        !json.contains("\"preserve_timestamps\""),
        "default preserve_timestamps should be skipped"
    );
    assert!(
        !json.contains("\"section_order\""),
        "empty section_order should be skipped"
    );
    assert!(
        !json.contains("\"command_order\""),
        "empty command_order should be skipped"
    );

    // All sections are skipped when they match their Default impl, even those
    // with non-trivial defaults like filter_files: true, etc.
    assert!(
        !json.contains("\"regex\""),
        "all-default regex section should be skipped"
    );
    assert!(
        !json.contains("\"numbering\""),
        "all-default numbering section should be skipped"
    );
    assert!(
        !json.contains("\"filters\""),
        "all-default filters section should be skipped"
    );
    assert!(
        !json.contains("\"copy_to\""),
        "all-default copy_to section should be skipped"
    );
}

#[test]
fn test_preset_roundtrip_via_json_default() {
    // Verify that a default config round-trips through JSON faithfully
    let cfg = RenameConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let restored: RenameConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg.replace, restored.replace);
    assert_eq!(cfg.remove, restored.remove);
    assert_eq!(cfg.numbering, restored.numbering);
    assert_eq!(cfg.filters, restored.filters);
    assert_eq!(cfg.regex, restored.regex);
    assert_eq!(cfg.auto_date, restored.auto_date);
    assert_eq!(cfg.append_folder, restored.append_folder);
    assert_eq!(cfg.move_copy, restored.move_copy);
    assert_eq!(cfg.copy_to, restored.copy_to);
    assert_eq!(cfg.special, restored.special);
    assert_eq!(cfg.name_segment, restored.name_segment);
}

#[test]
fn test_preset_roundtrip_via_json_minimal() {
    // A config with just a few fields set should round-trip
    let cfg = RenameConfig {
        replace: ReplaceSection {
            replace: Some("old".to_string()),
            with: Some("new".to_string()),
            ..Default::default()
        },
        remove: RemoveSection {
            remove_digits: true,
            remove_symbols: true,
            ..Default::default()
        },
        add: AddSection {
            add_prefix: Some("pre_".to_string()),
            add_suffix: Some("_suf".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let json = serde_json::to_string_pretty(&cfg).unwrap();
    let restored: RenameConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg.replace.replace, restored.replace.replace);
    assert_eq!(cfg.replace.with, restored.replace.with);
    assert!(restored.remove.remove_digits);
    assert!(restored.remove.remove_symbols);
    assert_eq!(cfg.add.add_prefix, restored.add.add_prefix);
    assert_eq!(cfg.add.add_suffix, restored.add.add_suffix);

    // Unset fields should still have defaults
    assert_eq!(restored.remove.remove_first, None);
    assert_eq!(restored.remove.remove_last, None);
    assert_eq!(restored.case.case_name, None);
    assert_eq!(restored.numbering.numbering_mode, None);
}

#[test]
fn test_preset_json_only_contains_set_fields() {
    // Verify that only the fields we set appear in the JSON
    let cfg = RenameConfig {
        replace: ReplaceSection {
            replace: Some("old".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).unwrap();

    // replace section should mention "replace" field
    assert!(json.contains("\"replace\":\"old\""));
    // but NOT empty/default fields
    assert!(
        !json.contains("\"replace_first\""),
        "default bool fields should be skipped"
    );
    assert!(
        !json.contains("\"with\""),
        "None Option fields should be skipped"
    );
}

#[test]
fn test_preset_load_empty_object() {
    // Loading an empty JSON object should produce a valid default config
    let restored: RenameConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(restored.replace.replace, None);
    assert_eq!(restored.remove.remove_first, None);
    assert_eq!(restored.numbering.numbering_start, 1);
    assert_eq!(restored.numbering.numbering_increment, 1);
    assert!(restored.filters.filter_files);
    assert!(restored.filters.filter_folders);
    assert!(restored.copy_to.copy_mode);
    assert_eq!(restored.append_folder.dirname_level, 1);
}

#[test]
fn test_preset_load_partial_section() {
    // Loading a partial section (only some fields) should merge with defaults
    let json = r#"{"replace": {"replace": "foo"}}"#;
    let restored: RenameConfig = serde_json::from_str(json).unwrap();
    assert_eq!(restored.replace.replace.as_deref(), Some("foo"));
    // with should still be None (not set in JSON)
    assert_eq!(restored.replace.with, None);
    // replace_case_sensitive defaults to false
    assert!(!restored.replace.replace_case_sensitive);
}

#[test]
fn test_preset_skip_serializing_bool_false() {
    // bool fields that are false should be skipped in serialization
    let cfg = RenameConfig {
        replace: ReplaceSection {
            replace_case_sensitive: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(!json.contains("replace_case_sensitive"));
}

#[test]
fn test_preset_skip_serializing_option_none() {
    // Option fields that are None should be skipped
    let cfg = RenameConfig {
        add: AddSection {
            add_prefix: None,
            ..Default::default()
        },
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(!json.contains("add_prefix"));
}

#[test]
fn test_preset_skip_serializing_zero_defaults() {
    // Numeric fields dragged back to their default (0) serialize as if
    // unset — "0 = not configured" holds for serialization too.
    let cfg = RenameConfig {
        remove: RemoveSection {
            remove_first: Some(0),
            remove_to: Some(0),
            remove_from: Some(-3), // a real value must still serialize
            ..Default::default()
        },
        add: AddSection {
            add_at: Some(0),
            ..Default::default()
        },
        filters: FiltersSection {
            min_name_len: Some(0),
            ..Default::default()
        },
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(!json.contains("remove_first"));
    assert!(!json.contains("remove_to"));
    assert!(!json.contains("add_at"));
    assert!(!json.contains("min_name_len"));
    assert!(json.contains("\"remove_from\":-3"));
    // Loading the serialized form behaves identically (0 ≡ None).
    let restored: RenameConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.remove.remove_first, None);
    assert_eq!(restored.remove.remove_from, Some(-3));
    assert_eq!(restored.remove.remove_to, None);
}

#[test]
fn test_preset_skip_serializing_empty_strings() {
    // Empty text fields serialize as if unset — "" ≡ None for export.
    // (The section wrapper may appear as `{}`, which loads identically.)
    let cfg = RenameConfig {
        replace: ReplaceSection {
            replace: Some(String::new()),
            with: Some(String::new()),
            ..Default::default()
        },
        add: AddSection {
            add_prefix: Some(String::new()),
            ..Default::default()
        },
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(!json.contains("\"replace\":\"\""));
    assert!(!json.contains("\"add_prefix\":\"\""));
    // Loading the serialized form behaves identically ("" ≡ None).
    let restored: RenameConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.replace.replace, None);
    assert_eq!(restored.add.add_prefix, None);
}

#[test]
fn test_set_config_field_empty_normalizes_to_none() {
    // Clearing a text field in the TUI must behave like clearing it in
    // the GUI (empty → None), never leaving Some("") behind.
    let mut cfg = RenameConfig::default();
    set_config_field(&mut cfg, "Replace", "").unwrap();
    assert_eq!(cfg.replace.replace, None);
    set_config_field(&mut cfg, "Prefix", "").unwrap();
    assert_eq!(cfg.add.add_prefix, None);
    set_config_field(&mut cfg, "Output Dir", "").unwrap();
    assert_eq!(cfg.copy_to.output_dir, None);
    // Non-empty values are still stored.
    set_config_field(&mut cfg, "Replace", "abc").unwrap();
    assert_eq!(cfg.replace.replace.as_deref(), Some("abc"));
    // Numeric fields: empty clears to the default.
    set_config_field(&mut cfg, "Remove First", "").unwrap();
    assert!(opt_num_default(cfg.remove.remove_first));
    // Bad numeric input is an error, not a silent 0.
    assert!(set_config_field(&mut cfg, "Remove First", "abc").is_err());
}

/// An inline edit used to discard the shared validator's report, so a value the
/// validator reset or clamped looked exactly like a successful edit — the field
/// showed something other than what was typed, with nothing saying so.
#[test]
fn test_set_config_field_reports_values_the_validator_changed() {
    // Reset: the tag is not one the engine implements.
    let mut cfg = RenameConfig::default();
    let err = set_config_field(&mut cfg, "Insert Meta", "exif:DateTimeOriginal")
        .expect_err("an unimplemented tag must not look like a successful edit");
    assert!(err.contains("unknown tag"), "{err}");
    assert_eq!(cfg.auto_date.insert_meta, None);

    // Clamped: the field holds the limit, not the typed number.
    let mut cfg = RenameConfig::default();
    let err = set_config_field(&mut cfg, "Num Pad", "9999")
        .expect_err("an out-of-range value must be reported, not silently clamped");
    assert!(err.contains("clamped"), "{err}");
    assert_eq!(cfg.numbering.numbering_pad, NUMBERING_PAD.1);

    // A value that survives validation is still a clean success.
    let mut cfg = RenameConfig::default();
    assert!(set_config_field(&mut cfg, "Num Pad", "4").is_ok());
    assert_eq!(cfg.numbering.numbering_pad, 4);
}

#[test]
fn test_make_replace_item_skips_empty_match() {
    // Defense against old states containing Some(""): an empty replace
    // match must not produce a Replace item (str::replace with "" would
    // insert the replacement between every character).
    let mut cfg = RenameConfig::default();
    cfg.replace.replace = Some(String::new());
    assert!(make_replace_item(&cfg).is_none());
    cfg.replace.replace = Some("a".into());
    assert!(make_replace_item(&cfg).is_some());
}

#[test]
fn test_set_config_field_covers_active_field_labels() {
    // Every editable label emitted by config_active_fields must be
    // handled by set_config_field (no silent no-ops in the TUI).
    let mut cfg = RenameConfig::default();
    set_config_field(&mut cfg, "Num At", "3").unwrap();
    assert_eq!(cfg.numbering.numbering_at, Some(3));
    set_config_field(&mut cfg, "Num Break", "5").unwrap();
    assert_eq!(cfg.numbering.numbering_break, Some(5));
    set_config_field(&mut cfg, "Num Type", "hex").unwrap();
    assert_eq!(cfg.numbering.numbering_type.as_deref(), Some("hex"));
    set_config_field(&mut cfg, "Num Case", "upper").unwrap();
    assert_eq!(cfg.numbering.numbering_case.as_deref(), Some("upper"));
    set_config_field(&mut cfg, "Filter Level", "2").unwrap();
    assert_eq!(cfg.filters.filter_level, Some(2));
    set_config_field(&mut cfg, "Case Exception", "iOS").unwrap();
    assert_eq!(cfg.case.case_exception.as_deref(), Some("iOS"));
    set_config_field(&mut cfg, "Name Mode", "fixed").unwrap();
    assert_eq!(cfg.name.name_mode.as_deref(), Some("fixed"));
    set_config_field(&mut cfg, "Name Value", "report").unwrap();
    assert_eq!(cfg.name.name_value.as_deref(), Some("report"));
    // Empty values clear to None.
    set_config_field(&mut cfg, "Case Exception", "").unwrap();
    assert_eq!(cfg.case.case_exception, None);
    // Unknown enum labels are errors.
    assert!(set_config_field(&mut cfg, "Case Name", "bogus").is_err());
}

#[test]
fn test_set_config_field_enum_and_composed_labels() {
    let mut cfg = RenameConfig::default();
    // Enum labels accept the clap value names shown in the display.
    set_config_field(&mut cfg, "Num Source", "Date").unwrap();
    assert_eq!(cfg.numbering.numbering_source, Some(NumberingSource::Date));
    set_config_field(&mut cfg, "Case Name", "title-enhanced").unwrap();
    assert_eq!(cfg.case.case_name, Some(CaseMode::TitleEnhanced));
    set_config_field(&mut cfg, "Date Position", "Suffix").unwrap();
    assert_eq!(cfg.auto_date.date_position, DatePosition::Suffix);
    // Composed move/copy values use the shared start:len:dest[:sep] format.
    set_config_field(&mut cfg, "Move Part", "1:3:end:_").unwrap();
    let v = cfg.move_copy.move_part.as_ref().unwrap();
    assert_eq!((v.start, v.length, v.destination), (1, 3, -1));
    assert_eq!(v.separator.as_deref(), Some("_"));
    assert!(cfg.move_copy.copy_part.is_none(), "move clears copy");
    set_config_field(&mut cfg, "Copy Part", "-2:1:1").unwrap();
    let v = cfg.move_copy.copy_part.as_ref().unwrap();
    assert_eq!((v.start, v.length, v.destination), (-2, 1, 1));
    assert!(cfg.move_copy.move_part.is_none(), "copy clears move");
    // "Add Date (suffix)" labels carry the date position.
    set_config_field(&mut cfg, "Add Date (suffix)", "YYYY-MM-DD").unwrap();
    assert_eq!(cfg.auto_date.add_date.as_deref(), Some("YYYY-MM-DD"));
    assert_eq!(cfg.auto_date.date_position, DatePosition::Suffix);
    // Malformed move/copy values are errors.
    assert!(set_config_field(&mut cfg, "Move Part", "nope").is_err());
}

#[test]
fn test_parse_move_copy_value_shared() {
    assert!(parse_move_copy_value("1:3:end").is_some());
    assert!(parse_move_copy_value("1:3:7:_").is_some());
    assert!(parse_move_copy_value("abc").is_none());
    assert!(parse_move_copy_value("1:3").is_none());
    let v = parse_move_copy_value("-2:1:1").unwrap();
    assert_eq!((v.start, v.length, v.destination), (-2, 1, 1));
    assert_eq!(v.separator, None);
    let v = parse_move_copy_value("1:3:end:_").unwrap();
    assert_eq!((v.start, v.length, v.destination), (1, 3, -1));
    assert_eq!(v.separator.as_deref(), Some("_"));
}

#[test]
fn test_preset_preserves_bool_true() {
    // bool fields that are true should be serialized
    let cfg = RenameConfig {
        remove: RemoveSection {
            remove_digits: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(json.contains("\"remove_digits\":true"));
}

#[test]
fn test_preset_preserves_option_some() {
    // Option fields that are Some should be serialized
    let cfg = RenameConfig {
        add: AddSection {
            add_prefix: Some("test_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(json.contains("\"add_prefix\":\"test_\""));
}
