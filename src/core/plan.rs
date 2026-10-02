//! Public facade for the rename planning pipeline.
//!
//! A [`RenamePlan`] is the single, immutable record of what a rename will do:
//! the preview displays its names, collision detection inspects its ops, and
//! execution applies them. It is produced by [`plan_renames`] (the
//! one place that applies numbering, runs the per-file pipeline, and applies the
//! output-directory transform) and consumed by [`RenamePlan::collisions`] and
//! [`execute_plan`] (the one place that mutates disk).
//!
//! Keeping the concrete types behind this module lets the internal
//! implementation change without disturbing callers.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::case::{
    CaseSensitivity, case_sensitivity_for, is_same_file, norm_path_for_collision,
    rename_case_insensitive,
};
use crate::core::config::{
    CompiledConfig, RenameConfig, config_has_file_effects, config_has_numbering,
};
use crate::core::execute::{
    Collision, CollisionStrategy, PermSnapshot, RenameEvent, RenameOp, RenameOptions, RenameResult,
    TS_ACCESSED, TS_CREATED, TS_MODIFIED, TargetRelation, TimestampCache, apply_output_dir,
    batch_source_set, detect_collisions_with, is_existing_dir_target, normalize_target_name,
    numbering_indices, op_is_copy, park_at_temp, remove_copied_entry, replace_final_component,
    set_file_attributes, target_key, target_relation,
};
use crate::core::rename::{FileContext, path_navigation_error, process_filename};
use filetime::FileTime;

// ──────────────────────────────────────────────────────────
// Planning inputs
// ──────────────────────────────────────────────────────────

/// Inputs that affect the computed names. Pure — no filesystem writes and no
/// collision policy.
pub struct PlanOptions<'a> {
    pub dirname_level: usize,
    /// When set, target paths are transformed into this directory (the
    /// copy/move-to-location feature). Part of the plan, not execution.
    pub output_dir: Option<&'a Path>,
    pub keep_structure: bool,
    /// Base directory used to derive relative paths for `keep_structure`.
    pub base_dir: Option<&'a Path>,
    pub parallel: bool,
    /// Copy entries instead of moving them. Recorded on the plan (and therefore
    /// part of collision classification), because whether an op vacates its
    /// source is a planning-time concern — see `op_is_copy`.
    pub copy_mode: bool,
}

impl Default for PlanOptions<'_> {
    fn default() -> Self {
        Self {
            dirname_level: 1,
            output_dir: None,
            keep_structure: false,
            base_dir: None,
            parallel: false,
            copy_mode: false,
        }
    }
}

/// Run the pipeline (numbering included) and build the op for one file.
fn finish_entry(
    ctx: FileContext,
    index: Option<usize>,
    compiled: &CompiledConfig,
    include_unchanged: bool,
) -> Option<RenameOp> {
    let new_name = process_filename(
        &ctx.file_name,
        compiled,
        index,
        ctx.parent_name.as_deref(),
        ctx.is_dir,
        &ctx.meta,
    );
    ctx.into_op(new_name, include_unchanged)
}

// ──────────────────────────────────────────────────────────
// The plan
// ──────────────────────────────────────────────────────────

/// A computed rename plan. Ops stay in input (numbering/display) order so
/// per-op indices are stable between collision detection and execution.
///
/// **Invariant:** a plan contains only actionable work — an op is present when
/// it changes the target path, or when the config applies file effects
/// (attributes/timestamps) to it (see [`config_has_file_effects`]). A pure
/// no-op ([`RenameOp::is_noop`] with no effects) is never planned, and
/// [`execute_plan`] enforces the same for externally built plans. Preview
/// synthesizes identity rows itself rather than relying on no-op ops.
///
/// `new_name` is the **logical** name produced by the pipeline. Windows
/// trailing-space/dot normalization is a result-only concern and is applied by
/// `detect_collisions` (target keys) and [`execute_plan`] (working copy), never
/// baked into the plan — the preview must show logical names.
pub struct RenamePlan {
    ops: Vec<RenameOp>,
    /// Copy mode active when this plan was built. Whether an op copies or moves
    /// decides collision classification (a copy keeps its source) and execution.
    copy_mode: bool,
    /// True when the plan was built with an output location (the Copy / Move to
    /// Location feature).
    output_dir_applied: bool,
}

impl RenamePlan {
    /// Wrap explicit ops (e.g. an F2 inline rename) as a one-off plan. Moves
    /// (never copies) — inline renames are always in place.
    pub fn from_ops(ops: Vec<RenameOp>) -> Self {
        Self {
            ops,
            copy_mode: false,
            output_dir_applied: false,
        }
    }

    /// Wrap explicit ops with an explicit copy mode (used by tests and callers
    /// that build plans by hand).
    pub fn from_ops_with_copy_mode(ops: Vec<RenameOp>, copy_mode: bool) -> Self {
        Self {
            ops,
            copy_mode,
            output_dir_applied: false,
        }
    }

    pub fn ops(&self) -> &[RenameOp] {
        &self.ops
    }

    /// Copy mode active for this plan.
    pub fn copy_mode(&self) -> bool {
        self.copy_mode
    }

    /// True when this plan was built with an output location (the Copy / Move to
    /// Location feature). The plan-level record of the transform; per-op detail
    /// lives on [`RenameOp::relocated`], which is what decides undoability and
    /// whether a directory is copied rather than moved.
    pub fn uses_output_location(&self) -> bool {
        self.output_dir_applied
    }

    /// Consume the plan, yielding its ops. Used by callers that take ownership
    /// (execution, preview) to avoid cloning every op.
    pub fn into_ops(self) -> Vec<RenameOp> {
        self.ops
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Collisions for this plan (SSoT: [`detect_collisions_with`]), classified
    /// with the plan's own copy mode so the report matches execution.
    pub fn collisions(&self) -> Vec<Collision> {
        detect_collisions_with(&self.ops, self.copy_mode)
    }
}

// ──────────────────────────────────────────────────────────
// Planning
// ──────────────────────────────────────────────────────────

/// Compute the rename plan for `file_paths`.
///
/// This is the only place that applies numbering, runs `process_filename`, and
/// applies the output-directory transform. `file_paths` must already be in
/// numbering/display order.
pub fn plan_renames(
    file_paths: &[String],
    config: &RenameConfig,
    opts: &PlanOptions,
) -> RenamePlan {
    static NEVER_CANCEL: AtomicBool = AtomicBool::new(false);
    plan_renames_cancellable(file_paths, config, opts, &NEVER_CANCEL)
        .expect("never-cancelled plan always completes")
}

/// Like [`plan_renames`] but abandons the work (returning `None`) when `cancel`
/// is set. Cancellation is checked between processing chunks, so a superseding
/// request aborts a large batch promptly.
pub fn plan_renames_cancellable(
    file_paths: &[String],
    config: &RenameConfig,
    opts: &PlanOptions,
    cancel: &AtomicBool,
) -> Option<RenamePlan> {
    let total = file_paths.len();
    let compiled = CompiledConfig::from_ref(config);
    // A file whose name is unchanged is still actionable when the output-dir
    // transform can move it or the config applies file effects (attributes /
    // timestamps). Otherwise no op is built for it.
    let has_effects = config_has_file_effects(config);
    let include_unchanged = opts.output_dir.is_some() || has_effects;
    let dirname_level = Some(opts.dirname_level);

    if cancel.load(Ordering::Relaxed) {
        return None;
    }

    // Numbering is purely positional (per-folder restart and `numbering_break`
    // group size both depend only on position), so the offsets are known from
    // the paths up front and each file can be processed in a single pass.
    let seq: Vec<Option<usize>> = if config_has_numbering(config) {
        let order: Vec<usize> = (0..total).collect();
        numbering_indices(file_paths, config, &order, |_| true)
    } else {
        vec![None; total]
    };
    let parallel = opts.parallel && total > 100;

    // Process in chunks so the cancel flag is observed promptly on huge batches.
    const CHUNK_SIZE: usize = 1000;

    let mut ops: Vec<RenameOp> = Vec::with_capacity(total);
    let mut start = 0;
    while start < total {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let end = (start + CHUNK_SIZE).min(total);
        let slice = &file_paths[start..end];
        let run = |base: usize, k: usize, fp: &String| -> Option<RenameOp> {
            let ctx = FileContext::from_path(fp, &compiled, dirname_level)?;
            finish_entry(ctx, seq[base + k], &compiled, include_unchanged)
        };
        if parallel {
            use rayon::prelude::*;
            let part: Vec<RenameOp> = slice
                .par_iter()
                .enumerate()
                .filter_map(|(k, fp)| run(start, k, fp))
                .collect();
            ops.extend(part);
        } else {
            for (k, fp) in slice.iter().enumerate() {
                if let Some(op) = run(start, k, fp) {
                    ops.push(op);
                }
            }
        }
        start = end;
    }

    if cancel.load(Ordering::Relaxed) {
        return None;
    }

    // Apply the output-dir / keep-structure transformation here so every
    // consumer (preview, collision detection, execution, undo) sees identical
    // paths, and execution never re-transforms.
    if let Some(out_dir) = opts.output_dir {
        let base = if opts.keep_structure {
            opts.base_dir
        } else {
            None
        };
        for op in &mut ops {
            apply_output_dir(op, out_dir, opts.keep_structure, base);
        }
    }

    // SSoT: a plan contains only actionable work. After the transform an
    // unchanged-name op whose target is still its source is a no-op — unless
    // the config applies file effects (attributes/timestamps), which are real
    // work on their own. A **navigational** name is kept even when it is
    // otherwise a no-op: `./a.txt` normalizes to the source path, but it is
    // still an invalid name that must be refused and reported, not silently
    // vanish into a misleading "nothing to rename".
    ops.retain(|op| !op.is_noop() || has_effects || path_navigation_error(&op.new_name).is_some());

    Some(RenamePlan {
        ops,
        // Copy mode is only meaningful with an output location; forcing it off
        // otherwise keeps a stray `copy_mode` from turning plain renames into
        // in-place copies.
        copy_mode: opts.copy_mode && opts.output_dir.is_some(),
        output_dir_applied: opts.output_dir.is_some(),
    })
}

// ──────────────────────────────────────────────────────────
// Collision resolution policy
// ──────────────────────────────────────────────────────────

/// Per-op collision decision, keyed by plan index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpDecision {
    /// Use [`Resolution::default_strategy`].
    Default,
    /// Do not rename this op. It still occupies its numbering slot.
    Skip,
    /// Ignore a detected collision and overwrite the target.
    Overwrite,
}

/// Collision policy for an execution: a default strategy plus optional per-op
/// overrides aligned with the plan's ops.
pub struct Resolution {
    pub default_strategy: CollisionStrategy,
    /// `Some(vec)` aligned with `plan.ops`; `None` means every op is `Default`.
    pub per_op: Option<Vec<OpDecision>>,
}

impl Resolution {
    pub fn new(default_strategy: CollisionStrategy, per_op: Option<Vec<OpDecision>>) -> Self {
        Self {
            default_strategy,
            per_op,
        }
    }

    pub fn default_for(default_strategy: CollisionStrategy) -> Self {
        Self {
            default_strategy,
            per_op: None,
        }
    }

    #[inline]
    fn decision(&self, op_index: usize) -> OpDecision {
        self.per_op
            .as_ref()
            .and_then(|v| v.get(op_index).copied())
            .unwrap_or(OpDecision::Default)
    }
}

// ─────────────────────────────────────────────────────────
// Execution
// ─────────────────────────────────────────────────────────

/// Report, once per distinct set, any requested attributes the target volume
/// cannot apply. Phrased as a single line for multiple attributes instead of
/// one sentence per attribute.
fn report_unsupported(
    requested: &str,
    path: &Path,
    reported: &mut HashSet<String>,
    emit: &mut impl FnMut(RenameEvent),
) {
    let caps = crate::core::attrs::caps_for_write(path);
    let mut missing = crate::core::attrs::unsupported_in(&caps, requested);
    if missing.is_empty() {
        return;
    }
    missing.sort();
    missing.dedup();
    // Same gap (regardless of order or repetition) reports only once.
    if !reported.insert(missing.join(",")) {
        return;
    }
    let message = match missing.as_slice() {
        [only] => format!("Attribute '{only}' is not supported on this volume; skipped"),
        many => format!(
            "Attributes not supported on this volume; skipped: {}",
            many.join(", ")
        ),
    };
    emit(RenameEvent::Status { message });
}

/// Reason recorded for a name-unchanged op with nothing it can actually do.
const NO_EFFECT_REASON: &str = "name unchanged and no requested attribute/timestamp applies";

/// Reason recorded when an `Overwrite` decision meets an existing folder target.
/// Folders cannot be replaced or merged, so the op is skipped instead.
const FOLDER_TARGET_REASON: &str = "Folder already exists; folders cannot be overwritten or merged";

/// Reason recorded when a folder source would replace an existing file target.
/// Folder handling applies on either side, so the file is left untouched.
const FOLDER_SOURCE_REASON: &str = "File already exists; a folder cannot replace it";

/// Whether at least one requested file effect can actually be applied to the
/// volume holding `path`.
///
/// Deliberately capability-based, not "already in the desired state": it exists
/// to drop name-unchanged ops that would otherwise be reported as successful
/// renames while doing nothing on disk. Resolved at execution time (the planner
/// stays pure).
fn effects_applicable(path: &Path, set_attrs: Option<&str>, config: &RenameConfig) -> bool {
    let caps = crate::core::attrs::caps_for_write(path);
    if let Some(attrs) = set_attrs
        && attrs.split(',').any(|token| {
            crate::core::schema::canonical_attribute_name(token)
                .is_some_and(|name| caps.supports(name))
        })
    {
        return true;
    }
    // mtime/atime are settable everywhere; birth time only where the volume has
    // a working setter (see `AttrCaps::birth_time`).
    if config.special.set_modified.is_some() || config.special.set_accessed.is_some() {
        return true;
    }
    config.special.set_created.is_some() && caps.birth_time
}

/// Execute a plan. This is the only function that mutates the filesystem; it
/// never recomputes names. Consumes the plan so ops can be moved, not cloned.
///
/// `options` supplies execution-only settings (dry-run, undo, attributes,
/// timestamps, cancellation).
pub fn execute_plan(
    plan: RenamePlan,
    config: &RenameConfig,
    options: &mut RenameOptions<'_>,
    resolution: &Resolution,
) -> RenameResult {
    // File effects (attributes/timestamps) are real work even when the name is
    // unchanged, so a path no-op is only inert when none apply.
    let applies_effects = options.set_attributes.is_some()
        || config.special.set_created.is_some()
        || config.special.set_modified.is_some()
        || config.special.set_accessed.is_some();

    let mut emit = |evt: RenameEvent| {
        if let Some(ref mut cb) = options.on_event {
            cb(evt)
        }
    };

    // Working items: (stable plan index, op with Windows-normalized target).
    // Dropping a Windows no-op keeps the plan index for the surviving ops.
    let copy_mode = plan.copy_mode;
    let plan_ops = plan.into_ops();

    // Validate the **whole plan** before touching the filesystem: a computed name
    // that navigates the path (rooted, `.` or `..`) invalidates the entire apply
    // (SSoT: `path_navigation_error`). Unlike a per-op skip, this refuses the
    // batch wholesale — nothing is renamed — so a bad name can never partially
    // apply, and the caller can explain which names were at fault.
    let invalid: Vec<(RenameOp, String)> = plan_ops
        .iter()
        .filter_map(|op| {
            path_navigation_error(&op.new_name).map(|nav| {
                (
                    op.clone(),
                    format!("New name {}; rename refused", nav.message()),
                )
            })
        })
        .collect();
    if !invalid.is_empty() {
        // Report the refused names, then return without a `Done` — that event
        // signals a completed *execution*, and none happened here.
        for (op, reason) in &invalid {
            emit(RenameEvent::Error {
                original: op.original_name.clone(),
                new: op.new_name.clone(),
                message: reason.clone(),
            });
        }
        emit(RenameEvent::Status {
            message: format!(
                "Invalid new name{}; the whole batch was not applied",
                if invalid.len() == 1 { "" } else { "s" }
            ),
        });
        return RenameResult {
            successful_ops: Vec::new(),
            failed_ops: invalid,
            skipped_ops: Vec::new(),
        };
    }

    // Per-batch memo: one case-sensitivity probe per distinct directory.
    let mut cs_memo: HashMap<PathBuf, CaseSensitivity> = HashMap::new();
    let mut cs_for = |p: &Path| -> CaseSensitivity {
        if let Some(&cs) = cs_memo.get(p) {
            return cs;
        }
        let cs = case_sensitivity_for(p);
        cs_memo.insert(p.to_path_buf(), cs);
        cs
    };

    // Source set for chain detection (shared rule with `detect_collisions_with`).
    // Only ops that *move* vacate their source: copies keep it, and ops decided
    // Skip never run.
    let source_set = batch_source_set(
        &plan_ops,
        |i, op| !op_is_copy(op, copy_mode) && resolution.decision(i) != OpDecision::Skip,
        &mut cs_for,
    );

    // Result buckets, filled by the main loop.
    let mut failed_ops: Vec<(RenameOp, String)> = Vec::new();
    let mut skipped_ops: Vec<(RenameOp, String)> = Vec::new();
    let mut successful_ops: Vec<RenameOp> = Vec::new();

    let mut items: Vec<(usize, RenameOp)> = Vec::with_capacity(plan_ops.len());
    for (i, mut op) in plan_ops.into_iter().enumerate() {
        let normalized = normalize_target_name(&op.new_name);
        if normalized != op.new_name {
            // Rewrite only the final component, keeping the planned target
            // directory intact (output-dir / keep-structure / a name-implied
            // subpath). Rebuilding from the source parent would drop that
            // transform, and joining the whole name would re-apply its subpath.
            op.new_path = replace_final_component(&op.new_path, &normalized);
            op.new_name = normalized;
            if op.new_name == op.original_name {
                continue; // Windows no-op: target normalizes back to the source
            }
        }
        // A path no-op is inert unless file effects apply to it. Plans built by
        // the planner already exclude inert ops; this guards externally built
        // plans (`RenamePlan::from_ops`, e.g. F2/tests) and legacy data.
        if op.is_noop() && !applies_effects {
            continue;
        }
        items.push((i, op));
    }

    // ── Sort by depth descending so children are renamed before parents ──
    // This prevents "lost child" errors when a parent directory rename moves
    // children out from under their original_path before the child op runs.
    // Stable sort preserves the plan order for same-depth entries.
    if items.iter().any(|(_, op)| op.original_path.is_dir()) {
        items.sort_by(|(_, a), (_, b)| {
            b.original_path
                .components()
                .count()
                .cmp(&a.original_path.components().count())
        });
    }

    // Deferred chain completions. Each entry is the parked source (at `tmp`) and
    // the op that must be completed to `op.new_path` once the conflicting op has
    // vacated it.
    let mut deferred_chain_ops: Vec<(PathBuf, RenameOp)> = Vec::new();

    // Deferred *copies*: a copy (copy mode, or a relocated directory) whose
    // target is currently a mover's source. A copy must not park its source, and
    // the target is only free after that mover vacates — so it runs in a phase
    // after all movers (and their chain completions).
    let mut deferred_copies: Vec<(RenameOp, CaseSensitivity)> = Vec::new();

    let mut processed_targets: HashSet<String> = HashSet::new();

    // Fixed timestamps are parsed once here instead of once per file.
    let ts_cache = TimestampCache::new(config);
    // The intentional timestamp/attribute effects are constant for the batch.
    let ts_mask = timestamp_mask(config);
    let set_attrs = options.set_attributes.unwrap_or("");

    // Unsupported-attribute notes: one concise line per distinct missing set,
    // so a large batch never repeats the same message per file.
    let mut warned_attrs: HashSet<String> = HashSet::new();
    // Same idea for name-unchanged ops that turn out to have no applicable effect.
    let mut noop_effect_note = false;
    // And for folder-involved collisions refused under Overwrite (one line per batch).
    let mut warned_folders = false;

    for (file_index, (plan_index, op)) in items.into_iter().enumerate() {
        if let Some(ref token) = options.cancel_token
            && token.load(std::sync::atomic::Ordering::Relaxed)
        {
            emit(RenameEvent::Cancelled {
                completed: successful_ops.len(),
            });
            break;
        }

        let mut op = op;

        let mut skip = false;
        let mut skip_reason = String::new();
        // Whether this op copies (keeps its source) is decided once, up front,
        // from the same rule collision detection uses.
        let is_copy_op = op_is_copy(&op, copy_mode);
        // Which deferral this op needs, if any: a mover parks its source (chain),
        // a copy whose target is a mover's source waits its turn.
        let mut defer_as_chain = false;
        let mut defer_as_copy = false;

        let parent = op.new_path.parent().unwrap_or_else(|| Path::new("."));
        let cs = cs_for(parent);
        let key = target_key(&op, cs);

        // Explicit per-op Skip wins over everything.
        if resolution.decision(plan_index) == OpDecision::Skip {
            skip = true;
            skip_reason = "Skipped".into();
        } else if processed_targets.contains(&key) {
            skip = true;
            skip_reason = "Another file has already been renamed to this target".into();
            emit(RenameEvent::Collision {
                original: op.original_name.clone(),
                new: op.new_name.clone(),
            });
        } else {
            match target_relation(&op, &key, cs, &source_set) {
                // A mover whose target is another mover's source parks its own
                // source so that op can vacate the path.
                TargetRelation::Chain if !is_copy_op => defer_as_chain = true,
                // A copy must never park its source; it waits until the mover
                // vacates the target (deferred-copy phase).
                TargetRelation::Chain => defer_as_copy = true,
                TargetRelation::OnDisk => {
                    skip = true;
                    skip_reason = "Target file already exists on disk".into();
                    emit(RenameEvent::Collision {
                        original: op.original_name.clone(),
                        new: op.new_name.clone(),
                    });
                }
                TargetRelation::Free => {}
            }
        }

        if defer_as_chain && !options.dry_run {
            let rename_res = park_at_temp(&op.original_path);
            match rename_res {
                Ok(tmp) => {
                    let pre_snap = PermSnapshot::capture_for(&op.original_path, set_attrs, ts_mask);
                    processed_targets.insert(key);
                    let orig_name = op.original_name.clone();
                    let new_name = op.new_name.clone();
                    let mut parked = op;
                    parked.original_permissions = pre_snap;
                    deferred_chain_ops.push((tmp, parked));
                    emit(RenameEvent::Progress {
                        original: orig_name,
                        new: new_name,
                    });
                }
                Err(e) => {
                    failed_ops.push((op.clone(), e.to_string()));
                    emit(RenameEvent::Error {
                        original: op.original_name.clone(),
                        new: op.new_name.clone(),
                        message: e.to_string(),
                    });
                }
            }
            continue;
        }

        if defer_as_copy && !options.dry_run {
            // Leave the copy's source untouched; run the copy once the movers
            // (and their chain completions) have vacated the target.
            processed_targets.insert(key);
            deferred_copies.push((op.clone(), cs));
            continue;
        }

        if skip {
            let overwrite = match resolution.decision(plan_index) {
                OpDecision::Overwrite => true,
                OpDecision::Skip => false,
                OpDecision::Default => resolution.default_strategy == CollisionStrategy::Overwrite,
            };
            // Folder handling whenever a folder is involved. `fs::rename` either
            // errors on a cross-type target (and is platform-dependent for a
            // folder source over a file) or silently replaces an *empty*
            // directory, so Overwrite never replaces or merges anything when
            // either side is a folder — keep the skip and report a reason that
            // names what actually already exists (the target's kind).
            let target_is_folder = is_existing_dir_target(&op.new_path);
            if overwrite && (target_is_folder || is_existing_dir_target(&op.original_path)) {
                skip_reason = if target_is_folder {
                    FOLDER_TARGET_REASON
                } else {
                    FOLDER_SOURCE_REASON
                }
                .into();
                if !warned_folders {
                    warned_folders = true;
                    emit(RenameEvent::Status {
                        message: "Some collisions involve a folder; folders cannot be overwritten or merged, so they were skipped"
                            .into(),
                    });
                }
            } else if overwrite {
                skip = false;
                skip_reason.clear();
            }
        }

        if skip {
            skipped_ops.push((op.clone(), skip_reason.clone()));
            emit(RenameEvent::Skipped {
                original: op.original_name.clone(),
                reason: skip_reason,
            });
            continue;
        }

        // A name-unchanged op is only real work if at least one requested effect
        // can actually be applied on this volume. Otherwise it would be counted
        // as a successful rename while changing nothing on disk.
        if op.is_noop() && !effects_applicable(&op.new_path, options.set_attributes, config) {
            skipped_ops.push((op.clone(), NO_EFFECT_REASON.to_string()));
            if !noop_effect_note {
                noop_effect_note = true;
                emit(RenameEvent::Status {
                    message: "Nothing to rename and no requested attribute/timestamp is applicable; skipped"
                        .into(),
                });
            }
            continue;
        }

        emit(RenameEvent::Progress {
            original: op.original_name.clone(),
            new: op.new_name.clone(),
        });

        if !options.dry_run {
            // Only create the target directory when the plan actually moved the
            // target elsewhere (output-dir / keep-structure). A plain rename
            // keeps the source's parent, so avoid a wasted syscall per file.
            if op.lands_in_other_directory() {
                ensure_parent_dir(&op.new_path);
            }
            let orig_meta = if options.preserve_timestamps {
                op.original_path.metadata().ok()
            } else {
                None
            };
            let pre_snap = PermSnapshot::capture_for(&op.original_path, set_attrs, ts_mask);

            // Copy vs move is decided once, up front (`op_is_copy`), so
            // classification and the transfer agree: a copy (copy mode, or a
            // relocated directory) keeps its source.
            let copy_this = is_copy_op;

            let rename_result = if op.is_noop() {
                // Effects-only op (attributes/timestamps): nothing to move.
                Ok(())
            } else if copy_this {
                copy_entry(&op.original_path, &op.new_path)
            } else if op.new_path.exists() && op.new_path != op.original_path {
                if is_same_file(&op.new_path, &op.original_path, cs) {
                    rename_case_insensitive(&op.original_path, &op.new_path, cs)
                } else {
                    fs::rename(&op.original_path, &op.new_path)
                }
            } else {
                fs::rename(&op.original_path, &op.new_path)
            };

            if let Err(e) = rename_result {
                failed_ops.push((op.clone(), e.to_string()));
                emit(RenameEvent::Error {
                    original: op.original_name.clone(),
                    new: op.new_name.clone(),
                    message: e.to_string(),
                });

                if options.stop_on_error {
                    emit(RenameEvent::Status {
                        message: "Stopping on error as requested, undoing...".into(),
                    });

                    for undo_op in successful_ops.iter().rev() {
                        emit(RenameEvent::RollbackProgress {
                            original: undo_op.original_name.clone(),
                            new: undo_op.new_name.clone(),
                        });
                        let result = if undo_op.was_copy && undo_op.original_path.exists() {
                            remove_copied_entry(&undo_op.new_path)
                        } else {
                            fs::rename(&undo_op.new_path, &undo_op.original_path)
                        };
                        if let Err(undo_err) = result {
                            emit(RenameEvent::Error {
                                original: undo_op.original_name.clone(),
                                new: undo_op.new_name.clone(),
                                message: format!(
                                    "Failed to undo '{}' -> '{}': {}",
                                    undo_op.new_name, undo_op.original_name, undo_err
                                ),
                            });
                        }
                    }

                    let result = RenameResult {
                        successful_ops: successful_ops.clone(),
                        failed_ops,
                        skipped_ops,
                    };
                    emit(RenameEvent::Done {
                        success: result.successful_ops.len(),
                        errors: result.failed_ops.len(),
                        skipped: result.skipped_ops.len(),
                    });
                    return result;
                }
            } else {
                if let Some(ref meta) = orig_meta {
                    if let Ok(mtime) = meta.modified() {
                        let ft = FileTime::from_system_time(mtime);
                        let _ = filetime::set_file_mtime(&op.new_path, ft);
                    }
                    if let Ok(atime) = meta.accessed() {
                        let ft = FileTime::from_system_time(atime);
                        let _ = filetime::set_file_atime(&op.new_path, ft);
                    }
                }

                if let Some(attrs) = &options.set_attributes {
                    report_unsupported(attrs, &op.new_path, &mut warned_attrs, &mut emit);
                    let effective = set_file_attributes(&op.new_path, attrs);
                    if effective != op.new_path {
                        let new_name = effective
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or(op.new_name.clone());
                        op.new_path = effective;
                        op.new_name = new_name;
                    }
                }

                ts_cache.apply(&op.new_path, file_index);

                op.was_copy = copy_this;
                op.original_permissions = pre_snap;
                op.applied_attributes = options
                    .set_attributes
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                op.applied_timestamps_mask |= ts_mask;

                processed_targets.insert(norm_path_for_collision(&op.new_path, cs));
                successful_ops.push(op.clone());
            }
        }
    }

    // ── Phase 2: complete deferred chain renames (temp → final target) ──
    // A parked mover must never clobber a live file: if the conflicting op did
    // not vacate the target (it skipped or failed), restore the parked source
    // and report a skip instead of overwriting it.
    for (deferred_idx, (tmp_path, op)) in deferred_chain_ops.into_iter().enumerate() {
        if let Some(ref token) = options.cancel_token
            && token.load(std::sync::atomic::Ordering::Relaxed)
        {
            emit(RenameEvent::Cancelled {
                completed: successful_ops.len(),
            });
            break;
        }

        if op.new_path.exists() {
            // Conflict never vacated — restore the parked source and skip.
            let restore = fs::rename(&tmp_path, &op.original_path);
            skipped_ops.push((op.clone(), "Target file already exists on disk".into()));
            emit(RenameEvent::Collision {
                original: op.original_name.clone(),
                new: op.new_name.clone(),
            });
            emit(RenameEvent::Skipped {
                original: op.original_name.clone(),
                reason: "Target file already exists on disk".into(),
            });
            if let Err(e) = restore {
                emit(RenameEvent::Error {
                    original: op.original_name.clone(),
                    new: op.new_name.clone(),
                    message: format!(
                        "Could not restore parked file '{}' from '{}': {}",
                        op.original_name,
                        tmp_path.display(),
                        e
                    ),
                });
            }
            continue;
        }

        ensure_parent_dir(&op.new_path);
        match fs::rename(&tmp_path, &op.new_path) {
            Ok(()) => {
                let mut op = op;
                if let Some(attrs) = &options.set_attributes {
                    report_unsupported(attrs, &op.new_path, &mut warned_attrs, &mut emit);
                    let effective = set_file_attributes(&op.new_path, attrs);
                    if effective != op.new_path {
                        op.new_name = effective
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or(op.new_name.clone());
                        op.new_path = effective;
                    }
                }
                ts_cache.apply(&op.new_path, deferred_idx);
                // Chain parking is only ever used for movers.
                op.was_copy = false;
                op.applied_attributes = options
                    .set_attributes
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                op.applied_timestamps_mask |= ts_mask;
                successful_ops.push(op);
            }
            Err(e) => {
                // Recover: rename the parked file back to its original path.
                let restore = fs::rename(&tmp_path, &op.original_path);
                let msg = if restore.is_err() {
                    format!(
                        "Chain rename failed: {} (source left at '{}')",
                        e,
                        tmp_path.display()
                    )
                } else {
                    format!("Chain rename failed: {}", e)
                };
                failed_ops.push((op.clone(), msg.clone()));
                emit(RenameEvent::Error {
                    original: op.original_name.clone(),
                    new: op.new_name.clone(),
                    message: msg,
                });
            }
        }
    }

    // ── Phase 3: deferred copies (target was a mover's source) ──
    // All movers and their chain completions have run, so the targets should be
    // free now. Re-check: a conflict that persists (e.g. the mover skipped) is
    // skipped rather than erroring or clobbering.
    for (deferred_idx, (op, cs)) in deferred_copies.into_iter().enumerate() {
        if let Some(ref token) = options.cancel_token
            && token.load(std::sync::atomic::Ordering::Relaxed)
        {
            emit(RenameEvent::Cancelled {
                completed: successful_ops.len(),
            });
            break;
        }

        if op.new_path.exists() && !is_same_file(&op.new_path, &op.original_path, cs) {
            skipped_ops.push((op.clone(), "Target file already exists on disk".into()));
            emit(RenameEvent::Collision {
                original: op.original_name.clone(),
                new: op.new_name.clone(),
            });
            emit(RenameEvent::Skipped {
                original: op.original_name.clone(),
                reason: "Target file already exists on disk".into(),
            });
            continue;
        }

        ensure_parent_dir(&op.new_path);
        let orig_meta = if options.preserve_timestamps {
            op.original_path.metadata().ok()
        } else {
            None
        };
        let pre_snap = PermSnapshot::capture_for(&op.original_path, set_attrs, ts_mask);

        match copy_entry(&op.original_path, &op.new_path) {
            Ok(()) => {
                if let Some(ref meta) = orig_meta {
                    if let Ok(mtime) = meta.modified() {
                        let _ = filetime::set_file_mtime(
                            &op.new_path,
                            FileTime::from_system_time(mtime),
                        );
                    }
                    if let Ok(atime) = meta.accessed() {
                        let _ = filetime::set_file_atime(
                            &op.new_path,
                            FileTime::from_system_time(atime),
                        );
                    }
                }
                let mut op = op;
                if let Some(attrs) = &options.set_attributes {
                    report_unsupported(attrs, &op.new_path, &mut warned_attrs, &mut emit);
                    let effective = set_file_attributes(&op.new_path, attrs);
                    if effective != op.new_path {
                        op.new_name = effective
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or(op.new_name.clone());
                        op.new_path = effective;
                    }
                }
                ts_cache.apply(&op.new_path, deferred_idx);
                op.was_copy = true;
                op.original_permissions = pre_snap;
                op.applied_attributes = options
                    .set_attributes
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                op.applied_timestamps_mask |= ts_mask;
                successful_ops.push(op);
            }
            Err(e) => {
                failed_ops.push((op.clone(), e.to_string()));
                emit(RenameEvent::Error {
                    original: op.original_name.clone(),
                    new: op.new_name.clone(),
                    message: e.to_string(),
                });
            }
        }
    }

    // The undo record holds only undoable ops: Copy / Move to Location transfers
    // are relocations, not renames, so they are not recorded (SSoT with the GUI
    // commit history). Serialize references to avoid cloning the batch.
    if let Some(path) = options.undo_file {
        let undoable: Vec<&RenameOp> = successful_ops
            .iter()
            .filter(|op| op.is_undoable())
            .collect();
        if let Ok(json) = serde_json::to_string_pretty(&undoable) {
            let _ = std::fs::write(path, &json);
        }
    }

    let result = RenameResult {
        successful_ops,
        failed_ops,
        skipped_ops,
    };

    emit(RenameEvent::Done {
        success: result.successful_ops.len(),
        errors: result.failed_ops.len(),
        skipped: result.skipped_ops.len(),
    });

    result
}

/// Timestamp mask for the deferred-chain success paths.
fn timestamp_mask(config: &RenameConfig) -> u8 {
    let mut mask = 0u8;
    if config.special.set_created.is_some() {
        mask |= TS_CREATED;
    }
    if config.special.set_modified.is_some() {
        mask |= TS_MODIFIED;
    }
    if config.special.set_accessed.is_some() {
        mask |= TS_ACCESSED;
    }
    mask
}

/// Create the parent directory of `path` when it is non-empty. Idempotent.
fn ensure_parent_dir(path: &Path) {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).ok();
    }
}

/// Copy-mode transfer for one entry. A regular file (or symlink to one) is
/// copied with its contents; a **directory is recreated by name, empty** —
/// copy mode copies only the selected entry, never descending into its
/// contents. This mirrors the selection semantics: what is in the plan is what
/// gets copied.
fn copy_entry(src: &Path, dst: &Path) -> std::io::Result<()> {
    if fs::metadata(src)?.is_dir() {
        fs::create_dir(dst)
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}

// ──────────────────────────────────────────────────────────
// Convenience: plan + execute
// ──────────────────────────────────────────────────────────

/// Plan and execute in one call for the common case (no output directory).
///
/// Collisions are handled with the safe default (`Skip`); callers that need
/// overwrite or per-op decisions should use [`plan_renames`] + [`execute_plan`]
/// with an explicit [`Resolution`].
pub fn execute_renames(
    file_paths: &[String],
    config: &RenameConfig,
    options: &mut RenameOptions<'_>,
) -> RenameResult {
    let plan = plan_renames(file_paths, config, &PlanOptions::default());
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    execute_plan(plan, config, options, &resolution)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn file(dir: &std::path::Path, name: &str) -> String {
        std::fs::write(dir.join(name), "").unwrap();
        dir.join(name).to_string_lossy().to_string()
    }

    #[test]
    fn report_unsupported_condenses_and_dedups() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, b"x").unwrap();

        let mut reported = HashSet::new();
        let mut events: Vec<RenameEvent> = Vec::new();
        let mut emit = |event| events.push(event);

        // Unknown attributes are unsupported on every platform, so this stays
        // deterministic regardless of the host filesystem.
        report_unsupported("bogus2,bogus1", &path, &mut reported, &mut emit);
        report_unsupported("bogus1,bogus2", &path, &mut reported, &mut emit); // same set

        assert_eq!(events.len(), 1, "the same missing set reports once");
        match &events[0] {
            RenameEvent::Status { message } => assert_eq!(
                message,
                "Attributes not supported on this volume; skipped: bogus1, bogus2"
            ),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn report_unsupported_uses_singular_phrasing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, b"x").unwrap();

        let mut reported = HashSet::new();
        let mut events: Vec<RenameEvent> = Vec::new();
        let mut emit = |event| events.push(event);

        report_unsupported("bogus", &path, &mut reported, &mut emit);

        match &events[..] {
            [RenameEvent::Status { message }] => assert_eq!(
                message,
                "Attribute 'bogus' is not supported on this volume; skipped"
            ),
            other => panic!("unexpected events: {other:?}"),
        }
    }

    fn noop_op(path: &Path) -> RenameOp {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        RenameOp {
            original_path: path.to_path_buf(),
            new_path: path.to_path_buf(),
            original_name: name.clone(),
            new_name: name,
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0,
        }
    }

    #[test]
    fn unchanged_name_with_no_applicable_effect_is_skipped() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, b"x").unwrap();

        let plan = RenamePlan::from_ops(vec![noop_op(&path)]);
        let mut events: Vec<RenameEvent> = Vec::new();
        let mut options = RenameOptions {
            // Unknown attribute: unsupported on every platform, so the check is
            // deterministic regardless of the host filesystem.
            set_attributes: Some("bogus"),
            on_event: Some(Box::new(|event| events.push(event))),
            ..Default::default()
        };
        let resolution = Resolution::default_for(CollisionStrategy::Skip);
        let result = execute_plan(plan, &RenameConfig::default(), &mut options, &resolution);
        drop(options); // release the event closure's borrow of `events`

        assert!(
            result.successful_ops.is_empty(),
            "counted nothing as renamed"
        );
        assert_eq!(result.skipped_ops.len(), 1);
        assert!(path.exists(), "file untouched");
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, RenameEvent::Status { .. }))
                .count(),
            1,
            "one aggregated note, not one per file"
        );
    }

    #[test]
    fn unchanged_name_with_an_applicable_timestamp_is_not_skipped() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, b"x").unwrap();

        let plan = RenamePlan::from_ops(vec![noop_op(&path)]);
        let mut config = RenameConfig::default();
        config.special.set_modified = Some(crate::core::config::TimestampSpec::Current);
        let mut options = RenameOptions {
            set_attributes: Some("bogus"), // unsupported, but mtime still applies
            ..Default::default()
        };
        let resolution = Resolution::default_for(CollisionStrategy::Skip);
        let result = execute_plan(plan, &config, &mut options, &resolution);

        assert_eq!(
            result.successful_ops.len(),
            1,
            "the timestamp effect makes the op real work"
        );
        assert!(result.skipped_ops.is_empty());
    }

    /// The names the plan reports must be exactly the names execution applies:
    /// the preview derives its rows from the same plan, and collision detection
    /// and execution share the one plan object built for the batch.
    #[test]
    fn test_plan_ops_equal_execution_results() {
        let dir = TempDir::new().unwrap();
        let files = vec![file(dir.path(), "a.txt"), file(dir.path(), "b.txt")];
        let config = RenameConfig {
            replace: crate::ReplaceSection {
                replace: Some("a".into()),
                with: Some("z".into()),
                ..Default::default()
            },
            add: crate::AddSection {
                add_prefix: Some("p_".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let opts = PlanOptions {
            parallel: true,
            ..Default::default()
        };

        let planned: Vec<(String, String)> = plan_renames(&files, &config, &opts)
            .ops()
            .iter()
            .map(|o| (o.original_name.clone(), o.new_name.clone()))
            .collect();

        // Re-plan for execution (execute_plan consumes the plan).
        let plan = plan_renames(&files, &config, &opts);
        let mut options = RenameOptions::default();
        let result = execute_plan(
            plan,
            &config,
            &mut options,
            &Resolution::default_for(CollisionStrategy::Skip),
        );
        let executed: Vec<(String, String)> = result
            .successful_ops
            .iter()
            .map(|o| (o.original_name.clone(), o.new_name.clone()))
            .collect();

        assert_eq!(planned, executed);
    }

    /// The output-dir transform is applied at planning time, so execution never
    /// re-transforms (and undo/collision/preview all see the final path).
    #[test]
    fn test_plan_applies_output_dir_transform() {
        let dir = TempDir::new().unwrap();
        let files = vec![file(dir.path(), "a.txt")];
        let out = dir.path().join("out");
        let config = RenameConfig::default();
        let opts = PlanOptions {
            output_dir: Some(out.as_path()),
            ..Default::default()
        };
        let plan = plan_renames(&files, &config, &opts);
        assert_eq!(plan.ops()[0].new_path, out.join("a.txt"));
    }

    /// A Copy / Move to Location plan is flagged, and every op in it is a location
    /// transfer. In-place renames — including a renamed subdirectory and its
    /// renamed children — are not transfers, so they stay undoable.
    #[test]
    fn test_location_transfer_classification() {
        let dir = TempDir::new().unwrap();
        let out = dir.path().join("out");

        // Output location: flagged, every op relocated.
        let files = vec![file(dir.path(), "a.txt")];
        let relocated = plan_renames(
            &files,
            &RenameConfig::default(),
            &PlanOptions {
                output_dir: Some(out.as_path()),
                ..Default::default()
            },
        );
        assert!(relocated.uses_output_location());
        assert!(relocated.ops().iter().all(|op| op.is_location_transfer()));
        assert!(
            relocated.ops().iter().all(|op| op.relocated),
            "the transform records relocation on each op"
        );
        assert!(
            !relocated.ops().iter().any(|op| op.is_undoable()),
            "location transfers are not undoable"
        );

        // A subdirectory and its child renamed in place: not flagged, no transfers.
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("a.txt"), b"x").unwrap();
        let nested = vec![
            sub.join("a.txt").to_string_lossy().to_string(),
            sub.to_string_lossy().to_string(),
        ];
        let config = RenameConfig {
            add: crate::AddSection {
                add_prefix: Some("x_".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let inplace = plan_renames(&nested, &config, &PlanOptions::default());
        assert!(!inplace.uses_output_location());
        assert_eq!(inplace.ops().len(), 2);
        assert!(
            inplace.ops().iter().all(|op| !op.is_location_transfer()),
            "in-place subdir/file renames are not location transfers"
        );
        assert!(
            inplace.ops().iter().all(|op| op.is_undoable()),
            "in-place renames stay undoable"
        );
    }

    /// A plan contains only actionable renames: an unchanged name is omitted
    /// unless the output-dir transform would still move the file. Numbering
    /// changes the name, so it is planned normally.
    #[test]
    fn test_plan_omits_unchanged_files() {
        use crate::NumberingMode;
        let dir = TempDir::new().unwrap();
        let files = vec![file(dir.path(), "a.txt")];

        let plan = plan_renames(&files, &RenameConfig::default(), &PlanOptions::default());
        assert!(
            plan.is_empty(),
            "unchanged file is not an actionable rename"
        );

        let numbered = RenameConfig {
            numbering: crate::NumberingSection {
                numbering_mode: Some(NumberingMode::Prefix),
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = plan_renames(&files, &numbered, &PlanOptions::default());
        assert_eq!(plan.ops()[0].new_name, "1a.txt");
    }

    /// `numbering_break` groups by counted position, so a prefix running before
    /// numbering does not affect where the sequence restarts.
    #[test]
    fn test_numbering_break_groups_even_with_prefix() {
        use crate::NumberingMode;
        let dir = TempDir::new().unwrap();
        let files = vec![
            file(dir.path(), "a1.txt"),
            file(dir.path(), "a2.txt"),
            file(dir.path(), "b1.txt"),
        ];
        let config = RenameConfig {
            add: crate::AddSection {
                add_prefix: Some("p_".into()),
                ..Default::default()
            },
            numbering: crate::NumberingSection {
                numbering_mode: Some(NumberingMode::Prefix),
                numbering_start: 1,
                numbering_increment: 1,
                numbering_sep: Some("_".into()),
                numbering_break: Some(2),
                ..Default::default()
            },
            ..Default::default()
        };
        let opts = PlanOptions::default();

        let plan = plan_renames(&files, &config, &opts);
        let names: Vec<&str> = plan.ops().iter().map(|o| o.new_name.as_str()).collect();
        assert_eq!(names, ["1_p_a1.txt", "2_p_a2.txt", "1_p_b1.txt"]);
    }

    /// A rename that does not change the target path is never planned and never
    /// executed. Reachable only via degenerate numbering (roman `0` prefixes
    /// nothing), and defensively for externally built plans.
    #[test]
    fn test_noop_rename_is_never_planned_or_executed() {
        use crate::NumberingMode;
        let dir = TempDir::new().unwrap();
        let files = vec![file(dir.path(), "a.txt")];
        let config = RenameConfig {
            numbering: crate::NumberingSection {
                numbering_mode: Some(NumberingMode::Prefix),
                numbering_start: 0,
                numbering_increment: 1,
                numbering_type: Some("roman".into()),
                numbering_sep: None,
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = plan_renames(&files, &config, &PlanOptions::default());
        assert!(plan.is_empty(), "identity rename is not in the plan");

        // An externally built no-op (e.g. `from_ops`) is dropped at execution too.
        let path = dir.path().join("a.txt");
        let op = RenameOp {
            original_path: path.clone(),
            new_path: path.clone(),
            original_name: "a.txt".into(),
            new_name: "a.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0u8,
        };
        let plan = RenamePlan::from_ops(vec![op]);
        let mut options = RenameOptions::default();
        let result = execute_plan(
            plan,
            &config,
            &mut options,
            &Resolution::default_for(CollisionStrategy::Skip),
        );
        assert_eq!(
            result.successful_ops.len() + result.failed_ops.len() + result.skipped_ops.len(),
            0,
            "a no-op is not executed"
        );
        assert!(path.exists(), "source name is preserved");
    }

    /// An output dir that equals the source dir leaves the path unchanged, so
    /// the op is a no-op and dropped after the transform.
    #[test]
    fn test_output_dir_equal_to_source_is_dropped() {
        let dir = TempDir::new().unwrap();
        let files = vec![file(dir.path(), "a.txt")];
        let opts = PlanOptions {
            output_dir: Some(dir.path()),
            ..Default::default()
        };
        let plan = plan_renames(&files, &RenameConfig::default(), &opts);
        assert!(plan.is_empty(), "moving into the same directory is a no-op");
    }

    /// A case-only rename changes the path bytes and must stay actionable:
    /// `is_noop` uses exact equality, not case-insensitive `is_same_file`.
    #[test]
    fn test_case_only_rename_is_actionable() {
        let dir = TempDir::new().unwrap();
        let files = vec![file(dir.path(), "a.txt")];
        let config = RenameConfig {
            case: crate::CaseSection {
                case_name: Some(crate::CaseMode::Upper),
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = plan_renames(&files, &config, &PlanOptions::default());
        assert_eq!(plan.len(), 1);
        assert!(!plan.ops()[0].is_noop());
        assert_eq!(plan.ops()[0].new_name, "A.txt");
    }

    /// Attributes make an otherwise unchanged file actionable: the op is kept
    /// and execution applies the attribute without renaming anything.
    #[test]
    fn test_unchanged_name_still_applies_attributes() {
        let dir = TempDir::new().unwrap();
        let path = PathBuf::from(file(dir.path(), "a.txt"));
        let config = RenameConfig {
            special: crate::SpecialSection {
                set_attributes: Some("readonly".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = plan_renames(
            &[path.to_string_lossy().to_string()],
            &config,
            &PlanOptions::default(),
        );
        assert_eq!(plan.len(), 1, "effects make an unchanged name actionable");

        let mut options = RenameOptions {
            set_attributes: Some("readonly"),
            ..Default::default()
        };
        let result = execute_plan(
            plan,
            &config,
            &mut options,
            &Resolution::default_for(CollisionStrategy::Skip),
        );
        assert_eq!(result.successful_ops.len(), 1);
        assert!(std::fs::metadata(&path).unwrap().permissions().readonly());
        assert!(path.exists(), "the name is unchanged");
    }

    /// Timestamps likewise apply without a rename.
    #[test]
    fn test_unchanged_name_still_applies_timestamps() {
        let dir = TempDir::new().unwrap();
        let path = PathBuf::from(file(dir.path(), "a.txt"));
        let fixed = || crate::TimestampSpec::Fixed("2020-01-02 03:04:05".into());
        let config = RenameConfig {
            special: crate::SpecialSection {
                set_modified: Some(fixed()),
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = plan_renames(
            &[path.to_string_lossy().to_string()],
            &config,
            &PlanOptions::default(),
        );
        assert_eq!(plan.len(), 1);

        let mut options = RenameOptions::default();
        let result = execute_plan(
            plan,
            &config,
            &mut options,
            &Resolution::default_for(CollisionStrategy::Skip),
        );
        assert_eq!(result.successful_ops.len(), 1);
        let got = std::fs::metadata(&path).unwrap().modified().unwrap();
        let expected = fixed().resolve(&path, "modified").unwrap();
        let secs =
            |t: std::time::SystemTime| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        assert!(
            secs(got).abs_diff(secs(expected)) <= 1,
            "mtime set to the fixed value"
        );
    }

    /// A mixed batch keeps its alignment: only changed files are renamed, but
    /// effects apply to every selected file (changed and unchanged alike).
    #[test]
    fn test_mixed_batch_renames_changed_and_applies_effects_to_all() {
        let dir = TempDir::new().unwrap();
        let a1 = file(dir.path(), "a1.txt");
        let b = file(dir.path(), "b.txt");
        let config = RenameConfig {
            replace: crate::ReplaceSection {
                replace: Some("a".into()),
                with: Some("z".into()),
                ..Default::default()
            },
            special: crate::SpecialSection {
                set_attributes: Some("readonly".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = plan_renames(&[a1.clone(), b.clone()], &config, &PlanOptions::default());
        let names: Vec<&str> = plan.ops().iter().map(|o| o.new_name.as_str()).collect();
        assert_eq!(
            names,
            ["z1.txt", "b.txt"],
            "order preserved, unchanged kept"
        );

        let mut options = RenameOptions {
            set_attributes: Some("readonly"),
            ..Default::default()
        };
        let result = execute_plan(
            plan,
            &config,
            &mut options,
            &Resolution::default_for(CollisionStrategy::Skip),
        );
        assert_eq!(result.successful_ops.len(), 2);
        let z1 = dir.path().join("z1.txt");
        let b = dir.path().join("b.txt");
        assert!(z1.exists() && b.exists());
        assert!(std::fs::metadata(&z1).unwrap().permissions().readonly());
        assert!(std::fs::metadata(&b).unwrap().permissions().readonly());
    }
}
