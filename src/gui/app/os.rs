use super::*;
#[cfg(target_os = "macos")]
use trash::macos::TrashContextExtMacos;

pub(crate) fn trash_file(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut ctx = trash::TrashContext::new();
        ctx.set_delete_method(trash::macos::DeleteMethod::NsFileManager);
        ctx.delete(path)
            .map_err(|e| format!("Failed to trash '{}': {}", path.display(), e))
    }
    #[cfg(not(target_os = "macos"))]
    {
        trash::delete(path).map_err(|e| format!("Failed to trash '{}': {}", path.display(), e))
    }
}

/// Copy text to the clipboard — shared helper used by both main table and revert dialog.
pub(crate) fn copy_text(text: &str, ctx: &egui::Context) {
    ctx.copy_text(text.to_owned());
}

/// Open a file or directory using the OS default application / file manager.
/// Open the parent directory of `path` in the file manager, selecting the file.
/// Does nothing if `path` has no parent.
pub(crate) fn open_parent_in_os(path: &Path) {
    open_parent_and_select(&[path]);
}

/// Open the containing folder(s) in the file manager, selecting the given files.
/// Groups by parent directory — one window per distinct parent.
/// Platform selection support:
/// - Linux:  detects default FM via `xdg-mime` — uses `--select` where supported
/// - macOS:  `open -R`        (one file per call; multi-file falls back to parent)
/// - Win:    `explorer /select` (one file per call; multi-file falls back to parent)
///
/// Fallback: opens parent via `open_in_os` (default file manager, no selection).
pub(crate) fn open_parent_and_select(paths: &[&Path]) {
    let mut by_parent: std::collections::BTreeMap<&Path, Vec<&Path>> =
        std::collections::BTreeMap::new();
    for p in paths {
        if let Some(parent) = p.parent() {
            by_parent.entry(parent).or_default().push(p);
        }
    }
    for (&parent, files) in &by_parent {
        if !try_select_in_fm(files) {
            open_in_os(parent);
        }
    }
}

/// Detect the default file manager and launch it with selection for the given files.
/// Returns `true` if the file manager was launched successfully.
#[cfg(target_os = "linux")]
fn try_default_fm_selection(files: &[&Path]) -> bool {
    // Query xdg-mime for the default handler of inode/directory
    let desktop = std::process::Command::new("xdg-mime")
        .args(["query", "default", "inode/directory"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_lowercase();
                (!s.is_empty()).then_some(s)
            } else {
                None
            }
        });

    let Some(desktop) = desktop else {
        return false;
    };

    // Map .desktop file names to their selection commands
    // Only supports FMs known to have a --select flag.
    macro_rules! fm_select {
        ($name:literal, $cmd:expr) => {
            if desktop.contains($name) {
                return $cmd;
            }
        };
    }

    fm_select!("dolphin", {
        // Dolphin handles multiple files in one window
        std::process::Command::new("dolphin")
            .arg("--select")
            .args(files)
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
    });

    fm_select!("nautilus", {
        // Nautilus --select accepts one path; for multiples, just open parent
        if files.len() == 1 {
            std::process::Command::new("nautilus")
                .args(["--select", &files[0].to_string_lossy()])
                .spawn()
                .is_ok()
        } else {
            false
        }
    });

    fm_select!("thunar", {
        // Thunar selects the file when passed directly
        if files.len() == 1 {
            std::process::Command::new("thunar")
                .arg(files[0])
                .spawn()
                .is_ok()
        } else {
            false
        }
    });

    fm_select!("nemo", {
        // Nemo --select accepts one path
        if files.len() == 1 {
            std::process::Command::new("nemo")
                .args(["--select", &files[0].to_string_lossy()])
                .spawn()
                .is_ok()
        } else {
            false
        }
    });

    // Unsupported FM — let caller fall through to xdg-open
    false
}

/// Try to open the parent with files selected in the native file manager.
/// Returns `true` if selection was launched (caller skips the plain parent open).
/// Only selects when it can be done in a single window — on macOS/Windows the
/// selection commands only accept one path, so multi-file falls through to the
/// caller opening the parent dir once.
fn try_select_in_fm(files: &[&Path]) -> bool {
    if files.is_empty() {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        // Detect the default FM via xdg-mime and use its selection flag.
        try_default_fm_selection(files)
    }
    #[cfg(target_os = "macos")]
    {
        // open -R only accepts one path; only offer selection for a single file
        if files.len() == 1 {
            std::process::Command::new("open")
                .args(["-R", &files[0].to_string_lossy()])
                .spawn()
                .is_ok()
        } else {
            false
        }
    }
    #[cfg(target_os = "windows")]
    {
        // Use the Shell API instead of `explorer /select`, which has a known
        // bug on Windows 10/11 where it opens to the default location (Quick
        // Access / This PC) instead of the target file.
        win_show_folder_and_select(files)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = files;
        false
    }
}

/// Opens parent folder in Explorer and selects the given files using `SHOpenFolderAndSelectItems`.
/// This is the proper Windows Shell API — far more reliable than `explorer /select,path`
/// which has a known bug on Windows 10/11 (opens to default location instead).
///
/// When multiple files share the same parent, only the first is selected (per the
/// MSDN recommendation for simple usage); the parent folder itself is always opened.
#[cfg(target_os = "windows")]
fn win_show_folder_and_select(files: &[&Path]) -> bool {
    if files.is_empty() {
        return false;
    }

    let path = files[0];

    // Build a null-terminated wide path.
    let wide: Vec<u16> = std::os::windows::ffi::OsStrExt::encode_wide(path.as_os_str())
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        // CoInitializeEx returns S_OK (0) or S_FALSE (1) on success
        let hr = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
        if hr != 0 && hr != 1 {
            return false;
        }

        // Parse the path into an absolute PIDL
        let mut pidl: PidlistAbsolute = std::ptr::null_mut();
        let hr = SHParseDisplayName(
            wide.as_ptr(),
            std::ptr::null_mut(),
            &mut pidl,
            0,
            std::ptr::null_mut(),
        );
        if hr != 0 || pidl.is_null() {
            CoUninitialize();
            return false;
        }

        // cidl=0, apidl=null → open parent and select the item
        let hr = SHOpenFolderAndSelectItems(pidl, 0, std::ptr::null(), 0);
        ILFree(pidl);
        CoUninitialize();
        hr == 0
    }
}

// ── Windows Shell API FFI declarations ─────────────────────────────────────
#[cfg(target_os = "windows")]
type Dword = u32;
#[cfg(target_os = "windows")]
type Hresult = i32;
#[cfg(target_os = "windows")]
type Lpcwstr = *const u16;
#[cfg(target_os = "windows")]
type PidlistAbsolute = *mut std::ffi::c_void;
#[cfg(target_os = "windows")]
type PcuitemidChild = *const std::ffi::c_void;
#[cfg(target_os = "windows")]
type PcidlistAbsolute = *const std::ffi::c_void;

#[cfg(target_os = "windows")]
const COINIT_APARTMENTTHREADED: Dword = 2;

#[cfg(target_os = "windows")]
#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(pvReserved: *mut std::ffi::c_void, dwCoInit: Dword) -> Hresult;
    fn CoUninitialize();
}

#[cfg(target_os = "windows")]
#[link(name = "shell32")]
unsafe extern "system" {
    fn SHParseDisplayName(
        pszName: Lpcwstr,
        pbc: *mut std::ffi::c_void,
        ppidl: *mut PidlistAbsolute,
        sfgaoIn: Dword,
        psfgaoOut: *mut Dword,
    ) -> Hresult;
    fn SHOpenFolderAndSelectItems(
        pidlFolder: PcidlistAbsolute,
        cidl: u32,
        apidl: *const PcuitemidChild,
        dwFlags: Dword,
    ) -> Hresult;
    fn ILFree(pidl: *mut std::ffi::c_void);
}

pub(crate) fn open_in_os(path: &Path) {
    #[cfg(target_os = "linux")]
    {
        // Suppress stderr to hide harmless KDE/KIO "Failed to lock recently used"
        // warnings — see https://bugs.kde.org/show_bug.cgi?id=455445
        // These are cosmetic: KIO tries to bookmark the accessed path, can't get a
        // lock on the recently-used file, prints the warning, but the open succeeds.
        let _ = std::process::Command::new("xdg-open")
            .arg(path)
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", &path.to_string_lossy()])
            .spawn();
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}
