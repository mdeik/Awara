use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::case::{
    CaseSensitivity, case_sensitivity_for, is_same_file, norm_path_for_collision,
    rename_case_insensitive,
};
use crate::core::config::{RenameConfig, TimestampSpec, config_has_numbering, numbering_sequence};

// Re-export filetime for CLI usage
pub use filetime::FileTime;

mod attributes; // file attributes and timestamps
mod collisions; // target-path and collision handling
mod numbering; // numbering sequence planning (order, restart, break)
mod types; // shared data types
mod undo; // undo and rollback

// Re-exported with the same visibility as before, so callers keep the
// same names.
pub use attributes::{
    TimestampCache, apply_timestamps_to_file, resolve_timestamp_for_file, set_file_attributes,
};
pub(crate) use collisions::{
    TargetRelation, batch_source_set, op_is_copy, replace_final_component, target_key,
    target_relation,
};
pub use collisions::{
    apply_output_dir, common_base_dir, detect_collisions, detect_collisions_with,
    is_existing_dir_target, normalize_target_name, resolve_output_dir,
};
pub(crate) use numbering::numbering_indices;
pub use types::{
    CancellationToken, Collision, CollisionReason, CollisionStrategy, OpBatchResult, PermSnapshot,
    RenameEvent, RenameOp, RenameOptions, RenameResult, TS_ACCESSED, TS_CREATED, TS_MODIFIED,
    cancel_token,
};
pub(crate) use undo::remove_copied_entry;
pub use undo::{
    apply_ops_chain_safe, apply_undo, invert_op, reapply_metadata, reapply_metadata_cached,
};

// Internal helpers shared between the submodules above.
// `trim_windows_trailing` is only used by tests, so its re-export is test-only.
#[cfg(test)]
pub(crate) use collisions::trim_windows_trailing;
pub(crate) use undo::park_at_temp;

#[cfg(test)]
mod tests;
