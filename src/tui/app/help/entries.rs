use super::*;

pub fn generate_cli_flag_entries() -> Vec<HelpEntry> {
    let cmd = Args::command();
    let mut entries = Vec::new();

    for arg in cmd.get_arguments() {
        let long = match arg.get_long() {
            Some(name) => name,
            None => continue, // skip positional args (e.g. "files")
        };

        use clap::builder::ArgAction;
        let takes_value = !matches!(
            arg.get_action(),
            ArgAction::SetTrue
                | ArgAction::SetFalse
                | ArgAction::Count
                | ArgAction::Help
                | ArgAction::Version
        );

        let flag_name = long.replace('_', "-");
        let command = if takes_value {
            format!("--{} <{}>", flag_name, flag_name)
        } else {
            format!("--{}", flag_name)
        };

        // Build description from clap's help text (SSoT)
        let description = match arg.get_help() {
            Some(help) => format!("Flag: {}", help),
            None => format!("Flag: --{} option", flag_name),
        };

        // Enrich with custom metadata
        let mut usage = String::new();
        let mut example = String::new();
        let mut category = String::new();

        for &(key, u, e, c) in FLAG_META {
            if key == long {
                usage = u.to_string();
                example = e.to_string();
                category = c.to_string();
                break;
            }
        }

        // Fallback category based on flag prefix
        if category.is_empty() {
            if long.starts_with("remove") {
                category = "Remove".to_string();
            } else if long.starts_with("add") || long.starts_with("dirname") {
                category = "Add".to_string();
            } else if long.starts_with("numbering") {
                category = "Numbering".to_string();
            } else if long.starts_with("extension") {
                category = "Extension".to_string();
            } else if long.starts_with("regex") {
                category = "Regex".to_string();
            } else if long.starts_with("case") {
                category = "Case".to_string();
            } else {
                category = "Other Flags".to_string();
            }
        }

        // Build default usage from command if none provided
        if usage.is_empty() {
            usage = command.clone();
        }

        entries.push(HelpEntry {
            command: Box::leak(command.into_boxed_str()),
            description: Box::leak(description.into_boxed_str()),
            usage: Box::leak(usage.into_boxed_str()),
            example: Box::leak(example.into_boxed_str()),
            category: Box::leak(category.into_boxed_str()),
        });
    }

    entries
}

pub fn help_entries() -> Vec<HelpEntry> {
    let mut entries: Vec<HelpEntry> = vec![
        // ── TUI Panes ──
        HelpEntry {
            command: "Navigator pane",
            description: "File browser. Navigate directories, select files to rename. Shows files/folders with size, filter/sort state.",
            usage: "",
            example: "Browses to the folder containing files you want to rename, selects target files with Space.",
            category: "TUI Panes",
        },
        HelpEntry {
            command: "Preview pane",
            description: "Shows a table of original → new filenames with diff highlighting. Green = added text, Red = removed.",
            usage: "",
            example: "After configuring --add-prefix vacation_, the preview shows each file's old name next to the new one.",
            category: "TUI Panes",
        },
        HelpEntry {
            command: "Config pane",
            description: "Lists pending rename operations. Supports reordering (Space+drag), inline editing (Enter), and deletion (d).",
            usage: "",
            example: "Shows 'add-prefix: vacation_' as a pending item. Press Enter to edit the value inline.",
            category: "TUI Panes",
        },
        HelpEntry {
            command: "Console pane",
            description: "Command-line interface for entering rename operations, changing directories, and executing renames.",
            usage: "",
            example: "Type '--add-prefix vacation_' then Enter to stage a prefix operation.",
            category: "TUI Panes",
        },
        // ── TUI Navigation ──
        HelpEntry {
            command: "↑/↓ or j/k",
            description: "Navigate file list up and down",
            usage: "",
            example: "Press j to move down one file, k to move up",
            category: "TUI Navigation",
        },
        HelpEntry {
            command: "Shift+↑/↓",
            description: "Move and add item to selection",
            usage: "",
            example: "Hold Shift while pressing j to select multiple files as you scroll",
            category: "TUI Navigation",
        },
        HelpEntry {
            command: "Enter",
            description: "Enter selected directory",
            usage: "",
            example: "Press Enter on a folder named 'photos' to browse its contents",
            category: "TUI Navigation",
        },
        HelpEntry {
            command: "Backspace",
            description: "Go up to parent directory",
            usage: "",
            example: "Press Backspace to go from /home/user/docs up to /home/user",
            category: "TUI Navigation",
        },
        HelpEntry {
            command: "R",
            description: "Toggle recursive mode (scan subdirectories)",
            usage: "",
            example: "Press R to see all files recursively instead of just the current folder",
            category: "TUI Navigation",
        },
        HelpEntry {
            command: "H",
            description: "Toggle showing hidden files (dotfiles)",
            usage: "",
            example: "Press H to show/hide .gitignore, .config, .hidden files",
            category: "TUI Navigation",
        },
        HelpEntry {
            command: "s / S",
            description: "Cycle sort mode / toggle sort direction",
            usage: "",
            example: "Press s to switch from Name to Date to Size sorting",
            category: "TUI Navigation",
        },
        HelpEntry {
            command: "F",
            description: "Cycle filter mode (Files → Folders → Both)",
            usage: "",
            example: "Press F to show only Folders when you want to see directory structure",
            category: "TUI Navigation",
        },
        HelpEntry {
            command: "Space",
            description: "Toggle selection on current item",
            usage: "",
            example: "Press Space on 'photo.jpg' to mark it for renaming",
            category: "TUI Navigation",
        },
        HelpEntry {
            command: "a / A",
            description: "Select all / deselect all",
            usage: "",
            example: "Press a to select every visible file, A to clear selection",
            category: "TUI Navigation",
        },
        HelpEntry {
            command: "r",
            description: "Refresh the navigator",
            usage: "",
            example: "Press r to re-scan the current directory",
            category: "TUI Navigation",
        },
        HelpEntry {
            command: "d / D",
            description: "In Config: remove item. Anywhere: toggle dry-run mode.",
            usage: "",
            example: "Press D to enable dry-run (safe mode), status bar shows [DRY-RUN]",
            category: "TUI Navigation",
        },
    ];

    // ── TUI Console ── (generated from parser::CONSOLE_COMMANDS — SSoT)
    // Every command the console accepts is declared in the registry with its
    // help text, so a command can never exist without help.
    for meta in parser::CONSOLE_COMMANDS {
        entries.push(HelpEntry {
            command: Box::leak(meta.help_label().into_boxed_str()),
            description: meta.summary,
            usage: meta.usage,
            example: meta.example,
            category: "TUI Console",
        });
    }

    entries.extend(vec![
        // ── TUI Global ──
        HelpEntry {
            command: "Tab / Shift+Tab",
            description: "Cycle focus forward/backward through panes",
            usage: "",
            example: "Tab moves: Navigator → Preview → Config → Console → Navigator",
            category: "TUI Global",
        },
        HelpEntry {
            command: "? / Esc",
            description: "Open this interactive help / close help or cancel (or type 'help' in the Console)",
            usage: "",
            example: "Press ? to open help, browse with j/k, filter categories with F, Esc to return.",
            category: "TUI Global",
        },
        HelpEntry {
            command: "Ctrl+C / q",
            description: "Quit the application (keys; you can also type 'exit' or 'quit' in the Console)",
            usage: "",
            example: "Ctrl+C quits from anywhere. q quits from Navigator or Help mode.",
            category: "TUI Global",
        },
        HelpEntry {
            command: "S (Config panel)",
            description: "Toggle stacking mode ON/OFF in the Config pane",
            usage: "",
            example: "While focused on the Config pane, press S to enable stacking for chaining multiple commands.",
            category: "TUI Global",
        },
        HelpEntry {
            command: "Esc (Navigator)",
            description: "Cancel an in-progress recursive scan",
            usage: "",
            example: "When scanning a large directory recursively, press Esc in Navigator to cancel and show partial results.",
            category: "TUI Global",
        },
    ]);

    // Append dynamically-generated CLI flag entries (SSoT from args.rs).
    // Group by category (stable sort) so each section appears exactly once and
    // contiguously, even though clap declares the flags in a different order.
    let mut flag_entries = generate_cli_flag_entries();
    flag_entries.sort_by(|a, b| a.category.cmp(b.category));
    entries.extend(flag_entries);

    entries
}
