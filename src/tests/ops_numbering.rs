use super::*;

#[test]
fn test_extension_ops() {
    let config = RenameConfig {
        extension: ExtensionSection {
            extension_remove: true,
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
        "file"
    );

    let config_replace = RenameConfig {
        extension: ExtensionSection {
            extension_replace: Some("bak".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled_replace = CompiledConfig::new(config_replace);
    assert_eq!(
        process_filename(
            "file.txt",
            &compiled_replace,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "file.bak"
    );

    let config_append = RenameConfig {
        extension: ExtensionSection {
            extension_append: Some("bak".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled_append = CompiledConfig::new(config_append);
    assert_eq!(
        process_filename(
            "file.txt",
            &compiled_append,
            None,
            None,
            false,
            &FileMeta::default()
        ),
        "file.txt.bak"
    );
}

#[test]
fn test_numbering_insert() {
    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Insert),
            numbering_at: Some(2),
            numbering_start: 10,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "file.txt",
            &compiled,
            Some(0),
            None,
            false,
            &FileMeta::default()
        ),
        "fi10le.txt"
    );
}

#[test]
fn test_numbering_bases() {
    let cases: &[(&str, &[&str])] = &[
        ("decimal", &["1", "2", "3", "4"]),
        ("2", &["1", "10", "11", "100"]),
        ("8", &["1", "2", "3", "4"]),
        ("16", &["1", "2", "3", "4"]),
        ("az_upper", &["A", "B", "C", "D"]),
        ("roman", &["I", "II", "III", "IV"]),
    ];
    for &(typ, expected) in cases {
        for (idx, &exp) in expected.iter().enumerate() {
            let config = RenameConfig {
                numbering: NumberingSection {
                    numbering_mode: Some(NumberingMode::Prefix),
                    numbering_start: 1,
                    numbering_increment: 1,
                    numbering_sep: Some("_".to_string()),
                    numbering_type: Some(typ.to_string()),
                    ..Default::default()
                },
                ..Default::default()
            };
            let compiled = CompiledConfig::new(config);
            let result = process_filename(
                "file.txt",
                &compiled,
                Some(idx),
                None,
                false,
                &FileMeta::default(),
            );
            assert_eq!(
                result,
                format!("{}_file.txt", exp),
                "numbering base '{typ}' at index {idx}"
            );
        }
    }
}

#[test]
fn test_numbering_start_and_increment() {
    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Suffix),
            numbering_start: 10,
            numbering_increment: 5,
            numbering_sep: Some("-".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "a.txt",
            &compiled,
            Some(0),
            None,
            false,
            &FileMeta::default()
        ),
        "a-10.txt"
    );
    assert_eq!(
        process_filename(
            "b.txt",
            &compiled,
            Some(1),
            None,
            false,
            &FileMeta::default()
        ),
        "b-15.txt"
    );
    assert_eq!(
        process_filename(
            "c.txt",
            &compiled,
            Some(2),
            None,
            false,
            &FileMeta::default()
        ),
        "c-20.txt"
    );
}

#[test]
fn test_numbering_break_resets_every_n_files() {
    // `numbering_break(n)` restarts the sequence every n files, yielding
    // offsets 0,1,0,1,... for n = 2.
    let files: Vec<String> = ["a1.txt", "a2.txt", "a3.txt", "b1.txt", "b2.txt", "b3.txt"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let order: Vec<usize> = (0..files.len()).collect();
    let seq = numbering_sequence(&order, &files, false, Some(2), |_| true);
    let offsets: Vec<usize> = seq.into_iter().map(|s| s.unwrap()).collect();
    assert_eq!(offsets, vec![0, 1, 0, 1, 0, 1]);
}

#[test]
fn test_numbering_break_is_independent_per_folder() {
    // With restart_folder, each folder keeps its own counter and break count, so
    // one folder's reset never affects another. Interleaved folders prove it:
    // break = 2 restarts a3 and b3 independently.
    let files: Vec<String> = ["a/a1.txt", "b/b1.txt", "a/a2.txt", "b/b2.txt", "a/a3.txt"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let order: Vec<usize> = (0..files.len()).collect();
    let seq = numbering_sequence(&order, &files, true, Some(2), |_| true);
    let offsets: Vec<usize> = seq.into_iter().map(|s| s.unwrap()).collect();
    assert_eq!(offsets, vec![0, 0, 1, 1, 0]);
}

#[test]
fn test_numbering_without_restart_folder_stays_global() {
    // Without restart_folder the counter and break count stay global, so a
    // single sequence spans folders.
    let files: Vec<String> = ["a/a1.txt", "b/b1.txt"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let order: Vec<usize> = (0..files.len()).collect();
    let seq = numbering_sequence(&order, &files, false, Some(10), |_| true);
    let offsets: Vec<usize> = seq.into_iter().map(|s| s.unwrap()).collect();
    assert_eq!(offsets, vec![0, 1]);
}

#[test]
fn test_numbering_break_larger_than_batch_keeps_sequence() {
    // A break larger than the number of files never triggers → one run.
    let files: Vec<String> = ["a1.txt", "a2.txt", "a3.txt"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let order: Vec<usize> = (0..files.len()).collect();
    let seq = numbering_sequence(&order, &files, false, Some(10), |_| true);
    let offsets: Vec<usize> = seq.into_iter().map(|s| s.unwrap()).collect();
    assert_eq!(offsets, vec![0, 1, 2]);
}

#[test]
fn test_numbering_pad() {
    let config = RenameConfig {
        numbering: NumberingSection {
            numbering_mode: Some(NumberingMode::Prefix),
            numbering_start: 1,
            numbering_increment: 1,
            numbering_sep: Some("-".to_string()),
            numbering_pad: 4,
            ..Default::default()
        },
        ..Default::default()
    };
    let compiled = CompiledConfig::new(config);
    assert_eq!(
        process_filename(
            "f.txt",
            &compiled,
            Some(0),
            None,
            false,
            &FileMeta::default()
        ),
        "0001-f.txt"
    );
    assert_eq!(
        process_filename(
            "f.txt",
            &compiled,
            Some(99),
            None,
            false,
            &FileMeta::default()
        ),
        "0100-f.txt"
    );
}

#[test]
fn test_numbering_case() {
    let cases: &[(&str, Option<&str>, &[&str])] = &[
        ("roman", Some("upper"), &["I", "II", "III", "IV"]),
        ("roman", Some("lower"), &["i", "ii", "iii", "iv"]),
        ("roman", None, &["I", "II", "III", "IV"]),
        ("az_upper", Some("upper"), &["A", "B", "C", "D"]),
        ("az_upper", Some("lower"), &["a", "b", "c", "d"]),
    ];
    for &(typ, case, expected) in cases {
        let mut cfg = RenameConfig {
            numbering: NumberingSection {
                numbering_mode: Some(NumberingMode::Prefix),
                numbering_start: 1,
                numbering_increment: 1,
                numbering_sep: Some("_".to_string()),
                numbering_type: Some(typ.to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        cfg.numbering.numbering_case = case.map(|s| s.to_string());
        let compiled = CompiledConfig::new(cfg);
        for (idx, &exp) in expected.iter().enumerate() {
            assert_eq!(
                process_filename(
                    "f.txt",
                    &compiled,
                    Some(idx),
                    None,
                    false,
                    &FileMeta::default()
                ),
                format!("{}_f.txt", exp),
                "{typ:?} case={case:?} idx={idx}"
            );
        }
    }
}
