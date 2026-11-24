//! XDG paths, environment expansion, and system fallback fonts on Linux.

use std::env;
use std::fs;
use std::path::PathBuf;

use crate::{Error, Result};

/// Windows default directory in `kapix_core::Config::default()`.
const WIN_DEFAULT_DIR: &str = "%USERPROFILE%\\Pictures\\Kapix";

/// `$XDG_CONFIG_HOME/kapix` or `$HOME/.config/kapix` (created if missing).
pub fn config_dir() -> Result<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .ok_or_else(|| Error("neither $XDG_CONFIG_HOME nor $HOME is set".into()))?;
    let dir = base.join("kapix");
    fs::create_dir_all(&dir).map_err(|e| Error(format!("cannot create {}: {e}", dir.display())))?;
    Ok(dir)
}

/// `$XDG_CONFIG_HOME/kapix/config.toml`.
pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

/// Default screenshot save directory: `$XDG_PICTURES_DIR/Kapix` (also reads
/// `~/.config/user-dirs.dirs`), falling back to `~/Pictures/Kapix`.
pub fn default_pictures_dir() -> PathBuf {
    if let Some(pic) = env::var_os("XDG_PICTURES_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(pic).join("Kapix");
    }
    if let Some(pic) = read_xdg_pictures_dir() {
        return pic.join("Kapix");
    }
    env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(|h| PathBuf::from(h).join("Pictures").join("Kapix"))
        .unwrap_or_else(|| PathBuf::from("Kapix"))
}

fn read_xdg_pictures_dir() -> Option<PathBuf> {
    let home = env::var("HOME").ok().filter(|v| !v.is_empty())?;
    let cfg = env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&home).join(".config"));
    let text = fs::read_to_string(cfg.join("user-dirs.dirs")).ok()?;
    for raw in text.lines() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix("XDG_PICTURES_DIR=") else {
            continue;
        };
        let unquoted = rest.trim().trim_matches('"');
        let expanded = unquoted.replace("$HOME", &home).replace("${HOME}", &home);
        if !expanded.is_empty() {
            return Some(PathBuf::from(expanded));
        }
    }
    None
}

/// Expand save directory paths from `config.toml`.
///
/// Maps the cross-platform default `%USERPROFILE%\Pictures\Kapix` to the XDG Pictures folder,
/// supports leading `~`, `$VAR` / `${VAR}`, and `%VAR%` (mapping `USERPROFILE` to `$HOME`).
pub fn expand(s: &str) -> String {
    if s == WIN_DEFAULT_DIR {
        return default_pictures_dir().to_string_lossy().into_owned();
    }
    let home = env::var("HOME").ok();
    let mut work = if let Some(rest) = s.strip_prefix("~/") {
        match &home {
            Some(h) => format!("{h}/{rest}"),
            None => s.to_string(),
        }
    } else if s == "~" {
        home.clone().unwrap_or_else(|| "~".to_string())
    } else {
        s.to_string()
    };

    // Expand `%VAR%` syntax (e.g. configs shared from Windows).
    work = kapix_core::template::expand_env(&work, |name| {
        env::var(name).ok().or_else(|| {
            if name.eq_ignore_ascii_case("USERPROFILE") {
                home.clone()
            } else {
                None
            }
        })
    });

    // Expand `$VAR` and `${VAR}`.
    work = expand_dollar_env(&work, |name| env::var(name).ok());

    if work.contains('\\') && !work.starts_with('\\') {
        work = work.replace('\\', "/");
    }
    work
}

fn expand_dollar_env(s: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('$') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        if let Some(braced) = after.strip_prefix('{') {
            if let Some(end) = braced.find('}') {
                let name = &braced[..end];
                if !name.is_empty()
                    && let Some(val) = lookup(name)
                {
                    out.push_str(&val);
                    rest = &braced[end + 1..];
                    continue;
                }
            }
            out.push('$');
            rest = after;
        } else {
            let len = after
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .count();
            if len > 0
                && let Some(val) = lookup(&after[..len])
            {
                out.push_str(&val);
                rest = &after[len..];
                continue;
            }
            out.push('$');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Optional system sans-serif fallback font for non-Latin scripts (Archivo handles Latin).
pub fn system_font() -> Option<Vec<u8>> {
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
    ];
    CANDIDATES.iter().find_map(|p| fs::read(p).ok())
}

/// Default `config.toml` tailored for Linux paths (still round-trips cleanly via `Config::parse`).
pub fn default_config_toml() -> String {
    let dir = default_pictures_dir();
    let dir_escaped = dir
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        r#"# Kapix configuration. Delete a line to fall back to its default.
# On Wayland/Niri, bind global hotkeys in your compositor config (see packaging/niri/kapix.kdl).

[capture]
# "annotate" (show floating toolbar) | "instant_copy" (copy immediately on release)
default_action = "annotate"
show_magnifier = true
window_snap = true
play_sound = false

[save]
auto_save = false
directory = "{dir_escaped}"
filename_template = "Kapix_{{YYYY-MM-DD}}_{{HH-mm-ss}}"
format = "png" # png | jpeg

[ocr]
# "auto" uses Tesseract default (eng); or BCP-47 / Tesseract tag e.g. "en-US", "ja", "eng+jpn"
language = "auto"
copy_notification = true

[ui]
# "system" follows KAPIX_THEME / GTK theme (light/dark) | "dark" | "light"
theme = "system"
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dollar_env_expansion() {
        let lookup = |k: &str| match k {
            "HOME" => Some("/home/karel".to_string()),
            "XDG_PICTURES_DIR" => Some("/home/karel/Pics".to_string()),
            _ => None,
        };
        assert_eq!(
            expand_dollar_env("$HOME/Pictures/${XDG_PICTURES_DIR}", lookup),
            "/home/karel/Pictures//home/karel/Pics"
        );
        assert_eq!(expand_dollar_env("$UNKNOWN/a", lookup), "$UNKNOWN/a");
        assert_eq!(expand_dollar_env("plain", lookup), "plain");
    }

    #[test]
    fn default_linux_toml_parses() {
        let toml = default_config_toml();
        let cfg = kapix_core::Config::parse(&toml).expect("valid default linux toml");
        assert_eq!(cfg.save.format, kapix_core::config::ImageFormat::Png);
        assert_eq!(cfg.ocr.language, "auto");
    }
}
