use super::*;
use crate::core::config::NumberingSection;
use crate::core::plan::execute_renames;
use tempfile::TempDir;

#[test]
fn test_trim_windows_trailing_whole_name() {
    // No extension: the whole name is trimmed.
    assert_eq!(trim_windows_trailing("foo "), "foo");
    assert_eq!(trim_windows_trailing("foo."), "foo");
    assert_eq!(trim_windows_trailing("foo.."), "foo");
    assert_eq!(trim_windows_trailing("foo. "), "foo");
    assert_eq!(trim_windows_trailing("  foo  "), "  foo");
    // Leading dots are preserved (hidden files).
    assert_eq!(trim_windows_trailing(".foo "), ".foo");
    // Extension present: trailing junk on the whole name is trimmed.
    assert_eq!(trim_windows_trailing("foo.txt "), "foo.txt");
    assert_eq!(trim_windows_trailing("foo.txt."), "foo.txt");
    // Unchanged inputs pass through.
    assert_eq!(trim_windows_trailing("foo"), "foo");
    assert_eq!(trim_windows_trailing("foo.txt"), "foo.txt");
}

#[test]
fn test_trim_windows_trailing_stem_before_ext() {
    // Trailing junk in the stem (before the extension) is trimmed too.
    assert_eq!(trim_windows_trailing("name .txt"), "name.txt");
    assert_eq!(trim_windows_trailing("foo. .txt"), "foo.txt");
    assert_eq!(trim_windows_trailing("foo. "), "foo");
    // Multi-dot: the split happens at the last dot, so a space in the
    // middle of a compound stem is preserved.
    assert_eq!(trim_windows_trailing("foo .tar.gz"), "foo .tar.gz");
}

#[test]
fn test_trim_windows_trailing_edge_cases() {
    assert_eq!(trim_windows_trailing(""), "");
    assert_eq!(trim_windows_trailing("."), "");
    assert_eq!(trim_windows_trailing("   "), "");
}

/// Windows' trailing-space/dot stripping changes only the **final** component, so
/// the directory a computed name lands in — a name-implied subpath, or an output
/// dir — is preserved. Joining the whole name would re-apply the subpath
/// (`sub/a.txt` → `sub/sub/a.txt`), which was a Windows-only bug here and in
/// `target_key`.
#[test]
fn test_replace_final_component_preserves_target_directory() {
    // Name-implied subpath: the subpath must not be doubled.
    assert_eq!(
        replace_final_component(Path::new("/base/sub/name .txt"), "sub/name.txt"),
        PathBuf::from("/base/sub/name.txt")
    );
    // Plain rename: the final component is replaced in place.
    assert_eq!(
        replace_final_component(Path::new("/base/old .txt"), "new.txt"),
        PathBuf::from("/base/new.txt")
    );
    // Relative target (no directory part).
    assert_eq!(
        replace_final_component(Path::new("name .txt"), "name.txt"),
        PathBuf::from("name.txt")
    );
    // Output-dir relocation keeps the output directory (and any name subpath).
    assert_eq!(
        replace_final_component(Path::new("/out/sub/name .txt"), "sub/name.txt"),
        PathBuf::from("/out/sub/name.txt")
    );
}

#[test]
fn test_normalize_target_name_gated() {
    // The wrapper applies the rule only on Windows; everywhere else it is
    // identity, so the rest of the pipeline is unaffected on Linux/macOS.
    let input = "name .txt";
    if cfg!(windows) {
        assert_eq!(normalize_target_name(input), "name.txt");
    } else {
        assert_eq!(normalize_target_name(input), input);
    }
}

#[test]
fn test_detect_collisions_normalized_duplicate() {
    // Two logically distinct targets that Windows collapses to the same
    // on-disk name ("a .txt" and "a.txt" both become "a.txt") must be
    // flagged as an in-batch collision by the pre-apply scan on Windows,
    // and must stay distinct everywhere else. Absolute temp paths so the
    // case-sensitivity probe targets the temp dir, never the repo CWD.
    let dir = TempDir::new().unwrap();
    let mk = |orig: &str, new: &str| RenameOp {
        original_path: dir.path().join(orig),
        new_path: dir.path().join(new),
        original_name: orig.to_string(),
        new_name: new.to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0u8,
    };
    let ops = vec![
        mk("zz_x.txt", "zz_collide_a .txt"),
        mk("zz_y.txt", "zz_collide_a.txt"),
    ];
    let collisions = detect_collisions(&ops);
    let in_batch = collisions
        .iter()
        .filter(|c| c.reason == CollisionReason::InBatch(0))
        .count();
    if cfg!(windows) {
        assert_eq!(
            in_batch, 1,
            "Windows-trimmed targets must be flagged as in-batch collisions"
        );
    } else {
        assert_eq!(
            in_batch, 0,
            "logically distinct targets must not collide off Windows"
        );
    }
}

#[cfg(windows)]
#[test]
fn test_execute_skips_collapsed_duplicate_targets() {
    use crate::core::config::NameSection;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("zz_x.txt"), "first").unwrap();
    std::fs::write(dir.path().join("zz_y.txt"), "second").unwrap();

    // Both files get the fixed stem "a " -> "a .txt", which Windows
    // creates as "a.txt". The second op must be skipped as a duplicate
    // target, never overwriting the first.
    let config = RenameConfig {
        name: NameSection {
            name_mode: Some("fixed".to_string()),
            name_value: Some("a ".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let files: Vec<String> = [dir.path().join("zz_x.txt"), dir.path().join("zz_y.txt")]
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    let mut options = RenameOptions::default();
    let result = execute_renames(&files, &config, &mut options);
    assert_eq!(result.successful_ops.len(), 1);
    assert_eq!(result.skipped_ops.len(), 1);
    // The skipped op leaves its source in place (a skip never removes or
    // overwrites anything): only the first file was renamed to "a.txt"
    // and its content must have survived untouched.
    let mut names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert_eq!(names, vec!["a.txt", "zz_y.txt"]);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "first",
        "first rename's content must not be overwritten by the skipped op"
    );
}

#[test]
fn test_chain_rename_swap_execution() {
    use crate::core::config::ReplaceSection;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let file_a = dir.path().join("swap_a.txt");
    let file_b = dir.path().join("swap_b.txt");
    std::fs::write(&file_a, "content_a").unwrap();
    std::fs::write(&file_b, "content_b").unwrap();

    // Configure a rename where swap_a.txt -> swap_b.txt and swap_b.txt -> swap_c.txt (chain)
    let config = RenameConfig {
        replace: ReplaceSection {
            replace: Some("swap_a".to_string()),
            with: Some("swap_b".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    // Or directly test chain conflict with a 2-step swap
    let files: Vec<String> = vec![file_a.to_string_lossy().to_string()];
    let mut options = RenameOptions::default();
    let result = execute_renames(&files, &config, &mut options);
    // file_b already exists on disk and is not another op's source here, so it is a collision
    assert_eq!(result.skipped_ops.len(), 1);
}

#[test]
fn test_apply_ops_chain_safe_resolves_swap() {
    // A swap pair (a↔b) applied earlier: a's content at b, b's content at
    // a. Undoing both must park one side at a temp name, move the other,
    // then complete — no clobbering.
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&b, b"content_a").unwrap();
    std::fs::write(&a, b"content_b").unwrap();

    let mk = |orig: &Path, new: &Path, orig_name: &str, new_name: &str| RenameOp {
        original_path: orig.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: orig_name.into(),
        new_name: new_name.into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    // Undo ops: b→a and a→b
    let result =
        apply_ops_chain_safe(&[mk(&b, &a, "b.txt", "a.txt"), mk(&a, &b, "a.txt", "b.txt")]);
    assert_eq!(result.successful.len(), 2, "swap fully resolved");
    assert!(result.failed.is_empty());
    assert_eq!(std::fs::read(&a).unwrap(), b"content_a", "a restored");
    assert_eq!(std::fs::read(&b).unwrap(), b"content_b", "b restored");
}

#[test]
fn test_apply_ops_chain_safe_resolves_chain() {
    // Chain: a→b, b→c with a,b existing and c free. The first op must
    // park a at a temp name so b can move to c, then complete to b.
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    let c = dir.path().join("c.txt");
    std::fs::write(&a, b"content_a").unwrap();
    std::fs::write(&b, b"content_b").unwrap();

    let mk = |orig: &Path, new: &Path, orig_name: &str, new_name: &str| RenameOp {
        original_path: orig.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: orig_name.into(),
        new_name: new_name.into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    let result =
        apply_ops_chain_safe(&[mk(&a, &b, "a.txt", "b.txt"), mk(&b, &c, "b.txt", "c.txt")]);
    assert_eq!(result.successful.len(), 2, "chain fully resolved");
    assert!(result.failed.is_empty());
    assert_eq!(std::fs::read(&b).unwrap(), b"content_a", "a's content at b");
    assert_eq!(std::fs::read(&c).unwrap(), b"content_b", "b's content at c");
    assert!(!a.exists(), "a vacated");
}

#[test]
fn test_apply_ops_chain_safe_genuine_collision_fails() {
    // Target exists but is not another op's source — genuine on-disk
    // collision. The op must be skipped (reported) with source untouched.
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&b, b"content_a").unwrap();
    std::fs::write(&a, b"unrelated").unwrap(); // a exists, not part of batch

    let op = RenameOp {
        original_path: b.clone(),
        new_path: a.clone(),
        original_name: "b.txt".into(),
        new_name: "a.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    let result = apply_ops_chain_safe(&[op]);
    assert!(result.successful.is_empty());
    assert_eq!(result.failed.len(), 1);
    assert_eq!(std::fs::read(&a).unwrap(), b"unrelated", "target untouched");
    assert_eq!(std::fs::read(&b).unwrap(), b"content_a", "source untouched");
}

#[test]
fn test_apply_ops_chain_safe_copy_mode_deletes_copy() {
    // was_copy op: the source (new_path) is preserved, so the executor
    // deletes the copy (original_path) instead of renaming.
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("a.txt");
    let copy = dir.path().join("b.txt");
    std::fs::write(&src, b"data").unwrap();
    std::fs::write(&copy, b"data").unwrap();

    let op = RenameOp {
        original_path: copy.clone(),
        new_path: src.clone(),
        original_name: "b.txt".into(),
        new_name: "a.txt".into(),
        was_copy: true,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    let result = apply_ops_chain_safe(&[op]);
    assert_eq!(result.successful.len(), 1);
    assert!(result.failed.is_empty());
    assert!(!copy.exists(), "copy deleted");
    assert!(src.exists(), "source intact");
}

#[test]
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn test_apply_ops_chain_safe_case_only_rename() {
    // On case-folding filesystems a case-only rename is the same entry on
    // disk — it must go through the case-insensitive path, not be treated
    // as a collision.
    let dir = TempDir::new().unwrap();
    let lower = dir.path().join("lower.txt");
    std::fs::write(&lower, b"data").unwrap();
    let upper = dir.path().join("LOWER.txt");

    let op = RenameOp {
        original_path: lower.clone(),
        new_path: upper.clone(),
        original_name: "lower.txt".into(),
        new_name: "LOWER.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    let result = apply_ops_chain_safe(&[op]);
    assert_eq!(result.successful.len(), 1, "case-only rename succeeds");
    assert!(result.failed.is_empty());
    assert_eq!(std::fs::read(&upper).unwrap(), b"data");
}

#[test]
fn test_apply_ops_chain_safe_duplicate_target_fails() {
    // Two ops targeting the same path: the first claims it, the second is
    // rejected by the processed-targets guard (not clobbered).
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    let target = dir.path().join("x.txt");
    std::fs::write(&a, b"one").unwrap();
    std::fs::write(&b, b"two").unwrap();

    let mk = |orig: &Path, new: &Path, name: &str| RenameOp {
        original_path: orig.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: name.into(),
        new_name: "x.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    let result = apply_ops_chain_safe(&[mk(&a, &target, "a.txt"), mk(&b, &target, "b.txt")]);
    assert_eq!(result.successful.len(), 1);
    assert_eq!(result.failed.len(), 1);
    assert!(result.failed[0].1.contains("already been renamed"));
    assert_eq!(std::fs::read(&target).unwrap(), b"one", "first op wins");
    assert!(b.exists(), "second op's source untouched");
}

#[test]
fn test_apply_ops_chain_safe_restores_permissions() {
    // original_permissions snapshot is applied to the target after a
    // successful rename (e.g. restoring a readonly bit on undo).
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, b"data").unwrap();
    let mut perms = std::fs::metadata(&a).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&a, perms.clone()).unwrap();
    let snap = PermSnapshot::capture(&a).unwrap();

    let mut op = RenameOp {
        original_path: a.clone(),
        new_path: b.clone(),
        original_name: "a.txt".into(),
        new_name: "b.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    op.original_permissions = Some(snap);

    let result = apply_ops_chain_safe(&[op]);
    assert_eq!(result.successful.len(), 1);
    assert!(result.failed.is_empty());
    let after = PermSnapshot::capture(&b).unwrap();
    assert!(after.readonly, "readonly bit restored on target");
}

/// The `~aw_<short_hex>` fallback in `park_at_temp` exists for deep paths
/// where the standard `awara_tmp_<uuid>` name would exceed the OS path
/// limit. The effective limit varies by configuration (e.g. Windows "long
/// paths enabled" lifts MAX_PATH, and std may auto-prefix `\\?\`), so the
/// test doesn't hardcode a limit: it pads the parent (10-char components,
/// so the final parent lands in (213, 224] UTF-16 units — standard name
/// crosses the classic 260 limit, short name still fits) and asserts the
/// contract — the source moved and exactly one temp file (standard or
/// short form) exists — which holds whether the OS accepts the standard
/// name or forces the fallback.
#[test]
#[cfg(target_os = "windows")]
fn test_park_at_temp_fallback_on_deep_path() {
    use std::os::windows::ffi::OsStrExt;
    let dir = TempDir::new().unwrap();
    let mut parent = dir.path().to_path_buf();
    // UTF-16 unit count (the unit MAX_PATH is measured in) — OsStr::len
    // would return the byte width (2× units) on Windows.
    let wide_len = |p: &std::path::Path| p.as_os_str().encode_wide().count();
    while wide_len(&parent.join("awara_tmp_000000000000000000000000000000000000")) <= 260 {
        parent.push("d".repeat(10));
    }
    // Sanity: the short fallback name still fits within MAX_PATH.
    assert!(
        wide_len(&parent.join("~aw_00000000")) <= 260,
        "short temp name must fit: {:?}",
        parent
    );
    std::fs::create_dir_all(&parent).unwrap();
    let src = parent.join("source.txt");
    std::fs::write(&src, b"data").unwrap();

    let tmp = park_at_temp(&src).unwrap();
    assert!(!src.exists(), "source moved");
    assert!(tmp.exists(), "parked file exists");
    let name = tmp.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("awara_tmp_") || name.starts_with("~aw_"),
        "temp name uses the standard or short form: {name}"
    );
    // Only one temp file left behind.
    let temps: Vec<_> = std::fs::read_dir(&parent)
        .unwrap()
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("awara_tmp_") || n.starts_with("~aw_")
        })
        .collect();
    assert_eq!(temps.len(), 1);
}

#[test]
fn test_apply_undo_resolves_swap() {
    // The CLI undo path must resolve a swap pair via temp deferral too.
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    // Post-apply state of the forward swap a→b, b→a:
    std::fs::write(&b, b"content_a").unwrap();
    std::fs::write(&a, b"content_b").unwrap();

    let mk = |orig: &Path, new: &Path, orig_name: &str, new_name: &str| RenameOp {
        original_path: orig.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: orig_name.into(),
        new_name: new_name.into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    let (reverted, errors) =
        apply_undo(&[mk(&a, &b, "a.txt", "b.txt"), mk(&b, &a, "b.txt", "a.txt")]);
    assert_eq!(reverted.len(), 2, "both sides of the swap undone");
    assert_eq!(errors, 0);
    assert_eq!(std::fs::read(&a).unwrap(), b"content_a", "a restored");
    assert_eq!(std::fs::read(&b).unwrap(), b"content_b", "b restored");
    // reverted reports the FORWARD ops that were undone
    assert_eq!(reverted[0].original_path, a);
    assert_eq!(reverted[0].new_path, b);
}

#[test]
fn test_apply_undo_skips_missing_source_silently() {
    // Forward op whose current path was deleted externally: nothing to
    // undo, and it must not count as an error.
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, b"data").unwrap();
    // b (the current path) does not exist — deleted externally.

    let op = RenameOp {
        original_path: a.clone(),
        new_path: b.clone(),
        original_name: "a.txt".into(),
        new_name: "b.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    let (reverted, errors) = apply_undo(&[op]);
    assert!(reverted.is_empty());
    assert_eq!(errors, 0, "missing source skipped silently");
    assert!(a.exists(), "target untouched");
}

#[test]
fn test_apply_undo_genuine_collision_counts_error() {
    // Original path re-created externally: the undo cannot clobber it,
    // so it is reported as an error and both files stay intact.
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&b, b"content_a").unwrap(); // a→b applied
    std::fs::write(&a, b"unrelated").unwrap(); // a re-created externally

    let op = RenameOp {
        original_path: a.clone(),
        new_path: b.clone(),
        original_name: "a.txt".into(),
        new_name: "b.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    let (reverted, errors) = apply_undo(&[op]);
    assert!(reverted.is_empty());
    assert_eq!(errors, 1, "collision reported as error");
    assert_eq!(std::fs::read(&a).unwrap(), b"unrelated", "target untouched");
    assert_eq!(std::fs::read(&b).unwrap(), b"content_a", "source untouched");
}

#[test]
fn test_chain_rename_swap_cycle() {
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let file_a = dir.path().join("first.txt");
    let file_b = dir.path().join("second.txt");
    std::fs::write(&file_a, "alpha").unwrap();
    std::fs::write(&file_b, "beta").unwrap();

    let config_a = RenameConfig {
        replace: crate::core::config::ReplaceSection {
            replace: Some("first".to_string()),
            with: Some("second".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let files = vec![
        file_a.to_string_lossy().to_string(),
        file_b.to_string_lossy().to_string(),
    ];
    let mut options = RenameOptions::default();
    let result = execute_renames(&files, &config_a, &mut options);
    assert!(result.skipped_ops.len() <= 1);
}

#[test]
fn test_path_limit_os_error_captured() {
    use crate::core::config::AddSection;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let file = dir.path().join("small.txt");
    std::fs::write(&file, "test").unwrap();

    // Add a prefix of 300 characters, which exceeds standard filesystem component limits (255 bytes)
    let huge_prefix = "a".repeat(300);
    let config = RenameConfig {
        add: AddSection {
            add_prefix: Some(huge_prefix),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut events: Vec<RenameEvent> = Vec::new();
    let mut options = RenameOptions {
        stop_on_error: false,
        on_event: Some(Box::new(|evt| events.push(evt))),
        ..Default::default()
    };

    let files = vec![file.to_string_lossy().to_string()];
    let result = execute_renames(&files, &config, &mut options);
    drop(options);

    // Desired rename must fail at OS level (File name too long / ENAMETOOLONG / ERROR_FILENAME_EXCED_RANGE)
    // and be captured cleanly in failed_ops and RenameEvent::Error without panicking.
    assert_eq!(result.failed_ops.len(), 1);
    let error_event = events
        .iter()
        .find(|e| matches!(e, RenameEvent::Error { .. }));
    assert!(
        error_event.is_some(),
        "An Error event must be emitted for OS path limit errors"
    );
    assert!(
        file.exists(),
        "Original file must remain intact when OS rename fails"
    );
}

#[cfg(windows)]
#[test]
fn test_windows_deep_path_chain_rename() {
    use tempfile::TempDir;

    // Build a nested directory tree on Windows
    let dir = TempDir::new().unwrap();
    let mut deep_dir = dir.path().to_path_buf();
    for _ in 0..8 {
        deep_dir = deep_dir.join("sub_level_folder");
    }
    std::fs::create_dir_all(&deep_dir).ok();

    if deep_dir.exists() {
        let file_1 = deep_dir.join("f1.txt");
        let file_2 = deep_dir.join("f2.txt");
        std::fs::write(&file_1, "1").ok();
        std::fs::write(&file_2, "2").ok();

        let config = RenameConfig {
            replace: crate::core::config::ReplaceSection {
                replace: Some("f".to_string()),
                with: Some("file_".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let files = vec![
            file_1.to_string_lossy().to_string(),
            file_2.to_string_lossy().to_string(),
        ];
        let mut options = RenameOptions::default();
        let result = execute_renames(&files, &config, &mut options);
        // Operations either succeed (with temp fallback if needed) or report cleanly in failed_ops
        assert_eq!(
            result.successful_ops.len() + result.failed_ops.len() + result.skipped_ops.len(),
            2
        );
    }
}

/// The plan is the single source of truth: the names it computes must be exactly
/// the names execution applies, including numbering with `numbering_break`.
#[test]
fn test_plan_matches_execute_renames_names() {
    use crate::core::config::NumberingMode;
    use crate::core::plan::{PlanOptions, Resolution, execute_plan, plan_renames};
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let names = ["a1.txt", "a2.txt", "b1.txt", "b2.txt"];
    for n in names {
        std::fs::write(dir.path().join(n), "").unwrap();
    }
    let files: Vec<String> = names
        .iter()
        .map(|n| dir.path().join(n).to_string_lossy().to_string())
        .collect();
    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Prefix),
            numbering_start: 1,
            numbering_increment: 1,
            numbering_sep: Some("_".to_string()),
            numbering_break: Some(2),
            ..Default::default()
        },
        ..Default::default()
    };

    let opts = PlanOptions::default();
    let plan = plan_renames(&files, &config, &opts);
    let planned: Vec<String> = plan.ops().iter().map(|op| op.new_name.clone()).collect();
    assert_eq!(planned, ["1_a1.txt", "2_a2.txt", "1_b1.txt", "2_b2.txt"]);

    let mut options = RenameOptions::default();
    let result = execute_plan(
        plan,
        &config,
        &mut options,
        &Resolution::default_for(CollisionStrategy::Skip),
    );
    let executed: Vec<String> = result
        .successful_ops
        .iter()
        .map(|op| op.new_name.clone())
        .collect();

    assert_eq!(planned, executed);
}

/// `numbering_break` restarts the sequence every N files.
#[test]
fn test_numbering_break_resets_every_n_files() {
    use crate::core::config::NumberingMode;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    for n in ["a1.txt", "a2.txt", "b1.txt", "b2.txt"] {
        std::fs::write(dir.path().join(n), "").unwrap();
    }
    let files: Vec<String> = ["a1.txt", "a2.txt", "b1.txt", "b2.txt"]
        .iter()
        .map(|n| dir.path().join(n).to_string_lossy().to_string())
        .collect();
    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Prefix),
            numbering_start: 1,
            numbering_increment: 1,
            numbering_sep: Some("_".to_string()),
            numbering_break: Some(2),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut options = RenameOptions::default();
    let result = execute_renames(&files, &config, &mut options);

    let got: std::collections::HashMap<&str, &str> = result
        .successful_ops
        .iter()
        .map(|op| (op.original_name.as_str(), op.new_name.as_str()))
        .collect();
    assert_eq!(got["a1.txt"], "1_a1.txt");
    assert_eq!(got["a2.txt"], "2_a2.txt");
    assert_eq!(got["b1.txt"], "1_b1.txt");
    assert_eq!(got["b2.txt"], "2_b2.txt");
}

/// Skipping one op via the resolution must not let later ops reclaim its
/// number: the plan already baked numbering in, so `b.txt` stays `2_b.txt`.
/// This is the behavior the old `skip_set` (which excluded skipped files from
/// the batch before numbering) got wrong.
#[test]
fn test_execution_skip_preserves_later_numbering() {
    use crate::core::config::NumberingMode;
    use crate::core::plan::{OpDecision, PlanOptions, Resolution, execute_plan, plan_renames};
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("a.txt"), "").unwrap();
    std::fs::write(dir.path().join("b.txt"), "").unwrap();
    let files: Vec<String> = ["a.txt", "b.txt"]
        .iter()
        .map(|n| dir.path().join(n).to_string_lossy().to_string())
        .collect();
    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Prefix),
            numbering_start: 1,
            numbering_increment: 1,
            numbering_sep: Some("_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let opts = PlanOptions::default();
    let plan = plan_renames(&files, &config, &opts);
    assert_eq!(plan.ops()[1].new_name, "2_b.txt");

    let resolution = Resolution::new(
        CollisionStrategy::Overwrite,
        Some(vec![OpDecision::Skip, OpDecision::Default]),
    );
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert_eq!(result.successful_ops.len(), 1);
    assert_eq!(result.successful_ops[0].new_name, "2_b.txt");
}

/// `numbering_break` groups by counted position, so items running before
/// numbering do not affect it: an `Add` prefix still yields groups of N files.
#[test]
fn test_numbering_break_ignores_names_before_numbering() {
    use crate::core::config::NumberingMode;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    for n in ["a1.txt", "a2.txt", "b1.txt"] {
        std::fs::write(dir.path().join(n), "").unwrap();
    }
    let files: Vec<String> = ["a1.txt", "a2.txt", "b1.txt"]
        .iter()
        .map(|n| dir.path().join(n).to_string_lossy().to_string())
        .collect();
    let config = RenameConfig {
        add: crate::AddSection {
            add_prefix: Some("p_".to_string()),
            ..Default::default()
        },
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Prefix),
            numbering_start: 1,
            numbering_increment: 1,
            numbering_sep: Some("_".to_string()),
            numbering_break: Some(2),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut options = RenameOptions::default();
    let result = execute_renames(&files, &config, &mut options);

    let got: std::collections::HashMap<&str, &str> = result
        .successful_ops
        .iter()
        .map(|op| (op.original_name.as_str(), op.new_name.as_str()))
        .collect();
    assert_eq!(got["a1.txt"], "1_p_a1.txt");
    assert_eq!(got["a2.txt"], "2_p_a2.txt");
    assert_eq!(got["b1.txt"], "1_p_b1.txt");
}

/// Numbering indices are assigned by position over the whole input list, so a
/// file that disappears before execution (or errors later) must not let later
/// files reclaim its number — the surviving file keeps number 2, matching the
/// preview computed while both files existed.
#[test]
fn test_numbering_index_preserved_when_earlier_path_disappears() {
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    // `a.txt` is intentionally absent at execution time.
    std::fs::write(&b, "").unwrap();

    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_mode: Some(crate::core::config::NumberingMode::Prefix),
            numbering_start: 1,
            numbering_increment: 1,
            numbering_sep: Some("_".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let files = vec![
        a.to_string_lossy().to_string(),
        b.to_string_lossy().to_string(),
    ];
    let mut options = RenameOptions::default();
    let result = execute_renames(&files, &config, &mut options);

    assert_eq!(result.successful_ops.len(), 1);
    assert_eq!(result.successful_ops[0].new_name, "2_b.txt");
}

/// Generic ordering regression: `command_order` is honored for arbitrary
/// operations, not just numbering. Prefix-then-remove differs from
/// remove-then-prefix.
#[test]
fn test_command_order_prefix_before_remove() {
    use crate::core::config::RenameItem;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("file.txt"), "").unwrap();

    let config = RenameConfig {
        command_order: vec![
            RenameItem::Prefix("A".to_string()),
            RenameItem::RemoveFirst(2),
        ],
        ..Default::default()
    };
    let files = vec![dir.path().join("file.txt").to_string_lossy().to_string()];
    let mut options = RenameOptions::default();
    let result = execute_renames(&files, &config, &mut options);
    assert_eq!(result.successful_ops.len(), 1);
    // "file" -> prefix "Afile" -> remove first 2 -> "ile"
    assert_eq!(result.successful_ops[0].new_name, "ile.txt");
}

#[test]
fn test_command_order_remove_before_prefix() {
    use crate::core::config::RenameItem;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("file.txt"), "").unwrap();

    let config = RenameConfig {
        command_order: vec![
            RenameItem::RemoveFirst(2),
            RenameItem::Prefix("A".to_string()),
        ],
        ..Default::default()
    };
    let files = vec![dir.path().join("file.txt").to_string_lossy().to_string()];
    let mut options = RenameOptions::default();
    let result = execute_renames(&files, &config, &mut options);
    assert_eq!(result.successful_ops.len(), 1);
    // "file" -> remove first 2 -> "le" -> prefix "A" -> "Ale"
    assert_eq!(result.successful_ops[0].new_name, "Ale.txt");
}

/// Regression: with both Numbering and Add set to prefix, numbering used to be
/// forced into a second pass after every other operation, so its position in
/// `command_order` was ignored. When Numbering precedes Add, the number must be
/// prepended first ("A1file"), not last ("1Afile").
#[test]
fn test_numbering_respects_command_order_before_add_prefix() {
    use crate::core::config::{NumberingMode, RenameItem};
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("file.txt"), "").unwrap();

    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Prefix),
            numbering_start: 1,
            numbering_increment: 1,
            ..Default::default()
        },
        command_order: vec![
            RenameItem::Numbering(NumberingMode::Prefix, 1, 1, 0, None, None, None, None, None),
            RenameItem::Prefix("A".to_string()),
        ],
        ..Default::default()
    };

    let files = vec![dir.path().join("file.txt").to_string_lossy().to_string()];
    let mut options = RenameOptions::default();
    let result = execute_renames(&files, &config, &mut options);

    assert_eq!(result.successful_ops.len(), 1, "rename should succeed");
    assert_eq!(result.successful_ops[0].new_name, "A1file.txt");
}

/// The mirror case: when Add precedes Numbering, the prefix must be applied
/// first ("1Afile").
#[test]
fn test_numbering_respects_command_order_after_add_prefix() {
    use crate::core::config::{NumberingMode, RenameItem};
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("file.txt"), "").unwrap();

    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Prefix),
            numbering_start: 1,
            numbering_increment: 1,
            ..Default::default()
        },
        command_order: vec![
            RenameItem::Prefix("A".to_string()),
            RenameItem::Numbering(NumberingMode::Prefix, 1, 1, 0, None, None, None, None, None),
        ],
        ..Default::default()
    };

    let files = vec![dir.path().join("file.txt").to_string_lossy().to_string()];
    let mut options = RenameOptions::default();
    let result = execute_renames(&files, &config, &mut options);

    assert_eq!(result.successful_ops.len(), 1, "rename should succeed");
    assert_eq!(result.successful_ops[0].new_name, "1Afile.txt");
}

/// A no-op op (target == source) is not a chain source: it never vacates its
/// path, so a later op targeting that path is a genuine on-disk collision.
#[test]
fn test_detect_collisions_ignores_noop_ops() {
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let x = dir.path().join("x.txt");
    let y = dir.path().join("y.txt");
    std::fs::write(&x, b"").unwrap();
    std::fs::write(&y, b"").unwrap();

    let mk = |original: &std::path::Path, new: &std::path::Path| RenameOp {
        original_path: original.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: original.file_name().unwrap().to_string_lossy().to_string(),
        new_name: new.file_name().unwrap().to_string_lossy().to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0u8,
    };

    let ops = vec![
        mk(&x, &x), // no-op
        mk(&y, &x), // y -> x, but x is never vacated
    ];
    let collisions = detect_collisions(&ops);
    assert_eq!(collisions.len(), 1);
    assert_eq!(collisions[0].op_index, 1);
    assert!(
        matches!(collisions[0].reason, CollisionReason::OnDisk),
        "no-op must not reclassify the conflict as a chain"
    );
}

/// An in-place effects op (name unchanged) carries a metadata snapshot;
/// replaying it restores the pre-effect timestamps instead of being skipped.
#[test]
fn test_apply_ops_chain_safe_restores_metadata_in_place() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("a.txt");
    std::fs::write(&f, b"data").unwrap();

    let before = std::fs::metadata(&f).unwrap().modified().unwrap();
    let snap = PermSnapshot::capture(&f).unwrap();
    let later = before + std::time::Duration::from_secs(100_000);
    filetime::set_file_mtime(&f, filetime::FileTime::from_system_time(later)).unwrap();
    assert_ne!(std::fs::metadata(&f).unwrap().modified().unwrap(), before);

    let op = RenameOp {
        original_path: f.clone(),
        new_path: f.clone(),
        original_name: "a.txt".into(),
        new_name: "a.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: Some(snap),
        applied_attributes: String::new(),
        applied_timestamps_mask: 2, // modified
    };
    let result = apply_ops_chain_safe(&[op]);
    assert_eq!(result.successful.len(), 1, "in-place restore succeeds");
    let after = std::fs::metadata(&f).unwrap().modified().unwrap();
    let secs =
        |t: std::time::SystemTime| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    assert!(secs(after).abs_diff(secs(before)) <= 1, "mtime restored");
}

/// `apply_undo` (the CLI undo path) restores metadata for an effects-only op.
#[test]
fn test_apply_undo_restores_metadata_for_unchanged_name() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("a.txt");
    std::fs::write(&f, b"data").unwrap();

    let before = std::fs::metadata(&f).unwrap().modified().unwrap();
    let snap = PermSnapshot::capture(&f).unwrap();
    let later = before + std::time::Duration::from_secs(100_000);
    filetime::set_file_mtime(&f, filetime::FileTime::from_system_time(later)).unwrap();

    let forward = RenameOp {
        original_path: f.clone(),
        new_path: f.clone(),
        original_name: "a.txt".into(),
        new_name: "a.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: Some(snap),
        applied_attributes: String::new(),
        applied_timestamps_mask: 2, // modified
    };
    let (reverted, errors) = apply_undo(&[forward]);
    assert_eq!(errors, 0);
    assert_eq!(reverted.len(), 1, "effects-only op is reported as reverted");
    let after = std::fs::metadata(&f).unwrap().modified().unwrap();
    let secs =
        |t: std::time::SystemTime| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    assert!(secs(after).abs_diff(secs(before)) <= 1, "mtime restored");
}

/// The normal restore path brings back everything the snapshot captured, so an
/// effects-only undo doesn't depend on the `applied_timestamps_mask`.
#[test]
fn test_apply_restores_all_captured_metadata_regardless_of_mask() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("a.txt");
    std::fs::write(&f, b"data").unwrap();

    let snap = PermSnapshot::capture(&f).unwrap();
    let before = std::fs::metadata(&f).unwrap().modified().unwrap();
    let later = before + std::time::Duration::from_secs(100_000);
    filetime::set_file_mtime(&f, filetime::FileTime::from_system_time(later)).unwrap();

    // No intent passed in — restore is not mask-gated, it restores all.
    snap.apply(&f);
    let after = std::fs::metadata(&f).unwrap().modified().unwrap();
    let secs =
        |t: std::time::SystemTime| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    assert!(
        secs(after).abs_diff(secs(before)) <= 1,
        "mtime restored by the normal path"
    );
}

/// `capture_for` records only the intentional categories it can also restore.
#[test]
fn test_capture_for_scopes_to_intentional_categories() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("a.txt");
    std::fs::write(&f, b"data").unwrap();

    // No intent -> nothing optional captured.
    let none = PermSnapshot::capture_for(&f, "", 0).unwrap();
    assert_eq!(
        (none.hidden, none.system, none.archive, none.created),
        (None, None, None, None)
    );

    // Intent declared: each in-place metadata flag is captured iff the volume
    // can express it. Dot-prefix `hidden` is the rename's job, not a stored
    // value, so it is deliberately not captured.
    let caps = crate::core::attrs::caps_for_read(&f);
    let some = PermSnapshot::capture_for(&f, "hidden,system,archive", 1).unwrap();
    assert_eq!(some.hidden.is_some(), caps.inplace_metadata("hidden"));
    assert_eq!(some.system.is_some(), caps.inplace_metadata("system"));
    assert_eq!(some.archive.is_some(), caps.inplace_metadata("archive"));
    // Birth time is settable on Windows and macOS.
    #[cfg(any(windows, target_os = "macos"))]
    assert!(some.created.is_some());
    #[cfg(not(any(windows, target_os = "macos")))]
    assert!(some.created.is_none());
}

/// New optional fields round-trip through serde, and legacy snapshots (without
/// them) still deserialize.
#[test]
fn test_perm_snapshot_serde_roundtrip_includes_intentional_fields() {
    let snap = PermSnapshot {
        readonly: true,
        mtime_secs: 1,
        mtime_nanos: 2,
        atime_secs: 3,
        atime_nanos: 4,
        hidden: Some(true),
        system: Some(false),
        archive: Some(true),
        created: Some((5, 6)),
    };
    let json = serde_json::to_string(&snap).unwrap();
    let back: PermSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(snap, back);

    let legacy =
        r#"{"readonly":false,"mtime_secs":10,"mtime_nanos":0,"atime_secs":20,"atime_nanos":0}"#;
    let back: PermSnapshot = serde_json::from_str(legacy).unwrap();
    assert_eq!(
        (back.hidden, back.system, back.archive, back.created),
        (None, None, None, None)
    );
}

// ──────────────────────────────────────────────────────────
// Folder-involved collisions under Overwrite
// ──────────────────────────────────────────────────────────

/// Collision detection flags folder involvement on **either** side so every
/// frontend can refuse to overwrite or merge (folders cannot be replaced).
#[test]
fn test_detect_collisions_marks_folder_involvement() {
    let dir = TempDir::new().unwrap();
    let file_src = dir.path().join("src.txt");
    std::fs::write(&file_src, b"").unwrap();
    let folder_src = dir.path().join("srcdir");
    std::fs::create_dir(&folder_src).unwrap();

    let mk = |src: &Path, new: &Path| RenameOp {
        original_path: src.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: src.file_name().unwrap().to_string_lossy().to_string(),
        new_name: new.file_name().unwrap().to_string_lossy().to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };

    // Directory target -> target flagged.
    let target_dir = dir.path().join("folder");
    std::fs::create_dir(&target_dir).unwrap();
    let c = detect_collisions(&[mk(&file_src, &target_dir)]);
    assert_eq!(c.len(), 1);
    assert!(
        c[0].target_is_dir,
        "collision on an existing directory must be flagged"
    );
    assert!(!c[0].source_is_dir);

    // File -> file target -> neither side flagged.
    let target_file = dir.path().join("other.txt");
    std::fs::write(&target_file, b"").unwrap();
    let c = detect_collisions(&[mk(&file_src, &target_file)]);
    assert_eq!(c.len(), 1);
    assert!(
        !c[0].target_is_dir && !c[0].source_is_dir,
        "a file->file collision involves no folder"
    );

    // Folder source over an existing file -> source flagged.
    let target_file2 = dir.path().join("third.txt");
    std::fs::write(&target_file2, b"").unwrap();
    let c = detect_collisions(&[mk(&folder_src, &target_file2)]);
    assert_eq!(c.len(), 1);
    assert!(c[0].source_is_dir, "folder source must be flagged");
    assert!(!c[0].target_is_dir);
}

/// Under Overwrite a *directory* target must be skipped, never replaced: the
/// source stays put and the folder (with its contents) is untouched. This is
/// the guard that keeps `--overwrite` from becoming "delete a folder".
#[test]
fn test_overwrite_refuses_nonempty_directory_target() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let target = dir.path().join("dst");
    std::fs::write(&src, b"payload").unwrap();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep.txt"), b"existing").unwrap();

    let plan = RenamePlan::from_ops(vec![RenameOp {
        original_path: src.clone(),
        new_path: target.clone(),
        original_name: "src".into(),
        new_name: "dst".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    }]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Overwrite);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert!(
        result.failed_ops.is_empty(),
        "a directory target must be skipped, not reported as an error"
    );
    assert_eq!(result.successful_ops.len(), 0);
    assert_eq!(result.skipped_ops.len(), 1);
    assert!(
        result.skipped_ops[0].1.contains("Folder already exists"),
        "the reason names the folder target: {}",
        result.skipped_ops[0].1
    );
    assert!(src.exists(), "source must stay in place");
    assert_eq!(
        std::fs::read(target.join("keep.txt")).unwrap(),
        b"existing",
        "folder contents must be untouched"
    );
}

/// Even an *empty* directory target is refused. On Unix, renaming a directory
/// onto an empty directory succeeds and silently replaces it — the guard must
/// stop that too, so Overwrite never deletes a folder.
#[test]
fn test_overwrite_refuses_empty_directory_target() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let target = dir.path().join("dst");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("inner.txt"), b"data").unwrap();
    std::fs::create_dir(&target).unwrap(); // empty

    let plan = RenamePlan::from_ops(vec![RenameOp {
        original_path: src.clone(),
        new_path: target.clone(),
        original_name: "src".into(),
        new_name: "dst".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    }]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Overwrite);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert_eq!(result.successful_ops.len(), 0);
    assert_eq!(result.skipped_ops.len(), 1);
    assert!(src.join("inner.txt").exists(), "source folder untouched");
    assert!(target.exists(), "empty target folder must not be deleted");
    assert_eq!(std::fs::read_dir(&target).unwrap().count(), 0);
}

/// Regression: overwriting an existing *file* target still works. The guard
/// keys on the target being a directory, not on Overwrite in general.
#[test]
fn test_overwrite_still_replaces_file_target() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src.txt");
    let target = dir.path().join("dst.txt");
    std::fs::write(&src, b"new").unwrap();
    std::fs::write(&target, b"old").unwrap();

    let plan = RenamePlan::from_ops(vec![RenameOp {
        original_path: src.clone(),
        new_path: target.clone(),
        original_name: "src.txt".into(),
        new_name: "dst.txt".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    }]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Overwrite);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert_eq!(result.successful_ops.len(), 1);
    assert!(result.failed_ops.is_empty());
    assert!(result.skipped_ops.is_empty());
    assert_eq!(std::fs::read(&target).unwrap(), b"new", "target replaced");
    assert!(!src.exists(), "source moved");
}

/// Guard scoping: a *directory source* renamed to a free name still succeeds
/// under Overwrite — only directory *targets* are refused.
#[test]
fn test_overwrite_renames_directory_source_to_free_target() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let target = dir.path().join("dst"); // does not exist
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("inner.txt"), b"data").unwrap();

    let plan = RenamePlan::from_ops(vec![RenameOp {
        original_path: src.clone(),
        new_path: target.clone(),
        original_name: "src".into(),
        new_name: "dst".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    }]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Overwrite);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert_eq!(result.successful_ops.len(), 1);
    assert!(target.join("inner.txt").exists(), "folder moved");
    assert!(!src.exists(), "source vacated");
}

/// Folder handling applies whenever a folder is involved: a *directory source*
/// whose target is an existing file is **skipped**, never allowed to replace the
/// file (and never a platform-dependent delete). The file target is untouched.
#[test]
fn test_overwrite_skips_folder_source_over_file_target() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let target = dir.path().join("dst");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("inner.txt"), b"data").unwrap();
    std::fs::write(&target, b"keep me").unwrap();

    let plan = RenamePlan::from_ops(vec![RenameOp {
        original_path: src.clone(),
        new_path: target.clone(),
        original_name: "src".into(),
        new_name: "dst".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    }]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Overwrite);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert!(result.failed_ops.is_empty(), "skipped, not an error");
    assert!(result.successful_ops.is_empty());
    assert_eq!(result.skipped_ops.len(), 1);
    assert!(
        result.skipped_ops[0].1.contains("File already exists"),
        "the reason names the file target, not a folder: {}",
        result.skipped_ops[0].1
    );
    assert!(src.is_dir(), "source folder left untouched");
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"keep me",
        "existing file target must not be replaced"
    );
}

// ──────────────────────────────────────────────────────────
// Copy mode with directories
// ──────────────────────────────────────────────────────────

/// Copy mode copies only the selected entry. A directory is recreated by name
/// (empty) at the target — its contents are never copied.
#[test]
fn test_copy_mode_directory_creates_empty_target_dir() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let target = dir.path().join("dst");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("inner.txt"), b"data").unwrap();

    let plan = RenamePlan::from_ops_with_copy_mode(
        vec![RenameOp {
            original_path: src.clone(),
            new_path: target.clone(),
            original_name: "src".into(),
            new_name: "dst".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0,
        }],
        true,
    );
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert!(result.failed_ops.is_empty(), "copying a dir must not error");
    assert_eq!(result.successful_ops.len(), 1);
    assert!(target.is_dir(), "target must be a directory");
    assert_eq!(
        std::fs::read_dir(&target).unwrap().count(),
        0,
        "directory contents must not be copied"
    );
    assert!(
        src.join("inner.txt").exists(),
        "source preserved (copy mode)"
    );
}

/// Regression: copy mode still copies file *contents*.
#[test]
fn test_copy_mode_file_copies_contents() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let src = dir.path().join("a.txt");
    let target = dir.path().join("b.txt");
    std::fs::write(&src, b"payload").unwrap();

    let plan = RenamePlan::from_ops_with_copy_mode(
        vec![RenameOp {
            original_path: src.clone(),
            new_path: target.clone(),
            original_name: "a.txt".into(),
            new_name: "b.txt".into(),
            was_copy: false,
            relocated: false,
            original_permissions: None,
            applied_attributes: String::new(),
            applied_timestamps_mask: 0,
        }],
        true,
    );
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert_eq!(result.successful_ops.len(), 1);
    assert_eq!(std::fs::read(&target).unwrap(), b"payload");
    assert!(src.exists(), "source preserved (copy mode)");
}

/// Undo of a copied directory removes the (empty) copy, leaving the source.
#[test]
fn test_apply_ops_chain_safe_copy_undo_removes_copied_dir() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("src");
    let copy = dir.path().join("dst");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&copy).unwrap(); // the empty copied directory

    // Undo-direction op for a copy: original = the copy location to remove,
    // new = the preserved source, was_copy = true.
    let op = RenameOp {
        original_path: copy.clone(),
        new_path: source.clone(),
        original_name: "dst".into(),
        new_name: "src".into(),
        was_copy: true,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    let result = apply_ops_chain_safe(&[op]);
    assert_eq!(result.successful.len(), 1);
    assert!(
        result.failed.is_empty(),
        "undo of a dir copy must not error"
    );
    assert!(!copy.exists(), "copied directory removed");
    assert!(source.is_dir(), "source preserved");
}

// ──────────────────────────────────────────────────────────
// Shared collision classification
// ──────────────────────────────────────────────────────────

/// The shared classifier is the single rule for chain / on-disk / free, and the
/// shared source set decides `Chain`. `detect_collisions`, `execute_plan`, and
/// `apply_ops_chain_safe` all read from these, so the reported set cannot drift
/// from what execution actually does.
#[test]
fn test_target_relation_classification() {
    use crate::core::case::case_sensitivity_for;
    use crate::core::execute::{TargetRelation, batch_source_set, target_key, target_relation};

    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let c = dir.path().join("c");
    let d = dir.path().join("d");
    let existing = dir.path().join("existing");
    std::fs::write(&b, b"").unwrap();
    std::fs::write(&existing, b"").unwrap();

    let mk = |orig: &Path, new: &Path| RenameOp {
        original_path: orig.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: orig.file_name().unwrap().to_string_lossy().to_string(),
        new_name: new.file_name().unwrap().to_string_lossy().to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    // a -> b (b exists and is op[1]'s source: chain)
    // b -> c (c is free)
    // d -> existing (exists on disk, not part of the batch)
    let ops = vec![mk(&a, &b), mk(&b, &c), mk(&d, &existing)];

    let mut cs_for = |p: &Path| case_sensitivity_for(p);
    let source_set = batch_source_set(&ops, |_, _| true, &mut cs_for);
    let cs = case_sensitivity_for(dir.path());

    let t0 = target_key(&ops[0], cs);
    assert_eq!(
        target_relation(&ops[0], &t0, cs, &source_set),
        TargetRelation::Chain,
        "target is another op's source"
    );
    let t1 = target_key(&ops[1], cs);
    assert_eq!(
        target_relation(&ops[1], &t1, cs, &source_set),
        TargetRelation::Free
    );
    let t2 = target_key(&ops[2], cs);
    assert_eq!(
        target_relation(&ops[2], &t2, cs, &source_set),
        TargetRelation::OnDisk,
        "target exists and is not part of the batch"
    );
}

/// A directory relocated to another folder is *copied* (recreated empty) even in
/// move mode — only the selected entry is ever moved, never the contents a
/// directory would drag along.
#[test]
fn test_move_mode_relocated_directory_is_copied() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let out = dir.path().join("out");
    let target = out.join("dst");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("inner.txt"), b"data").unwrap();

    let plan = RenamePlan::from_ops(vec![mk_relocated(&src, &target)]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    // Move mode: copy_mode stays false.
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert!(result.failed_ops.is_empty());
    assert_eq!(result.successful_ops.len(), 1);
    assert!(target.is_dir(), "directory recreated at the destination");
    assert_eq!(
        std::fs::read_dir(&target).unwrap().count(),
        0,
        "directory contents must not be moved/copied"
    );
    assert!(src.join("inner.txt").exists(), "source directory kept");
    assert!(
        result.successful_ops[0].was_copy,
        "the relocated dir is recorded as a copy (so undo removes it)"
    );
}

/// Regression: renaming a directory *in place* (same parent) still moves it with
/// its contents, and is not recorded as a copy.
#[test]
fn test_in_place_directory_rename_is_not_copy() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let target = dir.path().join("dst");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("inner.txt"), b"data").unwrap();

    let plan = RenamePlan::from_ops(vec![RenameOp {
        original_path: src.clone(),
        new_path: target.clone(),
        original_name: "src".into(),
        new_name: "dst".into(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    }]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert_eq!(result.successful_ops.len(), 1);
    assert!(
        target.join("inner.txt").exists(),
        "in-place rename moves the contents"
    );
    assert!(!src.exists(), "source vacated");
    assert!(!result.successful_ops[0].was_copy);
}

/// `op_is_copy` reads the explicit relocation flag, never path shape. A directory
/// whose target merely *looks* relocated (different parent) is still a move when
/// the flag is false — the case a computed name containing a separator used to be
/// mistaken for. Only a flagged **directory** is a copy; a relocated file is a
/// move, and copy mode makes anything a copy.
#[test]
fn test_op_is_copy_is_flag_driven_not_path_shaped() {
    use crate::core::execute::op_is_copy;

    let dir = TempDir::new().unwrap();
    let src_dir = dir.path().join("src");
    let other = dir.path().join("other");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, b"x").unwrap();

    // Directory with a different parent but no recorded relocation: not a copy.
    assert!(
        !op_is_copy(&mk_op(&src_dir, &other.join("src")), false),
        "path shape alone must not classify as a copy"
    );
    // The flag makes a relocated directory a copy...
    assert!(op_is_copy(
        &mk_relocated(&src_dir, &other.join("src")),
        false
    ));
    // ...but only directories: a relocated file is still a move.
    assert!(!op_is_copy(
        &mk_relocated(&file, &other.join("a.txt")),
        false
    ));
    // Copy mode makes any entry a copy.
    assert!(op_is_copy(&mk_relocated(&file, &other.join("a.txt")), true));
    assert!(op_is_copy(&mk_op(&file, &file), true));
}

// ──────────────────────────────────────────
// Chains vs copies
// ──────────────────────────────────────────

/// Copy mode never vacates a source, so a target that is another op's source is
/// a real on-disk conflict — never a chain that moves the source away.
/// (Regression: this used to silently *move* `a.txt`, losing the copy's source.)
#[test]
fn test_copy_mode_source_never_vacated() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let a = dir.path().join("a.txt");
    let b = out.join("b.txt");
    let c = out.join("c.txt");
    std::fs::write(&a, b"A").unwrap();
    std::fs::write(&b, b"B").unwrap();

    let mk = |orig: &Path, new: &Path| RenameOp {
        original_path: orig.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: orig.file_name().unwrap().to_string_lossy().to_string(),
        new_name: new.file_name().unwrap().to_string_lossy().to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    // op1 (copy): out/b -> out/c. op2 (copy): a.txt -> out/b (out/b stays put).
    let plan = RenamePlan::from_ops_with_copy_mode(vec![mk(&b, &c), mk(&a, &b)], true);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert!(result.failed_ops.is_empty());
    assert!(a.exists(), "the copy's source must be preserved");
    assert_eq!(std::fs::read(&a).unwrap(), b"A");
    assert!(c.exists(), "op1 copied its file");
    // op2 could not place its copy: the target is another copy's live source.
    assert_eq!(result.successful_ops.len(), 1);
    assert_eq!(result.skipped_ops.len(), 1);
    assert_eq!(
        std::fs::read(&b).unwrap(),
        b"B",
        "occupied target untouched"
    );
}

/// A relocated directory whose target is a mover's source must not park its own
/// source: it waits (deferred copy) and is recreated **empty** once the mover has
/// vacated the target. (Regression: it used to be moved, contents and all.)
#[test]
fn test_relocated_dir_chain_defers_and_copies_empty() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    // Same-depth sibling subtrees so ordering cannot avoid the conflict.
    let m = dir.path().join("m");
    let r = dir.path().join("r");
    std::fs::create_dir(&m).unwrap();
    std::fs::create_dir(&r).unwrap();
    let dst = m.join("a");
    let dst2 = m.join("a2");
    let src = r.join("src");
    std::fs::create_dir(&dst).unwrap();
    std::fs::write(dst.join("old.txt"), b"OLD").unwrap();
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("inner.txt"), b"INNER").unwrap();

    let mk = |orig: &Path, new: &Path| RenameOp {
        original_path: orig.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: orig.file_name().unwrap().to_string_lossy().to_string(),
        new_name: new.file_name().unwrap().to_string_lossy().to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    // op1 (relocated dir): r/src -> m/a (target is op2's source). op2 (mover):
    // m/a -> m/a2.
    let plan = RenamePlan::from_ops(vec![mk_relocated(&src, &dst), mk(&dst, &dst2)]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert!(result.failed_ops.is_empty());
    assert_eq!(result.successful_ops.len(), 2);
    assert!(
        src.join("inner.txt").exists(),
        "relocated source kept with its contents"
    );
    assert!(dst2.join("old.txt").exists(), "mover moved the contents");
    assert!(dst.is_dir(), "relocated dir recreated");
    assert_eq!(
        std::fs::read_dir(&dst).unwrap().count(),
        0,
        "recreated empty, not moved"
    );
    assert!(
        result
            .successful_ops
            .iter()
            .any(|op| op.was_copy && op.original_path == src),
        "the relocated dir is recorded as a copy"
    );
}

/// A parked mover whose conflict never vacated (the conflicting op skipped) must
/// not clobber the live file: the source is restored and the op is skipped.
/// (Regression: the deferred rename used to overwrite the occupied target.)
#[test]
fn test_mover_chain_skipped_conflict_does_not_clobber() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let a = dir.path().join("a.txt");
    let b = out.join("b.txt");
    let c = out.join("c.txt");
    std::fs::write(&a, b"A").unwrap();
    std::fs::write(&b, b"B").unwrap();
    std::fs::write(&c, b"C").unwrap();

    let mk = |orig: &Path, new: &Path| RenameOp {
        original_path: orig.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: orig.file_name().unwrap().to_string_lossy().to_string(),
        new_name: new.file_name().unwrap().to_string_lossy().to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    // op1 (mover): out/b -> out/c is a genuine on-disk collision and skips.
    // op2 (mover): a.txt -> out/b chains onto op1's source, which then never
    // vacates.
    let plan = RenamePlan::from_ops(vec![mk(&b, &c), mk(&a, &b)]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert!(result.failed_ops.is_empty(), "must skip, not error");
    assert_eq!(result.skipped_ops.len(), 2);
    assert!(a.exists(), "parked source restored, not moved away");
    assert_eq!(std::fs::read(&b).unwrap(), b"B", "live file not clobbered");
    assert_eq!(std::fs::read(&c).unwrap(), b"C");
}

/// The plan's collision report uses the same copy-aware classification as
/// execution: in copy mode a target that is another op's source is reported as
/// an on-disk conflict (chains are no longer filtered).
#[test]
fn test_collisions_report_copy_sources_as_ondisk() {
    use crate::core::plan::RenamePlan;

    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    let c = dir.path().join("c.txt");
    std::fs::write(&a, b"A").unwrap();
    std::fs::write(&b, b"B").unwrap();

    let mk = |orig: &Path, new: &Path| RenameOp {
        original_path: orig.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: orig.file_name().unwrap().to_string_lossy().to_string(),
        new_name: new.file_name().unwrap().to_string_lossy().to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    };
    // a -> b (b is op[1]'s source), b -> c.
    let ops = vec![mk(&a, &b), mk(&b, &c)];

    // Move mode: a -> b is a chain (b is another op's source) — not a collision.
    assert_eq!(
        RenamePlan::from_ops(ops.clone()).collisions().len(),
        0,
        "chain is filtered in move mode"
    );

    // Copy mode: sources are not vacated, so a -> b is a real on-disk conflict.
    let cols = RenamePlan::from_ops_with_copy_mode(ops, true).collisions();
    assert_eq!(cols.len(), 1);
    assert_eq!(cols[0].op_index, 0);
    assert!(matches!(cols[0].reason, CollisionReason::OnDisk));
}

// ──────────────────────────────────────────
// Directories in the batch, and parked directories
// ──────────────────────────────────────────

fn mk_op(orig: &Path, new: &Path) -> RenameOp {
    RenameOp {
        original_path: orig.to_path_buf(),
        new_path: new.to_path_buf(),
        original_name: orig.file_name().unwrap().to_string_lossy().to_string(),
        new_name: new.file_name().unwrap().to_string_lossy().to_string(),
        was_copy: false,
        relocated: false,
        original_permissions: None,
        applied_attributes: String::new(),
        applied_timestamps_mask: 0,
    }
}

/// Like [`mk_op`] but marked as relocated by the output location — what the
/// planner produces for an entry placed in a different directory. Mirrors a
/// real Copy / Move to Location plan, where relocation is an explicit fact.
fn mk_relocated(orig: &Path, new: &Path) -> RenameOp {
    RenameOp {
        relocated: true,
        ..mk_op(orig, new)
    }
}

/// A directory and its child are both in the batch, as a recursive scan with
/// keep-structure produces (`out/D` and `out/D/f.txt`). The child creates the
/// parent directory for its target; the parent dir op then finds its own target
/// already present and is skipped — no clash, and the subtree is populated.
#[test]
fn test_nested_dir_keep_structure_copy() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let d = dir.path().join("D");
    let f = d.join("f.txt");
    std::fs::create_dir(&d).unwrap();
    std::fs::write(&f, b"F").unwrap();
    let out = dir.path().join("out");

    let plan = RenamePlan::from_ops_with_copy_mode(
        vec![
            mk_op(&f, &out.join("D").join("f.txt")),
            mk_relocated(&d, &out.join("D")),
        ],
        true,
    );
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert!(
        result.failed_ops.is_empty(),
        "no clash: {:?}",
        result.failed_ops
    );
    assert_eq!(result.successful_ops.len(), 1, "the child copied");
    assert_eq!(result.skipped_ops.len(), 1, "the parent dir target exists");
    assert!(out.join("D").join("f.txt").exists());
    assert!(f.exists(), "copy mode keeps the source file");
    assert!(d.exists(), "copy mode keeps the source dir");
}

/// Same shape in move mode: the child moves into the output tree and the parent
/// directory op is skipped (its target already exists) — again no clash.
#[test]
fn test_nested_dir_keep_structure_move() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let d = dir.path().join("D");
    let f = d.join("f.txt");
    std::fs::create_dir(&d).unwrap();
    std::fs::write(&f, b"F").unwrap();
    let out = dir.path().join("out");

    let plan = RenamePlan::from_ops(vec![
        mk_op(&f, &out.join("D").join("f.txt")),
        mk_relocated(&d, &out.join("D")),
    ]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert!(
        result.failed_ops.is_empty(),
        "no clash: {:?}",
        result.failed_ops
    );
    assert_eq!(result.successful_ops.len(), 1);
    assert_eq!(result.skipped_ops.len(), 1);
    assert!(out.join("D").join("f.txt").exists());
    assert!(!f.exists(), "move mode vacates the source file");
}

/// A mover file whose target is a directory that is itself a mover parked at a
/// temp name (in-place dir rename). The file chains onto the dir's source; both
/// complete to their correct targets — the file lands at the dir's old path.
#[test]
fn test_move_file_onto_parked_dir_path() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let m = dir.path().join("m");
    let y = m.join("y");
    std::fs::create_dir_all(&y).unwrap();
    let a = m.join("a");
    let a2 = m.join("a2");
    let xf = y.join("x.txt");
    std::fs::create_dir(&a).unwrap();
    std::fs::write(a.join("old.txt"), b"OLD").unwrap();
    std::fs::write(&xf, b"X").unwrap();

    // Same depth; the file is listed first so it is classified before the dir
    // vacates `m/a` (forcing the chain path).
    let plan = RenamePlan::from_ops(vec![mk_op(&xf, &a), mk_op(&a, &a2)]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert!(result.failed_ops.is_empty(), "{:?}", result.failed_ops);
    assert_eq!(result.successful_ops.len(), 2);
    assert!(
        a.is_file(),
        "the moved file now occupies the dir's old path"
    );
    assert!(!xf.exists(), "the file's source is vacated");
    assert!(
        a2.join("old.txt").exists(),
        "the dir moved with its contents"
    );
}

/// A relocated directory (a copy) whose target is a directory that is parked: the
/// copy waits for the mover to vacate, then is recreated **empty** at the old
/// path while the mover carries the original contents away.
#[test]
fn test_copy_dir_onto_parked_dir_path() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let m = dir.path().join("m");
    let r = dir.path().join("r");
    std::fs::create_dir_all(&m).unwrap();
    std::fs::create_dir_all(&r).unwrap();
    let a = m.join("a");
    let a2 = m.join("a2");
    let src = r.join("src");
    std::fs::create_dir(&a).unwrap();
    std::fs::write(a.join("old.txt"), b"OLD").unwrap();
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("inner.txt"), b"INNER").unwrap();

    // Same depth; the relocated dir (copy) is listed first.
    let plan = RenamePlan::from_ops(vec![mk_relocated(&src, &a), mk_op(&a, &a2)]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert!(result.failed_ops.is_empty(), "{:?}", result.failed_ops);
    assert_eq!(result.successful_ops.len(), 2);
    assert_eq!(std::fs::read_dir(&a).unwrap().count(), 0, "recreated empty");
    assert!(a2.join("old.txt").exists(), "mover carried the contents");
    assert!(src.join("inner.txt").exists(), "the copy's source is kept");
}

// ──────────────────────────────────────────
// Undo record excludes Copy / Move to Location
// ──────────────────────────────────────────

/// The undo record written by `execute_plan` excludes Copy / Move to Location
/// transfers (SSoT: `RenameOp::is_undoable`), so replaying it reverts only
/// in-place renames.
#[test]
fn test_undo_file_excludes_location_transfers() {
    use crate::core::plan::{PlanOptions, Resolution, execute_plan, plan_renames};

    let dir = TempDir::new().unwrap();
    let src = dir.path().join("a.txt");
    std::fs::write(&src, b"x").unwrap();
    let out = dir.path().join("out");
    let undo = dir.path().join("undo.json");

    let files = vec![src.to_string_lossy().to_string()];
    let config = RenameConfig::default();
    let opts = PlanOptions {
        output_dir: Some(out.as_path()),
        copy_mode: true,
        ..Default::default()
    };
    let plan = plan_renames(&files, &config, &opts);
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions {
        undo_file: Some(undo.to_str().unwrap()),
        ..Default::default()
    };
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert_eq!(result.successful_ops.len(), 1, "the copy happened");
    let json = std::fs::read_to_string(&undo).unwrap();
    assert_eq!(json.trim(), "[]", "location transfers are not recorded");
}

/// An in-place rename is still recorded, so the CLI/TUI undo file keeps working.
#[test]
fn test_undo_file_records_in_place_renames() {
    use crate::core::plan::{PlanOptions, Resolution, execute_plan, plan_renames};

    let dir = TempDir::new().unwrap();
    let src = dir.path().join("a.txt");
    std::fs::write(&src, b"x").unwrap();
    let undo = dir.path().join("undo.json");

    let files = vec![src.to_string_lossy().to_string()];
    let config = RenameConfig {
        add: crate::AddSection {
            add_prefix: Some("x_".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let plan = plan_renames(&files, &config, &PlanOptions::default());
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions {
        undo_file: Some(undo.to_str().unwrap()),
        ..Default::default()
    };
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert_eq!(result.successful_ops.len(), 1);
    let json = std::fs::read_to_string(&undo).unwrap();
    assert!(json.contains("x_a.txt"), "the rename is recorded: {json}");
}

/// A mixed apply records exactly the undoable ops: a relocated transfer is
/// skipped while an in-place op in the same batch is written to the undo file.
/// Undoability is decided per op from the recorded flag, not from a plan-level
/// signal — the plan here is built by hand and carries none.
#[test]
fn test_undo_file_records_only_non_transfers_in_mixed_apply() {
    use crate::core::plan::{RenamePlan, Resolution, execute_plan};

    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    std::fs::write(&a, b"A").unwrap();
    std::fs::write(&b, b"B").unwrap();
    let undo = dir.path().join("undo.json");

    // One relocated transfer (a.txt -> out/a.txt), one in-place rename
    // (b.txt -> b2.txt).
    let plan = RenamePlan::from_ops(vec![
        mk_relocated(&a, &out.join("a.txt")),
        mk_op(&b, &dir.path().join("b2.txt")),
    ]);
    let config = RenameConfig::default();
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions {
        undo_file: Some(undo.to_str().unwrap()),
        ..Default::default()
    };
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert_eq!(result.successful_ops.len(), 2, "both ops applied");
    let ops: Vec<RenameOp> =
        serde_json::from_str(&std::fs::read_to_string(&undo).unwrap()).unwrap();
    assert_eq!(ops.len(), 1, "only the in-place rename is recorded");
    assert_eq!(ops[0].original_path, b);
    assert!(!ops[0].relocated);
}

/// A computed name carrying a path separator relocates the entry into a subpath.
/// It is a structural transfer like a location transfer, so it is excluded from
/// undo/redo/revert — but it is **not** an output-location relocation: `relocated`
/// stays false and `op_is_copy` is unaffected (classification stays separate).
#[test]
fn test_path_producing_rename_is_a_transfer_but_not_a_relocation() {
    use crate::core::execute::op_is_copy;
    use crate::core::plan::{PlanOptions, plan_renames};

    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    std::fs::write(&a, b"x").unwrap();
    let files = vec![a.to_string_lossy().to_string()];
    let config = RenameConfig {
        add: crate::AddSection {
            add_prefix: Some("sub/".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let plan = plan_renames(&files, &config, &PlanOptions::default());

    assert_eq!(plan.ops().len(), 1);
    let op = &plan.ops()[0];
    assert_eq!(op.new_name, "sub/a.txt");
    assert!(op.name_implies_subpath(), "the name produced a path");
    assert!(!op.relocated, "it is not an output-location relocation");
    assert!(
        !op.is_location_transfer(),
        "copy/move classification stays separate"
    );
    assert!(!op.is_undoable(), "a path-producing rename is not undoable");
    assert!(
        !op_is_copy(op, false),
        "a path-producing file rename is a move, not a copy"
    );
}

/// The undo record excludes a path-producing rename (the same rule as a location
/// transfer), and execution still creates the subpath it names.
#[test]
fn test_undo_file_excludes_path_producing_rename() {
    use crate::core::plan::{PlanOptions, Resolution, execute_plan, plan_renames};

    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    std::fs::write(&a, b"x").unwrap();
    let undo = dir.path().join("undo.json");

    let files = vec![a.to_string_lossy().to_string()];
    let config = RenameConfig {
        add: crate::AddSection {
            add_prefix: Some("sub/".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let plan = plan_renames(&files, &config, &PlanOptions::default());
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions {
        undo_file: Some(undo.to_str().unwrap()),
        ..Default::default()
    };
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert_eq!(result.successful_ops.len(), 1);
    assert!(
        dir.path().join("sub/a.txt").exists(),
        "the entry moved into the named subpath"
    );
    let json = std::fs::read_to_string(&undo).unwrap();
    assert_eq!(json.trim(), "[]", "path-producing renames are not recorded");
}

/// A computed name that *navigates* the path (rooted or `.`/`..`) is refused at
/// the execution boundary: the op is planned (so previews show it and the GUI can
/// explain), but nothing is renamed and nothing is created — only plain subfolder
/// creation is allowed.
#[test]
fn test_execute_refuses_navigational_name() {
    use crate::core::plan::{PlanOptions, Resolution, execute_plan, plan_renames};

    let dir = TempDir::new().unwrap();
    let parent = dir.path().join("parent");
    std::fs::create_dir(&parent).unwrap();
    let a = parent.join("a.txt");
    std::fs::write(&a, b"x").unwrap();

    for prefix in ["../", "/", "./well/"] {
        let files = vec![a.to_string_lossy().to_string()];
        let config = RenameConfig {
            add: crate::AddSection {
                add_prefix: Some(prefix.into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = plan_renames(&files, &config, &PlanOptions::default());
        assert_eq!(plan.ops().len(), 1, "the op is still planned ({prefix:?})");
        assert!(
            crate::path_navigation_error(&plan.ops()[0].new_name).is_some(),
            "{prefix:?} should be navigational"
        );

        let resolution = Resolution::default_for(CollisionStrategy::Skip);
        let mut options = RenameOptions::default();
        let result = execute_plan(plan, &config, &mut options, &resolution);

        assert!(result.successful_ops.is_empty(), "refused, not executed");
        assert_eq!(result.failed_ops.len(), 1, "the invalid name is reported");
        assert!(a.exists(), "source untouched");
        let entries: Vec<_> = std::fs::read_dir(&parent)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries.len(), 1, "nothing created: {entries:?}");
    }
}

/// A navigational name that resolves to the source path (`./a.txt`) is still an
/// invalid name: it must stay in the plan (so it is reported) instead of being
/// dropped as an inert no-op.
#[test]
fn test_navigational_noop_is_planned_and_refused() {
    use crate::core::plan::{PlanOptions, Resolution, execute_plan, plan_renames};

    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    std::fs::write(&a, b"x").unwrap();

    let files = vec![a.to_string_lossy().to_string()];
    let config = RenameConfig {
        add: crate::AddSection {
            add_prefix: Some("./".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let plan = plan_renames(&files, &config, &PlanOptions::default());
    assert_eq!(plan.ops().len(), 1, "kept, not dropped as a no-op");
    assert!(crate::path_navigation_error(&plan.ops()[0].new_name).is_some());

    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);
    assert!(result.successful_ops.is_empty());
    assert_eq!(result.failed_ops.len(), 1, "reported, not silently dropped");
    assert!(a.exists());
}

/// One invalid name aborts the **whole** apply: valid ops in the same batch must
/// not run, so a bad name can never partially apply.
#[test]
fn test_invalid_name_aborts_whole_batch() {
    use crate::core::plan::{PlanOptions, Resolution, execute_plan, plan_renames};

    let dir = TempDir::new().unwrap();
    let good = dir.path().join("good.txt");
    let bad = dir.path().join("bad.txt");
    std::fs::write(&good, b"g").unwrap();
    std::fs::write(&bad, b"b").unwrap();

    let config = RenameConfig {
        add: crate::AddSection {
            add_prefix: Some("x_".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let files = vec![
        good.to_string_lossy().to_string(),
        bad.to_string_lossy().to_string(),
    ];
    // Force the second op to be navigational, as a bad config would.
    let mut ops = plan_renames(&files, &config, &PlanOptions::default()).into_ops();
    ops[1].new_name = "../bad.txt".into();
    ops[1].new_path = dir.path().join("..").join("bad.txt");
    let plan = crate::RenamePlan::from_ops(ops);

    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let mut options = RenameOptions::default();
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert!(result.successful_ops.is_empty(), "nothing ran");
    assert_eq!(
        result.failed_ops.len(),
        1,
        "only the invalid name is reported"
    );
    assert_eq!(result.skipped_ops.len(), 0);
    assert!(good.exists(), "the valid op was not applied either");
    assert!(!dir.path().join("x_good.txt").exists());
    assert!(bad.exists());
}

/// A blocked apply reports the bad names but emits **no `Done`** — that event
/// marks a completed execution, and none happened.
#[test]
fn test_aborted_apply_emits_no_done() {
    use crate::core::plan::{PlanOptions, Resolution, execute_plan, plan_renames};
    use std::sync::{Arc, Mutex};

    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    std::fs::write(&a, b"x").unwrap();
    let files = vec![a.to_string_lossy().to_string()];
    let config = RenameConfig {
        add: crate::AddSection {
            add_prefix: Some("../".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let plan = plan_renames(&files, &config, &PlanOptions::default());

    let events: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let mut options = RenameOptions {
        on_event: Some(Box::new(move |evt| {
            let tag = match evt {
                RenameEvent::Error { .. } => "Error",
                RenameEvent::Status { .. } => "Status",
                RenameEvent::Done { .. } => "Done",
                _ => "other",
            };
            sink.lock().unwrap().push(tag);
        })),
        ..Default::default()
    };
    let resolution = Resolution::default_for(CollisionStrategy::Skip);
    let result = execute_plan(plan, &config, &mut options, &resolution);

    assert_eq!(result.failed_ops.len(), 1);
    let tags = events.lock().unwrap().clone();
    assert!(tags.contains(&"Error"), "reports the bad name: {tags:?}");
    assert!(
        !tags.contains(&"Done"),
        "no Done on a blocked apply: {tags:?}"
    );
}
