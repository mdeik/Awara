use crate::args::Args;
use awara::{RenameConfig, ScanSortBy};
use clap::Parser;
use dirs;
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub enum ParseAction {
    UpdateConfig(String),
    ChangeDir(PathBuf),
    UpdateSort(ScanSortBy),
    UpdateFilter(Option<String>),
    SavePreset(String),
    LoadPreset(String),
    SetUndoFile(Option<String>),
    ToggleDryRun,
}

/// How a console command is dispatched once recognized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandledBy {
    /// `parse_command` executes the command and returns [`ParseAction`]s.
    Parser,
    /// `handle_console_keys` in events.rs intercepts the command before parsing.
    Events,
}

/// Handler executed by `parse_command` for a declared console command.
type ParseFn = fn(&mut RenameConfig, &[&str]) -> Result<Vec<ParseAction>, String>;

/// A TUI console command. `CONSOLE_COMMANDS` is the single source of truth
/// for every command the console accepts: the help menu (app.rs), the parser
/// dispatch below, and events.rs recognition all derive from this table. A
/// command declared here automatically gets help text, and a command that
/// exists in the parser or events without a declaration here is never
/// recognized — the two cannot drift apart.
pub struct CommandMeta {
    /// Primary name the user types.
    pub name: &'static str,
    /// Alternate names (e.g. "proceed" for "apply").
    pub aliases: &'static [&'static str],
    /// Argument placeholder shown in the help list, e.g. "<path>" ("" if none).
    pub args: &'static str,
    /// One-line description shown in the help navigator.
    pub summary: &'static str,
    /// Usage line shown in the help preview.
    pub usage: &'static str,
    /// Example text shown in the help preview.
    pub example: &'static str,
    /// Which dispatcher runs the command.
    pub handled_by: HandledBy,
    /// Parser handler; required for [`HandledBy::Parser`], `None` for events.
    pub parse: Option<ParseFn>,
}

pub const CONSOLE_COMMANDS: &[CommandMeta] = &[
    CommandMeta {
        name: "apply",
        aliases: &["proceed"],
        args: "",
        summary: "Execute the pending rename operations on the selected files (or all visible files if none are selected)",
        usage: "apply | proceed [--overwrite | --skip]",
        example: "Workflow: 1) Select files in the Navigator with Space. 2) Stage operations in the Console, e.g. --add-prefix vacation_. 3) Check the Preview pane shows the new names. 4) Type 'apply' and press Enter.\nIf a target already exists, apply stops and reports the collision — use 'proceed --overwrite' to replace it or 'proceed --skip' to skip it. A collision involving a folder on either side is never overwritten or merged and is always skipped.\nSafety: press D to toggle dry-run (nothing changes on disk), or set 'undo-file undo.json' first to keep a revert record.",
        handled_by: HandledBy::Events,
        parse: None,
    },
    CommandMeta {
        name: "preview",
        aliases: &[],
        args: "",
        summary: "Manually recompute the rename preview",
        usage: "preview",
        example: "Type 'preview' to refresh the Preview pane showing old → new names.",
        handled_by: HandledBy::Events,
        parse: None,
    },
    CommandMeta {
        name: "stacking",
        aliases: &["--stacking"],
        args: "",
        summary: "Toggle stacking mode to chain multiple rename operations",
        usage: "stacking",
        example: "Type 'stacking', then add '--add-prefix A_' and '--case-name lower' as separate steps.",
        handled_by: HandledBy::Events,
        parse: None,
    },
    CommandMeta {
        name: "cd",
        aliases: &[],
        args: "<path>",
        summary: "Change the working directory",
        usage: "cd ~/Documents",
        example: "Supports ~ for home directory. Relative paths also work.",
        handled_by: HandledBy::Parser,
        parse: Some(cmd_cd),
    },
    CommandMeta {
        name: "filter",
        aliases: &[],
        args: "<pattern>",
        summary: "Set a glob filter for visible files in the Navigator",
        usage: "filter *.txt",
        example: "Only .txt files will show in the file list.",
        handled_by: HandledBy::Parser,
        parse: Some(cmd_filter),
    },
    CommandMeta {
        name: "sort",
        aliases: &[],
        args: "<mode>",
        summary: "Sort the Navigator by the given mode",
        usage: "sort name | date | size",
        example: "Modes: name, name-desc, date, date-desc, size, size-desc. E.g. 'sort date' orders files by modification date.",
        handled_by: HandledBy::Parser,
        parse: Some(cmd_sort),
    },
    CommandMeta {
        name: "reset",
        aliases: &[],
        args: "",
        summary: "Reset all pending config to defaults",
        usage: "reset",
        example: "Clears all staged rename operations from the Config pane.",
        handled_by: HandledBy::Parser,
        parse: Some(cmd_reset),
    },
    CommandMeta {
        name: "save-preset",
        aliases: &["save_preset"],
        args: "<path>",
        summary: "Save the current pending config as a preset JSON file",
        usage: "save-preset my-preset.json",
        example: "Saves the pending config to <path> so it can be restored later with 'load-preset'.",
        handled_by: HandledBy::Parser,
        parse: Some(cmd_save_preset),
    },
    CommandMeta {
        name: "load-preset",
        aliases: &["load_preset"],
        args: "<path>",
        summary: "Load a preset JSON file into the pending config",
        usage: "load-preset my-preset.json",
        example: "Restores a config previously saved with 'save-preset'.",
        handled_by: HandledBy::Parser,
        parse: Some(cmd_load_preset),
    },
    CommandMeta {
        name: "undo-file",
        aliases: &["undo_file"],
        args: "<path>",
        summary: "Write a JSON undo record of every successful in-place rename to <path> (call with no path to disable). Copy / Move to Location transfers are not recorded",
        usage: "undo-file undo.json",
        example: "Set 'undo-file undo.json' before applying, run 'apply', then later revert from the shell with 'awara --apply-undo undo.json'.",
        handled_by: HandledBy::Parser,
        parse: Some(cmd_undo_file),
    },
    CommandMeta {
        name: "dry-run",
        aliases: &["dry_run"],
        args: "",
        summary: "Toggle dry-run mode on/off (same as D key)",
        usage: "dry-run",
        example: "When dry-run is on, 'apply' shows what would happen without making changes.",
        handled_by: HandledBy::Parser,
        parse: Some(cmd_dry_run),
    },
    CommandMeta {
        name: "exit",
        aliases: &["quit"],
        args: "",
        summary: "Quit the application",
        usage: "exit",
        example: "Same as pressing Ctrl+C or q. 'quit' is an alias.",
        handled_by: HandledBy::Events,
        parse: None,
    },
    CommandMeta {
        name: "help",
        aliases: &["--help", "h"],
        args: "",
        summary: "Open this interactive help screen",
        usage: "help",
        example: "Same as pressing ?. 'h' and '--help' are aliases.",
        handled_by: HandledBy::Events,
        parse: None,
    },
];

impl CommandMeta {
    /// Help-list label for this command, e.g. "apply / proceed" or
    /// "cd <path>". Used by the help menu (app.rs) and its tests.
    pub fn help_label(&self) -> String {
        let names = if self.aliases.is_empty() {
            self.name.to_string()
        } else {
            format!("{} / {}", self.name, self.aliases.join(" / "))
        };
        if self.args.is_empty() {
            names
        } else {
            format!("{} {}", names, self.args)
        }
    }
}

/// Look up a console command by name or alias.
pub fn find(command: &str) -> Option<&'static CommandMeta> {
    CONSOLE_COMMANDS
        .iter()
        .find(|m| m.name == command || m.aliases.contains(&command))
}

/// Names of every console command, used by the "unknown command" error.
pub fn available_commands() -> String {
    CONSOLE_COMMANDS
        .iter()
        .map(|m| m.name)
        .collect::<Vec<_>>()
        .join(", ")
}

fn cmd_cd(_config: &mut RenameConfig, parts: &[&str]) -> Result<Vec<ParseAction>, String> {
    if parts.len() > 1 {
        // Handle ~ expansion manually
        let path_str = parts[1];
        let path = if path_str.starts_with("~") {
            if let Some(home) = dirs::home_dir() {
                if path_str == "~" {
                    home
                } else if let Some(stripped) = path_str.strip_prefix("~/") {
                    home.join(stripped)
                } else {
                    PathBuf::from(path_str) // ~user not supported
                }
            } else {
                PathBuf::from(path_str)
            }
        } else {
            PathBuf::from(path_str)
        };
        Ok(vec![ParseAction::ChangeDir(path)])
    } else {
        Err("Missing path for cd".to_string())
    }
}

fn cmd_filter(_config: &mut RenameConfig, parts: &[&str]) -> Result<Vec<ParseAction>, String> {
    if parts.len() > 1 {
        Ok(vec![ParseAction::UpdateFilter(Some(parts[1].to_string()))])
    } else {
        Ok(vec![ParseAction::UpdateFilter(None)])
    }
}

fn cmd_sort(_config: &mut RenameConfig, parts: &[&str]) -> Result<Vec<ParseAction>, String> {
    if parts.len() > 1 {
        // Parse sort mode manually or via clap value parser?
        // Simple manual mapping for now
        let mode = match parts[1].to_lowercase().as_str() {
            "name" => ScanSortBy::Name,
            "namedesc" | "name-desc" => ScanSortBy::NameDesc,
            "date" => ScanSortBy::Date,
            "datedesc" | "date-desc" => ScanSortBy::DateDesc,
            "size" => ScanSortBy::Size,
            "sizedesc" | "size-desc" => ScanSortBy::SizeDesc,
            _ => return Err(format!("Unknown sort mode: {}", parts[1])),
        };
        Ok(vec![ParseAction::UpdateSort(mode)])
    } else {
        Err("Missing mode for sort".to_string())
    }
}

fn cmd_reset(config: &mut RenameConfig, _parts: &[&str]) -> Result<Vec<ParseAction>, String> {
    *config = RenameConfig::empty();
    Ok(vec![ParseAction::UpdateConfig("reset".to_string())])
}

fn cmd_save_preset(_config: &mut RenameConfig, parts: &[&str]) -> Result<Vec<ParseAction>, String> {
    if parts.len() > 1 {
        Ok(vec![ParseAction::SavePreset(parts[1].to_string())])
    } else {
        Err("Usage: save-preset <path>".to_string())
    }
}

fn cmd_load_preset(_config: &mut RenameConfig, parts: &[&str]) -> Result<Vec<ParseAction>, String> {
    if parts.len() > 1 {
        Ok(vec![ParseAction::LoadPreset(parts[1].to_string())])
    } else {
        Err("Usage: load-preset <path>".to_string())
    }
}

fn cmd_undo_file(_config: &mut RenameConfig, parts: &[&str]) -> Result<Vec<ParseAction>, String> {
    if parts.len() > 1 {
        Ok(vec![ParseAction::SetUndoFile(Some(parts[1].to_string()))])
    } else {
        Ok(vec![ParseAction::SetUndoFile(None)])
    }
}

fn cmd_dry_run(_config: &mut RenameConfig, _parts: &[&str]) -> Result<Vec<ParseAction>, String> {
    Ok(vec![ParseAction::ToggleDryRun])
}

pub fn parse_command(input: &str, config: &mut RenameConfig) -> Result<Vec<ParseAction>, String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(vec![]);
    }

    // Handle quotes manually since we don't have shlex
    let mut parts = Vec::new();
    let mut current_part = String::new();
    let mut in_quote = false;
    let mut escape = false;

    for c in input.chars() {
        if escape {
            current_part.push(c);
            escape = false;
        } else if c == '\\' {
            escape = true;
        } else if c == '"' {
            in_quote = !in_quote;
        } else if c.is_whitespace() && !in_quote {
            if !current_part.is_empty() {
                parts.push(current_part.clone());
                current_part.clear();
            }
        } else {
            current_part.push(c);
        }
    }
    if !current_part.is_empty() {
        parts.push(current_part);
    }

    let parts_ref: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
    let parts = parts_ref; // shadowing

    if parts.is_empty() {
        return Ok(vec![]);
    }
    let command = parts[0];

    // Console commands are dispatched through the SSoT registry
    // (CONSOLE_COMMANDS). A command can only be recognized here if it is
    // declared there, so every parser command is guaranteed to have help text.
    if let Some(meta) = find(command) {
        return match meta.parse {
            Some(parse) => parse(config, &parts),
            None => Err(format!(
                "'{command}' is handled by the console, not the parser"
            )),
        };
    }

    // Only fall through to clap parsing if command starts with "--"
    if !command.starts_with("--") {
        return Err(format!(
            "error: unknown command '{}' found\n\
             available commands: {}\n\
             flag commands must start with '--'",
            command,
            available_commands()
        ));
    }

    // Delegate all flag parsing to Args (SSoT — single source of truth in args.rs)
    // Pass all parts to clap (the first part is the command which we already matched on)
    let mut clap_input = vec!["awara"];
    clap_input.extend(parts);

    match Args::try_parse_from(&clap_input) {
        Ok(cli_args) => {
            *config = cli_args.into_config();
            Ok(vec![ParseAction::UpdateConfig(input.to_string())])
        }
        Err(e) => Err(format!("{}", e)),
    }
}

#[cfg(test)]
mod tests;
