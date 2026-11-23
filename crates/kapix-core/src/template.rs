//! Filename templating and `%ENV%` expansion.

/// Local wall-clock time (supplied by the platform layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millis: u16,
}

/// Render a filename (without extension). Inside each `{...}` group, the tokens
/// `YYYY`, `YY`, `MM`, `DD`, `HH`, `mm`, `ss`, `ms` are replaced by zero-padded values and other
/// characters are kept (so `{YYYY-MM-DD}` → `2026-11-23`). Text outside braces is literal.
/// Unknown/unterminated groups are kept literally. The result is sanitized: Windows-invalid
/// filename characters `<>:"/\|?*` and control chars become `_`, trailing dots/spaces are
/// trimmed, and an empty result becomes `"Kapix"`.
pub fn render_filename(template: &str, now: &DateTime) -> String {
    todo!()
}

/// Expand `%NAME%` variables using `lookup` (case-insensitive name). Unknown variables and a
/// lone `%` are kept literally.
pub fn expand_env(s: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    todo!()
}

/// If `name.ext` exists according to `exists`, try `name (2).ext`, `name (3).ext`, ...
pub fn unique_name(name: &str, ext: &str, exists: impl Fn(&str) -> bool) -> String {
    todo!()
}
