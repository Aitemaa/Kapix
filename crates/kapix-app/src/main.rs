//! Kapix entrypoint.
//! - Windows: single-instance tray daemon + CLI.
//! - Linux (Wayland): native pure-Rust `zwlr_layer_shell_v1` overlay, floating pin window, CLI.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod app;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(windows)]
fn main() {
    std::process::exit(app::run());
}

#[cfg(target_os = "linux")]
fn main() {
    std::process::exit(linux::run());
}

#[cfg(not(any(windows, target_os = "linux")))]
fn main() {
    eprintln!("Kapix runs on Windows and Linux (Wayland).");
    std::process::exit(1);
}
