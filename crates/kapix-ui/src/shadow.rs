//! Kavox popover shadow: a soft, y-offset drop shadow rendered once per card size as a blurred
//! alpha sprite and composited with source-over (premultiplied BGRA).

use std::cell::RefCell;

use kapix_core::{Rect, Rgba};
use tiny_skia::Pixmap;

use crate::canvas::{self, fill_rrect, pm_rect};
use crate::theme::SHADOW_OFFSET_Y;

/// Gaussian sigma in logical px. Kavox uses blur 32 (CSS sigma 16); we keep the look but trim the
/// tail so the repaint area around moving cards stays small.
const SIGMA: f32 = 12.0;
/// Visible reach of the blur around the (offset) card, in sigmas.
const REACH_SIGMAS: f32 = 2.2;

fn reach(s: f32) -> i32 {
    (SIGMA * REACH_SIGMAS * s).ceil() as i32
}

fn offset(s: f32) -> i32 {
    (SHADOW_OFFSET_Y * s).round() as i32
}

/// Everything the shadow of `card` may touch (for dirty rects / transparent margins).
pub(crate) fn bounds(card: Rect, s: f32) -> Rect {
    let (r, o) = (reach(s), offset(s));
    Rect::from_ltrb(
        card.x - r,
        card.y - r + o,
        card.right() + r,
        card.bottom() + r + o,
    )
    .union(&card)
}

/// Margins `(left, top, right, bottom)` between a card and the edge of its shadow bounds.
pub(crate) fn margins(s: f32) -> (i32, i32, i32, i32) {
    let b = bounds(Rect::new(0, 0, 0, 0), s);
    (-b.x, -b.y, b.right(), b.bottom())
}

type Key = (i32, i32, i32, i32, u8);

thread_local! {
    static SPRITES: RefCell<Vec<(Key, Pixmap)>> = const { RefCell::new(Vec::new()) };
}

/// Draw the shadow of an **opaque** card (`card` outer rect, corner `radius`) onto `pm`. Nothing
/// is drawn under the card body, so call this before drawing the card itself.
pub(crate) fn draw(pm: &mut Pixmap, card: Rect, radius: f32, s: f32, color: Rgba) {
    if card.is_empty() || color.a == 0 {
        return;
    }
    let key = (card.w, card.h, radius as i32, (s * 100.0) as i32, color.a);
    SPRITES.with(|cache| {
        let mut cache = cache.borrow_mut();
        let idx = match cache.iter().position(|(k, _)| *k == key) {
            Some(i) => i,
            None => {
                if cache.len() > 8 {
                    cache.clear();
                }
                cache.push((key, sprite(card.w, card.h, radius, s, color)));
                cache.len() - 1
            }
        };
        let m = reach(s) + offset(s);
        let skip = card.inflate(-(radius.ceil() as i32));
        blend_sprite(
            pm,
            &cache[idx].1,
            card.x - m,
            card.y - m,
            bounds(card, s),
            skip,
        );
    });
}

fn sprite(w: i32, h: i32, radius: f32, s: f32, color: Rgba) -> Pixmap {
    let m = reach(s) + offset(s);
    let (sw, sh) = ((w + 2 * m) as usize, (h + 2 * m) as usize);
    // Alpha mask of the offset card.
    let mut mask = Pixmap::new(sw as u32, sh as u32).expect("shadow size");
    fill_rrect(
        &mut mask,
        Rect::new(m, m + offset(s), w, h),
        radius,
        Rgba::rgb(0, 0, 0),
    );
    let mut a: Vec<u8> = mask.data().chunks_exact(4).map(|p| p[3]).collect();
    // Three box blurs ≈ gaussian.
    let bw = ((12.0 * (SIGMA * s).powi(2) / 3.0 + 1.0).sqrt().floor() as usize).max(1);
    let mut tmp = vec![0u8; a.len()];
    for _ in 0..3 {
        box_blur(&a, &mut tmp, sw, sh, bw, true);
        box_blur(&tmp, &mut a, sw, sh, bw, false);
    }
    let mut out = Pixmap::new(sw as u32, sh as u32).expect("shadow size");
    let k = color.a as u32;
    for (px, &v) in out.data_mut().chunks_exact_mut(4).zip(&a) {
        let al = (v as u32 * k + 127) / 255;
        px[0] = (color.b as u32 * al / 255) as u8;
        px[1] = (color.g as u32 * al / 255) as u8;
        px[2] = (color.r as u32 * al / 255) as u8;
        px[3] = al as u8;
    }
    out
}

/// One 1-D box blur pass (window `bw`, centred) over `src` into `dst`; edges clamp to zero.
fn box_blur(src: &[u8], dst: &mut [u8], w: usize, h: usize, bw: usize, horizontal: bool) {
    let (lines, len, stride, step) = if horizontal {
        (h, w, w, 1)
    } else {
        (w, h, 1, w)
    };
    let (before, after) = (bw / 2, bw - bw / 2 - 1);
    for line in 0..lines {
        let base = line * stride;
        let at = |i: usize| src[base + i * step] as u32;
        let mut sum = 0u32;
        for i in 0..=after.min(len.saturating_sub(1)) {
            sum += at(i);
        }
        for i in 0..len {
            dst[base + i * step] = (sum / bw as u32) as u8;
            let add = i + after + 1;
            if add < len {
                sum += at(add);
            }
            if i >= before {
                sum -= at(i - before);
            }
        }
    }
}

/// Source-over a premultiplied sprite onto `pm` at `(x, y)`, only inside `clip` and outside `skip`.
fn blend_sprite(pm: &mut Pixmap, sprite: &Pixmap, x: i32, y: i32, clip: Rect, skip: Rect) {
    let area = Rect::new(x, y, sprite.width() as i32, sprite.height() as i32);
    let Some(c) = area
        .intersect(&pm_rect(pm))
        .and_then(|r| r.intersect(&clip))
    else {
        return;
    };
    for r in canvas::rect_minus(c, skip) {
        blend_region(pm, sprite, x, y, r);
    }
}

fn blend_region(pm: &mut Pixmap, sprite: &Pixmap, x: i32, y: i32, r: Rect) {
    let (dw, sw) = (pm.width() as usize * 4, sprite.width() as usize * 4);
    let n = r.w as usize * 4;
    let sdata = sprite.data();
    let ddata = pm.data_mut();
    for row in 0..r.h {
        let so = (r.y - y + row) as usize * sw + (r.x - x) as usize * 4;
        let doff = (r.y + row) as usize * dw + r.x as usize * 4;
        for (d, s) in ddata[doff..doff + n]
            .chunks_exact_mut(4)
            .zip(sdata[so..so + n].chunks_exact(4))
        {
            if s[3] == 0 {
                continue;
            }
            let inv = 255 - s[3] as u32;
            for k in 0..4 {
                d[k] = (s[k] as u32 + (d[k] as u32 * inv + 127) / 255).min(255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_is_soft_offset_down_and_skips_the_card() {
        let mut pm = Pixmap::new(300, 300).unwrap();
        let card = Rect::new(100, 100, 100, 80);
        draw(&mut pm, card, 16.0, 1.0, Rgba::rgb(0, 0, 0).with_alpha(160));
        let a = |x: i32, y: i32| pm.data()[(y as usize * 300 + x as usize) * 4 + 3];
        assert_eq!(a(150, 140), 0, "card interior untouched");
        assert!(a(150, 185) > a(150, 98), "heavier below than above");
        assert!(
            a(150, 185) > 20 && a(150, 185) < 160,
            "soft: {}",
            a(150, 185)
        );
        assert_eq!(a(5, 5), 0, "nothing far away");
        let b = bounds(card, 1.0);
        assert!(b.contains_rect(&card) && b.bottom() > card.bottom() + 12);
    }
}
