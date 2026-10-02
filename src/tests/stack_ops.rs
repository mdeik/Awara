use super::*;

#[test]
fn test_add_dirname() {
    let config = RenameConfig {
        append_folder: AppendFolderSection {
            add_dirname: true,
            add_dirname_sep: Some("-".to_string()),
            add_dirname_pos: Some(0),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "file.txt",
            &compiled,
            None,
            Some("parent"),
            false,
            &FileMeta::default()
        ),
        "parent-file.txt"
    );

    let config_suffix = RenameConfig {
        append_folder: AppendFolderSection {
            add_dirname: true,
            add_dirname_sep: Some("-".to_string()),
            add_dirname_pos: Some(100),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled_suffix = CompiledConfig::new(config_suffix);
    assert_eq!(
        process_filename(
            "file.txt",
            &compiled_suffix,
            None,
            Some("parent"),
            false,
            &FileMeta::default()
        ),
        "file-parent.txt"
    );
}

#[test]
fn test_add_dirname_multiple_levels() {
    // lvl covers ALL folders in the range (root-ward → parent), each
    // separated by `sep` — not just the single folder at that depth.
    use tempfile::TempDir;
    let tmp = TempDir::new().unwrap();
    let deep = tmp.path().join("a").join("b").join("c");
    std::fs::create_dir_all(&deep).unwrap();
    let file = deep.join("file.txt");
    std::fs::write(&file, "").unwrap();

    let config = RenameConfig {
        append_folder: AppendFolderSection {
            add_dirname: true,
            add_dirname_sep: Some("-".to_string()),
            add_dirname_pos: Some(0),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let path_str = file.to_string_lossy();

    // lvl=1: only the immediate parent.
    let op = calculate_rename(&path_str, &compiled, None, true, Some(1)).unwrap();
    assert_eq!(op.new_name, "c-file.txt");

    // lvl=2: parent + grandparent.
    let op = calculate_rename(&path_str, &compiled, None, true, Some(2)).unwrap();
    assert_eq!(op.new_name, "b-c-file.txt");

    // lvl=3: all folders in range.
    let op = calculate_rename(&path_str, &compiled, None, true, Some(3)).unwrap();
    assert_eq!(op.new_name, "a-b-c-file.txt");
}

#[test]
fn test_move_part_logic() {
    let config = RenameConfig {
        move_copy: MoveCopySection {
            move_part: Some(MoveCopyValue {
                start: 1,
                length: 3,
                destination: 7,
                separator: None,
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "123456.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "456123.txt"
    );
}

#[test]
fn test_move_part_separator_roundtrip() {
    // Config → items → config preserves the separator, so what the GUI
    // displays (Sep field) survives export/import.
    let config = RenameConfig {
        move_copy: MoveCopySection {
            move_part: Some(MoveCopyValue {
                start: 1,
                length: 3,
                destination: -1,
                separator: Some("_".into()),
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let items = make_move_copy_items(&config);
    assert_eq!(items.len(), 1);
    let mut back = RenameConfig::default();
    items_to_config(items, &mut back);
    let v = back.move_copy.move_part.unwrap();
    assert_eq!((v.start, v.length, v.destination), (1, 3, -1));
    assert_eq!(v.separator.as_deref(), Some("_"));
}

#[test]
fn test_copy_part_logic() {
    let config = RenameConfig {
        move_copy: MoveCopySection {
            copy_part: Some(MoveCopyValue {
                start: 1,
                length: 3,
                destination: 7,
                separator: None,
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "123456.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "123456123.txt"
    );
}

#[test]
fn test_remove_high_accents() {
    let config = RenameConfig {
        remove: RemoveSection {
            remove_high: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "café🦀.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "caf.txt"
    );

    let config_accents = RenameConfig {
        remove: RemoveSection {
            remove_accents: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled_accents = CompiledConfig::new(config_accents);
    assert_eq!(
        process_filename(
            "café🦀.txt",
            &compiled_accents,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "cafe🦀.txt"
    );
}

#[test]
fn test_extension_mode_case_applied() {
    let config = RenameConfig {
        extension: ExtensionSection {
            extension_mode: Some(ExtensionMode::Lower),
            extension_replace: Some("TXT".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "file.HTML",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "file.txt"
    );

    let config_upper = RenameConfig {
        extension: ExtensionSection {
            extension_mode: Some(ExtensionMode::Upper),
            extension_replace: Some("txt".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled_upper = CompiledConfig::new(config_upper);
    assert_eq!(
        process_filename(
            "file.html",
            &compiled_upper,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "file.TXT"
    );
}

#[test]
fn test_stacking_extension_mode_applied() {
    let config = RenameConfig {
        command_order: vec![RenameItem::Extension(
            ExtensionMode::Upper,
            Some("txt".to_string()),
            None,
            false,
        )],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "file.html",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "file.TXT"
    );
}

#[test]
fn test_stacking_multi_flag_commands() {
    let config = RenameConfig {
        command_order: vec![
            RenameItem::Prefix("pre_".to_string()),
            RenameItem::Suffix("_suf".to_string()),
        ],
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "file.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "pre_file_suf.txt"
    );
}

#[test]
fn test_stacking_remove_words_compiled() {
    let mut config = RenameConfig::default();
    config.remove.remove_words = Some("bad ugly".to_string());
    config.command_order = vec![RenameItem::RemoveWords(
        "bad ugly".to_string(),
        config.replace.replace_case_sensitive,
    )];
    let compiled = CompiledConfig::new(config);
    // Word removal leaves the surrounding spaces intact — they are no
    // longer trimmed from the stem.
    assert_eq!(
        process_filename(
            "bad file ugly.txt",
            &compiled,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        " file .txt"
    );
}
