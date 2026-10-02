//! Filesystem classification — a *candidate selector*, never the authority.
//!
//! The backend only decides which mechanisms are worth trying and in what
//! order; whether a mechanism actually works is confirmed by the write probe
//! (`probe.rs`). This matters because filesystem type names/magics are
//! heuristics: FUSE, bind mounts, and driver variants (NTFS-3g vs kernel
//! `ntfs3`) can all misreport.

use std::path::Path;

/// Coarse, diagnostic classification of the filesystem backend.
// Not every variant is constructed on every target (each OS constructs only
// its own), but `candidates_for` handles all of them.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttrBackend {
    /// POSIX-native filesystems (ext4, xfs, btrfs, tmpfs, overlayfs, nfs, ...).
    Posix,
    /// Filesystems storing DOS attribute bits with a native API: FAT/exFAT.
    DosFat,
    /// Filesystems storing DOS attribute bits in the `user.DOSATTRIB` xattr:
    /// SMB/CIFS shares and ntfs-3g.
    DosXattr,
    /// Windows: the Win32 attribute API is available on every supported volume.
    Win32,
    /// macOS: `UF_HIDDEN` is exposed through the VFS on APFS/HFS+ and maps to
    /// the hidden attribute on FAT-family volumes.
    MacNative,
    /// Backend could not be identified; fall back to the dot convention.
    Unknown,
}

#[cfg(windows)]
pub(crate) fn backend_for(_path: &Path) -> AttrBackend {
    AttrBackend::Win32
}

#[cfg(target_os = "macos")]
pub(crate) fn backend_for(path: &Path) -> AttrBackend {
    // macOS exposes `UF_HIDDEN` through the VFS on local filesystems; network
    // mounts don't honor `chflags`, so treat them as Unknown (dot fallback)
    // rather than probing a mechanism that cannot work.
    match fstype_name(path).as_deref() {
        Some("apfs") | Some("hfs") | Some("msdos") | Some("exfat") | Some("ntfs") => {
            AttrBackend::MacNative
        }
        _ => AttrBackend::Unknown,
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn backend_for(path: &Path) -> AttrBackend {
    // `statfs` magic resolves local filesystems directly, so the mount table is
    // only read when it is inconclusive (FUSE / unknown) — that is where the
    // concrete driver name matters (e.g. ntfs-3g reports `fuse`).
    if let Some(magic) = statfs_magic(path) {
        let backend = backend_from_magic(magic);
        if backend != AttrBackend::Unknown {
            return backend;
        }
    }
    match mount_fstype(path) {
        Some(name) => backend_from_name(&name),
        None => AttrBackend::Unknown,
    }
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
pub(crate) fn backend_for(_path: &Path) -> AttrBackend {
    AttrBackend::Posix
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn backend_for(_path: &Path) -> AttrBackend {
    AttrBackend::Posix
}

/// Map a mount-table filesystem type name to a candidate backend.
#[cfg(target_os = "linux")]
pub(crate) fn backend_from_name(name: &str) -> AttrBackend {
    use AttrBackend::{DosFat, DosXattr, Posix, Unknown};
    match name {
        "vfat" | "msdos" | "exfat" => DosFat,
        // ntfs-3g/fuseblk and CIFS/SMB persist DOS attributes in `user.DOSATTRIB`.
        "ntfs" | "ntfs3" | "cifs" | "smb3" | "smbfs" | "fuseblk" => DosXattr,
        "ext2" | "ext3" | "ext4" | "xfs" | "btrfs" | "f2fs" | "jfs" | "reiserfs" | "zfs"
        | "tmpfs" | "overlay" | "ramfs" | "nfs" | "nfs4" | "nfsd" | "devtmpfs" | "proc"
        | "sysfs" | "cgroup2" => Posix,
        _ => Unknown,
    }
}

// `statfs(2)` magic numbers. Defined locally rather than pulled from `libc`
// because `f_type`'s integer width varies across targets and not every magic is
// exported on every libc configuration.
#[cfg(target_os = "linux")]
const MSDOS_SUPER_MAGIC: u64 = 0x0000_4d44; // vfat
#[cfg(target_os = "linux")]
const EXFAT_SUPER_MAGIC: u64 = 0x2011_bab0; // exfat
#[cfg(target_os = "linux")]
const NTFS_SUPER_MAGIC: u64 = 0x5346_544e; // ntfs3
#[cfg(target_os = "linux")]
const CIFS_MAGIC_NUMBER: u64 = 0xff53_4d42; // cifs
#[cfg(target_os = "linux")]
const SMB_SUPER_MAGIC: u64 = 0x0000_517b; // smb
#[cfg(target_os = "linux")]
const NFS_SUPER_MAGIC: u64 = 0x0000_6969; // nfs
#[cfg(target_os = "linux")]
const TMPFS_MAGIC: u64 = 0x0102_1994; // tmpfs
#[cfg(target_os = "linux")]
const OVERLAYFS_SUPER_MAGIC: u64 = 0x794c_7630; // overlayfs
#[cfg(target_os = "linux")]
const EXT4_SUPER_MAGIC: u64 = 0x0000_ef53; // ext2/3/4
#[cfg(target_os = "linux")]
const XFS_SUPER_MAGIC: u64 = 0x5846_5342; // xfs
#[cfg(target_os = "linux")]
const BTRFS_SUPER_MAGIC: u64 = 0x9123_683e; // btrfs
#[cfg(target_os = "linux")]
const FUSE_SUPER_MAGIC: u64 = 0x6573_5546; // fuse

/// Map a `statfs.f_type` magic to a candidate backend (fallback when the mount
/// table can't be read).
#[cfg(target_os = "linux")]
pub(crate) fn backend_from_magic(magic: u64) -> AttrBackend {
    use AttrBackend::{DosFat, DosXattr, Posix, Unknown};
    match magic {
        MSDOS_SUPER_MAGIC | EXFAT_SUPER_MAGIC => DosFat,
        NTFS_SUPER_MAGIC | CIFS_MAGIC_NUMBER | SMB_SUPER_MAGIC => DosXattr,
        EXT4_SUPER_MAGIC
        | XFS_SUPER_MAGIC
        | BTRFS_SUPER_MAGIC
        | NFS_SUPER_MAGIC
        | TMPFS_MAGIC
        | OVERLAYFS_SUPER_MAGIC => Posix,
        // FUSE could be anything; the mount-table name (when readable) is the
        // better signal, and otherwise the dot fallback is safe.
        FUSE_SUPER_MAGIC => Unknown,
        _ => Unknown,
    }
}

#[cfg(target_os = "linux")]
fn statfs_magic(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statfs(cpath.as_ptr(), &mut st) };
    if rc == 0 {
        Some(st.f_type as u64)
    } else {
        None
    }
}

/// Filesystem type name for the mount containing `path`, from
/// `/proc/self/mounts`. Matches the longest mount-point prefix so nested mounts
/// win over their parents. Returns `None` if the table can't be read.
#[cfg(target_os = "linux")]
fn mount_fstype(path: &Path) -> Option<String> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mounts = std::fs::read_to_string("/proc/self/mounts").ok()?;

    let mut best: Option<(usize, String)> = None;
    for line in mounts.lines() {
        let mut cols = line.split_whitespace();
        let (Some(_source), Some(mount_point), Some(fstype)) =
            (cols.next(), cols.next(), cols.next())
        else {
            continue;
        };
        let mount_point = unescape_mount(mount_point);
        let mount_path = Path::new(&mount_point);
        if canonical.starts_with(mount_path) {
            let len = mount_path.as_os_str().len();
            if best.as_ref().is_none_or(|(best_len, _)| len > *best_len) {
                best = Some((len, fstype.to_string()));
            }
        }
    }
    best.map(|(_, fstype)| fstype)
}

/// Decode the octal escapes `proc(5)` uses for whitespace/backslash in mount
/// field values (`\040` space, `\011` tab, `\012` newline, `\134` backslash).
#[cfg(target_os = "linux")]
fn unescape_mount(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let oct = &bytes[i + 1..i + 4];
            if oct.iter().all(|b| (b'0'..=b'7').contains(b)) {
                let value = (oct[0] - b'0') * 64 + (oct[1] - b'0') * 8 + (oct[2] - b'0');
                out.push(value);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// macOS filesystem type name (`statfs.f_fstypename`).
#[cfg(target_os = "macos")]
fn fstype_name(path: &Path) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(cpath.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let name = unsafe { std::ffi::CStr::from_ptr(st.f_fstypename.as_ptr()) };
    Some(name.to_string_lossy().into_owned())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::unescape_mount;

    #[test]
    fn unescape_mount_decodes_octal_escapes() {
        assert_eq!(unescape_mount("/mnt/My\\040Disk"), "/mnt/My Disk");
        assert_eq!(unescape_mount("/plain"), "/plain");
        assert_eq!(unescape_mount("/back\\134slash"), "/back\\slash");
    }
}
