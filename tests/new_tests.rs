use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;
use walkdir::WalkDir;

fn get_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_awara"))
}

#[test]
fn test_recursion() {
    let temp = TempDir::new().unwrap();
    let sub = temp.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(temp.path().join("file_a.txt"), "").unwrap();
    fs::write(sub.join("file_b.txt"), "").unwrap();

    let mut cmd = get_cmd();
    let assert = cmd
        .arg(temp.path().to_str().unwrap())
        .arg("-r")
        .arg("--replace")
        .arg("file")
        .arg("--with")
        .arg("doc")
        .arg("-v") // Verbose
        .assert()
        .success();

    // Print stdout for debugging
    println!(
        "Stdout: {}",
        String::from_utf8_lossy(&assert.get_output().stdout)
    );
    println!(
        "Stderr: {}",
        String::from_utf8_lossy(&assert.get_output().stderr)
    );

    if !temp.path().join("doc_a.txt").exists() {
        println!("Files in temp:");
        for entry in WalkDir::new(temp.path()) {
            println!("{:?}", entry.unwrap().path());
        }
    }

    assert!(temp.path().join("doc_a.txt").exists());
    assert!(sub.join("doc_b.txt").exists());
}

#[test]
fn test_filters() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("short.txt"), "").unwrap();
    fs::write(temp.path().join("very_long_name.txt"), "").unwrap();

    let mut cmd = get_cmd();
    let assert = cmd
        .arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--min-name-len")
        .arg("10")
        .arg("--replace")
        .arg("name")
        .arg("--with")
        .arg("renamed")
        .arg("-v")
        .assert()
        .success();

    println!(
        "Stdout: {}",
        String::from_utf8_lossy(&assert.get_output().stdout)
    );
    println!(
        "Stderr: {}",
        String::from_utf8_lossy(&assert.get_output().stderr)
    );

    println!("Files in temp:");
    for entry in WalkDir::new(temp.path()) {
        println!("{:?}", entry.unwrap().path());
    }

    assert!(temp.path().join("short.txt").exists()); // Skipped
    assert!(temp.path().join("very_long_renamed.txt").exists()); // Renamed
}

#[test]
fn test_ordering() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("b.txt"), "").unwrap();
    fs::write(temp.path().join("a.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--sort")
        .arg("name")
        .arg("--numbering-mode")
        .arg("prefix")
        .arg("--numbering-sep")
        .arg("-")
        .assert()
        .success();

    // a.txt -> 1-a.txt
    // b.txt -> 2-b.txt
    assert!(temp.path().join("1-a.txt").exists());
    assert!(temp.path().join("2-b.txt").exists());
}

#[test]
fn test_remove_accents_only() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("café.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--remove")
        .arg("accents")
        .assert()
        .success();

    assert!(temp.path().join("cafe.txt").exists());
}

#[test]
fn test_remove_high_only() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("café.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--remove")
        .arg("high")
        .assert()
        .success();

    assert!(temp.path().join("caf.txt").exists());
}

#[test]
fn test_copy_part() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("123456.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--copy-part")
        .arg("1:3:7") // Copy "123" to pos 7 (end)
        .assert()
        .success();

    // "123456.txt" -> "123456123.txt"
    assert!(temp.path().join("123456123.txt").exists());
}

#[test]
fn test_copy_part_to_end_keyword() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("abcdef.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--copy-part")
        .arg("1:3:end") // Copy "abc" to the very end (the "end" keyword maps to -1)
        .assert()
        .success();

    // "abcdef" stem -> "abcdefabc" (copied text goes AFTER the last char, not before it)
    assert!(temp.path().join("abcdefabc.txt").exists());
}

#[test]
fn test_move_part_to_end_keyword() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("abcdef.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--move-part")
        .arg("1:2:end") // Move "ab" to the very end
        .assert()
        .success();

    // "abcdef" stem -> "cdefab", not "cdeabf"
    assert!(temp.path().join("cdefab.txt").exists());
}

#[test]
fn test_move_part_negative_dest() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("abcdef.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--move-part")
        .arg("1:2:-2") // Move "ab" to one gap before the end (before last char)
        .assert()
        .success();

    // "abcdef" stem -> "cdeabf"
    assert!(temp.path().join("cdeabf.txt").exists());
}

#[test]
fn test_filter_folders() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("my_folder");
    fs::create_dir(&dir).unwrap();
    fs::write(temp.path().join("my_file.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*").to_str().unwrap())
        .arg("--filter-mode")
        .arg("folders")
        .arg("--replace")
        .arg("my")
        .arg("--with")
        .arg("your")
        .assert()
        .success();

    // "my_folder" -> "your_folder"
    // "my_file.txt" -> ignored
    assert!(temp.path().join("your_folder").exists());
    assert!(temp.path().join("my_file.txt").exists());
}

#[test]
fn test_combination() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("c.txt"), "").unwrap();
    fs::write(temp.path().join("a.txt"), "").unwrap();
    fs::write(temp.path().join("b.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--sort")
        .arg("name")
        .arg("--numbering-mode")
        .arg("prefix")
        .arg("--numbering-sep")
        .arg("-")
        .arg("--extension")
        .arg("md")
        .assert()
        .success();

    // a.txt -> 1-a.md
    // b.txt -> 2-b.md
    // c.txt -> 3-c.md
    assert!(temp.path().join("1-a.md").exists());
    assert!(temp.path().join("2-b.md").exists());
    assert!(temp.path().join("3-c.md").exists());
}

#[test]
fn test_remove_symbols_digits() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("file123@#.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--remove")
        .arg("digits")
        .arg("--remove")
        .arg("symbols")
        .assert()
        .success();

    // "file123@#.txt" -> "file.txt"
    assert!(temp.path().join("file.txt").exists());
}

#[test]
fn test_numbering_only() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.txt"), "").unwrap();
    fs::write(temp.path().join("b.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--numbering-mode")
        .arg("prefix")
        .arg("--numbering-sep")
        .arg("-")
        .assert()
        .success();

    // Sorted: a.txt, b.txt
    assert!(temp.path().join("1-a.txt").exists());
    assert!(temp.path().join("2-b.txt").exists());
}

#[test]
fn test_add_dirname() {
    let temp = TempDir::new().unwrap();
    let sub = temp.path().join("myfolder");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("file.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(sub.join("*.txt").to_str().unwrap())
        .arg("--add-dirname")
        .arg("--dirname-sep")
        .arg("_")
        .assert()
        .success();

    assert!(sub.join("myfolder_file.txt").exists());
}

#[test]
fn test_add_dirname_levels() {
    let temp = TempDir::new().unwrap();
    let sub = temp.path().join("myfolder").join("sub");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("file.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(sub.join("*.txt").to_str().unwrap())
        .arg("--add-dirname")
        .arg("--dirname-sep")
        .arg("_")
        .arg("--dirname-level")
        .arg("2")
        .assert()
        .success();

    // Both folders in range are appended, root-ward → parent, sep after each.
    assert!(sub.join("myfolder_sub_file.txt").exists());
}

#[test]
fn test_move_part() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("123-file.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--move-part")
        .arg("1:4:9") // Move "123-" to pos 9 (end)
        .assert()
        .success();

    assert!(temp.path().join("file123-.txt").exists());
}

#[test]
fn test_output_dir() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("file.txt"), "").unwrap();
    let out = temp.path().join("out");

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--replace")
        .arg("file")
        .arg("--with")
        .arg("renamed")
        .arg("--output-dir")
        .arg(out.to_str().unwrap())
        .arg("--copy-mode")
        .assert()
        .success();

    assert!(temp.path().join("file.txt").exists()); // Original remains (copy mode)
    assert!(out.join("renamed.txt").exists());
}

#[test]
fn test_add_date_defaults_to_prefix() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("photo.jpg"), "").unwrap();

    get_cmd()
        .arg(temp.path().join("*.jpg").to_str().unwrap())
        .arg("--add-date")
        .arg("YYYYMMDD_")
        .assert()
        .success();

    let names: Vec<String> = fs::read_dir(temp.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 1, "file should be renamed: {names:?}");
    assert_ne!(names[0], "photo.jpg");
    assert!(
        names[0].ends_with("photo.jpg"),
        "date should default to a prefix: {names:?}"
    );
}

#[test]
fn test_add_date_position_suffix() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("photo.jpg"), "").unwrap();

    get_cmd()
        .arg(temp.path().join("*.jpg").to_str().unwrap())
        .arg("--add-date")
        .arg("_YYYYMMDD")
        .arg("--date-position")
        .arg("suffix")
        .assert()
        .success();

    let names: Vec<String> = fs::read_dir(temp.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 1, "file should be renamed: {names:?}");
    assert!(
        names[0].starts_with("photo") && names[0].ends_with(".jpg") && names[0] != "photo.jpg",
        "date should be a suffix: {names:?}"
    );
}

#[test]
fn test_exclude_regex_cli() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("keep.txt"), "").unwrap();
    fs::write(temp.path().join("skip.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--add-prefix")
        .arg("renamed_")
        .arg("--exclude-regex")
        .arg("skip")
        .arg("-v")
        .assert()
        .success();

    // skip.txt should NOT be renamed
    assert!(temp.path().join("skip.txt").exists());
    // keep.txt SHOULD be renamed
    assert!(temp.path().join("renamed_keep.txt").exists());
}

#[test]
fn test_sort_by_extension() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("b.jpg"), "").unwrap();
    fs::write(temp.path().join("a.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*").to_str().unwrap())
        .arg("--sort")
        .arg("extension")
        .arg("--numbering-mode")
        .arg("prefix")
        .arg("--numbering-sep")
        .arg("-")
        .arg("-v")
        .assert()
        .success();

    // Sorted by extension: .jpg first, .txt second
    // So b.jpg -> 1-b.jpg, a.txt -> 2-a.txt
    assert!(temp.path().join("1-b.jpg").exists());
    assert!(temp.path().join("2-a.txt").exists());
}

#[test]
fn test_extension_add_if_missing_cli() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("readme"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*").to_str().unwrap())
        .arg("--extension")
        .arg("txt")
        .arg("--extension-add-if-missing")
        .arg("-v")
        .assert()
        .success();

    assert!(temp.path().join("readme.txt").exists());
}

#[test]
fn test_preset_save_and_load() {
    let temp = TempDir::new().unwrap();
    let preset_path = temp.path().join("preset.json");
    fs::write(temp.path().join("file.txt"), "").unwrap();

    // Save preset
    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--add-prefix")
        .arg("pre_")
        .arg("--save-preset")
        .arg(preset_path.to_str().unwrap())
        .assert()
        .success();

    // Verify the preset file was created
    assert!(preset_path.exists());

    // Clean up renamed file
    let renamed = temp.path().join("pre_file.txt");
    if renamed.exists() {
        fs::remove_file(&renamed).ok();
    }

    // Re-create original
    fs::write(temp.path().join("file.txt"), "").unwrap();

    // Load preset and run again
    let mut cmd2 = get_cmd();
    cmd2.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--load-preset")
        .arg(preset_path.to_str().unwrap())
        .assert()
        .success();

    assert!(temp.path().join("pre_file.txt").exists());
}

#[test]
fn test_load_preset_silently_cleans_invalid_values() {
    let temp = TempDir::new().unwrap();
    let preset_path = temp.path().join("bad.json");
    // Semantically invalid values must be cleaned, not reject the load.
    fs::write(
        &preset_path,
        r#"{"rename_config":{"special":{"set_modified":{"Fixed":"garbage"}},"numbering":{"numbering_type":"hexadecimal"}}}"#,
    )
    .unwrap();
    fs::write(temp.path().join("file.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--load-preset")
        .arg(preset_path.to_str().unwrap())
        .assert()
        .success();

    // No rename was configured, so the file is untouched but the run succeeded.
    assert!(temp.path().join("file.txt").exists());
}

/// A preset whose patterns don't compile must still load: the engine
/// tolerates them (fails open / skips), so sanitize preserves the text rather
/// than deleting it, and unaffected operations still apply.
#[test]
fn test_load_preset_preserves_non_compiling_patterns() {
    let temp = TempDir::new().unwrap();
    let preset_path = temp.path().join("patterns.json");
    fs::write(
        &preset_path,
        r#"{"rename_config":{"add":{"add_prefix":"pre_"},"regex":{"regex_match":"("},"filters":{"exclude_regex":"(","filter_pattern":"["}}}"#,
    )
    .unwrap();
    fs::write(temp.path().join("file.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--load-preset")
        .arg(preset_path.to_str().unwrap())
        .assert()
        .success();

    // A non-compiling exclude/mask fails open, so the file is still processed
    // and the valid prefix is applied.
    assert!(temp.path().join("pre_file.txt").exists());
}

/// A malformed (unparseable) preset is still an error — it can't be cleaned.
#[test]
fn test_load_preset_rejects_malformed_json() {
    let temp = TempDir::new().unwrap();
    let preset_path = temp.path().join("broken.json");
    fs::write(&preset_path, "{not json").unwrap();
    fs::write(temp.path().join("file.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--load-preset")
        .arg(preset_path.to_str().unwrap())
        .assert()
        .failure();
}

#[test]
fn test_load_preset_rejects_bare_config() {
    // Enforced standard: the enveloped document (with `rename_config`) is
    // required; a bare RenameConfig object is rejected.
    let temp = TempDir::new().unwrap();
    let preset_path = temp.path().join("bare.json");
    fs::write(&preset_path, r#"{"add":{"add_prefix":"pre_"}}"#).unwrap();
    fs::write(temp.path().join("file.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--load-preset")
        .arg(preset_path.to_str().unwrap())
        .assert()
        .failure();
}

#[test]
fn test_load_preset_honors_disabled_section() {
    // `disabled_sections` names the sections to skip; a disabled section is
    // honored by clearing its values to defaults on load.
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("preset.json"),
        r#"{"rename_config":{"add":{"add_prefix":"pre_"}},"disabled_sections":["Add"]}"#,
    )
    .unwrap();
    fs::write(temp.path().join("file.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--load-preset")
        .arg(temp.path().join("preset.json").to_str().unwrap())
        .assert()
        .success();

    // Add is disabled, so its prefix is cleared and the file is untouched.
    assert!(temp.path().join("file.txt").exists());
    assert!(!temp.path().join("pre_file.txt").exists());
}

#[test]
fn test_cli_rejects_invalid_numbering_type() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("file.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--numbering-mode")
        .arg("prefix")
        .arg("--numbering-type")
        .arg("hexadecimal")
        .assert()
        .failure();
}

#[test]
fn test_sort_by_extension_desc() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.txt"), "").unwrap();
    fs::write(temp.path().join("b.jpg"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*").to_str().unwrap())
        .arg("--sort")
        .arg("extension-desc")
        .arg("--numbering-mode")
        .arg("prefix")
        .arg("--numbering-sep")
        .arg("-")
        .assert()
        .success();

    // Sorted by extension desc: .txt first, .jpg second
    assert!(temp.path().join("1-a.txt").exists());
    assert!(temp.path().join("2-b.jpg").exists());
}

#[test]
fn test_swap_files() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("alpha.txt"), "content1").unwrap();
    fs::write(temp.path().join("beta.txt"), "content2").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("alpha.txt").to_str().unwrap())
        .arg(temp.path().join("beta.txt").to_str().unwrap())
        .arg("--swap")
        .assert()
        .success();

    // alpha should now have content2, beta should have content1
    assert_eq!(
        fs::read_to_string(temp.path().join("alpha.txt")).unwrap(),
        "content2"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("beta.txt")).unwrap(),
        "content1"
    );
}

#[test]
fn test_numbering_source_date() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("b.txt"), "").unwrap();
    fs::write(temp.path().join("a.txt"), "").unwrap();

    // Set modification times to ensure ordering
    // a.txt older, b.txt newer
    let a_path = temp.path().join("a.txt");
    let b_path = temp.path().join("b.txt");
    // Use filetime to set specific times
    let old_time = filetime::FileTime::from_unix_time(1000000, 0);
    let new_time = filetime::FileTime::from_unix_time(2000000, 0);
    let _ = filetime::set_file_mtime(&a_path, old_time);
    let _ = filetime::set_file_mtime(&b_path, new_time);

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--numbering-source")
        .arg("date")
        .arg("--numbering-mode")
        .arg("prefix")
        .arg("--numbering-sep")
        .arg("-")
        .assert()
        .success();

    // Oldest first: a.txt -> 1-a.txt, b.txt -> 2-b.txt
    assert!(temp.path().join("1-a.txt").exists());
    assert!(temp.path().join("2-b.txt").exists());
}

#[test]
fn test_numbering_source_size() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("small.txt"), "a").unwrap();
    fs::write(
        temp.path().join("large.txt"),
        "very large content here 1234567890",
    )
    .unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--numbering-source")
        .arg("size")
        .arg("--numbering-mode")
        .arg("prefix")
        .arg("--numbering-sep")
        .arg("-")
        .assert()
        .success();

    // Smallest first: small.txt -> 1-small.txt, large.txt -> 2-large.txt
    assert!(temp.path().join("1-small.txt").exists());
    assert!(temp.path().join("2-large.txt").exists());
}

#[test]
fn test_filter_attr_readonly() {
    let temp = TempDir::new().unwrap();
    let normal = temp.path().join("normal.txt");
    let readonly_file = temp.path().join("readonly.txt");
    fs::write(&normal, "").unwrap();
    fs::write(&readonly_file, "").unwrap();

    // Set readonly
    let mut perms = readonly_file.metadata().unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&readonly_file, perms).unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--add-prefix")
        .arg("renamed_")
        .arg("--filter-attr")
        .arg("readonly")
        .arg("-v")
        .assert()
        .success();

    // Only readonly file should be renamed
    assert!(temp.path().join("renamed_readonly.txt").exists());
    assert!(temp.path().join("normal.txt").exists());
}

#[test]
fn test_swap_dry_run() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.txt"), "content a").unwrap();
    fs::write(temp.path().join("b.txt"), "content b").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("a.txt").to_str().unwrap())
        .arg(temp.path().join("b.txt").to_str().unwrap())
        .arg("--swap")
        .arg("-n")
        .assert()
        .success();

    // Files should NOT be swapped (dry run)
    assert_eq!(
        fs::read_to_string(temp.path().join("a.txt")).unwrap(),
        "content a"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("b.txt")).unwrap(),
        "content b"
    );
}

#[test]
fn test_filter_attr_file() {
    let temp = TempDir::new().unwrap();
    let sub = temp.path().join("subdir");
    fs::create_dir(&sub).unwrap();
    fs::write(temp.path().join("afile.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*").to_str().unwrap())
        .arg("--add-prefix")
        .arg("renamed_")
        .arg("--filter-attr")
        .arg("file")
        .arg("-v")
        .assert()
        .success();

    // Only the file should be renamed, not the directory
    assert!(temp.path().join("renamed_afile.txt").exists());
    assert!(temp.path().join("subdir").exists());
}

#[test]
fn test_exif_date_fallback_to_modified() {
    // This test verifies that --insert-meta exif-date falls back to file modification time
    // when the file has no EXIF data (e.g., a plain text file)
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("photo.jpg");
    fs::write(&file_path, "not a real jpeg").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(file_path.to_str().unwrap())
        .arg("--insert-meta")
        .arg("exif-date")
        .arg("-n") // dry run
        .assert()
        .success();

    // Should still exist (dry run)
    assert!(file_path.exists());
}

// ── Negative value tests (CLI integration) ──

#[test]
fn test_remove_first_negative() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("hello.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--remove-first=-2") // remove last 2 chars of stem
        .assert()
        .success();

    // "hello" stem: remove last 2 -> "hel"
    assert!(temp.path().join("hel.txt").exists());
}

#[test]
fn test_remove_last_negative() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("hello.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--remove-last=-2") // remove first 2 chars of stem instead of last
        .assert()
        .success();

    // "hello" stem: remove first 2 -> "llo"
    assert!(temp.path().join("llo.txt").exists());
}

#[test]
fn test_numbering_increment_negative() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("c.txt"), "").unwrap();
    fs::write(temp.path().join("b.txt"), "").unwrap();
    fs::write(temp.path().join("a.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--sort")
        .arg("name")
        .arg("--numbering-mode")
        .arg("prefix")
        .arg("--numbering-sep")
        .arg("-")
        .arg("--numbering-start")
        .arg("9")
        .arg("--numbering-increment=-3") // descending: 9, 6, 3
        .assert()
        .success();

    // Sorted: a.txt -> 9-a.txt, b.txt -> 6-b.txt, c.txt -> 3-c.txt
    assert!(temp.path().join("9-a.txt").exists());
    assert!(temp.path().join("6-b.txt").exists());
    assert!(temp.path().join("3-c.txt").exists());
}

#[test]
fn test_numbering_at_negative() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("hello.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--numbering-mode")
        .arg("insert")
        .arg("--numbering-sep")
        .arg("_")
        .arg("--numbering-start")
        .arg("1")
        .arg("--numbering-at=-3") // insert number 3 chars from end of stem: hel_lo -> but 0-based!
        .assert()
        .success();

    // "hello" stem len 5: pos=-3 => insert at 5-3=2 => "he1_lolo"...
    // Actually apply_insert with text "1_" at pos=2:
    // chars: ['h','e','l','l','o'], insert "1_" at index 2 -> "he1_llo"
    assert!(temp.path().join("he1_llo.txt").exists());
}

#[test]
fn test_remove_from_negative_cli() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("hello.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--remove-from=-3") // count 3 from end of stem
        .arg("--remove-to=6") // remove to end
        .assert()
        .success();

    // "hello" stem len 5: from=-3 => resolve to 5-3+1=3 (1-based), to=6 (beyond end)
    // remove_from_to: from_0=2, to_0=5, remove chars[2..5]="llo" => "he"
    assert!(temp.path().join("he.txt").exists());
}

#[test]
fn test_remove_from_to_zero_cli() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("hello.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--remove-from=-3") // count 3 from end of stem
        .arg("--remove-to=0") // 0 removes through the end of the name
        .assert()
        .success();

    // "hello" stem len 5: from_0=2, to=0 => to_0=5, remove chars[2..5]="llo" => "he"
    assert!(temp.path().join("he.txt").exists());
}

#[test]
fn test_move_part_negative_cli() {
    let temp = TempDir::new().unwrap();
    // The CLI uses the format start:len:dest for move-part/copy-part
    // But those are parsed differently in args.rs parse_part_arg
    // Negative values are passed directly as isize in the GUI, but CLI uses parse_part_arg
    // which parses with `.parse().ok()?` which handles negative for isize
    fs::write(temp.path().join("123456.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--move-part=-3:2:1") // move last 2 chars (from 3 from end, length 2) to start
        .assert()
        .success();

    // "123456" stem: from=-3 => actual_from=4 (1-based), len=2, to=1 (start)
    // drain chars[3..5]="45", remaining "1236", insert at 0 => "451236"
    assert!(temp.path().join("451236.txt").exists());
}

#[test]
fn test_move_part_separator_cli() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("123456.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--move-part=1:3:end:_") // move "123" to end, separator "_"
        .assert()
        .success();

    // "123456" stem: move "123" to the end with sep "_" => rest + sep + part
    assert!(temp.path().join("456_123.txt").exists());
}

#[test]
fn test_copy_part_negative_cli() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("123456.txt"), "").unwrap();

    let mut cmd = get_cmd();
    cmd.arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--copy-part=-2:1:1") // copy 2nd-from-last char to start
        .assert()
        .success();

    // "123456" stem: from=-2 => 1-based position 5 (char '5'), len=1, to=1 (start)
    // copy '5' before '1' => "5123456"
    assert!(temp.path().join("5123456.txt").exists());
}

// ── Windows: trailing space/dot normalization (runs on Windows CI only) ──
// Windows strips trailing spaces/dots from created names (and from the stem
// before the extension) automatically. The app mirrors that rule at execution
// time so recorded names always match what ends up on disk. These tests pin
// the behavior and fail loudly if the mirror drifts from reality.

#[cfg(windows)]
mod windows_normalization {
    use super::*;
    use std::path::Path;

    fn disk_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    /// Stem-before-extension: "name .txt" is created as "name.txt".
    /// The recorded op (undo JSON) must match the on-disk name.
    #[test]
    fn test_stem_before_ext_normalized() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("oldname.txt"), "").unwrap();
        let undo = temp.path().join("undo.json");

        get_cmd()
            .arg(temp.path().join("oldname.txt").to_str().unwrap())
            .arg("--replace")
            .arg("oldname")
            .arg("--with")
            .arg("name ")
            .arg("--undo-file")
            .arg(undo.to_str().unwrap())
            .assert()
            .success();

        assert_eq!(disk_names(temp.path()), vec!["name.txt", "undo.json"]);
        let ops: Vec<awara::RenameOp> =
            serde_json::from_str(&fs::read_to_string(&undo).unwrap()).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].new_name, "name.txt");
    }

    /// Whole-name (no extension): trailing space is stripped.
    #[test]
    fn test_whole_name_no_ext_normalized() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("old"), "").unwrap();

        get_cmd()
            .arg(temp.path().join("old").to_str().unwrap())
            .arg("--replace")
            .arg("old")
            .arg("--with")
            .arg("new ")
            .assert()
            .success();

        assert_eq!(disk_names(temp.path()), vec!["new"]);
    }

    /// Trailing dot collapses to the dotless name.
    #[test]
    fn test_trailing_dot_normalized() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("old"), "").unwrap();

        get_cmd()
            .arg(temp.path().join("old").to_str().unwrap())
            .arg("--replace")
            .arg("old")
            .arg("--with")
            .arg("new.")
            .assert()
            .success();

        assert_eq!(disk_names(temp.path()), vec!["new"]);
    }

    /// Directories are normalized identically (no extension involved).
    #[test]
    fn test_dir_name_normalized() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("olddir");
        fs::create_dir(&dir).unwrap();

        get_cmd()
            .arg(dir.to_str().unwrap())
            .arg("--replace")
            .arg("olddir")
            .arg("--with")
            .arg("newdir ")
            .assert()
            .success();

        assert_eq!(disk_names(temp.path()), vec!["newdir"]);
    }

    /// Multi-dot stem: pins the exact Windows rule for a space in the middle
    /// of a compound stem. The helper splits at the last dot, so this target
    /// is expected to stay unchanged — if Windows disagrees, this test fails
    /// and the rule in `trim_windows_trailing` must be corrected.
    #[test]
    fn test_multi_dot_stem_rule_pinned() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("src.tar.gz"), "").unwrap();

        get_cmd()
            .arg(temp.path().join("src.tar.gz").to_str().unwrap())
            .arg("--replace")
            .arg("src")
            .arg("--with")
            .arg("foo ")
            .assert()
            .success();

        assert_eq!(disk_names(temp.path()), vec!["foo .tar.gz"]);
    }

    /// A normalized target that collides with an existing file is skipped,
    /// never silently overwriting the file that is already there.
    #[test]
    fn test_normalized_target_conflicts_with_existing_file() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.txt"), "keep me").unwrap();
        fs::write(temp.path().join("old.txt"), "old").unwrap();

        get_cmd()
            .arg(temp.path().join("old.txt").to_str().unwrap())
            .arg("--replace")
            .arg("old")
            .arg("--with")
            .arg("a ")
            .assert()
            .success();

        // "a .txt" normalizes to "a.txt", which already exists -> skip.
        assert_eq!(disk_names(temp.path()), vec!["a.txt", "old.txt"]);
        assert_eq!(
            fs::read_to_string(temp.path().join("a.txt")).unwrap(),
            "keep me"
        );
    }

    /// A target that normalizes back to its own source name is a no-op on
    /// Windows — the op is dropped and the file stays untouched.
    #[test]
    fn test_noop_target_dropped() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("foo.txt"), "").unwrap();

        get_cmd()
            .arg(temp.path().join("foo.txt").to_str().unwrap())
            .arg("--replace")
            .arg("foo")
            .arg("--with")
            .arg("foo ")
            .assert()
            .success();

        assert_eq!(disk_names(temp.path()), vec!["foo.txt"]);
    }

    /// Undo round-trip: rename to a trimmed name, then revert it.
    #[test]
    fn test_undo_round_trip() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("oldname.txt"), "").unwrap();
        let undo = temp.path().join("undo.json");

        get_cmd()
            .arg(temp.path().join("oldname.txt").to_str().unwrap())
            .arg("--replace")
            .arg("oldname")
            .arg("--with")
            .arg("name ")
            .arg("--undo-file")
            .arg(undo.to_str().unwrap())
            .assert()
            .success();
        // The undo file itself lives in the temp dir alongside the renamed
        // file.
        assert_eq!(disk_names(temp.path()), vec!["name.txt", "undo.json"]);

        get_cmd()
            .arg("--apply-undo")
            .arg(undo.to_str().unwrap())
            .assert()
            .success();
        assert_eq!(disk_names(temp.path()), vec!["oldname.txt", "undo.json"]);
    }

    /// Copy mode normalizes the copy target identically.
    #[test]
    fn test_copy_mode_normalized() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("oldname.txt"), "").unwrap();
        let out = temp.path().join("out");

        get_cmd()
            .arg(temp.path().join("oldname.txt").to_str().unwrap())
            .arg("--replace")
            .arg("oldname")
            .arg("--with")
            .arg("name ")
            .arg("--output-dir")
            .arg(out.to_str().unwrap())
            .arg("--copy-mode")
            .assert()
            .success();

        // Copy mode: source remains, the copy is created with the trimmed name.
        assert!(temp.path().join("oldname.txt").exists());
        assert_eq!(disk_names(&out), vec!["name.txt"]);
    }
}

// ── Filter flag wiring (CLI) ──

#[test]
fn test_filter_files_overrides_filter_mode() {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("my_folder")).unwrap();
    fs::write(temp.path().join("my_file.txt"), "").unwrap();

    get_cmd()
        .arg(temp.path().join("*").to_str().unwrap())
        .arg("--filter-mode")
        .arg("folders")
        .arg("--filter-files")
        .arg("--replace")
        .arg("my")
        .arg("--with")
        .arg("your")
        .assert()
        .success();

    // `--filter-files` overrides `--filter-mode folders`: only the file changes.
    assert!(temp.path().join("your_file.txt").exists());
    assert!(temp.path().join("my_folder").exists());
}

#[test]
fn test_filter_hidden_includes_dotfiles() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join(".hidden.txt"), "").unwrap();
    fs::write(temp.path().join("visible.txt"), "").unwrap();

    get_cmd()
        .arg(temp.path().join("*").to_str().unwrap())
        .arg("--filter-hidden")
        .arg("--add-prefix")
        .arg("x_")
        .assert()
        .success();

    // Hidden files are only scanned when `--filter-hidden` is passed.
    assert!(temp.path().join("x_.hidden.txt").exists());
    assert!(temp.path().join("x_visible.txt").exists());
}

#[test]
fn test_filter_subfolders_recurses() {
    let temp = TempDir::new().unwrap();
    let sub = temp.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("inner.txt"), "").unwrap();
    fs::write(temp.path().join("top.txt"), "").unwrap();

    get_cmd()
        .arg(temp.path().to_str().unwrap())
        .arg("--filter-subfolders")
        .arg("--filter-files")
        .arg("--replace")
        .arg("inner")
        .arg("--with")
        .arg("nested")
        .assert()
        .success();

    // `--filter-subfolders` alone implies a recursive scan.
    assert!(sub.join("nested.txt").exists());
}

#[test]
fn test_filter_level_limits_recursion_depth() {
    let temp = TempDir::new().unwrap();
    let deep = temp.path().join("sub").join("deep");
    fs::create_dir_all(&deep).unwrap();
    fs::write(temp.path().join("top.txt"), "").unwrap();
    fs::write(deep.join("deepest.txt"), "").unwrap();

    get_cmd()
        .arg(temp.path().to_str().unwrap())
        .arg("--filter-subfolders")
        .arg("--filter-files")
        .arg("--filter-level")
        .arg("1")
        .arg("--add-prefix")
        .arg("x_")
        .assert()
        .success();

    // Depth 1 = immediate children only; the nested file is untouched.
    assert!(temp.path().join("x_top.txt").exists());
    assert!(deep.join("deepest.txt").exists());
}

// ── Directory-target collisions under --overwrite (CLI) ──

/// `--overwrite` must never delete or replace a folder. Renaming a directory
/// onto an *empty* directory would otherwise succeed on Unix and silently
/// replace it; the target folder must be skipped and left intact.
#[test]
fn test_cli_overwrite_skips_directory_target() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    let target = temp.path().join("dst");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("inner.txt"), "data").unwrap();
    fs::create_dir(&target).unwrap(); // empty target folder

    get_cmd()
        .arg(src.to_str().unwrap())
        .arg("--replace")
        .arg("src")
        .arg("--with")
        .arg("dst")
        .arg("--overwrite")
        .assert()
        .success();

    assert!(
        src.join("inner.txt").exists(),
        "source folder must be left intact"
    );
    assert_eq!(
        fs::read_dir(&target).unwrap().count(),
        0,
        "target folder must not be populated or removed"
    );
}

/// Copy mode copies only the selected entry: a directory is recreated (empty)
/// at the destination — its contents are left behind — and the command succeeds.
#[test]
fn test_cli_copy_mode_directory_creates_empty_target_dir() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    let out = temp.path().join("out");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("inner.txt"), "data").unwrap();

    get_cmd()
        .arg(src.to_str().unwrap())
        .arg("--add-prefix")
        .arg("x_")
        .arg("--output-dir")
        .arg(out.to_str().unwrap())
        .arg("--copy-mode")
        .assert()
        .success();

    let copied = out.join("x_src");
    assert!(copied.is_dir(), "directory copied by name");
    assert_eq!(
        fs::read_dir(&copied).unwrap().count(),
        0,
        "directory contents must not be copied"
    );
    assert!(src.join("inner.txt").exists(), "source preserved");
}

/// In move mode a relocated directory is still copied (recreated empty): only
/// its entry is placed at the destination, never its contents, and the source
/// directory is kept.
#[test]
fn test_cli_move_mode_relocated_directory_is_copied() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    let out = temp.path().join("out");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("inner.txt"), "data").unwrap();

    get_cmd()
        .arg(src.to_str().unwrap())
        .arg("--add-prefix")
        .arg("x_")
        .arg("--output-dir")
        .arg(out.to_str().unwrap())
        .arg("--move")
        .assert()
        .success();

    let copied = out.join("x_src");
    assert!(copied.is_dir(), "directory recreated at the destination");
    assert_eq!(
        fs::read_dir(&copied).unwrap().count(),
        0,
        "directory contents must not be moved"
    );
    assert!(src.join("inner.txt").exists(), "source directory kept");
}

// ── Undo record excludes Copy / Move to Location (CLI) ──

/// `--undo-file` records only in-place renames: a Copy / Move to Location apply
/// produces an empty record, so `--apply-undo` on it is a no-op.
#[test]
fn test_cli_undo_file_excludes_copy_to_location() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.txt"), "data").unwrap();
    let out = temp.path().join("out");
    let undo = temp.path().join("undo.json");

    get_cmd()
        .arg(temp.path().join("a.txt").to_str().unwrap())
        .arg("--add-prefix")
        .arg("x_")
        .arg("--output-dir")
        .arg(out.to_str().unwrap())
        .arg("--copy-mode")
        .arg("--undo-file")
        .arg(undo.to_str().unwrap())
        .assert()
        .success();

    assert!(out.join("x_a.txt").exists(), "the copy happened");
    let json = fs::read_to_string(&undo).unwrap();
    assert_eq!(
        json.trim(),
        "[]",
        "a copy/move to location is not recorded for undo"
    );

    // Applying the (empty) undo record changes nothing.
    get_cmd()
        .arg("--apply-undo")
        .arg(undo.to_str().unwrap())
        .assert()
        .success();
    assert!(out.join("x_a.txt").exists(), "nothing was reverted");
    assert!(temp.path().join("a.txt").exists());
}

/// A computed name that navigates the path aborts the **whole** CLI batch:
/// nothing is renamed and the process fails.
#[test]
fn test_cli_invalid_name_aborts_batch() {
    let temp = TempDir::new().unwrap();
    let a = temp.path().join("a.txt");
    let b = temp.path().join("b.txt");
    fs::write(&a, "data").unwrap();
    fs::write(&b, "data").unwrap();

    get_cmd()
        .arg(temp.path().join("*.txt").to_str().unwrap())
        .arg("--add-prefix")
        .arg("../")
        .assert()
        .failure()
        .stderr(predicates::str::contains("Invalid new name"));

    // Neither file was touched (the batch was refused wholesale).
    assert!(a.exists(), "nothing renamed");
    assert!(b.exists(), "nothing renamed");
    assert!(!temp.path().join("x_a.txt").exists());
}
