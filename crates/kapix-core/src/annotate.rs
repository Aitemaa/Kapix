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
    pub const ALL: [Tool; 7] =
        [Tool::Arrow, Tool::Rect, Tool::Pen, Tool::Highlighter, Tool::Text, Tool::Pixelate, Tool::Step];

    /// Keyboard shortcut letter (uppercase): A R P H T B N.
    pub fn key(self) -> char {
        todo!()
    }
    /// Inverse of [`Tool::key`], case-insensitive.
    pub fn from_key(c: char) -> Option<Tool> {
        todo!()
    }
    pub fn label(self) -> &'static str {
        todo!()
    }
    /// Default size for the tool: stroke width (px) for strokes, font px for Text,
    /// block size for Pixelate, badge radius for Step.
    pub fn default_size(self) -> f32 {
        todo!()
    }
    /// Allowed (min, max) size range for scroll-wheel adjustment.
    pub fn size_range(self) -> (f32, f32) {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    Arrow { from: PointF, to: PointF, color: Rgba, width: f32 },
    Rect { rect: Rect, color: Rgba, width: f32, filled: bool },
    Pen { points: Vec<PointF>, color: Rgba, width: f32 },
    /// Rendered with the color at reduced alpha (~40%) and multiply-like blending.
    Highlighter { points: Vec<PointF>, color: Rgba, width: f32 },
    /// `pos` is the top-left of the first line. `backing` draws a contrasting pill behind it.
    Text { pos: PointF, text: String, color: Rgba, size: f32, backing: bool },
    /// Destructive mosaic over `rect` with `block`-px cells.
    Pixelate { rect: Rect, block: u32 },
    /// Numbered circular badge.
    Step { center: PointF, number: u32, color: Rgba, radius: f32 },
}

impl Annotation {
    /// Conservative bounding box in buffer coordinates (including stroke width / arrowhead /
    /// estimated text extent) — used for dirty-rect repainting.
    pub fn bounds(&self) -> Rect {
        todo!()
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
        todo!()
    }
    /// Returns the undone annotation, if any.
    pub fn undo(&mut self) -> Option<&Annotation> {
        todo!()
    }
    /// Returns the redone annotation, if any.
    pub fn redo(&mut self) -> Option<&Annotation> {
        todo!()
    }
    pub fn can_undo(&self) -> bool {
        !self.items.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// Next step-counter number: 1 + max existing Step number (1 if none).
    pub fn next_step_number(&self) -> u32 {
        todo!()
    }
    pub fn clear(&mut self) {
        self.items.clear();
        self.redo.clear();
    }
}

/// Simplify a freehand polyline (Ramer–Douglas–Peucker) with tolerance `epsilon` px.
/// Always keeps first and last points.
pub fn simplify_path(points: &[PointF], epsilon: f32) -> Vec<PointF> {
    todo!()
}

/// Snap `to` so the segment `from→to` lies on the nearest 45° increment (Shift modifier).
pub fn snap_45(from: PointF, to: PointF) -> PointF {
    todo!()
}
