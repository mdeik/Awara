use crate::tui::app::{App, AppMode, StatusSeverity};
use crate::tui::parser;
use awara::{CollisionStrategy, CompiledConfig, RenameOptions, ScanSortBy};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub enum Action {
    Quit,
    None,
}

pub fn handle_key_events(key: KeyEvent, app: &mut App) -> Option<Action> {
    // Global keys
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(Action::Quit);
    }

    // Tab to switch modes in forward order (except in Console mode when there's input)
    if key.code == KeyCode::Tab && (app.mode != AppMode::Console || app.input.is_empty()) {
        app.mode = match app.mode {
            AppMode::Preview => AppMode::Config,
            AppMode::Navigator => AppMode::Preview,
            AppMode::Console => AppMode::Navigator,
            AppMode::Config => AppMode::Console,
            AppMode::Help => AppMode::Help,
        };
        return Some(Action::None);
    }

    // Shift+Tab to switch modes in reverse order
    if key.code == KeyCode::BackTab {
        app.mode = match app.mode {
            AppMode::Preview => AppMode::Navigator,
            AppMode::Navigator => AppMode::Console,
            AppMode::Console => AppMode::Config,
            AppMode::Config => AppMode::Preview,
            AppMode::Help => AppMode::Help,
        };
        return Some(Action::None);
    }

    // Global help toggle
    if key.code == KeyCode::Char('?') {
        if app.mode == AppMode::Help {
            app.help_state.select(Some(0));
            app.mode = AppMode::Navigator;
        } else {
            app.help_state.select(Some(0));
            app.mode = AppMode::Help;
        }
        return Some(Action::None);
    }

    match app.mode {
        AppMode::Navigator => handle_navigator_keys(key, app),
        AppMode::Console => handle_console_keys(key, app),
        AppMode::Preview => handle_preview_keys(key, app),
        AppMode::Config => handle_config_keys(key, app),
        AppMode::Help => handle_help_keys(key, app),
    }
}

mod console;
mod modes;

use console::handle_console_keys;
use modes::{
    handle_config_keys, handle_help_keys, handle_navigator_keys, handle_preview_keys,
    update_config_field, update_item_value,
};
