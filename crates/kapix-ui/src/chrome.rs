//! Overlay chrome drawn on top of the frozen frame: selection border + handles, size label,
//! snap highlight, magnifier loupe and the tool-size ring.

use kapix_core::{Point, Rect, Rgba, Tool};
use tiny_skia::{FillRule, PathBuilder, Pixmap, Stroke, Transform};

use crate::canvas::{fill_rect_px, paint, pm_rect, rrect_path, stroke_rect_px};
use crate::font::Font;
use crate::toolbar::{ACCENT, shadow};

const WHITE: Rgba = Rgba::rgb(255, 255, 255);
const CARD: Rgba = Rgba::rgb(0x11, 0x18, 0x27);
const TEXT: Rgba = Rgba::rgb(0xF3, 0xF4, 0xF6);
const MUTED: Rgba = Rgba::rgb(0x9C, 0xA3, 0xAF);

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

pub(crate) fn draw_border(pm: &mut Pixmap, r: Rect, handles: bool, s: f32) {
    let t = px(1.0, s).max(1);
    stroke_rect_px(pm, r.inflate(t), t, ACCENT);
    if !handles {
        return;
    }
    let hs = 4.5 * s;
    // Hide the midpoint handles when the selection is too small for them to be useful.
    let small = r.w < px(44.0, s) || r.h < px(44.0, s);
    for (h, p) in handle_points(r) {
        if small && matches!(h, Handle::N | Handle::E | Handle::S | Handle::W) {
            continue;
        }
        let (cx, cy) = (p.x as f32, p.y as f32);
        if let Some(path) = rrect_path(
            cx - hs - 1.0,
            cy - hs - 1.0,
            2.0 * hs + 2.0,
            2.0 * hs + 2.0,
            3.0 * s,
        ) {
            pm.fill_path(
                &path,
                &paint(Rgba::rgb(0, 0, 0).with_alpha(70)),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        if let Some(path) = rrect_path(cx - hs, cy - hs, 2.0 * hs, 2.0 * hs, 2.5 * s) {
            pm.fill_path(
                &path,
                &paint(WHITE),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
            let stroke = Stroke {
                width: 1.0,
                ..Stroke::default()
            };
            pm.stroke_path(&path, &paint(ACCENT), &stroke, Transform::identity(), None);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Snap highlight
// ---------------------------------------------------------------------------------------------

pub(crate) fn draw_snap(pm: &mut Pixmap, r: Rect, s: f32) {
    let t = px(2.0, s).max(2);
    stroke_rect_px(pm, r, t, ACCENT);
    // Subtle inner white hairline for contrast against accent-colored content.
    if r.w > 2 * t + 4 && r.h > 2 * t + 4 {
        stroke_rect_px(pm, r.inflate(-t), 1, WHITE.with_alpha(70));
    }
}

// ---------------------------------------------------------------------------------------------
// Size label
// ---------------------------------------------------------------------------------------------

pub(crate) fn label_size(font: &Font, text: &str, s: f32) -> (i32, i32) {
    let w = font.measure_bold(text, 12.0 * s) + 16.0 * s;
    (w.ceil() as i32, px(22.0, s))
}

pub(crate) fn draw_label(pm: &mut Pixmap, font: &Font, r: Rect, text: &str, s: f32) {
    if let Some(path) = rrect_path(
        r.x as f32,
        r.y as f32,
        r.w as f32,
        r.h as f32,
        r.h as f32 / 2.0,
    ) {
        pm.fill_path(
            &path,
            &paint(CARD.with_alpha(224)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    let px_ = 12.0 * s;
    let (tw, th) = (font.measure_bold(text, px_), font.measure("", px_).1);
    font.draw_bold(
        pm,
        text,
        r.x as f32 + (r.w as f32 - tw) / 2.0,
        r.y as f32 + (r.h as f32 - th) / 2.0,
        px_,
        TEXT,
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
        &paint(WHITE.with_alpha(235)),
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LoupeData {
    pub card: Rect,
    /// Buffer-space pixel being inspected (clamped inside the frame).
    pub center: Point,
    pub virtual_pos: Point,
    pub color: Rgba,
    pub dims: Option<(i32, i32)>,
    pub s: f32,
    pub text: bool,
}

pub(crate) fn loupe_card_size(s: f32, dims: bool, text: bool) -> (i32, i32) {
    let cell = px(8.0, s);
    let pad = px(6.0, s);
    let img = cell * LOUPE_CELLS;
    let rows = if text { if dims { 3 } else { 2 } } else { 0 };
    let row_h = px(17.0, s);
    let extra = if rows > 0 {
        px(5.0, s) + rows * row_h + px(1.0, s)
    } else {
        0
    };
    (img + 2 * pad, pad + img + extra + pad)
}

pub(crate) fn draw_loupe(pm: &mut Pixmap, frame: &Pixmap, d: &LoupeData, font: Option<&Font>) {
    let s = d.s;
    let card = d.card;
    let radius = px(10.0, s) as f32;
    shadow(pm, card, radius, s);
    if let Some(path) = rrect_path(
        card.x as f32,
        card.y as f32,
        card.w as f32,
        card.h as f32,
        radius,
    ) {
        pm.fill_path(
            &path,
            &paint(CARD.with_alpha(240)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        let stroke = Stroke {
            width: 1.0,
            ..Stroke::default()
        };
        if let Some(b) = rrect_path(
            card.x as f32 + 0.5,
            card.y as f32 + 0.5,
            card.w as f32 - 1.0,
            card.h as f32 - 1.0,
            radius - 0.5,
        ) {
            pm.stroke_path(
                &b,
                &paint(WHITE.with_alpha(34)),
                &stroke,
                Transform::identity(),
                None,
            );
        }
    }
    let cell = px(8.0, s);
    let pad = px(6.0, s);
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
        for cy in 0..LOUPE_CELLS {
            // Build one magnified pixel row, then replicate it `cell` times.
            for cx in 0..LOUPE_CELLS {
                let sp = Point::new(d.center.x - half + cx, d.center.y - half + cy);
                let col: [u8; 4] = if fb.contains(sp) {
                    let i = (sp.y as usize * fw + sp.x as usize) * 4;
                    [fdata[i], fdata[i + 1], fdata[i + 2], 255]
                } else if (cx + cy) % 2 == 0 {
                    [0x1E, 0x15, 0x0F, 255]
                } else {
                    [0x2A, 0x20, 0x17, 255]
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
    // Pixel grid.
    if cell >= 5 {
        let grid = Rgba::rgb(0, 0, 0).with_alpha(46);
        for i in 1..LOUPE_CELLS {
            fill_rect_px(pm, Rect::new(ix + i * cell, iy, 1, img), grid);
            fill_rect_px(pm, Rect::new(ix, iy + i * cell, img, 1), grid);
        }
    }
    // Frame around the magnified area + center pixel marker.
    stroke_rect_px(
        pm,
        Rect::new(ix - 1, iy - 1, img + 2, img + 2),
        1,
        WHITE.with_alpha(40),
    );
    let c = Rect::new(ix + half * cell, iy + half * cell, cell, cell);
    stroke_rect_px(pm, c.inflate(1), 1, Rgba::rgb(0, 0, 0).with_alpha(235));
    stroke_rect_px(pm, c, 1, WHITE.with_alpha(245));

    let Some(font) = font.filter(|_| d.text) else {
        return;
    };
    let row_h = px(17.0, s);
    let tpx = 11.5 * s;
    let th = font.measure("", tpx).1;
    let mut y = iy + img + px(5.0, s);
    let tx = ix as f32 + 2.0 * s;
    let row = |pm: &mut Pixmap, y: i32, f: &dyn Fn(&mut Pixmap, f32, f32)| {
        f(pm, tx, y as f32 + (row_h as f32 - th) / 2.0);
    };
    if let Some((w, h)) = d.dims {
        let t = format!("{w} × {h}");
        row(pm, y, &|pm, x, yy| {
            font.draw_bold(pm, &t, x, yy, tpx, Rgba::rgb(0x93, 0xC5, 0xFD))
        });
        y += row_h;
    }
    let coords = format!("{}, {}", d.virtual_pos.x, d.virtual_pos.y);
    row(pm, y, &|pm, x, yy| {
        font.draw(pm, &coords, x, yy, tpx, MUTED)
    });
    y += row_h;
    // Color chip + hex.
    let chip = px(10.0, s);
    let chip_r = Rect::new(ix + px(2.0, s), y + (row_h - chip) / 2, chip, chip);
    if let Some(path) = rrect_path(
        chip_r.x as f32,
        chip_r.y as f32,
        chip_r.w as f32,
        chip_r.h as f32,
        2.5 * s,
    ) {
        pm.fill_path(
            &path,
            &paint(d.color),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        let stroke = Stroke {
            width: 1.0,
            ..Stroke::default()
        };
        pm.stroke_path(
            &path,
            &paint(WHITE.with_alpha(110)),
            &stroke,
            Transform::identity(),
            None,
        );
    }
    let hex = d.color.hex();
    let hx = (chip_r.right() + px(6.0, s)) as f32;
    font.draw(
        pm,
        &hex,
        hx,
        y as f32 + (row_h as f32 - th) / 2.0,
        tpx,
        TEXT,
    );
}
