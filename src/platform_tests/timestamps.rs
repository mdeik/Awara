use super::*;

#[test]
fn test_resolve_timestamp_current() {
    let spec = TimestampSpec::Current;
    let result = spec.resolve(Path::new("/tmp"), "test");
    assert!(result.is_some());
    let now = std::time::SystemTime::now();
    let diff = now.duration_since(result.unwrap()).unwrap_or_default();
    assert!(
        diff.as_secs() < 5,
        "timestamp should be within 5 seconds of now"
    );
}

#[test]
fn test_resolve_timestamp_fixed() {
    let spec = TimestampSpec::Fixed("2024-06-15 14:30:00".to_string());
    let result = spec.resolve(Path::new("/tmp"), "test");
    assert!(result.is_some());
}

#[test]
fn test_resolve_timestamp_fixed_invalid() {
    let spec = TimestampSpec::Fixed("not-a-date".to_string());
    let result = spec.resolve(Path::new("/tmp"), "test");
    assert!(result.is_none());
}

/// A `TimestampCache` is built once and reused across files: `Fixed` is parsed
/// once, and the per-file increment still applies.
#[test]
fn test_timestamp_cache_shared_across_files() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    fs::write(&a, b"").unwrap();
    fs::write(&b, b"").unwrap();

    let config = RenameConfig {
        special: SpecialSection {
            set_modified: Some(TimestampSpec::Fixed("2020-01-02 03:04:05".into())),
            timestamp_incr_secs: 10,
            ..Default::default()
        },
        ..Default::default()
    };
    let cache = TimestampCache::new(&config);
    cache.apply(&a, 0);
    cache.apply(&b, 1);

    let base = TimestampSpec::Fixed("2020-01-02 03:04:05".into())
        .resolve(&a, "modified")
        .unwrap();
    assert_eq!(fs::metadata(&a).unwrap().modified().unwrap(), base);
    assert_eq!(
        fs::metadata(&b).unwrap().modified().unwrap(),
        base + std::time::Duration::from_secs(10)
    );
}

/// File-dependent specs still resolve per file through the cache.
#[test]
fn test_timestamp_cache_per_file_specs_still_resolve() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("a.txt");
    fs::write(&f, b"x").unwrap();

    let config = RenameConfig {
        special: SpecialSection {
            set_accessed: Some(TimestampSpec::CopyFrom("modified".into())),
            ..Default::default()
        },
        ..Default::default()
    };
    TimestampCache::new(&config).apply(&f, 0);

    let meta = fs::metadata(&f).unwrap();
    assert_eq!(meta.accessed().unwrap(), meta.modified().unwrap());
}

#[test]
fn test_resolve_timestamp_copy_from_modified() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("time_test.txt");
    fs::write(&file, b"data").unwrap();

    let spec = TimestampSpec::CopyFrom("modified".to_string());
    let result = resolve_timestamp_for_file(&spec, &file, "modified");
    assert!(
        result.is_some(),
        "copy-from:modified should resolve for an existing file"
    );
}

#[test]
fn test_resolve_timestamp_copy_from_nonexistent_target() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("nonexistent.txt");

    let spec = TimestampSpec::CopyFrom("modified".to_string());
    let result = resolve_timestamp_for_file(&spec, &file, "modified");
    assert!(
        result.is_none(),
        "copy-from on nonexistent file should return None"
    );
}

#[test]
fn test_resolve_timestamp_delta_positive() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("delta_test.txt");
    fs::write(&file, b"").unwrap();

    let spec = TimestampSpec::Delta(3600);
    let result = resolve_timestamp_for_file(&spec, &file, "modified");
    assert!(result.is_some());
}

#[test]
fn test_resolve_timestamp_delta_negative() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("delta_neg_test.txt");
    fs::write(&file, b"").unwrap();

    let spec = TimestampSpec::Delta(-3600);
    let result = resolve_timestamp_for_file(&spec, &file, "modified");
    assert!(result.is_some());
}

#[test]
fn test_resolve_timestamp_taken() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("taken_test.txt");
    fs::write(&file, b"").unwrap();

    let spec = TimestampSpec::Taken;
    let result = resolve_timestamp_for_file(&spec, &file, "modified");
    assert!(
        result.is_some(),
        "Taken should resolve to file's modified time"
    );
}

#[test]
fn test_apply_timestamps_modified() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("apply_time_test.txt");
    fs::write(&file, b"data").unwrap();

    let config = RenameConfig {
        special: SpecialSection {
            set_modified: Some(TimestampSpec::Fixed("2020-01-01 12:00:00".to_string())),
            ..Default::default()
        },
        ..Default::default()
    };

    apply_timestamps_to_file(&file, &config, 0);

    let meta = std::fs::metadata(&file).unwrap();
    let modified = meta.modified().unwrap();
    // "2020-01-01 12:00:00" is local wall-clock time; the offset in effect on
    // that date applies (not the current offset, which shifts across DST).
    let expected_epoch =
        crate::core::rename::local_datetime_to_utc_secs(2020, 1, 1, 12, 0, 0).unwrap();
    let epoch =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(expected_epoch as u64);

    let diff = if modified > epoch {
        modified.duration_since(epoch)
    } else {
        epoch.duration_since(modified)
    };
    assert!(
        diff.unwrap_or_default().as_secs() < 2,
        "modified time should be close to 2020-01-01 12:00:00 local -> UTC {:?}, got {:?}",
        epoch,
        modified,
    );
}

#[test]
fn test_apply_timestamps_accessed() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("apply_atime_test.txt");
    fs::write(&file, b"data").unwrap();

    let config = RenameConfig {
        special: SpecialSection {
            set_accessed: Some(TimestampSpec::Current),
            ..Default::default()
        },
        ..Default::default()
    };

    apply_timestamps_to_file(&file, &config, 0);

    let meta = std::fs::metadata(&file).unwrap();
    let accessed = meta.accessed();
    assert!(accessed.is_ok());
}

#[test]
fn test_timestamp_spec_cli_roundtrip() {
    let cases = vec![
        (TimestampSpec::Current, "current"),
        (
            TimestampSpec::Fixed("2024-06-15 14:30:00".to_string()),
            "2024-06-15 14:30:00",
        ),
        (
            TimestampSpec::CopyFrom("modified".to_string()),
            "copy-from:modified",
        ),
        (TimestampSpec::Taken, "taken"),
        (TimestampSpec::Delta(-3600), "delta:-3600"),
    ];

    for (spec, expected_str) in cases {
        let serialized = spec.to_cli_str();
        assert_eq!(serialized, expected_str, "to_cli_str for {:?}", spec);
        assert_eq!(
            TimestampSpec::from_cli_str(&serialized),
            Some(spec.clone()),
            "round-trip failed for {:?}",
            spec
        );
    }
}

#[test]
fn test_timestamp_spec_from_cli_str_invalid() {
    // Invalid input must be rejected (None) so it is never persisted to config.
    assert!(TimestampSpec::from_cli_str("").is_none());
    assert!(TimestampSpec::from_cli_str("garbage").is_none());
    assert!(TimestampSpec::from_cli_str("fixed:").is_none());
    assert!(TimestampSpec::from_cli_str("delta:not-a-number").is_none());
    assert!(TimestampSpec::from_cli_str("copy-from:").is_none());
    assert!(TimestampSpec::from_cli_str("copy-from:bogus").is_none());
    assert!(TimestampSpec::from_cli_str("2024-13-01").is_none());
    assert!(TimestampSpec::from_cli_str("2024-02-30").is_none());
    assert!(TimestampSpec::from_cli_str("1969-12-31").is_none());
}

/// Fixed timestamps are normalized to canonical local 24-hour form, so AM/PM
/// input is accepted and converted instead of silently misread.
#[test]
fn test_timestamp_spec_from_cli_str_normalizes_time() {
    assert_eq!(
        TimestampSpec::from_cli_str("2024-01-15 02:30:00 PM"),
        Some(TimestampSpec::Fixed("2024-01-15 14:30:00".into()))
    );
    assert_eq!(
        TimestampSpec::from_cli_str("fixed:2024-01-15 12:00:00 AM"),
        Some(TimestampSpec::Fixed("2024-01-15 00:00:00".into()))
    );
    assert_eq!(
        TimestampSpec::from_cli_str("2024-01-15 12:00:00 PM"),
        Some(TimestampSpec::Fixed("2024-01-15 12:00:00".into()))
    );
    assert_eq!(
        TimestampSpec::from_cli_str("2024-01-15 2:5:6 pm"),
        Some(TimestampSpec::Fixed("2024-01-15 14:05:06".into()))
    );
    // Date-only stays date-only.
    assert_eq!(
        TimestampSpec::from_cli_str("2024-01-15"),
        Some(TimestampSpec::Fixed("2024-01-15".into()))
    );
    // ISO `T` separator is normalized to a space.
    assert_eq!(
        TimestampSpec::from_cli_str("2024-01-15T14:30:00"),
        Some(TimestampSpec::Fixed("2024-01-15 14:30:00".into()))
    );
    // A meridiem requires a 12-hour value; a 24-hour hour is contradictory.
    assert!(TimestampSpec::from_cli_str("2024-01-15 14:30:00 PM").is_none());
    assert!(TimestampSpec::from_cli_str("2024-01-15 00:30:00 AM").is_none());
}

/// The documented `fixed:` / `copy-from:` prefixes must be stripped, not
/// treated as part of the value (which silently failed to parse).
#[test]
fn test_timestamp_spec_from_cli_str_prefixes() {
    assert_eq!(
        TimestampSpec::from_cli_str("fixed:2020-01-01 12:00:00"),
        Some(TimestampSpec::Fixed("2020-01-01 12:00:00".into()))
    );
    assert_eq!(
        TimestampSpec::from_cli_str("copy-from:modified"),
        Some(TimestampSpec::CopyFrom("modified".into()))
    );
    assert_eq!(
        TimestampSpec::from_cli_str("delta:+3600"),
        Some(TimestampSpec::Delta(3600))
    );
}

#[test]
fn test_platform_max_path_is_reasonable() {
    let max = platform_max_path();
    #[cfg(target_os = "windows")]
    assert_eq!(max, 247);
    #[cfg(target_os = "macos")]
    assert_eq!(max, 1024);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    assert_eq!(max, 4096);
}

#[test]
fn test_name_max_constant() {
    assert_eq!(NAME_MAX, 255);
}

#[test]
fn test_apply_timestamps_modified_using_current() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("current_time.txt");
    fs::write(&file, b"data").unwrap();

    let config = RenameConfig {
        special: SpecialSection {
            set_modified: Some(TimestampSpec::Current),
            ..Default::default()
        },
        ..Default::default()
    };
    apply_timestamps_to_file(&file, &config, 0);

    let meta = std::fs::metadata(&file).unwrap();
    assert!(meta.modified().is_ok());
}

#[test]
fn test_apply_timestamps_modified_using_copy_from() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("source.txt");
    let dst = dir.path().join("target.txt");
    fs::write(&src, b"data").unwrap();
    fs::write(&dst, b"data").unwrap();

    let epoch = std::time::SystemTime::UNIX_EPOCH;
    filetime::set_file_mtime(&src, filetime::FileTime::from_system_time(epoch)).ok();

    let config = RenameConfig {
        special: SpecialSection {
            set_modified: Some(TimestampSpec::CopyFrom("modified".to_string())),
            ..Default::default()
        },
        ..Default::default()
    };
    apply_timestamps_to_file(&src, &config, 0);

    let meta = std::fs::metadata(&src).unwrap();
    assert!(
        meta.modified().is_ok(),
        "modified should be readable after copy-from"
    );
}

#[test]
fn test_apply_timestamps_modified_using_delta() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("delta_time.txt");
    fs::write(&file, b"data").unwrap();

    let config = RenameConfig {
        special: SpecialSection {
            set_modified: Some(TimestampSpec::Delta(3600)),
            ..Default::default()
        },
        ..Default::default()
    };
    apply_timestamps_to_file(&file, &config, 0);

    let meta = std::fs::metadata(&file).unwrap();
    assert!(meta.modified().is_ok());
}

#[test]
#[cfg(windows)]
fn test_apply_timestamps_created_windows() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("created_time.txt");
    fs::write(&file, b"data").unwrap();

    let config = RenameConfig {
        special: SpecialSection {
            set_created: Some(TimestampSpec::Current),
            ..Default::default()
        },
        ..Default::default()
    };
    apply_timestamps_to_file(&file, &config, 0);

    let meta = std::fs::metadata(&file).unwrap();
    let created = meta.created();
    assert!(
        created.is_ok(),
        "created time should be readable on Windows"
    );
}

#[test]
#[cfg(not(windows))]
fn test_set_file_attributes_system_non_windows_is_noop() {
    // Native volumes don't expose the system bit: the request is a capability
    // no-op (never a rename), so the path is unchanged.
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("system_test.txt");
    fs::write(&file, b"data").unwrap();

    let result = set_file_attributes(&file, "system");
    assert_eq!(result, file);
    assert!(result.exists());
}

#[test]
#[cfg(not(windows))]
fn test_set_file_attributes_archive_non_windows_is_noop() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("archive_test.txt");
    fs::write(&file, b"data").unwrap();

    let result = set_file_attributes(&file, "archive");
    assert_eq!(result, file);
    assert!(result.exists());
}
