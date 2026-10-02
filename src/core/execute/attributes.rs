use super::*;
use crate::core::attrs::{AttrOutcome, caps_for_write};

/// Set or clear file attributes on a path.
///
/// The mechanism behind each attribute is resolved for the path's volume by
/// [`caps_for_write`] (once; the result is cached per volume). Returned
/// `Renamed` outcomes update the effective path so callers can follow a
/// dot-prefix hide. Unsupported attributes and failed syscalls are a capability
/// fact of the volume, not a per-file error.
pub fn set_file_attributes(path: &Path, attrs: &str) -> PathBuf {
    let mut effective = path.to_path_buf();
    let caps = caps_for_write(&effective);
    for attr in attrs.split(',') {
        let attr = attr.trim();
        if attr.is_empty() {
            continue;
        }
        let (set_op, name) = if let Some(name) = attr.strip_prefix('+') {
            (true, name.trim())
        } else if let Some(name) = attr.strip_prefix('-') {
            (false, name.trim())
        } else {
            (true, attr)
        };
        if let AttrOutcome::Renamed(new_path) = caps.set(&effective, name, set_op) {
            effective = new_path;
        }
    }
    effective
}

/// Resolve a `TimestampSpec` for a given file.
///
/// Thin compatibility wrapper: [`TimestampSpec::resolve`] is the single
/// implementation (field-aware for `CopyFrom`/`Delta`).
pub fn resolve_timestamp_for_file(
    spec: &TimestampSpec,
    path: &Path,
    field_name: &str,
) -> Option<std::time::SystemTime> {
    spec.resolve(path, field_name)
}

/// A timestamp spec's value, precomputed where it is file-independent.
#[derive(Debug, Clone)]
enum Resolved {
    /// Constant for the whole batch: `Fixed` is parsed once (`None` if the
    /// stored value is invalid, meaning "no-op").
    Constant(Option<std::time::SystemTime>),
    /// Depends on the file (`CopyFrom`/`Taken`/`Delta`) or on call time
    /// (`Current`), so it is resolved per file as before.
    PerFile(TimestampSpec),
}

impl Resolved {
    fn precompute(spec: &TimestampSpec) -> Self {
        match spec {
            TimestampSpec::Fixed(val) => {
                Resolved::Constant(crate::core::rename::parse_local_datetime(val))
            }
            other => Resolved::PerFile(other.clone()),
        }
    }

    fn resolve(&self, path: &Path, field_name: &str) -> Option<std::time::SystemTime> {
        match self {
            Resolved::Constant(t) => *t,
            Resolved::PerFile(spec) => spec.resolve(path, field_name),
        }
    }
}

/// Precomputed timestamp state for one execution.
///
/// Resolving a `Fixed` spec requires parsing a string plus a `mktime`
/// conversion; doing that once per batch instead of once per file keeps large
/// renames cheap. `Current`/`CopyFrom`/`Taken`/`Delta` stay per-file.
#[derive(Debug, Clone)]
pub struct TimestampCache {
    incr_secs: u32,
    created: Option<Resolved>,
    modified: Option<Resolved>,
    accessed: Option<Resolved>,
}

impl TimestampCache {
    pub fn new(config: &RenameConfig) -> Self {
        Self {
            incr_secs: config.special.timestamp_incr_secs,
            created: config
                .special
                .set_created
                .as_ref()
                .map(Resolved::precompute),
            modified: config
                .special
                .set_modified
                .as_ref()
                .map(Resolved::precompute),
            accessed: config
                .special
                .set_accessed
                .as_ref()
                .map(Resolved::precompute),
        }
    }

    /// Apply the configured timestamps to `path`, adding
    /// `file_index * timestamp_incr_secs` to each instant.
    pub fn apply(&self, path: &Path, file_index: usize) {
        let incr = file_index as u64 * self.incr_secs as u64;
        let add_incr = |t: std::time::SystemTime| -> std::time::SystemTime {
            if incr > 0 {
                t + std::time::Duration::from_secs(incr)
            } else {
                t
            }
        };

        if let Some(r) = &self.created
            && let Some(t) = r.resolve(path, "created")
        {
            let t = add_incr(t);
            let _ = crate::core::platform::set_creation_time(path, t);
        }
        if let Some(r) = &self.modified
            && let Some(t) = r.resolve(path, "modified")
        {
            let t = add_incr(t);
            let _ = filetime::set_file_mtime(path, filetime::FileTime::from_system_time(t));
        }
        if let Some(r) = &self.accessed
            && let Some(t) = r.resolve(path, "accessed")
        {
            let t = add_incr(t);
            let _ = filetime::set_file_atime(path, filetime::FileTime::from_system_time(t));
        }
    }
}

/// Apply timestamp specs from a `RenameConfig` to a single file.
///
/// Convenience wrapper over [`TimestampCache`] for one-off calls. Batch callers
/// should build a [`TimestampCache`] once and reuse it.
pub fn apply_timestamps_to_file(path: &Path, config: &RenameConfig, file_index: usize) {
    TimestampCache::new(config).apply(path, file_index);
}
