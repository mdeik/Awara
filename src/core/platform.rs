//! Platform- and filesystem-level helpers.
//!
//! Attribute mechanics live in [`crate::core::attrs`]; only creation-time
//! support and drive/root enumeration remain here.
//!
//! Creation (birth) time is settable on Windows and macOS (NTFS/ReFS/FAT and
//! APFS/HFS+ expose it); Linux has no portable setter, so it is reported
//! unsupported there even when the filesystem stores a birth time.

use std::path::{Path, PathBuf};

/// Creation (birth) time as `(secs, nanos)` from already-fetched metadata, but
/// only where it can also be set (Windows, macOS) — otherwise `None`.
pub fn creation_time_parts(meta: &std::fs::Metadata) -> Option<(i64, u32)> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        let t = meta.created().ok()?;
        let d = t.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some((d.as_secs() as i64, d.subsec_nanos()))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = meta;
        None
    }
}

/// Set the file creation (birth) time where supported. Returns `false` on
/// platforms without a settable birth time (so callers don't record a value
/// they could never restore).
pub fn set_creation_time(path: &Path, time: std::time::SystemTime) -> bool {
    #[cfg(windows)]
    {
        set_creation_time_windows(path, time)
    }
    #[cfg(target_os = "macos")]
    {
        set_creation_time_macos(path, time)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (path, time);
        false
    }
}

#[cfg(windows)]
fn set_creation_time_windows(path: &Path, time: std::time::SystemTime) -> bool {
    use std::fs::OpenOptions;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct Filetime {
        dw_low_date_time: u32,
        dw_high_date_time: u32,
    }

    unsafe extern "system" {
        fn SetFileTime(
            h_file: *mut std::ffi::c_void,
            lp_creation_time: *const Filetime,
            lp_last_access_time: *const Filetime,
            lp_last_write_time: *const Filetime,
        ) -> i32;
    }

    let ft = filetime::FileTime::from_system_time(time);
    let raw = (ft.seconds() as u64) * 10_000_000 + (ft.nanoseconds() as u64) / 100;
    let ctime = Filetime {
        dw_low_date_time: raw as u32,
        dw_high_date_time: (raw >> 32) as u32,
    };

    let file = match OpenOptions::new().write(true).open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let handle = file.as_raw_handle();

    unsafe { SetFileTime(handle as *mut _, &ctime, std::ptr::null(), std::ptr::null()) != 0 }
}

/// macOS has no `SetFileTime`; birth time is set through `setattrlist` with the
/// `ATTR_CMN_CRTIME` common attribute holding a single `timespec`.
#[cfg(target_os = "macos")]
fn set_creation_time_macos(path: &Path, time: std::time::SystemTime) -> bool {
    use std::os::unix::ffi::OsStrExt;

    const ATTR_BIT_MAP_COUNT: u16 = 5;
    const ATTR_CMN_CRTIME: u32 = 0x0000_0200;

    #[repr(C)]
    struct AttrList {
        bitmapcount: u16,
        reserved: u16,
        commonattr: u32,
        volattr: u32,
        dirattr: u32,
        fileattr: u32,
        forkattr: u32,
    }

    unsafe extern "C" {
        fn setattrlist(
            path: *const libc::c_char,
            attr_list: *const AttrList,
            attr_buf: *mut std::ffi::c_void,
            attr_buf_size: libc::size_t,
            options: libc::c_ulong,
        ) -> libc::c_int;
    }

    let Ok(cpath) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let ft = filetime::FileTime::from_system_time(time);
    let ts = libc::timespec {
        tv_sec: ft.seconds() as libc::time_t,
        tv_nsec: ft.nanoseconds() as libc::c_long,
    };
    let attr_list = AttrList {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ATTR_CMN_CRTIME,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    unsafe {
        setattrlist(
            cpath.as_ptr(),
            &attr_list,
            &ts as *const libc::timespec as *mut std::ffi::c_void,
            std::mem::size_of::<libc::timespec>(),
            0,
        ) == 0
    }
}

/// Return the top-level filesystem roots for the current platform.
/// On Windows, this enumerates all available drives (A:\..Z:\)
/// including network-mapped drives. On Unix, this returns just `/`.
pub fn get_drive_roots() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut drives = Vec::new();
        for letter in 'A'..='Z' {
            let path = PathBuf::from(format!(r"{}:\", letter));
            if path.exists() {
                drives.push(path);
            }
        }
        drives
    }
    #[cfg(not(windows))]
    {
        vec![PathBuf::from("/")]
    }
}

/// Return the path used as the tree root node.
/// On Windows this is an empty path (virtual "This PC" node).
/// On Unix it is the filesystem root `/`.
pub fn tree_root() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::new()
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/")
    }
}
