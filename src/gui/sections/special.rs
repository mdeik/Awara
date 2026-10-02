use super::*;

pub(super) fn draw_section_special(
    ui: &mut egui::Ui,
    app: &mut GuiApp,
    sec_order: &dyn Fn(SectionId) -> String,
    ts_clicked: &mut bool,
    order_status_str: &str,
) {
    let c = &mut app.config;
    let d = &mut app.preview_dirty;
    let en = &mut app.section_enabled;
    sec_group!(
        ui,
        en,
        d,
        sec_order,
        c,
        Special,
        "Special",
        |ui, _en| {
            egui::Grid::new("special_grid")
                .min_col_width(0.0)
                .show(ui, |ui| {
                    // Row 1: Order button | Attributes button | Timestamps button
                    if ui
                        .button("▦ Order")
                        .on_hover_text("Reorder the execution order of rename operation sections")
                        .clicked()
                    {
                        app.active_popup = Some(PopupKind::OrderEditor);
                    }
                    if ui
                        .button("⚙ Attributes")
                        .on_hover_text(
                            "Set file attributes (readonly, hidden, system, archive) after rename",
                        )
                        .clicked()
                    {
                        app.active_popup = Some(PopupKind::AttrEditor);
                    }
                    if ui
                        .button("🕒 Timestamps")
                        .on_hover_text(
                            "Set file timestamps (creation, modification, access) after rename",
                        )
                        .clicked()
                    {
                        *ts_clicked = true;
                    }
                    ui.end_row();

                    // Row 2: status labels
                    let order_status = order_status_str;
                    let attr_status = if c.special.set_attributes.is_some() {
                        "set"
                    } else {
                        "not set"
                    };
                    let ts_status = if c.special.set_created.is_some()
                        || c.special.set_modified.is_some()
                        || c.special.set_accessed.is_some()
                    {
                        "set"
                    } else {
                        "not set"
                    };
                    let muted = ui.style().visuals.text_color().gamma_multiply(0.5);
                    ui.label(RichText::new(order_status).size(FONT_LABEL).color(muted));
                    ui.label(RichText::new(attr_status).size(FONT_LABEL).color(muted));
                    ui.label(RichText::new(ts_status).size(FONT_LABEL).color(muted));
                    ui.end_row();
                });
            // undo_file intentionally omitted — CLI-only (undo is written to cwd on execution)
        },
        {
            reset_section(c, SectionId::Special);
        }
    );

    ui.add_space(6.0);
}
