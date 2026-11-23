//! Toast notification content (the host shows it in a layered, non-activating topmost window).

use kapix_core::Image;

use crate::font::Font;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub kind: ToastKind,
    pub title: String,
    /// Optional second line (e.g. OCR text preview or "42 chars · 3 lines").
    pub body: String,
}

/// Render a rounded dark pill/card with an accent dot for `kind`, title and body.
/// Output is **premultiplied BGRA with transparent corners** suitable for
/// `UpdateLayeredWindow(ULW_ALPHA)`. `scale` = monitor DPI scale (1.0 = 96 DPI).
/// Width adapts to text (min 220, max 420 logical px). Without a font, renders the card only.
pub fn render(toast: &Toast, font: Option<&Font>, scale: f32) -> Image {
    todo!()
}
