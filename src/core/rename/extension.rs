use super::*;

pub(crate) fn apply_extension_op(
    ext: Option<String>,
    mode: ExtensionMode,
    rep: Option<&str>,
    app: Option<&str>,
    rem: bool,
) -> Option<String> {
    let mut e = ext;
    if let Some(r) = rep {
        e = Some(r.to_string());
    }
    if let Some(a) = app {
        e = match e {
            Some(v) => Some(format!("{}.{}", v, a)),
            None => Some(a.to_string()),
        };
    }
    if rem {
        e = None;
    }
    if let Some(val) = e {
        e = Some(apply_extension_case(&val, mode));
    }
    e
}

pub(crate) fn apply_extension_case(s: &str, mode: ExtensionMode) -> String {
    match mode {
        ExtensionMode::Lower => s.to_lowercase(),
        ExtensionMode::Upper => s.to_uppercase(),
        ExtensionMode::Title => {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        }
    }
}

/// Multi-part extensions that the rename pipeline treats as a single
/// extension, so ops and sections keep the whole `.tar.gz` rather than only the
/// final `.gz`. Values include the leading dot and are matched
/// case-insensitively against the end of a name. No entry is a suffix of
/// another, so the order is irrelevant.
const COMPOUND_EXTENSIONS: &[&str] = &[
    ".tar.bz2",
    ".tar.lzma",
    ".tar.zst",
    ".tar.gz",
    ".tar.xz",
    ".tar.lz",
    ".tar.z",
];

/// Byte offset of the dot that begins a recognised compound extension, if
/// `name` ends with one. A non-empty stem is required, so a leading-dot name
/// like `.tar.gz` is still treated as an extensionless dotfile. Boundary-safe:
/// a name whose suffix offset lands mid-character simply does not match.
fn compound_extension_dot(name: &str) -> Option<usize> {
    for suffix in COMPOUND_EXTENSIONS {
        let Some(start) = name.len().checked_sub(suffix.len()) else {
            continue;
        };
        if start > 0
            && let Some(tail) = name.get(start..)
            && tail.eq_ignore_ascii_case(suffix)
        {
            return Some(start);
        }
    }
    None
}

/// Byte offset of the dot that starts the extension, or `None` when the name
/// has no extension (dotfiles and extensionless names included). The single
/// rule both the owned and borrowed splits are built on.
fn extension_dot(name: &str) -> Option<usize> {
    compound_extension_dot(name).or_else(|| name.rfind('.').filter(|&p| p > 0))
}

/// Split a name into (stem, extension) at the last dot (no split for dotfiles
/// like `.gitignore`), except that recognised multi-part extensions
/// (`COMPOUND_EXTENSIONS`, e.g. `.tar.gz`) are kept whole. SSoT for the
/// stem/extension split used by the rename pipeline — the GUI structural diff
/// and the F2 inline-rename preselect use the same rule.
pub fn split_extension(name: &str) -> (String, Option<String>) {
    match extension_dot(name) {
        Some(pos) => (name[..pos].to_string(), Some(name[pos + 1..].to_string())),
        None => (name.to_string(), None),
    }
}

/// Borrowed extension of `name`, using the exact same rule as
/// [`split_extension`]. Allocation-free — use this for display and sorting,
/// where only the extension text is needed.
pub fn extension_str(name: &str) -> Option<&str> {
    extension_dot(name).map(|pos| &name[pos + 1..])
}

/// Stem/extension split for a rename target. Extension identification applies
/// to files only: a directory keeps its whole name as the stem even when the
/// name contains dots, so `my.folder` has no extension. Prefer this over
/// [`split_extension`] wherever `is_dir` is known — it is the SSoT the rename
/// pipeline, the GUI structural diff, and the F2 preselect all share.
pub fn split_extension_for(name: &str, is_dir: bool) -> (String, Option<String>) {
    if is_dir {
        (name.to_string(), None)
    } else {
        split_extension(name)
    }
}

#[cfg(test)]
mod tests {
    use super::split_extension;

    #[test]
    fn borrowed_extension_matches_owned_split() {
        use super::extension_str;
        for name in [
            "photo.jpg",
            "archive.tar.gz",
            "DATA.TAR.GZ",
            "notes.gz",
            ".tar.gz",
            ".gitignore",
            "README",
            "caf\u{e9}.tar.gz",
        ] {
            assert_eq!(
                extension_str(name).map(str::to_string),
                split_extension(name).1,
                "{name}"
            );
        }
    }

    #[test]
    fn directories_have_no_extension() {
        use super::split_extension_for;
        // A dotted directory name is all stem: identification is files-only.
        assert_eq!(
            split_extension_for("my.folder", true),
            ("my.folder".to_string(), None)
        );
        assert_eq!(
            split_extension_for("archive.tar.gz", true),
            ("archive.tar.gz".to_string(), None)
        );
        // Files are unaffected.
        assert_eq!(
            split_extension_for("my.folder", false),
            ("my".to_string(), Some("folder".to_string()))
        );
    }

    #[test]
    fn splits_at_last_dot() {
        assert_eq!(
            split_extension("photo.jpg"),
            ("photo".to_string(), Some("jpg".to_string()))
        );
        assert_eq!(split_extension("README"), ("README".to_string(), None));
        // Dotfiles never split.
        assert_eq!(
            split_extension(".gitignore"),
            (".gitignore".to_string(), None)
        );
    }

    #[test]
    fn keeps_compound_extensions_whole() {
        let ext = |name: &str| split_extension(name).1;
        assert_eq!(ext("archive.tar.gz"), Some("tar.gz".to_string()));
        assert_eq!(ext("pkg.tar.xz"), Some("tar.xz".to_string()));
        assert_eq!(ext("log.tar.zst"), Some("tar.zst".to_string()));
        // Case-insensitive match, extension text preserved verbatim.
        assert_eq!(
            split_extension("DATA.TAR.GZ"),
            ("DATA".to_string(), Some("TAR.GZ".to_string()))
        );
        // A near-miss still splits at the last dot.
        assert_eq!(ext("notes.gz"), Some("gz".to_string()));
        // A bare compound suffix is a dotfile, not an extension.
        assert_eq!(
            split_extension(".tar.gz"),
            (".tar".to_string(), Some("gz".to_string()))
        );
    }

    #[test]
    fn non_ascii_names_do_not_panic() {
        // The suffix-length offset lands inside a multi-byte character.
        assert_eq!(
            split_extension("\u{65e5}\u{672c}\u{8a9e}abcde"),
            ("\u{65e5}\u{672c}\u{8a9e}abcde".to_string(), None)
        );
        assert_eq!(
            split_extension("caf\u{e9}.tar.gz"),
            ("caf\u{e9}".to_string(), Some("tar.gz".to_string()))
        );
    }
}
