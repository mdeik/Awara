use super::*;

/// Merge a possibly-partial stored section order with the canonical default.
///
/// Labels present in `current` keep their relative order. Labels missing from
/// `current` are inserted at their default positions: before the first present
/// label that follows them in `default_order`, or appended at the end when no
/// such label exists. Unknown labels (not part of `default_order`) are kept
/// where the user put them.
pub(crate) fn merge_section_order(current: &[String], default_order: &[&str]) -> Vec<String> {
    let mut result: Vec<String> = Vec::with_capacity(default_order.len().max(current.len()));
    let mut emitted: Vec<&str> = Vec::with_capacity(default_order.len());
    let pos = |label: &str| default_order.iter().position(|&d| d == label);

    for label in current {
        if let Some(p) = pos(label) {
            for &d in &default_order[..p] {
                if !current.iter().any(|c| c == d) && !emitted.contains(&d) {
                    result.push(d.to_string());
                    emitted.push(d);
                }
            }
        }
        result.push(label.clone());
    }
    for &d in default_order {
        if !current.iter().any(|c| c == d) && !emitted.contains(&d) {
            result.push(d.to_string());
        }
    }
    result
}

/// Working config with disabled sections cleared — delegates to the core SSoT
/// ([`awara::cleared_config`]). The UI keeps disabled sections' values.
pub(crate) fn cleared_working_config(
    cfg: &RenameConfig,
    section_enabled: &SectionEnabled,
) -> RenameConfig {
    awara::cleared_config(cfg, section_enabled).into_owned()
}

/// Normalize length-clamped fields in RenameConfig relative to the maximum
/// filename length currently loaded. If remove_first is 50 but the longest
/// file is 10 chars, clamping to 10 yields an identical RenameOp transformation.
///
/// `command_order` is a derived field rebuilt by `build_command_order` each
/// frame and contains the raw (un-clamped) numeric values. It is cleared here
/// so the PartialEq comparison is not corrupted by the raw vs clamped mismatch.
pub fn effective_normalized_config(config: &RenameConfig, max_len: usize) -> RenameConfig {
    let mut cfg = config.clone();
    // command_order is derived — exclude it from the equality fingerprint.
    cfg.command_order.clear();
    let max_i = max_len as isize;
    if let Some(n) = cfg.remove.remove_first {
        cfg.remove.remove_first = Some(n.clamp(-max_i, max_i));
    }
    if let Some(n) = cfg.remove.remove_last {
        cfg.remove.remove_last = Some(n.clamp(-max_i, max_i));
    }
    if let Some(n) = cfg.remove.remove_to {
        cfg.remove.remove_to = Some(n.clamp(-max_i, max_i));
    }
    cfg
}

/// Check if any field in this section differs from default.
/// Uses PartialEq on the section sub-struct for comparison.
pub fn section_is_modified(config: &RenameConfig, section: SectionId) -> bool {
    match section {
        SectionId::Name => {
            let mode = config.name.name_mode.as_deref();
            mode.is_some() && mode != Some("keep")
        }
        SectionId::Special => {
            config.special != SpecialSection::default() || !config.section_order.is_empty()
        }
        SectionId::Regex => {
            // Only the text fields Matter/With trigger modified state,
            // not the checkboxes (Full Name, Simple, v2). Empty text counts
            // as unset (awara::opt_str_field_default).
            !opt_str_field_default(&config.regex.regex_match)
                || !opt_str_field_default(&config.regex.regex_replace)
        }
        SectionId::Replace => {
            // Only the text fields Replace/With trigger modified state,
            // not the checkboxes (Match Case, First).
            !opt_str_field_default(&config.replace.replace)
                || !opt_str_field_default(&config.replace.with)
        }
        SectionId::Remove => {
            // All fields trigger modified state except crop_mode and remove_crop.
            // Numeric fields at 0 are the displayed default, so they count as
            // unset (awara::opt_num_default) — dragging a field back to 0 clears
            // the modified border again.
            let check = |s: &RemoveSection| -> RemoveSection {
                let mut c = s.clone();
                c.crop_mode = None;
                c.remove_crop = None;
                if opt_num_default(c.remove_first) {
                    c.remove_first = None;
                }
                if opt_num_default(c.remove_last) {
                    c.remove_last = None;
                }
                if opt_num_default(c.remove_from) {
                    c.remove_from = None;
                }
                if opt_num_default(c.remove_to) {
                    c.remove_to = None;
                }
                if opt_str_field_default(&c.remove_chars) {
                    c.remove_chars = None;
                }
                if opt_str_field_default(&c.remove_words) {
                    c.remove_words = None;
                }
                c
            };
            check(&config.remove) != check(&RemoveSection::default())
        }
        SectionId::Add => {
            // Position 0 is the default insert position — treat it as unset so
            // setting the position to 0 doesn't mark the section modified.
            let check = |s: &AddSection| -> AddSection {
                let mut c = s.clone();
                if opt_num_default(c.add_at) {
                    c.add_at = None;
                }
                if opt_str_field_default(&c.add_prefix) {
                    c.add_prefix = None;
                }
                if opt_str_field_default(&c.add_suffix) {
                    c.add_suffix = None;
                }
                if opt_str_field_default(&c.add_insert) {
                    c.add_insert = None;
                }
                c
            };
            check(&config.add) != check(&AddSection::default())
        }
        SectionId::Numbering => {
            // Only numbering_mode and restart_folder trigger modified state.
            config.numbering.numbering_mode.is_some() || config.numbering.numbering_restart_folder
        }
        SectionId::Case => {
            let check = |s: &CaseSection| -> CaseSection {
                let mut c = s.clone();
                if opt_str_field_default(&c.case_exception) {
                    c.case_exception = None;
                }
                c
            };
            check(&config.case) != check(&CaseSection::default())
        }
        SectionId::Extension => config.extension != ExtensionSection::default(),
        SectionId::AutoDate => {
            // Modified when any date-related field is set; an empty format
            // string (e.g. from a cleared Custom format) counts as unset.
            !opt_str_field_default(&config.auto_date.add_date)
                || !opt_str_field_default(&config.auto_date.add_file_date)
                || config.auto_date.insert_meta.is_some()
        }
        SectionId::AppendFolder => {
            // Only add_dirname_pos (first selector combo) triggers modified state.
            config.append_folder.add_dirname_pos.is_some()
        }
        SectionId::MoveCopy => {
            // Only the first selector (action combo) triggers modified state.
            config.move_copy.move_part.is_some() || config.move_copy.copy_part.is_some()
        }
        SectionId::Filters => {
            // Numeric filter fields at 0 and empty text fields (except the
            // mask, whose default is "*") are the displayed default (unset).
            let check = |s: &FiltersSection| -> FiltersSection {
                let mut c = s.clone();
                if opt_num_default(c.min_name_len) {
                    c.min_name_len = None;
                }
                if opt_num_default(c.max_name_len) {
                    c.max_name_len = None;
                }
                if opt_num_default(c.filter_level) {
                    c.filter_level = None;
                }
                if opt_num_default(c.min_path_len) {
                    c.min_path_len = None;
                }
                if opt_num_default(c.max_path_len) {
                    c.max_path_len = None;
                }
                if opt_str_field_default(&c.exclude_regex) {
                    c.exclude_regex = None;
                }
                c
            };
            check(&config.filters) != check(&FiltersSection::default())
        }
        SectionId::CopyTo => {
            // Only the path field triggers modified state,
            // not the checkboxes (Copy not Move, Keep Structure).
            !opt_str_field_default(&config.copy_to.output_dir)
        }
        SectionId::NameSegment => config.name_segment != NameSegmentSection::default(),
    }
}

pub fn reset_section(c: &mut RenameConfig, section: SectionId) {
    c.clear_section(section);
}

/// Reset only the move/copy portion of `GuiEditState` (without affecting auto-date fields).
pub fn reset_move_copy_edit_state(e: &mut GuiEditState) {
    let mv_def = MoveCopyValue::default();
    e.move_part_action = None;
    e.move_part_val1 = mv_def.start;
    e.move_part_val2 = mv_def.length;
    e.move_part_dest = None;
    e.move_part_dest_val = mv_def.destination;
    e.move_part_sep = String::new();
}

/// Compose the `MoveCopySection` from the GUI edit state.
/// Copy and move are mutually exclusive: choosing one clears the other, so the
/// preview never applies a stale operation after switching action types.
pub fn compose_move_copy_section(e: &GuiEditState) -> MoveCopySection {
    let action = e.move_part_action.as_deref().unwrap_or("");
    let val1 = e.move_part_val1;
    let val2 = e.move_part_val2;
    let to = match e.move_part_dest.as_deref() {
        Some("To start") => 1_isize,
        Some("To end") => -1_isize,
        Some("To pos") => e.move_part_dest_val,
        _ => 0_isize,
    };
    let (start, len) = match action {
        "Copy first n" | "Move first n" => (1_isize, val1.max(1)),
        "Copy last n" | "Move last n" => (-1_isize, val1.max(1)),
        "Copy range" | "Move range" => (val1, val2.max(1)),
        _ => (val1.max(1), 1_isize),
    };
    let entry = MoveCopyValue {
        start,
        length: len,
        destination: to,
        separator: if e.move_part_sep.is_empty() {
            None
        } else {
            Some(e.move_part_sep.clone())
        },
    };
    match action {
        "Copy first n" | "Copy last n" | "Copy range" => MoveCopySection {
            move_part: None,
            copy_part: Some(entry),
        },
        "Move first n" | "Move last n" | "Move range" => MoveCopySection {
            move_part: Some(entry),
            copy_part: None,
        },
        _ => MoveCopySection::default(),
    }
}

/// Reset only the auto-date portion of `GuiEditState` (without affecting move/copy fields).
pub fn reset_auto_date_edit_state(e: &mut GuiEditState) {
    e.auto_date_type = None;
    e.auto_date_format_key = None;
    e.auto_date_sep = None;
    e.auto_date_custom = None;
}
