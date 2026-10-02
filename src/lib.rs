mod core;

// Re-export everything from the core module
pub use core::*;

use std::path::{Path, PathBuf};

// ──────────────────────────────────────────────────────────
// Standalone utilities that don't fit neatly into submodules
// ──────────────────────────────────────────────────────────

/// Normalize a path by resolving `.` and `..` components lexically
/// WITHOUT following symlinks and WITHOUT anchoring relative paths to the
/// current directory. `..` that would rise above the root of a relative
/// path is dropped.
pub fn normalize_path_lexically(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut components: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                components.pop();
            }
            other => {
                components.push(other.as_os_str().to_os_string());
            }
        }
    }

    let mut result = PathBuf::new();
    for component in components {
        result.push(component);
    }
    result
}

/// Normalize a path by resolving `.` and `..` components lexically
/// WITHOUT following symlinks. Relative paths are first anchored to the
/// current directory so the result is absolute.
pub fn normalize_path_dedup(path: &Path) -> PathBuf {
    let path = if path.is_relative() {
        std::env::current_dir().unwrap_or_default().join(path)
    } else {
        path.to_path_buf()
    };
    normalize_path_lexically(&path)
}

/// Calculate a rename operation for a single file path.
pub fn calculate_rename(
    file_path_str: &str,
    compiled: &CompiledConfig,
    index: Option<usize>,
    include_unchanged: bool,
    dirname_level: Option<usize>,
) -> Option<RenameOp> {
    let ctx = crate::core::rename::FileContext::from_path(file_path_str, compiled, dirname_level)?;
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

#[cfg(test)]
mod folder_tests;
#[cfg(test)]
mod platform_tests;
#[cfg(test)]
mod recursive_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_tests;
