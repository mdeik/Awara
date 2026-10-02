use super::*;

pub(super) fn draw_section_move_copy(
    ui: &mut egui::Ui,
    en: &mut SectionEnabled,
    d: &mut bool,
    sec_order: &dyn Fn(SectionId) -> String,
    c: &mut RenameConfig,
    e: &mut GuiEditState,
) {
    sec_group!(
        ui,
        en,
        d,
        sec_order,
        c,
        MoveCopy,
        "Move / Copy Parts",
        |ui, _en| {
            // Read combo state from GUI edit state
            let action_names = [
                "None",
                "Copy first n",
                "Copy last n",
                "Move first n",
                "Move last n",
                "Copy range",
                "Move range",
            ];
            let action_idx = match e.move_part_action.as_deref() {
                Some("Copy first n") => 1,
                Some("Copy last n") => 2,
                Some("Move first n") => 3,
                Some("Move last n") => 4,
                Some("Copy range") => 5,
                Some("Move range") => 6,
                _ => 0,
            };
            let dest_names = ["None", "To start", "To end", "To pos"];
            let dest_idx = match e.move_part_dest.as_deref() {
                Some("To start") => 1,
                Some("To end") => 2,
                Some("To pos") => 3,
                _ => 0,
            };

            let has_action = action_idx > 0;
            let is_range = action_idx == 5 || action_idx == 6;

            // Helper to compose the move/copy config from edit state fields
            let compose_move_part = |c: &mut RenameConfig, e: &mut GuiEditState| {
                c.move_copy = compose_move_copy_section(e);
            };

            ui.horizontal(|ui| {
                // Action combo
                let aw = dyn_combo_width(ui, &action_names);
                egui::ComboBox::from_id_salt("mv_action")
                    .selected_text(action_names[action_idx])
                    .width(aw)
                    .show_ui(ui, |ui| {
                        for (i, _) in action_names.iter().enumerate() {
                            if ui.selectable_label(action_idx == i, action_names[i]).clicked()
                            {
                                e.move_part_action = if i == 0 { None } else { Some(action_names[i].to_string()) };
                                compose_move_part(c, e);
                                *d = true;
                            }
                        }
                    })
                    .response
                    .on_hover_text("Move or copy a portion of the filename to another position");

                // Value field 1
                let mut val1 = e.move_part_val1;
                let resp = ui.add_enabled(
                    has_action,
                    int_drag(&mut val1)
                        .range(MOVE_COPY_VALUE.0..=MOVE_COPY_VALUE.1),
                ).on_hover_text("Start position (1-based) or number of characters to copy/move (negative counts from end)");
                if resp.changed() {
                    e.move_part_val1 = val1;
                    compose_move_part(c, e);
                    *d = true;
                }

                // Value field 2
                let mut val2 = e.move_part_val2;
                let resp = ui.add_enabled(
                    is_range,
                    int_drag(&mut val2)
                        .range(MOVE_COPY_VALUE.0..=MOVE_COPY_VALUE.1),
                ).on_hover_text("Length of the range to copy/move (negative counts from end)");
                if resp.changed() {
                    e.move_part_val2 = val2;
                    compose_move_part(c, e);
                    *d = true;
                }

                // Destination combo
                ui.add_enabled_ui(has_action, |ui| {
                    egui::ComboBox::from_id_salt("mv_dest")
                        .selected_text(dest_names[dest_idx])
                        .width(dyn_combo_width(ui, &dest_names))
                        .show_ui(ui, |ui| {
                            for (i, _) in dest_names.iter().enumerate() {
                                if ui.selectable_label(dest_idx == i, dest_names[i]).clicked()
                                {
                                    e.move_part_dest = if i == 0 { None } else { Some(dest_names[i].to_string()) };
                                    compose_move_part(c, e);
                                    *d = true;
                                }
                            }
                        })
                        .response
                        .on_hover_text("Where to place the copied/moved substring");
                });

                // Destination value
                let dest_val_enabled = has_action && dest_idx == 3;
                let mut dest_val = e.move_part_dest_val;
                let resp = ui.add_enabled(
                    dest_val_enabled,
                    int_drag(&mut dest_val)
                        .range(MOVE_COPY_VALUE.0..=MOVE_COPY_VALUE.1),
                ).on_hover_text("Target character position to insert the copied/moved text (negative counts from end)");
                if resp.changed() {
                    e.move_part_dest_val = dest_val;
                    compose_move_part(c, e);
                    *d = true;
                }

                // Separator — inserted between the moved/copied
                // part and the rest of the name (stored on the
                // MoveCopyValue, so it survives config export).
                ui.label(RichText::new("Sep:").size(FONT_LABEL));
                let mut sep_buf = e.move_part_sep.clone();
                let _ = ui
                    .add_enabled_ui(has_action, |ui| {
                        fixed_text(ui, [44.0, 22.0], &mut sep_buf)
                            .on_hover_text("Separator inserted between the moved/copied part and the rest of the filename")
                    })
                    .inner;
                if sep_buf != e.move_part_sep {
                    e.move_part_sep = sep_buf;
                    compose_move_part(c, e);
                    *d = true;
                }
            });
        },
        {
            reset_section(c, SectionId::MoveCopy);
            reset_move_copy_edit_state(e);
        }
    );
}
