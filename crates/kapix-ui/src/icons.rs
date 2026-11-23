//! Lucide-style stroked icons on a 24-unit grid (Kavox's icon language): stroke 2.0 grid units,
//! round caps and joins, drawn from tiny SVG path strings.

use kapix_core::Rgba;
use tiny_skia::{FillRule, LineCap, LineJoin, Path, PathBuilder, Pixmap, Stroke, Transform};

use crate::canvas::{paint, rrect_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    Arrow,
    Square,
    Pencil,
    Highlighter,
    Type,
    Mosaic,
    Step,
    Undo,
    Redo,
    ScanText,
    Pin,
    Download,
    Copy,
    Close,
    Check,
    Info,
    Alert,
}

/// One drawing primitive of an icon (grid coordinates).
enum Part {
    /// Stroked SVG path.
    S(&'static str),
    /// Filled SVG path.
    F(&'static str),
    /// Stroked rounded rect `x, y, w, h, r`.
    R(f32, f32, f32, f32, f32),
    /// Stroked circle `cx, cy, r`.
    C(f32, f32, f32),
}

use Part::{C, F, R, S};

fn parts(icon: Icon) -> &'static [Part] {
    match icon {
        Icon::Arrow => &[S("M7 7h10v10"), S("M7 17 17 7")],
        Icon::Square => &[R(3.0, 3.0, 18.0, 18.0, 2.0)],
        Icon::Pencil => &[
            S(
                "M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z",
            ),
            S("m15 5 4 4"),
        ],
        Icon::Highlighter => &[
            S("m9 11-6 6v3h9l3-3"),
            S("m22 12-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4"),
        ],
        Icon::Type => &[
            S("M12 4v16"),
            S("M4 7V5a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v2"),
            S("M9 20h6"),
        ],
        Icon::Mosaic => &[
            R(3.0, 3.0, 18.0, 18.0, 2.0),
            S("M3 12h18M12 3v18"),
            F("M12 3h7a2 2 0 0 1 2 2v7h-9z"),
            F("M3 12h9v9H5a2 2 0 0 1-2-2z"),
        ],
        Icon::Step => &[C(12.0, 12.0, 10.0), S("M10 9.5 12 8v8")],
        Icon::Undo => &[
            S("M9 14 4 9l5-5"),
            S("M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11"),
        ],
        Icon::Redo => &[
            S("m15 14 5-5-5-5"),
            S("M20 9H9.5A5.5 5.5 0 0 0 4 14.5A5.5 5.5 0 0 0 9.5 20H13"),
        ],
        Icon::ScanText => &[
            S("M3 7V5a2 2 0 0 1 2-2h2"),
            S("M17 3h2a2 2 0 0 1 2 2v2"),
            S("M21 17v2a2 2 0 0 1-2 2h-2"),
            S("M7 21H5a2 2 0 0 1-2-2v-2"),
            S("M7 8h8M7 12h10M7 16h6"),
        ],
        Icon::Pin => &[
            S("M12 17v5"),
            S(
                "M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z",
            ),
        ],
        Icon::Download => &[
            S("M12 15V3"),
            S("M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"),
            S("m7 10 5 5 5-5"),
        ],
        Icon::Copy => &[
            R(9.0, 9.0, 13.0, 13.0, 2.0),
            S("M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"),
        ],
        Icon::Close => &[S("M18 6 6 18M6 6l12 12")],
        Icon::Check => &[S("M20 6 9 17l-5-5")],
        Icon::Info => &[C(12.0, 12.0, 10.0), S("M12 16v-4M12 8h.01")],
        Icon::Alert => &[
            S("m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"),
            S("M12 9v4M12 17h.01"),
        ],
    }
}

/// Draw `icon` centred on `(cx, cy)` in a `size`-px square (24 grid units). `filled` adds a solid
/// body to icons that have one (the rectangle tool's fill toggle).
pub(crate) fn draw(
    pm: &mut Pixmap,
    icon: Icon,
    cx: f32,
    cy: f32,
    size: f32,
    color: Rgba,
    filled: bool,
) {
    let k = size / 24.0;
    let ts = Transform::from_translate(cx - 12.0 * k, cy - 12.0 * k).pre_scale(k, k);
    let p = paint(color);
    let stroke = Stroke {
        width: 2.0,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    for part in parts(icon) {
        match part {
            S(d) => {
                if let Some(path) = svg_path(d) {
                    pm.stroke_path(&path, &p, &stroke, ts, None);
                }
            }
            F(d) => {
                if let Some(path) = svg_path(d) {
                    pm.fill_path(&path, &p, FillRule::Winding, ts, None);
                }
            }
            R(x, y, w, h, r) => {
                if let Some(path) = rrect_path(*x, *y, *w, *h, *r) {
                    if filled && icon == Icon::Square {
                        pm.fill_path(&path, &p, FillRule::Winding, ts, None);
                    }
                    pm.stroke_path(&path, &p, &stroke, ts, None);
                }
            }
            C(x, y, r) => {
                if let Some(path) = PathBuilder::from_circle(*x, *y, *r) {
                    pm.stroke_path(&path, &p, &stroke, ts, None);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Minimal SVG path parser (M L H V C S Q A Z, absolute and relative)
// ---------------------------------------------------------------------------------------------

struct Scanner<'a> {
    b: &'a [u8],
    i: usize,
}

impl Scanner<'_> {
    fn skip_sep(&mut self) {
        while self.i < self.b.len() && (self.b[self.i] == b' ' || self.b[self.i] == b',') {
            self.i += 1;
        }
    }

    fn peek_cmd(&mut self) -> Option<u8> {
        self.skip_sep();
        self.b.get(self.i).copied().filter(u8::is_ascii_alphabetic)
    }

    fn has_number(&mut self) -> bool {
        self.skip_sep();
        self.b
            .get(self.i)
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.'))
    }

    fn num(&mut self) -> Option<f32> {
        self.skip_sep();
        let start = self.i;
        let mut seen_dot = false;
        let mut seen_exp = false;
        while let Some(&c) = self.b.get(self.i) {
            match c {
                b'0'..=b'9' => {}
                b'-' | b'+' if self.i == start => {}
                b'-' | b'+' if seen_exp && matches!(self.b[self.i - 1], b'e' | b'E') => {}
                b'.' if !seen_dot && !seen_exp => seen_dot = true,
                b'e' | b'E' if !seen_exp && self.i > start => seen_exp = true,
                _ => break,
            }
            self.i += 1;
        }
        std::str::from_utf8(&self.b[start..self.i])
            .ok()?
            .parse()
            .ok()
    }

    fn flag(&mut self) -> Option<bool> {
        self.skip_sep();
        let c = *self.b.get(self.i)?;
        self.i += 1;
        Some(c == b'1')
    }
}

fn svg_path(d: &str) -> Option<Path> {
    let mut sc = Scanner {
        b: d.as_bytes(),
        i: 0,
    };
    let mut pb = PathBuilder::new();
    let (mut cur, mut start) = ((0f32, 0f32), (0f32, 0f32));
    let mut last_c: Option<(f32, f32)> = None;
    let mut cmd = b'M';
    loop {
        if let Some(c) = sc.peek_cmd() {
            sc.i += 1;
            cmd = c;
        } else if !sc.has_number() {
            break;
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { cur } else { (0.0, 0.0) };
        let mut next_c = None;
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (x, y) = (sc.num()? + ox, sc.num()? + oy);
                pb.move_to(x, y);
                cur = (x, y);
                start = cur;
                cmd = if rel { b'l' } else { b'L' };
            }
            b'L' => {
                let (x, y) = (sc.num()? + ox, sc.num()? + oy);
                pb.line_to(x, y);
                cur = (x, y);
            }
            b'H' => {
                let x = sc.num()? + ox;
                pb.line_to(x, cur.1);
                cur.0 = x;
            }
            b'V' => {
                let y = sc.num()? + oy;
                pb.line_to(cur.0, y);
                cur.1 = y;
            }
            b'C' => {
                let v: Vec<f32> = (0..6).map(|_| sc.num()).collect::<Option<_>>()?;
                let (c1, c2, p) = (
                    (v[0] + ox, v[1] + oy),
                    (v[2] + ox, v[3] + oy),
                    (v[4] + ox, v[5] + oy),
                );
                pb.cubic_to(c1.0, c1.1, c2.0, c2.1, p.0, p.1);
                next_c = Some(c2);
                cur = p;
            }
            b'S' => {
                let v: Vec<f32> = (0..4).map(|_| sc.num()).collect::<Option<_>>()?;
                let c1 = last_c.map_or(cur, |l| (2.0 * cur.0 - l.0, 2.0 * cur.1 - l.1));
                let (c2, p) = ((v[0] + ox, v[1] + oy), (v[2] + ox, v[3] + oy));
                pb.cubic_to(c1.0, c1.1, c2.0, c2.1, p.0, p.1);
                next_c = Some(c2);
                cur = p;
            }
            b'Q' => {
                let v: Vec<f32> = (0..4).map(|_| sc.num()).collect::<Option<_>>()?;
                let (c, p) = ((v[0] + ox, v[1] + oy), (v[2] + ox, v[3] + oy));
                pb.quad_to(c.0, c.1, p.0, p.1);
                cur = p;
            }
            b'A' => {
                let (rx, ry, rot) = (sc.num()?, sc.num()?, sc.num()?);
                let (large, sweep) = (sc.flag()?, sc.flag()?);
                let p = (sc.num()? + ox, sc.num()? + oy);
                arc(&mut pb, cur, rx, ry, rot, large, sweep, p);
                cur = p;
            }
            b'Z' => {
                pb.close();
                cur = start;
            }
            _ => return None,
        }
        last_c = next_c;
    }
    pb.finish()
}

/// SVG elliptical arc → cubic Béziers (endpoint → centre parameterisation).
#[allow(clippy::too_many_arguments)]
fn arc(
    pb: &mut PathBuilder,
    from: (f32, f32),
    rx: f32,
    ry: f32,
    rot_deg: f32,
    large: bool,
    sweep: bool,
    to: (f32, f32),
) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if from == to {
        return;
    }
    if rx == 0.0 || ry == 0.0 {
        pb.line_to(to.0, to.1);
        return;
    }
    let phi = rot_deg.to_radians();
    let (sp, cp) = phi.sin_cos();
    let (dx, dy) = ((from.0 - to.0) / 2.0, (from.1 - to.1) / 2.0);
    let (x1, y1) = (cp * dx + sp * dy, -sp * dx + cp * dy);
    let lam = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lam > 1.0 {
        let s = lam.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = (num / den).max(0.0).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let (cxp, cyp) = (coef * rx * y1 / ry, -coef * ry * x1 / rx);
    let cx = cp * cxp - sp * cyp + (from.0 + to.0) / 2.0;
    let cy = sp * cxp + cp * cyp + (from.1 + to.1) / 2.0;
    let ang = |ux: f32, uy: f32, vx: f32, vy: f32| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let th1 = ang(1.0, 0.0, (x1 - cxp) / rx, (y1 - cyp) / ry);
    let mut dth = ang(
        (x1 - cxp) / rx,
        (y1 - cyp) / ry,
        (-x1 - cxp) / rx,
        (-y1 - cyp) / ry,
    );
    if !sweep && dth > 0.0 {
        dth -= std::f32::consts::TAU;
    } else if sweep && dth < 0.0 {
        dth += std::f32::consts::TAU;
    }
    let n = (dth.abs() / (std::f32::consts::FRAC_PI_2 + 0.001))
        .ceil()
        .max(1.0) as usize;
    let step = dth / n as f32;
    let t = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |th: f32| {
        let (s, c) = th.sin_cos();
        (
            cx + rx * c * cp - ry * s * sp,
            cy + rx * c * sp + ry * s * cp,
        )
    };
    let deriv = |th: f32| {
        let (s, c) = th.sin_cos();
        (-rx * s * cp - ry * c * sp, -rx * s * sp + ry * c * cp)
    };
    let mut a = th1;
    for _ in 0..n {
        let b = a + step;
        let (p0, p3) = (point(a), point(b));
        let (d0, d3) = (deriv(a), deriv(b));
        pb.cubic_to(
            p0.0 + t * d0.0,
            p0.1 + t * d0.1,
            p3.0 - t * d3.0,
            p3.1 - t * d3.1,
            p3.0,
            p3.1,
        );
        a = b;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_path_parses() {
        for icon in [
            Icon::Arrow,
            Icon::Square,
            Icon::Pencil,
            Icon::Highlighter,
            Icon::Type,
            Icon::Mosaic,
            Icon::Step,
            Icon::Undo,
            Icon::Redo,
            Icon::ScanText,
            Icon::Pin,
            Icon::Download,
            Icon::Copy,
            Icon::Close,
            Icon::Check,
            Icon::Info,
            Icon::Alert,
        ] {
            let mut pm = Pixmap::new(24, 24).unwrap();
            for part in parts(icon) {
                if let S(d) | F(d) = part {
                    assert!(svg_path(d).is_some(), "{icon:?}: {d}");
                }
            }
            draw(
                &mut pm,
                icon,
                12.0,
                12.0,
                24.0,
                Rgba::rgb(255, 255, 255),
                false,
            );
            assert!(
                pm.data().chunks_exact(4).any(|p| p[3] > 200),
                "{icon:?} draws"
            );
        }
    }

    #[test]
    fn arcs_stay_on_the_circle() {
        // The "circle" path used by lucide: two half arcs.
        let p = svg_path("M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20").unwrap();
        let b = p.bounds();
        assert!(
            (b.left() - 2.0).abs() < 0.2 && (b.right() - 22.0).abs() < 0.2,
            "{b:?}"
        );
        assert!(
            (b.top() - 2.0).abs() < 0.2 && (b.bottom() - 22.0).abs() < 0.2,
            "{b:?}"
        );
    }
}
