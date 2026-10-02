use super::*;

impl GuiApp {
    pub fn open_tree_dir_external(dir: &Path) {
        open_in_os(dir);
    }

    /// ── Undo / Redo by cycling through commit history ──
    ///
    /// SSoT: the head commit for the file determines whether a new "Revert"
    /// commit is created when undoing. If the head is already a Revert commit
    /// we are cycling — just rename, no commit. If the head is a forward
    /// commit, create a Revert commit to preserve the forward state before
    /// rolling back.
    ///
    /// - undo  = find the most recent *forward* op whose `new_path` matches
    ///   the file's current path, then rename back to `original_path`.
    /// - redo  = find any op (forward or revert) whose `original_path` matches
    ///   the file's current path and whose `new_path` does not exist
    ///   on disk, then rename forward to `new_path`.
    ///
    ///   Find the most recent forward `RenameOp` that produced the current path of `idx`.
    ///   Skips toggle commits (Revert/Redo) to avoid cycling into our own inverse.
    pub(crate) fn find_undo_op(&self, idx: usize) -> Option<RenameOp> {
        let path = self.all_files.get(idx)?;
        for commit in self.cwd_commits().iter().rev() {
            if commit.label.starts_with("Revert") || commit.label.starts_with("Redo") {
                continue;
            }
            for op in commit.ops.iter() {
                if &op.new_path == path {
                    return Some(op.clone());
                }
            }
        }
        None
    }

    /// Find a `RenameOp` that can be redone for this file: the most recent op
    /// (forward or toggle) whose `original_path` matches the file's current
    /// path. The target may already exist — the chain-safe executor resolves
    /// chains/swaps within the batch and reports genuine on-disk collisions as
    /// failures, mirroring how undo treats occupied targets.
    pub(crate) fn find_redo_op(&self, idx: usize) -> Option<RenameOp> {
        let path = self.all_files.get(idx)?;
        for (_ci, commit) in self.cwd_commits().iter().enumerate().rev() {
            for op in commit.ops.iter() {
                if &op.original_path == path {
                    return Some(op.clone());
                }
            }
        }
        None
    }

    /// Return true if the most recent commit referencing `path` is a toggle commit
    /// (label starts with "Revert" or "Redo").
    pub(super) fn is_head_toggle_commit(&self, path: &Path) -> bool {
        for commit in self.cwd_commits().iter().rev() {
            if commit
                .ops
                .iter()
                .any(|op| op.original_path == path || op.new_path == path)
            {
                // This is the most recent commit that references this file.
                return commit.label.starts_with("Revert") || commit.label.starts_with("Redo");
            }
        }
        false
    }

    /// Like [`is_head_toggle_commit`], but spans every directory (the revert
    /// dialog indexes commits across all directories). The most recent commit is
    /// chosen by timestamp, so the result doesn't depend on map iteration order.
    pub(crate) fn is_head_toggle_commit_all(&self, path: &Path) -> bool {
        let mut newest: Option<&Commit> = None;
        for commit in self.all_commits() {
            if !commit
                .ops
                .iter()
                .any(|op| op.original_path == path || op.new_path == path)
            {
                continue;
            }
            match newest {
                Some(n) if n.timestamp >= commit.timestamp => {}
                _ => newest = Some(commit),
            }
        }
        newest.is_some_and(|c| c.label.starts_with("Revert") || c.label.starts_with("Redo"))
    }

    /// Build a human-readable timestamp label (same pattern as Apply / Inline / Revert).
    pub(super) fn now_label(prefix: &str) -> String {
        let sys_now = std::time::SystemTime::now();
        let dur = sys_now
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let total_secs = dur.as_secs();
        let h = (total_secs / 3600) % 24;
        let m = (total_secs / 60) % 60;
        let s = total_secs % 60;
        format!("{} — {:02}:{:02}:{:02}", prefix, h, m, s)
    }

    /// Undo the last rename for the file at `idx`.
    ///
    /// Execute exec-semantics ops (original = current path, new = target)
    /// through the chain-safe core executor — which resolves chains/cycles by
    /// parking files at temp names instead of clobbering — then sync
    /// `all_files`/`file_names` and re-key directory caches for every
    /// successful op. Sorts children-before-parents first, like batch apply.
    /// Returns (successes, failures).
    pub(super) fn execute_ops_batch(
        &mut self,
        pending: &mut [(usize, RenameOp)],
    ) -> (usize, usize) {
        // Children before parents, like execute_renames Pass 2.5. Stable for
        // same-depth entries so same-directory chains keep their order.
        pending.sort_by(|(_, a), (_, b)| {
            b.original_path
                .components()
                .count()
                .cmp(&a.original_path.components().count())
        });

        let ops: Vec<RenameOp> = pending.iter().map(|(_, op)| op.clone()).collect();
        let result = apply_ops_chain_safe(&ops);

        let mut idx_by_paths: HashMap<(PathBuf, PathBuf), usize> = HashMap::new();
        for (idx, op) in pending.iter() {
            idx_by_paths.insert((op.original_path.clone(), op.new_path.clone()), *idx);
        }
        for op in &result.successful {
            if let Some(&idx) = idx_by_paths.get(&(op.original_path.clone(), op.new_path.clone())) {
                let target_str = op.new_path.to_string_lossy().to_string();
                self.all_files[idx] = op.new_path.clone();
                self.file_names[idx] = target_str;
                // Directory rename: re-key every consumer (file list, history,
                // status maps, revert table, file tree). Safe — re-keying
                // preserves exact-match op paths, so records stay intact
                // while descendant entries follow the dir. A copied entry
                // (`was_copy`) was never moved, so it is never re-keyed.
                if op.new_path.is_dir() && !op.was_copy {
                    self.rekey_dir(&op.original_path, &op.new_path);
                }
            }
        }
        self.post_names_changed();

        (result.successful.len(), result.failed.len())
    }

    /// Undo the last rename of every file/folder in `indices` (typically the
    /// current selection) as one batch. Applies the same discipline as batch
    /// apply: ops execute children-before-parents so nested paths resolve
    /// safely (stable for same-depth entries, keeping chain order), chains and
    /// swaps are resolved via temp-name deferral, a single "Revert" commit
    /// records the batch, and the row cache is rebuilt once. Returns the
    /// number of successful undos.
    pub fn undo_files(&mut self, indices: &[usize]) -> usize {
        // Snapshot the forward ops to undo, then invert for execution.
        let mut fwd: Vec<(usize, RenameOp)> = Vec::new();
        for &idx in indices {
            if idx >= self.all_files.len() {
                continue;
            }
            if let Some(op) = self.find_undo_op(idx) {
                fwd.push((idx, op));
            }
        }
        if fwd.is_empty() {
            self.set_status("Nothing to undo for selected files", StatusKind::Error);
            return 0;
        }

        // Exec ops are the inverses (current → restored path) — build them
        // once and reuse them for the batch "Revert" commit (per-item toggle
        // check; the current path is the inverse op's original_path).
        let mut pending: Vec<(usize, RenameOp)> = fwd
            .into_iter()
            .map(|(idx, op)| (idx, invert_op(&op)))
            .collect();
        let inverse_ops: Vec<RenameOp> = pending
            .iter()
            .filter(|(_, op)| !self.is_head_toggle_commit(&op.original_path))
            .map(|(_, op)| op.clone())
            .collect();
        if !inverse_ops.is_empty() {
            self.push_cwd_commit(Commit {
                timestamp: std::time::Instant::now(),
                label: Self::now_label("Revert"),
                ops: inverse_ops,
            });
        }

        let (ok, failed) = self.execute_ops_batch(&mut pending);
        self.status_map_dirty = true;
        self.preview_dirty = true;
        self.rebuild_rows_cache_sync();
        let status = if ok == 1 && failed == 0 && pending.len() == 1 {
            let op = &pending[0].1;
            if op.is_noop() {
                format!("Reverted metadata: {}", op.original_name)
            } else {
                format!("Undid rename: {} → {}", op.new_name, op.original_name)
            }
        } else {
            format!("Undid: {} reverted, {} failed", ok, failed)
        };
        self.set_status(
            &status,
            if failed > 0 {
                StatusKind::Error
            } else {
                StatusKind::Persistent
            },
        );
        ok
    }

    /// Redo the last undone rename of every file/folder in `indices` (typically
    /// the current selection) as one batch, children-before-parents like
    /// batch apply, with the same chain/swap resolution. Never creates
    /// commits — the Revert commit from the undo already preserves the state.
    /// Returns the number of successful redos.
    pub fn redo_files(&mut self, indices: &[usize]) -> usize {
        let mut path_ops: Vec<(usize, RenameOp)> = Vec::new();
        let mut effects_ops: Vec<(usize, RenameOp)> = Vec::new();
        for &idx in indices {
            if idx >= self.all_files.len() {
                continue;
            }
            if let Some(op) = self.find_redo_op(idx) {
                if op.is_noop() {
                    effects_ops.push((idx, op));
                } else {
                    path_ops.push((idx, op));
                }
            }
        }
        if path_ops.is_empty() && effects_ops.is_empty() {
            self.set_status("Nothing to redo for selected files", StatusKind::Error);
            return 0;
        }

        // In-place effects ops (attributes/timestamps, name unchanged) are not
        // renames: re-apply their recorded effects rather than routing them
        // through the chain-safe rename executor (which would restore the
        // pre-effect snapshot, i.e. redo in the wrong direction).
        let mut ok = 0usize;
        let ts_cache = awara::TimestampCache::new(&self.config);
        for (i, (_, op)) in effects_ops.iter().enumerate() {
            awara::reapply_metadata_cached(op, &ts_cache, i);
            ok += 1;
        }
        let failed = if path_ops.is_empty() {
            0
        } else {
            let (path_ok, path_failed) = self.execute_ops_batch(&mut path_ops);
            ok += path_ok;
            path_failed
        };

        self.status_map_dirty = true;
        self.preview_dirty = true;
        self.rebuild_rows_cache_sync();
        let total = ok + failed;
        let status = if ok == 1 && failed == 0 && total == 1 {
            let op = if let Some((_, op)) = effects_ops.first() {
                op
            } else {
                &path_ops[0].1
            };
            if op.is_noop() {
                format!("Re-applied metadata: {}", op.original_name)
            } else {
                format!("Redid rename: {} → {}", op.original_name, op.new_name)
            }
        } else {
            format!("Redid: {} reverted, {} failed", ok, failed)
        };
        self.set_status(
            &status,
            if failed > 0 {
                StatusKind::Error
            } else {
                StatusKind::Persistent
            },
        );
        ok
    }

    /// Single-item convenience used by tests. The UI uses `undo_files`.
    #[cfg(test)]
    pub fn undo_file(&mut self, idx: usize) {
        let _ = self.undo_files(&[idx]);
    }

    /// Single-item convenience used by tests. The UI uses `redo_files`.
    #[cfg(test)]
    pub fn redo_file(&mut self, idx: usize) {
        let _ = self.redo_files(&[idx]);
    }

    /// Remove toggle-commit (Revert/Redo) ops that reference any of the given paths.
    /// Called when a new forward commit branches the history for these files.
    pub(super) fn cull_stale_toggle_commits(&mut self, new_ops: &[RenameOp]) {
        let affected: HashSet<&Path> = new_ops
            .iter()
            .flat_map(|op| [&op.original_path as &Path, &op.new_path as &Path])
            .collect();
        for commits in self.commits_by_dir.values_mut() {
            for commit in commits.iter_mut() {
                if !commit.label.starts_with("Revert") && !commit.label.starts_with("Redo") {
                    continue;
                }
                commit.ops.retain(|op| {
                    let orig: &Path = &op.original_path;
                    let new: &Path = &op.new_path;
                    !affected.contains(orig) && !affected.contains(new)
                });
            }
            commits.retain(|c| !c.ops.is_empty());
        }
    }

    /// Remove commit ops that reference any of the given paths.
    /// Used by trash (surgical, not nuclear) and overwrite cleanup.
    pub(super) fn remove_commit_ops_by_paths(&mut self, paths: &HashSet<String>) {
        if paths.is_empty() {
            return;
        }
        for commits in self.commits_by_dir.values_mut() {
            for commit in commits.iter_mut() {
                commit.ops.retain(|op| {
                    let op_orig = op.original_path.to_string_lossy().to_string();
                    let op_new = op.new_path.to_string_lossy().to_string();
                    !paths.contains(&op_orig) && !paths.contains(&op_new)
                });
            }
            commits.retain(|c| !c.ops.is_empty());
        }
        self.redo_stack.retain(|c| !c.ops.is_empty());
        self.status_map_dirty = true;
    }

    /// Sweep stale commit ops whose paths no longer exist on disk.
    /// Called after a scan replaces `all_files`.
    pub(super) fn sweep_dead_commits(&mut self) {
        for commits in self.commits_by_dir.values_mut() {
            for commit in commits.iter_mut() {
                commit
                    .ops
                    .retain(|op| op.original_path.exists() || op.new_path.exists());
            }
            commits.retain(|c| !c.ops.is_empty());
        }
        self.commits_by_dir.retain(|_, v| !v.is_empty());
        self.redo_stack.retain(|c| !c.ops.is_empty());
    }
}
