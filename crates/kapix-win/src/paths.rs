//! Well-known paths, environment expansion and system fonts.

use crate::{Error, wide};
use std::path::PathBuf;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

/// `%APPDATA%\Kapix` (created if missing).
pub fn app_dir() -> crate::Result<PathBuf> {
    let base = std::env::var_os("APPDATA").ok_or_else(|| Error("%APPDATA% is not set".into()))?;
    let dir = PathBuf::from(base).join("Kapix");
    std::fs::create_dir_all(&dir)
        .map_err(|e| Error(format!("cannot create {}: {e}", dir.display())))?;
    Ok(dir)
}

/// `%APPDATA%\Kapix\config.toml`.
pub fn config_path() -> crate::Result<PathBuf> {
    Ok(app_dir()?.join("config.toml"))
}

/// Expand `%VAR%` using the process environment (via `kapix_core::template::expand_env`).
pub fn expand(s: &str) -> String {
    kapix_core::template::expand_env(s, |name| std::env::var(name).ok())
}

/// Read a font file from `%WINDIR%\Fonts` (e.g. `"segoeui.ttf"`), `None` if missing.
pub fn system_font(file_name: &str) -> Option<Vec<u8>> {
    let win =
        std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    std::fs::read(win.join("Fonts").join(file_name)).ok()
}

/// Open a file or URL with the shell (`ShellExecuteW` "open"), e.g. config.toml in the default
/// editor or `ms-settings:regionlanguage`.
pub fn shell_open(target: &str) {
    let t = wide(target);
    // SAFETY: `t` is NUL-terminated and outlives the call; failure is non-fatal and ignored.
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(t.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_dir_is_created() {
        if std::env::var_os("APPDATA").is_some() {
            assert!(super::app_dir().unwrap().is_dir());
        }
    }

    #[test]
    fn missing_font_is_none() {
        assert!(super::system_font("definitely-not-a-font.ttf").is_none());
    }
}
