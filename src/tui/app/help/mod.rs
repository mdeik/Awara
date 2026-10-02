use super::*;

/// Custom metadata for CLI flags that can't be derived from clap: usage example, input/output example, category.
/// Keyed by clap long name. Flags not listed here still get a basic entry from clap's help text.
// The console help: command metadata, generated entries, and the help panel.
mod entries;
mod flag_meta;
mod methods; // App methods for the help panel

// FLAG_META powers generate_cli_flag_entries (entries.rs).
use flag_meta::FLAG_META;

pub use entries::{generate_cli_flag_entries, help_entries};

/// One entry in the in-app help panel: a command, what it does, and how to use it.
pub struct HelpEntry {
    pub command: &'static str,
    pub description: &'static str,
    pub usage: &'static str,
    pub example: &'static str,
    pub category: &'static str,
}
