use crate::gui::helpers::{FrontEndDiff, diff_colored, select_display_range};
pub use crate::gui::persist::*;
pub use crate::gui::types::*;
use awara::FuzzyIndex;
use awara::PathPool;
use awara::{
    AddSection, CaseSection, ExtensionSection, FiltersSection, MoveCopySection, MoveCopyValue,
    NameSegmentSection, RemoveSection, SpecialSection,
};
use awara::{
    Collision, CollisionReason, CollisionStrategy, CompiledConfig, EntryFilter, RenameConfig,
    RenameItem, RenameOp, RenameOptions, ScanOptions, ScanSortBy, TimestampSpec,
    apply_ops_chain_safe, calculate_rename, config_has_numbering, get_drive_roots, invert_op,
    is_hidden, nat_cmp, opt_num_default, opt_str_field_default, scan_directory, tree_root,
};
use eframe::egui;
use rayon::prelude::*;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::UNIX_EPOCH;

mod collision; // collision-resolution dialog
mod eframe_impl; // per-frame draw entry point
mod impl_basic; // construction + config/command order
mod impl_edit; // inline rename editing, clipboard, properties, trash
mod impl_execute; // execute / revert rename batches
mod impl_misc; // rekey, context actions, selection counts
mod impl_nav; // directory navigation
mod impl_preview; // preview and rows-cache updates
mod impl_reorder; // custom display-order manipulation
mod impl_scan; // directory scanning and filters
mod impl_state; // commit history, names, status, persistence
mod impl_timestamps; // timestamp edit state
mod impl_tree; // directory-tree navigation state
mod impl_undo; // undo / redo history
mod os; // platform glue: trash, clipboard, open-in-file-manager, Windows FFI
mod preview; // preview computation
mod rows_cache; // background row builder
mod section_helpers; // section reset/cleanup helpers
mod tree; // directory-tree helpers

// Items shared with the other GUI panels.
pub(crate) use collision::{CollisionDialogState, LiveCollisionReason};
pub(crate) use os::{copy_text, open_in_os, open_parent_and_select, open_parent_in_os, trash_file};
pub(crate) use preview::{PreviewParams, compute_preview};
pub(crate) use rows_cache::{RowsCacheRequest, RowsDatasetFp, build_rows_cache};
pub(super) use section_helpers::cleared_working_config;
pub(crate) use section_helpers::{
    compose_move_copy_section, effective_normalized_config, merge_section_order,
    reset_auto_date_edit_state, reset_move_copy_edit_state, reset_section, section_is_modified,
};
pub(crate) use tree::{
    dir_has_subdirs, maybe_shrink, nearest_existing_ancestor, scan_tree_children,
};

#[cfg(test)]
mod tests_execute_apply;
#[cfg(test)]
mod tests_execute_collision;
#[cfg(test)]
mod tests_execute_trash;
#[cfg(test)]
mod tests_keyboard;
#[cfg(test)]
mod tests_parallel;
#[cfg(test)]
mod tests_rebuild;
#[cfg(test)]
mod tests_rekey_history;
#[cfg(test)]
mod tests_revert;
#[cfg(test)]
mod tests_revert_dialog_confirm;
#[cfg(test)]
mod tests_revert_dialog_rekey;
#[cfg(test)]
mod tests_scan_filters;
#[cfg(test)]
mod tests_scan_results;
#[cfg(test)]
mod tests_selection_edit;
#[cfg(test)]
mod tests_selection_move;
#[cfg(test)]
mod tests_sort_order;
#[cfg(test)]
mod tests_state_config;
#[cfg(test)]
mod tests_state_persist_reset;
#[cfg(test)]
mod tests_state_persist_save;
#[cfg(test)]
mod tests_sync;
#[cfg(test)]
mod tests_tree_probe;
#[cfg(test)]
mod tests_tree_scan;
#[cfg(test)]
mod tests_undo_redo_files;
#[cfg(test)]
mod tests_undo_redo_swap;
#[cfg(test)]
mod tests_util;
#[cfg(test)]
mod tests_workers;

/// True when two preview row lists are equivalent for rendering. Used to skip
/// a row-cache rebuild when a recompute produced identical output.
///
/// Single definition so the async path (`check_scan_results`) and the sync path
/// (`compute_preview_sync`) cannot disagree about what counts as a change.
pub(crate) fn preview_rows_equal(a: &[PreviewItem], b: &[PreviewItem]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b.iter()).all(|(a, b)| {
            a.index == b.index
                && a.selected == b.selected
                && a.new_name == b.new_name
                // Catches a changed file set: the rows' current names are part of
                // what "same output" means, not just the generated names.
                && a.original_name == b.original_name
        })
}

pub(crate) const SELECTION_DISPATCH_THROTTLE: std::time::Duration =
    std::time::Duration::from_millis(80);

/// True when an active box-select drag should defer the background
/// rebuild/preview dispatch until the throttle window passes or the drag ends.
pub(crate) fn defer_selection_dispatch(
    dragging: bool,
    last: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    dragging && last.is_some_and(|t| now.saturating_duration_since(t) < SELECTION_DISPATCH_THROTTLE)
}

/// True when a deferred selection change should finally be dispatched: the
/// drag ended, or the throttle window passed even while still dragging.
pub(crate) fn should_flush_selection_dispatch(
    pending: bool,
    dragging: bool,
    last: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    pending
        && (!dragging
            || last.is_none_or(|t| now.saturating_duration_since(t) >= SELECTION_DISPATCH_THROTTLE))
}
pub(crate) const DEFAULT_STATUS: &str = "Ready";

/// Status-bar text shown while a directory scan is in flight. Replaces the
/// previous status for the duration of the scan; the previous status is
/// restored when the scan finishes or is cancelled.
pub(crate) const SCANNING_STATUS: &str = "Scanning... (Esc to cancel)";

/// Snapshot of the status bar taken when a scan starts, so the scanning
/// message can be swapped back to the previous status once the scan ends.
#[derive(Clone)]
struct SavedStatus {
    message: String,
    kind: StatusKind,
    timestamp: Option<std::time::Instant>,
    last_ok_message: String,
}

/// Default values for the GUI view-settings toggles shown in the Settings popup.
/// SSoT: both the struct initializer and the "Reset to defaults" button load from here.
pub(crate) const DEFAULT_VIEW_SETTINGS: ViewSettings = ViewSettings {
    auto_refresh: true,
    remember_rename_options: false,
    remember_last_dir: true,
    skip_trash_confirmation: false,
    skip_dir_creation_warning: false,
};

/// Maximum number of undo/revert commits retained per directory (oldest pruned FIFO).
pub const MAX_COMMITS_PER_DIR: usize = 50;
/// Maximum number of distinct directory commit histories retained globally (LRU eviction).
pub const MAX_COMMITS_DIRS: usize = 20;

/// Subset of `GuiApp` fields exposed in the Settings popup.
#[derive(Clone, Copy)]
pub(crate) struct ViewSettings {
    pub auto_refresh: bool,
    pub remember_rename_options: bool,
    pub remember_last_dir: bool,
    pub skip_trash_confirmation: bool,
    pub skip_dir_creation_warning: bool,
}

pub struct GuiApp {
    // ── Navigation / File list ──
    pub cwd: PathBuf,
    pub cwd_input: String,
    pub all_files: Vec<PathBuf>,
    /// Cached stringified paths — avoids repeated to_string_lossy allocations.
    pub file_names: Vec<String>,
    /// Live fuzzy index over `file_display_names` — rebuilt after each directory scan
    /// to allow instant sub-millisecond filename filtering on 1M+ items.
    pub fuzzy_index: FuzzyIndex,
    /// Shared parent-directory pool — deduplicates parent Arc<Path> allocations
    /// across all files, reducing RAM by >50% in deep directory trees.
    pub path_pool: PathPool,
    /// Pre-computed display names (file_name portion of path as String).
    /// Used in `sorted_indices()` to avoid `to_string_lossy()` in the hot
    /// sort comparator — over 161k files that malloc churn adds up.
    file_display_names: Vec<String>,
    pub selection: Vec<bool>,
    pub file_sizes: Vec<u64>,
    pub file_dates: Vec<i64>,
    /// Cached is_dir flag for each file, populated during scan to avoid
    /// redundant stat calls in rebuild_rows_cache.
    pub file_is_dir: Vec<bool>,
    pub scan_opts: ScanOptions,
    /// Raw unfiltered scan results — used for instant in-memory refiltering
    /// when non-structural filter fields change (mask, condition, exclude, etc.)
    /// without re-scanning the filesystem.
    pub raw_all_files: Vec<PathBuf>,
    raw_file_sizes: Vec<u64>,
    raw_file_dates: Vec<i64>,
    raw_file_is_dir: Vec<bool>,
    pub scanning: bool,
    pub scan_cancel: Arc<AtomicBool>,
    pub scan_error: Option<String>,
    scan_result_tx: mpsc::Sender<ScanResult>,
    scan_result_rx: mpsc::Receiver<ScanResult>,

    // ── Tree navigator ──
    pub tree_expanded: HashSet<PathBuf>,
    pub tree_children: HashMap<PathBuf, Vec<PathBuf>>,
    pub tree_scanned: HashSet<PathBuf>,
    /// Directories with a tree scan currently in flight, mapped to the
    /// generation of that request. The generation is echoed back in the result
    /// and must match for the result to be applied, so a scan read from disk
    /// before a trash/rename/refresh invalidated the node can't resurrect stale
    /// children. The generation is monotonic (see [`Self::tree_scan_gen_seq`])
    /// so a discarded one is never reused.
    pub tree_pending: HashMap<PathBuf, u64>,
    /// Monotonic counter feeding the generations in [`Self::tree_pending`].
    tree_scan_gen_seq: u64,
    tree_scan_tx: mpsc::Sender<(PathBuf, bool, u64)>,
    tree_scan_rx: mpsc::Receiver<(PathBuf, u64, Vec<PathBuf>)>,
    /// Cached "contains visible subdirectories?" answer for collapsed tree
    /// nodes, filled by a one-shot synchronous probe the first time a node is
    /// rendered, so the expand arrow is correct on the very first frame (no
    /// optimistic flash) and leaf folders never show an arrow.
    pub tree_has_subdirs: HashMap<PathBuf, bool>,
    /// The `filter_hidden` value the cached probe answers were computed under.
    /// Probes are invalidated only when it changes, not on every navigation.
    pub tree_probe_hidden: Option<bool>,
    /// Path the tree auto-scrolls its viewbox to once the node renders: the
    /// cwd on open/navigation/refresh, or the nearest existing ancestor when
    /// a refresh finds the cwd deleted externally (viewbox only — cwd is left
    /// unchanged). Cleared once the scroll happens.
    pub tree_scroll_target: Option<PathBuf>,

    // ── Rename config ──
    pub config: RenameConfig,

    // ── GUI editing state (combo fields for auto-date, move/copy) ──
    pub edit_state: GuiEditState,

    // ── UI filter / ephemeral form state (persisted alongside config) ──
    pub undo_file: String,

    pub timestamps: GuiTimestampsState,

    // Preview
    pub preview: Arc<Vec<PreviewItem>>,
    pub cached_rows: std::rc::Rc<Vec<Row>>,
    pub preview_dirty: bool,
    pub scan_generation: u64,
    pub preview_generation: u64,
    pub preview_last_gen: u64,
    pub preview_pending: bool,
    /// The full planner inputs of the last dispatch, with the config as the
    /// *length-clamped* form. Collapses what used to be five loose fields
    /// (section enablement, selection, file count, processed, order) into the
    /// same struct the gate compares, so they cannot drift apart.
    ///
    /// The clamped config is deliberate: the gate may treat a clamp-only edit as
    /// output-neutral (that is what keeps slider drags cheap).
    last_dispatched_key: Option<PreviewKey>,
    /// The last *dispatched* operation fingerprint, used to re-arm the processed
    /// guard on a user edit. A separate concern from the preview gate: an edit
    /// that clamps to the same effective value still re-arms the guard.
    last_dispatched_working: Option<RenameConfig>,
    last_dispatched_section_enabled: Option<SectionEnabled>,
    /// After a successful Apply the whole selected batch is marked processed:
    /// the preview freezes to identity (New Name = current filename) and Apply
    /// becomes a no-op until the user reselects files or changes the operation.
    /// Prevents accidental double-application of the same rename operation.
    pub processed: bool,
    /// Bumped whenever `processed` flips; forces a preview redispatch even when
    /// config, selection, and file count are all unchanged (the post-Apply case).
    pub processed_generation: u64,
    preview_tx: mpsc::Sender<PreviewRequest>,
    preview_rx: mpsc::Receiver<(u64, Vec<PreviewItem>)>,
    /// Shared cancel flag for the row-cache background thread.
    /// Set by the main thread when navigating to a new directory, so a
    /// running 161k `build_rows_cache` abandons work at the next chunk
    /// boundary instead of grinding to completion.
    rows_cache_cancel: Arc<AtomicBool>,
    /// Background row-cache builder channel — sends source data, receives `(generation, rows)`.
    rows_cache_tx: mpsc::Sender<RowsCacheRequest>,
    rows_cache_rx: mpsc::Receiver<(u64, std::sync::Arc<Vec<Row>>)>,
    /// True while a row-cache build request is in-flight.
    pub rows_cache_pending: bool,
    /// Generation counter for row-cache results — incremented on each request.
    /// Stale results from prior generations are discarded by `check_rows_cache_result`.
    rows_cache_gen: u64,
    /// Bumped by every full (non-selection-only) row-cache rebuild. Part of the
    /// `RowsDatasetFp` so the incremental selection-only fast path never carries
    /// rows over across an in-place file mutation (e.g. an F2 rename).
    rows_dataset_gen: u64,
    pub preview_sort_col: PreviewSortCol,
    pub preview_sort_asc: bool,
    /// Bumped every time the display order is rebuilt (sort click, manual
    /// reposition, rescan, filter). Part of `PreviewKey`, because numbering
    /// follows display order and therefore the order is a planner input.
    pub order_generation: u64,
    /// The current display order — the single source of truth for row order.
    /// Written only by `get_display_order()`, and only when
    /// `display_order_dirty` is set; the flag is set only by
    /// `invalidate_sorted_cache()`.
    display_order_cache: std::rc::Rc<Vec<usize>>,
    /// Set when the display order must be rebuilt; cleared by
    /// `get_display_order()`.
    display_order_dirty: std::cell::Cell<bool>,

    // Column layout
    pub layout: GuiLayoutState,
    /// Revert-dialog column widths (persisted separately in the UI config).
    pub revert_col_widths: Vec<f32>,
    // UI state
    pub section_enabled: SectionEnabled,
    pub status_message: String,
    pub status_kind: StatusKind,
    pub status_timestamp: Option<std::time::Instant>,
    /// Saved so we can restore it when a temporary/error message times out.
    pub last_ok_message: String,
    /// Status-bar snapshot captured when a scan starts; restored when the
    /// scan finishes or is cancelled so the scanning message doesn't linger.
    /// `None` while idle or when a scan is running without a snapshot
    /// (e.g. tests that set `scanning` directly).
    status_before_scan: Option<SavedStatus>,
    pub auto_refresh: bool,

    // Special windows — only one popup open at a time.
    pub active_popup: Option<PopupKind>,

    // Execution
    pub executing: bool,
    /// Commits grouped by the directory they were executed in.
    /// Key is the canonical cwd at the time of the rename.
    pub commits_by_dir: HashMap<PathBuf, Vec<Commit>>,
    /// LRU access list of directories in `commits_by_dir` for memory-bounded eviction.
    pub commit_dir_lru: Vec<PathBuf>,
    /// Redo stack for future Ctrl+Shift+Z (per-session, cleared on branch).
    pub redo_stack: Vec<Commit>,
    /// Cached status map: file_path → (0=success, 1=fail, 2=skip, Option<error_msg>).
    /// Rebuilt lazily when dirty. Error messages are stored inline (SSoT).
    pub status_map: HashMap<String, (u8, Option<String>)>,
    /// True when commits have changed and status_map must be rebuilt.
    pub status_map_dirty: bool,
    /// Separate status map for the revert dialog, independent of the main status_map.
    /// Cleared each time the revert dialog opens or closes; written by confirm_revert.
    pub revert_status_map: HashMap<String, (u8, Option<String>)>,
    /// Current revert dialog state, if open.
    pub revert_dialog: Option<RevertDialogState>,
    /// Collision resolution dialog state. When set, blocks Apply/F2/selection.
    pub collision_dialog: Option<CollisionDialogState>,
    /// The plan the current collision dialog is resolving. Kept so execution
    /// reuses the exact ops that were checked for collisions (no recompute).
    pub pending_plan: Option<awara::RenamePlan>,

    // Row selection — click toggles, shift-click ranges, ctrl-click toggles multi
    /// Last file index clicked or navigated to (the "active edge" of the selection).
    /// For shift-click/ranges, the anchor (fixed end) is in `selection_anchor`.
    pub last_clicked_idx: Option<usize>,
    /// Anchor for shift-click and shift+arrow range selection.
    /// Set on first click of a range action; cleared on single/toggle click.
    /// The range extends from this anchor to `last_clicked_idx` in display order.
    pub selection_anchor: Option<usize>,
    /// Incremented on every selection mutation. Used to avoid scanning all
    /// preview items for the "did selection change?" check every frame.
    pub selection_generation: u64,
    /// The selection_generation value at the last time we checked for changes.
    pub last_seen_selection_gen: u64,
    /// True while a box-select drag is active (pointer down in the preview).
    /// Used to throttle selection-driven background rebuilds during drags so a
    /// full rebuild is not dispatched on every mouse-move frame.
    pub selection_dragging: bool,
    /// A selection change was deferred by the drag throttle and still needs to
    /// be dispatched once the drag ends or the throttle window passes.
    pub selection_dispatch_pending: bool,
    /// When the last selection-driven background dispatch happened.
    pub last_selection_dispatch: Option<std::time::Instant>,
    /// Cached selected count to avoid O(N) iteration in status bar every frame.
    pub cached_selected_count: Cell<Option<usize>>,
    pub cached_selected_gen: Cell<u64>,
    pub cached_selected_len: Cell<usize>,
    /// Cached modified count to avoid O(N) iteration in status bar every frame.
    pub cached_modified_count: Cell<Option<usize>>,
    pub cached_modified_gen: Cell<u64>,
    pub cached_modified_len: Cell<usize>,

    /// File index being dragged for reorder, or None if no drag is active.
    pub reorder_drag_idx: Option<usize>,

    // ── F2 inline edit ──
    /// File index of the row currently being edited via F2, or None.
    pub editing_idx: Option<usize>,
    /// Buffer for the inline rename text edit.
    pub edit_buffer: String,
    /// Set to true on first frame of a new F2 edit to request focus + selection.
    pub edit_pending_focus: bool,
    /// The screen rect of the row currently being F2-edited, set each frame during rendering.
    pub editing_row_rect: Option<egui::Rect>,

    /// Set to true whenever the file preview section is clicked (row or empty
    /// space). Used to gate keyboard shortcuts like Ctrl+A so they only fire
    /// when the user has interacted with the file list.
    pub preview_active: bool,

    /// Set to true during `enter_dir()` so `check_scan_results` knows the scan
    /// was triggered by user navigation and should restore keyboard focus.
    pub navigating_to_dir: bool,

    /// When edge-scrolling a box selection, tracks when the drag reached the edge
    /// so we can accelerate the scroll speed progressively.
    pub preview_edge_drag_start: Option<std::time::Instant>,
    /// When set, the preview table should scroll to make this file index visible.
    /// Set by keyboard navigation, consumed by the scroll area each frame.
    pub scroll_to_idx: Option<usize>,
    /// Timestamp of the first arrow-key press in a held sequence (frozen until release).
    /// Used to compute how long the user has been holding.
    pub scroll_sequence_start: Option<std::time::Instant>,
    /// Timestamp of the most recent arrow-key press — used for release detection.
    pub scroll_last_event: Option<std::time::Instant>,
    /// Display-order position where the box selection started (captured once, not per frame).
    /// Used by `handle_box_selection_by_display_order` as the anchor end of the range.
    /// `isize` because the anchor may fall outside `[0, len)` (above/below the list).
    pub preview_box_select_anchor: Option<isize>,

    // ── Context menu / reposition / properties ──
    /// Manual row order override. None = use column sort. Some = user-repositioned.
    pub custom_order: Option<Vec<usize>>,
    pub show_properties_idx: Option<usize>,
    /// Data for the multi-file summary properties popup, set by context menu.
    pub show_properties_multi_data: Option<PropertiesMultiData>,
    /// Path whose properties popup is open, set by the tree navigator context
    /// menu. Tree entries are not necessarily part of `all_files`, so they
    /// cannot be addressed through `show_properties_idx`.
    pub show_tree_properties: Option<PathBuf>,

    /// Deferred context-menu action, processed after the scroll area closes.
    pub pending_context_action: Option<ContextAction>,
    /// Snapshot of selected indices captured when `pending_context_action` was set.
    /// Prevents row-click race conditions from corrupting the selection before
    /// `apply_context_action` processes the deferred action.
    pub pending_sel_indices: Vec<usize>,
    /// Set to true when a context menu action is triggered this frame.
    /// Prevents the empty-space click handler from clearing the selection.
    pub context_menu_action_taken: bool,

    // ── Persistence ──
    pub remember_rename_options: bool,
    pub remember_last_dir: bool,
    /// Override config directory for tests. `None` uses `dirs::config_dir()`.
    pub(crate) config_dir: Option<PathBuf>,

    /// Basenames to pre-select after the next directory scan completes.
    /// Set by `run_with_files` context-menu entry; consumed in check_scan_results.
    pub pending_preselection: Option<Vec<String>>,

    /// When true, show the "confirm trash" modal dialog.
    pub show_trash_confirmation: bool,
    /// One-shot flag: request focus on the trash confirm button on first frame.
    pub trash_init_focus: bool,
    /// When true, skip the trash confirmation dialog and trash immediately.
    pub skip_trash_confirmation: bool,
    /// Directory the pending trash confirmation targets, set by the tree
    /// navigator context menu. `None` means the confirmation is for the
    /// current file selection.
    pub trash_target: Option<PathBuf>,

    /// When true, show the modal listing computed names that would *navigate* the
    /// path (rooted or `.`/`..`). Such a batch cannot be applied, so Apply is
    /// blocked until the operation is fixed. Entries are `(current, detail)`.
    pub show_invalid_name_warning: bool,
    /// One-shot flag: request focus on the invalid-name dialog's button.
    pub invalid_name_init_focus: bool,
    pub invalid_name_entries: Vec<(String, String)>,

    /// When true, show the "this rename will create folders" confirmation before
    /// applying. Skippable like the trash confirmation.
    pub show_dir_creation_warning: bool,
    /// One-shot flag: request focus on the folder-creation dialog's button.
    pub dir_warning_init_focus: bool,
    /// When true, skip the folder-creation confirmation (persisted).
    pub skip_dir_creation_warning: bool,
    /// The validated plan awaiting folder-creation confirmation, plus how it
    /// should be dispatched once confirmed.
    pub pending_dir_plan: Option<awara::RenamePlan>,
    pub pending_dir_label: String,
    pub pending_dir_inline: bool,
    /// How many ops in the pending plan create folders (for the message).
    pub pending_dir_count: usize,
    /// F2 collision state held while the folder-creation warning is shown, so an
    /// inline rename that also collides still opens the **F2** dialog (keeping
    /// its inline semantics) once the warning is confirmed.
    pub(crate) pending_f2_collision: Option<CollisionDialogState>,
}

impl Default for GuiApp {
    fn default() -> Self {
        Self::new()
    }
}
