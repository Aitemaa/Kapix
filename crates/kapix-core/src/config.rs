//! `%APPDATA%\Kapix\config.toml` schema. Every field has a default so partial files work.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub hotkeys: HotkeysConfig,
    pub capture: CaptureConfig,
    pub save: SaveConfig,
    pub ocr: OcrConfig,
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
        todo!("region=Ctrl+Shift+S ocr=Ctrl+Shift+O fullscreen=Ctrl+Shift+F active_window=Ctrl+Shift+A")
    }
}

impl Default for CaptureConfig {
    fn default() -> Self {
        todo!("annotate, magnifier on, snap on, sound off")
    }
}

impl Default for SaveConfig {
    fn default() -> Self {
        todo!("auto_save off, %USERPROFILE%\\Pictures\\Kapix, Kapix_{{YYYY-MM-DD}}_{{HH-mm-ss}}, png")
    }
}

impl Default for OcrConfig {
    fn default() -> Self {
        todo!("auto, true")
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
        todo!()
    }
    /// Commented default config file written on first run (must round-trip through `parse`
    /// to `Config::default()`).
    pub fn default_toml() -> &'static str {
        todo!()
    }
    /// Validate hotkey strings; returns human-readable problems (empty = OK).
    pub fn validate(&self) -> Vec<String> {
        todo!()
    }
}
