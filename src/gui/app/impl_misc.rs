use super::*;

impl GuiApp {
    pub fn rekey_dir_prefix(&mut self, old_dir: &Path, new_dir: &Path) {
        if old_dir == new_dir {
            return;
        }

        // 1. Re-key commits_by_dir keys and stored RenameOps
        let mut rekeyed_commits = HashMap::new();
        for (dir, mut commits) in self.commits_by_dir.drain() {
            for commit in &mut commits {
                for op in &mut commit.ops {
                    if let Ok(rel) = op.original_path.strip_prefix(old_dir)
                        && rel.components().next().is_some()
                    {
                        op.original_path = new_dir.join(rel);
                    }
                    if let Ok(rel) = op.new_path.strip_prefix(old_dir)
                        && rel.components().next().is_some()
                    {
                        op.new_path = new_dir.join(rel);
                    }
                }
            }
            let new_key = if let Ok(rel) = dir.strip_prefix(old_dir) {
                new_dir.join(rel)
            } else {
                dir
            };
            rekeyed_commits.insert(new_key, commits);
        }
        self.commits_by_dir = rekeyed_commits;
        for p in &mut self.commit_dir_lru {
            if let Ok(rel) = p.strip_prefix(old_dir) {
                *p = new_dir.join(rel);
            }
        }

        // 2. Re-key redo_stack ops
        for commit in &mut self.redo_stack {
            for op in &mut commit.ops {
                if let Ok(rel) = op.original_path.strip_prefix(old_dir)
                    && rel.components().next().is_some()
                {
                    op.original_path = new_dir.join(rel);
                }
                if let Ok(rel) = op.new_path.strip_prefix(old_dir)
                    && rel.components().next().is_some()
                {
                    op.new_path = new_dir.join(rel);
                }
            }
        }

        // 3. Re-key all_files and file_names
        self.ensure_file_names_synced();
        for (i, p) in self.all_files.iter_mut().enumerate() {
            if let Ok(rel) = p.strip_prefix(old_dir) {
                let updated = new_dir.join(rel);
                *p = updated.clone();
                self.file_names[i] = updated.to_string_lossy().to_string();
            }
        }

        // 4. Re-key status_map
        let mut rekeyed_status = HashMap::new();
        for (path_str, status_val) in self.status_map.drain() {
            let path = PathBuf::from(&path_str);
            if let Ok(rel) = path.strip_prefix(old_dir) {
                rekeyed_status.insert(new_dir.join(rel).to_string_lossy().to_string(), status_val);
            } else {
                rekeyed_status.insert(path_str, status_val);
            }
        }
        self.status_map = rekeyed_status;

        // 5. Re-key revert_status_map
        let mut rekeyed_revert_status = HashMap::new();
        for (path_str, status_val) in self.revert_status_map.drain() {
            let path = PathBuf::from(&path_str);
            if let Ok(rel) = path.strip_prefix(old_dir) {
                rekeyed_revert_status
                    .insert(new_dir.join(rel).to_string_lossy().to_string(), status_val);
            } else {
                rekeyed_revert_status.insert(path_str, status_val);
            }
        }
        self.revert_status_map = rekeyed_revert_status;

        // 6. Re-key active revert_dialog entries if open
        if let Some(ref mut dialog) = self.revert_dialog {
            for entry in &mut dialog.entries {
                if let Ok(rel) = entry.current_path.strip_prefix(old_dir) {
                    entry.current_path = new_dir.join(rel);
                }
                for (p, _, _) in &mut entry.lineage {
                    if let Ok(rel) = p.strip_prefix(old_dir) {
                        *p = new_dir.join(rel);
                    }
                }
            }
        }
    }

    /// Single source of truth for re-keying all path-prefixed state after a
    /// directory rename (or revert). Calls `rekey_dir_prefix` (file list,
    /// rename history, status maps, revert table) and `rekey_tree_dir` (file
    /// tree caches) together, so no consumer can drift out of sync. Use this
    /// — not the two halves directly — at every site that renames a directory.
    pub fn rekey_dir(&mut self, old: &Path, new: &Path) {
        if old == new {
            return;
        }
        self.rekey_dir_prefix(old, new);
        self.rekey_tree_dir(old, new);
    }

    /// Set a deferred context-menu action with a snapshot of the selected indices.
    /// Capturing the indices at menu-creation time prevents row-click race conditions
    /// from corrupting the selection before `apply_context_action` processes the action.
    pub fn set_pending_action(&mut self, action: ContextAction, sel_indices: Vec<usize>) {
        self.pending_context_action = Some(action);
        self.pending_sel_indices = sel_indices;
        self.context_menu_action_taken = true;
    }

    /// Execute a deferred context-menu action.
    pub fn apply_context_action(&mut self) {
        let action = match self.pending_context_action.take() {
            Some(a) => a,
            None => return,
        };

        // Use the snapshot captured at context-menu time if available,
        // otherwise fall back to the current selection (for tests that set
        // pending_context_action directly without pending_sel_indices).
        let sel_indices: Vec<usize> = if !self.pending_sel_indices.is_empty() {
            std::mem::take(&mut self.pending_sel_indices)
        } else {
            self.selection
                .iter()
                .enumerate()
                .filter(|&(_, &s)| s)
                .map(|(i, _)| i)
                .collect()
        };

        match action {
            ContextAction::MoveUp => self.move_selected_up(),
            ContextAction::MoveDown => self.move_selected_down(),
            ContextAction::MoveToTop => self.move_selected_to_top(),
            ContextAction::MoveToBottom => self.move_selected_to_bottom(),
            ContextAction::ResetOrder => self.reset_custom_order(),
            ContextAction::Trash => {
                if self.skip_trash_confirmation {
                    match self.trash_selected_multi() {
                        Ok(()) => {}
                        Err(e) => self.set_status(&e, StatusKind::Error),
                    }
                } else {
                    self.show_trash_confirmation = true;
                    self.trash_init_focus = true;
                }
            }
            ContextAction::OpenFiles => {
                for &idx in &sel_indices {
                    self.open_file_external(idx);
                }
            }
            ContextAction::OpenFolders => {
                let paths: Vec<&Path> = sel_indices
                    .iter()
                    .filter_map(|&idx| self.all_files.get(idx).map(|p| p.as_path()))
                    .collect();
                open_parent_and_select(&paths);
            }
            ContextAction::Properties => {
                if sel_indices.len() == 1 {
                    self.show_properties_idx = Some(sel_indices[0]);
                } else {
                    self.show_properties_multi(&sel_indices);
                }
            }
            ContextAction::Rename(idx) => {
                self.start_edit(idx);
            }
            ContextAction::UndoFiles => {
                self.undo_files(&sel_indices);
            }
            ContextAction::RedoFiles => {
                self.redo_files(&sel_indices);
            }
        }
        self.context_menu_action_taken = false;
    }

    pub fn selected_count(&self) -> usize {
        if let Some(c) = self.cached_selected_count.get()
            && self.cached_selected_gen.get() == self.selection_generation
            && self.cached_selected_len.get() == self.selection.len()
        {
            return c;
        }
        let count = self.selection.iter().filter(|&&s| s).count();
        self.cached_selected_count.set(Some(count));
        self.cached_selected_gen.set(self.selection_generation);
        self.cached_selected_len.set(self.selection.len());
        count
    }
    /// Index of the first selected item, or None.
    pub fn selected_idx(&self) -> Option<usize> {
        self.selection.iter().position(|&s| s)
    }
    /// Indices of all currently selected items.
    pub fn selected_indices(&self) -> Vec<usize> {
        self.selection
            .iter()
            .enumerate()
            .filter(|&(_, &s)| s)
            .map(|(i, _)| i)
            .collect()
    }
    pub fn modified_count(&self) -> usize {
        if let Some(c) = self.cached_modified_count.get()
            && self.cached_modified_gen.get() == self.rows_cache_gen
            && self.cached_modified_len.get() == self.cached_rows.len()
        {
            return c;
        }
        let count = self
            .cached_rows
            .iter()
            .filter(|r| r.status == Some(0))
            .count();
        self.cached_modified_count.set(Some(count));
        self.cached_modified_gen.set(self.rows_cache_gen);
        self.cached_modified_len.set(self.cached_rows.len());
        count
    }

    /// True when any modal dialog is open and background interaction should be blocked.
    pub fn has_any_modal(&self) -> bool {
        self.active_popup.is_some()
            || self.collision_dialog.is_some()
            || self.show_properties_idx.is_some()
            || self.show_properties_multi_data.is_some()
            || self.show_tree_properties.is_some()
            || self.show_trash_confirmation
            || self.show_dir_creation_warning
            || self.show_invalid_name_warning
    }
}
