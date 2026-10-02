use super::*;

/// Deserialize a `usize` that may be stored as either a JSON number or a JSON string
/// (for backward compatibility with saved state that used `String` fields).
pub fn deserialize_usize_from_string<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrUsize {
        String(String),
        Usize(usize),
    }
    match StringOrUsize::deserialize(deserializer)? {
        StringOrUsize::String(s) => s.parse().map_err(serde::de::Error::custom),
        StringOrUsize::Usize(u) => Ok(u),
    }
}

/// Helper for `#[serde(skip_serializing_if)]` on `Option<T>` fields.
/// Skips serialization when the option is `None`.
pub(crate) fn is_none<T>(v: &Option<T>) -> bool {
    v.is_none()
}

/// Helper for `#[serde(skip_serializing_if)]` on `bool` fields.
/// Skips serialization when the bool is `false`.
pub(crate) fn is_false(v: &bool) -> bool {
    !v
}

/// Helper for `#[serde(skip_serializing_if)]` on numeric fields that should be
/// omitted when they are equal to the provided default.
pub(crate) fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// True when an optional numeric config field is at its default value: unset
/// (`None`) or equal to the field's default (`0` for numeric fields). The GUI
/// stores 0 instead of clearing to `None`, so change detection
/// (`section_is_modified`), the active-fields list, and serialization share
/// this as the single source of truth for "not configured".
pub fn opt_num_default<T: PartialEq + Default>(v: Option<T>) -> bool {
    v.is_none() || v == Some(T::default())
}

/// serde-compatible form of [`opt_num_default`] for `skip_serializing_if`:
/// omits the field when it is at its default value (None or 0), so saved
/// state and presets stay free of default-valued zeros.
pub fn opt_num_default_ref<T: PartialEq + Default>(v: &Option<T>) -> bool {
    match v {
        None => true,
        Some(x) => *x == T::default(),
    }
}

/// True when an optional text field is at its default value: unset (`None`)
/// or an empty string. Mirrors [`opt_num_default`] for text fields, so change
/// detection, serialization, and the active-fields list share one rule for
/// "not configured".
pub fn opt_str_field_default(v: &Option<String>) -> bool {
    v.as_deref().is_none_or(|s| s.is_empty())
}

pub(crate) fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

/// Helper for `#[serde(skip_serializing_if)]` on section struct fields.
/// Skips serialization when the section is all-default (nested sections
/// with `is_default` won't work for structs with custom Default impls,
/// so we implement this for each section type).
///
/// Deserialize an `isize` that may be stored as either a JSON number or a JSON string.
pub fn deserialize_isize_from_string<'de, D>(deserializer: D) -> Result<isize, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrIsize {
        String(String),
        Isize(isize),
    }
    match StringOrIsize::deserialize(deserializer)? {
        StringOrIsize::String(s) => s.parse().map_err(serde::de::Error::custom),
        StringOrIsize::Isize(u) => Ok(u),
    }
}
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct FileMeta {
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub exif_date: Option<i64>,
}

/// Convert an `exif::DateTime` to a `SystemTime` (epoch seconds).
/// Returns `None` if the date is out of range.
pub(crate) fn exif_dt_to_system_time(dt: &exif::DateTime) -> Option<SystemTime> {
    let total_days =
        crate::core::rename::days_from_civil(dt.year as i64, dt.month as u64, dt.day as u64)?;
    let total_secs =
        total_days * 86400 + (dt.hour as i64) * 3600 + (dt.minute as i64) * 60 + dt.second as i64;
    if total_secs < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(total_secs as u64))
}

/// Try to read the EXIF date from an image file, returning the parsed epoch seconds.
fn read_exif_date(path: &Path) -> Option<i64> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif_reader = exif::Reader::new();
    let exif = exif_reader.read_from_container(&mut reader).ok()?;

    // Priority: DateTimeOriginal > DateTimeDigitized > DateTime
    let tag = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exif.get_field(exif::Tag::DateTimeDigitized, exif::In::PRIMARY))
        .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))?;

    // Use the raw ASCII value ("2024:01:15 14:30:45"). `display_as` renders the
    // same instant with dashes, and `DateTime::from_ascii` only accepts the EXIF
    // colon form — feeding it the rendered form made every EXIF tag silently fall
    // back to the file's mtime.
    let raw = match &tag.value {
        exif::Value::Ascii(values) => values.first()?.clone(),
        _ => return None,
    };
    let dt = exif::DateTime::from_ascii(&raw).ok()?;
    exif_dt_to_system_time(&dt)
        .map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
}

impl FileMeta {
    /// Read the metadata the rename pipeline needs.
    ///
    /// `md` is the single `fs::metadata` result the caller already needed for the
    /// existence check and `is_dir` — passing it in avoids re-statting the same
    /// inode, which the per-file path used to do three times (twice in
    /// `FileContext::from_path`, once here).
    ///
    /// `needs_exif_date` gates the EXIF read. That read opens and parses the file
    /// and is the single most expensive part of gathering metadata, while its
    /// result is consumed by one tag set — see
    /// [`config_needs_exif_date`](crate::config_needs_exif_date). There is no
    /// convenience wrapper that defaults it to `true`, so a hot path cannot pick
    /// up the cost without saying so.
    pub fn from_metadata(
        path: &Path,
        md: Option<&std::fs::Metadata>,
        needs_exif_date: bool,
    ) -> Self {
        FileMeta {
            created: md.and_then(|m| m.created().ok()),
            modified: md.and_then(|m| m.modified().ok()),
            accessed: md.and_then(|m| m.accessed().ok()),
            exif_date: if needs_exif_date {
                read_exif_date(path)
            } else {
                None
            },
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CropMode {
    Before,
    After,
    Special,
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum, serde::Serialize, serde::Deserialize)]
pub enum LeadDotsMode {
    Single,
    Double,
    Both,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TimestampSpec {
    Current,
    Fixed(String),
    CopyFrom(String),
    Taken,
    Delta(i64),
}

impl TimestampSpec {
    /// Resolve this spec to a concrete instant for `path`.
    ///
    /// This is the single implementation of timestamp resolution, shared by
    /// execution (`apply_timestamps_to_file`) and every other consumer.
    /// `field_name` is the timestamp being set (`"created"`, `"modified"`,
    /// `"accessed"`); `Delta` offsets the file's *current* value of that field.
    pub fn resolve(&self, path: &Path, field_name: &str) -> Option<SystemTime> {
        let meta = |path: &Path| std::fs::metadata(path).ok();
        match self {
            TimestampSpec::Current => Some(SystemTime::now()),
            TimestampSpec::Fixed(val) => {
                // User-entered wall-clock time is local; convert to a UTC instant
                // using the offset in effect at that date (not the current one).
                crate::core::rename::parse_local_datetime(val)
            }
            TimestampSpec::CopyFrom(target) => {
                let meta = meta(path)?;
                match target.as_str() {
                    "created" => meta.created().ok(),
                    "modified" => meta.modified().ok(),
                    "accessed" => meta.accessed().ok(),
                    _ => None,
                }
            }
            TimestampSpec::Taken => meta(path)?.modified().ok(),
            TimestampSpec::Delta(secs) => {
                let current = match field_name {
                    "created" => meta(path)?.created().ok(),
                    "modified" => meta(path)?.modified().ok(),
                    "accessed" => meta(path)?.accessed().ok(),
                    _ => None,
                }?;
                let delta = std::time::Duration::from_secs(secs.unsigned_abs());
                Some(if *secs >= 0 {
                    current + delta
                } else {
                    current - delta
                })
            }
        }
    }

    pub fn to_cli_str(&self) -> String {
        match self {
            TimestampSpec::Current => "current".into(),
            TimestampSpec::Fixed(v) => v.clone(),
            TimestampSpec::CopyFrom(v) => format!("copy-from:{}", v),
            TimestampSpec::Taken => "taken".into(),
            TimestampSpec::Delta(secs) => format!("delta:{}", secs),
        }
    }

    /// Accepted value grammar for the `--set-*` flags. Shared by clap and the
    /// TUI parser so the description (and error text) lives in one place.
    pub const CLI_GRAMMAR: &'static str = "'current', 'taken', 'delta:<±secs>', \
        'copy-from:created|modified|accessed', or a local datetime \
        'YYYY-MM-DD[ HH:MM[:SS]][ AM|PM]'";

    /// Parse a `--set-created` / `--set-modified` / `--set-accessed` value.
    ///
    /// Returns `None` for invalid input so callers never persist a value that
    /// can't be resolved. Fixed timestamps are **normalized** to the canonical
    /// local `"YYYY-MM-DD[ HH:MM:SS]"` form (24-hour, so AM/PM input is
    /// converted); the UTC instant is derived later when the timestamp is
    /// written, using the offset in effect at that date.
    pub fn from_cli_str(s: &str) -> Option<Self> {
        let s = s.trim();
        match s {
            "current" => Some(TimestampSpec::Current),
            "taken" => Some(TimestampSpec::Taken),
            _ => {
                if let Some(v) = s.strip_prefix("delta:") {
                    v.parse().ok().map(TimestampSpec::Delta)
                } else if let Some(v) = s.strip_prefix("copy-from:") {
                    match v {
                        "created" | "modified" | "accessed" => {
                            Some(TimestampSpec::CopyFrom(v.to_string()))
                        }
                        _ => None,
                    }
                } else {
                    let v = s.strip_prefix("fixed:").unwrap_or(s);
                    crate::core::rename::normalize_local_datetime(v).map(TimestampSpec::Fixed)
                }
            }
        }
    }
}

impl std::str::FromStr for TimestampSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_cli_str(s)
            .ok_or_else(|| format!("invalid timestamp '{}' (expected {})", s, Self::CLI_GRAMMAR))
    }
}

// ── Canonical (GUI) defaults for fields whose section `Default` is not the
//    type zero value. Declared once so the serde `default`/`skip_serializing_if`
//    predicates and the section `Default` impls cannot drift: a value equal to
//    the canonical default is omitted, and an absent field loads as it. ──

/// Starting number for a numbering sequence.
pub const NUMBERING_START_DEFAULT: usize = 1;
/// Increment between numbering values.
pub const NUMBERING_INCREMENT_DEFAULT: isize = 1;
/// Decimal numbering base (the canonical spelling; `None` also means decimal).
pub const NUMBERING_TYPE_DEFAULT: &str = "10";
/// Match-all filter mask.
pub const FILTER_PATTERN_DEFAULT: &str = "*";
/// Parent-folder levels appended by default (immediate parent only).
pub const DIRNAME_LEVEL_DEFAULT: usize = 1;

fn numbering_start_default() -> usize {
    NUMBERING_START_DEFAULT
}
fn numbering_start_is_default(v: &usize) -> bool {
    *v == NUMBERING_START_DEFAULT
}
fn numbering_increment_default() -> isize {
    NUMBERING_INCREMENT_DEFAULT
}
fn numbering_increment_is_default(v: &isize) -> bool {
    *v == NUMBERING_INCREMENT_DEFAULT
}
fn numbering_type_default() -> Option<String> {
    Some(NUMBERING_TYPE_DEFAULT.to_string())
}
fn numbering_type_is_default(v: &Option<String>) -> bool {
    v.as_deref().is_none_or(|s| s == NUMBERING_TYPE_DEFAULT)
}
fn filter_flag_default() -> bool {
    true
}
fn filter_flag_is_default(v: &bool) -> bool {
    *v
}
fn filter_pattern_default() -> Option<String> {
    Some(FILTER_PATTERN_DEFAULT.to_string())
}
fn filter_pattern_is_default(v: &Option<String>) -> bool {
    v.as_deref().is_none_or(|s| s == FILTER_PATTERN_DEFAULT)
}
fn copy_mode_default() -> bool {
    true
}
fn copy_mode_is_default(v: &bool) -> bool {
    *v
}
fn dirname_level_default() -> usize {
    DIRNAME_LEVEL_DEFAULT
}
fn dirname_level_is_default(v: &usize) -> bool {
    *v == DIRNAME_LEVEL_DEFAULT
}
fn date_position_is_default(v: &DatePosition) -> bool {
    *v == DatePosition::None
}

// ── Section sub-structs (one per section, SSoT for fields) ──

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct RegexSection {
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub regex_match: Option<String>,
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub regex_replace: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub regex_full_name: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub regex_simple: bool,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct ReplaceSection {
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub replace: Option<String>,
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub with: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub replace_case_sensitive: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub replace_first: bool,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct RemoveSection {
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub remove_first: Option<isize>,
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub remove_last: Option<isize>,
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub remove_from: Option<isize>,
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub remove_to: Option<isize>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub remove_digits: bool,
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub remove_chars: Option<String>,
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub remove_words: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub remove_symbols: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub remove_all_chars: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub trim: bool,
    #[serde(default, skip_serializing_if = "is_none")]
    pub crop_mode: Option<CropMode>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub remove_crop: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub double_spaces: bool,
    #[serde(default, skip_serializing_if = "is_none")]
    pub remove_lead_dots: Option<LeadDotsMode>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub remove_high: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub remove_accents: bool,
    // remove_case_sensitive removed — users should regex in replace section instead
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct AddSection {
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub add_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub add_suffix: Option<String>,
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub add_insert: Option<String>,
    /// Insert position in chars; negative counts gaps from the end (-1 = before
    /// the last char), matching `resolve_insert_pos` used by numbering/dirname.
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub add_at: Option<isize>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub add_word_space: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NumberingSection {
    #[serde(default, skip_serializing_if = "is_none")]
    pub numbering_mode: Option<NumberingMode>,
    #[serde(
        default = "numbering_start_default",
        skip_serializing_if = "numbering_start_is_default"
    )]
    pub numbering_start: usize,
    #[serde(
        default = "numbering_increment_default",
        skip_serializing_if = "numbering_increment_is_default"
    )]
    pub numbering_increment: isize,
    #[serde(default, skip_serializing_if = "is_default")]
    pub numbering_pad: usize,
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub numbering_sep: Option<String>,
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub numbering_at: Option<isize>,
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub numbering_break: Option<usize>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub numbering_restart_folder: bool,
    #[serde(
        default = "numbering_type_default",
        skip_serializing_if = "numbering_type_is_default"
    )]
    pub numbering_type: Option<String>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub numbering_case: Option<String>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub numbering_source: Option<NumberingSource>,
}

impl Default for NumberingSection {
    fn default() -> Self {
        Self {
            numbering_mode: None,
            numbering_start: NUMBERING_START_DEFAULT,
            numbering_increment: NUMBERING_INCREMENT_DEFAULT,
            numbering_pad: 0,
            numbering_sep: None,
            numbering_at: None,
            numbering_break: None,
            numbering_restart_folder: false,
            numbering_type: Some(NUMBERING_TYPE_DEFAULT.to_string()),
            numbering_case: None,
            numbering_source: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct CaseSection {
    #[serde(default, skip_serializing_if = "is_none")]
    pub case_name: Option<CaseMode>,
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub case_exception: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct ExtensionSection {
    #[serde(default, skip_serializing_if = "is_none")]
    pub extension_mode: Option<ExtensionMode>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub extension_replace: Option<String>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub extension_append: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub extension_remove: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub extension_add_if_missing: bool,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct NameSection {
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub name_value: Option<String>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub name_mode: Option<String>,
}

/// Format applied by an `insert_meta` tag when `insert_meta_fmt` is unset.
///
/// SSoT: the engine formats with this, and a frontend that shows the tag's
/// format must credit it rather than an empty/default-looking value.
pub const INSERT_META_FMT_DEFAULT: &str = "YYYY-MM-DD";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AutoDateSection {
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub add_date: Option<String>,
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub add_file_date: Option<String>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub insert_meta: Option<String>,
    #[serde(default, skip_serializing_if = "date_position_is_default")]
    pub date_position: DatePosition,
    /// Include century (YYYY) in date formats. When false, uses YY.
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_date_century: bool,
    /// Extra offset in hours applied on top of local time.
    /// `None` or `Some(0)` means no extra offset (use local time as-is).
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub auto_date_offset: Option<i32>,
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub auto_date_sep_filename: Option<String>,
    /// Discriminator for which timestamp to read (e.g. "creation_curr", "modified_curr",
    /// "taken_original", etc.). Stored alongside the format so the rename
    /// engine can pick the right time source.
    #[serde(default, skip_serializing_if = "is_none")]
    pub auto_date_type: Option<String>,
    /// Format string for `insert_meta` dates. When absent, falls back to
    /// [`INSERT_META_FMT_DEFAULT`].
    #[serde(default, skip_serializing_if = "is_none")]
    pub insert_meta_fmt: Option<String>,
}

impl Default for AutoDateSection {
    fn default() -> Self {
        Self {
            add_date: None,
            add_file_date: None,
            insert_meta: None,
            date_position: DatePosition::None,
            auto_date_century: false,
            auto_date_offset: None,
            auto_date_sep_filename: None,
            auto_date_type: None,
            insert_meta_fmt: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AppendFolderSection {
    #[serde(default, skip_serializing_if = "is_false")]
    pub add_dirname: bool,
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub add_dirname_sep: Option<String>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub add_dirname_pos: Option<isize>,
    #[serde(
        default = "dirname_level_default",
        skip_serializing_if = "dirname_level_is_default"
    )]
    pub dirname_level: usize,
}

impl Default for AppendFolderSection {
    fn default() -> Self {
        Self {
            add_dirname: false,
            add_dirname_sep: None,
            add_dirname_pos: None,
            dirname_level: DIRNAME_LEVEL_DEFAULT,
        }
    }
}
/// A move or copy operation on a filename portion.
/// Named fields so defaults can be declared once and referenced from the UI.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MoveCopyValue {
    /// Start position (1-based). Negative values count from the end.
    pub start: isize,
    /// Number of characters to copy/move. Negative values count from the end.
    pub length: isize,
    /// Destination position (1-based). Negative values count from the end. `0` means invalid/no destination.
    pub destination: isize,
    /// Optional separator inserted between the moved/copied part and the rest
    /// of the name (empty/None = no separator).
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub separator: Option<String>,
}

impl Default for MoveCopyValue {
    fn default() -> Self {
        Self {
            start: 1,
            length: 0,
            destination: 1,
            separator: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MoveCopySection {
    #[serde(default, skip_serializing_if = "is_none")]
    pub move_part: Option<MoveCopyValue>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub copy_part: Option<MoveCopyValue>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FiltersSection {
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub min_name_len: Option<usize>,
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub max_name_len: Option<usize>,
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub min_path_len: Option<usize>,
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub max_path_len: Option<usize>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub filter_match_case: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub filter_use_regex: bool,
    #[serde(
        default = "filter_flag_default",
        skip_serializing_if = "filter_flag_is_default"
    )]
    pub filter_files: bool,
    #[serde(
        default = "filter_flag_default",
        skip_serializing_if = "filter_flag_is_default"
    )]
    pub filter_folders: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub filter_hidden: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub filter_subfolders: bool,
    #[serde(default, skip_serializing_if = "opt_num_default_ref")]
    pub filter_level: Option<usize>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub filter_attr: Option<String>,
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub exclude_regex: Option<String>,
    #[serde(
        default = "filter_pattern_default",
        skip_serializing_if = "filter_pattern_is_default"
    )]
    pub filter_pattern: Option<String>,
}

impl Default for FiltersSection {
    fn default() -> Self {
        Self {
            min_name_len: None,
            max_name_len: None,
            min_path_len: None,
            max_path_len: None,
            filter_match_case: false,
            filter_use_regex: false,
            filter_files: true,
            filter_folders: true,
            filter_hidden: false,
            filter_subfolders: false,
            filter_level: None,
            filter_attr: None,
            exclude_regex: None,
            filter_pattern: Some(FILTER_PATTERN_DEFAULT.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CopyToSection {
    #[serde(default, skip_serializing_if = "opt_str_field_default")]
    pub output_dir: Option<String>,
    #[serde(
        default = "copy_mode_default",
        skip_serializing_if = "copy_mode_is_default"
    )]
    pub copy_mode: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub keep_structure: bool,
}

impl Default for CopyToSection {
    fn default() -> Self {
        Self {
            output_dir: None,
            copy_mode: true,
            keep_structure: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct NameSegmentSection {
    /// Character index (1-based) to start extracting the name segment.
    /// Negative values count from the end.
    #[serde(
        default,
        skip_serializing_if = "is_default",
        deserialize_with = "deserialize_isize_from_string"
    )]
    pub copy_name_segment_from: isize,
    /// Character index (1-based, inclusive) to end extracting the name segment.
    /// Negative values count from the end.
    #[serde(
        default,
        skip_serializing_if = "is_default",
        deserialize_with = "deserialize_isize_from_string"
    )]
    pub copy_name_segment_to: isize,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct SpecialSection {
    #[serde(default, skip_serializing_if = "is_none")]
    pub set_attributes: Option<String>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub set_created: Option<TimestampSpec>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub set_modified: Option<TimestampSpec>,
    #[serde(default, skip_serializing_if = "is_none")]
    pub set_accessed: Option<TimestampSpec>,
    /// Per-file increment in seconds applied sequentially across files.
    /// File 0 gets +0s, file 1 gets +N s, file 2 gets +2N s, etc.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub timestamp_incr_secs: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, ValueEnum, serde::Serialize, serde::Deserialize)]
pub enum CaseMode {
    Lower,
    Upper,
    Title,
    TitleEnhanced,
    Sentence,
    Invert,
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum, serde::Serialize, serde::Deserialize)]
pub enum NumberingMode {
    Prefix,
    Suffix,
    Both,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum, serde::Serialize, serde::Deserialize)]
pub enum NumberingSource {
    Name,
    Date,
    Size,
}

/// The default prefix still needs manual Default impl for AutoDateSection.
/// But we add a Default derive so serde can use it with #[serde(default)].
#[derive(
    Debug, Clone, Copy, PartialEq, Default, ValueEnum, serde::Serialize, serde::Deserialize,
)]
pub enum DatePosition {
    #[default]
    None,
    Prefix,
    Suffix,
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum, serde::Serialize, serde::Deserialize)]
pub enum ExtensionMode {
    Lower,
    Upper,
    Title,
}

#[cfg(test)]
mod default_consistency {
    use super::*;
    use serde::de::DeserializeOwned;

    /// A section's canonical default must serialize to an empty object, and an
    /// empty object must deserialize back to that same default. This keeps every
    /// field's serde default in lock-step with the section's `Default` impl, so
    /// "absent" and "default" can never drift apart (which would silently change
    /// behavior when a field is omitted from a saved document).
    fn assert_consistent<T>()
    where
        T: serde::Serialize + DeserializeOwned + Default + PartialEq + std::fmt::Debug,
    {
        let def = T::default();
        assert_eq!(
            serde_json::to_value(&def).unwrap(),
            serde_json::json!({}),
            "default must serialize to an empty object"
        );
        let from_empty: T = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(
            from_empty, def,
            "an empty object must deserialize to the default"
        );
    }

    #[test]
    fn every_section_default_is_serde_consistent() {
        assert_consistent::<RegexSection>();
        assert_consistent::<ReplaceSection>();
        assert_consistent::<RemoveSection>();
        assert_consistent::<AddSection>();
        assert_consistent::<NumberingSection>();
        assert_consistent::<CaseSection>();
        assert_consistent::<ExtensionSection>();
        assert_consistent::<NameSection>();
        assert_consistent::<AutoDateSection>();
        assert_consistent::<AppendFolderSection>();
        assert_consistent::<MoveCopySection>();
        assert_consistent::<FiltersSection>();
        assert_consistent::<CopyToSection>();
        assert_consistent::<NameSegmentSection>();
        assert_consistent::<SpecialSection>();
        assert_consistent::<crate::RenameConfig>();
    }

    #[test]
    fn non_default_fields_serialize_minimally() {
        let cfg = crate::RenameConfig {
            add: AddSection {
                add_prefix: Some("pre_".into()),
                ..Default::default()
            },
            numbering: NumberingSection {
                numbering_mode: Some(NumberingMode::Prefix),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&cfg).unwrap(),
            serde_json::json!({
                "add": { "add_prefix": "pre_" },
                "numbering": { "numbering_mode": "Prefix" },
            })
        );
    }

    #[test]
    fn frontend_unset_spellings_are_cleaned_to_canonical() {
        let mut cfg = crate::RenameConfig::default();
        // CLI-style "unset" spellings.
        cfg.numbering.numbering_type = None;
        cfg.filters.filter_files = false;
        cfg.filters.filter_folders = false;
        cfg.filters.filter_pattern = None;
        cfg.copy_to.copy_mode = false;
        let cleaned = cfg.cleaned();
        assert_eq!(
            cleaned.numbering.numbering_type.as_deref(),
            Some(NUMBERING_TYPE_DEFAULT)
        );
        assert!(cleaned.filters.filter_files && cleaned.filters.filter_folders);
        assert_eq!(
            cleaned.filters.filter_pattern.as_deref(),
            Some(FILTER_PATTERN_DEFAULT)
        );
        assert!(cleaned.copy_to.copy_mode);
        // A cleaned, default-shaped config serializes to an empty object.
        assert_eq!(
            serde_json::to_value(&cleaned).unwrap(),
            serde_json::json!({})
        );
    }
}
