//! `%APPDATA%\Kapix\config.toml` schema. Every field has a default so partial files work.

use serde::{Deserialize, Serialize};

use crate::hotkey::Hotkey;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub hotkeys: HotkeysConfig,
    pub capture: CaptureConfig,
    pub save: SaveConfig,
    pub ocr: OcrConfig,
    pub ui: UiConfig,
}

/// Which palette the overlay and toasts use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    /// Follow the Windows "app mode" (light / dark) setting.
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UiConfig {
    pub theme: ThemeMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeysConfig {
    pub region: String,
    pub ocr: String,
    pub fullscreen: String,
    pub active_window: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DefaultAction {
    #[default]
    Annotate,
    InstantCopy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureConfig {
    pub default_action: DefaultAction,
    pub show_magnifier: bool,
    pub window_snap: bool,
    pub play_sound: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
}

impl ImageFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveConfig {
    pub auto_save: bool,
    /// May contain `%ENV%` variables; expand with [`crate::template::expand_env`].
    pub directory: String,
    pub filename_template: String,
    pub format: ImageFormat,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OcrConfig {
    /// `"auto"` = Windows user-profile languages; otherwise a BCP-47 tag like `"en-US"`.
    pub language: String,
    pub copy_notification: bool,
}

impl Default for HotkeysConfig {
    fn default() -> Self {
        Self {
            region: "Ctrl+Shift+S".into(),
            ocr: "Ctrl+Shift+O".into(),
            fullscreen: "Ctrl+Shift+F".into(),
            active_window: "Ctrl+Shift+A".into(),
        }
    }
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            default_action: DefaultAction::Annotate,
            show_magnifier: true,
            window_snap: true,
            play_sound: false,
        }
    }
}

impl Default for SaveConfig {
    fn default() -> Self {
        Self {
            auto_save: false,
            directory: "%USERPROFILE%\\Pictures\\Kapix".into(),
            filename_template: "Kapix_{YYYY-MM-DD}_{HH-mm-ss}".into(),
            format: ImageFormat::Png,
        }
    }
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            language: "auto".into(),
            copy_notification: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "config error: {}", self.0)
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Parse TOML. Unknown keys are ignored; missing keys take defaults.
    pub fn parse(s: &str) -> Result<Config, ConfigError> {
        toml::from_str(s).map_err(|e| ConfigError(e.to_string()))
    }
    /// Commented default config file written on first run (must round-trip through `parse`
    /// to `Config::default()`).
    pub fn default_toml() -> &'static str {
        DEFAULT_TOML
    }
    /// Validate hotkey strings; returns human-readable problems (empty = OK).
    pub fn validate(&self) -> Vec<String> {
        let h = &self.hotkeys;
        let entries = [
            ("hotkeys.region", &h.region),
            ("hotkeys.ocr", &h.ocr),
            ("hotkeys.fullscreen", &h.fullscreen),
            ("hotkeys.active_window", &h.active_window),
        ];
        let mut problems = Vec::new();
        let mut parsed: Vec<(&str, Hotkey)> = Vec::new();
        for (name, value) in entries {
            match Hotkey::parse(value) {
                Ok(hk) => {
                    if let Some((other, _)) = parsed.iter().find(|(_, o)| *o == hk) {
                        problems.push(format!(
                            "{name} and {other} use the same hotkey ({})",
                            hk.display()
                        ));
                    }
                    parsed.push((name, hk));
                }
                Err(e) => problems.push(format!("{name}: {e}")),
            }
        }
        problems
    }
}

const DEFAULT_TOML: &str = r#"# Kapix configuration. Delete a line to fall back to its default.

[hotkeys]
region = "Ctrl+Shift+S"
ocr = "Ctrl+Shift+O"
fullscreen = "Ctrl+Shift+F"
active_window = "Ctrl+Shift+A"

[capture]
# "annotate" (show floating toolbar) | "instant_copy" (copy immediately on release)
default_action = "annotate"
show_magnifier = true
window_snap = true
play_sound = false

[save]
auto_save = false
directory = "%USERPROFILE%\\Pictures\\Kapix"
filename_template = "Kapix_{YYYY-MM-DD}_{HH-mm-ss}"
format = "png" # png | jpeg

[ocr]
# "auto" uses Windows user profile languages; or BCP-47 tag e.g. "en-US", "ja"
language = "auto"
copy_notification = true

[ui]
# "system" follows the Windows app mode (light/dark) | "dark" | "light"
theme = "system"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_toml_round_trips() {
        assert_eq!(
            Config::parse(Config::default_toml()).unwrap(),
            Config::default()
        );
        assert!(Config::default_toml().contains('#'));
        assert_eq!(
            Config::default().save.directory,
            "%USERPROFILE%\\Pictures\\Kapix"
        );
    }

    #[test]
    fn defaults_match_prd() {
        let c = Config::default();
        assert_eq!(c.hotkeys.region, "Ctrl+Shift+S");
        assert_eq!(c.hotkeys.active_window, "Ctrl+Shift+A");
        assert_eq!(c.capture.default_action, DefaultAction::Annotate);
        assert!(c.capture.show_magnifier && c.capture.window_snap && !c.capture.play_sound);
        assert!(!c.save.auto_save);
        assert_eq!(c.save.filename_template, "Kapix_{YYYY-MM-DD}_{HH-mm-ss}");
        assert_eq!(c.save.format.extension(), "png");
        assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
        assert_eq!(c.ocr.language, "auto");
        assert!(c.ocr.copy_notification);
        assert_eq!(c.ui.theme, ThemeMode::System);
        assert!(c.validate().is_empty());
    }

    #[test]
    fn partial_and_empty_files_keep_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
        let c =
            Config::parse("[hotkeys]\nregion = \"Ctrl+Alt+X\"\n[ocr]\nlanguage = \"ja\"").unwrap();
        assert_eq!(c.hotkeys.region, "Ctrl+Alt+X");
        assert_eq!(c.hotkeys.ocr, "Ctrl+Shift+O");
        assert_eq!(c.ocr.language, "ja");
        assert!(c.ocr.copy_notification);
        assert_eq!(c.save, SaveConfig::default());
    }

    #[test]
    fn unknown_keys_ignored() {
        let c = Config::parse(
            "future = 1\n[capture]\nplay_sound = true\nwhatever = \"x\"\n[extra]\na = 1",
        )
        .unwrap();
        assert!(c.capture.play_sound);
        assert!(c.capture.window_snap);
    }

    #[test]
    fn enums_parse() {
        let c = Config::parse("[capture]\ndefault_action = \"instant_copy\"\n[save]\nformat = \"jpeg\"\nauto_save = true").unwrap();
        assert_eq!(c.capture.default_action, DefaultAction::InstantCopy);
        assert_eq!(c.save.format, ImageFormat::Jpeg);
        assert!(c.save.auto_save);
        assert!(Config::parse("[capture]\ndefault_action = \"bogus\"").is_err());
        let c = Config::parse("[ui]\ntheme = \"light\"").unwrap();
        assert_eq!(c.ui.theme, ThemeMode::Light);
        let c = Config::parse("[ui]\ntheme = \"dark\"").unwrap();
        assert_eq!(c.ui.theme, ThemeMode::Dark);
        assert!(Config::parse("[ui]\ntheme = \"neon\"").is_err());
    }

    #[test]
    fn invalid_toml_errors() {
        let e = Config::parse("[hotkeys\nregion =").unwrap_err();
        assert!(e.to_string().starts_with("config error:"));
        assert!(Config::parse("[capture]\nshow_magnifier = \"yes\"").is_err());
    }

    #[test]
    fn validate_reports_bad_and_duplicates() {
        let mut c = Config::default();
        c.hotkeys.ocr = "Ctrl+Shift".into();
        c.hotkeys.fullscreen = "ctrl+shift+s".into();
        let p = c.validate();
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(p.iter().any(|m| m.contains("hotkeys.ocr")));
        assert!(
            p.iter()
                .any(|m| m.contains("hotkeys.fullscreen") && m.contains("hotkeys.region"))
        );
    }
}
