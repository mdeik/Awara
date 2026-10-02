use super::collision::CollisionEntry;
use super::*;
use std::fs::File;
use tempfile::TempDir;

pub(super) fn app_with_live_threads() -> GuiApp {
    let cfg = RenameConfig::default();
    let (scan_result_tx, scan_result_rx) = mpsc::channel();
    let (tree_scan_tx, _tree_scan_rx_thread) = mpsc::channel::<(PathBuf, bool, u64)>();
    let (_tree_result_tx, tree_scan_rx) = mpsc::channel::<(PathBuf, u64, Vec<PathBuf>)>();

    // Preview channels — both ends live
    let (preview_tx, preview_rx_thread) = mpsc::channel::<PreviewRequest>();
    let (preview_result_tx, preview_rx) = mpsc::channel::<(u64, Vec<PreviewItem>)>();

    // Row-cache channels — both ends live
    let (rows_cache_tx, rows_cache_rx_thread) = mpsc::channel::<RowsCacheRequest>();
    let (rows_cache_result_tx, rows_cache_rx) = mpsc::channel::<(u64, std::sync::Arc<Vec<Row>>)>();

    // Preview thread — the production worker's exact loop (see
    // `GuiApp::with_config`): requests are computed via the shared
    // `compute_preview`, and superseding is drain-based.
    thread::spawn(move || {
        loop {
            let mut req = match preview_rx_thread.try_recv() {
                Ok(r) => r,
                Err(std::sync::mpsc::TryRecvError::Empty) => match preview_rx_thread.recv() {
                    Ok(r) => r,
                    Err(_) => break,
                },
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            };

            // Inner loop: process the current request, aborting if a
            // newer request arrives mid-way.  AtomicBool signals the
            // shared compute_preview to abandon work between chunks.
            // Drain any request that arrived while we were processing the
            // previous one before starting new work — closes the race window
            // between cancel flag reset and the first chunk boundary.
            loop {
                match preview_rx_thread.try_recv() {
                    Ok(new_req) => req = new_req,
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                }
            }

            let cancel = AtomicBool::new(false);
            loop {
                let r#gen = req.generation;

                let params = PreviewParams {
                    files: &req.files,
                    selection: &req.selection,
                    config: &req.config,
                    section_enabled: &req.section_enabled,
                    num_order: &req.num_order,
                    cwd: &req.cwd,
                    processed: req.processed,
                };

                cancel.store(false, Ordering::Relaxed);

                let result = compute_preview(params, &cancel);

                // After compute finishes, drain any queued request (catches
                // one that arrived during the final chunk after the last
                // flag check).  Also catch one that arrived between the
                // pre-loop drain above and cancel.store(false, ...).
                while let Ok(new_req) = preview_rx_thread.try_recv() {
                    req = new_req;
                    cancel.store(true, Ordering::Relaxed);
                }

                if cancel.load(Ordering::Relaxed) {
                    continue;
                }

                if let Some(preview) = result
                    && preview_result_tx.send((r#gen, preview)).is_err()
                {
                    return;
                }
                break;
            }
        }
    });

    // Row-cache thread (same logic as new())
    thread::spawn(move || {
        // State of the last *completed* build — the incremental base for
        // `selection_only` requests (mirrors the production worker).
        let mut last_rows: Option<Arc<Vec<Row>>> = None;
        let mut last_selection: Option<Vec<bool>> = None;
        let mut last_fp: Option<RowsDatasetFp> = None;

        loop {
            let req = match rows_cache_rx_thread.try_recv() {
                Ok(r) => r,
                Err(std::sync::mpsc::TryRecvError::Empty) => match rows_cache_rx_thread.recv() {
                    Ok(r) => r,
                    Err(_) => break,
                },
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            };

            let base = match (&last_rows, &last_selection, &last_fp) {
                (Some(rows), Some(sel), Some(fp)) => Some((rows.as_slice(), sel.as_slice(), fp)),
                _ => None,
            };
            let rows = build_rows_cache(&req, base);

            // Completed build: record it as the incremental base.
            let rows_arc = Arc::new(rows);
            last_selection = Some(req.selection.clone());
            last_fp = Some(req.fp.clone());
            last_rows = Some(Arc::clone(&rows_arc));

            if rows_cache_result_tx.send((req.r#gen, rows_arc)).is_err() {
                return;
            }
        }
    });

    GuiApp {
        cwd: PathBuf::new(),
        cwd_input: String::new(),
        all_files: Vec::new(),
        file_names: Vec::new(),
        fuzzy_index: FuzzyIndex::new(),
        path_pool: PathPool::new(),
        file_display_names: Vec::new(),
        selection: Vec::new(),
        file_sizes: Vec::new(),
        file_dates: Vec::new(),
        file_is_dir: Vec::new(),
        raw_all_files: Vec::new(),
        raw_file_sizes: Vec::new(),
        raw_file_dates: Vec::new(),
        raw_file_is_dir: Vec::new(),
        scan_opts: ScanOptions::default(),
        scanning: false,
        scan_cancel: Arc::new(AtomicBool::new(false)),
        scan_error: None,
        scan_result_tx,
        scan_result_rx,
        tree_expanded: HashSet::new(),
        tree_children: HashMap::new(),
        tree_scanned: HashSet::new(),
        tree_pending: HashMap::new(),
        tree_scan_gen_seq: 0,
        tree_scan_tx,
        tree_scan_rx,
        tree_has_subdirs: HashMap::new(),
        tree_probe_hidden: None,
        tree_scroll_target: None,
        config: cfg,
        edit_state: GuiEditState::default(),
        undo_file: String::new(),
        timestamps: GuiTimestampsState::default(),
        preview: Arc::new(Vec::new()),
        cached_rows: std::rc::Rc::new(Vec::new()),
        preview_dirty: false,
        scan_generation: 0,
        preview_generation: 0,
        preview_last_gen: 0,
        preview_pending: false,
        last_dispatched_key: None,
        last_dispatched_working: None,
        last_dispatched_section_enabled: None,
        processed: false,
        processed_generation: 0,
        preview_tx,
        preview_rx,
        rows_cache_cancel: Arc::new(AtomicBool::new(false)),
        rows_cache_tx,
        rows_cache_rx,
        rows_cache_gen: 0,
        rows_dataset_gen: 0,
        rows_cache_pending: false,
        preview_sort_col: PreviewSortCol::Name,
        preview_sort_asc: true,
        order_generation: 0,
        display_order_cache: std::rc::Rc::new(Vec::new()),
        display_order_dirty: std::cell::Cell::new(true),
        layout: GuiLayoutState {
            col_order: (0..COL_IDS.len()).collect(),
            col_widths: COL_DEFAULT_WIDTHS.to_vec(),
        },
        section_enabled: DEFAULT_SECTION_ENABLED,
        status_message: "test".into(),
        status_kind: StatusKind::Persistent,
        status_timestamp: None,
        last_ok_message: "test".into(),
        status_before_scan: None,
        auto_refresh: true,
        executing: false,
        commits_by_dir: HashMap::new(),
        commit_dir_lru: Vec::new(),
        redo_stack: Vec::new(),
        status_map: HashMap::new(),
        status_map_dirty: true,
        revert_status_map: HashMap::new(),
        revert_dialog: None,
        collision_dialog: None,
        pending_plan: None,
        active_popup: None,
        last_clicked_idx: None,
        selection_anchor: None,
        selection_generation: 0,
        last_seen_selection_gen: 0,
        selection_dragging: false,
        selection_dispatch_pending: false,
        last_selection_dispatch: None,
        cached_selected_count: Cell::new(None),
        cached_selected_gen: Cell::new(0),
        cached_selected_len: Cell::new(0),
        cached_modified_count: Cell::new(None),
        cached_modified_gen: Cell::new(0),
        cached_modified_len: Cell::new(0),
        reorder_drag_idx: None,
        editing_idx: None,
        edit_buffer: String::new(),
        edit_pending_focus: false,
        editing_row_rect: None,
        preview_edge_drag_start: None,
        scroll_to_idx: None,
        scroll_sequence_start: None,
        scroll_last_event: None,
        preview_box_select_anchor: None,
        preview_active: false,
        navigating_to_dir: false,
        custom_order: None,
        show_properties_idx: None,
        show_properties_multi_data: None,
        show_tree_properties: None,
        pending_context_action: None,
        pending_sel_indices: Vec::new(),
        context_menu_action_taken: false,
        remember_rename_options: false,
        remember_last_dir: true,
        config_dir: None,
        pending_preselection: None,
        show_trash_confirmation: false,
        trash_init_focus: false,
        skip_trash_confirmation: DEFAULT_VIEW_SETTINGS.skip_trash_confirmation,
        trash_target: None,
        show_invalid_name_warning: false,
        invalid_name_init_focus: false,
        invalid_name_entries: Vec::new(),
        show_dir_creation_warning: false,
        dir_warning_init_focus: false,
        skip_dir_creation_warning: false,
        pending_dir_plan: None,
        pending_dir_label: String::new(),
        pending_dir_inline: false,
        pending_dir_count: 0,
        pending_f2_collision: None,
        revert_col_widths: REVERT_COL_DEFAULT_WIDTHS.to_vec(),
    }
}
pub(super) fn rows_cache_test_request(
    files: Vec<String>,
    selection: Vec<bool>,
    selection_only: bool,
    fp: RowsDatasetFp,
) -> RowsCacheRequest {
    let n = files.len();
    let mut cfg = RenameConfig::default();
    cfg.add.add_suffix = Some("_X".to_string());
    let preview: Vec<PreviewItem> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let path = PathBuf::from(f);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            preview_row(
                RenameOp {
                    original_path: path.clone(),
                    new_path: path,
                    original_name: name.clone(),
                    new_name: name,
                    was_copy: false,
                    relocated: false,
                    original_permissions: None,
                    applied_attributes: String::new(),
                    applied_timestamps_mask: 0u8,
                },
                false,
                i,
            )
        })
        .collect();
    RowsCacheRequest {
        r#gen: 1,
        files,
        sizes: vec![0; n],
        dates: vec![0; n],
        is_dirs: vec![false; n],
        preview: Arc::new(preview),
        status_map: HashMap::new(),
        selection,
        config: cfg,
        section_enabled: DEFAULT_SECTION_ENABLED,
        processed: false,
        selection_only,
        fp,
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

/// Sentinel rows with a distinct name/new_name so carried rows are detectable.
pub(super) fn sentinel_rows(n: usize) -> Vec<Row> {
    (0..n)
        .map(|i| Row {
            idx: i,
            name: format!("BASE_{}", i),
            is_dir: false,
            size: 0,
            date: 0,
            new_name: format!("SENTINEL_{}", i),
            changed: true,
            status: None,
            orig_segs: None,
            new_segs: None,
            error_msg: None,
            missing: false,
        })
        .collect()
}

pub(super) fn row_sig(rows: &[Row]) -> Vec<(usize, String, String, bool)> {
    rows.iter()
        .map(|r| (r.idx, r.name.clone(), r.new_name.clone(), r.changed))
        .collect()
}

/// Build a preview row from an op, for tests.
///
/// Rows no longer carry a whole `RenameOp` (they keep only the two names the
/// table shows), so this maps one to that shape: an op whose new name equals the
/// original is an identity row (`new_name: None`), which is exactly how the
/// preview represents "no rename".
pub(super) fn preview_row(op: RenameOp, selected: bool, index: usize) -> PreviewItem {
    let changed = op.new_name != op.original_name;
    PreviewItem {
        original_name: op.original_name,
        new_name: if changed { Some(op.new_name) } else { None },
        selected,
        index,
    }
}

pub(super) fn test_fp(file_count: usize) -> RowsDatasetFp {
    RowsDatasetFp {
        preview_generation: 0,
        scan_generation: 0,
        processed_generation: 0,
        rows_dataset_gen: 1,
        file_count,
        working: cleared_working_config(&RenameConfig::default(), &DEFAULT_SECTION_ENABLED),
    }
}
pub(super) fn selection_test_app(files: &[&str]) -> (GuiApp, TempDir) {
    let dir = TempDir::new().unwrap();
    for name in files {
        let p = dir.path().join(name);
        // Create as file or directory based on name ending with '/'
        if name.ends_with('/') {
            std::fs::create_dir_all(&p).unwrap();
        } else {
            // Ensure parent exists
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            File::create(&p).unwrap();
        }
    }
    let mut app = GuiApp::new_with_config_for_test();
    let paths: Vec<PathBuf> = files.iter().map(|f| dir.path().join(f)).collect();
    // The listing is a scan of `cwd`; point it at the tempdir the files live in
    // so scope decisions (what stays in the pane after a move) match reality.
    app.cwd = dir.path().to_path_buf();
    app.cwd_input = dir.path().to_string_lossy().to_string();
    app.all_files = paths;
    app.file_sizes = vec![0; files.len()];
    app.file_dates = vec![0; files.len()];
    app.selection = vec![true; files.len()];
    // Enable a simple replace operation
    app.section_enabled[SectionId::Replace as usize] = true;
    // Disable CopyTo — default copy_mode=true would copy instead of rename
    app.config.copy_to.copy_mode = false;
    app.config.replace.replace = Some("old".into());
    app.config.replace.with = Some("new".into());
    (app, dir)
}
pub(super) fn edit_state(
    action: Option<&str>,
    val1: isize,
    val2: isize,
    dest: Option<&str>,
    dest_val: isize,
) -> GuiEditState {
    GuiEditState {
        auto_date_type: None,
        auto_date_format_key: None,
        auto_date_sep: None,
        auto_date_custom: None,
        move_part_action: action.map(|s| s.into()),
        move_part_val1: val1,
        move_part_val2: val2,
        move_part_dest: dest.map(|s| s.into()),
        move_part_dest_val: dest_val,
        move_part_sep: String::new(),
    }
}
pub(super) fn make_entry(
    source_path: &str,
    source_display: &str,
    target_display: &str,
    reason: CollisionReason,
    conflict_source_path: Option<String>,
) -> CollisionEntry {
    // Mirror the plan: the target sits beside the source unless the path says
    // otherwise. Kept in one place so tests match `from_plan`.
    let target_path = std::path::Path::new(source_path)
        .parent()
        .map(|p| p.join(target_display))
        .unwrap_or_else(|| std::path::PathBuf::from(target_display));
    CollisionEntry {
        source_path: source_path.to_string(),
        source_display: source_display.to_string(),
        target_display: target_display.to_string(),
        target_path,
        target_is_dir: false,
        source_is_dir: false,
        reason,
        decided_replace: false,
        decided_skip: false,
        conflict_source_path,
        live_reason: LiveCollisionReason::InBatch, // dummy, recompute fixes it
    }
}
