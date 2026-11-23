//! BGRA-order helpers around `tiny_skia::Pixmap`.

use kapix_core::{Image, Rgba};

/// Convert a straight RGBA color into a tiny-skia color **with R and B swapped**, so drawing
/// into a pixmap whose bytes are interpreted as BGRA produces the intended color.
pub fn sk_color(c: Rgba) -> tiny_skia::Color {
    todo!()
}

/// Solid paint (anti-aliased) for `c`.
pub fn paint(c: Rgba) -> tiny_skia::Paint<'static> {
    todo!()
}

/// Move an [`Image`] into a pixmap without copying pixel bytes when possible.
/// Panics if the image is 0×0.
pub fn image_to_pixmap(img: Image) -> tiny_skia::Pixmap {
    todo!()
}

/// Move a pixmap back into an [`Image`] (BGRA bytes kept as-is).
pub fn pixmap_to_image(p: tiny_skia::Pixmap) -> Image {
    todo!()
}

/// Copy `src` rect of `from` into `to` at `(dx, dy)` (clipped; plain copy, no blending).
pub fn blit(
    from: &tiny_skia::Pixmap,
    src: kapix_core::Rect,
    to: &mut tiny_skia::Pixmap,
    dx: i32,
    dy: i32,
) {
    todo!()
}
