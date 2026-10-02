use super::*;
use std::path::PathBuf;

// ── parse_gui_args tests ─────────────────────────────────────────────
// These verify that the --gui argument parsing correctly dispatches to
// the right handler for each context-menu case.

#[test]
fn test_parse_gui_no_args() {
    let args = vec!["--gui".to_string()];
    let result = parse_gui_args(&args);
    assert_eq!(
        result,
        GuiDispatch::NoFiles {
            theme: "system".into()
        }
    );
}

#[test]
fn test_parse_gui_theme_only() {
    let args = vec!["--gui".to_string(), "dark".to_string()];
    let result = parse_gui_args(&args);
    assert_eq!(
        result,
        GuiDispatch::NoFiles {
            theme: "dark".into()
        }
    );
}

#[test]
fn test_parse_gui_single_file() {
    let args = vec!["--gui".to_string(), "photo.jpg".to_string()];
    let result = parse_gui_args(&args);
    // photo.jpg is not a directory (doesn't exist), so it goes to Files
    assert_eq!(
        result,
        GuiDispatch::Files {
            theme: "system".into(),
            paths: vec![PathBuf::from("photo.jpg")],
        }
    );
}

#[test]
fn test_parse_gui_single_dir() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().to_path_buf();
    let args = vec!["--gui".to_string(), dir_path.to_string_lossy().to_string()];
    let result = parse_gui_args(&args);
    assert_eq!(
        result,
        GuiDispatch::Directory {
            theme: "system".into(),
            dir: dir_path,
        }
    );
}

#[test]
fn test_parse_gui_dark_theme_with_dir() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().to_path_buf();
    let args = vec![
        "--gui".to_string(),
        "dark".to_string(),
        dir_path.to_string_lossy().to_string(),
    ];
    let result = parse_gui_args(&args);
    assert_eq!(
        result,
        GuiDispatch::Directory {
            theme: "dark".into(),
            dir: dir_path,
        }
    );
}

#[test]
fn test_parse_gui_dark_theme_with_files() {
    let args = vec![
        "--gui".to_string(),
        "dark".to_string(),
        "a.txt".to_string(),
        "b.txt".to_string(),
    ];
    let result = parse_gui_args(&args);
    assert_eq!(
        result,
        GuiDispatch::Files {
            theme: "dark".into(),
            paths: vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")],
        }
    );
}

#[test]
fn test_parse_gui_unknown_theme_treated_as_file() {
    // "foo" is not a recognized theme, so it's treated as a file path
    let args = vec!["--gui".to_string(), "foo".to_string()];
    let result = parse_gui_args(&args);
    assert_eq!(
        result,
        GuiDispatch::Files {
            theme: "system".into(),
            paths: vec![PathBuf::from("foo")],
        }
    );
}
