use super::*;

// ── Helper functions ──

pub(crate) fn apply_replace_op(s: &str, find: &str, with: &str, case: bool, first: bool) -> String {
    if case {
        if first {
            s.replacen(find, with, 1)
        } else {
            s.replace(find, with)
        }
    } else {
        let pattern = regex::escape(find);
        let re_str = format!("(?i){}", pattern);
        if let Ok(re) = Regex::new(&re_str) {
            if first {
                re.replace(s, with).to_string()
            } else {
                re.replace_all(s, with).to_string()
            }
        } else {
            s.to_string()
        }
    }
}

pub(crate) fn apply_remove_first(s: &str, n: isize) -> String {
    let len = s.chars().count() as isize;
    if n < 0 {
        // Negative: remove from end instead
        let remove = (-n).min(len).max(0) as usize;
        let take = (len as usize).saturating_sub(remove);
        s.chars().take(take).collect()
    } else if len >= n {
        s.chars().skip(n as usize).collect()
    } else {
        String::new()
    }
}

pub(crate) fn apply_remove_last(s: &str, n: isize) -> String {
    let len = s.chars().count() as isize;
    if n < 0 {
        // Negative: remove from start instead
        let skip = (-n).min(len).max(0) as usize;
        s.chars().skip(skip).collect()
    } else if len >= n {
        s.chars().take((len - n) as usize).collect()
    } else {
        String::new()
    }
}

/// Resolve a 1-based `from` position to a clamped 0-based start index — the
/// single source of truth shared by Remove From/To, Name Segment, and
/// Move/Copy Parts. Positive values are 1-based positions (1 = first
/// character); negative values count from the end (-1 = last character).
pub(super) fn resolve_from_index(len: usize, from: isize) -> usize {
    let len_i = len as isize;
    let from_0 = if from < 0 {
        (len_i + from).max(0)
    } else {
        from - 1
    };
    from_0.max(0).min(len_i) as usize
}

/// Resolve a 1-based `from`/`to` position pair to 0-based start and
/// exclusive-end indices — the single source of truth for all from/to
/// position math (Remove From/To and Name Segment):
///  - positive values are 1-based positions (1 = first character)
///  - negative values count from the end (-1 = last character)
///  - `to = 0` removes/extracts through the end of the name
///
/// Returns `None` when the resolved range is empty.
pub fn resolve_from_to_range(len: usize, from: isize, to: isize) -> Option<(usize, usize)> {
    let len_i = len as isize;
    let from_0 = resolve_from_index(len, from);
    // Resolve `to` to a 0-based exclusive end index:
    //  - positive: 1-based position of the last character to remove
    //  - negative: counts from the end, -1 includes the last character
    //  - 0: through the end of the name
    let to_0 = if to < 0 {
        (len_i + to + 1).max(0)
    } else if to == 0 {
        len_i
    } else {
        to
    };
    let to_0 = to_0.max(from_0 as isize).min(len_i) as usize;
    if from_0 < to_0 {
        Some((from_0, to_0))
    } else {
        None
    }
}

pub(crate) fn apply_remove_from_to(s: &str, from: isize, to: isize) -> String {
    let chars: Vec<char> = s.chars().collect();
    match resolve_from_to_range(chars.len(), from, to) {
        Some((from_0, to_0)) => {
            let part1: String = chars.iter().take(from_0).collect();
            let part2: String = chars.iter().skip(to_0).collect();
            format!("{}{}", part1, part2)
        }
        None => s.to_string(),
    }
}

pub(crate) fn apply_remove_digits(s: &str) -> String {
    s.chars().filter(|c| !c.is_numeric()).collect()
}

pub(crate) fn apply_crop(s: &str, mode: CropMode, text: &str) -> String {
    match mode {
        CropMode::Before => {
            if let Some(pos) = s.find(text) {
                s[pos + text.len()..].to_string()
            } else {
                s.to_string()
            }
        }
        CropMode::After => {
            if let Some(pos) = s.find(text) {
                s[..pos].to_string()
            } else {
                s.to_string()
            }
        }
        CropMode::Special => {
            if s.contains(text) {
                text.to_string()
            } else {
                s.to_string()
            }
        }
    }
}

pub(crate) fn apply_remove_chars(s: &str, chars: &str) -> String {
    let set: HashSet<char> = chars.chars().collect();
    s.chars().filter(|c| !set.contains(c)).collect()
}

pub(crate) fn apply_remove_words(s: &str, words: &str, case: bool) -> String {
    let mut res = s.to_string();
    for w in words.split_whitespace() {
        if let Ok(re) = build_word_regex(w, case) {
            res = re.replace_all(&res, "").to_string();
        }
    }
    res
}

pub(crate) fn apply_remove_symbols(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect()
}

pub(crate) fn apply_double_spaces(s: &str) -> String {
    Regex::new(r"\s+").unwrap().replace_all(s, " ").to_string()
}

pub(crate) fn apply_remove_high(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii()).collect()
}

pub(crate) fn apply_remove_accents(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfd()
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .nfc()
        .collect()
}

pub(crate) fn apply_remove_lead_dots(s: &str, mode: LeadDotsMode) -> String {
    match mode {
        LeadDotsMode::Single => s.strip_prefix('.').unwrap_or(s).to_string(),
        LeadDotsMode::Double => s.strip_prefix("..").unwrap_or(s).to_string(),
        LeadDotsMode::Both => {
            let trimmed = s.trim_start_matches('.');
            if trimmed.len() < s.len() {
                trimmed.to_string()
            } else {
                s.to_string()
            }
        }
    }
}
