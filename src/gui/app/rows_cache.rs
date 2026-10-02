use super::*;

#[derive(Clone, PartialEq)]
pub(crate) struct RowsDatasetFp {
    pub(crate) preview_generation: u64,
    pub(crate) scan_generation: u64,
    pub(crate) processed_generation: u64,
    /// Bumped by every full (non-selection-only) row-cache rebuild so an
    /// in-place file mutation (e.g. an F2 rename that keeps the file count)
    /// invalidates the incremental base too.
    pub(crate) rows_dataset_gen: u64,
    pub(crate) file_count: usize,
    /// Cleared working config (disabled sections stripped) rows are built from.
    pub(crate) working: RenameConfig,
}

/// Data sent to the background row-cache thread.
pub(crate) struct RowsCacheRequest {
    pub(crate) r#gen: u64,
    pub(crate) files: Vec<String>,
    pub(crate) sizes: Vec<u64>,
    pub(crate) dates: Vec<i64>,
    pub(crate) is_dirs: Vec<bool>,
    pub(crate) preview: Arc<Vec<PreviewItem>>,
    pub(crate) status_map: HashMap<String, (u8, Option<String>)>,
    /// Config and selection for incremental rename computation.
    /// When the preview has not caught up yet (rows carried over after a
    /// selection change), items that are selected but have an identity op need
    /// their rename computed.
    pub(crate) selection: Vec<bool>,
    pub(crate) config: RenameConfig,
    pub(crate) section_enabled: SectionEnabled,
    /// True when the batch was processed by an Apply: every row renders its
    /// plain current name (no rename, no diff) regardless of preview ops.
    pub(crate) processed: bool,
    /// True when this request is a pure selection change: the worker may
    /// carry rows over from its previous completed build (verified against
    /// `fp`) and only recompute rows whose selection state flipped.
    pub(crate) selection_only: bool,
    /// Dataset fingerprint — must match the worker's stored base for the
    /// `selection_only` fast path to apply.
    pub(crate) fp: RowsDatasetFp,
    /// Cancel flag — set by the background thread when a newer request
    /// arrives.  `build_rows_cache` checks this between chunks so the thread
    /// can abandon stale work for a prior directory without processing all
    /// 161k items first.
    pub(crate) cancel: Arc<AtomicBool>,
}

/// Build a row cache from the source data — runs on the background thread.
/// Extracted so both async (thread) and sync (inline fallback) paths share the logic.
///
/// `base` is the worker's previous completed build (rows, the selection they
/// were built from, and the dataset fingerprint). When the request is a pure
/// selection change (`selection_only`) and the fingerprint matches, only rows
/// whose selection state flipped are recomputed; the rest are carried over.
pub(crate) fn build_rows_cache(
    request: &RowsCacheRequest,
    base: Option<(&[Row], &[bool], &RowsDatasetFp)>,
) -> Vec<Row> {
    let preview_by_idx: HashMap<usize, &PreviewItem> =
        request.preview.iter().map(|pr| (pr.index, pr)).collect();

    // Build compiled config once for incremental rename computation.
    // `calculate_rename` is called with no index below, so numbering is skipped;
    // this lazy path only runs when numbering is inactive.
    let working = cleared_working_config(&request.config, &request.section_enabled);
    let compiled = CompiledConfig::from_ref(&working);
    let dir_level = Some(working.append_folder.dirname_level);

    // Structural front/end diff context: `None` when the config can change the
    // middle of names (rows then fall back to the lazy LCS diff at render time).
    let diff_ctx = FrontEndDiff::from_config(&working);

    let n = request.files.len();
    let mut result_slots: Vec<Option<Row>> = Vec::with_capacity(n);
    result_slots.resize_with(n, || None);

    // A pure selection change can carry rows over from the previous build
    // when the dataset fingerprint matches and the base is index-aligned
    // with the current file list (rows are keyed by file index).
    let incremental = request.selection_only
        && base
            .is_some_and(|(rows, sel, fp)| rows.len() == n && sel.len() == n && request.fp == *fp);

    // Process in chunks so we can check the cancel flag between chunks
    // and abandon stale work promptly (e.g. when the user navigates to
    // a new directory).
    const ROW_CHUNK: usize = 2000;
    let mut chunk_start = 0;
    while chunk_start < n {
        if request.cancel.load(Ordering::Relaxed) {
            return Vec::new();
        }

        let chunk_end = (chunk_start + ROW_CHUNK).min(n);

        let chunk_rows: Vec<(usize, Row)> = (chunk_start..chunk_end)
            .into_par_iter()
            .map(|i| {
                let row = if incremental {
                    let (base_rows, base_sel, _) = base.unwrap();
                    if request.selection[i] == base_sel[i] {
                        // Selection state unchanged — the row content cannot
                        // have changed (dataset fingerprint matches).
                        base_rows[i].clone()
                    } else {
                        build_row(request, i, &preview_by_idx, &compiled, dir_level, &diff_ctx)
                    }
                } else {
                    build_row(request, i, &preview_by_idx, &compiled, dir_level, &diff_ctx)
                };
                (i, row)
            })
            .collect();

        for (idx, row) in chunk_rows {
            result_slots[idx] = Some(row);
        }

        chunk_start = chunk_end;
    }

    // Flatten result_slots into a contiguous vec.
    let mut result: Vec<Row> = Vec::with_capacity(n);
    for row in result_slots.into_iter().flatten() {
        result.push(row);
    }
    result
}

/// Build a single row cache entry. Shared by the full rebuild and the
/// incremental selection-only path so both produce identical rows.
pub(crate) fn build_row(
    request: &RowsCacheRequest,
    i: usize,
    preview_by_idx: &HashMap<usize, &PreviewItem>,
    compiled: &CompiledConfig,
    dir_level: Option<usize>,
    diff_ctx: &Option<FrontEndDiff>,
) -> Row {
    let path_str = &request.files[i];
    let path = std::path::Path::new(path_str);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let is_dir = request.is_dirs.get(i).copied().unwrap_or(false);

    // Decide on the new name from preview or incremental compute.
    let is_selected = request.selection.get(i).copied().unwrap_or(true);
    let new_name: String;
    let changed: bool;

    if request.processed {
        // The batch was renamed by the last Apply — every row shows
        // its plain current name (no rename, no diff) until the user
        // reselects files or changes the operation.
        new_name = name.clone();
        changed = false;
    } else if let Some(pr) = preview_by_idx.get(&i) {
        if is_selected && !pr.is_changed() {
            // This item is selected but the preview carries no rename for it
            // (it was computed when the item was unselected). Compute the rename
            // now so the row cache shows the correct new name.
            //
            // This lazy recompute has no numbering index, so it is only valid
            // when numbering is inactive. With numbering active, selection
            // changes force a full preview recompute instead of a
            // selection-only row rebuild, so this branch must not run.
            debug_assert!(
                !awara::config_has_numbering(&compiled.config),
                "lazy row recompute must not run with active numbering"
            );
            match calculate_rename(path_str, compiled, None, true, dir_level) {
                // SSoT with the planner: a **path** no-op (`RenameOp::is_noop`,
                // which normalizes `.` segments and trailing separators) is the
                // same as "no rename" — the plan drops it, so the row must show
                // no diff either. Comparing names textually here made a row
                // disagree with the preview (`./a.txt`, `a.txt/`).
                // SSoT with the planner: a **path** no-op (`RenameOp::is_noop`,
                // which normalizes `.` segments and trailing separators) is the
                // same as "no rename" — the plan drops it, so the row shows no
                // diff either. Comparing names textually here made a row
                // disagree with the preview (`./a.txt`, `a.txt/`).
                Some(op) if !op.is_noop() => {
                    changed = op.new_name != name;
                    new_name = op.new_name;
                }
                // Unchanged (or a dropped path no-op): show the file's current
                // name — `name`, not a possibly-stale preview row (an in-place
                // rename leaves the preview row pointing at the old name).
                _ => {
                    new_name = name.clone();
                    changed = false;
                }
            }
        } else if !is_selected {
            // Deselected file: the preview row may still hold the rename
            // from when the file was selected (the preview is not
            // recomputed on selection changes without numbering — only
            // the row cache is rebuilt). Clear the preview so no diff
            // lingers on the deselected row.
            new_name = name.clone();
            changed = false;
        } else {
            new_name = pr.new_name().to_string();
            changed = pr.is_changed();
        }
    } else {
        new_name = name.clone();
        changed = false;
    }

    let entry = request.status_map.get(path_str);
    let status = entry.map(|(s, _)| *s);
    let error_msg = entry.and_then(|(_, e)| e.clone());
    // File is considered missing when it no longer exists on disk (deleted
    // outside the GUI). The row is kept so the user can see what disappeared,
    // but the render site will cross it out.
    let missing = !std::path::Path::new(path_str).exists();
    // Preserved extension (with leading dot) for the structural diff. Extension
    // identification is files-only: directories keep their whole name as the
    // stem. SSoT with the core rename pipeline via `awara::split_extension_for`.
    let ext_text = match awara::split_extension_for(&name, is_dir) {
        (_, Some(e)) => format!(".{}", e),
        (_, None) => String::new(),
    };
    // Precompute structural diff segments for front/end-only configs
    // (memoised in Row; otherwise fast diff_colored precomputed here in the worker).
    let (orig_segs, new_segs) = if changed {
        match diff_ctx
            .as_ref()
            .and_then(|d| d.diff_segs(&name, &new_name, &ext_text))
        {
            Some((o, n)) => (Some(o), Some(n)),
            None => {
                let (o, n) = diff_colored(&name, &new_name);
                (Some(o), Some(n))
            }
        }
    } else {
        (None, None)
    };

    Row {
        idx: i,
        name,
        is_dir,
        size: request.sizes.get(i).copied().unwrap_or(0),
        date: request.dates.get(i).copied().unwrap_or(0),
        new_name,
        changed,
        status,
        orig_segs,
        new_segs,
        error_msg,
        missing,
    }
}
