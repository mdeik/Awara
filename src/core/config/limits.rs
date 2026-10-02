//! Inclusive numeric limits for config fields — the single source of truth for
//! bounds shared by the GUI widgets and core validation.
//!
//! The GUI is the reference: each limit mirrors the `DragValue` range of the
//! corresponding widget. `RenameConfig::sanitize` clamps to the same bounds (so a
//! hand-edited preset, CLI flags, or the TUI cannot produce a value the GUI could
//! not), and the GUI widgets read these constants instead of repeating literals.

/// `add.add_at` — insert position.
pub const ADD_AT: (isize, isize) = (-999, 999);
/// `append_folder.dirname_level` — parent folders to append.
pub const DIRNAME_LEVEL: (usize, usize) = (1, 10);
/// `auto_date.auto_date_offset` — extra hours on top of local time.
pub const AUTO_DATE_OFFSET: (i32, i32) = (-8760, 8760);
/// `filters.min_name_len` / `filters.max_name_len`.
pub const NAME_LEN: (usize, usize) = (0, 999);
/// `filters.min_path_len` / `filters.max_path_len`.
pub const PATH_LEN: (usize, usize) = (0, 999);
/// `filters.filter_level` — maximum nesting depth.
pub const FILTER_LEVEL: (usize, usize) = (0, 99);
/// `move_copy` start/length/destination values.
pub const MOVE_COPY_VALUE: (isize, isize) = (-9999, 9999);
/// `name_segment.copy_name_segment_from` / `copy_name_segment_to`.
pub const NAME_SEGMENT: (isize, isize) = (-9999, 9999);
/// `numbering.numbering_at` — insert position.
pub const NUMBERING_AT: (isize, isize) = (-999, 999);
/// `numbering.numbering_start`.
pub const NUMBERING_START: (usize, usize) = (0, 99999);
/// `numbering.numbering_increment`.
pub const NUMBERING_INCREMENT: (isize, isize) = (-999, 999);
/// `numbering.numbering_pad` — zero-padding width.
pub const NUMBERING_PAD: (usize, usize) = (0, 20);
/// `numbering.numbering_break` — reset every N files.
pub const NUMBERING_BREAK: (usize, usize) = (0, 999);
/// `special.timestamp_incr_secs` — per-file timestamp increment.
pub const TIMESTAMP_INCR_SECS: (u32, u32) = (0, 86400);
/// Magnitude bound for `TimestampSpec::Delta`, i.e. the GUI delta editor's
/// maximum (99999 days, 23 h, 59 m, 59 s) composed to seconds.
pub const TIMESTAMP_DELTA_MAX_SECS: i64 = 8_639_999_999;
