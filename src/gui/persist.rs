use crate::gui::app::GuiApp;
use crate::gui::types::{GuiEditState, GuiTimestampsState, PreviewSortCol};

pub(crate) const RENAME_OPTIONS_FILE_NAME: &str = "rename_options.json";

pub(crate) const APP_SETTINGS_FILE_NAME: &str = "app_settings.json";

/// UI view state that is always persisted (independent of "Remember Rename Options").
/// This includes table-column sizing for the main preview table and the revert
/// dialog, matching where "Remember Last Directory" lives.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct AppSettings {
    pub remember_rename_options: bool,
    pub remember_last_dir: bool,
    pub auto_refresh: bool,
    pub last_dir: String,
    #[serde(default)]
    pub skip_trash_confirmation: bool,
    #[serde(default)]
    pub skip_dir_creation_warning: bool,
    /// Preview-table column widths, in column index order (not display order).
    #[serde(default)]
    pub table_col_widths: Vec<f32>,
    /// Preview-table column display order.
    #[serde(default)]
    pub table_col_order: Vec<usize>,
    /// Revert-dialog column widths.
    #[serde(default)]
    pub revert_col_widths: Vec<f32>,
    /// Preview-table sort column (always persisted, like column sizing).
    #[serde(default)]
    pub preview_sort_col: PreviewSortCol,
    /// Preview-table sort direction (true = ascending). Defaults ascending so
    /// old settings files (which lack these fields) restore the app default.
    #[serde(default = "default_sort_asc")]
    pub preview_sort_asc: bool,
}

fn default_sort_asc() -> bool {
    true
}

impl AppSettings {
    pub(crate) fn state_path_in(
        config_dir: Option<&std::path::Path>,
    ) -> Option<std::path::PathBuf> {
        let base: std::path::PathBuf = config_dir
            .map(|p| p.to_path_buf())
            .or_else(dirs::config_dir)?;
        let mut d = base;
        // Linux (XDG): lowercase; Windows (%APPDATA%) and macOS (Application Support): Title Case
        #[cfg(target_os = "linux")]
        d.push("awara");
        #[cfg(not(target_os = "linux"))]
        d.push("Awara");
        let _ = std::fs::create_dir_all(&d);
        d.push(APP_SETTINGS_FILE_NAME);
        Some(d)
    }

    pub fn save_to(path: &std::path::Path, app: &GuiApp) {
        let state = AppSettings {
            remember_rename_options: app.remember_rename_options,
            remember_last_dir: app.remember_last_dir,
            auto_refresh: app.auto_refresh,
            last_dir: app.cwd.to_string_lossy().to_string(),
            skip_trash_confirmation: app.skip_trash_confirmation,
            skip_dir_creation_warning: app.skip_dir_creation_warning,
            table_col_widths: app.layout.col_widths.clone(),
            table_col_order: app.layout.col_order.clone(),
            revert_col_widths: app.revert_col_widths.clone(),
            preview_sort_col: app.preview_sort_col,
            preview_sort_asc: app.preview_sort_asc,
        };
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn save(app: &GuiApp) {
        Self::save_in(app, None);
    }

    pub fn save_in(app: &GuiApp, config_dir: Option<&std::path::Path>) {
        if let Some(path) = Self::state_path_in(config_dir) {
            Self::save_to(&path, app);
        }
    }

    pub fn load_from(path: &std::path::Path) -> Option<AppSettings> {
        if !path.exists() {
            return None;
        }
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn load() -> Option<AppSettings> {
        Self::load_in(None)
    }

    pub fn load_in(config_dir: Option<&std::path::Path>) -> Option<AppSettings> {
        let path = Self::state_path_in(config_dir)?;
        Self::load_from(&path)
    }
}

/// GUI-persisted state: the shared [`awara::RenameDoc`] plus GUI-only form state.
/// The document is flattened so the file has the same top-level shape as a shared
/// rename document (`rename_config` + `section_enabled`), with the GUI-only keys
/// optional.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct GuiPersistedState {
    #[serde(flatten)]
    pub doc: awara::RenameDoc,
    #[serde(default)]
    pub edit_state: GuiEditState,
    #[serde(default)]
    pub undo_file: String,
    #[serde(default)]
    pub timestamps: GuiTimestampsState,
}

impl GuiPersistedState {
    /// Config directory holding the persisted files, created if needed.
    fn dir_in(config_dir: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
        let base: std::path::PathBuf = config_dir
            .map(|p| p.to_path_buf())
            .or_else(dirs::config_dir)?;
        let mut d = base;
        // Linux (XDG): lowercase; Windows (%APPDATA%) and macOS (Application Support): Title Case
        #[cfg(target_os = "linux")]
        d.push("awara");
        #[cfg(not(target_os = "linux"))]
        d.push("Awara");
        let _ = std::fs::create_dir_all(&d);
        Some(d)
    }

    pub(crate) fn state_path_in(
        config_dir: Option<&std::path::Path>,
    ) -> Option<std::path::PathBuf> {
        Some(Self::dir_in(config_dir)?.join(RENAME_OPTIONS_FILE_NAME))
    }

    pub fn save_to(path: &std::path::Path, app: &GuiApp) {
        let state = GuiPersistedState {
            doc: awara::RenameDoc::with_disabled(
                app.config.clone(),
                awara::disabled_from_enabled(&app.section_enabled),
            ),
            edit_state: app.edit_state.clone(),
            undo_file: app.undo_file.clone(),
            timestamps: app.timestamps.clone(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn save_in(app: &GuiApp, config_dir: Option<&std::path::Path>) {
        if let Some(path) = Self::state_path_in(config_dir) {
            Self::save_to(&path, app);
        }
    }

    pub fn load_from(path: &std::path::Path) -> Option<GuiPersistedState> {
        if !path.exists() {
            return None;
        }
        let data = std::fs::read_to_string(path).ok()?;
        let mut state: GuiPersistedState = serde_json::from_str(&data).ok()?;
        // Auto-restore stays silent, but is normalized like any other load so
        // the in-memory config can't contain out-of-range values.
        state.doc.rename_config.sanitize();
        Some(state)
    }

    pub fn load() -> Option<GuiPersistedState> {
        Self::load_in(None)
    }

    pub fn load_in(config_dir: Option<&std::path::Path>) -> Option<GuiPersistedState> {
        let path = Self::state_path_in(config_dir)?;
        Self::load_from(&path)
    }
}
