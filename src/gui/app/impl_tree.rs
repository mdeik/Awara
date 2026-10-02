use super::*;

impl GuiApp {
    pub fn ensure_tree_scanned(&mut self, dir: &Path) {
        if !self.tree_scanned.contains(dir) && !self.tree_pending.contains_key(dir) {
            self.tree_scan_gen_seq += 1;
            let generation = self.tree_scan_gen_seq;
            self.tree_pending.insert(dir.to_path_buf(), generation);
            let _ = self.tree_scan_tx.send((
                dir.to_path_buf(),
                self.config.filters.filter_hidden,
                generation,
            ));
        }
    }

    /// Invalidate a directory's cached scan so its next render re-reads from
    /// disk. Dropping any in-flight request also makes its result — read from
    /// disk *before* this invalidation (a trash, a rename, a refresh) — fail the
    /// pending/generation check in `check_scan_results`, so it can't resurrect
    /// stale children.
    pub(crate) fn invalidate_tree_scan(&mut self, dir: &Path) {
        self.tree_scanned.remove(dir);
        self.tree_pending.remove(dir);
    }
    /// Synchronously probe whether `dir` contains any visible subdirectories
    /// and cache the answer for the expand arrow. Called at most once per
    /// rendered node (only when the cache misses), so the arrow is correct on
    /// the very first frame the node appears — no optimistic flash — and the
    /// probe cost is one cheap readdir per node, not per frame.
    pub fn probe_tree_arrow(&mut self, dir: &Path) -> bool {
        let has = dir_has_subdirs(dir, self.config.filters.filter_hidden);
        self.tree_has_subdirs.insert(dir.to_path_buf(), has);
        has
    }
    pub fn toggle_tree_expand(&mut self, dir: &Path) {
        if self.tree_expanded.contains(dir) {
            self.collapse_tree_dir(dir);
        } else {
            self.ensure_tree_scanned(dir);
            self.tree_expanded.insert(dir.to_path_buf());
        }
    }
    /// Collapse an expanded tree node. Data is no longer displayed and may
    /// be stale — evict it so re-expanding re-reads from the filesystem. The
    /// freshly-known child list still answers the expand-arrow question, so
    /// it is recorded as the probe answer instead of re-probing after
    /// collapse.
    pub(super) fn collapse_tree_dir(&mut self, dir: &Path) {
        self.tree_expanded.remove(dir);
        if let Some(children) = self.tree_children.remove(dir) {
            self.tree_has_subdirs
                .insert(dir.to_path_buf(), !children.is_empty());
        }
        self.invalidate_tree_scan(dir);
    }
    /// Expand all ancestor directories of `dir` in the file tree so that
    /// `dir` is visible (all its ancestors are expanded in the tree widget).
    pub(super) fn ensure_tree_path_expanded(&mut self, dir: &Path) {
        let mut to_expand = Vec::new();
        let mut p = Some(dir);
        while let Some(d) = p {
            let parent = d.parent();
            if let Some(pp) = parent {
                if pp.as_os_str().is_empty() {
                    break;
                }
                if !self.tree_expanded.contains(pp) {
                    to_expand.push(pp.to_path_buf());
                }
                p = Some(pp);
            } else {
                break;
            }
        }
        for d in to_expand.into_iter().rev() {
            self.ensure_tree_scanned(&d);
            self.tree_expanded.insert(d);
        }
    }

    pub fn navigate_tree_to(&mut self, dir: &Path) {
        self.ensure_tree_path_expanded(dir);
        self.cwd = dir.to_path_buf();
        self.scan_dir();
    }

    /// Recursively invalidate tree cache for `dir` and all expanded
    /// descendants. On the next frame, `draw_tree_node` will re-request
    /// scans for each re-rendered node, picking up new subdirectories
    /// from the filesystem.
    pub fn invalidate_tree_subtree(&mut self, dir: &Path) {
        self.invalidate_tree_scan(dir);
        // Drop expand-arrow answers for the whole subtree, not just `dir`
        // itself: a collapsed child whose subdirectories were removed
        // externally would otherwise keep its stale arrow until manually
        // expanded. Rendered nodes re-probe synchronously (one cheap
        // readdir each), so this is flash-free.
        self.tree_has_subdirs.retain(|p, _| !p.starts_with(dir));
        if let Some(children) = self.tree_children.remove(dir) {
            for child in &children {
                if self.tree_expanded.contains(child) {
                    self.invalidate_tree_subtree(child);
                }
            }
        }
    }

    /// Evict a directory that left the filesystem (a trash) from the tree
    /// caches: its own children/scanned/expanded/arrow entries and those of its
    /// descendants, and its row from the parent's cached child list.
    ///
    /// The parent's child list is pruned **synchronously** rather than left for
    /// the parent's next async re-scan to fix. That re-scan lands one or more
    /// frames later, so a viewbox scroll performed in the meantime is computed
    /// against a layout that still contains the removed rows; when they finally
    /// disappear the scroll offset no longer points at its target, drifting one
    /// row per removed directory. Dropping the row here keeps the rendered
    /// layout correct for any scroll this frame. The parent is still invalidated
    /// so the re-scan picks up unrelated external changes and refreshes its
    /// expand arrow.
    pub(super) fn prune_tree_dir(&mut self, dir: &Path) {
        // Cancel in-flight scans for the directory and its whole subtree:
        // dropping their pending entries makes their results — read from disk
        // before the removal — fail the check in `check_scan_results`, so they
        // can't resurrect pruned children.
        self.tree_pending
            .retain(|p, _| !(p == dir || p.starts_with(dir)));

        // Drop the removed directory's own caches and those of its subtree.
        self.tree_children.remove(dir);
        self.tree_scanned.remove(dir);
        self.tree_expanded.remove(dir);
        self.tree_has_subdirs.remove(dir);
        self.tree_children.retain(|k, _| !k.starts_with(dir));
        self.tree_scanned.retain(|p| !p.starts_with(dir));
        self.tree_expanded.retain(|p| !p.starts_with(dir));
        self.tree_has_subdirs.retain(|k, _| !k.starts_with(dir));

        // Drop the row from the parent's displayed child list now, then
        // invalidate the parent so its next render re-reads from disk.
        if let Some(parent) = dir.parent()
            && parent != dir
        {
            if let Some(children) = self.tree_children.get_mut(parent) {
                children.retain(|c| c != dir);
            }
            // The parent's own in-flight scan (if any) may have read the
            // removed dir too; invalidating bumps its generation so it is
            // discarded as well.
            self.invalidate_tree_scan(parent);
            self.tree_has_subdirs.remove(parent);
        }
    }

    /// Re-key every tree cache entry under `old` to `new` after a directory
    /// rename (or revert). The renamed directory keeps its subdirectory list,
    /// scanned flags, expanded state, and arrow-probe answers — only the path
    /// prefix changes — so descendants (`old/…`) are re-keyed too, keeping an
    /// expanded subtree consistent instead of going stale.
    pub fn rekey_tree_dir(&mut self, old: &Path, new: &Path) {
        if old == new {
            return;
        }
        // An in-flight scan for `old` (or its subtree) read the old path's
        // children. Drop its pending entry so `check_scan_results` discards the
        // result instead of re-inserting children under the now-stale path.
        self.tree_pending
            .retain(|p, _| p != old && !p.starts_with(old));

        // Children lists: re-key both the entry for `old` itself and any
        // cached descendants (e.g. an expanded grandchild that was scanned).
        let keys: Vec<PathBuf> = self.tree_children.keys().cloned().collect();
        for k in keys {
            if k != old && !k.starts_with(old) {
                continue;
            }
            let children = self.tree_children.remove(&k).unwrap();
            let rekeyed: Vec<PathBuf> = children
                .into_iter()
                .map(|c| {
                    if c.starts_with(old) {
                        new.join(c.strip_prefix(old).unwrap())
                    } else {
                        c
                    }
                })
                .collect();
            let new_key = if k == old {
                new.to_path_buf()
            } else {
                new.join(k.strip_prefix(old).unwrap())
            };
            self.tree_children.insert(new_key, rekeyed);
        }
        // Scanned flags (own key + descendants)
        let to_rekey: Vec<PathBuf> = self
            .tree_scanned
            .iter()
            .filter(|p| *p == old || p.starts_with(old))
            .map(|p| {
                if p == old {
                    new.to_path_buf()
                } else {
                    new.join(p.strip_prefix(old).unwrap())
                }
            })
            .collect();
        self.tree_scanned
            .retain(|p| p != old && !p.starts_with(old));
        for p in to_rekey {
            self.tree_scanned.insert(p);
        }
        // Expanded state (own key + descendants)
        let to_rekey: Vec<PathBuf> = self
            .tree_expanded
            .iter()
            .filter(|p| *p == old || p.starts_with(old))
            .map(|p| {
                if p == old {
                    new.to_path_buf()
                } else {
                    new.join(p.strip_prefix(old).unwrap())
                }
            })
            .collect();
        self.tree_expanded
            .retain(|p| p != old && !p.starts_with(old));
        for p in to_rekey {
            self.tree_expanded.insert(p);
        }
        // Arrow-probe answers survive a rename unchanged (the folder still
        // holds the same subdirectories), so re-key them old → new too.
        let to_rekey_probe: Vec<(PathBuf, bool)> = self
            .tree_has_subdirs
            .iter()
            .filter(|(p, _)| *p == old || p.starts_with(old))
            .map(|(p, has)| {
                if p == old {
                    (new.to_path_buf(), *has)
                } else {
                    (new.join(p.strip_prefix(old).unwrap()), *has)
                }
            })
            .collect();
        self.tree_has_subdirs
            .retain(|p, _| p != old && !p.starts_with(old));
        for (p, has) in to_rekey_probe {
            self.tree_has_subdirs.insert(p, has);
        }
    }

    /// Test helper: register an in-flight tree scan for `dir` and return the
    /// generation a simulated result must echo to be accepted by
    /// `check_scan_results`.
    #[cfg(test)]
    pub(super) fn begin_tree_scan_for_test(&mut self, dir: &Path) -> u64 {
        self.tree_scan_gen_seq += 1;
        let generation = self.tree_scan_gen_seq;
        self.tree_pending.insert(dir.to_path_buf(), generation);
        generation
    }

    /// Test helper: the generation of the in-flight scan for `dir`.
    #[cfg(test)]
    pub(super) fn pending_tree_scan_generation_for_test(&self, dir: &Path) -> u64 {
        self.tree_pending[dir]
    }
}
