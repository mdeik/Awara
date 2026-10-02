use super::*;

pub(super) fn draw_section_append_folder(
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
        AppendFolder,
        "Append Folder Name",
        |ui, _en| {
            ui.horizontal(|ui| {
                let fn_names = ["None", "Prefix", "Suffix"];
                let idx = if !c.append_folder.add_dirname { 0 }
                    else if c.append_folder.add_dirname_pos == Some(0) { 1 }
                    else { 2 };
                egui::ComboBox::from_id_salt("afn")
                    .selected_text(fn_names[idx])
                    .width(dyn_combo_width(ui, &fn_names))
                    .show_ui(ui, |ui| {
                        for (i, n) in fn_names.iter().enumerate() {
                            if ui.selectable_label(idx == i, *n).clicked() {
                                match i {
                                    1 => { c.append_folder.add_dirname = true; c.append_folder.add_dirname_pos = Some(0); }
                                    2 => { c.append_folder.add_dirname = true; c.append_folder.add_dirname_pos = Some(-1); }
                                    _ => { c.append_folder.add_dirname = false; c.append_folder.add_dirname_pos = None; }
                                }
                                *d = true;
                            }
                        }
                    })
                    .response
                    .on_hover_text("Prepend or append the parent folder name(s) to each filename");
                ui.label(RichText::new("Sep:").size(FONT_LABEL));
                let mut sep_buf = c.append_folder.add_dirname_sep.clone().unwrap_or_default();
                {
                    if fixed_text(ui, [44.0, 22.0], &mut sep_buf)
                        .on_hover_text("Separator between folder name and filename")
                        .changed()
                    {
                        c.append_folder.add_dirname_sep = if sep_buf.is_empty() {
                            None
                        } else {
                            Some(sep_buf)
                        };
                        *d = true;
                    }
                }
                ui.label(RichText::new("Lvl:").size(FONT_LABEL));
                let mut v = c.append_folder.dirname_level;
                if ui
                    .add_sized(
                        egui::vec2(30.0, 20.0),
                        int_drag(&mut v).range(DIRNAME_LEVEL.0..=DIRNAME_LEVEL.1),
                    )
                    .on_hover_text("Number of parent folders to append: 1 = immediate parent, 2 = parent + grandparent, etc. (separator is added after each folder)")
                    .changed()
                {
                    c.append_folder.dirname_level = v;
                    *d = true;
                }
            });
        },
        {
            reset_section(c, SectionId::AppendFolder);
        }
    );
}
