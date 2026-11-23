//! Instant OCR through WinRT `Windows.Media.Ocr` — zero bundled models.

use kapix_core::Image;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::HSTRING;

#[derive(Debug, Clone, PartialEq)]
pub enum OcrError {
    /// No OCR-capable language pack installed (or requested language unavailable).
    NoLanguagePack,
    /// Image too large/small or other engine failure.
    Failed(String),
}

impl std::fmt::Display for OcrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcrError::NoLanguagePack => f.write_str("no Windows OCR language pack installed"),
            OcrError::Failed(m) => write!(f, "OCR failed: {m}"),
        }
    }
}

impl std::error::Error for OcrError {}

/// Recognize text in an opaque BGRA image. Blocking; run on a worker thread.
///
/// `language`: `"auto"` → `OcrEngine::TryCreateFromUserProfileLanguages`; otherwise a BCP-47 tag
/// → `TryCreateFromLanguage(Language::CreateLanguage(tag))` (falling back to auto if
/// unsupported). Images larger than `OcrEngine::MaxImageDimension` are downscaled; very small
/// crops (< 40 px tall) are upscaled 2× (nearest) for better accuracy. Bitmap is built with
/// `SoftwareBitmap::CreateCopyFromBuffer(…, Bgra8, Premultiplied)` from a `DataWriter` buffer.
///
/// Returns recognized lines (`OcrResult.Lines()[i].Text()`), top to bottom.
pub fn recognize(img: &Image, language: &str) -> Result<Vec<String>, OcrError> {
    let (w, h) = (img.width as usize, img.height as usize);
    if w == 0 || h == 0 || img.data.len() != w * h * 4 {
        return Err(OcrError::Failed("empty image".into()));
    }
    // SAFETY: plain WinRT initialization; failure (e.g. already initialized with another
    // apartment model) is benign for our use and ignored.
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    let engine = create_engine(language)?;
    let max = OcrEngine::MaxImageDimension().map_or(2600, |m| m as usize);
    let (tw, th) = target_size(w, h, max);
    let bitmap = if (tw, th) == (w, h) {
        make_bitmap(&img.data, w, h)
    } else {
        make_bitmap(&resize(&img.data, w, h, tw, th), tw, th)
    }
    .map_err(fail)?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .and_then(|op| op.join())
        .map_err(fail)?;
    let lines = result.Lines().map_err(fail)?;
    let mut out = Vec::with_capacity(lines.Size().unwrap_or(0) as usize);
    for line in &lines {
        out.push(line.Text().map_err(fail)?.to_string());
    }
    Ok(out)
}

fn fail(e: windows::core::Error) -> OcrError {
    OcrError::Failed(format!("{} (0x{:08X})", e.message(), e.code().0 as u32))
}

fn create_engine(language: &str) -> Result<OcrEngine, OcrError> {
    let tag = language.trim();
    if !tag.is_empty()
        && !tag.eq_ignore_ascii_case("auto")
        && let Ok(lang) = Language::CreateLanguage(&HSTRING::from(tag))
        && OcrEngine::IsLanguageSupported(&lang).unwrap_or(false)
        && let Ok(e) = OcrEngine::TryCreateFromLanguage(&lang)
    {
        return Ok(e);
    }
    // A null engine surfaces as an error here: no usable OCR language pack.
    OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| OcrError::NoLanguagePack)
}

fn make_bitmap(bgra: &[u8], w: usize, h: usize) -> windows::core::Result<SoftwareBitmap> {
    let writer = DataWriter::new()?;
    writer.WriteBytes(bgra)?;
    let buffer = writer.DetachBuffer()?;
    SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        w as i32,
        h as i32,
        BitmapAlphaMode::Premultiplied,
    )
}

/// Size actually fed to the engine: crops under 40 px tall are doubled, then everything is
/// scaled down (keeping aspect) so the longest side fits `max`.
fn target_size(w: usize, h: usize, max: usize) -> (usize, usize) {
    let f = if h < 40 { 2 } else { 1 };
    let (mut tw, mut th) = (w * f, h * f);
    let long = tw.max(th);
    if long > max {
        tw = (tw * max / long).max(1);
        th = (th * max / long).max(1);
    }
    (tw, th)
}

/// Resample BGRA: box filter when shrinking, nearest when enlarging.
fn resize(src: &[u8], w: usize, h: usize, nw: usize, nh: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(nw * nh * 4);
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1).min(h));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1).min(w));
            if nw >= w && nh >= h {
                let i = (y0 * w + x0) * 4;
                out.extend_from_slice(&src[i..i + 4]);
                continue;
            }
            let mut acc = [0u32; 4];
            for sy in y0..y1 {
                for px in src[(sy * w + x0) * 4..(sy * w + x1) * 4].as_chunks::<4>().0 {
                    for (a, &v) in acc.iter_mut().zip(px) {
                        *a += v as u32;
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            out.extend(acc.iter().map(|&a| ((a + n / 2) / n) as u8));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(target_size(100, 100, 2600), (100, 100));
        assert_eq!(target_size(100, 20, 2600), (200, 40));
        assert_eq!(target_size(5000, 1000, 2600), (2600, 520));
        assert_eq!(target_size(2000, 30, 2600), (2600, 39));
    }

    #[test]
    fn resample() {
        let px = [10u8, 20, 30, 255];
        let src: Vec<u8> = px.iter().copied().cycle().take(4 * 4 * 4).collect();
        assert_eq!(resize(&src, 4, 4, 2, 2), px.repeat(4));
        assert_eq!(resize(&src, 4, 4, 8, 8).len(), 8 * 8 * 4);
    }
}
