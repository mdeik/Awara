use awara::{
    AddSection, AppendFolderSection, AutoDateSection, CaseSection, CopyToSection, DatePosition,
    EntryFilter, ExtensionSection, FiltersSection, MoveCopySection, MoveCopyValue, NameSection,
    NameSegmentSection, NumberingMode, NumberingSection, NumberingSource, RegexSection,
    RemoveSection, RenameConfig, ReplaceSection, SpecialSection,
};

use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(name = "awara", version, about = "Batch rename utility")]
pub struct Args {
    /// Files or directories to rename
    pub files: Vec<String>,

    // ── Regex options ──
    /// Regex pattern to match in filenames
    #[arg(long)]
    pub regex_match: Option<String>,
    /// Regex replacement string
    #[arg(long)]
    pub regex_replace: Option<String>,
    /// Apply regex to the full filename (including extension)
    #[arg(long)]
    pub regex_full_name: bool,
    /// Simplified regex mode (no escaping needed) — shortcut for --regex-match with simple mode
    #[arg(long)]
    pub regex_simple: Option<String>,
    /// Replacement string
    #[arg(long)]
    pub with: Option<String>,
    /// String to replace in filenames
    #[arg(long)]
    pub replace: Option<String>,
    /// Enable case-sensitive matching for replace/remove-words operations (regex is always case-sensitive; use (?i))
    #[arg(long)]
    pub replace_case_sensitive: bool,
    /// Replace only the first occurrence
    #[arg(long)]
    pub replace_first: bool,

    // ── Case options ──
    /// Case mode for filename part: lower/upper/title/title-enhanced/sentence/invert
    #[arg(long)]
    pub case_name: Option<awara::CaseMode>,

    // ── Remove options ──
    /// Remove first N characters (negative counts from end)
    #[arg(long)]
    pub remove_first: Option<isize>,
    /// Remove last N characters (negative counts from start)
    #[arg(long)]
    pub remove_last: Option<isize>,
    /// Remove from character position N (negative counts from end)
    #[arg(long)]
    pub remove_from: Option<isize>,
    /// Remove to character position N (negative counts from end)
    #[arg(long)]
    pub remove_to: Option<isize>,
    /// Remove all digits
    #[arg(long)]
    pub remove_digits: bool,
    /// Remove specified characters
    #[arg(long)]
    pub remove_chars: Option<String>,
    /// Remove specified words
    #[arg(long)]
    pub remove_words: Option<String>,
    /// Remove symbols
    #[arg(long)]
    pub remove_symbols: bool,
    /// Trim whitespace
    #[arg(long)]
    pub trim: bool,
    /// Replace multiple spaces with single
    #[arg(long)]
    pub double_spaces: bool,

    // ── Add options ──
    /// Prefix text to add to filenames
    #[arg(long)]
    pub add_prefix: Option<String>,
    /// Suffix text to add to filenames
    #[arg(long)]
    pub add_suffix: Option<String>,
    /// Text to insert at a specific position
    #[arg(long)]
    pub add_insert: Option<String>,
    /// Character position to insert at (negative counts from the end)
    #[arg(long, allow_hyphen_values = true)]
    pub add_at: Option<isize>,
    /// Add spaces between words in CamelCase
    #[arg(long)]
    pub add_word_space: bool,

    // ── Numbering options ──
    /// Numbering mode: prefix/suffix/insert
    #[arg(long)]
    pub numbering_mode: Option<NumberingMode>,
    /// Starting number for numbering sequence (default: 1)
    #[arg(long, default_value_t = 1)]
    pub numbering_start: usize,
    /// Increment between numbers (default: 1, negative for descending)
    #[arg(long, default_value_t = 1)]
    pub numbering_increment: isize,
    /// Zero-pad numbers to this width
    #[arg(long, default_value_t = 0)]
    pub numbering_pad: usize,
    /// Separator between number and filename
    #[arg(long)]
    pub numbering_sep: Option<String>,
    /// Extension mode: replace/append/case/remove
    #[arg(long)]
    pub extension_mode: Option<awara::ExtensionMode>,
    /// New extension value (for "Fixed" mode)
    #[arg(long)]
    pub extension: Option<String>,
    /// Additional extension text to append
    #[arg(long)]
    pub extension_add: Option<String>,
    /// Remove file extension entirely
    #[arg(long)]
    pub extension_remove: bool,

    // ── Execution options ──
    /// Dry run (show what would be done)
    #[arg(long, short = 'n')]
    pub dry_run: bool,
    /// Verbose output
    #[arg(long, short = 'v')]
    pub verbose: bool,
    /// Undo file path. Records the successful in-place renames only — Copy / Move
    /// to Location transfers are not recorded (and so cannot be undone).
    #[arg(long)]
    pub undo_file: Option<String>,
    /// Apply undo operations from a file
    #[arg(long)]
    pub apply_undo: Option<String>,
    /// Stop on first error
    #[arg(long)]
    pub stop_on_error: bool,
    /// Recursively process subdirectories
    #[arg(long, short = 'r')]
    pub recursive: bool,
    /// Maximum recursion depth
    #[arg(long)]
    pub max_depth: Option<usize>,

    // ── Append folder options ──
    /// Add parent directory name to filename
    #[arg(long)]
    pub add_dirname: bool,
    /// Separator between dirname and filename
    #[arg(long)]
    pub dirname_sep: Option<String>,
    /// Position for dirname: prefix (0) or suffix (-1)
    #[arg(long)]
    pub dirname_pos: Option<isize>,
    /// Number of parent directory levels to append (each separated by `sep`)
    #[arg(long, default_value_t = 1)]
    pub dirname_level: usize,

    // ── Move/copy part options ──
    /// Move part of filename: start:len:dest
    #[arg(long)]
    pub move_part: Option<String>,
    /// Copy part of filename: start:len:dest
    #[arg(long)]
    pub copy_part: Option<String>,

    // ── Output options ──
    /// Output directory for renamed files
    #[arg(long)]
    pub output_dir: Option<String>,
    /// Copy files instead of moving (copy is the default when --output-dir is set).
    /// A directory is recreated empty; its contents are not copied.
    #[arg(long, conflicts_with = "move_mode")]
    pub copy_mode: bool,
    /// Move files to the output directory instead of copying them. A relocated
    /// directory is still recreated empty (its contents are not moved).
    #[arg(long = "move", conflicts_with = "copy_mode")]
    pub move_mode: bool,
    /// Overwrite existing target files (applies to all collisions, no prompt)
    ///
    /// A collision involving a folder on either side (an existing folder target,
    /// or a folder source over an existing file) is never overwritten or merged;
    /// such conflicts are skipped.
    #[arg(long)]
    pub overwrite: bool,
    /// Skip existing target files (applies to all collisions, no prompt)
    #[arg(long)]
    pub skip: bool,

    // ── Filter options ──
    /// Minimum filename length
    #[arg(long)]
    pub min_name_len: Option<usize>,
    /// Maximum filename length
    #[arg(long)]
    pub max_name_len: Option<usize>,
    /// Minimum full path length
    #[arg(long)]
    pub min_path_len: Option<usize>,
    /// Maximum full path length
    #[arg(long)]
    pub max_path_len: Option<usize>,
    /// Filter mode: files/folders/both
    #[arg(long)]
    pub filter_mode: Option<EntryFilter>,
    /// Sort by field
    #[arg(long)]
    pub sort: Option<String>,

    // ── Remove mode (combo CLI flag) ──
    #[arg(long)]
    pub remove: Vec<CliRemoveMode>,

    // ── Date options ──
    /// Date format string for adding dates
    #[arg(long)]
    pub add_date: Option<String>,
    /// Use file modification date
    #[arg(long)]
    pub add_file_date: Option<String>,
    /// Add extension even if file has no extension
    #[arg(long)]
    pub extension_add_if_missing: bool,
    /// Regex pattern to exclude files
    #[arg(long)]
    pub exclude_regex: Option<String>,
    /// Metadata tag to insert (e.g. taken_original)
    #[arg(long, value_parser = awara::parse_insert_meta_arg)]
    pub insert_meta: Option<String>,
    /// Preserve original file timestamps
    #[arg(long)]
    pub preserve_timestamps: bool,
    /// File attributes to set after rename (e.g. readonly,hidden)
    #[arg(long, value_parser = awara::parse_attributes_arg)]
    pub set_attributes: Option<String>,
    /// Save current options as a preset
    #[arg(long)]
    pub save_preset: Option<String>,
    /// Load options from a preset file
    #[arg(long)]
    pub load_preset: Option<String>,
    /// Launch GUI mode
    #[arg(long)]
    pub gui: bool,
    /// GUI theme: dark/light/system
    #[arg(long)]
    pub theme: Option<String>,
    /// Launch TUI mode
    #[arg(long)]
    pub tui: bool,
    /// Reverse sort order
    #[arg(long)]
    pub swap: bool,
    /// Source field for numbering: name/date/size
    #[arg(long)]
    pub numbering_source: Option<NumberingSource>,
    /// Filter by file attributes
    #[arg(long, value_parser = awara::parse_filter_attr_arg)]
    pub filter_attr: Option<String>,
    /// Set creation timestamp: current, taken, delta:<secs>, copy-from:<field>,
    /// or a local datetime (YYYY-MM-DD[ HH:MM[:SS]][ AM|PM])
    #[arg(long)]
    pub set_created: Option<awara::TimestampSpec>,
    /// Set modification timestamp (same syntax as --set-created)
    #[arg(long)]
    pub set_modified: Option<awara::TimestampSpec>,
    /// Set access timestamp (same syntax as --set-created)
    #[arg(long)]
    pub set_accessed: Option<awara::TimestampSpec>,
    /// Date position: prefix or suffix
    #[arg(long)]
    pub date_position: Option<DatePosition>,
    /// Include century (YYYY) in date output
    #[arg(long)]
    pub auto_date_century: bool,
    /// Offset in hours for auto date
    #[arg(long)]
    pub auto_date_offset: Option<i32>,
    /// Separator between filename and auto-date
    #[arg(long)]
    pub auto_date_sep_filename: Option<String>,
    /// Keep directory structure when copying to output
    #[arg(long)]
    pub keep_structure: bool,
    /// Remove leading dots from filenames
    #[arg(long)]
    pub remove_lead_dots: Option<awara::LeadDotsMode>,
    /// Remove all characters from the filename
    #[arg(long)]
    pub remove_all_chars: bool,
    /// Crop the filename: before:TEXT, after:TEXT, or special:TEXT
    #[arg(long, value_parser = awara::parse_crop_arg)]
    pub crop: Option<String>,
    /// Number position (character index) for numbering insert mode (negative counts from end)
    #[arg(long)]
    pub numbering_at: Option<isize>,
    /// Reset numbering every N files
    #[arg(long)]
    pub numbering_break: Option<usize>,
    /// Restart numbering for each subfolder
    #[arg(long)]
    pub numbering_restart_folder: bool,
    /// Numbering type/base: decimal, hex, binary, etc.
    #[arg(long, value_parser = awara::parse_numbering_type_arg)]
    pub numbering_type: Option<String>,
    /// Case for alpha/roman numbering: upper/lower
    #[arg(long, value_parser = awara::parse_numbering_case_arg)]
    pub numbering_case: Option<String>,
    /// Show only files
    #[arg(long)]
    pub filter_files: bool,
    /// Show only folders
    #[arg(long)]
    pub filter_folders: bool,
    /// Show hidden files
    #[arg(long)]
    pub filter_hidden: bool,
    /// Recursively include subfolders
    #[arg(long)]
    pub filter_subfolders: bool,
    /// Maximum recursion level
    #[arg(long)]
    pub filter_level: Option<usize>,
    /// Treat filter pattern as regex
    #[arg(long)]
    pub filter_use_regex: bool,
    /// Case-sensitive filter matching
    #[arg(long)]
    pub filter_match_case: bool,
    // ── Name mode ──
    /// Name mode: keep/remove/fixed/reverse/pad_numbers/reformat_date
    #[arg(long)]
    pub name_mode: Option<String>,
    /// Custom name value
    #[arg(long)]
    pub name_value: Option<String>,
    /// Case exception word (preserved as-is)
    #[arg(long)]
    pub case_exception: Option<String>,
    /// Name segment: start character index to extract from filename (negative counts from end)
    #[arg(long)]
    pub name_segment_from: Option<isize>,
    /// Name segment: end character index (exclusive, negative counts from end)
    #[arg(long)]
    pub name_segment_to: Option<isize>,
}

fn parse_part_arg(arg: Option<String>) -> Option<MoveCopyValue> {
    // SSoT: the start:len:dest[:sep] format lives in awara::parse_move_copy_value.
    arg.as_deref().and_then(awara::parse_move_copy_value)
}

impl Args {
    /// Build the config from CLI flags, then enforce the shared validation and
    /// limits — so CLI/TUI flags cannot produce a state that preset loading (or
    /// the GUI) would have cleaned up.
    pub fn into_config(self) -> RenameConfig {
        let mut config = self.into_config_inner();
        config.sanitize();
        config
    }

    /// Raw flag → config translation (no validation).
    fn into_config_inner(self) -> RenameConfig {
        // Handle presets: if loading a preset, override config fields.
        // Validated parsing is the SSoT, so an invalid preset is treated exactly
        // like an unparseable one (ignored here; the CLI/TUI report the error).
        if let Some(preset_path) = &self.load_preset
            && let Ok(doc) = awara::load_rename_doc(std::path::Path::new(preset_path))
        {
            // Disabled sections are honored: their values are cleared to defaults.
            // Merge: preset values override defaults, CLI flags override preset.
            let mut config = doc.effective_config().into_owned();
            // CLI flags override preset
            if self.add_prefix.is_some() {
                config.add.add_prefix = self.add_prefix;
            }
            if self.add_suffix.is_some() {
                config.add.add_suffix = self.add_suffix;
            }
            // ... other overrides would go here. For simplicity, we just return the preset with CLI overrides.
            // Apply simple scalar overrides
            if self.regex_match.is_some() {
                config.regex.regex_match = self.regex_match;
            }
            if self.regex_replace.is_some() {
                config.regex.regex_replace = self.regex_replace;
            }
            if self.replace.is_some() {
                config.replace.replace = self.replace;
            }
            if self.with.is_some() {
                config.replace.with = self.with;
            }
            if self.replace_case_sensitive {
                config.replace.replace_case_sensitive = true;
            }
            if self.add_date.is_some() {
                config.auto_date.add_date = self.add_date;
            }
            if self.add_file_date.is_some() {
                config.auto_date.add_file_date = self.add_file_date;
            }
            if self.exclude_regex.is_some() {
                config.filters.exclude_regex = self.exclude_regex;
            }
            if self.insert_meta.is_some() {
                config.auto_date.insert_meta = self.insert_meta;
            }
            if self.numbering_source.is_some() {
                config.numbering.numbering_source = self.numbering_source;
            }
            if self.numbering_mode.is_some() {
                config.numbering.numbering_mode = self.numbering_mode;
            }
            if self.case_name.is_some() {
                config.case.case_name = self.case_name;
            }
            if self.set_attributes.is_some() {
                config.special.set_attributes = self.set_attributes;
            }
            if self.set_created.is_some() {
                config.special.set_created = self.set_created;
            }
            if self.set_modified.is_some() {
                config.special.set_modified = self.set_modified;
            }
            if self.set_accessed.is_some() {
                config.special.set_accessed = self.set_accessed;
            }
            return config;
        }

        // A date field with no explicit position defaults to a prefix (matching the
        // GUI, where enabling Auto Date selects Prefix first). Without this,
        // `--add-date`/`--add-file-date`/`--insert-meta` would be silent no-ops.
        let has_date =
            self.add_date.is_some() || self.add_file_date.is_some() || self.insert_meta.is_some();
        let date_position = match self.date_position {
            Some(p) => p,
            None if has_date => DatePosition::Prefix,
            None => DatePosition::None,
        };

        RenameConfig {
            section_order: Vec::new(),
            regex: RegexSection {
                regex_match: self.regex_match.or(self.regex_simple.clone()),
                regex_replace: self.regex_replace,
                regex_full_name: self.regex_full_name,
                regex_simple: self.regex_simple.is_some(),
            },
            replace: ReplaceSection {
                replace: self.replace.or(self.with.clone()),
                with: self.with,
                replace_case_sensitive: self.replace_case_sensitive,
                replace_first: self.replace_first,
            },
            remove: RemoveSection {
                remove_first: self.remove_first,
                remove_last: self.remove_last,
                remove_from: self.remove_from,
                remove_to: self.remove_to,
                remove_digits: self.remove_digits || self.remove.contains(&CliRemoveMode::Digits),
                remove_chars: self.remove_chars,
                remove_words: self.remove_words,
                remove_symbols: self.remove_symbols
                    || self.remove.contains(&CliRemoveMode::Symbols),
                remove_all_chars: self.remove_all_chars,
                trim: self.trim || self.remove.contains(&CliRemoveMode::Trim),
                crop_mode: self.crop.as_ref().and_then(|c| {
                    if c.starts_with("before:") {
                        Some(awara::CropMode::Before)
                    } else if c.starts_with("after:") {
                        Some(awara::CropMode::After)
                    } else if c.starts_with("special:") {
                        Some(awara::CropMode::Special)
                    } else {
                        None
                    }
                }),
                remove_crop: self.crop.as_ref().and_then(|c| {
                    if let Some(val) = c.strip_prefix("before:") {
                        if val.is_empty() {
                            None
                        } else {
                            Some(val.to_string())
                        }
                    } else if let Some(val) = c.strip_prefix("after:") {
                        if val.is_empty() {
                            None
                        } else {
                            Some(val.to_string())
                        }
                    } else if let Some(val) = c.strip_prefix("special:") {
                        if val.is_empty() {
                            None
                        } else {
                            Some(val.to_string())
                        }
                    } else {
                        None
                    }
                }),
                double_spaces: self.double_spaces
                    || self.remove.contains(&CliRemoveMode::DoubleSpaces),
                remove_lead_dots: self.remove_lead_dots,
                remove_high: self.remove.contains(&CliRemoveMode::High),
                remove_accents: self.remove.contains(&CliRemoveMode::Accents),
            },
            add: AddSection {
                add_prefix: self.add_prefix,
                add_suffix: self.add_suffix,
                add_insert: self.add_insert,
                add_at: self.add_at,
                add_word_space: self.add_word_space,
            },
            numbering: NumberingSection {
                numbering_mode: self.numbering_mode,
                numbering_start: self.numbering_start,
                numbering_increment: self.numbering_increment,
                numbering_pad: self.numbering_pad,
                numbering_sep: self.numbering_sep,
                numbering_at: self.numbering_at,
                numbering_break: self.numbering_break,
                numbering_restart_folder: self.numbering_restart_folder,
                numbering_type: self.numbering_type,
                numbering_case: self.numbering_case,
                numbering_source: self.numbering_source,
            },
            case: CaseSection {
                case_name: self.case_name,
                case_exception: self.case_exception,
            },
            extension: ExtensionSection {
                extension_mode: self.extension_mode,
                extension_replace: self.extension,
                extension_append: self.extension_add,
                extension_remove: self.extension_remove,
                extension_add_if_missing: self.extension_add_if_missing,
            },
            name: NameSection {
                name_value: self.name_value,
                name_mode: self.name_mode,
            },
            auto_date: AutoDateSection {
                add_date: self.add_date,
                add_file_date: self.add_file_date,
                insert_meta: self.insert_meta,
                date_position,
                auto_date_century: self.auto_date_century,
                auto_date_offset: self.auto_date_offset,
                auto_date_sep_filename: self.auto_date_sep_filename,
                auto_date_type: None,
                insert_meta_fmt: None,
            },
            append_folder: AppendFolderSection {
                add_dirname: self.add_dirname,
                add_dirname_sep: self.dirname_sep,
                add_dirname_pos: self.dirname_pos,
                dirname_level: self.dirname_level,
            },
            move_copy: MoveCopySection {
                move_part: parse_part_arg(self.move_part),
                copy_part: parse_part_arg(self.copy_part),
            },
            filters: FiltersSection {
                min_name_len: self.min_name_len,
                max_name_len: self.max_name_len,
                min_path_len: self.min_path_len,
                max_path_len: self.max_path_len,
                filter_attr: self.filter_attr,
                filter_match_case: self.filter_match_case,
                filter_use_regex: self.filter_use_regex,
                filter_files: self.filter_files
                    || self.filter_mode == Some(EntryFilter::Files)
                    || self.filter_mode == Some(EntryFilter::Both),
                filter_folders: self.filter_folders
                    || self.filter_mode == Some(EntryFilter::Folders)
                    || self.filter_mode == Some(EntryFilter::Both),
                filter_hidden: self.filter_hidden,
                filter_subfolders: self.filter_subfolders || self.recursive,
                filter_level: self.filter_level,
                filter_pattern: None,
                exclude_regex: self.exclude_regex,
            },
            copy_to: CopyToSection {
                // Copy is the default when copying/moving to a location (docs:
                // "Copy Not Move ... Enabled by default"); --move opts into move.
                copy_mode: !self.move_mode && (self.copy_mode || self.output_dir.is_some()),
                output_dir: self.output_dir,
                keep_structure: self.keep_structure,
            },
            name_segment: NameSegmentSection {
                copy_name_segment_from: self.name_segment_from.unwrap_or(0),
                copy_name_segment_to: self.name_segment_to.unwrap_or(0),
            },
            special: SpecialSection {
                set_attributes: self.set_attributes,
                set_created: self.set_created,
                set_modified: self.set_modified,
                set_accessed: self.set_accessed,
                ..Default::default()
            },
            command_order: Vec::new(),
            stop_on_error: self.stop_on_error,
            preserve_timestamps: self.preserve_timestamps,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CliRemoveMode {
    Digits,
    High,
    Trim,
    DoubleSpaces,
    Accents,
    Symbols,
}

impl std::str::FromStr for CliRemoveMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "digits" => Ok(CliRemoveMode::Digits),
            "high" => Ok(CliRemoveMode::High),
            "trim" => Ok(CliRemoveMode::Trim),
            "double-spaces" | "doublespaces" => Ok(CliRemoveMode::DoubleSpaces),
            "accents" => Ok(CliRemoveMode::Accents),
            "symbols" => Ok(CliRemoveMode::Symbols),
            _ => Err(format!("Unknown remove mode: {s}")),
        }
    }
}
