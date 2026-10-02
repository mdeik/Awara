use super::tests_diff::{bgs, cfg_with, texts};
use super::*;

// ── Positional inserts ──

/// Lone insert at position 0: pure prefix.
#[test]
fn test_insert_front_single() {
    let cfg = cfg_with(vec![RenameItem::Insert("X".into(), 0)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("ab", "Xab", "").unwrap();
    assert_eq!(texts(&orig), vec!["ab"]);
    assert_eq!(bgs(&orig), vec![DIFF_KEEP_BG]);
    assert_eq!(texts(&new), vec!["X", "ab"]);
    assert_eq!(bgs(&new), vec![DIFF_ADD_BG, DIFF_KEEP_BG]);
}

/// Lone insert past the end clamps to the end: pure suffix.
#[test]
fn test_insert_end_single_clamped() {
    let cfg = cfg_with(vec![RenameItem::Insert("X".into(), 99)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("ab", "abX", "").unwrap();
    assert_eq!(texts(&orig), vec!["ab"]);
    assert_eq!(texts(&new), vec!["ab", "X"]);
    assert_eq!(bgs(&new), vec![DIFF_KEEP_BG, DIFF_ADD_BG]);
}

/// Lone middle insert: exact split at the resolved position.
#[test]
fn test_insert_middle_single() {
    let cfg = cfg_with(vec![RenameItem::Insert("X".into(), 2)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("abcd", "abXcd", "").unwrap();
    assert_eq!(texts(&orig), vec!["abcd"]);
    assert_eq!(bgs(&orig), vec![DIFF_KEEP_BG]);
    assert_eq!(texts(&new), vec!["ab", "X", "cd"]);
    assert_eq!(bgs(&new), vec![DIFF_KEEP_BG, DIFF_ADD_BG, DIFF_KEEP_BG]);
}

/// Negative insert positions count gaps from the end (-1 = before the last
/// character), mirroring the core's `apply_insert`.
#[test]
fn test_insert_middle_single_negative_pos() {
    let d = FrontEndDiff::Exact {
        ops: vec![ExactOp::InsertAt {
            pos: -1,
            text: "X".into(),
        }],
    };
    let (orig, new) = d.diff_segs("abcd", "abcXd", "").unwrap();
    assert_eq!(texts(&orig), vec!["abcd"]);
    assert_eq!(texts(&new), vec!["abc", "X", "d"]);
    assert_eq!(bgs(&new), vec![DIFF_KEEP_BG, DIFF_ADD_BG, DIFF_KEEP_BG]);
}

/// Position resolution matches `apply_insert` clamping rules.
/// (Resolver itself lives in core — `test_resolve_insert_pos` there covers it.)
#[test]
fn test_resolve_insert_pos() {
    assert_eq!(resolve_insert_pos(0, 4), 0);
    assert_eq!(resolve_insert_pos(2, 4), 2);
    assert_eq!(resolve_insert_pos(99, 4), 4);
    assert_eq!(resolve_insert_pos(-1, 4), 3); // before last char
    assert_eq!(resolve_insert_pos(-4, 4), 0); // very front
    assert_eq!(resolve_insert_pos(-10, 4), 0); // clamped
}

// ── Multiple prepends / appends and op order ──

/// Core *prepends* prefixes, so the last prefix op is front-most.
#[test]
fn test_front_end_multiple_prefixes_order() {
    let cfg = cfg_with(vec![
        RenameItem::Prefix("a".into()),
        RenameItem::Prefix("b".into()),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("orig", "baorig", "").unwrap();
    assert_eq!(texts(&orig), vec!["orig"]);
    assert_eq!(texts(&new), vec!["ba", "orig"]);
    assert_eq!(bgs(&new), vec![DIFF_ADD_BG, DIFF_KEEP_BG]);
}

/// Suffixes append in op order.
#[test]
fn test_front_end_multiple_suffixes_order() {
    let cfg = cfg_with(vec![
        RenameItem::Suffix("a".into()),
        RenameItem::Suffix("b".into()),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (_, new) = d.diff_segs("orig", "origab", "").unwrap();
    assert_eq!(texts(&new), vec!["orig", "ab"]);
}

/// Interleaved prefixes/suffixes keep their relative order on each side.
#[test]
fn test_front_end_interleaved_prepend_append() {
    let cfg = cfg_with(vec![
        RenameItem::Prefix("p1".into()),
        RenameItem::Suffix("s1".into()),
        RenameItem::Prefix("p2".into()),
        RenameItem::Suffix("s2".into()),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (_, new) = d.diff_segs("ab", "p2p1abs1s2", "").unwrap();
    assert_eq!(texts(&new), vec!["p2p1", "ab", "s1s2"]);
    assert_eq!(bgs(&new), vec![DIFF_ADD_BG, DIFF_KEEP_BG, DIFF_ADD_BG]);
}

/// Insert-at-0 combined with a prefix: op order decides which is front-most.
#[test]
fn test_insert_zero_with_prefix_order() {
    // Insert first, then prefix → the prefix is front-most.
    let cfg = cfg_with(vec![
        RenameItem::Insert("Y".into(), 0),
        RenameItem::Prefix("x".into()),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (_, new) = d.diff_segs("ab", "xYab", "").unwrap();
    assert_eq!(texts(&new), vec!["xY", "ab"]);

    // Prefix first, then insert-at-0 → the insert is front-most.
    let cfg = cfg_with(vec![
        RenameItem::Prefix("x".into()),
        RenameItem::Insert("Y".into(), 0),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (_, new) = d.diff_segs("ab", "Yxab", "").unwrap();
    assert_eq!(texts(&new), vec!["Yx", "ab"]);
}

/// End-resolving insert combined with a suffix: order preserved.
#[test]
fn test_insert_end_with_suffix_order() {
    let cfg = cfg_with(vec![
        RenameItem::Suffix("s".into()),
        RenameItem::Insert("X".into(), 99),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (_, new) = d.diff_segs("ab", "absX", "").unwrap();
    assert_eq!(texts(&new), vec!["ab", "sX"]);

    let cfg = cfg_with(vec![
        RenameItem::Insert("X".into(), 99),
        RenameItem::Suffix("s".into()),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (_, new) = d.diff_segs("ab", "abXs", "").unwrap();
    assert_eq!(texts(&new), vec!["ab", "Xs"]);
}

/// End-resolving insert after a removal: exact, position resolved against
/// the post-removal string length.
#[test]
fn test_insert_end_after_removal() {
    let cfg = cfg_with(vec![
        RenameItem::RemoveLast(1),
        RenameItem::Insert("X".into(), 99),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("abcd", "abcX", "").unwrap();
    assert_eq!(texts(&orig), vec!["abc", "d"]);
    assert_eq!(bgs(&orig), vec![DIFF_KEEP_BG, DIFF_REMOVE_BG]);
    assert_eq!(texts(&new), vec!["abc", "X"]);
}

/// Insert-at-0 after a front removal.
#[test]
fn test_insert_zero_after_removal() {
    let cfg = cfg_with(vec![
        RenameItem::RemoveFirst(2),
        RenameItem::Insert("X".into(), 0),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("wxyz", "Xyz", "").unwrap();
    assert_eq!(texts(&orig), vec!["wx", "yz"]);
    assert_eq!(bgs(&orig), vec![DIFF_REMOVE_BG, DIFF_KEEP_BG]);
    assert_eq!(texts(&new), vec!["X", "yz"]);
}

/// A removal eats multiple prefix pieces in order (front-most first).
#[test]
fn test_removal_eats_multiple_prefixes() {
    let cfg = cfg_with(vec![
        RenameItem::Prefix("a".into()),
        RenameItem::Prefix("b".into()),
        RenameItem::RemoveFirst(3),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("xyz", "yz", "").unwrap();
    assert_eq!(texts(&orig), vec!["x", "yz"]);
    assert_eq!(bgs(&orig), vec![DIFF_REMOVE_BG, DIFF_KEEP_BG]);
    assert_eq!(texts(&new), vec!["yz"]);
    assert_eq!(bgs(&new), vec![DIFF_KEEP_BG]);
}

// ── Mixed / fallback cases ──

/// Middle insert combined with other ops falls back to LCS.
#[test]
fn test_insert_middle_mixed_ops_fall_back() {
    let cfg = cfg_with(vec![
        RenameItem::Insert("X".into(), 2),
        RenameItem::Prefix("p".into()),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    assert!(d.diff_segs("abcd", "pabXcd", "").is_none());

    let cfg = cfg_with(vec![
        RenameItem::RemoveFirst(1),
        RenameItem::Insert("X".into(), 1),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    assert!(d.diff_segs("abcd", "bXcd", "").is_none());

    // Two inserts that both land in the middle.
    let cfg = cfg_with(vec![
        RenameItem::Insert("A".into(), 1),
        RenameItem::Insert("B".into(), 3),
    ]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    assert!(d.diff_segs("abcd", "aAbBcd", "").is_none());
}

/// A lone insert whose new name doesn't match the reconstruction is rejected.
#[test]
fn test_insert_single_verification_rejects_mismatch() {
    let cfg = cfg_with(vec![RenameItem::Insert("X".into(), 2)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    assert!(d.diff_segs("abcd", "zz", "").is_none());
}

/// Positional insert combined with a middle-changing op → whole config LCS.
#[test]
fn test_insert_with_replace_falls_back() {
    let cfg = cfg_with(vec![
        RenameItem::Insert("X".into(), 0),
        RenameItem::Replace("a".into(), "b".into(), true, false),
    ]);
    assert!(FrontEndDiff::from_config(&cfg).is_none());
}

/// Same-side combo: front insert + date prefix both become part of X.
#[test]
fn test_insert_with_generated_prefix_same_side() {
    let mut cfg = cfg_with(vec![
        RenameItem::Insert("X".into(), 0),
        RenameItem::AddDate("ymd".into()),
    ]);
    cfg.auto_date.date_position = DatePosition::Prefix;
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("ab", "2026-08-12Xab", "").unwrap();
    assert_eq!(texts(&orig), vec!["ab"]);
    assert_eq!(texts(&new), vec!["2026-08-12X", "ab"]);
    assert_eq!(bgs(&new), vec![DIFF_ADD_BG, DIFF_KEEP_BG]);
}

/// Known insert on the *opposite* side of a generated insert: the retained
/// name loses its clean prefix/suffix position in `new`, so the row falls
/// back to the general LCS diff (accuracy preserved, just not structural).
#[test]
fn test_insert_with_generated_suffix_opposite_side_falls_back() {
    let mut cfg = cfg_with(vec![
        RenameItem::Insert("X".into(), 0),
        RenameItem::AddDate("ymd".into()),
    ]);
    cfg.auto_date.date_position = DatePosition::Suffix;
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    assert!(d.diff_segs("ab", "Xab2026-08-12", "").is_none());
}

// ── Stem/extension split ──

/// Remove-last on an extension-bearing name: the removal applies to the
/// stem (before the extension), stays one contiguous segment, and keeps the
/// stem's true tail (`B1`) rather than the LCS's fragmented alignment.
#[test]
fn test_remove_last_keeps_extension_contiguous() {
    let cfg = cfg_with(vec![RenameItem::RemoveLast(19)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let orig =
        "Sample Series\u{A789} Chronicles (2021) - S01E06 - Segment B1 (1080p WEB-DL x265).mkv";
    let new = "Sample Series\u{A789} Chronicles (2021) - S01E06 - Segment B1 .mkv";
    let (orig_segs, new_segs) = d.diff_segs(orig, new, ".mkv").unwrap();
    assert_eq!(
        texts(&orig_segs),
        vec![
            "Sample Series\u{A789} Chronicles (2021) - S01E06 - Segment B1 ",
            "(1080p WEB-DL x265)",
            ".mkv",
        ]
    );
    assert_eq!(
        bgs(&orig_segs),
        vec![DIFF_KEEP_BG, DIFF_REMOVE_BG, DIFF_KEEP_BG]
    );
    assert_eq!(
        texts(&new_segs),
        vec![
            "Sample Series\u{A789} Chronicles (2021) - S01E06 - Segment B1 ",
            ".mkv"
        ]
    );
    assert_eq!(bgs(&new_segs), vec![DIFF_KEEP_BG, DIFF_KEEP_BG]);

    // Passing the wrong extension must fail verification (→ LCS), never
    // emit a wrong structural diff.
    assert!(d.diff_segs(orig, new, "").is_none());
}

/// Suffix inserts before the preserved extension.
#[test]
fn test_suffix_keeps_extension() {
    let cfg = cfg_with(vec![RenameItem::Suffix("_x".into())]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("a.txt", "a_x.txt", ".txt").unwrap();
    assert_eq!(texts(&orig), vec!["a", ".txt"]);
    assert_eq!(bgs(&orig), vec![DIFF_KEEP_BG, DIFF_KEEP_BG]);
    assert_eq!(texts(&new), vec!["a", "_x", ".txt"]);
    assert_eq!(bgs(&new), vec![DIFF_KEEP_BG, DIFF_ADD_BG, DIFF_KEEP_BG]);
}

/// A lone middle insert splits the stem; the extension stays attached.
#[test]
fn test_insert_middle_keeps_extension() {
    let cfg = cfg_with(vec![RenameItem::Insert("X".into(), 2)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, new) = d.diff_segs("ab.txt", "abX.txt", ".txt").unwrap();
    assert_eq!(texts(&orig), vec!["ab.txt"]);
    assert_eq!(texts(&new), vec!["ab", "X", ".txt"]);
    assert_eq!(bgs(&new), vec![DIFF_KEEP_BG, DIFF_ADD_BG, DIFF_KEEP_BG]);
}

/// Directories keep the whole name as the stem (no extension split), so a
/// dot in a directory name is part of the stem.
#[test]
fn test_remove_last_on_directory_keeps_dots() {
    let cfg = cfg_with(vec![RenameItem::RemoveLast(1)]);
    let d = FrontEndDiff::from_config(&cfg).unwrap();
    let (orig, _) = d.diff_segs("folder.v2", "folder.v", "").unwrap();
    assert_eq!(texts(&orig), vec!["folder.v", "2"]);
    assert_eq!(bgs(&orig), vec![DIFF_KEEP_BG, DIFF_REMOVE_BG]);
}

// ── Remove From/To linked drag ──

/// Increasing From while To equals the old From drags To along.
#[test]
fn test_linked_from_to_increase_follows() {
    let mut to = Some(3);
    linked_from_to(Some(3), Some(4), &mut to);
    assert_eq!(to, Some(4));
}

/// Decreasing From never changes To, even when they were equal.
#[test]
fn test_linked_from_to_decrease_does_not_follow() {
    let mut to = Some(3);
    linked_from_to(Some(3), Some(2), &mut to);
    assert_eq!(to, Some(3));
}

/// To only follows when it equals the old From; otherwise it is untouched.
#[test]
fn test_linked_from_to_only_when_equal() {
    let mut to = Some(5);
    linked_from_to(Some(3), Some(4), &mut to);
    assert_eq!(to, Some(5));
}

/// A cleared field displays as 0, so the linkage fires when both fields
/// show 0 (whether cleared or stored as Some(0)) and From is increased.
#[test]
fn test_linked_from_to_zero_values_follow() {
    let mut to = None; // cleared, displays as 0
    linked_from_to(None, Some(1), &mut to);
    assert_eq!(to, Some(1));
    let mut to = Some(0); // stored To=0 ("through the end")
    linked_from_to(None, Some(1), &mut to);
    assert_eq!(to, Some(1));
}

/// Negative positions never trigger the linkage, even when From and To
/// are the same — it only fires for values >= 0.
#[test]
fn test_linked_from_to_negative_does_not_follow() {
    let mut to = Some(-3);
    linked_from_to(Some(-3), Some(-2), &mut to);
    assert_eq!(to, Some(-3));
}

/// Clearing From to 0 (None) leaves To alone.
#[test]
fn test_linked_from_to_clear_does_not_follow() {
    let mut to = Some(3);
    linked_from_to(Some(3), None, &mut to);
    assert_eq!(to, Some(3));
}

// ── Plain isize variant (Name Segment) ──

/// The plain-value rule (used by Name Segment) is the same as the
/// Option-wrapped one with 0 standing in for unset.
#[test]
fn test_linked_from_to_values() {
    // Increase with To == old From → To follows.
    let mut to = 3;
    linked_from_to_values(3, 4, &mut to);
    assert_eq!(to, 4);
    // Decrease never changes To.
    let mut to = 3;
    linked_from_to_values(3, 2, &mut to);
    assert_eq!(to, 3);
    // Only fires when To equals the old From.
    let mut to = 5;
    linked_from_to_values(3, 4, &mut to);
    assert_eq!(to, 5);
    // 0 → 1 with To at 0 fires (both fields at the default).
    let mut to = 0;
    linked_from_to_values(0, 1, &mut to);
    assert_eq!(to, 1);
    // Negative positions never fire.
    let mut to = -3;
    linked_from_to_values(-3, -2, &mut to);
    assert_eq!(to, -3);
}

/// The Option wrapper delegates to the plain rule: an unset To becomes
/// Some(0) (behaviorally identical to unset everywhere).
#[test]
fn test_linked_from_to_delegates_to_values() {
    let mut to = None;
    linked_from_to(Some(3), Some(4), &mut to); // no fire, To unrelated
    assert_eq!(to, Some(0));
    let mut to = Some(0);
    linked_from_to(None, Some(1), &mut to); // fire at 0 → 1
    assert_eq!(to, Some(1));
}

// ── Integer drag reachability ──

/// egui's `DragValue` snaps a dragged value to the "roundest" number within
/// `speed × aim_radius` of the precise drag position (`emath::smart_aim`), then
/// rounds to the displayed decimals. [`INT_DRAG_SPEED`] must keep that aim
/// window narrower than one unit, otherwise some integers become impossible to
/// drag to — e.g. at `speed(1)` the value `9` snaps to `10` and `24` to `25`.
///
/// This mirrors egui's drag math at the worst case of one point of drag per
/// pixel (`aim_radius == 1`); a regression to a coarser speed fails here.
#[test]
fn test_int_drag_speed_reaches_every_integer() {
    let aim_radius = 1.0; // `InputState::aim_radius` at one point per pixel
    let aim = aim_radius * INT_DRAG_SPEED;

    for target in -50..=50 {
        let target = target as f64;
        let reached = (0..=400).any(|i| {
            let precise = target - 1.0 + i as f64 * INT_DRAG_SPEED;
            let aimed = egui::emath::smart_aim::best_in_range_f64(precise - aim, precise + aim);
            egui::emath::round_to_decimals(aimed, 0) == target
        });
        assert!(reached, "value {target} is unreachable while dragging");
    }
}
