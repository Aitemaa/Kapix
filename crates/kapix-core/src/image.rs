//! Owned BGRA8 bitmap.
#![allow(clippy::chunks_exact_to_as_chunks)]

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
        Self {
            width,
            height,
            data: [0, 0, 0, 255].repeat(width as usize * height as usize),
        }
    }
    /// Wrap existing BGRA data. Panics if the length doesn't match.
    pub fn from_bgra(width: u32, height: u32, data: Vec<u8>) -> Self {
        assert_eq!(
            data.len(),
            width as usize * height as usize * 4,
            "BGRA buffer length mismatch"
        );
        Self {
            width,
            height,
            data,
        }
    }
    pub fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.width as i32, self.height as i32)
    }
    /// `[b, g, r, a]` at (x, y). Panics when out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(x < self.width && y < self.height, "pixel out of bounds");
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ]
    }
    /// Copy of the sub-rectangle `r ∩ bounds`. Returns a 0x0 image if empty.
    pub fn crop(&self, r: Rect) -> Image {
        let Some(r) = r.intersect(&self.bounds()) else {
            return Image {
                width: 0,
                height: 0,
                data: Vec::new(),
            };
        };
        let (w, h) = (r.w as usize, r.h as usize);
        let stride = self.width as usize * 4;
        let mut data = Vec::with_capacity(w * h * 4);
        for row in 0..h {
            let start = (r.y as usize + row) * stride + r.x as usize * 4;
            data.extend_from_slice(&self.data[start..start + w * 4]);
        }
        Image {
            width: w as u32,
            height: h as u32,
            data,
        }
    }
    /// RGBA8 copy (for encoders that need RGBA).
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = self.data.clone();
        for px in out.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        out
    }
    /// PNG-encode (RGB8, alpha dropped since frames are opaque; fast compression,
    /// no metadata chunks — nothing that could leak redacted content or identity).
    pub fn encode_png(&self) -> Result<Vec<u8>, ImageError> {
        let mut rgb = Vec::with_capacity(self.width as usize * self.height as usize * 3);
        for px in self.data.chunks_exact(4) {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        let mut out = Vec::with_capacity(rgb.len() / 2 + 256);
        let mut enc = png::Encoder::new(&mut out, self.width, self.height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc
            .write_header()
            .map_err(|e| ImageError::Encode(e.to_string()))?;
        w.write_image_data(&rgb)
            .map_err(|e| ImageError::Encode(e.to_string()))?;
        w.finish().map_err(|e| ImageError::Encode(e.to_string()))?;
        Ok(out)
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
    let block = block.max(2) as i64;
    let Some(r) = rect.intersect(&Rect::new(0, 0, width as i32, height as i32)) else {
        return;
    };
    if data.len() < width as usize * height as usize * 4 {
        return;
    }
    let stride = width as usize * 4;
    // Cells are aligned to the *requested* rect's top-left (not the clipped one).
    let (ox, oy) = (rect.x as i64, rect.y as i64);
    let (x0, y0, x1, y1) = (r.x as i64, r.y as i64, r.right() as i64, r.bottom() as i64);
    let mut cy = oy + (y0 - oy).div_euclid(block) * block;
    while cy < y1 {
        let ys = cy.max(y0) as usize;
        let ye = (cy + block).min(y1) as usize;
        let mut cx = ox + (x0 - ox).div_euclid(block) * block;
        while cx < x1 {
            let xs = cx.max(x0) as usize;
            let xe = (cx + block).min(x1) as usize;
            let n = ((xe - xs) * (ye - ys)) as u64;
            let mut sum = [0u64; 4];
            for y in ys..ye {
                for px in data[y * stride + xs * 4..y * stride + xe * 4].chunks_exact(4) {
                    for (s, v) in sum.iter_mut().zip(px) {
                        *s += *v as u64;
                    }
                }
            }
            let avg = sum.map(|s| ((s + n / 2) / n) as u8);
            for y in ys..ye {
                for px in data[y * stride + xs * 4..y * stride + xe * 4].chunks_exact_mut(4) {
                    px.copy_from_slice(&avg);
                }
            }
            cx += block;
        }
        cy += block;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Image {
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[x as u8, y as u8, (x + y) as u8, 255]);
            }
        }
        Image::from_bgra(w, h, data)
    }

    /// Deterministic high-entropy pixels (xorshift).
    fn noise(w: u32, h: u32) -> Image {
        let mut s = 0x9E37_79B9_7F4A_7C15u64;
        let mut data = Vec::new();
        for _ in 0..w * h {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            data.extend_from_slice(&[s as u8, (s >> 8) as u8, (s >> 16) as u8, 255]);
        }
        Image::from_bgra(w, h, data)
    }

    #[test]
    fn new_is_opaque_black() {
        let i = Image::new(3, 2);
        assert_eq!(i.data.len(), 24);
        assert_eq!(i.pixel(2, 1), [0, 0, 0, 255]);
    }

    #[test]
    #[should_panic]
    fn from_bgra_length_mismatch_panics() {
        let _ = Image::from_bgra(2, 2, vec![0; 3]);
    }

    #[test]
    fn pixel_order_and_rgba() {
        let i = Image::from_bgra(1, 1, vec![10, 20, 30, 255]);
        assert_eq!(i.pixel(0, 0), [10, 20, 30, 255]);
        assert_eq!(i.to_rgba(), vec![30, 20, 10, 255]);
    }

    #[test]
    fn crop_inside_and_clipped() {
        let i = gradient(10, 8);
        let c = i.crop(Rect::new(2, 3, 4, 2));
        assert_eq!((c.width, c.height), (4, 2));
        assert_eq!(c.pixel(0, 0), i.pixel(2, 3));
        assert_eq!(c.pixel(3, 1), i.pixel(5, 4));
        let c = i.crop(Rect::new(-5, -5, 8, 8));
        assert_eq!((c.width, c.height), (3, 3));
        assert_eq!(c.pixel(0, 0), i.pixel(0, 0));
        let c = i.crop(Rect::new(8, 6, 100, 100));
        assert_eq!((c.width, c.height), (2, 2));
        assert_eq!(c.pixel(1, 1), i.pixel(9, 7));
        let c = i.crop(Rect::new(50, 50, 5, 5));
        assert_eq!((c.width, c.height, c.data.len()), (0, 0, 0));
        assert_eq!(i.crop(Rect::new(0, 0, 0, 5)).width, 0);
    }

    #[test]
    fn encode_png_is_valid_rgb() {
        let i = noise(17, 9);
        let bytes = i.encode_png().unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let dec = png::Decoder::new(std::io::Cursor::new(&bytes));
        let mut reader = dec.read_info().unwrap();
        let info = reader.info();
        assert_eq!((info.width, info.height), (17, 9));
        assert!(info.uncompressed_latin1_text.is_empty() && info.compressed_latin1_text.is_empty());
        assert!(info.exif_metadata.is_none());
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let frame = reader.next_frame(&mut buf).unwrap();
        assert_eq!(frame.color_type, png::ColorType::Rgb);
        assert_eq!(frame.bit_depth, png::BitDepth::Eight);
        let rgb = &buf[..frame.buffer_size()];
        for (k, px) in i.data.chunks_exact(4).enumerate() {
            assert_eq!(&rgb[k * 3..k * 3 + 3], &[px[2], px[1], px[0]]);
        }
        // only critical chunks, no tEXt/eXIf/etc.
        let mut pos = 8;
        while pos < bytes.len() {
            let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            let ty = &bytes[pos + 4..pos + 8];
            assert!(
                matches!(ty, b"IHDR" | b"IDAT" | b"IEND"),
                "unexpected chunk {:?}",
                ty
            );
            pos += 12 + len;
        }
    }

    #[test]
    fn pixelate_blocks_uniform_and_average() {
        let mut i = gradient(20, 20);
        let orig = i.clone();
        i.pixelate(Rect::new(4, 4, 8, 8), 4);
        // outside untouched
        assert_eq!(i.pixel(3, 4), orig.pixel(3, 4));
        assert_eq!(i.pixel(12, 4), orig.pixel(12, 4));
        assert_eq!(i.pixel(4, 12), orig.pixel(4, 12));
        // each 4x4 cell uniform and equal to its average
        for (cx, cy) in [(4u32, 4u32), (8, 4), (4, 8), (8, 8)] {
            let first = i.pixel(cx, cy);
            let mut sum = [0u32; 4];
            for y in cy..cy + 4 {
                for x in cx..cx + 4 {
                    assert_eq!(i.pixel(x, y), first);
                    for (s, v) in sum.iter_mut().zip(orig.pixel(x, y)) {
                        *s += v as u32;
                    }
                }
            }
            for c in 0..4 {
                assert_eq!(first[c] as u32, (sum[c] + 8) / 16);
            }
        }
    }

    #[test]
    fn pixelate_irreversible_on_high_entropy() {
        let mut i = noise(64, 48);
        let orig = i.clone();
        let rect = Rect::new(0, 0, 64, 48);
        i.pixelate(rect, 8);
        let changed = i
            .data
            .chunks_exact(4)
            .zip(orig.data.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        assert!(
            changed * 100 > 64 * 48 * 90,
            "only {changed} pixels changed"
        );
        for cy in (0..48).step_by(8) {
            for cx in (0..64).step_by(8) {
                let first = i.pixel(cx, cy);
                for y in cy..cy + 8 {
                    for x in cx..cx + 8 {
                        assert_eq!(i.pixel(x, y), first);
                    }
                }
            }
        }
        // number of distinct colors collapses to number of cells
        let mut colors: Vec<[u8; 4]> = i
            .data
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect();
        colors.sort();
        colors.dedup();
        assert!(colors.len() <= 8 * 6);
    }

    #[test]
    fn pixelate_partial_edge_cells() {
        // 5x3 rect with block 4 → cells 4x3 and 1x3 (partial), averaged independently.
        let mut i = gradient(10, 6);
        let orig = i.clone();
        i.pixelate(Rect::new(1, 1, 5, 3), 4);
        let edge = i.pixel(5, 1);
        let mut sum = 0u32;
        for y in 1..4 {
            sum += orig.pixel(5, y)[0] as u32;
            assert_eq!(i.pixel(5, y), edge);
        }
        assert_eq!(edge[0] as u32, (sum + 1) / 3);
        assert_eq!(i.pixel(6, 1), orig.pixel(6, 1));
        assert_eq!(i.pixel(0, 1), orig.pixel(0, 1));
        assert_eq!(i.pixel(1, 4), orig.pixel(1, 4));
    }

    #[test]
    fn pixelate_rect_partly_out_of_bounds() {
        let mut i = noise(10, 10);
        let orig = i.clone();
        i.pixelate(Rect::new(-3, -3, 8, 8), 4);
        // cells aligned to rect origin (-3,-3): first in-bounds cell is x,y in 0..1 (cell -3..1)
        assert_eq!(
            i.pixel(0, 0),
            orig.pixel(0, 0),
            "1x1 clipped cell averages to itself"
        );
        let c = i.pixel(1, 1);
        for y in 1..5 {
            for x in 1..5 {
                assert_eq!(i.pixel(x, y), c);
            }
            assert_eq!(
                i.pixel(0, y),
                i.pixel(0, 1),
                "clipped column cell is uniform"
            );
        }
        assert_ne!(c, orig.pixel(1, 1));
        // in-bounds region is 0..5; pixel at 5 untouched
        assert_eq!(i.pixel(5, 0), orig.pixel(5, 0));
        assert_eq!(i.pixel(0, 5), orig.pixel(0, 5));
        assert_eq!(i.pixel(9, 9), orig.pixel(9, 9));
        // fully outside / empty: no-ops
        let before = i.clone();
        i.pixelate(Rect::new(100, 100, 10, 10), 4);
        i.pixelate(Rect::new(0, 0, 0, 10), 4);
        assert_eq!(i, before);
        // bigger than the image
        i.pixelate(Rect::new(-50, -50, 500, 500), 4);
    }

    #[test]
    fn pixelate_block_clamped_to_two() {
        let mut a = noise(8, 8);
        let mut b = a.clone();
        a.pixelate(Rect::new(0, 0, 8, 8), 0);
        b.pixelate(Rect::new(0, 0, 8, 8), 2);
        assert_eq!(a, b);
        let mut c = noise(8, 8);
        c.pixelate(Rect::new(0, 0, 8, 8), 1);
        assert_eq!(a, c);
        assert_ne!(a, noise(8, 8));
    }
}
