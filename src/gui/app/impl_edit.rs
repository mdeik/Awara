use super::*;

impl GuiApp {
    pub fn copy_filename_multi(&self, indices: &[usize], ctx: &egui::Context) {
        let text: Vec<String> = indices
            .iter()
            .filter_map(|&idx| {
                self.all_files
                    .get(idx)
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            })
            .collect();
        ctx.copy_text(text.join("\n"));
    }

    pub fn copy_full_path_multi(&self, indices: &[usize], ctx: &egui::Context) {
        let text: Vec<String> = indices
            .iter()
            .filter_map(|&idx| {
                self.all_files
                    .get(idx)
                    .map(|p| p.to_string_lossy().to_string())
            })
            .collect();
        ctx.copy_text(text.join("\n"));
    }

    pub fn copy_dir_path_multi(&self, indices: &[usize], ctx: &egui::Context) {
        let text: Vec<String> = indices
            .iter()
            .filter_map(|&idx| {
                self.all_files.get(idx).and_then(|p| {
                    p.parent()
                        .map(|parent| parent.to_string_lossy().to_string())
                })
            })
            .collect();
        ctx.copy_text(text.join("\n"));
    }

    pub fn copy_new_name_multi(&self, indices: &[usize], ctx: &egui::Context) {
        let text: Vec<String> = indices
            .iter()
            .filter_map(|&idx| {
                self.preview
                    .iter()
                    .find(|pr| pr.index == idx)
                    .map(|pr| pr.new_name().to_string())
            })
            .collect();
        ctx.copy_text(text.join("\n"));
    }

    pub fn show_properties_multi(&mut self, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }
        let count = indices.len();
        let total_size: u64 = indices
            .iter()
            .filter_map(|&idx| self.file_sizes.get(idx))
            .sum();
        let names: Vec<String> = indices
            .iter()
            .filter_map(|&idx| {
                self.all_files
                    .get(idx)
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            })
            .collect();
        self.show_properties_multi_data = Some(crate::gui::types::PropertiesMultiData {
            count,
            total_size,
            names,
        });
    }

    // ── External open / trash ──

    pub fn open_file_external(&self, idx: usize) {
        let path = match self.all_files.get(idx) {
            Some(p) => p,
            None => return,
        };
        open_in_os(path);
    }

    // ── F2 inline edit ──

    /// Commit the current F2 edit (rename file on disk) and clear editing state.
    /// On success the operation is pushed to undo_stack so revert can undo it.
    pub fn commit_edit(&mut self) {
        self.ensure_file_names_synced();
        let idx = match self.editing_idx {
            Some(i) => i,
            None => return,
        };
        let new_name = self.edit_buffer.trim().to_string();
        if new_name.is_empty() {
            self.editing_idx = None;
            self.edit_buffer.clear();
            return;
        }
        let old_path = match self.all_files.get(idx).cloned() {
            Some(p) => p,
            None => {
                self.editing_idx = None;
                self.edit_buffer.clear();
                return;
            }
        };
        let parent = match old_path.parent() {
            Some(p) => p,
            None => {
                self.editing_idx = None;
                self.edit_buffer.clear();
                return;
            }
        };

        // Build a single-op plan for the inline rename. Collision detection and
        // execution share it (SSoT with batch Apply).
        let source_name = old_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // A name that navigates the path (rooted, `.`/`..`) can't be executed
        // (SSoT: `path_navigation_error`). Checked **before** the no-op shortcut
        // below so a path-equal navigational name (`./a.txt`) is explained rather
        // than silently dropped — mirroring the batch Apply gate.
        if let Some(nav) = awara::path_navigation_error(&new_name) {
            self.invalid_name_entries =
                vec![(source_name, format!("{} — {}", new_name, nav.message()))];
            self.invalid_name_init_focus = true;
            self.show_invalid_name_warning = true;
            self.editing_idx = None;
            self.edit_buffer.clear();
            return;
        }

        let new_path = parent.join(&new_name);
        if new_path == old_path {
            self.editing_idx = None;
            self.edit_buffer.clear();
            return;
        }

        let op = awara::RenameOp {
            original_path: old_path.clone(),
            new_path: new_path.clone(),
            original_name: source_name.clone(),
            new_name: new_name.clone(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        };
        let plan = awara::RenamePlan::from_ops(vec![op]);
        let collisions = plan.collisions();

        self.editing_idx = None;
        self.edit_buffer.clear();

        // Folder-creation gate (SSoT with batch Apply): warn before an inline
        // rename creates subfolders, unless the user opted out. Any collision is
        // stashed so the F2 dialog (and its inline semantics) resumes on confirm.
        if plan.ops().iter().any(|op| op.name_implies_subpath()) && !self.skip_dir_creation_warning
        {
            self.pending_f2_collision = if collisions.is_empty() {
                None
            } else {
                Some(CollisionDialogState::from_f2(
                    idx,
                    new_name.clone(),
                    source_name.clone(),
                    old_path.to_string_lossy().to_string(),
                ))
            };
            self.pending_dir_count = plan.ops().len();
            self.pending_dir_label = "Inline".into();
            self.pending_dir_inline = true;
            self.pending_dir_plan = Some(plan);
            self.dir_warning_init_focus = true;
            self.show_dir_creation_warning = true;
            return;
        }

        if collisions.is_empty() {
            // Fast path — single rename, no collision dialog. Inline mode
            // bypasses output-dir/copy/attribute/timestamp config.
            let resolution = awara::Resolution::default_for(CollisionStrategy::Skip);
            self.execute_plan_inner(plan, &resolution, "Inline", true);
        } else {
            // Collision detected — show dialog. The pending plan is executed
            // by the dialog once the user resolves the conflict.
            self.pending_plan = Some(plan);
            self.collision_dialog = Some(CollisionDialogState::from_f2(
                idx,
                new_name,
                source_name,
                old_path.to_string_lossy().to_string(),
            ));
        }
    }

    /// Cancel the current F2 edit without renaming: used by Esc, and by any
    /// rescan that replaces `all_files` (a stale `editing_idx` would otherwise
    /// silently rebind to whatever file now sits at that index).
    pub fn cancel_edit(&mut self) {
        self.editing_idx = None;
        self.edit_buffer.clear();
        self.edit_pending_focus = false;
        self.editing_row_rect = None;
    }

    /// Start inline editing for the given file index.
    pub fn start_edit(&mut self, idx: usize) {
        if let Some(path) = self.all_files.get(idx)
            && let Some(name) = path.file_name()
        {
            self.editing_idx = Some(idx);
            self.edit_buffer = name.to_string_lossy().to_string();
            self.edit_pending_focus = true;
        }
    }

    /// Cycle the F2 edit cursor forward or backward in display order.
    /// Commits the current edit, wraps at boundaries, and updates selection.
    pub fn cycle_edit(&mut self, forward: bool) {
        let current_idx = match self.editing_idx {
            Some(i) => i,
            None => return,
        };
        let order = self.get_display_order();
        self.commit_edit();
        let n = order.len();
        if n == 0 {
            return;
        }
        if let Some(pos) = order.iter().position(|&i| i == current_idx) {
            let next_pos = if forward {
                if pos + 1 < n { pos + 1 } else { 0 }
            } else {
                if pos > 0 { pos - 1 } else { n - 1 }
            };
            let new_sel = order[next_pos];
            self.start_edit(new_sel);
            // Move selection to the newly edited item.
            self.selection.fill(false);
            if let Some(s) = self.selection.get_mut(new_sel) {
                *s = true;
            }
            self.last_clicked_idx = Some(new_sel);
            self.selection_anchor = None; // fresh single select → new anchor
            // The selection moved, so notify the same way every other selection
            // mutation does: the deferred selection-change handling reads this
            // counter to decide whether to recompute (numbering follows
            // selection) or just rebuild the affected rows.
            self.selection_generation += 1;
        }
    }

    /// Move the first visually-selected file to the system trash.
    /// Returns an error message on failure.
    #[cfg_attr(not(test), expect(dead_code))]
    pub fn trash_selected(&mut self) -> Result<(), String> {
        self.ensure_file_names_synced();
        let sel_file_idx = match self.selected_idx() {
            Some(i) => i,
            None => return Err("No file selected".into()),
        };
        let path = match self.all_files.get(sel_file_idx) {
            Some(p) => p.clone(),
            None => return Err("Invalid index".into()),
        };

        let was_dir = path.is_dir();
        trash_file(&path)?;

        // Collect all indices to remove: the trashed path itself, plus any
        // entries under it (if it was a directory in recursive mode).
        let to_remove: Vec<usize> = if was_dir {
            self.all_files
                .iter()
                .enumerate()
                .filter(|(_, p)| *p == &path || p.starts_with(&path))
                .map(|(i, _)| i)
                .collect()
        } else {
            vec![sel_file_idx]
        };

        // Remove the trashed entries: every index-keyed vector shrinks together and
        // the display order is pruned, so the table keeps the user's order.
        let trashed_paths: HashSet<String> =
            self.remove_file_indices(&to_remove).into_iter().collect();
        self.reindex_preview_after_removal(&to_remove);

        // Remove commit ops referencing trashed paths.
        self.remove_commit_ops_by_paths(&trashed_paths);

        // ── Surgical tree cache cleanup ──
        if was_dir {
            // Prune the row from the tree synchronously — before any viewbox
            // scroll this frame — so a removed directory can't shift the rows
            // under the scroll target when the async re-scan lands later.
            self.prune_tree_dir(&path);
        } else if let Some(parent) = path.parent() {
            // A trashed file never appears in the tree; just invalidate the
            // parent so its next render re-reads its children.
            self.invalidate_tree_scan(parent);
            self.tree_has_subdirs.remove(parent);
        }

        self.preview_dirty = true;
        // After trash, file indices have shifted — rebuild synchronously so
        // the UI is immediately consistent on the next frame.
        self.rebuild_rows_cache_sync();
        Ok(())
    }

    /// Move an arbitrary path (file or directory) to the system trash, cleaning
    /// up every index-keyed vector and the tree caches.
    ///
    /// Used by the tree navigator's context menu, where the target directory
    /// need not be part of the current scan, so it cannot go through
    /// [`Self::trash_selected`] / [`Self::trash_selected_multi`].
    pub fn trash_path(&mut self, path: &Path) -> Result<(), String> {
        // Defense in depth: a filesystem root has no parent, and trashing a
        // whole volume is never intended (the tree menu hides the action too).
        if path.parent().is_none() {
            return Err(format!(
                "Refusing to trash filesystem root: {}",
                path.display()
            ));
        }
        self.ensure_file_names_synced();
        let was_dir = path.is_dir();
        trash_file(path)?;

        // Collect everything the listing must drop: the trashed entry itself
        // plus, for a directory, any scanned descendants.
        let to_remove: Vec<usize> = self
            .all_files
            .iter()
            .enumerate()
            .filter(|(_, p)| *p == path || (was_dir && p.starts_with(path)))
            .map(|(i, _)| i)
            .collect();

        let trashed_paths: HashSet<String> =
            self.remove_file_indices(&to_remove).into_iter().collect();
        self.reindex_preview_after_removal(&to_remove);
        self.remove_commit_ops_by_paths(&trashed_paths);

        // ── Surgical tree cache cleanup ──
        // Prune the row from the tree synchronously so it is gone before any
        // viewbox scroll this frame, rather than after the async re-scan.
        self.prune_tree_dir(path);

        self.preview_dirty = true;

        // If the current directory lived inside the trashed subtree it no
        // longer exists on disk — fall back to the trashed folder's parent.
        if self.cwd.starts_with(path)
            && let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            self.navigate_tree_to(parent);
            return Ok(());
        }

        // After trash, file indices have shifted — rebuild synchronously so the
        // UI is immediately consistent on the next frame.
        self.rebuild_rows_cache_sync();
        Ok(())
    }

    /// Trash all selected files. Collects indices before mutating to avoid
    /// borrow conflicts, and processes in reverse so earlier indices stay valid.
    pub fn trash_selected_multi(&mut self) -> Result<(), String> {
        self.ensure_file_names_synced();

        // Collect and sort selected indices in descending order.
        let sel_indices: Vec<usize> = self
            .selection
            .iter()
            .enumerate()
            .filter(|&(_, &s)| s)
            .map(|(i, _)| i)
            .rev()
            .collect();

        if sel_indices.is_empty() {
            return Err("No file selected".into());
        }

        // Capture which selections are directories *before* trashing: moving a
        // directory to the trash makes `is_dir()` false afterwards, which would
        // otherwise skip both the tree cleanup and the removal of the
        // directory's scanned descendants below.
        let selected_is_dir: Vec<bool> = sel_indices
            .iter()
            .map(|&idx| self.all_files.get(idx).is_some_and(|p| p.is_dir()))
            .collect();

        // Trash each file, pruning its tree row as we go.
        for (&idx, &was_dir) in sel_indices.iter().zip(&selected_is_dir) {
            let path = match self.all_files.get(idx) {
                Some(p) => p.clone(),
                None => return Err("Invalid index".into()),
            };
            // If the file was already deleted outside the GUI, skip the trash
            // call — trying to trash a non-existent path would return an error.
            // The removal pass below will still clean it from the list.
            if path.exists() {
                trash_file(&path)?;
            }

            // ── Surgical tree cache cleanup ──
            if was_dir {
                // Prune the row from the tree synchronously, before any viewbox
                // scroll this frame, rather than after the async re-scan.
                self.prune_tree_dir(&path);
            } else if let Some(parent) = path.parent() {
                self.invalidate_tree_scan(parent);
                self.tree_has_subdirs.remove(parent);
            }
        }

        // Collect all indices to remove (file + descendants for directories).
        let mut to_remove: HashSet<usize> = sel_indices.iter().copied().collect();
        for (&idx, &was_dir) in sel_indices.iter().zip(&selected_is_dir) {
            if was_dir {
                let path = &self.all_files[idx];
                for (i, p) in self.all_files.iter().enumerate() {
                    if p.starts_with(path) && i != idx {
                        to_remove.insert(i);
                    }
                }
            }
        }

        // Remove the trashed entries (deduplicated, sorted by the helper) and prune
        // the display order rather than letting it rebuild into a re-sort.
        let removed: Vec<usize> = to_remove.into_iter().collect();
        let trashed_paths: HashSet<String> =
            self.remove_file_indices(&removed).into_iter().collect();
        self.reindex_preview_after_removal(&removed);

        // Remove commit ops referencing trashed paths.
        self.remove_commit_ops_by_paths(&trashed_paths);

        self.preview_dirty = true;
        self.rebuild_rows_cache_sync();
        Ok(())
    }
}
