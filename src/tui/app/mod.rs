use crate::args::Args;
use crate::tui::parser;
use awara::{
    CompiledConfig, EntryFilter, PlanOptions, RenameConfig, ScanOptions, ScanSortBy, plan_renames,
    scan_directory, sort_paths,
};
use clap::CommandFactory;
use ratatui::text::Span;
use ratatui::widgets::{ListState, TableState};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

mod app_impl; // app state and actions
mod help; // help screens and command descriptions

// Re-exported for the other TUI panels.
#[allow(unused_imports)]
pub use help::{HelpEntry, generate_cli_flag_entries, help_entries};

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Preview,
    Navigator,
    Console,
    Config,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatusSeverity {
    Info,
    Success,
    Warning,
    Error,
}

pub struct PreviewItem {
    pub original: String,
    pub new: String,
    pub status: String,
    pub is_dir: bool,
    pub orig_spans: Vec<Span<'static>>,
    pub new_spans: Vec<Span<'static>>,
}

pub struct ScanRequest {
    pub path: PathBuf,
    pub recursive: bool,
    pub max_depth: Option<usize>,
    pub show_hidden: bool,
    pub sort_mode: ScanSortBy,
    pub filter_mode: EntryFilter,
    pub filter_pattern: Option<String>,
    pub exclude_regex: Option<String>,
    pub min_name_len: Option<usize>,
    pub max_name_len: Option<usize>,
    pub min_path_len: Option<usize>,
    pub max_path_len: Option<usize>,
    pub filter_attr: Option<String>,
    pub filter_use_regex: bool,
    pub filter_match_case: bool,
}

pub struct App {
    pub cwd: PathBuf,
    pub navigator_entries: Vec<PathBuf>,
    pub navigator_state: ListState,
    pub selection: HashSet<PathBuf>,

    // Navigator State
    pub recursive: bool,
    pub max_depth: Option<usize>,
    pub show_hidden: bool,
    pub sort_mode: ScanSortBy,
    pub filter_mode: EntryFilter,

    pub active_config: RenameConfig,
    pub compiled_active: CompiledConfig,

    pub pending_config: RenameConfig,
    pub compiled_pending: CompiledConfig,
    pub config_state: ListState,
    pub config_drag_index: Option<usize>,
    pub config_drag_start_order: Option<Vec<String>>,
    pub config_display_order: Vec<String>,
    pub editing_config_key: Option<String>,
    pub editing_value: Option<(String, String)>, // (field_key, current_value) for inline editing
    pub stacking_mode: bool,                     // Allow multiple commands of same type

    pub last_status_update: Option<Instant>,

    pub preview_items: Vec<PreviewItem>,
    pub preview_state: TableState,

    pub input: String,
    pub input_cursor_position: usize,
    pub history: Vec<String>,
    pub history_index: usize,
    pub stash_input: String,

    pub filter_pattern: Option<String>,

    pub mode: AppMode,
    pub should_quit: bool,
    pub status_message: String,

    // Help system
    pub help_entries: Vec<HelpEntry>,
    pub help_state: ListState,
    pub help_filter_categories: Vec<&'static str>,
    pub help_filter_index: usize,
    /// Entry indices currently visible in the help list (derived from
    /// `help_filter_index` every frame in draw()). Selection is a position in
    /// this list, never an entry index, so it always stays in bounds.
    pub help_visible: Vec<usize>,

    // Channels
    pub scan_rx: Receiver<Vec<PathBuf>>,
    pub scan_req_tx: Sender<ScanRequest>,
    pub scan_cancel_flag: Arc<AtomicBool>,
    pub scanning_in_progress: bool,

    pub status_severity: StatusSeverity,

    pub preview_rx: Receiver<Vec<PreviewItem>>,
    pub preview_req_tx: Sender<(Vec<PathBuf>, CompiledConfig)>,
    pub console_phantom_line: bool,
    pub dry_run: bool,
    pub undo_file: Option<String>,
}

#[cfg(test)]
mod tests_help;
#[cfg(test)]
mod tests_parse;
