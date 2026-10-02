use super::*;

impl GuiApp {
    pub fn reset_all(&mut self) {
        self.config = RenameConfig::default();
        self.edit_state = GuiEditState::default();
        self.section_enabled = DEFAULT_SECTION_ENABLED;
        self.timestamps = GuiTimestampsState::default();
        self.undo_file.clear();
        self.preview_dirty = true;
        self.scan_dir();
        self.set_status("All settings reset", StatusKind::Temporary);
    }

    pub fn scan_dir(&mut self) {
        // A rescan replaces `all_files`/`selection`, so any in-flight inline
        // edit's index would silently rebind to a different file in the new
        // listing (and a later commit would rename the wrong file). Drop it.
        self.cancel_edit();
        // Cancel any in-flight scan first so filter changes (subfolders
        // uncheck, etc.) immediately take effect instead of being ignored.
        if self.scanning {
            self.cancel_scan();
        }
        self.build_command_order();
        if !self.cwd.exists() {
            self.all_files.clear();
            self.raw_all_files.clear();
            self.raw_file_sizes.clear();
            self.raw_file_dates.clear();
            self.raw_file_is_dir.clear();
            self.file_names.clear();
            self.selection.clear();
            // Fresh listing → re-arm the rename guard (safety; also covers
            // programmatic pre-selection after the scan).
            self.set_processed(false);
            self.file_sizes.clear();
            self.file_dates.clear();
            self.file_is_dir.clear();
            self.preview = Arc::new(Vec::new());
            // Only warn when a directory was actually set but is now gone.
            // An empty cwd (no directory ever selected) is not an error.
            if !self.cwd.as_os_str().is_empty() {
                // Don't keep resetting the timer on repeated calls — if the
                // message is already showing, let the existing timer run.
                let msg = format!("Directory no longer exists: {}", self.cwd.display());
                if self.status_message != msg {
                    self.set_status(&msg, StatusKind::Error);
                }
            }
            return;
        }
        // ── Structural scan options (require filesystem traversal) ──
        // Non-structural filters (mask, condition, exclude, etc.) are applied
        // in-memory via `apply_filters_in_memory`, avoiding a full re-scan.
        self.scan_opts.recursive = self.config.filters.filter_subfolders;
        self.scan_opts.show_hidden = self.config.filters.filter_hidden;
        self.scan_opts.max_depth = self.config.filters.filter_level;
        // Reset non-structural scan opts to pass-all defaults so the scan
        // returns all files (respecting subfolders/hidden/level).
        self.scan_opts.entry_filter = EntryFilter::Both;
        self.scan_opts.filter_pattern = None;
        self.scan_opts.exclude_regex = None;
        self.scan_opts.min_name_len = None;
        self.scan_opts.max_name_len = None;
        self.scan_opts.min_path_len = None;
        self.scan_opts.max_path_len = None;
        self.scan_opts.filter_attr = None;
        self.scan_opts.filter_use_regex = false;
        self.scan_opts.filter_match_case = false;

        self.scan_generation += 1;
        self.scanning = true;
        self.scan_error = None;
        // Swap the status bar to the scanning message for the duration of the
        // scan, remembering what was showing so it can be restored when the
        // scan finishes (or is cancelled).
        self.status_before_scan = Some(SavedStatus {
            message: self.status_message.clone(),
            kind: self.status_kind,
            timestamp: self.status_timestamp,
            last_ok_message: self.last_ok_message.clone(),
        });
        self.status_message = SCANNING_STATUS.to_string();
        self.status_kind = StatusKind::Persistent;
        self.status_timestamp = None;
        self.raw_all_files.clear();
        self.raw_file_sizes.clear();
        self.raw_file_dates.clear();
        self.raw_file_is_dir.clear();
        self.all_files.clear();
        self.file_names.clear();
        self.selection.clear();
        // Fresh listing → re-arm the rename guard (safety; also covers
        // programmatic pre-selection after the scan).
        self.set_processed(false);
        self.file_sizes.clear();
        self.file_dates.clear();
        self.file_is_dir.clear();
        self.preview = Arc::new(Vec::new());

        // Signal background threads to abandon in-flight work for the old
        // directory.  Without this, the preview and row-cache threads keep
        // grinding through 161k items even though the user has navigated
        // elsewhere, delaying the new directory's first render.
        //
        // The threads check for new requests between chunks (every 1000-
        // 2000 items) — sending a dummy request with an incremented
        // generation ensures they notice promptly and restart with fresh
        // (empty) data for the new scan.
        self.cancel_background_work();
        // Don't clear commits_by_dir — they persist across navigation.
        // Redo is cleared since it's per-session.
        self.redo_stack.clear();

        // Invalidate expanded tree nodes so they re-scan with the current
        // filter_hidden value when the user toggles the Hidden filter.
        for dir in self.tree_expanded.iter().cloned().collect::<Vec<_>>() {
            self.invalidate_tree_scan(&dir);
        }
        // Arrow-availability answers depend on the hidden filter — recompute
        // them only when it changes, so plain navigation doesn't clear the
        // cache and flash arrows on every visible folder.
        let hidden = self.config.filters.filter_hidden;
        if self.tree_probe_hidden != Some(hidden) {
            self.tree_has_subdirs.clear();
            self.tree_probe_hidden = Some(hidden);
        }

        let path = self.cwd.clone();
        let opts = self.scan_opts.clone();
        let cancel = self.scan_cancel.clone();
        cancel.store(false, Ordering::Relaxed);
        let r#gen = self.scan_generation;
        let tx = self.scan_result_tx.clone();

        thread::spawn(move || {
            let files = scan_directory(&path, &opts, Some(&cancel));
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let total = files.len();
            let mut sizes = Vec::with_capacity(total);
            let mut dates = Vec::with_capacity(total);
            let mut is_dirs = Vec::with_capacity(total);

            // Collect metadata in parallel chunks — syscalls are independent
            // and parallelism speeds up cold-cache scenarios significantly.
            // Cancellation is checked between chunks.
            // A partial "streaming" ScanResult is sent after the first chunk
            // so the UI can render immediately (< 1 ms to first visible rows)
            // while the rest of the metadata is still being collected.
            const META_CHUNK: usize = 2000;
            let mut chunk_start = 0;
            let mut first_chunk_sent = false;
            while chunk_start < total {
                let chunk_end = (chunk_start + META_CHUNK).min(total);

                if cancel.load(Ordering::Relaxed) {
                    return;
                }

                let chunk_results: Vec<(u64, bool, i64)> = (chunk_start..chunk_end)
                    .into_par_iter()
                    .map(|i| {
                        let f = &files[i];
                        if let Ok(meta) = std::fs::metadata(f) {
                            (
                                meta.len(),
                                meta.is_dir(),
                                meta.modified()
                                    .ok()
                                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                                    .map(|d| d.as_secs() as i64)
                                    .unwrap_or(0),
                            )
                        } else {
                            (0, false, 0)
                        }
                    })
                    .collect();

                for (s, d, t) in chunk_results {
                    sizes.push(s);
                    is_dirs.push(d);
                    dates.push(t);
                }

                // Stream a partial result after first chunk so the UI renders
                // visible rows immediately while background fills in the rest.
                if !first_chunk_sent && chunk_end < total {
                    first_chunk_sent = true;
                    let _ = tx.send(ScanResult {
                        files: files[..chunk_end].to_vec(),
                        sizes: sizes.clone(),
                        dates: dates.clone(),
                        is_dirs: is_dirs.clone(),
                        r#gen,
                        is_partial: true,
                    });
                }

                chunk_start = chunk_end;
            }

            // Send the complete final result.
            let _ = tx.send(ScanResult {
                files,
                sizes,
                dates,
                is_dirs,
                r#gen,
                is_partial: false,
            });
        });
    }

    /// Apply non-structural filters (mask, condition, exclude, files/folders,
    /// match-case, use-regex, name/path length) in-memory on the raw scan
    /// results, avoiding a filesystem re-scan.
    ///
    /// Called after a new scan completes, and whenever the user changes a
    /// non-structural filter field in the UI.
    pub fn apply_filters_in_memory(&mut self) {
        if self.raw_all_files.is_empty() {
            return;
        }

        let estimated = self.raw_all_files.len();
        let mut new_files = Vec::with_capacity(estimated);
        let mut new_sizes = Vec::with_capacity(estimated);
        let mut new_dates = Vec::with_capacity(estimated);
        let mut new_is_dir = Vec::with_capacity(estimated);

        for i in 0..self.raw_all_files.len() {
            let path = &self.raw_all_files[i];
            let is_dir = self.raw_file_is_dir[i];

            if !self.passes_in_memory_filters(path, is_dir) {
                continue;
            }

            // All checks passed — include this file
            new_files.push(self.raw_all_files[i].clone());
            new_sizes.push(self.raw_file_sizes[i]);
            new_dates.push(self.raw_file_dates[i]);
            new_is_dir.push(is_dir);
        }

        self.all_files = new_files;
        self.file_sizes = new_sizes;
        self.file_dates = new_dates;
        self.file_is_dir = new_is_dir;
        self.file_names = self
            .all_files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        self.file_display_names = self
            .all_files
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .collect();
        self.selection = vec![false; self.all_files.len()];
        // Fresh listing → re-arm the rename guard (safety; also covers
        // programmatic pre-selection applied right below).
        self.set_processed(false);

        // Apply any pending pre-selection from context-menu launch.
        // Comparison is case-insensitive on all platforms because
        // Windows and macOS use case-insensitive file systems, and
        // being forgiving on Linux has no negative side effect.
        if let Some(ref names) = self.pending_preselection.take() {
            let names_lower: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
            for (i, fpath) in self.all_files.iter().enumerate() {
                let fname = std::path::Path::new(fpath)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if names_lower.contains(&fname)
                    && let Some(s) = self.selection.get_mut(i)
                {
                    *s = true;
                }
            }
        }

        self.last_clicked_idx = None;
        self.selection_anchor = None;
        self.selection_generation += 1;
        self.preview_dirty = true;
        self.invalidate_sorted_cache();
        self.cached_rows = std::rc::Rc::new(Vec::new());
        self.rebuild_rows_cache();
        // Rebuild fuzzy index so instant search works on the filtered list.
        self.fuzzy_index = FuzzyIndex::from_items(&self.file_display_names);
    }

    /// Whether an entry passes the in-memory (non-structural) filters — the exact
    /// predicate [`Self::apply_filters_in_memory`] applies. Shared so a row
    /// inserted surgically (a new folder an apply created) matches what a rescan
    /// would show, instead of appearing despite a mask/exclude/length filter.
    pub(crate) fn passes_in_memory_filters(&self, path: &Path, is_dir: bool) -> bool {
        let cfg = &self.config.filters;

        // entry_filter is resolved from filter_files/filter_folders and checked
        // against the known is_dir flag rather than hitting the filesystem.
        let entry_filter = if cfg.filter_files && cfg.filter_folders {
            EntryFilter::Both
        } else if cfg.filter_files {
            EntryFilter::Files
        } else if cfg.filter_folders {
            EntryFilter::Folders
        } else {
            EntryFilter::Both
        };
        let include_type = match entry_filter {
            EntryFilter::Files => !is_dir,
            EntryFilter::Folders => is_dir,
            EntryFilter::Both => true,
        };
        if !include_type {
            return false;
        }

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();

        // filter_pattern (Mask)
        if let Some(ref pattern) = cfg.filter_pattern
            && !pattern.is_empty()
            && !awara::mask_matches(&name, pattern, cfg.filter_use_regex, cfg.filter_match_case)
        {
            return false;
        }

        // exclude_regex (Exclude)
        if !awara::exclude_regex_passes(&name, cfg.exclude_regex.as_deref()) {
            return false;
        }

        // min/max name/path length
        awara::length_filters_pass(
            name.chars().count(),
            path.to_string_lossy().chars().count(),
            cfg.min_name_len,
            cfg.max_name_len,
            cfg.min_path_len,
            cfg.max_path_len,
        )
    }

    /// Check if background threads have finished. Call this each frame.
    /// Drains preview results, tree scan results, file scan results, and row-cache results.
    pub fn check_scan_results(&mut self) {
        // Drain completed row-cache builds before checking other results so
        // the UI always renders with the freshest row data.
        self.check_rows_cache_result();

        // Drain preview results — discard stale results from prior generations
        if self.preview_pending {
            while let Ok((r#gen, rows)) = self.preview_rx.try_recv() {
                if r#gen == self.preview_generation {
                    let same_as_current = preview_rows_equal(&self.preview, &rows);

                    self.preview = Arc::new(rows);
                    self.preview_last_gen = r#gen;
                    self.preview_pending = false;
                    self.preview_dirty = false;

                    if !same_as_current {
                        // Deliberately NOT invalidating the display order here:
                        // re-sorting on every completed preview moves rows under
                        // the user mid-edit, and makes the order depend on the
                        // preview it sorts (the `NewName` column). The order is
                        // rebuilt at explicit events instead.
                        self.rebuild_rows_cache();
                    }
                }
                // Stale result (gen < self.preview_generation): discard
            }
        }

        // Drain tree scan results from the background thread
        while let Ok((dir, generation, children)) = self.tree_scan_rx.try_recv() {
            // Apply a result only if it answers the directory's current in-flight
            // request. An invalidated request (a trash, rename or refresh dropped
            // its pending entry, or a newer request superseded it) was read from
            // disk before that change, so applying it would resurrect stale
            // children.
            if self.tree_pending.get(&dir) != Some(&generation) {
                continue;
            }
            self.tree_pending.remove(&dir);
            // A scan that comes back with no subdirectories means the folder
            // is now a leaf — auto-collapse it so a hollow expanded folder
            // (with its useless collapse triangle) doesn't linger. Also
            // cleans up expanded dirs that were deleted externally.
            let became_empty = children.is_empty() && self.tree_expanded.contains(&dir);
            self.tree_children.insert(dir.clone(), children);
            self.tree_scanned.insert(dir.clone());
            if became_empty {
                self.collapse_tree_dir(&dir);
            }
        }

        // File scan results — discard stale results from old generations
        // to prevent a race: an old scan thread that missed the cancel flag
        // (because scan_dir resets it before the thread checks) can send a
        // result AFTER a newer scan starts.  The generation check ensures
        // only the latest scan's result is applied.
        if !self.scanning {
            return;
        }
        while let Ok(result) = self.scan_result_rx.try_recv() {
            if result.r#gen != self.scan_generation {
                // Stale result from a cancelled scan thread — discard.
                continue;
            }

            if result.is_partial {
                // Partial result: show immediately for instant first render.
                // (raw fields remain empty until the final result arrives.)
                self.all_files = result.files;
                self.file_names = self
                    .all_files
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                self.file_display_names = self
                    .all_files
                    .iter()
                    .map(|p| {
                        p.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default()
                    })
                    .collect();
                self.file_sizes = result.sizes;
                self.file_dates = result.dates;
                self.file_is_dir = result.is_dirs;
                self.selection = vec![false; self.all_files.len()];
                self.last_clicked_idx = None;
                self.selection_anchor = None;
                // Fresh listing → re-arm the rename guard.
                self.set_processed(false);
                if !self.navigating_to_dir {
                    self.preview_active = false;
                }
                self.preview_dirty = true;
                self.selection_generation += 1;
                self.invalidate_sorted_cache();
                self.cached_rows = std::rc::Rc::new(Vec::new());
                self.rebuild_rows_cache();
                self.cwd_input = self.cwd.to_string_lossy().to_string();
                // Still scanning — don't clear status or commits.
            } else {
                // Final result: store in raw fields and apply in-memory filters.
                self.raw_all_files = result.files;
                self.raw_file_sizes = result.sizes;
                self.raw_file_dates = result.dates;
                self.raw_file_is_dir = result.is_dirs;

                if !self.navigating_to_dir {
                    self.preview_active = false;
                }
                self.navigating_to_dir = false;
                self.scanning = false;

                // Apply non-structural filters in-memory on the raw data.
                self.apply_filters_in_memory();

                self.cwd_input = self.cwd.to_string_lossy().to_string();
                maybe_shrink(&mut self.raw_all_files);
                maybe_shrink(&mut self.raw_file_sizes);
                maybe_shrink(&mut self.raw_file_dates);
                maybe_shrink(&mut self.raw_file_is_dir);
                // Clear transient statuses — files were re-scanned, previous
                // execution results (success/fail/skip) no longer apply.
                self.status_map.clear();
                self.restore_status_after_scan();
                // Sweep stale commit ops — files that no longer exist on disk.
                self.sweep_dead_commits();
                // Reset PathPool so parent-directory allocations are deduplicated for the new cwd.
                self.path_pool = PathPool::new();
                for path in &self.raw_all_files {
                    if let Some(parent) = path.parent() {
                        self.path_pool.get_or_insert(parent);
                    }
                }

                // The preview scan just observed the current directory's
                // contents — drop stale expand-arrow answers for every cached
                // dir under it, so a dir whose subdirectories were removed
                // loses its arrow (and a removed dir's stale probe entry is
                // dropped) without waiting for manual interaction. Probes are
                // synchronous and memoized, so this is flash-free and costs one
                // cheap readdir per visible node on the next frame. The
                // `tree_scanned` invalidation in `scan_dir` already re-reads
                // the children lists of expanded nodes, which drops removed
                // rows.
                let cwd = self.cwd.clone();
                if !cwd.as_os_str().is_empty() {
                    self.tree_has_subdirs.retain(|p, _| !p.starts_with(&cwd));
                }
            }
        }
    }

    /// Halt background threads gracefully when navigating to a new directory.
    /// Sets the shared cancel flag so a running 161k `build_rows_cache`
    /// abandons work at the next chunk boundary.  Also bumps generation
    /// counters so any stale results that sneak through are discarded.
    pub(super) fn cancel_background_work(&mut self) {
        self.rows_cache_cancel.store(true, Ordering::Relaxed);
        self.preview_generation += 1;
        self.preview_pending = false;
        self.rows_cache_gen += 1;
        self.rows_cache_pending = false;
        self.cached_rows = std::rc::Rc::new(Vec::new());
        self.invalidate_sorted_cache();
    }

    pub fn cancel_scan(&mut self) {
        self.scan_cancel.store(true, Ordering::Relaxed);
        self.scanning = false;
        // Drain any results already sent before cancel took effect
        while self.scan_result_rx.try_recv().is_ok() {}
        // Drain stale preview results so the UI doesn't apply results from
        // a cancelled scan. The background thread will also notice a newer
        // generation request and abandon its stale workload.
        while self.preview_rx.try_recv().is_ok() {}
        self.preview_pending = false;
        // Drain stale row-cache results for the same reason.
        while self.rows_cache_rx.try_recv().is_ok() {}
        self.rows_cache_pending = false;
        self.raw_all_files.clear();
        self.raw_file_sizes.clear();
        self.raw_file_dates.clear();
        self.raw_file_is_dir.clear();
        self.restore_status_after_scan();
    }
}
