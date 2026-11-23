//! Tray icon: the Kapix mark ([`kapix_ui::brand::render_app_icon`]) as a 32-bit alpha `HICON`.

use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DIB_RGB_COLORS, DeleteObject, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

use super::winutil::bmi;

/// Straight-alpha BGRA bytes (what `CreateIconIndirect` expects) from premultiplied BGRA.
#[allow(clippy::chunks_exact_to_as_chunks)]
fn unpremultiply(px: &mut [u8]) {
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a != 0 && a != 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

/// Build an `HICON` of `n x n` pixels (caller owns it; `DestroyIcon` when done).
pub fn make_icon(n: i32) -> Option<HICON> {
    let n = n.clamp(16, 256);
    let mut img = kapix_ui::brand::render_app_icon(n as u32);
    unpremultiply(&mut img.data);
    unsafe {
        let info = bmi(n, n);
        let mut bits = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(color.0));
            return None;
        }
        std::ptr::copy_nonoverlapping(img.data.as_ptr(), bits.cast::<u8>(), img.data.len());
        // All-zero 1-bpp AND mask (rows padded to 16 bits): every pixel opaque, alpha rules.
        let mask_bits = vec![0u8; (n as usize).div_ceil(16) * 2 * n as usize];
        let mask = CreateBitmap(n, n, 1, 1, Some(mask_bits.as_ptr().cast()));
        let ii = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&ii).ok();
        let _ = DeleteObject(HGDIOBJ(color.0));
        let _ = DeleteObject(HGDIOBJ(mask.0));
        icon
    }
}
