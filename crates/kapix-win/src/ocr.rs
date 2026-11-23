//! Instant OCR through WinRT `Windows.Media.Ocr` — zero bundled models.

use kapix_core::Image;

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
    todo!()
}
