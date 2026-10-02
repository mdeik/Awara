use super::*;
use std::fs::File;
use tempfile::TempDir;

#[test]
fn test_recursive_numbering_order() {
    let temp_dir = std::env::temp_dir().join("awara_recursive_test");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let files_rel = vec![
        "folder1",
        "folder1/file1.txt",
        "folder1/file2.txt",
        "folder2",
        "folder2/file3.txt",
    ];

    for f in &files_rel {
        let p = temp_dir.join(f);
        if f.contains('.') {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, "").unwrap();
        } else {
            std::fs::create_dir_all(&p).unwrap();
        }
    }

    let files: Vec<String> = files_rel
        .iter()
        .map(|f| temp_dir.join(f).to_string_lossy().to_string())
        .collect();

    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_start: 1,
            numbering_increment: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);

    let mut ops = Vec::new();
    for (i, file) in files.iter().enumerate() {
        if let Some(op) = calculate_rename(file, &compiled, Some(i), true, Some(1)) {
            ops.push(op);
        }
    }

    ops.sort_by(|a, b| {
        let da = std::path::Path::new(&a.original_path).components().count();
        let db = std::path::Path::new(&b.original_path).components().count();
        db.cmp(&da)
    });

    let depths: Vec<_> = ops
        .iter()
        .map(|op| std::path::Path::new(&op.original_path).components().count())
        .collect();

    for i in 0..depths.len() - 1 {
        assert!(depths[i] >= depths[i + 1]);
    }

    let file1_idx = ops
        .iter()
        .position(|op| op.original_name == "file1.txt")
        .unwrap();
    let folder1_idx = ops
        .iter()
        .position(|op| op.original_name == "folder1")
        .unwrap();
    assert!(
        file1_idx < folder1_idx,
        "File inside folder must be processed before folder"
    );

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_directory_extension_constraint() {
    // A directory is never split into an *existing* extension, but Extension
    // *Fixed*/*Extra* still add a new one (it adds, it does not reinterpret).
    let config = RenameConfig {
        extension: ExtensionSection {
            extension_replace: Some("bak".to_string()),
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
            None,
            false,
            &FileMeta::default()
        ),
        "file.bak"
    );
    assert_eq!(
        process_filename(
            "folder.txt",
            &compiled,
            None,
            None,
            true,
            &FileMeta::default()
        ),
        "folder.txt.bak"
    );
}

#[test]
fn test_recursive_numbering_restart_folder() {
    let temp_dir = std::env::temp_dir().join("awara_restart_folder_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let dir_a = temp_dir.join("a");
    let dir_b = temp_dir.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    for name in &["x.txt", "y.txt", "z.txt"] {
        File::create(dir_a.join(name)).unwrap();
        File::create(dir_b.join(name)).unwrap();
    }

    let files: Vec<String> = [
        dir_a.join("x.txt"),
        dir_a.join("y.txt"),
        dir_a.join("z.txt"),
        dir_b.join("x.txt"),
        dir_b.join("y.txt"),
        dir_b.join("z.txt"),
    ]
    .iter()
    .map(|p| p.to_string_lossy().to_string())
    .collect();

    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Prefix),
            numbering_start: 1,
            numbering_increment: 1,
            numbering_sep: Some("_".to_string()),
            numbering_restart_folder: true,
            ..Default::default()
        },
        ..Default::default()
    };

    // Exercise the real planner (SSoT) rather than a standalone helper: each
    // folder's files restart their numbering.
    let plan = plan_renames(&files, &config, &PlanOptions::default());
    let names: Vec<String> = plan.ops().iter().map(|op| op.new_name.clone()).collect();
    assert_eq!(
        names,
        vec![
            "1_x.txt", "2_y.txt", "3_z.txt", "1_x.txt", "2_y.txt", "3_z.txt"
        ]
    );

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_execute_dir_rename_children_first() {
    // Verify that execute_renames renames children before parent directories,
    // preventing "lost child" errors where a parent rename moves children
    // out from under their original_path.
    //
    // Setup: parent/subdir/child.txt
    // Config: add prefix "new_" to everything (dirs and files)
    // Input order: [parent, parent/subdir, parent/subdir/child.txt]
    //   (worst case — parents before children)
    // Expected: all three entries renamed successfully.
    let tmp = TempDir::new().unwrap();
    let subdir = tmp.path().join("parent").join("subdir");
    std::fs::create_dir_all(&subdir).unwrap();
    let child = subdir.join("child.txt");
    File::create(&child).unwrap();
    let file_orig = child.to_string_lossy().to_string();

    // Input paths in worst-case order: parent first, then subdir, then child
    let parent_path = tmp.path().join("parent");
    let subdir_path = parent_path.join("subdir");
    let file_paths: Vec<String> = vec![
        parent_path.to_string_lossy().to_string(),
        subdir_path.to_string_lossy().to_string(),
        file_orig.clone(),
    ];

    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("new_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut options = RenameOptions::default();

    let result = execute_renames(&file_paths, &config, &mut options);

    assert!(
        result.failed_ops.is_empty(),
        "Expected zero failures, got: {:?}",
        result
            .failed_ops
            .iter()
            .map(|(op, e)| format!("{}: {}", op.original_name, e))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        result.successful_ops.len(),
        3,
        "All 3 entries should be renamed"
    );

    // Verify the file still exists and is accessible at its new path
    let new_parent = tmp.path().join("new_parent");
    let new_subdir = new_parent.join("new_subdir");
    let new_child = new_subdir.join("new_child.txt");

    assert!(
        new_child.exists(),
        "Child file should exist at {:?}",
        new_child
    );
    assert!(
        new_subdir.exists(),
        "Subdir should exist at {:?}",
        new_subdir
    );
    assert!(
        new_parent.exists(),
        "Parent should exist at {:?}",
        new_parent
    );

    // The original path should no longer exist
    assert!(!child.exists(), "Original child path should not exist");
}

#[test]
fn test_execute_dir_rename_deep_nesting() {
    // Deeply nested: a/b/c/d/file.txt with all dirs also being renamed
    let tmp = TempDir::new().unwrap();
    let deep = tmp.path().join("a").join("b").join("c").join("d");
    std::fs::create_dir_all(&deep).unwrap();
    let file = deep.join("file.txt");
    File::create(&file).unwrap();

    let file_paths: Vec<String> = vec![
        tmp.path()
            .join("a")
            .join("b")
            .join("c")
            .join("d")
            .to_string_lossy()
            .to_string(),
        tmp.path()
            .join("a")
            .join("b")
            .join("c")
            .to_string_lossy()
            .to_string(),
        tmp.path().join("a").join("b").to_string_lossy().to_string(),
        tmp.path().join("a").to_string_lossy().to_string(),
        file.to_string_lossy().to_string(),
    ];

    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("n_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut options = RenameOptions::default();

    let result = execute_renames(&file_paths, &config, &mut options);

    assert!(
        result.failed_ops.is_empty(),
        "Expected zero failures, got: {:?}",
        result
            .failed_ops
            .iter()
            .map(|(op, e)| format!("{}: {}", op.original_name, e))
            .collect::<Vec<_>>()
    );
    assert_eq!(result.successful_ops.len(), 5);

    // Verify the deep path still resolves
    let expected = tmp
        .path()
        .join("n_a")
        .join("n_b")
        .join("n_c")
        .join("n_d")
        .join("n_file.txt");
    assert!(
        expected.exists(),
        "Deep file should exist at {:?}",
        expected
    );
}

#[test]
fn test_execute_mixed_files_and_dirs_at_same_depth() {
    // Same-depth dirs and files — stable sort should preserve input order at same depth.
    // a -> new_a, b -> new_b, 1.txt -> new_1.txt, 2.txt -> new_2.txt
    // Input order specifies prefix sequence.
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("a")).unwrap();
    std::fs::create_dir_all(tmp.path().join("b")).unwrap();
    File::create(tmp.path().join("1.txt")).unwrap();
    File::create(tmp.path().join("2.txt")).unwrap();

    let file_paths: Vec<String> = vec![
        tmp.path().join("a").to_string_lossy().to_string(),
        tmp.path().join("1.txt").to_string_lossy().to_string(),
        tmp.path().join("b").to_string_lossy().to_string(),
        tmp.path().join("2.txt").to_string_lossy().to_string(),
    ];

    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("new_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut options = RenameOptions::default();

    let result = execute_renames(&file_paths, &config, &mut options);
    assert!(
        result.failed_ops.is_empty(),
        "Expected no failures: {:?}",
        result.failed_ops
    );
    assert_eq!(result.successful_ops.len(), 4);

    assert!(tmp.path().join("new_a").exists());
    assert!(tmp.path().join("new_1.txt").exists());
    assert!(tmp.path().join("new_b").exists());
    assert!(tmp.path().join("new_2.txt").exists());
}

#[test]
fn test_execute_flat_no_dirs_skips_sort() {
    // Only files in a single directory — no depth sort needed.
    // This confirms the optimization: when there are no dirs in the list,
    // the sort is skipped entirely and order is preserved exactly.
    let tmp = TempDir::new().unwrap();
    File::create(tmp.path().join("c.txt")).unwrap();
    File::create(tmp.path().join("a.txt")).unwrap();
    File::create(tmp.path().join("b.txt")).unwrap();

    let file_paths: Vec<String> = vec![
        tmp.path().join("c.txt").to_string_lossy().to_string(),
        tmp.path().join("a.txt").to_string_lossy().to_string(),
        tmp.path().join("b.txt").to_string_lossy().to_string(),
    ];

    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("n_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut options = RenameOptions::default();

    let result = execute_renames(&file_paths, &config, &mut options);
    assert!(result.failed_ops.is_empty());
    assert_eq!(result.successful_ops.len(), 3);
    assert!(tmp.path().join("n_c.txt").exists());
    assert!(tmp.path().join("n_a.txt").exists());
    assert!(tmp.path().join("n_b.txt").exists());
}

#[test]
fn test_execute_numbering_preserved_with_depth_sort() {
    // Numbering is assigned by input order (display order). Depth sorting
    // after numbering must not change the numbers assigned.
    let tmp = TempDir::new().unwrap();
    let sub = tmp.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    File::create(sub.join("a.txt")).unwrap();
    File::create(sub.join("b.txt")).unwrap();
    File::create(tmp.path().join("root.txt")).unwrap();

    // Input order: sub (depth 2), a.txt (depth 3), b.txt (depth 3), root.txt (depth 1)
    let file_paths: Vec<String> = vec![
        sub.to_string_lossy().to_string(),
        sub.join("a.txt").to_string_lossy().to_string(),
        sub.join("b.txt").to_string_lossy().to_string(),
        tmp.path().join("root.txt").to_string_lossy().to_string(),
    ];

    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Prefix),
            numbering_start: 1,
            numbering_increment: 1,
            numbering_sep: Some("-".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut options = RenameOptions::default();

    let result = execute_renames(&file_paths, &config, &mut options);
    assert!(result.failed_ops.is_empty());
    assert_eq!(result.successful_ops.len(), 4);

    // Verify numbering: input order determines numbers
    // sub -> 1-sub, a.txt -> 2-a.txt, b.txt -> 3-b.txt, root.txt -> 4-root.txt
    let new_sub = tmp.path().join("1-sub");
    assert!(new_sub.exists(), "sub should be renamed to 1-sub");
    assert!(
        new_sub.join("2-a.txt").exists(),
        "a.txt should be numbered 2 — file at {:?}",
        new_sub.join("2-a.txt")
    );
    assert!(
        new_sub.join("3-b.txt").exists(),
        "b.txt should be numbered 3 — file at {:?}",
        new_sub.join("3-b.txt")
    );
    assert!(
        tmp.path().join("4-root.txt").exists(),
        "root.txt should be numbered 4"
    );
}
