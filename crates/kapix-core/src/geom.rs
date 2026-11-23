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
        PointF { x: self.x as f32, y: self.y as f32 }
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
        todo!()
    }
    /// From left/top/right/bottom (right/bottom exclusive). Normalizes if inverted.
    pub fn from_ltrb(l: i32, t: i32, r: i32, b: i32) -> Self {
        todo!()
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
        todo!()
    }
    pub fn contains(&self, p: Point) -> bool {
        todo!()
    }
    pub fn contains_rect(&self, other: &Rect) -> bool {
        todo!()
    }
    /// Intersection; `None` if empty.
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        todo!()
    }
    /// Bounding union. If one side is empty, returns the other.
    pub fn union(&self, other: &Rect) -> Rect {
        todo!()
    }
    pub fn translate(&self, dx: i32, dy: i32) -> Rect {
        todo!()
    }
    /// Grow (or shrink with negative `by`) on all sides.
    pub fn inflate(&self, by: i32) -> Rect {
        todo!()
    }
    /// Move (not resize) the rect so it lies inside `bounds` as much as possible.
    pub fn clamp_inside(&self, bounds: &Rect) -> Rect {
        todo!()
    }
    pub fn center(&self) -> Point {
        todo!()
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
        todo!()
    }
    /// Buffer → virtual coordinates.
    pub fn to_virtual(&self, p: Point) -> Point {
        todo!()
    }
    pub fn rect_to_buffer(&self, r: Rect) -> Rect {
        todo!()
    }
    pub fn rect_to_virtual(&self, r: Rect) -> Rect {
        todo!()
    }
    /// Monitor containing a **virtual** point, or the nearest monitor if none contains it.
    pub fn monitor_at(&self, p: Point) -> Option<&Monitor> {
        todo!()
    }
    /// Monitor containing a **buffer** point (nearest if none). Convenience for the overlay.
    pub fn monitor_at_buffer(&self, p: Point) -> Option<&Monitor> {
        todo!()
    }
}
