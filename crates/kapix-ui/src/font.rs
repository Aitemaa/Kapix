//! System-font text rendering via `ab_glyph`. Fonts are loaded from bytes supplied by the host
//! (on Windows: `%WINDIR%\Fonts\segoeui.ttf`); nothing is embedded in the binary.

use ab_glyph::{Font as _, FontArc, Glyph, PxScale, ScaleFont, point};
use kapix_core::Rgba;

use crate::canvas::blend_px;

#[derive(Clone)]
pub struct Font {
    inner: FontArc,
}

impl Font {
    /// Parse TTF/OTF bytes. `None` if the data isn't a valid font.
    pub fn from_bytes(data: Vec<u8>) -> Option<Font> {
        FontArc::try_from_vec(data).ok().map(|inner| Font { inner })
    }

    /// `(width, height)` in px of a single line of `text` at `px` size
    /// (height = ascent - descent).
    pub fn measure(&self, text: &str, px: f32) -> (f32, f32) {
        let scaled = self.inner.as_scaled(PxScale::from(px));
        (
            self.line_width(text, px),
            scaled.ascent() - scaled.descent(),
        )
    }

    /// Line height (ascent - descent + line gap) at `px`.
    pub fn line_height(&self, px: f32) -> f32 {
        let s = self.inner.as_scaled(PxScale::from(px));
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

    /// Same as [`Font::draw`] but with a faux-bold look (double-struck with a sub-pixel offset).
    pub fn draw_bold(
        &self,
        pixmap: &mut tiny_skia::Pixmap,
        text: &str,
        x: f32,
        y: f32,
        px: f32,
        color: Rgba,
    ) {
        let o = (px * 0.028).max(0.3);
        self.draw(pixmap, text, x, y, px, color);
        self.draw(pixmap, text, x + o, y, px, color);
    }

    /// Width of the bold variant drawn by [`Font::draw_bold`].
    pub fn measure_bold(&self, text: &str, px: f32) -> f32 {
        self.line_width(text, px) + (px * 0.028).max(0.3)
    }

    fn line_width(&self, text: &str, px: f32) -> f32 {
        let scaled = self.inner.as_scaled(PxScale::from(px));
        let mut w = 0.0;
        let mut prev = None;
        for c in text.chars().filter(|c| !c.is_control()) {
            let id = scaled.glyph_id(c);
            if let Some(p) = prev {
                w += scaled.kern(p, id);
            }
            w += scaled.h_advance(id);
            prev = Some(id);
        }
        w
    }

    fn for_each_glyph(
        &self,
        text: &str,
        x: f32,
        y: f32,
        px: f32,
        mut f: impl FnMut(&ab_glyph::OutlinedGlyph),
    ) {
        let scaled = self.inner.as_scaled(PxScale::from(px));
        let baseline = y + scaled.ascent();
        let mut caret = x;
        let mut prev = None;
        for c in text.chars().filter(|c| !c.is_control()) {
            let id = scaled.glyph_id(c);
            if let Some(p) = prev {
                caret += scaled.kern(p, id);
            }
            let glyph: Glyph =
                id.with_scale_and_position(PxScale::from(px), point(caret, baseline));
            caret += scaled.h_advance(id);
            prev = Some(id);
            if let Some(og) = self.inner.outline_glyph(glyph) {
                f(&og);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test_font {
    use super::Font;

    /// Look for any usable TTF on the machine; tests skip text assertions when none exists.
    pub fn load() -> Option<Font> {
        const CANDIDATES: &[&str] = &[
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/TTF/Roboto-Regular.ttf",
            "C:\\Windows\\Fonts\\segoeui.ttf",
        ];
        CANDIDATES
            .iter()
            .find_map(|p| std::fs::read(p).ok().and_then(Font::from_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage() {
        assert!(Font::from_bytes(vec![1, 2, 3]).is_none());
    }

    #[test]
    fn measure_and_draw() {
        let Some(f) = test_font::load() else { return };
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
        assert!(pm.data().chunks_exact(4).any(|p| p[0] > 200));
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
    fn color_channel_order_is_bgra() {
        let Some(f) = test_font::load() else { return };
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
}
