//! Screen capture: DXGI Desktop Duplication with GDI BitBlt fallback.

use kapix_core::{Image, Monitor, Rect, VirtualDesktop};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Dxgi,
    Gdi,
}

pub struct Capture {
    pub frame: Image,
    pub desktop: VirtualDesktop,
    pub backend: Backend,
}

/// Enumerate monitors (`EnumDisplayMonitors` + `GetMonitorInfoW` + `GetDpiForMonitor`).
pub fn desktop_layout() -> crate::Result<VirtualDesktop> {
    todo!()
}

/// Capture the full virtual desktop into one BGRA frame (buffer (0,0) = virtual bounds
/// top-left; gaps between monitors are black; alpha forced to 255).
///
/// Tries DXGI Desktop Duplication per output (`DuplicateOutput` → `AcquireNextFrame` with a
/// short timeout → staging texture → copy rows), handling rotation (DXGI_MODE_ROTATION) and HDR
/// (`R16G16B16A16_FLOAT` via `DuplicateOutput1` → tone-map scRGB to sRGB using the SDR white
/// level). Any failure (RDP, ACCESS_LOST, DEVICE_REMOVED, no frame within timeout) falls back to
/// GDI `BitBlt(SRCCOPY | CAPTUREBLT)` of the whole virtual screen into a top-down DIB section.
/// Must not keep D3D devices alive after returning (idle RAM budget).
pub fn capture_desktop() -> crate::Result<Capture> {
    todo!()
}

/// Force the GDI path (used by the CLI `--gdi` flag and as fallback).
pub fn capture_desktop_gdi() -> crate::Result<Capture> {
    todo!()
}

/// Rect of the foreground window in **virtual** coordinates
/// (`DWMWA_EXTENDED_FRAME_BOUNDS`, falling back to `GetWindowRect`), or `None`.
pub fn foreground_window_rect() -> Option<Rect> {
    todo!()
}

/// Monitor under the cursor (virtual coords), for "current monitor" fullscreen capture.
pub fn monitor_under_cursor(desktop: &VirtualDesktop) -> Option<Monitor> {
    todo!()
}
