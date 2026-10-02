use super::*;

pub(super) fn draw_section_extension(
    ui: &mut egui::Ui,
    en: &mut SectionEnabled,
    d: &mut bool,
    sec_order: &dyn Fn(SectionId) -> String,
    c: &mut RenameConfig,
) {
    sec_group!(
        ui,
        en,
        d,
        sec_order,
        c,
        Extension,
        "Extension",
        |ui, _en| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Mode:").size(FONT_LABEL));
                let en = [
                    "Same", "Upper", "Lower", "Title", "Fixed", "Extra", "Remove",
                ];
                let idx = if c.extension.extension_remove { 6 }
                    else if c.extension.extension_append.is_some() { 5 }
                    else if c.extension.extension_replace.is_some() { 4 }
                    else if c.extension.extension_mode == Some(ExtensionMode::Title) { 3 }
                    else if c.extension.extension_mode == Some(ExtensionMode::Lower) { 2 }
                    else if c.extension.extension_mode == Some(ExtensionMode::Upper) { 1 }
                    else { 0 };
                egui::ComboBox::from_id_salt("extm")
                    .selected_text(en[idx])
                    .width(dyn_combo_width(ui, &en))
                    .show_ui(ui, |ui| {
                        for (i, _) in en.iter().enumerate() {
                            if ui.selectable_label(idx == i, en[i]).clicked() {
                                match i {
                                    1 => { c.extension.extension_mode = Some(ExtensionMode::Upper); c.extension.extension_replace = None; c.extension.extension_append = None; c.extension.extension_remove = false; }
                                    2 => { c.extension.extension_mode = Some(ExtensionMode::Lower); c.extension.extension_replace = None; c.extension.extension_append = None; c.extension.extension_remove = false; }
                                    3 => { c.extension.extension_mode = Some(ExtensionMode::Title); c.extension.extension_replace = None; c.extension.extension_append = None; c.extension.extension_remove = false; }
                                    4 => { c.extension.extension_mode = None; c.extension.extension_replace = Some(String::new()); c.extension.extension_append = None; c.extension.extension_remove = false; }
                                    5 => { c.extension.extension_mode = None; c.extension.extension_replace = None; c.extension.extension_append = Some(String::new()); c.extension.extension_remove = false; }
                                    6 => { c.extension.extension_mode = None; c.extension.extension_replace = None; c.extension.extension_append = None; c.extension.extension_remove = true; }
                                    _ => { c.extension.extension_mode = None; c.extension.extension_replace = None; c.extension.extension_append = None; c.extension.extension_remove = false; }
                                }
                                *d = true;
                            }
                        }
                    })
                    .response
                    .on_hover_text("Extension action: Lower/Upper/Title case, Fixed replacement, Extra append, or Remove entirely");
                {
                    let val_enabled = idx == 4 || idx == 5;
                    let is_replace = idx == 4;
                    let val = if is_replace {
                        c.extension.extension_replace.as_deref().unwrap_or("").to_string()
                    } else {
                        c.extension.extension_append.as_deref().unwrap_or("").to_string()
                    };
                    let mut buf = val;
                    ui.add_enabled_ui(val_enabled, |ui| {
                        if fixed_text(ui, [80.0, 20.0], &mut buf)
                            .on_hover_text("Extension value: new extension for 'Fixed', text to append for 'Extra'")
                            .changed()
                        {
                            if is_replace {
                                c.extension.extension_replace = if buf.is_empty() { Some(String::new()) } else { Some(buf.clone()) };
                            } else {
                                c.extension.extension_append = if buf.is_empty() { Some(String::new()) } else { Some(buf.clone()) };
                            }
                            *d = true;
                        }
                    });
                }
            });
            // extension_add_if_missing intentionally omitted — overlaps with Replace/Append mode
        },
        {
            reset_section(c, SectionId::Extension);
        }
    );
}
