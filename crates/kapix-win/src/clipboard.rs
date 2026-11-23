//! Win32 clipboard (no `arboard`: writing formats directly keeps the binary small and lets us
//! publish both `CF_DIBV5` and the registered `"PNG"` format).

use kapix_core::Image;

/// Put an image on the clipboard as **both** `CF_DIBV5` (BI_BITFIELDS, 32bpp, bottom-up, with
/// sRGB color space) and the registered `"PNG"` format (encoded with `Image::encode_png`), so
/// it pastes into Chrome/Electron/Slack/Discord as well as Office/Paint. Retries `OpenClipboard`
/// briefly if another process holds it. `owner` may be `0` (no owner window).
pub fn set_image(owner: isize, img: &Image) -> crate::Result<()> {
    todo!()
}

/// Put Unicode text on the clipboard (`CF_UNICODETEXT`).
pub fn set_text(owner: isize, text: &str) -> crate::Result<()> {
    todo!()
}
