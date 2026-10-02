use super::*;

pub(super) fn handle_console_keys(key: KeyEvent, app: &mut App) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            if app.editing_value.is_some() {
                app.editing_value = None;
                app.editing_config_key = None;
                app.set_status("Editing cancelled".to_string(), StatusSeverity::Info);
            }
            app.input.clear();
            app.input_cursor_position = 0;
            app.mode = AppMode::Navigator;
        }
        KeyCode::Up => {
            if app.console_phantom_line {
                // Return to draft
                app.input = app.stash_input.clone();
                app.input_cursor_position = app.input.len();
                app.console_phantom_line = false;
                return Some(Action::None);
            }
            if !app.history.is_empty() {
                if app.history_index == app.history.len() {
                    app.stash_input = app.input.clone();
                }
                if app.history_index > 0 {
                    app.history_index -= 1;
                    app.input = app.history[app.history_index].clone();
                    app.input_cursor_position = app.input.len();
                }
            }
        }
        KeyCode::Down => {
            if !app.history.is_empty() {
                if app.history_index < app.history.len() {
                    app.history_index += 1;
                    if app.history_index == app.history.len() {
                        // Returning to bottom
                        // If we were editing a draft (phantom line), we should restore it?
                        // No, phantom line is only when we are AT bottom and go DOWN.
                        // Here we are just navigating history.
                        // Restore stash if any
                        app.input = app.stash_input.clone();
                        app.input_cursor_position = app.input.len();
                    } else {
                        app.input = app.history[app.history_index].clone();
                        app.input_cursor_position = app.input.len();
                    }
                } else {
                    // Already at bottom (empty line or draft)
                    if !app.input.is_empty() && !app.console_phantom_line {
                        // User wants to start a NEW line below this draft
                        app.stash_input = app.input.clone();
                        app.input.clear();
                        app.input_cursor_position = 0;
                        app.console_phantom_line = true;
                    }
                }
            } else {
                // History empty
                if !app.input.is_empty() && !app.console_phantom_line {
                    app.stash_input = app.input.clone();
                    app.input.clear();
                    app.input_cursor_position = 0;
                    app.console_phantom_line = true;
                }
            }
        }
        KeyCode::Enter => {
            // Inline editing: save value directly
            if let Some((ref key, _)) = app.editing_value.clone() {
                let new_value = app.input.clone();
                if app.stacking_mode {
                    // Update the specific RenameItem in command_order
                    if let Some(idx_str) = key.strip_prefix("step_")
                        && let Ok(idx) = idx_str.parse::<usize>()
                        && idx < app.pending_config.command_order.len()
                    {
                        let item = app.pending_config.command_order[idx].clone();
                        match update_item_value(&item, &new_value) {
                            Ok(new_item) => {
                                app.pending_config.command_order[idx] = new_item;
                                app.set_status(
                                    format!("Updated step {}: {}", idx + 1, new_value),
                                    StatusSeverity::Success,
                                );
                            }
                            Err(e) => {
                                app.set_status(format!("Invalid value: {e}"), StatusSeverity::Error)
                            }
                        }
                    }
                } else {
                    // Update named field on RenameConfig
                    match update_config_field(&mut app.pending_config, key, &new_value) {
                        Ok(()) => app.set_status(
                            format!("Updated {}: {}", key, new_value),
                            StatusSeverity::Success,
                        ),
                        Err(e) => {
                            app.set_status(format!("Invalid value: {e}"), StatusSeverity::Error)
                        }
                    }
                }
                app.editing_value = None;
                app.editing_config_key = None;
                app.compiled_pending = CompiledConfig::new(app.pending_config.clone());
                app.update_preview();
                app.input.clear();
                app.input_cursor_position = 0;
                return Some(Action::None);
            }

            let cmd = app.input.clone();
            if !cmd.trim().is_empty() {
                app.history.push(cmd.clone());
                app.history_index = app.history.len();
                app.stash_input.clear();
            }

            // Console commands are recognized through the SSoT registry
            // (parser::CONSOLE_COMMANDS) so a command can never exist without
            // being declared — and every declared command has help text.
            let base = cmd.split_whitespace().next().unwrap_or("");
            let events_command =
                parser::find(base).filter(|m| m.handled_by == parser::HandledBy::Events);
            let recognized = events_command.is_some_and(|m| {
                cmd == m.name
                    || m.aliases.contains(&cmd.as_str())
                    || (m.name == "apply"
                        && (cmd.starts_with("apply --") || cmd.starts_with("proceed --")))
            });

            if recognized {
                // Safe: `recognized` implies a matching registry entry.
                let meta = events_command.expect("recognized events command has an entry");
                match meta.name {
                    "stacking" => {
                        app.toggle_stacking_mode();
                        app.set_status(
                            format!(
                                "Stacking Mode: {}",
                                if app.stacking_mode { "ON" } else { "OFF" }
                            ),
                            StatusSeverity::Info,
                        );
                        app.compiled_pending = CompiledConfig::new(app.pending_config.clone());
                        app.update_preview();
                        return Some(Action::None);
                    }
                    "apply" => {
                        // Extract values before creating options to avoid borrow issues
                        let active_config = app.pending_config.clone();
                        app.active_config = active_config.clone();
                        app.compiled_active = CompiledConfig::new(active_config.clone());

                        let dry_run = app.dry_run;
                        let undo_file = app.undo_file.clone();
                        let output_dir = active_config
                            .copy_to
                            .output_dir
                            .as_deref()
                            .map(|d| awara::resolve_output_dir(d, &app.cwd));
                        let set_attributes = active_config.special.set_attributes.clone();
                        let dirname_level = active_config.append_folder.dirname_level;
                        let stop_on_error = active_config.stop_on_error;
                        let keep_structure = active_config.copy_to.keep_structure;
                        let base_dir = if keep_structure {
                            Some(app.cwd.clone())
                        } else {
                            None
                        };

                        // Parse flags from command
                        let wants_overwrite = cmd.contains("--overwrite");
                        let wants_skip = cmd.contains("--skip");

                        // Collect files
                        let files: Vec<std::path::PathBuf> = if app.selection.is_empty() {
                            app.navigator_entries.clone()
                        } else {
                            app.selection.iter().cloned().collect()
                        };

                        let sorted_files: Vec<String> = files
                            .iter()
                            .map(|p| p.to_string_lossy().to_string())
                            .collect();

                        // Build the plan once: collision awareness and execution
                        // read the same ops (SSoT).
                        let plan_opts = awara::PlanOptions {
                            dirname_level,
                            output_dir: output_dir.as_deref(),
                            keep_structure,
                            base_dir: base_dir.as_deref(),
                            parallel: false,
                            copy_mode: output_dir.is_some() && active_config.copy_to.copy_mode,
                        };
                        let plan = awara::plan_renames(&sorted_files, &active_config, &plan_opts);
                        let collisions = plan.collisions();
                        let collision_count = collisions.len();
                        let folder_count =
                            collisions.iter().filter(|c| c.folder_involved()).count();

                        // Handle collision strategy. Like the GUI's collision dialog,
                        // don't proceed until the user picks a handling — unless they
                        // passed a flag that applies one to all collisions.
                        let collision_strategy = if wants_overwrite {
                            CollisionStrategy::Overwrite
                        } else if wants_skip {
                            CollisionStrategy::Skip
                        } else if collision_count > 0 {
                            let folder_note = if folder_count > 0 {
                                format!(
                                    " {} of them involve a folder and will be skipped.",
                                    folder_count
                                )
                            } else {
                                String::new()
                            };
                            app.set_status(
                        format!(
                            "{} collision(s).{} Use `proceed --overwrite` to overwrite all or `proceed --skip` to skip them.",
                            collision_count, folder_note
                        ),
                        StatusSeverity::Warning,
                    );
                            app.input.clear();
                            app.input_cursor_position = 0;
                            return Some(Action::None);
                        } else {
                            CollisionStrategy::Skip
                        };

                        let warnings: std::cell::RefCell<Vec<String>> =
                            std::cell::RefCell::new(Vec::new());
                        let mut options = RenameOptions {
                            dry_run,
                            stop_on_error,
                            preserve_timestamps: active_config.preserve_timestamps,
                            set_attributes: set_attributes.as_deref(),
                            undo_file: undo_file.as_deref(),
                            cancel_token: None,
                            on_event: Some(Box::new(|event| {
                                if let awara::RenameEvent::Status { message } = event {
                                    warnings.borrow_mut().push(message);
                                }
                            })),
                        };

                        let resolution = awara::Resolution::default_for(collision_strategy);
                        let result =
                            awara::execute_plan(plan, &active_config, &mut options, &resolution);
                        drop(options);
                        let warnings = warnings.into_inner();

                        let dry_run_tag = if dry_run { " (dry-run)" } else { "" };
                        let collision_tag = if collision_count > 0 {
                            format!(", {} collision(s)", collision_count)
                        } else {
                            String::new()
                        };
                        let has_errors = !result.failed_ops.is_empty();
                        let severity = if has_errors {
                            StatusSeverity::Warning
                        } else {
                            StatusSeverity::Success
                        };
                        let mut message = format!(
                            "Applied{}: {} renamed, {} errors, {} skipped{}",
                            dry_run_tag,
                            result.successful_ops.len(),
                            result.failed_ops.len(),
                            result.skipped_ops.len(),
                            collision_tag
                        );
                        if !warnings.is_empty() {
                            message.push_str(" — ");
                            message.push_str(&warnings.join("; "));
                        }
                        app.set_status(message, severity);
                        app.refresh();
                    }
                    "preview" => {
                        app.compiled_pending = CompiledConfig::new(app.pending_config.clone());
                        app.update_preview();
                        app.set_status("Preview updated".to_string(), StatusSeverity::Success);
                    }
                    "exit" => {
                        app.should_quit = true;
                        return Some(Action::Quit);
                    }
                    "help" => {
                        app.help_state.select(Some(0));
                        app.mode = AppMode::Help;
                    }
                    other => {
                        unreachable!("events command '{other}' is declared but has no handler here")
                    }
                }
            } else if app.stacking_mode {
                // `reset` must clear the staged operations even in stacking mode. The
                // parser mutates its own scratch config, so handle it against the
                // pending config directly, matching the command's "Reset all pending
                // config to defaults" help entry and the normal-mode path below.
                if base == "reset" {
                    app.pending_config = awara::RenameConfig::empty();
                    app.compiled_pending = CompiledConfig::new(app.pending_config.clone());
                    app.update_preview();
                    app.refresh();
                    app.set_status("Command list cleared".to_string(), StatusSeverity::Success);
                    return Some(Action::None);
                }
                // Stacking mode: parse via parser (SSoT), then convert to RenameItems
                use awara::config_to_items;
                let mut temp = awara::RenameConfig::default();
                match parser::parse_command(&cmd, &mut temp) {
                    Ok(_actions) => {
                        let items = config_to_items(&temp);
                        if !items.is_empty() {
                            // Check if we're editing an existing stack item
                            let is_edit = app
                                .editing_config_key
                                .as_ref()
                                .map(|k| k.starts_with("step_"))
                                .unwrap_or(false);

                            if is_edit
                                && let Some(key) = &app.editing_config_key
                                && let Ok(idx) = key.trim_start_matches("step_").parse::<usize>()
                                && idx < app.pending_config.command_order.len()
                            {
                                // Replace with first item, then append rest
                                let count = items.len();
                                if count > 0 {
                                    app.pending_config.command_order[idx] = items[0].clone();
                                    for item in &items[1..] {
                                        app.pending_config.command_order.push(item.clone());
                                    }
                                    app.set_status(
                                        format!(
                                            "Updated command step {}.<- {} item(s)",
                                            idx + 1,
                                            count
                                        ),
                                        StatusSeverity::Success,
                                    );
                                }
                                app.editing_config_key = None;
                            } else {
                                let count = items.len();
                                for item in items {
                                    app.pending_config.command_order.push(item);
                                }
                                app.set_status(
                                    format!(
                                        "Added {} command(s). Stack size: {}",
                                        count,
                                        app.pending_config.command_order.len()
                                    ),
                                    StatusSeverity::Success,
                                );
                            }
                        }
                        // In stacking mode, non-flag actions (cd, sort, etc.) are irrelevant
                        // since we're only building command_order
                        app.compiled_pending = CompiledConfig::new(app.pending_config.clone());
                        app.update_preview();
                    }
                    Err(e) => app.set_status(e.to_string(), StatusSeverity::Error),
                }
            } else {
                // Normal mode: parse command, fully replaces pending_config
                match parser::parse_command(&cmd, &mut app.pending_config) {
                    Ok(actions) => {
                        for action in actions {
                            match action {
                                parser::ParseAction::UpdateConfig(_msg) => {
                                    app.set_status(
                                        App::describe_command(&cmd),
                                        StatusSeverity::Info,
                                    );
                                    app.compiled_pending =
                                        CompiledConfig::new(app.pending_config.clone());
                                    app.update_preview();
                                    app.refresh(); // re-scan with new filter settings
                                }
                                parser::ParseAction::ChangeDir(path) => {
                                    let new_path = if path.is_absolute() {
                                        path
                                    } else {
                                        app.cwd.join(path)
                                    };

                                    match awara::validate_input_path_as(
                                        &new_path.to_string_lossy(),
                                        true, /* expect_dir */
                                    ) {
                                        awara::InputPathValidation::Ok(canon) => {
                                            app.cwd = canon;
                                            app.refresh();
                                            app.selection.clear();
                                            app.navigator_state.select(Some(0));
                                            app.set_status(
                                                format!("Changed dir to {:?}", app.cwd),
                                                StatusSeverity::Success,
                                            );
                                        }
                                        awara::InputPathValidation::Empty => {
                                            app.set_status(
                                                "Empty path".to_string(),
                                                StatusSeverity::Warning,
                                            );
                                        }
                                        awara::InputPathValidation::WrongType => {
                                            app.set_status(
                                                format!("Not a directory: {:?}", new_path),
                                                StatusSeverity::Warning,
                                            );
                                        }
                                        awara::InputPathValidation::NotFound => {
                                            app.set_status(
                                                format!("Directory not found: {:?}", new_path),
                                                StatusSeverity::Warning,
                                            );
                                        }
                                        awara::InputPathValidation::NotAccessible(e) => {
                                            app.set_status(
                                                format!("Cannot access '{:?}': {}", new_path, e),
                                                StatusSeverity::Warning,
                                            );
                                        }
                                        awara::InputPathValidation::PathTooLong {
                                            length,
                                            limit,
                                        } => {
                                            app.set_status(
                                                format!(
                                                    "Path too long: {} chars (limit is {})",
                                                    length, limit
                                                ),
                                                StatusSeverity::Warning,
                                            );
                                        }
                                        awara::InputPathValidation::ComponentTooLong {
                                            component,
                                            length,
                                            limit,
                                        } => {
                                            app.set_status(
                                                format!(
                                                    "Component '{}' is {} bytes (limit is {})",
                                                    component, length, limit
                                                ),
                                                StatusSeverity::Warning,
                                            );
                                        }
                                        awara::InputPathValidation::TrailingDotOrSpace => {
                                            app.set_status(
                                                format!(
                                                    "Path ends with a space or dot: {:?}",
                                                    new_path
                                                ),
                                                StatusSeverity::Warning,
                                            );
                                        }
                                        other => {
                                            app.set_status(
                                                format!(
                                                    "Invalid path: {:?} ({:?})",
                                                    new_path, other
                                                ),
                                                StatusSeverity::Warning,
                                            );
                                        }
                                    }
                                }
                                parser::ParseAction::UpdateSort(mode) => {
                                    app.sort_mode = mode;
                                    app.refresh();
                                    app.set_status(
                                        format!("Sort mode: {:?}", mode),
                                        StatusSeverity::Info,
                                    );
                                }
                                parser::ParseAction::UpdateFilter(pat) => {
                                    app.filter_pattern = pat.clone();
                                    app.refresh();
                                    app.set_status(
                                        format!("Filter: {:?}", pat),
                                        StatusSeverity::Info,
                                    );
                                }
                                parser::ParseAction::SavePreset(path) => {
                                    match awara::RenameDoc::new(app.pending_config.clone())
                                        .to_json_pretty()
                                    {
                                        Ok(json) => {
                                            if std::fs::write(&path, &json).is_ok() {
                                                app.set_status(
                                                    format!("Preset saved to {}", path),
                                                    StatusSeverity::Success,
                                                );
                                            } else {
                                                app.set_status(
                                                    format!("Failed to save preset to {}", path),
                                                    StatusSeverity::Error,
                                                );
                                            }
                                        }
                                        Err(e) => app.set_status(
                                            format!("Failed to serialize: {}", e),
                                            StatusSeverity::Error,
                                        ),
                                    }
                                }
                                parser::ParseAction::LoadPreset(path) => {
                                    match awara::load_rename_doc_with_warnings(
                                        std::path::Path::new(&path),
                                    ) {
                                        Ok(loaded) => {
                                            // Disabled sections are honored by clearing
                                            // them to defaults.
                                            app.pending_config =
                                                loaded.doc.effective_config().into_owned();
                                            app.compiled_pending =
                                                CompiledConfig::new(app.pending_config.clone());
                                            app.update_preview();
                                            let status = if loaded.warnings.is_empty() {
                                                format!("Preset loaded from {}", path)
                                            } else {
                                                format!(
                                                    "Preset loaded from {} ({} value(s) cleaned)",
                                                    path,
                                                    loaded.warnings.len()
                                                )
                                            };
                                            app.set_status(status, StatusSeverity::Success);
                                        }
                                        Err(e) => app.set_status(
                                            format!("Failed to load preset: {}", e),
                                            StatusSeverity::Error,
                                        ),
                                    }
                                }
                                parser::ParseAction::SetUndoFile(Some(path)) => {
                                    app.undo_file = Some(path.clone());
                                    app.set_status(
                                        format!("Undo file set to {}", path),
                                        StatusSeverity::Success,
                                    );
                                }
                                parser::ParseAction::SetUndoFile(None) => {
                                    app.undo_file = None;
                                    app.set_status(
                                        "Undo file disabled".to_string(),
                                        StatusSeverity::Info,
                                    );
                                }
                                parser::ParseAction::ToggleDryRun => {
                                    app.dry_run = !app.dry_run;
                                    app.set_status(
                                        format!(
                                            "Dry-run: {}",
                                            if app.dry_run { "ON" } else { "OFF" }
                                        ),
                                        StatusSeverity::Info,
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => app.set_status(e.to_string(), StatusSeverity::Error),
                }
            }
            app.input.clear();
            app.input_cursor_position = 0;
        }
        KeyCode::Char(c) => {
            if app.console_phantom_line {
                app.stash_input.clear(); // Discard previous draft
                app.console_phantom_line = false;
            }
            app.input.insert(app.input_cursor_position, c);
            app.input_cursor_position += 1;
        }
        KeyCode::Backspace => {
            if app.console_phantom_line {
                app.stash_input.clear();
                app.console_phantom_line = false;
            }
            if app.input_cursor_position > 0 {
                app.input.remove(app.input_cursor_position - 1);
                app.input_cursor_position -= 1;
            }
        }
        KeyCode::Delete => {
            if app.console_phantom_line {
                app.stash_input.clear();
                app.console_phantom_line = false;
            }
            if app.input_cursor_position < app.input.len() {
                app.input.remove(app.input_cursor_position);
            }
        }
        KeyCode::Left => {
            if app.input_cursor_position > 0 {
                app.input_cursor_position -= 1;
            }
        }
        KeyCode::Right => {
            if app.input_cursor_position < app.input.len() {
                app.input_cursor_position += 1;
            }
        }
        KeyCode::Tab => {
            // Tab completion for cd command (basic: matches subdirs in cwd)
            if let Some(partial) = app.input.strip_prefix("cd ").map(|s| s.trim())
                && let Ok(read_dir) = std::fs::read_dir(&app.cwd)
            {
                let mut matches: Vec<String> = read_dir
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                    .filter(|name| name.starts_with(partial))
                    .collect();
                matches.sort();
                if matches.len() == 1 {
                    app.input = format!("cd {}/", matches[0]);
                    app.input_cursor_position = app.input.len();
                    app.set_status(
                        format!("Completed: {}", matches[0]),
                        StatusSeverity::Success,
                    );
                } else if matches.len() > 1 {
                    app.set_status(
                        format!("Matches: {}", matches.join(", ")),
                        StatusSeverity::Info,
                    );
                }
            }
        }
        _ => {}
    }
    Some(Action::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use awara::RenameItem;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    /// `reset` is documented as clearing the staged operations ("Command list
    /// cleared"); it must do so in stacking mode too, not just normal mode.
    #[test]
    fn reset_clears_staged_commands_in_stacking_mode() {
        let mut app = App::new();
        app.stacking_mode = true;
        app.mode = AppMode::Console;
        app.pending_config
            .command_order
            .push(RenameItem::RemoveDigits);
        app.input = "reset".to_string();
        app.input_cursor_position = app.input.len();

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let _ = handle_console_keys(key, &mut app);

        assert!(
            app.pending_config.command_order.is_empty(),
            "reset must clear the staged command list in stacking mode"
        );
    }
}
