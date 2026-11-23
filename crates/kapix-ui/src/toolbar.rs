//! The floating pill toolbar: layout, hit-testing and drawing (vector icons, no assets).

use kapix_core::color::PRESETS;
use kapix_core::{Rect, Rgba, Tool};
use tiny_skia::{FillRule, LineCap, LineJoin, Path, PathBuilder, Pixmap, Stroke, Transform};

use crate::canvas::{paint, rrect_path};

pub(crate) const ACCENT: Rgba = Rgba::rgb(0x3B, 0x82, 0xF6);
const BAR: Rgba = Rgba::rgb(0x1F, 0x29, 0x37);
const ICON: Rgba = Rgba::rgb(0xD1, 0xD5, 0xDB);
const ICON_ACTIVE: Rgba = Rgba::rgb(0x93, 0xC5, 0xFD);

/// A clickable toolbar element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Btn {
    Tool(Tool),
    Color(usize),
    Undo,
    Redo,
    Ocr,
    Pin,
    Save,
    Copy,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Icon {
    Arrow,
    Rect,
    Pen,
    Highlighter,
    Text,
    Pixelate,
    Step,
    Undo,
    Redo,
    Ocr,
    Pin,
    Save,
    Copy,
    Close,
}

#[derive(Debug, Clone)]
pub(crate) struct Layout {
    pub rect: Rect,
    pub items: Vec<(Rect, Btn)>,
    /// Separator x positions (absolute) — drawn as short vertical hairlines.
    pub seps: Vec<i32>,
}

/// Everything the drawing needs to know besides geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct State {
    pub tool: Option<Tool>,
    pub color: Rgba,
    pub filled: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub hover: Option<Btn>,
    pub pressed: Option<Btn>,
}

fn px(v: f32, s: f32) -> i32 {
    (v * s).round() as i32
}

/// Toolbar size in physical pixels.
pub(crate) fn size(s: f32) -> (i32, i32) {
    let l = layout_at(0, 0, s);
    (l.rect.w, l.rect.h)
}

/// Shadow extent around the bar (for dirty rects).
pub(crate) fn shadow_pad(s: f32) -> i32 {
    px(12.0, s)
}

/// Lay the bar out with its top-left at `(x, y)`.
pub(crate) fn layout_at(x: i32, y: i32, s: f32) -> Layout {
    let h = px(40.0, s);
    let pad = px(6.0, s);
    let btn = px(28.0, s);
    let pitch = px(30.0, s);
    let sw_pitch = px(20.0, s);
    let sep_w = px(9.0, s);
    let by = y + (h - btn) / 2;
    let mut cx = x + pad;
    let mut items = Vec::new();
    let mut seps = Vec::new();

    for t in Tool::ALL {
        items.push((Rect::new(cx, by, btn, btn), Btn::Tool(t)));
        cx += pitch;
    }
    cx += -(pitch - btn) + sep_w / 2;
    seps.push(cx);
    cx += sep_w / 2 + px(2.0, s);
    for i in 0..PRESETS.len() {
        items.push((Rect::new(cx, by, sw_pitch, btn), Btn::Color(i)));
        cx += sw_pitch;
    }
    cx += sep_w / 2;
    seps.push(cx);
    cx += sep_w / 2 + px(2.0, s);
    for b in [Btn::Undo, Btn::Redo] {
        items.push((Rect::new(cx, by, btn, btn), b));
        cx += pitch;
    }
    cx += -(pitch - btn) + sep_w / 2;
    seps.push(cx);
    cx += sep_w / 2 + px(2.0, s);
    for b in [Btn::Ocr, Btn::Pin, Btn::Save] {
        items.push((Rect::new(cx, by, btn, btn), b));
        cx += pitch;
    }
    cx += -(pitch - btn) + sep_w / 2;
    seps.push(cx);
    cx += sep_w / 2 + px(2.0, s);
    for b in [Btn::Copy, Btn::Close] {
        items.push((Rect::new(cx, by, btn, btn), b));
        cx += pitch;
    }
    let w = cx - (pitch - btn) + pad - x;
    Layout {
        rect: Rect::new(x, y, w, h),
        items,
        seps,
    }
}

impl Layout {
    pub fn hit(&self, p: kapix_core::Point) -> Option<Btn> {
        self.items
            .iter()
            .find(|(r, _)| r.contains(p))
            .map(|(_, b)| *b)
    }
}

fn rrect(pm: &mut Pixmap, r: Rect, radius: f32, color: Rgba) {
    if let Some(p) = rrect_path(r.x as f32, r.y as f32, r.w as f32, r.h as f32, radius) {
        pm.fill_path(
            &p,
            &paint(color),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

thread_local! {
    /// Pre-rendered shadow sprites keyed by (w, h, radius, scale*100): the layered AA fills are
    /// too slow to redo on every mouse move.
    static SHADOWS: std::cell::RefCell<Vec<((i32, i32, i32, i32), Pixmap)>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn shadow_margin(s: f32) -> i32 {
    px(7.0, s).max(3) + px(3.0, s) + 1
}

/// Soft drop shadow made of stacked translucent rounded rects (cached as a sprite).
pub(crate) fn shadow(pm: &mut Pixmap, r: Rect, radius: f32, s: f32) {
    let key = (r.w, r.h, radius as i32, (s * 100.0) as i32);
    let m = shadow_margin(s);
    SHADOWS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let idx = match cache.iter().position(|(k, _)| *k == key) {
            Some(i) => i,
            None => {
                let steps = px(7.0, s).max(3);
                let mut sprite =
                    Pixmap::new((r.w + 2 * m) as u32, (r.h + 2 * m) as u32).expect("shadow size");
                for i in (1..=steps).rev() {
                    let a = (9.0 * (1.0 - i as f32 / (steps as f32 + 1.0))).max(1.0) as u8;
                    let g = Rect::new(m - i, m - i + px(3.0, s), r.w + 2 * i, r.h + 2 * i);
                    rrect(
                        &mut sprite,
                        g,
                        radius + i as f32,
                        Rgba::rgb(0, 0, 0).with_alpha(a),
                    );
                }
                if cache.len() > 8 {
                    cache.clear();
                }
                cache.push((key, sprite));
                cache.len() - 1
            }
        };
        blend_sprite(pm, &cache[idx].1, r.x - m, r.y - m);
    });
}

/// Source-over a premultiplied sprite onto `pm` at `(x, y)` (clipped).
fn blend_sprite(pm: &mut Pixmap, sprite: &Pixmap, x: i32, y: i32) {
    let dst_r = crate::canvas::pm_rect(pm);
    let Some(c) = Rect::new(x, y, sprite.width() as i32, sprite.height() as i32).intersect(&dst_r)
    else {
        return;
    };
    let (dw, sw) = (pm.width() as usize * 4, sprite.width() as usize * 4);
    let n = c.w as usize * 4;
    let sdata = sprite.data();
    let ddata = pm.data_mut();
    for row in 0..c.h {
        let so = (c.y - y + row) as usize * sw + (c.x - x) as usize * 4;
        let doff = (c.y + row) as usize * dw + c.x as usize * 4;
        for (d, s) in ddata[doff..doff + n]
            .chunks_exact_mut(4)
            .zip(sdata[so..so + n].chunks_exact(4))
        {
            let inv = 255 - s[3] as u32;
            if s[3] == 0 {
                continue;
            }
            for k in 0..4 {
                d[k] = (s[k] as u32 + (d[k] as u32 * inv + 127) / 255).min(255) as u8;
            }
        }
    }
}

/// Draw the toolbar.
pub(crate) fn draw(pm: &mut Pixmap, l: &Layout, st: &State, s: f32) {
    let r = l.rect;
    let radius = r.h as f32 / 2.0;
    shadow(pm, r, radius, s);
    rrect(pm, r, radius, BAR.with_alpha(245));
    // Hairline border.
    if let Some(p) = rrect_path(
        r.x as f32 + 0.5,
        r.y as f32 + 0.5,
        r.w as f32 - 1.0,
        r.h as f32 - 1.0,
        radius - 0.5,
    ) {
        let stroke = Stroke {
            width: 1.0,
            ..Stroke::default()
        };
        pm.stroke_path(
            &p,
            &paint(Rgba::rgb(255, 255, 255).with_alpha(30)),
            &stroke,
            Transform::identity(),
            None,
        );
    }
    let sep_h = px(18.0, s);
    for &x in &l.seps {
        let top = r.y + (r.h - sep_h) / 2;
        crate::canvas::fill_rect_px(
            pm,
            Rect::new(x, top, 1.max(px(1.0, s)), sep_h),
            Rgba::rgb(255, 255, 255).with_alpha(36),
        );
    }
    let corner = px(8.0, s) as f32;
    let icon_px = 17.0 * s;
    for &(ir, btn) in &l.items {
        let hovered = st.hover == Some(btn);
        let pressed = st.pressed == Some(btn);
        let disabled = (btn == Btn::Undo && !st.can_undo) || (btn == Btn::Redo && !st.can_redo);
        let (cx, cy) = (
            ir.x as f32 + ir.w as f32 / 2.0,
            ir.y as f32 + ir.h as f32 / 2.0,
        );
        match btn {
            Btn::Color(i) => {
                let selected = st.color == PRESETS[i];
                if hovered && !selected {
                    rrect(
                        pm,
                        Rect::new(ir.x, ir.y, ir.w, ir.h),
                        corner,
                        Rgba::rgb(255, 255, 255).with_alpha(26),
                    );
                }
                swatch(pm, cx, cy, PRESETS[i], selected, s);
            }
            Btn::Copy => {
                let bg = if pressed {
                    Rgba::rgb(0x25, 0x63, 0xEB)
                } else if hovered {
                    Rgba::rgb(0x60, 0xA5, 0xFA)
                } else {
                    ACCENT
                };
                rrect(pm, ir, corner, bg);
                draw_icon(
                    pm,
                    Icon::Copy,
                    cx,
                    cy,
                    icon_px,
                    Rgba::rgb(255, 255, 255),
                    false,
                );
            }
            _ => {
                let active = matches!(btn, Btn::Tool(t) if st.tool == Some(t));
                if active {
                    rrect(
                        pm,
                        ir,
                        corner,
                        ACCENT.with_alpha(if hovered { 96 } else { 72 }),
                    );
                } else if (hovered || pressed) && !disabled {
                    rrect(
                        pm,
                        ir,
                        corner,
                        Rgba::rgb(255, 255, 255).with_alpha(if pressed { 40 } else { 26 }),
                    );
                }
                let mut col = if active { ICON_ACTIVE } else { ICON };
                if hovered && !active && !disabled {
                    col = Rgba::rgb(0xF9, 0xFA, 0xFB);
                }
                if disabled {
                    col = col.with_alpha(70);
                }
                let filled = st.filled && matches!(btn, Btn::Tool(Tool::Rect));
                draw_icon(pm, icon_for(btn), cx, cy, icon_px, col, filled);
            }
        }
    }
}

fn swatch(pm: &mut Pixmap, cx: f32, cy: f32, c: Rgba, selected: bool, s: f32) {
    let r = 6.5 * s;
    if selected {
        if let Some(p) = PathBuilder::from_circle(cx, cy, r + 3.4 * s) {
            let stroke = Stroke {
                width: (1.6 * s).max(1.0),
                ..Stroke::default()
            };
            pm.stroke_path(
                &p,
                &paint(Rgba::rgb(255, 255, 255).with_alpha(235)),
                &stroke,
                Transform::identity(),
                None,
            );
        }
    }
    if let Some(p) = PathBuilder::from_circle(cx, cy, r) {
        pm.fill_path(
            &p,
            &paint(c),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        // Inner hairline so white/ink swatches read against the bar.
        let stroke = Stroke {
            width: 1.0,
            ..Stroke::default()
        };
        pm.stroke_path(
            &p,
            &paint(Rgba::rgb(255, 255, 255).with_alpha(60)),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

fn icon_for(b: Btn) -> Icon {
    match b {
        Btn::Tool(Tool::Arrow) => Icon::Arrow,
        Btn::Tool(Tool::Rect) => Icon::Rect,
        Btn::Tool(Tool::Pen) => Icon::Pen,
        Btn::Tool(Tool::Highlighter) => Icon::Highlighter,
        Btn::Tool(Tool::Text) => Icon::Text,
        Btn::Tool(Tool::Pixelate) => Icon::Pixelate,
        Btn::Tool(Tool::Step) => Icon::Step,
        Btn::Undo => Icon::Undo,
        Btn::Redo => Icon::Redo,
        Btn::Ocr => Icon::Ocr,
        Btn::Pin => Icon::Pin,
        Btn::Save => Icon::Save,
        Btn::Copy => Icon::Copy,
        Btn::Close | Btn::Color(_) => Icon::Close,
    }
}

// ---------------------------------------------------------------------------------------------
// Icons (24×24 design grid, stroked)
// ---------------------------------------------------------------------------------------------

fn poly(pts: &[(f32, f32)], close: bool) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].0, pts[0].1);
    for p in &pts[1..] {
        pb.line_to(p.0, p.1);
    }
    if close {
        pb.close();
    }
    pb.finish()
}

fn draw_icon(pm: &mut Pixmap, icon: Icon, cx: f32, cy: f32, size: f32, color: Rgba, filled: bool) {
    let k = size / 24.0;
    let ts = Transform::from_translate(cx - 12.0 * k, cy - 12.0 * k).pre_scale(k, k);
    let p = paint(color);
    let stroke = Stroke {
        width: 2.1,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    let line = |pm: &mut Pixmap, path: Option<Path>| {
        if let Some(path) = path {
            pm.stroke_path(&path, &p, &stroke, ts, None);
        }
    };
    let fill = |pm: &mut Pixmap, path: Option<Path>| {
        if let Some(path) = path {
            pm.fill_path(&path, &p, FillRule::Winding, ts, None);
        }
    };
    match icon {
        Icon::Arrow => {
            line(pm, poly(&[(6.5, 17.5), (17.0, 7.0)], false));
            line(pm, poly(&[(8.5, 6.5), (17.5, 6.5), (17.5, 15.5)], false));
        }
        Icon::Rect => {
            let path = rrect_path(4.5, 5.5, 15.0, 13.0, 2.5);
            if filled {
                fill(pm, path.clone());
            }
            line(pm, path);
        }
        Icon::Pen => {
            line(
                pm,
                poly(
                    &[
                        (15.6, 4.8),
                        (19.2, 8.4),
                        (8.2, 19.4),
                        (3.6, 20.4),
                        (4.6, 15.8),
                    ],
                    true,
                ),
            );
            line(pm, poly(&[(13.0, 7.4), (16.6, 11.0)], false));
        }
        Icon::Highlighter => {
            line(
                pm,
                poly(&[(16.0, 3.5), (20.5, 8.0), (11.0, 17.5), (6.5, 13.0)], true),
            );
            line(pm, poly(&[(6.5, 13.0), (11.0, 17.5), (4.5, 19.5)], true));
            if let Some(path) = poly(&[(11.0, 21.0), (21.0, 21.0)], false) {
                let mut f = paint(color.with_alpha(color.a / 2));
                f.anti_alias = true;
                pm.stroke_path(
                    &path,
                    &f,
                    &Stroke {
                        width: 3.0,
                        line_cap: LineCap::Butt,
                        ..Stroke::default()
                    },
                    ts,
                    None,
                );
            }
        }
        Icon::Text => {
            line(
                pm,
                poly(&[(5.0, 8.0), (5.0, 5.0), (19.0, 5.0), (19.0, 8.0)], false),
            );
            line(pm, poly(&[(12.0, 5.0), (12.0, 19.0)], false));
            line(pm, poly(&[(9.0, 19.0), (15.0, 19.0)], false));
        }
        Icon::Pixelate => {
            let c = 16.0 / 3.0;
            for (gx, gy) in [(0, 0), (2, 0), (1, 1), (0, 2), (2, 2)] {
                let (x, y) = (4.0 + gx as f32 * c, 4.0 + gy as f32 * c);
                fill(pm, rrect_path(x + 0.3, y + 0.3, c - 0.6, c - 0.6, 0.8));
            }
        }
        Icon::Step => {
            if let Some(path) = PathBuilder::from_circle(12.0, 12.0, 8.6) {
                pm.stroke_path(&path, &p, &stroke, ts, None);
            }
            line(pm, poly(&[(10.0, 10.4), (12.6, 8.2), (12.6, 16.0)], false));
        }
        Icon::Undo | Icon::Redo => {
            let mx = |x: f32| if icon == Icon::Redo { 24.0 - x } else { x };
            line(
                pm,
                poly(&[(mx(9.0), 14.0), (mx(4.0), 9.0), (mx(9.0), 4.0)], false),
            );
            let mut pb = PathBuilder::new();
            pb.move_to(mx(4.0), 9.0);
            pb.line_to(mx(14.5), 9.0);
            pb.cubic_to(mx(17.5), 9.0, mx(20.0), 11.5, mx(20.0), 14.5);
            pb.cubic_to(mx(20.0), 17.5, mx(17.5), 20.0, mx(14.5), 20.0);
            pb.line_to(mx(11.0), 20.0);
            line(pm, pb.finish());
        }
        Icon::Ocr => {
            line(pm, poly(&[(4.0, 8.0), (4.0, 4.0), (8.0, 4.0)], false));
            line(pm, poly(&[(16.0, 4.0), (20.0, 4.0), (20.0, 8.0)], false));
            line(pm, poly(&[(20.0, 16.0), (20.0, 20.0), (16.0, 20.0)], false));
            line(pm, poly(&[(8.0, 20.0), (4.0, 20.0), (4.0, 16.0)], false));
            line(pm, poly(&[(8.0, 9.5), (16.0, 9.5)], false));
            line(pm, poly(&[(8.0, 13.0), (16.0, 13.0)], false));
            line(pm, poly(&[(8.0, 16.5), (12.5, 16.5)], false));
        }
        Icon::Pin => {
            line(
                pm,
                poly(
                    &[
                        (9.0, 3.8),
                        (15.0, 3.8),
                        (14.2, 9.6),
                        (17.8, 13.6),
                        (6.2, 13.6),
                        (9.8, 9.6),
                    ],
                    true,
                ),
            );
            line(pm, poly(&[(12.0, 13.6), (12.0, 20.4)], false));
        }
        Icon::Save => {
            line(pm, poly(&[(12.0, 4.0), (12.0, 15.0)], false));
            line(pm, poly(&[(7.5, 10.5), (12.0, 15.0), (16.5, 10.5)], false));
            line(pm, poly(&[(5.0, 19.5), (19.0, 19.5)], false));
        }
        Icon::Copy => {
            line(pm, rrect_path(9.5, 9.5, 10.0, 10.0, 2.2));
            let mut pb = PathBuilder::new();
            pb.move_to(9.5, 14.5);
            pb.line_to(6.5, 14.5);
            pb.quad_to(4.5, 14.5, 4.5, 12.5);
            pb.line_to(4.5, 6.5);
            pb.quad_to(4.5, 4.5, 6.5, 4.5);
            pb.line_to(12.5, 4.5);
            pb.quad_to(14.5, 4.5, 14.5, 6.5);
            pb.line_to(14.5, 9.5);
            line(pm, pb.finish());
        }
        Icon::Close => {
            line(pm, poly(&[(6.5, 6.5), (17.5, 17.5)], false));
            line(pm, poly(&[(17.5, 6.5), (6.5, 17.5)], false));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_is_compact_and_ordered() {
        let (w, h) = size(1.0);
        assert!(w <= 600, "{w}");
        assert_eq!(h, 40);
        let l = layout_at(10, 20, 1.0);
        assert_eq!(l.items.len(), 7 + 6 + 2 + 3 + 2);
        // Every button lies inside the bar and none overlap.
        for (i, (r, _)) in l.items.iter().enumerate() {
            assert!(l.rect.contains_rect(r), "{r:?} outside {:?}", l.rect);
            for (r2, _) in &l.items[i + 1..] {
                assert!(r.intersect(r2).is_none());
            }
        }
        let (w15, _) = size(1.5);
        assert!(w15 > w && w15 <= 900);
    }

    #[test]
    fn hit_testing_finds_buttons() {
        let l = layout_at(0, 0, 1.0);
        let (r, b) = l.items[l.items.len() - 2];
        assert_eq!(b, Btn::Copy);
        assert_eq!(l.hit(r.center()), Some(Btn::Copy));
        assert_eq!(l.hit(kapix_core::Point::new(-3, -3)), None);
    }
}
