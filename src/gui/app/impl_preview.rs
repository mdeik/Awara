use super::*;

impl GuiApp {
    pub fn update_preview(&mut self) {
        if !self.preview_dirty || self.all_files.is_empty() {
            return;
        }
        self.ensure_file_names_synced();
        self.build_command_order();

        // 1. Semantic config normalization check:
        // Clamping length-bounded options (e.g. remove_first/last/to) relative to the
        // maximum filename length currently loaded avoids re-dispatching when dragging past limits.
        let max_len = self
            .file_display_names
            .iter()
            .map(|n| n.len())
            .max()
            .unwrap_or(0);
        let working = cleared_working_config(&self.config, &self.section_enabled);
        // Fingerprint of the operation as the user configured it (unclamped).
        // Distinct from `norm_cfg`: clamping can change `norm_cfg` without any
        // user edit (e.g. after an Apply shortens names), which must NOT re-arm
        // the processed guard. `command_order` is derived, so it is stripped.
        let mut op_fp = working.clone();
        op_fp.command_order.clear();
        let op_changed = self.last_dispatched_working.as_ref() != Some(&op_fp)
            || self.last_dispatched_section_enabled != Some(self.section_enabled);

        // The user edited the operation → re-arm the rename guard. Decoupled
        // from the dispatch decision: a raw edit that clamps to the same
        // normalized config (e.g. dragging "remove first" past the longest
        // name) re-arms but must NOT trigger a recompute.
        if op_changed {
            self.set_processed(false);
        }

        // The display order is the planner's input order. Materialize it here
        // (it is frozen between explicit sort events) and hand the *same* order
        // to the planner that `try_execute_renames` would use, so preview and
        // Apply cannot plan in different orders.
        let num_order = self.get_display_order().to_vec();

        let norm_cfg = effective_normalized_config(&working, max_len);

        // Skip the recompute when nothing that can change the output changed.
        // The gate key carries the *clamped* config, so dragging a length-bounded
        // option past the longest name is not a change; `same_names` additionally
        // ignores the display order when numbering does not follow it. Everything
        // else is compared by `PlanInputs`, so a new planner input is covered here
        // without touching this function.
        let gate_key = self.preview_key_with(norm_cfg);
        let key_eq = self
            .last_dispatched_key
            .as_ref()
            .is_some_and(|k| k.same_names(&gate_key));

        if key_eq {
            self.preview_dirty = false;
            return;
        }

        self.last_dispatched_working = Some(op_fp);
        self.last_dispatched_key = Some(gate_key);

        self.preview_generation += 1;
        let generation = self.preview_generation;

        let file_strs = self.file_names.clone();

        let request = PreviewRequest {
            files: file_strs,
            config: self.config.clone(),
            section_enabled: self.section_enabled,
            selection: self.selection.clone(),
            num_order,
            generation,
            cwd: self.cwd.clone(),
            processed: self.processed,
        };

        if self.preview_tx.send(request).is_ok() {
            self.preview_dirty = false;
            self.preview_pending = true;
        } else {
            // Thread dead — fall back to synchronous computation
            self.compute_preview_sync(generation);
        }
    }

    /// Synchronous fallback for preview computation.
    /// Used by `execute_renames` which needs the preview immediately,
    /// or if the background thread has died.
    pub(super) fn compute_preview_sync(&mut self, generation: u64) {
        if self.all_files.is_empty() {
            return;
        }
        self.ensure_file_names_synced();

        // Same input order the async path and Apply use — the frozen display
        // order, materialized once per explicit sort event.
        let num_order: Vec<usize> = self.get_display_order().to_vec();

        let params = PreviewParams {
            files: &self.file_names,
            selection: &self.selection,
            config: &self.config,
            section_enabled: &self.section_enabled,
            num_order: &num_order,
            cwd: &self.cwd,
            processed: self.processed,
        };

        // Sync path — no cancellation needed.
        static NEVER_CANCEL: AtomicBool = AtomicBool::new(false);
        let preview =
            compute_preview(params, &NEVER_CANCEL).expect("sync compute_preview never cancels");

        let same_as_current = preview_rows_equal(&self.preview, &preview);

        self.preview = Arc::new(preview);
        self.preview_dirty = false;
        self.preview_last_gen = generation;
        self.preview_pending = false;

        if !same_as_current {
            // NOTE: the display order is deliberately *not* invalidated here.
            // Re-sorting on every completed preview would move rows under the
            // user while they edit options, and would also make the order a
            // function of the preview it is ordering (a feedback loop).
            self.rebuild_rows_cache();
        }
    }

    /// Rebuild the cached row data in a background thread.
    /// Returns immediately; results are picked up in `check_rows_cache_result`.
    /// Must be called whenever `all_files`, `file_sizes`, `file_dates`, or `preview` changes.
    /// If a previous request is still in-flight, it is superseded — the old result
    /// will be discarded when the next one arrives.
    pub fn rebuild_rows_cache(&mut self) {
        self.rebuild_rows_cache_impl(false);
    }

    /// Selection-only variant used by the preview table when the selection
    /// changed without any other data change. The background thread carries
    /// rows over from its previous build and recomputes only the rows whose
    /// selection state flipped; if the dataset changed meanwhile (fingerprint
    /// mismatch) it falls back to a full rebuild automatically.
    pub(crate) fn rebuild_rows_cache_selection_only(&mut self) {
        self.rebuild_rows_cache_impl(true);
    }

    pub(super) fn rebuild_rows_cache_impl(&mut self, selection_only: bool) {
        if self.all_files.is_empty() {
            return;
        }
        self.ensure_file_names_synced();
        let status_map = self.ensure_status_map().clone();
        if !selection_only {
            // Every full rebuild changes the dataset fingerprint so a
            // subsequent selection-only request cannot carry rows over a
            // dataset mutation (file rename in place, filter, scan, ...).
            self.rows_dataset_gen += 1;
        }
        self.rows_cache_gen += 1;

        let cancel = Arc::new(AtomicBool::new(false));
        let request = RowsCacheRequest {
            r#gen: self.rows_cache_gen,
            files: self.file_names.clone(),
            sizes: self.file_sizes.clone(),
            dates: self.file_dates.clone(),
            is_dirs: self.file_is_dir.clone(),
            preview: self.preview.clone(),
            status_map,
            selection: self.selection.clone(),
            config: self.config.clone(),
            section_enabled: self.section_enabled,
            processed: self.processed,
            selection_only,
            fp: self.rows_dataset_fp(),
            cancel: Arc::clone(&cancel),
        };

        if self.rows_cache_tx.send(request).is_err() {
            // Thread dead — fall back to synchronous build
            self.rebuild_rows_cache_sync();
            return;
        }
        self.rows_cache_pending = true;
    }

    /// Dataset fingerprint of the current state — part of the incremental
    /// row-cache safety check (see `RowsDatasetFp`).
    pub(super) fn rows_dataset_fp(&self) -> RowsDatasetFp {
        RowsDatasetFp {
            preview_generation: self.preview_generation,
            scan_generation: self.scan_generation,
            processed_generation: self.processed_generation,
            rows_dataset_gen: self.rows_dataset_gen,
            file_count: self.all_files.len(),
            working: cleared_working_config(&self.config, &self.section_enabled),
        }
    }

    /// Synchronous fallback — builds the row cache inline.
    /// Used when the background thread is unavailable or when the cache is
    /// needed immediately (e.g., after trash where file indices shift).
    pub(crate) fn rebuild_rows_cache_sync(&mut self) {
        self.ensure_file_names_synced();
        let status_map = self.ensure_status_map().clone();
        self.rows_cache_gen += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let request = RowsCacheRequest {
            r#gen: self.rows_cache_gen,
            files: self.file_names.clone(),
            sizes: self.file_sizes.clone(),
            dates: self.file_dates.clone(),
            is_dirs: self.file_is_dir.clone(),
            preview: self.preview.clone(),
            status_map,
            selection: self.selection.clone(),
            config: self.config.clone(),
            section_enabled: self.section_enabled,
            processed: self.processed,
            selection_only: false,
            fp: self.rows_dataset_fp(),
            cancel: Arc::clone(&cancel),
        };
        self.cached_rows = std::rc::Rc::new(build_rows_cache(&request, None));
        self.cached_modified_count.set(None);
        self.rows_cache_pending = false;
    }

    /// Drain completed row-cache results from the background thread.
    /// Drains all queued results keeping only the latest — stale requests from
    /// rapid config changes are discarded automatically.
    /// Call once per frame (from `check_scan_results`).
    pub(super) fn check_rows_cache_result(&mut self) {
        let mut found = false;
        while let Ok((r#gen, rows)) = self.rows_cache_rx.try_recv() {
            // Discard stale results from prior generations.
            if r#gen != self.rows_cache_gen {
                continue;
            }
            // Arc -> Rc conversion
            let rc = match std::sync::Arc::try_unwrap(rows) {
                Ok(v) => std::rc::Rc::new(v),
                Err(arc) => std::rc::Rc::new(arc.as_ref().clone()),
            };
            self.cached_rows = rc;
            self.cached_modified_count.set(None);
            found = true;
        }
        if found {
            self.rows_cache_pending = false;
        }
    }

    /// Returns indices in the current preview display order (sort column & direction).
    /// SSoT: this is the sole definition of sort behavior for the preview table.
    /// Uses a cached value that is invalidated when sort criteria change.
    pub fn sorted_indices(&mut self) -> std::rc::Rc<Vec<usize>> {
        // If user has manually repositioned rows, respect that order (not cached).
        // A manual order only covers the file set it was built from, so drop it
        // when the file list has since changed (filter, trash, rescan) — a stale
        // permutation would produce out-of-range rows.
        if let Some(order) = &self.custom_order {
            if order.len() == self.all_files.len() {
                return std::rc::Rc::new(order.clone());
            }
            self.custom_order = None;
        }
        let n = self.all_files.len();
        if n == 0 {
            return std::rc::Rc::new(Vec::new());
        }

        // Pre-build lookup arrays for O(1) access during sort, avoiding
        // O(n²) scans inside the comparator (critical for 93k+ files).
        let preview_by_idx: Vec<Option<&str>> = if self.preview_sort_col == PreviewSortCol::NewName
        {
            let mut lookup = vec![None; n];
            for pr in self.preview.iter() {
                if pr.index < n {
                    lookup[pr.index] = Some(pr.new_name());
                }
            }
            lookup
        } else {
            Vec::new()
        };

        // Pre-extract extensions if sorting by Type to avoid re-deriving them in
        // the comparison loop. Uses the shared rename rule (normal + compound
        // extensions) and treats folders as extensionless, matching the Type
        // column's "Folder" label. Borrowed from `all_files`, so no allocation.
        let extensions: Vec<Option<&str>> = if self.preview_sort_col == PreviewSortCol::Type {
            let is_dir = &self.file_is_dir;
            self.all_files
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if is_dir.get(i).copied().unwrap_or(false) {
                        None
                    } else {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .and_then(awara::extension_str)
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        // Build a status lookup before sort (cannot call &mut self from closure).
        let committed: HashSet<&Path> = self
            .all_commits()
            .flat_map(|c| c.ops.iter())
            .flat_map(|op| [op.original_path.as_path(), op.new_path.as_path()])
            .collect();

        let mut order: Vec<usize> = (0..n).collect();
        let is_dir = &self.file_is_dir;
        let display_names = &self.file_display_names;
        let sizes = &self.file_sizes;
        let dates = &self.file_dates;
        let files = &self.all_files;
        let sort_col = self.preview_sort_col;
        let sort_asc = self.preview_sort_asc;

        order.par_sort_by(|&a, &b| {
            // Always group folders before files in ASC (file-browser style).
            let a_is_dir = is_dir.get(a).copied().unwrap_or(false);
            let b_is_dir = is_dir.get(b).copied().unwrap_or(false);
            let dir_cmp = b_is_dir.cmp(&a_is_dir);

            let col_cmp = match sort_col {
                PreviewSortCol::Name => {
                    let a_name = display_names.get(a).map(|s| s.as_str()).unwrap_or("");
                    let b_name = display_names.get(b).map(|s| s.as_str()).unwrap_or("");
                    nat_cmp(a_name, b_name)
                }
                PreviewSortCol::Size => sizes[a].cmp(&sizes[b]),
                PreviewSortCol::Date => dates[a].cmp(&dates[b]),
                PreviewSortCol::NewName => {
                    let na = preview_by_idx.get(a).copied().flatten();
                    let nb = preview_by_idx.get(b).copied().flatten();
                    match (na, nb) {
                        (Some(na), Some(nb)) => nat_cmp(na, nb),
                        (None, None) => {
                            let a_name = display_names.get(a).map(|s| s.as_str()).unwrap_or("");
                            let b_name = display_names.get(b).map(|s| s.as_str()).unwrap_or("");
                            nat_cmp(a_name, b_name)
                        }
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                    }
                }
                PreviewSortCol::Type => extensions[a].cmp(&extensions[b]),
                PreviewSortCol::Status => {
                    let sa = committed.contains(files[a].as_path());
                    let sb = committed.contains(files[b].as_path());
                    sb.cmp(&sa) // false < true, so reverse: committed files first
                }
            };

            let cmp = dir_cmp.then(col_cmp);
            if sort_asc { cmp } else { cmp.reverse() }
        });
        std::rc::Rc::new(order)
    }

    /// Returns the display order: custom repositioning if set, else sorted indices.
    ///
    /// The order is a *stored* value, rebuilt only at explicit events (sort
    /// click, manual reposition, rescan, filter) — never as a side effect of a
    /// completed preview. That keeps rows from moving under the user while they
    /// edit rename options, and stops the order from depending on the preview
    /// it is ordering (the `NewName` column sorts by generated names).
    pub fn get_display_order(&mut self) -> std::rc::Rc<Vec<usize>> {
        // Defensive: a stored order that does not cover the current file set is
        // unusable (out-of-range rows), so rebuild regardless of the flag.
        if !self.display_order_dirty.get() && self.display_order_cache.len() == self.all_files.len()
        {
            return std::rc::Rc::clone(&self.display_order_cache);
        }
        let order = self.sorted_indices();
        // The order is a planner input (numbering follows display order), so
        // every rebuild invalidates any plan computed against the old order.
        self.order_generation += 1;
        self.display_order_cache = std::rc::Rc::clone(&order);
        self.display_order_dirty.set(false);
        order
    }

    /// The dispatch gate's key: the planner's inputs as the *clamped* config form
    /// (see [`PlanInputs`]).
    ///
    /// Takes `&mut self` and materializes the display order first. The order is
    /// part of the key, but `order_generation` only advances when the order is
    /// *rebuilt*, so reading it before the rebuild would report the previous
    /// generation and let a stale order go unnoticed. Doing it here makes that
    /// impossible to get wrong at the call site.
    fn preview_key_with(&mut self, working: RenameConfig) -> PreviewKey {
        let _ = self.get_display_order();
        PreviewKey {
            inputs: PlanInputs {
                section_enabled: self.section_enabled,
                selection_generation: self.selection_generation,
                scan_generation: self.scan_generation,
                processed_generation: self.processed_generation,
                file_count: self.all_files.len(),
                cwd: self.cwd.clone(),
                working,
            },
            order_generation: self.order_generation,
        }
    }

    /// Mark the display order stale; it is rebuilt lazily by
    /// `get_display_order()`.
    ///
    /// The single entry point for order invalidation, and deliberately *not*
    /// called by anything that only renames files in place (apply, F2, undo/redo,
    /// revert): re-sorting is an explicit action. Call it when the file set
    /// changes (scan, filter, navigation, an apply that drops an entry), when the
    /// sort key changes, or on a manual reposition.
    pub fn invalidate_sorted_cache(&self) {
        self.display_order_dirty.set(true);
    }

    pub fn sort_preview_by(&mut self, col: PreviewSortCol) {
        // Clicking a column header clears any manual reposition
        self.custom_order = None;
        if self.preview_sort_col == col {
            self.preview_sort_asc = !self.preview_sort_asc;
        } else {
            self.preview_sort_col = col;
            self.preview_sort_asc = true;
        }
        // Invalidate cached sort order so it's recomputed on next access
        self.invalidate_sorted_cache();
        // Numbering follows display order, so re-sorting renumbers every file
        // and the preview must be rebuilt. Without numbering, names are
        // order-independent: the rows stay valid, and only the dispatch gate's
        // `order_generation` (compared when numbering is active) is involved.
        if self.numbering_active() {
            self.preview_dirty = true;
        }
    }
}
