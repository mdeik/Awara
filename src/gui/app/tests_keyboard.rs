use super::*;
use eframe::App as _;
use eframe::egui::{self, Event, Key, Modifiers, Pos2, Rect, Vec2};
use std::fs::File;
use tempfile::TempDir;

/// Drive one real frame through the same entry point eframe uses: `App::ui`
/// inside `Context::run_ui` (which hands out the root `Ui`). Reusing one
/// `Context` across calls preserves widget/focus memory between frames.
fn run_frame(ctx: &egui::Context, app: &mut GuiApp, events: Vec<Event>) {
    let mut frame = eframe::Frame::_new_kittest();
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
        events,
        ..Default::default()
    };
    // `run_ui` mirrors eframe's entry point. The returned font-atlas deltas are
    // normally uploaded by the render backend; clear them so epaint doesn't
    // assert on drop.
    let mut output = ctx.run_ui(raw, |ui| app.ui(ui, &mut frame));
    output.textures_delta.clear();
}

fn key_event(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// A key press, preceded by the modifiers change real backends report first.
/// `InputState::modifiers` (read by handlers via `i.modifiers`) is only updated
/// by `Event::ModifiersChanged`, never by a key event's own modifier field.
fn press(key: Key, modifiers: Modifiers) -> Vec<Event> {
    vec![
        Event::ModifiersChanged(modifiers),
        key_event(key, modifiers),
    ]
}

/// App with `names` as the file list, `preview_active`, nothing selected.
fn app_with_files(names: &[&str]) -> GuiApp {
    let mut app = GuiApp::new_with_config_for_test();
    app.all_files = names.iter().map(PathBuf::from).collect();
    app.file_display_names = names.iter().map(|n| n.to_string()).collect();
    app.selection = vec![false; names.len()];
    app.preview_active = true;
    app
}

/// Start an inline (F2) edit on row 0, selecting it.
fn begin_edit(app: &mut GuiApp) {
    app.selection[0] = true;
    app.last_clicked_idx = Some(0);
    app.editing_idx = Some(0);
    app.edit_buffer = app
        .all_files
        .first()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
}

// ── Delete ──

#[test]
fn delete_during_inline_edit_does_not_arm_trash() {
    let ctx = egui::Context::default();
    let mut app = app_with_files(&["/tmp/a.txt", "/tmp/b.txt"]);
    begin_edit(&mut app);

    run_frame(&ctx, &mut app, press(Key::Delete, Modifiers::NONE));

    assert!(
        !app.show_trash_confirmation,
        "Delete during an F2 edit must delete a character, not arm Confirm Trash"
    );
    assert_eq!(
        app.editing_idx,
        Some(0),
        "the edit must survive the key press"
    );
}

#[test]
fn delete_without_edit_arms_trash() {
    let ctx = egui::Context::default();
    let mut app = app_with_files(&["/tmp/a.txt", "/tmp/b.txt"]);
    app.selection[0] = true;
    app.last_clicked_idx = Some(0);

    run_frame(&ctx, &mut app, press(Key::Delete, Modifiers::NONE));

    assert!(
        app.show_trash_confirmation,
        "Delete with a selection and no active edit should arm Confirm Trash"
    );
}

// ── Selection-navigation keys the TextEdit owns ──

#[test]
fn arrow_nav_is_blocked_during_inline_edit() {
    let ctx = egui::Context::default();
    let mut app = app_with_files(&["/tmp/a.txt", "/tmp/b.txt", "/tmp/c.txt"]);
    begin_edit(&mut app);

    run_frame(&ctx, &mut app, press(Key::ArrowDown, Modifiers::NONE));

    assert_eq!(
        app.last_clicked_idx,
        Some(0),
        "↓ during an F2 edit belongs to the text caret, not the file selection"
    );
    assert_eq!(app.selection, vec![true, false, false]);
}

#[test]
fn arrow_nav_moves_selection_without_edit() {
    let ctx = egui::Context::default();
    let mut app = app_with_files(&["/tmp/a.txt", "/tmp/b.txt", "/tmp/c.txt"]);
    app.selection[0] = true;
    app.last_clicked_idx = Some(0);

    run_frame(&ctx, &mut app, press(Key::ArrowDown, Modifiers::NONE));

    assert_eq!(app.last_clicked_idx, Some(1));
}

#[test]
fn home_and_end_are_blocked_during_inline_edit() {
    let ctx = egui::Context::default();
    let mut app = app_with_files(&["/tmp/a.txt", "/tmp/b.txt", "/tmp/c.txt"]);
    begin_edit(&mut app);

    run_frame(&ctx, &mut app, press(Key::Home, Modifiers::NONE));
    run_frame(&ctx, &mut app, press(Key::End, Modifiers::NONE));

    assert_eq!(
        app.last_clicked_idx,
        Some(0),
        "Home/End during an F2 edit belong to the text caret"
    );
}

#[test]
fn shift_arrow_range_extend_is_blocked_during_inline_edit() {
    let ctx = egui::Context::default();
    let mut app = app_with_files(&["/tmp/a.txt", "/tmp/b.txt", "/tmp/c.txt"]);
    begin_edit(&mut app);

    run_frame(&ctx, &mut app, press(Key::ArrowDown, Modifiers::SHIFT));

    assert_eq!(
        app.selection,
        vec![true, false, false],
        "Shift+↓ during an F2 edit extends the text selection, not the file range"
    );
}

// ── Ctrl+Z ──

/// Rename `a.txt` → `b.txt` through the inline-edit path, leaving one entry in
/// the undo history. The caller must keep the returned `TempDir` alive.
fn app_with_one_recorded_rename() -> (TempDir, GuiApp, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let original = dir.path().join("a.txt");
    File::create(&original).unwrap();
    let renamed = dir.path().join("b.txt");

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.all_files = vec![original.clone()];
    app.selection = vec![true];
    app.preview_active = true;

    app.start_edit(0);
    app.edit_buffer = "b.txt".into();
    app.commit_edit();
    assert!(renamed.exists(), "precondition: rename happened");

    (dir, app, original, renamed)
}

#[test]
fn ctrl_z_during_inline_edit_does_not_undo_file_rename() {
    let (_dir, mut app, original, renamed) = app_with_one_recorded_rename();

    // Start a fresh edit, then press Ctrl+Z: it must undo the *typing*, not the
    // recorded file rename.
    begin_edit(&mut app);

    let ctx = egui::Context::default();
    run_frame(&ctx, &mut app, press(Key::Z, Modifiers::COMMAND));

    assert!(
        renamed.exists() && !original.exists(),
        "Ctrl+Z during an F2 edit must not undo a file rename"
    );
}

/// Positive control for the test above: with no active edit, the very same key
/// and history *must* undo the rename.
#[test]
fn ctrl_z_without_edit_undoes_file_rename() {
    let (_dir, mut app, original, renamed) = app_with_one_recorded_rename();
    assert!(app.editing_idx.is_none());

    let ctx = egui::Context::default();
    run_frame(&ctx, &mut app, press(Key::Z, Modifiers::COMMAND));

    assert!(
        original.exists() && !renamed.exists(),
        "Ctrl+Z with no active edit should undo the file rename"
    );
}

// ── Rescan invalidation ──

#[test]
fn rescan_cancels_inline_edit() {
    let dir = TempDir::new().unwrap();
    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.cwd_input = dir.path().to_string_lossy().to_string();
    app.all_files = vec![dir.path().join("a.txt")];
    app.selection = vec![false];
    begin_edit(&mut app);
    assert!(app.editing_idx.is_some());

    // A rescan replaces `all_files`; keeping `editing_idx` would rebind the
    // editor (and a later commit) to whatever now sits at that index.
    app.scan_dir();

    assert_eq!(app.editing_idx, None, "a rescan must drop the inline edit");
    assert!(app.edit_buffer.is_empty());
    assert!(!app.edit_pending_focus);
}

#[test]
fn rescan_then_commit_does_not_rename_a_different_file() {
    let dir = TempDir::new().unwrap();
    for name in ["a.txt", "b.txt"] {
        File::create(dir.path().join(name)).unwrap();
    }

    let mut app = GuiApp::new_with_config_for_test();
    app.cwd = dir.path().to_path_buf();
    app.cwd_input = dir.path().to_string_lossy().to_string();
    app.all_files = vec![dir.path().join("a.txt"), dir.path().join("b.txt")];
    app.selection = vec![false, false];
    begin_edit(&mut app);
    app.edit_buffer = "renamed.txt".into();

    // F5-style rescan while editing, then the scan's result lands with a
    // listing where index 0 is a *different* file. Commit (as Enter would).
    // Without the guard the stale index would rename b.txt instead of a.txt.
    app.scan_dir();
    app.all_files = vec![dir.path().join("b.txt")];
    app.commit_edit();

    assert!(dir.path().join("a.txt").exists());
    assert!(
        dir.path().join("b.txt").exists(),
        "a stale edit index must not rename whatever file now sits at that index"
    );
    assert!(!dir.path().join("renamed.txt").exists());
}
