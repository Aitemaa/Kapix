//! Colors and the 6 curated annotation presets.

/// Straight (non-premultiplied) RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }
    /// From a BGRA pixel as stored in [`crate::Image`].
    pub const fn from_bgra(p: [u8; 4]) -> Self {
        Self { r: p[2], g: p[1], b: p[0], a: p[3] }
    }
    /// `#RRGGBB` uppercase.
    pub fn hex(&self) -> String {
        todo!()
    }
    /// Relative luminance 0..1 (sRGB, approximate) — used to pick contrasting text color.
    pub fn luminance(&self) -> f32 {
        todo!()
    }
}

/// Preset palette, keys `1`–`6`: Red, Amber, Emerald, Sky, White, Ink.
pub const PRESETS: [Rgba; 6] = [
    Rgba::rgb(0xEF, 0x44, 0x44),
    Rgba::rgb(0xF5, 0x9E, 0x0B),
    Rgba::rgb(0x10, 0xB9, 0x81),
    Rgba::rgb(0x0E, 0xA5, 0xE9),
    Rgba::rgb(0xFF, 0xFF, 0xFF),
    Rgba::rgb(0x11, 0x18, 0x27),
];

pub const PRESET_NAMES: [&str; 6] = ["Red", "Amber", "Emerald", "Sky", "White", "Ink"];
