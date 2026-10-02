use super::*;

#[test]
fn test_no_trailing_dot_or_space() {
    let config = RenameConfig {
        remove: RemoveSection {
            remove_symbols: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "file @.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "file.txt"
    );

    assert_eq!(
        process_filename(
            "trailing  ",
            &compiled,
            None,
            None,
            true,
            &FileMeta::default()
        ),
        "trailing"
    );
}

// ============ New Feature Tests ============

#[test]
fn test_format_date_tokens() {
    use std::time::UNIX_EPOCH;
    let time = UNIX_EPOCH + std::time::Duration::from_secs(1673793045);
    // Use format_date_offset with 0 (UTC) to test raw token replacement
    assert_eq!(format_date_offset("YYYY", time, true, 0), "2023");
    assert_eq!(format_date_offset("YY", time, true, 0), "23");
    assert_eq!(format_date_offset("MM", time, true, 0), "01");
    assert_eq!(format_date_offset("DD", time, true, 0), "15");
    assert_eq!(format_date_offset("hh", time, true, 0), "14");
    assert_eq!(format_date_offset("mm", time, true, 0), "30");
    assert_eq!(format_date_offset("ss", time, true, 0), "45");
    assert_eq!(
        format_date_offset("YYYY-MM-DD", time, true, 0),
        "2023-01-15"
    );
    assert_eq!(format_date_offset("YYMMDD", time, true, 0), "230115");
}

#[test]
fn test_add_date_prefix() {
    let config = RenameConfig::default();
    let result = apply_add_date("file.txt", "YYYY-MM-DD_", None, false, &config);
    assert!(result.len() > "file.txt".len());
    assert!(result.chars().next().unwrap().is_numeric());
    assert!(result.ends_with("file.txt"));
}

#[test]
fn test_add_date_via_config() {
    let config = RenameConfig {
        auto_date: AutoDateSection {
            add_date: Some("YYYY-MM-DD_".to_string()),
            auto_date_century: true,
            date_position: DatePosition::Prefix,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename(
        "photo.jpg",
        &compiled,
        None,
        None,
        false,
        &FileMeta::default(),
    );
    assert!(result.len() > "photo.jpg".len());
    assert!(result[..4].chars().all(|c| c.is_numeric()));
    assert!(result.contains("photo"));
}

#[test]
fn test_add_file_date_with_modified_time() {
    use std::time::UNIX_EPOCH;
    let file_time = UNIX_EPOCH + std::time::Duration::from_secs(1673793045);
    let meta = FileMeta {
        modified: Some(file_time),
        ..Default::default()
    };
    let config = RenameConfig {
        auto_date: AutoDateSection {
            add_file_date: Some("YYYY".to_string()),
            auto_date_century: true,
            date_position: DatePosition::Prefix,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename("report.pdf", &compiled, None, None, false, &meta);
    assert_eq!(result, "2023report.pdf");
}

#[test]
fn test_add_file_date_via_config_no_time() {
    let config = RenameConfig {
        auto_date: AutoDateSection {
            add_file_date: Some("YYYY.MM.DD_".to_string()),
            date_position: DatePosition::Prefix,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename(
        "data.csv",
        &compiled,
        None,
        None,
        false,
        &FileMeta::default(),
    );
    assert!(result.len() > "data.csv".len());
    assert!(result.chars().next().unwrap().is_numeric());
}

#[test]
fn test_insert_meta_exif_date() {
    use std::time::UNIX_EPOCH;
    let file_time = UNIX_EPOCH + std::time::Duration::from_secs(1704067200);
    let meta = FileMeta {
        modified: Some(file_time),
        ..Default::default()
    };
    // auto_date_offset: Some(0) means local+0 = local.
    // Compute expected date string dynamically from the local time of the epoch.
    let local_secs = crate::core::rename::local_utc_offset_secs_at(file_time);
    let expected = format_date_offset("YYYY-MM-DD", file_time, true, local_secs);
    let config = RenameConfig {
        auto_date: AutoDateSection {
            auto_date_century: true,
            auto_date_offset: Some(0),
            ..Default::default()
        },
        ..Default::default()
    };
    let result = apply_insert_meta("IMG_0001.jpg", "exif-date", &meta, false, &config);
    assert_eq!(result, format!("{}IMG_0001.jpg", expected));
}

#[test]
fn test_insert_meta_no_time() {
    let config = RenameConfig::default();
    let result = apply_insert_meta(
        "file.txt",
        "exif-date",
        &FileMeta::default(),
        false,
        &config,
    );
    assert_eq!(result, "file.txt");
}

/// The EXIF-backed tags are handled by one shared branch, so they must all
/// behave the same: prefer the EXIF date, fall back to mtime, else leave the
/// name alone. Previously each was its own match arm with the same body.
#[test]
fn test_exif_backed_insert_meta_tags_agree() {
    use std::time::UNIX_EPOCH;
    let mtime = UNIX_EPOCH + std::time::Duration::from_secs(1704067200);
    let exif_epoch = 1_600_000_000i64;
    let config = RenameConfig {
        auto_date: AutoDateSection {
            auto_date_century: true,
            auto_date_offset: Some(0),
            ..Default::default()
        },
        ..Default::default()
    };

    let with_exif = FileMeta {
        modified: Some(mtime),
        exif_date: Some(exif_epoch),
        ..Default::default()
    };
    let without_exif = FileMeta {
        modified: Some(mtime),
        exif_date: None,
        ..Default::default()
    };

    let exif_expected = format_date_offset(
        "YYYY-MM-DD",
        UNIX_EPOCH + std::time::Duration::from_secs(exif_epoch as u64),
        true,
        crate::core::rename::local_utc_offset_secs_at(
            UNIX_EPOCH + std::time::Duration::from_secs(exif_epoch as u64),
        ),
    );
    let mtime_expected = format_date_offset(
        "YYYY-MM-DD",
        mtime,
        true,
        crate::core::rename::local_utc_offset_secs_at(mtime),
    );

    for tag in [
        "exif-date",
        "taken_original",
        "taken_digitized",
        "taken_recent",
    ] {
        assert!(
            insert_meta_tag_uses_exif(tag),
            "{tag} must be flagged as exif"
        );
        // Prefers the EXIF date...
        assert_eq!(
            apply_insert_meta("x.jpg", tag, &with_exif, false, &config),
            format!("{exif_expected}x.jpg"),
            "{tag} should use the EXIF date"
        );
        // ...falls back to mtime when the image has none...
        assert_eq!(
            apply_insert_meta("x.jpg", tag, &without_exif, false, &config),
            format!("{mtime_expected}x.jpg"),
            "{tag} should fall back to mtime"
        );
        // ...and leaves the name alone with no time at all.
        assert_eq!(
            apply_insert_meta("x.jpg", tag, &FileMeta::default(), false, &config),
            "x.jpg"
        );
    }

    // The mtime-only tags must not be flagged as needing EXIF.
    for tag in ["modified", "taken_modified"] {
        assert!(
            !insert_meta_tag_uses_exif(tag),
            "{tag} must not be flagged as exif"
        );
        assert_eq!(
            apply_insert_meta("x.jpg", tag, &with_exif, false, &config),
            format!("{mtime_expected}x.jpg"),
            "{tag} should use mtime, never the EXIF date"
        );
    }

    // Unknown tags are no-ops, and are not flagged as needing EXIF either.
    assert!(!insert_meta_tag_uses_exif("nonsense"));
    assert_eq!(
        apply_insert_meta("x.jpg", "nonsense", &with_exif, false, &config),
        "x.jpg"
    );
}

/// `config_needs_exif_date` is what decides whether EXIF is read at all, so it
/// must be true exactly when a tag that consumes EXIF will run.
#[test]
fn test_config_needs_exif_date_predicate() {
    // Nothing configured: nothing reads EXIF.
    assert!(!config_needs_exif_date(&RenameConfig::default()));

    // Section field set.
    let mut cfg = RenameConfig::default();
    cfg.auto_date.insert_meta = Some("exif-date".into());
    assert!(config_needs_exif_date(&cfg));
    cfg.auto_date.insert_meta = Some("modified".into());
    assert!(
        !config_needs_exif_date(&cfg),
        "mtime-only tag needs no EXIF"
    );

    // Item in command_order (what actually runs).
    let cfg = RenameConfig {
        command_order: vec![RenameItem::InsertMeta("taken_digitized".into())],
        ..Default::default()
    };
    assert!(config_needs_exif_date(&cfg));

    let cfg = RenameConfig {
        command_order: vec![
            RenameItem::Prefix("pre_".into()),
            RenameItem::InsertMeta("taken_modified".into()),
        ],
        ..Default::default()
    };
    assert!(!config_needs_exif_date(&cfg));
}

/// A date fixture whose fields are all distinct, so a test can tell which one
/// the engine actually read.
fn distinct_date_meta() -> (FileMeta, FileMeta) {
    use std::time::{Duration, UNIX_EPOCH};
    let mtime = UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    let created = UNIX_EPOCH + Duration::from_secs(1_100_000_000);
    let accessed = UNIX_EPOCH + Duration::from_secs(1_200_000_000);
    let exif: i64 = 915_148_800;
    let with_exif = FileMeta {
        created: Some(created),
        modified: Some(mtime),
        accessed: Some(accessed),
        exif_date: Some(exif),
    };
    let without_exif = FileMeta {
        exif_date: None,
        ..with_exif
    };
    (with_exif, without_exif)
}

/// Render `time`'s year the way the pipeline does for a `YYYY` Add Date with no
/// extra offset (i.e. local time), so expectations never hard-code a timezone.
fn local_year(time: std::time::SystemTime) -> String {
    format_date_offset(
        "YYYY",
        time,
        true,
        crate::core::rename::local_utc_offset_secs_at(time),
    )
}

/// `sanitize` accepts the EXIF-backed `auto_date_type` keys, so the engine has to
/// implement them. It did not: `date_type_time_source` had no arm for `taken_*`,
/// so it returned `None` and `apply_add_date` fell back to the wall clock —
/// silently stamping *today* on files whose photo date was available.
#[test]
fn test_auto_date_type_exif_keys_use_file_date() {
    use std::time::{Duration, UNIX_EPOCH};
    let (with_exif, without_exif) = distinct_date_meta();
    let exif_year = local_year(UNIX_EPOCH + Duration::from_secs(915_148_800));
    let mtime_year = local_year(UNIX_EPOCH + Duration::from_secs(1_000_000_000));
    assert_ne!(exif_year, mtime_year, "fixture must distinguish the dates");

    for ty in ["taken_original", "taken_digitized", "taken_recent"] {
        let config = RenameConfig {
            auto_date: AutoDateSection {
                add_date: Some("YYYY".into()),
                auto_date_type: Some(ty.into()),
                date_position: DatePosition::Prefix,
                auto_date_century: true,
                auto_date_offset: Some(0),
                ..Default::default()
            },
            ..Default::default()
        };
        let compiled = CompiledConfig::new(config);
        assert!(
            compiled.needs_exif_date,
            "{ty} must make planning read EXIF"
        );
        assert_eq!(
            process_filename("x.jpg", &compiled, None, None, false, &with_exif),
            format!("{exif_year}x.jpg"),
            "{ty} must use the file's EXIF date"
        );
        assert_eq!(
            process_filename("x.jpg", &compiled, None, None, false, &without_exif),
            format!("{mtime_year}x.jpg"),
            "{ty} must fall back to the file's mtime"
        );
    }
}

/// The remaining `taken_*` key is the Insert Meta spelling of the file's
/// modification time (`taken_modified` is an alias of `modified`), so both
/// spellings must keep that one meaning — and must never stamp the clock.
#[test]
fn test_auto_date_type_taken_modified_uses_mtime() {
    use std::time::{Duration, UNIX_EPOCH};
    let (with_exif, _) = distinct_date_meta();
    let mtime_year = local_year(UNIX_EPOCH + Duration::from_secs(1_000_000_000));
    let config = RenameConfig {
        auto_date: AutoDateSection {
            add_date: Some("YYYY".into()),
            auto_date_type: Some("taken_modified".into()),
            date_position: DatePosition::Prefix,
            auto_date_century: true,
            auto_date_offset: Some(0),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert!(
        !compiled.needs_exif_date,
        "an mtime-backed type must not pay for EXIF"
    );
    assert_eq!(
        process_filename("x.jpg", &compiled, None, None, false, &with_exif),
        format!("{mtime_year}x.jpg"),
        "taken_modified must use mtime, not the EXIF date or the clock"
    );
}

/// The EXIF gate must cover *every* input the pipeline reads EXIF for, not just
/// the Insert Meta tags: an `AddDate` served by an EXIF-backed type needs the
/// date, and without the read it silently degraded to mtime.
#[test]
fn test_config_needs_exif_date_covers_auto_date_type() {
    let add_date = AutoDateSection {
        add_date: Some("YYYY".into()),
        date_position: DatePosition::Prefix,
        ..Default::default()
    };
    let cfg_with = |ty: &str| RenameConfig {
        auto_date: AutoDateSection {
            auto_date_type: Some(ty.into()),
            ..add_date.clone()
        },
        ..Default::default()
    };

    for ty in ["taken_original", "taken_digitized", "taken_recent"] {
        let cfg = cfg_with(ty);
        assert!(config_needs_exif_date(&cfg), "{ty}: untranslated config");
        assert!(
            CompiledConfig::new(cfg).needs_exif_date,
            "{ty}: compiled config"
        );
    }

    // Served by stat() or the clock: reading EXIF would be pure cost.
    for ty in [
        "creation_curr",
        "creation_new",
        "modified_curr",
        "modified_new",
        "accessed_curr",
        "accessed_new",
        "taken_modified",
        "current",
    ] {
        let cfg = cfg_with(ty);
        assert!(!config_needs_exif_date(&cfg), "{ty} must not read EXIF");
    }

    // An EXIF-backed type with no Add Date to apply it: nothing runs.
    let cfg = RenameConfig {
        auto_date: AutoDateSection {
            auto_date_type: Some("taken_original".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(!config_needs_exif_date(&cfg));
}

#[test]
fn test_insert_meta_unknown_tag() {
    use std::time::UNIX_EPOCH;
    let config = RenameConfig::default();
    let result = apply_insert_meta(
        "file.txt",
        "unknown-tag",
        &FileMeta {
            modified: Some(UNIX_EPOCH),
            ..Default::default()
        },
        false,
        &config,
    );
    assert_eq!(result, "file.txt");
}

#[test]
fn test_extension_add_if_missing_no_ext() {
    let config = RenameConfig {
        extension: ExtensionSection {
            extension_replace: Some("txt".to_string()),
            extension_add_if_missing: true,
            extension_mode: None,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename("readme", &compiled, None, None, false, &FileMeta::default());
    assert_eq!(result, "readme.txt");
}

#[test]
fn test_extension_add_if_missing_already_has_ext() {
    let config = RenameConfig {
        extension: ExtensionSection {
            extension_replace: Some("md".to_string()),
            extension_add_if_missing: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename(
        "readme.txt",
        &compiled,
        None,
        None,
        false,
        &FileMeta::default(),
    );
    assert_eq!(result, "readme.md");
}

#[test]
fn test_exclude_regex_compilation() {
    let config = RenameConfig {
        filters: FiltersSection {
            exclude_regex: Some(r"^\\.".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        compiled.config.filters.exclude_regex.as_deref(),
        Some(r"^\\.")
    );
}

#[test]
fn test_add_date_in_stacking() {
    let config = RenameConfig {
        command_order: vec![
            RenameItem::AddDate("YYYY_".to_string()),
            RenameItem::Prefix("backup_".to_string()),
        ],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename(
        "photo.jpg",
        &compiled,
        None,
        None,
        false,
        &FileMeta::default(),
    );
    assert!(result.starts_with("backup_"));
    assert!(result.ends_with("photo.jpg"));
}

#[test]
fn test_add_file_date_in_stacking() {
    use std::time::UNIX_EPOCH;
    let file_time = UNIX_EPOCH + std::time::Duration::from_secs(1673793045);
    let meta = FileMeta {
        modified: Some(file_time),
        ..Default::default()
    };
    let config = RenameConfig {
        command_order: vec![RenameItem::AddFileDate("YYYY_".to_string())],
        auto_date: AutoDateSection {
            auto_date_century: true,
            date_position: DatePosition::Prefix,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename("notes.txt", &compiled, None, None, false, &meta);
    assert_eq!(result, "2023_notes.txt");
}

#[test]
fn test_insert_meta_in_stacking() {
    use std::time::UNIX_EPOCH;
    let file_time = UNIX_EPOCH + std::time::Duration::from_secs(1704067200);
    let meta = FileMeta {
        modified: Some(file_time),
        ..Default::default()
    };
    // auto_date_offset: Some(0) means local+0 = local.
    let local_secs = crate::core::rename::local_utc_offset_secs_at(file_time);
    let expected = format_date_offset("YYYY-MM-DD", file_time, true, local_secs);
    let config = RenameConfig {
        command_order: vec![RenameItem::InsertMeta("exif-date".to_string())],
        auto_date: AutoDateSection {
            auto_date_century: true,
            auto_date_offset: Some(0),
            date_position: DatePosition::Prefix,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename("photo.jpg", &compiled, None, None, false, &meta);
    assert_eq!(result, format!("{}photo.jpg", expected));
}

#[test]
fn test_insert_meta_in_stacking_no_time() {
    let config = RenameConfig {
        command_order: vec![RenameItem::InsertMeta("exif-date".to_string())],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename(
        "photo.jpg",
        &compiled,
        None,
        None,
        false,
        &FileMeta::default(),
    );
    assert_eq!(result, "photo.jpg");
}

#[test]
fn test_preset_serialization_roundtrip() {
    let cfg = RenameConfig {
        replace: ReplaceSection {
            replace: Some("old".to_string()),
            with: Some("new".to_string()),
            ..Default::default()
        },
        case: CaseSection {
            case_name: Some(CaseMode::Title),
            ..Default::default()
        },
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Suffix),
            numbering_start: 10,
            numbering_increment: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let restored: RenameConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.replace.replace, cfg.replace.replace);
    assert_eq!(restored.replace.with, cfg.replace.with);
    assert_eq!(restored.case.case_name, cfg.case.case_name);
    assert_eq!(
        restored.numbering.numbering_mode,
        cfg.numbering.numbering_mode
    );
    assert_eq!(
        restored.numbering.numbering_start,
        cfg.numbering.numbering_start
    );
    assert_eq!(
        restored.numbering.numbering_increment,
        cfg.numbering.numbering_increment
    );
}

#[test]
fn test_preset_empty_config() {
    let cfg = RenameConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let restored: RenameConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.replace.replace, None);
    assert_eq!(restored.remove.remove_first, None);
}

#[test]
fn test_extension_add_if_missing_with_stacking() {
    let config = RenameConfig {
        command_order: vec![
            RenameItem::AddDate("2024_".to_string()),
            RenameItem::ExtensionAddIfMissing,
        ],
        auto_date: AutoDateSection {
            date_position: DatePosition::Prefix,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename("noext", &compiled, None, None, false, &FileMeta::default());
    assert!(result.starts_with("2024_"));
    assert!(!result.ends_with('.'));
}

#[test]
fn test_new_fields_default_values() {
    let cfg = RenameConfig::default();
    // Compare section defaults against Default::default() to avoid hardcoding
    assert_eq!(cfg.numbering, NumberingSection::default());
    assert_eq!(cfg.auto_date.date_position, DatePosition::None);
    assert_eq!(cfg.filters, FiltersSection::default());
    assert_eq!(cfg.copy_to, CopyToSection::default());
    assert_eq!(cfg.regex, RegexSection::default());
    assert_eq!(cfg.replace, ReplaceSection::default());
    assert_eq!(cfg.remove, RemoveSection::default());
    assert_eq!(cfg.add, AddSection::default());
    assert_eq!(cfg.case, CaseSection::default());
    assert_eq!(cfg.extension, ExtensionSection::default());
    assert_eq!(cfg.name, NameSection::default());
    assert_eq!(cfg.auto_date, AutoDateSection::default());
    assert_eq!(cfg.append_folder, AppendFolderSection::default());
    assert_eq!(cfg.move_copy, MoveCopySection::default());
    assert_eq!(cfg.name_segment, NameSegmentSection::default());
    assert_eq!(cfg.special, SpecialSection::default());
}

#[test]
fn test_crop_special() {
    // Crop Special keeps only the matched substring
    let config = RenameConfig {
        remove: RemoveSection {
            crop_mode: Some(CropMode::Special),
            remove_crop: Some("keep_this".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "prefix_keep_this_suffix.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "keep_this.txt"
    );
}

#[test]
fn test_crop_special_no_match() {
    // Crop Special when text is not found preserves the original
    let config = RenameConfig {
        remove: RemoveSection {
            crop_mode: Some(CropMode::Special),
            remove_crop: Some("nonexistent".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "original_name.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "original_name.txt"
    );
}

#[test]
fn test_crop_special_in_stacking() {
    // Crop Special works in command_order stacking mode
    let config = RenameConfig {
        command_order: vec![
            RenameItem::RemoveCrop(CropMode::Special, "target".to_string()),
            RenameItem::Prefix("found_".to_string()),
        ],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "before_target_after.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "found_target.txt"
    );
}

#[test]
fn test_format_date_no_century() {
    use std::time::UNIX_EPOCH;
    let time = UNIX_EPOCH + std::time::Duration::from_secs(1673793045);
    // Use format_date_offset with 0 (UTC) to test century flag
    assert_eq!(format_date_offset("YYYY", time, true, 0), "2023");
    // century=false -> YYYY = 23
    assert_eq!(format_date_offset("YYYY", time, false, 0), "23");
    // YY always uses 2 digits regardless
    assert_eq!(format_date_offset("YY", time, true, 0), "23");
    assert_eq!(format_date_offset("YY", time, false, 0), "23");
}

#[test]
fn test_format_date_offset_hours() {
    use std::time::UNIX_EPOCH;
    // Use format_date_offset with raw seconds to test offset logic directly.
    let time = UNIX_EPOCH; // 1970-01-01 00:00:00 UTC
    assert_eq!(
        format_date_offset("YYYY-MM-DD_hh", time, true, 0),
        "1970-01-01_00"
    );
    // +7200 seconds = +2 hours
    assert_eq!(
        format_date_offset("YYYY-MM-DD_hh", time, true, 7200),
        "1970-01-01_02"
    );
    let later = UNIX_EPOCH + std::time::Duration::from_secs(86400); // 1970-01-02 00:00:00
    assert_eq!(
        format_date_offset("YYYY-MM-DD_hh", later, true, 0),
        "1970-01-02_00"
    );
    // +18000 seconds = +5 hours
    assert_eq!(
        format_date_offset("YYYY-MM-DD_hh", later, true, 18000),
        "1970-01-02_05"
    );
    // -43200 seconds = -12 hours -> previous day
    assert_eq!(
        format_date_offset("YYYY-MM-DD_hh", later, true, -43200),
        "1970-01-01_12"
    );
}

#[test]
fn test_add_date_no_century() {
    use std::time::UNIX_EPOCH;
    let file_time = UNIX_EPOCH + std::time::Duration::from_secs(1673793045);
    let meta = FileMeta {
        modified: Some(file_time),
        ..Default::default()
    };
    // century=false -> YYYY formatted as 2-digit year
    let config = RenameConfig {
        auto_date: AutoDateSection {
            add_file_date: Some("YYYY".to_string()),
            auto_date_century: false,
            date_position: DatePosition::Prefix,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename("photo.jpg", &compiled, None, None, false, &meta);
    assert_eq!(result, "23photo.jpg");
}

#[test]
fn test_add_file_date_offset_hours() {
    use std::time::UNIX_EPOCH;
    let file_time = UNIX_EPOCH + std::time::Duration::from_secs(86400);
    let meta = FileMeta {
        modified: Some(file_time),
        ..Default::default()
    };
    // auto_date_offset is on top of local time. Compute expected by
    // getting local offset seconds and adding 5*3600.
    let local_secs = crate::core::rename::local_utc_offset_secs_at(file_time);
    let total_offset_secs = local_secs + 5 * 3600;
    let expected = format_date_offset("hh", file_time, false, total_offset_secs);
    let config = RenameConfig {
        auto_date: AutoDateSection {
            add_file_date: Some("hh".to_string()),
            auto_date_offset: Some(5),
            date_position: DatePosition::Prefix,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename("photo.jpg", &compiled, None, None, false, &meta);
    assert_eq!(result, format!("{}photo.jpg", expected));
}

#[test]
fn test_add_file_date_offset_negative_hours() {
    use std::time::UNIX_EPOCH;
    let file_time = UNIX_EPOCH + std::time::Duration::from_secs(86400); // 1970-01-02 00:00:00
    let meta = FileMeta {
        modified: Some(file_time),
        ..Default::default()
    };
    // offset is on top of local time. Compute expected by
    // getting local offset seconds and adding -12*3600.
    let local_secs = crate::core::rename::local_utc_offset_secs_at(file_time);
    let total_offset_secs = local_secs + (-12 * 3600);
    let expected = format_date_offset("YYYY-MM-DD_hh", file_time, true, total_offset_secs);
    let config = RenameConfig {
        auto_date: AutoDateSection {
            add_file_date: Some("YYYY-MM-DD_hh".to_string()),
            auto_date_century: true,
            auto_date_offset: Some(-12),
            date_position: DatePosition::Prefix,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename("photo.jpg", &compiled, None, None, false, &meta);
    assert_eq!(result, format!("{}photo.jpg", expected));
}

#[test]
fn test_add_file_date_offset_and_century_in_stacking() {
    use std::time::UNIX_EPOCH;
    let file_time = UNIX_EPOCH + std::time::Duration::from_secs(1673793045); // 2023-01-15 14:30:45
    let meta = FileMeta {
        modified: Some(file_time),
        ..Default::default()
    };
    // In stacking, century and offset are read from config, not the RenameItem.
    // offset is on top of local time. Compute expected dynamically.
    let local_secs = crate::core::rename::local_utc_offset_secs_at(file_time);
    let total_offset_secs = local_secs + 3 * 3600;
    let expected = format_date_offset("YYYY-MM-DD_hh", file_time, false, total_offset_secs);
    let config = RenameConfig {
        command_order: vec![RenameItem::AddFileDate("YYYY-MM-DD_hh".to_string())],
        auto_date: AutoDateSection {
            auto_date_century: false,
            auto_date_offset: Some(3),
            date_position: DatePosition::Prefix,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename("photo.jpg", &compiled, None, None, false, &meta);
    assert_eq!(result, format!("{}photo.jpg", expected));
}

#[test]
fn test_insert_meta_with_century_and_offset() {
    use std::time::UNIX_EPOCH;
    let file_time = UNIX_EPOCH + std::time::Duration::from_secs(1704067200); // 2024-01-01 00:00:00
    let meta = FileMeta {
        modified: Some(file_time),
        ..Default::default()
    };
    let config = RenameConfig {
        auto_date: AutoDateSection {
            auto_date_century: false,
            auto_date_offset: Some(6),
            ..Default::default()
        },
        ..Default::default()
    };
    // offset is on top of local time. Compute expected dynamically so the
    // test passes in any timezone (a hardcoded date can roll back a day for
    // western UTC offsets, e.g. 2024-01-01 00:00 UTC - 8h + 6h = 2023-12-31).
    let local_secs = crate::core::rename::local_utc_offset_secs();
    let total_offset_secs = local_secs + 6 * 3600;
    // century=false -> "YYYY" formatted as 2-digit year
    let expected = format_date_offset("YYYY-MM-DD", file_time, false, total_offset_secs);
    let result = apply_insert_meta("IMG_0001.jpg", "exif-date", &meta, false, &config);
    assert_eq!(result, format!("{}IMG_0001.jpg", expected));
}

#[test]
fn test_new_fields_default_values_extended() {
    let cfg = RenameConfig::default();
    // Verify section defaults match Default::default() to avoid hardcoding
    assert_eq!(cfg.numbering, NumberingSection::default());
    assert_eq!(cfg.auto_date, AutoDateSection::default());
    assert_eq!(cfg.append_folder, AppendFolderSection::default());
    assert_eq!(cfg.remove, RemoveSection::default());
}

#[test]
fn test_serde_crop_special_roundtrip() {
    let cfg = RenameConfig {
        remove: RemoveSection {
            crop_mode: Some(CropMode::Special),
            remove_crop: Some("match_me".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let restored: RenameConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.remove.crop_mode, Some(CropMode::Special));
    assert_eq!(restored.remove.remove_crop, Some("match_me".to_string()));
}
