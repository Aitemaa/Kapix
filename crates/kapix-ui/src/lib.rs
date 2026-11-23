//! Pure-Rust, platform-independent Kapix frontend.
//!
//! The Win32 host (in `kapix-app`) owns the windows and translates OS messages into
//! [`overlay::Input`]; everything visual — overlay dimming, selection handles, magnifier loupe,
//! floating toolbar, annotations, toasts — is rasterized here on the CPU with `tiny-skia` and
//! `ab_glyph`, styled with the Kavox-family tokens in [`theme`] and the embedded Archivo typeface. The same renderer produces the live preview and the exported image (WYSIWYG).
//!
//! **Pixel order:** every buffer handled here is BGRA8 (premultiplied; opaque frames are
//! identical either way) so it can be blitted to GDI (`SetDIBitsToDevice` / `UpdateLayeredWindow`)
//! without conversion. `tiny-skia` itself thinks in RGBA, so colors must go through
//! [`canvas::sk_color`], which swaps R and B.

// Pixel-loop code reads better with `chunks_exact(4)` than `as_chunks`, and nested `if let`
// keeps the input handlers scannable.
#![allow(
    clippy::chunks_exact_to_as_chunks,
    clippy::collapsible_if,
    clippy::type_complexity,
    clippy::too_many_arguments,
    clippy::excessive_precision
)]

pub mod brand;
pub mod canvas;
mod chrome;
pub mod font;
mod icons;
pub mod overlay;
pub mod render;
mod shadow;
pub mod theme;
pub mod toast;
mod toolbar;

pub use font::{Font, Weight};
pub use overlay::{
    CursorKind, Input, Key, Mods, MouseButton, Outcome, Overlay, OverlayInit, OverlayMode,
};
pub use theme::{Palette, ThemeMode};
