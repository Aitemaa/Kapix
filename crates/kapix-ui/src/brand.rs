//! The Kapix mark, rasterized from the same geometry as `assets/icon.svg`.

use kapix_core::{Image, Rgba};
use tiny_skia::{FillRule, LineCap, LineJoin, PathBuilder, Pixmap, Stroke, Transform};

use crate::canvas::{self, paint, rrect_path};

const TILE: Rgba = Rgba::rgb(0x22, 0x22, 0x22);
const BRACKET: Rgba = Rgba::rgb(0x4a, 0x4a, 0x4a);
const K: Rgba = Rgba::rgb(0xff, 0xff, 0xff);
const CROSSHAIR: Rgba = Rgba::rgb(0xe8, 0x26, 0x2d);

/// Render the app icon at `size_px` × `size_px` as **premultiplied BGRA**, transparent outside
/// the rounded tile. At 24 px and below the viewfinder corners are dropped and the strokes get
/// heavier so the K and the red crosshair stay legible.
pub fn render_app_icon(size_px: u32) -> Image {
    let n = size_px.max(1);
    let mut pm = Pixmap::new(n, n).expect("non-empty icon");
    let ts = Transform::from_scale(n as f32 / 1024.0, n as f32 / 1024.0);
    let small = n <= 24;

    if let Some(tile) = rrect_path(32.0, 32.0, 960.0, 960.0, 216.0) {
        pm.fill_path(&tile, &paint(TILE), FillRule::Winding, ts, None);
    }

    let line = |pm: &mut Pixmap, pts: &[(f32, f32)], color: Rgba, width: f32| {
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0].0, pts[0].1);
        for p in &pts[1..] {
            pb.line_to(p.0, p.1);
        }
        if let Some(path) = pb.finish() {
            pm.stroke_path(
                &path,
                &paint(color),
                &Stroke {
                    width,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Stroke::default()
                },
                ts,
                None,
            );
        }
    };

    if !small {
        // Viewfinder corner brackets (quarter-arc corners, radius 50).
        for d in BRACKETS {
            let mut pb = PathBuilder::new();
            pb.move_to(d[0].0, d[0].1);
            pb.line_to(d[1].0, d[1].1);
            // Quarter circle r=50 from d[1] to d[2] about center d[3].
            let (c, r) = (d[3], 50.0f32);
            let (a0, a1) = (
                (d[1].1 - c.1).atan2(d[1].0 - c.0),
                (d[2].1 - c.1).atan2(d[2].0 - c.0),
            );
            let mut da = a1 - a0;
            if da > std::f32::consts::PI {
                da -= std::f32::consts::TAU;
            } else if da < -std::f32::consts::PI {
                da += std::f32::consts::TAU;
            }
            let t = 4.0 / 3.0 * (da / 4.0).tan();
            let (p0, p3) = (d[1], d[2]);
            let t0 = (-(a0.sin()) * r * t, a0.cos() * r * t);
            let t3 = (-(a1.sin()) * r * t, a1.cos() * r * t);
            pb.cubic_to(
                p0.0 + t0.0,
                p0.1 + t0.1,
                p3.0 - t3.0,
                p3.1 - t3.1,
                p3.0,
                p3.1,
            );
            pb.line_to(d[4].0, d[4].1);
            if let Some(path) = pb.finish() {
                pm.stroke_path(
                    &path,
                    &paint(BRACKET),
                    &Stroke {
                        width: 44.0,
                        line_cap: LineCap::Round,
                        line_join: LineJoin::Round,
                        ..Stroke::default()
                    },
                    ts,
                    None,
                );
            }
        }
    }

    let (kw, cw, arm) = if small {
        (92.0, 52.0, 70.0)
    } else {
        (72.0, 32.0, 56.0)
    };
    line(&mut pm, &[(327.4, 268.0), (327.4, 756.0)], K, kw);
    line(&mut pm, &[(497.2, 445.4), (688.6, 268.0)], K, kw);
    line(&mut pm, &[(497.2, 578.6), (688.6, 756.0)], K, kw);
    line(
        &mut pm,
        &[(425.4, 512.0 - arm), (425.4, 512.0 + arm)],
        CROSSHAIR,
        cw,
    );
    line(
        &mut pm,
        &[(425.4 - arm, 512.0), (425.4 + arm, 512.0)],
        CROSSHAIR,
        cw,
    );

    canvas::pixmap_to_image(pm)
}

/// Corner brackets: start, corner-arc start, corner-arc end, arc centre, end.
const BRACKETS: [[(f32, f32); 5]; 4] = [
    [
        (150.0, 290.0),
        (150.0, 200.0),
        (200.0, 150.0),
        (200.0, 200.0),
        (290.0, 150.0),
    ],
    [
        (734.0, 150.0),
        (824.0, 150.0),
        (874.0, 200.0),
        (824.0, 200.0),
        (874.0, 290.0),
    ],
    [
        (874.0, 734.0),
        (874.0, 824.0),
        (824.0, 874.0),
        (824.0, 824.0),
        (734.0, 874.0),
    ],
    [
        (290.0, 874.0),
        (200.0, 874.0),
        (150.0, 824.0),
        (200.0, 824.0),
        (150.0, 734.0),
    ],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_outside_the_tile_and_premultiplied() {
        for n in [16, 24, 32, 48, 256] {
            let img = render_app_icon(n);
            assert_eq!((img.width, img.height), (n, n));
            assert_eq!(img.pixel(0, 0)[3], 0, "{n}px corner is transparent");
            let mid = img.pixel(n / 2, n / 2);
            assert_eq!(mid[3], 255, "{n}px centre is inside the tile");
            for p in img.data.chunks_exact(4) {
                assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
            }
        }
    }

    #[test]
    fn has_a_red_crosshair_and_white_k() {
        let img = render_app_icon(256);
        // Crosshair centre (425.4, 512)/1024*256 = (106, 128): red in BGRA.
        let p = img.pixel(106, 128);
        assert!(p[2] > 200 && p[1] < 80 && p[0] < 80, "{p:?}");
        // K stem (327, 512) -> (82, 128): white.
        let p = img.pixel(82, 128);
        assert!(p[0] > 240 && p[1] > 240 && p[2] > 240, "{p:?}");
    }
}
