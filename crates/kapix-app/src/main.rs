//! Kapix entrypoint. On Windows: single-instance tray daemon + CLI. Elsewhere: prints a notice.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod app;

#[cfg(windows)]
fn main() {
    std::process::exit(app::run());
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Kapix only runs on Windows.");
    std::process::exit(1);
}
