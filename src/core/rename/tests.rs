use super::*;
use crate::core::config::*;

fn make_config(name_mode: Option<&str>, name_value: Option<&str>) -> RenameConfig {
    RenameConfig {
        name: NameSection {
            name_mode: name_mode.map(|s| s.to_string()),
            name_value: name_value.map(|s| s.to_string()),
        },
        ..RenameConfig::default()
    }
}

fn run_process(name_mode: Option<&str>, name_value: Option<&str>, original: &str) -> String {
    use std::time::SystemTime;
    let config = make_config(name_mode, name_value);
    let compiled = CompiledConfig::new(config);
    let meta = FileMeta {
        modified: Some(SystemTime::now()),
        created: None,
        accessed: None,
        exif_date: None,
    };
    process_filename(original, &compiled, None, None, false, &meta)
}

/// Same as [`run_process`] but for a directory target.
fn run_process_dir(name_mode: Option<&str>, name_value: Option<&str>, original: &str) -> String {
    let config = make_config(name_mode, name_value);
    let compiled = CompiledConfig::new(config);
    process_filename(original, &compiled, None, None, true, &FileMeta::default())
}

// ── Keep ──

#[test]
fn test_name_keep_default() {
    assert_eq!(run_process(None, None, "photo.txt"), "photo.txt");
}

#[test]
fn test_name_keep_no_extension() {
    assert_eq!(run_process(Some("keep"), None, "photo"), "photo");
}

// ── Fixed ──

#[test]
fn test_name_fixed_replaces_stem() {
    assert_eq!(
        run_process(Some("fixed"), Some("new_name"), "photo.txt"),
        "new_name.txt"
    );
}

#[test]
fn test_name_fixed_empty_value_clears_stem() {
    assert_eq!(run_process(Some("fixed"), Some(""), "photo.txt"), ".txt");
}

#[test]
fn test_name_fixed_no_value_clears_stem() {
    assert_eq!(run_process(Some("fixed"), None, "photo.txt"), ".txt");
}

#[test]
fn test_name_fixed_no_extension() {
    assert_eq!(
        run_process(Some("fixed"), Some("new_name"), "photo"),
        "new_name"
    );
}

// ── Remove ──

#[test]
fn test_name_remove_keeps_ext_as_dotfile() {
    let result = run_process(Some("remove"), None, "photo.txt");
    assert_eq!(result, ".txt");
}

#[test]
fn test_name_remove_no_extension() {
    // Empty result falls back to original
    assert_eq!(run_process(Some("remove"), None, "photo"), "photo");
}

// ── Reverse ──

#[test]
fn test_name_reverse_stem_only() {
    assert_eq!(run_process(Some("reverse"), None, "12345.txt"), "54321.txt");
}

#[test]
fn test_name_reverse_no_extension() {
    assert_eq!(run_process(Some("reverse"), None, "hello"), "olleh");
}

#[test]
fn test_name_reverse_multi_dot() {
    // Only the last dot is the extension boundary
    assert_eq!(
        run_process(Some("reverse"), None, "my.file.txt"),
        "elif.ym.txt"
    );
}

// ── Pad Numbers ──

#[test]
fn test_name_pad_numbers_default_zero() {
    assert_eq!(
        run_process(Some("pad_numbers"), Some("5"), "img_42.txt"),
        "img_00042.txt"
    );
}

#[test]
fn test_name_pad_numbers_custom_char() {
    assert_eq!(
        run_process(Some("pad_numbers"), Some("5>A"), "img_42.txt"),
        "img_AAA42.txt"
    );
}

#[test]
fn test_name_pad_numbers_pad_smaller_than_digits() {
    // If pad_amount <= existing digits, no change
    assert_eq!(
        run_process(Some("pad_numbers"), Some("2"), "img_12345.txt"),
        "img_12345.txt"
    );
}

#[test]
fn test_name_pad_numbers_no_digits() {
    assert_eq!(
        run_process(Some("pad_numbers"), Some("5"), "photo.txt"),
        "photo.txt"
    );
}

#[test]
fn test_name_pad_numbers_no_value() {
    assert_eq!(
        run_process(Some("pad_numbers"), Some(""), "img_42.txt"),
        "img_42.txt"
    );
}

#[test]
fn test_name_pad_numbers_only_first_digit_group() {
    assert_eq!(
        run_process(Some("pad_numbers"), Some("4"), "42_99.txt"),
        "0042_99.txt"
    );
}

// ── Reformat Date ──

#[test]
fn test_name_reformat_date_yyyy_mm_dd() {
    assert_eq!(
        run_process(Some("reformat_date"), None, "report-2024-01-15.txt"),
        "report-2024-01-15.txt"
    );
}

#[test]
fn test_name_reformat_date_mm_dd_yyyy() {
    assert_eq!(
        run_process(Some("reformat_date"), None, "report-01-15-2024.txt"),
        "report-2024-01-15.txt"
    );
}

#[test]
fn test_name_reformat_date_no_date() {
    assert_eq!(
        run_process(Some("reformat_date"), None, "plain-filename.txt"),
        "plain-filename.txt"
    );
}

#[test]
fn test_name_reformat_date_with_dots() {
    assert_eq!(
        run_process(Some("reformat_date"), None, "2024.01.15_report.txt"),
        "2024-01-15_report.txt"
    );
}

#[test]
fn test_name_reformat_date_custom_output() {
    assert_eq!(
        run_process(
            Some("reformat_date"),
            Some("%m/%d/%Y"),
            "2024-01-15_report.txt"
        ),
        "01/15/2024_report.txt"
    );
}

#[test]
fn test_name_reformat_date_custom_parse_and_output() {
    assert_eq!(
        run_process(
            Some("reformat_date"),
            Some("%d.%m.%Y>%Y-%m-%d"),
            "15.01.2024_report.txt"
        ),
        "2024-01-15_report.txt"
    );
}

// ── Negative value tests ──
// These test that the processing functions correctly handle isize values
// where negative means "count from the opposite end".

#[test]
fn test_apply_remove_first_positive() {
    assert_eq!(apply_remove_first("hello", 2), "llo");
    assert_eq!(apply_remove_first("hello", 0), "hello");
    assert_eq!(apply_remove_first("hi", 5), "");
}

#[test]
fn test_apply_remove_first_negative() {
    // Negative: remove from end instead
    assert_eq!(apply_remove_first("hello", -2), "hel"); // remove last 2
    assert_eq!(apply_remove_first("hello", -1), "hell"); // remove last 1
    assert_eq!(apply_remove_first("hello", -5), ""); // remove all
    assert_eq!(apply_remove_first("hello", -10), ""); // clamp
}

#[test]
fn test_apply_remove_last_positive() {
    assert_eq!(apply_remove_last("hello", 2), "hel");
    assert_eq!(apply_remove_last("hello", 0), "hello");
    assert_eq!(apply_remove_last("hi", 5), "");
}

#[test]
fn test_apply_remove_last_negative() {
    // Negative: remove from start instead
    assert_eq!(apply_remove_last("hello", -2), "llo"); // remove first 2
    assert_eq!(apply_remove_last("hello", -1), "ello"); // remove first 1
    assert_eq!(apply_remove_last("hello", -5), ""); // remove all
    assert_eq!(apply_remove_last("hello", -10), ""); // clamp
}

#[test]
fn test_apply_remove_last_counts_all_chars() {
    // N counts every character — spaces, symbols, and unicode included.
    let name = "Sample Series\u{A789} Chronicles (2021) - S03E03 - Episode One\u{A789} Part Two (1080p WEB-DL x265)";
    assert_eq!(
        apply_remove_last(name, 18),
        "Sample Series\u{A789} Chronicles (2021) - S03E03 - Episode One\u{A789} Part Two ("
    );
    // Each increment removes exactly one more character (the trailing space).
    assert_eq!(
        apply_remove_last(name, 19),
        "Sample Series\u{A789} Chronicles (2021) - S03E03 - Episode One\u{A789} Part Two "
    );
}

#[test]
fn test_remove_last_pipeline_keeps_trailing_space() {
    // The pipeline must not trim the trailing space produced by the cut:
    // Windows strips it automatically when creating the file, and trimming
    // here would make "remove last N" remove N+1 characters.
    let mut cfg = RenameConfig::default();
    cfg.remove.remove_last = Some(19);
    let compiled = CompiledConfig::from_ref(&cfg);
    let out = process_filename(
        "Sample Series\u{A789} Chronicles (2021) - S03E03 - Episode One\u{A789} Part Two (1080p WEB-DL x265).mkv",
        &compiled,
        None,
        None,
        false,
        &FileMeta::default(),
    );
    assert_eq!(
        out,
        "Sample Series\u{A789} Chronicles (2021) - S03E03 - Episode One\u{A789} Part Two .mkv"
    );
}

#[test]
fn test_apply_remove_from_to_positive() {
    assert_eq!(apply_remove_from_to("hello", 2, 4), "ho"); // remove "el"
    assert_eq!(apply_remove_from_to("hello", 1, 3), "lo"); // remove "hel" -> wait: from=1,to=3 => remove chars at 0..=2?
}

#[test]
fn test_apply_remove_from_to_positive_edge() {
    // from=1, to=3  in "hello" (len 5): from_0=0, to_0=3 => remove chars[0..3] = "hel" => "lo"
    assert_eq!(apply_remove_from_to("hello", 1, 3), "lo");
    // from=3, to=5  => remove chars[2..5] = "llo" => "he"
    assert_eq!(apply_remove_from_to("hello", 3, 5), "he");
}

#[test]
fn test_apply_remove_from_to_negative_from() {
    // "hello" len 5: from=-3 => 5 + (-3) = 2 (0-based), to=4 (0-based)
    // remove chars[2..4] = "ll" => "heo"
    assert_eq!(apply_remove_from_to("hello", -3, 4), "heo");
}

#[test]
fn test_apply_remove_from_to_negative_to() {
    // "hello" len 5: from=2 (1-based) => 0-based 1, to=-1 => the last
    // character, so 5+(-1)+1 = 5 (0-based exclusive)
    // remove chars[1..5] = "ello" => "h"
    assert_eq!(apply_remove_from_to("hello", 2, -1), "h");
}

#[test]
fn test_apply_remove_from_to_both_negative() {
    // "hello world" len 11: from=-6 => 5 (0-based), to=-2 => 2nd-to-last
    // character, so 11+(-2)+1 = 10 (0-based exclusive)
    // remove chars[5..10] = " worl" => "hell" + "d" = "hellod"
    assert_eq!(apply_remove_from_to("hello world", -6, -2), "hellod");
    // Doc example: From -3 To -2 removes third-to-last to second-to-last
    // => chars[8..10] = "rl" => "hello wod"
    assert_eq!(apply_remove_from_to("hello world", -3, -2), "hello wod");
}

#[test]
fn test_apply_remove_from_to_zero_to() {
    // to=0 means "through the end of the name": from=-3 => last 3 chars
    assert_eq!(apply_remove_from_to("hello world", -3, 0), "hello wo");
    // from=-1, to=0 => just the last character
    assert_eq!(apply_remove_from_to("hello world", -1, 0), "hello worl");
    // from positive, to=0 => from position N through the end
    assert_eq!(apply_remove_from_to("hello", 3, 0), "he");
}

#[test]
fn test_apply_remove_from_to_mixed_neg_pos() {
    // from=4 (4th char), to=-3 (3rd-from-last): remove chars[3..9] = "lo wor"
    assert_eq!(apply_remove_from_to("hello world", 4, -3), "helld");
    // from=2 (2nd char), to=-1 (last char): remove 2nd through last
    assert_eq!(apply_remove_from_to("hello", 2, -1), "h");
    // from=-3 (3rd-from-last) comes after to=4 (4th char): nothing to remove
    assert_eq!(apply_remove_from_to("hello world", -3, 4), "hello world");
}

#[test]
fn test_apply_remove_from_to_invalid() {
    // from > to  => no change
    assert_eq!(apply_remove_from_to("hello", 4, 2), "hello");
    // from=-1, to=-1 removes just the last character (-1 = last char)
    assert_eq!(apply_remove_from_to("hello", -1, -1), "hell");
    // to=0 on an empty string removes nothing
    assert_eq!(apply_remove_from_to("", -3, 0), "");
}

#[test]
fn test_apply_insert_negative() {
    // "hello" len 5: pos=-1 => insert at 5-1=4
    assert_eq!(apply_insert("hello", "X", -1), "hellXo");
    // pos=-2 => insert at 5-2=3
    assert_eq!(apply_insert("hello", "X", -2), "helXlo");
    // pos=-5 => insert at 0
    assert_eq!(apply_insert("hello", "X", -5), "Xhello");
    // pos=-10 => insert at 0 (clamp)
    assert_eq!(apply_insert("hello", "X", -10), "Xhello");
    // pos=0 => insert at 0 (prefix)
    assert_eq!(apply_insert("hello", "X", 0), "Xhello");
}

#[test]
fn test_apply_insert_positive() {
    assert_eq!(apply_insert("hello", "X", 3), "helXlo");
    assert_eq!(apply_insert("hello", "X", 5), "helloX");
    assert_eq!(apply_insert("hello", "X", 10), "helloX");
}

#[test]
fn test_resolve_insert_pos() {
    assert_eq!(resolve_insert_pos(0, 4), 0);
    assert_eq!(resolve_insert_pos(2, 4), 2);
    assert_eq!(resolve_insert_pos(99, 4), 4); // clamps to the end
    assert_eq!(resolve_insert_pos(-1, 4), 3); // before last char
    assert_eq!(resolve_insert_pos(-4, 4), 0); // very front
    assert_eq!(resolve_insert_pos(-10, 4), 0); // clamped
}

#[test]
fn test_apply_move_part_negative_from() {
    // "abcdef" len 6: from=-3 (1-based position 4), len=2, to=1 (start)
    // move chars[3..5] = "de" to start => "deabcf"
    let result = apply_move_part("abcdef", -3, 2, 1, None);
    assert_eq!(result, "deabcf");
}

#[test]
fn test_apply_move_part_negative_len_and_to() {
    // Negative length: use absolute value
    // "abcdef", from=2, len=-3 => len=3, move chars at 1..4 = "bcd"
    // to=-1 (end) => insert after last char: chars=['a','e','f'], len=3, insert at 3 => "aefbcd"
    let result = apply_move_part("abcdef", 2, -3, -1, None);
    assert_eq!(result, "aefbcd");
}

#[test]
fn test_apply_move_part_positive_to_end() {
    // Move first 2 chars to end
    // "abcdef", from=1, len=2, to=-1 (end) => after last char
    // drain 'a','b', chars=['c','d','e','f'], len=4, insert at index 4 => "cdefab"
    let result = apply_move_part("abcdef", 1, 2, -1, None);
    assert_eq!(result, "cdefab");
}

#[test]
fn test_apply_move_part_negative_dest_before_last() {
    // "abcdef", from=1, len=2, to=-2 => one gap before the end (before last char)
    // drain 'a','b', chars=['c','d','e','f'], len=4, insert at index 3 => "cdeabf"
    let result = apply_move_part("abcdef", 1, 2, -2, None);
    assert_eq!(result, "cdeabf");
}

#[test]
fn test_apply_move_part_positive_dest_beyond_end() {
    // Positive dest larger than string clamps to the very end
    let result = apply_move_part("abcdef", 1, 2, 20, None);
    assert_eq!(result, "cdefab");
}

#[test]
fn test_apply_move_part_with_separator() {
    // Move first 3 chars to end with "_": rest + sep + part
    assert_eq!(apply_move_part("123456", 1, 3, -1, Some("_")), "456_123");
    // Move last 2 chars to start with "-": part + sep + rest
    assert_eq!(apply_move_part("abcdef", -2, 2, 1, Some("-")), "ef-abcd");
    // Middle destination: head + part + sep + tail ("abcdef" → drain "ab",
    // insert at gap 2 of "cdef" → "cd" + "ab" + "_" + "ef")
    assert_eq!(apply_move_part("abcdef", 1, 2, 3, Some("_")), "cdab_ef");
}

#[test]
fn test_apply_copy_part_with_separator() {
    // Copy first 3 chars to end with "_": original intact + sep + part
    assert_eq!(apply_copy_part("123456", 1, 3, -1, Some("_")), "123456_123");
    // Copy range "cd" to position 2 with "-": original "cd" stays,
    // inserted copy: head + part + sep + tail => "a" + "cd" + "-" + "bcdef"
    assert_eq!(apply_copy_part("abcdef", 3, 2, 2, Some("-")), "acd-bcdef");
}

#[test]
fn test_apply_copy_part_negative() {
    // "abcdef", from=-2 => 1-based position 5 (char 'e'), len=1, to=-1 (end)
    // Copy 'e' to the very end => "abcdefe"
    let result = apply_copy_part("abcdef", -2, 1, -1, None);
    assert_eq!(result, "abcdefe");
}

#[test]
fn test_apply_copy_part_to_end() {
    // to=-1 means "to end": insert after the last character
    let result = apply_copy_part("abcdef", 1, 3, -1, None);
    // Copy chars[0..3]="abc", insert at index 6 (end) => "abcdefabc"
    assert_eq!(result, "abcdefabc");
}

#[test]
fn test_apply_copy_part_negative_dest_before_last() {
    // to=-2 => one gap before the end (before last char)
    // Copy "abc" at index 5 of "abcdef" => "abcdeabc" + "f" = "abcdeabcf"
    let result = apply_copy_part("abcdef", 1, 3, -2, None);
    assert_eq!(result, "abcdeabcf");
}

#[test]
fn test_apply_dirname_negative_pos() {
    // Verify it runs without panicking
    let _ = apply_dirname("file.txt", "folder", Some("_"), Some(-1));
    let _ = apply_dirname("file.txt", "folder", Some("_"), Some(-2));
}

#[test]
fn test_name_segment_with_negative_values() {
    // Test NameSegment via process_filename with command_order items
    // Since name_segment only works in the command_order path,
    // we set it up via items_to_config → RenameItem::NameSegment
    let config = RenameConfig {
        command_order: vec![RenameItem::NameSegment(-6, -1)],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let meta = FileMeta::default();
    // "hello_world" len 11: from=-6 => 11-6=5, to=-1 => 11+(-1)+1=11
    // chars[5..11] = "_world"
    let result = process_filename("hello_world.txt", &compiled, None, None, false, &meta);
    assert_eq!(result, "_world.txt");
}

#[test]
fn test_name_segment_positive_values_via_items() {
    let config = RenameConfig {
        command_order: vec![RenameItem::NameSegment(1, 5)],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let meta = FileMeta::default();
    // 1-based: from=1 → chars index 0, to=5 → chars index 5
    // chars[0..5] = "hello"
    let result = process_filename("hello_world.txt", &compiled, None, None, false, &meta);
    assert_eq!(result, "hello.txt");
}

#[test]
fn test_name_segment_to_zero() {
    // to=0 means "through the end of the name" (same as Remove From/To).
    let config = RenameConfig {
        command_order: vec![RenameItem::NameSegment(3, 0)],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let meta = FileMeta::default();
    // "hello_world" len 11: from=3 → chars index 2, to=0 → through the end
    // chars[2..11] = "llo_world"
    let result = process_filename("hello_world.txt", &compiled, None, None, false, &meta);
    assert_eq!(result, "llo_world.txt");
}

#[test]
fn test_resolve_from_to_range_shared() {
    // Same math for Remove and Name Segment: 1-based positions, negative
    // counts from the end, to=0 = through the end.
    assert_eq!(resolve_from_to_range(11, -3, -2), Some((8, 10))); // doc example
    assert_eq!(resolve_from_to_range(11, -3, 0), Some((8, 11))); // last 3 chars
    assert_eq!(resolve_from_to_range(5, 2, 4), Some((1, 4)));
    assert_eq!(resolve_from_to_range(5, -1, -1), Some((4, 5))); // last char
    assert_eq!(resolve_from_to_range(5, 4, 2), None); // from > to
    assert_eq!(resolve_from_to_range(0, -3, 0), None); // empty name
}

#[test]
fn test_name_fixed_before_replace() {
    // NameFixed runs first, then Replace operates on the new stem
    let config = RenameConfig {
        command_order: vec![
            RenameItem::NameFixed("abc123".to_string()),
            RenameItem::Replace("abc".to_string(), "xyz".to_string(), false, false),
        ],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let meta = FileMeta::default();
    // 1. NameFixed: "report_final.txt" → stem="abc123", ext="txt"
    // 2. Replace "abc" → "xyz": stem="abc123" → "xyz123"
    let result = process_filename("report_final.txt", &compiled, None, None, false, &meta);
    assert_eq!(result, "xyz123.txt");
}

#[test]
fn test_name_fixed_after_replace() {
    // Replace runs first, then NameFixed overwrites with its value
    let config = RenameConfig {
        command_order: vec![
            RenameItem::Replace(
                "report".to_string(),
                "overwritten".to_string(),
                false,
                false,
            ),
            RenameItem::NameFixed("final".to_string()),
        ],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let meta = FileMeta::default();
    // 1. Replace: "report_final.txt" → stem="report_final" → "overwritten_final"
    // 2. NameFixed: stem="overwritten_final" → "final", ext kept
    let result = process_filename("report_final.txt", &compiled, None, None, false, &meta);
    assert_eq!(result, "final.txt");
}

#[test]
fn test_name_reverse_then_replace() {
    let config = RenameConfig {
        command_order: vec![
            RenameItem::NameReverse,
            RenameItem::Replace("tseT".to_string(), "Pass".to_string(), false, false),
        ],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let meta = FileMeta::default();
    // 1. Reverse: "TestFile.txt" → stem="eliFtseT"
    // 2. Replace "tseT" → "Pass": stem="eliFtseT" → "eliFPass"
    let result = process_filename("TestFile.txt", &compiled, None, None, false, &meta);
    assert_eq!(result, "eliFPass.txt");
}

#[test]
fn test_name_remove_then_prefix() {
    // Remove stem entirely, then prefix adds to the empty stem
    let config = RenameConfig {
        command_order: vec![
            RenameItem::NameRemove,
            RenameItem::Prefix("new_".to_string()),
        ],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let meta = FileMeta::default();
    // 1. Remove: "photo.jpg" → stem="", ext="jpg"
    // 2. Prefix: stem="" → "new_"
    let result = process_filename("photo.jpg", &compiled, None, None, false, &meta);
    assert_eq!(result, "new_.jpg");
}

#[test]
fn test_name_reformat_date_before_case() {
    let config = RenameConfig {
        command_order: vec![
            RenameItem::NameReformatDate {
                parse_fmt: None,
                output_fmt: "YYYY_MM_DD".to_string(),
            },
            RenameItem::CaseName(CaseMode::Upper),
        ],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let meta = FileMeta::default();
    // 1. Reformat: "2024-01-15_photo.txt" → stem="2024_01_15_photo"
    // 2. Upper: stem → "2024_01_15_PHOTO"
    let result = process_filename("2024-01-15_photo.txt", &compiled, None, None, false, &meta);
    assert_eq!(result, "2024_01_15_PHOTO.txt");
}

// ── Directories have no extension (identification is files-only) ──

#[test]
fn test_name_fixed_dir_replaces_whole_name() {
    // A dotted directory name is all stem: Fixed replaces everything, leaving
    // no extension behind.
    assert_eq!(
        run_process_dir(Some("fixed"), Some("new"), "my.folder"),
        "new"
    );
}

#[test]
fn test_name_reverse_dir_reverses_whole_name() {
    // The dot is not an extension boundary for a directory, so it is reversed
    // along with the rest of the name instead of being preserved.
    assert_eq!(
        run_process_dir(Some("reverse"), None, "my.folder"),
        "redlof.ym"
    );
}

#[test]
fn test_name_remove_dir_has_no_extension_to_keep() {
    // With no extension, removing the stem leaves nothing, so the result falls
    // back to the original (mirroring an extensionless file).
    assert_eq!(
        run_process_dir(Some("remove"), None, "my.folder"),
        "my.folder"
    );
}

/// Run the pipeline for `original` with an explicit command order and `is_dir`.
fn run_items_ordered(order: Vec<RenameItem>, original: &str, is_dir: bool) -> String {
    let config = RenameConfig {
        command_order: order,
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    process_filename(
        original,
        &compiled,
        None,
        None,
        is_dir,
        &FileMeta::default(),
    )
}

fn extension_item(rep: Option<&str>, app: Option<&str>, rem: bool) -> RenameItem {
    RenameItem::Extension(
        ExtensionMode::Lower,
        rep.map(str::to_string),
        app.map(str::to_string),
        rem,
    )
}

#[test]
fn test_extension_fixed_and_extra_add_to_dir() {
    // Fixed/Extra *add* an extension, which is meaningful for a folder even
    // though a folder's own name is never split into an extension.
    for item in [
        extension_item(Some("bak"), None, false),
        extension_item(None, Some("bak"), false),
    ] {
        assert_eq!(
            run_items_ordered(vec![item], "my.folder", true),
            "my.folder.bak"
        );
    }
}

#[test]
fn test_extension_case_and_remove_ignored_for_dir() {
    // Case and Remove operate on an *existing* extension, which a directory
    // does not have, so both are no-ops.
    assert_eq!(
        run_items_ordered(
            vec![RenameItem::Extension(
                ExtensionMode::Upper,
                None,
                None,
                false
            )],
            "my.folder",
            true
        ),
        "my.folder"
    );
    assert_eq!(
        run_items_ordered(vec![extension_item(None, None, true)], "my.folder", true),
        "my.folder"
    );
}

#[test]
fn test_extension_fixed_on_dir_survives_later_name_op() {
    // An extension added to a folder is kept attached through later stem ops,
    // exactly like a real file extension.
    let order = || {
        vec![
            extension_item(Some("bak"), None, false),
            RenameItem::NameFixed("new".to_string()),
        ]
    };
    assert_eq!(run_items_ordered(order(), "MyFolder", true), "new.bak");
    // Same order on a file behaves identically.
    assert_eq!(run_items_ordered(order(), "photo.txt", false), "new.bak");
}

#[test]
fn test_extension_empty_value_shows_separator_dot() {
    // With no value typed yet, Fixed/Extra still show the separator dot, and
    // they do so uniformly for files (with and without an extension), folders,
    // and extensionless names.
    let cases = [
        // (original, is_dir, Extra result, Fixed result)
        ("photo.jpg", false, "photo.jpg.", "photo."),
        ("readme", false, "readme.", "readme."),
        ("myfolder", true, "myfolder.", "myfolder."),
    ];
    for (original, is_dir, extra, fixed) in cases {
        assert_eq!(
            run_items_ordered(
                vec![extension_item(None, Some(""), false)],
                original,
                is_dir
            ),
            extra,
            "Extra on {original} (is_dir={is_dir})"
        );
        assert_eq!(
            run_items_ordered(
                vec![extension_item(Some(""), None, false)],
                original,
                is_dir
            ),
            fixed,
            "Fixed on {original} (is_dir={is_dir})"
        );
    }
}

#[test]
fn test_extension_remove_has_no_dot() {
    // Removing the extension leaves no separator dot, for files or folders.
    assert_eq!(
        run_items_ordered(vec![extension_item(None, None, true)], "photo.jpg", false),
        "photo"
    );
    assert_eq!(
        run_items_ordered(vec![extension_item(None, None, true)], "myfolder", true),
        "myfolder"
    );
}
