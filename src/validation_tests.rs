use super::*;
use std::fs;
use std::sync::Mutex;
use tempfile::TempDir;

/// Serializes tests that call `set_current_dir` (process-wide side effect).
static CWD_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_validate_empty_string() {
    assert_eq!(validate_input_path(""), InputPathValidation::Empty);
}

#[test]
fn test_validate_whitespace_only() {
    assert_eq!(validate_input_path("   "), InputPathValidation::Empty);
    assert_eq!(validate_input_path("\t"), InputPathValidation::Empty);
    assert_eq!(validate_input_path("\n"), InputPathValidation::Empty);
}

#[test]
fn test_validate_path_too_long() {
    let long = "/a".repeat(2500);
    match validate_input_path(&long) {
        InputPathValidation::PathTooLong { length, limit } => {
            assert!(length > 4900);
            assert!(limit > 0);
        }
        other => panic!("Expected PathTooLong, got {:?}", other),
    }
}

#[test]
fn test_validate_normal_length_path() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_string_lossy().to_string();
    match validate_input_path(&path) {
        InputPathValidation::Ok(p) => {
            assert!(p.is_absolute());
        }
        other => panic!("Expected Ok, got {:?}", other),
    }
}

#[test]
fn test_validate_component_too_long() {
    let long_component = "x".repeat(300);
    match validate_input_path(&long_component) {
        InputPathValidation::ComponentTooLong {
            component,
            length,
            limit,
        } => {
            assert_eq!(limit, 255);
            assert!(length > 255);
            assert!(component.len() <= 43);
        }
        other => panic!("Expected ComponentTooLong, got {:?}", other),
    }
}

#[test]
fn test_validate_mixed_components_length() {
    let long = format!("/tmp/{}/ok", "x".repeat(300));
    match validate_input_path(&long) {
        InputPathValidation::ComponentTooLong { .. } => {}
        other => panic!("Expected ComponentTooLong, got {:?}", other),
    }
}

#[test]
fn test_validate_ok_component_length() {
    let dir = TempDir::new().unwrap();
    let base_len = dir.path().to_string_lossy().len();
    let comp_len = (platform_max_path().saturating_sub(base_len + 10)).clamp(50, 200);
    let sub = dir.path().join("a".repeat(comp_len));
    fs::create_dir_all(&sub).unwrap();
    match validate_input_path(&sub.to_string_lossy()) {
        InputPathValidation::Ok(_) => {}
        other => panic!("Expected Ok, got {:?}", other),
    }
}

#[test]
fn test_validate_reserved_con() {
    assert_eq!(
        validate_input_path("con.txt"),
        InputPathValidation::ReservedName
    );
}

#[test]
fn test_validate_reserved_nul() {
    assert_eq!(
        validate_input_path("NUL"),
        InputPathValidation::ReservedName
    );
}

#[test]
fn test_validate_reserved_lpt1() {
    assert_eq!(
        validate_input_path("LPT1"),
        InputPathValidation::ReservedName
    );
}

#[test]
fn test_validate_reserved_in_subdir() {
    match validate_input_path("/home/user/con") {
        InputPathValidation::ReservedName => {}
        other => panic!("Expected ReservedName for 'con' filename, got {:?}", other),
    }
}

#[test]
fn test_validate_not_reserved() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("console");
    fs::write(&path, "").unwrap();
    match validate_input_path(&path.to_string_lossy()) {
        InputPathValidation::Ok(_) => {}
        other => panic!("Expected Ok for 'console', got {:?}", other),
    }
}

#[test]
fn test_validate_null_byte() {
    match validate_input_path("bad\0file.txt") {
        InputPathValidation::InvalidChars(msg) => {
            assert!(msg.contains("null byte"));
        }
        other => panic!("Expected InvalidChars(null), got {:?}", other),
    }
}

#[test]
fn test_validate_windows_forbidden_in_filename() {
    for &ch in &['<', '>', ':', '"', '|', '?', '*'] {
        let name = format!("foo{}bar.txt", ch);
        let result = validate_input_path(&name);
        assert!(
            matches!(result, InputPathValidation::InvalidChars(_)),
            "Expected InvalidChars for '{}', got {:?}",
            ch.escape_default(),
            result
        );
    }
}

#[test]
fn test_validate_forbidden_chars_that_are_separators() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("sub");
    fs::create_dir_all(&file).unwrap();
    let input = file.to_string_lossy().to_string();
    match validate_input_path(&input) {
        InputPathValidation::Ok(_) => {}
        other => panic!("Expected Ok for path with forward slashes, got {:?}", other),
    }
}

#[test]
fn test_validate_control_char_in_filename() {
    let name = "foo\x00bar.txt".to_string();
    match validate_input_path(&name) {
        InputPathValidation::InvalidChars(msg) => {
            assert!(msg.contains("null byte"));
        }
        other => panic!("Expected InvalidChars(null), got {:?}", other),
    }
}

#[test]
fn test_validate_path_separators_are_valid() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_string_lossy().to_string();
    match validate_input_path(&path) {
        InputPathValidation::Ok(_) => {}
        other => panic!("Expected Ok for path with forward slashes, got {:?}", other),
    }
}

#[test]
fn test_validate_trailing_space() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("foo.txt ");
    let input = path.to_string_lossy().to_string();
    match validate_input_path(&input) {
        InputPathValidation::TrailingDotOrSpace => {}
        other => panic!("Expected TrailingDotOrSpace, got {:?}", other),
    }
}

#[test]
fn test_validate_trailing_dot() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("foo.");
    let input = path.to_string_lossy().to_string();
    match validate_input_path(&input) {
        InputPathValidation::TrailingDotOrSpace => {}
        other => panic!("Expected TrailingDotOrSpace, got {:?}", other),
    }
}

#[test]
fn test_validate_no_false_trailing_dot() {
    let dir = TempDir::new().unwrap();
    let parent = dir.path().parent().unwrap();
    if parent.exists() {
        match validate_input_path(parent.to_str().unwrap()) {
            InputPathValidation::Ok(_) => {}
            other => panic!("Expected Ok for parent dir '..', got {:?}", other),
        }
    }
}

#[test]
fn test_validate_existing_file() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    match validate_input_path(&file.to_string_lossy()) {
        InputPathValidation::Ok(p) => {
            assert!(p.is_absolute());
            assert!(p.to_string_lossy().contains("test.txt"));
        }
        other => panic!("Expected Ok, got {:?}", other),
    }
}

#[test]
fn test_validate_existing_directory() {
    let dir = TempDir::new().unwrap();
    match validate_input_path(&dir.path().to_string_lossy()) {
        InputPathValidation::Ok(p) => {
            assert!(p.is_absolute());
        }
        other => panic!("Expected Ok, got {:?}", other),
    }
}

#[test]
fn test_validate_not_found() {
    assert_eq!(
        validate_input_path("/nonexistent_path_xyzzy_123"),
        InputPathValidation::NotFound
    );
}

#[test]
fn test_validate_not_found_relative() {
    assert_eq!(
        validate_input_path("nonexistent_file_xyzzy_123.txt"),
        InputPathValidation::NotFound
    );
}

#[test]
fn test_validate_broken_symlink() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("nonexistent_target");
    let link = dir.path().join("broken_link");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, &link).unwrap();
        match validate_input_path(&link.to_string_lossy()) {
            InputPathValidation::NotAccessible(msg) => {
                assert!(msg.contains("symlink"));
                assert!(msg.contains("non-existent"));
            }
            other => {
                panic!(
                    "Expected NotAccessible(symlink) for broken symlink, got {:?}",
                    other
                )
            }
        }
    }
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_file(&target, &link).is_ok() {
            match validate_input_path(&link.to_string_lossy()) {
                InputPathValidation::NotAccessible(msg) => {
                    assert!(msg.contains("symlink"));
                }
                other => {
                    panic!(
                        "Expected NotAccessible(symlink) for broken symlink, got {:?}",
                        other
                    )
                }
            }
        }
    }
}

#[test]
fn test_validate_working_symlink() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("real_file");
    fs::write(&target, "content").unwrap();
    #[cfg(unix)]
    {
        let link = dir.path().join("working_link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        match validate_input_path(&link.to_string_lossy()) {
            InputPathValidation::Ok(p) => {
                assert!(p.to_string_lossy().contains("real_file"));
            }
            other => panic!("Expected Ok for working symlink, got {:?}", other),
        }
    }
    #[cfg(windows)]
    {
        let link = dir.path().join("working_link");
        if std::os::windows::fs::symlink_file(&target, &link).is_ok() {
            match validate_input_path(&link.to_string_lossy()) {
                InputPathValidation::Ok(p) => {
                    assert!(p.to_string_lossy().contains("real_file"));
                }
                other => panic!("Expected Ok for working symlink, got {:?}", other),
            }
        }
    }
}

#[test]
fn test_validate_as_expect_dir_ok() {
    let dir = TempDir::new().unwrap();
    match validate_input_path_as(&dir.path().to_string_lossy(), true) {
        InputPathValidation::Ok(_) => {}
        other => panic!("Expected Ok for directory, got {:?}", other),
    }
}

#[test]
fn test_validate_as_expect_dir_but_is_file() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("a_file.txt");
    fs::write(&file, "").unwrap();
    assert_eq!(
        validate_input_path_as(&file.to_string_lossy(), true),
        InputPathValidation::WrongType
    );
}

#[test]
fn test_validate_as_expect_file_ok() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("a_file.txt");
    fs::write(&file, "").unwrap();
    match validate_input_path_as(&file.to_string_lossy(), false) {
        InputPathValidation::Ok(_) => {}
        other => panic!("Expected Ok for file, got {:?}", other),
    }
}

#[test]
fn test_validate_as_expect_file_but_is_dir() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        validate_input_path_as(&dir.path().to_string_lossy(), false),
        InputPathValidation::WrongType
    );
}

#[test]
fn test_validate_canonicalizes_path() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("subdir");
    fs::create_dir_all(&sub).unwrap();

    let non_canon = sub.join("..").join(sub.file_name().unwrap());
    match validate_input_path(&non_canon.to_string_lossy()) {
        InputPathValidation::Ok(canon) => {
            assert_eq!(canon, sub.canonicalize().unwrap());
        }
        other => panic!("Expected Ok(canonicalized), got {:?}", other),
    }
}

#[test]
fn test_validate_root() {
    if cfg!(unix) {
        match validate_input_path("/") {
            InputPathValidation::Ok(p) => {
                assert_eq!(p, Path::new("/"));
            }
            other => panic!("Expected Ok for root, got {:?}", other),
        }
    }
}

#[test]
fn test_validate_current_dir() {
    let _lock = CWD_LOCK.lock().unwrap();
    let dir = TempDir::new().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    match validate_input_path(".") {
        InputPathValidation::Ok(p) => {
            assert!(p.is_absolute());
            assert_eq!(p, dir.path().canonicalize().unwrap());
        }
        other => panic!("Expected Ok for '.', got {:?}", other),
    }

    std::env::set_current_dir(original).unwrap();
}

#[test]
fn test_validate_parent_dir() {
    let _lock = CWD_LOCK.lock().unwrap();
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("subdir");
    fs::create_dir_all(&sub).unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&sub).unwrap();

    match validate_input_path("..") {
        InputPathValidation::Ok(p) => {
            assert!(p.is_absolute());
            assert_eq!(p, dir.path().canonicalize().unwrap());
        }
        other => panic!("Expected Ok for '..', got {:?}", other),
    }

    std::env::set_current_dir(original).unwrap();
}

#[test]
fn test_validate_home_directory() {
    if let Some(home) = dirs::home_dir() {
        match validate_input_path(&home.to_string_lossy()) {
            InputPathValidation::Ok(p) => {
                assert!(p.is_absolute());
            }
            other => panic!("Expected Ok for home, got {:?}", other),
        }
    }
}

#[test]
fn test_validate_forward_slash_on_windows() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sub");
    fs::create_dir_all(&path).unwrap();
    let fwd = dir.path().to_string_lossy().replace('\\', "/");
    let fwd_path = Path::new(&fwd).join("sub");
    let input = fwd_path.to_string_lossy().to_string();
    match validate_input_path(&input) {
        InputPathValidation::Ok(_) => {}
        other => panic!("Expected Ok for forward-slash path, got {:?}", other),
    }
}

#[test]
fn test_validate_unc_path() {
    let result = validate_input_path("//server/share/path");
    assert_eq!(
        result,
        InputPathValidation::NotFound,
        "UNC-like path should be NotFound, not InvalidChars: got {:?}",
        result
    );
}

#[test]
fn test_validate_empty_trumps_all_other_checks() {
    assert_eq!(validate_input_path(""), InputPathValidation::Empty);
    assert_eq!(validate_input_path("   "), InputPathValidation::Empty);
}

#[test]
fn test_validate_component_trumps_path_length() {
    let long_path = format!("{}/ok", "a".repeat(300));
    match validate_input_path(&long_path) {
        InputPathValidation::ComponentTooLong { .. } => {}
        other => panic!(
            "Expected ComponentTooLong (trumps PathTooLong), got {:?}",
            other
        ),
    }
}

#[test]
fn test_validate_path_too_long_fires_when_components_are_short() {
    let long_path = "/ok".repeat(2000);
    match validate_input_path(&long_path) {
        InputPathValidation::PathTooLong { .. } => {}
        other => panic!(
            "Expected PathTooLong (no component exceeds limit), got {:?}",
            other
        ),
    }
}

#[test]
fn test_validate_null_byte_trumps_forbidden_char() {
    match validate_input_path("bad\0<file>.txt") {
        InputPathValidation::InvalidChars(msg) => {
            assert!(msg.contains("null byte"));
        }
        other => panic!("Expected InvalidChars(null), got {:?}", other),
    }
}
