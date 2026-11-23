//! Well-known paths, environment expansion and system fonts.

use std::path::PathBuf;

/// `%APPDATA%\Kapix` (created if missing).
pub fn app_dir() -> crate::Result<PathBuf> {
    todo!()
}

/// `%APPDATA%\Kapix\config.toml`.
pub fn config_path() -> crate::Result<PathBuf> {
    todo!()
}

/// Expand `%VAR%` using the process environment (via `kapix_core::template::expand_env`).
pub fn expand(s: &str) -> String {
    todo!()
}

/// Read a font file from `%WINDIR%\Fonts` (e.g. `"segoeui.ttf"`), `None` if missing.
pub fn system_font(file_name: &str) -> Option<Vec<u8>> {
    todo!()
}

/// Open a file or URL with the shell (`ShellExecuteW` "open"), e.g. config.toml in the default
/// editor or `ms-settings:regionlanguage`.
pub fn shell_open(target: &str) {
    todo!()
}
