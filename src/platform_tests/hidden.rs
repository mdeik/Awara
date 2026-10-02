use super::*;

#[test]
#[cfg(unix)]
fn test_is_hidden_unix_dot_prefix() {
    let dir = TempDir::new().unwrap();
    let hidden = dir.path().join(".hidden_file");
    let visible = dir.path().join("visible_file");
    fs::write(&hidden, b"").unwrap();
    fs::write(&visible, b"").unwrap();

    assert!(is_hidden(&hidden));
    assert!(!is_hidden(&visible));
}

#[test]
#[cfg(unix)]
fn test_is_hidden_unix_hidden_directory() {
    let dir = TempDir::new().unwrap();
    let hidden = dir.path().join(".hidden_dir");
    fs::create_dir_all(&hidden).unwrap();

    assert!(is_hidden(&hidden));
}

#[test]
#[cfg(windows)]
fn test_is_hidden_windows_attr() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("test_file.txt");
    fs::write(&file, b"").unwrap();

    assert!(!is_hidden(&file));

    set_file_attributes(&file, "hidden");
    assert!(
        is_hidden(&file),
        "file should be hidden after setting the hidden attribute"
    );

    set_file_attributes(&file, "-hidden");
    assert!(
        !is_hidden(&file),
        "file should not be hidden after clearing the hidden attribute"
    );
}

#[test]
fn test_set_file_attributes_readonly() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("readonly_test.txt");
    fs::write(&file, b"data").unwrap();

    let result = set_file_attributes(&file, "readonly");
    assert!(result.metadata().unwrap().permissions().readonly());

    let result = set_file_attributes(&result, "-readonly");
    assert!(!result.metadata().unwrap().permissions().readonly());
}

#[test]
fn test_set_file_attributes_hidden() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("hidden_test.txt");
    fs::write(&file, b"data").unwrap();

    let result = set_file_attributes(&file, "hidden");
    assert!(result.exists(), "file should exist after hiding");
    assert!(is_hidden(&result), "file should be hidden after setting");
    // The dot convention is the native mechanism only on non-macOS Unix.
    #[cfg(all(unix, not(target_os = "macos")))]
    assert!(
        result
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with('.'),
        "hidden file should start with '.' on Unix"
    );

    let result = set_file_attributes(&result, "-hidden");
    assert!(result.exists(), "file should exist after unhiding");
    assert!(
        !is_hidden(&result),
        "file should not be hidden after clearing"
    );
    #[cfg(all(unix, not(target_os = "macos")))]
    assert!(
        !result
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with('.'),
        "visible file should NOT start with '.' on Unix"
    );
}

#[test]
fn test_set_file_attributes_invalid_attr_ignored() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("ignore_test.txt");
    fs::write(&file, b"data").unwrap();

    let result = set_file_attributes(&file, "nonexistent_attr");
    assert_eq!(result, file);
    assert!(result.exists());
}

#[test]
#[cfg(windows)]
fn test_set_file_attributes_system_windows() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("system_test.txt");
    fs::write(&file, b"data").unwrap();

    let result = set_file_attributes(&file, "system");
    let meta = std::fs::metadata(&result).unwrap();
    use std::os::windows::fs::MetadataExt;
    assert!(
        meta.file_attributes() & 0x4 != 0,
        "system attribute bit should be set"
    );
}

#[test]
#[cfg(windows)]
fn test_set_file_attributes_archive_windows() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("archive_test.txt");
    fs::write(&file, b"data").unwrap();

    let result = set_file_attributes(&file, "archive");
    let meta = std::fs::metadata(&result).unwrap();
    use std::os::windows::fs::MetadataExt;
    assert!(
        meta.file_attributes() & 0x20 != 0,
        "archive attribute bit should be set"
    );
}

#[test]
fn test_set_file_attributes_plus_minus_syntax() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("syntax_test.txt");
    fs::write(&file, b"data").unwrap();

    let result = set_file_attributes(&file, "+readonly");
    assert!(result.metadata().unwrap().permissions().readonly());

    let result = set_file_attributes(&result, "-readonly");
    assert!(!result.metadata().unwrap().permissions().readonly());
}

#[test]
fn test_rename_case_insensitive_same_name() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("OriginalCase.TXT");
    fs::write(&src, b"data").unwrap();
    let dst = dir.path().join("originalcase.txt");

    let result = rename_case_insensitive(&src, &dst, case_sensitivity_for(dir.path()));
    assert!(
        result.is_ok(),
        "case-only rename should succeed: {:?}",
        result
    );
    assert!(dst.exists());
    assert_eq!(
        std::fs::read_to_string(&dst).unwrap(),
        "data",
        "renamed file should have expected content"
    );
}

#[test]
fn test_rename_case_insensitive_different_name() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("original.txt");
    fs::write(&src, b"data").unwrap();
    let dst = dir.path().join("renamed.txt");

    let result = rename_case_insensitive(&src, &dst, case_sensitivity_for(dir.path()));
    assert!(
        result.is_ok(),
        "rename to different name should succeed: {:?}",
        result
    );
    assert!(!src.exists());
    assert!(dst.exists());
}
