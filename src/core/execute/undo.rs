use super::*;

/// Inverse of a rename op (undo direction): swaps original/new paths and
/// names, preserving metadata and flags.
pub fn invert_op(op: &RenameOp) -> RenameOp {
    RenameOp {
        original_path: op.new_path.clone(),
        new_path: op.original_path.clone(),
        original_name: op.new_name.clone(),
        new_name: op.original_name.clone(),
        was_copy: op.was_copy,
        relocated: op.relocated,
        original_permissions: op.original_permissions.clone(),
        applied_attributes: op.applied_attributes.clone(),
        applied_timestamps_mask: op.applied_timestamps_mask,
    }
}

/// Re-apply the file effects recorded on an in-place op (redo direction):
/// attributes from `applied_attributes`, and timestamps from `config` when any
/// were applied. This is the inverse of the snapshot restore that
/// [`apply_ops_chain_safe`] performs for a no-op in the undo direction.
pub fn reapply_metadata(op: &RenameOp, config: &RenameConfig, file_index: usize) {
    reapply_metadata_cached(op, &TimestampCache::new(config), file_index);
}

/// Like [`reapply_metadata`], but reuses a prebuilt [`TimestampCache`] so a batch
/// of ops doesn't re-parse `Fixed` timestamps per op.
pub fn reapply_metadata_cached(op: &RenameOp, cache: &TimestampCache, file_index: usize) {
    if !op.applied_attributes.is_empty() {
        set_file_attributes(&op.original_path, &op.applied_attributes);
    }
    if op.applied_timestamps_mask != 0 {
        cache.apply(&op.original_path, file_index);
    }
}

/// Reverse a set of forward rename ops (undo): move each file back from its
/// `new_path` to its `original_path`, restoring metadata. Delegates to
/// `apply_ops_chain_safe`, so chains/swaps are resolved via temp-name
/// deferral (no clobbering) and genuine on-disk collisions are reported as
/// errors. Ops whose current path no longer exists (deleted externally) are
/// skipped silently. Returns (forward ops successfully undone, error count).
pub fn apply_undo(ops: &[RenameOp]) -> (Vec<RenameOp>, usize) {
    // Inverse ops in exec semantics (original = current, new = restored path).
    // Ops whose current location is gone have nothing to undo — skip them
    // silently, matching the previous behavior.
    let inverse: Vec<RenameOp> = ops
        .iter()
        .filter(|op| op.new_path.exists())
        .map(invert_op)
        .collect();
    let result = apply_ops_chain_safe(&inverse);
    // Report the forward ops that were successfully undone, in input order
    // (the executor may reorder chain-deferred completions to the end).
    let undone: HashSet<(PathBuf, PathBuf)> = result
        .successful
        .iter()
        .map(|op| (op.original_path.clone(), op.new_path.clone()))
        .collect();
    let reverted: Vec<RenameOp> = ops
        .iter()
        .filter(|op| undone.contains(&(op.new_path.clone(), op.original_path.clone())))
        .cloned()
        .collect();
    (reverted, result.failed.len())
}

/// Park a file at a temporary name in the same directory so a chain/swap
/// conflict can vacate its path. Tries `awara_tmp_<uuid>` first, then a
/// compact `~aw_<short_hex>` name if the OS rejects it (e.g. filename/path
/// length limits on deep paths). Returns the temp path. Shared by the forward
/// apply pipeline and the chain-safe executor.
pub(crate) fn park_at_temp(src: &Path) -> std::io::Result<PathBuf> {
    let chain_parent = src.parent().unwrap_or_else(|| Path::new("."));
    let tmp_standard = chain_parent.join(format!("awara_tmp_{}", uuid::Uuid::new_v4()));
    match fs::rename(src, &tmp_standard) {
        Ok(()) => Ok(tmp_standard),
        Err(first_err) => {
            let short_uuid = uuid::Uuid::new_v4().simple().to_string();
            let tmp_short = chain_parent.join(format!("~aw_{}", &short_uuid[..8]));
            match fs::rename(src, &tmp_short) {
                Ok(()) => Ok(tmp_short),
                Err(_) => Err(first_err),
            }
        }
    }
}

/// Remove a copied entry during undo. A copied directory is removed only when
/// empty (`remove_dir` fails on a non-empty dir, so undo can never delete
/// unexpected contents); a copied file or symlink is removed as a file.
pub(crate) fn remove_copied_entry(path: &Path) -> std::io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

/// Execute a batch of explicit rename ops (original_path → new_path) with the
/// same chain/cycle resolution as `execute_renames`: when a target is occupied
/// by another op's source, the file is parked at a temporary name in the same
/// directory so the conflicting op can vacate the path, then moved to its
/// final target. Genuine on-disk collisions (target exists and is not part of
/// the batch) are skipped and reported as failures. Case-only renames use the
/// case-insensitive path, and `original_permissions` snapshots are restored to
/// the target after a successful rename. Copy-mode ops (`was_copy` with the
/// source preserved) delete the copy instead of renaming.
pub fn apply_ops_chain_safe(ops: &[RenameOp]) -> OpBatchResult {
    // Per-batch memo: one case-sensitivity probe per distinct directory (the
    // global cache is the SSOT across batches).
    let mut cs_memo: HashMap<PathBuf, CaseSensitivity> = HashMap::new();
    let mut cs_for = |p: &Path| -> CaseSensitivity {
        if let Some(&cs) = cs_memo.get(p) {
            return cs;
        }
        let cs = case_sensitivity_for(p);
        cs_memo.insert(p.to_path_buf(), cs);
        cs
    };

    // Every op that *moves* vacates its source; a copy (undo-direction `was_copy`)
    // keeps its source, so its source is neither a chain participant nor a
    // target that can be safely overwritten.
    let source_set = batch_source_set(ops, |_, op| !op.was_copy, &mut cs_for);

    // Deferred chain completions: (temp_path, op) — the parked file is moved
    // to `op.new_path` after the conflicting ops have vacated it.
    let mut deferred: Vec<(PathBuf, RenameOp)> = Vec::new();
    let mut processed_targets: HashSet<String> = HashSet::new();
    let mut successful: Vec<RenameOp> = Vec::new();
    let mut failed: Vec<(RenameOp, String)> = Vec::new();

    for op in ops {
        let mut op = op.clone();
        // In-place effects op (attributes/timestamps, name unchanged): replaying
        // it restores the captured pre-effect metadata (undo direction), using
        // the same restore path as a rename op. A snapshot-less no-op is inert.
        if op.is_noop() {
            if let Some(ref snap) = op.original_permissions {
                snap.apply(&op.original_path);
                successful.push(op);
            }
            continue;
        }
        let parent = op.new_path.parent().unwrap_or_else(|| Path::new("."));
        let cs = cs_for(parent);
        let key = target_key(&op, cs);

        if processed_targets.contains(&key) {
            failed.push((
                op,
                "Another file has already been renamed to this target".into(),
            ));
            continue;
        }

        // Copy-mode undo: the source is preserved — just delete the copy. A
        // copied directory is removed only if empty (`remove_copied_entry`),
        // never recursively.
        if op.was_copy && op.new_path.exists() {
            match remove_copied_entry(&op.original_path) {
                Ok(()) => {
                    processed_targets.insert(key);
                    successful.push(op);
                }
                Err(e) => failed.push((op, e.to_string())),
            }
            continue;
        }

        match target_relation(&op, &key, cs, &source_set) {
            TargetRelation::OnDisk => {
                // Genuine on-disk collision — not part of this batch.
                failed.push((op, "Target file already exists on disk".into()));
                continue;
            }
            TargetRelation::Chain => {
                // Target is another op's source. Park the file at a temporary path
                // in the same directory so the conflicting op can vacate the path.
                let rename_res = park_at_temp(&op.original_path);
                match rename_res {
                    Ok(tmp) => {
                        // Capture before the park (the file is still at its original
                        // path), but keep an intent-aware snapshot if the op already
                        // carries one (plan-built ops do). Falling back to the op's
                        // own intent keeps reversible metadata across a deferral.
                        if op.original_permissions.is_none() {
                            op.original_permissions = PermSnapshot::capture_for(
                                &op.original_path,
                                &op.applied_attributes,
                                op.applied_timestamps_mask,
                            );
                        }
                        processed_targets.insert(key);
                        deferred.push((tmp, op));
                    }
                    Err(e) => failed.push((op, e.to_string())),
                }
                continue;
            }
            TargetRelation::Free => {}
        }

        // Plain rename; case-only targets (same entry on disk) go through the
        // case-insensitive path.
        let rename_result = if op.new_path.exists() && op.new_path != op.original_path {
            if is_same_file(&op.new_path, &op.original_path, cs) {
                rename_case_insensitive(&op.original_path, &op.new_path, cs)
            } else {
                fs::rename(&op.original_path, &op.new_path)
            }
        } else {
            fs::rename(&op.original_path, &op.new_path)
        };

        match rename_result {
            Ok(()) => {
                if let Some(ref snap) = op.original_permissions {
                    snap.apply(&op.new_path);
                }
                processed_targets.insert(key);
                successful.push(op);
            }
            Err(e) => failed.push((op, e.to_string())),
        }
    }

    // ── Complete deferred chain renames (temp → final target) ──
    for (tmp_path, op) in deferred {
        match fs::rename(&tmp_path, &op.new_path) {
            Ok(()) => {
                if let Some(ref snap) = op.original_permissions {
                    snap.apply(&op.new_path);
                }
                successful.push(op);
            }
            Err(e) => failed.push((op, e.to_string())),
        }
    }

    OpBatchResult { successful, failed }
}
