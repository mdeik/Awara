use super::*;

#[test]
fn test_regex() {
    let config = RenameConfig {
        regex: RegexSection {
            regex_match: Some(r"track(\d+)".to_string()),
            regex_replace: Some("Song_$1".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "track01.mp3",
            &compiled,
            Some(0),
            None,
            false,
            &FileMeta::default()
        ),
        "Song_01.mp3"
    );
}

#[test]
fn test_replace() {
    let config = RenameConfig {
        replace: ReplaceSection {
            replace: Some("IMG_".to_string()),
            with: Some("Photo_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "IMG_2021.jpg",
            &compiled,
            Some(0),
            None,
            false,
            &FileMeta::default()
        ),
        "Photo_2021.jpg"
    );
}

#[test]
fn test_case() {
    let config = RenameConfig {
        case: CaseSection {
            case_name: Some(CaseMode::Title),
            // case_ext removed — use Extension section instead
            ..Default::default()
        },
        extension: ExtensionSection {
            extension_mode: Some(ExtensionMode::Upper),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "my_cool_file.TXT",
            &compiled,
            Some(0),
            None,
            false,
            &FileMeta::default()
        ),
        "My_Cool_File.TXT"
    );
}

#[test]
fn test_remove() {
    let config = RenameConfig {
        remove: RemoveSection {
            remove_first: Some(1),
            remove_last: Some(1),
            remove_symbols: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "[2021] Report.pdf",
            &compiled,
            Some(0),
            None,
            false,
            &FileMeta::default()
        ),
        "2021 Repor.pdf"
    );
}

#[test]
fn test_add() {
    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("Final_".to_string()),
            add_suffix: Some("_v1".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "report.doc",
            &compiled,
            Some(0),
            None,
            false,
            &FileMeta::default()
        ),
        "Final_report_v1.doc"
    );
}

#[test]
fn test_numbering() {
    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Suffix),
            numbering_start: 1,
            numbering_increment: 1,
            numbering_sep: Some("-".to_string()),
            numbering_pad: 3,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "photo.jpg",
            &compiled,
            Some(0),
            None,
            false,
            &FileMeta::default()
        ),
        "photo-001.jpg"
    );
    assert_eq!(
        process_filename(
            "image.png",
            &compiled,
            Some(1),
            None,
            false,
            &FileMeta::default()
        ),
        "image-002.png"
    );
}

#[test]
fn test_remove_extra() {
    let config = RenameConfig {
        remove: RemoveSection {
            remove_words: Some("bad ugly".to_string()),
            remove_symbols: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "bad file name!@#.txt",
            &compiled,
            Some(0),
            None,
            false,
            &FileMeta::default()
        ),
        " file name.txt"
    );
}

#[test]
fn test_regex_full_name() {
    let config = RenameConfig {
        regex: RegexSection {
            regex_match: Some(r"file\.txt".to_string()),
            regex_replace: Some(r"document.md".to_string()),
            regex_full_name: true,
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
        "document.md"
    );
}

#[test]
fn test_combined_flow() {
    let config = RenameConfig {
        regex: RegexSection {
            regex_match: Some(r"track(\d+)".to_string()),
            regex_replace: Some(r"song_$1".to_string()),
            ..Default::default()
        },
        remove: RemoveSection {
            remove_first: Some(5),
            ..Default::default()
        },
        add: AddSection {
            add_prefix: Some("Audio_".to_string()),
            ..Default::default()
        },
        case: CaseSection {
            case_name: Some(CaseMode::Upper),
            ..Default::default()
        },
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Suffix),
            numbering_start: 1,
            numbering_sep: Some("-".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "track01.mp3",
            &compiled,
            Some(0),
            None,
            false,
            &FileMeta::default()
        ),
        "AUDIO_01-1.mp3"
    );
}

#[test]
fn test_regex_capture_groups() {
    let config = RenameConfig {
        regex: RegexSection {
            regex_match: Some(r"(\w+)_(\d+)".to_string()),
            regex_replace: Some(r"$2-$1".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "file_123.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "123-file.txt"
    );
}

#[test]
fn test_remove_from_to() {
    let config = RenameConfig {
        remove: RemoveSection {
            remove_from: Some(2),
            remove_to: Some(4),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "abcdef.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "aef.txt"
    );
}

#[test]
fn test_remove_from_only_removes_to_end() {
    // From set without To removes through the end of the name (To=0), so
    // the GUI's cleared To field (displayed as 0) behaves like --remove-to 0.
    let config = RenameConfig {
        remove: RemoveSection {
            remove_from: Some(-3),
            remove_to: None,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    // "hello" stem len 5: from_0=2, to=0 => to_0=5, remove chars[2..5]="llo"
    assert_eq!(
        process_filename(
            "hello.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "he.txt"
    );
}

#[test]
fn test_remove_digits_chars_symbols() {
    let config = RenameConfig {
        remove: RemoveSection {
            remove_digits: true,
            remove_chars: Some("aeiou".to_string()),
            remove_symbols: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "123 apple!@#.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        " ppl.txt"
    );
}

#[test]
fn test_trim_double_spaces() {
    let config = RenameConfig {
        remove: RemoveSection {
            trim: true,
            double_spaces: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "  foo   bar  .txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "foo bar.txt"
    );
}

#[test]
fn test_add_word_space() {
    let config = RenameConfig {
        add: AddSection {
            add_word_space: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "CamelCaseName.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "Camel Case Name.txt"
    );
}

/// A high insert position clamps to the end of the stem; within the add
/// section the suffix must still be emitted last (never after the insert).
#[test]
fn test_add_suffix_last_with_high_insert() {
    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("pre_".to_string()),
            add_suffix: Some("_suf".to_string()),
            add_insert: Some("X".to_string()),
            add_at: Some(99),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "myfile.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "pre_myfileX_suf.txt"
    );
}

/// Insert positions are relative to the original filename, so a front
/// insert lands right after the prefix: the prefix stays the leading chars.
#[test]
fn test_add_prefix_first_with_front_insert() {
    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("pre_".to_string()),
            add_suffix: Some("_suf".to_string()),
            add_insert: Some("X".to_string()),
            add_at: Some(0),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "myfile.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "pre_Xmyfile_suf.txt"
    );
}

/// Negative insert positions count gaps from the end of the stem (-1 =
/// before the last character), reusing the same resolution as numbering's
/// insert mode (`resolve_insert_pos`).
#[test]
fn test_add_insert_negative_position() {
    let config = RenameConfig {
        add: AddSection {
            add_insert: Some("X".to_string()),
            add_at: Some(-1),
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
        "filXe.txt"
    );

    // -2 = before the second-to-last char; -99 clamps to the very front.
    let config = RenameConfig {
        add: AddSection {
            add_insert: Some("X".to_string()),
            add_at: Some(-2),
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
        "fiXle.txt"
    );
}

/// An unset insert position defaults to 0 (start of filename), so typing
/// insert text alone still inserts at the front.
#[test]
fn test_add_insert_default_position_zero() {
    let config = RenameConfig {
        add: AddSection {
            add_insert: Some("X".to_string()),
            add_at: None,
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
        "Xfile.txt"
    );
}

#[test]
fn test_case_modes() {
    let config = RenameConfig {
        case: CaseSection {
            case_name: Some(CaseMode::Sentence),
            ..Default::default()
        },
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
        "Hello world.txt"
    );

    let config_invert = RenameConfig {
        case: CaseSection {
            case_name: Some(CaseMode::Invert),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled_invert = CompiledConfig::new(config_invert);
    assert_eq!(
        process_filename(
            "HeLLo.txt",
            &compiled_invert,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "hEllO.txt"
    );
}
