//! Integer/float geometry in physical pixels.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PointF {
    pub x: f32,
    pub y: f32,
}

/// Axis-aligned rectangle. `w`/`h` are always >= 0 for rects produced by this module.
/// Right/bottom edges are exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
    pub fn to_f(self) -> PointF {
        PointF {
            x: self.x as f32,
            y: self.y as f32,
        }
    }
}

impl PointF {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    /// From two corner points in any order (normalized, inclusive-of-both-points semantics are
    /// NOT used: result spans `min..max`, so identical points give an empty rect).
    pub fn from_points(a: Point, b: Point) -> Self {
        Self::from_ltrb(a.x, a.y, b.x, b.y)
    }
    /// From left/top/right/bottom (right/bottom exclusive). Normalizes if inverted.
    pub fn from_ltrb(l: i32, t: i32, r: i32, b: i32) -> Self {
        Self {
            x: l.min(r),
            y: t.min(b),
            w: (r - l).abs(),
            h: (b - t).abs(),
        }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
    pub fn area(&self) -> i64 {
        if self.is_empty() {
            0
        } else {
            self.w as i64 * self.h as i64
        }
    }
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }
    pub fn contains_rect(&self, other: &Rect) -> bool {
        !other.is_empty()
            && other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }
    /// Intersection; `None` if empty.
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let l = self.x.max(other.x);
        let t = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        if r > l && b > t && !self.is_empty() && !other.is_empty() {
            Some(Rect::new(l, t, r - l, b - t))
        } else {
            None
        }
    }
    /// Bounding union. If one side is empty, returns the other.
    pub fn union(&self, other: &Rect) -> Rect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Rect::from_ltrb(
            self.x.min(other.x),
            self.y.min(other.y),
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        )
    }
    pub fn translate(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
    /// Grow (or shrink with negative `by`) on all sides.
    pub fn inflate(&self, by: i32) -> Rect {
        Rect::new(
            self.x - by,
            self.y - by,
            (self.w + 2 * by).max(0),
            (self.h + 2 * by).max(0),
        )
    }
    /// Move (not resize) the rect so it lies inside `bounds` as much as possible.
    pub fn clamp_inside(&self, bounds: &Rect) -> Rect {
        let max_x = (bounds.right() - self.w).max(bounds.x);
        let max_y = (bounds.bottom() - self.h).max(bounds.y);
        Rect::new(
            self.x.clamp(bounds.x, max_x),
            self.y.clamp(bounds.y, max_y),
            self.w,
            self.h,
        )
    }
    pub fn center(&self) -> Point {
        Point::new(self.x + self.w / 2, self.y + self.h / 2)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    /// Full monitor rect in virtual coordinates.
    pub rect: Rect,
    /// Work area (excluding taskbar) in virtual coordinates.
    pub work: Rect,
    /// DPI scale factor (effective DPI / 96.0), e.g. 1.5 for 150%.
    pub scale: f32,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VirtualDesktop {
    /// Bounding rect of all monitors in virtual coordinates (SM_XVIRTUALSCREEN...).
    pub bounds: Rect,
    pub monitors: Vec<Monitor>,
}

impl VirtualDesktop {
    /// Virtual → buffer coordinates.
    pub fn to_buffer(&self, p: Point) -> Point {
        Point::new(p.x - self.bounds.x, p.y - self.bounds.y)
    }
    /// Buffer → virtual coordinates.
    pub fn to_virtual(&self, p: Point) -> Point {
        Point::new(p.x + self.bounds.x, p.y + self.bounds.y)
    }
    pub fn rect_to_buffer(&self, r: Rect) -> Rect {
        r.translate(-self.bounds.x, -self.bounds.y)
    }
    pub fn rect_to_virtual(&self, r: Rect) -> Rect {
        r.translate(self.bounds.x, self.bounds.y)
    }
    /// Monitor containing a **virtual** point, or the nearest monitor if none contains it.
    pub fn monitor_at(&self, p: Point) -> Option<&Monitor> {
        if let Some(m) = self.monitors.iter().find(|m| m.rect.contains(p)) {
            return Some(m);
        }
        self.monitors.iter().min_by_key(|m| dist2(&m.rect, p))
    }
    /// Monitor containing a **buffer** point (nearest if none). Convenience for the overlay.
    pub fn monitor_at_buffer(&self, p: Point) -> Option<&Monitor> {
        self.monitor_at(self.to_virtual(p))
    }
}

/// Squared distance from a point to the nearest point of `r` (0 if inside).
fn dist2(r: &Rect, p: Point) -> i64 {
    let dx = (r.x - p.x).max(0).max(p.x - (r.right() - 1)) as i64;
    let dy = (r.y - p.y).max(0).max(p.y - (r.bottom() - 1)) as i64;
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desktop() -> VirtualDesktop {
        let left = Monitor {
            rect: Rect::new(-1920, 0, 1920, 1080),
            work: Rect::new(-1920, 0, 1920, 1040),
            scale: 1.0,
            primary: false,
        };
        let main = Monitor {
            rect: Rect::new(0, 0, 2560, 1440),
            work: Rect::new(0, 0, 2560, 1400),
            scale: 1.5,
            primary: true,
        };
        VirtualDesktop {
            bounds: Rect::new(-1920, 0, 4480, 1440),
            monitors: vec![left, main],
        }
    }

    #[test]
    fn from_points_normalizes() {
        let r = Rect::from_points(Point::new(10, 20), Point::new(4, 5));
        assert_eq!(r, Rect::new(4, 5, 6, 15));
        assert_eq!(
            Rect::from_points(Point::new(3, 3), Point::new(3, 3)).area(),
            0
        );
        assert!(Rect::from_points(Point::new(3, 3), Point::new(3, 3)).is_empty());
        assert_eq!(Rect::from_ltrb(10, 10, 0, 0), Rect::new(0, 0, 10, 10));
    }

    #[test]
    fn intersect_and_union() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        assert_eq!(a.intersect(&b), Some(Rect::new(5, 5, 5, 5)));
        assert_eq!(a.union(&b), Rect::new(0, 0, 15, 15));
        assert_eq!(
            a.intersect(&Rect::new(10, 0, 5, 5)),
            None,
            "touching edges do not intersect"
        );
        assert_eq!(a.intersect(&Rect::new(2, 2, 0, 5)), None);
        let empty = Rect::new(100, 100, 0, 0);
        assert_eq!(a.union(&empty), a);
        assert_eq!(empty.union(&a), a);
        // negative coordinates
        let n = Rect::new(-1920, 0, 1920, 1080);
        assert_eq!(
            n.intersect(&Rect::new(-100, -50, 200, 100)),
            Some(Rect::new(-100, 0, 100, 50))
        );
        assert_eq!(n.union(&a), Rect::new(-1920, 0, 1930, 1080));
    }

    #[test]
    fn contains_translate_inflate_center() {
        let r = Rect::new(-10, -10, 20, 20);
        assert!(r.contains(Point::new(-10, -10)));
        assert!(!r.contains(Point::new(10, 0)));
        assert!(r.contains_rect(&Rect::new(-5, -5, 10, 10)));
        assert!(!r.contains_rect(&Rect::new(-5, -5, 20, 10)));
        assert_eq!(r.translate(5, -5), Rect::new(-5, -15, 20, 20));
        assert_eq!(r.inflate(2), Rect::new(-12, -12, 24, 24));
        assert_eq!(r.inflate(-20).w, 0);
        assert_eq!(r.center(), Point::new(0, 0));
        assert_eq!(r.area(), 400);
    }

    #[test]
    fn clamp_inside_moves_not_resizes() {
        let b = Rect::new(-1920, 0, 1920, 1080);
        assert_eq!(
            Rect::new(-2000, -10, 100, 50).clamp_inside(&b),
            Rect::new(-1920, 0, 100, 50)
        );
        assert_eq!(
            Rect::new(-50, 1070, 100, 50).clamp_inside(&b),
            Rect::new(-100, 1030, 100, 50)
        );
        assert_eq!(
            Rect::new(-500, 500, 100, 50).clamp_inside(&b),
            Rect::new(-500, 500, 100, 50)
        );
        // larger than bounds: pinned to top-left
        assert_eq!(
            Rect::new(5, 5, 3000, 3000).clamp_inside(&b),
            Rect::new(-1920, 0, 3000, 3000)
        );
    }

    #[test]
    fn buffer_virtual_round_trip() {
        let d = desktop();
        let v = Point::new(-1920, 0);
        assert_eq!(d.to_buffer(v), Point::new(0, 0));
        assert_eq!(d.to_buffer(Point::new(5, 7)), Point::new(1925, 7));
        for p in [
            Point::new(-1000, 300),
            Point::new(0, 0),
            Point::new(2559, 1439),
        ] {
            assert_eq!(d.to_virtual(d.to_buffer(p)), p);
        }
        let r = Rect::new(-100, 10, 300, 40);
        assert_eq!(d.rect_to_buffer(r), Rect::new(1820, 10, 300, 40));
        assert_eq!(d.rect_to_virtual(d.rect_to_buffer(r)), r);
    }

    #[test]
    fn monitor_lookup() {
        let d = desktop();
        assert!(!d.monitor_at(Point::new(-1, 10)).unwrap().primary);
        assert!(d.monitor_at(Point::new(0, 10)).unwrap().primary);
        // outside everything: nearest
        assert!(!d.monitor_at(Point::new(-5000, 500)).unwrap().primary);
        assert!(d.monitor_at(Point::new(9000, 500)).unwrap().primary);
        assert!(!d.monitor_at(Point::new(-300, 1200)).unwrap().primary);
        // buffer variant: buffer x=10 is virtual -1910
        assert!(!d.monitor_at_buffer(Point::new(10, 10)).unwrap().primary);
        assert!(d.monitor_at_buffer(Point::new(1920, 10)).unwrap().primary);
        let empty = VirtualDesktop {
            bounds: Rect::default(),
            monitors: vec![],
        };
        assert!(empty.monitor_at(Point::new(0, 0)).is_none());
    }
}
