//! Tests for the EXIF gating in `FileMeta::from_metadata`.
//!
//! Reading EXIF opens and parses every file, which dominated planning cost, so
//! it now happens only for configs that consume an EXIF date
//! (`config_needs_exif_date`). These tests use a real (minimal) EXIF file to
//! prove the gate skips work without changing what a tag that needs EXIF sees.

use super::*;

/// A minimal JPEG carrying an EXIF `DateTimeOriginal`.
///
/// The tag lives in the Exif sub-IFD (reached via an ExifIFDPointer from IFD0),
/// which is where real files put it — placing it in IFD0 directly parses as TIFF
/// but never resolves as `DateTimeOriginal`.
fn exif_jpeg(datetime: &str) -> Vec<u8> {
    let value = format!("{datetime}\0");
    let ifd0 = 8u32;
    let sub_ifd = ifd0 + 2 + 12 + 4;
    let value_offset = sub_ifd + 2 + 12 + 4;

    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II\x2a\x00"); // little-endian, magic 42
    tiff.extend_from_slice(&ifd0.to_le_bytes());
    // IFD0: one ExifIFDPointer entry.
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x8769u16.to_le_bytes());
    tiff.extend_from_slice(&4u16.to_le_bytes()); // LONG
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&sub_ifd.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    // Exif sub-IFD: one DateTimeOriginal entry.
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x9003u16.to_le_bytes());
    tiff.extend_from_slice(&2u16.to_le_bytes()); // ASCII
    tiff.extend_from_slice(&(value.len() as u32).to_le_bytes());
    tiff.extend_from_slice(&value_offset.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(value.as_bytes());

    let mut jpeg = vec![0xFF, 0xD8]; // SOI
    jpeg.extend_from_slice(&[0xFF, 0xE1]); // APP1
    // The length field counts itself plus the payload.
    jpeg.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
    jpeg.extend_from_slice(b"Exif\0\0");
    jpeg.extend_from_slice(&tiff);
    jpeg.extend_from_slice(&[0xFF, 0xD9]); // EOI
    jpeg
}

/// The crafted file really carries a parseable EXIF date. If this fails, the
/// fixture is wrong and the gating test below proves nothing.
#[test]
fn test_crafted_exif_fixture_parses() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("photo.jpg");
    std::fs::write(&path, exif_jpeg("2021:01:02 03:04:05")).unwrap();

    let meta = FileMeta::from_metadata(&path, std::fs::metadata(&path).ok().as_ref(), true);
    let expected =
        crate::core::rename::days_from_civil(2021, 1, 2).unwrap() * 86_400 + 3 * 3600 + 4 * 60 + 5;
    assert_eq!(meta.exif_date, Some(expected));
}

/// The gate itself: with a config that consumes EXIF the date is read; without
/// one the file is never opened, and nothing else about the metadata changes.
#[test]
fn test_exif_is_read_only_when_a_config_needs_it() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("photo.jpg");
    std::fs::write(&path, exif_jpeg("2021:01:02 03:04:05")).unwrap();
    let md = std::fs::metadata(&path).ok();

    let expected =
        crate::core::rename::days_from_civil(2021, 1, 2).unwrap() * 86_400 + 3 * 3600 + 4 * 60 + 5;

    let needs_exif = FileMeta::from_metadata(&path, md.as_ref(), true);
    assert_eq!(
        needs_exif.exif_date,
        Some(expected),
        "a tag that consumes EXIF must still see the EXIF date"
    );

    let skips_exif = FileMeta::from_metadata(&path, md.as_ref(), false);
    assert_eq!(
        skips_exif.exif_date, None,
        "a config with no EXIF tag must not open and parse the file"
    );

    // Everything else comes from the single stat the caller already needed, so
    // gating EXIF cannot change any mtime/ctime-based name.
    assert_eq!(needs_exif.modified, skips_exif.modified);
    assert_eq!(needs_exif.created, skips_exif.created);
    assert_eq!(needs_exif.accessed, skips_exif.accessed);
    assert!(skips_exif.modified.is_some(), "times still present");
}

/// End to end: the same config that names a file from its EXIF date still does
/// so through the real pipeline, and the pipeline reports that it needs EXIF.
#[test]
fn test_exif_date_reaches_the_name_with_the_gate_on() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("photo.jpg");
    std::fs::write(&path, exif_jpeg("2021:01:02 03:04:05")).unwrap();

    let config = RenameConfig {
        auto_date: AutoDateSection {
            date_position: DatePosition::Prefix,
            auto_date_century: true,
            auto_date_offset: Some(0),
            insert_meta: Some("exif-date".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(
        config_needs_exif_date(&config),
        "an exif tag must make the config request EXIF"
    );

    let compiled = CompiledConfig::new(config.clone());
    assert!(
        compiled.needs_exif_date,
        "flag is hoisted onto the compiled config"
    );

    let renamed = calculate_rename(
        path.to_string_lossy().as_ref(),
        &compiled,
        None,
        false,
        None,
    )
    .expect("file exists");
    // The EXIF instant is formatted to *local* time, so a fixture near midnight
    // UTC can shift by a day; assert the year/month it must have come from rather
    // than an exact day. The file's mtime is unrelated (it is "now"), which is
    // what distinguishes a real EXIF read from the old silent mtime fallback.
    assert!(
        renamed.new_name.starts_with("2021-01-0"),
        "expected the EXIF date as a prefix, got {:?}",
        renamed.new_name
    );

    // The same file under a config that never asks for EXIF still renames, from
    // mtime instead — and must not report needing EXIF.
    let mut plain = config.clone();
    plain.auto_date.insert_meta = Some("modified".into());
    assert!(!config_needs_exif_date(&plain));
    let plain_compiled = CompiledConfig::new(plain);
    assert!(!plain_compiled.needs_exif_date);
    let from_mtime = calculate_rename(
        path.to_string_lossy().as_ref(),
        &plain_compiled,
        None,
        false,
        None,
    )
    .expect("mtime-based naming still works");
    assert_ne!(
        from_mtime.new_name, renamed.new_name,
        "the exif-tag name must not be the mtime fallback"
    );
}
