use super::*;

/// A single front/end operation, in command order, for exact structural diffing.
#[derive(Clone, Debug)]
pub(crate) enum ExactOp {
    /// Insert known text at the front of the name (core prepends, so the last
    /// prefix op ends up front-most).
    Prefix(String),
    /// Insert known text at the end of the name.
    Suffix(String),
    /// Positional insert. Resolved per row: the position refers to the string
    /// as it is at this point in the op sequence, exactly like the core's
    /// `apply_insert`. Rows where it lands in the middle (while other ops are
    /// present) fall back to the LCS diff.
    InsertAt { pos: isize, text: String },
    /// Remove N characters from the front (front insert text first, then the name).
    RemoveFront(usize),
    /// Remove N characters from the end (end insert text first, then the name).
    RemoveEnd(usize),
    /// Remove From/To (1-based, negative from the end, `to = 0` = through the
    /// end). Resolved per row; only pure front or end crops are exact, any
    /// middle removal falls back to the LCS diff for that row.
    RemoveFromTo { from: isize, to: isize },
}

/// Structural (front/end) diff for configs whose ops only ever touch the first
/// or last characters of the name. Avoids the general LCS diff (O(n·m)) and —
/// more importantly — aligns removed/added text with the op's real front/end
/// semantics, which the LCS backtracking can contradict on repeated patterns
/// (e.g. remove-first-3 of `abcabc` should highlight the leading `abc`).
///
/// Built once per rows-cache build via [`FrontEndDiff::from_config`];
/// [`FrontEndDiff::diff_segs`] is then applied per row in O(len). Configs that
/// don't fit (or rows that don't verify) return `None`, keeping the lazy LCS
/// fallback exactly as-is.
#[derive(Clone, Debug)]
pub(crate) enum FrontEndDiff {
    /// Only `RemoveFirst`/`RemoveLast`/`Prefix`/`Suffix`/`Insert` — replay the
    /// ops per row (removal clamping and insert-position resolution depend on
    /// the name length) and verify the reconstructed name matches the real one
    /// before emitting segments. A lone insert is split exactly at its resolved
    /// position (front, middle, or end); middle inserts combined with other ops
    /// fall back to the LCS diff.
    Exact { ops: Vec<ExactOp> },
    /// No removals; inserts include generated text of unknown length
    /// (date/numbering/folder names). The retained name is located inside the
    /// new name by search instead of being reconstructed.
    Search { has_front: bool, has_end: bool },
}

impl FrontEndDiff {
    /// Classify `cfg` as front/end-only. Returns `None` (→ general LCS diff)
    /// when any op can change the middle of the name.
    pub(crate) fn from_config(cfg: &RenameConfig) -> Option<FrontEndDiff> {
        let mut ops: Vec<ExactOp> = Vec::new();
        let mut tier1 = true; // every op is an exact front/end op so far
        let mut has_unknown_front = false;
        let mut has_unknown_end = false;

        for item in &cfg.command_order {
            match item {
                RenameItem::RemoveFirst(n) => {
                    if has_unknown_front || has_unknown_end {
                        return None; // a removal could eat generated text
                    }
                    ops.push(if *n >= 0 {
                        ExactOp::RemoveFront(*n as usize)
                    } else {
                        ExactOp::RemoveEnd((-*n) as usize)
                    });
                }
                RenameItem::RemoveLast(n) => {
                    if has_unknown_front || has_unknown_end {
                        return None;
                    }
                    ops.push(if *n >= 0 {
                        ExactOp::RemoveEnd(*n as usize)
                    } else {
                        ExactOp::RemoveFront((-*n) as usize)
                    });
                }
                RenameItem::RemoveFromTo(f, t) => {
                    if has_unknown_front || has_unknown_end {
                        return None;
                    }
                    ops.push(ExactOp::RemoveFromTo { from: *f, to: *t });
                }
                RenameItem::Prefix(p) => ops.push(ExactOp::Prefix(p.clone())),
                RenameItem::Suffix(s) => ops.push(ExactOp::Suffix(s.clone())),
                RenameItem::Insert(text, pos) => ops.push(ExactOp::InsertAt {
                    pos: *pos,
                    text: text.clone(),
                }),
                RenameItem::Dirname(add, _, pos) => {
                    if !add {
                        continue;
                    }
                    tier1 = false;
                    match pos {
                        Some(0) => has_unknown_front = true,
                        Some(p) if *p < 0 => has_unknown_end = true,
                        _ => return None, // middle insert
                    }
                }
                RenameItem::AddDate(_) | RenameItem::AddFileDate(_) | RenameItem::InsertMeta(_) => {
                    tier1 = false;
                    match cfg.auto_date.date_position {
                        DatePosition::Prefix => has_unknown_front = true,
                        DatePosition::Suffix => has_unknown_end = true,
                        DatePosition::None => {}
                    }
                }
                RenameItem::Numbering(mode, ..) => {
                    tier1 = false;
                    match mode {
                        NumberingMode::Prefix => has_unknown_front = true,
                        NumberingMode::Suffix => has_unknown_end = true,
                        NumberingMode::Both => {
                            has_unknown_front = true;
                            has_unknown_end = true;
                        }
                        NumberingMode::Insert => return None,
                    }
                }
                _ => return None,
            }
        }

        if tier1 {
            Some(FrontEndDiff::Exact { ops })
        } else if ops
            .iter()
            .any(|op| matches!(op, ExactOp::RemoveFront(_) | ExactOp::RemoveEnd(_)))
        {
            // Removals combined with generated inserts: fall back to the general diff.
            None
        } else {
            Some(FrontEndDiff::Search {
                has_front: has_unknown_front,
                has_end: has_unknown_end,
            })
        }
    }

    /// Compute diff segments for one row, or `None` to fall back to `diff_colored`.
    ///
    /// `ext_text` is the preserved extension *including the leading dot* (`""`
    /// when there is none, e.g. extensionless files and directories). The core
    /// rename pipeline applies stem ops (remove/suffix/insert) before the
    /// extension, so the structural model must too — otherwise end-side ops on
    /// extension-bearing names would reconstruct the wrong name and fall back
    /// to the fragmented LCS diff.
    pub(crate) fn diff_segs(
        &self,
        orig: &str,
        new: &str,
        ext_text: &str,
    ) -> Option<(Vec<DiffSeg>, Vec<DiffSeg>)> {
        match self {
            FrontEndDiff::Exact { ops } => self.diff_exact(ops, orig, new, ext_text),
            FrontEndDiff::Search { has_front, has_end } => {
                self.diff_search(*has_front, *has_end, orig, new)
            }
        }
    }

    /// Exact variant: replay the ops against the row's stem (removal clamping
    /// and insert-position resolution depend on the stem length; the extension
    /// is preserved untouched, like the core pipeline) and verify the result
    /// against `new`. Front/end insert texts are tracked as piece lists so op
    /// order is preserved exactly — core *prepends* prefixes, so the last
    /// front op ends up front-most.
    fn diff_exact(
        &self,
        ops: &[ExactOp],
        orig: &str,
        new: &str,
        ext_text: &str,
    ) -> Option<(Vec<DiffSeg>, Vec<DiffSeg>)> {
        // A lone positional insert: split exactly at the resolved position.
        if let [ExactOp::InsertAt { pos, text }] = ops {
            return self.diff_single_insert(*pos, text, orig, new, ext_text);
        }

        let stem = orig.strip_suffix(ext_text)?;
        let len = stem.chars().count();
        let mut front: Vec<String> = Vec::new(); // index 0 = front-most piece
        let mut end: Vec<String> = Vec::new(); // index 0 = piece closest to the name
        let mut removed_front = 0usize;
        let mut removed_end = 0usize;

        // Front/end removals share the "eat insert pieces first, then clamp to
        // the name" logic between RemoveFirst/RemoveLast and RemoveFromTo crops.
        let remove_front =
            |n: usize, front: &mut Vec<String>, removed_front: &mut usize, removed_end: usize| {
                let mut remaining = n;
                while remaining > 0 && !front.is_empty() {
                    let fl = front[0].chars().count();
                    let take = remaining.min(fl);
                    front[0] = front[0].chars().skip(take).collect();
                    if front[0].is_empty() {
                        front.remove(0);
                    }
                    remaining -= take;
                }
                if remaining > 0 {
                    let avail = len - *removed_front - removed_end;
                    *removed_front += remaining.min(avail);
                }
            };
        let remove_end =
            |n: usize, end: &mut Vec<String>, removed_end: &mut usize, removed_front: usize| {
                let mut remaining = n;
                while remaining > 0 && !end.is_empty() {
                    let last = end.len() - 1;
                    let el = end[last].chars().count();
                    let take = remaining.min(el);
                    end[last] = end[last].chars().take(el - take).collect();
                    if end[last].is_empty() {
                        end.pop();
                    }
                    remaining -= take;
                }
                if remaining > 0 {
                    let avail = len - removed_front - *removed_end;
                    *removed_end += remaining.min(avail);
                }
            };

        for op in ops {
            match op {
                ExactOp::Prefix(p) => front.insert(0, p.clone()),
                ExactOp::Suffix(s) => end.push(s.clone()),
                ExactOp::RemoveFront(n) => {
                    remove_front(*n, &mut front, &mut removed_front, removed_end)
                }
                ExactOp::RemoveEnd(n) => remove_end(*n, &mut end, &mut removed_end, removed_front),
                ExactOp::RemoveFromTo { from, to } => {
                    let kept_len = len - removed_front - removed_end;
                    let current_len = pieces_chars(&front) + kept_len + pieces_chars(&end);
                    match resolve_from_to_range(current_len, *from, *to) {
                        // Pure front crop: remove from the start of the name.
                        Some((0, to_idx)) => {
                            remove_front(to_idx, &mut front, &mut removed_front, removed_end)
                        }
                        // Pure end crop: remove through the end of the name.
                        Some((from_idx, to_idx)) if to_idx == current_len => remove_end(
                            current_len - from_idx,
                            &mut end,
                            &mut removed_end,
                            removed_front,
                        ),
                        // Middle removal: not exact — fall back to the LCS diff.
                        _ => return None,
                    }
                }
                ExactOp::InsertAt { pos, text } => {
                    let kept_len = len - removed_front - removed_end;
                    let current_len = pieces_chars(&front) + kept_len + pieces_chars(&end);
                    let p = resolve_insert_pos(*pos, current_len);
                    if p == 0 {
                        front.insert(0, text.clone());
                    } else if p == current_len {
                        end.push(text.clone());
                    } else {
                        // Middle insert combined with other ops: not exact — LCS.
                        return None;
                    }
                }
            }
        }

        let kept: String = stem
            .chars()
            .skip(removed_front)
            .take(len - removed_front - removed_end)
            .collect();
        let front_str: String = front.concat();
        let end_str: String = end.concat();
        if format!("{}{}{}{}", front_str, kept, end_str, ext_text) != new {
            return None; // model drifted from the real pipeline — use LCS
        }

        let removed_front_txt: String = stem.chars().take(removed_front).collect();
        let removed_end_txt: String = stem
            .chars()
            .skip(len - removed_end)
            .take(removed_end)
            .collect();
        let mut orig_segs = Vec::new();
        push_seg(&mut orig_segs, &removed_front_txt, DIFF_REMOVE_BG);
        push_seg(&mut orig_segs, &kept, DIFF_KEEP_BG);
        push_seg(&mut orig_segs, &removed_end_txt, DIFF_REMOVE_BG);
        push_seg(&mut orig_segs, ext_text, DIFF_KEEP_BG);
        let mut new_segs = Vec::new();
        push_seg(&mut new_segs, &front_str, DIFF_ADD_BG);
        push_seg(&mut new_segs, &kept, DIFF_KEEP_BG);
        push_seg(&mut new_segs, &end_str, DIFF_ADD_BG);
        push_seg(&mut new_segs, ext_text, DIFF_KEEP_BG);
        Some((orig_segs, new_segs))
    }

    /// Lone positional insert: `new == stem[..p] + text + stem[p..] + ext` with
    /// `p` resolved exactly like the core's `apply_insert` (char-based, against
    /// the stem; negative positions count gaps from the end, positive ones
    /// clamp to the stem length).
    fn diff_single_insert(
        &self,
        pos: isize,
        text: &str,
        orig: &str,
        new: &str,
        ext_text: &str,
    ) -> Option<(Vec<DiffSeg>, Vec<DiffSeg>)> {
        let stem = orig.strip_suffix(ext_text)?;
        let len = stem.chars().count();
        let p = resolve_insert_pos(pos, len);
        let head: String = stem.chars().take(p).collect();
        let tail: String = stem.chars().skip(p).collect();
        if format!("{}{}{}{}", head, text, tail, ext_text) != new {
            return None;
        }
        let mut orig_segs = Vec::new();
        push_seg(&mut orig_segs, orig, DIFF_KEEP_BG);
        let mut new_segs = Vec::new();
        push_seg(&mut new_segs, &head, DIFF_KEEP_BG);
        push_seg(&mut new_segs, text, DIFF_ADD_BG);
        push_seg(&mut new_segs, &tail, DIFF_KEEP_BG);
        push_seg(&mut new_segs, ext_text, DIFF_KEEP_BG);
        Some((orig_segs, new_segs))
    }

    /// Search variant: no removals, so the whole original name is retained and
    /// only needs to be located inside `new`.
    fn diff_search(
        &self,
        has_front: bool,
        has_end: bool,
        orig: &str,
        new: &str,
    ) -> Option<(Vec<DiffSeg>, Vec<DiffSeg>)> {
        if orig.is_empty() {
            // Nothing retained: everything in `new` was inserted.
            let mut new_segs = Vec::new();
            push_seg(&mut new_segs, new, DIFF_ADD_BG);
            return Some((Vec::new(), new_segs));
        }

        let (x, y): (&str, &str) = if !has_end {
            // Front inserts only: new == X + orig.
            let k = new.strip_suffix(orig)?;
            (k, "")
        } else if !has_front {
            // End inserts only: new == orig + Y.
            let k = new.strip_prefix(orig)?;
            ("", k)
        } else {
            // Inserts on both sides: locate the retained name in the middle.
            let pos = new.find(orig)?;
            (&new[..pos], &new[pos + orig.len()..])
        };

        let mut orig_segs = Vec::new();
        push_seg(&mut orig_segs, orig, DIFF_KEEP_BG);
        let mut new_segs = Vec::new();
        push_seg(&mut new_segs, x, DIFF_ADD_BG);
        push_seg(&mut new_segs, orig, DIFF_KEEP_BG);
        push_seg(&mut new_segs, y, DIFF_ADD_BG);
        Some((orig_segs, new_segs))
    }
}

/// Total char length of a piece list without materialising the concatenation.
fn pieces_chars(pieces: &[String]) -> usize {
    pieces.iter().map(|s| s.chars().count()).sum()
}
