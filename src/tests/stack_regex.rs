use super::*;

#[test]
fn test_stacking_regex_basic() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"track(\d+)".to_string(),
            "Song_$1".to_string(),
            false,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "track01.mp3",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "Song_01.mp3"
    );
}

#[test]
fn test_stacking_regex_full_name() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"file\.(.+)".to_string(),
            "doc.$1".to_string(),
            true,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "file.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "doc.txt"
    );
}

#[test]
fn test_stacking_regex_simple() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"(track)".to_string(),
            "song".to_string(),
            false,
            true,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "test_track.mp3",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "test_track.mp3",
        "simple mode escapes parens, so literal '(track)' does not match 'track'"
    );
}

#[test]
fn test_stacking_regex_matches_whitespace_literally() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"hello world".to_string(),
            "hi".to_string(),
            false,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "hello world.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "hi.txt"
    );
    assert_eq!(
        process_filename(
            "helloworld.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "helloworld.txt",
        "whitespace in the pattern must be matched literally"
    );
}

#[test]
fn test_stacking_regex_is_case_sensitive_by_default() {
    // Default: case-sensitive — "foo" must not match "FOO".
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"foo".to_string(),
            "bar".to_string(),
            false,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "FOO.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "FOO.txt",
        "case-sensitive: FOO does not match foo"
    );

    // Opt into case-insensitivity with an inline (?i) in the pattern.
    let config_insens = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"(?i)foo".to_string(),
            "bar".to_string(),
            false,
            false,
        )],
        ..Default::default()
    };
    let compiled_insens = CompiledConfig::new(config_insens);
    assert_eq!(
        process_filename(
            "FOO.txt",
            &compiled_insens,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "bar.txt",
        "(?i) opts into case-insensitive matching"
    );
}

#[test]
fn test_stacking_regex_capture_groups_and_replace() {
    let config = RenameConfig {
        command_order: vec![
            RenameItem::Regex(
                r"(\w+)_(\d+)".to_string(),
                r"$2-$1".to_string(),
                false,
                false,
            ),
            RenameItem::Prefix("ID_".to_string()),
        ],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "photo_42.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "ID_42-photo.txt",
        "regex reorders via capture groups, then prefix is prepended"
    );
}

#[test]
fn test_stacking_regex_simple_with_special_chars() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"file.txt".to_string(),
            "document.md".to_string(),
            true,
            true,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "file.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "document.md",
        "simple+full: escaped '.' matches literal '.' in 'file.txt'"
    );
    let config_no_simple = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"file.txt".to_string(),
            "doc.md".to_string(),
            true,
            false,
        )],
        ..Default::default()
    };
    let compiled_no_simple = CompiledConfig::new(config_no_simple);
    assert_eq!(
        process_filename(
            "fileXtxt",
            &compiled_no_simple,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "doc.md",
        "non-simple mode: '.' matches any single char in 'fileXtxt'"
    );
    let config_simple_2 = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"file.txt".to_string(),
            "doc.md".to_string(),
            true,
            true,
        )],
        ..Default::default()
    };
    let compiled_simple_2 = CompiledConfig::new(config_simple_2);
    assert_eq!(
        process_filename(
            "fileXtxt",
            &compiled_simple_2,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "fileXtxt",
        "simple+full: 'fileXtxt' should NOT match literal 'file.txt' pattern"
    );
}

#[test]
fn test_stacking_regex_with_capture_groups() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"(\d+)\s+(\w+)".to_string(),
            r"${2}_${1}".to_string(),
            false,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "42  photo.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "photo_42.txt",
        r"pattern with \s+ and capture groups reorders correctly"
    );
}

#[test]
fn test_stacking_regex_full_name_with_extension_change() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"(.*)\.jpg".to_string(),
            r"${1}_converted.png".to_string(),
            true,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "photo.jpg",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "photo_converted.png",
        "full name regex matches .jpg and replaces whole name including extension"
    );
}

#[test]
fn test_stacking_regex_no_match_preserves_original() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"nonexistent".to_string(),
            "replaced".to_string(),
            false,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "hello.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "hello.txt",
        "no regex match -> filename unchanged"
    );
}

#[test]
fn test_stacking_regex_first_match_only() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"a".to_string(),
            "X".to_string(),
            false,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "banana.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "bXnana.txt",
        "regex replaces only the first 'a' with 'X'"
    );
}

#[test]
fn test_stacking_regex_config_to_items_roundtrip() {
    let cfg = RenameConfig {
        regex: RegexSection {
            regex_match: Some(r"(\d+)".to_string()),
            regex_replace: Some(r"$1_copy".to_string()),
            regex_full_name: true,
            regex_simple: false,
        },
        ..Default::default()
    };
    let items = config_to_items(&cfg);
    let mut restored = RenameConfig::default();
    items_to_config(items, &mut restored);
    assert_eq!(restored.regex.regex_match, cfg.regex.regex_match);
    assert_eq!(restored.regex.regex_replace, cfg.regex.regex_replace);
    assert_eq!(restored.regex.regex_full_name, cfg.regex.regex_full_name);
    assert_eq!(restored.regex.regex_simple, cfg.regex.regex_simple);
}

#[test]
fn test_stacking_regex_replace_roundtrip() {
    let cfg = RenameConfig {
        replace: ReplaceSection {
            replace: Some("hello".to_string()),
            with: Some("world".to_string()),
            replace_case_sensitive: true,
            replace_first: false,
        },
        ..Default::default()
    };
    let items = config_to_items(&cfg);
    let mut restored = RenameConfig::default();
    items_to_config(items, &mut restored);
    assert_eq!(restored.replace.replace, cfg.replace.replace);
    assert_eq!(restored.replace.with, cfg.replace.with);
    assert_eq!(
        restored.replace.replace_case_sensitive,
        cfg.replace.replace_case_sensitive
    );
}

#[test]
fn test_stacking_remove_words_roundtrip() {
    let cfg = RenameConfig {
        remove: RemoveSection {
            remove_words: Some("bad ugly".to_string()),
            ..Default::default()
        },
        replace: ReplaceSection {
            replace_case_sensitive: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let items = config_to_items(&cfg);
    let mut restored = RenameConfig::default();
    items_to_config(items, &mut restored);
    assert_eq!(restored.remove.remove_words, cfg.remove.remove_words);
    assert_eq!(
        restored.replace.replace_case_sensitive,
        cfg.replace.replace_case_sensitive
    );
}

#[test]
fn test_regex_group_match_show_single_digit() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"(\d)".to_string(),
            "".to_string(),
            false,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename(
        "Show 04.mkv",
        &compiled,
        None,
        None,
        false,
        &FileMeta::default(),
    );
    assert_eq!(result, "Show 4.mkv");
}

/// `(\d\d)` matches two-digit runs. Only the FIRST run (`04`) is removed;
/// the second run (`12`) is left untouched.
#[test]
fn test_regex_first_match_only_consecutive_runs() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"(\d\d)".to_string(),
            "".to_string(),
            false,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "Show 0412.mkv",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "Show 12.mkv",
        "only the first two-digit run is removed"
    );
}

/// The same pattern appearing at multiple positions in the stem: only the
/// first occurrence is replaced. Here `\d+` matches `04` first; the `05`
/// after "Episode" is untouched.
#[test]
fn test_regex_first_match_only_multiple_positions() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"\d+".to_string(),
            "XX".to_string(),
            false,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "Show 04 Episode 05.mkv",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "Show XX Episode 05.mkv",
        "only the first number run is replaced"
    );
}

/// Capture group `(\d)` with replacement `[$1]` wraps only the first digit in
/// brackets; the second digit is left verbatim.
#[test]
fn test_regex_group_replacement_wraps_first_match_only() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"(\d)".to_string(),
            "[$1]".to_string(),
            false,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "Show 04.mkv",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "Show [0]4.mkv",
        "only the first digit is wrapped; second digit unchanged"
    );
}

/// Two capture groups: `(Show) (\d+)` with replacement `$2 - $1` reorders
/// the first match only. A second numeric run elsewhere is untouched.
#[test]
fn test_regex_two_groups_reorder_first_match_only() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Regex(
            r"(Show) (\d+)".to_string(),
            "$2 - $1".to_string(),
            false,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "Show 04 Show 07.mkv",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "04 - Show Show 07.mkv",
        "only the first 'Show NN' is reordered"
    );
}
