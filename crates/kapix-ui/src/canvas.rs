//! BGRA-order helpers around `tiny_skia::Pixmap`.

use kapix_core::{Image, Rect, Rgba};
use tiny_skia::{Color, IntSize, Paint, Path, PathBuilder, Pixmap};

/// Convert a straight RGBA color into a tiny-skia color **with R and B swapped**, so drawing
/// into a pixmap whose bytes are interpreted as BGRA produces the intended color.
pub fn sk_color(c: Rgba) -> tiny_skia::Color {
    Color::from_rgba8(c.b, c.g, c.r, c.a)
}

/// Solid paint (anti-aliased) for `c`.
pub fn paint(c: Rgba) -> tiny_skia::Paint<'static> {
    let mut p = Paint::default();
    p.set_color(sk_color(c));
    p.anti_alias = true;
    p
}

/// Move an [`Image`] into a pixmap without copying pixel bytes when possible.
/// Panics if the image is 0×0.
pub fn image_to_pixmap(img: Image) -> tiny_skia::Pixmap {
    let size = IntSize::from_wh(img.width, img.height).expect("image must be non-empty");
    Pixmap::from_vec(img.data, size).expect("image buffer length matches its size")
}

/// Move a pixmap back into an [`Image`] (BGRA bytes kept as-is).
pub fn pixmap_to_image(p: tiny_skia::Pixmap) -> Image {
    let (w, h) = (p.width(), p.height());
    Image::from_bgra(w, h, p.take())
}

/// Copy `src` rect of `from` into `to` at `(dx, dy)` (clipped; plain copy, no blending).
pub fn blit(
    from: &tiny_skia::Pixmap,
    src: kapix_core::Rect,
    to: &mut tiny_skia::Pixmap,
    dx: i32,
    dy: i32,
) {
    blit_with(from, src, to, dx, dy, |d, s| d.copy_from_slice(s));
}

/// Like [`blit`] but darkens the copied pixels (the "frozen screen dimmer"); alpha is forced
/// opaque.
pub(crate) fn blit_dimmed(from: &Pixmap, src: Rect, to: &mut Pixmap, dx: i32, dy: i32) {
    blit_with(from, src, to, dx, dy, dim_row);
}

fn blit_with(
    from: &Pixmap,
    src: Rect,
    to: &mut Pixmap,
    dx: i32,
    dy: i32,
    row: impl Fn(&mut [u8], &[u8]),
) {
    let Some(s) = src.intersect(&pm_rect(from)) else {
        return;
    };
    let (ddx, ddy) = (dx + (s.x - src.x), dy + (s.y - src.y));
    let Some(d) = Rect::new(ddx, ddy, s.w, s.h).intersect(&pm_rect(to)) else {
        return;
    };
    let (sx, sy) = (s.x + (d.x - ddx), s.y + (d.y - ddy));
    let (fw, tw) = (from.width() as usize * 4, to.width() as usize * 4);
    let n = d.w as usize * 4;
    let fdata = from.data();
    let tdata = to.data_mut();
    for r in 0..d.h as usize {
        let so = (sy as usize + r) * fw + sx as usize * 4;
        let doff = (d.y as usize + r) * tw + d.x as usize * 4;
        row(&mut tdata[doff..doff + n], &fdata[so..so + n]);
    }
}

/// Brightness multiplier of the dimmed frame, out of 256 (~45%).
const DIM: u32 = 115;

fn dim_row(dst: &mut [u8], src: &[u8]) {
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let p = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
        let rb = (((p & 0x00FF_00FF) * DIM) >> 8) & 0x00FF_00FF;
        let g = ((((p >> 8) & 0xFF) * DIM) >> 8) << 8;
        d.copy_from_slice(&(rb | g | 0xFF00_0000).to_le_bytes());
    }
}

/// Bounds of a pixmap as a core rect.
pub(crate) fn pm_rect(p: &Pixmap) -> Rect {
    Rect::new(0, 0, p.width() as i32, p.height() as i32)
}

/// Source-over blend a straight-alpha color into one premultiplied BGRA pixel.
#[inline]
pub(crate) fn blend_px(dst: &mut [u8], c: Rgba, cov: f32) {
    let a = (c.a as f32 / 255.0) * cov;
    if a <= 0.0 {
        return;
    }
    let inv = 1.0 - a;
    dst[0] = (c.b as f32 * a + dst[0] as f32 * inv + 0.5) as u8;
    dst[1] = (c.g as f32 * a + dst[1] as f32 * inv + 0.5) as u8;
    dst[2] = (c.r as f32 * a + dst[2] as f32 * inv + 0.5) as u8;
    dst[3] = (255.0 * a + dst[3] as f32 * inv + 0.5) as u8;
}

/// Fast axis-aligned solid rect fill with source-over blending (clipped).
pub(crate) fn fill_rect_px(pm: &mut Pixmap, r: Rect, c: Rgba) {
    let Some(r) = r.intersect(&pm_rect(pm)) else {
        return;
    };
    let stride = pm.width() as usize * 4;
    let opaque = c.a == 255;
    let data = pm.data_mut();
    for y in r.y..r.bottom() {
        let off = y as usize * stride;
        let row = &mut data[off + r.x as usize * 4..off + r.right() as usize * 4];
        for px in row.chunks_exact_mut(4) {
            if opaque {
                px.copy_from_slice(&[c.b, c.g, c.r, 255]);
            } else {
                blend_px(px, c, 1.0);
            }
        }
    }
}

/// Outline of `r` (drawn inside it), `t` px thick.
pub(crate) fn stroke_rect_px(pm: &mut Pixmap, r: Rect, t: i32, c: Rgba) {
    let t = t.min(r.w / 2).min(r.h / 2).max(0);
    if t == 0 {
        fill_rect_px(pm, r, c);
        return;
    }
    fill_rect_px(pm, Rect::new(r.x, r.y, r.w, t), c);
    fill_rect_px(pm, Rect::new(r.x, r.bottom() - t, r.w, t), c);
    fill_rect_px(pm, Rect::new(r.x, r.y + t, t, r.h - 2 * t), c);
    fill_rect_px(pm, Rect::new(r.right() - t, r.y + t, t, r.h - 2 * t), c);
}

/// `a` minus `b` as up to four disjoint rects.
pub(crate) fn rect_minus(a: Rect, b: Rect) -> Vec<Rect> {
    if a.is_empty() {
        return Vec::new();
    }
    let Some(i) = a.intersect(&b) else {
        return vec![a];
    };
    let mut out = Vec::with_capacity(4);
    let mut push = |r: Rect| {
        if !r.is_empty() {
            out.push(r);
        }
    };
    push(Rect::new(a.x, a.y, a.w, i.y - a.y));
    push(Rect::new(a.x, i.bottom(), a.w, a.bottom() - i.bottom()));
    push(Rect::new(a.x, i.y, i.x - a.x, i.h));
    push(Rect::new(i.right(), i.y, a.right() - i.right(), i.h));
    out
}

/// Rounded rectangle path (`r` is clamped to half the shorter side).
pub(crate) fn rrect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let k = r * 0.552_284_75;
    let (x1, y1) = (x + w, y + h);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x1 - r, y);
    pb.cubic_to(x1 - r + k, y, x1, y + r - k, x1, y + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
    pb.line_to(x + r, y1);
    pb.cubic_to(x + r - k, y1, x, y1 - r + k, x, y1 - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sk_color_swaps_red_and_blue() {
        let c = sk_color(Rgba::rgb(10, 20, 30));
        assert_eq!((c.red() * 255.0).round() as u8, 30);
        assert_eq!((c.green() * 255.0).round() as u8, 20);
        assert_eq!((c.blue() * 255.0).round() as u8, 10);
    }

    #[test]
    fn drawing_with_sk_color_yields_bgra_bytes() {
        let mut pm = Pixmap::new(4, 4).unwrap();
        pm.fill(sk_color(Rgba::rgb(200, 100, 50)));
        assert_eq!(&pm.data()[0..4], &[50, 100, 200, 255]);
    }

    #[test]
    fn image_pixmap_roundtrip_keeps_bytes() {
        let img = Image::from_bgra(2, 1, vec![1, 2, 3, 255, 4, 5, 6, 255]);
        let pm = image_to_pixmap(img.clone());
        assert_eq!(pm.data(), &img.data[..]);
        assert_eq!(pixmap_to_image(pm), img);
    }

    fn numbered(w: u32, h: u32) -> Pixmap {
        let mut pm = Pixmap::new(w, h).unwrap();
        for (i, px) in pm.data_mut().chunks_exact_mut(4).enumerate() {
            px.copy_from_slice(&[i as u8, 0, 0, 255]);
        }
        pm
    }

    #[test]
    fn blit_copies_and_clips() {
        let src = numbered(4, 4);
        let mut dst = Pixmap::new(4, 4).unwrap();
        blit(&src, Rect::new(1, 1, 2, 2), &mut dst, 0, 0);
        assert_eq!(dst.data()[0], 5);
        assert_eq!(dst.data()[4], 6);
        assert_eq!(dst.data()[16], 9);
        // Clipped on the destination's bottom-right and the source's own edge.
        let mut dst = Pixmap::new(4, 4).unwrap();
        blit(&src, Rect::new(2, 2, 10, 10), &mut dst, 3, 3);
        assert_eq!(dst.data()[(3 * 4 + 3) * 4], 10);
        // Negative destination offset clips the left/top of the source.
        let mut dst = Pixmap::new(4, 4).unwrap();
        blit(&src, Rect::new(0, 0, 4, 4), &mut dst, -2, -1);
        assert_eq!(dst.data()[0], 6);
        // Fully outside: no-op, no panic.
        blit(&src, Rect::new(10, 10, 2, 2), &mut dst, 0, 0);
        blit(&src, Rect::new(0, 0, 2, 2), &mut dst, 99, 99);
    }

    #[test]
    fn dimmed_blit_darkens_and_keeps_opaque() {
        let mut src = Pixmap::new(2, 2).unwrap();
        src.fill(Color::from_rgba8(200, 200, 200, 255));
        let mut dst = Pixmap::new(2, 2).unwrap();
        blit_dimmed(&src, Rect::new(0, 0, 2, 2), &mut dst, 0, 0);
        let v = dst.data()[0];
        assert!((85..=95).contains(&v), "{v}");
        assert_eq!(dst.data()[3], 255);
    }

    #[test]
    fn rect_minus_covers_difference() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(3, 3, 4, 4);
        let parts = rect_minus(a, b);
        let area: i64 = parts.iter().map(|r| r.area()).sum();
        assert_eq!(area, 100 - 16);
        assert_eq!(rect_minus(a, Rect::new(20, 20, 2, 2)), vec![a]);
        assert!(rect_minus(a, a).is_empty());
    }
}
