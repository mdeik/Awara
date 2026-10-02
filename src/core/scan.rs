use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;
use walkdir::WalkDir;

use crate::core::attrs::is_hidden;
use clap::ValueEnum;

/// How to filter entries by filesystem type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum EntryFilter {
    Files,
    Folders,
    Both,
}

/// Sort criterion for file discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum ScanSortBy {
    Name,
    NameDesc,
    Date,
    DateDesc,
    Size,
    SizeDesc,
    Extension,
    ExtensionDesc,
}

/// Options for scanning/filtering files.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub recursive: bool,
    pub max_depth: Option<usize>,
    pub show_hidden: bool,
    pub entry_filter: EntryFilter,
    pub filter_pattern: Option<String>,
    pub exclude_regex: Option<String>,
    pub min_name_len: Option<usize>,
    pub max_name_len: Option<usize>,
    pub min_path_len: Option<usize>,
    pub max_path_len: Option<usize>,
    pub filter_attr: Option<String>,
    pub filter_use_regex: bool,
    pub filter_match_case: bool,
    pub sort_by: Option<ScanSortBy>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            recursive: false,
            max_depth: None,
            show_hidden: false,
            entry_filter: EntryFilter::Files,
            filter_pattern: None,
            exclude_regex: None,
            min_name_len: None,
            max_name_len: None,
            min_path_len: None,
            max_path_len: None,
            filter_attr: None,
            filter_use_regex: false,
            filter_match_case: false,
            sort_by: Some(ScanSortBy::Name),
        }
    }
}

/// Apply all file filters to a single path. Returns `true` if the path should be included.
/// Shared glob/regex mask matcher — the single source of truth used by both
/// the scanner ([`filter_path`]) and the GUI's in-memory filter. A pattern
/// that fails to compile falls back to "match" (true), as both callers
/// originally did.
///
/// In regex mode the pattern is compiled verbatim, so matching is
/// case-sensitive; opt into case-insensitivity with an inline `(?i)`. The
/// `match_case` flag therefore only governs the glob branch (glob masks are
/// case-insensitive unless `match_case` is set).
pub fn mask_matches(name: &str, pattern: &str, use_regex: bool, match_case: bool) -> bool {
    if use_regex {
        crate::core::rename::build_regex(pattern)
            .map(|re| re.is_match(name))
            .unwrap_or(true)
    } else if match_case {
        glob::Pattern::new(pattern)
            .map(|matcher| matcher.matches(name))
            .unwrap_or(true)
    } else {
        glob::Pattern::new(&pattern.to_lowercase())
            .map(|matcher| matcher.matches(&name.to_lowercase()))
            .unwrap_or(true)
    }
}

/// Shared exclude-regex filter — `true` when `name` is not excluded. An
/// empty or unparseable pattern never excludes. The single source of truth
/// used by both the scanner and the GUI's in-memory filter.
pub fn exclude_regex_passes(name: &str, pattern: Option<&str>) -> bool {
    match pattern {
        Some(pat) if !pat.is_empty() => crate::core::rename::build_regex(pat)
            .map(|re| !re.is_match(name))
            .unwrap_or(true),
        _ => true,
    }
}

/// Shared min/max name/path length filter — `true` when the name and path
/// lengths fall inside the configured bounds. The single source of truth
/// used by both the scanner and the GUI's in-memory filter.
pub fn length_filters_pass(
    name_chars: usize,
    path_chars: usize,
    min_name_len: Option<usize>,
    max_name_len: Option<usize>,
    min_path_len: Option<usize>,
    max_path_len: Option<usize>,
) -> bool {
    if let Some(min) = min_name_len
        && name_chars < min
    {
        return false;
    }
    if let Some(max) = max_name_len
        && name_chars > max
    {
        return false;
    }
    if let Some(min) = min_path_len
        && path_chars < min
    {
        return false;
    }
    if let Some(max) = max_path_len
        && path_chars > max
    {
        return false;
    }
    true
}

pub fn filter_path(path: &Path, opts: &ScanOptions) -> bool {
    let is_file = path.is_file();
    let is_dir = path.is_dir();

    let include_type = match opts.entry_filter {
        EntryFilter::Files => is_file,
        EntryFilter::Folders => is_dir,
        EntryFilter::Both => is_file || is_dir,
    };
    if !include_type {
        return false;
    }

    // Hidden filtering lives here (not only in `scan_directory`) so direct
    // callers such as the CLI's glob path honour `show_hidden` too.
    if !opts.show_hidden && is_hidden(path) {
        return false;
    }

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();

    if let Some(ref pattern) = opts.filter_pattern
        && !pattern.is_empty()
        && !mask_matches(
            &name,
            pattern,
            opts.filter_use_regex,
            opts.filter_match_case,
        )
    {
        return false;
    }
    if !exclude_regex_passes(&name, opts.exclude_regex.as_deref()) {
        return false;
    }
    let name_chars = name.chars().count();
    let path_chars = path.to_string_lossy().chars().count();
    if !length_filters_pass(
        name_chars,
        path_chars,
        opts.min_name_len,
        opts.max_name_len,
        opts.min_path_len,
        opts.max_path_len,
    ) {
        return false;
    }

    if let Some(ref attr) = opts.filter_attr
        && !attr.is_empty()
    {
        let lower = attr.to_ascii_lowercase();
        match lower.as_str() {
            "file" => {
                if !path.is_file() {
                    return false;
                }
            }
            "dir" | "directory" | "folder" => {
                if !path.is_dir() {
                    return false;
                }
            }
            _ => {
                // Attribute filters route through the capability layer (SSOT),
                // and the accepted names/aliases come from the shared schema.
                if let Some(name) = crate::core::schema::canonical_attribute_name(&lower) {
                    let matched = if name == crate::core::schema::ATTR_HIDDEN {
                        is_hidden(path)
                    } else {
                        crate::core::attrs::caps_for_read(path).get(path, name) == Some(true)
                    };
                    if !matched {
                        return false;
                    }
                }
            }
        }
    }

    true
}

/// Compare two strings with natural (human) ordering matching the OS file
/// managers (Windows Explorer's `StrCmpLogicalW`): numeric segments are
/// compared numerically, so "img2" sorts before "img10", and other
/// characters are compared case-insensitively, so punctuation sorts before
/// letters ("[Alpha Group]" before "Example.Show") and "Zebra"/"zebra" sort
/// together.
///
/// Uses byte-level access internally — no heap allocation per call.
pub fn nat_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let mut i = 0;
    let mut j = 0;

    loop {
        match (i >= a_bytes.len(), j >= b_bytes.len()) {
            (true, true) => return std::cmp::Ordering::Equal,
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }

        let a_is_digit = a_bytes[i].is_ascii_digit();
        let b_is_digit = b_bytes[j].is_ascii_digit();

        if a_is_digit && b_is_digit {
            // Consume full numeric runs using byte-level scanning.
            let ia_end = i + a_bytes[i..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            let jb_end = j + b_bytes[j..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();

            let na = &a[i..ia_end];
            let nb = &b[j..jb_end];

            // Strip leading zeros for numeric comparison, but keep at least one digit.
            let na_stripped = na.trim_start_matches('0');
            let nb_stripped = nb.trim_start_matches('0');
            let na_val = if na_stripped.is_empty() {
                "0"
            } else {
                na_stripped
            };
            let nb_val = if nb_stripped.is_empty() {
                "0"
            } else {
                nb_stripped
            };

            // Compare by length first (faster than parsing), then lexicographically.
            match na_val.len().cmp(&nb_val.len()) {
                std::cmp::Ordering::Equal => match na_val.cmp(nb_val) {
                    std::cmp::Ordering::Equal => {
                        i = ia_end;
                        j = jb_end;
                        continue;
                    }
                    other => return other,
                },
                other => return other,
            }
        } else {
            // Non-digit: compare case-insensitively (like the OS's `towlower`
            // per character), with multi-byte UTF-8 handled via `char`. This
            // makes punctuation sort before letters, matching Explorer.
            let ca = a[i..].chars().next().unwrap();
            let cb = b[j..].chars().next().unwrap();
            let la = ca.to_lowercase().next().unwrap_or(ca);
            let lb = cb.to_lowercase().next().unwrap_or(cb);
            match la.cmp(&lb) {
                std::cmp::Ordering::Equal => {
                    i += ca.len_utf8();
                    j += cb.len_utf8();
                }
                other => return other,
            }
        }
    }
}

/// Extension text of `path` by the shared rename rule (normal + compound
/// extensions, so `.tar.gz` is one extension), or `None` when it has none.
/// Non-UTF-8 names report no extension.
fn path_extension(path: &Path) -> Option<&str> {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(crate::core::rename::extension_str)
}

/// Ascending column comparator for one sort mode. DESC modes share the same
/// ascending comparator — descending order is applied by reversing the fully
/// grouped result in `sort_paths`, exactly like the GUI preview table.
fn compare_asc(a: &Path, b: &Path, sort_by: ScanSortBy) -> std::cmp::Ordering {
    match sort_by {
        ScanSortBy::Name | ScanSortBy::NameDesc => {
            let an = a
                .file_name()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();
            let bn = b
                .file_name()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();
            nat_cmp(&an, &bn)
        }
        ScanSortBy::Date | ScanSortBy::DateDesc => {
            let ta = a
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let tb = b
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            ta.cmp(&tb)
        }
        ScanSortBy::Size | ScanSortBy::SizeDesc => {
            let sa = a.metadata().map(|m| m.len()).unwrap_or(0);
            let sb = b.metadata().map(|m| m.len()).unwrap_or(0);
            sa.cmp(&sb)
        }
        ScanSortBy::Extension | ScanSortBy::ExtensionDesc => {
            // SSoT with the rename pipeline: normal + compound extensions, and
            // directories have none (handled by the caller's group split).
            path_extension(a).cmp(&path_extension(b))
        }
    }
}

/// Sort paths in-place by the specified criterion, grouping folders before
/// files in ascending order (file-browser style) and after files in
/// descending order — mirroring the GUI preview table's sort behavior
/// (`GuiApp::sorted_indices`).
///
/// Within each group entries are sorted by the criterion (names use natural
/// ordering, so `img2` sorts before `img10`). Descending modes reverse the
/// fully grouped order, so the folder group moves below the file group.
pub fn sort_paths(paths: &mut [PathBuf], opts: &ScanOptions) {
    let sort_by = match opts.sort_by {
        Some(s) => s,
        None => return,
    };
    let asc = matches!(
        sort_by,
        ScanSortBy::Name | ScanSortBy::Date | ScanSortBy::Size | ScanSortBy::Extension
    );

    // Stable partition: all folders first, then files. `sort_by_cached_key`
    // computes the is_dir() key at most once per entry, so grouping adds only
    // O(n) stat calls instead of one per comparison.
    paths.sort_by_cached_key(|p| !p.is_dir());
    let split = paths.iter().take_while(|p| p.is_dir()).count();

    // Sort within each group separately (folders and files), same as the GUI.
    // Directories have no extension, so for the Extension sort the folder group
    // keeps scan order (every key is `None`) — skip it rather than compare
    // folders by their dotted names.
    let (dirs, files) = paths.split_at_mut(split);
    if !matches!(sort_by, ScanSortBy::Extension | ScanSortBy::ExtensionDesc) {
        dirs.sort_by(|a, b| compare_asc(a, b, sort_by));
    }
    files.sort_by(|a, b| compare_asc(a, b, sort_by));

    // DESC reverses the fully grouped order: files above, folders below, each
    // group sorted descending — identical to the GUI preview's reverse.
    if !asc {
        paths.reverse();
    }
}

/// Scan a directory, filter, and sort files. Returns filtered and sorted paths.
pub fn scan_directory(
    path: &Path,
    opts: &ScanOptions,
    cancel_flag: Option<&AtomicBool>,
) -> Vec<PathBuf> {
    let estimated = if opts.recursive { 4096 } else { 512 };
    let mut entries = Vec::with_capacity(estimated);

    if opts.recursive {
        let mut walker = WalkDir::new(path).contents_first(true);
        if let Some(depth) = opts.max_depth {
            walker = walker.max_depth(depth);
        }
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            if let Some(cf) = cancel_flag
                && cf.load(Ordering::Relaxed)
            {
                break;
            }
            let entry_path = entry.path();
            if entry_path == path {
                continue;
            }
            if !opts.show_hidden && is_hidden(entry_path) {
                continue;
            }
            if !filter_path(entry_path, opts) {
                continue;
            }
            entries.push(entry_path.to_path_buf());
        }
    } else {
        if let Ok(read_dir) = std::fs::read_dir(path) {
            for entry in read_dir.flatten() {
                if let Some(cf) = cancel_flag
                    && cf.load(Ordering::Relaxed)
                {
                    break;
                }
                let entry_path = entry.path();
                if !opts.show_hidden && is_hidden(&entry_path) {
                    continue;
                }
                if !filter_path(&entry_path, opts) {
                    continue;
                }
                entries.push(entry_path);
            }
        }
    }

    sort_paths(&mut entries, opts);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn opts(sort_by: ScanSortBy) -> ScanOptions {
        ScanOptions {
            sort_by: Some(sort_by),
            ..Default::default()
        }
    }

    /// The mask treats empty like unset (no filtering), matching the GUI's
    /// in-memory filter and the exclude_regex guard — an empty glob pattern
    /// would otherwise match only the empty name and hide every file.
    #[test]
    fn test_filter_path_empty_mask_is_no_filter() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("file.txt");
        fs::write(&f, "").unwrap();
        let base = ScanOptions {
            entry_filter: EntryFilter::Files,
            ..Default::default()
        };
        assert!(filter_path(&f, &base), "no mask: everything passes");
        assert!(
            filter_path(
                &f,
                &ScanOptions {
                    filter_pattern: Some(String::new()),
                    ..base.clone()
                }
            ),
            "empty mask: no filtering"
        );
        assert!(
            filter_path(
                &f,
                &ScanOptions {
                    filter_pattern: Some("*".to_string()),
                    ..base.clone()
                }
            ),
            "default '*' mask matches everything"
        );
        assert!(
            !filter_path(
                &f,
                &ScanOptions {
                    filter_pattern: Some("*.rs".to_string()),
                    ..base
                }
            ),
            "a real mask filters"
        );
    }

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    /// Hidden filtering is part of `filter_path` so direct callers (the CLI's
    /// glob path) honour `show_hidden`, not only `scan_directory`.
    #[test]
    fn test_filter_path_respects_show_hidden() {
        let dir = TempDir::new().unwrap();
        let visible = dir.path().join("secret.txt");
        fs::write(&visible, "").unwrap();

        // Mark the file hidden using the volume's convention: a leading dot on
        // Unix (the rename returns the new path), the hidden attribute on
        // Windows.
        let to_hide = dir.path().join("hidden_secret.txt");
        fs::write(&to_hide, "").unwrap();
        let hidden = crate::core::execute::set_file_attributes(&to_hide, "hidden");

        let base = ScanOptions {
            entry_filter: EntryFilter::Files,
            ..Default::default()
        };
        assert!(!filter_path(&hidden, &base), "hidden excluded by default");
        assert!(filter_path(&visible, &base), "visible included");

        let show = ScanOptions {
            show_hidden: true,
            ..base
        };
        assert!(
            filter_path(&hidden, &show),
            "hidden included when show_hidden is set"
        );
    }

    fn read_all(dir: &TempDir) -> Vec<PathBuf> {
        fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect()
    }

    #[test]
    fn test_sort_paths_groups_folders_first_asc() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("zdir")).unwrap();
        fs::create_dir(dir.path().join("adir")).unwrap();
        fs::write(dir.path().join("file2.txt"), "").unwrap();
        fs::write(dir.path().join("file10.txt"), "").unwrap();
        fs::write(dir.path().join("file1.txt"), "").unwrap();

        let mut paths = read_all(&dir);
        sort_paths(&mut paths, &opts(ScanSortBy::Name));

        assert_eq!(
            names(&paths),
            vec!["adir", "zdir", "file1.txt", "file2.txt", "file10.txt"],
            "folders grouped above files; natural number ordering within groups"
        );
    }

    #[test]
    fn test_sort_paths_groups_folders_last_desc() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("adir")).unwrap();
        fs::write(dir.path().join("file1.txt"), "").unwrap();
        fs::write(dir.path().join("file10.txt"), "").unwrap();
        fs::write(dir.path().join("file2.txt"), "").unwrap();

        let mut paths = read_all(&dir);
        sort_paths(&mut paths, &opts(ScanSortBy::NameDesc));

        assert_eq!(
            names(&paths),
            vec!["file10.txt", "file2.txt", "file1.txt", "adir"],
            "desc reverses the grouped order: files above, folders below, names descending"
        );
    }

    #[test]
    fn test_sort_paths_files_only_keeps_natural_ordering() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("img2.png"), "").unwrap();
        fs::write(dir.path().join("img10.png"), "").unwrap();
        fs::write(dir.path().join("img1.png"), "").unwrap();

        let mut paths = read_all(&dir);
        sort_paths(&mut paths, &opts(ScanSortBy::Name));
        assert_eq!(names(&paths), vec!["img1.png", "img2.png", "img10.png"]);

        sort_paths(&mut paths, &opts(ScanSortBy::NameDesc));
        assert_eq!(names(&paths), vec!["img10.png", "img2.png", "img1.png"]);
    }

    #[test]
    fn test_sort_paths_extension_uses_shared_rule_and_skips_folders() {
        let dir = TempDir::new().unwrap();
        // A dotted folder name has no extension, so it must not sort as "folder".
        fs::create_dir(dir.path().join("a.folder")).unwrap();
        fs::write(dir.path().join("a.txt"), "").unwrap();
        // A compound extension is one key ("tar.gz"), not "gz".
        fs::write(dir.path().join("b.tar.gz"), "").unwrap();
        fs::write(dir.path().join("c.zip"), "").unwrap();

        let mut paths = read_all(&dir);
        sort_paths(&mut paths, &opts(ScanSortBy::Extension));

        assert_eq!(
            names(&paths),
            vec!["a.folder", "b.tar.gz", "a.txt", "c.zip"],
            "folders first (extensionless), then files by normal/compound extension"
        );
    }

    /// Regex masks are case-sensitive; `match_case` only governs glob masks.
    #[test]
    fn test_mask_regex_is_case_sensitive() {
        assert!(
            !mask_matches("README.TXT", "readme", true, false),
            "regex mask is case-sensitive even with match_case=false"
        );
        assert!(
            mask_matches("README.TXT", "(?i)readme", true, false),
            "inline (?i) opts into case-insensitive regex matching"
        );
        // Glob masks keep their existing match_case behavior.
        assert!(mask_matches("README.TXT", "readme*", false, false));
        assert!(!mask_matches("README.TXT", "readme*", false, true));
    }

    /// Exclude is case-sensitive; inline (?i) opts into case-insensitivity.
    #[test]
    fn test_exclude_regex_is_case_sensitive() {
        assert!(
            exclude_regex_passes("SKIP.txt", Some("skip")),
            "case-sensitive: 'skip' does not exclude 'SKIP.txt'"
        );
        assert!(
            !exclude_regex_passes("SKIP.txt", Some("(?i)skip")),
            "inline (?i) excludes case-insensitively"
        );
    }
}
