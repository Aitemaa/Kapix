//! System-font text rendering via `ab_glyph`. Fonts are loaded from bytes supplied by the host
//! (on Windows: `%WINDIR%\Fonts\segoeui.ttf`); nothing is embedded in the binary.

use kapix_core::Rgba;

#[derive(Clone)]
pub struct Font {
    inner: ab_glyph::FontArc,
}

impl Font {
    /// Parse TTF/OTF bytes. `None` if the data isn't a valid font.
    pub fn from_bytes(data: Vec<u8>) -> Option<Font> {
        todo!()
    }
    /// `(width, height)` in px of a single line of `text` at `px` size
    /// (height = ascent - descent).
    pub fn measure(&self, text: &str, px: f32) -> (f32, f32) {
        todo!()
    }
    /// Line height (ascent - descent + line gap) at `px`.
    pub fn line_height(&self, px: f32) -> f32 {
        todo!()
    }
    /// Draw one line with its **top-left** at `(x, y)`, alpha-blended (source-over) into a
    /// BGRA pixmap. Supports kerning; clips to the pixmap.
    pub fn draw(&self, pixmap: &mut tiny_skia::Pixmap, text: &str, x: f32, y: f32, px: f32, color: Rgba) {
        todo!()
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
        todo!()
    }
    /// Multi-line measurement matching [`Font::draw_multiline`].
    pub fn measure_multiline(&self, text: &str, px: f32) -> (f32, f32) {
        todo!()
    }
}
