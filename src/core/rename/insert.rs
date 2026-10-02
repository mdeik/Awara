use super::ops::resolve_from_index;
use super::*;

/// Optional formatting parameters for number generation.
#[derive(Default)]
pub(crate) struct NumberingConfig<'a> {
    pub sep: Option<&'a str>,
    pub pos: Option<isize>,
    pub num_type: Option<&'a str>,
    pub num_case: Option<&'a str>,
}

/// Resolve a character-gap position exactly like `apply_insert`/`apply_dirname`:
/// negative values count gaps from the end (`-1` = before the last character),
/// positive values clamp to the length. SSoT for positional inserts.
pub fn resolve_insert_pos(pos: isize, len: usize) -> usize {
    if pos < 0 {
        len.saturating_sub((-pos) as usize)
    } else {
        std::cmp::min(pos as usize, len)
    }
}

pub(crate) fn apply_insert(s: &str, text: &str, pos: isize) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    let pos = resolve_insert_pos(pos, chars.len());
    for c in text.chars().rev() {
        chars.insert(pos, c);
    }
    chars.into_iter().collect()
}

pub(crate) fn apply_word_space(s: &str) -> String {
    Regex::new(r"([a-z])([A-Z])")
        .unwrap()
        .replace_all(s, "${1} ${2}")
        .to_string()
}

pub(crate) fn apply_dirname(s: &str, pname: &str, sep: Option<&str>, pos: Option<isize>) -> String {
    let sep = sep.unwrap_or("");
    let len = s.chars().count();
    let pos = resolve_insert_pos(pos.unwrap_or(0), len);
    let mut chars: Vec<char> = s.chars().collect();
    let insert_str = if pos == 0 {
        format!("{}{}", pname, sep)
    } else if pos >= chars.len() {
        format!("{}{}", sep, pname)
    } else {
        format!("{}{}", pname, sep)
    };
    for c in insert_str.chars().rev() {
        chars.insert(pos, c);
    }
    chars.into_iter().collect()
}

/// Shared helper: resolve `from` (1-based, negative = from end) and `len` to
/// 0-based start/end indices. Returns `None` when the range is empty.
/// The `from` index math is shared with Remove From/To via
/// [`resolve_from_index`]; `from = 0` is not a 1-based position and is
/// treated as invalid.
fn resolve_move_copy_range(len: usize, from: isize, len_val: isize) -> Option<(usize, usize)> {
    if from == 0 {
        return None;
    }
    let start = resolve_from_index(len, from);
    let actual_len = if len_val < 0 {
        (-len_val).max(1)
    } else {
        len_val
    };
    let end = std::cmp::min(start + (actual_len as usize), len);
    if start < end {
        Some((start, end))
    } else {
        None
    }
}

/// Shared helper: resolve `to` to a 0-based insert index (a gap between characters).
/// Positive values are 1-based character positions: `1` = start, `N` = before char N.
/// Negative values count gaps from the end: `-1` = after the last character (very end),
/// `-2` = before the last character, and so on.
fn resolve_move_copy_dest(len: usize, to: isize) -> usize {
    if to < 0 {
        let from_end = (-to) as usize;
        len.saturating_add(1).saturating_sub(from_end)
    } else if to > 0 {
        std::cmp::min((to - 1) as usize, len)
    } else {
        0
    }
}

/// Insert `part` at the 0-based gap `insert_pos`, with an optional separator
/// between the part and the text that follows it at the destination. When the
/// part lands at the very end the separator goes before the part instead.
pub(crate) fn insert_part(
    chars: &mut Vec<char>,
    insert_pos: usize,
    part: &[char],
    sep: Option<&str>,
) {
    for c in part.iter().rev() {
        chars.insert(insert_pos, *c);
    }
    let Some(sep) = sep.filter(|s| !s.is_empty()) else {
        return;
    };
    let sep_chars: Vec<char> = sep.chars().collect();
    let sep_pos = if insert_pos + part.len() >= chars.len() {
        // Part is at the very end: separator sits before the part.
        insert_pos
    } else {
        insert_pos + part.len()
    };
    for c in sep_chars.iter().rev() {
        chars.insert(sep_pos, *c);
    }
}

pub(crate) fn apply_move_part(
    s: &str,
    from: isize,
    len_val: isize,
    to: isize,
    sep: Option<&str>,
) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    let orig_len = chars.len();
    if let Some((start_idx, end_idx)) = resolve_move_copy_range(orig_len, from, len_val) {
        let part: Vec<char> = chars.drain(start_idx..end_idx).collect();
        // Resolve destination against post-drain length for move (string is shorter now)
        let mut insert_pos = resolve_move_copy_dest(chars.len(), to);
        insert_pos = std::cmp::min(insert_pos, chars.len());
        insert_part(&mut chars, insert_pos, &part, sep);
        return chars.into_iter().collect();
    }
    s.to_string()
}

pub(crate) fn apply_copy_part(
    s: &str,
    from: isize,
    len_val: isize,
    to: isize,
    sep: Option<&str>,
) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    if let Some((start_idx, end_idx)) = resolve_move_copy_range(chars.len(), from, len_val) {
        let part: Vec<char> = chars[start_idx..end_idx].to_vec();
        let insert_pos = resolve_move_copy_dest(chars.len(), to);
        insert_part(&mut chars, insert_pos, &part, sep);
        return chars.into_iter().collect();
    }
    s.to_string()
}
