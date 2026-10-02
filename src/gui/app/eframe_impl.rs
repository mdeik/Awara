use super::*;

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Check if background scan finished
        self.check_scan_results();

        // Auto-clear temporary/error status after their timeout.
        // Schedule a repaint so the timer runs even without user interaction.
        if let Some(timeout) = self.status_kind.timeout()
            && let Some(ts) = self.status_timestamp
        {
            let elapsed = ts.elapsed();
            if elapsed >= timeout {
                self.status_message = self.last_ok_message.clone();
                self.status_kind = StatusKind::Persistent;
                self.status_timestamp = None;
            } else {
                ctx.request_repaint_after(timeout - elapsed);
            }
        }

        // ── Popup management ──
        // Esc: close any open popup, collision dialog, cancel F2 edit,
        // or dismiss properties/trash dialogs.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            if self.editing_idx.is_some() {
                self.cancel_edit();
            } else if self.collision_dialog.is_some() {
                self.collision_dialog = None;
                self.pending_plan = None;
            } else if self.active_popup.is_some() {
                self.active_popup = None;
            } else if self.show_properties_idx.is_some() {
                self.show_properties_idx = None;
            } else if self.show_properties_multi_data.is_some() {
                self.show_properties_multi_data = None;
            } else if self.show_tree_properties.is_some() {
                self.show_tree_properties = None;
            } else if self.show_trash_confirmation {
                self.show_trash_confirmation = false;
                self.trash_target = None;
            } else if self.show_dir_creation_warning {
                self.cancel_dir_creation_warning();
            } else if self.show_invalid_name_warning {
                self.dismiss_invalid_name_warning();
            } else if self.scanning {
                self.cancel_scan();
                // Feedback for the user-initiated cancel; restores the
                // pre-scan status after its normal timeout.
                self.set_status("Scan cancelled", StatusKind::Temporary);
            }
        }

        let has_popup = self.has_any_modal() || ctx.any_popup_open();

        // ── F2: inline rename ──
        if !has_popup
            && self.preview_active
            && !self.all_files.is_empty()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2))
            && let Some(idx) = self.selected_idx()
        {
            self.commit_edit(); // commit any in-flight edit first
            self.start_edit(idx);
        }

        // Tab / Shift+Tab while editing: commit and move to next / prev.
        if !has_popup && self.editing_idx.is_some() {
            let shift_held = ctx.input(|i| i.modifiers.shift);
            let tab_pressed = ctx
                .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
            if tab_pressed {
                self.cycle_edit(!shift_held);
            }
            // Enter while editing: commit and stay.
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                self.commit_edit();
            }
        }

        // ── Preview-section shortcuts ──
        // All require preview_active and no focused text widget. The F2 inline
        // TextEdit counts as focused: while renaming, these shortcuts must not
        // steal keys the TextEdit owns (↑/↓, Home/End, Shift+arrow, Ctrl+Z).
        let focus_free = ctx.memory(|m| m.focused()).is_none() && self.editing_idx.is_none();

        // Ctrl+A/⌘A: select all files (not during F2 edit — captured by TextEdit).
        if !has_popup
            && self.editing_idx.is_none()
            && self.preview_active
            && !self.all_files.is_empty()
            && focus_free
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::A))
        {
            self.select_all();
        }
        // Ctrl+D / ⌘D: deselect all.
        if !has_popup
            && self.preview_active
            && focus_free
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::D))
        {
            self.selection.fill(false);
            self.selection_generation += 1;
        }
        // Ctrl+I / ⌘I: invert selection.
        if !has_popup
            && self.preview_active
            && focus_free
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::I))
        {
            for s in &mut self.selection {
                *s = !*s;
            }
            self.selection_generation += 1;
            // Inverting includes previously-unselected files → re-arm.
            self.set_processed(false);
        }
        // Alt+↑: parent directory (global — does not need preview_active).
        if !has_popup && ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowUp))
        {
            self.go_up();
        }
        // ↑ / ↓: move selection up / down in the preview (navigates by display order).
        // Shift+↑/↓ extends range selection in display order.
        if !has_popup && self.preview_active && focus_free && !self.all_files.is_empty() {
            let shift_down =
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowDown));
            let shift_up =
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowUp));
            if shift_down || shift_up {
                let display_order = self.get_display_order();
                if display_order.is_empty() {
                    return;
                }
                // First shift+arrow after a non-shift action: pin the anchor.
                if self.selection_anchor.is_none() {
                    self.selection_anchor = self.last_clicked_idx;
                }
                // Resolve anchor display position.
                let anchor_pos = self
                    .selection_anchor
                    .and_then(|a| display_order.iter().position(|&i| i == a))
                    .unwrap_or(0);
                // Current active edge (last moved to). If None, use anchor.
                let cur_pos = self
                    .last_clicked_idx
                    .and_then(|a| display_order.iter().position(|&i| i == a))
                    .unwrap_or(anchor_pos);
                // Compute new position.
                let new_pos = if shift_down {
                    (cur_pos + 1).min(display_order.len().saturating_sub(1))
                } else if shift_up && cur_pos > 0 {
                    cur_pos - 1
                } else {
                    cur_pos
                };
                if new_pos != cur_pos {
                    let (lo, hi) = if anchor_pos <= new_pos {
                        (anchor_pos, new_pos)
                    } else {
                        (new_pos, anchor_pos)
                    };
                    select_display_range(&mut self.selection, &display_order, lo, hi);
                    self.last_clicked_idx = Some(display_order[new_pos]);
                    self.scroll_to_idx = Some(display_order[new_pos]);
                    self.scroll_sequence_start
                        .get_or_insert_with(std::time::Instant::now);
                    self.scroll_last_event = Some(std::time::Instant::now());
                    self.selection_generation += 1;
                    // Range selection re-arms the rename guard.
                    self.set_processed(false);
                }
            }
        }

        // Non-shift ↑ / ↓ / PageUp / PageDown / Home / End: single-selection navigation by display order.
        if !has_popup && self.preview_active && focus_free && !self.all_files.is_empty() {
            let arrow_down =
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
            let arrow_up =
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
            let page_down =
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown));
            let page_up =
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp));
            let home = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Home));
            let end = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::End));

            if arrow_down || arrow_up || page_down || page_up || home || end {
                self.selection_anchor = None;
                let display_order = self.get_display_order();
                let cur_pos = self
                    .last_clicked_idx
                    .and_then(|a| display_order.iter().position(|&i| i == a))
                    .unwrap_or(0);
                let target = if arrow_down {
                    if cur_pos + 1 < display_order.len() {
                        Some(display_order[cur_pos + 1])
                    } else {
                        None
                    }
                } else if arrow_up {
                    if cur_pos > 0 {
                        Some(display_order[cur_pos - 1])
                    } else {
                        None
                    }
                } else if page_down {
                    let next = (cur_pos + 25).min(display_order.len().saturating_sub(1));
                    Some(display_order[next])
                } else if page_up {
                    let next = cur_pos.saturating_sub(25);
                    Some(display_order[next])
                } else if home {
                    display_order.first().copied()
                } else if end {
                    display_order.last().copied()
                } else {
                    None
                };
                if let Some(target) = target
                    && target != self.last_clicked_idx.unwrap_or(usize::MAX)
                {
                    self.selection.fill(false);
                    if let Some(s) = self.selection.get_mut(target) {
                        *s = true;
                    }
                    self.last_clicked_idx = Some(target);
                    self.scroll_to_idx = Some(target);
                    self.scroll_sequence_start
                        .get_or_insert_with(std::time::Instant::now);
                    self.scroll_last_event = Some(std::time::Instant::now());
                    self.selection_generation += 1;
                    // Keyboard navigation selects a file → re-arm.
                    self.set_processed(false);
                }
            }
        }
        // Enter: open selected directory / open file with default app.
        if !has_popup
            && self.preview_active
            && focus_free
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
            && self.selected_count() == 1
            && let Some(idx) = self.selected_idx()
            && let Some(path) = self.all_files.get(idx).cloned()
        {
            if path.is_dir() {
                self.enter_dir(&path);
            } else {
                self.open_file_external(idx);
            }
        }

        // F5: refresh current directory + invalidate expanded tree cache
        // so new/removed subdirectories appear in the file tree.
        if !has_popup && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
            let cwd = self.cwd.clone();
            self.invalidate_tree_subtree(&cwd);
            // Re-derive every expand arrow from disk on the next frame too —
            // collapsed folders outside the cwd subtree keep their cached
            // answers otherwise, even when their contents changed externally.
            self.tree_has_subdirs.clear();
            self.scan_dir();
        }
        // Ctrl+F5 / ⌘F5: full refresh (re-scan + re-expand tree).
        if !has_popup && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F5)) {
            self.refresh();
        }

        // Ctrl+Z / ⌘Z: undo the last rename for all selected files.
        // Ctrl+Shift+Z / ⌘Shift+Z: redo for all selected files.
        // Check shift state *before* consuming the key to avoid losing it.
        let cmd_z = ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.command);
        if !has_popup
            && self.preview_active
            && focus_free
            && !self.all_files.is_empty()
            && self.selected_count() > 0
            && cmd_z
        {
            let shift = ctx.input(|i| i.modifiers.shift);
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
            if shift {
                self.redo_files(&self.selected_indices());
            } else {
                self.undo_files(&self.selected_indices());
            }
        }

        // Delete: trash selected files (with confirmation unless skipped).
        // Not during F2 edit — the TextEdit owns Delete to remove a character.
        if !has_popup
            && self.editing_idx.is_none()
            && self.preview_active
            && focus_free
            && !self.all_files.is_empty()
            && self.selected_count() > 0
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
        {
            if self.skip_trash_confirmation {
                match self.trash_selected_multi() {
                    Ok(()) => {}
                    Err(e) => self.set_status(&e, StatusKind::Error),
                }
            } else {
                self.show_trash_confirmation = true;
                self.trash_init_focus = true;
            }
        }

        if self.preview_dirty && self.auto_refresh && !self.scanning && !self.executing {
            self.update_preview();
        }
        crate::gui::ui::render(self, ui);

        // Keep polling while a preview (or row-cache) result is pending.
        // Without this, egui won't schedule further frames after a one-shot
        // interaction (like a section reset button), leaving the UI stale
        // until the user moves the mouse or presses a key.
        if self.preview_pending || self.rows_cache_pending {
            ctx.request_repaint();
        }
    }
}
