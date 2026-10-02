use std::path::{Path, PathBuf};

/// Outcome of validating a user-supplied input path.
#[derive(Debug, Clone, PartialEq)]
pub enum InputPathValidation {
    /// Path is valid and points to an accessible filesystem entry.
    Ok(PathBuf),
    /// Input is empty or only whitespace.
    Empty,
    /// Path does not exist on the filesystem.
    NotFound,
    /// Path exists but is not the expected type (file vs directory).
    WrongType,
    /// Path exists but metadata cannot be read (permission denied, etc.).
    NotAccessible(String),
    /// Path length exceeds platform limits.
    PathTooLong { length: usize, limit: usize },
    /// A single path component exceeds the filesystem limit (typically 255 bytes).
    ComponentTooLong {
        component: String,
        length: usize,
        limit: usize,
    },
    /// Path uses a Windows-reserved name (CON, NUL, etc.).
    ReservedName,
    /// Path contains characters forbidden on this platform.
    InvalidChars(String),
    /// Path ends with a trailing space or dot, which is silently stripped on Windows.
    TrailingDotOrSpace,
}

/// Maximum length of a single path component (filename or directory name).
pub(crate) const NAME_MAX: usize = 255;

/// Parse a token of `1..=max_len` ASCII digits with no sign.
///
/// Rejects empty, longer, signed, or non-numeric tokens so malformed input such
/// as `015` or `+5` is never silently reinterpreted as a number.
pub fn parse_digits(s: &str, max_len: usize) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s.len() > max_len || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Parse a 1–2 digit unsigned calendar/clock component.
///
/// Zero-padding to the canonical 2-digit width is allowed (`5` and `05` are
/// equivalent), but longer tokens and signed/non-numeric input are rejected so
/// malformed values such as `015` or `+5` are never silently reinterpreted as a
/// number. Used by the core datetime parser and the GUI timestamp editor so both
/// apply the same token rule.
pub fn parse_short_numeric(s: &str) -> Option<u64> {
    parse_digits(s, 2)
}

/// Maximum recommended path component length per platform.
pub fn platform_max_path() -> usize {
    #[cfg(target_os = "windows")]
    {
        247
    }
    #[cfg(target_os = "macos")]
    {
        1024
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        4096
    }
}

/// Validate a user-supplied input path for common issues.
pub fn validate_input_path(input: &str) -> InputPathValidation {
    let trimmed = input.trim();

    if input.is_empty() || trimmed.is_empty() {
        return InputPathValidation::Empty;
    }

    let path = Path::new(input);

    for component in path.components() {
        let c = component.as_os_str();
        let len = c.len();
        if len > NAME_MAX {
            let name = c.to_string_lossy().to_string();
            return InputPathValidation::ComponentTooLong {
                component: if name.len() > 40 {
                    format!("{}…", &name[..40])
                } else {
                    name
                },
                length: len,
                limit: NAME_MAX,
            };
        }
        let char_len = c.to_string_lossy().chars().count();
        if char_len > NAME_MAX {
            let name = c.to_string_lossy().to_string();
            return InputPathValidation::ComponentTooLong {
                component: if name.len() > 40 {
                    format!("{}…", &name[..40])
                } else {
                    name
                },
                length: char_len,
                limit: NAME_MAX,
            };
        }
    }

    let path_len = input.len();
    let max_path = platform_max_path();
    if path_len > max_path {
        return InputPathValidation::PathTooLong {
            length: path_len,
            limit: max_path,
        };
    }

    if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
        let stem = file_name.split('.').next().unwrap_or("");
        let reserved = [
            "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
            "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
        ];
        if reserved.contains(&stem.to_lowercase().as_str()) {
            return InputPathValidation::ReservedName;
        }
    }

    if input.contains('\0') {
        return InputPathValidation::InvalidChars("path contains null byte (\\0)".to_string());
    }

    if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
        let win_forbidden = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        if let Some(bad) = file_name.chars().find(|c| win_forbidden.contains(c)) {
            return InputPathValidation::InvalidChars(format!(
                "filename component '{file_name}' contains character '{bad}' which is \
                 not allowed on Windows (and may fail on other platforms when targeting \
                 Windows filesystems)",
            ));
        }
        if let Some(bad) = file_name.chars().find(|c| (*c as u32) < 0x20) {
            return InputPathValidation::InvalidChars(format!(
                "filename component '{file_name}' contains control character '{}' (0x{:02X})",
                bad.escape_default(),
                bad as u32,
            ));
        }

        if file_name.ends_with(' ') || file_name.ends_with('.') {
            return InputPathValidation::TrailingDotOrSpace;
        }
    }

    let is_broken_symlink = path.symlink_metadata().is_ok() && path.metadata().is_err();
    if is_broken_symlink {
        return InputPathValidation::NotAccessible(format!(
            "'{}' is a symlink pointing to a non-existent target",
            input
        ));
    }

    match path.metadata() {
        Ok(_) => {
            let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            InputPathValidation::Ok(canon)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => InputPathValidation::NotFound,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            InputPathValidation::NotAccessible(e.to_string())
        }
        Err(e) => InputPathValidation::NotAccessible(e.to_string()),
    }
}

/// Validate a user-supplied path and check it matches the expected filesystem type.
pub fn validate_input_path_as(input: &str, expect_dir: bool) -> InputPathValidation {
    let result = validate_input_path(input);
    if let InputPathValidation::Ok(ref path) = result {
        if expect_dir {
            if !path.is_dir() {
                return InputPathValidation::WrongType;
            }
        } else if path.is_dir() || !path.is_file() {
            return InputPathValidation::WrongType;
        }
    }
    result
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_short_numeric_allows_2_digit_padding_only() {
        assert_eq!(parse_short_numeric("5"), Some(5));
        assert_eq!(parse_short_numeric("05"), Some(5));
        assert_eq!(parse_short_numeric("59"), Some(59));
        assert_eq!(parse_short_numeric(" 5 "), Some(5));
        for bad in ["", "005", "015", "+5", "-5", "5.0", "abc", "5a"] {
            assert_eq!(parse_short_numeric(bad), None, "should reject {bad:?}");
        }
    }
}
