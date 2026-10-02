use crate::gui::app::{
    GuiApp, compose_move_copy_section, reset_auto_date_edit_state, reset_move_copy_edit_state,
    reset_section, section_is_modified,
};
use crate::gui::helpers::{
    CUSTOM_FMT, FMT_COMBO_LEN, FONT_LABEL, build_date_fmt, cb_tip, combo_enum_tip, combo_opt_tip,
    drag_field_tip, drag_from_field_tip, drag_label_tip, dyn_combo_width, dyn_combo_width_iter,
    fixed_text, fixed_text_enabled, fmt_combo_entry, fmt_index, int_drag, linked_from_to_values,
    section_group, split_date_fmt,
};
use crate::gui::types::{GuiEditState, NUMBERING_BASES, PopupKind, SectionEnabled, SectionId};
use awara::{
    ADD_AT, AUTO_DATE_OFFSET, AUTO_DATE_TYPES, CaseMode, CropMode, DIRNAME_LEVEL, DatePosition,
    ExtensionMode, FILTER_LEVEL, INSERT_META_FMT_DEFAULT, LeadDotsMode, MOVE_COPY_VALUE,
    MetaSource, NAME_LEN, NAME_SEGMENT, NUMBERING_AT, NUMBERING_BREAK, NUMBERING_INCREMENT,
    NUMBERING_PAD, NUMBERING_START, NumberingMode, PATH_LEN, RenameConfig,
    auto_date_type_for_source, auto_date_type_for_tag, auto_date_type_index,
    meta_key_is_insert_tag,
};
use eframe::egui::{
    self, RichText, ScrollArea,
    containers::scroll_area::{DragScroll, ScrollSource},
};
mod add;
mod append_folder;
mod auto_date;
mod case;
mod copy_to;
mod extension;
mod filters;
mod move_copy;
mod name;
mod name_segment;
mod numbering;
mod regex;
mod remove;
mod replace;
mod special;

use add::draw_section_add;
use append_folder::draw_section_append_folder;
use auto_date::draw_section_auto_date;
use case::draw_section_case;
use copy_to::draw_section_copy_to;
use extension::draw_section_extension;
use filters::{FilterScanCtl, draw_section_filters};
use move_copy::draw_section_move_copy;
use name::draw_section_name;
use name_segment::draw_section_name_segment;
use numbering::draw_section_numbering;
use regex::draw_section_regex;
use remove::draw_section_remove;
use replace::draw_section_replace;
use special::draw_section_special;

// ── Rename Options (flat scrollable layout) ──

/// The Auto Date Type shown when the config has no date at all — also what the
/// Mode combo writes when it switches a date on.
///
/// The Type combo falls back to index 0 for an unlisted key, so this must be that
/// entry (asserted in `tests`, since the two only agree by convention otherwise).
const DEFAULT_DATE_TYPE: &str = "creation_curr";

/// The Auto Date Type shown for `add_file_date`, which always reads the file's
/// mtime (its own item ignores `auto_date_type`).
const FILE_DATE_TYPE: &str = "modified_curr";

/// The Auto Date widgets' state for a config: the Type they should show, and the
/// stored format split into the pieces the Fmt/Seg/Custom widgets edit.
pub(crate) struct AutoDateEditSeed {
    pub date_type: String,
    pub format_key: String,
    pub sep: String,
    pub custom: String,
}

/// Reconstruct the Auto Date widgets' state from `c`.
///
/// The config stores a format *string* (plus a tag, for an Insert Meta date),
/// while the widgets edit a (Type, Fmt key, Seg, Custom) tuple. Deriving that
/// tuple from the config is what keeps the widgets honest: recomposing the seeded
/// tuple reproduces the stored string exactly, so editing an unrelated field
/// cannot silently rewrite the date format or its source.
pub(crate) fn auto_date_edit_seed(c: &RenameConfig) -> AutoDateEditSeed {
    let (date_type, fmt) = auto_date_edit_source(c);
    let (format_key, sep, custom) = match split_date_fmt(&fmt) {
        Some((key, sep)) => (key.to_string(), sep, String::new()),
        // Not expressible as one Fmt key plus one separator: the Custom field
        // holds the whole string, so it survives an edit untouched.
        None if !fmt.is_empty() => (CUSTOM_FMT.0.to_string(), String::new(), fmt),
        None => (
            fmt_combo_entry(0).0.to_string(),
            String::new(),
            String::new(),
        ),
    };
    AutoDateEditSeed {
        date_type,
        format_key,
        sep,
        custom,
    }
}

/// The Type combo entry for `c`, and the date format the config will apply.
///
/// Mirrors the engine, which is why each mechanism reads its *own* format field:
/// an Insert Meta date uses `insert_meta_fmt` (or its default), an Add File Date
/// always reads mtime, and an Add Date with no recorded type is the wall clock.
fn auto_date_edit_source(c: &RenameConfig) -> (String, String) {
    let d = &c.auto_date;
    if let Some(tag) = d.insert_meta.as_deref() {
        let ty = auto_date_type_for_tag(tag).unwrap_or(DEFAULT_DATE_TYPE);
        let fmt = d
            .insert_meta_fmt
            .clone()
            .unwrap_or_else(|| INSERT_META_FMT_DEFAULT.to_string());
        return (ty.to_string(), fmt);
    }
    if let Some(fmt) = d.add_file_date.as_deref().filter(|s| !s.is_empty()) {
        return (FILE_DATE_TYPE.to_string(), fmt.to_string());
    }
    if let Some(fmt) = d.add_date.as_deref().filter(|s| !s.is_empty()) {
        // `--add-date` sets a format without a type; the engine stamps the clock.
        let ty = d.auto_date_type.clone().unwrap_or_else(|| {
            auto_date_type_for_source(MetaSource::Clock)
                .unwrap_or(DEFAULT_DATE_TYPE)
                .to_string()
        });
        return (ty, fmt.to_string());
    }
    (DEFAULT_DATE_TYPE.to_string(), String::new())
}

pub(crate) fn draw_rename_options(app: &mut GuiApp, ui: &mut egui::Ui) {
    // Execute button row (uses app directly, no field borrows yet)
    ui.horizontal(|ui| {
        // Section is edge-to-edge — keep the heading off the window edge.
        ui.add_space(8.0);
        ui.heading("Rename Options");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Section has no right margin — keep the buttons off the window edge.
            ui.add_space(8.0);
            if app.executing {
                ui.label("Executing...");
            } else {
                if ui.button("⚡ Apply").clicked() {
                    app.try_execute_renames();
                }
                ui.separator();
                if ui
                    .button("Reset All")
                    .on_hover_text("Reset all configuration to default values")
                    .clicked()
                {
                    app.reset_all();
                }
            }
        });
    });
    ui.separator();

    let mut scan_clicked = false;
    let mut refilter_clicked = false;
    let mut browse_clicked = false;
    let mut ts_clicked = false;
    let auto_scan = app.auto_refresh;

    // Resolve section order from config before taking mutable borrow
    let section_order: Vec<SectionId> = if app.config.section_order.is_empty() {
        SectionId::default_order().to_vec()
    } else {
        app.config
            .section_order
            .iter()
            .filter_map(|name| SectionId::from_label(name))
            .collect()
    };
    let sec_order = |id: SectionId| -> String {
        section_order
            .iter()
            .position(|&i| i == id)
            .map(|p| (p + 1).to_string())
            .unwrap_or_default()
    };
    let order_status_str = {
        // Compared element-wise rather than by building the default label list:
        // this runs every frame and only produces a status word.
        let default_order = SectionId::default_order();
        let is_default = app.config.section_order.is_empty()
            || (app.config.section_order.len() == default_order.len()
                && default_order
                    .iter()
                    .zip(&app.config.section_order)
                    .all(|(id, name)| id.label() == name.as_str()));
        if is_default { "not set" } else { "set" }
    };
    ScrollArea::both()
        .id_salt("options_scroll")
        .scroll_source(ScrollSource {
            drag: DragScroll::Never,
            ..Default::default()
        })
        // Fill the full section height so the horizontal scrollbar sits at the
        // bottom edge (against the status bar) even when the content is short.
        .auto_shrink([false; 2])
        // Leading padding for every row of groups — matches the 6px spacing
        // used between section rows.
        .content_margin(egui::Margin {
            left: 6,
            right: 0,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            let c = &mut app.config;
            let e = &mut app.edit_state;
            let d = &mut app.preview_dirty;
            let en = &mut app.section_enabled;
            // ── Row 1: flex columns ──
            // Weights: RegEx+Name(27) | Replace+Case(24) | Remove(25) | Add(18) | Auto Date(22) | Numbering(28)
            let gap = 2.0_f32;
            let avail = ui.available_width();
            let gaps_w = gap * 5.0_f32;
            let total_w = 180.0_f32;
            let col_w = |w: f32| -> f32 { ((avail - gaps_w) * w / total_w).max(70.0) };
            ui.horizontal_top(|ui| {
                sec_col!(ui, gap, col_w, 27.0, |ui: &mut egui::Ui| {
                    ui.vertical(|ui| {
                        draw_section_regex(ui, en, d, &sec_order, c);
                        ui.add_space(6.0);
                        draw_section_name(ui, en, d, &sec_order, c);
                    });
                });
                sec_col!(ui, gap, col_w, 24.0, |ui: &mut egui::Ui| {
                    ui.vertical(|ui| {
                        draw_section_replace(ui, en, d, &sec_order, c);
                        ui.add_space(6.0);
                        draw_section_case(ui, en, d, &sec_order, c);
                    });
                });
                sec_col!(ui, gap, col_w, 25.0, |ui: &mut egui::Ui| {
                    draw_section_remove(ui, en, d, &sec_order, c);
                });
                sec_col!(ui, gap, col_w, 18.0, |ui: &mut egui::Ui| {
                    draw_section_add(ui, en, d, &sec_order, c);
                });
                sec_col!(ui, gap, col_w, 22.0, |ui: &mut egui::Ui| {
                    draw_section_auto_date(ui, en, d, &sec_order, c, e);
                });
                sec_col!(ui, gap, col_w, 28.0, |ui: &mut egui::Ui| {
                    draw_section_numbering(ui, en, d, &sec_order, c);
                });
            });
            ui.add_space(6.0);
            // ── Row 2: Move/Copy | Name Segment | Append Folder | Extension ──
            let gap = 2.0_f32;
            let avail = ui.available_width();
            let gaps_w = gap * 3.0_f32;
            let total_w = 90.0_f32;
            let col_w = |w: f32| -> f32 { ((avail - gaps_w) * w / total_w).max(80.0) };
            ui.horizontal_top(|ui| {
                sec_col!(ui, gap, col_w, 30.0, |ui: &mut egui::Ui| {
                    draw_section_move_copy(ui, en, d, &sec_order, c, e);
                });
                sec_col!(ui, gap, col_w, 15.0, |ui: &mut egui::Ui| {
                    draw_section_name_segment(ui, en, d, &sec_order, c);
                });
                sec_col!(ui, gap, col_w, 20.0, |ui: &mut egui::Ui| {
                    draw_section_append_folder(ui, en, d, &sec_order, c);
                });
                sec_col!(ui, gap, col_w, 25.0, |ui: &mut egui::Ui| {
                    draw_section_extension(ui, en, d, &sec_order, c);
                });
            });
            ui.add_space(6.0);
            // ── Row 3: Filters | Copy / Move to Location ──
            ui.horizontal_top(|ui| {
                sec_col!(ui, gap, col_w, 55.0, |ui: &mut egui::Ui| {
                    draw_section_filters(
                        ui,
                        en,
                        d,
                        &sec_order,
                        c,
                        FilterScanCtl {
                            auto_scan,
                            scan_clicked: &mut scan_clicked,
                            refilter_clicked: &mut refilter_clicked,
                        },
                    );
                });
                sec_col!(ui, gap, col_w, 35.0, |ui: &mut egui::Ui| {
                    draw_section_copy_to(ui, en, d, &sec_order, c, &mut browse_clicked);
                });
            });
            ui.add_space(6.0);
            // ── Row 4: Special (needs the whole `app`; field borrows end here) ──
            draw_section_special(ui, app, &sec_order, &mut ts_clicked, order_status_str);
            ui.add_space(6.0);
        }); // c, d, en dropped here

    // Non-borrowing deferred actions (app is free again)
    if scan_clicked {
        app.scan_dir();
    }
    if refilter_clicked && !app.raw_all_files.is_empty() {
        app.apply_filters_in_memory();
    } else if refilter_clicked && !app.cwd.as_os_str().is_empty() {
        // No raw data yet — fall back to a full scan.
        app.scan_dir();
    }
    if browse_clicked {
        app.browse_for_output();
    }
    if ts_clicked {
        app.init_timestamps_ui();
        app.active_popup = Some(PopupKind::TsEditor);
    }

    // Persist state when config changes (saves or deletes state file depending on toggle)
    if app.preview_dirty {
        app.save_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awara::{AutoDateSection, MetaSource, auto_date_type_source};

    fn with_add_date(fmt: &str) -> RenameConfig {
        RenameConfig {
            auto_date: AutoDateSection {
                add_date: Some(fmt.to_string()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn with_insert_meta(tag: &str, fmt: Option<&str>) -> RenameConfig {
        RenameConfig {
            auto_date: AutoDateSection {
                insert_meta: Some(tag.to_string()),
                insert_meta_fmt: fmt.map(|f| f.to_string()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// The Type the combo shows must be an entry the combo lists, on pain of the
    /// fallback index 0 silently naming a different timestamp than the config uses.
    #[test]
    fn default_and_file_date_types_are_listed_and_faithful() {
        assert_eq!(
            auto_date_type_index(DEFAULT_DATE_TYPE),
            Some(0),
            "the combo's fallback index must be the default Type"
        );
        assert_eq!(
            auto_date_type_source(FILE_DATE_TYPE),
            Some(MetaSource::Mtime),
            "Add File Date always reads mtime"
        );
        assert!(auto_date_type_index(FILE_DATE_TYPE).is_some());
    }

    /// The Type shown for a stored tag. Before, the tag was shown verbatim, so
    /// `exif-date` and `modified` — which the combo has no entry for — fell back
    /// to "Creation (Curr.)" while the config applied an EXIF date or the mtime.
    #[test]
    fn seeded_type_matches_what_the_config_reads() {
        let ty = |c: &RenameConfig| auto_date_edit_seed(c).date_type;
        assert_eq!(ty(&with_insert_meta("exif-date", None)), "taken_original");
        assert_eq!(ty(&with_insert_meta("modified", None)), "modified_curr");
        assert_eq!(
            ty(&with_insert_meta("taken_digitized", None)),
            "taken_digitized"
        );
        // The stored type wins for an Add Date; the CLI's `--add-date` has none,
        // and the engine stamps the clock.
        let mut clock = with_add_date("YYYY");
        assert_eq!(ty(&clock), "current");
        clock.auto_date.auto_date_type = Some("modified_new".into());
        assert_eq!(ty(&clock), "modified_new");
        // An Add File Date item ignores the type and reads mtime.
        let mut file_date = with_add_date("YYYY");
        file_date.auto_date.add_date = None;
        file_date.auto_date.add_file_date = Some("YYYY".into());
        assert_eq!(ty(&file_date), "modified_curr");
        // Nothing configured: the Mode combo's own default.
        assert_eq!(ty(&RenameConfig::default()), DEFAULT_DATE_TYPE);
    }

    /// The headline property: the seeded widgets reproduce the stored format, so
    /// editing an unrelated field cannot rewrite the date format.
    #[test]
    fn seeded_format_round_trips_the_stored_string() {
        let round_trip = |c: &RenameConfig| {
            let seed = auto_date_edit_seed(c);
            build_date_fmt(&seed.format_key, &seed.sep, &seed.custom)
        };
        for fmt in [
            "DDMMYYYY",
            "DD-MM-YYYY",
            "YYYY_MM_DD_hh",
            "MM/DD/YYYY hh/mm/ss",
        ] {
            assert_eq!(round_trip(&with_add_date(fmt)), fmt, "add_date {fmt}");
        }
        // An Insert Meta date formats with its own field, defaulted by the engine.
        assert_eq!(
            round_trip(&with_insert_meta("taken_original", None)),
            INSERT_META_FMT_DEFAULT
        );
        assert_eq!(
            round_trip(&with_insert_meta("exif-date", Some("DD.MM.YYYY"))),
            "DD.MM.YYYY"
        );
        // Formats the widgets cannot express become a Custom string, kept whole.
        for fmt in ["date_DDMMYYYY", "DDMMYY", "YYYY-MM-DD-ext"] {
            let seed = auto_date_edit_seed(&with_add_date(fmt));
            assert_eq!(seed.format_key, "custom", "{fmt} must be Custom");
            assert_eq!(seed.custom, fmt, "{fmt} must be preserved verbatim");
            assert_eq!(seed.sep, "", "{fmt}: the whole string is Custom");
        }
    }
}
