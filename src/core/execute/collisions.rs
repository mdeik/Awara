use super::*;

/// Windows' trailing space/dot stripping rule, applied at file creation:
/// trailing spaces/dots are removed from the file name, and from the stem
/// before the extension. Pure and unconditional so it is testable everywhere.
pub(crate) fn trim_windows_trailing(name: &str) -> String {
    let name = name.trim_end_matches([' ', '.']);
    match name.rfind('.') {
        Some(pos) if pos > 0 => {
            let (stem, ext) = name.split_at(pos);
            format!("{}{}", stem.trim_end_matches([' ', '.']), ext)
        }
        _ => name.to_string(),
    }
}

/// Normalize a computed target name to the name Windows will actually create
/// (the OS strips trailing spaces/dots automatically). Returns the input
/// unchanged on every other platform.
///
/// ── IMPORTANT: result only, NOT preview ──
/// This gate exists so executed renames, collision handling, undo data, and
/// the post-apply file list all match on-disk reality on Windows. It must
/// NEVER be applied to the preview: the preview stays logical (untrimmed) so
/// "remove first/last N" always shows exactly N characters removed. Preview
/// names are produced by `process_filename`, which deliberately does not call
/// this. `cfg!(windows)` is a compile-time constant, so this is the single
/// gate for the Windows-only behavior.
pub fn normalize_target_name(name: &str) -> String {
    if cfg!(windows) {
        trim_windows_trailing(name)
    } else {
        name.to_string()
    }
}

/// Replace the final component of `path` with the final component of `name`,
/// preserving the target directory. Windows strips trailing spaces/dots from the
/// name it creates, so a computed name's last component changes — but the
/// directory it lands in (output-dir, keep-structure, or a name-implied subpath)
/// must not. `name` may itself contain separators; only its **last** component is
/// taken, because `path` was already built from that same name. Joining the whole
/// name onto `path.parent()` would re-apply a name-implied subpath
/// (`sub/a.txt` → `sub/sub/a.txt`).
pub(crate) fn replace_final_component(path: &Path, name: &str) -> PathBuf {
    let mut out = path.to_path_buf();
    if let Some(final_component) = Path::new(name).file_name() {
        out.set_file_name(final_component);
    }
    out
}

/// True when `path` is an existing directory, using no-follow semantics so a
/// symlink to a directory is not treated as one (matching `fs::rename`, which
/// operates on the symlink itself). This is the single definition of a
/// "directory target" for collision detection and the `Overwrite` guard.
pub fn is_existing_dir_target(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

/// The normalized target key of an op — the path a collision check compares.
/// Mirrors Windows' trailing space/dot stripping so in-batch duplicates that
/// collapse to the same on-disk name are caught up front. Single definition
/// used by `detect_collisions`, the forward executor, and the chain-safe
/// undo/revert executor.
pub(crate) fn target_key(op: &RenameOp, cs: CaseSensitivity) -> String {
    let normalized = normalize_target_name(&op.new_name);
    if normalized != op.new_name {
        norm_path_for_collision(&replace_final_component(&op.new_path, &normalized), cs)
    } else {
        norm_path_for_collision(&op.new_path, cs)
    }
}

/// The set of paths a batch vacates (its sources), normalized for comparison.
/// `vacates(i, op)` selects which ops participate: no-ops never do, copies never
/// do (a copy keeps its source — see [`op_is_copy`]), and the forward executor
/// additionally excludes ops decided `Skip`.
pub(crate) fn batch_source_set(
    ops: &[RenameOp],
    mut vacates: impl FnMut(usize, &RenameOp) -> bool,
    cs_for: &mut impl FnMut(&Path) -> CaseSensitivity,
) -> HashSet<String> {
    ops.iter()
        .enumerate()
        .filter(|&(i, op)| !op.is_noop() && vacates(i, op))
        .map(|(_, op)| {
            let parent = op.original_path.parent().unwrap_or_else(|| Path::new("."));
            norm_path_for_collision(&op.original_path, cs_for(parent))
        })
        .collect()
}

/// True when the op transfers by **copying** rather than moving, so its source is
/// not vacated. A directory relocated by the output location is always a copy
/// (only its entry is placed, never its contents); a file is a copy only when the
/// batch is in copy mode. Relocation is read from the explicit
/// [`RenameOp::relocated`] flag — never inferred from path shape. The single
/// definition used by collision detection and both executors.
pub(crate) fn op_is_copy(op: &RenameOp, copy_mode: bool) -> bool {
    copy_mode || (op.original_path.is_dir() && op.relocated)
}

/// How an op's target relates to disk and the rest of the batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetRelation {
    /// Target exists on disk because the batch itself vacates it (chain/cycle).
    Chain,
    /// Target exists on disk and is not part of this batch.
    OnDisk,
    /// Target is free.
    Free,
}

/// Classify `op`'s target as chain / on-disk / free. The single definition
/// shared by `detect_collisions` and both executors, so the rules cannot drift
/// between the reported collisions and what execution actually does.
pub(crate) fn target_relation(
    op: &RenameOp,
    target_key: &str,
    cs: CaseSensitivity,
    source_set: &HashSet<String>,
) -> TargetRelation {
    if op.new_path.exists() && !is_same_file(&op.new_path, &op.original_path, cs) {
        if source_set.contains(target_key) {
            TargetRelation::Chain
        } else {
            TargetRelation::OnDisk
        }
    } else {
        TargetRelation::Free
    }
}

/// Detect collisions in a batch of pre-computed RenameOps.
///
/// Returns only "real" collisions — in-batch duplicates and on-disk conflicts
/// where the target is NOT another op's source. Chain conflicts (a→b, b→c)
/// are filtered out because execute_renames resolves them internally via
/// temp-name renaming.
///
/// Calls .exists() on disk for OnDisk checks. O(n).
pub fn detect_collisions(ops: &[RenameOp]) -> Vec<Collision> {
    detect_collisions_with(ops, false)
}

/// Like [`detect_collisions`], but aware of copy mode so the reported collisions
/// match what execution will do: sources of copies are not vacated, so a target
/// that is a copy's source is a real on-disk conflict, not a chain.
pub fn detect_collisions_with(ops: &[RenameOp], copy_mode: bool) -> Vec<Collision> {
    // Per-batch memo: one case-sensitivity probe per distinct directory per
    // batch (the global cache dedupes across batches). Keyed by owned path
    // because the memo outlives any single borrow.
    let mut cs_memo: HashMap<PathBuf, CaseSensitivity> = HashMap::new();
    let mut cs_for = |p: &Path| -> CaseSensitivity {
        if let Some(&cs) = cs_memo.get(p) {
            return cs;
        }
        let cs = case_sensitivity_for(p);
        cs_memo.insert(p.to_path_buf(), cs);
        cs
    };

    // Only ops that move vacate their source (shared rule with execution).
    let source_set = batch_source_set(ops, |_, op| !op_is_copy(op, copy_mode), &mut cs_for);

    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut collisions = Vec::new();

    for (i, op) in ops.iter().enumerate() {
        if op.is_noop() {
            continue;
        }
        let parent = op.new_path.parent().unwrap_or_else(|| Path::new("."));
        let cs = cs_for(parent);
        let key = target_key(op, cs);
        // No-follow so a symlink-to-directory is not mistaken for a real
        // directory (rename replaces the symlink itself). Only meaningful when
        // the target already exists on disk; an in-batch target that does not
        // exist yet is not a directory.
        let target_is_dir = is_existing_dir_target(&op.new_path);
        // A folder *source* cannot replace an existing target either, so the
        // collision is handled with folder rules whenever one is involved.
        let source_is_dir = is_existing_dir_target(&op.original_path);

        if let Some(&prev) = seen.get(&key) {
            collisions.push(Collision {
                op_index: i,
                reason: CollisionReason::InBatch(prev),
                target_is_dir,
                source_is_dir,
            });
        } else if target_relation(op, &key, cs, &source_set) == TargetRelation::OnDisk {
            collisions.push(Collision {
                op_index: i,
                reason: CollisionReason::OnDisk,
                target_is_dir,
                source_is_dir,
            });
        }

        seen.insert(key, i);
    }

    collisions
}

/// True when `rel` can be safely joined onto another path. A path with a
/// root component (e.g. `/b` or `C:\b`) or a drive prefix (e.g. `C:b`)
/// makes `Path::join` replace the base path instead of appending. On
/// Windows a rooted path like `/b` is drive-relative, so `is_absolute()`
/// alone is not a sufficient guard.
fn is_join_safe_rel(rel: &Path) -> bool {
    !rel.has_root()
        && !matches!(
            rel.components().next(),
            Some(std::path::Component::Prefix(_))
        )
}

/// Apply the output-dir and keep-structure path transformation to a RenameOp.
/// Pure computation — no filesystem side effects (caller handles mkdir).
///
/// This is the one place the Copy / Move to Location transform runs, so it is
/// also the one place that records whether it actually relocated the op: it sets
/// [`RenameOp::relocated`] to whether the computed target lands in a different
/// directory. Consumers (`op_is_copy`, [`RenameOp::is_location_transfer`]) read
/// that flag instead of comparing parents themselves, so a computed name that
/// happens to contain a separator can never be mistaken for a relocation.
pub fn apply_output_dir(
    op: &mut RenameOp,
    output_dir: &Path,
    keep_structure: bool,
    base_dir: Option<&Path>,
) {
    if keep_structure
        && let Some(base) = base_dir
        && let Ok(rel) = op
            .original_path
            .parent()
            .unwrap_or(Path::new("."))
            .strip_prefix(base)
        // Never join a rel that could replace the output dir — absolute
        // paths, drive-relative roots (`/b` on Windows), or drive-prefixed
        // paths (`C:b`) all make `Path::join` discard `output_dir`.
        && is_join_safe_rel(rel)
    {
        op.new_path = output_dir.join(rel).join(&op.new_name);
    } else {
        op.new_path = output_dir.join(&op.new_name);
    }
    op.relocated = op.lands_in_other_directory();
}

/// Resolve an output-dir string against the caller's current folder.
/// Absolute paths are used verbatim; relative paths (`.`, `..`, `Subfolder`)
/// are resolved against `base` — the folder currently being browsed in the
/// GUI/TUI, or the working directory in the CLI. This is the single rule all
/// frontends share, so "Copy/Move to Location" behaves like Awara's: relative
/// targets are relative to the current folder, not the process cwd.
pub fn resolve_output_dir(output_dir: &str, base: &Path) -> PathBuf {
    let p = Path::new(output_dir);
    if p.is_absolute() {
        crate::normalize_path_lexically(p)
    } else {
        crate::normalize_path_lexically(&base.join(p))
    }
}

/// Compute the longest common ancestor directory of a set of paths — the
/// directory whose subtree contains every input path. Used by the CLI as the
/// `base_dir` for keep-structure so the relative subfolder layout is recreated
/// under the output directory regardless of where the inputs were rooted.
/// Returns `None` only when `files` is empty; otherwise the empty path is the
/// universal common ancestor of relative inputs (structure is then preserved
/// relative to the working directory). `apply_output_dir` falls back to a flat
/// layout whenever the relative path cannot be derived safely.
pub fn common_base_dir(files: &[PathBuf]) -> Option<PathBuf> {
    let mut parents = files.iter().filter_map(|f| f.parent());
    let mut base = parents.next()?.to_path_buf();
    for p in parents {
        while !p.starts_with(&base) {
            if !base.pop() {
                return None;
            }
        }
    }
    Some(base)
}
