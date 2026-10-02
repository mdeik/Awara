use awara::{
    AddSection, CompiledConfig, RenameConfig, ReplaceSection, calculate_rename, cancel_token,
};
use std::cell::Cell;
use std::fs::File;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use tempfile::tempdir;

#[test]
fn test_preview_recompute() {
    let dir = tempdir().unwrap();
    let file1 = dir.path().join("file1.txt");
    let file2 = dir.path().join("file2.txt");
    File::create(&file1).unwrap();
    File::create(&file2).unwrap();

    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("test_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);

    let op1 = calculate_rename(file1.to_str().unwrap(), &compiled, Some(0), true, Some(1)).unwrap();
    let op2 = calculate_rename(file2.to_str().unwrap(), &compiled, Some(0), true, Some(1)).unwrap();

    assert_eq!(op1.new_name, "test_file1.txt");
    assert_eq!(op2.new_name, "test_file2.txt");
}

#[test]
fn test_conflict_detection_simulation() {
    let dir = tempdir().unwrap();
    let foo = dir.path().join("foo.txt");
    let bar = dir.path().join("bar.txt");
    File::create(&foo).unwrap();
    File::create(&bar).unwrap();

    let config = RenameConfig {
        replace: ReplaceSection {
            replace: Some("foo".to_string()),
            with: Some("bar".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);

    let op = calculate_rename(foo.to_str().unwrap(), &compiled, None, true, Some(1)).unwrap();
    assert_eq!(op.new_name, "bar.txt");
    assert!(op.new_path.exists());
}

fn stacking_config(items: Vec<awara::RenameItem>) -> RenameConfig {
    RenameConfig {
        command_order: items,
        ..Default::default()
    }
}

#[test]
fn test_stacking_multiple_prefixes() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("file.txt");
    File::create(&f).unwrap();

    let config = stacking_config(vec![
        awara::RenameItem::Prefix("pre1_".into()),
        awara::RenameItem::Prefix("pre2_".into()),
    ]);
    let compiled = CompiledConfig::new(config);
    let op = calculate_rename(f.to_str().unwrap(), &compiled, None, true, Some(1)).unwrap();
    assert_eq!(
        op.new_name, "pre2_pre1_file.txt",
        "prefixes are applied sequentially, so the last prefix added ends up leftmost"
    );
}

#[test]
fn test_stacking_multiple_remove_first() {
    let config = stacking_config(vec![
        awara::RenameItem::RemoveFirst(2),
        awara::RenameItem::RemoveFirst(3),
    ]);
    let compiled = CompiledConfig::new(config);
    let result = awara::process_filename(
        "abcdef.txt",
        &compiled,
        None,
        None,
        false,
        &awara::FileMeta::default(),
    );
    assert_eq!(
        result, "f.txt",
        "two RemoveFirst should chain: remove 2, then 3 more"
    );
}

#[test]
fn test_stacking_replace_then_case() {
    let config = stacking_config(vec![
        awara::RenameItem::Replace("hello".into(), "world".into(), false, false),
        awara::RenameItem::CaseName(awara::CaseMode::Upper),
    ]);
    let compiled = CompiledConfig::new(config);
    let result = awara::process_filename(
        "hello.txt",
        &compiled,
        None,
        None,
        false,
        &awara::FileMeta::default(),
    );
    assert_eq!(
        result, "WORLD.txt",
        "replace then case should apply in stacking order"
    );
}

#[test]
fn test_stacking_order_matters() {
    let config = stacking_config(vec![
        awara::RenameItem::CaseName(awara::CaseMode::Upper),
        awara::RenameItem::Replace("HELLO".into(), "world".into(), false, false),
    ]);
    let compiled = CompiledConfig::new(config);
    let result = awara::process_filename(
        "hello.txt",
        &compiled,
        None,
        None,
        false,
        &awara::FileMeta::default(),
    );
    assert_eq!(
        result, "world.txt",
        "reverse order should give different result"
    );
}

#[test]
fn test_stacking_preview_matches_execution() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("hello_world.txt");
    File::create(&f).unwrap();

    let config = stacking_config(vec![
        awara::RenameItem::RemoveFirst(6),
        awara::RenameItem::Prefix("prefix_".into()),
        awara::RenameItem::DoubleSpaces,
    ]);
    let compiled = CompiledConfig::new(config.clone());

    let op = calculate_rename(f.to_str().unwrap(), &compiled, None, true, Some(1)).unwrap();
    assert_eq!(op.new_name, "prefix_world.txt");

    let file_strs = vec![f.to_string_lossy().to_string()];
    let mut options = awara::RenameOptions::default();
    let result = awara::execute_renames(&file_strs, &config, &mut options);
    assert_eq!(result.successful_ops.len(), 1);
    assert_eq!(result.successful_ops[0].new_name, "prefix_world.txt");
    assert!(dir.path().join("prefix_world.txt").exists());
}

#[test]
fn test_cancel_before_execution() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("test.txt");
    File::create(&f).unwrap();

    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("pre_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let (token, signal) = cancel_token();
    signal.store(true, Ordering::SeqCst);

    let file_strs = vec![f.to_string_lossy().to_string()];
    let cancelled = Rc::new(Cell::new(false));
    let cancelled_clone = Rc::clone(&cancelled);
    let mut options = awara::RenameOptions {
        dry_run: false,
        stop_on_error: true,
        preserve_timestamps: false,
        set_attributes: None,
        undo_file: None,
        cancel_token: Some(token),
        on_event: Some(Box::new(move |event| {
            if matches!(event, awara::RenameEvent::Cancelled { .. }) {
                cancelled_clone.set(true);
            }
        })),
    };
    let result = awara::execute_renames(&file_strs, &config, &mut options);

    assert!(result.successful_ops.is_empty());
    assert!(cancelled.get());
    assert!(f.exists());
}

#[test]
fn test_cancel_mid_execution() {
    let dir = tempdir().unwrap();
    let count = 10;
    let mut files = Vec::new();
    for i in 0..count {
        let f = dir.path().join(format!("file_{}.txt", i));
        File::create(&f).unwrap();
        files.push(f);
    }

    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("pre_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let (token, signal) = cancel_token();
    let signal_clone = signal.clone();
    let first = AtomicBool::new(true);

    let file_strs: Vec<String> = files
        .iter()
        .map(|f| f.to_string_lossy().to_string())
        .collect();
    let cancelled = Rc::new(Cell::new(false));
    let cancelled_clone = Rc::clone(&cancelled);
    let mut options = awara::RenameOptions {
        dry_run: false,
        stop_on_error: true,
        preserve_timestamps: false,
        set_attributes: None,
        undo_file: None,
        cancel_token: Some(token),
        on_event: Some(Box::new(move |event| match event {
            awara::RenameEvent::Progress { .. } => {
                if first.swap(false, Ordering::SeqCst) {
                    signal_clone.store(true, Ordering::SeqCst);
                }
            }
            awara::RenameEvent::Cancelled { completed } => {
                cancelled_clone.set(true);
                assert_eq!(completed, 1);
            }
            _ => {}
        })),
    };
    let result = awara::execute_renames(&file_strs, &config, &mut options);

    assert!(cancelled.get());
    assert_eq!(result.successful_ops.len(), 1);
    let renamed = dir.path().join("pre_file_0.txt");
    assert!(renamed.exists());
    for i in 1..count {
        let original = dir.path().join(format!("file_{}.txt", i));
        assert!(original.exists());
    }
}

#[test]
fn test_cancel_writes_undo_file() {
    let dir = tempdir().unwrap();
    let count = 10;
    let mut files = Vec::new();
    for i in 0..count {
        let f = dir.path().join(format!("file_{}.txt", i));
        File::create(&f).unwrap();
        files.push(f);
    }

    let undo_path = dir.path().join("undo.json");

    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("pre_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let (token, signal) = cancel_token();
    let signal_clone = signal.clone();
    let first = AtomicBool::new(true);

    let file_strs: Vec<String> = files
        .iter()
        .map(|f| f.to_string_lossy().to_string())
        .collect();
    let cancelled = Rc::new(Cell::new(false));
    let cancelled_clone = Rc::clone(&cancelled);
    let mut options = awara::RenameOptions {
        dry_run: false,
        stop_on_error: true,
        preserve_timestamps: false,
        set_attributes: None,
        undo_file: Some(undo_path.to_str().unwrap()),
        cancel_token: Some(token),
        on_event: Some(Box::new(move |event| match event {
            awara::RenameEvent::Progress { .. } => {
                if first.swap(false, Ordering::SeqCst) {
                    signal_clone.store(true, Ordering::SeqCst);
                }
            }
            awara::RenameEvent::Cancelled { completed } => {
                cancelled_clone.set(true);
                assert_eq!(completed, 1);
            }
            _ => {}
        })),
    };
    let result = awara::execute_renames(&file_strs, &config, &mut options);

    assert!(cancelled.get());
    assert_eq!(result.successful_ops.len(), 1);

    assert!(undo_path.exists());
    let undo_data = std::fs::read_to_string(&undo_path).unwrap();
    let undo_ops: Vec<awara::RenameOp> = serde_json::from_str(&undo_data).unwrap();
    assert_eq!(undo_ops.len(), 1);
    assert_eq!(undo_ops[0].new_name, "pre_file_0.txt");
}

#[test]
fn test_rollback_completes_despite_cancel_signal() {
    // Verify that once stop_on_error triggers rollback, the undo loop
    // runs to completion even if the cancellation token is set mid-way.
    // The rollback loop intentionally does NOT check the token — aborting
    // a rollback mid-way would leave the filesystem in an inconsistent
    // half-undone state.
    let dir = tempdir().unwrap();

    // Create 3 files. file_0 and file_1 will be renamed via add_prefix.
    // file_2's rename will fail because we delete its source file right
    // before the rename attempt (from the on_event callback after the
    // second Progress fires, so file_2 has a source at calculation time
    // but not at rename time).
    for i in 0..3 {
        let f = dir.path().join(format!("file_{}.txt", i));
        File::create(&f).unwrap();
    }

    let file2_path = dir.path().join("file_2.txt");
    let progress_count = Rc::new(Cell::new(0u8));
    let progress_count_clone = Rc::clone(&progress_count);

    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some("pre_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let (token, signal) = cancel_token();

    let rollback_count = Rc::new(Cell::new(0usize));
    let rollback_count_clone = Rc::clone(&rollback_count);
    let cancelled_emitted = Rc::new(Cell::new(false));
    let cancelled_emitted_clone = Rc::clone(&cancelled_emitted);

    let file_strs: Vec<String> = (0..3)
        .map(|i| {
            dir.path()
                .join(format!("file_{}.txt", i))
                .to_string_lossy()
                .to_string()
        })
        .collect();

    let mut options = awara::RenameOptions {
        dry_run: false,
        stop_on_error: true,
        preserve_timestamps: false,
        set_attributes: None,
        undo_file: None,
        cancel_token: Some(token),
        on_event: Some(Box::new(move |event| {
            match event {
                awara::RenameEvent::Progress { .. } => {
                    let count = progress_count_clone.get() + 1;
                    progress_count_clone.set(count);
                    // After the second Progress (file_0 and file_1 done),
                    // delete file_2's source so its rename fails (ENOENT).
                    if count == 2 {
                        let _ = std::fs::remove_file(&file2_path);
                    }
                }
                awara::RenameEvent::RollbackProgress { .. } => {
                    // Fire cancellation signal DURING the rollback.
                    // The rollback loop does NOT check the token,
                    // so it should run to completion regardless.
                    signal.store(true, Ordering::SeqCst);
                    rollback_count_clone.set(rollback_count_clone.get() + 1);
                }
                awara::RenameEvent::Cancelled { .. } => {
                    cancelled_emitted_clone.set(true);
                }
                _ => {}
            }
        })),
    };
    let result = awara::execute_renames(&file_strs, &config, &mut options);

    // file_0 and file_1 were renamed (pre_file_0.txt and pre_file_1.txt)
    assert_eq!(result.successful_ops.len(), 2);
    // file_2 failed (source deleted before rename)
    assert!(!result.failed_ops.is_empty());

    // Rollback was triggered — it ran for all 2 successful ops
    assert_eq!(
        rollback_count.get(),
        2,
        "rollback should have undone both files despite cancel signal"
    );
    assert!(
        !cancelled_emitted.get(),
        "Cancelled event should NOT fire during rollback"
    );
    // After rollback, file_0 and file_1 are restored to original names.
    // file_2 was deleted to trigger the error, so it no longer exists.
    for i in 0..2 {
        let orig = dir.path().join(format!("file_{}.txt", i));
        assert!(orig.exists(), "file_{}.txt should exist after rollback", i);
        let renamed = dir.path().join(format!("pre_file_{}.txt", i));
        assert!(!renamed.exists(), "pre_file_{}.txt should not exist", i);
    }
    assert!(
        !dir.path().join("file_2.txt").exists(),
        "file_2.txt was deliberately deleted to trigger the error"
    );
}
