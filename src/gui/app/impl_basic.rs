use super::*;

impl GuiApp {
    pub fn new() -> Self {
        Self::with_config(RenameConfig::default())
    }

    /// Create the GUI app from a loaded document (config + section enable state).
    /// Used when `--load-preset` is passed alongside `--gui`.
    pub fn with_doc(doc: awara::RenameDoc) -> Self {
        let mut app = Self::with_config(doc.rename_config.clone());
        app.config = doc.rename_config;
        app.section_enabled = awara::enabled_from_disabled(&doc.disabled_sections);
        app.preview_dirty = true;
        app
    }

    /// Create the GUI app with an initial rename config (all sections enabled).
    pub fn with_config(cfg: RenameConfig) -> Self {
        let root = tree_root();
        let mut tree_expanded = HashSet::new();
        let mut tree_children = HashMap::new();
        let tree_scanned = HashSet::new();

        let show_hidden = cfg.filters.filter_hidden;
        let root_children = scan_tree_children(&root, show_hidden);
        tree_children.insert(root.clone(), root_children);
        tree_expanded.insert(root.clone());

        let (scan_result_tx, scan_result_rx) = mpsc::channel();

        // Tree scan background thread channels
        let (tree_scan_tx, tree_scan_rx_thread) = mpsc::channel::<(PathBuf, bool, u64)>();
        let (tree_result_tx, tree_scan_rx) = mpsc::channel::<(PathBuf, u64, Vec<PathBuf>)>();

        // Preview computation channels
        let (preview_tx, preview_rx_thread) = mpsc::channel::<PreviewRequest>();
        // Result carries (generation, rows) so stale results can be discarded.
        let (preview_result_tx, preview_rx) = mpsc::channel::<(u64, Vec<PreviewItem>)>();

        // Row-cache builder channels
        let (rows_cache_tx, rows_cache_rx_thread) = mpsc::channel::<RowsCacheRequest>();
        let (rows_cache_result_tx, rows_cache_rx) =
            mpsc::channel::<(u64, std::sync::Arc<Vec<Row>>)>();
        // Shared cancel flag — main thread sets it on navigation, the
        // background thread checks it between chunks to abandon stale work.
        let rows_cache_cancel = Arc::new(AtomicBool::new(false));
        let rows_cache_cancel_thread = Arc::clone(&rows_cache_cancel);

        let mut app = GuiApp {
            cwd: PathBuf::new(),
            cwd_input: String::new(),
            all_files: Vec::new(),
            file_names: Vec::new(),
            fuzzy_index: FuzzyIndex::new(),
            path_pool: PathPool::new(),
            selection: Vec::new(),
            file_sizes: Vec::new(),
            file_dates: Vec::new(),
            file_is_dir: Vec::new(),
            file_display_names: Vec::new(),
            raw_all_files: Vec::new(),
            raw_file_sizes: Vec::new(),
            raw_file_dates: Vec::new(),
            raw_file_is_dir: Vec::new(),
            scan_opts: ScanOptions {
                recursive: false,
                max_depth: None,
                show_hidden: false,
                entry_filter: EntryFilter::Files,
                filter_pattern: None,
                exclude_regex: None,
                min_name_len: None,
                max_name_len: None,
                min_path_len: None,
                max_path_len: None,
                filter_attr: None,
                filter_use_regex: false,
                filter_match_case: false,
                sort_by: Some(ScanSortBy::Name),
            },
            scanning: false,
            scan_cancel: Arc::new(AtomicBool::new(false)),
            scan_error: None,
            scan_result_tx,
            scan_result_rx,
            tree_expanded,
            tree_children,
            tree_scanned,
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
            preview_dirty: true,
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
            rows_cache_cancel: Arc::clone(&rows_cache_cancel),
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
            revert_col_widths: REVERT_COL_DEFAULT_WIDTHS.to_vec(),
            section_enabled: DEFAULT_SECTION_ENABLED,
            status_message: DEFAULT_STATUS.to_string(),
            status_kind: StatusKind::Persistent,
            status_timestamp: None,
            last_ok_message: DEFAULT_STATUS.to_string(),
            status_before_scan: None,
            auto_refresh: DEFAULT_VIEW_SETTINGS.auto_refresh,
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
            remember_rename_options: DEFAULT_VIEW_SETTINGS.remember_rename_options,
            remember_last_dir: DEFAULT_VIEW_SETTINGS.remember_last_dir,
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
        };
        // section_order stays empty — consumers fall back to default_order().
        // This keeps preset JSON clean (empty vec is skipped on serialization).
        // Load app settings (always persisted)
        if let Some(settings) = AppSettings::load() {
            app.remember_rename_options = settings.remember_rename_options;
            app.remember_last_dir = settings.remember_last_dir;
            app.auto_refresh = settings.auto_refresh;
            app.skip_trash_confirmation = settings.skip_trash_confirmation;
            app.skip_dir_creation_warning = settings.skip_dir_creation_warning;
            // Restore last directory if enabled
            if app.remember_last_dir && !settings.last_dir.is_empty() {
                let saved = PathBuf::from(&settings.last_dir);
                if saved.exists() {
                    app.cwd = saved;
                    app.cwd_input = app.cwd.to_string_lossy().to_string();
                }
            }
            // Restore table-column sizing (always persisted, like last dir).
            if settings.table_col_widths.len() == COL_IDS.len() {
                app.layout.col_widths = settings.table_col_widths;
            }
            if settings.table_col_order.len() == COL_IDS.len() {
                app.layout.col_order = settings.table_col_order;
            }
            if settings.revert_col_widths.len() == REVERT_COL_DEFAULT_WIDTHS.len() {
                app.revert_col_widths = settings.revert_col_widths;
            } else if !settings.revert_col_widths.is_empty() {
                let mut widths = settings.revert_col_widths;
                while widths.len() < REVERT_COL_DEFAULT_WIDTHS.len() {
                    widths.push(REVERT_COL_DEFAULT_WIDTHS[widths.len()]);
                }
                widths.truncate(REVERT_COL_DEFAULT_WIDTHS.len());
                app.revert_col_widths = widths;
            }
            // Restore preview-table sort preference (always persisted).
            // Defaults are applied by serde when the fields are missing, so
            // existing settings files still load cleanly.
            app.preview_sort_col = settings.preview_sort_col;
            app.preview_sort_asc = settings.preview_sort_asc;
            app.invalidate_sorted_cache();
        }

        // If cwd was restored from settings, scan and expand tree; otherwise stay empty.
        // Expand ancestors only — cwd itself stays collapsed.
        if !app.cwd.as_os_str().is_empty() {
            // Pre-scan parent so cwd appears in the tree
            if let Some(parent) = app.cwd.parent()
                && parent != Path::new("")
                && !app.tree_scanned.contains(parent)
            {
                let children = scan_tree_children(parent, app.config.filters.filter_hidden);
                app.tree_children.insert(parent.to_path_buf(), children);
            }

            let mut ancestors: Vec<PathBuf> = Vec::new();
            let mut p = app.cwd.parent();
            while let Some(dir) = p {
                if dir == Path::new("") {
                    break;
                }
                ancestors.push(dir.to_path_buf());
                p = dir.parent();
                if let Some(grandparent) = p {
                    if grandparent == Path::new("") {
                        break;
                    }
                    if !app.tree_scanned.contains(grandparent) {
                        let children =
                            scan_tree_children(grandparent, app.config.filters.filter_hidden);
                        app.tree_children
                            .insert(grandparent.to_path_buf(), children);
                    }
                }
            }
            for a in ancestors.into_iter().rev() {
                app.tree_expanded.insert(a);
            }
            app.tree_scroll_target = Some(app.cwd.clone());
        }

        // Try to restore rename-config state (only if "Remember Rename Options" was enabled)
        if let Some(state) = GuiPersistedState::load()
            && app.remember_rename_options
        {
            app.config = state.doc.rename_config.clone();
            app.section_enabled = awara::enabled_from_disabled(&state.doc.disabled_sections);
            app.edit_state = state.edit_state.clone();
            app.undo_file = state.undo_file.clone();
            app.timestamps = state.timestamps.clone();
            app.preview_dirty = true;
        }

        // Background thread for async tree directory scanning.
        // Runs for the entire lifetime of the app — exits when the sender is dropped.
        // (Arrow-availability is probed synchronously once per rendered node
        // in `draw_tree_node`, so no probe channel is needed here.)
        thread::spawn(move || {
            while let Ok((dir, show_hidden, generation)) = tree_scan_rx_thread.recv() {
                let children = scan_tree_children(&dir, show_hidden);
                if tree_result_tx.send((dir, generation, children)).is_err() {
                    break;
                }
            }
        });

        // Background thread for async preview computation.
        // Runs the two-pass rename pipeline without blocking the UI.
        // Periodically checks for new requests while processing so a large
        // workload (e.g. 93k files) does not permanently block subsequent
        // preview updates — the current workload is abandoned and a fresh
        // request is picked up immediately.
        thread::spawn(move || {
            // Outer loop: obtain the next request, either already queued or
            // by blocking until one arrives.
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

        // Background thread for async row-cache building.
        // Uses a single persistent cancel flag shared across all iterations.
        // When a new request arrives mid-build, the flag is set so the
        // running `build_rows_cache` sees it at the next chunk boundary
        // (every 2000 items) and aborts promptly.
        thread::spawn(move || {
            let cancel_flag = rows_cache_cancel_thread;
            // State of the last *completed* build — the incremental base for
            // `selection_only` requests. Cancelled builds must not update it.
            let mut last_rows: Option<Arc<Vec<Row>>> = None;
            let mut last_selection: Option<Vec<bool>> = None;
            let mut last_fp: Option<RowsDatasetFp> = None;

            loop {
                let mut req = match rows_cache_rx_thread.try_recv() {
                    Ok(r) => r,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        match rows_cache_rx_thread.recv() {
                            Ok(r) => r,
                            Err(_) => break,
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                };

                // Drain any requests that arrived while we were blocked on recv.
                // Keep the LATEST request — intermediate ones are obsolete.
                // Set the cancel flag so any stale build from a prior iteration
                // (still running in the inner loop below) aborts at the next
                // chunk boundary.
                cancel_flag.store(true, Ordering::Relaxed);
                loop {
                    match rows_cache_rx_thread.try_recv() {
                        Ok(new_req) => {
                            req = new_req;
                            cancel_flag.store(true, Ordering::Relaxed);
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                    }
                }

                loop {
                    let r#gen = req.r#gen;
                    cancel_flag.store(false, Ordering::Relaxed);

                    // Point the request's cancel flag at our shared flag.
                    // This way, when a new request arrives in the drain loop
                    // below, `cancel_flag.store(true)` is visible to the
                    // currently-running `build_rows_cache` at the next chunk.
                    req.cancel = Arc::clone(&cancel_flag);

                    let base = match (&last_rows, &last_selection, &last_fp) {
                        (Some(rows), Some(sel), Some(fp)) => {
                            Some((rows.as_slice(), sel.as_slice(), fp))
                        }
                        _ => None,
                    };
                    let rows = build_rows_cache(&req, base);

                    // Drain any request that arrived mid-build.
                    // Set the shared flag so `build_rows_cache` (if still
                    // processing a chunk) aborts at the next check.
                    while let Ok(new_req) = rows_cache_rx_thread.try_recv() {
                        req = new_req;
                        cancel_flag.store(true, Ordering::Relaxed);
                    }

                    if cancel_flag.load(Ordering::Relaxed) {
                        continue;
                    }

                    // Completed build: record it as the incremental base.
                    let rows_arc = Arc::new(rows);
                    last_selection = Some(req.selection.clone());
                    last_fp = Some(req.fp.clone());
                    last_rows = Some(Arc::clone(&rows_arc));

                    if rows_cache_result_tx.send((r#gen, rows_arc)).is_err() {
                        return;
                    }
                    break;
                }
            }
        });

        if !app.cwd.as_os_str().is_empty() {
            app.scan_dir();
        }
        app
    }

    /// Minimal constructor for tests — skips scans and file I/O.
    #[cfg(test)]
    pub(super) fn new_with_config_for_test() -> Self {
        let cfg = RenameConfig::default();
        let (scan_result_tx, scan_result_rx) = mpsc::channel();
        let (tree_scan_tx, _) = mpsc::channel();
        let (_, tree_scan_rx) = mpsc::channel();
        let (preview_tx, _) = mpsc::channel::<PreviewRequest>();
        let (_, preview_rx) = mpsc::channel::<(u64, Vec<PreviewItem>)>();
        let (rows_cache_tx, _) = mpsc::channel();
        let (_, rows_cache_rx) = mpsc::channel();
        GuiApp {
            cwd: PathBuf::from("/tmp"),
            cwd_input: "/tmp".into(),
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
            skip_trash_confirmation: false,
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

    /// Build command_order from enabled sections.
    /// When section_order is empty, uses [`SectionId::default_order`] as fallback.
    /// A non-empty but partial section_order is normalized by slotting any missing
    /// sections at their default positions (SSoT: [`merge_section_order`]).
    /// Delegates item dispatch to [`awara::emit_section_items`] for SSoT with the CLI fallback.
    pub fn build_command_order(&mut self) {
        // Disabled sections are handled later in update_preview by cloning
        // the config and clearing on the clone — so the UI keeps the values.
        let default_labels: Vec<&str> = SectionId::default_order()
            .iter()
            .map(|s| s.label())
            .collect();
        let labels: Vec<String> = if self.config.section_order.is_empty() {
            default_labels.iter().map(|s| s.to_string()).collect()
        } else {
            let merged = merge_section_order(&self.config.section_order, &default_labels);
            if merged != self.config.section_order {
                // Persist the completed order so the Order Editor and saved
                // presets always carry every section.
                self.config.section_order = merged.clone();
            }
            merged
        };

        // Filter out disabled sections and unknown labels, then emit items
        let mut items: Vec<RenameItem> = Vec::new();
        for label in &labels {
            let sec_id = SectionId::from_label(label);
            let enabled = sec_id
                .map(|id| self.section_enabled[id as usize])
                .unwrap_or(true); // unknown labels pass through
            if !enabled {
                continue;
            }
            // Emit items for this section via the shared dispatch. Unknown
            // labels resolve to `None` and emit nothing.
            items.extend(awara::emit_section_items(&self.config, sec_id.into_iter()));
        }
        // Only mark dirty when the command order actually changed. This
        // prevents an endless cycle of preview recomputation on every
        // frame — without it, the async preview thread is constantly
        // interrupted by a superseding request and never completes.
        let changed = self.config.command_order != items;
        self.config.command_order = items;
        if changed {
            self.preview_dirty = true;
        }
    }

    /// True when numbering will actually run: a numbering mode/item is configured
    /// AND the Numbering section is enabled. SSoT for callers that must decide
    /// whether numbering indices (and therefore display-order sorting) matter.
    pub(crate) fn numbering_active(&self) -> bool {
        self.section_enabled[SectionId::Numbering as usize] && config_has_numbering(&self.config)
    }
}
