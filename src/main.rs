#![cfg_attr(all(windows, feature = "gui"), windows_subsystem = "windows")]

mod args;
mod cli;
#[cfg(feature = "gui")]
mod gui;
mod launcher;
mod tui;

fn main() {
    launcher::run();
}
