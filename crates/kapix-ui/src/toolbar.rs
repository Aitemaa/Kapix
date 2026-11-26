//! The floating pill toolbar: layout, hit-testing and drawing (vector icons, no assets).

use kapix_core::color::{PRESET_NAMES, PRESETS};
use kapix_core::{Rect, Rgba, Tool};
use tiny_skia::Pixmap;

use crate::canvas::{fill_circle, fill_rect_px, fill_rrect, stroke_circle, stroke_rrect};
use crate::font::Font;
use crate::icons::{self, Icon};
use crate::shadow;
use crate::theme::{Palette, RADIUS_FIELD, RADIUS_PAD, mix};

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

/// Logical metrics (Kavox popover: 6 px padding, 32 px buttons, radius 10 / 16).
const PAD: f32 = 6.0;
const BTN: f32 = 32.0;
const GAP: f32 = 2.0;
const SWATCH_SLOT: f32 = 24.0;
const SWATCH_R: f32 = 9.0;
const SEP_SLOT: f32 = 13.0;
const SEP_H: f32 = 20.0;
const ICON: f32 = 18.0;

/// Logical tooltip metrics (concentric radii: outer 10, keycap inset 5, inner 5).
const TIP_H: f32 = 28.0;
const TIP_PAD_L: f32 = 10.0;
const TIP_PAD_R: f32 = 5.0;
const TIP_GAP: f32 = 7.0;
const TIP_FONT: f32 = 12.0;
const TIP_KEY_FONT: f32 = 11.0;
const TIP_KEY_H: f32 = 18.0;
const TIP_KEY_PAD_X: f32 = 5.0;
const TIP_KEY_MIN_W: f32 = 18.0;
const TIP_KEY_R: f32 = 5.0;

/// Toolbar size in physical pixels.
pub(crate) fn size(s: f32) -> (i32, i32) {
    let l = layout_at(0, 0, s);
    (l.rect.w, l.rect.h)
}

/// Lay the bar out with its top-left at `(x, y)`.
pub(crate) fn layout_at(x: i32, y: i32, s: f32) -> Layout {
    let h = px(2.0 * PAD + BTN, s);
    let pad = px(PAD, s);
    let btn = px(BTN, s);
    let pitch = px(BTN + GAP, s);
    let sw_pitch = px(SWATCH_SLOT, s);
    let sep_slot = px(SEP_SLOT, s);
    let by = y + (h - btn) / 2;
    let mut cx = x + pad;
    let mut items = Vec::new();
    let mut seps = Vec::new();
    let sep = |cx: &mut i32, seps: &mut Vec<i32>| {
        // `cx` sits one gap past the last button; centre the hairline in the separator slot.
        *cx += sep_slot / 2 - (pitch - btn);
        seps.push(*cx);
        *cx += sep_slot - sep_slot / 2;
    };

    for t in Tool::ALL {
        items.push((Rect::new(cx, by, btn, btn), Btn::Tool(t)));
        cx += pitch;
    }
    sep(&mut cx, &mut seps);
    for i in 0..PRESETS.len() {
        items.push((Rect::new(cx, by, sw_pitch, btn), Btn::Color(i)));
        cx += sw_pitch;
    }
    cx += pitch - btn; // swatch slots have no trailing gap: match the buttons' spacing
    sep(&mut cx, &mut seps);
    for b in [Btn::Undo, Btn::Redo] {
        items.push((Rect::new(cx, by, btn, btn), b));
        cx += pitch;
    }
    sep(&mut cx, &mut seps);
    for b in [Btn::Ocr, Btn::Pin, Btn::Save] {
        items.push((Rect::new(cx, by, btn, btn), b));
        cx += pitch;
    }
    sep(&mut cx, &mut seps);
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

    pub fn item_rect(&self, btn: Btn) -> Option<Rect> {
        self.items.iter().find(|(_, b)| *b == btn).map(|(r, _)| *r)
    }
}

/// Human-readable `(label, shortcut)` tooltip for a toolbar button.
pub(crate) fn tooltip(btn: Btn) -> (&'static str, &'static str) {
    match btn {
        Btn::Tool(Tool::Arrow) => ("Arrow", "A"),
        Btn::Tool(Tool::Rect) => ("Rectangle", "R"),
        Btn::Tool(Tool::Pen) => ("Pen", "P"),
        Btn::Tool(Tool::Highlighter) => ("Highlighter", "H"),
        Btn::Tool(Tool::Text) => ("Text", "T"),
        Btn::Tool(Tool::Pixelate) => ("Pixelate", "B"),
        Btn::Tool(Tool::Step) => ("Step counter", "N"),
        Btn::Color(i) => {
            const KEYS: [&str; 6] = ["1", "2", "3", "4", "5", "6"];
            (PRESET_NAMES[i], KEYS[i])
        }
        Btn::Undo => ("Undo", "Ctrl+Z"),
        Btn::Redo => ("Redo", "Ctrl+Y"),
        Btn::Ocr => ("Copy text (OCR)", "O"),
        Btn::Pin => ("Pin to screen", "Ctrl+P"),
        Btn::Save => ("Save", "Ctrl+S"),
        Btn::Copy => ("Copy", "Ctrl+C"),
        Btn::Close => ("Cancel", "Esc"),
    }
}

/// Physical size `(w, h)` of the hover tooltip pill for `btn`.
pub(crate) fn tooltip_size(font: &Font, btn: Btn, s: f32) -> (i32, i32) {
    let (label, shortcut) = tooltip(btn);
    let f = font.medium();
    let lw = f.measure(label, TIP_FONT * s).0;
    let kw = f.measure(shortcut, TIP_KEY_FONT * s).0;
    let key_w = (kw + 2.0 * TIP_KEY_PAD_X * s).max(TIP_KEY_MIN_W * s);
    let w = TIP_PAD_L * s + lw + TIP_GAP * s + key_w + TIP_PAD_R * s;
    (w.ceil() as i32, px(TIP_H, s))
}

/// Draw the hover tooltip pill for `btn` inside `r`.
pub(crate) fn draw_tooltip(pm: &mut Pixmap, font: &Font, r: Rect, btn: Btn, s: f32, p: &Palette) {
    let (label, shortcut) = tooltip(btn);
    let radius = RADIUS_FIELD * s;
    fill_rrect(pm, r, radius, p.panel);
    stroke_rrect(pm, r, radius, 1.0, p.line);

    let f = font.medium();
    let lpx = TIP_FONT * s;
    let lh = f.measure("", lpx).1;
    let lx = r.x as f32 + TIP_PAD_L * s;
    let ly = r.y as f32 + (r.h as f32 - lh) / 2.0;
    f.draw(pm, label, lx, ly, lpx, p.text);

    let kpx = TIP_KEY_FONT * s;
    let kw = f.measure(shortcut, kpx).0;
    let key_w = (kw + 2.0 * TIP_KEY_PAD_X * s)
        .max(TIP_KEY_MIN_W * s)
        .round() as i32;
    let key_h = px(TIP_KEY_H, s);
    let pad_r = px(TIP_PAD_R, s);
    let kr = Rect::new(
        r.right() - pad_r - key_w,
        r.y + (r.h - key_h) / 2,
        key_w,
        key_h,
    );
    let key_bg = if p.is_dark { p.surface_2 } else { p.surface };
    fill_rrect(pm, kr, TIP_KEY_R * s, key_bg);
    f.draw_centered(
        pm,
        shortcut,
        kr.x as f32 + kr.w as f32 / 2.0,
        kr.y as f32 + kr.h as f32 / 2.0,
        kpx,
        p.muted,
    );
}

/// Shadow-inclusive bounds of the bar (dirty rect).
pub(crate) fn bounds(bar: Rect, s: f32) -> Rect {
    shadow::bounds(bar, s)
}

/// Draw the toolbar.
pub(crate) fn draw(pm: &mut Pixmap, l: &Layout, st: &State, s: f32, p: &Palette) {
    let r = l.rect;
    let radius = RADIUS_PAD * s;
    shadow::draw(pm, r, radius, s, p.shadow);
    fill_rrect(pm, r, radius, p.panel);
    stroke_rrect(pm, r, radius, 1.0, p.line);

    let sep_h = px(SEP_H, s);
    for &x in &l.seps {
        let top = r.y + (r.h - sep_h) / 2;
        fill_rect_px(pm, Rect::new(x, top, px(1.0, s).max(1), sep_h), p.line);
    }
    let corner = RADIUS_FIELD * s;
    let icon_px = ICON * s;
    for &(ir, btn) in &l.items {
        let hovered = st.hover == Some(btn);
        let pressed = st.pressed == Some(btn);
        let disabled = (btn == Btn::Undo && !st.can_undo) || (btn == Btn::Redo && !st.can_redo);
        let (cx, cy) = (
            ir.x as f32 + ir.w as f32 / 2.0,
            ir.y as f32 + ir.h as f32 / 2.0,
        );
        if let Btn::Color(i) = btn {
            let selected = st.color == PRESETS[i];
            if hovered && !selected {
                fill_circle(
                    pm,
                    cx,
                    cy,
                    12.0 * s,
                    if pressed { p.line_strong } else { p.surface_2 },
                );
            }
            swatch(pm, cx, cy, PRESETS[i], selected, s, p);
            continue;
        }
        let active = matches!(btn, Btn::Tool(t) if st.tool == Some(t));
        let primary = btn == Btn::Copy;
        let danger = btn == Btn::Close;
        let lit = active || primary;
        let (bg, fg) = if lit {
            let bg = if pressed {
                mix(p.lit, p.muted, 0.75)
            } else if hovered {
                mix(p.lit, p.muted, 0.86)
            } else {
                p.lit
            };
            (Some(bg), p.on_lit)
        } else if danger && (hovered || pressed) {
            (
                Some(p.red.with_alpha(if pressed { 52 } else { 30 })),
                p.red_text,
            )
        } else if disabled {
            (None, p.faint)
        } else if pressed {
            (Some(p.line_strong), p.text)
        } else if hovered {
            (Some(p.surface_2), p.text)
        } else {
            (None, p.muted)
        };
        if let Some(bg) = bg {
            fill_rrect(pm, ir, corner, bg);
        }
        let filled = st.filled && matches!(btn, Btn::Tool(Tool::Rect));
        icons::draw(pm, icon_for(btn), cx, cy, icon_px, fg, filled);
    }
}

/// Colour swatch: 18 px disc with a 1 px `line` border; the selected one gets a 2 px `focus` ring.
fn swatch(pm: &mut Pixmap, cx: f32, cy: f32, c: Rgba, selected: bool, s: f32, p: &Palette) {
    let r = SWATCH_R * s;
    if selected {
        stroke_circle(pm, cx, cy, r + 2.5 * s, (2.0 * s).max(1.5), p.focus);
    }
    fill_circle(pm, cx, cy, r, c);
    stroke_circle(pm, cx, cy, r - 0.5 * s.max(1.0), s.max(1.0), p.line_strong);
}

fn icon_for(b: Btn) -> Icon {
    match b {
        Btn::Tool(Tool::Arrow) => Icon::Arrow,
        Btn::Tool(Tool::Rect) => Icon::Square,
        Btn::Tool(Tool::Pen) => Icon::Pencil,
        Btn::Tool(Tool::Highlighter) => Icon::Highlighter,
        Btn::Tool(Tool::Text) => Icon::Type,
        Btn::Tool(Tool::Pixelate) => Icon::Mosaic,
        Btn::Tool(Tool::Step) => Icon::Step,
        Btn::Undo => Icon::Undo,
        Btn::Redo => Icon::Redo,
        Btn::Ocr => Icon::ScanText,
        Btn::Pin => Icon::Pin,
        Btn::Save => Icon::Download,
        Btn::Copy => Icon::Copy,
        Btn::Close | Btn::Color(_) => Icon::Close,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_is_compact_and_ordered() {
        let (w, h) = size(1.0);
        assert!(w <= 720, "{w}");
        assert_eq!(h, 44);
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
        assert!(w15 > w && w15 <= 1080);
    }

    #[test]
    fn hit_testing_finds_buttons() {
        let l = layout_at(0, 0, 1.0);
        let (r, b) = l.items[l.items.len() - 2];
        assert_eq!(b, Btn::Copy);
        assert_eq!(l.hit(r.center()), Some(Btn::Copy));
        assert_eq!(l.item_rect(Btn::Copy), Some(r));
        assert_eq!(l.hit(kapix_core::Point::new(-3, -3)), None);
    }

    #[test]
    fn every_button_has_tooltip_and_renders() {
        let font = crate::font::test_font::load();
        let l = layout_at(0, 0, 1.0);
        for &(_, btn) in &l.items {
            let (label, key) = tooltip(btn);
            assert!(!label.is_empty(), "{btn:?}");
            assert!(!key.is_empty(), "{btn:?}");
            let (w, h) = tooltip_size(&font, btn, 1.0);
            assert!(w > 40 && h == 28, "{btn:?}: {w}x{h}");
            let (w15, h15) = tooltip_size(&font, btn, 1.5);
            assert!(w15 > w && h15 > h, "{btn:?}: {w15}x{h15} vs {w}x{h}");
        }
        for p in [Palette::DARK, Palette::LIGHT] {
            let (w, h) = tooltip_size(&font, Btn::Ocr, 1.0);
            let mut pm = Pixmap::new((w + 20) as u32, (h + 20) as u32).unwrap();
            let r = Rect::new(10, 10, w, h);
            draw_tooltip(&mut pm, &font, r, Btn::Ocr, 1.0, &p);
            let bg = [p.panel.b, p.panel.g, p.panel.r, 255];
            let drawn = pm.data().chunks_exact(4).filter(|px| px[3] > 0).count();
            let ink = pm
                .data()
                .chunks_exact(4)
                .filter(|px| px[3] == 255 && *px != bg)
                .count();
            assert!(drawn > (w * h / 2) as usize);
            assert!(ink > 20, "label + keycap ink drawn");
        }
    }
}
