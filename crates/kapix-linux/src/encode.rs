//! Pure-Rust JPEG encoding (matching `kapix-win::encode::jpeg`).

use jpeg_encoder::{ColorType, Encoder};
use kapix_core::Image;

use crate::{Error, Result};

/// Encode `img` (BGRA8) as baseline JPEG with quality in `0.0..=1.0`.
pub fn jpeg(img: &Image, quality: f32) -> Result<Vec<u8>> {
    if img.width == 0 || img.height == 0 {
        return Err(Error("cannot encode empty image".into()));
    }
    let w = u16::try_from(img.width).map_err(|_| Error("image too wide for JPEG".into()))?;
    let h = u16::try_from(img.height).map_err(|_| Error("image too tall for JPEG".into()))?;
    let q = (quality.clamp(0.01, 1.0) * 100.0).round() as u8;

    let mut rgb = Vec::with_capacity(img.width as usize * img.height as usize * 3);
    for px in img.data.as_chunks::<4>().0 {
        rgb.extend_from_slice(&[px[2], px[1], px[0]]);
    }

    let mut out = Vec::with_capacity(rgb.len() / 6 + 1024);
    let enc = Encoder::new(&mut out, q);
    enc.encode(&rgb, w, h, ColorType::Rgb)
        .map_err(|e| Error(format!("JPEG encode failed: {e}")))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_valid_jpeg() {
        let img = Image::new(16, 12);
        let bytes = jpeg(&img, 0.92).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
        assert_eq!(&bytes[bytes.len() - 2..], &[0xFF, 0xD9]);
    }
}
