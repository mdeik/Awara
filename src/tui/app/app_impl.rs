use super::*;
use std::path::Path;

impl App {
    pub fn new() -> App {
        let (scan_tx, scan_rx) = mpsc::channel();
        let (scan_req_tx, scan_req_rx) = mpsc::channel::<ScanRequest>();

        let (preview_tx, preview_rx) = mpsc::channel();
        let (preview_req_tx, preview_req_rx) = mpsc::channel::<(Vec<PathBuf>, CompiledConfig)>();

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let scan_cancel = cancel_flag.clone();

        // Background Scanner Thread — uses shared scan_directory from lib.rs (SSoT with CLI)
        thread::spawn(move || {
            while let Ok(req) = scan_req_rx.recv() {
                // Reset cancellation flag at start of each request
                scan_cancel.store(false, Ordering::Relaxed);

                let opts = ScanOptions {
                    recursive: req.recursive,
                    max_depth: req.max_depth,
                    show_hidden: req.show_hidden,
                    entry_filter: req.filter_mode,
                    filter_pattern: req.filter_pattern,
                    exclude_regex: req.exclude_regex,
                    min_name_len: req.min_name_len,
                    max_name_len: req.max_name_len,
                    min_path_len: req.min_path_len,
                    max_path_len: req.max_path_len,
                    filter_attr: req.filter_attr,
                    filter_use_regex: req.filter_use_regex,
                    filter_match_case: req.filter_match_case,
                    sort_by: Some(req.sort_mode),
                };

                let entries = scan_directory(&req.path, &opts, Some(&scan_cancel));
                let _ = scan_tx.send(entries);
            }
        });

        // Background Preview Thread
        thread::spawn(move || {
            while let Ok((files, compiled)) = preview_req_rx.recv() {
                let files_str: Vec<String> = files
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                // One plan for preview and execution (SSoT).
                let plan_opts = PlanOptions {
                    dirname_level: compiled.config.append_folder.dirname_level,
                    output_dir: None,
                    keep_structure: false,
                    base_dir: None,
                    parallel: false,
                    copy_mode: false,
                };
                let plan = plan_renames(&files_str, &compiled.config, &plan_opts);

                // The plan holds only actionable renames; the planner preserves
                // input order, so one forward pass places each op and fills the
                // files it skipped (unchanged names) with identity rows.
                let mut plan_iter = plan.ops().iter().peekable();
                let items = files_str
                    .iter()
                    .map(|f| {
                        let want = Path::new(f.as_str());
                        let op = match plan_iter.peek() {
                            Some(op) if op.original_path.as_path() == want => plan_iter.next(),
                            _ => None,
                        };
                        match op {
                            Some(op) => {
                                let status =
                                    if op.new_path.exists() && op.new_path != op.original_path {
                                        "Collision".to_string()
                                    } else {
                                        "OK".to_string()
                                    };
                                PreviewItem {
                                    original: op.original_name.clone(),
                                    new: op.new_name.clone(),
                                    status,
                                    is_dir: op.original_path.is_dir(),
                                    orig_spans: Vec::new(),
                                    new_spans: Vec::new(),
                                }
                            }
                            None => {
                                let path = Path::new(f.as_str());
                                let name = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                PreviewItem {
                                    original: name.clone(),
                                    new: name,
                                    status: "Unchanged".to_string(),
                                    is_dir: path.is_dir(),
                                    orig_spans: Vec::new(),
                                    new_spans: Vec::new(),
                                }
                            }
                        }
                    })
                    .collect();
                let _ = preview_tx.send(items);
            }
        });

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        // Initial scan — use App default filter_mode for consistency
        let _ = scan_req_tx.send(ScanRequest {
            path: cwd.clone(),
            recursive: false,
            max_depth: None,
            show_hidden: false,
            sort_mode: ScanSortBy::Name,
            filter_mode: EntryFilter::Files,
            filter_pattern: None,
            exclude_regex: None,
            min_name_len: None,
            max_name_len: None,
            min_path_len: None,
            max_path_len: None,
            filter_attr: None,
            filter_use_regex: false,
            filter_match_case: false,
        });

        App {
            cwd,
            navigator_entries: Vec::new(),
            navigator_state: ListState::default(),
            selection: HashSet::new(),
            recursive: false,
            max_depth: None,
            show_hidden: false,
            sort_mode: ScanSortBy::Name,
            filter_mode: EntryFilter::Files,
            active_config: RenameConfig::empty(),
            compiled_active: CompiledConfig::new(RenameConfig::empty()),
            pending_config: RenameConfig::empty(),
            compiled_pending: CompiledConfig::new(RenameConfig::empty()),
            config_state: ListState::default(),
            config_drag_index: None,
            config_drag_start_order: None,
            config_display_order: Vec::new(),
            editing_config_key: None,
            editing_value: None,
            stacking_mode: false,
            last_status_update: None,
            preview_items: Vec::new(),
            preview_state: TableState::default(),
            input: String::new(),
            input_cursor_position: 0,
            history: Vec::new(),
            history_index: 0,
            stash_input: String::new(),
            filter_pattern: None,
            mode: AppMode::Console,
            should_quit: false,
            status_message: "Ready. Press ? for help.".to_string(),
            status_severity: StatusSeverity::Info,
            help_entries: help_entries(),
            help_state: ListState::default(),
            help_filter_categories: {
                let entries = help_entries();
                let mut cats: Vec<&str> = entries.iter().map(|e| e.category).collect();
                cats.sort();
                cats.dedup();
                cats
            },
            help_filter_index: 0,
            help_visible: Vec::new(),
            scan_rx,
            scan_req_tx,
            scan_cancel_flag: cancel_flag,
            scanning_in_progress: true, // initial scan in flight
            preview_rx,
            preview_req_tx,
            console_phantom_line: false,
            dry_run: false,
            undo_file: None,
        }
    }

    pub fn set_status(&mut self, msg: String, severity: StatusSeverity) {
        let clean_msg = msg.trim().replace('\n', " ");
        self.status_message = clean_msg;
        self.status_severity = severity;
        self.last_status_update = Some(Instant::now());
    }

    pub fn on_tick(&mut self) {
        // Check for scan results
        if let Ok(entries) = self.scan_rx.try_recv() {
            self.navigator_entries = entries;
            self.scanning_in_progress = false;
            // Trigger preview update if needed
            self.update_preview();
        }

        // Check for preview results
        if let Ok(items) = self.preview_rx.try_recv() {
            self.preview_items = items
                .into_iter()
                .map(|item| {
                    let (orig_spans, new_spans) =
                        crate::tui::ui::diff_strings(&item.original, &item.new);
                    PreviewItem {
                        orig_spans,
                        new_spans,
                        ..item
                    }
                })
                .collect();
        }

        // Status cleanup — timeout depends on severity
        if let Some(last) = self.last_status_update {
            let timeout = match self.status_severity {
                StatusSeverity::Error => Duration::from_secs(20),
                StatusSeverity::Warning => Duration::from_secs(12),
                StatusSeverity::Success => Duration::from_secs(8),
                StatusSeverity::Info => Duration::from_secs(6),
            };
            if last.elapsed() > timeout {
                self.status_message = "Ready".to_string();
                self.status_severity = StatusSeverity::Info;
                self.last_status_update = None;
            }
        }
    }

    /// Toggle stacking mode, preserving config state across the switch
    pub fn toggle_stacking_mode(&mut self) {
        self.stacking_mode = !self.stacking_mode;
        if self.stacking_mode {
            // Switching ON: delegate to SSoT config_to_items
            // Preserve non-operation fields that aren't RenameItem variants
            let output_dir = self.pending_config.copy_to.output_dir.take();
            let copy_mode = self.pending_config.copy_to.copy_mode;
            let stop_on_error = self.pending_config.stop_on_error;
            let min_name_len = self.pending_config.filters.min_name_len.take();
            let max_name_len = self.pending_config.filters.max_name_len.take();
            let min_path_len = self.pending_config.filters.min_path_len.take();
            let max_path_len = self.pending_config.filters.max_path_len.take();

            let new_order = awara::config_to_items(&self.pending_config);
            self.pending_config = RenameConfig::empty();
            self.pending_config.command_order = new_order;

            // Restore preserved fields
            self.pending_config.copy_to.output_dir = output_dir;
            self.pending_config.copy_to.copy_mode = copy_mode;
            self.pending_config.stop_on_error = stop_on_error;
            self.pending_config.filters.min_name_len = min_name_len;
            self.pending_config.filters.max_name_len = max_name_len;
            self.pending_config.filters.min_path_len = min_path_len;
            self.pending_config.filters.max_path_len = max_path_len;
        } else {
            // Switching OFF: delegate to SSoT items_to_config
            let order = std::mem::take(&mut self.pending_config.command_order);
            let mut cfg = RenameConfig::default();
            awara::items_to_config(order, &mut cfg);
            // Preserve non-operation fields that aren't RenameItem variants
            cfg.copy_to.output_dir = self.pending_config.copy_to.output_dir.take();
            cfg.copy_to.copy_mode = self.pending_config.copy_to.copy_mode;
            cfg.stop_on_error = self.pending_config.stop_on_error;
            cfg.filters.min_name_len = self.pending_config.filters.min_name_len.take();
            cfg.filters.max_name_len = self.pending_config.filters.max_name_len.take();
            cfg.filters.min_path_len = self.pending_config.filters.min_path_len.take();
            cfg.filters.max_path_len = self.pending_config.filters.max_path_len.take();
            self.pending_config = cfg;
        }
        self.config_display_order.clear();
        self.sync_config_order();
    }

    pub fn update_preview(&mut self) {
        self.sync_config_order();
        // Collect files to preview: navigator order, or the selection if any.
        let mut files: Vec<PathBuf> = if self.selection.is_empty() {
            self.navigator_entries.clone()
        } else {
            self.selection.iter().cloned().collect()
        };
        // Apply the same folder-grouping + natural-name sort as the navigator
        // (SSoT: sort_paths in core/scan.rs), so the preview mirrors the GUI
        // preview table's ordering — even when the selection (a HashSet) would
        // otherwise produce an arbitrary order.
        let sort_opts = ScanOptions {
            sort_by: Some(self.sort_mode),
            ..Default::default()
        };
        sort_paths(&mut files, &sort_opts);

        // Send request
        let _ = self
            .preview_req_tx
            .send((files, self.compiled_pending.clone()));
    }

    pub fn next_nav(&mut self) {
        let i = match self.navigator_state.selected() {
            Some(i) => {
                if i >= self.navigator_entries.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.navigator_state.select(Some(i));
    }

    pub fn previous_nav(&mut self) {
        let i = match self.navigator_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.navigator_entries.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.navigator_state.select(Some(i));
    }

    pub fn toggle_selection(&mut self) {
        if let Some(i) = self.navigator_state.selected()
            && let Some(path) = self.navigator_entries.get(i)
        {
            if self.selection.contains(path) {
                self.selection.remove(path);
            } else {
                self.selection.insert(path.clone());
            }
            self.update_preview();
        }
    }

    pub fn enter_dir(&mut self) {
        if let Some(i) = self.navigator_state.selected()
            && let Some(path) = self.navigator_entries.get(i)
            && path.is_dir()
        {
            self.cwd = path.clone();
            self.refresh();
            self.selection.clear();
            self.navigator_state.select(Some(0));
        }
    }

    pub fn go_up(&mut self) {
        if let Some(parent) = self.cwd.parent() {
            self.cwd = parent.to_path_buf();
            self.refresh();
            self.selection.clear();
            self.navigator_state.select(Some(0));
        }
    }

    pub fn refresh(&mut self) {
        self.scanning_in_progress = true;
        let _ = self.scan_req_tx.send(ScanRequest {
            path: self.cwd.clone(),
            recursive: self.recursive,
            max_depth: self.max_depth,
            show_hidden: self.show_hidden,
            sort_mode: self.sort_mode,
            filter_mode: self.filter_mode,
            filter_pattern: self.filter_pattern.clone(),
            exclude_regex: self.pending_config.filters.exclude_regex.clone(),
            min_name_len: self.pending_config.filters.min_name_len,
            max_name_len: self.pending_config.filters.max_name_len,
            min_path_len: self.pending_config.filters.min_path_len,
            max_path_len: self.pending_config.filters.max_path_len,
            filter_attr: self.pending_config.filters.filter_attr.clone(),
            filter_use_regex: self.pending_config.filters.filter_use_regex,
            filter_match_case: self.pending_config.filters.filter_match_case,
        });
    }

    pub fn cancel_scan(&mut self) {
        self.scan_cancel_flag.store(true, Ordering::Relaxed);
        self.scanning_in_progress = false;
    }

    pub fn toggle_recursive(&mut self) {
        self.recursive = !self.recursive;
        self.refresh();
    }

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.refresh();
    }

    pub fn cycle_sort(&mut self) {
        self.sort_mode = match self.sort_mode {
            ScanSortBy::Name => ScanSortBy::NameDesc,
            ScanSortBy::NameDesc => ScanSortBy::Date,
            ScanSortBy::Date => ScanSortBy::DateDesc,
            ScanSortBy::DateDesc => ScanSortBy::Size,
            ScanSortBy::Size => ScanSortBy::SizeDesc,
            ScanSortBy::SizeDesc => ScanSortBy::Extension,
            ScanSortBy::Extension => ScanSortBy::ExtensionDesc,
            ScanSortBy::ExtensionDesc => ScanSortBy::Name,
        };
        self.refresh();
    }

    pub fn cycle_filter(&mut self) {
        self.filter_mode = match self.filter_mode {
            EntryFilter::Files => EntryFilter::Folders,
            EntryFilter::Folders => EntryFilter::Both,
            EntryFilter::Both => EntryFilter::Files,
        };
        self.refresh();
    }

    pub fn sync_config_order(&mut self) {
        let items = self.generate_config_items_map();
        let mut new_order = Vec::new();
        let mut seen = std::collections::HashSet::new();

        let insert_after_key = self.editing_config_key.clone();
        self.editing_config_key = None; // Reset after use

        // Keep existing items in order
        for key in &self.config_display_order {
            if items.contains_key(key) {
                new_order.push(key.clone());
                seen.insert(key.clone());

                // If this is the key we edited, insert new items here
                if let Some(target) = &insert_after_key
                    && key == target
                {
                    // Find new items — deterministic insertion order
                    if self.stacking_mode {
                        let mut new_keys: Vec<&String> = items
                            .keys()
                            .filter(|k| {
                                !self.config_display_order.contains(*k) && !seen.contains(*k)
                            })
                            .collect();
                        new_keys.sort_by_key(|k| {
                            k.trim_start_matches("step_").parse::<usize>().unwrap_or(0)
                        });
                        for new_key in new_keys {
                            new_order.push(new_key.clone());
                            seen.insert(new_key.clone());
                        }
                    } else {
                        // Normal mode: predefined canonical order handles this below
                    }
                }
            }
        }

        // Add remaining new items — use deterministic ordering
        if self.stacking_mode {
            // Sort by numerical step index so step_10 comes after step_9
            let mut remaining: Vec<&String> = items.keys().filter(|k| !seen.contains(*k)).collect();
            remaining.sort_by_key(|k| k.trim_start_matches("step_").parse::<usize>().unwrap_or(0));
            for key in remaining {
                new_order.push(key.clone());
            }
        } else {
            // Canonical field order for normal mode
            let canonical_order = [
                "Prefix",
                "Suffix",
                "Regex",
                "Regex Full Name",
                "Replace",
                "Case Sensitive",
                "Replace First",
                "Remove First",
                "Remove Last",
                "Remove From",
                "Remove To",
                "Remove Digits",
                "Remove Chars",
                "Remove Words",
                "Remove Symbols",
                "Trim",
                "Double Spaces",
                "Remove High",
                "Remove Accents",
                "Insert",
                "Insert At",
                "Word Space",
                "Case Name",
                "Case Ext",
                "Numbering",
                "Num Start",
                "Num Inc",
                "Num Pad",
                "Num Sep",
                "Ext Mode",
                "Ext Replace",
                "Ext Add",
                "Ext Remove",
                "Add Dirname",
                "Dirname Sep",
                "Dirname Pos",
                "Move Part",
                "Copy Part",
            ];
            for key in &canonical_order {
                if items.contains_key(*key) && !seen.contains(*key) {
                    new_order.push(key.to_string());
                    seen.insert(key.to_string());
                }
            }
            // Any remaining unknown/plugin keys
            for key in items.keys() {
                if !seen.contains(key) {
                    new_order.push(key.clone());
                }
            }
        }

        self.config_display_order = new_order;
    }

    fn generate_config_items_map(
        &self,
    ) -> std::collections::HashMap<String, (String, String, String)> {
        let c = &self.pending_config;
        let mut items = std::collections::HashMap::new();

        if self.stacking_mode {
            // Stacked Mode: delegate to SSoT
            for (i, item) in c.command_order.iter().enumerate() {
                let key = format!("step_{}", i);
                let (name, val, cmd) = awara::rename_item_display(item);
                items.insert(key, (format!("{}. {}", i + 1, name), val, cmd));
            }
            return items;
        }

        // Default Mode: delegate to SSoT in lib.rs
        for (key, val, cmd) in awara::config_active_fields(c) {
            items.insert(key.clone(), (key, val, cmd));
        }
        items
    }

    pub fn get_config_items(&self) -> Vec<(String, String, String)> {
        let items_map = self.generate_config_items_map();
        let mut items = Vec::new();

        // Use display order
        for key in &self.config_display_order {
            if let Some(item) = items_map.get(key) {
                items.push(item.clone());
            }
        }

        // Add any items not in order (should be handled by sync, but safety check)
        for (key, item) in &items_map {
            if !self.config_display_order.contains(key) {
                items.push(item.clone());
            }
        }

        items
    }

    pub fn describe_command(cmd: &str) -> String {
        // Show the raw command — it's what the user typed and is always accurate.
        // Avoids duplicating flag-name mappings that already live in Args clap derive.
        cmd.to_string()
    }
}
