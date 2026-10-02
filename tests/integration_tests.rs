use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Parse JSON undo file bytes back into ops for verification.
fn parse_undo_json(data: &str) -> Vec<awara::RenameOp> {
    serde_json::from_str(data).expect("undo JSON should be valid")
}

fn get_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_awara"))
}

#[test]
fn test_basic_rename() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("test_file.txt");
    fs::write(&file_path, "content").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--replace")
        .arg("test")
        .arg("--with")
        .arg("renamed")
        .assert()
        .success();

    assert!(!file_path.exists());
    assert!(temp.path().join("renamed_file.txt").exists());
}

#[test]
fn test_dry_run() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("dry_run.txt");
    fs::write(&file_path, "content").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--replace")
        .arg("dry")
        .arg("--with")
        .arg("wet")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("'dry_run.txt' -> 'wet_run.txt'"));

    assert!(file_path.exists()); // Should still exist
    assert!(!temp.path().join("wet_run.txt").exists());
}

#[test]
fn test_undo_file_json() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("undo_me.txt");
    fs::write(&file_path, "content").unwrap();
    let undo_file = temp.path().join("undo.json");

    // Rename: undo_me.txt -> done.txt, saving undo info as JSON
    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--replace")
        .arg("undo_me")
        .arg("--with")
        .arg("done")
        .arg("--undo-file")
        .arg(undo_file.to_str().unwrap())
        .assert()
        .success();

    assert!(
        temp.path().join("done.txt").exists(),
        "file should be renamed"
    );
    assert!(undo_file.exists(), "undo file should exist");

    // Apply the undo file — runs on ALL platforms now
    let mut undo_cmd = get_cmd();
    undo_cmd
        .arg("--apply-undo")
        .arg(undo_file.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("Undo complete"));

    assert!(
        file_path.exists(),
        "original file should be restored after undo"
    );
    assert!(
        !temp.path().join("done.txt").exists(),
        "renamed file should no longer exist after undo"
    );
}

#[test]
fn test_undo_file_special_chars() {
    let temp = TempDir::new().unwrap();
    // Filenames with spaces, emoji, brackets, and quotes
    let file_path = temp.path().join("my  file (1) 🎉.txt");
    fs::write(&file_path, "hello").unwrap();
    let undo_file = temp.path().join("undo.json");

    // Rename using a regex that matches special chars
    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--regex-match")
        .arg("🎉")
        .arg("--regex-replace")
        .arg("✨")
        .arg("--undo-file")
        .arg(undo_file.to_str().unwrap())
        .assert()
        .success();

    let expected = temp.path().join("my  file (1) ✨.txt");
    assert!(expected.exists(), "file should be renamed with new emoji");
    assert!(undo_file.exists(), "undo JSON file should exist");

    // Verify the JSON is well-formed by reading it back
    let content = fs::read_to_string(&undo_file).unwrap();
    let ops = parse_undo_json(&content);
    assert_eq!(ops.len(), 1, "should have one undo op");

    // Apply the undo — restores original filename
    let mut undo_cmd = get_cmd();
    undo_cmd
        .arg("--apply-undo")
        .arg(undo_file.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("Undo complete"));

    assert!(
        file_path.exists(),
        "original file with special chars should be restored"
    );
    assert!(!expected.exists(), "renamed file should no longer exist");
}

#[test]
fn test_collision_handling_internal() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.txt"), "").unwrap();
    fs::write(temp.path().join("b.txt"), "").unwrap();

    // Rename both to 'c.txt' — pipeline now uses Skip strategy by default
    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--replace")
        .arg(".*")
        .arg("--with")
        .arg("c") // Regex replace all with c
        .arg("--regex-match")
        .arg(".*") // Need regex mode for this
        .arg("--regex-replace")
        .arg("c")
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipping"));

    // First file renamed, second skipped due to collision
    assert!(temp.path().join("c.txt").exists());
    // b.txt was skipped because it would also become c.txt
    assert!(temp.path().join("b.txt").exists());
}

#[test]
fn test_case_only_rename() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("case.txt");
    fs::write(&file_path, "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("case.txt").to_str().unwrap())
        .arg("--case-name")
        .arg("upper")
        .assert()
        .success();

    // On case-insensitive FS, "CASE.txt" and "case.txt" are same file.
    // But we want to ensure the name IS changed in the directory entry.
    // Rust's `exists` might return true for both on Windows/macOS.
    // We can check `fs::read_dir` to see the actual casing.

    let mut found = false;
    for entry in fs::read_dir(temp.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == "CASE.txt" {
            found = true;
        }
    }
    assert!(found, "File should be renamed to CASE.txt");
}

#[test]
fn test_name_mode_custom() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("old_name.txt"), "").unwrap();

    // --name-mode fixed replaces the entire filename (including extension)
    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("old_name.txt").to_str().unwrap())
        .arg("--name-mode")
        .arg("fixed")
        .arg("--name-value")
        .arg("entirely_new")
        .assert()
        .success();

    assert!(!temp.path().join("old_name.txt").exists());
    assert!(
        temp.path().join("entirely_new.txt").exists(),
        "Fixed mode replaces the stem, preserving extension"
    );
}

#[test]
fn test_name_mode_stem() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("data.txt"), "").unwrap();

    // --name-mode fixed replaces the entire filename (including extension)
    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("data.txt").to_str().unwrap())
        .arg("--name-mode")
        .arg("fixed")
        .arg("--name-value")
        .arg("new_name")
        .assert()
        .success();

    assert!(
        temp.path().join("new_name.txt").exists(),
        "Fixed mode should replace the stem, preserving extension"
    );
    assert!(!temp.path().join("data.txt").exists());
}

#[test]
fn test_collision_handling_external() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.txt"), "content a").unwrap();
    fs::write(temp.path().join("b.txt"), "content b").unwrap();

    // Rename a.txt -> b.txt (External collision) — pipeline skips by default
    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("a.txt").to_str().unwrap())
        .arg("--replace")
        .arg("a")
        .arg("--with")
        .arg("b")
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipping"));

    // a.txt was NOT renamed (skipped due to collision with existing b.txt)
    assert!(temp.path().join("a.txt").exists());
    assert!(temp.path().join("b.txt").exists());
    // Verify content didn't change
    let content = fs::read_to_string(temp.path().join("b.txt")).unwrap();
    assert_eq!(content, "content b");
}

#[test]
fn test_collision_handling_overwrite_flag() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.txt"), "content a").unwrap();
    fs::write(temp.path().join("b.txt"), "content b").unwrap();

    // --overwrite applies the policy to all collisions without prompting.
    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("a.txt").to_str().unwrap())
        .arg("--replace")
        .arg("a")
        .arg("--with")
        .arg("b")
        .arg("--overwrite")
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipping").not());

    // a.txt was renamed over b.txt (overwritten)
    assert!(!temp.path().join("a.txt").exists());
    let content = fs::read_to_string(temp.path().join("b.txt")).unwrap();
    assert_eq!(content, "content a");
}

#[test]
fn test_collision_handling_skip_flag() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.txt"), "content a").unwrap();
    fs::write(temp.path().join("b.txt"), "content b").unwrap();

    // --skip applies the policy to all collisions without prompting.
    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("a.txt").to_str().unwrap())
        .arg("--replace")
        .arg("a")
        .arg("--with")
        .arg("b")
        .arg("--skip")
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipping"));

    // a.txt was skipped — both files still present, content unchanged.
    assert!(temp.path().join("a.txt").exists());
    assert!(temp.path().join("b.txt").exists());
    assert_eq!(
        fs::read_to_string(temp.path().join("b.txt")).unwrap(),
        "content b"
    );
}

#[test]
fn test_set_attributes_readonly() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("target.txt");
    fs::write(&file, "data").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(file.to_str().unwrap())
        .arg("--replace")
        .arg("target")
        .arg("--with")
        .arg("locked")
        .arg("--set-attributes")
        .arg("readonly")
        .assert()
        .success();

    let renamed = temp.path().join("locked.txt");
    assert!(renamed.exists(), "File should be renamed");
    let meta = renamed.metadata().unwrap();
    assert!(meta.permissions().readonly(), "File should be readonly");
}

#[test]
fn test_glob_no_match() {
    let temp = TempDir::new().unwrap();
    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.nonexistent").to_str().unwrap())
        .arg("--replace")
        .arg("foo")
        .arg("--with")
        .arg("bar")
        .assert()
        .success() // Should succeed but warn
        .stderr(predicate::str::contains("No files matched glob"));
}

#[test]
fn test_regex_backreference() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("track01.mp3"), "data").unwrap();

    // Capture group from --regex-match is referenced with $1 in --regex-replace
    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("track01.mp3").to_str().unwrap())
        .arg("--regex-match")
        .arg(r"track(\d+)")
        .arg("--regex-replace")
        .arg("Song_$1")
        .assert()
        .success();

    assert!(temp.path().join("Song_01.mp3").exists());
}

#[test]
fn test_case_name_title() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("my_cool_file.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("my_cool_file.txt").to_str().unwrap())
        .arg("--case-name")
        .arg("title")
        .assert()
        .success();

    // On case-insensitive FS (macOS APFS/HFS+, Windows NTFS) "My_Cool_File.txt"
    // and "my_cool_file.txt" resolve to the same file, so `exists()` can't
    // prove the casing changed. Compare the actual directory entry names.
    let mut new_case = false;
    let mut old_case = false;
    for entry in fs::read_dir(temp.path()).unwrap() {
        let name = entry.unwrap().file_name();
        if name == "My_Cool_File.txt" {
            new_case = true;
        }
        if name == "my_cool_file.txt" {
            old_case = true;
        }
    }
    assert!(new_case, "File should be renamed to My_Cool_File.txt");
    assert!(!old_case, "Old casing my_cool_file.txt should be gone");
}

#[test]
fn test_numbering_suffix_mode() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.txt"), "").unwrap();
    fs::write(temp.path().join("b.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--numbering-mode")
        .arg("suffix")
        .arg("--numbering-sep")
        .arg("-")
        .arg("--numbering-pad")
        .arg("3")
        .assert()
        .success();

    // Sorted: a.txt, b.txt -> a-001.txt, b-002.txt
    assert!(temp.path().join("a-001.txt").exists());
    assert!(temp.path().join("b-002.txt").exists());
}

#[test]
fn test_stop_on_error_rolls_back() {
    let temp = TempDir::new().unwrap();
    // Two files renamed by prefix. The second rename also drops its result into
    // a subfolder ("x_b_zzz") that a *file* already occupies, so creating that
    // folder is impossible and the rename fails at the syscall on every platform
    // (POSIX and Windows alike) — the error --stop-on-error must roll back.
    // (A folder *replacing an existing file* is refused and skipped, covered by
    // `test_overwrite_skips_folder_source_over_file`.)
    fs::write(temp.path().join("a_1.txt"), "one").unwrap();
    fs::write(temp.path().join("b_aaa_2.txt"), "two").unwrap();
    fs::write(temp.path().join("x_b_zzz"), "blocker").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("a_1.txt").to_str().unwrap())
        .arg(temp.path().join("b_aaa_2.txt").to_str().unwrap())
        .arg("--add-prefix")
        .arg("x_")
        .arg("--replace")
        .arg("aaa")
        .arg("--with")
        .arg("zzz/sub")
        .arg("--stop-on-error")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Stopping on error as requested, undoing...",
        ));

    // Rollback restored the first rename; the failed source is untouched, and
    // the file that blocked the second target is intact (not replaced).
    assert!(temp.path().join("a_1.txt").exists());
    assert!(!temp.path().join("x_a_1.txt").exists());
    assert!(temp.path().join("b_aaa_2.txt").exists());
    assert!(temp.path().join("x_b_zzz").is_file());
    assert_eq!(
        fs::read_to_string(temp.path().join("x_b_zzz")).unwrap(),
        "blocker"
    );
}

/// Folder handling applies whenever a folder is involved: a folder source is
/// never allowed to replace an existing file, so the collision is skipped and the
/// file survives — identically on every platform.
#[test]
fn test_overwrite_skips_folder_source_over_file() {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("aaa_2")).unwrap();
    fs::write(temp.path().join("aaa_2").join("inner.txt"), "data").unwrap();
    fs::write(temp.path().join("zzz_2"), "existing").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("aaa_2").to_str().unwrap())
        .arg("--replace")
        .arg("aaa")
        .arg("--with")
        .arg("zzz")
        .arg("--overwrite")
        .assert()
        .success();

    assert!(
        temp.path().join("aaa_2").is_dir(),
        "source folder untouched"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("zzz_2")).unwrap(),
        "existing",
        "file target must not be replaced"
    );
}
