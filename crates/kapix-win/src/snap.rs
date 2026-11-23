//! Window snap candidates for the overlay.

use kapix_core::Rect;

/// Visible top-level windows in z-order (top-most first), **virtual** coordinates, using
/// `DWMWA_EXTENDED_FRAME_BOUNDS` (excludes invisible shadow margins). Excludes: invisible,
/// minimized/iconic, DWM-cloaked (`DWMWA_CLOAKED`, e.g. other virtual desktops / suspended UWP),
/// zero-area, tool windows with no title that cover the whole desktop (shell/wallpaper
/// `Progman`/`WorkerW`), and windows belonging to `exclude_pid` (Kapix itself).
/// Child-control snapping is out of scope.
pub fn window_rects(exclude_pid: u32) -> Vec<Rect> {
    todo!()
}
