//! Codecs that would be expensive to bundle in Rust, via built-in Windows Imaging Component.

use kapix_core::Image;

/// JPEG-encode via WIC (`IWICImagingFactory` → `CreateEncoder(GUID_ContainerFormatJpeg)` with
/// `ImageQuality` property). `quality` 0.0–1.0. No metadata is written.
pub fn jpeg(img: &Image, quality: f32) -> crate::Result<Vec<u8>> {
    todo!()
}
