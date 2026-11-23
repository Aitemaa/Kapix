//! Vector annotation model and undo/redo stack. Coordinates are **buffer** coordinates (f32),
//! so annotations stay attached to screen content if the selection is moved/resized.

use crate::color::Rgba;
use crate::geom::{PointF, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Arrow,
    Rect,
    Pen,
    Highlighter,
    Text,
    Pixelate,
    Step,
}

impl Tool {
    pub const ALL: [Tool; 7] = [
        Tool::Arrow,
        Tool::Rect,
        Tool::Pen,
        Tool::Highlighter,
        Tool::Text,
        Tool::Pixelate,
        Tool::Step,
    ];

    /// Keyboard shortcut letter (uppercase): A R P H T B N.
    pub fn key(self) -> char {
        match self {
            Tool::Arrow => 'A',
            Tool::Rect => 'R',
            Tool::Pen => 'P',
            Tool::Highlighter => 'H',
            Tool::Text => 'T',
            Tool::Pixelate => 'B',
            Tool::Step => 'N',
        }
    }
    /// Inverse of [`Tool::key`], case-insensitive.
    pub fn from_key(c: char) -> Option<Tool> {
        Tool::ALL
            .into_iter()
            .find(|t| t.key() == c.to_ascii_uppercase())
    }
    pub fn label(self) -> &'static str {
        match self {
            Tool::Arrow => "Arrow",
            Tool::Rect => "Rectangle",
            Tool::Pen => "Pen",
            Tool::Highlighter => "Highlighter",
            Tool::Text => "Text",
            Tool::Pixelate => "Pixelate",
            Tool::Step => "Step",
        }
    }
    /// Default size for the tool: stroke width (px) for strokes, font px for Text,
    /// block size for Pixelate, badge radius for Step.
    pub fn default_size(self) -> f32 {
        match self {
            Tool::Arrow => 4.0,
            Tool::Rect => 3.0,
            Tool::Pen => 3.0,
            Tool::Highlighter => 18.0,
            Tool::Text => 18.0,
            Tool::Pixelate => 12.0,
            Tool::Step => 14.0,
        }
    }
    /// Allowed (min, max) size range for scroll-wheel adjustment.
    pub fn size_range(self) -> (f32, f32) {
        match self {
            Tool::Arrow | Tool::Rect | Tool::Pen => (1.0, 24.0),
            Tool::Highlighter => (6.0, 60.0),
            Tool::Text => (10.0, 96.0),
            Tool::Pixelate => (4.0, 48.0),
            Tool::Step => (8.0, 40.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    Arrow {
        from: PointF,
        to: PointF,
        color: Rgba,
        width: f32,
    },
    Rect {
        rect: Rect,
        color: Rgba,
        width: f32,
        filled: bool,
    },
    Pen {
        points: Vec<PointF>,
        color: Rgba,
        width: f32,
    },
    /// Rendered with the color at reduced alpha (~40%) and multiply-like blending.
    Highlighter {
        points: Vec<PointF>,
        color: Rgba,
        width: f32,
    },
    /// `pos` is the top-left of the first line. `backing` draws a contrasting pill behind it.
    Text {
        pos: PointF,
        text: String,
        color: Rgba,
        size: f32,
        backing: bool,
    },
    /// Destructive mosaic over `rect` with `block`-px cells.
    Pixelate { rect: Rect, block: u32 },
    /// Numbered circular badge.
    Step {
        center: PointF,
        number: u32,
        color: Rgba,
        radius: f32,
    },
}

impl Annotation {
    /// Conservative bounding box in buffer coordinates (including stroke width / arrowhead /
    /// estimated text extent) — used for dirty-rect repainting.
    pub fn bounds(&self) -> Rect {
        match self {
            Annotation::Arrow {
                from, to, width, ..
            } => {
                let pad = width / 2.0 + (width * 3.0).max(10.0) + 1.0;
                pts_bounds(&[*from, *to], pad)
            }
            Annotation::Rect { rect, width, .. } => rect.inflate((width / 2.0).ceil() as i32 + 1),
            Annotation::Pen { points, width, .. }
            | Annotation::Highlighter { points, width, .. } => {
                pts_bounds(points, width / 2.0 + 1.0)
            }
            Annotation::Text {
                pos,
                text,
                size,
                backing,
                ..
            } => {
                let lines = text.split('\n').count().max(1) as f32;
                let cols = text
                    .split('\n')
                    .map(|l| l.chars().count())
                    .max()
                    .unwrap_or(0) as f32;
                let pad = if *backing { 8.0 } else { 3.0 };
                let w = cols * size * 0.6 + 2.0 * pad;
                let h = lines * size * 1.3 + 2.0 * pad;
                Rect::from_ltrb(
                    (pos.x - pad).floor() as i32,
                    (pos.y - pad).floor() as i32,
                    (pos.x - pad + w).ceil() as i32,
                    (pos.y - pad + h).ceil() as i32,
                )
            }
            Annotation::Pixelate { rect, .. } => *rect,
            Annotation::Step { center, radius, .. } => pts_bounds(&[*center], radius + 2.0),
        }
    }
}

/// Ordered command stack. Only "add" commands exist; undo pops into the redo stack.
#[derive(Debug, Clone, Default)]
pub struct Document {
    items: Vec<Annotation>,
    redo: Vec<Annotation>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn items(&self) -> &[Annotation] {
        &self.items
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    /// Push a new annotation; clears the redo stack.
    pub fn push(&mut self, a: Annotation) {
        self.items.push(a);
        self.redo.clear();
    }
    /// Returns the undone annotation, if any.
    pub fn undo(&mut self) -> Option<&Annotation> {
        let a = self.items.pop()?;
        self.redo.push(a);
        self.redo.last()
    }
    /// Returns the redone annotation, if any.
    pub fn redo(&mut self) -> Option<&Annotation> {
        let a = self.redo.pop()?;
        self.items.push(a);
        self.items.last()
    }
    pub fn can_undo(&self) -> bool {
        !self.items.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// Next step-counter number: 1 + max existing Step number (1 if none).
    pub fn next_step_number(&self) -> u32 {
        self.items
            .iter()
            .filter_map(|a| match a {
                Annotation::Step { number, .. } => Some(*number),
                _ => None,
            })
            .max()
            .map_or(1, |n| n + 1)
    }
    pub fn clear(&mut self) {
        self.items.clear();
        self.redo.clear();
    }
}

/// Simplify a freehand polyline (Ramer–Douglas–Peucker) with tolerance `epsilon` px.
/// Always keeps first and last points.
pub fn simplify_path(points: &[PointF], epsilon: f32) -> Vec<PointF> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let mut worst = (0.0f32, a);
        for (i, p) in points.iter().enumerate().take(b).skip(a + 1) {
            let d = seg_dist(*p, points[a], points[b]);
            if d > worst.0 {
                worst = (d, i);
            }
        }
        if worst.0 > epsilon {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}

/// Snap `to` so the segment `from→to` lies on the nearest 45° increment (Shift modifier).
pub fn snap_45(from: PointF, to: PointF) -> PointF {
    use std::f32::consts::{FRAC_1_SQRT_2 as S, FRAC_PI_4};
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = dx.hypot(dy);
    if len == 0.0 {
        return to;
    }
    const DIRS: [(f32, f32); 8] = [
        (1.0, 0.0),
        (S, S),
        (0.0, 1.0),
        (-S, S),
        (-1.0, 0.0),
        (-S, -S),
        (0.0, -1.0),
        (S, -S),
    ];
    let k = ((dy.atan2(dx) / FRAC_PI_4).round() as i32).rem_euclid(8) as usize;
    PointF::new(from.x + DIRS[k].0 * len, from.y + DIRS[k].1 * len)
}

fn pts_bounds(points: &[PointF], pad: f32) -> Rect {
    let Some(first) = points.first() else {
        return Rect::default();
    };
    let (mut l, mut t, mut r, mut b) = (first.x, first.y, first.x, first.y);
    for p in points {
        l = l.min(p.x);
        t = t.min(p.y);
        r = r.max(p.x);
        b = b.max(p.y);
    }
    Rect::from_ltrb(
        (l - pad).floor() as i32,
        (t - pad).floor() as i32,
        (r + pad).ceil() as i32,
        (b + pad).ceil() as i32,
    )
}

/// Distance from `p` to the segment `a`–`b`.
fn seg_dist(p: PointF, a: PointF, b: PointF) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0)
    };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgba = Rgba::rgb(255, 0, 0);

    fn step(n: u32) -> Annotation {
        Annotation::Step {
            center: PointF::new(10.0, 10.0),
            number: n,
            color: RED,
            radius: 14.0,
        }
    }

    fn pxl(r: Rect) -> Annotation {
        Annotation::Pixelate { rect: r, block: 8 }
    }

    #[test]
    fn tool_key_round_trip() {
        let keys: String = Tool::ALL.iter().map(|t| t.key()).collect();
        assert_eq!(keys, "ARPHTBN");
        for t in Tool::ALL {
            assert_eq!(Tool::from_key(t.key()), Some(t));
            assert_eq!(Tool::from_key(t.key().to_ascii_lowercase()), Some(t));
            let (lo, hi) = t.size_range();
            assert!(lo <= t.default_size() && t.default_size() <= hi);
            assert!(!t.label().is_empty());
        }
        assert_eq!(Tool::from_key('x'), None);
        assert_eq!(Tool::from_key('1'), None);
    }

    #[test]
    fn undo_redo_semantics() {
        let mut d = Document::new();
        assert!(d.is_empty() && !d.can_undo() && !d.can_redo());
        assert!(d.undo().is_none() && d.redo().is_none());
        d.push(step(1));
        d.push(step(2));
        assert_eq!(d.items().len(), 2);
        assert_eq!(d.undo(), Some(&step(2)));
        assert!(d.can_redo());
        assert_eq!(d.items().len(), 1);
        assert_eq!(d.redo(), Some(&step(2)));
        assert!(!d.can_redo());
        d.undo();
        d.undo();
        assert!(d.is_empty() && d.can_redo());
        assert_eq!(d.redo(), Some(&step(1)));
        d.undo();
        // push clears redo
        d.push(pxl(Rect::new(0, 0, 5, 5)));
        assert!(!d.can_redo());
        assert!(d.redo().is_none());
        d.undo();
        d.clear();
        assert!(!d.can_redo() && d.is_empty());
    }

    #[test]
    fn next_step_number_uses_max() {
        let mut d = Document::new();
        assert_eq!(d.next_step_number(), 1);
        d.push(pxl(Rect::new(0, 0, 5, 5)));
        assert_eq!(d.next_step_number(), 1);
        d.push(step(1));
        d.push(step(5));
        d.push(step(3));
        assert_eq!(d.next_step_number(), 6);
        d.undo();
        assert_eq!(d.next_step_number(), 6);
        d.undo();
        assert_eq!(d.next_step_number(), 2);
    }

    #[test]
    fn simplify_keeps_endpoints_drops_collinear() {
        let p = |x: f32, y: f32| PointF::new(x, y);
        let line = [
            p(0.0, 0.0),
            p(1.0, 1.0),
            p(2.0, 2.0),
            p(3.0, 3.0),
            p(10.0, 10.0),
        ];
        assert_eq!(simplify_path(&line, 0.5), vec![p(0.0, 0.0), p(10.0, 10.0)]);
        let bent = [
            p(0.0, 0.0),
            p(5.0, 0.2),
            p(10.0, 0.0),
            p(10.0, 5.0),
            p(10.0, 10.0),
        ];
        let s = simplify_path(&bent, 1.0);
        assert_eq!(s, vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)]);
        let s = simplify_path(&bent, 0.0);
        assert_eq!(
            s.len(),
            4,
            "exactly collinear point still dropped at epsilon 0"
        );
        assert_eq!(simplify_path(&[], 1.0), vec![]);
        assert_eq!(simplify_path(&[p(1.0, 1.0)], 1.0), vec![p(1.0, 1.0)]);
        let two = [p(0.0, 0.0), p(0.0, 0.0)];
        assert_eq!(simplify_path(&two, 5.0), two.to_vec());
        // closed loop (first == last) still keeps both ends and the far point
        let lp = [p(0.0, 0.0), p(5.0, 0.0), p(5.0, 5.0), p(0.0, 0.0)];
        assert_eq!(simplify_path(&lp, 1.0).len(), 4);
    }

    fn close(a: PointF, x: f32, y: f32) {
        assert!(
            (a.x - x).abs() < 1e-3 && (a.y - y).abs() < 1e-3,
            "{a:?} != ({x},{y})"
        );
    }

    #[test]
    fn snap_45_angles() {
        let o = PointF::new(100.0, 100.0);
        close(
            snap_45(o, PointF::new(150.0, 105.0)),
            100.0 + 50.0f32.hypot(5.0),
            100.0,
        );
        close(
            snap_45(o, PointF::new(105.0, 150.0)),
            100.0,
            100.0 + 50.0f32.hypot(5.0),
        );
        let d = 50.0f32.hypot(40.0);
        let s = std::f32::consts::FRAC_1_SQRT_2;
        close(
            snap_45(o, PointF::new(150.0, 140.0)),
            100.0 + d * s,
            100.0 + d * s,
        );
        close(
            snap_45(o, PointF::new(60.0, 60.0)),
            100.0 - 40.0 * 1.0,
            100.0 - 40.0,
        );
        close(snap_45(o, PointF::new(50.0, 100.0)), 50.0, 100.0);
        close(snap_45(o, PointF::new(100.0, 50.0)), 100.0, 50.0);
        close(
            snap_45(o, PointF::new(160.0, 40.0)),
            100.0 + 60.0,
            100.0 - 60.0,
        );
        close(
            snap_45(o, PointF::new(40.0, 160.0)),
            100.0 - 60.0,
            100.0 + 60.0,
        );
        close(snap_45(o, o), 100.0, 100.0);
        // 22° rounds to 0°, 23° rounds to 45° (length preserved)
        let a = 22.0f32.to_radians();
        close(
            snap_45(
                PointF::new(0.0, 0.0),
                PointF::new(10.0 * a.cos(), 10.0 * a.sin()),
            ),
            10.0,
            0.0,
        );
        let a = 23.0f32.to_radians();
        let r = snap_45(
            PointF::new(0.0, 0.0),
            PointF::new(10.0 * a.cos(), 10.0 * a.sin()),
        );
        close(r, 10.0 * s, 10.0 * s);
    }

    #[test]
    fn bounds_cover_geometry() {
        let a = Annotation::Arrow {
            from: PointF::new(10.0, 10.0),
            to: PointF::new(50.0, 30.0),
            color: RED,
            width: 4.0,
        };
        let b = a.bounds();
        assert!(b.contains_rect(&Rect::new(10, 10, 40, 20)));
        assert!(
            b.x <= 10 - 10 && b.right() >= 50 + 10,
            "room for arrowhead: {b:?}"
        );
        let r = Annotation::Rect {
            rect: Rect::new(5, 5, 10, 10),
            color: RED,
            width: 6.0,
            filled: false,
        };
        assert!(r.bounds().contains_rect(&Rect::new(2, 2, 16, 16)));
        let pen = Annotation::Pen {
            points: vec![PointF::new(0.0, 0.0), PointF::new(20.5, 9.2)],
            color: RED,
            width: 5.0,
        };
        assert!(pen.bounds().contains_rect(&Rect::new(-2, -2, 25, 14)));
        let empty = Annotation::Highlighter {
            points: vec![],
            color: RED,
            width: 5.0,
        };
        assert!(empty.bounds().is_empty());
        let t = Annotation::Text {
            pos: PointF::new(100.0, 100.0),
            text: "hello\nhi".into(),
            color: RED,
            size: 20.0,
            backing: true,
        };
        let tb = t.bounds();
        assert!(tb.contains_rect(&Rect::new(100, 100, 60, 52)), "{tb:?}");
        assert_eq!(pxl(Rect::new(1, 2, 3, 4)).bounds(), Rect::new(1, 2, 3, 4));
        let s = step(1).bounds();
        assert!(s.contains_rect(&Rect::new(-4, -4, 28, 28)));
    }
}
