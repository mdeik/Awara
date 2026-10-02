use super::*;

/// Compute diff segments for a rename. The general LCS diff is the fallback;
/// common prefix/suffix, pure middle insert/delete, interior containment, and
/// fully disjoint middles are all caught structurally before it runs.
///
/// Kept characters get no highlight (`DIFF_KEEP_BG`), removed characters get
/// `DIFF_REMOVE_BG`, added characters get `DIFF_ADD_BG`. Text colour is left to
/// the theme (default) so the highlight composites correctly over panel,
/// zebra, hover, and selection backgrounds in both themes.
pub(crate) fn diff_colored(orig: &str, new: &str) -> (Vec<DiffSeg>, Vec<DiffSeg>) {
    if orig == new {
        let seg = if orig.is_empty() {
            vec![]
        } else {
            vec![DiffSeg {
                text: orig.to_string(),
                fg: None,
                bg: DIFF_KEEP_BG,
            }]
        };
        return (seg.clone(), seg);
    }
    if orig.is_empty() {
        return (
            vec![],
            vec![DiffSeg {
                text: new.to_string(),
                fg: None,
                bg: DIFF_ADD_BG,
            }],
        );
    }
    if new.is_empty() {
        return (
            vec![DiffSeg {
                text: orig.to_string(),
                fg: None,
                bg: DIFF_REMOVE_BG,
            }],
            vec![],
        );
    }

    let orig_chars: Vec<char> = orig.chars().collect();
    let new_chars: Vec<char> = new.chars().collect();
    let m = orig_chars.len();
    let n = new_chars.len();

    // 1. O(N) Common prefix
    let mut prefix_len = 0;
    while prefix_len < m && prefix_len < n && orig_chars[prefix_len] == new_chars[prefix_len] {
        prefix_len += 1;
    }

    // 2. O(N) Common suffix (cannot overlap with prefix)
    let mut suffix_len = 0;
    while suffix_len < (m - prefix_len)
        && suffix_len < (n - prefix_len)
        && orig_chars[m - 1 - suffix_len] == new_chars[n - 1 - suffix_len]
    {
        suffix_len += 1;
    }

    let prefix_orig: String = orig_chars[..prefix_len].iter().collect();
    let suffix_orig: String = orig_chars[m - suffix_len..].iter().collect();

    let orig_mid = &orig_chars[prefix_len..m - suffix_len];
    let new_mid = &new_chars[prefix_len..n - suffix_len];

    let mut orig_segs = Vec::new();
    let mut new_segs = Vec::new();

    // Push prefix if non-empty
    if !prefix_orig.is_empty() {
        orig_segs.push(DiffSeg {
            text: prefix_orig.clone(),
            fg: None,
            bg: DIFF_KEEP_BG,
        });
        new_segs.push(DiffSeg {
            text: prefix_orig,
            fg: None,
            bg: DIFF_KEEP_BG,
        });
    }

    // Middle diff
    if orig_mid.is_empty() && !new_mid.is_empty() {
        // Pure insertion
        let inserted: String = new_mid.iter().collect();
        new_segs.push(DiffSeg {
            text: inserted,
            fg: None,
            bg: DIFF_ADD_BG,
        });
    } else if !orig_mid.is_empty() && new_mid.is_empty() {
        // Pure deletion
        let deleted: String = orig_mid.iter().collect();
        orig_segs.push(DiffSeg {
            text: deleted,
            fg: None,
            bg: DIFF_REMOVE_BG,
        });
    } else if let Some((m_orig_segs, m_new_segs)) = diff_middle_structural(orig_mid, new_mid) {
        // Structural fast-path: single interior insert/delete, or a fully
        // disjoint replacement — no LCS needed.
        orig_segs.extend(m_orig_segs);
        new_segs.extend(m_new_segs);
    } else {
        // Genuine mixed edit — general LCS diff on the reduced middle slices.
        let (m_orig_segs, m_new_segs) = diff_middle_lcs(orig_mid, new_mid);
        orig_segs.extend(m_orig_segs);
        new_segs.extend(m_new_segs);
    }

    // Push suffix if non-empty
    if !suffix_orig.is_empty() {
        orig_segs.push(DiffSeg {
            text: suffix_orig.clone(),
            fg: None,
            bg: DIFF_KEEP_BG,
        });
        new_segs.push(DiffSeg {
            text: suffix_orig,
            fg: None,
            bg: DIFF_KEEP_BG,
        });
    }

    (orig_segs, new_segs)
}

/// Structural fast-paths for the middle slices left after prefix/suffix
/// trimming. Returns `Some` segments when the edit is a single interior
/// insert, a single interior delete, or a full replacement; `None` when the
/// middle is a genuine mixed edit that needs the LCS diff.
///
/// The containment checks align the kept block to the *first* occurrence,
/// which is deterministic — the LCS backtracking can instead pick a different
/// alignment on repeated patterns (e.g. deleting a block from `abcabc`).
fn diff_middle_structural(
    orig_mid: &[char],
    new_mid: &[char],
) -> Option<(Vec<DiffSeg>, Vec<DiffSeg>)> {
    // Pure insertion: every original char survives, `new_mid` wraps it. After
    // prefix/suffix trimming the match is strictly interior, so both wrapping
    // pieces are non-empty (the kept block is flanked by inserts).
    if let Some(k) = find_slice(new_mid, orig_mid) {
        let before: String = new_mid[..k].iter().collect();
        let kept: String = orig_mid.iter().collect();
        let after: String = new_mid[k + orig_mid.len()..].iter().collect();
        let mut orig_segs = Vec::new();
        push_seg(&mut orig_segs, &kept, DIFF_KEEP_BG);
        let mut new_segs = Vec::new();
        push_seg(&mut new_segs, &before, DIFF_ADD_BG);
        push_seg(&mut new_segs, &kept, DIFF_KEEP_BG);
        push_seg(&mut new_segs, &after, DIFF_ADD_BG);
        return Some((orig_segs, new_segs));
    }
    // Pure deletion: symmetric — every new char survives, `orig_mid` wraps it.
    if let Some(k) = find_slice(orig_mid, new_mid) {
        let before: String = orig_mid[..k].iter().collect();
        let kept: String = new_mid.iter().collect();
        let after: String = orig_mid[k + new_mid.len()..].iter().collect();
        let mut orig_segs = Vec::new();
        push_seg(&mut orig_segs, &before, DIFF_REMOVE_BG);
        push_seg(&mut orig_segs, &kept, DIFF_KEEP_BG);
        push_seg(&mut orig_segs, &after, DIFF_REMOVE_BG);
        let mut new_segs = Vec::new();
        push_seg(&mut new_segs, &kept, DIFF_KEEP_BG);
        return Some((orig_segs, new_segs));
    }
    // No shared character: the whole middle is replaced. LCS would emit the
    // same all-delete/all-insert alignment, so skip the DP table entirely.
    if !shares_char(orig_mid, new_mid) {
        let mut orig_segs = Vec::new();
        let deleted: String = orig_mid.iter().collect();
        push_seg(&mut orig_segs, &deleted, DIFF_REMOVE_BG);
        let mut new_segs = Vec::new();
        let inserted: String = new_mid.iter().collect();
        push_seg(&mut new_segs, &inserted, DIFF_ADD_BG);
        return Some((orig_segs, new_segs));
    }
    None
}

/// First index where `needle` occurs as a contiguous slice of `haystack`.
fn find_slice(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    let last = haystack.len() - needle.len();
    (0..=last).find(|&k| &haystack[k..k + needle.len()] == needle)
}

/// True when `a` and `b` share at least one character.
fn shares_char(a: &[char], b: &[char]) -> bool {
    a.iter().any(|c| b.contains(c))
}

fn diff_middle_lcs(orig_chars: &[char], new_chars: &[char]) -> (Vec<DiffSeg>, Vec<DiffSeg>) {
    let m = orig_chars.len();
    let n = new_chars.len();

    // Flat DP table indexed as dp[i * (n+1) + j] for improved cache locality
    let stride = n + 1;
    let mut dp = vec![0usize; (m + 1) * stride];
    for i in 1..=m {
        for j in 1..=n {
            let idx = i * stride + j;
            if orig_chars[i - 1] == new_chars[j - 1] {
                dp[idx] = dp[(i - 1) * stride + (j - 1)] + 1;
            } else {
                dp[idx] = std::cmp::max(dp[(i - 1) * stride + j], dp[i * stride + (j - 1)]);
            }
        }
    }

    enum Op {
        Keep(char),
        Delete(char),
        Insert(char),
    }
    let mut ops = Vec::new();
    let (mut i, mut j) = (m, n);
    while i > 0 && j > 0 {
        if orig_chars[i - 1] == new_chars[j - 1] {
            ops.push(Op::Keep(orig_chars[i - 1]));
            i -= 1;
            j -= 1;
        } else if dp[(i - 1) * stride + j] >= dp[i * stride + (j - 1)] {
            ops.push(Op::Delete(orig_chars[i - 1]));
            i -= 1;
        } else {
            ops.push(Op::Insert(new_chars[j - 1]));
            j -= 1;
        }
    }
    while i > 0 {
        ops.push(Op::Delete(orig_chars[i - 1]));
        i -= 1;
    }
    while j > 0 {
        ops.push(Op::Insert(new_chars[j - 1]));
        j -= 1;
    }
    ops.reverse();

    let keep_bg = DIFF_KEEP_BG;
    let remove_bg = DIFF_REMOVE_BG;
    let add_bg = DIFF_ADD_BG;

    let mut orig_segs = Vec::new();
    let mut new_segs = Vec::new();
    let mut orig_buf = String::new();
    let mut new_buf = String::new();
    let mut orig_bg = keep_bg;
    let mut new_bg = keep_bg;

    for op in &ops {
        match op {
            Op::Keep(c) => {
                if orig_bg != keep_bg {
                    flush_seg(&mut orig_segs, &mut orig_buf, orig_bg);
                    orig_bg = keep_bg;
                }
                if new_bg != keep_bg {
                    flush_seg(&mut new_segs, &mut new_buf, new_bg);
                    new_bg = keep_bg;
                }
                orig_buf.push(*c);
                new_buf.push(*c);
            }
            Op::Delete(c) => {
                if orig_bg != remove_bg {
                    flush_seg(&mut orig_segs, &mut orig_buf, orig_bg);
                    orig_bg = remove_bg;
                }
                orig_buf.push(*c);
            }
            Op::Insert(c) => {
                if new_bg != add_bg {
                    flush_seg(&mut new_segs, &mut new_buf, new_bg);
                    new_bg = add_bg;
                }
                new_buf.push(*c);
            }
        }
    }
    flush_seg(&mut orig_segs, &mut orig_buf, orig_bg);
    flush_seg(&mut new_segs, &mut new_buf, new_bg);

    (orig_segs, new_segs)
}

fn flush_seg(segs: &mut Vec<DiffSeg>, buf: &mut String, bg: Color32) {
    if !buf.is_empty() {
        segs.push(DiffSeg {
            text: std::mem::take(buf),
            fg: None,
            bg,
        });
    }
}

/// Helper: append a non-empty text segment with the given background.
pub(crate) fn push_seg(segs: &mut Vec<DiffSeg>, text: &str, bg: Color32) {
    if !text.is_empty() {
        segs.push(DiffSeg {
            text: text.to_string(),
            fg: None,
            bg,
        });
    }
}
