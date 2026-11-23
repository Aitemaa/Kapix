//! Owned BGRA8 bitmap.

use crate::geom::Rect;

/// Tightly packed, top-down BGRA8 bitmap (`data.len() == width * height * 4`).
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

#[derive(Debug)]
pub enum ImageError {
    Encode(String),
    Io(std::io::Error),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageError::Encode(e) => write!(f, "image encode error: {e}"),
            ImageError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for ImageError {}

impl Image {
    /// Opaque black image.
    pub fn new(width: u32, height: u32) -> Self {
        todo!()
    }
    /// Wrap existing BGRA data. Panics if the length doesn't match.
    pub fn from_bgra(width: u32, height: u32, data: Vec<u8>) -> Self {
        todo!()
    }
    pub fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.width as i32, self.height as i32)
    }
    /// `[b, g, r, a]` at (x, y). Panics when out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        todo!()
    }
    /// Copy of the sub-rectangle `r ∩ bounds`. Returns a 0x0 image if empty.
    pub fn crop(&self, r: Rect) -> Image {
        todo!()
    }
    /// RGBA8 copy (for encoders that need RGBA).
    pub fn to_rgba(&self) -> Vec<u8> {
        todo!()
    }
    /// PNG-encode (RGB8, alpha dropped since frames are opaque; fast compression,
    /// no metadata chunks — nothing that could leak redacted content or identity).
    pub fn encode_png(&self) -> Result<Vec<u8>, ImageError> {
        todo!()
    }
    /// Irreversible mosaic redaction (delegates to [`pixelate_bgra`]).
    pub fn pixelate(&mut self, rect: Rect, block: u32) {
        pixelate_bgra(&mut self.data, self.width, self.height, rect, block)
    }
}

/// Destructively pixelate `rect ∩ (0,0,width,height)` of a tightly packed 4-byte-per-pixel
/// buffer: each `block × block` cell (aligned to the rect's top-left) is replaced by its
/// average color. Channel order agnostic. `block` is clamped to >= 2.
pub fn pixelate_bgra(data: &mut [u8], width: u32, height: u32, rect: Rect, block: u32) {
    todo!()
}
