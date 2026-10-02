// TimestampSpec resolution & formatting utilities.
// The type and its basic methods (resolve, to_cli_str, from_cli_str) live in config.rs.
// File-system-aware resolution (resolve_timestamp_for_file, apply_timestamps_to_file)
// lives in execute.rs.
//
// This module exists as a logical home for future timestamp utilities.

// Re-export TimestampSpec so the module is not empty and pub use timestamp::* works.
pub use crate::core::config::TimestampSpec;
