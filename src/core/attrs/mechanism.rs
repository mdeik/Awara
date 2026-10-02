//! The mechanism by which one file attribute is read and written.
//!
//! A mechanism is the single point where an attribute touches the filesystem,
//! so every consumer (set, capture/restore, read/filter) goes through the same
//! `get`/`set` implementation instead of re-deriving "Windows vs not".

use std::fs;
use std::io;
use std::path::Path;

use super::AttrOutcome;

// DOS/Windows attribute bit values. These are numerically identical to the FAT
// attribute bits (`ATTR_*`), so the same numbers drive Win32, the Linux FAT
// ioctl, and the `user.DOSATTRIB` xattr.
pub(crate) const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
pub(crate) const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
pub(crate) const FILE_ATTRIBUTE_ARCHIVE: u32 = 0x20;

/// Mechanism by which one file attribute is read and written on a volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttrMechanism {
    /// Unsupported on this volume.
    None,
    /// POSIX permission bits (`readonly` ↔ write bit). On Windows this is what
    /// `std::fs::Permissions::set_readonly` manipulates (the readonly DOS bit).
    PermBits,
    /// A DOS/Windows `FILE_ATTRIBUTE_*` bit via `SetFileAttributes` on Windows
    /// or the FAT attribute ioctls on vfat/exfat. These carry the attribute
    /// bits directly.
    DosBit(u32),
    /// A DOS/Windows `FILE_ATTRIBUTE_*` bit stored in the `user.DOSATTRIB`
    /// extended attribute (SMB/CIFS shares, ntfs-3g). Only used where the
    /// filesystem/driver is known to interpret that xattr.
    DosXattr(u32),
    /// macOS `UF_HIDDEN` BSD file flag (does not rename).
    BsdHiddenFlag,
    /// Unix leading-dot convention (implemented by renaming the entry).
    DotPrefix,
}

impl AttrMechanism {
    /// Read the attribute. `None` means the mechanism is unsupported or the
    /// value could not be determined. `meta`, when supplied, avoids a second
    /// metadata syscall.
    pub fn get(self, path: &Path, meta: Option<&fs::Metadata>) -> Option<bool> {
        match self {
            AttrMechanism::None => None,
            AttrMechanism::PermBits => {
                let owned;
                let meta = match meta {
                    Some(m) => m,
                    None => {
                        owned = fs::metadata(path).ok()?;
                        &owned
                    }
                };
                Some(meta.permissions().readonly())
            }
            AttrMechanism::DotPrefix => Some(is_dot_prefixed(path)),
            AttrMechanism::DosBit(bit) => get_dos_bit(path, bit, meta),
            AttrMechanism::DosXattr(bit) => get_dos_xattr(path, bit, meta),
            AttrMechanism::BsdHiddenFlag => get_bsd_hidden(path, meta),
        }
    }

    /// Apply the attribute. `Renamed` is returned when the mechanism had to
    /// move the entry (dot-prefix); callers must continue with the new path.
    pub fn set(self, path: &Path, on: bool) -> AttrOutcome {
        match self {
            AttrMechanism::None => AttrOutcome::Unsupported,
            AttrMechanism::PermBits => set_perm_bits(path, on),
            AttrMechanism::DotPrefix => set_dot_prefix(path, on),
            AttrMechanism::DosBit(bit) => set_dos_bit(path, bit, on),
            AttrMechanism::DosXattr(bit) => set_dos_xattr(path, bit, on),
            AttrMechanism::BsdHiddenFlag => set_bsd_hidden(path, on),
        }
    }

    /// Whether a successful-looking set can silently no-op, so capability must
    /// be confirmed by a read-back probe. `PermBits`/`DotPrefix`/`None` are
    /// self-evident: the syscall result *is* the answer.
    pub(crate) fn needs_probe(self) -> bool {
        self.is_inplace_metadata()
    }

    /// Whether this is an in-place metadata flag (as opposed to a rename or a
    /// permission bit). Only these are captured and restored as a stored value;
    /// `DotPrefix` is expressed in the file *name* and handled by the rename.
    pub fn is_inplace_metadata(self) -> bool {
        matches!(
            self,
            AttrMechanism::DosBit(_) | AttrMechanism::DosXattr(_) | AttrMechanism::BsdHiddenFlag
        )
    }
}

fn set_perm_bits(path: &Path, on: bool) -> AttrOutcome {
    match fs::metadata(path).map(|m| m.permissions()) {
        Ok(mut perms) => {
            perms.set_readonly(on);
            match fs::set_permissions(path, perms) {
                Ok(()) => AttrOutcome::Applied,
                Err(e) => AttrOutcome::Failed(e.to_string()),
            }
        }
        Err(e) => AttrOutcome::Failed(e.to_string()),
    }
}

pub(crate) fn is_dot_prefixed(path: &Path) -> bool {
    path.file_name()
        .map(|n| n.to_string_lossy().starts_with('.'))
        .unwrap_or(false)
}

fn set_dot_prefix(path: &Path, on: bool) -> AttrOutcome {
    let Some(file_name) = path.file_name() else {
        return AttrOutcome::Failed("path has no file name".into());
    };
    let name = file_name.to_string_lossy();

    let new_name = if on {
        if name.starts_with('.') {
            return AttrOutcome::Applied;
        }
        format!(".{name}")
    } else {
        if !name.starts_with('.') {
            return AttrOutcome::Applied;
        }
        let stripped = name.trim_start_matches('.').to_string();
        if stripped.is_empty() {
            return AttrOutcome::Failed("cannot unhide a dots-only name".into());
        }
        stripped
    };

    let new_path = path.parent().unwrap_or(Path::new(".")).join(new_name);
    match fs::rename(path, &new_path) {
        Ok(()) => AttrOutcome::Renamed(new_path),
        Err(e) => AttrOutcome::Failed(e.to_string()),
    }
}

// ── DOS attribute bits (Win32 API / FAT ioctl) ─────────────────────────────

#[cfg(windows)]
fn get_dos_bit(path: &Path, bit: u32, meta: Option<&fs::Metadata>) -> Option<bool> {
    use std::os::windows::fs::MetadataExt;
    let owned;
    let meta = match meta {
        Some(m) => m,
        None => {
            owned = fs::metadata(path).ok()?;
            &owned
        }
    };
    Some(meta.file_attributes() & bit != 0)
}

#[cfg(windows)]
fn set_dos_bit(path: &Path, bit: u32, on: bool) -> AttrOutcome {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;

    let current = match fs::metadata(path) {
        Ok(m) => m.file_attributes(),
        Err(e) => return AttrOutcome::Failed(e.to_string()),
    };
    let new = if on { current | bit } else { current & !bit };
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        if SetFileAttributesW(wide.as_ptr(), new) != 0 {
            AttrOutcome::Applied
        } else {
            AttrOutcome::Failed(io::Error::last_os_error().to_string())
        }
    }
}

#[cfg(windows)]
unsafe extern "system" {
    fn SetFileAttributesW(lp_file_name: *const u16, dw_file_attributes: u32) -> i32;
}

// FAT-specific attribute ioctls (`linux/msdos_fs.h`): unlike the generic
// `FS_IOC_*FLAGS`, these operate on the FAT attribute bits, not on unrelated
// inode flags, so they are safe for vfat/exfat and nowhere else. Values are
// `_IOR('r', 0x10, u32)` / `_IOW('r', 0x11, u32)`, typed as `libc::Ioctl`
// because that type differs between glibc (`c_ulong`) and musl (`c_int`).
#[cfg(target_os = "linux")]
const FAT_IOCTL_GET_ATTRIB: libc::Ioctl = 0x8004_7210u32 as libc::Ioctl;
#[cfg(target_os = "linux")]
const FAT_IOCTL_SET_ATTRIB: libc::Ioctl = 0x4004_7211u32 as libc::Ioctl;

/// Linux: FAT-family drivers (vfat/exfat) store DOS attribute bits directly and
/// expose them through `FAT_IOCTL_{GET,SET}_ATTRIB`. Candidates only reach this
/// for volumes classified as FAT-like.
#[cfg(target_os = "linux")]
fn get_dos_bit(path: &Path, bit: u32, _meta: Option<&fs::Metadata>) -> Option<bool> {
    linux_get_attrs(path).map(|attrs| attrs & bit != 0)
}

#[cfg(target_os = "linux")]
fn set_dos_bit(path: &Path, bit: u32, on: bool) -> AttrOutcome {
    let Some(current) = linux_get_attrs(path) else {
        return AttrOutcome::Unsupported;
    };
    let new = if on { current | bit } else { current & !bit };
    match linux_set_attrs(path, new) {
        Ok(()) => AttrOutcome::Applied,
        Err(e) => AttrOutcome::Failed(e.to_string()),
    }
}

#[cfg(target_os = "linux")]
fn linux_get_attrs(path: &Path) -> Option<u32> {
    use std::os::unix::ffi::OsStrExt;
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
    if fd < 0 {
        return None;
    }
    let mut attrs: u32 = 0;
    let rc = unsafe { libc::ioctl(fd, FAT_IOCTL_GET_ATTRIB, &mut attrs) };
    unsafe { libc::close(fd) };
    if rc == 0 { Some(attrs) } else { None }
}

#[cfg(target_os = "linux")]
fn linux_set_attrs(path: &Path, attrs: u32) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let rc = unsafe { libc::ioctl(fd, FAT_IOCTL_SET_ATTRIB, &attrs as *const u32) };
    let err = if rc == 0 {
        None
    } else {
        Some(io::Error::last_os_error())
    };
    unsafe { libc::close(fd) };
    match err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn get_dos_bit(_path: &Path, _bit: u32, _meta: Option<&fs::Metadata>) -> Option<bool> {
    None
}

#[cfg(not(any(windows, target_os = "linux")))]
fn set_dos_bit(_path: &Path, _bit: u32, _on: bool) -> AttrOutcome {
    AttrOutcome::Unsupported
}

// ── DOS attribute bits stored in `user.DOSATTRIB` ──────────────────────────

/// xattr name used by Samba/CIFS and related drivers to persist DOS attributes.
#[cfg(target_os = "linux")]
const DOSATTRIB_XATTR: &str = "user.DOSATTRIB";

#[cfg(target_os = "linux")]
fn get_dos_xattr(path: &Path, bit: u32, _meta: Option<&fs::Metadata>) -> Option<bool> {
    let raw = linux_get_xattr(path, DOSATTRIB_XATTR)?;
    let attrs = parse_dosattrib(&raw)?;
    Some(attrs & bit != 0)
}

#[cfg(target_os = "linux")]
fn set_dos_xattr(path: &Path, bit: u32, on: bool) -> AttrOutcome {
    let current = linux_get_xattr(path, DOSATTRIB_XATTR)
        .and_then(|raw| parse_dosattrib(&raw))
        .unwrap_or(0);
    let new = if on { current | bit } else { current & !bit };
    let value = format!("0x{new:x}");
    match linux_set_xattr(path, DOSATTRIB_XATTR, value.as_bytes()) {
        Ok(()) => AttrOutcome::Applied,
        Err(e) => AttrOutcome::Failed(e.to_string()),
    }
}

/// Parse a `user.DOSATTRIB` value such as `0x20` into the attribute bits.
#[cfg(target_os = "linux")]
fn parse_dosattrib(raw: &[u8]) -> Option<u32> {
    let s = std::str::from_utf8(raw).ok()?.trim();
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u32::from_str_radix(s.trim(), 16).ok()
}

#[cfg(target_os = "linux")]
fn linux_get_xattr(path: &Path, name: &str) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let cname = std::ffi::CString::new(name).ok()?;
    let size = unsafe { libc::getxattr(cpath.as_ptr(), cname.as_ptr(), std::ptr::null_mut(), 0) };
    if size < 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let read = unsafe {
        libc::getxattr(
            cpath.as_ptr(),
            cname.as_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
        )
    };
    if read < 0 {
        return None;
    }
    buf.truncate(read as usize);
    Some(buf)
}

#[cfg(target_os = "linux")]
fn linux_set_xattr(path: &Path, name: &str, value: &[u8]) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let cname = std::ffi::CString::new(name)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "name contains NUL"))?;
    let rc = unsafe {
        libc::setxattr(
            cpath.as_ptr(),
            cname.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "linux"))]
fn get_dos_xattr(_path: &Path, _bit: u32, _meta: Option<&fs::Metadata>) -> Option<bool> {
    None
}

#[cfg(not(target_os = "linux"))]
fn set_dos_xattr(_path: &Path, _bit: u32, _on: bool) -> AttrOutcome {
    AttrOutcome::Unsupported
}

// ── macOS BSD hidden flag ──────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn get_bsd_hidden(path: &Path, meta: Option<&fs::Metadata>) -> Option<bool> {
    use std::os::macos::fs::MetadataExt;
    let owned;
    let meta = match meta {
        Some(m) => m,
        None => {
            owned = fs::symlink_metadata(path).ok()?;
            &owned
        }
    };
    Some(meta.st_flags() & libc::UF_HIDDEN != 0)
}

#[cfg(target_os = "macos")]
fn set_bsd_hidden(path: &Path, on: bool) -> AttrOutcome {
    use std::os::macos::fs::MetadataExt;
    use std::os::unix::ffi::OsStrExt;

    let cpath = match std::ffi::CString::new(path.as_os_str().as_bytes()) {
        Ok(c) => c,
        Err(e) => return AttrOutcome::Failed(e.to_string()),
    };
    let current = fs::symlink_metadata(path)
        .map(|m| m.st_flags())
        .unwrap_or(0);
    let new = if on {
        current | libc::UF_HIDDEN
    } else {
        current & !libc::UF_HIDDEN
    };
    if unsafe { libc::chflags(cpath.as_ptr(), new) } == 0 {
        AttrOutcome::Applied
    } else {
        AttrOutcome::Failed(io::Error::last_os_error().to_string())
    }
}

#[cfg(not(target_os = "macos"))]
fn get_bsd_hidden(_path: &Path, _meta: Option<&fs::Metadata>) -> Option<bool> {
    None
}

#[cfg(not(target_os = "macos"))]
fn set_bsd_hidden(_path: &Path, _on: bool) -> AttrOutcome {
    AttrOutcome::Unsupported
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::parse_dosattrib;

    #[test]
    fn parse_dosattrib_accepts_0x_prefix_and_bare_hex() {
        assert_eq!(parse_dosattrib(b"0x20"), Some(0x20));
        assert_eq!(parse_dosattrib(b"0X4"), Some(0x4));
        assert_eq!(parse_dosattrib(b" 0x2 \n"), Some(0x2));
        assert_eq!(parse_dosattrib(b"ff"), Some(0xff));
        assert_eq!(parse_dosattrib(b"nonsense"), None);
    }
}
