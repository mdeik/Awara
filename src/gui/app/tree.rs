use super::*;

pub(crate) fn maybe_shrink<T>(v: &mut Vec<T>) {
    if v.capacity() > 16384 && v.len() * 10 < v.capacity() {
        v.shrink_to_fit();
    }
}

/// Sort tree children with the same natural (numeric-aware) comparison the
/// main file table uses (`nat_cmp` on the file-name component), so the tree
/// navigator and the preview agree on ordering (`b2` before `b10`).
/// Children without a file name (Windows drive roots) all compare equal and
/// keep their existing insertion order via the stable sort.
pub(crate) fn sort_tree_children(children: &mut [PathBuf]) {
    children.sort_by(|a, b| {
        let an = a
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        let bn = b
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        nat_cmp(&an, &bn)
    });
}

/// Walk up from `path` and return the first ancestor that still exists on
/// disk (falling back to the tree root, which always exists). Used to pick a
/// tree scroll target when the cwd was deleted externally.
pub(crate) fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut p = path;
    while !p.exists() {
        match p.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => p = parent,
            _ => return tree_root(),
        }
    }
    p.to_path_buf()
}

pub(crate) fn scan_tree_children(dir: &Path, show_hidden: bool) -> Vec<PathBuf> {
    // Empty path = virtual root ("This PC" on Windows, "/" on Unix)
    if dir.as_os_str().is_empty() {
        let mut dirs = get_drive_roots();
        sort_tree_children(&mut dirs);
        return dirs;
    }

    let mut dirs = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            // Skip symlinks to directories to prevent infinite recursion in
            // the tree navigator (a symlink pointing to a parent or itself
            // would create an infinitely deep tree).
            // Symlink → regular file is fine (tested as a file, not a dir).
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            // entry.metadata() follows symlinks; entry.path().is_symlink()
            // checks if the entry itself is a symlink regardless of target.
            if path.is_symlink() {
                continue;
            }
            if meta.is_dir() && (show_hidden || !is_hidden(&path)) {
                dirs.push(path);
            }
        }
    }
    sort_tree_children(&mut dirs);
    dirs
}

/// Cheap "does this directory contain any visible subdirectories?" probe used
/// by the tree navigator to hide the expand arrow on leaf folders. Unlike
/// `scan_tree_children` it stops at the first hit and never sorts, so it costs
/// O(1) on typical folders (readdir order finds a subdirectory quickly) and at
/// worst one full listing for folders with no subdirectories at all. Results
/// are cached in `tree_has_subdirs`.
pub(crate) fn dir_has_subdirs(dir: &Path, show_hidden: bool) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in rd.flatten() {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        // Skip symlinks (may point at a parent/self, which the tree never
        // shows) — mirrors scan_tree_children. file_type() does not follow
        // symlinks, so this is a single cheap call.
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() && (show_hidden || !is_hidden(&entry.path())) {
            return true;
        }
    }
    false
}
