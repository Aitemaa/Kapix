//! Win32 clipboard (no `arboard`: writing formats directly keeps the binary small and lets us
//! publish both `CF_DIBV5` and the registered `"PNG"` format).

use crate::{Error, hwnd, wide};
use kapix_core::Image;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::Graphics::Gdi::{BI_BITFIELDS, BITMAPV5HEADER, LCS_GM_IMAGES};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{CF_DIBV5, CF_UNICODETEXT};
use windows::core::PCWSTR;

/// `LCS_sRGB` ('sRGB' as a little-endian tag).
const LCS_SRGB: u32 = 0x7352_4742;

/// Holds the clipboard open; closes it on drop.
struct Open;

impl Open {
    fn new(owner: isize) -> crate::Result<Open> {
        let owner = (owner != 0).then(|| hwnd(owner));
        let mut last = None;
        for _ in 0..10 {
            // SAFETY: `owner` is null or a window handle supplied by the app.
            match unsafe { OpenClipboard(owner) } {
                Ok(()) => return Ok(Open),
                Err(e) => last = Some(e),
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Err(last.map_or_else(|| Error("clipboard unavailable".into()), Error::from))
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: the clipboard was opened by `Open::new` on this thread.
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

/// Allocate a movable global block of `len` bytes, let `fill` initialize it, and hand it to the
/// clipboard under `format`. On failure the block is freed; on success the clipboard owns it.
fn put(format: u32, len: usize, fill: impl FnOnce(&mut [u8])) -> crate::Result<()> {
    // SAFETY: standard GlobalAlloc/GlobalLock/SetClipboardData ownership protocol; the slice
    // covers exactly the `len` bytes allocated and is not used after GlobalUnlock.
    unsafe {
        let h: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, len)?;
        let p = GlobalLock(h).cast::<u8>();
        if p.is_null() {
            let _ = GlobalFree(Some(h));
            return Err(Error("GlobalLock failed".into()));
        }
        fill(std::slice::from_raw_parts_mut(p, len));
        let _ = GlobalUnlock(h);
        match SetClipboardData(format, Some(HANDLE(h.0))) {
            Ok(_) => Ok(()),
            Err(e) => {
                let _ = GlobalFree(Some(h));
                Err(e.into())
            }
        }
    }
}

/// Build the `CF_DIBV5` payload: header followed by bottom-up BGRA rows.
fn dibv5(img: &Image, out: &mut [u8]) {
    let hdr = BITMAPV5HEADER {
        bV5Size: size_of::<BITMAPV5HEADER>() as u32,
        bV5Width: img.width as i32,
        bV5Height: img.height as i32,
        bV5Planes: 1,
        bV5BitCount: 32,
        bV5Compression: BI_BITFIELDS,
        bV5SizeImage: img.data.len() as u32,
        bV5RedMask: 0x00FF_0000,
        bV5GreenMask: 0x0000_FF00,
        bV5BlueMask: 0x0000_00FF,
        bV5AlphaMask: 0xFF00_0000,
        bV5CSType: LCS_SRGB,
        bV5Intent: LCS_GM_IMAGES as u32,
        ..Default::default()
    };
    let (head, body) = out.split_at_mut(size_of::<BITMAPV5HEADER>());
    // SAFETY: `head` is exactly size_of::<BITMAPV5HEADER>() bytes (plain-old-data, align 1 write).
    unsafe {
        std::ptr::copy_nonoverlapping((&raw const hdr).cast::<u8>(), head.as_mut_ptr(), head.len())
    };
    let stride = img.width as usize * 4;
    for (dst, src) in body
        .chunks_exact_mut(stride)
        .zip(img.data.chunks_exact(stride).rev())
    {
        dst.copy_from_slice(src);
    }
}

/// Put an image on the clipboard as **both** `CF_DIBV5` (BI_BITFIELDS, 32bpp, bottom-up, with
/// sRGB color space) and the registered `"PNG"` format (encoded with `Image::encode_png`), so
/// it pastes into Chrome/Electron/Slack/Discord as well as Office/Paint. Retries `OpenClipboard`
/// briefly if another process holds it. `owner` may be `0` (no owner window).
pub fn set_image(owner: isize, img: &Image) -> crate::Result<()> {
    if img.width == 0
        || img.height == 0
        || img.data.len() != img.width as usize * img.height as usize * 4
    {
        return Err(Error("empty image".into()));
    }
    // Encode before taking the clipboard so it is held as briefly as possible.
    let png = img.encode_png().map_err(|e| Error(e.to_string()))?;
    let _open = Open::new(owner)?;
    // SAFETY: clipboard is open; ownership is taken by `OpenClipboard`'s owner or the process.
    unsafe { EmptyClipboard()? };
    put(
        CF_DIBV5.0 as u32,
        size_of::<BITMAPV5HEADER>() + img.data.len(),
        |b| dibv5(img, b),
    )?;
    // The PNG flavour is an enhancement; the DIB already succeeded, so don't fail on it.
    let name = wide("PNG");
    // SAFETY: `name` is NUL-terminated.
    let fmt = unsafe { RegisterClipboardFormatW(PCWSTR(name.as_ptr())) };
    if fmt != 0 {
        let _ = put(fmt, png.len(), |b| b.copy_from_slice(&png));
    }
    Ok(())
}

/// Put Unicode text on the clipboard (`CF_UNICODETEXT`).
pub fn set_text(owner: isize, text: &str) -> crate::Result<()> {
    let w = wide(text);
    let _open = Open::new(owner)?;
    // SAFETY: clipboard is open.
    unsafe { EmptyClipboard()? };
    put(CF_UNICODETEXT.0 as u32, w.len() * 2, |b| {
        for (d, s) in b.as_chunks_mut::<2>().0.iter_mut().zip(&w) {
            *d = s.to_le_bytes();
        }
    })
}
