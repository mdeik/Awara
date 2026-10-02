use super::*;

#[test]
fn test_regex_simple() {
    let config = RenameConfig {
        regex: RegexSection {
            regex_match: Some(r"(track)".to_string()),
            regex_replace: Some("song".to_string()),
            regex_simple: true,
            ..Default::default()
        },
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
        "simple mode should not match parens as capture group"
    );
}

#[test]
fn test_regex_matches_whitespace_literally() {
    let config = RenameConfig {
        regex: RegexSection {
            regex_match: Some(r"hello world".to_string()),
            regex_replace: Some("hi".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    // Whitespace in the pattern is literal, not the extended `/x` flag.
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

/// Regression: a literal space in the pattern must match under the default
/// config, e.g. `- \d+` against "... - 01 ...".
#[test]
fn test_regex_literal_space_in_pattern() {
    let config = RenameConfig {
        regex: RegexSection {
            regex_match: Some(r"- \d+".to_string()),
            regex_replace: Some("- 001".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "Example Movie - 01 (Sample Cut).mkv",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "Example Movie - 001 (Sample Cut).mkv"
    );
}

/// A lone Match pattern is not a no-op: an empty "With" deletes the match.
#[test]
fn test_regex_empty_replacement_deletes_match() {
    let config = RenameConfig {
        regex: RegexSection {
            regex_match: Some(r"\d+".to_string()),
            regex_replace: None,
            ..Default::default()
        },
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
        "track.mp3"
    );
}

#[test]
fn test_case_exception() {
    let config = RenameConfig {
        case: CaseSection {
            case_name: Some(CaseMode::Lower),
            case_exception: Some("iOS".to_string()),
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "iOS App",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "iOS app",
        "iOS should stay iOS while rest is lowercased"
    );

    let config2 = RenameConfig {
        case: CaseSection {
            case_name: Some(CaseMode::Upper),
            case_exception: Some("and".to_string()),
        },
        ..Default::default()
    };
    let compiled2 = CompiledConfig::new(config2);
    assert_eq!(
        process_filename(
            "rock and roll",
            &compiled2,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "ROCK and ROLL",
        "'and' should stay lowercase while rest is uppercased"
    );
}

#[test]
fn test_case_exception_multi_word() {
    let config = RenameConfig {
        case: CaseSection {
            case_name: Some(CaseMode::Title),
            case_exception: Some("an the of in".to_string()),
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "the keeper of the key",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "the Keeper of the Key",
        "Title case with exceptions"
    );
}

#[test]
fn test_remove_unicode() {
    let config = RenameConfig {
        remove: RemoveSection {
            remove_first: Some(2),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "🦀cafe.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "afe.txt"
    );
}

#[test]
fn test_replace_first() {
    let config = RenameConfig {
        replace: ReplaceSection {
            replace: Some("foo".to_string()),
            with: Some("bar".to_string()),
            replace_first: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "foo foo.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "bar foo.txt"
    );

    let config_all = RenameConfig {
        replace: ReplaceSection {
            replace: Some("foo".to_string()),
            with: Some("bar".to_string()),
            replace_first: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled_all = CompiledConfig::new(config_all);
    assert_eq!(
        process_filename(
            "foo foo.txt",
            &compiled_all,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "bar bar.txt"
    );
}

#[test]
fn test_remove_last() {
    let config = RenameConfig {
        remove: RemoveSection {
            remove_last: Some(4),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "filename.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "file.txt"
    );
}

#[test]
fn test_trailing_space_fix() {
    // Removing symbols trims the trailing space it leaves behind.
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
            "name @.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "name.txt"
    );

    let config_no_remove = RenameConfig::default();
    let compiled_no_remove = CompiledConfig::new(config_no_remove);
    // No operation: trailing spaces are preserved (Windows strips them
    // automatically when creating the file).
    assert_eq!(
        process_filename(
            "name .txt",
            &compiled_no_remove,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "name .txt"
    );
}

#[test]
fn test_regex_case_sensitivity() {
    // Regex is case-sensitive regardless of `replace_case_sensitive`.
    let config = RenameConfig {
        regex: RegexSection {
            regex_match: Some(r"foo".to_string()),
            regex_replace: Some(r"bar".to_string()),
            ..Default::default()
        },
        replace: ReplaceSection {
            replace_case_sensitive: false,
            ..Default::default()
        },
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
        "regex is case-sensitive: FOO does not match foo"
    );

    // Case-insensitivity is opt-in via an inline (?i).
    let config_insens = RenameConfig {
        regex: RegexSection {
            regex_match: Some(r"(?i)foo".to_string()),
            regex_replace: Some(r"bar".to_string()),
            ..Default::default()
        },
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
fn test_add_at_clamp() {
    let config = RenameConfig {
        add: AddSection {
            add_insert: Some("X".to_string()),
            add_at: Some(100),
            ..Default::default()
        },
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
        "fileX.txt"
    );
}

#[test]
fn test_remove_words_boundary() {
    let config = RenameConfig {
        remove: RemoveSection {
            remove_words: Some("cat".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "cat catastrophe.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        " catastrophe.txt"
    );
}
