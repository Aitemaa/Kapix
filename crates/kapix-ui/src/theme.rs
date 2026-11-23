//! Kavox-family design tokens. Values are copied one-for-one from Kavox's `theme.rs`
//! (`Palette::DARK` / `Palette::LIGHT`); egui's premultiplied scrim/shadow colors are converted
//! to straight alpha.

use kapix_core::Rgba;

pub use kapix_core::config::ThemeMode;

/// Control / field corner radius (logical px).
pub const RADIUS_FIELD: f32 = 10.0;
/// Popover / card corner radius.
pub const RADIUS_PAD: f32 = 16.0;
/// Large panel corner radius.
pub const RADIUS_PANEL: f32 = 20.0;
/// Selected segment pill radius.
pub const RADIUS_SEGMENT: f32 = 15.0;

/// Popover shadow: vertical offset and blur (logical px).
pub const SHADOW_OFFSET_Y: f32 = 12.0;
pub const SHADOW_BLUR: f32 = 32.0;

/// The Kavox colour tokens for one theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub is_dark: bool,
    pub bg: Rgba,
    pub panel: Rgba,
    pub surface: Rgba,
    pub surface_2: Rgba,
    pub line: Rgba,
    pub line_strong: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub faint: Rgba,
    /// Primary / selected fill (off-white in dark, near-black in light).
    pub lit: Rgba,
    /// Content drawn on `lit`.
    pub on_lit: Rgba,
    /// Brand red.
    pub red: Rgba,
    /// Red for text / icons on `panel`.
    pub red_text: Rgba,
    pub amber: Rgba,
    /// Focus ring colour.
    pub focus: Rgba,
    /// Dimming layer over the frozen screenshot (straight alpha).
    pub scrim: Rgba,
    /// Popover shadow colour (straight alpha).
    pub shadow: Rgba,
}

impl Palette {
    pub const LIGHT: Palette = Palette {
        is_dark: false,
        bg: Rgba::rgb(0xef, 0xee, 0xe9),
        panel: Rgba::rgb(0xf8, 0xf7, 0xf4),
        surface: Rgba::rgb(0xe4, 0xe3, 0xde),
        surface_2: Rgba::rgb(0xd9, 0xd8, 0xd2),
        line: Rgba::rgb(0xd8, 0xd6, 0xcf),
        line_strong: Rgba::rgb(0xbd, 0xbb, 0xb4),
        text: Rgba::rgb(0x19, 0x19, 0x19),
        muted: Rgba::rgb(0x69, 0x68, 0x63),
        faint: Rgba::rgb(0xa3, 0xa1, 0x9a),
        lit: Rgba::rgb(0x1e, 0x1e, 0x1e),
        on_lit: Rgba::rgb(0xf4, 0xf3, 0xef),
        red: Rgba::rgb(0xe8, 0x26, 0x2d),
        red_text: Rgba::rgb(0xc2, 0x16, 0x1c),
        amber: Rgba::rgb(0xa8, 0x64, 0x00),
        focus: Rgba::rgb(0x19, 0x19, 0x19),
        // egui premultiplied (7, 7, 7, 89) == rgb(20 20 20 / 0.35)
        scrim: Rgba::rgb(20, 20, 20).with_alpha(89),
        // premultiplied black @ 64/255 (~25 %)
        shadow: Rgba::rgb(0, 0, 0).with_alpha(64),
    };

    pub const DARK: Palette = Palette {
        is_dark: true,
        bg: Rgba::rgb(0x14, 0x14, 0x14),
        panel: Rgba::rgb(0x1b, 0x1b, 0x1b),
        surface: Rgba::rgb(0x22, 0x22, 0x22),
        surface_2: Rgba::rgb(0x2b, 0x2b, 0x2b),
        line: Rgba::rgb(0x2b, 0x2b, 0x2b),
        line_strong: Rgba::rgb(0x3d, 0x3d, 0x3d),
        text: Rgba::rgb(0xf2, 0xf1, 0xee),
        muted: Rgba::rgb(0x8f, 0x8e, 0x89),
        faint: Rgba::rgb(0x5c, 0x5b, 0x57),
        lit: Rgba::rgb(0xf2, 0xf1, 0xee),
        on_lit: Rgba::rgb(0x14, 0x14, 0x14),
        red: Rgba::rgb(0xe8, 0x26, 0x2d),
        red_text: Rgba::rgb(0xff, 0x4d, 0x53),
        amber: Rgba::rgb(0xf2, 0xa9, 0x3b),
        focus: Rgba::rgb(0xf2, 0xf1, 0xee),
        // black @ 60 %
        scrim: Rgba::rgb(0, 0, 0).with_alpha(153),
        // premultiplied black @ 160/255 (~63 %)
        shadow: Rgba::rgb(0, 0, 0).with_alpha(160),
    };

    /// Pick the palette for a resolved light/dark decision.
    pub const fn for_light(light: bool) -> Palette {
        if light { Self::LIGHT } else { Self::DARK }
    }

    /// Resolve `mode`; `system_light` is what the OS reports (only read for `System`).
    pub fn resolve(mode: ThemeMode, system_light: impl FnOnce() -> bool) -> Palette {
        match mode {
            ThemeMode::Light => Self::LIGHT,
            ThemeMode::Dark => Self::DARK,
            ThemeMode::System => Self::for_light(system_light()),
        }
    }
}

/// Linear sRGB-space blend `a * weight_a + b * (1 - weight_a)` (Kavox's `mix`).
pub(crate) fn mix(a: Rgba, b: Rgba, weight_a: f32) -> Rgba {
    let wa = weight_a.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 * wa + y as f32 * (1.0 - wa)).round() as u8;
    Rgba {
        r: m(a.r, b.r),
        g: m(a.g, b.g),
        b: m(a.b, b.b),
        a: m(a.a, b.a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_match_kavox() {
        assert_eq!(Palette::DARK.lit, Palette::DARK.text);
        assert_eq!(Palette::DARK.panel.hex(), "#1B1B1B");
        assert_eq!(Palette::LIGHT.bg.hex(), "#EFEEE9");
        assert_eq!(Palette::LIGHT.focus, Palette::LIGHT.text);
        assert_eq!(Palette::DARK.scrim.a, 153);
        assert_eq!(Palette::LIGHT.scrim, Rgba::rgb(20, 20, 20).with_alpha(89));
    }

    #[test]
    fn resolve_follows_mode() {
        assert_eq!(Palette::resolve(ThemeMode::Dark, || true), Palette::DARK);
        assert_eq!(Palette::resolve(ThemeMode::Light, || false), Palette::LIGHT);
        assert_eq!(Palette::resolve(ThemeMode::System, || true), Palette::LIGHT);
        assert_eq!(Palette::resolve(ThemeMode::System, || false), Palette::DARK);
    }
}
