//! Linux (Wayland) OS services for Kapix. Compiles to an empty crate on non-Linux targets.
//!
//! 100% pure Rust over `$WAYLAND_DISPLAY` (`wayrs-client` + `wayrs-protocols` + `rustix`) with
//! zero C libraries (`libwayland`, `libX11`, `libgtk`, `libQt`, `libfontconfig`).
#![cfg(target_os = "linux")]

pub mod capture;
pub mod clipboard;
pub mod dialog;
pub mod encode;
pub mod notify;
pub mod ocr;
pub mod paths;
pub mod shm;
pub mod snap;
pub mod theme;
pub mod time;

/// Error with a human-readable message.
#[derive(Debug, Clone)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
