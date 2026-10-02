use crate::args::Args;
use crate::cli;
#[cfg(feature = "gui")]
use crate::gui;
use crate::tui;
use clap::Parser;

/// Describes how the --gui fast-path should behave.
/// Extracted into its own type so the dispatch logic is testable.
#[cfg(feature = "gui")]
#[derive(Debug, PartialEq, Eq)]
enum GuiDispatch {
    /// No files passed — open in CWD.
    NoFiles { theme: String },
    /// Single path that is a directory — open showing that directory.
    Directory {
        theme: String,
        dir: std::path::PathBuf,
    },
    /// One or more files — open showing that directory with the files selected.
    Files {
        theme: String,
        paths: Vec<std::path::PathBuf>,
    },
}

#[cfg(feature = "gui")]
fn parse_gui_args(args: &[String]) -> GuiDispatch {
    let mut theme = "system".to_string();
    let mut i = 1; // args[0] is "--gui", skip it
    // First optional arg may be a theme keyword
    if i < args.len() && matches!(args[i].as_str(), "dark" | "light" | "system") {
        theme = args[i].clone();
        i += 1;
    }
    let files: Vec<std::path::PathBuf> = args[i..].iter().map(std::path::PathBuf::from).collect();

    if files.is_empty() {
        return GuiDispatch::NoFiles { theme };
    }
    if files.len() == 1 && files[0].is_dir() {
        return GuiDispatch::Directory {
            theme,
            dir: files.into_iter().next().unwrap(),
        };
    }
    GuiDispatch::Files {
        theme,
        paths: files,
    }
}

pub fn run() {
    let args_raw: Vec<String> = std::env::args().collect();
    let has_args = args_raw.len() >= 2;

    // Fast-path: --tui bypasses clap entirely
    if has_args && args_raw[1] == "--tui" {
        return run_tui();
    }

    // Fast-path: --gui [theme] [files...]
    // Used by platform context-menu integration (Windows, macOS, Linux).
    // Positional args after --gui are files/directories passed by the OS.
    #[cfg(feature = "gui")]
    if has_args && args_raw[1] == "--gui" {
        return match parse_gui_args(&args_raw[1..]) {
            GuiDispatch::NoFiles { theme } => run_gui_or_fallback(&theme, None),
            GuiDispatch::Directory { theme, dir } => run_gui_or_fallback_dir(&theme, &dir),
            GuiDispatch::Files { theme, paths } => run_gui_or_fallback_files(&theme, &paths),
        };
    }

    // No CLI arguments at all → auto-detect mode
    if !has_args {
        #[cfg(feature = "gui")]
        if gui::is_supported() {
            return run_gui_or_fallback("system", None);
        }
        return run_tui();
    }

    // Parse full args (including --gui/--tui among other flags)
    let args = Args::parse();

    // Explicit --gui or --tui with other args
    if args.gui {
        #[cfg(feature = "gui")]
        {
            // Load preset if specified, before launching GUI
            let preset =
                args.load_preset.as_ref().and_then(
                    |path| match awara::load_rename_doc_with_warnings(std::path::Path::new(path)) {
                        Ok(loaded) => {
                            for warning in &loaded.warnings {
                                eprintln!("Warning: preset '{}': {}", path, warning);
                            }
                            Some(loaded.doc)
                        }
                        Err(e) => {
                            eprintln!("Warning: preset '{}' is not valid: {}", path, e);
                            None
                        }
                    },
                );
            return run_gui_or_fallback(args.theme.as_deref().unwrap_or(""), preset);
        }
        #[cfg(not(feature = "gui"))]
        {
            eprintln!("GUI support not compiled. Rebuild with the `gui` feature or use --tui.");
            std::process::exit(1);
        }
    }

    if args.tui {
        return run_tui();
    }

    // No GUI/TUI flags - run the batch CLI.
    cli::run(args);
}

#[cfg(feature = "gui")]
fn run_gui_or_fallback(theme: &str, preset: Option<awara::RenameDoc>) {
    match gui::run_with_config(theme, preset) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("GUI failed ({}), falling back to TUI", e);
            run_tui();
        }
    }
}

#[cfg(feature = "gui")]
fn run_gui_or_fallback_dir(theme: &str, dir: &std::path::Path) {
    match gui::run_with_dir(theme, dir) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("GUI failed ({}), falling back to TUI", e);
            run_tui();
        }
    }
}

#[cfg(feature = "gui")]
fn run_gui_or_fallback_files(theme: &str, files: &[std::path::PathBuf]) {
    match gui::run_with_files(theme, files) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("GUI failed ({}), falling back to TUI", e);
            run_tui();
        }
    }
}

fn run_tui() {
    if let Err(e) = tui::run() {
        eprintln!("TUI Error: {}", e);
        std::process::exit(1);
    }
}

#[cfg(all(test, feature = "gui"))]
mod tests;
