use super::*;

/// `replace_case_sensitive` controls case sensitivity for replace and
/// remove-words operations. Regex fields are always case-sensitive; use an
/// inline `(?i)` for case-insensitive matching.
///
/// Metadata for a single config field used for TUI display and editing.
pub struct FieldMeta {
    pub key: &'static str,
    pub flag: &'static str,
    pub description: &'static str,
}

pub(crate) fn fmt_to(t: isize) -> String {
    if t == -1_isize {
        "end".into()
    } else {
        t.to_string()
    }
}

/// Format a move/copy separator for the `start:len:dest` CLI string:
/// appends `:sep` when a separator is set, nothing when empty.
pub(crate) fn fmt_sep(sep: &str) -> String {
    if sep.is_empty() {
        String::new()
    } else {
        format!(":{sep}")
    }
}

/// The clap value name for a `ValueEnum` variant (kebab-case, e.g.
/// `TitleEnhanced` → `title-enhanced`), so display CLI strings round-trip
/// through clap parsing. Falls back to lowercased Debug output.
pub fn clap_value<T: clap::ValueEnum + std::fmt::Debug>(v: T) -> String {
    v.to_possible_value()
        .map(|p| p.get_name().to_string())
        .unwrap_or_else(|| format!("{v:?}").to_lowercase())
}

/// Parse the `start:len:dest[:sep]` value used by `--move-part`/`--copy-part`,
/// the TUI field editor, and the config display. `end` as the destination
/// means "to the very end" (-1). The single source of truth for the format.
pub fn parse_move_copy_value(s: &str) -> Option<MoveCopyValue> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 || parts.len() == 4 {
        let start = parts[0].parse().ok()?;
        let length = parts[1].parse().ok()?;
        let destination = if parts[2] == "end" {
            -1_isize
        } else {
            parts[2].parse().ok()?
        };
        let separator = parts
            .get(3)
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
        Some(MoveCopyValue {
            start,
            length,
            destination,
            separator,
        })
    } else {
        None
    }
}

// One file per group of rename-option sections; each fn appends the
// active (non-default) fields of its section to `items`.
mod active_flags;
mod active_numbering;
mod active_replace;

use active_flags::{
    active_fields_case_exception, active_fields_date_position, active_fields_exec_flags,
    active_fields_filters, active_fields_keep_structure, active_fields_name_mode,
    active_fields_name_segment, active_fields_output, active_fields_regex_mode,
};
use active_numbering::{
    active_fields_date, active_fields_dirname, active_fields_exclude, active_fields_extension,
    active_fields_metadata, active_fields_move_copy, active_fields_numbering,
};
use active_replace::{
    active_fields_add, active_fields_case, active_fields_regex, active_fields_remove,
    active_fields_replace,
};

pub fn config_active_fields(config: &RenameConfig) -> Vec<(String, String, String)> {
    let c = config;
    let mut items = Vec::new();
    active_fields_replace(c, &mut items);
    active_fields_regex(c, &mut items);
    active_fields_case(c, &mut items);
    active_fields_remove(c, &mut items);
    active_fields_add(c, &mut items);
    active_fields_numbering(c, &mut items);
    active_fields_extension(c, &mut items);
    active_fields_dirname(c, &mut items);
    active_fields_move_copy(c, &mut items);
    active_fields_date(c, &mut items);
    active_fields_exclude(c, &mut items);
    active_fields_metadata(c, &mut items);
    active_fields_exec_flags(c, &mut items);
    active_fields_filters(c, &mut items);
    active_fields_name_mode(c, &mut items);
    active_fields_case_exception(c, &mut items);
    active_fields_regex_mode(c, &mut items);
    active_fields_name_segment(c, &mut items);
    active_fields_date_position(c, &mut items);
    active_fields_keep_structure(c, &mut items);
    active_fields_output(c, &mut items);
    items
}

/// Parse a numeric field value. Empty clears the field to its default; a
/// non-empty value that does not parse is an error (never silently 0).
fn field_isize(key: &str, v: &str) -> Result<isize, String> {
    let t = v.trim();
    if t.is_empty() {
        return Ok(0);
    }
    t.parse()
        .map_err(|_| format!("{key}: invalid number '{v}'"))
}

/// Parse an unsigned numeric field value. Empty clears; bad input errors.
fn field_usize(key: &str, v: &str) -> Result<usize, String> {
    let t = v.trim();
    if t.is_empty() {
        return Ok(0);
    }
    t.parse()
        .map_err(|_| format!("{key}: invalid number '{v}'"))
}

/// Set a named config field from a text value (TUI inline editing).
///
/// Returns an error, labelled with the field, when the value could not be stored
/// as given: an unparseable number or an unknown enum label (the field is left
/// untouched), or a value the shared validation had to reset or clamp (the field
/// then holds the corrected value). Both are reported so an edit that did not
/// survive as typed can never look like a successful one.
pub fn set_config_field(cfg: &mut RenameConfig, key: &str, value: &str) -> Result<(), String> {
    let v = value.to_string();
    // "Add Date (suffix)"-style labels carry the date position with them.
    let suffix_label = key.ends_with(" (suffix)");
    let key = key.trim_end_matches(" (suffix)");
    match key {
        "Prefix" => cfg.add.add_prefix = Some(v).filter(|s| !s.is_empty()),
        "Suffix" => cfg.add.add_suffix = Some(v).filter(|s| !s.is_empty()),
        "Replace" => cfg.replace.replace = Some(v).filter(|s| !s.is_empty()),
        "Regex" => cfg.regex.regex_match = Some(v).filter(|s| !s.is_empty()),
        "Remove First" => cfg.remove.remove_first = Some(field_isize(key, &v)?),
        "Remove Last" => cfg.remove.remove_last = Some(field_isize(key, &v)?),
        "Remove Chars" => cfg.remove.remove_chars = Some(v).filter(|s| !s.is_empty()),
        "Remove Words" => cfg.remove.remove_words = Some(v).filter(|s| !s.is_empty()),
        "Remove Crop" => cfg.remove.remove_crop = Some(v).filter(|s| !s.is_empty()),
        "Remove Lead Dots" => {
            cfg.remove.remove_lead_dots = Some(match v.as_str() {
                "." => LeadDotsMode::Single,
                ".." => LeadDotsMode::Double,
                "Both" => LeadDotsMode::Both,
                other => return Err(format!("{key}: unknown value '{other}'")),
            });
        }
        "Insert" => cfg.add.add_insert = Some(v).filter(|s| !s.is_empty()),
        "Remove From" => cfg.remove.remove_from = Some(field_isize(key, &v)?),
        "Remove To" => cfg.remove.remove_to = Some(field_isize(key, &v)?),
        "Insert At" => cfg.add.add_at = Some(field_isize(key, &v)?),
        "Add Date" => {
            cfg.auto_date.add_date = Some(v).filter(|s| !s.is_empty());
            if suffix_label {
                cfg.auto_date.date_position = DatePosition::Suffix;
            }
        }
        "Add File Date" => {
            cfg.auto_date.add_file_date = Some(v).filter(|s| !s.is_empty());
            if suffix_label {
                cfg.auto_date.date_position = DatePosition::Suffix;
            }
        }
        "Exclude Regex" => cfg.filters.exclude_regex = Some(v).filter(|s| !s.is_empty()),
        "Insert Meta" => cfg.auto_date.insert_meta = Some(v).filter(|s| !s.is_empty()),
        "Set Attributes" => cfg.special.set_attributes = Some(v).filter(|s| !s.is_empty()),
        "Dirname Sep" => cfg.append_folder.add_dirname_sep = Some(v).filter(|s| !s.is_empty()),
        "Num Sep" => cfg.numbering.numbering_sep = Some(v).filter(|s| !s.is_empty()),
        "Num At" => cfg.numbering.numbering_at = Some(field_isize(key, &v)?),
        "Num Break" => cfg.numbering.numbering_break = Some(field_usize(key, &v)?),
        "Num Type" => cfg.numbering.numbering_type = Some(v).filter(|s| !s.is_empty()),
        "Num Case" => cfg.numbering.numbering_case = Some(v).filter(|s| !s.is_empty()),
        "Num Source" => {
            cfg.numbering.numbering_source = Some(match v.to_lowercase().as_str() {
                "name" => NumberingSource::Name,
                "date" => NumberingSource::Date,
                "size" => NumberingSource::Size,
                other => return Err(format!("{key}: unknown value '{other}'")),
            });
        }
        "Case Exception" => cfg.case.case_exception = Some(v).filter(|s| !s.is_empty()),
        "Case Name" => {
            cfg.case.case_name = Some(match v.to_lowercase().as_str() {
                "upper" => CaseMode::Upper,
                "lower" => CaseMode::Lower,
                "title" => CaseMode::Title,
                "title-enhanced" => CaseMode::TitleEnhanced,
                "sentence" => CaseMode::Sentence,
                "invert" => CaseMode::Invert,
                other => return Err(format!("{key}: unknown value '{other}'")),
            });
        }
        "Name Mode" => cfg.name.name_mode = Some(v).filter(|s| !s.is_empty()),
        "Name Value" => cfg.name.name_value = Some(v).filter(|s| !s.is_empty()),
        "Date Position" => {
            cfg.auto_date.date_position = match v.to_lowercase().as_str() {
                "prefix" => DatePosition::Prefix,
                "suffix" => DatePosition::Suffix,
                "none" => DatePosition::None,
                other => return Err(format!("{key}: unknown value '{other}'")),
            };
        }
        "Move Part" | "Copy Part" => {
            let mv = parse_move_copy_value(&v).ok_or_else(|| {
                format!("{key}: invalid value '{v}' (expected start:length:to[:sep])")
            })?;
            if key == "Move Part" {
                cfg.move_copy.move_part = Some(mv);
                cfg.move_copy.copy_part = None;
            } else {
                cfg.move_copy.copy_part = Some(mv);
                cfg.move_copy.move_part = None;
            }
        }
        "Num Start" | "Num Inc" | "Num Pad" | "Dirname Pos" | "Dirname Level" | "Min Name Len"
        | "Max Name Len" | "Min Path Len" | "Max Path Len" | "Filter Level" => {
            if key == "Num Inc" {
                cfg.numbering.numbering_increment = field_isize(key, &v)?;
            } else {
                let n = field_usize(key, &v)?;
                match key {
                    "Num Start" => cfg.numbering.numbering_start = n,
                    "Num Pad" => cfg.numbering.numbering_pad = n,
                    "Dirname Pos" => cfg.append_folder.add_dirname_pos = Some(n as isize),
                    "Dirname Level" => cfg.append_folder.dirname_level = n,
                    "Min Name Len" => cfg.filters.min_name_len = Some(n),
                    "Max Name Len" => cfg.filters.max_name_len = Some(n),
                    "Min Path Len" => cfg.filters.min_path_len = Some(n),
                    "Max Path Len" => cfg.filters.max_path_len = Some(n),
                    "Filter Level" => cfg.filters.filter_level = Some(n),
                    _ => {}
                }
            }
        }
        "Output Dir" => cfg.copy_to.output_dir = Some(v).filter(|s| !s.is_empty()),
        "Filter Attr" => cfg.filters.filter_attr = Some(v).filter(|s| !s.is_empty()),
        "Crop Mode" => match v.as_str() {
            "Before" => cfg.remove.crop_mode = Some(CropMode::Before),
            "After" => cfg.remove.crop_mode = Some(CropMode::After),
            "Special" => cfg.remove.crop_mode = Some(CropMode::Special),
            _ => cfg.remove.crop_mode = None,
        },
        _ => {}
    }
    // Enforce the shared limits/normalization so inline TUI edits cannot leave
    // the config in a state the GUI (or preset loading) would have cleaned up.
    // Report what it did: the reset/clamped value is stored, so the caller has to
    // be able to say that the typed value was not what the field now holds.
    let reset = cfg.sanitize();
    if reset.is_empty() {
        Ok(())
    } else {
        Err(reset.join("; "))
    }
}
