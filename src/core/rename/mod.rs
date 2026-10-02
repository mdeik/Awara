use regex::{Regex, RegexBuilder};
use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::config::{
    CaseMode, CompiledConfig, CropMode, DatePosition, ExtensionMode, FileMeta, LeadDotsMode,
    NumberingMode, RenameConfig, RenameItem, TimestampSpec,
};

mod case; // case conversion
mod dates; // date and metadata insertion
mod extension; // extension handling
mod insert; // insert, dirname, move/copy
mod numbering; // numbering
mod ops; // replace/remove/crop operations
mod process; // the rename pipeline
mod sanitize;
mod user_regex; // regex compilation SSoT // filename sanitising

pub use dates::{
    format_date, local_datetime_to_utc_secs, local_utc_offset_secs, local_utc_offset_secs_at,
    normalize_local_datetime, parse_date, parse_local_datetime,
};
pub use extension::{extension_str, split_extension, split_extension_for};
pub use insert::resolve_insert_pos;
pub use ops::resolve_from_to_range;
pub(crate) use process::FileContext;
pub use process::process_filename;
pub use sanitize::{PathNavigation, path_navigation_error, sanitize_file_name};

// Re-exported with the same visibility as before, so callers keep the
// same names. Some are only used inside this module, so the import
// lint is silenced.
// API (`awara::*` re-exports everything through `pub use core::*`).
#[allow(unused_imports)]
pub(crate) use case::{apply_case, apply_case_exceptions};
#[allow(unused_imports)]
pub(crate) use dates::{
    apply_add_date, apply_insert_meta, civil_from_days, date_type_time_source, days_from_civil,
    days_in_month, format_date_offset, is_leap,
};
#[allow(unused_imports)]
pub(crate) use extension::{apply_extension_case, apply_extension_op};
#[allow(unused_imports)]
pub(crate) use insert::{
    NumberingConfig, apply_copy_part, apply_dirname, apply_insert, apply_move_part,
    apply_word_space, insert_part,
};
#[allow(unused_imports)]
pub(crate) use numbering::{
    apply_numbering, format_number, number_to_alpha, number_to_base, number_to_roman,
};
#[allow(unused_imports)]
pub(crate) use ops::{
    apply_crop, apply_double_spaces, apply_remove_accents, apply_remove_chars, apply_remove_digits,
    apply_remove_first, apply_remove_from_to, apply_remove_high, apply_remove_last,
    apply_remove_lead_dots, apply_remove_symbols, apply_remove_words, apply_replace_op,
};
#[allow(unused_imports)]
pub(crate) use user_regex::{build_regex, build_word_regex, effective_pattern};

#[cfg(test)]
mod tests;
