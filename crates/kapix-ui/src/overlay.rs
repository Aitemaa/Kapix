//! The capture overlay: frozen frame + selection + annotation editing, as a pure state machine.
//!
//! The host window covers the whole virtual desktop; window client pixel (x, y) == buffer
//! coordinate (x, y). The host forwards [`Input`] events and acts on the returned [`Outcome`].
//! After any `Outcome::Redraw` (or any outcome other than terminal ones) the host calls
//! [`Overlay::paint`] and blits the returned dirty rect from [`Overlay::buffer`].
//!
//! Behavior (see PRD §5.1–5.3, §7.2):
//! - **Selecting** (initial): dimmed frozen frame; crosshair; magnifier loupe near cursor showing
//!   zoomed pixel grid, `(x, y)` in *virtual* coords, `#RRGGBB`. If window snap is on, hovering
//!   highlights the top-most snap rect under the cursor; a click without dragging (< 4 px
//!   movement) selects it; dragging creates a freeform rect. `C` → `Outcome::CopyText(hex)`.
//! - On mouse-up after a selection: `OverlayMode::Ocr` → `Outcome::Ocr(raw crop)`.
//!   Region mode with `instant_copy` (and Alt *not* held) → `Outcome::Copy(flattened)`.
//!   Otherwise enter **Annotating** with a floating pill toolbar placed outside the selection
//!   (below, flipping above/inside near screen edges).
//! - **Annotating**: 8 resize handles + move (drag inside with no tool active... see tool rules);
//!   arrow keys nudge 1 px, Shift+arrows 10 px; tool keys A R P H T B N toggle tools (pressing the
//!   active tool's key again deselects it → move/resize mode); `1`–`6` colors; wheel changes the
//!   active tool's size; `F` toggles rect fill; Shift snaps arrows to 45° / rects to squares;
//!   `Ctrl+Z`/`Ctrl+Y` undo/redo; `T` places an inline text box at click, typed via
//!   [`Input::Char`], Enter commits (Shift+Enter newline), Esc cancels the text box only.
//!   `Enter`/`Ctrl+C` → Copy, `Ctrl+S` → Save, `O` → Ocr (raw crop, no annotations),
//!   `Ctrl+P` → Pin. Toolbar buttons mirror these actions.
//! - `Esc` / right-click (outside text editing) → `Outcome::Cancel` at any time.

use kapix_core::annotate::{simplify_path, snap_45};
use kapix_core::color::PRESETS;
use kapix_core::config::DefaultAction;
use kapix_core::{Annotation, Document, Image, Point, PointF, Rect, Rgba, Tool, VirtualDesktop};
use tiny_skia::Pixmap;

use crate::canvas::{self, rect_minus};
use crate::chrome::{self, Handle, LoupeData, hit_handle, px};
use crate::font::Font;
use crate::render;
use crate::toolbar::{self, Btn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    /// Regular region capture (annotate or instant-copy per config).
    Region,
    /// OCR hotkey: selection → OCR immediately, no toolbar.
    Ocr,
}

pub struct OverlayInit {
    /// Frozen virtual-desktop frame (buffer coordinates). Ownership moves into the overlay and is
    /// dropped with it (memory-reclamation requirement).
    pub frame: Image,
    pub desktop: VirtualDesktop,
    /// Snap candidates in **buffer** coordinates, top-most first (already DWM-frame-adjusted).
    pub windows: Vec<Rect>,
    pub mode: OverlayMode,
    pub default_action: DefaultAction,
    pub show_magnifier: bool,
    pub window_snap: bool,
    pub font: Option<Font>,
    /// Initial cursor position in buffer coordinates (so the loupe shows immediately).
    pub cursor: (i32, i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Non-text keys plus letters/digits. Letters are delivered uppercase (`Key::Char('A')`)
/// regardless of Shift; typed text for the text tool arrives separately as [`Input::Char`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Escape,
    Enter,
    Backspace,
    Delete,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Char(char),
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    MouseMove {
        x: i32,
        y: i32,
        mods: Mods,
    },
    MouseDown {
        x: i32,
        y: i32,
        button: MouseButton,
        mods: Mods,
    },
    MouseUp {
        x: i32,
        y: i32,
        button: MouseButton,
        mods: Mods,
    },
    /// `delta` in notches (+1 = one notch away from user / up).
    Wheel {
        x: i32,
        y: i32,
        delta: f32,
        mods: Mods,
    },
    KeyDown {
        key: Key,
        mods: Mods,
    },
    KeyUp {
        key: Key,
        mods: Mods,
    },
    /// Translated character input (WM_CHAR) — only meaningful while editing text.
    Char(char),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Nothing visible changed.
    None,
    /// Something changed; call [`Overlay::paint`].
    Redraw,
    /// Close overlay, drop all buffers.
    Cancel,
    /// Close overlay and copy this flattened image.
    Copy(Image),
    /// Close overlay and save this flattened image (host decides dialog vs auto path).
    Save(Image),
    /// Close overlay and OCR this raw (unannotated) crop.
    Ocr(Image),
    /// Close overlay and copy text (HEX color).
    CopyText(String),
    /// Close overlay and pin this flattened image at `at` (**virtual** coordinates).
    Pin { image: Image, at: Rect },
}

impl Outcome {
    /// True for outcomes after which the host must destroy the overlay.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Outcome::None | Outcome::Redraw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    Arrow,
    Cross,
    Move,
    SizeNS,
    SizeWE,
    SizeNWSE,
    SizeNESW,
    IBeam,
    Hand,
}

/// Movement (px) below which a press-release counts as a click rather than a drag.
const CLICK_SLOP: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Selecting,
    Annotating,
}

/// A freehand/shape annotation being dragged out.
#[derive(Debug, Clone)]
struct Draft {
    tool: Tool,
    start: PointF,
    cur: PointF,
    points: Vec<PointF>,
    shift: bool,
}

#[derive(Debug, Clone)]
enum Drag {
    None,
    NewSel {
        anchor: Point,
        moved: bool,
        prev: Option<Rect>,
    },
    Move {
        start: Point,
        orig: Rect,
    },
    Resize {
        handle: Handle,
        start: Point,
        orig: Rect,
    },
    Draw(Draft),
    Button(Btn),
}

/// Inline text box being edited.
#[derive(Debug, Clone)]
struct TextEdit {
    pos: PointF,
    text: String,
}

/// Current drawing style (shared by all tools).
#[derive(Debug, Clone, Copy)]
struct Style {
    color: Rgba,
    fill: bool,
    backing: bool,
    sizes: [f32; 7],
}

fn tool_idx(t: Tool) -> usize {
    Tool::ALL.iter().position(|x| *x == t).unwrap_or(0)
}

impl Draft {
    fn annotation(&self, st: &Style, bounds: Rect) -> Option<Annotation> {
        let size = st.sizes[tool_idx(self.tool)];
        let end = if self.shift {
            snap_45(self.start, self.cur)
        } else {
            self.cur
        };
        match self.tool {
            Tool::Arrow => Some(Annotation::Arrow {
                from: self.start,
                to: end,
                color: st.color,
                width: size,
            }),
            Tool::Rect | Tool::Pixelate => {
                let (mut ex, mut ey) = (self.cur.x, self.cur.y);
                if self.shift {
                    let (dx, dy) = (ex - self.start.x, ey - self.start.y);
                    let side = dx.abs().max(dy.abs());
                    ex = self.start.x + side * if dx < 0.0 { -1.0 } else { 1.0 };
                    ey = self.start.y + side * if dy < 0.0 { -1.0 } else { 1.0 };
                }
                let r = Rect::from_ltrb(
                    self.start.x.round() as i32,
                    self.start.y.round() as i32,
                    ex.round() as i32,
                    ey.round() as i32,
                );
                if self.tool == Tool::Rect {
                    Some(Annotation::Rect {
                        rect: r,
                        color: st.color,
                        width: size,
                        filled: st.fill,
                    })
                } else {
                    let r = r.intersect(&bounds)?;
                    Some(Annotation::Pixelate {
                        rect: r,
                        block: size.round().max(2.0) as u32,
                    })
                }
            }
            Tool::Pen => Some(Annotation::Pen {
                points: simplify_path(&self.points, 0.75),
                color: st.color,
                width: size,
            }),
            Tool::Highlighter => {
                let points = if self.shift {
                    vec![self.start, end]
                } else {
                    simplify_path(&self.points, 0.75)
                };
                Some(Annotation::Highlighter {
                    points,
                    color: st.color,
                    width: size,
                })
            }
            Tool::Text | Tool::Step => None,
        }
    }

    /// Big enough to be worth committing?
    fn meaningful(&self, a: &Annotation) -> bool {
        match a {
            Annotation::Arrow { from, to, .. } => (to.x - from.x).hypot(to.y - from.y) >= 3.0,
            Annotation::Rect { rect, .. } => rect.w >= 2 && rect.h >= 2,
            Annotation::Pixelate { rect, .. } => rect.w >= 3 && rect.h >= 3,
            Annotation::Pen { points, .. } | Annotation::Highlighter { points, .. } => {
                let ext = points.iter().fold(0f32, |m, p| {
                    m.max((p.x - self.start.x).abs())
                        .max((p.y - self.start.y).abs())
                });
                ext >= 2.0
            }
            _ => true,
        }
    }
}

/// A drawable piece of chrome / live preview. Equality decides whether it needs repainting.
#[derive(Debug, Clone, PartialEq)]
enum Elem {
    Preview {
        ann: Annotation,
        outline: Option<Rect>,
    },
    Edit {
        ann: Annotation,
        caret: Rect,
        frame: Rect,
    },
    Snap {
        rect: Rect,
        s: f32,
    },
    Border {
        rect: Rect,
        handles: bool,
        s: f32,
    },
    Ring {
        c: Point,
        tool: Tool,
        size: f32,
    },
    Label {
        rect: Rect,
        text: String,
        s: f32,
    },
    Bar {
        origin: Point,
        st: toolbar::State,
        s: f32,
    },
    Loupe(LoupeData),
}

struct Item {
    elem: Elem,
    /// Everything this element may modify (clipped to the frame).
    pieces: Vec<Rect>,
}

pub struct Overlay {
    w: u32,
    h: u32,
    bounds: Rect,
    /// Raw frozen frame (never annotated; source of exports, OCR and the loupe).
    frame: Pixmap,
    /// Frame + committed annotations over the whole frame; allocated on first annotation.
    composed: Option<Pixmap>,
    /// What the host blits.
    back: Pixmap,
    desktop: VirtualDesktop,
    windows: Vec<Rect>,
    mode: OverlayMode,
    default_action: DefaultAction,
    show_magnifier: bool,
    window_snap: bool,
    font: Option<Font>,

    cursor: Point,
    phase: Phase,
    sel: Option<Rect>,
    snap: Option<Rect>,
    drag: Drag,
    hover: Option<Btn>,
    shift_down: bool,

    doc: Document,
    tool: Option<Tool>,
    style: Style,
    text: Option<TextEdit>,

    // Incremental paint bookkeeping.
    items: Vec<Item>,
    last_bright: Option<Rect>,
    pending: Vec<Rect>,
    painted: bool,
}

impl Overlay {
    pub fn new(init: OverlayInit) -> Overlay {
        let (w, h) = (init.frame.width, init.frame.height);
        let frame = canvas::image_to_pixmap(init.frame);
        let back = Pixmap::new(w, h).expect("non-empty frame");
        let mut sizes = [0f32; 7];
        for t in Tool::ALL {
            sizes[tool_idx(t)] = t.default_size();
        }
        let mut o = Overlay {
            w,
            h,
            bounds: Rect::new(0, 0, w as i32, h as i32),
            frame,
            composed: None,
            back,
            desktop: init.desktop,
            windows: init.windows,
            mode: init.mode,
            default_action: init.default_action,
            show_magnifier: init.show_magnifier,
            window_snap: init.window_snap,
            font: init.font,
            cursor: Point::new(init.cursor.0, init.cursor.1),
            phase: Phase::Selecting,
            sel: None,
            snap: None,
            drag: Drag::None,
            hover: None,
            shift_down: false,
            doc: Document::new(),
            tool: None,
            style: Style {
                color: PRESETS[0],
                fill: false,
                backing: true,
                sizes,
            },
            text: None,
            items: Vec::new(),
            last_bright: None,
            pending: Vec::new(),
            painted: false,
        };
        o.update_snap();
        o
    }

    /// Frame size (== host window client size).
    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    /// Current selection in buffer coordinates, if any.
    pub fn selection(&self) -> Option<Rect> {
        self.sel.filter(|r| !r.is_empty())
    }

    /// Committed annotations (excludes the text box being edited).
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Active annotation tool, if any.
    pub fn tool(&self) -> Option<Tool> {
        self.tool
    }

    /// Toolbar bounds (buffer coordinates) while it is visible.
    pub fn toolbar_rect(&self) -> Option<Rect> {
        self.toolbar_layout().map(|l| l.rect)
    }

    /// True once a selection exists and the toolbar workflow is active.
    pub fn is_annotating(&self) -> bool {
        self.phase == Phase::Annotating
    }

    pub fn handle(&mut self, input: Input) -> Outcome {
        match input {
            Input::MouseMove { x, y, mods } => self.on_move(Point::new(x, y), mods),
            Input::MouseDown { x, y, button, mods } => self.on_down(Point::new(x, y), button, mods),
            Input::MouseUp { x, y, button, mods } => self.on_up(Point::new(x, y), button, mods),
            Input::Wheel { delta, .. } => self.on_wheel(delta),
            Input::KeyDown { key, mods } => self.on_key(key, mods),
            Input::KeyUp { mods, .. } => self.sync_shift(mods),
            Input::Char(c) => self.on_char(c),
        }
    }

    /// Recompose the backbuffer where needed; returns the dirty rect to blit (buffer coords),
    /// or `None` if nothing changed. The first call returns the full bounds.
    /// Target: < 4 ms for a typical mouse-move at 4K (only recompose dirty areas).
    pub fn paint(&mut self) -> Option<Rect> {
        let bounds = self.bounds;
        let new_items = self.build_items();
        let bright = self.bright_rect();
        let mut dirty: Vec<Rect> = Vec::new();
        if !self.painted {
            dirty.push(bounds);
        } else {
            dirty.append(&mut self.pending);
            match (self.last_bright, bright) {
                (a, b) if a == b => {}
                (Some(a), Some(b)) => {
                    dirty.extend(rect_minus(a, b));
                    dirty.extend(rect_minus(b, a));
                }
                (Some(a), None) | (None, Some(a)) => dirty.push(a),
                (None, None) => {}
            }
            for old in &self.items {
                if !new_items.iter().any(|n| n.elem == old.elem) {
                    dirty.extend(old.pieces.iter().copied());
                }
            }
            for new in &new_items {
                if !self.items.iter().any(|o| o.elem == new.elem) {
                    dirty.extend(new.pieces.iter().copied());
                }
            }
        }
        self.pending.clear();
        self.last_bright = bright;
        self.painted = true;
        let mut dirty: Vec<Rect> = dirty
            .into_iter()
            .filter_map(|r| r.intersect(&bounds))
            .collect();
        if dirty.is_empty() {
            self.items = new_items;
            return None;
        }
        // Anything overlapping a dirty area must be redrawn whole, which in turn dirties the
        // rest of its footprint (and so on): this keeps stacked translucent chrome exact.
        let mut included = vec![false; new_items.len()];
        loop {
            let mut changed = false;
            for (i, it) in new_items.iter().enumerate() {
                if !included[i]
                    && it
                        .pieces
                        .iter()
                        .any(|p| dirty.iter().any(|d| d.intersect(p).is_some()))
                {
                    included[i] = true;
                    dirty.extend(it.pieces.iter().copied());
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let dirty = merge_rects(dirty);

        let Overlay {
            back,
            frame,
            composed,
            font,
            ..
        } = self;
        let base: &Pixmap = composed.as_ref().unwrap_or(frame);
        for r in &dirty {
            restore(back, base, bright, *r);
        }
        for (i, it) in new_items.iter().enumerate() {
            if included[i] {
                draw_elem(back, base, frame, bright, font.as_ref(), &it.elem);
            }
        }
        self.items = new_items;
        dirty.iter().copied().reduce(|a, b| a.union(&b))
    }

    /// Composited BGRA backbuffer, `size().0 * size().1 * 4` bytes, top-down.
    pub fn buffer(&self) -> &[u8] {
        self.back.data()
    }

    /// Cursor the host should show at the current pointer position.
    pub fn cursor(&self) -> CursorKind {
        if self.phase == Phase::Selecting {
            return CursorKind::Cross;
        }
        match &self.drag {
            Drag::Move { .. } => return CursorKind::Move,
            Drag::Resize { handle, .. } => return handle_cursor(*handle),
            Drag::Draw(_) | Drag::NewSel { .. } => return CursorKind::Cross,
            Drag::Button(_) => return CursorKind::Hand,
            Drag::None => {}
        }
        if let Some(l) = self.toolbar_layout() {
            if l.rect.contains(self.cursor) {
                return if l.hit(self.cursor).is_some() {
                    CursorKind::Hand
                } else {
                    CursorKind::Arrow
                };
            }
        }
        if self.text.is_some() {
            return CursorKind::IBeam;
        }
        match self.tool {
            Some(Tool::Text) => CursorKind::IBeam,
            Some(_) => CursorKind::Cross,
            None => {
                let Some(sel) = self.selection() else {
                    return CursorKind::Cross;
                };
                let s = self.chrome_scale();
                if let Some(h) = hit_handle(sel, self.cursor, s) {
                    handle_cursor(h)
                } else if sel.contains(self.cursor) {
                    CursorKind::Move
                } else {
                    CursorKind::Cross
                }
            }
        }
    }

    /// True while an inline text box is being edited (host should route WM_CHAR).
    pub fn is_editing_text(&self) -> bool {
        self.text.is_some()
    }

    // -----------------------------------------------------------------------------------------
    // Geometry helpers
    // -----------------------------------------------------------------------------------------

    fn clamp_pt(&self, p: Point) -> Point {
        Point::new(p.x.clamp(0, self.w as i32), p.y.clamp(0, self.h as i32))
    }

    fn scale_at(&self, p: Point) -> f32 {
        self.desktop
            .monitor_at_buffer(p)
            .map(|m| m.scale)
            .filter(|s| *s > 0.0)
            .unwrap_or(1.0)
    }

    fn monitor_rect_at(&self, p: Point) -> Rect {
        self.desktop
            .monitor_at_buffer(p)
            .map(|m| self.desktop.rect_to_buffer(m.rect))
            .filter(|r| !r.is_empty())
            .unwrap_or(self.bounds)
    }

    /// DPI scale for chrome: monitor under the selection while annotating, else under the cursor.
    fn chrome_scale(&self) -> f32 {
        match (self.phase, self.selection()) {
            (Phase::Annotating, Some(sel)) => self.scale_at(sel.center()),
            _ => self.scale_at(self.cursor),
        }
    }

    fn update_snap(&mut self) {
        self.snap = None;
        if self.window_snap && self.phase == Phase::Selecting {
            let p = self.cursor;
            self.snap = self
                .windows
                .iter()
                .find(|r| r.contains(p))
                .and_then(|r| r.intersect(&self.bounds));
        }
    }

    fn toolbar_visible(&self) -> bool {
        self.phase == Phase::Annotating
            && self.selection().is_some()
            && !matches!(
                self.drag,
                Drag::NewSel { .. } | Drag::Move { .. } | Drag::Resize { .. }
            )
    }

    fn toolbar_layout(&self) -> Option<toolbar::Layout> {
        if !self.toolbar_visible() {
            return None;
        }
        let sel = self.selection()?;
        let s = self.chrome_scale();
        let (tw, th) = toolbar::size(s);
        let mon = self.monitor_rect_at(sel.center());
        let gap = px(10.0, s);
        let margin = px(8.0, s);
        let x = (sel.x + sel.w / 2 - tw / 2).clamp(
            mon.x + margin,
            (mon.right() - tw - margin).max(mon.x + margin),
        );
        let below = sel.bottom() + gap;
        let y = if below + th + margin <= mon.bottom() {
            below
        } else if sel.y - gap - th >= mon.y + margin {
            sel.y - gap - th
        } else {
            (sel.bottom() - th - gap).clamp(
                mon.y + margin,
                (mon.bottom() - th - margin).max(mon.y + margin),
            )
        };
        Some(toolbar::layout_at(x, y, s))
    }

    /// Region shown at full brightness: the selection, or the snap highlight while hovering.
    fn bright_rect(&self) -> Option<Rect> {
        if let Some(sel) = self.selection() {
            return Some(sel);
        }
        if self.phase == Phase::Selecting && !matches!(self.drag, Drag::NewSel { moved: true, .. })
        {
            return self.snap;
        }
        None
    }

    fn pixel_at(&self, p: Point) -> Rgba {
        let p = Point::new(
            p.x.clamp(0, self.w as i32 - 1),
            p.y.clamp(0, self.h as i32 - 1),
        );
        let i = (p.y as usize * self.w as usize + p.x as usize) * 4;
        let d = self.frame.data();
        Rgba::from_bgra([d[i], d[i + 1], d[i + 2], d[i + 3]])
    }

    // -----------------------------------------------------------------------------------------
    // Input handling
    // -----------------------------------------------------------------------------------------

    fn sync_shift(&mut self, mods: Mods) -> Outcome {
        self.shift_down = mods.shift;
        if let Drag::Draw(d) = &mut self.drag {
            if d.shift != mods.shift {
                d.shift = mods.shift;
                return Outcome::Redraw;
            }
        }
        Outcome::None
    }

    fn on_move(&mut self, p: Point, mods: Mods) -> Outcome {
        self.cursor = p;
        self.shift_down = mods.shift;
        let bounds = self.bounds;
        match &mut self.drag {
            Drag::NewSel { anchor, moved, .. } => {
                if !*moved
                    && ((p.x - anchor.x).abs() >= CLICK_SLOP
                        || (p.y - anchor.y).abs() >= CLICK_SLOP)
                {
                    *moved = true;
                }
                if *moved {
                    let a = *anchor;
                    let q = Point::new(p.x.clamp(0, bounds.w), p.y.clamp(0, bounds.h));
                    self.sel = Some(Rect::from_points(a, q));
                }
            }
            Drag::Move { start, orig } => {
                let (dx, dy) = (p.x - start.x, p.y - start.y);
                self.sel = Some(orig.translate(dx, dy).clamp_inside(&bounds));
            }
            Drag::Resize {
                handle,
                start,
                orig,
            } => {
                let r = resized(*orig, *handle, p.x - start.x, p.y - start.y, bounds);
                if !r.is_empty() {
                    self.sel = Some(r);
                }
            }
            Drag::Draw(d) => {
                d.shift = mods.shift;
                d.cur = PointF::new(p.x as f32, p.y as f32);
                if matches!(d.tool, Tool::Pen | Tool::Highlighter) {
                    let last = *d.points.last().unwrap_or(&d.start);
                    if (d.cur.x - last.x).hypot(d.cur.y - last.y) >= 0.5 {
                        d.points.push(d.cur);
                    }
                }
            }
            Drag::Button(_) | Drag::None => {}
        }
        match self.drag {
            Drag::None | Drag::Button(_) => {
                self.hover = self.toolbar_layout().and_then(|l| l.hit(p));
            }
            _ => self.hover = None,
        }
        if matches!(self.drag, Drag::None) {
            self.update_snap();
        }
        Outcome::Redraw
    }

    fn on_down(&mut self, p: Point, button: MouseButton, mods: Mods) -> Outcome {
        self.cursor = p;
        if button != MouseButton::Left {
            return Outcome::None;
        }
        match self.phase {
            Phase::Selecting => {
                self.drag = Drag::NewSel {
                    anchor: self.clamp_pt(p),
                    moved: false,
                    prev: None,
                };
                self.sel = None;
                Outcome::Redraw
            }
            Phase::Annotating => self.annotating_down(p, mods),
        }
    }

    fn annotating_down(&mut self, p: Point, mods: Mods) -> Outcome {
        let Some(sel) = self.selection() else {
            return Outcome::None;
        };
        // Clicking away from an open text box commits it first.
        let mut out = Outcome::None;
        if self.text.is_some() {
            let inside = self.edit_frame().is_some_and(|r| r.contains(p));
            if inside {
                return Outcome::None;
            }
            self.commit_text();
            out = Outcome::Redraw;
        }
        if let Some(l) = self.toolbar_layout() {
            if l.rect.contains(p) {
                self.drag = match l.hit(p) {
                    Some(b) => Drag::Button(b),
                    None => Drag::None,
                };
                self.hover = l.hit(p);
                return Outcome::Redraw;
            }
        }
        let s = self.chrome_scale();
        let pf = PointF::new(p.x as f32, p.y as f32);
        match self.tool {
            Some(Tool::Text) => {
                self.begin_text(pf);
                Outcome::Redraw
            }
            Some(Tool::Step) => {
                let a = Annotation::Step {
                    center: pf,
                    number: self.doc.next_step_number(),
                    color: self.style.color,
                    radius: self.style.sizes[tool_idx(Tool::Step)],
                };
                self.push_annotation(a);
                Outcome::Redraw
            }
            Some(t) => {
                self.drag = Drag::Draw(Draft {
                    tool: t,
                    start: pf,
                    cur: pf,
                    points: vec![pf],
                    shift: mods.shift,
                });
                Outcome::Redraw
            }
            None => {
                if let Some(h) = hit_handle(sel, p, s) {
                    self.drag = Drag::Resize {
                        handle: h,
                        start: p,
                        orig: sel,
                    };
                } else if sel.contains(p) {
                    self.drag = Drag::Move {
                        start: p,
                        orig: sel,
                    };
                } else {
                    self.drag = Drag::NewSel {
                        anchor: self.clamp_pt(p),
                        moved: false,
                        prev: Some(sel),
                    };
                }
                if matches!(out, Outcome::None) {
                    Outcome::Redraw
                } else {
                    out
                }
            }
        }
    }

    fn on_up(&mut self, p: Point, button: MouseButton, mods: Mods) -> Outcome {
        self.cursor = p;
        if button == MouseButton::Right {
            self.drag = Drag::None;
            if self.text.is_some() {
                self.cancel_text();
                return Outcome::Redraw;
            }
            return Outcome::Cancel;
        }
        if button != MouseButton::Left {
            return Outcome::None;
        }
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::None => Outcome::None,
            Drag::NewSel { moved, prev, .. } => match self.phase {
                Phase::Selecting => {
                    let chosen = if moved {
                        self.sel.filter(|r| !r.is_empty())
                    } else {
                        self.update_snap();
                        self.snap
                    };
                    match chosen {
                        Some(r) => self.finish_selection(r, mods),
                        None => {
                            self.sel = None;
                            self.update_snap();
                            Outcome::Redraw
                        }
                    }
                }
                Phase::Annotating => {
                    if !moved || self.selection().is_none() {
                        self.sel = prev;
                    }
                    Outcome::Redraw
                }
            },
            Drag::Move { .. } | Drag::Resize { .. } => Outcome::Redraw,
            Drag::Draw(d) => {
                if let Some(a) = d
                    .annotation(&self.style, self.bounds)
                    .filter(|a| d.meaningful(a))
                {
                    self.push_annotation(a);
                }
                Outcome::Redraw
            }
            Drag::Button(b) => {
                let still = self.toolbar_layout().and_then(|l| l.hit(p));
                self.hover = still;
                if still == Some(b) {
                    self.activate(b)
                } else {
                    Outcome::Redraw
                }
            }
        }
    }

    fn on_wheel(&mut self, delta: f32) -> Outcome {
        if self.phase != Phase::Annotating {
            return Outcome::None;
        }
        let Some(tool) = self.tool else {
            return Outcome::None;
        };
        let (lo, hi) = tool.size_range();
        let step = ((hi - lo) / 16.0).round().max(1.0);
        let i = tool_idx(tool);
        let v = (self.style.sizes[i] + step * delta.signum() * delta.abs().clamp(1.0, 5.0))
            .clamp(lo, hi);
        self.style.sizes[i] = v.round();
        Outcome::Redraw
    }

    fn on_char(&mut self, c: char) -> Outcome {
        match &mut self.text {
            Some(t) if !c.is_control() => {
                t.text.push(c);
                Outcome::Redraw
            }
            _ => Outcome::None,
        }
    }

    fn on_key(&mut self, key: Key, mods: Mods) -> Outcome {
        self.shift_down = mods.shift;
        if self.phase == Phase::Selecting {
            return match key {
                Key::Escape => Outcome::Cancel,
                Key::Char('C') if !mods.ctrl => {
                    let hex = self.pixel_at(self.cursor).hex();
                    Outcome::CopyText(hex)
                }
                _ => Outcome::None,
            };
        }
        if self.text.is_some() {
            return match key {
                Key::Escape => {
                    self.cancel_text();
                    Outcome::Redraw
                }
                Key::Enter if mods.shift => {
                    if let Some(t) = &mut self.text {
                        t.text.push('\n');
                    }
                    Outcome::Redraw
                }
                Key::Enter => {
                    self.commit_text();
                    Outcome::Redraw
                }
                Key::Backspace => {
                    if let Some(t) = &mut self.text {
                        t.text.pop();
                    }
                    Outcome::Redraw
                }
                Key::Char(c) if mods.ctrl => match c {
                    'C' => self.export(Export::Copy),
                    'S' => self.export(Export::Save),
                    'P' => self.export(Export::Pin),
                    _ => Outcome::None,
                },
                _ => Outcome::None,
            };
        }
        match key {
            Key::Escape => Outcome::Cancel,
            Key::Enter => self.export(Export::Copy),
            Key::Left | Key::Right | Key::Up | Key::Down => {
                let step = if mods.shift { 10 } else { 1 };
                let (dx, dy) = match key {
                    Key::Left => (-step, 0),
                    Key::Right => (step, 0),
                    Key::Up => (0, -step),
                    _ => (0, step),
                };
                match self.selection() {
                    Some(sel) => {
                        self.sel = Some(sel.translate(dx, dy).clamp_inside(&self.bounds));
                        Outcome::Redraw
                    }
                    None => Outcome::None,
                }
            }
            Key::Char(c) if mods.ctrl => match c {
                'C' => self.export(Export::Copy),
                'S' => self.export(Export::Save),
                'P' => self.export(Export::Pin),
                'Z' if mods.shift => self.redo(),
                'Z' => self.undo(),
                'Y' => self.redo(),
                _ => Outcome::None,
            },
            Key::Char('O') => self.export(Export::Ocr),
            Key::Char('F') => {
                match self.tool {
                    Some(Tool::Text) => self.style.backing = !self.style.backing,
                    _ => self.style.fill = !self.style.fill,
                }
                Outcome::Redraw
            }
            Key::Char(c @ '1'..='6') => {
                self.style.color = PRESETS[(c as u8 - b'1') as usize];
                Outcome::Redraw
            }
            Key::Char(c) => match Tool::from_key(c) {
                Some(t) => {
                    self.select_tool(t);
                    Outcome::Redraw
                }
                None => Outcome::None,
            },
            _ => Outcome::None,
        }
    }

    fn select_tool(&mut self, t: Tool) {
        self.tool = if self.tool == Some(t) { None } else { Some(t) };
    }

    fn activate(&mut self, b: Btn) -> Outcome {
        match b {
            Btn::Tool(t) => {
                self.select_tool(t);
                Outcome::Redraw
            }
            Btn::Color(i) => {
                self.style.color = PRESETS[i];
                Outcome::Redraw
            }
            Btn::Undo => self.undo(),
            Btn::Redo => self.redo(),
            Btn::Ocr => self.export(Export::Ocr),
            Btn::Pin => self.export(Export::Pin),
            Btn::Save => self.export(Export::Save),
            Btn::Copy => self.export(Export::Copy),
            Btn::Close => Outcome::Cancel,
        }
    }

    // -----------------------------------------------------------------------------------------
    // Selection / export
    // -----------------------------------------------------------------------------------------

    fn finish_selection(&mut self, r: Rect, mods: Mods) -> Outcome {
        self.sel = Some(r);
        let instant = self.mode == OverlayMode::Region
            && self.default_action == DefaultAction::InstantCopy
            && !mods.alt;
        if self.mode == OverlayMode::Ocr {
            return Outcome::Ocr(render::crop_image(self.frame.as_ref(), r));
        }
        if instant {
            return Outcome::Copy(render::crop_image(self.frame.as_ref(), r));
        }
        self.phase = Phase::Annotating;
        self.snap = None;
        self.drag = Drag::None;
        self.hover = None;
        Outcome::Redraw
    }

    fn export(&mut self, kind: Export) -> Outcome {
        self.commit_text();
        let Some(sel) = self.selection() else {
            return Outcome::None;
        };
        match kind {
            Export::Ocr => Outcome::Ocr(render::crop_image(self.frame.as_ref(), sel)),
            _ => {
                let img = render::flatten_ref(
                    self.frame.as_ref(),
                    sel,
                    self.doc.items(),
                    self.font.as_ref(),
                );
                match kind {
                    Export::Copy => Outcome::Copy(img),
                    Export::Save => Outcome::Save(img),
                    _ => Outcome::Pin {
                        image: img,
                        at: self.desktop.rect_to_virtual(sel),
                    },
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Annotation document
    // -----------------------------------------------------------------------------------------

    fn ann_bounds(&self, a: &Annotation) -> Rect {
        render::annotation_bounds(a, self.font.as_ref())
    }

    fn ensure_composed(&mut self) -> &mut Pixmap {
        let frame = &self.frame;
        self.composed.get_or_insert_with(|| frame.clone())
    }

    fn push_annotation(&mut self, a: Annotation) {
        let b = self.ann_bounds(&a);
        let font = self.font.clone();
        let pm = self.ensure_composed();
        render::render_annotations(pm, std::slice::from_ref(&a), (0, 0), font.as_ref());
        self.doc.push(a);
        self.pending.push(b);
    }

    fn rebuild_composed(&mut self) {
        if self.doc.is_empty() {
            self.composed = None;
        } else {
            let mut pm = self.frame.clone();
            render::render_annotations(&mut pm, self.doc.items(), (0, 0), self.font.as_ref());
            self.composed = Some(pm);
        }
    }

    fn undo(&mut self) -> Outcome {
        self.commit_text();
        let Some(a) = self.doc.undo().cloned() else {
            return Outcome::None;
        };
        let b = self.ann_bounds(&a);
        self.rebuild_composed();
        self.pending.push(b);
        Outcome::Redraw
    }

    fn redo(&mut self) -> Outcome {
        self.commit_text();
        let Some(a) = self.doc.redo().cloned() else {
            return Outcome::None;
        };
        let b = self.ann_bounds(&a);
        let font = self.font.clone();
        let pm = self.ensure_composed();
        render::render_annotations(pm, std::slice::from_ref(&a), (0, 0), font.as_ref());
        self.pending.push(b);
        Outcome::Redraw
    }

    // -----------------------------------------------------------------------------------------
    // Inline text
    // -----------------------------------------------------------------------------------------

    fn text_size(&self) -> f32 {
        self.style.sizes[tool_idx(Tool::Text)]
    }

    fn begin_text(&mut self, p: PointF) {
        let size = self.text_size();
        let lh = self
            .font
            .as_ref()
            .map_or(size * 1.2, |f| f.measure("", size).1);
        self.text = Some(TextEdit {
            pos: PointF::new(p.x, p.y - lh / 2.0),
            text: String::new(),
        });
    }

    fn text_annotation(&self) -> Option<Annotation> {
        let t = self.text.as_ref()?;
        Some(Annotation::Text {
            pos: t.pos,
            text: t.text.clone(),
            color: self.style.color,
            size: self.text_size(),
            backing: self.style.backing,
        })
    }

    fn commit_text(&mut self) {
        let Some(a) = self.text_annotation() else {
            return;
        };
        let edit_area = self.edit_elem().map(|(_, _, f)| f);
        self.text = None;
        if let Annotation::Text { text, .. } = &a {
            if !text.trim().is_empty() {
                self.push_annotation(a);
            }
        }
        if let Some(f) = edit_area {
            self.pending.push(f);
        }
    }

    fn cancel_text(&mut self) {
        if let Some((_, _, f)) = self.edit_elem() {
            self.pending.push(f);
        }
        self.text = None;
    }

    /// Annotation preview, caret rect and outline frame for the open text box.
    fn edit_elem(&self) -> Option<(Annotation, Rect, Rect)> {
        let ann = self.text_annotation()?;
        let Annotation::Text {
            pos, text, size, ..
        } = &ann
        else {
            return None;
        };
        let font = self.font.as_ref();
        let (w, h, last_w, lh) = match font {
            Some(f) => {
                let (w, h) = f.measure_multiline(text, *size);
                let last = text.rsplit('\n').next().unwrap_or("");
                (w, h, f.measure(last, *size).0, f.line_height(*size))
            }
            None => {
                let n = text.split('\n').count() as f32;
                (
                    size * 0.6 * text.len() as f32,
                    size * 1.2 * n,
                    size * 0.6 * text.rsplit('\n').next().unwrap_or("").len() as f32,
                    size * 1.2,
                )
            }
        };
        let lines = text.split('\n').count() as f32;
        let fh = if lines > 0.0 {
            h - lh * (lines - 1.0)
        } else {
            h
        };
        let (padx, pady) = (size * 0.55, size * 0.28);
        let caret = Rect::new(
            (pos.x + last_w).round() as i32 + 1,
            (pos.y + lh * (lines - 1.0)).round() as i32,
            2,
            fh.round() as i32,
        );
        let inner_w = w.max(size * 2.5) + 4.0;
        let frame = Rect::new(
            (pos.x - padx - 3.0).floor() as i32,
            (pos.y - pady - 3.0).floor() as i32,
            (inner_w + 2.0 * padx + 6.0).ceil() as i32,
            (h + 2.0 * pady + 6.0).ceil() as i32,
        );
        Some((ann, caret, frame))
    }

    fn edit_frame(&self) -> Option<Rect> {
        self.edit_elem().map(|(_, _, f)| f)
    }

    // -----------------------------------------------------------------------------------------
    // Painting
    // -----------------------------------------------------------------------------------------

    fn build_items(&self) -> Vec<Item> {
        let mut v: Vec<Item> = Vec::new();
        let bounds = self.bounds;
        let font = self.font.as_ref();
        let s = self.chrome_scale();
        let clip = |r: Rect| r.intersect(&bounds).into_iter().collect::<Vec<_>>();

        if let Drag::Draw(d) = &self.drag {
            if let Some(ann) = d.annotation(&self.style, bounds) {
                let b = self.ann_bounds(&ann);
                let outline = match &ann {
                    Annotation::Pixelate { rect, .. } => Some(*rect),
                    _ => None,
                };
                v.push(Item {
                    elem: Elem::Preview { ann, outline },
                    pieces: clip(b.inflate(2)),
                });
            }
        }
        if let Some((ann, caret, frame)) = self.edit_elem() {
            let b = self.ann_bounds(&ann).union(&frame).union(&caret);
            v.push(Item {
                elem: Elem::Edit { ann, caret, frame },
                pieces: clip(b.inflate(3)),
            });
        }

        let sel = self.selection();
        if self.phase == Phase::Selecting && sel.is_none() {
            if let Some(r) = self.snap {
                let t = px(3.0, s).max(3);
                v.push(Item {
                    elem: Elem::Snap { rect: r, s },
                    pieces: rect_minus(r, r.inflate(-t)),
                });
            }
        }
        if let Some(r) = sel {
            let handles = self.phase == Phase::Annotating
                && self.tool.is_none()
                && !matches!(self.drag, Drag::Draw(_));
            let pad = chrome::border_pad(s);
            let mut pieces: Vec<Rect> = rect_minus(r.inflate(pad), r.inflate(-pad));
            if r.inflate(-pad).is_empty() {
                pieces = vec![r.inflate(pad)];
            }
            pieces = pieces
                .into_iter()
                .filter_map(|p| p.intersect(&bounds))
                .collect();
            v.push(Item {
                elem: Elem::Border {
                    rect: r,
                    handles,
                    s,
                },
                pieces,
            });
        }

        let bar = self.toolbar_layout();
        // Size label for the selection, or for the hovered snap window.
        let label_target = sel.or(if self.phase == Phase::Selecting {
            self.snap
        } else {
            None
        });
        if let (Some(r), Some(f)) = (label_target, font) {
            let text = format!("{} × {}", r.w, r.h);
            let (lw, lh) = chrome::label_size(f, &text, s);
            let gap = px(6.0, s);
            let overlaps = |c: Rect| {
                bar.as_ref()
                    .is_some_and(|b| b.rect.inflate(shadow_gap(s)).intersect(&c).is_some())
            };
            let mut lr = Rect::new(r.x, r.y - lh - gap, lw, lh);
            if lr.y < 0 || overlaps(lr) {
                lr = Rect::new(r.x + gap, r.y + gap, lw, lh);
            }
            lr.x = lr.x.clamp(0, (self.w as i32 - lw).max(0));
            v.push(Item {
                elem: Elem::Label { rect: lr, text, s },
                pieces: clip(lr.inflate(2)),
            });
        }

        if let Some(l) = &bar {
            let st = toolbar::State {
                tool: self.tool,
                color: self.style.color,
                filled: self.style.fill,
                can_undo: self.doc.can_undo(),
                can_redo: self.doc.can_redo(),
                hover: self.hover,
                pressed: match self.drag {
                    Drag::Button(b) if self.hover == Some(b) => Some(b),
                    _ => None,
                },
            };
            let pad = toolbar::shadow_pad(s);
            v.push(Item {
                elem: Elem::Bar {
                    origin: Point::new(l.rect.x, l.rect.y),
                    st,
                    s,
                },
                pieces: clip(l.rect.inflate(pad)),
            });
        }

        // Tool-size ring.
        if self.phase == Phase::Annotating
            && self.text.is_none()
            && self.bounds.contains(self.cursor)
        {
            if let Some(t) = self.tool.filter(|t| *t != Tool::Text) {
                let drawing_other = matches!(&self.drag, Drag::Draw(d) if !matches!(d.tool, Tool::Pen | Tool::Highlighter));
                let over_bar = bar
                    .as_ref()
                    .is_some_and(|b| b.rect.inflate(shadow_gap(s)).contains(self.cursor));
                let idle = matches!(self.drag, Drag::None | Drag::Draw(_));
                if idle && !drawing_other && !over_bar {
                    let size = self.style.sizes[tool_idx(t)];
                    let e = chrome::ring_extent(t, size).ceil() as i32 + 2;
                    let c = self.cursor;
                    v.push(Item {
                        elem: Elem::Ring { c, tool: t, size },
                        pieces: clip(Rect::new(c.x - e, c.y - e, 2 * e + 1, 2 * e + 1)),
                    });
                }
            }
        }

        // Loupe.
        let want_loupe = self.show_magnifier
            && self.bounds.contains(self.cursor)
            && match self.phase {
                Phase::Selecting => matches!(self.drag, Drag::None | Drag::NewSel { .. }),
                Phase::Annotating => matches!(
                    self.drag,
                    Drag::NewSel { moved: true, .. } | Drag::Resize { .. }
                ),
            };
        if want_loupe {
            let cs = self.scale_at(self.cursor);
            let dragging = !matches!(self.drag, Drag::None) && sel.is_some();
            let dims = if dragging {
                sel.map(|r| (r.w, r.h))
            } else {
                None
            };
            let has_text = font.is_some();
            let (cw, ch) = chrome::loupe_card_size(cs, dims.is_some(), has_text);
            let mon = self.monitor_rect_at(self.cursor);
            let off = px(20.0, cs);
            let c = self.cursor;
            let mut x = c.x + off;
            let mut y = c.y + off;
            if x + cw > mon.right() {
                x = c.x - off - cw;
            }
            if y + ch > mon.bottom() {
                y = c.y - off - ch;
            }
            x = x.clamp(mon.x, (mon.right() - cw).max(mon.x));
            y = y.clamp(mon.y, (mon.bottom() - ch).max(mon.y));
            let card = Rect::new(x, y, cw, ch);
            let center = Point::new(
                c.x.clamp(0, self.w as i32 - 1),
                c.y.clamp(0, self.h as i32 - 1),
            );
            let data = LoupeData {
                card,
                center,
                virtual_pos: self.desktop.to_virtual(c),
                color: self.pixel_at(c),
                dims,
                s: cs,
                text: has_text,
            };
            v.push(Item {
                elem: Elem::Loupe(data),
                pieces: clip(card.inflate(toolbar::shadow_pad(cs))),
            });
        }
        v
    }
}

fn shadow_gap(s: f32) -> i32 {
    px(4.0, s)
}

#[derive(Clone, Copy)]
enum Export {
    Copy,
    Save,
    Ocr,
    Pin,
}

fn handle_cursor(h: Handle) -> CursorKind {
    match h {
        Handle::N | Handle::S => CursorKind::SizeNS,
        Handle::E | Handle::W => CursorKind::SizeWE,
        Handle::NW | Handle::SE => CursorKind::SizeNWSE,
        Handle::NE | Handle::SW => CursorKind::SizeNESW,
    }
}

/// Apply a handle drag (`dx`,`dy` since the press) to `orig`, clamped to the frame.
fn resized(orig: Rect, h: Handle, dx: i32, dy: i32, bounds: Rect) -> Rect {
    let (mut l, mut t, mut r, mut b) = (orig.x, orig.y, orig.right(), orig.bottom());
    match h {
        Handle::NW => (l, t) = (l + dx, t + dy),
        Handle::N => t += dy,
        Handle::NE => (r, t) = (r + dx, t + dy),
        Handle::E => r += dx,
        Handle::SE => (r, b) = (r + dx, b + dy),
        Handle::S => b += dy,
        Handle::SW => (l, b) = (l + dx, b + dy),
        Handle::W => l += dx,
    }
    let c = |v: i32, hi: i32| v.clamp(0, hi);
    Rect::from_ltrb(
        c(l, bounds.w),
        c(t, bounds.h),
        c(r, bounds.w),
        c(b, bounds.h),
    )
}

/// Drop rects fully contained in another one.
fn merge_rects(mut v: Vec<Rect>) -> Vec<Rect> {
    v.sort_by_key(|r| std::cmp::Reverse(r.area()));
    let mut out: Vec<Rect> = Vec::with_capacity(v.len());
    for r in v {
        if !out.iter().any(|o| o.contains_rect(&r)) {
            out.push(r);
        }
    }
    out
}

/// Rewrite `r` of `back` from the base layer: bright inside `bright`, dimmed elsewhere.
fn restore(back: &mut Pixmap, base: &Pixmap, bright: Option<Rect>, r: Rect) {
    match bright.and_then(|b| b.intersect(&r)) {
        Some(i) => {
            canvas::blit(base, i, back, i.x, i.y);
            for o in rect_minus(r, i) {
                canvas::blit_dimmed(base, o, back, o.x, o.y);
            }
        }
        None => canvas::blit_dimmed(base, r, back, r.x, r.y),
    }
}

/// Draw `anns` on a scratch copy of the base layer and copy the result to `back`, bright inside
/// `bright` and dimmed outside (so annotations dim together with the rest of the frame).
fn draw_preview(
    back: &mut Pixmap,
    base: &Pixmap,
    bright: Option<Rect>,
    anns: &[Annotation],
    area: Rect,
    font: Option<&Font>,
) {
    let Some(b) = area.intersect(&canvas::pm_rect(base)) else {
        return;
    };
    // Common case: everything lies inside the bright selection, where `back` already holds the
    // base layer (restored just before) — draw straight onto it, in frame coordinates.
    if bright.is_some_and(|br| br.contains_rect(&b)) {
        render::render_annotations(back, anns, (0, 0), font);
        return;
    }
    let Some(mut scratch) = Pixmap::new(b.w as u32, b.h as u32) else {
        return;
    };
    canvas::blit(base, b, &mut scratch, 0, 0);
    render::render_annotations(&mut scratch, anns, (b.x, b.y), font);
    let local = |r: Rect| r.translate(-b.x, -b.y);
    match bright.and_then(|br| br.intersect(&b)) {
        Some(i) => {
            canvas::blit(&scratch, local(i), back, i.x, i.y);
            for o in rect_minus(b, i) {
                canvas::blit_dimmed(&scratch, local(o), back, o.x, o.y);
            }
        }
        None => canvas::blit_dimmed(&scratch, local(b), back, b.x, b.y),
    }
}

fn draw_elem(
    back: &mut Pixmap,
    base: &Pixmap,
    frame: &Pixmap,
    bright: Option<Rect>,
    font: Option<&Font>,
    e: &Elem,
) {
    match e {
        Elem::Preview { ann, outline } => {
            let area = render::annotation_bounds(ann, font).inflate(2);
            draw_preview(back, base, bright, std::slice::from_ref(ann), area, font);
            if let Some(r) = outline {
                canvas::stroke_rect_px(back, *r, 1, Rgba::rgb(255, 255, 255).with_alpha(200));
                canvas::stroke_rect_px(back, r.inflate(1), 1, Rgba::rgb(0, 0, 0).with_alpha(120));
            }
        }
        Elem::Edit {
            ann,
            caret,
            frame: fr,
        } => {
            let area = render::annotation_bounds(ann, font).union(fr).inflate(2);
            let drawn = match ann {
                Annotation::Text { text, .. } if text.is_empty() => &[][..],
                _ => std::slice::from_ref(ann),
            };
            draw_preview(back, base, bright, drawn, area, font);
            canvas::stroke_rect_px(back, *fr, 1, toolbar::ACCENT.with_alpha(190));
            let color = match ann {
                Annotation::Text { color, .. } => *color,
                _ => Rgba::rgb(255, 255, 255),
            };
            // Caret: contrast ring then ink.
            canvas::fill_rect_px(
                back,
                caret.inflate(1),
                Rgba::rgb(255, 255, 255).with_alpha(if color.luminance() > 0.5 { 0 } else { 90 }),
            );
            canvas::fill_rect_px(back, *caret, color);
        }
        Elem::Snap { rect, s } => chrome::draw_snap(back, *rect, *s),
        Elem::Border { rect, handles, s } => chrome::draw_border(back, *rect, *handles, *s),
        Elem::Ring { c, tool, size } => chrome::draw_ring(back, *c, *tool, *size),
        Elem::Label { rect, text, s } => {
            if let Some(f) = font {
                chrome::draw_label(back, f, *rect, text, *s);
            }
        }
        Elem::Bar { origin, st, s } => {
            let l = toolbar::layout_at(origin.x, origin.y, *s);
            toolbar::draw(back, &l, st, *s);
        }
        Elem::Loupe(d) => chrome::draw_loupe(back, frame, d, font),
    }
}

#[cfg(test)]
mod tests;
