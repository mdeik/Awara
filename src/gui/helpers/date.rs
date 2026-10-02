// ── Date format helpers (compose / decompose auto_date format strings) ──

/// The Fmt combo: `(key, label, tokens)` in combo order.
///
/// `tokens` are the format string's components, joined by the separator the Seg
/// field holds. This is the single source of truth for the format list: the combo
/// labels, [`build_date_fmt`], and [`split_date_fmt`] all read it instead of
/// restating the tokens, so the three cannot drift apart.
pub(crate) const DATE_FORMATS: &[(&str, &str, &[&str])] = &[
    ("dmy", "DMY", &["DD", "MM", "YYYY"]),
    ("dmy_hh", "DMY HH", &["DD", "MM", "YYYY", "hh"]),
    (
        "dmy_hms",
        "DMY HMS",
        &["DD", "MM", "YYYY", "hh", "mm", "ss"],
    ),
    ("mdy", "MDY", &["MM", "DD", "YYYY"]),
    ("mdy_hh", "MDY HH", &["MM", "DD", "YYYY", "hh"]),
    (
        "mdy_hms",
        "MDY HMS",
        &["MM", "DD", "YYYY", "hh", "mm", "ss"],
    ),
    ("ymd", "YMD", &["YYYY", "MM", "DD"]),
    ("ymd_hh", "YMD HH", &["YYYY", "MM", "DD", "hh"]),
    (
        "ymd_hms",
        "YMD HMS",
        &["YYYY", "MM", "DD", "hh", "mm", "ss"],
    ),
];

/// The Fmt combo's last entry: the user's own format string, which no token list
/// describes. It sits outside [`DATE_FORMATS`] so that every entry in the table is
/// a format [`build_date_fmt`] can compose — which is exactly what
/// [`split_date_fmt`] checks before reusing a key.
pub(crate) const CUSTOM_FMT: (&str, &str) = ("custom", "Custom");

/// Number of entries in the Fmt combo: the token formats, then Custom.
pub(crate) const FMT_COMBO_LEN: usize = DATE_FORMATS.len() + 1;

/// The Fmt combo entry at index `i` as `(key, label)`. Anything past the table is
/// the Custom entry, so the combo can walk `0..FMT_COMBO_LEN` with no special case.
pub(crate) fn fmt_combo_entry(i: usize) -> (&'static str, &'static str) {
    match DATE_FORMATS.get(i) {
        Some((key, label, _)) => (*key, *label),
        _ => CUSTOM_FMT,
    }
}

/// The Fmt combo index of a stored key: the Custom entry, the token format's
/// position, or the first entry for an absent (or unlisted) key — the fallback the
/// combo shows when it cannot name the stored key.
pub(crate) fn fmt_index(key: Option<&str>) -> usize {
    let Some(key) = key else { return 0 };
    if key == CUSTOM_FMT.0 {
        return FMT_COMBO_LEN - 1;
    }
    DATE_FORMATS
        .iter()
        .position(|(k, _, _)| *k == key)
        .unwrap_or(0)
}

/// Build a date format string from a format key and the Seg separator: the key's
/// tokens joined by `sep`. The Custom key uses `custom` verbatim.
pub(crate) fn build_date_fmt(key: &str, sep: &str, custom: &str) -> String {
    if key == CUSTOM_FMT.0 {
        return custom.to_string();
    }
    DATE_FORMATS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, tokens)| tokens.join(sep))
        .unwrap_or_default()
}

/// Split a stored format string into the Fmt key and the single separator the Seg
/// field holds, or `None` when no combo entry reproduces it.
///
/// The round trip through [`build_date_fmt`] is the point: a key is reused only
/// when composing it back yields the stored string, so a format the widgets cannot
/// express (a short-year or mixed-separator string) is edited as Custom instead of
/// being quietly rewritten.
pub(crate) fn split_date_fmt(fmt: &str) -> Option<(&'static str, String)> {
    DATE_FORMATS.iter().find_map(|(key, _, tokens)| {
        let sep = tokens_sep(fmt, tokens)?;
        (build_date_fmt(key, &sep, "") == fmt).then_some((*key, sep))
    })
}

/// The separator joining `tokens` in `fmt`, if `fmt` is exactly those tokens in
/// order and every gap between them is the same string — the Seg field holds one.
fn tokens_sep(fmt: &str, tokens: &[&str]) -> Option<String> {
    let mut rest = fmt;
    let mut sep: Option<&str> = None;
    for (i, token) in tokens.iter().enumerate() {
        if i > 0 {
            let gap_end = rest.find(token)?;
            let gap = &rest[..gap_end];
            match sep {
                Some(prev) if prev != gap => return None,
                None => sep = Some(gap),
                _ => {}
            }
            rest = &rest[gap_end..];
        }
        rest = rest.strip_prefix(token)?;
    }
    rest.is_empty().then(|| sep.unwrap_or_default().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The combo's entries and its key→index lookup must agree, or the combo would
    /// show one format while writing another.
    #[test]
    fn combo_entries_and_indices_agree() {
        for i in 0..FMT_COMBO_LEN {
            let (key, label) = fmt_combo_entry(i);
            assert_eq!(fmt_index(Some(key)), i, "{key}: index {i}");
            assert!(!label.is_empty(), "{key}: needs a label");
        }
        // An absent or unlisted key falls back to the first entry, and Custom is
        // always the last one.
        assert_eq!(fmt_combo_entry(fmt_index(None)).0, "dmy");
        assert_eq!(fmt_combo_entry(fmt_index(Some("nope"))).0, "dmy");
        assert_eq!(fmt_combo_entry(FMT_COMBO_LEN - 1), CUSTOM_FMT);
    }

    /// Every table entry composes to a string that splits back to the same key and
    /// separator — the round trip the Auto Date widgets rely on.
    #[test]
    fn every_format_round_trips_through_its_own_key() {
        for (key, _, tokens) in DATE_FORMATS {
            for sep in ["", "-", "_", "."] {
                let fmt = build_date_fmt(key, sep, "");
                assert_eq!(fmt, tokens.join(sep), "{key}: built from its own tokens");
                assert_eq!(
                    split_date_fmt(&fmt),
                    Some((*key, sep.to_string())),
                    "{key} with separator {sep:?}"
                );
            }
        }
        // A format outside the table is not reused under a wrong key: a short year
        // and a mixed separator are what the one Seg field cannot express.
        for fmt in ["date_DDMMYYYY", "DDMMYY", "YYYY-MM-DD hh:mm:ss"] {
            assert_eq!(split_date_fmt(fmt), None, "{fmt} must be Custom");
        }
        // Custom is not a token format; it composes from the caller's string.
        assert_eq!(build_date_fmt(CUSTOM_FMT.0, "-", "DD.MM"), "DD.MM");
    }
}
