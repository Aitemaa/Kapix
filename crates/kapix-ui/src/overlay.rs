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

use kapix_core::config::DefaultAction;
use kapix_core::{Image, Rect, VirtualDesktop};

use crate::font::Font;

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
    MouseMove { x: i32, y: i32, mods: Mods },
    MouseDown { x: i32, y: i32, button: MouseButton, mods: Mods },
    MouseUp { x: i32, y: i32, button: MouseButton, mods: Mods },
    /// `delta` in notches (+1 = one notch away from user / up).
    Wheel { x: i32, y: i32, delta: f32, mods: Mods },
    KeyDown { key: Key, mods: Mods },
    KeyUp { key: Key, mods: Mods },
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

pub struct Overlay {
    // Private state chosen by the implementer.
    _private: (),
}

impl Overlay {
    pub fn new(init: OverlayInit) -> Overlay {
        todo!()
    }
    /// Frame size (== host window client size).
    pub fn size(&self) -> (u32, u32) {
        todo!()
    }
    pub fn handle(&mut self, input: Input) -> Outcome {
        todo!()
    }
    /// Recompose the backbuffer where needed; returns the dirty rect to blit (buffer coords),
    /// or `None` if nothing changed. The first call returns the full bounds.
    /// Target: < 4 ms for a typical mouse-move at 4K (only recompose dirty areas).
    pub fn paint(&mut self) -> Option<Rect> {
        todo!()
    }
    /// Composited BGRA backbuffer, `size().0 * size().1 * 4` bytes, top-down.
    pub fn buffer(&self) -> &[u8] {
        todo!()
    }
    /// Cursor the host should show at the current pointer position.
    pub fn cursor(&self) -> CursorKind {
        todo!()
    }
    /// True while an inline text box is being edited (host should route WM_CHAR).
    pub fn is_editing_text(&self) -> bool {
        todo!()
    }
}
