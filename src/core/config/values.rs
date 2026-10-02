//! Single source of truth for validating user-supplied option values.
//!
//! Each parser/normalizer here is reused in two places:
//!   * as a clap `value_parser` in `args.rs`, so the CLI **and** TUI reject bad
//!     input while parsing, before it can reach the config, and
//!   * by [`RenameConfig::validate`], so a preset that already contains an
//!     invalid value is reported instead of silently ignored.
//!
//! Keeping the accepted grammar next to the code that consumes it (`filter_path`,
//! `set_file_attributes`, `format_number`) keeps these two paths from drifting.

use super::*;
use crate::core::schema::{
    ATTR_ARCHIVE, ATTR_HIDDEN, ATTR_READONLY, ATTR_READONLY_ALIAS, ATTR_SYSTEM,
};

/// Valid `--set-attributes` names (see `set_file_attributes`).
const VALID_ATTRIBUTES: &[&str] = &[
    ATTR_READONLY,
    ATTR_READONLY_ALIAS,
    ATTR_HIDDEN,
    ATTR_SYSTEM,
    ATTR_ARCHIVE,
];

/// Valid `--filter-attr` values (see `filter_path`).
const VALID_FILTER_ATTRS: &[&str] = &[
    ATTR_READONLY,
    ATTR_READONLY_ALIAS,
    ATTR_HIDDEN,
    ATTR_SYSTEM,
    ATTR_ARCHIVE,
    "file",
    "dir",
    "directory",
    "folder",
];

/// Named `--numbering-type` values other than a numeric base 2–36
/// (see `format_number`).
const VALID_NUMBERING_TYPES: &[&str] = &[
    "decimal", "hex", "octal", "az_upper", "az_lower", "alpha", "roman",
];

/// Where an Auto Date / Insert Meta timestamp comes from.
///
/// The two tables below are the single source of truth for this: validation,
/// the GUI's Type combo, the EXIF gate, and the pipeline all read them, so a key
/// that validation accepts cannot be one the engine does not implement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaSource {
    /// The file's EXIF capture date (`FileMeta::exif_date`).
    Exif,
    /// The file's modification time.
    Mtime,
    /// The file's creation time.
    Created,
    /// The file's access time.
    Accessed,
    /// The wall clock at run time.
    Clock,
}

/// SSoT for the Auto Date `Type` combo and the `auto_date_type` keys it writes:
/// `(key, display label, time source)`, in combo order.
///
/// The EXIF-backed keys are named after the EXIF timestamp they intend to read,
/// but all of them resolve through the one date `FileMeta` carries (see
/// `read_exif_date`).
///
/// A key here must mean the same thing in [`INSERT_META_TAGS`] when it appears in
/// both: `taken_modified` is the Insert Meta spelling of the file's modification
/// time (an alias of `modified`), so it is mtime-backed here too.
pub const AUTO_DATE_TYPES: &[(&str, &str, MetaSource)] = &[
    ("creation_curr", "Creation (Curr.)", MetaSource::Created),
    ("creation_new", "Creation (New)", MetaSource::Created),
    ("modified_curr", "Modified (Curr.)", MetaSource::Mtime),
    ("modified_new", "Modified (New)", MetaSource::Mtime),
    ("accessed_curr", "Accessed (Curr.)", MetaSource::Accessed),
    ("accessed_new", "Accessed (New)", MetaSource::Accessed),
    ("taken_original", "Taken (Original)", MetaSource::Exif),
    ("taken_digitized", "Taken (Digitized)", MetaSource::Exif),
    ("taken_modified", "Taken (Modified)", MetaSource::Mtime),
    ("taken_recent", "Taken (Recent)", MetaSource::Exif),
    ("current", "Current", MetaSource::Clock),
];

/// SSoT for the `auto_date.insert_meta` tag list and the `--insert-meta` values:
/// `(tag, time source)`. This is exactly the set the engine implements.
///
/// The EXIF-backed tags fall back to the file's mtime when it has no EXIF date
/// (see `apply_insert_meta`).
pub const INSERT_META_TAGS: &[(&str, MetaSource)] = &[
    ("exif-date", MetaSource::Exif),
    ("taken_original", MetaSource::Exif),
    ("taken_digitized", MetaSource::Exif),
    ("taken_recent", MetaSource::Exif),
    ("modified", MetaSource::Mtime),
    ("taken_modified", MetaSource::Mtime),
];

/// The time source behind an `auto_date_type` key, if it is a known key.
pub fn auto_date_type_source(key: &str) -> Option<MetaSource> {
    AUTO_DATE_TYPES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, source)| *source)
}

/// The Auto Date `Type` key backed by `source`, if the combo has one.
pub fn auto_date_type_for_source(source: MetaSource) -> Option<&'static str> {
    AUTO_DATE_TYPES
        .iter()
        .find(|(_, _, s)| *s == source)
        .map(|(key, _, _)| *key)
}

/// The Auto Date `Type` key to show for an Insert Meta `tag`, for a frontend that
/// has to display a tag in the Type combo.
///
/// A tag that is also a Type key (`taken_original`) is returned unchanged. The
/// two that are not are tag spellings of a Type entry — `exif-date` is the EXIF
/// capture date that `taken_original` also names, and `modified` is the file's
/// modification time — so they resolve to the first Type entry with the same
/// source. Returns `None` for an unknown tag.
pub fn auto_date_type_for_tag(tag: &str) -> Option<&'static str> {
    if let Some(i) = auto_date_type_index(tag) {
        return Some(AUTO_DATE_TYPES[i].0);
    }
    auto_date_type_for_source(insert_meta_tag_source(tag)?)
}

/// The position of an `auto_date_type` key in the Type combo.
pub fn auto_date_type_index(key: &str) -> Option<usize> {
    AUTO_DATE_TYPES.iter().position(|(k, _, _)| *k == key)
}

/// The time source behind an `insert_meta` tag, if it is a known tag.
pub fn insert_meta_tag_source(tag: &str) -> Option<MetaSource> {
    INSERT_META_TAGS
        .iter()
        .find(|(t, _)| *t == tag)
        .map(|(_, source)| *source)
}

/// True when an `auto_date_type` key makes the pipeline read the EXIF date.
///
/// SSoT for that key set: the EXIF gate and `date_type_time_source` both route
/// on this, so they cannot drift apart and silently drop a date.
pub fn auto_date_type_uses_exif(key: &str) -> bool {
    auto_date_type_source(key) == Some(MetaSource::Exif)
}

/// True when an `Insert Meta` tag makes the pipeline read the EXIF date.
///
/// SSoT for that tag set: [`apply_insert_meta`](crate::core::rename::apply_insert_meta)
/// routes on this same predicate, so the behaviour and the EXIF gate cannot
/// drift apart and silently drop a date.
pub fn insert_meta_tag_uses_exif(tag: &str) -> bool {
    insert_meta_tag_source(tag) == Some(MetaSource::Exif)
}

/// True when a date key is stored as `auto_date.insert_meta` rather than as the
/// `auto_date_type` of an `Add Date`.
///
/// The EXIF-backed Auto Date keys are named after the Insert Meta tag they are
/// equivalent to, so they are tags too; every other tag is an explicit Insert
/// Meta choice. Routing on this (rather than on a `taken_` name prefix) covers
/// `exif-date` and `modified` as well, which the prefix test silently sent down
/// the `Add Date` path — turning them into a format string and losing the tag.
pub fn meta_key_is_insert_tag(key: &str) -> bool {
    insert_meta_tag_source(key).is_some() || auto_date_type_uses_exif(key)
}

/// Parse/normalize a `--set-attributes` value: a comma-separated list of
/// `[+|-]name` tokens. Unknown names are rejected instead of silently ignored.
pub fn parse_attributes_arg(input: &str) -> Result<String, String> {
    let mut out: Vec<String> = Vec::new();
    for raw in input.split(',') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let (sign, name) = if let Some(n) = token.strip_prefix('+') {
            ("+", n.trim())
        } else if let Some(n) = token.strip_prefix('-') {
            ("-", n.trim())
        } else {
            ("", token)
        };
        let lower = name.to_lowercase();
        if !VALID_ATTRIBUTES.contains(&lower.as_str()) {
            return Err(format!(
                "unknown attribute '{name}' (valid: readonly, hidden, system, archive)"
            ));
        }
        out.push(format!("{sign}{lower}"));
    }
    if out.is_empty() {
        return Err("no attributes given (valid: readonly, hidden, system, archive)".into());
    }
    Ok(out.join(","))
}

/// Parse/normalize a `--numbering-type` value.
pub fn parse_numbering_type_arg(input: &str) -> Result<String, String> {
    let t = input.trim().to_lowercase();
    if VALID_NUMBERING_TYPES.contains(&t.as_str()) {
        return Ok(t);
    }
    if let Ok(base) = t.parse::<u32>()
        && (2..=36).contains(&base)
    {
        return Ok(t);
    }
    Err(format!(
        "unknown numbering type '{input}' (valid: decimal, hex, octal, az_upper, az_lower, \
         alpha, roman, or a base 2-36)"
    ))
}

/// Parse/normalize a `--numbering-case` value.
pub fn parse_numbering_case_arg(input: &str) -> Result<String, String> {
    match input.trim().to_lowercase().as_str() {
        "upper" => Ok("upper".into()),
        "lower" => Ok("lower".into()),
        other => Err(format!(
            "unknown numbering case '{other}' (valid: upper, lower)"
        )),
    }
}

/// Parse/normalize a `--crop` value (`before:TEXT`, `after:TEXT`, `special:TEXT`).
pub fn parse_crop_arg(input: &str) -> Result<String, String> {
    for prefix in ["before:", "after:", "special:"] {
        if let Some(v) = input.strip_prefix(prefix) {
            return if v.is_empty() {
                Err(format!("crop '{prefix}' requires a value"))
            } else {
                Ok(input.to_string())
            };
        }
    }
    Err(format!(
        "crop '{input}' must start with before:, after:, or special:"
    ))
}

/// Parse/normalize an `--insert-meta` tag.
///
/// Exactly the tags the engine implements: an unknown tag (or an unimplemented
/// `exif:<Tag>`) is a silent no-op in `apply_insert_meta`, so it is rejected
/// while parsing instead of reaching the config.
pub fn parse_insert_meta_arg(input: &str) -> Result<String, String> {
    if insert_meta_tag_source(input).is_some() {
        Ok(input.to_string())
    } else {
        let valid = INSERT_META_TAGS
            .iter()
            .map(|(tag, _)| *tag)
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!("unknown metadata tag '{input}' (valid: {valid})"))
    }
}

/// Parse/normalize a `--filter-attr` value.
pub fn parse_filter_attr_arg(input: &str) -> Result<String, String> {
    let t = input.trim().to_lowercase();
    if VALID_FILTER_ATTRS.contains(&t.as_str()) {
        Ok(t)
    } else {
        Err(format!(
            "unknown filter attribute '{input}' (valid: readonly, hidden, file, dir)"
        ))
    }
}

impl RenameConfig {
    /// Validate the config **in place**, replacing out-of-domain values with
    /// safe defaults (`None` = "feature off") and normalizing accepted but
    /// non-canonical values. Returns the list of values that were reset (empty
    /// when the config was already clean); normalization is silent.
    ///
    /// Only **domain/enum** values are sanitized. Pattern fields
    /// (`regex_match`, `exclude_regex`, `filter_pattern`) are deliberately left
    /// untouched: the engine tolerates a pattern that fails to compile (it
    /// fails open or skips it), so the text is a legitimate stored state and
    /// must survive a load.
    ///
    /// This is the single implementation: [`RenameConfig::validate`] reports
    /// what this would reset, and config import calls it to *silently clean*
    /// a hand-edited or legacy file instead of failing the load.
    pub fn sanitize(&mut self) -> Vec<String> {
        let mut reset = Vec::new();

        for (field, slot) in [
            ("set-created", &mut self.special.set_created),
            ("set-modified", &mut self.special.set_modified),
            ("set-accessed", &mut self.special.set_accessed),
        ] {
            let problem = match slot {
                Some(TimestampSpec::Fixed(v)) => crate::core::rename::normalize_local_datetime(v)
                    .is_none()
                    .then(|| format!("invalid datetime '{v}'")),
                Some(TimestampSpec::CopyFrom(f))
                    if !["created", "modified", "accessed"].contains(&f.as_str()) =>
                {
                    Some(format!("invalid copy-from field '{f}'"))
                }
                _ => None,
            };
            if let Some(problem) = problem {
                reset.push(format!("{field}: {problem} (reset to default)"));
                *slot = None;
            }
            if let Some(TimestampSpec::Delta(secs)) = slot {
                let clamped = (*secs).clamp(-TIMESTAMP_DELTA_MAX_SECS, TIMESTAMP_DELTA_MAX_SECS);
                if clamped != *secs {
                    reset.push(format!("{field}: delta {secs}s out of range (clamped)"));
                    *secs = clamped;
                }
            }
        }

        if let Some(attrs) = self.special.set_attributes.take() {
            match parse_attributes_arg(&attrs) {
                Ok(normalized) => self.special.set_attributes = Some(normalized),
                Err(e) => reset.push(format!("set-attributes: {e} (reset to default)")),
            }
        }
        if let Some(t) = self.numbering.numbering_type.take() {
            match parse_numbering_type_arg(&t) {
                Ok(normalized) => self.numbering.numbering_type = Some(normalized),
                Err(e) => reset.push(format!("numbering-type: {e} (reset to default)")),
            }
        }
        if let Some(c) = self.numbering.numbering_case.take() {
            match parse_numbering_case_arg(&c) {
                Ok(normalized) => self.numbering.numbering_case = Some(normalized),
                Err(e) => reset.push(format!("numbering-case: {e} (reset to default)")),
            }
        }
        if let Some(a) = self.filters.filter_attr.take() {
            match parse_filter_attr_arg(&a) {
                Ok(normalized) => self.filters.filter_attr = Some(normalized),
                Err(e) => reset.push(format!("filter-attr: {e} (reset to default)")),
            }
        }

        // Free-form mode/tag strings: an unknown value would be a silent no-op
        // (`make_name_items` / `apply_insert_meta` fall through), so validate
        // them against the accepted set instead of storing dead config.
        if let Some(mode) = self.name.name_mode.take() {
            const VALID: &[&str] = &[
                "keep",
                "fixed",
                "remove",
                "reverse",
                "pad_numbers",
                "reformat_date",
            ];
            let normalized = mode.trim().to_ascii_lowercase();
            if VALID.contains(&normalized.as_str()) {
                self.name.name_mode = Some(normalized);
            } else {
                reset.push(format!(
                    "name-mode: unknown mode '{mode}' (reset to default)"
                ));
            }
        }
        if let Some(tag) = self.auto_date.insert_meta.take() {
            // Exactly the tags the engine implements. An unknown tag — including
            // an unimplemented `exif:<Tag>` — is a silent no-op in
            // `apply_insert_meta`, so report it instead of storing dead config.
            if insert_meta_tag_source(&tag).is_some() {
                self.auto_date.insert_meta = Some(tag);
            } else {
                reset.push(format!(
                    "insert-meta: unknown tag '{tag}' (reset to default)"
                ));
            }
        }
        if let Some(t) = self.auto_date.auto_date_type.take() {
            let normalized = t.trim().to_ascii_lowercase();
            if auto_date_type_source(&normalized).is_some() {
                self.auto_date.auto_date_type = Some(normalized);
            } else {
                reset.push(format!(
                    "auto-date-type: unknown key '{t}' (reset to default)"
                ));
            }
        }

        // Empty string means "unset" everywhere else: `opt_str_field_default`,
        // the GUI/TUI field setters, and `active_fields_output` all treat it as
        // absent. Execution, however, branches on `output_dir.is_some()`, so a
        // preset that smuggles in `Some("")` would be shown as unmodified yet
        // enable copy mode and resolve the target to the cwd. Normalize it.
        // A path that simply doesn't exist yet is left alone: it is created at
        // rename time, so it is a legitimate stored value.
        if self.copy_to.output_dir.as_deref() == Some("") {
            self.copy_to.output_dir = None;
        }

        // Pattern fields (`regex_match`, `exclude_regex`, `filter_pattern`) are
        // intentionally **not** validated here. A pattern that fails to compile
        // is a legitimate stored state that the runtime preserves:
        //   * `mask_matches` / `exclude_regex_passes` (core::scan) fail open,
        //     so an unparseable mask or exclude never filters anything;
        //   * `CompiledConfig::new` / `run_items` skip an uncompilable rename
        //     regex (`.ok()` -> None), leaving the name unchanged.
        // Clearing the text would silently delete user input (e.g. a
        // half-typed pattern), so sanitize leaves these fields alone.

        // Stacking mode stores operations in `command_order`; the numbering
        // item carries its own type/case.
        for item in &mut self.command_order {
            if let RenameItem::Numbering(_, _, _, _, _, _, typ, case, _) = item {
                if let Some(t) = typ.take() {
                    match parse_numbering_type_arg(&t) {
                        Ok(normalized) => *typ = Some(normalized),
                        Err(e) => reset.push(format!(
                            "command-order numbering-type: {e} (reset to default)"
                        )),
                    }
                }
                if let Some(c) = case.take() {
                    match parse_numbering_case_arg(&c) {
                        Ok(normalized) => *case = Some(normalized),
                        Err(e) => reset.push(format!(
                            "command-order numbering-case: {e} (reset to default)"
                        )),
                    }
                }
            }
        }

        // Clamp numeric fields to the GUI widget ranges (the reference limits).
        // The GUI cannot produce out-of-range values, so clamping here keeps
        // CLI/TUI flags and hand-edited presets consistent with it.
        clamp_opt(&mut self.add.add_at, ADD_AT, "add-at", &mut reset);
        clamp_val(
            &mut self.append_folder.dirname_level,
            DIRNAME_LEVEL,
            "dirname-level",
            &mut reset,
        );
        clamp_opt(
            &mut self.auto_date.auto_date_offset,
            AUTO_DATE_OFFSET,
            "auto-date-offset",
            &mut reset,
        );
        clamp_opt(
            &mut self.filters.min_name_len,
            NAME_LEN,
            "min-name-len",
            &mut reset,
        );
        clamp_opt(
            &mut self.filters.max_name_len,
            NAME_LEN,
            "max-name-len",
            &mut reset,
        );
        clamp_opt(
            &mut self.filters.min_path_len,
            PATH_LEN,
            "min-path-len",
            &mut reset,
        );
        clamp_opt(
            &mut self.filters.max_path_len,
            PATH_LEN,
            "max-path-len",
            &mut reset,
        );
        clamp_opt(
            &mut self.filters.filter_level,
            FILTER_LEVEL,
            "filter-level",
            &mut reset,
        );
        clamp_move_copy(&mut self.move_copy.move_part, "move-part", &mut reset);
        clamp_move_copy(&mut self.move_copy.copy_part, "copy-part", &mut reset);
        // Destination 0 is documented as "invalid/no destination" (the `end`
        // keyword maps to -1), so drop the whole value rather than keeping a
        // config that can never apply.
        if self
            .move_copy
            .move_part
            .as_ref()
            .is_some_and(|v| v.destination == 0)
        {
            reset.push("move-part: destination 0 is invalid (reset to default)".into());
            self.move_copy.move_part = None;
        }
        if self
            .move_copy
            .copy_part
            .as_ref()
            .is_some_and(|v| v.destination == 0)
        {
            reset.push("copy-part: destination 0 is invalid (reset to default)".into());
            self.move_copy.copy_part = None;
        }
        clamp_val(
            &mut self.name_segment.copy_name_segment_from,
            NAME_SEGMENT,
            "name-segment-from",
            &mut reset,
        );
        clamp_val(
            &mut self.name_segment.copy_name_segment_to,
            NAME_SEGMENT,
            "name-segment-to",
            &mut reset,
        );
        clamp_opt(
            &mut self.numbering.numbering_at,
            NUMBERING_AT,
            "numbering-at",
            &mut reset,
        );
        clamp_val(
            &mut self.numbering.numbering_start,
            NUMBERING_START,
            "numbering-start",
            &mut reset,
        );
        clamp_val(
            &mut self.numbering.numbering_increment,
            NUMBERING_INCREMENT,
            "numbering-increment",
            &mut reset,
        );
        clamp_val(
            &mut self.numbering.numbering_pad,
            NUMBERING_PAD,
            "numbering-pad",
            &mut reset,
        );
        clamp_opt(
            &mut self.numbering.numbering_break,
            NUMBERING_BREAK,
            "numbering-break",
            &mut reset,
        );
        clamp_val(
            &mut self.special.timestamp_incr_secs,
            TIMESTAMP_INCR_SECS,
            "timestamp-incr-secs",
            &mut reset,
        );

        reset
    }
}

/// Clamp a value to `range`, recording a message when it was out of bounds.
fn clamp_val<T>(value: &mut T, range: (T, T), name: &str, reset: &mut Vec<String>)
where
    T: Ord + Copy + std::fmt::Display,
{
    let clamped = (*value).clamp(range.0, range.1);
    if clamped != *value {
        reset.push(format!(
            "{name}: {value} out of range {}..={} (clamped)",
            range.0, range.1
        ));
        *value = clamped;
    }
}

/// Clamp an optional value, if present.
fn clamp_opt<T>(value: &mut Option<T>, range: (T, T), name: &str, reset: &mut Vec<String>)
where
    T: Ord + Copy + std::fmt::Display,
{
    if let Some(inner) = value {
        clamp_val(inner, range, name, reset);
    }
}

/// Clamp a move/copy value's position, length, and destination.
fn clamp_move_copy(value: &mut Option<MoveCopyValue>, name: &str, reset: &mut Vec<String>) {
    if let Some(v) = value {
        clamp_val(
            &mut v.start,
            MOVE_COPY_VALUE,
            &format!("{name} start"),
            reset,
        );
        clamp_val(
            &mut v.length,
            MOVE_COPY_VALUE,
            &format!("{name} length"),
            reset,
        );
        clamp_val(
            &mut v.destination,
            MOVE_COPY_VALUE,
            &format!("{name} destination"),
            reset,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_attributes_normalizes_and_rejects() {
        assert_eq!(parse_attributes_arg("readonly").unwrap(), "readonly");
        assert_eq!(parse_attributes_arg("READONLY").unwrap(), "readonly");
        assert_eq!(parse_attributes_arg("+readonly").unwrap(), "+readonly");
        assert_eq!(parse_attributes_arg("-hidden").unwrap(), "-hidden");
        assert_eq!(
            parse_attributes_arg("readonly, hidden").unwrap(),
            "readonly,hidden"
        );
        assert!(parse_attributes_arg("bogus").is_err());
        assert!(parse_attributes_arg("").is_err());
        assert!(parse_attributes_arg("readonly,bogus").is_err());
    }

    #[test]
    fn parse_numbering_type_normalizes_and_rejects() {
        for ok in [
            "decimal", "hex", "octal", "az_upper", "az_lower", "alpha", "roman",
        ] {
            assert_eq!(parse_numbering_type_arg(ok).unwrap(), ok);
        }
        assert_eq!(parse_numbering_type_arg("2").unwrap(), "2");
        assert_eq!(parse_numbering_type_arg("36").unwrap(), "36");
        assert_eq!(parse_numbering_type_arg("HEX").unwrap(), "hex");
        for bad in ["1", "37", "0", "hexadecimal", ""] {
            assert!(
                parse_numbering_type_arg(bad).is_err(),
                "should reject {bad:?}"
            );
        }
    }

    #[test]
    fn parse_numbering_case_normalizes_and_rejects() {
        assert_eq!(parse_numbering_case_arg("UPPER").unwrap(), "upper");
        assert_eq!(parse_numbering_case_arg("lower").unwrap(), "lower");
        assert!(parse_numbering_case_arg("mixed").is_err());
    }

    #[test]
    fn parse_crop_requires_known_prefix() {
        assert_eq!(parse_crop_arg("before:prefix_").unwrap(), "before:prefix_");
        assert_eq!(parse_crop_arg("after:_suffix").unwrap(), "after:_suffix");
        assert_eq!(parse_crop_arg("special:keep").unwrap(), "special:keep");
        assert!(parse_crop_arg("prefix_").is_err());
        assert!(parse_crop_arg("before:").is_err());
    }

    #[test]
    fn parse_filter_attr_normalizes_and_rejects() {
        for ok in ["readonly", "hidden", "file", "dir", "directory", "folder"] {
            assert_eq!(parse_filter_attr_arg(ok).unwrap(), ok);
        }
        assert_eq!(parse_filter_attr_arg("Hidden").unwrap(), "hidden");
        assert!(parse_filter_attr_arg("bogus").is_err());
    }

    #[test]
    fn sanitize_accepts_default() {
        assert!(RenameConfig::default().sanitize().is_empty());
    }

    #[test]
    fn sanitize_reports_invalid_values() {
        let mut cfg = RenameConfig::default();
        cfg.special.set_modified = Some(TimestampSpec::Fixed("garbage".into()));
        cfg.special.set_created = Some(TimestampSpec::CopyFrom("bogus".into()));
        cfg.special.set_attributes = Some("nope".into());
        cfg.numbering.numbering_type = Some("hexadecimal".into());
        cfg.numbering.numbering_case = Some("mixed".into());
        cfg.filters.filter_attr = Some("nope".into());

        let errors = cfg.sanitize();
        assert_eq!(errors.len(), 6, "expected 6 errors, got {errors:?}");
        assert!(errors.iter().any(|e| e.starts_with("set-modified")));
        assert!(errors.iter().any(|e| e.starts_with("set-created")));
        assert!(errors.iter().any(|e| e.starts_with("set-attributes")));
        assert!(errors.iter().any(|e| e.starts_with("numbering-type")));
        assert!(errors.iter().any(|e| e.starts_with("numbering-case")));
        assert!(errors.iter().any(|e| e.starts_with("filter-attr")));
    }

    #[test]
    fn sanitize_checks_stacking_numbering() {
        let mut cfg = RenameConfig::default();
        cfg.command_order.push(RenameItem::Numbering(
            NumberingMode::Prefix,
            1,
            1,
            0,
            None,
            None,
            Some("hexadecimal".into()),
            Some("mixed".into()),
            None,
        ));
        let errors = cfg.sanitize();
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("numbering-type")));
        assert!(errors.iter().any(|e| e.contains("numbering-case")));
    }

    #[test]
    fn sanitize_resets_invalid_values_to_defaults() {
        let mut cfg = RenameConfig::default();
        cfg.special.set_modified = Some(TimestampSpec::Fixed("garbage".into()));
        cfg.special.set_created = Some(TimestampSpec::CopyFrom("bogus".into()));
        cfg.special.set_attributes = Some("nope".into());
        cfg.numbering.numbering_type = Some("hexadecimal".into());
        cfg.numbering.numbering_case = Some("mixed".into());
        cfg.filters.filter_attr = Some("nope".into());
        cfg.filters.exclude_regex = Some("(".into());
        cfg.regex.regex_match = Some("(".into());

        let reset = cfg.sanitize();
        assert_eq!(reset.len(), 6, "one per reset value: {reset:?}");
        assert_eq!(cfg.special.set_modified, None);
        assert_eq!(cfg.special.set_created, None);
        assert_eq!(cfg.special.set_attributes, None);
        assert_eq!(cfg.numbering.numbering_type, None);
        assert_eq!(cfg.numbering.numbering_case, None);
        assert_eq!(cfg.filters.filter_attr, None);
        // Non-compiling patterns are a legitimate stored state: the engine
        // fails open (or skips them), so sanitize must not clear the text.
        assert_eq!(cfg.filters.exclude_regex.as_deref(), Some("("));
        assert_eq!(cfg.regex.regex_match.as_deref(), Some("("));
        // A clean config is a no-op.
        assert!(RenameConfig::default().sanitize().is_empty());
    }

    #[test]
    fn sanitize_preserves_non_compiling_patterns() {
        let mut cfg = RenameConfig::default();
        // Rename regex: execution skips an uncompilable pattern (`.ok()`).
        cfg.regex.regex_match = Some("(".into());
        // Exclude regex: core::scan fails open (`unwrap_or(true)`).
        cfg.filters.exclude_regex = Some("(".into());
        // Filter mask (glob): core::scan fails open (`unwrap_or(true)`).
        cfg.filters.filter_pattern = Some("[".into());

        let reset = cfg.sanitize();
        assert!(reset.is_empty(), "patterns are never reset: {reset:?}");
        assert_eq!(cfg.regex.regex_match.as_deref(), Some("("));
        assert_eq!(cfg.filters.exclude_regex.as_deref(), Some("("));
        assert_eq!(cfg.filters.filter_pattern.as_deref(), Some("["));
    }

    #[test]
    fn sanitize_normalizes_empty_output_dir_but_keeps_paths() {
        let mut cfg = RenameConfig::default();
        cfg.copy_to.output_dir = Some(String::new());
        let reset = cfg.sanitize();
        assert!(reset.is_empty(), "normalization is silent: {reset:?}");
        assert_eq!(cfg.copy_to.output_dir, None, "empty means unset");

        // A path that doesn't exist yet is legitimate: it is created at rename
        // time, so sanitize must not clear it.
        let mut cfg = RenameConfig::default();
        cfg.copy_to.output_dir = Some("/no/such/dir/awara".into());
        assert!(cfg.sanitize().is_empty());
        assert_eq!(
            cfg.copy_to.output_dir.as_deref(),
            Some("/no/such/dir/awara")
        );
    }

    #[test]
    fn sanitize_clamps_numeric_fields_to_gui_limits() {
        let mut cfg = RenameConfig::default();
        cfg.add.add_at = Some(50_000);
        cfg.append_folder.dirname_level = 9_999;
        cfg.numbering.numbering_pad = 99;
        cfg.numbering.numbering_increment = 5_000;
        cfg.filters.filter_level = Some(1_000);
        cfg.name_segment.copy_name_segment_from = 123_456;
        cfg.move_copy.move_part = Some(MoveCopyValue {
            start: 99_999,
            length: -99_999,
            destination: 1,
            separator: None,
        });

        let reset = cfg.sanitize();
        assert!(!reset.is_empty(), "clamps are reported");
        assert_eq!(cfg.add.add_at, Some(ADD_AT.1));
        assert_eq!(cfg.append_folder.dirname_level, DIRNAME_LEVEL.1);
        assert_eq!(cfg.numbering.numbering_pad, NUMBERING_PAD.1);
        assert_eq!(cfg.numbering.numbering_increment, NUMBERING_INCREMENT.1);
        assert_eq!(cfg.filters.filter_level, Some(FILTER_LEVEL.1));
        assert_eq!(cfg.name_segment.copy_name_segment_from, NAME_SEGMENT.1);
        let mc = cfg.move_copy.move_part.as_ref().unwrap();
        assert_eq!(mc.start, MOVE_COPY_VALUE.1);
        assert_eq!(mc.length, MOVE_COPY_VALUE.0);
    }

    #[test]
    fn sanitize_rejects_unknown_name_mode_and_insert_meta() {
        let mut cfg = RenameConfig::default();
        cfg.name.name_mode = Some("fixd".into());
        cfg.auto_date.insert_meta = Some("nope".into());
        let reset = cfg.sanitize();
        assert_eq!(reset.len(), 2, "{reset:?}");
        assert_eq!(cfg.name.name_mode, None);
        assert_eq!(cfg.auto_date.insert_meta, None);
        // The `exif:<Tag>` family is not implemented by the engine, so it is
        // rejected like any other unknown tag instead of stored as a no-op.
        cfg.auto_date.insert_meta = Some("exif:DateTimeOriginal".into());
        let reset = cfg.sanitize();
        assert_eq!(reset.len(), 1, "{reset:?}");
        assert_eq!(cfg.auto_date.insert_meta, None);
        // Every tag the engine implements survives validation.
        for (tag, _) in INSERT_META_TAGS {
            cfg.auto_date.insert_meta = Some((*tag).to_string());
            assert!(cfg.sanitize().is_empty(), "{tag} must be accepted");
            assert_eq!(cfg.auto_date.insert_meta.as_deref(), Some(*tag));
        }
    }

    /// The key tables are the SSoT for validation, the GUI combo, the engine,
    /// and the EXIF gate, so the lookups must agree and a key may not mean one
    /// thing as an `auto_date_type` and another as an `insert_meta` tag.
    #[test]
    fn meta_source_tables_agree() {
        for (i, (key, _, source)) in AUTO_DATE_TYPES.iter().enumerate() {
            assert!(
                !AUTO_DATE_TYPES[..i].iter().any(|(k, _, _)| *k == *key),
                "duplicate auto-date-type key {key}"
            );
            assert_eq!(auto_date_type_index(key), Some(i), "{key}: combo index");
            assert_eq!(auto_date_type_source(key), Some(*source), "{key}: source");
            assert_eq!(
                auto_date_type_uses_exif(key),
                *source == MetaSource::Exif,
                "{key}: exif flag"
            );
        }
        for (i, (tag, source)) in INSERT_META_TAGS.iter().enumerate() {
            assert!(
                !INSERT_META_TAGS[..i].iter().any(|(t, _)| *t == *tag),
                "duplicate insert-meta tag {tag}"
            );
            assert_eq!(insert_meta_tag_source(tag), Some(*source), "{tag}: source");
            assert_eq!(
                insert_meta_tag_uses_exif(tag),
                *source == MetaSource::Exif,
                "{tag}: exif flag"
            );
            // A name in both tables must resolve to the same timestamp.
            if let Some(other) = auto_date_type_source(tag) {
                assert_eq!(other, *source, "{tag} means two different things");
            }
            // Every tag is stored as `insert_meta`, never as an Add Date
            // format string.
            assert!(meta_key_is_insert_tag(tag), "{tag}: insert_meta routing");
        }
        // Unknown keys are nothing at all — and the parsers must reject them
        // too, so a bad flag never reaches the config as a silent no-op.
        for unknown in ["nope", "exif:DateTimeOriginal", ""] {
            assert_eq!(auto_date_type_source(unknown), None, "{unknown}");
            assert_eq!(insert_meta_tag_source(unknown), None, "{unknown}");
            assert!(!meta_key_is_insert_tag(unknown), "{unknown}");
            assert!(parse_insert_meta_arg(unknown).is_err(), "{unknown}");
        }
        for (tag, _) in INSERT_META_TAGS {
            assert_eq!(
                parse_insert_meta_arg(tag).as_deref(),
                Ok(*tag),
                "{tag} must parse"
            );
        }
        // The non-EXIF Auto Date keys stay Add Dates, including the ones the
        // old `taken_` prefix test could not tell apart.
        for key in ["current", "creation_curr", "modified_new"] {
            assert!(!meta_key_is_insert_tag(key), "{key} is an Add Date");
        }
        // And the Insert Meta spellings the prefix test missed are tags.
        assert!(meta_key_is_insert_tag("exif-date"));
        assert!(meta_key_is_insert_tag("modified"));
    }

    /// A frontend showing a tag in the Type combo must land on an entry the combo
    /// actually lists, and on one that reads the same timestamp.
    #[test]
    fn every_insert_meta_tag_has_an_equivalent_type_entry() {
        for (tag, source) in INSERT_META_TAGS {
            let ty = auto_date_type_for_tag(tag).unwrap_or_else(|| panic!("{tag}: no Type"));
            assert_eq!(
                auto_date_type_source(ty),
                Some(*source),
                "{tag} → {ty} must read the same timestamp"
            );
            assert!(
                auto_date_type_index(ty).is_some(),
                "{tag} → {ty} must be a combo entry"
            );
        }
        // A tag that is itself a Type key stays itself, rather than being folded
        // onto the first entry that shares its source.
        assert_eq!(
            auto_date_type_for_tag("taken_original"),
            Some("taken_original")
        );
        assert_eq!(
            auto_date_type_for_tag("taken_modified"),
            Some("taken_modified")
        );
        assert_eq!(auto_date_type_for_tag("exif-date"), Some("taken_original"));
        assert_eq!(auto_date_type_for_tag("modified"), Some("modified_curr"));
        assert_eq!(auto_date_type_for_tag("nope"), None);
    }

    #[test]
    fn sanitize_resets_move_copy_with_destination_zero() {
        let mut cfg = RenameConfig::default();
        cfg.move_copy.move_part = Some(MoveCopyValue {
            start: 1,
            length: 2,
            destination: 0,
            separator: None,
        });
        cfg.move_copy.copy_part = Some(MoveCopyValue {
            start: 1,
            length: 2,
            destination: 0,
            separator: None,
        });
        let reset = cfg.sanitize();
        assert_eq!(reset.len(), 2, "{reset:?}");
        assert!(cfg.move_copy.move_part.is_none());
        assert!(cfg.move_copy.copy_part.is_none());
    }

    #[test]
    fn sanitize_validates_auto_date_type_and_timestamp_bounds() {
        let mut cfg = RenameConfig::default();
        cfg.auto_date.auto_date_type = Some("nope".into());
        cfg.special.timestamp_incr_secs = 99_999_999;
        cfg.special.set_created = Some(TimestampSpec::Delta(i64::MAX));
        let reset = cfg.sanitize();
        assert_eq!(cfg.auto_date.auto_date_type, None);
        assert_eq!(cfg.special.timestamp_incr_secs, TIMESTAMP_INCR_SECS.1);
        assert_eq!(
            cfg.special.set_created,
            Some(TimestampSpec::Delta(TIMESTAMP_DELTA_MAX_SECS))
        );
        assert!(reset.len() >= 3, "{reset:?}");
    }

    #[test]
    fn sanitize_normalizes_valid_values_silently() {
        let mut cfg = RenameConfig::default();
        cfg.numbering.numbering_type = Some("HEX".into());
        cfg.numbering.numbering_case = Some("UPPER".into());
        cfg.special.set_attributes = Some("readonly,HIDDEN".into());

        let reset = cfg.sanitize();
        assert!(reset.is_empty(), "normalization is not reported: {reset:?}");
        assert_eq!(cfg.numbering.numbering_type.as_deref(), Some("hex"));
        assert_eq!(cfg.numbering.numbering_case.as_deref(), Some("upper"));
        assert_eq!(
            cfg.special.set_attributes.as_deref(),
            Some("readonly,hidden")
        );
    }
}
