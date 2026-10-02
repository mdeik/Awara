use super::*;

impl GuiApp {
    pub fn cwd_commits(&self) -> &[Commit] {
        self.commits_by_dir
            .get(&self.cwd)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Mutable access to commits for the current working directory.
    /// Used by tests to seed undo history; production code uses
    /// `push_cwd_commit` instead.
    #[cfg(test)]
    pub fn cwd_commits_mut(&mut self) -> &mut Vec<Commit> {
        self.touch_commit_dir_lru(&self.cwd.clone());
        self.commits_by_dir.entry(self.cwd.clone()).or_default()
    }

    /// Push a commit for the current working directory, enforcing the per-directory
    /// FIFO cap (oldest dropped first) and the global LRU directory cap.
    pub fn push_cwd_commit(&mut self, commit: Commit) {
        self.touch_commit_dir_lru(&self.cwd.clone());
        let commits = self.commits_by_dir.entry(self.cwd.clone()).or_default();
        commits.push(commit);
        if commits.len() > MAX_COMMITS_PER_DIR {
            let excess = commits.len() - MAX_COMMITS_PER_DIR;
            commits.drain(0..excess);
        }
        self.enforce_commits_dir_cap();
    }

    /// Touch directory in commit LRU order (most recent at the back).
    pub fn touch_commit_dir_lru(&mut self, dir: &Path) {
        if let Some(pos) = self.commit_dir_lru.iter().position(|p| p == dir) {
            self.commit_dir_lru.remove(pos);
        }
        self.commit_dir_lru.push(dir.to_path_buf());
    }

    /// Enforce max commit directories cap by evicting least recently used inactive directories.
    pub fn enforce_commits_dir_cap(&mut self) {
        while self.commits_by_dir.len() > MAX_COMMITS_DIRS {
            let evict_idx = self.commit_dir_lru.iter().position(|p| *p != self.cwd);
            if let Some(idx) = evict_idx {
                let evicted = self.commit_dir_lru.remove(idx);
                self.commits_by_dir.remove(&evicted);
            } else {
                break;
            }
        }
    }

    /// Iterate over all commits across all directories in a deterministic order
    /// (directories sorted by path; commits oldest→newest within each). The
    /// revert dialog indexes this list, so the order must be stable across the
    /// build/draw/confirm calls that share an active-commit index.
    pub fn all_commits(&self) -> impl Iterator<Item = &Commit> + '_ {
        let mut dirs: Vec<&PathBuf> = self.commits_by_dir.keys().collect();
        dirs.sort();
        dirs.into_iter()
            .flat_map(move |d| self.commits_by_dir[d].iter())
    }

    /// Ensure file_names is in sync with all_files. Auto-populates if out of sync.
    /// This handles tests that set all_files directly without setting file_names.
    pub fn ensure_file_names_synced(&mut self) {
        if self.file_names.len() != self.all_files.len() {
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
        }
    }

    /// Refresh name-derived caches after file names change in place (apply,
    /// F2 inline rename, undo, redo, revert): rebuild `file_display_names` and
    /// the fuzzy search index. The index build is cheap (sub-millisecond on 1M+
    /// items), so calling this on every name-mutating operation keeps instant
    /// search correct without a rescan.
    ///
    /// The display order is deliberately **not** invalidated. A rename does not
    /// change which files are listed, so the frozen order still covers them, and
    /// rebuilding it would re-sort the table by the new names the moment the user
    /// clicked Apply / F2 / undo. Sorting is an explicit action — a header click,
    /// a manual reposition, a rescan, a filter — each of which invalidates the
    /// order itself (`invalidate_sorted_cache`).
    ///
    /// The one case a caller must handle is a *set* change (an apply that drops a
    /// duplicate entry, a trash): `get_display_order` rebuilds when the stored
    /// order no longer covers the list, and such a caller invalidates explicitly.
    pub(crate) fn post_names_changed(&mut self) {
        self.file_display_names = self
            .all_files
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .collect();
        self.fuzzy_index = FuzzyIndex::from_items(&self.file_display_names);
        // In-place file mutations (F2 rename, Apply, undo/redo) change the
        // row-cache dataset without a scan; bump the fingerprint counter so a
        // later selection-only rebuild cannot carry stale rows over.
        self.rows_dataset_gen += 1;
    }

    /// Remove `indices` from the file listing, keeping everything keyed by file
    /// index consistent: the parallel per-file vectors, the frozen display order,
    /// and the selection anchors.
    ///
    /// The single implementation of "the listing lost entries in place" — an apply
    /// that overwrote or collapsed an entry, a trash. The order is **pruned**
    /// (removed indices dropped, survivors shifted), never rebuilt: rebuilding
    /// would re-sort the table, and sorting is an explicit action
    /// (see `invalidate_sorted_cache`).
    ///
    /// `indices` need not be sorted or deduplicated. Returns the paths that left
    /// the listing, for the caller's path-keyed cleanup (commit records). Preview
    /// rows are not reindexed here: a caller that renders from them before the
    /// next recompute calls [`Self::reindex_preview_after_removal`].
    pub(crate) fn remove_file_indices(&mut self, indices: &[usize]) -> Vec<String> {
        let mut removed: Vec<usize> = indices.to_vec();
        removed.sort_unstable();
        removed.dedup();
        if removed.is_empty() {
            return Vec::new();
        }

        // Back to front, so the indices still address the entries they name.
        let mut paths = Vec::with_capacity(removed.len());
        for &i in removed.iter().rev() {
            if let Some(path) = self.file_names.get(i) {
                paths.push(path.clone());
            }
            remove_at(&mut self.all_files, i);
            remove_at(&mut self.file_names, i);
            remove_at(&mut self.file_display_names, i);
            remove_at(&mut self.selection, i);
            remove_at(&mut self.file_sizes, i);
            remove_at(&mut self.file_dates, i);
            remove_at(&mut self.file_is_dir, i);
        }
        paths.reverse();

        // One old index → new index mapping, applied to every stored index.
        let remap = |old: usize| -> Option<usize> {
            if removed.binary_search(&old).is_ok() {
                None
            } else {
                Some(old - removed.partition_point(|&r| r < old))
            }
        };
        self.last_clicked_idx = self.last_clicked_idx.and_then(remap);
        self.selection_anchor = self.selection_anchor.and_then(remap);
        prune_order(std::rc::Rc::make_mut(&mut self.display_order_cache), remap);
        if let Some(custom) = &mut self.custom_order {
            prune_order(custom, remap);
        }
        // The order is a planner input, and it no longer says what it did.
        self.order_generation += 1;
        paths
    }

    /// Append a listing entry — used when an apply creates a new folder in the
    /// current directory. The set-change counterpart of [`Self::remove_file_indices`]
    /// for a listing that only *gained* a row: every existing row keeps its index,
    /// so the frozen display order is extended rather than rebuilt (a rebuild
    /// would re-sort the table, and sorting is an explicit action).
    pub(crate) fn append_listing_entry(
        &mut self,
        path: PathBuf,
        size: u64,
        date: i64,
        is_dir: bool,
    ) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let name_str = path.to_string_lossy().to_string();
        let idx = self.all_files.len();
        self.all_files.push(path);
        self.file_names.push(name_str);
        self.file_display_names.push(name);
        self.selection.push(false);
        self.file_sizes.push(size);
        self.file_dates.push(date);
        self.file_is_dir.push(is_dir);
        // Extend the frozen order. Only meaningful when it already covered the
        // list; a stale (length-mismatched) order is rebuilt by
        // `get_display_order` regardless.
        std::rc::Rc::make_mut(&mut self.display_order_cache).push(idx);
        if let Some(custom) = &mut self.custom_order {
            custom.push(idx);
        }
        self.order_generation += 1;
    }

    /// Drop the raw (pre in-memory filter) scan entry for `path`, if present, so
    /// a later filter change cannot resurrect an entry that left the listing.
    /// No-op when the raw scan is empty (a listing never built from a scan).
    pub(crate) fn remove_raw_path(&mut self, path: &str) {
        if let Some(i) = self
            .raw_all_files
            .iter()
            .position(|p| p.to_string_lossy().as_ref() == path)
        {
            remove_at(&mut self.raw_all_files, i);
            remove_at(&mut self.raw_file_sizes, i);
            remove_at(&mut self.raw_file_dates, i);
            remove_at(&mut self.raw_file_is_dir, i);
        }
    }

    /// Add a raw scan entry (a folder an apply created) so a later filter change
    /// keeps it instead of rebuilding from a stale scan. The caller decides
    /// whether the raw scan is live (see `maintain_raw` in `execute_plan_inner`).
    pub(crate) fn append_raw_entry(&mut self, path: PathBuf, size: u64, date: i64, is_dir: bool) {
        self.raw_all_files.push(path);
        self.raw_file_sizes.push(size);
        self.raw_file_dates.push(date);
        self.raw_file_is_dir.push(is_dir);
    }

    /// Drop the preview rows for `removed` and shift the survivors' indices, so
    /// rows stay keyed to the file list they describe. The preview half of an
    /// in-place removal, for callers that render from the rows before the next
    /// recompute.
    pub(crate) fn reindex_preview_after_removal(&mut self, removed: &[usize]) {
        if removed.is_empty() {
            return;
        }
        let removed_set: HashSet<usize> = removed.iter().copied().collect();
        let preview = Arc::make_mut(&mut self.preview);
        preview.retain(|pr| !removed_set.contains(&pr.index));
        for pr in preview.iter_mut() {
            pr.index -= removed.iter().filter(|&&r| r < pr.index).count();
        }
    }

    /// Returns a reference to the status map (no rebuild — entries are managed
    /// directly by `execute_plan_inner`).
    pub fn ensure_status_map(&mut self) -> &HashMap<String, (u8, Option<String>)> {
        &self.status_map
    }
    pub fn browse_for_output(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Select output directory")
            .pick_folder()
        {
            let path_str = path.to_string_lossy().to_string();
            self.config.copy_to.output_dir = Some(path_str.clone());
            self.preview_dirty = true;
            self.set_status(&format!("Output: {}", path_str), StatusKind::Persistent);
        }
    }

    pub fn set_status(&mut self, msg: &str, kind: StatusKind) {
        self.status_message = msg.to_string();
        self.status_kind = kind;
        if kind == StatusKind::Persistent {
            self.last_ok_message = msg.to_string();
            self.status_timestamp = None;
        } else {
            self.status_timestamp = Some(std::time::Instant::now());
        }
    }

    /// Swap the scanning status back to the message that was showing when the
    /// scan started. If something set a newer status while the scan ran
    /// (an error, an operation result, a reset message), that takes
    /// precedence and is left untouched — only the scanning message itself is
    /// replaced.
    pub(super) fn restore_status_after_scan(&mut self) {
        if let Some(saved) = self.status_before_scan.take()
            && self.status_message == SCANNING_STATUS
        {
            self.status_message = saved.message;
            self.status_kind = saved.kind;
            self.status_timestamp = saved.timestamp;
            self.last_ok_message = saved.last_ok_message;
        }
    }

    /// Set the global processed (rename-guard) flag. When `true`, the whole
    /// selected batch is frozen: previews show identity and Apply is a no-op
    /// until the user reselects files or changes the operation. Bumps
    /// `processed_generation` so the preview thread redispatch checks fire.
    pub(crate) fn set_processed(&mut self, processed: bool) {
        if self.processed != processed {
            self.processed = processed;
            self.processed_generation += 1;
        }
    }

    /// Unclamped fingerprint of the currently-configured operation.
    pub(super) fn op_fingerprint(&self) -> RenameConfig {
        let mut fp = cleared_working_config(&self.config, &self.section_enabled);
        fp.command_order.clear();
        fp
    }

    /// True when the rename operation (config + enabled sections) differs from
    /// what the preview thread last dispatched — i.e. the user edited the
    /// rules. Lets `try_execute_renames` re-arm the processed guard even when
    /// `update_preview` hasn't run yet (Apply clicked immediately after
    /// editing a field in the same frame).
    pub(super) fn operation_changed(&self) -> bool {
        self.last_dispatched_working.as_ref() != Some(&self.op_fingerprint())
            || self.last_dispatched_section_enabled != Some(self.section_enabled)
    }

    /// Save current state to disk, or delete the saved state if toggle is off.
    /// Always called on config changes — the toggle itself decides what to do.
    pub fn save_state(&self) {
        let base = self.config_dir.as_deref();
        AppSettings::save_in(self, base);
        if self.remember_rename_options {
            GuiPersistedState::save_in(self, base);
        } else {
            // Toggle was turned off: clean up so next launch starts fresh
            if let Some(path) = GuiPersistedState::state_path_in(base) {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    pub fn save_last_dir(&self) {
        if self.remember_last_dir && !self.cwd.as_os_str().is_empty() {
            AppSettings::save_in(self, self.config_dir.as_deref());
        }
    }

    /// Persist only the always-saved UI settings (incl. table-column sizing),
    /// e.g. when the user finishes dragging a column-resize handle.
    pub fn save_settings(&self) {
        AppSettings::save_in(self, self.config_dir.as_deref());
    }
}

/// Remove the entry at `i`, if the vector has one. The per-file vectors are
/// parallel, but a partial listing (or a test that sets only some of them) can
/// leave one short, and a missing entry is not a reason to panic.
fn remove_at<T>(v: &mut Vec<T>, i: usize) {
    if i < v.len() {
        v.remove(i);
    }
}

/// Drop the indices `remap` cannot place and rewrite the rest, so a stored order
/// keeps its relative sequence for the entries that survived a removal.
fn prune_order(order: &mut Vec<usize>, remap: impl Fn(usize) -> Option<usize>) {
    order.retain_mut(|i| match remap(*i) {
        Some(new) => {
            *i = new;
            true
        }
        None => false,
    });
}
