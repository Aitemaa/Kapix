//! Procedurally drawn tray icon: accent rounded square with white viewfinder corners.

use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DIB_RGB_COLORS, DeleteObject, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

use super::winutil::bmi;

/// Signed distance (approx.) to an axis-aligned box given by centre and half-size.
fn sd_box(px: f32, py: f32, cx: f32, cy: f32, hx: f32, hy: f32) -> f32 {
    ((px - cx).abs() - hx).max((py - cy).abs() - hy)
}

/// Straight-alpha BGRA pixels of a `n x n` icon.
fn render(n: usize) -> Vec<u8> {
    let nf = n as f32;
    let c = nf / 2.0;
    let radius = nf * 0.24;
    let corner = nf * 0.30; // bracket outer corner, relative to the centre
    let arm = nf * 0.18;
    let t = (nf * 0.085).max(1.4);
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            // Rounded square covering the whole canvas.
            let (qx, qy) = (px.abs() - (c - radius), py.abs() - (c - radius));
            let d_bg =
                (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
            let a_bg = (0.5 - d_bg).clamp(0.0, 1.0);
            // Four L-shaped brackets (folded into one quadrant) plus a centre dot.
            let (fx, fy) = (px.abs(), py.abs());
            let d_h = sd_box(
                fx,
                fy,
                corner - arm / 2.0,
                corner - t / 2.0,
                arm / 2.0,
                t / 2.0,
            );
            let d_v = sd_box(
                fx,
                fy,
                corner - t / 2.0,
                corner - arm / 2.0,
                t / 2.0,
                arm / 2.0,
            );
            let d_dot = (fx * fx + fy * fy).sqrt() - nf * 0.06;
            let a_fg = (0.5 - d_h.min(d_v).min(d_dot)).clamp(0.0, 1.0);
            let mix = |bg: f32, fg: f32| (bg + (fg - bg) * a_fg).round() as u8;
            let i = (y * n + x) * 4;
            // #0EA5E9 accent -> white.
            out[i] = mix(233.0, 255.0);
            out[i + 1] = mix(165.0, 255.0);
            out[i + 2] = mix(14.0, 255.0);
            out[i + 3] = (a_bg * 255.0).round() as u8;
        }
    }
    out
}

/// Build an `HICON` of `n x n` pixels (caller owns it; `DestroyIcon` when done).
pub fn make_icon(n: i32) -> Option<HICON> {
    let n = n.clamp(16, 64);
    let px = render(n as usize);
    unsafe {
        let info = bmi(n, n);
        let mut bits = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(color.0));
            return None;
        }
        std::ptr::copy_nonoverlapping(px.as_ptr(), bits.cast::<u8>(), px.len());
        let mask = CreateBitmap(n, n, 1, 1, None);
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
