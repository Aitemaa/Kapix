//! Hotkey strings like `"Ctrl+Shift+S"` ↔ structured form.

/// A parsed global hotkey. `vk` uses **Windows virtual-key code values** (plain numbers, so
/// this stays platform-independent): letters `A`–`Z` = 0x41–0x5A, digits `0`–`9` = 0x30–0x39,
/// `F1`–`F24` = 0x70–0x87, `PrintScreen` = 0x2C, `Space` = 0x20, `Insert` = 0x2D,
/// `Delete` = 0x2E, `Home` = 0x24, `End` = 0x23, `PageUp` = 0x21, `PageDown` = 0x22.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    pub vk: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyParseError(pub String);

impl std::fmt::Display for HotkeyParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid hotkey: {}", self.0)
    }
}

impl std::error::Error for HotkeyParseError {}

impl Hotkey {
    /// Case-insensitive, whitespace-tolerant. Modifier aliases: Ctrl/Control, Shift, Alt,
    /// Win/Super/Meta. Key aliases: PrintScreen/PrtSc/PrtScn/Print, Space, F1–F24, A–Z, 0–9,
    /// Insert/Ins, Delete/Del, Home, End, PageUp/PgUp, PageDown/PgDn.
    /// Exactly one non-modifier key is required. `PrintScreen` alone (no modifiers) is allowed.
    pub fn parse(s: &str) -> Result<Hotkey, HotkeyParseError> {
        todo!()
    }
    /// Canonical display form, e.g. `"Ctrl+Shift+S"`, `"PrintScreen"`.
    pub fn display(&self) -> String {
        todo!()
    }
}
