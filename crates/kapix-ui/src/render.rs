//! Annotation rasterizer shared by live preview and export (pixel-identical WYSIWYG).

use kapix_core::{Annotation, Image, Rect};

use crate::font::Font;

/// Draw `items` in order into `pixmap`, where pixmap pixel (0,0) corresponds to buffer
/// coordinate `origin` (i.e. every annotation is translated by `-origin`).
/// `Pixelate` destructively mosaics the pixmap's *current* pixels in its rect (so it also
/// obscures anything drawn before it). Text is skipped when `font` is `None`.
pub fn render_annotations(
    pixmap: &mut tiny_skia::Pixmap,
    items: &[Annotation],
    origin: (i32, i32),
    font: Option<&Font>,
) {
    todo!()
}

/// Export path: crop `selection` out of the raw frame and render `items` onto it.
/// Returns an opaque BGRA image of `selection ∩ frame.bounds()`.
pub fn flatten(frame: &Image, selection: Rect, items: &[Annotation], font: Option<&Font>) -> Image {
    todo!()
}
