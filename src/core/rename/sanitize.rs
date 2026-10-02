/// Replace control characters with underscores.
///
/// Trailing spaces/dots are intentionally left alone: Windows strips them
/// automatically when creating the file, and trimming them here would make
/// "remove first/last N" remove more than N characters whenever the cut
/// lands on a space.
///
/// This does NOT guard against:
/// - Windows reserved names (CON, NUL, etc.) — the OS will reject these.
/// - Platform-forbidden characters (\0, <, >, :, etc.) — the OS will reject these.
/// - Path *navigation* (rooted names, `.`/`..` segments) — see
///   [`path_navigation_error`], which is the SSoT for that rule.
pub fn sanitize_file_name(name: &str) -> String {
    let mut s = name.to_string();

    if s.is_empty() || s == "." {
        return "_unnamed".to_string();
    }

    // Control characters (U+0000–U+001F) are invalid everywhere.
    s = s
        .chars()
        .map(|c| if (c as u32) < 0x20 { '_' } else { c })
        .collect();

    s
}

/// Why a computed name cannot be used as a rename target: it would navigate the
/// path instead of only creating subfolders. See [`path_navigation_error`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathNavigation {
    /// Rooted or absolute — a leading separator (`/x`, `\x`) or a drive prefix
    /// (`C:x`, `C:\x`). Joining it would discard the source folder.
    Rooted,
    /// Contains a `.` path segment (`./x`, `sub/./x`).
    CurDir,
    /// Contains a `..` path segment (`../x`, `sub/../x`) — would escape the folder.
    ParentDir,
}

impl PathNavigation {
    /// Short, user-facing reason (shared by the GUI popup and the skip text).
    pub fn message(self) -> &'static str {
        match self {
            PathNavigation::Rooted => "points outside the folder (absolute path)",
            PathNavigation::CurDir => "contains a '.' path segment",
            PathNavigation::ParentDir => "contains a '..' path segment",
        }
    }
}

/// The single validity rule for a computed rename name: a name may only create
/// subfolders (`sub/file`, `sub/deep/file`). It must not *navigate* the path —
/// no rooted/absolute name and no `.` or `..` segment. Returns the reason when
/// the name is invalid, `None` when it is fine.
///
/// This is the SSoT for "this name cannot be executed as a rename target". The
/// executor refuses such names (so a name can never make a rename escape its
/// folder) and the GUI blocks Apply and explains. `sanitize_file_name` lives in
/// this module too: names are only ever made safe here.
///
/// Allowed: `foo.txt`, `sub/foo.txt`, `sub/deep/foo.txt`.
/// Rejected: `./foo.txt`, `.//foo.txt`, `sub/../foo.txt`, `../foo.txt`,
/// `/foo.txt`, `//foo.txt` (and Windows `\foo` / `C:x`).
pub fn path_navigation_error(name: &str) -> Option<PathNavigation> {
    let path = std::path::Path::new(name);
    if path.is_absolute() {
        return Some(PathNavigation::Rooted);
    }
    for component in path.components() {
        match component {
            // `C:x` is drive-relative, so `is_absolute` misses it.
            std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                return Some(PathNavigation::Rooted);
            }
            std::path::Component::ParentDir => return Some(PathNavigation::ParentDir),
            _ => {}
        }
    }
    // `components()` normalizes `.` away, so scan the raw segments for it.
    if name.split(std::path::is_separator).any(|seg| seg == ".") {
        return Some(PathNavigation::CurDir);
    }
    None
}
