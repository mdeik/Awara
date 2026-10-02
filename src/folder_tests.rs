use super::*;

#[test]
fn test_rename_folder() {
    let temp_dir = std::env::temp_dir().join("awara_test_folder");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let folder_path = temp_dir.join("my_folder");
    std::fs::create_dir_all(&folder_path).unwrap();

    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("new_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);

    let op = calculate_rename(
        folder_path.to_str().unwrap(),
        &compiled,
        None,
        true,
        Some(1),
    )
    .unwrap();
    assert_eq!(op.new_name, "new_my_folder");

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_rename_folder_empty_result_keeps_original() {
    let config = RenameConfig {
        remove: RemoveSection {
            remove_all_chars: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    let result = process_filename(
        "my_folder",
        &compiled,
        None,
        None,
        true,
        &FileMeta::default(),
    );
    assert_eq!(result, "my_folder");
}
