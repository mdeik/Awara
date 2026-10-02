//! Guards that `docs/cli.md` stays in sync with the real CLI.
//!
//! The option list is read straight from `awara --help` (clap is the single
//! source of truth), so adding/removing a flag fails these tests until the
//! reference is updated — in both directions (missing docs, stale docs).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

/// clap built-ins; intentionally not part of the documented flag reference.
const BUILTINS: &[&str] = &["--help", "--version"];

fn doc_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// Extract every `--long-flag` token from a blob of text.
fn extract_flags(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut flags = BTreeSet::new();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'-' && bytes[i + 1] == b'-' && bytes[i + 2].is_ascii_lowercase() {
            let mut j = i + 2;
            while j < bytes.len() && (bytes[j].is_ascii_lowercase() || bytes[j] == b'-') {
                j += 1;
            }
            flags.insert(text[i..j].to_string());
            i = j;
        } else {
            i += 1;
        }
    }
    flags
}

fn help_flags() -> BTreeSet<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_awara"))
        .arg("--help")
        .output()
        .expect("failed to run `awara --help`");
    assert!(out.status.success(), "`awara --help` exited with failure");
    extract_flags(&String::from_utf8_lossy(&out.stdout))
}

fn documented_flags() -> BTreeSet<String> {
    let doc = std::fs::read_to_string(doc_path("docs/cli.md")).expect("read docs/cli.md");
    extract_flags(&doc)
}

#[test]
fn every_cli_flag_is_documented() {
    let documented = documented_flags();
    let missing: Vec<_> = help_flags()
        .into_iter()
        .filter(|f| !BUILTINS.contains(&f.as_str()))
        .filter(|f| !documented.contains(f))
        .collect();
    assert!(
        missing.is_empty(),
        "CLI flags present in --help but missing from docs/cli.md: {missing:?}"
    );
}

#[test]
fn every_documented_cli_flag_exists() {
    let real = help_flags();
    let stale: Vec<_> = documented_flags()
        .into_iter()
        .filter(|f| !BUILTINS.contains(&f.as_str()))
        .filter(|f| !real.contains(f))
        .collect();
    assert!(
        stale.is_empty(),
        "docs/cli.md references flags that do not exist: {stale:?}"
    );
}
