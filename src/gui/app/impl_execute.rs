use super::*;

impl GuiApp {
    pub fn try_execute_renames(&mut self) {
        if self.executing || self.collision_dialog.is_some() {
            return;
        }
        self.ensure_file_names_synced();
        self.build_command_order();
        // The user edited the operation → unfreeze before the guard check
        // (config edits re-arm even if `update_preview` hasn't run yet this
        // frame — e.g. Apply clicked immediately after a field edit).
        if self.operation_changed() {
            self.set_processed(false);
        }
        // Nothing selected → leave the status bar alone.
        if self.selected_count() == 0 {
            return;
        }
        // The batch was already processed by a prior Apply — re-applying the
        // same operation is a no-op until the user reselects files or changes
        // the operation. Temporary: reverts to the ready message after the
        // standard status timeout.
        if self.processed {
            self.set_status(
                "All selected files have already been processed",
                StatusKind::Temporary,
            );
            return;
        }
        // Materialize the frozen display order first: it is a planner input, so
        // the plan must be built with exactly this order. The preview is handed
        // the same `get_display_order()` snapshot, which is what keeps its names
        // and the executed names identical.
        let display_order = self.get_display_order();
        let config = cleared_working_config(&self.config, &self.section_enabled);

        // Always plan fresh. Apply is a deliberate, one-shot action, so it does
        // not depend on the preview having computed a plan for exactly these
        // inputs first.
        let selected_files: Vec<String> = display_order
            .iter()
            .filter(|&&i| self.selection.get(i).copied().unwrap_or(true))
            .filter_map(|&i| self.file_names.get(i).cloned())
            .collect();
        let output_dir = config
            .copy_to
            .output_dir
            .as_deref()
            .map(|d| awara::resolve_output_dir(d, &self.cwd));
        let base_dir = if config.copy_to.keep_structure {
            Some(self.cwd.clone())
        } else {
            None
        };
        let plan_opts = awara::PlanOptions {
            dirname_level: config.append_folder.dirname_level,
            output_dir: output_dir.as_deref(),
            keep_structure: config.copy_to.keep_structure,
            base_dir: base_dir.as_deref(),
            parallel: true,
            // Copy mode is recorded on the plan: it decides whether a source is
            // vacated, and therefore collision classification.
            copy_mode: output_dir.is_some() && config.copy_to.copy_mode,
        };
        let plan = awara::plan_renames(&selected_files, &config, &plan_opts);

        if plan.is_empty() {
            // No sections configured at all → nothing to apply. An operation
            // that is configured but produces no changes falls back to the
            // generic message. Both are self-clearing statuses.
            if self.config.command_order.is_empty() {
                self.set_status("No rename operations configured", StatusKind::Temporary);
            } else {
                self.set_status("No files to rename", StatusKind::Temporary);
            }
            return;
        }

        // Validate (navigational names are refused, folder creation is confirmed)
        // and dispatch. The processed guard is disarmed inside `dispatch_plan`,
        // so a batch blocked by either gate can be retried after fixing it.
        self.gate_and_dispatch(plan, "Apply", false);
    }

    /// Validate a freshly-planned batch, then dispatch it.
    ///
    /// Two gates run before the processed guard is disarmed:
    ///
    /// 1. **Navigational names** — a computed name that would navigate the path
    ///    (rooted, `.` or `..`) cannot be executed (SSoT:
    ///    `awara::path_navigation_error`), so Apply is blocked and explained.
    /// 2. **Folder creation** — a rename that will create subfolders asks first,
    ///    unless the user checked "Don't ask again" (`skip_dir_creation_warning`).
    pub(crate) fn gate_and_dispatch(
        &mut self,
        plan: awara::RenamePlan,
        label_prefix: &str,
        inline: bool,
    ) {
        // One pass over the ops: navigational names are refused, and the ones
        // that will create folders are counted (for the confirmation below).
        let mut invalid: Vec<(String, String)> = Vec::new();
        let mut dir_count = 0usize;
        for op in plan.ops() {
            if let Some(nav) = awara::path_navigation_error(&op.new_name) {
                invalid.push((
                    op.original_name.clone(),
                    format!("{} — {}", op.new_name, nav.message()),
                ));
            }
            if op.name_implies_subpath() {
                dir_count += 1;
            }
        }
        if !invalid.is_empty() {
            self.invalid_name_entries = invalid;
            self.invalid_name_init_focus = true;
            self.show_invalid_name_warning = true;
            return;
        }

        if dir_count > 0 && !self.skip_dir_creation_warning {
            self.pending_dir_count = dir_count;
            self.pending_dir_label = label_prefix.to_string();
            self.pending_dir_inline = inline;
            self.pending_dir_plan = Some(plan);
            self.dir_warning_init_focus = true;
            self.show_dir_creation_warning = true;
            return;
        }

        self.dispatch_plan(plan, label_prefix, inline);
    }

    /// Disarm the processed guard and run a validated plan: execute it when it
    /// has no collisions, otherwise open the collision dialog with it.
    pub(crate) fn dispatch_plan(
        &mut self,
        plan: awara::RenamePlan,
        label_prefix: &str,
        inline: bool,
    ) {
        // The user clicked Apply with real work, so even a fully-failed or
        // fully-skipped run leaves the batch processed until a deliberate
        // reselect/operation change. Record the baseline so the guard compares
        // against what was actually applied.
        self.last_dispatched_working = Some(self.op_fingerprint());
        self.last_dispatched_section_enabled = Some(self.section_enabled);
        self.set_processed(true);

        let collisions = plan.collisions();
        if collisions.is_empty() {
            // Fast path — no collisions, execute directly.
            let resolution = awara::Resolution::default_for(CollisionStrategy::Skip);
            self.execute_plan_inner(plan, &resolution, label_prefix, inline);
        } else {
            // Show dialog; keep the plan so resolution executes the exact ops
            // that were checked.
            self.collision_dialog = Some(CollisionDialogState::from_plan(&plan, &collisions));
            self.pending_plan = Some(plan);
        }
    }

    /// Confirm the folder-creation warning: dispatch the plan it was holding.
    /// An F2 plan with a collision resumes into the F2 collision dialog.
    pub(crate) fn confirm_dir_creation_warning(&mut self) {
        self.show_dir_creation_warning = false;
        if let Some(plan) = self.pending_dir_plan.take() {
            if let Some(state) = self.pending_f2_collision.take() {
                self.collision_dialog = Some(state);
                self.pending_plan = Some(plan);
            } else {
                let label = std::mem::take(&mut self.pending_dir_label);
                let inline = self.pending_dir_inline;
                self.dispatch_plan(plan, &label, inline);
            }
        }
    }

    /// Cancel the folder-creation warning: drop the pending plan, nothing runs.
    pub(crate) fn cancel_dir_creation_warning(&mut self) {
        self.show_dir_creation_warning = false;
        self.pending_dir_plan = None;
        self.pending_f2_collision = None;
    }

    /// Dismiss the invalid-name modal. Apply stays blocked until the operation
    /// no longer produces a navigational name.
    pub(crate) fn dismiss_invalid_name_warning(&mut self) {
        self.show_invalid_name_warning = false;
        self.invalid_name_entries.clear();
    }

    /// Backward-compatible entry point used by tests.
    #[cfg(test)]
    pub fn execute_renames(&mut self) {
        self.try_execute_renames();
    }

    /// Execute a pre-computed plan and update all app state.
    ///
    /// `resolution` carries the collision policy: per-op decisions from the
    /// dialog, or a default strategy. The plan already has numbering and
    /// output-dir paths applied, so per-op skips never renumber later ops.
    ///
    /// `inline` is true for an F2 single-file edit: config-driven behaviors
    /// (output dir, copy mode, attributes, timestamps) are bypassed so the file
    /// is simply renamed in place.
    pub(crate) fn execute_plan_inner(
        &mut self,
        plan: awara::RenamePlan,
        resolution: &awara::Resolution,
        label_prefix: &str,
        inline: bool,
    ) {
        self.ensure_file_names_synced();
        if plan.is_empty() {
            return;
        }
        self.executing = true;
        let strategy = resolution.default_strategy;

        // Clone config and clear disabled sections so they don't affect execution.
        // Inline edits use a default config: no timestamps/attributes are applied.
        let config = if inline {
            RenameConfig::default()
        } else {
            cleared_working_config(&self.config, &self.section_enabled)
        };
        // Copy mode lives on the plan now (it decides whether a source is
        // vacated). `output_dir` still gates it: without a target directory
        // there's nowhere to copy to.
        let undo_file = if self.undo_file.is_empty() {
            None
        } else {
            Some(self.undo_file.clone())
        };
        let set_attrs = if inline {
            None
        } else {
            config.special.set_attributes.clone()
        };
        let warnings: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
        let mut options = RenameOptions {
            dry_run: false,
            stop_on_error: false,
            preserve_timestamps: !inline && config.preserve_timestamps,
            set_attributes: set_attrs.as_deref(),
            undo_file: undo_file.as_deref(),
            cancel_token: None,
            on_event: Some(Box::new(|event| {
                if let awara::RenameEvent::Status { message } = event {
                    warnings.borrow_mut().push(message);
                }
            })),
        };
        let result = awara::execute_plan(plan, &config, &mut options, resolution);
        drop(options);
        let warnings = warnings.into_inner();
        let (success, errors, skipped) = (
            result.successful_ops.len(),
            result.failed_ops.len(),
            result.skipped_ops.len(),
        );
        let mut message = format!("Done: {success} renamed, {errors} errors, {skipped} skipped");
        if !warnings.is_empty() {
            message.push_str(" — ");
            message.push_str(&warnings.join("; "));
        }
        self.set_status(
            &message,
            if errors > 0 {
                StatusKind::Error
            } else {
                StatusKind::Persistent
            },
        );

        // When Overwrite was used, remove overwritten-file commits and entries from all_files.
        if strategy == CollisionStrategy::Overwrite {
            // Build overwrite_set: entries whose path matches a success target but
            // whose path is NOT among the success sources → overwritten files on disk.
            let success_targets: HashSet<String> = result
                .successful_ops
                .iter()
                .map(|op| op.new_path.to_string_lossy().to_string())
                .collect();

            let success_sources: HashSet<String> = result
                .successful_ops
                .iter()
                .map(|op| op.original_path.to_string_lossy().to_string())
                .collect();

            // Collect indices to remove.  An entry is stale when its path matches
            // a success target (the file was replaced on disk) but is not itself a
            // success source (it wasn't the file doing the renaming).
            // Selection is irrelevant — the file is gone regardless.
            let remove_indices: Vec<usize> = self
                .file_names
                .iter()
                .enumerate()
                .filter(|(_, name)| {
                    success_targets.contains(*name) && !success_sources.contains(*name)
                })
                .map(|(i, _)| i)
                .collect();

            if !remove_indices.is_empty() {
                // The overwritten files are gone from the listing. The helper hands
                // back their paths (which the cleanup below filters commits by) and
                // prunes the display order rather than rebuilding it — the table
                // keeps the order the user sorted it into.
                let overwrite_paths: HashSet<String> = self
                    .remove_file_indices(&remove_indices)
                    .into_iter()
                    .collect();

                // Remove overwritten paths from commits_by_dir.
                for commits in self.commits_by_dir.values_mut() {
                    for commit in commits.iter_mut() {
                        commit.ops.retain(|op| {
                            let op_orig = op.original_path.to_string_lossy().to_string();
                            let op_new = op.new_path.to_string_lossy().to_string();
                            !overwrite_paths.contains(&op_orig)
                                && !overwrite_paths.contains(&op_new)
                        });
                    }
                    commits.retain(|c| !c.ops.is_empty());
                }
                // Also clear redo_stack entries referencing overwritten paths.
                for commit in &mut self.redo_stack {
                    commit.ops.retain(|op| {
                        let op_orig = op.original_path.to_string_lossy().to_string();
                        let op_new = op.new_path.to_string_lossy().to_string();
                        !overwrite_paths.contains(&op_orig) && !overwrite_paths.contains(&op_new)
                    });
                }
                self.redo_stack.retain(|c| !c.ops.is_empty());
                self.status_map_dirty = true;
            }
        }

        // Whether a move that landed elsewhere stays in the listing. A
        // non-recursive scan lists only the cwd's direct children, so moving an
        // entry into a subfolder (or anywhere outside) takes it out of the pane;
        // a recursive scan (subfolders on) keeps anything still under the cwd.
        // This is what keeps the pane matching a fresh scan without one.
        let recursive = self.config.filters.filter_subfolders;
        let cwd = self.cwd.clone();
        let stays_in_listing = |op: &awara::RenameOp| -> bool {
            if cwd.as_os_str().is_empty() {
                return false;
            }
            if recursive {
                op.new_path.starts_with(&cwd)
            } else {
                op.new_path.parent() == Some(cwd.as_path())
            }
        };

        // Map: original_path → new_path for successful *moves that stay* in the
        // listing. A copied entry (copy mode, or a relocated directory) leaves
        // its source in place, so the listing keeps pointing at the source; a
        // move that left the folder is removed from the listing instead (below).
        let path_updates: std::collections::HashMap<String, std::path::PathBuf> = result
            .successful_ops
            .iter()
            .filter(|op| !op.was_copy && stays_in_listing(op))
            .map(|op| {
                (
                    op.original_path.to_string_lossy().to_string(),
                    op.new_path.clone(),
                )
            })
            .collect();

        // Source paths of moves that left the listing: their rows are dropped so
        // the pane reflects the new location, as a manual refresh would.
        let left_listing: HashSet<String> = result
            .successful_ops
            .iter()
            .filter(|op| !op.was_copy && !stays_in_listing(op))
            .map(|op| op.original_path.to_string_lossy().to_string())
            .collect();

        // New folders to show: the first path component under the cwd that a
        // move descended into (`sub` for `sub/file` or `sub/deep/file`), when it
        // is a directory and not already listed. The tree gains the node through
        // the invalidation below.
        let mut new_dirs: Vec<std::path::PathBuf> = Vec::new();
        if self.config.filters.filter_folders && !cwd.as_os_str().is_empty() {
            for op in &result.successful_ops {
                let Ok(rel) = op.new_path.strip_prefix(&cwd) else {
                    continue;
                };
                // One pass over the components: the first is the folder under the
                // cwd; a second component means the move descended into it.
                let mut components = rel.components();
                let Some(first) = components.next() else {
                    continue;
                };
                if components.next().is_none() {
                    continue; // direct child of cwd: no folder was created
                }
                let candidate = cwd.join(first.as_os_str());
                if new_dirs.contains(&candidate) {
                    continue; // already queued — skip the stat for duplicates
                }
                let cand_str = candidate.to_string_lossy().to_string();
                if self.file_names.contains(&cand_str) || !candidate.is_dir() {
                    continue;
                }
                new_dirs.push(candidate);
            }
        }

        // Whether the raw (pre-filter) scan is live. Captured before the removals
        // below so an apply that empties the raw list still appends new folders.
        let maintain_raw = !self.raw_all_files.is_empty();

        // Directory renames for tree cache invalidation. `was_copy` entries are
        // new directories beside an untouched source — invalidate the parents,
        // but never re-key the source away (it is still on disk).
        let dir_changes: Vec<(std::path::PathBuf, std::path::PathBuf, bool)> = result
            .successful_ops
            .iter()
            .filter(|op| op.new_path.is_dir())
            .map(|op| (op.original_path.clone(), op.new_path.clone(), op.was_copy))
            .collect();

        // Ops that moved the entry to a different directory are structural
        // transfers, not in-place renames, so they are never recorded for
        // undo/redo/revert — a Copy / Move to Location relocation, or a
        // path-producing rename (a computed name carrying a separator). Both are
        // explicit facts on the op (`RenameOp::is_undoable`), never inferred from
        // path shape. Any other op in the same apply is still recorded; if none
        // remain, no commit is created at all.
        let commit_ops: Vec<awara::RenameOp> = result
            .successful_ops
            .iter()
            .filter(|op| op.is_undoable())
            .cloned()
            .collect();

        if !commit_ops.is_empty() {
            let commit_label = {
                let sys_now = std::time::SystemTime::now();
                let dur = sys_now
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                let total_secs = dur.as_secs();
                let h = (total_secs / 3600) % 24;
                let m = (total_secs / 60) % 60;
                let s = total_secs % 60;
                format!("{} — {:02}:{:02}:{:02}", label_prefix, h, m, s)
            };
            self.push_cwd_commit(Commit {
                timestamp: std::time::Instant::now(),
                label: commit_label,
                ops: commit_ops.clone(),
            });
            // A new rename branches the history — clear redo and cull stale toggles.
            self.cull_stale_toggle_commits(&commit_ops);
            self.redo_stack.clear();
        }

        // Update status_map with results from this execution only.
        // Don't clear — that would lose error statuses from prior executions.
        // insert() overwrites any previous entry for the same path.
        for op in &result.successful_ops {
            let orig = op.original_path.to_string_lossy().to_string();
            let new = op.new_path.to_string_lossy().to_string();
            self.status_map.insert(orig, (0u8, None));
            self.status_map.insert(new, (0u8, None));
        }
        for (op, err) in &result.failed_ops {
            let orig = op.original_path.to_string_lossy().to_string();
            self.status_map.insert(orig, (1u8, Some(err.clone())));
        }
        for (op, _) in &result.skipped_ops {
            let orig = op.original_path.to_string_lossy().to_string();
            self.status_map.insert(orig, (2u8, None));
        }
        self.status_map_dirty = false;

        // Update all_files in-place for renamed entries that stayed in the
        // listing. Deduplicate: when multiple ops target the same path
        // (collision + Overwrite), only the first matching entry gets the new
        // path; subsequent duplicates are removed — the overwritten file is gone
        // from disk.
        let mut seen_new_paths: HashSet<String> = HashSet::new();
        let mut remove_indices: Vec<usize> = Vec::new();
        for (i, p) in self.all_files.iter_mut().enumerate() {
            if let Some(new_path) = path_updates.get(&self.file_names[i]) {
                let new_str = new_path.to_string_lossy().to_string();
                if seen_new_paths.contains(&new_str) {
                    // Duplicate: another entry already moved to this path.
                    remove_indices.push(i);
                } else {
                    *p = new_path.clone();
                    self.file_names[i] = new_str.clone();
                    seen_new_paths.insert(new_str);
                }
            }
        }
        // Rows whose entry left the folder (`folder/file` relocations, or any
        // move out of scope): drop them, exactly as a refresh would.
        remove_indices.extend(
            self.file_names
                .iter()
                .enumerate()
                .filter(|(_, name)| left_listing.contains(*name))
                .map(|(i, _)| i),
        );
        // One removal pass: only the first entry keeps a contested path, and
        // departed entries no longer name a file in this folder. Indices shift,
        // so the display order is pruned rather than rebuilt.
        self.remove_file_indices(&remove_indices);
        // Keep the raw scan consistent too, so a later filter change cannot
        // resurrect a departed entry.
        if maintain_raw {
            for src in &left_listing {
                self.remove_raw_path(src);
            }
        }

        // The folders the moves created appear as rows (after the survivors,
        // keeping the frozen order intact) so the pane reflects the new layout
        // without a manual refresh. A folder the in-memory filters would exclude
        // is not shown (it still lands in the raw scan, so clearing the filter
        // reveals it, matching a rescan).
        for dir in new_dirs {
            if self.passes_in_memory_filters(&dir, true) {
                self.append_listing_entry(dir.clone(), 0, 0, true);
            }
            if maintain_raw {
                self.append_raw_entry(dir, 0, 0, true);
            }
        }
        self.post_names_changed();

        // Successful ops that landed in a *different* directory. A path-producing
        // rename (computed name with a separator) or a location transfer may have
        // created that directory, so the tree must re-read it.
        let moved: Vec<(std::path::PathBuf, std::path::PathBuf)> = result
            .successful_ops
            .iter()
            .filter(|op| op.lands_in_other_directory())
            .map(|op| (op.original_path.clone(), op.new_path.clone()))
            .collect();

        // ── Tree cache invalidation for renamed / moved / new directories ──
        // Invalidates old parents (lost a child), new parents (gained a child)
        // that are currently expanded, and re-keys tree data on old→new path
        // for genuine renames (a copy keeps the source, so no re-key).
        if !dir_changes.is_empty() || !moved.is_empty() {
            let mut parents = HashSet::new();
            for (old, new, was_copy) in &dir_changes {
                if let Some(parent) = old.parent() {
                    parents.insert(parent.to_path_buf());
                }
                // New parent may have gained a directory (move into / copy into).
                // Skip the parent if it's collapsed — lazy re-scan on expand.
                if let Some(parent) = new.parent()
                    && self.tree_expanded.contains(parent)
                {
                    parents.insert(parent.to_path_buf());
                }
                if !was_copy {
                    self.rekey_dir(old, new);
                    if self.cwd == *old {
                        self.cwd = new.clone();
                        self.cwd_input = new.to_string_lossy().to_string();
                    }
                }
            }
            // Any move may have created the destination folder: re-read the old
            // parent, the destination folder, and the destination's parent, so a
            // newly created subfolder appears without a manual refresh.
            for (old, new) in &moved {
                if let Some(parent) = old.parent() {
                    parents.insert(parent.to_path_buf());
                }
                if let Some(parent) = new.parent() {
                    parents.insert(parent.to_path_buf());
                    if let Some(grandparent) = parent.parent() {
                        parents.insert(grandparent.to_path_buf());
                    }
                }
            }
            for parent in &parents {
                self.invalidate_tree_scan(parent);
                self.tree_has_subdirs.remove(parent);
            }
        }

        self.preview_dirty = true;
        // Rebuild row cache synchronously so the status column shows execution
        // results (success ✓ / fail ✗ + error message) on the very next frame,
        // regardless of when the async preview result arrives.
        self.rebuild_rows_cache_sync();
        self.executing = false;
    }

    /// Open the revert dialog — considers all files in the preview and
    /// all commits across all directories (not just the current cwd).
    pub fn revert_renames(&mut self) {
        if self.executing {
            return;
        }
        let all_commits: Vec<&Commit> = self.all_commits().collect();
        if all_commits.is_empty() {
            self.set_status(
                "No rename history found in any directory",
                StatusKind::Error,
            );
            return;
        }
        if all_commits.iter().all(|c| c.ops.is_empty()) {
            self.set_status("Nothing to revert", StatusKind::Error);
            return;
        }
        self.active_popup = Some(PopupKind::RevertDialog);
        // Clear revert status so the dialog always opens fresh.
        self.revert_status_map.clear();
        self.build_revert_dialog_state();
    }
}
