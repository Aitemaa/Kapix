//! Annotation rasterizer shared by live preview and export (pixel-identical WYSIWYG).

use kapix_core::{Annotation, Image, PointF, Rect, Rgba};
use tiny_skia::{
    BlendMode, FillRule, LineCap, LineJoin, Path, PathBuilder, Pixmap, PixmapRef, Stroke, Transform,
};

use crate::canvas::{self, paint, rrect_path};
use crate::font::Font;

/// Ink color used for text/numbers on light fills and for dark backing pills (Kavox `bg` ink).
pub(crate) const INK: Rgba = Rgba::rgb(0x14, 0x14, 0x14);
/// Warm off-white used on dark fills and for light backing pills (Kavox `lit`).
const PAPER: Rgba = Rgba::rgb(0xF2, 0xF1, 0xEE);
/// Alpha of the highlighter color (~40%).
const HIGHLIGHT_ALPHA: u8 = 102;

/// Draw `items` in order into `pixmap`, where pixmap pixel (0,0) corresponds to buffer
/// coordinate `origin` (i.e. every annotation is translated by `-origin`).
/// `Pixelate` destructively mosaics the pixmap's *current* pixels in its rect (so it also
/// obscures anything drawn before it).
pub fn render_annotations(
    pixmap: &mut tiny_skia::Pixmap,
    items: &[Annotation],
    origin: (i32, i32),
    font: &Font,
) {
    let ts = Transform::from_translate(-origin.0 as f32, -origin.1 as f32);
    for item in items {
        match item {
            Annotation::Arrow {
                from,
                to,
                color,
                width,
            } => draw_arrow(pixmap, *from, *to, *color, *width, ts),
            Annotation::Rect {
                rect,
                color,
                width,
                filled,
            } => draw_rect(pixmap, *rect, *color, *width, *filled, ts),
            Annotation::Pen {
                points,
                color,
                width,
            } => {
                draw_polyline(pixmap, points, *color, *width, false, ts);
            }
            Annotation::Highlighter {
                points,
                color,
                width,
            } => {
                draw_polyline(
                    pixmap,
                    points,
                    color.with_alpha(color.a.min(HIGHLIGHT_ALPHA)),
                    *width,
                    true,
                    ts,
                );
            }
            Annotation::Text {
                pos,
                text,
                color,
                size,
                backing,
            } => {
                draw_text(pixmap, font, *pos, text, *color, *size, *backing, origin);
            }
            Annotation::Pixelate { rect, block } => {
                let r = rect.translate(-origin.0, -origin.1);
                let (w, h) = (pixmap.width(), pixmap.height());
                kapix_core::image::pixelate_bgra(pixmap.data_mut(), w, h, r, *block);
            }
            Annotation::Step {
                center,
                number,
                color,
                radius,
            } => {
                draw_step(pixmap, font, *center, *number, *color, *radius, ts, origin);
            }
        }
    }
}

/// Export path: crop `selection` out of the raw frame and render `items` onto it.
/// Returns an opaque BGRA image of `selection ∩ frame.bounds()`.
pub fn flatten(frame: &Image, selection: Rect, items: &[Annotation], font: &Font) -> Image {
    match PixmapRef::from_bytes(&frame.data, frame.width, frame.height) {
        Some(pm) => flatten_ref(pm, selection, items, font),
        None => Image {
            width: 0,
            height: 0,
            data: Vec::new(),
        },
    }
}

pub(crate) fn flatten_ref(
    frame: PixmapRef,
    selection: Rect,
    items: &[Annotation],
    font: &Font,
) -> Image {
    let bounds = Rect::new(0, 0, frame.width() as i32, frame.height() as i32);
    let Some(sel) = selection.intersect(&bounds) else {
        return Image {
            width: 0,
            height: 0,
            data: Vec::new(),
        };
    };
    // Pixelate averages whole cells, so the working region must contain every pixelate rect
    // that touches the selection: rendering on a tighter crop would change cell averages at the
    // crop border and break WYSIWYG with the full-frame preview.
    let mut region = sel;
    for a in items {
        if let Annotation::Pixelate { rect, .. } = a
            && rect.intersect(&sel).is_some()
            && let Some(r) = rect.intersect(&bounds)
        {
            region = region.union(&r);
        }
    }
    if items.is_empty() {
        return crop_image(frame, sel);
    }
    // Render in frame coordinates (origin 0,0) like the live overlay does: tiny-skia's
    // anti-aliasing is not bit-exact under translation, and WYSIWYG must be. Only the pages of
    // the working pixmap (zeroed lazily by the allocator) inside `region` are ever touched; it is
    // frame-sized so that clipping at the pixmap edge matches the overlay's as well.
    let mut work = Pixmap::new(frame.width(), frame.height()).expect("non-empty frame");
    let stride = frame.width() as usize * 4;
    let wstride = work.width() as usize * 4;
    let n = region.w as usize * 4;
    for row in 0..region.h as usize {
        let src = (region.y as usize + row) * stride + region.x as usize * 4;
        let dst = (region.y as usize + row) * wstride + region.x as usize * 4;
        work.data_mut()[dst..dst + n].copy_from_slice(&frame.data()[src..src + n]);
    }
    render_annotations(&mut work, items, (0, 0), font);
    crop_image(work.as_ref(), sel)
}

/// Raw copy of `r ∩ pixmap` (no annotations).
pub(crate) fn crop_image(frame: PixmapRef, r: Rect) -> Image {
    let bounds = Rect::new(0, 0, frame.width() as i32, frame.height() as i32);
    match r.intersect(&bounds) {
        Some(r) => canvas::pixmap_to_image(crop_pixmap(frame, r)),
        None => Image {
            width: 0,
            height: 0,
            data: Vec::new(),
        },
    }
}

fn crop_pixmap(frame: PixmapRef, r: Rect) -> Pixmap {
    let mut out = Pixmap::new(r.w as u32, r.h as u32).expect("non-empty crop");
    let stride = frame.width() as usize * 4;
    let n = r.w as usize * 4;
    let src = frame.data();
    for (row, dst) in out.data_mut().chunks_exact_mut(n).enumerate() {
        let o = (r.y as usize + row) * stride + r.x as usize * 4;
        dst.copy_from_slice(&src[o..o + n]);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Geometry shared by renderer and dirty-rect bookkeeping
// ---------------------------------------------------------------------------------------------

/// Arrowhead length for a stroke width.
fn head_len(width: f32) -> f32 {
    (width * 4.0).max(11.0)
}

/// Padding of the text backing pill around the text block.
fn pill_pad(size: f32) -> (f32, f32) {
    (size * 0.55, size * 0.28)
}

/// Rendering-accurate bounds of an annotation (buffer coordinates, integer, conservative).
pub fn annotation_bounds(a: &Annotation, font: &Font) -> Rect {
    fn pts(points: &[PointF], pad: f32) -> Rect {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in points {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        if x0 > x1 {
            return Rect::default();
        }
        Rect::from_ltrb(
            (x0 - pad).floor() as i32,
            (y0 - pad).floor() as i32,
            (x1 + pad).ceil() as i32 + 1,
            (y1 + pad).ceil() as i32 + 1,
        )
    }
    match a {
        Annotation::Arrow {
            from, to, width, ..
        } => pts(&[*from, *to], head_len(*width).max(*width) + 3.0),
        Annotation::Rect { rect, width, .. } => rect.inflate(width.ceil() as i32 + 2),
        Annotation::Pen { points, width, .. } => pts(points, width / 2.0 + 3.0),
        Annotation::Highlighter { points, width, .. } => pts(points, width * 0.75 + 3.0),
        Annotation::Pixelate { rect, .. } => *rect,
        Annotation::Step { center, radius, .. } => pts(&[*center], radius + 4.0),
        Annotation::Text {
            pos, text, size, ..
        } => {
            let (w, h) = font.medium().measure_multiline(text, *size);
            let (px, py) = pill_pad(*size);
            pts(&[*pos, PointF::new(pos.x + w, pos.y + h)], px.max(py) + 4.0)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------------------------

fn stroke(width: f32, cap: LineCap, join: LineJoin) -> Stroke {
    Stroke {
        width,
        line_cap: cap,
        line_join: join,
        ..Stroke::default()
    }
}

fn dot(pm: &mut Pixmap, p: PointF, r: f32, color: Rgba, ts: Transform) {
    if let Some(path) = PathBuilder::from_circle(p.x, p.y, r.max(0.5)) {
        pm.fill_path(&path, &paint(color), FillRule::Winding, ts, None);
    }
}

fn draw_arrow(pm: &mut Pixmap, from: PointF, to: PointF, color: Rgba, width: f32, ts: Transform) {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = dx.hypot(dy);
    if len < 0.5 {
        dot(pm, from, width / 2.0, color, ts);
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let hl = head_len(width).min(len * 0.85);
    let half = hl * 0.42;
    let base = PointF::new(to.x - ux * hl, to.y - uy * hl);
    let (nx, ny) = (-uy, ux);
    let p = paint(color);

    // Shaft ends inside the head so there is never a seam.
    let shaft_end = PointF::new(to.x - ux * hl * 0.75, to.y - uy * hl * 0.75);
    let mut pb = PathBuilder::new();
    pb.move_to(from.x, from.y);
    pb.line_to(shaft_end.x, shaft_end.y);
    if let Some(path) = pb.finish() {
        pm.stroke_path(
            &path,
            &p,
            &stroke(width, LineCap::Round, LineJoin::Round),
            ts,
            None,
        );
    }
    let mut pb = PathBuilder::new();
    pb.move_to(to.x, to.y);
    pb.line_to(base.x + nx * half, base.y + ny * half);
    pb.line_to(base.x - nx * half, base.y - ny * half);
    pb.close();
    if let Some(path) = pb.finish() {
        pm.fill_path(&path, &p, FillRule::Winding, ts, None);
        // Slightly rounded corners on the head.
        pm.stroke_path(
            &path,
            &p,
            &stroke((width * 0.3).max(1.0), LineCap::Round, LineJoin::Round),
            ts,
            None,
        );
    }
}

fn draw_rect(pm: &mut Pixmap, rect: Rect, color: Rgba, width: f32, filled: bool, ts: Transform) {
    if rect.is_empty() {
        return;
    }
    let (x, y, w, h) = (rect.x as f32, rect.y as f32, rect.w as f32, rect.h as f32);
    let radius = (width * 0.9).min(w.min(h) / 4.0).min(8.0);
    let p = paint(color);
    if filled {
        if let Some(path) = rrect_path(x, y, w, h, radius) {
            pm.fill_path(&path, &p, FillRule::Winding, ts, None);
        }
    } else {
        // Stroke is centered on the path: inset by half the width so the outer edge is `rect`.
        let i = width / 2.0;
        if w <= width || h <= width {
            if let Some(path) = rrect_path(x, y, w, h, radius) {
                pm.fill_path(&path, &p, FillRule::Winding, ts, None);
            }
        } else if let Some(path) =
            rrect_path(x + i, y + i, w - width, h - width, (radius - i).max(0.0))
        {
            pm.stroke_path(
                &path,
                &p,
                &stroke(width, LineCap::Butt, LineJoin::Round),
                ts,
                None,
            );
        }
    }
}

/// Smooth path through `points` using midpoint quadratic curves.
fn smooth_path(points: &[PointF]) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].x, points[0].y);
    if points.len() == 2 {
        pb.line_to(points[1].x, points[1].y);
    } else {
        for i in 1..points.len() - 1 {
            let (p, q) = (points[i], points[i + 1]);
            pb.quad_to(p.x, p.y, (p.x + q.x) / 2.0, (p.y + q.y) / 2.0);
        }
        let last = points[points.len() - 1];
        pb.line_to(last.x, last.y);
    }
    pb.finish()
}

fn draw_polyline(
    pm: &mut Pixmap,
    points: &[PointF],
    color: Rgba,
    width: f32,
    highlighter: bool,
    ts: Transform,
) {
    let Some(first) = points.first() else { return };
    let mut p = paint(color);
    if highlighter {
        p.blend_mode = BlendMode::Multiply;
    }
    let extent = points.iter().fold(0f32, |m, q| {
        m.max((q.x - first.x).abs()).max((q.y - first.y).abs())
    });
    if points.len() < 2 || extent < 0.25 {
        if highlighter {
            let r = width / 2.0;
            if let Some(path) = rrect_path(first.x - r, first.y - r, width, width, r * 0.3) {
                pm.fill_path(&path, &p, FillRule::Winding, ts, None);
            }
        } else {
            dot(pm, *first, width / 2.0, color, ts);
        }
        return;
    }
    if let Some(path) = smooth_path(points) {
        let cap = if highlighter {
            LineCap::Square
        } else {
            LineCap::Round
        };
        pm.stroke_path(&path, &p, &stroke(width, cap, LineJoin::Round), ts, None);
    }
}

/// Backing-pill color that contrasts with text color `c`.
pub(crate) fn pill_color(c: Rgba) -> Rgba {
    if c.luminance() > 0.45 {
        INK.with_alpha(230)
    } else {
        PAPER.with_alpha(235)
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    pm: &mut Pixmap,
    font: &Font,
    pos: PointF,
    text: &str,
    color: Rgba,
    size: f32,
    backing: bool,
    origin: (i32, i32),
) {
    let font = &font.medium();
    let (x, y) = (pos.x - origin.0 as f32, pos.y - origin.1 as f32);
    let (w, h) = font.measure_multiline(text, size);
    if backing && !text.is_empty() {
        let (px, py) = pill_pad(size);
        let (bw, bh) = (w + 2.0 * px, h + 2.0 * py);
        if let Some(path) = rrect_path(x - px, y - py, bw, bh, (bh / 2.0).min(size * 0.8)) {
            pm.fill_path(
                &path,
                &paint(pill_color(color)),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
    font.draw_multiline(pm, text, x, y, size, color);
}

#[allow(clippy::too_many_arguments)]
fn draw_step(
    pm: &mut Pixmap,
    font: &Font,
    center: PointF,
    number: u32,
    color: Rgba,
    radius: f32,
    ts: Transform,
    origin: (i32, i32),
) {
    let contrast = if color.luminance() > 0.4 { INK } else { PAPER };
    // Contrasting outer ring first (so the fill edge stays crisp), then the badge.
    if let Some(ring) = PathBuilder::from_circle(center.x, center.y, radius + 0.9) {
        pm.stroke_path(
            &ring,
            &paint(contrast.with_alpha(235)),
            &stroke(1.8, LineCap::Butt, LineJoin::Round),
            ts,
            None,
        );
    }
    dot(pm, center, radius, color, ts);
    let s = number.to_string();
    let px = radius
        * match s.len() {
            1 => 1.12,
            2 => 0.95,
            _ => 0.78,
        };
    font.heading().draw_centered(
        pm,
        &s,
        center.x - origin.0 as f32,
        center.y - origin.1 as f32,
        px,
        contrast,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::test_font;
    use kapix_core::color::PRESETS;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Image {
        Image::from_bgra(w, h, c.repeat(w as usize * h as usize))
    }

    fn px(pm: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * pm.width() + x) * 4) as usize;
        pm.data()[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn rect_outline_lands_where_expected_in_bgra() {
        let mut pm = canvas::image_to_pixmap(solid(40, 40, [255, 255, 255, 255]));
        let red = PRESETS[0];
        let r = Annotation::Rect {
            rect: Rect::new(10, 10, 20, 20),
            color: red,
            width: 4.0,
            filled: false,
        };
        render_annotations(&mut pm, &[r], (0, 0), &test_font::load());
        // Middle of the left edge is solid red: BGRA = (0x2D, 0x26, 0xE8, 255).
        assert_eq!(px(&pm, 11, 20), [0x2D, 0x26, 0xE8, 255]);
        // Interior and exterior stay white.
        assert_eq!(px(&pm, 20, 20), [255, 255, 255, 255]);
        assert_eq!(px(&pm, 5, 20), [255, 255, 255, 255]);
        // Origin shift moves it.
        let mut pm = canvas::image_to_pixmap(solid(40, 40, [255, 255, 255, 255]));
        let r = Annotation::Rect {
            rect: Rect::new(110, 10, 20, 20),
            color: red,
            width: 4.0,
            filled: false,
        };
        render_annotations(&mut pm, &[r], (100, 0), &test_font::load());
        assert_eq!(px(&pm, 11, 20), [0x2D, 0x26, 0xE8, 255]);
    }

    #[test]
    fn filled_rect_is_solid() {
        let mut pm = canvas::image_to_pixmap(solid(30, 30, [0, 0, 0, 255]));
        let r = Annotation::Rect {
            rect: Rect::new(5, 5, 20, 20),
            color: PRESETS[3],
            width: 3.0,
            filled: true,
        };
        render_annotations(&mut pm, &[r], (0, 0), &test_font::load());
        assert_eq!(px(&pm, 15, 15), [0xE9, 0xA5, 0x0E, 255]);
    }

    #[test]
    fn arrow_draws_shaft_and_head_toward_target() {
        let mut pm = canvas::image_to_pixmap(solid(100, 40, [255, 255, 255, 255]));
        let a = Annotation::Arrow {
            from: PointF::new(10.0, 20.0),
            to: PointF::new(90.0, 20.0),
            color: PRESETS[0],
            width: 4.0,
        };
        render_annotations(&mut pm, &[a], (0, 0), &test_font::load());
        assert_eq!(px(&pm, 40, 20), [0x2D, 0x26, 0xE8, 255], "shaft");
        assert_eq!(px(&pm, 86, 20), [0x2D, 0x26, 0xE8, 255], "head body");
        // The head is wider than the shaft near its base, absent behind the start.
        assert_ne!(px(&pm, 78, 14), [255, 255, 255, 255], "head flare");
        assert_eq!(
            px(&pm, 40, 12),
            [255, 255, 255, 255],
            "no shaft thickness there"
        );
        assert_eq!(px(&pm, 3, 20), [255, 255, 255, 255]);
    }

    #[test]
    fn pen_and_highlighter_blend() {
        let mut pm = canvas::image_to_pixmap(solid(60, 30, [255, 255, 255, 255]));
        let pts = vec![
            PointF::new(5.0, 15.0),
            PointF::new(30.0, 15.0),
            PointF::new(55.0, 15.0),
        ];
        let pen = Annotation::Pen {
            points: pts.clone(),
            color: PRESETS[2],
            width: 4.0,
        };
        render_annotations(&mut pm, &[pen], (0, 0), &test_font::load());
        assert_eq!(px(&pm, 30, 15), [0x81, 0xB9, 0x10, 255]);
        let mut pm = canvas::image_to_pixmap(solid(60, 30, [200, 200, 200, 255]));
        let hl = Annotation::Highlighter {
            points: pts,
            color: PRESETS[1],
            width: 12.0,
        };
        render_annotations(&mut pm, &[hl], (0, 0), &test_font::load());
        let p = px(&pm, 30, 15);
        // Multiply with a 40% amber over gray darkens blue the most, keeps red high.
        assert!(p[0] < p[2] && p[2] > 150 && p[3] == 255, "{p:?}");
        assert!(p[0] < 200);
    }

    #[test]
    fn single_click_pen_makes_a_dot() {
        let mut pm = canvas::image_to_pixmap(solid(20, 20, [255, 255, 255, 255]));
        let pen = Annotation::Pen {
            points: vec![PointF::new(10.0, 10.0)],
            color: PRESETS[0],
            width: 6.0,
        };
        render_annotations(&mut pm, &[pen], (0, 0), &test_font::load());
        assert_eq!(px(&pm, 10, 10), [0x2D, 0x26, 0xE8, 255]);
    }

    #[test]
    fn pixelate_makes_cells_uniform_and_is_destructive() {
        let mut img = solid(32, 32, [0, 0, 0, 255]);
        for (i, p) in img.data.chunks_exact_mut(4).enumerate() {
            p[0] = (i * 7 % 251) as u8;
            p[1] = (i * 13 % 253) as u8;
        }
        let original = img.clone();
        let items = [Annotation::Pixelate {
            rect: Rect::new(4, 4, 16, 16),
            block: 8,
        }];
        let out = flatten(&img, Rect::new(0, 0, 32, 32), &items, &test_font::load());
        let first = &out.data[(4 * 32 + 4) * 4..(4 * 32 + 4) * 4 + 4];
        for y in 4..12usize {
            for x in 4..12usize {
                assert_eq!(&out.data[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4], first);
            }
        }
        assert_eq!(out.pixel(0, 0), original.pixel(0, 0));
        assert_ne!(out.pixel(5, 6), original.pixel(5, 6));
    }

    #[test]
    fn flatten_matches_full_render_cropped() {
        let mut img = solid(80, 60, [30, 60, 90, 255]);
        for (i, p) in img.data.chunks_exact_mut(4).enumerate() {
            p[2] = (i % 200) as u8;
        }
        let font = test_font::load();
        let items = vec![
            Annotation::Rect {
                rect: Rect::new(20, 10, 40, 30),
                color: PRESETS[0],
                width: 3.0,
                filled: false,
            },
            Annotation::Arrow {
                from: PointF::new(5.0, 5.0),
                to: PointF::new(70.0, 50.0),
                color: PRESETS[3],
                width: 5.0,
            },
            // Straddles the selection edge: cells must match the full-frame render.
            Annotation::Pixelate {
                rect: Rect::new(25, 15, 33, 27),
                block: 7,
            },
            Annotation::Step {
                center: PointF::new(40.0, 30.0),
                number: 3,
                color: PRESETS[2],
                radius: 11.0,
            },
            Annotation::Text {
                pos: PointF::new(24.0, 40.0),
                text: "ab".into(),
                color: PRESETS[4],
                size: 14.0,
                backing: true,
            },
        ];
        let sel = Rect::new(18, 8, 50, 40);
        let flat = flatten(&img, sel, &items, &font);
        let mut full = canvas::image_to_pixmap(img.clone());
        render_annotations(&mut full, &items, (0, 0), &font);
        let expect = canvas::pixmap_to_image(full).crop(sel);
        assert_eq!((flat.width, flat.height), (50, 40));
        let diffs: Vec<_> = (0..flat.data.len() / 4)
            .filter(|i| flat.data[i * 4..i * 4 + 4] != expect.data[i * 4..i * 4 + 4])
            .map(|i| (i % 50, i / 50))
            .collect();
        assert!(
            diffs.is_empty(),
            "differing pixels: {:?}",
            &diffs[..diffs.len().min(20)]
        );
    }

    #[test]
    fn flatten_clips_to_frame_and_handles_empty() {
        let img = solid(10, 10, [1, 2, 3, 255]);
        let f = flatten(&img, Rect::new(5, 5, 100, 100), &[], &test_font::load());
        assert_eq!((f.width, f.height), (5, 5));
        let f = flatten(&img, Rect::new(50, 50, 4, 4), &[], &test_font::load());
        assert_eq!((f.width, f.height), (0, 0));
    }

    #[test]
    fn text_is_drawn_with_the_embedded_font() {
        let items = [Annotation::Text {
            pos: PointF::new(2.0, 2.0),
            text: "Hi".into(),
            color: PRESETS[0],
            size: 20.0,
            backing: false,
        }];
        let mut pm = canvas::image_to_pixmap(solid(60, 40, [255, 255, 255, 255]));
        assert!(pm.data().chunks_exact(4).all(|p| p == [255, 255, 255, 255]));
        render_annotations(&mut pm, &items, (0, 0), &test_font::load());
        assert!(pm.data().chunks_exact(4).any(|p| p != [255, 255, 255, 255]));
    }

    #[test]
    fn bounds_contain_drawn_pixels() {
        let items = vec![
            Annotation::Arrow {
                from: PointF::new(10.0, 10.0),
                to: PointF::new(60.0, 40.0),
                color: PRESETS[0],
                width: 6.0,
            },
            Annotation::Rect {
                rect: Rect::new(8, 8, 30, 20),
                color: PRESETS[0],
                width: 5.0,
                filled: false,
            },
            Annotation::Pen {
                points: vec![
                    PointF::new(10.0, 10.0),
                    PointF::new(50.0, 12.0),
                    PointF::new(30.0, 50.0),
                ],
                color: PRESETS[0],
                width: 8.0,
            },
            Annotation::Highlighter {
                points: vec![PointF::new(15.0, 30.0), PointF::new(70.0, 30.0)],
                color: PRESETS[1],
                width: 18.0,
            },
            Annotation::Step {
                center: PointF::new(40.0, 40.0),
                number: 12,
                color: PRESETS[0],
                radius: 14.0,
            },
        ];
        for it in items {
            let b = annotation_bounds(&it, &test_font::load());
            let mut pm = canvas::image_to_pixmap(solid(100, 80, [255, 255, 255, 255]));
            render_annotations(
                &mut pm,
                std::slice::from_ref(&it),
                (0, 0),
                &test_font::load(),
            );
            for y in 0..80 {
                for x in 0..100 {
                    if px(&pm, x, y) != [255, 255, 255, 255] {
                        assert!(
                            b.contains(kapix_core::Point::new(x as i32, y as i32)),
                            "{it:?} pixel {x},{y} outside {b:?}"
                        );
                    }
                }
            }
        }
    }
}
