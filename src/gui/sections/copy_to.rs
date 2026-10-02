use super::*;

pub(super) fn draw_section_copy_to(
    ui: &mut egui::Ui,
    en: &mut SectionEnabled,
    d: &mut bool,
    sec_order: &dyn Fn(SectionId) -> String,
    c: &mut RenameConfig,
    browse_clicked: &mut bool,
) {
    sec_group!(
        ui,
        en,
        d,
        sec_order,
        c,
        CopyTo,
        "Copy / Move to Location",
        |ui, _en| {
            egui::Grid::new("copyto_grid")
            .min_col_width(0.0)
            .show(ui, |ui| {
                // Row 1: Path: label | field + Browse
                ui.label(RichText::new("Path:").size(FONT_LABEL));
                ui.horizontal(|ui| {
                    let mut path_buf = c.copy_to.output_dir.clone().unwrap_or_default();
                    if fixed_text(ui, [410.0, 22.0], &mut path_buf)
                        .on_hover_text("Output directory path -- renamed files will be copied/moved here")
                        .changed()
                    {
                        c.copy_to.output_dir = if path_buf.is_empty() { None } else { Some(path_buf) };
                        *d = true;
                        }
                    if ui.button("Browse").on_hover_text("Browse for output directory").clicked() {
                        *browse_clicked = true;
                    }
                });
                ui.end_row();

                // Row 2: (empty) | checkboxes
                ui.label("");
                ui.horizontal(|ui| {
                    cb_tip(ui, d, &mut c.copy_to.copy_mode, "Copy not Move", "Copy files to the output directory instead of moving them");
                    cb_tip(ui, d, &mut c.copy_to.keep_structure, "Keep Structure", "Preserve the directory tree structure when copying to the output directory");
                });
                ui.end_row();
            });
        },
        {
            reset_section(c, SectionId::CopyTo);
        }
    );
}
