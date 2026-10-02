use eframe::egui;
use std::path::{Path, PathBuf};

mod app;
mod persist;
mod revert;
mod types;
#[macro_use]
mod helpers;
mod dialogs;
mod preview_table;
mod sections;
mod table_shared;
mod ui;

/// Returns true if a graphical display is likely available.
pub fn is_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Load the pre-rendered icon data from the build script.
/// Generated from resources/icons/awara.svg at build time.
fn load_awara_icon() -> Option<egui::IconData> {
    Some(include!(concat!(env!("OUT_DIR"), "/awara_icon.rs")))
}

/// Shared eframe window setup used by all entry points.
fn build_window(
    theme: &str,
    icon: Option<egui::IconData>,
    title: &str,
    app: app::GuiApp,
) -> Result<(), Box<dyn std::error::Error>> {
    let theme_owned = theme.to_string();

    let mut viewport = egui::ViewportBuilder::default()
        .with_app_id("awara") // Must match the .desktop filename → Wayland titlebar/taskbar icon lookup
        .with_inner_size([1275.0, 695.0])
        .with_min_inner_size([530.0, 390.0])
        .with_resizable(true);

    if let Some(ref icon_data) = icon {
        viewport = viewport.with_icon(icon_data.clone());
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        title,
        options,
        Box::new(move |cc| {
            if let Some(icon_data) = icon {
                cc.egui_ctx.send_viewport_cmd_to(
                    egui::ViewportId::ROOT,
                    egui::ViewportCommand::Icon(Some(std::sync::Arc::new(icon_data))),
                );
            }

            // ── Font setup: Noto Sans (body) + Noto Sans Math + Noto Symbols 2 ──
            // Noto Sans Math → Mathematical Alphanumeric (𝝅, 𝛑) & math operators (∞, ≠, ≤)
            // Noto Sans Symbols 2 → dingbats (✓, ✔, ✗, ★, ➜) & many symbol blocks
            // Default egui fonts (NotoEmoji) remain as final fallback for emoji.
            let mut fonts = egui::FontDefinitions::default();

            macro_rules! load_font {
                ($path:literal) => {
                    std::sync::Arc::new(egui::FontData::from_static(include_bytes!($path)))
                };
            }

            fonts.font_data.insert(
                "noto-sans".to_owned(),
                load_font!("../../resources/fonts/NotoSans-Regular.ttf"),
            );
            fonts.font_data.insert(
                "noto-math".to_owned(),
                load_font!("../../resources/fonts/NotoSansMath-Regular.ttf"),
            );
            fonts.font_data.insert(
                "noto-symbols2".to_owned(),
                load_font!("../../resources/fonts/NotoSansSymbols2-Regular.ttf"),
            );

            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                let list = fonts.families.entry(family).or_default();
                list.insert(0, "noto-sans".to_owned());
                list.push("noto-math".to_owned());
                list.push("noto-symbols2".to_owned());
            }

            cc.egui_ctx.set_fonts(fonts);

            match theme_owned.as_str() {
                "dark" => {
                    let mut visuals = egui::Visuals::dark();
                    // More subtle selection color — less intense than default bright blue
                    visuals.selection.bg_fill = egui::Color32::from_rgb(32, 58, 78);
                    cc.egui_ctx.set_visuals(visuals);
                }
                "light" => cc.egui_ctx.set_visuals(egui::Visuals::light()),
                _ => {}
            }

            // egui's default `max_click_duration` (0.8s) treats a press held longer
            // than that as a drag, so releasing it never registers as a click.
            // Disable the heuristic entirely: a still-held button always clicks on release.
            cc.egui_ctx
                .options_mut(|o| o.input_options.max_click_duration = f64::INFINITY);

            Ok(Box::new(app))
        }),
    )?;

    Ok(())
}

/// Launch the GUI with an optional preset document.
/// When `config` is `None`, uses `GuiApp::new()` (default config).
pub fn run_with_config(
    theme: &str,
    config: Option<awara::RenameDoc>,
) -> Result<(), Box<dyn std::error::Error>> {
    let icon = load_awara_icon();
    let app = match config {
        Some(doc) => app::GuiApp::with_doc(doc),
        None => app::GuiApp::new(),
    };
    build_window(theme, icon, "Awara", app)
}

/// Launch the GUI already navigated to a specific directory.
/// Case 1: right-click on a directory then "Rename with Awara".
pub fn run_with_dir(theme: &str, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let icon = load_awara_icon();
    let mut app = app::GuiApp::new();
    app.enter_dir(dir);
    build_window(theme, icon, "Awara", app)
}

/// Launch the GUI with specific files pre-selected.
/// Case 3: right-click on file(s) then "Rename with Awara".
/// All files must share the same parent directory (caller should verify).
/// The directory is scanned and matching filenames are pre-selected.
pub fn run_with_files(theme: &str, files: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
    let icon = load_awara_icon();

    // Compute common parent directory
    let cwd = files
        .first()
        .and_then(|f| f.parent())
        .filter(|p| p.exists())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Extract basenames for pre-selection matching
    let preselected: Vec<String> = files
        .iter()
        .filter_map(|f| f.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .collect();

    let mut app = app::GuiApp::new();
    app.cwd = cwd.clone();
    app.cwd_input = cwd.to_string_lossy().to_string();
    app.pending_preselection = Some(preselected);
    app.scan_dir();
    build_window(theme, icon, "Awara", app)
}
