use super::*;

// The rename-operation list: its items, display, builders, and config roundtrip.
mod builders;
mod display;
mod roundtrip;

pub(crate) use builders::numbering_sequence;
pub use builders::{
    config_has_file_effects, config_has_numbering, config_needs_exif_date, make_add_items,
    make_auto_date_items, make_case_items, make_dirname_item, make_extension_item,
    make_move_copy_items, make_name_items, make_numbering_item, make_regex_item, make_remove_items,
    make_replace_item,
};
pub use display::rename_item_display;
pub use roundtrip::{config_to_items, emit_section_items, items_to_config};

/// One rename operation, stored in the config's `command_order`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RenameItem {
    /// Fields: match, replace, full_name (include extension), simple.
    /// The pattern is compiled verbatim (case-sensitive); use an inline `(?i)`
    /// for case-insensitive matching.
    Regex(String, String, bool, bool),
    Replace(String, String, bool, bool),
    RemoveFirst(isize),
    RemoveLast(isize),
    RemoveFromTo(isize, isize),
    RemoveDigits,
    RemoveChars(String),
    RemoveWords(String, bool),
    RemoveCrop(CropMode, String),
    RemoveSymbols,
    Trim,
    DoubleSpaces,
    RemoveLeadDots(LeadDotsMode),
    RemoveAllChars,
    RemoveHigh,
    RemoveAccents,
    Prefix(String),
    Suffix(String),
    Insert(String, isize),
    WordSpace,
    CaseName(CaseMode),
    Numbering(
        NumberingMode,
        usize,
        isize,
        usize,
        Option<String>,
        Option<isize>,
        Option<String>,
        Option<String>,
        Option<usize>,
    ),
    CaseExt(CaseMode),
    Extension(ExtensionMode, Option<String>, Option<String>, bool),
    Dirname(bool, Option<String>, Option<isize>),
    MovePart(isize, isize, isize, Option<String>),
    CopyPart(isize, isize, isize, Option<String>),
    AddDate(String),
    AddFileDate(String),
    ExtensionAddIfMissing,
    InsertMeta(String),
    NameSegment(isize, isize),
    /// Replace the stem with a fixed value (keeps extension).
    NameFixed(String),
    /// Remove the stem entirely (keeps extension as a dotfile).
    NameRemove,
    /// Reverse the stem characters (keeps extension).
    NameReverse,
    /// Pad the first digit group in the stem to `amount` digits using `char`.
    NamePadNumbers(usize, char),
    /// Reformat dates in the stem: parse with optional custom format, output with given format.
    NameReformatDate {
        parse_fmt: Option<String>,
        output_fmt: String,
    },
}
