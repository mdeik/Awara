use super::*;

impl GuiApp {
    pub fn go_up(&mut self) {
        if self.cwd.as_os_str().is_empty() {
            return;
        }
        if let Some(parent) = self.cwd.parent() {
            if parent.as_os_str().is_empty() {
                return;
            }
            let new_cwd = parent.to_path_buf();
            self.ensure_tree_path_expanded(&new_cwd);
            self.tree_scroll_target = Some(new_cwd.clone());
            self.cwd = new_cwd;
            self.scan_dir();
            self.save_last_dir();
        }
    }

    pub fn navigate_to_path(&mut self, raw: &str) {
        if raw.trim().is_empty() {
            return;
        }
        let expanded = if raw.starts_with("~") {
            if let Some(home) = dirs::home_dir() {
                if raw == "~" {
                    home
                } else {
                    home.join(&raw[2..])
                }
            } else {
                PathBuf::from(raw)
            }
        } else {
            PathBuf::from(raw)
        };
        let input = expanded.to_string_lossy().to_string();
        match awara::validate_input_path_as(&input, true /* expect_dir */) {
            awara::InputPathValidation::Ok(path) => {
                let dest = path;
                self.ensure_tree_path_expanded(&dest);
                self.tree_scroll_target = Some(dest.clone());
                self.cwd = dest;
                self.scan_dir();
                self.save_last_dir();
            }
            awara::InputPathValidation::WrongType => {
                self.set_status(&format!("Not a directory: {}", raw), StatusKind::Error);
            }
            awara::InputPathValidation::NotFound => {
                self.set_status(&format!("Directory not found: {}", raw), StatusKind::Error);
            }
            awara::InputPathValidation::NotAccessible(e) => {
                self.set_status(
                    &format!("Cannot access '{}': {}", raw, e),
                    StatusKind::Error,
                );
            }
            awara::InputPathValidation::PathTooLong { length, limit } => {
                self.set_status(
                    &format!("Path too long: {} chars (limit is {})", length, limit),
                    StatusKind::Error,
                );
            }
            awara::InputPathValidation::ReservedName => {
                self.set_status(
                    &format!("'{}' is a reserved system name and cannot be used", raw),
                    StatusKind::Error,
                );
            }
            awara::InputPathValidation::Empty => {
                self.set_status("Path is empty", StatusKind::Error);
            }
            awara::InputPathValidation::InvalidChars(msg) => {
                self.set_status(
                    &format!("Invalid path '{}': {}", raw, msg),
                    StatusKind::Error,
                );
            }
            awara::InputPathValidation::ComponentTooLong {
                component,
                length,
                limit,
            } => {
                self.set_status(
                    &format!(
                        "Component '{}' is {} bytes (limit is {})",
                        component, length, limit
                    ),
                    StatusKind::Error,
                );
            }
            awara::InputPathValidation::TrailingDotOrSpace => {
                self.set_status(
                    &format!("Path ends with a space or dot: {}", raw),
                    StatusKind::Error,
                );
            }
        }
    }

    pub fn enter_dir(&mut self, dir: &Path) {
        if dir.is_dir() {
            let dest = dir.to_path_buf();
            self.ensure_tree_path_expanded(&dest);
            self.tree_scroll_target = Some(dest.clone());
            self.cwd = dest;
            self.scan_dir();
            self.save_last_dir();
        }
    }

    pub fn refresh(&mut self) {
        // Preserve expanded state — only invalidate scan caches so all open
        // nodes re-read from disk on next frame. The tree collapses to just
        // the root while scans run, so re-target the viewbox at the cwd node
        // once it renders again (same as navigation does).
        self.tree_children.clear();
        self.tree_scanned.clear();
        self.tree_has_subdirs.clear();
        // Cancel any in-flight scans from before the refresh so their results
        // are discarded rather than landing on the freshly-cleared tree.
        self.tree_pending.clear();

        // Synchronously re-scan root so the tree is never blank.
        let root = tree_root();
        let root_children = scan_tree_children(&root, self.config.filters.filter_hidden);
        self.tree_children.insert(root.clone(), root_children);
        // tree_expanded is untouched — node state stays as the user left it.

        // If no directory loaded, just reset tree
        if self.cwd.as_os_str().is_empty() {
            return;
        }

        // Ensure path to cwd is expanded (user may have navigated via path bar)
        // and re-target the viewbox at the cwd node once it renders again.
        // After the cache wipe the tree renders only its root for a frame or
        // two; keep the scroll pending (see draw_tree_navigator) so the view
        // lands on the cwd node once its ancestors finish re-scanning. If the
        // cwd no longer exists, scroll to its nearest existing ancestor
        // instead — viewbox only, cwd is left untouched.
        let target = if self.cwd.exists() {
            let cwd = self.cwd.clone();
            self.ensure_tree_path_expanded(&cwd);
            cwd
        } else {
            let nearest = nearest_existing_ancestor(&self.cwd);
            self.ensure_tree_path_expanded(&nearest);
            nearest
        };
        self.tree_scroll_target = Some(target);

        self.scan_dir();
    }

    pub fn select_all(&mut self) {
        self.selection.fill(true);
        self.selection_generation += 1;
        // Selecting files re-arms the rename guard.
        self.set_processed(false);
    }
}
