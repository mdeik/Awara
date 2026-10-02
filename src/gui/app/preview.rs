use super::*;

pub(crate) struct PreviewParams<'a> {
    pub(crate) files: &'a [String],
    pub(crate) selection: &'a [bool],
    pub(crate) config: &'a RenameConfig,
    pub(crate) section_enabled: &'a SectionEnabled,
    pub(crate) num_order: &'a [usize],
    pub(crate) cwd: &'a Path,
    /// True when the batch was processed: selected items render identity ops
    /// (New Name = current filename) and are excluded from rename computation.
    pub(crate) processed: bool,
}

/// Core preview computation — shared by both the background thread and the
/// synchronous fallback (`compute_preview_sync`).
///
/// The preview *is* the plan: selected rows are exactly the ops that
/// [`awara::plan_renames`] computes. Only the two names each row displays are
/// kept — the plan itself is dropped here, because Apply plans its own (the
/// plan is not a reusable artifact: retaining one for every preview cost far more
/// memory than re-deriving it costs time). Unselected (and already-processed)
/// rows get identity rows.
///
/// `params.num_order` is the frozen display order; both this function and
/// `try_execute_renames` plan with it, so preview and execution cannot order
/// their input differently.
///
/// `cancel_flag` is checked between plan chunks; the caller sets it to `true`
/// to abandon superseded work. Returns `None` when cancelled.
pub(crate) fn compute_preview(
    params: PreviewParams,
    cancel_flag: &AtomicBool,
) -> Option<Vec<PreviewItem>> {
    let PreviewParams {
        files,
        selection,
        config,
        section_enabled,
        num_order,
        cwd,
        processed,
    } = params;

    // Working config with disabled sections cleared.
    let working = cleared_working_config(config, section_enabled);
    let total = files.len();

    // Selected, non-processed files in display order — the exact batch a plan
    // would rename.
    let mut selected_indices: Vec<usize> = Vec::new();
    let mut selected_files: Vec<String> = Vec::new();
    if !processed {
        for &i in num_order {
            if selection.get(i).copied().unwrap_or(true)
                && let Some(p) = files.get(i)
            {
                selected_indices.push(i);
                selected_files.push(p.clone());
            }
        }
    }

    let output_dir = working
        .copy_to
        .output_dir
        .as_deref()
        .map(|d| awara::resolve_output_dir(d, cwd));
    let base_dir = if working.copy_to.keep_structure {
        Some(cwd)
    } else {
        None
    };
    let opts = awara::PlanOptions {
        dirname_level: working.append_folder.dirname_level,
        output_dir: output_dir.as_deref(),
        keep_structure: working.copy_to.keep_structure,
        base_dir,
        parallel: true,
        copy_mode: output_dir.is_some() && working.copy_to.copy_mode,
    };
    let plan = awara::plan_renames_cancellable(&selected_files, &working, &opts, cancel_flag)?;

    // Rows borrow the plan's ops read-only and copy only the two names they
    // display. Nothing else of the plan survives this function.
    let plan_ops = plan.ops();

    let identity = |i: usize, sel: bool| -> PreviewItem {
        let name = Path::new(&files[i])
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        PreviewItem {
            original_name: name,
            new_name: None,
            selected: sel,
            index: i,
        }
    };

    let mut result_slots: Vec<Option<PreviewItem>> = Vec::with_capacity(total);
    result_slots.resize_with(total, || None);

    // Identity for unselected or processed rows.
    for (i, slot) in result_slots.iter_mut().enumerate() {
        let sel = selection.get(i).copied().unwrap_or(true);
        if processed || !sel {
            *slot = Some(identity(i, sel));
        }
    }

    // Plan ops for selected rows. A plan contains only actionable renames, so
    // unchanged files have no op; the planner preserves input order, so a single
    // forward pass over both lists places each op and synthesizes an identity
    // row for the files it skipped (including files that disappeared since scan).
    let mut cursor = 0usize;
    for &i in &selected_indices {
        let want = Path::new(files[i].as_str());
        let op = match plan_ops.get(cursor) {
            Some(op) if op.original_path.as_path() == want => {
                cursor += 1;
                Some(op)
            }
            _ => None,
        };
        result_slots[i] = Some(match op {
            Some(op) => PreviewItem {
                original_name: op.original_name.clone(),
                new_name: Some(op.new_name.clone()),
                selected: true,
                index: i,
            },
            None => identity(i, true),
        });
    }

    // Flatten result_slots into a contiguous vec in file-index order.
    let mut preview: Vec<PreviewItem> = Vec::with_capacity(total);
    for item in result_slots.into_iter().flatten() {
        preview.push(item);
    }
    Some(preview)
}
