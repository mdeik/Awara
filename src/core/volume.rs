//! Volume identity and ancestor resolution, shared by the filesystem probes.
//!
//! Both the case-sensitivity probe (`case.rs`) and the attribute-capability
//! probe (`attrs`) need the same primitive: "which volume does this path live
//! on, and what is the nearest existing directory on that same volume?".
//! Keeping one implementation here means a capability result is never borrowed
//! across a mount boundary, and mount-boundary rules live in exactly one place.

use std::fs;
use std::path::{Path, PathBuf};

/// Identity of the volume holding `path`, used to detect mount boundaries and
/// to key per-volume probe caches. Stable APIs only:
/// - Unix: the metadata device ID (`st_dev`) — a true volume identity.
/// - Windows: a hash of the root prefix (drive letter or UNC share). The
///   metadata volume serial (`volume_serial_number`/`file_index`) is unstable
///   (`windows_by_handle`), so it can't be used on stable. This is an
///   identity *heuristic*, not a true volume ID: it can't see mount points
///   inside a drive letter. That gap is covered by `is_reparse_point`
///   (junctions/mount points are treated as boundaries) in `case.rs`.
/// - Other: `None` (boundary detection and caching disabled).
pub(crate) fn volume_id(path: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(path).ok().map(|m| m.dev())
    }
    #[cfg(windows)]
    {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::path::Component;
        match path.components().next() {
            Some(Component::Prefix(p)) => {
                let mut h = DefaultHasher::new();
                p.as_os_str().hash(&mut h);
                Some(h.finish())
            }
            _ => None,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        None
    }
}

/// Walk up from `dir` to the nearest existing directory, never crossing a
/// device boundary. If the starting path itself doesn't exist, its nearest
/// existing ancestor is the volume where the path will be created, so it is
/// the correct probe location.
pub(crate) fn nearest_existing_same_device(dir: &Path) -> PathBuf {
    let mut cur = if dir.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        dir.to_path_buf()
    };

    let start_dev = volume_id(&cur);
    loop {
        match fs::metadata(&cur) {
            Ok(_) => {
                // Stop if we crossed onto a different volume — a mount
                // boundary (e.g. a read-only SMB/CD-ROM mount inside a
                // writable parent) must not borrow the parent's result.
                if let (Some(start), Some(here)) = (start_dev, volume_id(&cur))
                    && start != here
                {
                    return cur.parent().unwrap_or(Path::new(".")).to_path_buf();
                }
                return cur;
            }
            Err(_) => match cur.parent() {
                Some(parent) => {
                    // Relative paths like `foo.txt` have an empty parent that
                    // resolves against the CWD — where the path actually lives.
                    cur = if parent.as_os_str().is_empty() {
                        PathBuf::from(".")
                    } else {
                        parent.to_path_buf()
                    };
                }
                None => return cur,
            },
        }
    }
}
