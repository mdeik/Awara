use super::*;

/// Verifies that `diff_colored` handles various inputs without panicking
/// and produces non-empty segments.
#[test]
fn test_diff_colored_smoke() {
    let cases = &[
        ("", ""),
        ("a", ""),
        ("", "a"),
        ("abc", "abc"),
        ("abc", "abd"),
        ("hello", "world"),
        ("hello world", "hello mars"),
        ("abcdef", "azcefd"),
        ("short", "longer text here"),
        ("abc123", "xyz789"),
        ("\u{00e9}", "e"),
        ("foobar", "barfoo"),
        ("  spaces  ", "spaces"),
        ("rust_is_cool", "rust-is-cool"),
    ];
    for (orig, new) in cases {
        let (orig_segs, new_segs) = diff_colored(orig, new);
        // Verify all segments are non-empty
        for seg in &orig_segs {
            assert!(
                !seg.text.is_empty(),
                "Empty orig seg for {:?} -> {:?}",
                orig,
                new
            );
        }
        for seg in &new_segs {
            assert!(
                !seg.text.is_empty(),
                "Empty new seg for {:?} -> {:?}",
                orig,
                new
            );
        }
    }
}

#[test]
fn test_diff_colored_prefix_suffix_optimization() {
    // Middle change
    let (orig_segs, new_segs) = diff_colored("file_01.txt", "file_02.txt");
    assert_eq!(orig_segs.len(), 3);
    assert_eq!(orig_segs[0].text, "file_0");
    assert_eq!(orig_segs[0].bg, DIFF_KEEP_BG);
    assert_eq!(orig_segs[1].text, "1");
    assert_eq!(orig_segs[1].bg, DIFF_REMOVE_BG);
    assert_eq!(orig_segs[2].text, ".txt");
    assert_eq!(orig_segs[2].bg, DIFF_KEEP_BG);

    assert_eq!(new_segs.len(), 3);
    assert_eq!(new_segs[0].text, "file_0");
    assert_eq!(new_segs[0].bg, DIFF_KEEP_BG);
    assert_eq!(new_segs[1].text, "2");
    assert_eq!(new_segs[1].bg, DIFF_ADD_BG);
    assert_eq!(new_segs[2].text, ".txt");
    assert_eq!(new_segs[2].bg, DIFF_KEEP_BG);

    // Pure insertion
    let (orig_segs, new_segs) = diff_colored("file.txt", "my_file.txt");
    assert_eq!(orig_segs.len(), 1);
    assert_eq!(orig_segs[0].text, "file.txt");
    assert_eq!(orig_segs[0].bg, DIFF_KEEP_BG);

    assert_eq!(new_segs.len(), 2);
    assert_eq!(new_segs[0].text, "my_");
    assert_eq!(new_segs[0].bg, DIFF_ADD_BG);
    assert_eq!(new_segs[1].text, "file.txt");
    assert_eq!(new_segs[1].bg, DIFF_KEEP_BG);

    // Pure deletion
    let (orig_segs, new_segs) = diff_colored("prefix_file.txt", "file.txt");
    assert_eq!(orig_segs.len(), 2);
    assert_eq!(orig_segs[0].text, "prefix_");
    assert_eq!(orig_segs[0].bg, DIFF_REMOVE_BG);
    assert_eq!(orig_segs[1].text, "file.txt");
    assert_eq!(orig_segs[1].bg, DIFF_KEEP_BG);

    assert_eq!(new_segs.len(), 1);
    assert_eq!(new_segs[0].text, "file.txt");
    assert_eq!(new_segs[0].bg, DIFF_KEEP_BG);
}

// ── Middle structural fast-paths (avoid LCS) ──

/// Inserts flanking a kept middle block are caught structurally: the kept
/// block stays contiguous with inserts before and after it.
#[test]
fn test_middle_containment_insert() {
    let (orig_segs, new_segs) = diff_colored("abcd", "aXbYcd");
    assert_eq!(texts(&orig_segs), vec!["a", "b", "cd"]);
    assert_eq!(
        bgs(&orig_segs),
        vec![DIFF_KEEP_BG, DIFF_KEEP_BG, DIFF_KEEP_BG]
    );
    assert_eq!(texts(&new_segs), vec!["a", "X", "b", "Y", "cd"]);
    assert_eq!(
        bgs(&new_segs),
        vec![
            DIFF_KEEP_BG,
            DIFF_ADD_BG,
            DIFF_KEEP_BG,
            DIFF_ADD_BG,
            DIFF_KEEP_BG,
        ]
    );
}

/// Symmetric deletion: the kept block stays contiguous, removals flank it.
#[test]
fn test_middle_containment_delete() {
    let (orig_segs, new_segs) = diff_colored("aXbYcd", "abcd");
    assert_eq!(texts(&orig_segs), vec!["a", "X", "b", "Y", "cd"]);
    assert_eq!(
        bgs(&orig_segs),
        vec![
            DIFF_KEEP_BG,
            DIFF_REMOVE_BG,
            DIFF_KEEP_BG,
            DIFF_REMOVE_BG,
            DIFF_KEEP_BG,
        ]
    );
    assert_eq!(texts(&new_segs), vec!["a", "b", "cd"]);
    assert_eq!(
        bgs(&new_segs),
        vec![DIFF_KEEP_BG, DIFF_KEEP_BG, DIFF_KEEP_BG]
    );
}

/// Realistic removal: dropping the parens around a counter keeps the
/// counter itself aligned instead of fragmenting it via LCS backtracking.
#[test]
fn test_middle_containment_remove_parens() {
    let (orig_segs, new_segs) = diff_colored("photo (1)", "photo1");
    assert_eq!(texts(&orig_segs), vec!["photo", " (", "1", ")"]);
    assert_eq!(
        bgs(&orig_segs),
        vec![DIFF_KEEP_BG, DIFF_REMOVE_BG, DIFF_KEEP_BG, DIFF_REMOVE_BG]
    );
    assert_eq!(texts(&new_segs), vec!["photo", "1"]);
    assert_eq!(bgs(&new_segs), vec![DIFF_KEEP_BG, DIFF_KEEP_BG]);
}

/// Multiple candidate positions: the *first* occurrence is kept, so the
/// result is deterministic (LCS backtracking could keep either one).
#[test]
fn test_middle_containment_first_occurrence() {
    let (orig_segs, new_segs) = diff_colored("ab", "XabYabZ");
    assert_eq!(texts(&orig_segs), vec!["ab"]);
    assert_eq!(bgs(&orig_segs), vec![DIFF_KEEP_BG]);
    assert_eq!(texts(&new_segs), vec!["X", "ab", "YabZ"]);
    assert_eq!(bgs(&new_segs), vec![DIFF_ADD_BG, DIFF_KEEP_BG, DIFF_ADD_BG]);
}

/// Middles with no shared character are a full replacement — same output as
/// LCS, but without allocating the DP table.
#[test]
fn test_middle_disjoint_replacement() {
    let (orig_segs, new_segs) = diff_colored("old.txt", "new.txt");
    assert_eq!(texts(&orig_segs), vec!["old", ".txt"]);
    assert_eq!(bgs(&orig_segs), vec![DIFF_REMOVE_BG, DIFF_KEEP_BG]);
    assert_eq!(texts(&new_segs), vec!["new", ".txt"]);
    assert_eq!(bgs(&new_segs), vec![DIFF_ADD_BG, DIFF_KEEP_BG]);
}

/// A genuine mixed edit (shared chars, neither side contained) still falls
/// through to LCS and keeps its exact alignment.
#[test]
fn test_middle_mixed_edit_still_lcs() {
    let (orig_segs, new_segs) = diff_colored("abcde", "aXcdZ");
    assert_eq!(texts(&orig_segs), vec!["a", "b", "cd", "e"]);
    assert_eq!(
        bgs(&orig_segs),
        vec![DIFF_KEEP_BG, DIFF_REMOVE_BG, DIFF_KEEP_BG, DIFF_REMOVE_BG]
    );
    assert_eq!(texts(&new_segs), vec!["a", "X", "cd", "Z"]);
    assert_eq!(
        bgs(&new_segs),
        vec![DIFF_KEEP_BG, DIFF_ADD_BG, DIFF_KEEP_BG, DIFF_ADD_BG]
    );
}

// ── Front/end structural diff ──

pub(super) fn cfg_with(items: Vec<RenameItem>) -> RenameConfig {
    RenameConfig {
        command_order: items,
        ..Default::default()
    }
}

pub(super) fn texts(segs: &[DiffSeg]) -> Vec<&str> {
    segs.iter().map(|s| s.text.as_str()).collect()
}

pub(super) fn bgs(segs: &[DiffSeg]) -> Vec<Color32> {
    segs.iter().map(|s| s.bg).collect()
}

/// Prefix insert: kept name untouched, prefix added at the front.
#[test]
fn test_front_end_prefix() {
    let cfg = cfg_with(vec![RenameItem::Prefix("IMG_".into())]);
    let d = FrontEndDiff::from_config(&cfg).expect("prefix is front/end");
    let (orig, new) = d.diff_segs("photo.jpg", "IMG_photo.jpg", "").unwrap();
    assert_eq!(texts(&orig), vec!["photo.jpg"]);
    assert_eq!(bgs(&orig), vec![DIFF_KEEP_BG]);
    assert_eq!(texts(&new), vec!["IMG_", "photo.jpg"]);
    assert_eq!(bgs(&new), vec![DIFF_ADD_BG, DIFF_KEEP_BG]);
}

/// Remove-first on a repeated pattern: the leading chars are removed, not
/// whatever the LCS backtracking happens to align.
#[test]
fn test_front_end_remove_first_repeated_pattern() {
    let cfg = cfg_with(vec![RenameItem::RemoveFirst(3)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("abcabc", "abc", "").unwrap();
    assert_eq!(texts(&orig), vec!["abc", "abc"]);
    assert_eq!(bgs(&orig), vec![DIFF_REMOVE_BG, DIFF_KEEP_BG]);
    assert_eq!(texts(&new), vec!["abc"]);
    assert_eq!(bgs(&new), vec![DIFF_KEEP_BG]);
}

/// Remove-last: kept head, removed tail.
#[test]
fn test_front_end_remove_last() {
    let cfg = cfg_with(vec![RenameItem::RemoveLast(2)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("hello", "hel", "").unwrap();
    assert_eq!(texts(&orig), vec!["hel", "lo"]);
    assert_eq!(bgs(&orig), vec![DIFF_KEEP_BG, DIFF_REMOVE_BG]);
    assert_eq!(texts(&new), vec!["hel"]);
}

/// Negative counts flip the side: RemoveFirst(-2) removes from the end.
#[test]
fn test_front_end_sign_flip() {
    let cfg = cfg_with(vec![RenameItem::RemoveFirst(-2)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, _) = d.diff_segs("hello", "hel", "").unwrap();
    assert_eq!(texts(&orig), vec!["hel", "lo"]);
    assert_eq!(bgs(&orig), vec![DIFF_KEEP_BG, DIFF_REMOVE_BG]);

    let cfg = cfg_with(vec![RenameItem::RemoveLast(-2)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, _) = d.diff_segs("hello", "llo", "").unwrap();
    assert_eq!(texts(&orig), vec!["he", "llo"]);
    assert_eq!(bgs(&orig), vec![DIFF_REMOVE_BG, DIFF_KEEP_BG]);
}

/// Remove From/To that only crops the end (e.g. From=-3 To=0) uses the
/// exact front/end path instead of the O(n·m) LCS string diff.
#[test]
fn test_front_end_remove_from_to_end_crop() {
    let cfg = cfg_with(vec![RenameItem::RemoveFromTo(-3, 0)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("hello", "he", "").unwrap();
    assert_eq!(texts(&orig), vec!["he", "llo"]);
    assert_eq!(bgs(&orig), vec![DIFF_KEEP_BG, DIFF_REMOVE_BG]);
    assert_eq!(texts(&new), vec!["he"]);
    assert_eq!(bgs(&new), vec![DIFF_KEEP_BG]);
}

/// A pure front crop (From=1 To=3) is exact too.
#[test]
fn test_front_end_remove_from_to_front_crop() {
    let cfg = cfg_with(vec![RenameItem::RemoveFromTo(1, 3)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("hello", "lo", "").unwrap();
    assert_eq!(texts(&orig), vec!["hel", "lo"]);
    assert_eq!(bgs(&orig), vec![DIFF_REMOVE_BG, DIFF_KEEP_BG]);
    assert_eq!(texts(&new), vec!["lo"]);
}

/// A middle Remove From/To cannot be modelled exactly → LCS fallback.
#[test]
fn test_front_end_remove_from_to_middle_falls_back() {
    let cfg = cfg_with(vec![RenameItem::RemoveFromTo(2, 4)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    assert!(d.diff_segs("hello", "ho", "").is_none());
}

/// An end crop after a prefix resolves against the prefix-inclusive
/// current name and still produces exact segments.
#[test]
fn test_front_end_remove_from_to_after_prefix() {
    let cfg = cfg_with(vec![
        RenameItem::Prefix("x".into()),
        RenameItem::RemoveFromTo(-2, 0),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("hello", "xhel", "").unwrap();
    assert_eq!(texts(&orig), vec!["hel", "lo"]);
    assert_eq!(bgs(&orig), vec![DIFF_KEEP_BG, DIFF_REMOVE_BG]);
    assert_eq!(texts(&new), vec!["x", "hel"]);
}

/// A removal can consume previously inserted prefix text.
#[test]
fn test_front_end_removal_eats_prefix() {
    let cfg = cfg_with(vec![
        RenameItem::Prefix("xy".into()),
        RenameItem::RemoveFirst(1),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("abcd", "yabcd", "").unwrap();
    assert_eq!(texts(&orig), vec!["abcd"]);
    assert_eq!(bgs(&orig), vec![DIFF_KEEP_BG]);
    assert_eq!(texts(&new), vec!["y", "abcd"]);
    assert_eq!(bgs(&new), vec![DIFF_ADD_BG, DIFF_KEEP_BG]);
}

/// Interleaved front/end removals and inserts, in command order.
#[test]
fn test_front_end_interleaved() {
    let cfg = cfg_with(vec![
        RenameItem::RemoveFirst(2),
        RenameItem::Prefix("x".into()),
        RenameItem::RemoveLast(1),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("abcdef", "xcde", "").unwrap();
    assert_eq!(texts(&orig), vec!["ab", "cde", "f"]);
    assert_eq!(
        bgs(&orig),
        vec![DIFF_REMOVE_BG, DIFF_KEEP_BG, DIFF_REMOVE_BG]
    );
    assert_eq!(texts(&new), vec!["x", "cde"]);
    assert_eq!(bgs(&new), vec![DIFF_ADD_BG, DIFF_KEEP_BG]);
}

/// Date prefix: generated text of unknown length, located by search.
#[test]
fn test_front_end_date_prefix_search() {
    let mut cfg = cfg_with(vec![RenameItem::AddDate("ymd".into())]);
    cfg.auto_date.date_position = DatePosition::Prefix;
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("ab", "2026-08-12ab", "").unwrap();
    assert_eq!(texts(&orig), vec!["ab"]);
    assert_eq!(texts(&new), vec!["2026-08-12", "ab"]);
    assert_eq!(bgs(&new), vec![DIFF_ADD_BG, DIFF_KEEP_BG]);
}

/// Numbering on both sides: unknown insert text before and after the name.
#[test]
fn test_front_end_numbering_both_search() {
    let cfg = cfg_with(vec![RenameItem::Numbering(
        NumberingMode::Both,
        1,
        1,
        2,
        None,
        None,
        None,
        None,
        None,
    )]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("ab", "01ab01", "").unwrap();
    assert_eq!(texts(&orig), vec!["ab"]);
    assert_eq!(texts(&new), vec!["01", "ab", "01"]);
    assert_eq!(bgs(&new), vec![DIFF_ADD_BG, DIFF_KEEP_BG, DIFF_ADD_BG]);
}

/// Middle-changing ops (replace, case, ...) fall back to LCS. Inserts are
/// now part of the front/end family (middle positions are resolved per row).
#[test]
fn test_front_end_non_frontend_ops_fall_back() {
    let cfg = cfg_with(vec![RenameItem::Replace(
        "a".into(),
        "b".into(),
        true,
        false,
    )]);
    assert!(FrontEndDiff::from_config(&cfg).is_none());

    // Removals combined with generated inserts also fall back.
    let cfg = cfg_with(vec![
        RenameItem::RemoveFirst(1),
        RenameItem::AddDate("ymd".into()),
    ]);
    assert!(FrontEndDiff::from_config(&cfg).is_none());

    // Numbering in insert mode is not front/end.
    let cfg = cfg_with(vec![RenameItem::Numbering(
        NumberingMode::Insert,
        1,
        1,
        2,
        None,
        Some(2),
        None,
        None,
        None,
    )]);
    assert!(FrontEndDiff::from_config(&cfg).is_none());
}

/// Everything removed: original is entirely red, new name entirely green.
#[test]
fn test_front_end_removes_everything() {
    let cfg = cfg_with(vec![RenameItem::RemoveFirst(5)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("hello", "", "").unwrap();
    assert_eq!(texts(&orig), vec!["hello"]);
    assert_eq!(bgs(&orig), vec![DIFF_REMOVE_BG]);
    assert!(new.is_empty());
}

/// A config that models the pipeline incorrectly must not emit segments
/// (the caller falls back to the LCS diff).
#[test]
fn test_front_end_verification_rejects_mismatch() {
    let cfg = cfg_with(vec![RenameItem::Prefix("x".into())]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    // The new name does not match the model's reconstruction.
    assert!(d.diff_segs("ab", "zz", "").is_none());
}
