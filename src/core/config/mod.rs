use clap::ValueEnum;
use regex::Regex;
use serde::{Deserialize, Deserializer};
use std::io::BufReader;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod compiled; // CompiledConfig + impl
mod doc; // shared rename document (config + disabled sections)
mod fields; // which fields each rename option maps to
mod items; // rename-operation list and its builders
mod limits; // inclusive numeric limits (SSoT for GUI + validation)
mod section_id; // section identity + default execution order (SSoT)
mod sections; // the config sections and their data types
mod values; // SSoT validation/normalization of option values

pub use compiled::*;
pub use doc::*;
pub use fields::*;
pub use items::*;
pub use limits::*;
pub use section_id::*;
pub use sections::*;
pub use values::*;

// ── The main config struct ──
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct RenameConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub section_order: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command_order: Vec<RenameItem>,

    #[serde(default, skip_serializing_if = "is_default")]
    pub regex: RegexSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub replace: ReplaceSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub remove: RemoveSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub add: AddSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub numbering: NumberingSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub case: CaseSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub extension: ExtensionSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: NameSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub auto_date: AutoDateSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub append_folder: AppendFolderSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub move_copy: MoveCopySection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub filters: FiltersSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub copy_to: CopyToSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub name_segment: NameSegmentSection,
    #[serde(default, skip_serializing_if = "is_default")]
    pub special: SpecialSection,

    #[serde(default, skip_serializing_if = "is_false")]
    pub stop_on_error: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub preserve_timestamps: bool,
}

impl RenameConfig {
    /// Reset every field in `section` to its default. SSoT for section clearing,
    /// used by the GUI reset buttons and by section gating
    /// ([`RenameDoc::effective_config`]).
    pub fn clear_section(&mut self, section: SectionId) {
        match section {
            SectionId::Regex => self.regex = RegexSection::default(),
            SectionId::Replace => self.replace = ReplaceSection::default(),
            SectionId::Remove => self.remove = RemoveSection::default(),
            SectionId::Add => self.add = AddSection::default(),
            SectionId::Numbering => self.numbering = NumberingSection::default(),
            SectionId::Case => self.case = CaseSection::default(),
            SectionId::Extension => self.extension = ExtensionSection::default(),
            SectionId::Name => self.name = NameSection::default(),
            SectionId::AutoDate => self.auto_date = AutoDateSection::default(),
            SectionId::AppendFolder => self.append_folder = AppendFolderSection::default(),
            SectionId::MoveCopy => self.move_copy = MoveCopySection::default(),
            SectionId::Filters => self.filters = FiltersSection::default(),
            SectionId::CopyTo => self.copy_to = CopyToSection::default(),
            SectionId::NameSegment => self.name_segment = NameSegmentSection::default(),
            SectionId::Special => {
                self.special = SpecialSection::default();
                self.section_order.clear(); // empty = default order
            }
        }
    }

    /// Normalize frontend-specific "unset" spellings to the canonical (GUI)
    /// defaults so serialization omits them. Each mapping is an equivalence the
    /// engine already honors, so this only affects the saved representation.
    /// Applied when building a document to save.
    pub fn cleaned(mut self) -> Self {
        // Decimal numbering may be spelled `None` (CLI) or `"10"` (GUI default).
        if self.numbering.numbering_type.is_none() {
            self.numbering.numbering_type = Some(NUMBERING_TYPE_DEFAULT.to_string());
        }
        // "Both false" means show both files and folders — the same as both true.
        if !self.filters.filter_files && !self.filters.filter_folders {
            self.filters.filter_files = true;
            self.filters.filter_folders = true;
        }
        // No mask is equivalent to the canonical match-all.
        if self.filters.filter_pattern.is_none() {
            self.filters.filter_pattern = Some(FILTER_PATTERN_DEFAULT.to_string());
        }
        // copy_mode only matters when there is an output directory.
        if self.copy_to.output_dir.is_none() {
            self.copy_to.copy_mode = true;
        }
        self
    }

    /// Create a truly empty config (all fields set to false/None).
    /// Unlike `Default::default()` which may enable certain features by default
    /// (e.g. copy_mode), this returns a completely blank slate.
    pub fn empty() -> Self {
        Self {
            section_order: Vec::new(),
            command_order: Vec::new(),
            regex: RegexSection::default(),
            replace: ReplaceSection::default(),
            remove: RemoveSection::default(),
            add: AddSection::default(),
            numbering: NumberingSection {
                numbering_start: 0,
                numbering_increment: 0,
                ..Default::default()
            },
            case: CaseSection::default(),
            extension: ExtensionSection::default(),
            name: NameSection::default(),
            auto_date: AutoDateSection {
                date_position: DatePosition::None,
                ..Default::default()
            },
            append_folder: AppendFolderSection {
                dirname_level: 0,
                ..Default::default()
            },
            move_copy: MoveCopySection::default(),
            filters: FiltersSection {
                filter_files: false,
                filter_folders: false,
                ..Default::default()
            },
            copy_to: CopyToSection {
                copy_mode: false,
                ..Default::default()
            },
            name_segment: NameSegmentSection::default(),
            special: SpecialSection::default(),
            stop_on_error: false,
            preserve_timestamps: false,
        }
    }
}
/// Numbering base/type options: (label, value) pairs.
/// SSoT for the numbering base dropdown — shared by GUI and any other consumers.
/// Order: binary → base 3-9 → decimal → base 11-15 → hex → base 17-35 → A-Z → Roman.
pub const NUMBERING_BASES: &[(&str, &str)] = &[
    ("Binary", "2"),
    ("Base 3", "3"),
    ("Base 4", "4"),
    ("Base 5", "5"),
    ("Base 6", "6"),
    ("Base 7", "7"),
    ("Base 8", "8"),
    ("Base 9", "9"),
    ("Decimal", "10"),
    ("Base 11", "11"),
    ("Base 12", "12"),
    ("Base 13", "13"),
    ("Base 14", "14"),
    ("Base 15", "15"),
    ("Hex", "16"),
    ("Base 17", "17"),
    ("Base 18", "18"),
    ("Base 19", "19"),
    ("Base 20", "20"),
    ("Base 21", "21"),
    ("Base 22", "22"),
    ("Base 23", "23"),
    ("Base 24", "24"),
    ("Base 25", "25"),
    ("Base 26", "26"),
    ("Base 27", "27"),
    ("Base 28", "28"),
    ("Base 29", "29"),
    ("Base 30", "30"),
    ("Base 31", "31"),
    ("Base 32", "32"),
    ("Base 33", "33"),
    ("Base 34", "34"),
    ("Base 35", "35"),
    ("A-Z", "36"),
    ("Roman", "roman"),
];
