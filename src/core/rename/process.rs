use super::*;
use std::path::{Path, PathBuf};

use crate::core::execute::RenameOp;

// ──────────────────────────────────────────────────────────
// Filename processing pipeline
// ──────────────────────────────────────────────────────────

pub fn process_filename(
    original: &str,
    compiled: &CompiledConfig,
    index: Option<usize>,
    parent_name: Option<&str>,
    is_dir: bool,
    meta: &FileMeta,
) -> String {
    let mut state = FilenameState::new(original, is_dir);
    run_items(
        &mut state,
        &compiled.config.command_order,
        index,
        compiled,
        parent_name,
        is_dir,
        meta,
    );
    finalize_name(original, &state)
}

/// Filename split into stem + extension while the rename pipeline runs.
pub(crate) struct FilenameState {
    pub(crate) stem: String,
    pub(crate) ext: Option<String>,
}

impl FilenameState {
    pub(crate) fn new(original: &str, is_dir: bool) -> Self {
        let (stem, ext) = split_extension_for(original, is_dir);
        Self { stem, ext }
    }
}

/// Re-derive `(stem, ext)` after an op rebuilt `full` from the current
/// stem/extension. Files re-identify the extension from `full`. Directories
/// have no extension to identify from a name, so they keep `ext` untouched and
/// strip its suffix back off `full` — this preserves an extension that an
/// Extension *Fixed*/*Extra* op added earlier, so later stem ops treat it like
/// a file's extension instead of folding it into the name.
fn resplit(full: &str, ext: Option<&str>, is_dir: bool) -> (String, Option<String>) {
    if !is_dir {
        return split_extension_for(full, false);
    }
    match ext {
        Some(e) if !e.is_empty() => match full.strip_suffix(&format!(".{e}")) {
            Some(s) => (s.to_string(), Some(e.to_string())),
            None => (full.to_string(), None),
        },
        _ => (full.to_string(), None),
    }
}

/// Filesystem-derived inputs for one file's pipeline run.
///
/// Building this performs the single `exists`/dirname/`stat` lookup a file
/// needs, so the prefix, numbering, and suffix stages all share one context
/// instead of re-deriving it per stage.
pub(crate) struct FileContext {
    pub(crate) path: PathBuf,
    pub(crate) parent: PathBuf,
    pub(crate) file_name: String,
    pub(crate) parent_name: Option<String>,
    pub(crate) is_dir: bool,
    pub(crate) meta: FileMeta,
}

impl FileContext {
    /// Derive the pipeline context for `file_path_str`, or `None` when the path
    /// does not exist (mirroring [`calculate_rename`](crate::calculate_rename)).
    pub(crate) fn from_path(
        file_path_str: &str,
        compiled: &CompiledConfig,
        dirname_level: Option<usize>,
    ) -> Option<Self> {
        let path = Path::new(file_path_str);
        // One stat answers existence, `is_dir` and the file times. This used to be
        // three stats of the same inode: `path.exists()`, `path.is_dir()`, and then
        // another `fs::metadata` inside `FileMeta`.
        let md = std::fs::metadata(path).ok()?;

        let parent = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let file_name = path.file_name()?.to_string_lossy().into_owned();

        let dl = dirname_level
            .unwrap_or(compiled.config.append_folder.dirname_level)
            .max(1);
        let sep = compiled
            .config
            .append_folder
            .add_dirname_sep
            .as_deref()
            .unwrap_or("");
        // Collect `dl` folder names from the immediate parent upward, then order
        // them root-ward → parent so `lvl` covers every folder in the range
        // (e.g. lvl=2 on /a/b/c/file.txt appends "b-c-"), not just one level.
        let mut ancestor = Some(parent.as_path());
        let mut dirs: Vec<String> = Vec::with_capacity(dl);
        for _ in 0..dl {
            let Some(p) = ancestor else { break };
            match p.file_name() {
                Some(name) => dirs.push(name.to_string_lossy().into_owned()),
                None => {
                    // Root reached. On Windows the drive prefix counts as a folder
                    // (e.g. `C:`); strip the colon since it is illegal in names.
                    if let Some(std::path::Component::Prefix(pre)) = p.components().next() {
                        dirs.push(
                            pre.as_os_str()
                                .to_string_lossy()
                                .trim_end_matches(':')
                                .to_string(),
                        );
                    }
                    break;
                }
            }
            ancestor = p.parent();
        }
        dirs.reverse();
        let parent_name = if dirs.is_empty() {
            None
        } else {
            Some(dirs.join(sep))
        };

        let is_dir = md.is_dir();
        // `compiled` carries whether this config consumes EXIF at all (see
        // `config_needs_exif_date`), so the read is skipped for the common case.
        let meta = FileMeta::from_metadata(path, Some(&md), compiled.needs_exif_date);
        Some(Self {
            path: path.to_path_buf(),
            parent,
            file_name,
            parent_name,
            is_dir,
            meta,
        })
    }

    /// Build the rename op for a computed final name, or `None` when the name is
    /// unchanged and `include_unchanged` is false.
    ///
    /// `include_unchanged` is only used by the single-file primitive
    /// ([`calculate_rename`](crate::calculate_rename), the GUI row cache) and by
    /// the planner when an output directory could still move the file. Batch
    /// plans never retain a no-op (see [`RenameOp::is_noop`]).
    pub(crate) fn into_op(self, new_name: String, include_unchanged: bool) -> Option<RenameOp> {
        if new_name != self.file_name || include_unchanged {
            Some(RenameOp {
                new_path: self.parent.join(&new_name),
                original_path: self.path,
                original_name: self.file_name,
                new_name,
                was_copy: false,
                relocated: false,
                original_permissions: None,
                applied_attributes: String::new(),
                applied_timestamps_mask: 0u8,
            })
        } else {
            None
        }
    }
}

/// Run a slice of pipeline items over a filename state.
///
/// This is the reusable core of the pipeline; `process_filename` runs every
/// item in order, applying `index` when it reaches the `Numbering` item.
pub(crate) fn run_items(
    state: &mut FilenameState,
    items: &[RenameItem],
    index: Option<usize>,
    compiled: &CompiledConfig,
    parent_name: Option<&str>,
    is_dir: bool,
    meta: &FileMeta,
) {
    let config = &compiled.config;
    let re_yyyy_mm_dd = &compiled.re_yyyy_mm_dd;
    let re_mm_dd_yyyy = &compiled.re_mm_dd_yyyy;

    // Move the state into locals so the item arms can read/write plain
    // `stem`/`ext`; write back once at the end.
    let mut stem = std::mem::take(&mut state.stem);
    let mut ext = state.ext.take();

    for item in items {
        match item {
            RenameItem::Regex(re, repl, full, simple) => {
                let pat = effective_pattern(*simple, re);
                let re_obj = build_regex(&pat).ok();
                if let Some(r) = re_obj {
                    if *full {
                        let mut full_name = if let Some(e) = &ext {
                            format!("{}.{}", stem, e)
                        } else {
                            stem.clone()
                        };
                        full_name = r.replace(&full_name, repl.as_str()).to_string();
                        let (s, e) = resplit(&full_name, ext.as_deref(), is_dir);
                        stem = s;
                        ext = e;
                    } else {
                        stem = r.replace(&stem, repl.as_str()).to_string();
                    }
                }
            }
            RenameItem::Replace(find, with, case, first) => {
                stem = apply_replace_op(&stem, find, with, *case, *first);
            }
            RenameItem::RemoveFirst(n) => stem = apply_remove_first(&stem, *n),
            RenameItem::RemoveLast(n) => stem = apply_remove_last(&stem, *n),
            RenameItem::RemoveFromTo(f, t) => stem = apply_remove_from_to(&stem, *f, *t),
            RenameItem::RemoveDigits => stem = apply_remove_digits(&stem),
            RenameItem::RemoveHigh => stem = apply_remove_high(&stem),
            RenameItem::RemoveAllChars => stem.clear(),
            RenameItem::RemoveLeadDots(mode) => stem = apply_remove_lead_dots(&stem, *mode),
            RenameItem::RemoveChars(s) => stem = apply_remove_chars(&stem, s),
            RenameItem::RemoveCrop(mode, s) => stem = apply_crop(&stem, *mode, s),
            RenameItem::RemoveWords(s, case_sensitive) => {
                if !compiled.re_words.is_empty() {
                    for re in &compiled.re_words {
                        stem = re.replace_all(&stem, "").to_string();
                    }
                } else {
                    stem = apply_remove_words(&stem, s, *case_sensitive);
                }
            }
            RenameItem::RemoveSymbols => {
                stem = apply_remove_symbols(&stem);
                stem = stem.trim_end().to_string();
            }
            RenameItem::Trim => stem = stem.trim().to_string(),
            RenameItem::DoubleSpaces => stem = apply_double_spaces(&stem),
            RenameItem::RemoveAccents => stem = apply_remove_accents(&stem),
            RenameItem::Prefix(s) => stem = format!("{}{}", s, stem),
            RenameItem::Suffix(s) => stem = format!("{}{}", stem, s),
            RenameItem::Insert(s, pos) => stem = apply_insert(&stem, s, *pos),
            RenameItem::WordSpace => stem = apply_word_space(&stem),
            RenameItem::CaseName(mode) => {
                stem = apply_case(&stem, *mode);
                if let Some(ref ex) = config.case.case_exception
                    && !ex.is_empty()
                {
                    stem = apply_case_exceptions(&stem, ex);
                }
            }
            RenameItem::CaseExt(mode) => {
                if !is_dir && let Some(e) = &ext {
                    ext = Some(apply_case(e, *mode));
                }
            }
            RenameItem::Numbering(mode, start, inc, pad, sep, pos, typ, cas, _brk) => {
                if let Some(idx) = index {
                    // `idx` already accounts for per-folder restart and
                    // `numbering_break` resets (see `numbering_indices`).
                    let num = (*start as isize) + (idx as isize) * (*inc);
                    stem = apply_numbering(
                        &stem,
                        *mode,
                        num,
                        *pad,
                        &NumberingConfig {
                            sep: sep.as_deref(),
                            pos: *pos,
                            num_type: typ.as_deref(),
                            num_case: cas.as_deref(),
                        },
                    );
                }
            }
            RenameItem::Extension(mode, rep, app, rem) => {
                // Directories have no extension to case or remove, but *Fixed*
                // and *Extra* add a new one — that is meaningful for a folder,
                // so it is allowed (and later re-splits keep it attached).
                if !is_dir {
                    ext = apply_extension_op(ext, *mode, rep.as_deref(), app.as_deref(), *rem);
                } else if rep.is_some() || app.is_some() {
                    ext = apply_extension_op(None, *mode, rep.as_deref(), app.as_deref(), *rem);
                }
            }
            RenameItem::Dirname(add, sep, pos) => {
                if *add && let Some(pname) = parent_name {
                    stem = apply_dirname(&stem, pname, sep.as_deref(), *pos);
                }
            }
            RenameItem::MovePart(s, l, t, sep) => {
                stem = apply_move_part(&stem, *s, *l, *t, sep.as_deref())
            }
            RenameItem::CopyPart(s, l, t, sep) => {
                stem = apply_copy_part(&stem, *s, *l, *t, sep.as_deref())
            }
            RenameItem::AddDate(fmt) => {
                if config.auto_date.date_position != DatePosition::None {
                    let suffix = config.auto_date.date_position == DatePosition::Suffix;
                    let time = date_type_time_source(
                        config.auto_date.auto_date_type.as_deref(),
                        meta,
                        config,
                    );
                    stem = apply_add_date(&stem, fmt, time, suffix, config);
                }
            }
            RenameItem::AddFileDate(fmt) => {
                if config.auto_date.date_position != DatePosition::None {
                    let suffix = config.auto_date.date_position == DatePosition::Suffix;
                    stem = apply_add_date(&stem, fmt, meta.modified, suffix, config);
                }
            }
            RenameItem::ExtensionAddIfMissing => {}
            RenameItem::InsertMeta(tag) => {
                if config.auto_date.date_position != DatePosition::None {
                    let suffix = config.auto_date.date_position == DatePosition::Suffix;
                    stem = apply_insert_meta(&stem, tag, meta, suffix, config);
                }
            }
            RenameItem::NameSegment(from, to) => {
                // Same position semantics as Remove From/To (see
                // `resolve_from_to_range`): 1-based, negative from the end,
                // `to = 0` means through the end of the name.
                if let Some((start, end)) = resolve_from_to_range(stem.chars().count(), *from, *to)
                {
                    stem = stem.chars().skip(start).take(end - start).collect();
                }
            }
            // ── Name variants (operate on full name, like Regex with full=true) ──
            RenameItem::NameFixed(val) => {
                // Replace the stem, keep extension
                // Reconstruct full name from current stem+ext, split, and replace stem
                let full = if let Some(e) = &ext {
                    format!("{}.{}", stem, e)
                } else {
                    stem.clone()
                };
                let (_, e) = resplit(&full, ext.as_deref(), is_dir);
                stem = val.clone();
                ext = e;
            }
            RenameItem::NameRemove => {
                // Remove the stem, keep extension as dotfile
                let full = if let Some(e) = &ext {
                    format!("{}.{}", stem, e)
                } else {
                    stem.clone()
                };
                let (_, e) = resplit(&full, ext.as_deref(), is_dir);
                stem = String::new();
                ext = e;
            }
            RenameItem::NameReverse => {
                // Reverse the stem, keep extension
                let full = if let Some(e) = &ext {
                    format!("{}.{}", stem, e)
                } else {
                    stem.clone()
                };
                let (s, e) = resplit(&full, ext.as_deref(), is_dir);
                stem = s.chars().rev().collect();
                ext = e;
            }
            RenameItem::NamePadNumbers(amount, pad_char) => {
                if *amount == 0 {
                    continue;
                }
                let full = if let Some(e) = &ext {
                    format!("{}.{}", stem, e)
                } else {
                    stem.clone()
                };
                let (s, e) = resplit(&full, ext.as_deref(), is_dir);
                // Find the first contiguous digit sequence in the stem
                let mut first_digit_start = None;
                let mut first_digit_end = None;
                let chars: Vec<char> = s.chars().collect();
                for (i, &c) in chars.iter().enumerate() {
                    if c.is_ascii_digit() {
                        if first_digit_start.is_none() {
                            first_digit_start = Some(i);
                        }
                    } else if first_digit_start.is_some() && first_digit_end.is_none() {
                        first_digit_end = Some(i);
                        break;
                    }
                }
                let end = first_digit_end.unwrap_or(chars.len());
                if let Some(start) = first_digit_start
                    && end <= chars.len()
                {
                    let digit_str: String = chars[start..end].iter().collect();
                    let padded = if *amount > digit_str.len() {
                        format!(
                            "{}{}",
                            pad_char.to_string().repeat(*amount - digit_str.len()),
                            digit_str
                        )
                    } else {
                        digit_str
                    };
                    stem = chars[..start].iter().collect::<String>()
                        + &padded
                        + &chars[end..].iter().collect::<String>();
                }
                ext = e;
            }
            RenameItem::NameReformatDate {
                parse_fmt,
                output_fmt,
            } => {
                let full = if let Some(e) = &ext {
                    format!("{}.{}", stem, e)
                } else {
                    stem.clone()
                };
                let (stem_str, e) = resplit(&full, ext.as_deref(), is_dir);
                let result = if let Some(pf) = parse_fmt {
                    // User specified both parse and output format
                    // Convert Awara-style % tokens to regex capture groups
                    let mut pattern = String::new();
                    let mut tokens: Vec<&str> = Vec::new();
                    let mut i = 0;
                    let pchars: Vec<char> = pf.chars().collect();
                    while i < pchars.len() {
                        if pchars[i] == '%' && i + 1 < pchars.len() {
                            let token: String = pchars[i..=i + 1].iter().collect();
                            match token.as_str() {
                                "%Y" => {
                                    pattern.push_str("(\\d{4})");
                                    tokens.push("%Y");
                                }
                                "%y" => {
                                    pattern.push_str("(\\d{2})");
                                    tokens.push("%y");
                                }
                                "%m" => {
                                    pattern.push_str("(\\d{2})");
                                    tokens.push("%m");
                                }
                                "%d" => {
                                    pattern.push_str("(\\d{2})");
                                    tokens.push("%d");
                                }
                                "%H" => {
                                    pattern.push_str("(\\d{2})");
                                    tokens.push("%H");
                                }
                                "%M" => {
                                    pattern.push_str("(\\d{2})");
                                    tokens.push("%M");
                                }
                                "%S" => {
                                    pattern.push_str("(\\d{2})");
                                    tokens.push("%S");
                                }
                                "%a" => {
                                    pattern.push_str("(\\w{3})");
                                    tokens.push("%a");
                                }
                                "%b" => {
                                    pattern.push_str("(\\w{3})");
                                    tokens.push("%b");
                                }
                                _ => {
                                    pattern.push_str(&regex::escape(&token));
                                }
                            }
                            i += 2;
                        } else {
                            pattern.push_str(&regex::escape(&pchars[i].to_string()));
                            i += 1;
                        }
                    }
                    if let Ok(re) = Regex::new(&pattern) {
                        let mut out_replacement = output_fmt.to_string();
                        for (idx, tk) in tokens.iter().enumerate() {
                            out_replacement =
                                out_replacement.replace(tk, &format!("${{{}}}", idx + 1));
                        }
                        re.replace_all(&stem_str, &out_replacement).to_string()
                    } else {
                        stem_str.clone()
                    }
                } else {
                    // Auto-detect: try known date patterns and reformat
                    let (ymd_replacement, mdy_replacement) = if output_fmt.contains('%') {
                        (
                            output_fmt
                                .replace("%Y", "${1}")
                                .replace("%y", "${1}")
                                .replace("%m", "${2}")
                                .replace("%d", "${3}"),
                            output_fmt
                                .replace("%Y", "${3}")
                                .replace("%y", "${3}")
                                .replace("%m", "${1}")
                                .replace("%d", "${2}"),
                        )
                    } else {
                        (
                            output_fmt
                                .replace("YYYY", "${1}")
                                .replace("YY", "${1}")
                                .replace("MM", "${2}")
                                .replace("DD", "${3}"),
                            output_fmt
                                .replace("YYYY", "${3}")
                                .replace("YY", "${3}")
                                .replace("MM", "${1}")
                                .replace("DD", "${2}"),
                        )
                    };
                    let mut result = stem_str.clone();
                    let mut has_match = false;
                    if let Some(re) = re_yyyy_mm_dd {
                        let new = re.replace_all(&result, &ymd_replacement).to_string();
                        if new != result {
                            has_match = true;
                            result = new;
                        }
                    }
                    if let Some(re) = re_mm_dd_yyyy
                        && !has_match
                    {
                        let new = re.replace_all(&result, &mdy_replacement).to_string();
                        if new != result {
                            result = new;
                        }
                    }
                    result
                };
                stem = result;
                ext = e;
            }
        }
    }

    // Write the mutated state back.
    state.stem = stem;
    state.ext = ext;
}

/// Reassemble (and sanitize) the final filename from a pipeline state.
pub(crate) fn finalize_name(original: &str, state: &FilenameState) -> String {
    // No trailing-space/dot trimming here: Windows strips trailing spaces and
    // dots from the filename (and from the stem before the extension)
    // automatically, and trimming here would make "remove first/last N" remove
    // more than N characters whenever the cut lands on a space.
    //
    // An empty extension still renders its separator dot. Extension *Fixed* and
    // *Extra* apply as soon as the mode is chosen (before a value is typed), so
    // the dot marks where the extension goes — consistently for files, folders,
    // and extensionless names. The lone empty-stem case stays empty so it falls
    // back to the original name instead of becoming a bare ".".
    let result = match &state.ext {
        Some(e) if !(e.is_empty() && state.stem.is_empty()) => {
            format!("{}.{}", state.stem, e)
        }
        _ => state.stem.clone(),
    };
    if result.trim().is_empty() {
        original.to_string()
    } else {
        sanitize_file_name(&result)
    }
}
