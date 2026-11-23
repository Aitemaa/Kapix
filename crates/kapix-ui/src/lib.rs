//! Pure-Rust, platform-independent Kapix frontend.
//!
//! The Win32 host (in `kapix-app`) owns the windows and translates OS messages into
//! [`overlay::Input`]; everything visual — overlay dimming, selection handles, magnifier loupe,
//! floating toolbar, annotations, toasts — is rasterized here on the CPU with `tiny-skia` and
//! `ab_glyph`. The same renderer produces the live preview and the exported image (WYSIWYG).
//!
//! **Pixel order:** every buffer handled here is BGRA8 (premultiplied; opaque frames are
//! identical either way) so it can be blitted to GDI (`SetDIBitsToDevice` / `UpdateLayeredWindow`)
//! without conversion. `tiny-skia` itself thinks in RGBA, so colors must go through
//! [`canvas::sk_color`], which swaps R and B.

pub mod canvas;
pub mod font;
pub mod overlay;
pub mod render;
pub mod toast;

pub use font::Font;
pub use overlay::{CursorKind, Input, Key, Mods, MouseButton, Outcome, Overlay, OverlayInit, OverlayMode};
