//! Text rendering via `ab_glyph`. Kapix embeds the Kavox-family Archivo typeface (Latin subset:
//! Regular, Medium and SemiExpanded SemiBold); characters Archivo lacks are drawn with an
//! optional fallback face supplied by the host (on Windows: `%WINDIR%\Fonts\segoeui.ttf`).
//!
//! All `px` arguments are **font sizes** (CSS-style em size), so "14" means the same as in
//! Kavox's UI.

use std::sync::{Arc, OnceLock};

use ab_glyph::{Font as _, FontArc, Glyph, GlyphId, PxScale, ScaleFont, point};
use kapix_core::Rgba;

use crate::canvas::blend_px;

const ARCHIVO_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Archivo-Regular.ttf");
const ARCHIVO_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/Archivo-Medium.ttf");
const ARCHIVO_HEADING: &[u8] =
    include_bytes!("../../../assets/fonts/ArchivoSemiExpanded-SemiBold.ttf");

/// Which embedded Archivo cut to draw with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    /// Body text.
    Regular,
    /// Labels and buttons.
    Medium,
    /// Headings (SemiExpanded SemiBold).
    Heading,
}

struct Faces {
    archivo: [FontArc; 3],
    fallback: Option<FontArc>,
}

fn archivo() -> &'static [FontArc; 3] {
    static FACES: OnceLock<[FontArc; 3]> = OnceLock::new();
    FACES.get_or_init(|| {
        let load =
            |b: &'static [u8]| FontArc::try_from_slice(b).expect("embedded Archivo is valid");
        [
            load(ARCHIVO_REGULAR),
            load(ARCHIVO_MEDIUM),
            load(ARCHIVO_HEADING),
        ]
    })
}

/// The Kapix font family at one weight; clones are cheap (shared faces).
#[derive(Clone)]
pub struct Font {
    faces: Arc<Faces>,
    weight: Weight,
}

impl Font {
    /// Archivo plus an optional fallback face (TTF/OTF bytes; invalid data is ignored).
    pub fn new(fallback: Option<Vec<u8>>) -> Font {
        Font {
            faces: Arc::new(Faces {
                archivo: archivo().clone(),
                fallback: fallback.and_then(|d| FontArc::try_from_vec(d).ok()),
            }),
            weight: Weight::Regular,
        }
    }

    /// Archivo only (no fallback for non-Latin scripts).
    pub fn builtin() -> Font {
        Font::new(None)
    }

    pub fn with_weight(&self, weight: Weight) -> Font {
        Font {
            faces: self.faces.clone(),
            weight,
        }
    }
    pub fn regular(&self) -> Font {
        self.with_weight(Weight::Regular)
    }
    pub fn medium(&self) -> Font {
        self.with_weight(Weight::Medium)
    }
    pub fn heading(&self) -> Font {
        self.with_weight(Weight::Heading)
    }

    fn primary(&self) -> &FontArc {
        &self.faces.archivo[self.weight as usize]
    }

    /// `PxScale` that makes `face` render at em size `px`.
    fn scale_for(face: &FontArc, px: f32) -> PxScale {
        let upem = face.units_per_em().unwrap_or(1000.0);
        PxScale::from(px * face.height_unscaled() / upem)
    }

    /// `(width, height)` in px of a single line of `text` at `px` size
    /// (height = ascent - descent).
    pub fn measure(&self, text: &str, px: f32) -> (f32, f32) {
        let p = self.primary();
        let scaled = p.as_scaled(Self::scale_for(p, px));
        (
            self.line_width(text, px),
            scaled.ascent() - scaled.descent(),
        )
    }

    /// Line height (ascent - descent + line gap) at `px`.
    pub fn line_height(&self, px: f32) -> f32 {
        let p = self.primary();
        let s = p.as_scaled(Self::scale_for(p, px));
        s.ascent() - s.descent() + s.line_gap()
    }

    /// Draw one line with its **top-left** at `(x, y)`, alpha-blended (source-over) into a
    /// BGRA pixmap. Supports kerning; clips to the pixmap.
    pub fn draw(
        &self,
        pixmap: &mut tiny_skia::Pixmap,
        text: &str,
        x: f32,
        y: f32,
        px: f32,
        color: Rgba,
    ) {
        self.for_each_glyph(text, x, y, px, |og| {
            let b = og.px_bounds();
            let (ox, oy) = (b.min.x as i32, b.min.y as i32);
            let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
            if ox >= w || oy >= h || b.max.x < 0.0 || b.max.y < 0.0 {
                return;
            }
            let data = pixmap.data_mut();
            og.draw(|gx, gy, cov| {
                let (tx, ty) = (ox + gx as i32, oy + gy as i32);
                if cov > 0.0 && tx >= 0 && ty >= 0 && tx < w && ty < h {
                    let i = (ty as usize * w as usize + tx as usize) * 4;
                    blend_px(&mut data[i..i + 4], color, cov);
                }
            });
        });
    }

    /// Multi-line (`\n`) variant of [`Font::draw`]; returns the drawn `(width, height)`.
    pub fn draw_multiline(
        &self,
        pixmap: &mut tiny_skia::Pixmap,
        text: &str,
        x: f32,
        y: f32,
        px: f32,
        color: Rgba,
    ) -> (f32, f32) {
        let lh = self.line_height(px);
        for (i, line) in text.split('\n').enumerate() {
            self.draw(pixmap, line, x, y + lh * i as f32, px, color);
        }
        self.measure_multiline(text, px)
    }

    /// Multi-line measurement matching [`Font::draw_multiline`].
    pub fn measure_multiline(&self, text: &str, px: f32) -> (f32, f32) {
        let lh = self.line_height(px);
        let (_, h1) = self.measure("", px);
        let mut n = 0usize;
        let mut w = 0f32;
        for line in text.split('\n') {
            w = w.max(self.line_width(line, px));
            n += 1;
        }
        (w, lh * (n - 1) as f32 + h1)
    }

    /// Ink bounding box `(x0, y0, x1, y1)` of a single line, relative to the top-left draw
    /// origin. `None` for text without visible glyphs. Used for optical centering.
    pub fn ink_bounds(&self, text: &str, px: f32) -> Option<(f32, f32, f32, f32)> {
        let mut acc: Option<(f32, f32, f32, f32)> = None;
        self.for_each_glyph(text, 0.0, 0.0, px, |og| {
            let b = og.px_bounds();
            acc = Some(match acc {
                None => (b.min.x, b.min.y, b.max.x, b.max.y),
                Some((a, bb, c, d)) => (
                    a.min(b.min.x),
                    bb.min(b.min.y),
                    c.max(b.max.x),
                    d.max(b.max.y),
                ),
            });
        });
        acc
    }

    /// Draw one line so its *ink* is centered on `(cx, cy)`.
    pub fn draw_centered(
        &self,
        pixmap: &mut tiny_skia::Pixmap,
        text: &str,
        cx: f32,
        cy: f32,
        px: f32,
        color: Rgba,
    ) {
        if let Some((x0, y0, x1, y1)) = self.ink_bounds(text, px) {
            self.draw(
                pixmap,
                text,
                cx - (x0 + x1) / 2.0,
                cy - (y0 + y1) / 2.0,
                px,
                color,
            );
        }
    }

    fn line_width(&self, text: &str, px: f32) -> f32 {
        self.walk(text, px, |_, _, _, _| {})
    }

    /// Lay `text` out on a single line, calling `f(face, scale, glyph, caret_x)` for every
    /// glyph; returns the total advance. Characters missing from Archivo use the fallback face.
    fn walk(&self, text: &str, px: f32, mut f: impl FnMut(&FontArc, PxScale, GlyphId, f32)) -> f32 {
        let primary = self.primary();
        let pscale = Self::scale_for(primary, px);
        let fscale = self
            .faces
            .fallback
            .as_ref()
            .map(|fb| Self::scale_for(fb, px));
        let mut caret = 0.0;
        let mut prev: Option<(bool, GlyphId)> = None;
        for c in text.chars().filter(|c| !c.is_control()) {
            let (mut face, mut scale, mut id, mut fb) =
                (primary, pscale, primary.glyph_id(c), false);
            if id.0 == 0 {
                if let (Some(ff), Some(fs)) = (&self.faces.fallback, fscale) {
                    let fid = ff.glyph_id(c);
                    if fid.0 != 0 {
                        (face, scale, id, fb) = (ff, fs, fid, true);
                    }
                }
            }
            let sf = face.as_scaled(scale);
            if let Some((pfb, pid)) = prev {
                if pfb == fb {
                    caret += sf.kern(pid, id);
                }
            }
            f(face, scale, id, caret);
            caret += sf.h_advance(id);
            prev = Some((fb, id));
        }
        caret
    }

    fn for_each_glyph(
        &self,
        text: &str,
        x: f32,
        y: f32,
        px: f32,
        mut f: impl FnMut(&ab_glyph::OutlinedGlyph),
    ) {
        let baseline = y + self.ascent(px);
        self.walk(text, px, |face, scale, id, caret| {
            let glyph: Glyph = id.with_scale_and_position(scale, point(x + caret, baseline));
            if let Some(og) = face.outline_glyph(glyph) {
                f(&og);
            }
        });
    }

    fn ascent(&self, px: f32) -> f32 {
        let p = self.primary();
        p.as_scaled(Self::scale_for(p, px)).ascent()
    }
}

#[cfg(test)]
pub(crate) mod test_font {
    use super::Font;

    /// Archivo only: deterministic on every machine.
    pub fn load() -> Font {
        Font::builtin()
    }

    /// A system face with broad Unicode coverage, when the machine has one.
    pub fn system_fallback() -> Option<Vec<u8>> {
        const CANDIDATES: &[&str] = &[
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "C:\\Windows\\Fonts\\segoeui.ttf",
        ];
        CANDIDATES.iter().find_map(|p| std::fs::read(p).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink(pm: &tiny_skia::Pixmap) -> usize {
        pm.data().chunks_exact(4).filter(|p| p[0] > 100).count()
    }

    #[test]
    fn measure_and_draw() {
        let f = test_font::load();
        let (w1, h1) = f.measure("Hello", 20.0);
        let (w2, _) = f.measure("Hello Hello", 20.0);
        assert!(w1 > 20.0 && w2 > w1 * 1.8, "{w1} {w2}");
        assert!(h1 > 15.0 && h1 < 35.0);
        assert!(f.line_height(20.0) >= h1);
        let (mw, mh) = f.measure_multiline("Hello\nHello Hello", 20.0);
        assert!((mw - w2).abs() < 0.01);
        assert!(mh > h1 + f.line_height(20.0) - 0.01);
        let mut pm = tiny_skia::Pixmap::new(80, 40).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
        f.draw(&mut pm, "Hi", 4.0, 4.0, 24.0, Rgba::rgb(255, 255, 255));
        assert!(ink(&pm) > 20);
        // Clipping far outside is harmless.
        f.draw(
            &mut pm,
            "Hi",
            -500.0,
            -500.0,
            24.0,
            Rgba::rgb(255, 255, 255),
        );
        f.draw(&mut pm, "Hi", 500.0, 500.0, 24.0, Rgba::rgb(255, 255, 255));
    }

    #[test]
    fn weights_differ() {
        let f = test_font::load();
        let (r, m, h) = (
            f.measure("Capture", 14.0).0,
            f.medium().measure("Capture", 14.0).0,
            f.heading().measure("Capture", 14.0).0,
        );
        assert!(m > r && h > m, "{r} {m} {h}");
    }

    #[test]
    fn color_channel_order_is_bgra() {
        let f = test_font::load();
        let mut pm = tiny_skia::Pixmap::new(60, 40).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
        f.draw(&mut pm, "H", 4.0, 2.0, 32.0, Rgba::rgb(255, 0, 0));
        let hit = pm
            .data()
            .chunks_exact(4)
            .find(|p| p[2] > 200)
            .expect("red ink");
        assert!(hit[0] < 30, "blue channel stays low: {hit:?}");
    }

    #[test]
    fn fallback_covers_missing_glyphs() {
        let plain = Font::builtin();
        assert!(
            plain.primary().glyph_id('ж').0 == 0,
            "Latin subset has no Cyrillic"
        );
        let Some(bytes) = test_font::system_fallback() else {
            return;
        };
        let with = Font::new(Some(bytes));
        let draw = |f: &Font| {
            let mut pm = tiny_skia::Pixmap::new(60, 40).unwrap();
            pm.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
            f.draw(&mut pm, "ж", 4.0, 2.0, 28.0, Rgba::rgb(255, 255, 255));
            ink(&pm)
        };
        assert!(draw(&with) > 20, "fallback glyph drawn");
        // Latin text never touches the fallback.
        assert_eq!(with.measure("Hello", 14.0), plain.measure("Hello", 14.0));
        // Invalid fallback bytes are ignored.
        assert!(Font::new(Some(vec![1, 2, 3])).faces.fallback.is_none());
    }
}
