//! Filesystem attribute capability — the single source of truth for how each
//! file attribute (`readonly`, `hidden`, `system`, `archive`) is read and
//! written on a given volume.
//!
//! Attribute support is a property of the *filesystem*, not the OS: a FAT or
//! SMB volume mounted on Linux can store DOS attribute bits, while a native
//! ext4/APFS volume cannot (it uses permission bits and the leading-dot
//! convention instead). The resolution chain is:
//!
//! ```text
//! classification (`classify`)  -> candidate mechanisms
//! write probe (`probe`)        -> confirms which candidates actually work
//! operation result             -> final authority for that operation
//! ```
//!
//! Classification is only a candidate selector. Reads never write: before any
//! write they use the classified preference, and once a write probe has
//! confirmed the volume, reads use that confirmed result too (the per-directory
//! read memo is invalidated on confirmation). The probe's memory result is a
//! hint, never a gate — callers must still report the real outcome of each
//! operation.
//!
//! Attribute *names* are a global schema concern (see `core::schema`); this
//! module only maps a name to the mechanism that implements it on a volume.

mod classify;
mod mechanism;
mod probe;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::core::memo::{BoundedMemo, MEMO_CAP};
use crate::core::schema::{
    ATTR_ARCHIVE, ATTR_HIDDEN, ATTR_READONLY, ATTR_SYSTEM, canonical_attribute_name,
};

pub(crate) use classify::AttrBackend;
pub use mechanism::AttrMechanism;

/// Result of attempting to apply one attribute to one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrOutcome {
    /// Applied in place.
    Applied,
    /// Applied by renaming; carries the new path callers must continue with.
    Renamed(PathBuf),
    /// The mechanism is unsupported on this volume (or this path).
    Unsupported,
    /// A syscall failed.
    Failed(String),
}

/// Resolved capability for one volume: the mechanism behind each attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttrCaps {
    pub readonly: AttrMechanism,
    pub hidden: AttrMechanism,
    pub system: AttrMechanism,
    pub archive: AttrMechanism,
    /// Whether a settable creation (birth) time is available.
    pub birth_time: bool,
    /// True when the write probe confirmed the mechanisms; false when the
    /// volume was only classified (no writable place to probe).
    pub confirmed: bool,
}

impl AttrCaps {
    /// Mechanism implementing `attr` on this volume, or `None` for an unknown
    /// attribute name. An unsupported-but-known attribute yields
    /// `Some(AttrMechanism::None)`.
    pub fn mechanism(&self, attr: &str) -> Option<AttrMechanism> {
        Some(match canonical_attribute_name(attr)? {
            ATTR_READONLY => self.readonly,
            ATTR_HIDDEN => self.hidden,
            ATTR_SYSTEM => self.system,
            ATTR_ARCHIVE => self.archive,
            _ => return Option::None,
        })
    }

    /// Whether `attr` can be applied on this volume.
    pub fn supports(&self, attr: &str) -> bool {
        matches!(self.mechanism(attr), Some(m) if m != AttrMechanism::None)
    }

    /// Whether `attr` is backed by an in-place metadata flag on this volume
    /// (as opposed to a rename or a permission bit).
    pub fn inplace_metadata(&self, attr: &str) -> bool {
        matches!(self.mechanism(attr), Some(m) if m.is_inplace_metadata())
    }

    /// Read `attr` for `path`, if supported and determinable.
    pub fn get(&self, path: &Path, attr: &str) -> Option<bool> {
        self.mechanism(attr)?.get(path, None)
    }

    /// Apply `attr` to `path`.
    pub fn set(&self, path: &Path, attr: &str, on: bool) -> AttrOutcome {
        match self.mechanism(attr) {
            Some(m) => m.set(path, on),
            Option::None => AttrOutcome::Unsupported,
        }
    }

    /// Read `attr` through an injected syscall layer (simulation tests).
    #[cfg(test)]
    pub(crate) fn get_with(&self, fs: &dyn Fs, path: &Path, attr: &str) -> Option<bool> {
        self.mechanism(attr).and_then(|m| fs.get(m, path))
    }

    /// Apply `attr` through an injected syscall layer (simulation tests).
    #[cfg(test)]
    pub(crate) fn set_with(&self, fs: &dyn Fs, path: &Path, attr: &str, on: bool) -> AttrOutcome {
        match self.mechanism(attr) {
            Some(m) => fs.set(m, path, on),
            Option::None => AttrOutcome::Unsupported,
        }
    }

    fn confirmed(mut self) -> Self {
        self.confirmed = true;
        self
    }
}

/// The syscall layer behind classification and the capability probe.
///
/// Injectable so tests can simulate a filesystem (FAT, SMB, read-only, a driver
/// that lies about persisting) without mounting one. Production always uses
/// [`RealFs`]. This is an *implementation* seam, not a second authority: the
/// policy (which mechanism, and whether it is confirmed) stays in `Candidates`.
pub(crate) trait Fs {
    /// Candidate backend for the volume holding `path`.
    fn backend(&self, path: &Path) -> AttrBackend;
    /// Read one attribute via `mechanism`.
    fn get(&self, mechanism: AttrMechanism, path: &Path) -> Option<bool>;
    /// Apply one attribute via `mechanism`.
    fn set(&self, mechanism: AttrMechanism, path: &Path, on: bool) -> AttrOutcome;
    /// A path the probe may mutate freely and then discard, or `None` when the
    /// volume has no writable place to probe.
    fn probe_target(&self, dir: &Path) -> Option<PathBuf>;
    /// Discard a probe target created by [`Fs::probe_target`].
    fn cleanup_probe(&self, path: &Path);
}

/// Real filesystem syscalls.
pub(crate) struct RealFs;

impl Fs for RealFs {
    fn backend(&self, path: &Path) -> AttrBackend {
        classify::backend_for(path)
    }

    fn get(&self, mechanism: AttrMechanism, path: &Path) -> Option<bool> {
        mechanism.get(path, None)
    }

    fn set(&self, mechanism: AttrMechanism, path: &Path, on: bool) -> AttrOutcome {
        mechanism.set(path, on)
    }

    fn probe_target(&self, dir: &Path) -> Option<PathBuf> {
        probe::create_probe_file(dir)
    }

    fn cleanup_probe(&self, path: &Path) {
        let _ = std::fs::remove_file(path);
    }
}

/// Whether `path` is hidden by any convention the volume can expose: the Unix
/// leading-dot rule (on Unix) or the volume's in-place hidden mechanism (a DOS
/// hidden bit or macOS `UF_HIDDEN`). This never writes.
pub fn is_hidden(path: &Path) -> bool {
    #[cfg(unix)]
    if mechanism::is_dot_prefixed(path) {
        return true;
    }
    matches!(caps_for_read(path).get(path, ATTR_HIDDEN), Some(true))
}

/// The attributes in `attrs` (comma-separated `[+|-]name` tokens) that this
/// volume cannot apply. Unknown names are reported as unsupported.
pub fn unsupported_in(caps: &AttrCaps, attrs: &str) -> Vec<String> {
    attrs
        .split(',')
        .filter_map(|token| {
            let raw = token.trim().trim_start_matches(['+', '-']).trim();
            if raw.is_empty() {
                return None;
            }
            match canonical_attribute_name(raw) {
                Some(name) if caps.supports(name) => None,
                Some(name) => Some(name.to_string()),
                None => Some(raw.to_ascii_lowercase()),
            }
        })
        .collect()
}

/// Per-volume resolved capabilities, keyed by [`volume::volume_id`].
static CACHE: OnceLock<Mutex<HashMap<u64, AttrCaps>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<u64, AttrCaps>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached(key: u64) -> Option<AttrCaps> {
    cache().lock().unwrap().get(&key).copied()
}

/// Directory-keyed capability memo. `is_hidden` runs once per scanned entry and
/// attribute application once per file; a volume lookup walks/stats the path, so
/// resolving it per entry would be wasteful. Bounded (eldest evicted): a dropped
/// entry simply re-resolves. Confirming a volume invalidates the memo so reads
/// afterwards observe the confirmed mechanisms. This is a pure memo, not an
/// authority — the authority is [`AttrCaps`] combined with each operation's real
/// result.
static DIR_CACHE: BoundedMemo<PathBuf, AttrCaps, MEMO_CAP> = BoundedMemo::new();

fn dir_key(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Capability for *reading* attributes on the volume holding `path`.
///
/// Never writes and therefore never probes; safe to call from scanning and
/// listing paths. Memoized per containing directory. Directories that don't
/// exist yet resolve through their nearest existing ancestor on the same volume.
pub fn caps_for_read(path: &Path) -> AttrCaps {
    let dir = dir_key(path);
    if let Some(caps) = DIR_CACHE.lock().get(&dir) {
        return caps;
    }
    let anchor = crate::core::volume::nearest_existing_same_device(path);
    let classified = caps_for_read_anchor(&anchor, &RealFs);
    let confirmed = crate::core::volume::volume_id(&anchor).and_then(cached);
    let caps = read_caps_from_classification(classified, confirmed);
    DIR_CACHE.lock().insert(dir, caps);
    caps
}

/// A read must never lag a confirmed write: prefer the volume's confirmed
/// result over a fresh classification.
fn read_caps_from_classification(classified: AttrCaps, confirmed: Option<AttrCaps>) -> AttrCaps {
    match confirmed {
        Some(caps) if caps.confirmed => caps,
        _ => classified,
    }
}

/// Read capability for an already-resolved anchor directory.
pub(crate) fn caps_for_read_anchor(anchor: &Path, fs: &dyn Fs) -> AttrCaps {
    probe::candidates_for(fs.backend(anchor)).preferred()
}

/// Read capability using an injected syscall layer (no caching).
#[cfg(test)]
pub(crate) fn caps_for_read_with(path: &Path, fs: &dyn Fs) -> AttrCaps {
    let anchor = crate::core::volume::nearest_existing_same_device(path);
    caps_for_read_anchor(&anchor, fs)
}

/// Capability for *writing* attributes on the volume holding `path`.
///
/// May run the one-time per-volume probe, which writes (and removes) a
/// throwaway file. Resolved results are cached per directory (and, for probing,
/// per volume); subsequent calls for the same directory are a single lookup.
pub fn caps_for_write(path: &Path) -> AttrCaps {
    let dir = dir_key(path);
    if let Some(caps) = DIR_CACHE.lock().get(&dir)
        && caps.confirmed
    {
        return caps;
    }

    let anchor = crate::core::volume::nearest_existing_same_device(path);
    let key = crate::core::volume::volume_id(&anchor);

    // A probe for this volume already ran (possibly from another directory).
    if let Some(k) = key
        && let Some(caps) = cached(k)
        && caps.confirmed
    {
        DIR_CACHE.lock().insert(dir, caps);
        return caps;
    }

    // Probes are idempotent and residue cleanup is age-guarded, so concurrent
    // probes need no global lock: a slow or hung volume cannot stall attribute
    // writes elsewhere in the process.
    let caps = caps_for_write_anchor(&anchor, &RealFs);
    if let Some(k) = key {
        cache().lock().unwrap().insert(k, caps);
    }
    if caps.confirmed {
        // Classified (unconfirmed) read entries on this volume are now stale;
        // drop them so subsequent reads observe the confirmed mechanisms.
        DIR_CACHE.lock().clear();
    }
    DIR_CACHE.lock().insert(dir, caps);
    caps
}

/// Write capability for an already-resolved anchor directory, using an
/// injected syscall layer (no caching).
pub(crate) fn caps_for_write_anchor(anchor: &Path, fs: &dyn Fs) -> AttrCaps {
    let candidates = probe::candidates_for(fs.backend(anchor));
    if candidates.needs_probe() {
        candidates.resolve_with(anchor, fs)
    } else {
        candidates.preferred().confirmed()
    }
}

/// Write capability using an injected syscall layer (no caching).
#[cfg(test)]
pub(crate) fn caps_for_write_with(path: &Path, fs: &dyn Fs) -> AttrCaps {
    let anchor = crate::core::volume::nearest_existing_same_device(path);
    caps_for_write_anchor(&anchor, fs)
}
