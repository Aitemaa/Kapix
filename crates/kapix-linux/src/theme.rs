//! Fast, zero-D-Bus Linux light/dark theme detection (matches the ka-suite convention).
//!
//! Priority:
//! 1. `KAPIX_THEME=light|dark`
//! 2. `$XDG_CONFIG_HOME/gtk-3.0/settings.ini` or `~/.config/gtk-3.0/settings.ini`
//! 3. `GTK_THEME` environment variable
//! 4. Defaults to dark (`false`), matching Niri/CachyOS and ka-suite defaults.

use std::env;
use std::fs;
use std::path::PathBuf;

/// Returns `true` when the desktop appears to prefer a light theme.
pub fn is_system_light() -> bool {
    if let Ok(override_val) = env::var("KAPIX_THEME") {
        let lower = override_val.trim().to_ascii_lowercase();
        if lower == "light" {
            return true;
        }
        if lower == "dark" {
            return false;
        }
    }

    if let Some(is_dark) = read_gtk3_prefer_dark() {
        return !is_dark;
    }

    if let Ok(gtk_theme) = env::var("GTK_THEME") {
        let lower = gtk_theme.to_ascii_lowercase();
        if lower.contains("dark") {
            return false;
        }
        if lower.contains("light") {
            return true;
        }
    }

    false
}

fn gtk3_settings_path() -> Option<PathBuf> {
    if let Some(cfg) = env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(cfg).join("gtk-3.0/settings.ini"));
    }
    let home = env::var_os("HOME").filter(|v| !v.is_empty())?;
    Some(PathBuf::from(home).join(".config/gtk-3.0/settings.ini"))
}

fn read_gtk3_prefer_dark() -> Option<bool> {
    let path = gtk3_settings_path()?;
    let contents = fs::read_to_string(path).ok()?;
    let mut theme_name_dark: Option<bool> = None;

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim();
        if key.eq_ignore_ascii_case("gtk-application-prefer-dark-theme") {
            if val == "1" || val.eq_ignore_ascii_case("true") {
                return Some(true);
            }
            if val == "0" || val.eq_ignore_ascii_case("false") {
                return Some(false);
            }
        } else if key.eq_ignore_ascii_case("gtk-theme-name") {
            let lower = val.to_ascii_lowercase();
            if lower.contains("dark") {
                theme_name_dark = Some(true);
            } else if lower.contains("light") {
                theme_name_dark = Some(false);
            }
        }
    }

    theme_name_dark
}
