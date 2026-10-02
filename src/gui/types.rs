use awara::{MoveCopyValue, PermSnapshot, RenameConfig, RenameOp, config_has_numbering};
use eframe::egui::Color32;
use std::path::PathBuf;

/// Section identity, enable flags, and the all-enabled default all live in the
/// core crate so the GUI and the CLI/TUI fallback cannot drift.
pub use awara::{DEFAULT_SECTION_ENABLED, SectionEnabled, SectionId};

/// Result sent from the background scan thread to the main thread.
pub(crate) struct ScanResult {
    pub files: Vec<PathBuf>,
    pub sizes: Vec<u64>,
    pub dates: Vec<i64>,
    /// Whether each file is a directory (populated from metadata to avoid
    /// redundant stat calls in rebuild_rows_cache).
    pub is_dirs: Vec<bool>,
    /// Generation counter used to discard stale results from old scan threads.
    /// Matches `GuiApp::scan_generation` at the time the thread was spawned.
    pub r#gen: u64,
    /// If `true`, this is a partial streaming result (first chunk only) sent
    /// before the complete scan finishes, so the UI can render immediately.
    pub is_partial: bool,
}

/// The planner's inputs *other than* the display order.
///
/// The derived `PartialEq` is the single, complete definition of "same inputs":
/// a field added here is picked up by the preview dispatch gate
/// ([`PreviewKey::same_names`]) automatically, so the gate cannot drift apart
/// from what the planner actually reads.
///
/// Cheap generation counters precede `working` so the derived comparison
/// short-circuits before the deep config compare.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlanInputs {
    pub section_enabled: SectionEnabled,
    /// Which files are selected.
    pub selection_generation: u64,
    /// Which files exist (and their names): incremented by every scan/filter.
    pub scan_generation: u64,
    /// Whether the batch is frozen to identity rows.
    pub processed_generation: u64,
    pub file_count: usize,
    /// Output-dir keep-structure paths resolve against the cwd.
    pub cwd: PathBuf,
    /// Working config (disabled sections cleared, `command_order` populated).
    pub working: RenameConfig,
}

/// Everything the preview computes from: the planner's inputs plus the display
/// order.
///
/// This is the dispatch gate's identity — "would a recompute produce different
/// rows?" — and nothing else. Applying always re-plans, so there is no second
/// consumer that could treat it as a licence to reuse a stored plan.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreviewKey {
    pub inputs: PlanInputs,
    /// Bumped whenever the display order is rebuilt (sort, reposition, rescan).
    /// The planner numbers in display order, so this is a plan input.
    pub order_generation: u64,
}

impl PreviewKey {
    /// True when numbering follows the display order, which is what makes the
    /// order a name-affecting input rather than only a collision-order input.
    pub(crate) fn numbering_follows_order(&self) -> bool {
        self.inputs.section_enabled[SectionId::Numbering as usize]
            && config_has_numbering(&self.inputs.working)
    }

    /// Gate for skipping a preview recompute.
    ///
    /// Differs from `==` in exactly one way: without numbering the display order
    /// cannot change any generated name (each name is computed from its own file
    /// alone), so a reorder needs no recompute. The order is still a planner
    /// input either way — Apply materializes it itself — so this tolerance can
    /// only skip work, never change a result.
    pub(crate) fn same_names(&self, other: &Self) -> bool {
        self.inputs == other.inputs
            && (!self.numbering_follows_order() || self.order_generation == other.order_generation)
    }
}

/// Request sent from the main thread to the background preview thread.
pub(crate) struct PreviewRequest {
    pub files: Vec<String>,
    pub config: RenameConfig,
    pub section_enabled: SectionEnabled,
    pub selection: Vec<bool>,
    /// Display-order indices for numbering (the frozen display order).
    pub num_order: Vec<usize>,
    pub generation: u64,
    /// Current working directory (used for output-dir keep_structure).
    pub cwd: PathBuf,
    /// True when the last Apply processed this batch: the preview must render
    /// identity ops (New Name = current filename) so re-applying is a no-op
    /// until the user reselects files or changes the operation.
    pub processed: bool,
}

/// Background for "kept" diff segments — no highlight, theme default text.
/// Defined here (rather than in helpers) so it can be referenced from
/// both the diff computation and the render site without cross-module coupling.
pub(crate) const DIFF_KEEP_BG: Color32 = Color32::TRANSPARENT;

/// Translucent red background for characters removed from the name.
/// Translucency lets the highlight composite over panel and selection
/// backgrounds in both light and dark themes (unmultiplied rgba: 220, 80, 80, 90).
pub(crate) const DIFF_REMOVE_BG: Color32 = Color32::from_rgba_premultiplied(78, 28, 28, 90);

/// Translucent green background for characters added to the name.
/// (unmultiplied rgba: 46, 160, 67, 90)
pub(crate) const DIFF_ADD_BG: Color32 = Color32::from_rgba_premultiplied(16, 56, 24, 90);

/// Shared green used for status "OK" labels.
pub(crate) const GREEN: Color32 = Color32::from_rgb(17, 179, 84);

/// Return the shared selection/highlight background colour.
/// Used for both preview table rows and the tree navigator.
pub(crate) fn selection_bg(dark_mode: bool) -> Color32 {
    if dark_mode {
        Color32::from_rgb(40, 44, 52)
    } else {
        Color32::from_rgb(234, 240, 255)
    }
}

/// A segment of text with an optional foreground override and a background
/// highlight, used for diff rendering.
///
/// - `fg` is `None` for diff segments (theme default text colour) and `Some`
///   only for non-diff decorations such as the file/folder icon.
/// - `bg` is the highlight colour: `DIFF_KEEP_BG` (transparent) for kept text,
///   `DIFF_REMOVE_BG`/`DIFF_ADD_BG` for removed/added text. Highlighting the
///   background rather than the text colour makes changes to whitespace visible.
///
/// Defined here (rather than in helpers) to make it available in `Row` without
/// cross-module coupling.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DiffSeg {
    pub text: String,
    pub fg: Option<Color32>,
    pub bg: Color32,
}

/// Pre-built row data for the file preview table. Rebuilt when data changes, not every frame.
#[derive(Clone)]
pub struct Row {
    pub idx: usize,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub date: i64,
    pub new_name: String,
    pub changed: bool,
    /// None=pending, Some(0)=success(✓), Some(1)=failed(✗), Some(2)=skipped
    pub status: Option<u8>,
    /// Precomputed diff segments for the original name (None if not changed).
    /// Memoized to avoid recomputing diffs every frame.
    pub orig_segs: Option<Vec<DiffSeg>>,
    /// Precomputed diff segments for the new name (None if not changed).
    pub new_segs: Option<Vec<DiffSeg>>,
    /// Error message if this file failed to rename (None otherwise).
    pub error_msg: Option<String>,
    /// True when the file no longer exists on disk (deleted outside the GUI).
    /// The row is kept in the list so the user can see what was removed,
    /// but it is rendered with a strikethrough to signal it is unavailable.
    pub missing: bool,
}

#[derive(Debug, Clone)]
pub struct PreviewItem {
    /// The file's current name.
    pub original_name: String,
    /// The planned new name, or `None` when the row has no rename (the file is
    /// unselected or already processed).
    ///
    /// Rows are deliberately *not* a copy of the plan's `RenameOp`. There is one
    /// row per file in the listing — an identity row for every unselected file —
    /// while an op carries two `PathBuf`s, a permission snapshot and attributes
    /// that no row consumer reads. Keeping only what the table shows is what
    /// makes the common case (few selected of many listed) cheap, and lets a
    /// changed row copy just its two names instead of the whole op.
    ///
    /// Measured allocations for a 2000-file preview (one per file): 6.5 → 3.5
    /// with nothing selected, 6.8 → 3.8 with 1% selected, 29.2 → 27.2 with all
    /// selected (where planning work dominates). See `test_row_stays_name_sized`.
    pub new_name: Option<String>,
    pub selected: bool,
    /// File index into `all_files`.
    pub index: usize,
}

impl PreviewItem {
    /// The name this row displays: identity rows show their current name.
    pub fn new_name(&self) -> &str {
        self.new_name.as_deref().unwrap_or(&self.original_name)
    }

    /// True when the displayed new name differs from the current name. An op can
    /// legitimately keep the name and still be present (attributes/timestamps),
    /// which is not a visible change.
    pub fn is_changed(&self) -> bool {
        self.new_name
            .as_deref()
            .is_some_and(|n| n != self.original_name)
    }
}

/// A context-menu action deferred for processing outside render borrow scope.
///
/// Variants that carry a `usize` are single-file actions (only the right-clicked
/// file is affected). Variants without an index operate on all selected files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    /// Reset custom display order back to sorted order.
    ResetOrder,
    /// Move all selected items up one display position.
    MoveUp,
    /// Move all selected items down one display position.
    MoveDown,
    /// Move all selected items to the top of the display order.
    MoveToTop,
    /// Move all selected items to the bottom of the display order.
    MoveToBottom,
    /// Trash all selected files (with or without confirmation dialog).
    Trash,
    /// Open all selected files with the default OS application.
    OpenFiles,
    /// Open the containing folder for each selected file.
    OpenFolders,
    /// Show summary properties (count, total size) for all selected files.
    Properties,
    /// Single-file inline rename (only the right-clicked file, not the whole selection).
    Rename(usize),
    /// Undo the last rename for every selected file that can be undone.
    /// Files without an undo operation are silently skipped.
    UndoFiles,
    /// Redo the last undone rename for every selected file that can be redone.
    /// Files without a redo operation are silently skipped.
    RedoFiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum PreviewSortCol {
    #[default]
    Name,
    NewName,
    Type,
    Size,
    Date,
    Status,
}

/// Column descriptor for the preview table.
pub const COL_IDS: [PreviewSortCol; 6] = [
    PreviewSortCol::Name,
    PreviewSortCol::NewName,
    PreviewSortCol::Type,
    PreviewSortCol::Size,
    PreviewSortCol::Date,
    PreviewSortCol::Status,
];

pub const COL_LABELS: [&str; 6] = ["Name", "New Name", "Type", "Size", "Modified", "Status"];

// Re-export NUMBERING_BASES from the core SSoT
pub use awara::NUMBERING_BASES;

pub const COL_DEFAULT_WIDTHS: [f32; 6] = [180.0, 180.0, 60.0, 80.0, 150.0, 60.0];

/// Minimum column width enforced by the drag-to-resize handles (main table and revert dialog).
pub const COL_MIN_WIDTH: f32 = 40.0;

/// Uniform inner padding around the preview table and the revert-dialog table.
/// Matches the tree navigator panel's default frame margin so headers, rows,
/// and status/empty/scanning messages line up consistently across sections.
pub const TABLE_INNER_MARGIN: i8 = 8;

/// Default widths for the revert-dialog table: [Current, Rollback, Original, Attr, Timestamps, Status].
/// SSoT for both the initial dialog state and the persisted UI config.
pub const REVERT_COL_DEFAULT_WIDTHS: [f32; 6] = [170.0, 140.0, 120.0, 90.0, 100.0, 110.0];

/// Categorises a status-bar message by its behaviour and appearance.
///
/// - `Persistent` — stays until replaced by another persistent or error message.
/// - `Temporary` — auto-clears after a timeout, restoring the previous persistent message.
/// - `Error` — auto-clears after a timeout (same as Temporary), but renders in red.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Persistent,
    Temporary,
    Error,
}

impl StatusKind {
    /// How long this status kind is shown before auto-clearing.
    /// `Persistent` returns `None` (never auto-clears).
    pub fn timeout(&self) -> Option<std::time::Duration> {
        match self {
            StatusKind::Persistent => None,
            StatusKind::Temporary | StatusKind::Error => Some(std::time::Duration::from_secs(5)),
        }
    }
}

// ── Revert window types ──

/// A batch of rename operations applied atomically (one undoable unit).
#[derive(Clone)]
pub struct Commit {
    /// Monotonic execution timestamp
    pub timestamp: std::time::Instant,
    /// Human-readable label (e.g. "Batch — 10:32:15")
    pub label: String,
    /// Forward operations: original_path → new_path as they were applied
    pub ops: Vec<RenameOp>,
}

/// Metadata effects one commit applied to a file, read from that commit's
/// recorded `RenameOp` (the op stays the source of truth; this is the compact
/// per-entry view the revert dialog needs).
#[derive(Clone, Default)]
pub struct RevertEffect {
    /// Pre-op snapshot (readonly + mtime/atime). Restoring it undoes the effect.
    pub snapshot: Option<PermSnapshot>,
    /// Attribute string applied by the op (empty = none).
    pub applied_attributes: String,
    /// Timestamp mask applied by the op (0 = none).
    pub applied_timestamps_mask: u8,
}

impl RevertEffect {
    /// True when the op actually changed attributes or timestamps.
    pub fn has_effects(&self) -> bool {
        !self.applied_attributes.is_empty() || self.applied_timestamps_mask != 0
    }

    /// True when the effect can be rolled back (a pre-op snapshot was captured).
    pub fn is_restorable(&self) -> bool {
        self.has_effects() && self.snapshot.is_some()
    }
}

/// One file entry in the revert dialog.
#[derive(Clone)]
pub struct RevertEntry {
    pub current_path: PathBuf,
    pub current_name: String,
    /// [(path, name, producing_commit_idx_or_none_for_original)]
    /// Lineage[0] = original, lineage[last] = current
    pub lineage: Vec<(PathBuf, String, Option<usize>)>,
    /// Precomputed target name for each commit index:
    /// target_by_commit[i] = name at state just before commit i
    pub target_by_commit: Vec<String>,
    /// Metadata effects applied to this file by each commit (index = commit
    /// index), read from the producing op. `None` = that commit didn't change
    /// attributes/timestamps. This is the single per-commit effect view; it
    /// replaces the old `attr_summary`/`ts_summary` pair, which always showed
    /// the newest commit regardless of which commit was selected.
    pub effects_by_commit: Vec<Option<RevertEffect>>,
    /// Is revert viable?
    pub revertable: bool,
    /// If not revertable, why?
    #[allow(dead_code)]
    pub block_reason: Option<String>,
}

impl RevertEntry {
    /// True when `commit_idx` has anything to roll back for this entry: a name
    /// change, or restorable metadata effects (attributes/timestamps applied
    /// without a rename). Single predicate for selection, display, and confirm.
    pub fn has_change_at(&self, commit_idx: usize) -> bool {
        if !self.revertable {
            return false;
        }
        if self
            .target_by_commit
            .get(commit_idx)
            .is_some_and(|t| t != &self.current_name)
        {
            return true;
        }
        self.effects_by_commit
            .get(commit_idx)
            .and_then(|e| e.as_ref())
            .is_some_and(RevertEffect::is_restorable)
    }
}

/// Full state of the revert dialog.
pub struct RevertDialogState {
    pub entries: Vec<RevertEntry>,
    /// Checked = will be reverted
    pub selection: Vec<bool>,
    /// Index of the active commit in side panel (the one being assigned)
    pub active_commit_idx: usize,

    /// Whether a revert is currently executing (disable button during I/O)
    pub executing: bool,
    /// Column widths for the table (persisted across frames for drag-to-resize).
    /// [Current Name, Rollback To, Original Name]
    pub col_widths: Vec<f32>,
    /// Deferred context action: (entry_index, action_name)
    /// action_name: "copy_filename", "copy_full_path", "copy_dir_path", "open_file", "open_folder"
    pub pending_context_action: Option<(usize, String)>,
    /// Last entry index clicked or navigated to (the "active edge" of the
    /// selection). For shift-click/ranges, the anchor (fixed end) is in
    /// `selection_anchor`.
    pub last_clicked_idx: Option<usize>,
    /// Anchor for shift-click and shift-arrow range selection.
    /// Set on the first click of a range action; cleared on single/toggle
    /// click. The range extends from this anchor to `last_clicked_idx`.
    pub selection_anchor: Option<usize>,
    /// Current sort column: 0=Current Name, 1=Rollback To, 2=Original Name,
    /// 3=Attributes, 4=Timestamps.
    pub sort_col: usize,
    /// Sort ascending (true) or descending (false).
    pub sort_asc: bool,
    /// When set, show properties popup for this entry index.
    pub show_properties_idx: Option<usize>,
    /// Cached display order indices — invalidated on sort/commit change.
    pub display_order: Vec<usize>,
    /// Dirty flag for display_order + sort_keys.
    pub display_order_dirty: bool,
    /// Precomputed sort key for each entry at the current sort column + active_commit_idx.
    /// Avoids string clones in the sort comparator on every frame.
    pub sort_keys: Vec<String>,
    /// When set, scroll to this entry index in the revert table.
    pub scroll_to_idx: Option<usize>,
    /// Timestamp of the first arrow-key press in a held sequence.
    pub scroll_sequence_start: Option<std::time::Instant>,
    /// Timestamp of the most recent arrow-key press.
    pub scroll_last_event: Option<std::time::Instant>,
    /// When edge-scrolling a box selection, tracks when the drag reached the edge.
    pub edge_drag_start: Option<std::time::Instant>,
    /// Box-select anchor for display-order-aware selection (persisted across scroll).\
    /// `isize` because the anchor may fall outside `[0, len)` (above/below the list).
    pub box_select_anchor: Option<isize>,
    /// Cached selected count for O(1) toolbar label — None means stale, recompute on next frame.
    pub cached_selected_count: Option<usize>,
}

/// Timestamp-edit UI state — persisted alongside rename config.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GuiTimestampState {
    pub mode: String,
    pub fixed_date: String,
    /// Time as shown in the editor, in 12-hour `hh:mm:ss` form.
    pub fixed_time: String,
    /// Whether `fixed_time` is PM (used with the 12-hour field).
    #[serde(default)]
    pub fixed_pm: bool,
    pub incr_by: u32,
    pub delta_days: u32,
    pub delta_hours: u32,
    pub delta_mins: u32,
    pub delta_secs: u32,
    pub negative: bool,
}

impl Default for GuiTimestampState {
    fn default() -> Self {
        Self {
            mode: "no_change".into(),
            fixed_date: String::new(),
            fixed_time: String::new(),
            fixed_pm: false,
            incr_by: 0,
            delta_days: 0,
            delta_hours: 0,
            delta_mins: 0,
            delta_secs: 0,
            negative: false,
        }
    }
}

/// Timestamp UI state for all three timestamps (created, modified, accessed).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct GuiTimestampsState {
    pub created: GuiTimestampState,
    pub modified: GuiTimestampState,
    pub accessed: GuiTimestampState,
}

/// Column layout state — persisted across sessions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct GuiLayoutState {
    pub col_order: Vec<usize>,
    pub col_widths: Vec<f32>,
}

/// UI editing state for auto-date and move/copy sections.
/// These are purely GUI editing helpers — they do not belong in `RenameConfig`
/// (which is the SSoT for rename operations).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GuiEditState {
    // ── Auto-date combo state (composed into add_date/add_file_date/insert_meta) ──
    pub auto_date_type: Option<String>,
    pub auto_date_format_key: Option<String>,
    /// Separator between date components (e.g. '-' for YYYY-MM-DD).
    pub auto_date_sep: Option<String>,
    pub auto_date_custom: Option<String>,

    // ── Move/Copy combo state (composed into move_part/copy_part tuples) ──
    pub move_part_action: Option<String>,
    /// Default 1: start position or count when action is first selected (negative counts from end)
    pub move_part_val1: isize,
    pub move_part_val2: isize,
    pub move_part_dest: Option<String>,
    /// Default 1: target position when "To pos" is selected (negative counts from end)
    pub move_part_dest_val: isize,
    /// Separator inserted between the moved/copied part and the rest of the name.
    pub move_part_sep: String,
}

impl Default for GuiEditState {
    fn default() -> Self {
        let mv_def = MoveCopyValue::default();
        Self {
            auto_date_type: None,
            auto_date_format_key: None,
            auto_date_sep: None,
            auto_date_custom: None,
            move_part_action: None,
            move_part_val1: mv_def.start,
            move_part_val2: mv_def.length,
            move_part_dest: None,
            move_part_dest_val: mv_def.destination,
            move_part_sep: String::new(),
        }
    }
}

/// Data for the multi-file summary properties popup.
#[derive(Clone)]
pub struct PropertiesMultiData {
    pub count: usize,
    pub total_size: u64,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PopupKind {
    OrderEditor,
    AttrEditor,
    TsEditor,
    About,
    RevertDialog,
    Settings,
}
