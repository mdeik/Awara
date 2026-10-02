use super::*;
use tempfile::TempDir;

#[test]
fn test_persist_roundtrip() {
    let mut app = GuiApp::new_with_config_for_test();
    // Populate some fields to verify they survive serialization
    app.config.regex.regex_match = Some("test".into());
    app.config.replace.replace = Some("foo".into());
    app.config.numbering.numbering_start = 42;
    app.config.filters.filter_files = false;
    app.config.stop_on_error = true;
    app.config.filters.filter_pattern = Some("*.rs".into());
    app.timestamps.created.mode = "fixed".into();
    app.section_enabled[SectionId::Regex as usize] = false;
    app.config.section_order = vec![
        "Name".to_string(),
        "Numbering".to_string(),
        "Replace".to_string(),
    ];
    app.layout.col_widths = vec![100.0, 200.0, 50.0];
    app.layout.col_order = vec![2, 0, 1];

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");

    // Save to temp path
    GuiPersistedState::save_to(&path, &app);
    assert!(path.exists(), "state file should exist after save");

    // Load back
    let loaded = GuiPersistedState::load_from(&path).expect("load_from should succeed");

    // Verify all fields match
    assert_eq!(
        loaded.doc.rename_config.regex.regex_match,
        Some("test".into())
    );
    assert_eq!(loaded.doc.rename_config.replace.replace, Some("foo".into()));
    assert_eq!(loaded.doc.rename_config.numbering.numbering_start, 42);
    assert!(!loaded.doc.rename_config.filters.filter_files);
    assert!(loaded.doc.rename_config.stop_on_error);
    assert_eq!(
        loaded.doc.rename_config.filters.filter_pattern,
        Some("*.rs".into())
    );
    assert_eq!(loaded.timestamps.created.mode, "fixed");
    assert!(loaded.doc.disabled_sections.contains(&SectionId::Regex));
    assert_eq!(
        loaded.doc.rename_config.section_order,
        vec![
            "Name".to_string(),
            "Numbering".to_string(),
            "Replace".to_string()
        ]
    );

    // Table layout lives in the always-saved UI settings, not the rename state.
    let settings_path = dir.path().join("app_settings.json");
    AppSettings::save_to(&settings_path, &app);
    let loaded_settings = AppSettings::load_from(&settings_path).expect("settings should load");
    assert_eq!(loaded_settings.table_col_widths, vec![100.0, 200.0, 50.0]);
    assert_eq!(loaded_settings.table_col_order, vec![2, 0, 1]);

    // The GUI state file is a superset of the shared rename document, so the
    // CLI/TUI parser can read it (GUI-only keys are ignored).
    let raw = std::fs::read_to_string(&path).unwrap();
    let doc = awara::RenameDoc::from_json(&raw).expect("GUI state parses as a rename document");
    assert_eq!(doc.rename_config.replace.replace, Some("foo".into()));
    assert_eq!(doc.enabled_sections(), app.section_enabled);
}

#[test]
fn test_persist_missing_file_returns_none() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.json");
    assert!(!path.exists());
    assert!(GuiPersistedState::load_from(&path).is_none());
}

#[test]
fn test_persist_corrupt_file_returns_none() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("corrupt.json");
    std::fs::write(&path, "this is not valid json").unwrap();
    assert!(GuiPersistedState::load_from(&path).is_none());
}

#[test]
fn test_persist_remember_config_default_false() {
    let app = GuiApp::new_with_config_for_test();
    assert!(
        !app.remember_rename_options,
        "remember_rename_options should default to false"
    );
    assert!(
        app.remember_last_dir,
        "remember_last_dir should default to true"
    );
}

#[test]
fn test_persist_save_state_deletes_file_when_toggled_off() {
    let dir = TempDir::new().unwrap();
    let mut app = GuiApp::new_with_config_for_test();
    app.config_dir = Some(dir.path().to_path_buf());
    app.remember_rename_options = true;
    app.config.replace.replace = Some("test".into());

    // Save via the state path (isolated to temp dir)
    app.save_state();
    let path = GuiPersistedState::state_path_in(Some(dir.path()))
        .expect("state_path should return a path");
    assert!(
        path.exists(),
        "state file should exist after save with toggle on"
    );

    // Toggle off and save again — should delete the file
    app.remember_rename_options = false;
    app.save_state();
    assert!(
        !path.exists(),
        "state file should be deleted when toggle is off"
    );
}

#[test]
fn test_app_settings_roundtrip() {
    let mut app = GuiApp::new_with_config_for_test();
    app.remember_rename_options = true;
    app.remember_last_dir = true;
    app.auto_refresh = false;
    app.cwd = PathBuf::from("/some/custom/dir");
    app.layout.col_widths = vec![210.0, 90.0, 70.0, 160.0, 55.0, 120.0];
    app.layout.col_order = vec![5, 0, 1, 2, 3, 4];
    app.revert_col_widths = vec![200.0, 140.0, 120.0, 95.0, 130.0, 115.0];
    app.preview_sort_col = PreviewSortCol::Size;
    app.preview_sort_asc = false;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app_settings.json");

    AppSettings::save_to(&path, &app);
    assert!(path.exists(), "app_settings file should exist after save");

    let loaded = AppSettings::load_from(&path).expect("load_from should succeed");
    assert!(loaded.remember_rename_options);
    assert!(loaded.remember_last_dir);
    assert!(!loaded.auto_refresh);
    assert_eq!(loaded.last_dir, "/some/custom/dir");
    assert_eq!(loaded.table_col_widths, app.layout.col_widths);
    assert_eq!(loaded.table_col_order, app.layout.col_order);
    assert_eq!(loaded.revert_col_widths, app.revert_col_widths);
    assert_eq!(loaded.preview_sort_col, PreviewSortCol::Size);
    assert!(!loaded.preview_sort_asc);
}

#[test]
fn test_app_settings_sort_defaults_for_legacy_file() {
    // Settings files written before sort persistence was added lack the sort
    // fields; they must still load, falling back to Name/ascending.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app_settings.json");
    std::fs::write(
        &path,
        r#"{
            "remember_rename_options": true,
            "remember_last_dir": true,
            "auto_refresh": true,
            "last_dir": "/some/dir",
            "skip_trash_confirmation": false,
            "table_col_widths": [180.0, 180.0, 60.0, 80.0, 150.0, 60.0],
            "table_col_order": [0, 1, 2, 3, 4, 5],
            "revert_col_widths": [200.0, 140.0, 120.0, 95.0, 130.0, 115.0]
        }"#,
    )
    .unwrap();

    let loaded = AppSettings::load_from(&path).expect("legacy settings should load");
    assert_eq!(loaded.preview_sort_col, PreviewSortCol::Name);
    assert!(loaded.preview_sort_asc);
}

#[test]
fn test_app_settings_missing_file_returns_none() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.json");
    assert!(!path.exists());
    assert!(AppSettings::load_from(&path).is_none());
}

#[test]
fn test_app_settings_corrupt_file_returns_none() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("corrupt.json");
    std::fs::write(&path, "this is not valid json").unwrap();
    assert!(AppSettings::load_from(&path).is_none());
}

#[test]
fn test_save_last_dir_respects_toggle() {
    let dir = TempDir::new().unwrap();
    let mut app = GuiApp::new_with_config_for_test();
    app.config_dir = Some(dir.path().to_path_buf());
    app.remember_last_dir = false;
    app.cwd = PathBuf::from("/some/dir");

    let settings_path =
        AppSettings::state_path_in(Some(dir.path())).expect("state_path should return a path");

    // Should NOT save when toggle is off
    app.save_last_dir();
    assert!(
        !settings_path.exists(),
        "should not save when remember_last_dir is off"
    );

    // Should save when toggle is on
    app.remember_last_dir = true;
    app.save_last_dir();
    assert!(
        settings_path.exists(),
        "should save when remember_last_dir is on"
    );
}
#[test]
fn test_gui_edit_state_persist_roundtrip() {
    let mut app = GuiApp::new_with_config_for_test();
    // Populate edit_state
    app.edit_state.auto_date_type = Some("creation_curr".into());
    app.edit_state.auto_date_format_key = Some("ymd_hms".into());
    app.edit_state.auto_date_sep = Some("-".into());
    app.edit_state.auto_date_custom = Some("YYYY/MM/DD".into());
    app.edit_state.move_part_action = Some("Copy first n".into());
    app.edit_state.move_part_val1 = 5;
    app.edit_state.move_part_val2 = 3;
    app.edit_state.move_part_dest = Some("To end".into());
    app.edit_state.move_part_dest_val = 10;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    GuiPersistedState::save_to(&path, &app);

    let loaded = GuiPersistedState::load_from(&path).unwrap();
    assert_eq!(
        loaded.edit_state.auto_date_type,
        Some("creation_curr".into())
    );
    assert_eq!(
        loaded.edit_state.auto_date_format_key,
        Some("ymd_hms".into())
    );
    assert_eq!(loaded.edit_state.auto_date_sep, Some("-".into()));
    assert_eq!(
        loaded.edit_state.auto_date_custom,
        Some("YYYY/MM/DD".into())
    );
    assert_eq!(
        loaded.edit_state.move_part_action,
        Some("Copy first n".into())
    );
    assert_eq!(loaded.edit_state.move_part_val1, 5);
    assert_eq!(loaded.edit_state.move_part_val2, 3);
    assert_eq!(loaded.edit_state.move_part_dest, Some("To end".into()));
    assert_eq!(loaded.edit_state.move_part_dest_val, 10);
}
