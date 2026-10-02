//! Candidate mechanisms and the one-time per-volume capability probe.
//!
//! Classification yields an ordered candidate chain per attribute. A write
//! resolves the chain by applying each candidate to a throwaway file and
//! reading the value back; the first candidate that verifies wins. Self-evident
//! mechanisms (`PermBits`, `DotPrefix`, `None`) are accepted without probing,
//! and act as the final fallback in a chain.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::AttrCaps;
use super::AttrOutcome;
use super::Fs;
use super::classify::AttrBackend;
use super::mechanism::{
    AttrMechanism, FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM,
};

/// Ordered candidate mechanisms for one backend. Order matters: the first
/// entry is the preferred mechanism, later entries are fallbacks.
pub(crate) struct Candidates {
    pub readonly: AttrMechanism,
    pub hidden: &'static [AttrMechanism],
    pub system: &'static [AttrMechanism],
    pub archive: &'static [AttrMechanism],
    pub birth_time: bool,
}

const NO_ATTR: &[AttrMechanism] = &[AttrMechanism::None];

pub(crate) fn candidates_for(backend: AttrBackend) -> Candidates {
    match backend {
        AttrBackend::Win32 => Candidates {
            readonly: AttrMechanism::PermBits,
            hidden: &[AttrMechanism::DosBit(FILE_ATTRIBUTE_HIDDEN)],
            system: &[AttrMechanism::DosBit(FILE_ATTRIBUTE_SYSTEM)],
            archive: &[AttrMechanism::DosBit(FILE_ATTRIBUTE_ARCHIVE)],
            birth_time: true,
        },
        AttrBackend::DosFat => Candidates {
            readonly: AttrMechanism::PermBits,
            hidden: &[
                AttrMechanism::DosBit(FILE_ATTRIBUTE_HIDDEN),
                AttrMechanism::DotPrefix,
            ],
            system: &[AttrMechanism::DosBit(FILE_ATTRIBUTE_SYSTEM)],
            archive: &[AttrMechanism::DosBit(FILE_ATTRIBUTE_ARCHIVE)],
            birth_time: false,
        },
        AttrBackend::DosXattr => Candidates {
            readonly: AttrMechanism::PermBits,
            hidden: &[
                AttrMechanism::DosXattr(FILE_ATTRIBUTE_HIDDEN),
                AttrMechanism::DotPrefix,
            ],
            system: &[AttrMechanism::DosXattr(FILE_ATTRIBUTE_SYSTEM)],
            archive: &[AttrMechanism::DosXattr(FILE_ATTRIBUTE_ARCHIVE)],
            birth_time: false,
        },
        AttrBackend::MacNative => Candidates {
            readonly: AttrMechanism::PermBits,
            hidden: &[AttrMechanism::BsdHiddenFlag, AttrMechanism::DotPrefix],
            system: NO_ATTR,
            archive: NO_ATTR,
            birth_time: true,
        },
        AttrBackend::Posix | AttrBackend::Unknown => Candidates {
            readonly: AttrMechanism::PermBits,
            hidden: &[AttrMechanism::DotPrefix],
            system: NO_ATTR,
            archive: NO_ATTR,
            birth_time: false,
        },
    }
}

impl Candidates {
    /// The best guess without writing: first entry of each chain.
    pub(crate) fn preferred(&self) -> AttrCaps {
        AttrCaps {
            readonly: self.readonly,
            hidden: first(self.hidden),
            system: first(self.system),
            archive: first(self.archive),
            birth_time: self.birth_time,
            confirmed: false,
        }
    }

    /// Whether any candidate can silently no-op and therefore needs a probe.
    pub(crate) fn needs_probe(&self) -> bool {
        self.hidden
            .iter()
            .chain(self.system)
            .chain(self.archive)
            .any(|m| m.needs_probe())
    }

    /// Resolve every probing chain against a throwaway target on `anchor`'s
    /// volume, through the injected syscall layer. If the volume has no
    /// writable place to probe (read-only), the candidate preference is
    /// returned unconfirmed — writes then attempt normally and report their
    /// real result.
    pub(crate) fn resolve_with(&self, anchor: &Path, fs: &dyn Fs) -> AttrCaps {
        let mut caps = self.preferred();
        let Some(tmp) = fs.probe_target(anchor) else {
            return caps;
        };
        caps.hidden = resolve_chain(self.hidden, &tmp, fs);
        caps.system = resolve_chain(self.system, &tmp, fs);
        caps.archive = resolve_chain(self.archive, &tmp, fs);
        fs.cleanup_probe(&tmp);
        caps.confirmed = true;
        caps
    }
}

fn first(chain: &'static [AttrMechanism]) -> AttrMechanism {
    chain.first().copied().unwrap_or(AttrMechanism::None)
}

fn resolve_chain(chain: &'static [AttrMechanism], tmp: &Path, fs: &dyn Fs) -> AttrMechanism {
    for &mechanism in chain {
        if !mechanism.needs_probe() {
            // Self-evident mechanism: its syscall result is the answer.
            return mechanism;
        }
        if verify(mechanism, tmp, fs) {
            return mechanism;
        }
    }
    AttrMechanism::None
}

/// Apply `mechanism` to `tmp`, confirm both set and clear by reading back.
fn verify(mechanism: AttrMechanism, tmp: &Path, fs: &dyn Fs) -> bool {
    if !matches!(fs.set(mechanism, tmp, true), AttrOutcome::Applied) {
        return false;
    }
    let set_ok = matches!(fs.get(mechanism, tmp), Some(true));
    let clear_ok = matches!(fs.set(mechanism, tmp, false), AttrOutcome::Applied)
        && matches!(fs.get(mechanism, tmp), Some(false));
    set_ok && clear_ok
}

/// Probe-file name prefix. Distinct from the case-sensitivity probe's prefix so
/// the two cleanups never remove each other's files.
const PROBE_PREFIX: &str = ".awara-attrprobe-";

/// Residue from a probe that died before cleanup is only removed once it is
/// older than this, so a concurrent probe in another process can't have its
/// live file deleted.
const RESIDUE_MIN_AGE: Duration = Duration::from_secs(300);

/// Create a uniquely named throwaway file in `dir`, first clearing stale
/// residue. Returns `None` when nothing can be created (read-only directory /
/// volume).
pub(super) fn create_probe_file(dir: &Path) -> Option<PathBuf> {
    cleanup_residue(dir);
    for _ in 0..8 {
        let path = dir.join(format!("{PROBE_PREFIX}{}", uuid::Uuid::new_v4()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return Some(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

fn cleanup_residue(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(PROBE_PREFIX)
        {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .map(|modified| {
                now.duration_since(modified)
                    .map(|age| age >= RESIDUE_MIN_AGE)
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if stale {
            let _ = fs::remove_file(entry.path());
        }
    }
}
