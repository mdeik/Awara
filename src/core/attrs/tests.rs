use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::mechanism::{FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM};
use super::*;

fn posix_caps() -> AttrCaps {
    AttrCaps {
        readonly: AttrMechanism::PermBits,
        hidden: AttrMechanism::DotPrefix,
        system: AttrMechanism::None,
        archive: AttrMechanism::None,
        birth_time: false,
        confirmed: false,
    }
}

/// The hidden mechanism the host's native backend resolves to: the DOS hidden
/// bit on Windows, `UF_HIDDEN` on macOS, the dot convention elsewhere.
fn native_hidden() -> AttrMechanism {
    if cfg!(windows) {
        AttrMechanism::DosBit(FILE_ATTRIBUTE_HIDDEN)
    } else if cfg!(target_os = "macos") {
        AttrMechanism::BsdHiddenFlag
    } else {
        AttrMechanism::DotPrefix
    }
}

// ── Real-filesystem tests (whatever the host actually is) ──────────────────

#[test]
fn native_volume_read_caps_match_the_platform() {
    let dir = TempDir::new().unwrap();
    let caps = caps_for_read(dir.path());
    assert_eq!(caps.readonly, AttrMechanism::PermBits);
    assert_eq!(caps.hidden, native_hidden());
    if cfg!(windows) {
        assert_eq!(caps.system, AttrMechanism::DosBit(FILE_ATTRIBUTE_SYSTEM));
        assert_eq!(caps.archive, AttrMechanism::DosBit(FILE_ATTRIBUTE_ARCHIVE));
    } else {
        assert_eq!(caps.system, AttrMechanism::None);
        assert_eq!(caps.archive, AttrMechanism::None);
    }
    assert_eq!(caps.birth_time, cfg!(any(windows, target_os = "macos")));
}

#[test]
fn native_volume_write_caps_resolve() {
    let dir = TempDir::new().unwrap();
    let caps = caps_for_write(dir.path());
    assert_eq!(caps.hidden, native_hidden());
    assert!(caps.confirmed, "a writable native volume confirms");
}

#[test]
fn dot_prefix_renames_and_reads_back() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("plain.txt");
    std::fs::write(&file, b"x").unwrap();

    // Exercise the mechanism directly: the dot convention is not the native
    // hidden mechanism on Windows/macOS.
    let mechanism = AttrMechanism::DotPrefix;
    let AttrOutcome::Renamed(hidden) = mechanism.set(&file, true) else {
        panic!("expected a rename to the dot-prefixed name");
    };
    assert!(
        hidden
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with('.')
    );
    assert_eq!(mechanism.get(&hidden, None), Some(true));

    let AttrOutcome::Renamed(visible) = mechanism.set(&hidden, false) else {
        panic!("expected a rename back to the visible name");
    };
    assert!(
        !visible
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with('.')
    );
    assert_eq!(mechanism.get(&visible, None), Some(false));
}

#[test]
fn readonly_roundtrips_through_perm_bits() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("ro.txt");
    std::fs::write(&file, b"x").unwrap();

    let caps = caps_for_read(dir.path());
    assert_eq!(caps.set(&file, "readonly", true), AttrOutcome::Applied);
    assert_eq!(caps.get(&file, "readonly"), Some(true));
    assert_eq!(caps.set(&file, "readonly", false), AttrOutcome::Applied);
    assert_eq!(caps.get(&file, "readonly"), Some(false));
}

#[test]
fn unsupported_and_unknown_attributes_are_reported() {
    let caps = posix_caps();
    assert!(caps.supports("readonly"));
    assert!(caps.supports("read-only"));
    assert!(caps.supports("hidden"));
    assert!(!caps.supports("system"));
    assert!(!caps.supports("archive"));
    assert!(!caps.supports("bogus"));

    let dir = TempDir::new().unwrap();
    let file = dir.path().join("a");
    std::fs::write(&file, b"x").unwrap();
    assert_eq!(caps.set(&file, "system", true), AttrOutcome::Unsupported);
    assert_eq!(caps.set(&file, "bogus", true), AttrOutcome::Unsupported);
}

#[test]
fn dots_only_name_cannot_be_unhidden() {
    // No file needed: clearing would leave an empty filename, so the mechanism
    // must refuse before touching the filesystem. (A literal "..." is also not
    // a creatable name on Windows.)
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("...");

    let outcome = AttrMechanism::DotPrefix.set(&file, false);
    assert!(matches!(outcome, AttrOutcome::Failed(_)));
}

#[test]
fn probe_file_creation_cleans_stale_residue() {
    let dir = TempDir::new().unwrap();
    let residue = dir
        .path()
        .join(format!(".awara-attrprobe-{}", uuid::Uuid::new_v4()));
    std::fs::write(&residue, b"stale").unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    filetime::set_file_mtime(&residue, filetime::FileTime::from_system_time(old)).unwrap();

    let created = super::probe::create_probe_file(dir.path()).expect("probe file created");
    assert!(!residue.exists(), "stale residue removed on the next probe");
    assert!(created.exists());
    let _ = std::fs::remove_file(&created);
}

#[test]
fn fresh_residue_is_left_alone() {
    let dir = TempDir::new().unwrap();
    let live = dir
        .path()
        .join(format!(".awara-attrprobe-{}", uuid::Uuid::new_v4()));
    std::fs::write(&live, b"in use").unwrap();

    let created = super::probe::create_probe_file(dir.path()).expect("probe file created");
    assert!(live.exists(), "recent residue must not be swept");
    let _ = std::fs::remove_file(&created);
    let _ = std::fs::remove_file(&live);
}

// ── Simulated filesystems ──────────────────────────────────────────────────
//
// `SimFs` models the syscall layer so the capability *policy* (candidate
// selection, probe confirmation, fallback) can be exercised deterministically
// on any host, without mounting FAT/SMB/APFS or a read-only volume.

struct SimFs {
    backend: AttrBackend,
    /// In-place mechanisms the volume knows about.
    known: Vec<AttrMechanism>,
    /// Subset of `known` whose writes are accepted but never persisted (the
    /// probe must reject these).
    lying: Vec<AttrMechanism>,
    writable: bool,
    store: RefCell<HashMap<(AttrMechanism, PathBuf), bool>>,
}

impl SimFs {
    fn new(backend: AttrBackend) -> Self {
        Self {
            backend,
            known: Vec::new(),
            lying: Vec::new(),
            writable: true,
            store: RefCell::new(HashMap::new()),
        }
    }

    fn supports(mut self, mechanisms: &[AttrMechanism]) -> Self {
        self.known.extend_from_slice(mechanisms);
        self
    }

    fn lying(mut self, mechanisms: &[AttrMechanism]) -> Self {
        self.known.extend_from_slice(mechanisms);
        self.lying.extend_from_slice(mechanisms);
        self
    }

    fn readonly(mut self) -> Self {
        self.writable = false;
        self
    }

    fn persists(&self, mechanism: AttrMechanism) -> bool {
        self.known.contains(&mechanism) && !self.lying.contains(&mechanism)
    }
}

impl Fs for SimFs {
    fn backend(&self, _path: &Path) -> AttrBackend {
        self.backend
    }

    fn probe_target(&self, dir: &Path) -> Option<PathBuf> {
        self.writable
            .then(|| dir.join(format!(".sim-probe-{}", uuid::Uuid::new_v4())))
    }

    fn cleanup_probe(&self, _path: &Path) {}

    fn get(&self, mechanism: AttrMechanism, path: &Path) -> Option<bool> {
        match mechanism {
            AttrMechanism::None => None,
            AttrMechanism::DotPrefix => Some(
                path.file_name()
                    .map(|n| n.to_string_lossy().starts_with('.'))
                    .unwrap_or(false),
            ),
            AttrMechanism::PermBits => Some(
                self.store
                    .borrow()
                    .get(&(mechanism, path.to_path_buf()))
                    .copied()
                    .unwrap_or(false),
            ),
            _ if self.known.contains(&mechanism) => Some(
                self.store
                    .borrow()
                    .get(&(mechanism, path.to_path_buf()))
                    .copied()
                    .unwrap_or(false),
            ),
            _ => None,
        }
    }

    fn set(&self, mechanism: AttrMechanism, path: &Path, on: bool) -> AttrOutcome {
        match mechanism {
            AttrMechanism::None => AttrOutcome::Unsupported,
            AttrMechanism::DotPrefix => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if on {
                    if name.starts_with('.') {
                        AttrOutcome::Applied
                    } else {
                        AttrOutcome::Renamed(path.with_file_name(format!(".{name}")))
                    }
                } else if !name.starts_with('.') {
                    AttrOutcome::Applied
                } else {
                    let stripped = name.trim_start_matches('.').to_string();
                    if stripped.is_empty() {
                        AttrOutcome::Failed("dots-only".into())
                    } else {
                        AttrOutcome::Renamed(path.with_file_name(stripped))
                    }
                }
            }
            AttrMechanism::PermBits => {
                self.store
                    .borrow_mut()
                    .insert((mechanism, path.to_path_buf()), on);
                AttrOutcome::Applied
            }
            _ if self.known.contains(&mechanism) => {
                if self.persists(mechanism) {
                    self.store
                        .borrow_mut()
                        .insert((mechanism, path.to_path_buf()), on);
                }
                AttrOutcome::Applied // a lying driver still claims success
            }
            _ => AttrOutcome::Unsupported,
        }
    }
}

fn sim_file(dir: &TempDir) -> PathBuf {
    let file = dir.path().join("sample.txt");
    std::fs::write(&file, b"x").unwrap();
    file
}

#[test]
fn sim_native_volume_hides_with_dot_prefix_only() {
    let dir = TempDir::new().unwrap();
    let file = sim_file(&dir);
    let sim = SimFs::new(AttrBackend::Posix);

    let caps = caps_for_write_with(&file, &sim);
    assert!(caps.confirmed);
    assert_eq!(caps.hidden, AttrMechanism::DotPrefix);
    assert_eq!(caps.system, AttrMechanism::None);
    assert_eq!(caps.archive, AttrMechanism::None);
    assert!(!caps.supports("system"));
    assert!(caps.set_with(&sim, &file, "hidden", true).is_renamed());
}

#[test]
fn sim_fat_volume_uses_fat_bits_without_renaming() {
    let dir = TempDir::new().unwrap();
    let file = sim_file(&dir);
    let sim = SimFs::new(AttrBackend::DosFat).supports(&[
        AttrMechanism::DosBit(FILE_ATTRIBUTE_HIDDEN),
        AttrMechanism::DosBit(FILE_ATTRIBUTE_SYSTEM),
        AttrMechanism::DosBit(FILE_ATTRIBUTE_ARCHIVE),
    ]);

    let caps = caps_for_write_with(&file, &sim);
    assert!(caps.confirmed);
    assert_eq!(caps.hidden, AttrMechanism::DosBit(FILE_ATTRIBUTE_HIDDEN));
    assert_eq!(caps.system, AttrMechanism::DosBit(FILE_ATTRIBUTE_SYSTEM));

    assert_eq!(
        caps.set_with(&sim, &file, "hidden", true),
        AttrOutcome::Applied
    );
    assert_eq!(caps.get_with(&sim, &file, "hidden"), Some(true));
    assert_eq!(
        caps.set_with(&sim, &file, "archive", true),
        AttrOutcome::Applied
    );
    assert_eq!(caps.get_with(&sim, &file, "archive"), Some(true));
}

#[test]
fn sim_xattr_volume_uses_dosattrib() {
    let dir = TempDir::new().unwrap();
    let file = sim_file(&dir);
    let sim = SimFs::new(AttrBackend::DosXattr).supports(&[
        AttrMechanism::DosXattr(FILE_ATTRIBUTE_HIDDEN),
        AttrMechanism::DosXattr(FILE_ATTRIBUTE_SYSTEM),
        AttrMechanism::DosXattr(FILE_ATTRIBUTE_ARCHIVE),
    ]);

    let caps = caps_for_write_with(&file, &sim);
    assert!(caps.confirmed);
    assert_eq!(caps.hidden, AttrMechanism::DosXattr(FILE_ATTRIBUTE_HIDDEN));
    assert_eq!(caps.system, AttrMechanism::DosXattr(FILE_ATTRIBUTE_SYSTEM));

    assert_eq!(
        caps.set_with(&sim, &file, "system", true),
        AttrOutcome::Applied
    );
    assert_eq!(caps.get_with(&sim, &file, "system"), Some(true));
}

#[test]
fn sim_lying_driver_falls_back_to_dot_prefix() {
    let dir = TempDir::new().unwrap();
    let file = sim_file(&dir);
    // The driver claims success but never persists: the probe must reject it.
    let sim =
        SimFs::new(AttrBackend::DosXattr).lying(&[AttrMechanism::DosXattr(FILE_ATTRIBUTE_HIDDEN)]);

    let caps = caps_for_write_with(&file, &sim);
    assert!(caps.confirmed);
    assert_eq!(caps.hidden, AttrMechanism::DotPrefix);
    assert_eq!(caps.system, AttrMechanism::None);
}

#[test]
fn sim_readonly_volume_stays_unconfirmed_with_preferred_mechanism() {
    let dir = TempDir::new().unwrap();
    let file = sim_file(&dir);
    let sim = SimFs::new(AttrBackend::DosXattr)
        .supports(&[AttrMechanism::DosXattr(FILE_ATTRIBUTE_HIDDEN)])
        .readonly();

    let caps = caps_for_write_with(&file, &sim);
    assert!(!caps.confirmed, "no writable place to probe");
    assert_eq!(
        caps.hidden,
        AttrMechanism::DosXattr(FILE_ATTRIBUTE_HIDDEN),
        "reads still get the classified preference"
    );
}

#[test]
fn sim_mac_volume_uses_uf_hidden() {
    let dir = TempDir::new().unwrap();
    let file = sim_file(&dir);
    let sim = SimFs::new(AttrBackend::MacNative).supports(&[AttrMechanism::BsdHiddenFlag]);

    let caps = caps_for_write_with(&file, &sim);
    assert_eq!(caps.hidden, AttrMechanism::BsdHiddenFlag);
    assert_eq!(caps.system, AttrMechanism::None);

    assert_eq!(
        caps.set_with(&sim, &file, "hidden", true),
        AttrOutcome::Applied
    );
    assert_eq!(caps.get_with(&sim, &file, "hidden"), Some(true));
}

#[test]
fn sim_unsupported_attrs_reported_for_volume() {
    let dir = TempDir::new().unwrap();
    let file = sim_file(&dir);
    let caps = caps_for_read_with(&file, &SimFs::new(AttrBackend::Posix));
    assert_eq!(
        super::unsupported_in(&caps, "readonly,-hidden,system,archive,nope"),
        vec![
            "system".to_string(),
            "archive".to_string(),
            "nope".to_string()
        ]
    );
}

// Small convenience so the native test reads clearly.
trait OutcomeExt {
    fn is_renamed(&self) -> bool;
}
impl OutcomeExt for AttrOutcome {
    fn is_renamed(&self) -> bool {
        matches!(self, AttrOutcome::Renamed(_))
    }
}

#[test]
fn read_prefers_a_confirmed_volume_result() {
    let classified = posix_caps(); // unconfirmed
    assert_eq!(
        super::read_caps_from_classification(classified, None),
        classified
    );
    assert_eq!(
        super::read_caps_from_classification(classified, Some(classified)),
        classified,
        "an unconfirmed volume entry is ignored"
    );

    let confirmed = AttrCaps {
        hidden: AttrMechanism::DosXattr(FILE_ATTRIBUTE_HIDDEN),
        confirmed: true,
        ..classified
    };
    assert_eq!(
        super::read_caps_from_classification(classified, Some(confirmed)),
        confirmed,
        "a confirmed volume entry wins over classification"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn magic_classification_selects_candidates() {
    use super::classify::{backend_from_magic, backend_from_name};
    assert_eq!(backend_from_magic(0x0000_4d44), AttrBackend::DosFat); // vfat
    assert_eq!(backend_from_magic(0x2011_bab0), AttrBackend::DosFat); // exfat
    assert_eq!(backend_from_magic(0x5346_544e), AttrBackend::DosXattr); // ntfs3
    assert_eq!(backend_from_magic(0xff53_4d42), AttrBackend::DosXattr); // cifs
    assert_eq!(backend_from_magic(0x0000_ef53), AttrBackend::Posix); // ext4
    assert_eq!(backend_from_magic(0x0102_1994), AttrBackend::Posix); // tmpfs
    assert_eq!(backend_from_magic(0x6573_5546), AttrBackend::Unknown); // fuse
    assert_eq!(backend_from_magic(0xdead_beef), AttrBackend::Unknown);

    // The mount-table name is preferred where available: it distinguishes
    // ntfs-3g (fuseblk) from a plain FUSE mount.
    assert_eq!(backend_from_name("vfat"), AttrBackend::DosFat);
    assert_eq!(backend_from_name("fuseblk"), AttrBackend::DosXattr);
    assert_eq!(backend_from_name("cifs"), AttrBackend::DosXattr);
    assert_eq!(backend_from_name("ext4"), AttrBackend::Posix);
    assert_eq!(backend_from_name("somethingweird"), AttrBackend::Unknown);
}
