use super::*;

pub(super) fn handle_navigator_keys(key: KeyEvent, app: &mut App) -> Option<Action> {
    // Cancel scan in progress
    if key.code == KeyCode::Esc && app.scanning_in_progress {
        app.cancel_scan();
        app.set_status("Scan cancelled".to_string(), StatusSeverity::Info);
        return Some(Action::None);
    }

    match key.code {
        KeyCode::Char('q') => return Some(Action::Quit),
        KeyCode::Down | KeyCode::Char('j') => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.next_nav();
                if let Some(i) = app.navigator_state.selected()
                    && let Some(path) = app.navigator_entries.get(i)
                {
                    app.selection.insert(path.clone());
                    app.update_preview();
                }
            } else {
                app.next_nav();
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.previous_nav();
                if let Some(i) = app.navigator_state.selected()
                    && let Some(path) = app.navigator_entries.get(i)
                {
                    app.selection.insert(path.clone());
                    app.update_preview();
                }
            } else {
                app.previous_nav();
            }
        }
        KeyCode::Enter => app.enter_dir(),
        KeyCode::Backspace => app.go_up(),
        KeyCode::Char(' ') => app.toggle_selection(),
        KeyCode::Char('a') => {
            for path in &app.navigator_entries {
                app.selection.insert(path.clone());
            }
            app.update_preview();
        }
        KeyCode::Char('A') => {
            app.selection.clear();
            app.update_preview();
        }
        KeyCode::Char('r') => {
            app.refresh();
        }
        KeyCode::Char('R') => app.toggle_recursive(),
        KeyCode::Char('H') => app.toggle_hidden(),
        KeyCode::Char('s') => app.cycle_sort(),
        KeyCode::Char('S') => {
            // Toggle Asc/Desc
            app.sort_mode = match app.sort_mode {
                ScanSortBy::Name => ScanSortBy::NameDesc,
                ScanSortBy::NameDesc => ScanSortBy::Name,
                ScanSortBy::Date => ScanSortBy::DateDesc,
                ScanSortBy::DateDesc => ScanSortBy::Date,
                ScanSortBy::Size => ScanSortBy::SizeDesc,
                ScanSortBy::SizeDesc => ScanSortBy::Size,
                ScanSortBy::Extension => ScanSortBy::ExtensionDesc,
                ScanSortBy::ExtensionDesc => ScanSortBy::Extension,
            };
            app.refresh();
        }
        KeyCode::Char('F') => app.cycle_filter(),
        _ => {}
    }
    Some(Action::None)
}

pub(super) fn handle_preview_keys(key: KeyEvent, app: &mut App) -> Option<Action> {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            let i = match app.preview_state.selected() {
                Some(i) => {
                    if i >= app.preview_items.len() - 1 {
                        0
                    } else {
                        i + 1
                    }
                }
                None => 0,
            };
            app.preview_state.select(Some(i));
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let i = match app.preview_state.selected() {
                Some(i) => {
                    if i == 0 {
                        app.preview_items.len() - 1
                    } else {
                        i - 1
                    }
                }
                None => 0,
            };
            app.preview_state.select(Some(i));
        }
        KeyCode::Char('s') => app.cycle_sort(),
        KeyCode::Char('S') => {
            // Toggle Asc/Desc
            app.sort_mode = match app.sort_mode {
                ScanSortBy::Name => ScanSortBy::NameDesc,
                ScanSortBy::NameDesc => ScanSortBy::Name,
                ScanSortBy::Date => ScanSortBy::DateDesc,
                ScanSortBy::DateDesc => ScanSortBy::Date,
                ScanSortBy::Size => ScanSortBy::SizeDesc,
                ScanSortBy::SizeDesc => ScanSortBy::Size,
                ScanSortBy::Extension => ScanSortBy::ExtensionDesc,
                ScanSortBy::ExtensionDesc => ScanSortBy::Extension,
            };
            app.refresh();
        }
        _ => {}
    }
    Some(Action::None)
}

pub(super) fn handle_config_keys(key: KeyEvent, app: &mut App) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            if let Some(start_order) = &app.config_drag_start_order {
                // Cancel drag, restore order
                app.config_display_order = start_order.clone();
                app.config_drag_start_order = None;
                app.config_drag_index = None;
                app.set_status("Reordering cancelled".to_string(), StatusSeverity::Info);
            } else {
                app.mode = AppMode::Navigator;
                app.config_drag_index = None;
            }
        }
        KeyCode::Char(' ') => {
            if let Some(i) = app.config_state.selected() {
                if app.config_drag_index.is_some() {
                    // Cancel drag (Space toggles off, but if we want Space to CANCEL like prompt says:
                    // "When an item is selected and user hits space, it should deselect the item and return to its original position"
                    if let Some(start_order) = &app.config_drag_start_order {
                        app.config_display_order = start_order.clone();
                        app.config_drag_start_order = None;
                        app.config_drag_index = None;
                        app.set_status("Reordering cancelled".to_string(), StatusSeverity::Info);
                    } else {
                        // Should not happen if logic is correct, but safe fallback
                        app.config_drag_index = None;
                    }
                } else {
                    // Start drag
                    app.config_drag_index = Some(i);
                    app.config_drag_start_order = Some(app.config_display_order.clone());
                    app.set_status(
                        "Reordering... Enter to confirm, Space/Esc to cancel".to_string(),
                        StatusSeverity::Info,
                    );
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let items_len = app.get_config_items().len();
            if items_len > 0 {
                if let Some(drag_idx) = app.config_drag_index {
                    if drag_idx < items_len - 1 {
                        app.config_display_order.swap(drag_idx, drag_idx + 1);
                        app.config_drag_index = Some(drag_idx + 1);
                        app.config_state.select(Some(drag_idx + 1));
                    }
                } else {
                    let i = match app.config_state.selected() {
                        Some(i) => {
                            if i >= items_len - 1 {
                                0
                            } else {
                                i + 1
                            }
                        }
                        None => 0,
                    };
                    app.config_state.select(Some(i));
                }
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let items_len = app.get_config_items().len();
            if items_len > 0 {
                if let Some(drag_idx) = app.config_drag_index {
                    if drag_idx > 0 {
                        app.config_display_order.swap(drag_idx, drag_idx - 1);
                        app.config_drag_index = Some(drag_idx - 1);
                        app.config_state.select(Some(drag_idx - 1));
                    }
                } else {
                    let i = match app.config_state.selected() {
                        Some(i) => {
                            if i == 0 {
                                items_len - 1
                            } else {
                                i - 1
                            }
                        }
                        None => 0,
                    };
                    app.config_state.select(Some(i));
                }
            }
        }
        KeyCode::Char('S') => {
            app.toggle_stacking_mode();
            app.compiled_pending = CompiledConfig::new(app.pending_config.clone());
            app.update_preview();
            app.set_status(
                format!(
                    "Stacking Mode: {}",
                    if app.stacking_mode { "ON" } else { "OFF" }
                ),
                StatusSeverity::Info,
            );
        }
        KeyCode::Char('D') => {
            app.dry_run = !app.dry_run;
            app.set_status(
                format!("Dry-run: {}", if app.dry_run { "ON" } else { "OFF" }),
                StatusSeverity::Info,
            );
        }
        KeyCode::Delete | KeyCode::Char('d') => {
            // ... (existing remove logic)
            if let Some(i) = app.config_state.selected() {
                let items = app.get_config_items();
                if let Some((_, _, cmd)) = items.get(i) {
                    if app.stacking_mode {
                        // Remove from command_order
                        if i < app.config_display_order.len() {
                            let key = &app.config_display_order[i];
                            if let Ok(idx) = key.trim_start_matches("step_").parse::<usize>()
                                && idx < app.pending_config.command_order.len()
                            {
                                app.pending_config.command_order.remove(idx);
                                app.config_display_order.clear(); // Regen
                                app.compiled_pending =
                                    CompiledConfig::new(app.pending_config.clone());
                                app.update_preview();
                            }
                        }
                    } else {
                        // Existing logic
                        let key = cmd.split_whitespace().next().unwrap_or("");
                        let clean_key = key.trim_start_matches('-');

                        // Try to clear using clean_key
                        let _ = parser::parse_command(clean_key, &mut app.pending_config);
                        app.compiled_pending = CompiledConfig::new(app.pending_config.clone());
                        app.update_preview();
                    }

                    let new_len = app.get_config_items().len();
                    if new_len == 0 {
                        app.config_state.select(None);
                    } else if i >= new_len {
                        app.config_state.select(Some(new_len - 1));
                    }
                }
            }
        }
        KeyCode::Enter => {
            if app.config_drag_index.is_some() {
                // Confirm move
                if app.stacking_mode {
                    // Update command_order from display_order
                    let mut new_order = Vec::new();
                    for key in &app.config_display_order {
                        if let Ok(idx) = key.trim_start_matches("step_").parse::<usize>()
                            && idx < app.pending_config.command_order.len()
                        {
                            new_order.push(app.pending_config.command_order[idx].clone());
                        }
                    }
                    if !new_order.is_empty() {
                        app.pending_config.command_order = new_order;
                        app.config_display_order.clear(); // Force regen
                        app.compiled_pending = CompiledConfig::new(app.pending_config.clone());
                        app.update_preview();
                    }
                }

                app.config_drag_index = None;
                app.config_drag_start_order = None;
                app.set_status("Reordering confirmed".to_string(), StatusSeverity::Success);
            } else if let Some(i) = app.config_state.selected() {
                let items = app.get_config_items();
                if let Some((label, value, _cmd)) = items.get(i) {
                    // Inline edit: pre-fill input with current value, track the field key
                    app.input = value.clone();
                    app.input_cursor_position = app.input.len();
                    app.mode = AppMode::Console;

                    let key = if app.stacking_mode {
                        if i < app.config_display_order.len() {
                            app.config_display_order[i].clone()
                        } else {
                            format!("step_{}", i)
                        }
                    } else {
                        app.config_display_order.get(i).cloned().unwrap_or_default()
                    };

                    app.editing_value = Some((key, value.clone()));
                    app.editing_config_key =
                        Some(app.config_display_order.get(i).cloned().unwrap_or_default());
                    app.set_status(
                        format!("Editing {}: enter new value (Esc to cancel)", label),
                        StatusSeverity::Info,
                    );
                }
            }
        }
        _ => {}
    }
    Some(Action::None)
}

pub(super) fn handle_help_keys(key: KeyEvent, app: &mut App) -> Option<Action> {
    // Selection is a position in the filtered view (app.help_visible, rebuilt
    // every frame), so navigation is plain index math — every visible position
    // matches the filter by construction, and the position always stays in bounds.
    let len = app.help_visible.len();

    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if len == 0 {
                return Some(Action::None);
            }
            let current = app.help_state.selected().unwrap_or(0);
            app.help_state.select(Some((current + 1) % len));
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if len == 0 {
                return Some(Action::None);
            }
            let current = app.help_state.selected().unwrap_or(0);
            app.help_state
                .select(Some(if current == 0 { len - 1 } else { current - 1 }));
        }
        KeyCode::Char('F') => {
            app.help_cycle_filter();
        }
        KeyCode::Esc => {
            app.help_state.select(Some(0));
            app.help_filter_index = 0;
            app.mode = AppMode::Navigator;
        }
        KeyCode::Char('q') => return Some(Action::Quit),
        _ => {}
    }
    Some(Action::None)
}

/// Parse a numeric item value. Empty clears to 0; bad input is an error.
fn item_num(v: &str) -> Result<isize, String> {
    let t = v.trim();
    if t.is_empty() {
        return Ok(0);
    }
    t.parse().map_err(|_| format!("invalid number '{v}'"))
}

/// Update a single-field RenameItem's value with new text.
pub(super) fn update_item_value(
    item: &awara::RenameItem,
    new_value: &str,
) -> Result<awara::RenameItem, String> {
    let v = new_value.to_string();
    Ok(match item {
        awara::RenameItem::Prefix(_) => awara::RenameItem::Prefix(v),
        awara::RenameItem::Suffix(_) => awara::RenameItem::Suffix(v),
        awara::RenameItem::Regex(_, r, f, simple) => {
            awara::RenameItem::Regex(v, r.clone(), *f, *simple)
        }
        awara::RenameItem::Replace(_, w, c, rf) => {
            awara::RenameItem::Replace(v, w.clone(), *c, *rf)
        }
        awara::RenameItem::RemoveFirst(_) => awara::RenameItem::RemoveFirst(item_num(&v)?),
        awara::RenameItem::RemoveLast(_) => awara::RenameItem::RemoveLast(item_num(&v)?),
        awara::RenameItem::RemoveChars(_) => awara::RenameItem::RemoveChars(v),
        awara::RenameItem::RemoveWords(_, cs) => awara::RenameItem::RemoveWords(v, *cs),
        awara::RenameItem::Insert(_, p) => awara::RenameItem::Insert(v, *p),
        awara::RenameItem::CaseName(m) => awara::RenameItem::CaseName(*m),
        awara::RenameItem::AddDate(_) => awara::RenameItem::AddDate(v),
        awara::RenameItem::AddFileDate(_) => awara::RenameItem::AddFileDate(v),
        awara::RenameItem::InsertMeta(_) => awara::RenameItem::InsertMeta(v),
        awara::RenameItem::RemoveFromTo(_, t) => awara::RenameItem::RemoveFromTo(item_num(&v)?, *t),
        other => other.clone(),
    })
}

/// Update a named config field with a new string value.
/// Delegates to the SSoT in lib.rs; propagates its validation error.
pub(super) fn update_config_field(
    cfg: &mut awara::RenameConfig,
    key: &str,
    value: &str,
) -> Result<(), String> {
    awara::set_config_field(cfg, key, value)
}
