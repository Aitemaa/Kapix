//! Pure-Rust Wayland screen capture via `zwlr_screencopy_manager_v1` + `zxdg_output_manager_v1`.
//!
//! Captures every connected `wl_output` into `wl_shm`, applies `YInvert` and output rotation/flip
//! transforms, converts to top-down BGRA8 (alpha forced to 255), and stitches all outputs into
//! one unified [`Image`] + [`VirtualDesktop`].

use std::ffi::CStr;

use kapix_core::{Image, Monitor, Rect, VirtualDesktop};
use wayrs_client::global::GlobalExt;
use wayrs_client::protocol::{WlOutput, WlShm, wl_output, wl_shm};
use wayrs_client::{Connection, EventCtx, IoMode};
use wayrs_protocols::wlr_screencopy_unstable_v1::{
    ZwlrScreencopyFrameV1, ZwlrScreencopyManagerV1, zwlr_screencopy_frame_v1,
};
use wayrs_protocols::xdg_output_unstable_v1::{ZxdgOutputManagerV1, ZxdgOutputV1, zxdg_output_v1};

use crate::shm::ShmBuffer;
use crate::{Error, Result};

/// Metadata for one captured Wayland output, linking Wayland logical coordinates to its
/// physical-pixel slice inside [`Capture::frame`].
#[derive(Debug, Clone)]
pub struct OutputInfo {
    /// Registry global name of the `wl_output` (used to match surfaces in the overlay host).
    pub global_name: u32,
    /// Connector name (e.g. `"eDP-1"`, `"DP-1"`).
    pub name: String,
    /// Output rectangle in Wayland logical coordinates.
    pub logical_rect: Rect,
    /// Output rectangle inside [`Capture::frame`] (physical pixels, buffer coordinates).
    pub buffer_rect: Rect,
    /// Effective scale factor (`phys_w / logical_w`), e.g. `1.0`, `1.25`, `1.5`, `2.0`.
    pub scale: f32,
}

/// Complete frozen desktop capture.
pub struct Capture {
    pub frame: Image,
    pub desktop: VirtualDesktop,
    pub outputs: Vec<OutputInfo>,
}

struct PendingOutput {
    global_name: u32,
    wl: WlOutput,
    xdg: Option<ZxdgOutputV1>,
    name: String,
    logical_x: i32,
    logical_y: i32,
    logical_w: i32,
    logical_h: i32,
    has_logical_pos: bool,
    has_logical_size: bool,
    mode_w: i32,
    mode_h: i32,
    int_scale: i32,
    transform: wl_output::Transform,
}

#[derive(Clone, Copy)]
struct FormatOffer {
    format: wl_shm::Format,
    width: u32,
    height: u32,
    stride: u32,
}

enum FrameState {
    WaitingBuffer,
    Copied { shm: ShmBuffer, y_invert: bool },
    Done { image: Image },
    Failed(String),
}

struct CaptureSlot {
    frame: ZwlrScreencopyFrameV1,
    best_offer: Option<FormatOffer>,
    state: FrameState,
    transform: wl_output::Transform,
}

struct CaptureApp {
    shm: WlShm,
    outputs: Vec<PendingOutput>,
    slots: Vec<CaptureSlot>,
}

/// Capture all connected Wayland outputs into a single top-down BGRA8 frame.
pub fn capture_desktop() -> Result<Capture> {
    let mut conn: Connection<CaptureApp> = Connection::connect().map_err(|e| {
        Error(format!(
            "cannot connect to Wayland display (is WAYLAND_DISPLAY set?): {e}"
        ))
    })?;
    conn.blocking_roundtrip()
        .map_err(|e| Error(format!("Wayland initial roundtrip failed: {e}")))?;
    let globals = conn.globals().to_vec();

    let shm: WlShm = conn
        .bind_singleton(1..=2)
        .map_err(|e| Error(format!("Wayland compositor did not advertise wl_shm: {e}")))?;

    let sc_ver = globals
        .iter()
        .find(|g| g.is::<ZwlrScreencopyManagerV1>())
        .map(|g| g.version.min(3))
        .ok_or_else(|| {
            Error(
                "compositor does not advertise zwlr_screencopy_manager_v1 (supported on Niri, Hyprland, Sway, River, LabWC)"
                    .into(),
            )
        })?;
    let screencopy_mgr: ZwlrScreencopyManagerV1 = conn
        .bind_singleton(1..=3)
        .map_err(|e| Error(format!("failed to bind zwlr_screencopy_manager_v1: {e}")))?;
    let xdg_output_mgr: Option<ZxdgOutputManagerV1> = conn.bind_singleton(1..=3).ok();

    let mut outputs = Vec::new();
    for g in &globals {
        if g.is::<WlOutput>() {
            let idx = outputs.len();
            if let Ok(wl) = g.bind_with_cb::<WlOutput, _, _>(&mut conn, 1..=4, move |ctx| {
                wl_output_cb(ctx, idx);
            }) {
                outputs.push(PendingOutput {
                    global_name: g.name,
                    wl,
                    xdg: None,
                    name: format!("output-{idx}"),
                    logical_x: 0,
                    logical_y: 0,
                    logical_w: 0,
                    logical_h: 0,
                    has_logical_pos: false,
                    has_logical_size: false,
                    mode_w: 0,
                    mode_h: 0,
                    int_scale: 1,
                    transform: wl_output::Transform::Normal,
                });
            }
        }
    }

    if outputs.is_empty() {
        return Err(Error("no Wayland outputs found".into()));
    }

    if let Some(mgr) = xdg_output_mgr {
        for (i, out) in outputs.iter_mut().enumerate() {
            let xdg = mgr.get_xdg_output_with_cb(&mut conn, out.wl, move |ctx| {
                xdg_output_cb(ctx, i);
            });
            out.xdg = Some(xdg);
        }
    }

    let mut state = CaptureApp {
        shm,
        outputs,
        slots: Vec::new(),
    };

    // Roundtrip to receive wl_output + zxdg_output_v1 geometry & names.
    conn.blocking_roundtrip()
        .map_err(|e| Error(format!("Wayland output info roundtrip failed: {e}")))?;
    conn.dispatch_events(&mut state);

    // Start screencopy on every output concurrently (overlay_cursor = 0).
    for i in 0..state.outputs.len() {
        let wl = state.outputs[i].wl;
        let transform = state.outputs[i].transform;
        let frame = screencopy_mgr.capture_output_with_cb(&mut conn, 0, wl, move |ctx| {
            screencopy_frame_cb(ctx, i, sc_ver);
        });
        state.slots.push(CaptureSlot {
            frame,
            best_offer: None,
            state: FrameState::WaitingBuffer,
            transform,
        });
    }

    conn.flush(IoMode::Blocking)
        .map_err(|e| Error(format!("Wayland flush failed: {e}")))?;

    // Pump events until every output finishes or fails.
    while state
        .slots
        .iter()
        .any(|s| !matches!(s.state, FrameState::Done { .. } | FrameState::Failed(_)))
    {
        conn.recv_events(IoMode::Blocking)
            .map_err(|e| Error(format!("Wayland event receive failed: {e}")))?;
        conn.dispatch_events(&mut state);
        conn.flush(IoMode::Blocking)
            .map_err(|e| Error(format!("Wayland flush failed: {e}")))?;
    }

    // Clean up protocol objects.
    for slot in &state.slots {
        slot.frame.destroy(&mut conn);
    }
    for out in &state.outputs {
        if let Some(xdg) = out.xdg {
            xdg.destroy(&mut conn);
        }
    }
    let _ = conn.flush(IoMode::NonBlocking);

    // Collect captured images per output.
    let mut captured: Vec<(PendingOutput, Image)> = Vec::with_capacity(state.outputs.len());
    for (out, slot) in state.outputs.into_iter().zip(state.slots) {
        match slot.state {
            FrameState::Done { image } => captured.push((out, image)),
            FrameState::Failed(msg) => return Err(Error(msg)),
            _ => return Err(Error("screen capture did not complete".into())),
        }
    }

    Ok(stitch_outputs(captured))
}

fn wl_output_cb(ctx: EventCtx<CaptureApp, WlOutput>, idx: usize) {
    let Some(out) = ctx.state.outputs.get_mut(idx) else {
        return;
    };
    match ctx.event {
        wl_output::Event::Geometry(g) => {
            if !out.has_logical_pos {
                out.logical_x = g.x;
                out.logical_y = g.y;
            }
            out.transform = g.transform;
        }
        wl_output::Event::Mode(m) => {
            if m.flags.contains(wl_output::Mode::Current) {
                out.mode_w = m.width;
                out.mode_h = m.height;
            }
        }
        wl_output::Event::Scale(factor) => {
            out.int_scale = factor.max(1);
        }
        wl_output::Event::Name(name) => {
            if let Ok(s) = CStr::from_bytes_with_nul(name.to_bytes_with_nul()).and_then(|c| {
                c.to_str()
                    .map_err(|_| std::ffi::FromBytesWithNulError::NotNulTerminated)
            }) {
                out.name = s.to_string();
            } else {
                out.name = name.to_string_lossy().into_owned();
            }
        }
        _ => {}
    }
}

fn xdg_output_cb(ctx: EventCtx<CaptureApp, ZxdgOutputV1>, idx: usize) {
    let Some(out) = ctx.state.outputs.get_mut(idx) else {
        return;
    };
    match ctx.event {
        zxdg_output_v1::Event::LogicalPosition(p) => {
            out.logical_x = p.x;
            out.logical_y = p.y;
            out.has_logical_pos = true;
        }
        zxdg_output_v1::Event::LogicalSize(s) => {
            out.logical_w = s.width;
            out.logical_h = s.height;
            out.has_logical_size = true;
        }
        zxdg_output_v1::Event::Name(name) => {
            out.name = name.to_string_lossy().into_owned();
        }
        _ => {}
    }
}

fn format_priority(fmt: wl_shm::Format) -> u8 {
    match fmt {
        wl_shm::Format::Xrgb8888 => 5,
        wl_shm::Format::Argb8888 => 4,
        wl_shm::Format::Xbgr8888 => 3,
        wl_shm::Format::Abgr8888 => 2,
        wl_shm::Format::Bgr888 | wl_shm::Format::Rgb888 => 1,
        _ => 0,
    }
}

fn screencopy_frame_cb(ctx: EventCtx<CaptureApp, ZwlrScreencopyFrameV1>, idx: usize, sc_ver: u32) {
    let shm = ctx.state.shm;
    let Some(slot) = ctx.state.slots.get_mut(idx) else {
        return;
    };
    match ctx.event {
        zwlr_screencopy_frame_v1::Event::Buffer(b) => {
            let prio = format_priority(b.format);
            if prio == 0 {
                return;
            }
            let replace = slot
                .best_offer
                .is_none_or(|cur| prio > format_priority(cur.format));
            if replace {
                slot.best_offer = Some(FormatOffer {
                    format: b.format,
                    width: b.width,
                    height: b.height,
                    stride: b.stride,
                });
            }
            // Protocol versions 1 and 2 have no `buffer_done`; submit immediately.
            if sc_ver < 3 && matches!(slot.state, FrameState::WaitingBuffer) {
                submit_copy(ctx.conn, shm, slot);
            }
        }
        zwlr_screencopy_frame_v1::Event::BufferDone => {
            if matches!(slot.state, FrameState::WaitingBuffer) {
                submit_copy(ctx.conn, shm, slot);
            }
        }
        zwlr_screencopy_frame_v1::Event::Flags(flags) => {
            if let FrameState::Copied { y_invert, .. } = &mut slot.state {
                *y_invert = flags.contains(zwlr_screencopy_frame_v1::Flags::YInvert);
            }
        }
        zwlr_screencopy_frame_v1::Event::Ready(_) => {
            let prev = std::mem::replace(&mut slot.state, FrameState::WaitingBuffer);
            if let FrameState::Copied { shm: buf, y_invert } = prev {
                let res = decode_shm_frame(&buf, y_invert, slot.transform);
                buf.destroy_wl(ctx.conn);
                slot.state = match res {
                    Ok(image) => FrameState::Done { image },
                    Err(e) => FrameState::Failed(e.0),
                };
            } else {
                slot.state = FrameState::Failed("unexpected screencopy ready state".into());
            }
        }
        zwlr_screencopy_frame_v1::Event::Failed => {
            let prev = std::mem::replace(&mut slot.state, FrameState::WaitingBuffer);
            if let FrameState::Copied { shm: buf, .. } = prev {
                buf.destroy_wl(ctx.conn);
            }
            slot.state = FrameState::Failed("compositor rejected screencopy frame".into());
        }
        _ => {}
    }
}

fn submit_copy(conn: &mut Connection<CaptureApp>, shm: WlShm, slot: &mut CaptureSlot) {
    let Some(offer) = slot.best_offer else {
        slot.state = FrameState::Failed("compositor offered no supported wl_shm format".into());
        return;
    };
    match ShmBuffer::new(
        conn,
        shm,
        offer.width,
        offer.height,
        offer.stride,
        offer.format,
    ) {
        Ok(buf) => {
            slot.frame.copy(conn, buf.buffer);
            slot.state = FrameState::Copied {
                shm: buf,
                y_invert: false,
            };
        }
        Err(e) => {
            slot.state = FrameState::Failed(e.0);
        }
    }
}

/// Convert a raw screencopy SHM buffer to top-down BGRA8 (alpha = 255), applying `y_invert`
/// and `wl_output::Transform`.
fn decode_shm_frame(
    buf: &ShmBuffer,
    y_invert: bool,
    transform: wl_output::Transform,
) -> Result<Image> {
    let w = buf.width as usize;
    let h = buf.height as usize;
    let stride = buf.stride as usize;
    let src = buf.bytes();

    if w == 0 || h == 0 {
        return Err(Error("captured output has zero dimensions".into()));
    }
    if src.len() < stride * h {
        return Err(Error("captured SHM buffer truncated".into()));
    }

    // Step 1: decode into tightly packed top-down BGRA8 in buffer orientation (undoing YInvert).
    let mut bgra = vec![0u8; w * h * 4];
    for y in 0..h {
        let src_y = if y_invert { h - 1 - y } else { y };
        let row = &src[src_y * stride..src_y * stride + stride];
        let dst_row = &mut bgra[y * w * 4..(y + 1) * w * 4];
        match buf.format {
            wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => {
                dst_row.copy_from_slice(&row[..w * 4]);
                for px in dst_row.as_chunks_mut::<4>().0 {
                    px[3] = 255;
                }
            }
            wl_shm::Format::Abgr8888 | wl_shm::Format::Xbgr8888 => {
                for (d, s) in dst_row
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(row[..w * 4].as_chunks::<4>().0)
                {
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = 255;
                }
            }
            wl_shm::Format::Bgr888 => {
                for (d, s) in dst_row
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(row[..w * 3].as_chunks::<3>().0)
                {
                    d[0] = s[0];
                    d[1] = s[1];
                    d[2] = s[2];
                    d[3] = 255;
                }
            }
            wl_shm::Format::Rgb888 => {
                for (d, s) in dst_row
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(row[..w * 3].as_chunks::<3>().0)
                {
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = 255;
                }
            }
            other => {
                return Err(Error(format!("unsupported screencopy format {other:?}")));
            }
        }
    }

    // Step 2: apply wl_output::Transform so the resulting image matches display orientation.
    let img = Image::from_bgra(buf.width, buf.height, bgra);
    Ok(apply_output_transform(img, transform))
}

fn apply_output_transform(img: Image, transform: wl_output::Transform) -> Image {
    use wl_output::Transform as T;
    if matches!(transform, T::Normal) {
        return img;
    }
    let (sw, sh) = (img.width as usize, img.height as usize);
    let swapped = matches!(transform, T::_90 | T::_270 | T::Flipped90 | T::Flipped270);
    let (dw, dh) = if swapped { (sh, sw) } else { (sw, sh) };
    let mut out = vec![0u8; dw * dh * 4];

    for dy in 0..dh {
        for dx in 0..dw {
            // Wayland wl_output::transform is counter-clockwise; invert to map display -> buffer.
            let (sx, sy) = match transform {
                T::Normal => (dx, dy),
                T::_90 => (sw - 1 - dy, dx),
                T::_180 => (sw - 1 - dx, sh - 1 - dy),
                T::_270 => (dy, sh - 1 - dx),
                T::Flipped => (sw - 1 - dx, dy),
                T::Flipped90 => (dy, dx),
                T::Flipped180 => (dx, sh - 1 - dy),
                T::Flipped270 => (sw - 1 - dy, sh - 1 - dx),
                _ => (dx, dy),
            };
            let si = (sy * sw + sx) * 4;
            let di = (dy * dw + dx) * 4;
            out[di..di + 4].copy_from_slice(&img.data[si..si + 4]);
        }
    }
    Image::from_bgra(dw as u32, dh as u32, out)
}

/// Stitch one or more captured outputs into a single unified [`Capture`].
fn stitch_outputs(items: Vec<(PendingOutput, Image)>) -> Capture {
    // Fast path for a single output: zero extra copy!
    if items.len() == 1 {
        let (out, img) = items.into_iter().next().unwrap();
        let (pw, ph) = (img.width as i32, img.height as i32);
        let lw = if out.has_logical_size && out.logical_w > 0 {
            out.logical_w
        } else {
            (pw / out.int_scale.max(1)).max(1)
        };
        let lh = if out.has_logical_size && out.logical_h > 0 {
            out.logical_h
        } else {
            (ph / out.int_scale.max(1)).max(1)
        };
        let scale = (pw as f32 / lw as f32).max(0.5);
        let logical_rect = Rect::new(out.logical_x, out.logical_y, lw, lh);
        let buffer_rect = Rect::new(0, 0, pw, ph);
        let desktop = VirtualDesktop {
            bounds: buffer_rect,
            monitors: vec![Monitor {
                rect: buffer_rect,
                work: buffer_rect,
                scale,
                primary: true,
            }],
        };
        let outputs = vec![OutputInfo {
            global_name: out.global_name,
            name: out.name,
            logical_rect,
            buffer_rect,
            scale,
        }];
        return Capture {
            frame: img,
            desktop,
            outputs,
        };
    }

    // Multi-output: compute physical placement from logical layout using the max scale so
    // touching monitors remain contiguous without overlap.
    struct Placed {
        global_name: u32,
        name: String,
        logical_rect: Rect,
        phys_rect: Rect,
        scale: f32,
        img: Image,
    }

    let mut max_scale = 1.0f32;
    let mut temp = Vec::with_capacity(items.len());
    for (out, img) in items {
        let (pw, ph) = (img.width as i32, img.height as i32);
        let lw = if out.has_logical_size && out.logical_w > 0 {
            out.logical_w
        } else {
            (pw / out.int_scale.max(1)).max(1)
        };
        let lh = if out.has_logical_size && out.logical_h > 0 {
            out.logical_h
        } else {
            (ph / out.int_scale.max(1)).max(1)
        };
        let scale = (pw as f32 / lw as f32).max(0.5);
        max_scale = max_scale.max(scale);
        let logical_rect = Rect::new(out.logical_x, out.logical_y, lw, lh);
        temp.push((out, img, logical_rect, scale));
    }

    let mut placed: Vec<Placed> = Vec::with_capacity(temp.len());
    for (out, img, logical_rect, scale) in temp {
        let px = (logical_rect.x as f32 * max_scale).round() as i32;
        let py = (logical_rect.y as f32 * max_scale).round() as i32;
        let phys_rect = Rect::new(px, py, img.width as i32, img.height as i32);
        placed.push(Placed {
            global_name: out.global_name,
            name: out.name,
            logical_rect,
            phys_rect,
            scale,
            img,
        });
    }

    let bounds = placed
        .iter()
        .fold(Rect::default(), |acc, p| acc.union(&p.phys_rect));
    let total_w = bounds.w.max(1) as u32;
    let total_h = bounds.h.max(1) as u32;
    let mut frame = Image::new(total_w, total_h);

    let mut monitors = Vec::with_capacity(placed.len());
    let mut outputs = Vec::with_capacity(placed.len());
    let dst_stride = total_w as usize * 4;

    for (idx, p) in placed.into_iter().enumerate() {
        let bx = (p.phys_rect.x - bounds.x).max(0);
        let by = (p.phys_rect.y - bounds.y).max(0);
        let buf_rect = Rect::new(bx, by, p.img.width as i32, p.img.height as i32);
        let src_stride = p.img.width as usize * 4;
        for row in 0..p.img.height as usize {
            let dst_off = (by as usize + row) * dst_stride + bx as usize * 4;
            let src_off = row * src_stride;
            frame.data[dst_off..dst_off + src_stride]
                .copy_from_slice(&p.img.data[src_off..src_off + src_stride]);
        }
        monitors.push(Monitor {
            rect: buf_rect,
            work: buf_rect,
            scale: p.scale,
            primary: idx == 0,
        });
        outputs.push(OutputInfo {
            global_name: p.global_name,
            name: p.name,
            logical_rect: p.logical_rect,
            buffer_rect: buf_rect,
            scale: p.scale,
        });
    }

    let desktop = VirtualDesktop {
        bounds: Rect::new(0, 0, total_w as i32, total_h as i32),
        monitors,
    };

    Capture {
        frame,
        desktop,
        outputs,
    }
}

/// Map a logical rectangle on a specific output (or in global logical space) to physical buffer
/// coordinates inside [`Capture::frame`].
pub fn logical_rect_to_buffer(outputs: &[OutputInfo], logical: Rect) -> Option<Rect> {
    let center = kapix_core::Point::new(logical.x + logical.w / 2, logical.y + logical.h / 2);
    let out = outputs
        .iter()
        .find(|o| o.logical_rect.contains(center))
        .or_else(|| outputs.first())?;
    let sx = out.buffer_rect.w as f32 / out.logical_rect.w.max(1) as f32;
    let sy = out.buffer_rect.h as f32 / out.logical_rect.h.max(1) as f32;
    let x = out.buffer_rect.x + ((logical.x - out.logical_rect.x) as f32 * sx).round() as i32;
    let y = out.buffer_rect.y + ((logical.y - out.logical_rect.y) as f32 * sy).round() as i32;
    let w = (logical.w as f32 * sx).round().max(1.0) as i32;
    let h = (logical.h as f32 * sy).round().max(1.0) as i32;
    Some(Rect::new(x, y, w, h))
}
