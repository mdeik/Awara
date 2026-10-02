use std::collections::{HashMap, HashSet};

type ForwardEntry = (
    usize,
    PathBuf,
    String,
    bool,
    Option<PermSnapshot>,
    String,
    u8,
);
use std::path::PathBuf;

use crate::gui::app::GuiApp;
use awara::{PermSnapshot, RenameOp, apply_ops_chain_safe};

use crate::gui::types::{
    Commit, PopupKind, RevertDialogState, RevertEffect, RevertEntry, StatusKind,
};

/// One state in a file's lineage, gathered while walking the backward/forward
/// op maps: (path, name, producing commit index, pre-op snapshot, applied
/// attributes, applied timestamp mask).
type ChainSegment = (
    PathBuf,
    String,
    Option<usize>,
    Option<PermSnapshot>,
    String,
    u8,
);

/// Maps new_path → (commit_idx, original_path, original_name, was_copy, perms, attr, ts_mask)
type PathInfo = (
    usize,
    PathBuf,
    String,
    bool,
    Option<PermSnapshot>,
    String,
    u8,
);

impl GuiApp {
    /// Build the revert dialog state from all files in the current preview.
    ///
    /// Performance: uses a single HashMap pass over all commits to build
    /// O(1) path→producer lookups, then only iterates files that have
    /// commit history — no per-file commit scan.
    pub fn build_revert_dialog_state(&mut self) {
        // 1. Collect all commits and build bidirectional lookup maps.
        //    backward: new_path → (commit_idx, original_path, original_name, was_copy, perms, attr, ts_mask)
        //    forward:  original_path → (commit_idx, new_path, new_name, was_copy, perms, attr, ts_mask)
        //    Iterate forward so newer commits overwrite older entries, giving
        //    the most recent producer for each path.
        let all_commits: Vec<&Commit> = self.all_commits().collect();
        let n_commits = all_commits.len();
        // backward: new_path → (ci, original_path, original_name, was_copy, perms, attr, ts_mask)
        let mut backward: HashMap<PathBuf, PathInfo> = HashMap::new();
        // forward: original_path → (ci, new_path, new_name, was_copy, perms, attr, ts_mask)
        let mut forward: HashMap<PathBuf, ForwardEntry> = HashMap::new();
        let mut committed_paths: HashSet<PathBuf> = HashSet::new();
        // Build backward/forward maps:
        // 1. Forward commits inserted first (they define the true history).
        // 2. Toggle commits inserted after, but only for keys NOT already in map.
        //    This keeps forward mappings as the primary lineage while still
        //    allowing toggle commits to contribute unique boundary paths.
        for (ci, commit) in all_commits.iter().enumerate() {
            let is_toggle = commit.label.starts_with("Revert") || commit.label.starts_with("Redo");
            for op in &commit.ops {
                if !is_toggle || !backward.contains_key(&op.new_path) {
                    backward.insert(
                        op.new_path.clone(),
                        (
                            ci,
                            op.original_path.clone(),
                            op.original_name.clone(),
                            op.was_copy,
                            op.original_permissions.clone(),
                            op.applied_attributes.clone(),
                            op.applied_timestamps_mask,
                        ),
                    );
                }
                if !is_toggle || !forward.contains_key(&op.original_path) {
                    forward.insert(
                        op.original_path.clone(),
                        (
                            ci,
                            op.new_path.clone(),
                            op.new_name.clone(),
                            op.was_copy,
                            op.original_permissions.clone(),
                            op.applied_attributes.clone(),
                            op.applied_timestamps_mask,
                        ),
                    );
                }
                committed_paths.insert(op.new_path.clone());
                committed_paths.insert(op.original_path.clone());
            }
        }

        // 2. Build entries — only iterate files that have been touched by any commit.
        let mut entries = Vec::new();
        // Use sorted_indices for deterministic display order that matches the preview table.
        let display_indices = self.sorted_indices();
        for &idx in display_indices.iter() {
            let current_path = match self.all_files.get(idx) {
                Some(p) => p.clone(),
                None => continue,
            };
            // Skip files never touched by any commit
            if !committed_paths.contains(&current_path) {
                continue;
            }

            let current_name = current_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // 3. Build full lineage by walking BOTH backward and forward from the current path.
            //    Use a seen set to detect cycles (e.g. a→b→a from back-to-back renames).
            //    Both walks also stop when the next hop was produced by the SAME
            //    commit as the segment just left: within one commit, chained ops
            //    (a→b, b→c) share intermediate paths between DIFFERENT files, so
            //    the earlier links belong to another file's lineage (a batch
            //    revert then rolls each file back one step instead of both to the
            //    origin).

            // Walk backward (current → original → original-of-original …)
            let mut backward_chain: Vec<ChainSegment> = Vec::new();
            let mut seen: HashSet<PathBuf> = HashSet::new();
            let mut cursor = current_path.clone();
            let mut cursor_name = current_name.clone();
            let mut prev_producer: Option<usize> = None;
            seen.insert(cursor.clone());
            while let Some((ci, orig_path, orig_name, _, perms, attr, ts_mask)) =
                backward.get(&cursor)
            {
                if prev_producer == Some(*ci) {
                    break;
                }
                backward_chain.push((
                    cursor.clone(),
                    cursor_name.clone(),
                    Some(*ci),
                    perms.clone(),
                    attr.clone(),
                    *ts_mask,
                ));
                cursor = orig_path.clone();
                cursor_name = orig_name.clone();
                prev_producer = Some(*ci);
                if !seen.insert(cursor.clone()) {
                    break;
                }
            }
            backward_chain.push((
                cursor.clone(),
                cursor_name.clone(),
                None,
                None,
                String::new(),
                0u8,
            ));

            // Walk forward (current → new → new-of-new …), skipping the current entry
            // since it's already in backward_chain as the last (most recent) element.
            let mut forward_chain: Vec<ChainSegment> = Vec::new();
            let mut f_cursor = current_path.clone();
            // Use a separate seen set (starts empty — f_cursor may also appear in backward_chain,
            // but we only care about cycles in the forward direction here).
            let mut f_seen: HashSet<PathBuf> = HashSet::new();
            f_seen.insert(f_cursor.clone());
            // Producer of the current segment — a forward hop from the same commit
            // would be a chain intermediate belonging to a different file.
            let mut f_prev = backward.get(&f_cursor).map(|(ci, _, _, _, _, _, _)| *ci);
            while let Some((ci, new_path, new_name, _, perms, attr, ts_mask)) =
                forward.get(&f_cursor)
            {
                if f_prev == Some(*ci) {
                    break;
                }
                forward_chain.push((
                    new_path.clone(),
                    new_name.clone(),
                    Some(*ci),
                    perms.clone(),
                    attr.clone(),
                    *ts_mask,
                ));
                f_cursor = new_path.clone();
                f_prev = Some(*ci);
                if !f_seen.insert(f_cursor.clone()) {
                    break;
                }
            }

            // Merge: backward_chain (oldest-first after rev) + forward_chain
            // backward_chain is in reverse order: [current, ..., oldest]
            // After .rev(): [oldest, ..., current]
            let mut chain: Vec<ChainSegment> = backward_chain.into_iter().rev().collect();
            // Append forward states (skip the first if it duplicates chain's last)
            for f_item in forward_chain {
                if let Some(last) = chain.last()
                    && last.0 == f_item.0
                {
                    continue;
                }
                chain.push(f_item);
            }

            let lineage: Vec<(PathBuf, String, Option<usize>)> = chain
                .iter()
                .map(|(p, n, ci, _, _, _)| (p.clone(), n.clone(), *ci))
                .collect();

            // 4. Determine revertability (early exit before expensive target_by_commit).
            let exists = current_path.exists();
            let has_history = lineage.len() > 1;
            if !exists || !has_history {
                continue;
            }

            // 5. Compute target_by_commit efficiently.
            //    target_by_commit[ci] = the rollback name when commit ci is the
            //    active (reverting) commit: the lineage name immediately before
            //    the most recent segment produced by commit ci. Defaults to the
            //    current name (commit ci didn't touch this file → no-op).
            //    Unlike "always roll back to the origin", this handles chains
            //    WITHIN a single commit (a→b, b→c in one commit): entry c rolls
            //    back to b and entry b to a, so a batch revert resolves through
            //    the chain-safe executor instead of both targeting the origin
            //    and colliding. Swaps (a↔b) likewise cross-target and resolve.
            let mut target_by_commit = vec![current_name.clone(); n_commits];
            for (ci, target) in target_by_commit.iter_mut().enumerate() {
                for (idx, segment) in lineage.iter().enumerate().rev() {
                    if segment.2 == Some(ci) {
                        if idx > 0 {
                            *target = lineage[idx - 1].1.clone();
                        }
                        break;
                    }
                }
            }

            // 6. Per-commit metadata effects (read from the producing op, captured
            //    while walking the chain). Later segments overwrite earlier ones,
            //    so the newest segment for a commit wins — matching target_by_commit.
            let mut effects_by_commit: Vec<Option<RevertEffect>> = vec![None; n_commits];
            for (_, _, ci_opt, perms, attr, ts_mask) in &chain {
                if let Some(ci) = ci_opt {
                    effects_by_commit[*ci] = Some(RevertEffect {
                        snapshot: perms.clone(),
                        applied_attributes: attr.clone(),
                        applied_timestamps_mask: *ts_mask,
                    });
                }
            }

            entries.push(RevertEntry {
                current_path,
                current_name,
                lineage,
                target_by_commit,
                effects_by_commit,
                revertable: true,
                block_reason: None,
            });
        }

        if entries.is_empty() {
            self.set_status(
                "No files in the current view have rename history",
                StatusKind::Error,
            );
            self.active_popup = None;
            return;
        }
        // Ensure active_popup is set so draw_windows renders the modal.
        // May already be set when called from revert_renames, but might not
        // be when called after confirm_revert rebuilds the dialog.
        self.active_popup = Some(PopupKind::RevertDialog);
        let n_entries = entries.len();
        let active_commit_idx = n_commits.saturating_sub(1);

        // 7. Compute initial sort keys and display order.
        let sort_keys: Vec<String> = entries.iter().map(|e| e.current_name.clone()).collect();
        let mut display_order: Vec<usize> = (0..n_entries).collect();
        display_order.sort_by(|&a, &b| sort_keys[a].cmp(&sort_keys[b]));

        self.revert_dialog = Some(RevertDialogState {
            entries,
            selection: vec![false; n_entries],
            active_commit_idx,
            executing: false,
            col_widths: self.revert_col_widths.clone(),
            pending_context_action: None,
            last_clicked_idx: None,
            selection_anchor: None,
            sort_col: 0,
            sort_asc: true,
            show_properties_idx: None,
            display_order,
            display_order_dirty: false,
            sort_keys,
            scroll_to_idx: None,
            scroll_sequence_start: None,
            scroll_last_event: None,
            edge_drag_start: None,
            box_select_anchor: None,
            cached_selected_count: None,
        });
    }

    /// Format attribute changes into a concise label.
    pub(crate) fn format_attr_summary(applied: &str) -> String {
        if applied.is_empty() {
            String::new()
        } else {
            applied.to_string()
        }
    }

    /// Format timestamp changes into a concise label.
    pub(crate) fn format_ts_summary(mask: u8) -> String {
        if mask == 0 {
            return String::new();
        }
        let mut parts = Vec::with_capacity(3);
        if mask & awara::TS_CREATED != 0 {
            parts.push("created");
        }
        if mask & awara::TS_MODIFIED != 0 {
            parts.push("modified");
        }
        if mask & awara::TS_ACCESSED != 0 {
            parts.push("accessed");
        }
        parts.join(", ")
    }

    /// Attribute-summary text for `entry`'s effects at `commit_idx` (empty when
    /// the commit applied no attributes). Reads the entry's per-commit effect,
    /// so the dialog's columns follow the *active* commit.
    pub(crate) fn revert_attr_summary(entry: &RevertEntry, commit_idx: usize) -> String {
        entry
            .effects_by_commit
            .get(commit_idx)
            .and_then(|e| e.as_ref())
            .map(|e| Self::format_attr_summary(&e.applied_attributes))
            .unwrap_or_default()
    }

    /// Timestamp-summary text for `entry`'s effects at `commit_idx`.
    pub(crate) fn revert_ts_summary(entry: &RevertEntry, commit_idx: usize) -> String {
        entry
            .effects_by_commit
            .get(commit_idx)
            .and_then(|e| e.as_ref())
            .map(|e| Self::format_ts_summary(e.applied_timestamps_mask))
            .unwrap_or_default()
    }

    /// Execute the selected reverts and split commits as needed.
    pub fn confirm_revert(&mut self) {
        let state = match self.revert_dialog.take() {
            Some(s) => s,
            None => return,
        };
        if state.executing {
            self.revert_dialog = Some(state);
            return;
        }

        // 1. Execute renames and build inverse ops
        let mut inverse_ops: Vec<RenameOp> = Vec::new();
        let mut path_updates: std::collections::HashMap<String, PathBuf> =
            std::collections::HashMap::new();
        let mut dir_reverts: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut errors = 0usize;

        let active_ci = state.active_commit_idx;

        // Build lookup: current_path → (was_copy, original_permissions) from the
        // active commit. Same index space as the dialog (`all_commits`), so the
        // active commit is always the one the user selected in the sidebar.
        let mut forward_info: HashMap<PathBuf, (bool, Option<PermSnapshot>)> = HashMap::new();
        if let Some(active_commit) = self.all_commits().nth(active_ci) {
            for fwd_op in &active_commit.ops {
                forward_info.insert(
                    fwd_op.new_path.clone(),
                    (fwd_op.was_copy, fwd_op.original_permissions.clone()),
                );
            }
        }

        // Collect exec ops (current → target) for every selected entry.
        let mut pending: Vec<(usize, RenameOp)> = Vec::new();
        for (i, &selected) in state.selection.iter().enumerate() {
            if !selected {
                continue;
            }
            let ci = active_ci;
            let entry = &state.entries[i];
            let target_name = &entry.target_by_commit[ci];
            let effect = entry.effects_by_commit.get(ci).and_then(|e| e.as_ref());

            // Find target path: walk lineage for the version matching target_name
            let target_path = entry
                .lineage
                .iter()
                .rev()
                .find(|(_, n, _)| n == target_name)
                .map(|(p, _, _)| p.clone())
                .unwrap_or_else(|| entry.lineage[0].0.clone());

            if target_path == entry.current_path {
                // Name unchanged: offer an effects-only rollback when the active
                // commit applied attributes/timestamps (a no-op `RenameOp`).
                // `apply_ops_chain_safe` executes the no-op branch by restoring
                // the pre-op snapshot — the same path batch Undo uses.
                if let Some(e) = effect
                    && e.is_restorable()
                {
                    pending.push((
                        i,
                        RenameOp {
                            original_path: entry.current_path.clone(),
                            new_path: entry.current_path.clone(),
                            original_name: entry.current_name.clone(),
                            new_name: entry.current_name.clone(),
                            was_copy: false,
                            relocated: false,
                            original_permissions: e.snapshot.clone(),
                            applied_attributes: e.applied_attributes.clone(),
                            applied_timestamps_mask: e.applied_timestamps_mask,
                        },
                    ));
                }
                continue;
            }

            let (was_copy, perms_snap) = forward_info
                .get(&entry.current_path)
                .cloned()
                .unwrap_or((false, None));
            // Prefer the snapshot captured before the active commit's segment
            // (the producing op's own snapshot), falling back to the commit-level
            // info for cases where the entry's effect isn't known.
            let perms_snap = effect.and_then(|e| e.snapshot.clone()).or(perms_snap);

            pending.push((
                i,
                RenameOp {
                    original_path: entry.current_path.clone(),
                    new_path: target_path,
                    original_name: entry.current_name.clone(),
                    new_name: target_name.clone(),
                    was_copy,
                    relocated: false,
                    // Copy mode: original was never modified — the executor's
                    // copy branch deletes the copy and never applies perms.
                    original_permissions: if was_copy { None } else { perms_snap },
                    applied_attributes: String::new(),
                    applied_timestamps_mask: 0u8,
                },
            ));
        }

        // Execute all reverts chain-safely: chains/swaps resolve via temp-name
        // deferral (no clobbering), genuine on-disk collisions fail per entry.
        let ops: Vec<RenameOp> = pending.iter().map(|(_, op)| op.clone()).collect();
        let result = apply_ops_chain_safe(&ops);
        let mut ok_by_current: HashMap<PathBuf, RenameOp> = HashMap::new();
        for op in &result.successful {
            ok_by_current.insert(op.original_path.clone(), op.clone());
        }
        let mut failed_by_current: HashMap<PathBuf, String> = HashMap::new();
        for (op, err) in &result.failed {
            failed_by_current.insert(op.original_path.clone(), err.clone());
        }

        for (_, op) in &pending {
            let current_str = op.original_path.to_string_lossy().to_string();
            if let Some(err) = failed_by_current.get(&op.original_path) {
                errors += 1;
                self.revert_status_map
                    .insert(current_str, (1u8, Some(err.clone())));
                continue;
            }
            let done = match ok_by_current.get(&op.original_path) {
                Some(d) => d,
                None => continue,
            };
            let target_path = &done.new_path;
            let target_str = target_path.to_string_lossy().to_string();

            // SSoT: only create an inverse op if the head for this file
            // is NOT already a toggle commit (cycling → don't preserve).
            // Considers commits across every directory, matching the dialog.
            if !self.is_head_toggle_commit_all(&op.original_path) {
                inverse_ops.push(RenameOp {
                    original_path: op.original_path.clone(),
                    new_path: target_path.clone(),
                    original_name: op.original_name.clone(),
                    new_name: done.new_name.clone(),
                    was_copy: op.was_copy,
                    relocated: op.relocated,
                    original_permissions: op.original_permissions.clone(),
                    // Carry the applied effects so a later redo can replay a
                    // metadata-only restore via `reapply_metadata`.
                    applied_attributes: op.applied_attributes.clone(),
                    applied_timestamps_mask: op.applied_timestamps_mask,
                });
            }
            path_updates.insert(current_str.clone(), target_path.clone());
            // Write OK to revert status; clear main table status for both old and new paths.
            self.revert_status_map
                .insert(target_str.clone(), (0u8, None));
            self.status_map.remove(&target_str);
            self.status_map.remove(&current_str);
            if target_path.is_dir() {
                dir_reverts.push((op.original_path.clone(), target_path.clone()));
            }
        }

        // 2. Push inverse commit (if any actual renames happened).
        //    Original commits are preserved so users can jump back and forth.
        //    Stale commits are only cleaned up by the branching logic
        //    (cull_stale_toggle_commits on new F2/batch).
        let n_reverted = inverse_ops.len();
        if !inverse_ops.is_empty() {
            let commit_label = {
                let sys_now = std::time::SystemTime::now();
                let dur = sys_now
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                let total_secs = dur.as_secs();
                let h = (total_secs / 3600) % 24;
                let m = (total_secs / 60) % 60;
                let s = total_secs % 60;
                format!("Revert — {:02}:{:02}:{:02}", h, m, s)
            };
            self.push_cwd_commit(Commit {
                timestamp: std::time::Instant::now(),
                label: commit_label,
                ops: inverse_ops,
            });
        }
        self.redo_stack.clear();

        // 4. Update all_files and file_names in-place (uses cached file_names to avoid 93k string allocs)
        self.ensure_file_names_synced();
        for (i, p) in self.all_files.iter_mut().enumerate() {
            if let Some(new_path) = path_updates.get(&self.file_names[i]) {
                *p = new_path.clone();
                self.file_names[i] = new_path.to_string_lossy().to_string();
            }
        }
        // Names changed in place — refresh the name-derived caches (the same
        // bookkeeping as apply/undo/redo). The display order is left frozen.
        self.post_names_changed();

        // 5. Tree cache invalidation & prefix re-keying for reverted directories
        if !dir_reverts.is_empty() {
            let mut parents = HashSet::new();
            for (old_new_path, orig_path) in &dir_reverts {
                if let Some(parent) = old_new_path.parent() {
                    parents.insert(parent.to_path_buf());
                }
                // Original parent gains the directory back — invalidate it too
                // if it's expanded (mirrors execute_plan_inner).
                if let Some(parent) = orig_path.parent()
                    && self.tree_expanded.contains(parent)
                {
                    parents.insert(parent.to_path_buf());
                }

                self.rekey_dir(old_new_path, orig_path);
            }

            for parent in &parents {
                self.tree_scanned.remove(parent);
                self.tree_has_subdirs.remove(parent);
            }

            for (old_new_path, orig_path) in &dir_reverts {
                if self.cwd == *old_new_path {
                    self.cwd = orig_path.clone();
                    self.cwd_input = orig_path.to_string_lossy().to_string();
                }
            }
        }

        self.revert_dialog = None;
        self.active_popup = None;

        self.set_status(
            &format!("Done: {} reverted, {} errors", n_reverted, errors),
            if errors > 0 {
                StatusKind::Error
            } else {
                StatusKind::Persistent
            },
        );
        // Mark status_map dirty so the main preview table rebuilds and
        // reflects the entries we cleared for successfully reverted items.
        self.status_map_dirty = true;
        self.preview_dirty = true;
        self.rebuild_rows_cache_sync();
    }
}
