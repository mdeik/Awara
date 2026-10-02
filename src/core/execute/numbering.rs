use super::*;

/// Compute the numbering sequence (0-based offsets) for `files` in `order`.
///
/// `numbering_break` restarts the sequence every N files; `participates`
/// selects which entries consume a number (e.g. the GUI's selection).
///
/// The result is aligned with `order`: `result[k]` is the offset for
/// `order[k]`, or `None` when the file does not participate (or numbering is
/// inactive).
pub(crate) fn numbering_indices(
    files: &[String],
    config: &RenameConfig,
    order: &[usize],
    participates: impl FnMut(usize) -> bool,
) -> Vec<Option<usize>> {
    if !config_has_numbering(config) {
        return vec![None; order.len()];
    }
    let break_every = config.numbering.numbering_break.filter(|&n| n > 0);
    numbering_sequence(
        order,
        files,
        config.numbering.numbering_restart_folder,
        break_every,
        participates,
    )
}
