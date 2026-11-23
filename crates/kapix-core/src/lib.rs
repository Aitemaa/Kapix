//! Platform-independent domain logic for Kapix.
//!
//! Conventions shared by every crate:
//! - All coordinates are **physical pixels**.
//! - "Virtual" coordinates are Windows virtual-desktop coordinates (may be negative).
//! - "Buffer" coordinates index the captured virtual-desktop [`Image`]; buffer (0,0) is
//!   `VirtualDesktop::bounds` top-left. Overlay state and annotations use buffer coordinates.
//! - Pixel format everywhere is **BGRA8**, tightly packed (stride = width * 4), top-down rows.
//!   Captured frames are opaque (alpha = 255).

pub mod annotate;
pub mod color;
pub mod config;
pub mod geom;
pub mod history;
pub mod hotkey;
pub mod image;
pub mod ocr_text;
pub mod template;

pub use annotate::{Annotation, Document, Tool};
pub use color::Rgba;
pub use config::Config;
pub use geom::{Monitor, Point, PointF, Rect, VirtualDesktop};
pub use hotkey::Hotkey;
pub use image::Image;
