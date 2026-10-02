use super::*;
use std::collections::HashMap;
use std::path::Path;

pub fn make_regex_item(c: &RenameConfig) -> Option<RenameItem> {
    // A match pattern is enough to act: an empty "With" deletes the matched
    // text (standard substitution semantics), so a lone Match is not a no-op.
    if let Some(m) = &c.regex.regex_match
        && !m.is_empty()
    {
        Some(RenameItem::Regex(
            m.clone(),
            c.regex.regex_replace.clone().unwrap_or_default(),
            c.regex.regex_full_name,
            c.regex.regex_simple,
        ))
    } else {
        None
    }
}

pub fn make_replace_item(c: &RenameConfig) -> Option<RenameItem> {
    if let Some(v) = &c.replace.replace
        && !v.is_empty()
    {
        let w = c.replace.with.clone().unwrap_or_default();
        Some(RenameItem::Replace(
            v.clone(),
            w,
            c.replace.replace_case_sensitive,
            c.replace.replace_first,
        ))
    } else {
        None
    }
}

pub fn make_remove_items(c: &RenameConfig) -> Vec<RenameItem> {
    let mut items = Vec::new();
    if let Some(n) = c.remove.remove_first
        && n != 0
    {
        items.push(RenameItem::RemoveFirst(n));
    }
    if let Some(n) = c.remove.remove_last
        && n != 0
    {
        items.push(RenameItem::RemoveLast(n));
    }
    if let Some(f) = c.remove.remove_from
        && f != 0
    {
        // To defaults to 0 ("remove through the end of the name"), so a
        // cleared To field (displayed as 0) still removes from `from` to the
        // end — same result as an explicit `--remove-to 0`. From=0 is not a
        // 1-based position, so it is treated as unset.
        items.push(RenameItem::RemoveFromTo(f, c.remove.remove_to.unwrap_or(0)));
    }
    if c.remove.remove_digits {
        items.push(RenameItem::RemoveDigits);
    }
    if let Some(s) = &c.remove.remove_chars {
        items.push(RenameItem::RemoveChars(s.clone()));
    }
    if let Some(s) = &c.remove.remove_words {
        items.push(RenameItem::RemoveWords(
            s.clone(),
            c.replace.replace_case_sensitive,
        ));
    }
    if c.remove.remove_symbols {
        items.push(RenameItem::RemoveSymbols);
    }
    if c.remove.trim {
        items.push(RenameItem::Trim);
    }
    if c.remove.double_spaces {
        items.push(RenameItem::DoubleSpaces);
    }
    if let Some(mode) = c.remove.remove_lead_dots {
        items.push(RenameItem::RemoveLeadDots(mode));
    }
    if c.remove.remove_all_chars {
        items.push(RenameItem::RemoveAllChars);
    }
    if c.remove.remove_high {
        items.push(RenameItem::RemoveHigh);
    }
    if c.remove.remove_accents {
        items.push(RenameItem::RemoveAccents);
    }
    if let (Some(mode), Some(s)) = (c.remove.crop_mode, &c.remove.remove_crop)
        && !s.is_empty()
    {
        items.push(RenameItem::RemoveCrop(mode, s.clone()));
    }
    items
}

pub fn make_add_items(c: &RenameConfig) -> Vec<RenameItem> {
    let mut items = Vec::new();
    // Insert runs first so its position is relative to the original filename;
    // the prefix is then prepended (always the leading chars) and the suffix
    // appended last (always the trailing chars, even when the insert position
    // clamps to the end). An unset position defaults to 0 (start of filename).
    if let Some(t) = &c.add.add_insert {
        items.push(RenameItem::Insert(t.clone(), c.add.add_at.unwrap_or(0)));
    }
    if let Some(v) = &c.add.add_prefix {
        items.push(RenameItem::Prefix(v.clone()));
    }
    if let Some(v) = &c.add.add_suffix {
        items.push(RenameItem::Suffix(v.clone()));
    }
    items
}

/// True when numbering is active (section mode or a `Numbering` item in
/// `command_order`). SSoT for deciding whether numbering indices must be
/// computed for the pipeline (CLI executor, preview, row cache).
pub fn config_has_numbering(cfg: &RenameConfig) -> bool {
    cfg.numbering.numbering_mode.is_some()
        || cfg
            .command_order
            .iter()
            .any(|item| matches!(item, RenameItem::Numbering(..)))
}

/// True when the config applies file effects that are independent of the name:
/// attributes and/or timestamps. Such effects make an otherwise unchanged file
/// actionable (like the output-dir transform does), so the planner must keep an
/// op for it and execution must not treat it as a no-op to skip.
pub fn config_has_file_effects(cfg: &RenameConfig) -> bool {
    cfg.special.set_attributes.is_some()
        || cfg.special.set_created.is_some()
        || cfg.special.set_modified.is_some()
        || cfg.special.set_accessed.is_some()
}

/// True when planning must read EXIF data for this config.
///
/// Reading EXIF means opening and parsing every file, which costs far more than
/// the stat the pipeline already needs, so it is skipped unless something
/// actually consumes it. Checks `command_order` as well as the Auto Date section
/// fields, mirroring [`config_has_numbering`]: the items are what actually run,
/// and an empty `command_order` runs nothing.
///
/// Both consumers are covered — the Insert Meta tags and an `Add Date` served by
/// an EXIF-backed `auto_date_type` — because the date types used to pass this
/// gate unread, degrading to mtime without saying so.
pub fn config_needs_exif_date(cfg: &RenameConfig) -> bool {
    let exif_type =
        auto_date_type_uses_exif(cfg.auto_date.auto_date_type.as_deref().unwrap_or_default());
    // A section-set `add_date` becomes an `AddDate` item in `CompiledConfig::new`,
    // but this predicate is also used on untranslated configs, so check the
    // section field too. `make_auto_date_items` drops an empty format.
    let add_date_runs = cfg
        .auto_date
        .add_date
        .as_deref()
        .is_some_and(|fmt| !fmt.is_empty())
        || cfg
            .command_order
            .iter()
            .any(|item| matches!(item, RenameItem::AddDate(_)));
    let insert_meta_runs = cfg
        .auto_date
        .insert_meta
        .as_deref()
        .is_some_and(insert_meta_tag_uses_exif)
        || cfg.command_order.iter().any(|item| match item {
            RenameItem::InsertMeta(tag) => insert_meta_tag_uses_exif(tag),
            _ => false,
        });
    (add_date_runs && exif_type) || insert_meta_runs
}

/// Compute the numbering sequence number for each entry of `order`.
///
/// `order` holds file indices in display (numbering) order and the result is
/// aligned with it: `result[k]` is the number for `order[k]`, or `None` when
/// `participates` returns false for that file. Numbering runs inline at its
/// `command_order` position, so callers precompute these indices and pass them
/// to [`calculate_rename`](crate::calculate_rename).
///
/// When `restart_folder` is set, numbering is independent per parent directory:
/// each folder keeps its own counter. When `break_every` is set, the counter
/// restarts every N participating files — within each folder when
/// `restart_folder` is set, or globally otherwise.
pub(crate) fn numbering_sequence(
    order: &[usize],
    paths: &[String],
    restart_folder: bool,
    break_every: Option<usize>,
    mut participates: impl FnMut(usize) -> bool,
) -> Vec<Option<usize>> {
    let mut seq = vec![None; order.len()];
    // Borrowed `&Path` keys — no per-file allocation for large batches.
    let mut folder_counter: HashMap<&Path, usize> = HashMap::new();
    // Per-folder participating files numbered since that folder's last break.
    let mut folder_since_break: HashMap<&Path, usize> = HashMap::new();
    let mut counter = 0usize;
    // Participating files numbered since the last break (global numbering).
    let mut since_break = 0usize;

    for (k, &i) in order.iter().enumerate() {
        if !participates(i) {
            continue;
        }

        if restart_folder {
            let parent = Path::new(&paths[i])
                .parent()
                .unwrap_or_else(|| Path::new("."));
            if let Some(n) = break_every {
                let sb = folder_since_break.entry(parent).or_insert(0);
                if *sb >= n {
                    // Every N files in this folder → restart this folder only.
                    folder_counter.insert(parent, 0);
                    *sb = 0;
                }
                *sb += 1;
            }
            let c = folder_counter.entry(parent).or_insert(0);
            let v = *c;
            *c += 1;
            seq[k] = Some(v);
        } else {
            if let Some(n) = break_every {
                if since_break >= n {
                    // Every N files numbered → restart.
                    counter = 0;
                    since_break = 0;
                }
                since_break += 1;
            }
            let v = counter;
            counter += 1;
            seq[k] = Some(v);
        }
    }
    seq
}

pub fn make_numbering_item(c: &RenameConfig) -> Option<RenameItem> {
    c.numbering.numbering_mode.map(|mode| {
        RenameItem::Numbering(
            mode,
            c.numbering.numbering_start,
            c.numbering.numbering_increment,
            c.numbering.numbering_pad,
            c.numbering.numbering_sep.clone(),
            c.numbering.numbering_at,
            c.numbering.numbering_type.clone(),
            c.numbering.numbering_case.clone(),
            c.numbering.numbering_break,
        )
    })
}

pub fn make_case_items(c: &RenameConfig) -> Vec<RenameItem> {
    let mut items = Vec::new();
    if let Some(m) = c.case.case_name {
        items.push(RenameItem::CaseName(m));
    }
    // case_ext removed — users should use the Extension section instead
    items
}

pub fn make_extension_item(c: &RenameConfig) -> Option<RenameItem> {
    if c.extension.extension_replace.is_some()
        || c.extension.extension_append.is_some()
        || c.extension.extension_remove
        || c.extension.extension_mode.is_some()
    {
        Some(RenameItem::Extension(
            c.extension.extension_mode.unwrap_or(ExtensionMode::Lower),
            c.extension.extension_replace.clone(),
            c.extension.extension_append.clone(),
            c.extension.extension_remove,
        ))
    } else {
        None
    }
}

pub fn make_dirname_item(c: &RenameConfig) -> Option<RenameItem> {
    if c.append_folder.add_dirname {
        Some(RenameItem::Dirname(
            c.append_folder.add_dirname,
            c.append_folder.add_dirname_sep.clone(),
            c.append_folder.add_dirname_pos,
        ))
    } else {
        None
    }
}

pub fn make_move_copy_items(c: &RenameConfig) -> Vec<RenameItem> {
    let mut items = Vec::new();
    if let Some(v) = &c.move_copy.move_part {
        items.push(RenameItem::MovePart(
            v.start,
            v.length,
            v.destination,
            v.separator.clone(),
        ));
    }
    if let Some(v) = &c.move_copy.copy_part {
        items.push(RenameItem::CopyPart(
            v.start,
            v.length,
            v.destination,
            v.separator.clone(),
        ));
    }
    items
}

pub fn make_name_items(c: &RenameConfig) -> Vec<RenameItem> {
    let mut items = Vec::new();
    match c.name.name_mode.as_deref() {
        Some("fixed") => {
            let val = c.name.name_value.clone().unwrap_or_default();
            items.push(RenameItem::NameFixed(val));
        }
        Some("remove") => items.push(RenameItem::NameRemove),
        Some("reverse") => items.push(RenameItem::NameReverse),
        Some("pad_numbers") => {
            let val = c.name.name_value.as_deref().unwrap_or("");
            let (amount, pad_char) = if let Some((amt, ch)) = val.split_once('>') {
                (
                    amt.trim().parse::<usize>().unwrap_or(0),
                    ch.chars().next().unwrap_or('0'),
                )
            } else {
                (val.trim().parse::<usize>().unwrap_or(0), '0')
            };
            items.push(RenameItem::NamePadNumbers(amount, pad_char));
        }
        Some("reformat_date") => {
            let val = c.name.name_value.as_deref().unwrap_or("");
            if let Some((parse_part, out_part)) = val.split_once('>') {
                items.push(RenameItem::NameReformatDate {
                    parse_fmt: Some(parse_part.trim().to_string()),
                    output_fmt: out_part.trim().to_string(),
                });
            } else if val.is_empty() {
                items.push(RenameItem::NameReformatDate {
                    parse_fmt: None,
                    output_fmt: "YYYY-MM-DD".to_string(),
                });
            } else {
                items.push(RenameItem::NameReformatDate {
                    parse_fmt: None,
                    output_fmt: val.to_string(),
                });
            }
        }
        _ => {}
    }
    items
}

pub fn make_auto_date_items(c: &RenameConfig) -> Vec<RenameItem> {
    let mut items = Vec::new();
    // An empty format string is the displayed default (unset) — the GUI can
    // produce it via an empty Custom format, so skip it like None.
    if let Some(v) = &c.auto_date.add_date
        && !v.is_empty()
    {
        items.push(RenameItem::AddDate(v.clone()));
    }
    if let Some(v) = &c.auto_date.add_file_date
        && !v.is_empty()
    {
        items.push(RenameItem::AddFileDate(v.clone()));
    }
    if let Some(v) = &c.auto_date.insert_meta {
        items.push(RenameItem::InsertMeta(v.clone()));
    }
    items
}
