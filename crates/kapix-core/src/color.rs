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
        Self {
            r: p[2],
            g: p[1],
            b: p[0],
            a: p[3],
        }
    }
    /// `#RRGGBB` uppercase.
    pub fn hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
    /// Relative luminance 0..1 (sRGB, approximate) — used to pick contrasting text color.
    pub fn luminance(&self) -> f32 {
        fn lin(c: u8) -> f32 {
            let c = c as f32 / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_bgra() {
        assert_eq!(PRESETS[0].hex(), "#EF4444");
        assert_eq!(Rgba::from_bgra([1, 2, 3, 255]), Rgba::rgb(3, 2, 1));
        assert_eq!(Rgba::rgb(1, 2, 3).with_alpha(9).a, 9);
    }

    #[test]
    fn luminance_ordering() {
        assert!(Rgba::rgb(0, 0, 0).luminance() < 0.001);
        assert!((Rgba::rgb(255, 255, 255).luminance() - 1.0).abs() < 0.001);
        assert!(PRESETS[4].luminance() > PRESETS[5].luminance());
        assert!(PRESETS[1].luminance() > 0.4);
    }
}
