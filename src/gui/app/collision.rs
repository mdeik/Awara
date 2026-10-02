use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LiveCollisionReason {
    /// Target path already exists on disk and is a real file the user cares about.
    OnDisk,
    /// Another op in the same batch targets this path — the file at the target
    /// was (or will be) written by our own batch, not a pre-existing user file.
    InBatch,
}

pub(crate) struct CollisionEntry {
    /// Full path of the source file (stable across scans; survives index shifts).
    pub(crate) source_path: String,
    pub(crate) source_display: String, // display name (original)
    pub(crate) target_display: String, // display name (new target)
    /// Full target path from the plan op (`op.new_path`) — the single source of
    /// truth for conflict re-checks, instead of rebuilding it from the parent +
    /// display name.
    pub(crate) target_path: std::path::PathBuf,
    /// True when the colliding target is an existing directory (from the core
    /// `Collision`). Folders cannot be overwritten, so `Replace` is refused for
    /// such entries.
    pub(crate) target_is_dir: bool,
    /// True when the *source* is an existing directory (from the core
    /// `Collision`). A folder source cannot replace an existing target either,
    /// so `Replace` is refused whenever a folder is involved on either side.
    pub(crate) source_is_dir: bool,
    pub(crate) reason: CollisionReason, // from core (static, pre-scan)
    pub(crate) decided_replace: bool,
    pub(crate) decided_skip: bool,
    /// For InBatch only: source_path of the earlier op that claimed this target.
    /// Used to check whether that op's collision was skipped.
    pub(crate) conflict_source_path: Option<String>,
    /// Live collision reason, re-computed whenever decisions change.
    /// This is the single source of truth — both the dialog message and
    /// the overwrite warning read from this field. Never derive it ad-hoc.
    pub(crate) live_reason: LiveCollisionReason,
}

/// Persistent state for the collision resolution dialog.
pub(crate) struct CollisionDialogState {
    pub(crate) entries: Vec<CollisionEntry>,
    /// Which entry the dialog is currently showing.
    pub(crate) current: usize,
    /// True if user clicked "Skip All" — skips remaining entries, no I/O.
    pub(crate) all_skip: bool,
    /// True when this is an F2 rename (single entry, simpler UI).
    pub(crate) is_f2: bool,
    /// One-shot flag: request focus on the primary action button on first frame.
    pub(crate) init_focus: bool,
}

impl CollisionDialogState {
    /// Re-compute `live_reason` for every entry based on current decisions.
    /// Must be called after any `decided_skip` / `decided_replace` change and
    /// before rendering, so the dialog always shows up-to-date information.
    ///
    /// The rules:
    /// - OnDisk stays OnDisk — the file was there before any of our ops ran.
    /// - InBatch becomes OnDisk only when the conflicting earlier entry was
    ///   skipped AND the target file still exists on disk. Otherwise it stays
    ///   InBatch (prior undecided, prior Replaced, or prior skipped but file gone).
    pub(crate) fn recompute_live_reasons(&mut self) {
        for i in 0..self.entries.len() {
            let entry = &self.entries[i];
            self.entries[i].live_reason = match entry.reason {
                CollisionReason::OnDisk => LiveCollisionReason::OnDisk,
                CollisionReason::InBatch(_) => {
                    if let Some(csp) = &entry.conflict_source_path {
                        // Check if the earlier conflicting entry was skipped. If so, and
                        // the target still exists on disk, this is now a real conflict.
                        // `target_path` comes from the plan op (SSoT), never re-derived.
                        let prior_skipped = self
                            .entries
                            .iter()
                            .any(|e| e.source_path == *csp && e.decided_skip);
                        if prior_skipped && entry.target_path.exists() {
                            LiveCollisionReason::OnDisk
                        } else {
                            LiveCollisionReason::InBatch
                        }
                    } else {
                        // No conflict source path — purely an in-batch collision.
                        LiveCollisionReason::InBatch
                    }
                }
            };
        }
    }

    /// Build the dialog from a plan and its detected collisions. Entries are in
    /// plan order, which is display order for the selected batch.
    pub(crate) fn from_plan(plan: &awara::RenamePlan, collisions: &[Collision]) -> Self {
        let ops = plan.ops();
        let entries = collisions
            .iter()
            .filter_map(|c| {
                let op = ops.get(c.op_index)?;
                let old_name = op
                    .original_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let conflict_source_path = match &c.reason {
                    CollisionReason::InBatch(prev) => ops
                        .get(*prev)
                        .map(|p| p.original_path.to_string_lossy().to_string()),
                    _ => None,
                };
                Some(CollisionEntry {
                    source_path: op.original_path.to_string_lossy().to_string(),
                    source_display: old_name,
                    target_display: op.new_name.clone(),
                    target_path: op.new_path.clone(),
                    target_is_dir: c.target_is_dir,
                    source_is_dir: c.source_is_dir,
                    reason: c.reason.clone(),
                    decided_replace: false,
                    decided_skip: false,
                    conflict_source_path,
                    live_reason: LiveCollisionReason::InBatch, // overwritten after build
                })
            })
            .collect();

        let mut state = CollisionDialogState {
            entries,
            current: 0,
            all_skip: false,
            is_f2: false,
            init_focus: true,
        };
        state.recompute_live_reasons();
        state
    }

    pub(crate) fn from_f2(
        _file_idx: usize,
        new_name: String,
        source_name: String,
        source_path: String,
    ) -> Self {
        // F2 has no core `Collision`, so derive the target path here and reuse
        // the shared no-follow directory check (same rule as the executor).
        let target_path = std::path::Path::new(&source_path)
            .parent()
            .map(|p| p.join(&new_name))
            .unwrap_or_else(|| std::path::PathBuf::from(&new_name));
        let target_is_dir = awara::is_existing_dir_target(&target_path);
        let source_is_dir = awara::is_existing_dir_target(std::path::Path::new(&source_path));
        let mut state = CollisionDialogState {
            entries: vec![CollisionEntry {
                source_path,
                source_display: source_name,
                target_display: new_name.clone(),
                target_path,
                target_is_dir,
                source_is_dir,
                reason: CollisionReason::OnDisk,
                decided_replace: false,
                decided_skip: false,
                conflict_source_path: None,
                live_reason: LiveCollisionReason::OnDisk, // overwritten below
            }],
            current: 0,
            all_skip: false,
            is_f2: true,
            init_focus: true,
        };
        state.recompute_live_reasons();
        state
    }

    /// Per-op collision decisions aligned with `plan.ops`. Entries the user left
    /// undecided (or that are not in the dialog) default to `OpDecision::Default`.
    ///
    /// Keyed by source path → plan index so index shifts (from background scans)
    /// between showing the dialog and resolving it cannot mis-map a decision.
    pub(crate) fn decisions(&self, plan: &awara::RenamePlan) -> Vec<awara::OpDecision> {
        let mut decisions = vec![awara::OpDecision::Default; plan.len()];
        if self.all_skip {
            return decisions;
        }

        // Map only the paths this dialog references, in one O(plan) pass with
        // no per-op allocation.
        let wanted: HashSet<&str> = self
            .entries
            .iter()
            .map(|e| e.source_path.as_str())
            .collect();
        let mut idx_of: HashMap<&str, usize> = HashMap::new();
        for (i, op) in plan.ops().iter().enumerate() {
            if let Some(s) = op.original_path.to_str()
                && wanted.contains(s)
            {
                idx_of.insert(s, i);
            }
        }

        for e in &self.entries {
            if let Some(&i) = idx_of.get(e.source_path.as_str()) {
                decisions[i] = if e.target_is_dir || e.source_is_dir {
                    // A folder on either side cannot be overwritten or merged; a
                    // Replace choice degrades to Skip. Execution refuses such
                    // collisions too (defense in depth), but mapping it here keeps
                    // the reported counts honest.
                    awara::OpDecision::Skip
                } else if e.decided_skip {
                    awara::OpDecision::Skip
                } else if e.decided_replace {
                    awara::OpDecision::Overwrite
                } else {
                    awara::OpDecision::Default
                };
            }
        }
        decisions
    }
}
