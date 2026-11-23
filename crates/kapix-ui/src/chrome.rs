//! Overlay chrome drawn on top of the frozen frame: selection border + handles, size label,
//! snap highlight, magnifier loupe and the tool-size ring.

use kapix_core::{Point, Rect, Rgba, Tool};
use tiny_skia::{FillRule, PathBuilder, Pixmap, Stroke, Transform};

use crate::canvas::{
    fill_circle, fill_rect_px, fill_rrect, paint, pm_rect, rrect_path, stroke_rect_px, stroke_rrect,
};
use crate::font::Font;
use crate::shadow;
use crate::theme::{Palette, RADIUS_FIELD, RADIUS_PAD};

/// Off-white used by the cursor ring (readable on any scrim, independent of theme).
const RING_LIGHT: Rgba = Rgba::rgb(0xF2, 0xF1, 0xEE);

pub(crate) fn px(v: f32, s: f32) -> i32 {
    (v * s).round() as i32
}

// ---------------------------------------------------------------------------------------------
// Selection border + handles
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Handle {
    NW,
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
}

pub(crate) fn handle_points(r: Rect) -> [(Handle, Point); 8] {
    let (l, t, rr, b) = (r.x, r.y, r.right(), r.bottom());
    let (mx, my) = (r.x + r.w / 2, r.y + r.h / 2);
    [
        (Handle::NW, Point::new(l, t)),
        (Handle::N, Point::new(mx, t)),
        (Handle::NE, Point::new(rr, t)),
        (Handle::E, Point::new(rr, my)),
        (Handle::SE, Point::new(rr, b)),
        (Handle::S, Point::new(mx, b)),
        (Handle::SW, Point::new(l, b)),
        (Handle::W, Point::new(l, my)),
    ]
}

/// Which handle (or edge) of `r` is under `p`.
pub(crate) fn hit_handle(r: Rect, p: Point, s: f32) -> Option<Handle> {
    let reach = px(7.0, s).max(5);
    let near = |a: Point| (a.x - p.x).abs() <= reach && (a.y - p.y).abs() <= reach;
    let pts = handle_points(r);
    // Corners first so they win over the edges on tiny selections.
    for &(h, a) in pts
        .iter()
        .filter(|(h, _)| matches!(h, Handle::NW | Handle::NE | Handle::SE | Handle::SW))
    {
        if near(a) {
            return Some(h);
        }
    }
    for &(h, a) in pts
        .iter()
        .filter(|(h, _)| matches!(h, Handle::N | Handle::E | Handle::S | Handle::W))
    {
        if near(a) {
            return Some(h);
        }
    }
    let edge = px(5.0, s).max(3);
    let (l, t, rr, b) = (r.x, r.y, r.right(), r.bottom());
    let in_x = p.x >= l - edge && p.x <= rr + edge;
    let in_y = p.y >= t - edge && p.y <= b + edge;
    if in_x && (p.y - t).abs() <= edge {
        return Some(Handle::N);
    }
    if in_x && (p.y - b).abs() <= edge {
        return Some(Handle::S);
    }
    if in_y && (p.x - l).abs() <= edge {
        return Some(Handle::W);
    }
    if in_y && (p.x - rr).abs() <= edge {
        return Some(Handle::E);
    }
    None
}

/// Half-extent of the region around the selection edge that border + handles may touch.
pub(crate) fn border_pad(s: f32) -> i32 {
    px(7.0, s) + 2
}

/// Border width (logical px) of the selection frame.
const BORDER_W: f32 = 1.5;
/// Handle radius (logical px).
const HANDLE_R: f32 = 4.5;

/// Contrast line outside the focus ring so the frame reads on any screenshot content.
fn contrast_line(p: &Palette) -> Rgba {
    if p.is_dark {
        Rgba::rgb(0, 0, 0).with_alpha(120)
    } else {
        Rgba::rgb(255, 255, 255).with_alpha(160)
    }
}

pub(crate) fn draw_border(pm: &mut Pixmap, r: Rect, handles: bool, s: f32, p: &Palette) {
    // 1.5 px focus ring just outside the selection (whole pixels + one fractional-alpha pixel)
    // followed by a 1 px contrast line.
    let w = BORDER_W * s;
    let whole = (w.floor() as i32).max(1);
    let frac = w - whole as f32;
    stroke_rect_px(pm, r.inflate(whole), whole, p.focus);
    let mut outer = whole;
    if frac > 0.05 {
        outer += 1;
        stroke_rect_px(
            pm,
            r.inflate(outer),
            1,
            p.focus.with_alpha((p.focus.a as f32 * frac) as u8),
        );
    }
    let c = px(1.0, s).max(1);
    stroke_rect_px(pm, r.inflate(outer + c), c, contrast_line(p));
    if !handles {
        return;
    }
    let hr = HANDLE_R * s;
    // Hide the midpoint handles when the selection is too small for them to be useful.
    let small = r.w < px(44.0, s) || r.h < px(44.0, s);
    for (h, pt) in handle_points(r) {
        if small && matches!(h, Handle::N | Handle::E | Handle::S | Handle::W) {
            continue;
        }
        let (cx, cy) = (pt.x as f32, pt.y as f32);
        // Slider-thumb look: lit dot with a 1 px `bg` ring.
        fill_circle(pm, cx, cy, hr + s.max(1.0), p.bg);
        fill_circle(pm, cx, cy, hr, p.lit);
    }
}

// ---------------------------------------------------------------------------------------------
// Snap highlight
// ---------------------------------------------------------------------------------------------

/// Border of the hovered window; the faint `lit` fill is part of the base restore
/// (see [`snap_tint`]) so it never accumulates.
pub(crate) fn draw_snap(pm: &mut Pixmap, r: Rect, s: f32, p: &Palette) {
    let t = px(2.0, s).max(2);
    stroke_rect_px(pm, r, t, p.focus);
    // Hairline contrast line just inside the ring so it reads on any window content.
    if r.w > 2 * t + 4 && r.h > 2 * t + 4 {
        stroke_rect_px(pm, r.inflate(-t), 1, contrast_line(p));
    }
}

/// ~8 % `lit` wash over the hovered window.
pub(crate) fn snap_tint(p: &Palette) -> Rgba {
    p.lit.with_alpha(20)
}

// ---------------------------------------------------------------------------------------------
// Size label
// ---------------------------------------------------------------------------------------------

pub(crate) fn label_size(font: &Font, text: &str, s: f32) -> (i32, i32) {
    let w = font.medium().measure(text, 12.0 * s).0 + 20.0 * s;
    (w.ceil() as i32, px(24.0, s))
}

pub(crate) fn draw_label(pm: &mut Pixmap, font: &Font, r: Rect, text: &str, s: f32, p: &Palette) {
    let radius = RADIUS_FIELD * s;
    fill_rrect(pm, r, radius, p.panel);
    stroke_rrect(pm, r, radius, 1.0, p.line);
    let f = font.medium();
    let tpx = 12.0 * s;
    let (tw, th) = (f.measure(text, tpx).0, f.measure("", tpx).1);
    f.draw(
        pm,
        text,
        r.x as f32 + (r.w as f32 - tw) / 2.0,
        r.y as f32 + (r.h as f32 - th) / 2.0,
        tpx,
        p.text,
    );
}

// ---------------------------------------------------------------------------------------------
// Tool-size ring (the "live cursor preview" for the wheel)
// ---------------------------------------------------------------------------------------------

pub(crate) fn ring_extent(tool: Tool, size: f32) -> f32 {
    match tool {
        Tool::Step => size + 3.0,
        Tool::Pixelate => size.max(2.0) / 2.0 * std::f32::consts::SQRT_2 + 3.0,
        _ => (size / 2.0 + 3.0).max(5.0),
    }
}

pub(crate) fn draw_ring(pm: &mut Pixmap, c: Point, tool: Tool, size: f32) {
    let (cx, cy) = (c.x as f32 + 0.5, c.y as f32 + 0.5);
    let path = match tool {
        Tool::Pixelate => {
            let h = size.max(2.0) / 2.0;
            rrect_path(cx - h, cy - h, 2.0 * h, 2.0 * h, 1.0)
        }
        Tool::Step => PathBuilder::from_circle(cx, cy, size),
        _ => PathBuilder::from_circle(cx, cy, (size / 2.0).max(2.0)),
    };
    let Some(path) = path else { return };
    pm.stroke_path(
        &path,
        &paint(Rgba::rgb(0, 0, 0).with_alpha(120)),
        &Stroke {
            width: 2.6,
            ..Stroke::default()
        },
        Transform::identity(),
        None,
    );
    pm.stroke_path(
        &path,
        &paint(RING_LIGHT.with_alpha(235)),
        &Stroke {
            width: 1.0,
            ..Stroke::default()
        },
        Transform::identity(),
        None,
    );
}

// ---------------------------------------------------------------------------------------------
// Loupe
// ---------------------------------------------------------------------------------------------

pub(crate) const LOUPE_CELLS: i32 = 15;
const CELL: f32 = 8.0;
const LOUPE_PAD: f32 = 8.0;
const ROW_H: f32 = 18.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LoupeData {
    pub card: Rect,
    /// Buffer-space pixel being inspected (clamped inside the frame).
    pub center: Point,
    pub virtual_pos: Point,
    pub color: Rgba,
    pub dims: Option<(i32, i32)>,
    pub s: f32,
}

pub(crate) fn loupe_card_size(s: f32, dims: bool) -> (i32, i32) {
    let cell = px(CELL, s);
    let pad = px(LOUPE_PAD, s);
    let img = cell * LOUPE_CELLS;
    let rows = if dims { 3 } else { 2 };
    (
        img + 2 * pad,
        pad + img + px(6.0, s) + rows * px(ROW_H, s) + pad - px(2.0, s),
    )
}

/// Fill the part of `r`'s corner square that lies outside the corner arc with `bg`
/// (rounds a pasted rectangular image).
fn round_corners(pm: &mut Pixmap, r: Rect, radius: f32, bg: Rgba) {
    let k = radius * 0.552_284_75;
    let (x0, y0, x1, y1) = (r.x as f32, r.y as f32, r.right() as f32, r.bottom() as f32);
    let corners = [
        // (corner point, along-x end, along-y end, control points)
        (
            (x0, y0),
            (x0 + radius, y0),
            (x0, y0 + radius),
            (x0 + radius - k, y0),
            (x0, y0 + radius - k),
        ),
        (
            (x1, y0),
            (x1 - radius, y0),
            (x1, y0 + radius),
            (x1 - radius + k, y0),
            (x1, y0 + radius - k),
        ),
        (
            (x1, y1),
            (x1 - radius, y1),
            (x1, y1 - radius),
            (x1 - radius + k, y1),
            (x1, y1 - radius + k),
        ),
        (
            (x0, y1),
            (x0 + radius, y1),
            (x0, y1 - radius),
            (x0 + radius - k, y1),
            (x0, y1 - radius + k),
        ),
    ];
    for (c, a, b, ca, cb) in corners {
        let mut pb = PathBuilder::new();
        pb.move_to(c.0, c.1);
        pb.line_to(a.0, a.1);
        pb.cubic_to(ca.0, ca.1, cb.0, cb.1, b.0, b.1);
        pb.close();
        if let Some(path) = pb.finish() {
            // Slightly grown so no un-blended image pixel survives the AA edge.
            pm.fill_path(
                &path,
                &paint(bg),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

pub(crate) fn draw_loupe(pm: &mut Pixmap, frame: &Pixmap, d: &LoupeData, font: &Font, p: &Palette) {
    let s = d.s;
    let card = d.card;
    let radius = RADIUS_PAD * s;
    shadow::draw(pm, card, radius, s, p.shadow);
    fill_rrect(pm, card, radius, p.panel);
    stroke_rrect(pm, card, radius, 1.0, p.line);

    let cell = px(CELL, s);
    let pad = px(LOUPE_PAD, s);
    let img = cell * LOUPE_CELLS;
    let (ix, iy) = (card.x + pad, card.y + pad);
    let half = LOUPE_CELLS / 2;
    let fb = pm_rect(frame);
    let (pw, fw) = (pm.width() as usize, frame.width() as usize);
    let clip = pm_rect(pm);
    {
        let fdata = frame.data();
        let pdata = pm.data_mut();
        let width = (cell * LOUPE_CELLS) as usize;
        let mut row = vec![0u8; width * 4];
        let off_a = [p.surface.b, p.surface.g, p.surface.r, 255];
        let off_b = [p.surface_2.b, p.surface_2.g, p.surface_2.r, 255];
        for cy in 0..LOUPE_CELLS {
            // Build one magnified pixel row, then replicate it `cell` times.
            for cx in 0..LOUPE_CELLS {
                let sp = Point::new(d.center.x - half + cx, d.center.y - half + cy);
                let col: [u8; 4] = if fb.contains(sp) {
                    let i = (sp.y as usize * fw + sp.x as usize) * 4;
                    [fdata[i], fdata[i + 1], fdata[i + 2], 255]
                } else if (cx + cy) % 2 == 0 {
                    off_a
                } else {
                    off_b
                };
                for k in 0..cell as usize {
                    let o = (cx as usize * cell as usize + k) * 4;
                    row[o..o + 4].copy_from_slice(&col);
                }
            }
            for k in 0..cell {
                let y = iy + cy * cell + k;
                if y < 0 || y >= clip.bottom() {
                    continue;
                }
                let x0 = ix.max(0);
                let x1 = (ix + width as i32).min(clip.right());
                if x1 <= x0 {
                    continue;
                }
                let o = (y as usize * pw + x0 as usize) * 4;
                let so = (x0 - ix) as usize * 4;
                let n = (x1 - x0) as usize * 4;
                pdata[o..o + n].copy_from_slice(&row[so..so + n]);
            }
        }
    }
    let img_rect = Rect::new(ix, iy, img, img);
    // Pixel grid: `line` at low alpha.
    if cell >= 5 {
        let grid = p.line.with_alpha(if p.is_dark { 60 } else { 90 });
        for i in 1..LOUPE_CELLS {
            fill_rect_px(pm, Rect::new(ix + i * cell, iy, 1, img), grid);
            fill_rect_px(pm, Rect::new(ix, iy + i * cell, img, 1), grid);
        }
    }
    // Red crosshair through the centre pixel (echoes the logo): four round-capped arms with the
    // centre pixel itself left visible.
    let c = Rect::new(ix + half * cell, iy + half * cell, cell, cell);
    let (ccx, ccy) = (c.x as f32 + c.w as f32 / 2.0, c.y as f32 + c.h as f32 / 2.0);
    let (inner, outer) = (c.w as f32 / 2.0 + 2.0 * s, c.w as f32 * 2.5);
    let lw = (1.6 * s).max(1.5);
    for (dx, dy) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let mut pb = PathBuilder::new();
        pb.move_to(ccx + dx * inner, ccy + dy * inner);
        pb.line_to(ccx + dx * outer, ccy + dy * outer);
        if let Some(path) = pb.finish() {
            let stroke = |w: f32| Stroke {
                width: w,
                line_cap: tiny_skia::LineCap::Round,
                ..Stroke::default()
            };
            pm.stroke_path(
                &path,
                &paint(Rgba::rgb(0, 0, 0).with_alpha(90)),
                &stroke(lw + 2.0),
                Transform::identity(),
                None,
            );
            pm.stroke_path(
                &path,
                &paint(p.red),
                &stroke(lw),
                Transform::identity(),
                None,
            );
        }
    }
    stroke_rect_px(pm, c.inflate(1), 1, Rgba::rgb(0, 0, 0).with_alpha(150));
    stroke_rect_px(pm, c, 1, p.red);
    // Round the magnified area to sit concentric inside the card.
    let ir = (RADIUS_PAD - LOUPE_PAD).max(4.0) * s;
    round_corners(pm, img_rect, ir, p.panel);
    stroke_rrect(pm, img_rect, ir, 1.0, p.line_strong.with_alpha(120));

    let reg = font.regular();
    let med = font.medium();
    let row_h = px(ROW_H, s);
    let (cpx, hpx) = (12.0 * s, 13.0 * s);
    let mut y = iy + img + px(6.0, s);
    let tx = ix as f32 + 2.0 * s;
    let center_y = |y: i32, th: f32| y as f32 + (row_h as f32 - th) / 2.0;
    if let Some((w, h)) = d.dims {
        let th = med.measure("", cpx).1;
        med.draw(pm, &format!("{w} × {h}"), tx, center_y(y, th), cpx, p.text);
        y += row_h;
    }
    let th = reg.measure("", cpx).1;
    let coords = format!("{}, {}", d.virtual_pos.x, d.virtual_pos.y);
    reg.draw(pm, &coords, tx, center_y(y, th), cpx, p.muted);
    y += row_h;
    // Colour swatch + hex.
    let chip = px(12.0, s);
    let chip_r = Rect::new(ix + px(2.0, s), y + (row_h - chip) / 2, chip, chip);
    fill_rrect(pm, chip_r, 4.0 * s, d.color);
    stroke_rrect(pm, chip_r, 4.0 * s, 1.0, p.line_strong);
    let th = med.measure("", hpx).1;
    med.draw(
        pm,
        &d.color.hex(),
        (chip_r.right() + px(8.0, s)) as f32,
        center_y(y, th),
        hpx,
        p.text,
    );
}

/// Shadow-inclusive bounds of the loupe card (dirty rect).
pub(crate) fn loupe_bounds(card: Rect, s: f32) -> Rect {
    shadow::bounds(card, s)
}
