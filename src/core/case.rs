//! Filesystem case-sensitivity detection — the single source of truth for
//! every case-folding decision in the rename pipeline (collision detection,
//! execution, undo, CLI pre-scan).
//!
//! Historically the pipeline assumed "macOS/Windows are always
//! case-insensitive, everything else is case-sensitive" at compile time.
//! That is wrong for case-sensitive APFS/HFS+ volumes on macOS, case-sensitive
//! NTFS directories on Windows, and case-insensitive Linux mounts (FAT32,
//! exFAT, SMB/CIFS, casefold ext4). Instead we probe the real filesystem once
//! per directory (cached) and route every comparison through the probed
//! result.
//!
//! Known limitations:
//! - Directories that don't exist yet (e.g. a not-yet-created `--output-dir`)
//!   or that can't be probed (read-only, ACL) resolve through the nearest
//!   existing, writable ancestor on the same device; the result self-corrects
//!   once the directory itself can be probed.
//! - The probe can *prove* `Insensitive` (a differently-cased spelling of a
//!   fresh file resolves to the same entry). `Sensitive` is a high-confidence
//!   inference from a confirmed lookup failure, not proof — any ambiguous
//!   failure yields `Unknown`, which falls back to the platform default
//!   behavior instead of guessing.
//! - Per-directory case sensitivity (NTFS case-sensitive directories, ZFS
//!   per-dataset settings) is handled by keying the cache on the exact
//!   canonical directory, at the cost of one small probe per directory.
//! - Dry-run briefly creates a µs-lived probe file in touched directories so
//!   dry-run collision reports match the real run; nothing is written on
//!   read-only directories. A probe interrupted by a hard kill (SIGKILL,
//!   power loss) leaves a hidden residue that is removed the next time the
//!   same directory is probed.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::core::memo::{BoundedMemo, MEMO_CAP};
use crate::core::volume::{nearest_existing_same_device, volume_id};

/// Whether a directory's filesystem folds `Foo` and `foo` into one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseSensitivity {
    /// Probed: the filesystem distinguishes case.
    Sensitive,
    /// Probed: the filesystem folds case.
    Insensitive,
    /// Probe inconclusive (no writable directory on the volume, ambiguous
    /// lookup error). Falls back to the platform default behavior.
    Unknown,
}

impl CaseSensitivity {
    pub fn is_insensitive(self) -> bool {
        matches!(self, CaseSensitivity::Insensitive)
    }

    pub fn is_unknown(self) -> bool {
        matches!(self, CaseSensitivity::Unknown)
    }

    /// Whether comparisons under this sensitivity should fold case.
    /// `Unknown` conservatively falls back to the platform's historical
    /// default (fold on macOS/Windows, exact elsewhere) rather than guessing.
    pub fn folds_case(self) -> bool {
        match self {
            CaseSensitivity::Insensitive => true,
            CaseSensitivity::Sensitive => false,
            CaseSensitivity::Unknown => platform_default_folds_case(),
        }
    }

    /// Lowercase `s` when the filesystem folds case, otherwise unchanged.
    pub fn normalize(self, s: &str) -> String {
        if self.folds_case() {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    }
}

fn platform_default_folds_case() -> bool {
    cfg!(windows) || cfg!(target_os = "macos")
}

/// Probe cache, keyed by the canonical full path of an existing directory.
/// Canonical-only: raw spellings are never cached globally, so symlink
/// re-pointing and directory renames naturally miss and re-probe instead of
/// serving a stale result. (`Unknown` is never cached either — a failed probe
/// is often transient and is retried cheaply.)
///
/// Bounded (eldest evicted): a recursive scan can touch a very large number of
/// directories, and a dropped entry only costs one re-probe on its next use.
static CACHE: BoundedMemo<PathBuf, CaseSensitivity, MEMO_CAP> = BoundedMemo::new();

/// Case sensitivity of the filesystem holding `dir`.
///
/// Probed once per canonical directory and memoized (bounded, eldest evicted).
/// Directories that don't exist yet (e.g. a not-yet-created `--output-dir`) or
/// that can't be probed (read-only, ACL) resolve through the nearest existing,
/// probeable ancestor on the same device; if none exists, `Unknown` falls back
/// to the platform default.
pub fn case_sensitivity_for(dir: &Path) -> CaseSensitivity {
    // The probe needs an existing, writable directory. Start at the nearest
    // existing ancestor, and if it can't be probed (read-only directory,
    // ACL, transient failure) keep walking up the same device — never
    // borrowing a result from a different volume (e.g. a read-only mount
    // inside a writable parent).
    let mut probe_dir = nearest_existing_same_device(dir);
    let start_dev = volume_id(&probe_dir);

    loop {
        let canonical = probe_dir
            .canonicalize()
            .unwrap_or_else(|_| probe_dir.clone());

        // Hold the guard across the probe: this probe's residue sweep assumes
        // no other probe is in flight, so same-process probes stay serialized.
        let mut cache = CACHE.lock();
        if let Some(cs) = cache.get(&canonical) {
            return cs;
        }
        let cs = probe(&canonical);
        if !cs.is_unknown() {
            cache.insert(canonical, cs);
            return cs;
        }
        drop(cache);

        // Inconclusive (e.g. read-only directory): try the parent, but never
        // cross a device boundary.
        let Some(parent) = probe_dir.parent() else {
            return CaseSensitivity::Unknown;
        };
        if parent == probe_dir.as_path() {
            return CaseSensitivity::Unknown;
        }
        // On Windows the root-prefix hash can't see volume mount points;
        // a junction / mount point / symlink here is a volume boundary even
        // on the same drive letter — don't walk through it.
        #[cfg(windows)]
        if is_reparse_point(&probe_dir) {
            return CaseSensitivity::Unknown;
        }
        let on_same_device = match volume_id(parent) {
            Some(id) => start_dev.is_none() || Some(id) == start_dev,
            None => false,
        };
        if !on_same_device {
            return CaseSensitivity::Unknown;
        }
        probe_dir = parent.to_path_buf();
    }
}

/// Windows `FILE_ATTRIBUTE_REPARSE_POINT` — set on junctions, volume mount
/// points, and symlinks.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// True when `path` is a reparse point (junction, volume mount point, or
/// symlink) on Windows. `symlink_metadata` (not `metadata`) so we see the
/// link's own attributes, not its target's.
#[cfg(windows)]
fn is_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    std::fs::symlink_metadata(path)
        .map(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}

/// Probe `dir` by creating a uniquely named file and checking whether a
/// differently-cased spelling of its name resolves to the same entry.
///
/// The two spellings share a stem and differ only in the case of a fixed
/// extension (`.probe-<uuid>.AwaraCaseProbe` → `.probe-<uuid>.awaracaseprobe`),
/// so they are byte-distinct by construction without relying on Unicode
/// `to_uppercase` semantics (which can change length or expand to multiple
/// characters).
///
/// Result semantics:
/// - alternate open succeeds → `Insensitive` (proof: the FS folded the two
///   spellings onto one entry),
/// - alternate open reports `NotFound` while the original provably still
///   exists → `Sensitive` (high-confidence inference, not proof),
/// - anything ambiguous (race/deletion, permission errors) → `Unknown`.
///
/// The probe is fully synchronous with no yield or cancellation points
/// (create → lookup → cleanup in microseconds), so the process's shutdown
/// token never comes into play: only a hard kill (SIGKILL, power loss) in
/// that window can leave a residue file. Any such residue is removed here on
/// the next visit to the directory.
fn probe(dir: &Path) -> CaseSensitivity {
    // Self-heal: remove residue from a probe that was killed between create
    // and cleanup. Only names that are exactly a probe stem + valid UUID +
    // the probe extension are touched — a real user file matching that
    // pattern is effectively impossible. Safe while probes are serialized
    // (single thread); a future parallel-probe design must probe outside any
    // shared lock and tolerate concurrent cleanup.
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if is_probe_residue(&name) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    let (lower_name, upper_name) = probe_names(&uuid::Uuid::new_v4());
    let lower = dir.join(&lower_name);
    let upper = dir.join(&upper_name);

    let _ = fs::remove_file(&lower);
    let _ = fs::remove_file(&upper);

    let Ok(_file) = fs::File::create(&lower) else {
        // Directory not writable: no probe possible.
        return CaseSensitivity::Unknown;
    };

    let result = match fs::File::open(&upper) {
        Ok(_) => CaseSensitivity::Insensitive,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // Distinguish "the alternate spelling didn't resolve" (case
            // evidence) from "the probe file itself vanished" (race/deletion
            // — inconclusive). Retry once to rule out transient lookup
            // caching on network filesystems.
            if fs::metadata(&lower).is_err() {
                CaseSensitivity::Unknown
            } else if fs::File::open(&upper).is_ok() {
                CaseSensitivity::Insensitive
            } else {
                CaseSensitivity::Sensitive
            }
        }
        Err(_) => CaseSensitivity::Unknown,
    };

    let _ = fs::remove_file(&lower);
    let _ = fs::remove_file(&upper);
    result
}

/// Probe file naming: dot-prefixed stem (hidden on Unix) + UUID + a fixed
/// extension whose case is the toggled spelling.
const PROBE_STEM_PREFIX: &str = ".probe-";
const PROBE_EXT: &str = ".AwaraCaseProbe";
const PROBE_EXT_ALT: &str = ".awaracaseprobe";

/// The two spellings of the probe file for `uuid` — identical stems, only
/// the extension's case differs. Equal length, byte-distinct by
/// construction, no Unicode mapping involved.
fn probe_names(uuid: &uuid::Uuid) -> (String, String) {
    let stem = format!("{PROBE_STEM_PREFIX}{uuid}");
    let lower = format!("{stem}{PROBE_EXT}");
    (lower.clone(), alternate_spelling(&lower))
}

/// Swap the probe extension for its case-toggle, leaving the stem untouched.
/// An involution (toggling twice is the identity).
fn alternate_spelling(name: &str) -> String {
    let toggled = if let Some(stem) = name.strip_suffix(PROBE_EXT) {
        format!("{stem}{PROBE_EXT_ALT}")
    } else {
        let stem = name
            .strip_suffix(PROBE_EXT_ALT)
            .expect("name must use a probe extension");
        format!("{stem}{PROBE_EXT}")
    };
    debug_assert_ne!(toggled, name, "alternate spelling must differ byte-wise");
    toggled
}

/// True when `name` is exactly a probe file: probe stem prefix + valid UUID +
/// the probe extension in either case.
fn is_probe_residue(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(PROBE_STEM_PREFIX) else {
        return false;
    };
    let Some(uuid_part) = rest
        .strip_suffix(PROBE_EXT)
        .or_else(|| rest.strip_suffix(PROBE_EXT_ALT))
    else {
        return false;
    };
    uuid::Uuid::parse_str(uuid_part).is_ok()
}

/// Normalize a path for collision comparison under `cs`.
pub fn norm_path_for_collision(p: &Path, cs: CaseSensitivity) -> String {
    cs.normalize(&p.to_string_lossy())
}

/// Check whether two paths refer to the same file under `cs`'s filesystem.
pub fn is_same_file(a: &Path, b: &Path, cs: CaseSensitivity) -> bool {
    if cs.folds_case() {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// Rename `src` to `dst` even when the two differ only by case on a
/// case-insensitive filesystem (where a plain rename can fail or resolve to
/// the same entry). On case-sensitive filesystems this is a plain rename.
///
/// Safety: never overwrites an existing destination unless `is_same_file`
/// confirms it is the same entry as `src`.
pub fn rename_case_insensitive(src: &Path, dst: &Path, cs: CaseSensitivity) -> io::Result<()> {
    if dst.exists() && !is_same_file(src, dst, cs) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "refusing to overwrite '{}' with '{}': distinct files",
                dst.display(),
                src.display()
            ),
        ));
    }
    if cs.folds_case() {
        let tmp = dst.with_extension(format!("tmp_{}", uuid::Uuid::new_v4()));
        fs::rename(src, &tmp)?;
        fs::rename(&tmp, dst)
    } else {
        fs::rename(src, dst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_alternate_spelling_distinct_by_construction() {
        let uuid = uuid::Uuid::parse_str("01234567-89ab-cdef-0123-456789abcdef").unwrap();
        let (lower, upper) = probe_names(&uuid);
        assert_eq!(
            lower,
            ".probe-01234567-89ab-cdef-0123-456789abcdef.AwaraCaseProbe"
        );
        assert_eq!(
            upper,
            ".probe-01234567-89ab-cdef-0123-456789abcdef.awaracaseprobe"
        );
        assert_ne!(lower, upper);
        assert_eq!(lower.len(), upper.len());
        // Toggling is an involution.
        assert_eq!(alternate_spelling(&lower), upper);
        assert_eq!(alternate_spelling(&upper), lower);
    }

    #[test]
    fn test_is_probe_residue_matching() {
        let uuid = uuid::Uuid::new_v4();
        let (lower, upper) = probe_names(&uuid);
        assert!(is_probe_residue(&lower));
        assert!(is_probe_residue(&upper));
        // Non-UUID stems, unrelated extensions, and bare prefixes are not
        // probe residue.
        assert!(!is_probe_residue(".probe-not-a-uuid.AwaraCaseProbe"));
        assert!(!is_probe_residue(&format!(".probe-{uuid}.txt")));
        assert!(!is_probe_residue(".probe-"));
        assert!(!is_probe_residue("important.txt"));
    }

    #[test]
    fn test_normalize_semantics() {
        assert_eq!(
            CaseSensitivity::Sensitive.normalize("My_Cool_File.txt"),
            "My_Cool_File.txt"
        );
        assert_eq!(
            CaseSensitivity::Insensitive.normalize("My_Cool_File.txt"),
            "my_cool_file.txt"
        );
        // Unknown follows the platform default.
        assert_eq!(
            CaseSensitivity::Unknown.folds_case(),
            platform_default_folds_case()
        );
    }

    #[test]
    fn test_norm_path_for_collision_both_variants() {
        let p = Path::new("/Home/User/File.TXT");
        assert_eq!(
            norm_path_for_collision(p, CaseSensitivity::Sensitive),
            "/Home/User/File.TXT"
        );
        assert_eq!(
            norm_path_for_collision(p, CaseSensitivity::Insensitive),
            "/home/user/file.txt"
        );
    }

    #[test]
    fn test_norm_path_for_collision_unicode_preserved() {
        let p = Path::new("/Café/Resumé.pdf");
        assert_eq!(
            norm_path_for_collision(p, CaseSensitivity::Sensitive),
            "/Café/Resumé.pdf"
        );
        assert_eq!(
            norm_path_for_collision(p, CaseSensitivity::Insensitive),
            "/café/resumé.pdf"
        );
    }

    #[test]
    fn test_is_same_file_both_variants() {
        let a = Path::new("/tmp/File.TXT");
        let b = Path::new("/tmp/file.txt");
        assert!(!is_same_file(a, b, CaseSensitivity::Sensitive));
        assert!(is_same_file(a, b, CaseSensitivity::Insensitive));
        assert!(is_same_file(a, a, CaseSensitivity::Sensitive));
        assert!(!is_same_file(
            a,
            Path::new("/other/file.txt"),
            CaseSensitivity::Insensitive
        ));
    }

    #[test]
    fn test_case_sensitivity_for_is_consistent_and_hygienic() {
        let dir = TempDir::new().unwrap();
        // Probe twice: cached result, and the directory is left clean.
        let cs1 = case_sensitivity_for(dir.path());
        let cs2 = case_sensitivity_for(dir.path());
        assert_eq!(cs1, cs2);
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.is_empty(),
            "probe must leave no files behind, got: {names:?}"
        );
    }

    #[test]
    fn test_case_sensitivity_for_matches_direct_lookup() {
        // The cached probe must agree with a direct, independent check of the
        // filesystem's case-folding behavior in the same directory.
        let dir = TempDir::new().unwrap();
        let (lower_name, upper_name) = probe_names(&uuid::Uuid::new_v4());
        let lower = dir.path().join(&lower_name);
        let upper = dir.path().join(&upper_name);
        fs::write(&lower, b"").unwrap();
        let lookup_folds = fs::File::open(&upper).is_ok();
        fs::remove_file(&lower).unwrap();

        let expected = if lookup_folds {
            CaseSensitivity::Insensitive
        } else {
            CaseSensitivity::Sensitive
        };
        assert_eq!(case_sensitivity_for(dir.path()), expected);
    }

    #[test]
    fn test_case_sensitivity_for_ancestor_fallback() {
        // A directory that doesn't exist yet resolves through its nearest
        // existing ancestor (same device) — and agrees with a direct probe of
        // that ancestor.
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("not").join("yet").join("created");
        let from_missing = case_sensitivity_for(&missing);

        let (lower_name, upper_name) = probe_names(&uuid::Uuid::new_v4());
        let lower = dir.path().join(&lower_name);
        let upper = dir.path().join(&upper_name);
        fs::write(&lower, b"").unwrap();
        let lookup_folds = fs::File::open(&upper).is_ok();
        fs::remove_file(&lower).unwrap();
        let expected = if lookup_folds {
            CaseSensitivity::Insensitive
        } else {
            CaseSensitivity::Sensitive
        };
        assert_eq!(from_missing, expected);
    }

    #[cfg(unix)]
    #[test]
    fn test_case_sensitivity_for_read_only_dir_falls_back_to_ancestor() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("locked");
        fs::create_dir(&sub).unwrap();
        let mut perms = fs::metadata(&sub).unwrap().permissions();
        perms.set_mode(0o555);
        fs::set_permissions(&sub, perms).unwrap();

        let actual = case_sensitivity_for(&sub);
        let expected = case_sensitivity_for(dir.path());

        // Restore permissions before asserting so a panic can't leak the dir.
        let mut perms = fs::metadata(&sub).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&sub, perms).unwrap();

        // Whether the locked dir is probed directly (as root) or resolves
        // through its writable ancestor, both are the same volume — results
        // must agree.
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_case_sensitivity_for_canonical_key_sharing() {
        // Symlinked spellings of the same directory share one cache entry:
        // probing the real dir first, then the symlink, must not change the
        // result (and the second call is a cache hit).
        let dir = TempDir::new().unwrap();
        let link = TempDir::new().unwrap();
        let link_path = link.path().join("alias");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), &link_path).unwrap();
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(dir.path(), &link_path).unwrap();
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = &link_path;
            return;
        }
        let direct = case_sensitivity_for(dir.path());
        let via_link = case_sensitivity_for(&link_path);
        assert_eq!(direct, via_link);
    }

    #[test]
    fn test_probe_cleans_up_stale_residue() {
        let dir = TempDir::new().unwrap();
        // Simulate two probes killed between create and cleanup.
        let (stale_lower_name, stale_upper_name) = probe_names(&uuid::Uuid::new_v4());
        let stale_lower = dir.path().join(&stale_lower_name);
        let stale_upper = dir.path().join(&stale_upper_name);
        fs::write(&stale_lower, b"").unwrap();
        fs::write(&stale_upper, b"").unwrap();
        // User files that merely resemble probe names must be left alone: a
        // non-UUID stem, and a probe-shaped stem with an unrelated extension.
        let user_bad_stem = dir.path().join(".probe-not-a-uuid.AwaraCaseProbe");
        let user_bad_ext = dir
            .path()
            .join(".probe-01234567-89ab-cdef-0123-456789abcdef.txt");
        fs::write(&user_bad_stem, b"important").unwrap();
        fs::write(&user_bad_ext, b"keep").unwrap();

        case_sensitivity_for(dir.path());

        assert!(!stale_lower.exists(), "stale residue must be removed");
        assert!(!stale_upper.exists(), "stale residue must be removed");
        assert!(user_bad_stem.exists(), "non-UUID stems must be left alone");
        assert_eq!(fs::read_to_string(&user_bad_stem).unwrap(), "important");
        assert!(
            user_bad_ext.exists(),
            "unrelated extensions must be left alone"
        );
        assert_eq!(fs::read_to_string(&user_bad_ext).unwrap(), "keep");
    }

    #[test]
    fn test_rename_case_insensitive_guard() {
        let dir = TempDir::new().unwrap();
        let cs = case_sensitivity_for(dir.path());

        // Distinct existing destination: must refuse, never clobber.
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");
        fs::write(&src, b"src").unwrap();
        fs::write(&dst, b"dst").unwrap();
        let result = rename_case_insensitive(&src, &dst, cs);
        assert!(result.is_err(), "must refuse to overwrite a distinct file");
        assert_eq!(fs::read_to_string(&dst).unwrap(), "dst");
        assert!(src.exists(), "source must be untouched after refusal");

        // Same file (case-only rename): succeeds.
        let case_src = dir.path().join("OriginalCase.TXT");
        let case_dst = dir.path().join("originalcase.txt");
        fs::write(&case_src, b"data").unwrap();
        let result = rename_case_insensitive(&case_src, &case_dst, cs);
        assert!(
            result.is_ok(),
            "case-only rename should succeed: {:?}",
            result
        );
        assert!(case_dst.exists());
        assert_eq!(fs::read_to_string(&case_dst).unwrap(), "data");
    }
}
