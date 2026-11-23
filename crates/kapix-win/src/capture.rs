//! Screen capture: DXGI Desktop Duplication with GDI BitBlt fallback.

use crate::Error;
use kapix_core::{Image, Monitor, Point, Rect, VirtualDesktop};
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_DEVICE_INFO_TYPE, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SDR_WHITE_LEVEL, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
    QueryDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, HMODULE, LPARAM, LUID, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT,
    DXGI_MODE_ROTATION_ROTATE90, DXGI_MODE_ROTATION_ROTATE180, DXGI_MODE_ROTATION_ROTATE270,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter,
    IDXGIFactory1, IDXGIOutput, IDXGIOutput1, IDXGIOutput5, IDXGIOutputDuplication, IDXGIResource,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, EnumDisplayMonitors, GdiFlush, GetDC, GetMonitorInfoW,
    HDC, HGDIOBJ, HMONITOR, MONITORINFO, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetSystemMetrics, IsIconic, MONITORINFOF_PRIMARY,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};
use windows::core::{BOOL, Interface};

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

unsafe extern "system" fn collect_monitor(
    h: HMONITOR,
    _: HDC,
    _: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    // SAFETY: `lparam` is the `&mut Vec<HMONITOR>` passed by `desktop_layout`.
    unsafe { (*(lparam.0 as *mut Vec<HMONITOR>)).push(h) };
    true.into()
}

fn rect_of(r: RECT) -> Rect {
    Rect::from_ltrb(r.left, r.top, r.right, r.bottom)
}

/// Enumerate monitors (`EnumDisplayMonitors` + `GetMonitorInfoW` + `GetDpiForMonitor`).
pub fn desktop_layout() -> crate::Result<VirtualDesktop> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only pushes into `handles`, which outlives the synchronous call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM((&raw mut handles) as isize),
        );
    }
    let mut monitors = Vec::with_capacity(handles.len());
    for h in handles {
        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `mi.cbSize` is set; `h` came from EnumDisplayMonitors.
        if !unsafe { GetMonitorInfoW(h, &mut mi) }.as_bool() {
            continue;
        }
        let (mut dx, mut dy) = (96u32, 96u32);
        // SAFETY: valid out pointers. On failure the 96 dpi default stands.
        unsafe {
            let _ = GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        }
        monitors.push(Monitor {
            rect: rect_of(mi.rcMonitor),
            work: rect_of(mi.rcWork),
            scale: dx.max(1) as f32 / 96.0,
            primary: mi.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    let first = monitors
        .first()
        .ok_or_else(|| Error("no display monitors found".into()))?;
    // SAFETY: plain metric queries.
    let mut bounds = unsafe {
        Rect::new(
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    if bounds.is_empty() {
        bounds = monitors.iter().fold(first.rect, |a, m| a.union(&m.rect));
    }
    Ok(VirtualDesktop { bounds, monitors })
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
    let desktop = desktop_layout()?;
    match capture_dxgi(&desktop) {
        Ok(frame) => Ok(Capture {
            frame,
            desktop,
            backend: Backend::Dxgi,
        }),
        Err(_) => capture_desktop_gdi(),
    }
}

/// Force the GDI path (used by the CLI `--gdi` flag and as fallback).
pub fn capture_desktop_gdi() -> crate::Result<Capture> {
    let desktop = desktop_layout()?;
    let b = desktop.bounds;
    let (w, h) = (b.w as usize, b.h as usize);
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: b.w,
            biHeight: -b.h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut data = Vec::new();
    // SAFETY: standard GDI screen-grab sequence; every handle is released on all paths below
    // and `bits` points to a w*h*4 byte DIB section that is only read before DeleteObject.
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err(Error("GetDC failed".into()));
        }
        let mem = CreateCompatibleDC(Some(screen));
        let mut bits = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(screen), &bmi, DIB_RGB_COLORS, &mut bits, None, 0);
        let res = match &dib {
            Ok(bmp) if !mem.is_invalid() && !bits.is_null() => {
                let old = SelectObject(mem, HGDIOBJ(bmp.0));
                let r = BitBlt(
                    mem,
                    0,
                    0,
                    b.w,
                    b.h,
                    Some(screen),
                    b.x,
                    b.y,
                    SRCCOPY | CAPTUREBLT,
                );
                let _ = GdiFlush();
                data = std::slice::from_raw_parts(bits.cast::<u8>(), w * h * 4).to_vec();
                SelectObject(mem, old);
                r.map_err(Error::from)
            }
            Ok(_) => Err(Error("GDI setup failed".into())),
            Err(e) => Err(e.clone().into()),
        };
        if let Ok(bmp) = dib {
            let _ = DeleteObject(HGDIOBJ(bmp.0));
        }
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        res?;
    }
    for px in data.as_chunks_mut::<4>().0 {
        px[3] = 255;
    }
    Ok(Capture {
        frame: Image::from_bgra(b.w as u32, b.h as u32, data),
        desktop,
        backend: Backend::Gdi,
    })
}

/// Rect of the foreground window in **virtual** coordinates
/// (`DWMWA_EXTENDED_FRAME_BOUNDS`, falling back to `GetWindowRect`), or `None`.
pub fn foreground_window_rect() -> Option<Rect> {
    // SAFETY: plain queries on the foreground window handle.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() || unsafe { IsIconic(hwnd) }.as_bool() {
        return None;
    }
    crate::snap::frame_rect(hwnd).filter(|r| !r.is_empty())
}

/// Monitor under the cursor (virtual coords), for "current monitor" fullscreen capture.
pub fn monitor_under_cursor(desktop: &VirtualDesktop) -> Option<Monitor> {
    let mut p = windows::Win32::Foundation::POINT::default();
    // SAFETY: valid out pointer.
    unsafe { GetCursorPos(&mut p) }.ok()?;
    desktop.monitor_at(Point::new(p.x, p.y)).cloned()
}

// ---------------------------------------------------------------------------------------------
// DXGI Desktop Duplication
// ---------------------------------------------------------------------------------------------

/// Duplicate every attached output of every adapter into one canvas. Everything D3D/DXGI is
/// dropped before this returns.
fn capture_dxgi(desktop: &VirtualDesktop) -> crate::Result<Image> {
    let b = desktop.bounds;
    let mut canvas = Image::new(b.w as u32, b.h as u32);
    let mut captured = 0usize;
    // SAFETY: COM calls on interfaces owned by this function; all raw buffers are sized from
    // the descriptors the API returned (see `grab_output`).
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut a = 0;
        while let Ok(adapter) = factory.EnumAdapters1(a) {
            a += 1;
            let adapter: IDXGIAdapter = adapter.cast()?;
            let (mut dev, mut ctx) = (None, None);
            if D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut dev),
                None,
                Some(&mut ctx),
            )
            .is_err()
            {
                continue; // e.g. software adapter without a usable device; outputs counted below
            }
            let (dev, ctx) = dev
                .zip(ctx)
                .ok_or_else(|| Error("no D3D11 device".into()))?;
            let mut o = 0;
            while let Ok(output) = adapter.EnumOutputs(o) {
                o += 1;
                if !output.GetDesc()?.AttachedToDesktop.as_bool() {
                    continue;
                }
                grab_output(&dev, &ctx, &output, b, &mut canvas)?;
                captured += 1;
            }
        }
    }
    if captured == 0 || captured < desktop.monitors.len() {
        return Err(Error("DXGI did not cover all monitors".into()));
    }
    Ok(canvas)
}

/// Acquire one full frame from `output` and write it (rotated/tone-mapped as needed) into `canvas`.
unsafe fn grab_output(
    dev: &ID3D11Device,
    ctx: &ID3D11DeviceContext,
    output: &IDXGIOutput,
    bounds: Rect,
    canvas: &mut Image,
) -> crate::Result<()> {
    // SAFETY (whole fn): COM calls on live interfaces; the mapped pointer is only read between
    // Map and Unmap, within `RowPitch * height` bytes as guaranteed by D3D11.
    unsafe {
        let od = output.GetDesc()?;
        let dup: IDXGIOutputDuplication = match output.cast::<IDXGIOutput5>() {
            Ok(o5) => o5
                .DuplicateOutput1(
                    dev,
                    0,
                    &[DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT],
                )
                .or_else(|_| output.cast::<IDXGIOutput1>()?.DuplicateOutput(dev))?,
            Err(_) => output.cast::<IDXGIOutput1>()?.DuplicateOutput(dev)?,
        };
        let rot = match dup.GetDesc().Rotation {
            DXGI_MODE_ROTATION_ROTATE90 => 1,
            DXGI_MODE_ROTATION_ROTATE180 => 2,
            DXGI_MODE_ROTATION_ROTATE270 => 3,
            _ => 0,
        };

        // The first frame is normally a full image; retry past timeouts / pointer-only frames.
        let mut frame: Option<ID3D11Texture2D> = None;
        for _ in 0..4 {
            let (mut info, mut res): (DXGI_OUTDUPL_FRAME_INFO, Option<IDXGIResource>) =
                Default::default();
            match dup.AcquireNextFrame(100, &mut info, &mut res) {
                Ok(()) => {
                    if info.LastPresentTime != 0 {
                        frame = res.and_then(|r| r.cast().ok());
                        break;
                    }
                    let _ = dup.ReleaseFrame();
                }
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {}
                Err(e) => return Err(e.into()),
            }
        }
        let frame = frame.ok_or_else(|| Error("no desktop frame".into()))?;

        let mut desc = D3D11_TEXTURE2D_DESC::default();
        frame.GetDesc(&mut desc);
        let staging_desc = D3D11_TEXTURE2D_DESC {
            MipLevels: 1,
            ArraySize: 1,
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
            ..desc
        };
        let mut staging = None;
        let created = dev.CreateTexture2D(&staging_desc, None, Some(&mut staging));
        if created.is_ok()
            && let Some(s) = &staging
        {
            ctx.CopyResource(s, &frame);
        }
        let _ = dup.ReleaseFrame();
        created?;
        let staging = staging.ok_or_else(|| Error("no staging texture".into()))?;

        // Placement of this output inside the canvas.
        let dc = rect_of(od.DesktopCoordinates);
        let (tw, th) = (desc.Width as usize, desc.Height as usize);
        let (dw, dh) = if rot & 1 == 1 { (th, tw) } else { (tw, th) };
        let (ox, oy) = (dc.x - bounds.x, dc.y - bounds.y);
        if (dw, dh) != (dc.w as usize, dc.h as usize)
            || ox < 0
            || oy < 0
            || ox as usize + dw > canvas.width as usize
            || oy as usize + dh > canvas.height as usize
        {
            return Err(Error("output geometry mismatch".into()));
        }
        let hdr = match desc.Format {
            DXGI_FORMAT_B8G8R8A8_UNORM => false,
            DXGI_FORMAT_R16G16B16A16_FLOAT => true,
            DXGI_FORMAT(f) => return Err(Error(format!("unsupported duplication format {f}"))),
        };
        let scale = if hdr { sdr_scale(&od.DeviceName) } else { 1.0 };
        let lut = if hdr { srgb_lut() } else { Vec::new() };

        let mut m = D3D11_MAPPED_SUBRESOURCE::default();
        ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut m))?;
        let bpp = if hdr { 8 } else { 4 };
        let mut row = vec![0u8; tw * 4];
        let cstride = canvas.width as usize * 4;
        for y in 0..th {
            let src = std::slice::from_raw_parts(
                m.pData.cast::<u8>().add(y * m.RowPitch as usize),
                tw * bpp,
            );
            if hdr {
                f16_row_to_bgra(src, scale, &lut, &mut row);
            } else {
                bgra_row_opaque(src, &mut row);
            }
            place_row(
                &row,
                y,
                tw,
                th,
                rot,
                &mut canvas.data,
                cstride,
                ox as usize,
                oy as usize,
            );
        }
        ctx.Unmap(&staging, 0);
    }
    Ok(())
}

/// Scale factor taking scRGB (1.0 = 80 nits) to SDR-white-relative linear light, from
/// `DISPLAYCONFIG_SDR_WHITE_LEVEL` (1000 = 80 nits) of the target driving `device_name`.
/// `1.0` when unavailable.
fn sdr_scale(device_name: &[u16; 32]) -> f32 {
    fn header(
        ty: DISPLAYCONFIG_DEVICE_INFO_TYPE,
        size: usize,
        adapter: LUID,
        id: u32,
    ) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
        DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: ty,
            size: size as u32,
            adapterId: adapter,
            id,
        }
    }
    let nul = |n: &[u16; 32]| n.iter().position(|&c| c == 0).unwrap_or(32);
    // SAFETY: buffers are sized from GetDisplayConfigBufferSizes; request structs are fully
    // initialized with correct header sizes.
    unsafe {
        let (mut np, mut nm) = (0u32, 0u32);
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) != ERROR_SUCCESS {
            return 1.0;
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
        if QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut np,
            paths.as_mut_ptr(),
            &mut nm,
            modes.as_mut_ptr(),
            None,
        ) != ERROR_SUCCESS
        {
            return 1.0;
        }
        for p in &paths[..np as usize] {
            let mut src = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: header(
                    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>(),
                    p.sourceInfo.adapterId,
                    p.sourceInfo.id,
                ),
                ..Default::default()
            };
            if DisplayConfigGetDeviceInfo(&mut src.header) != 0
                || src.viewGdiDeviceName[..nul(&src.viewGdiDeviceName)]
                    != device_name[..nul(device_name)]
            {
                continue;
            }
            let mut w = DISPLAYCONFIG_SDR_WHITE_LEVEL {
                header: header(
                    DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                    size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>(),
                    p.targetInfo.adapterId,
                    p.targetInfo.id,
                ),
                ..Default::default()
            };
            if DisplayConfigGetDeviceInfo(&mut w.header) == 0 && w.SDRWhiteLevel > 0 {
                return 1000.0 / w.SDRWhiteLevel as f32;
            }
        }
    }
    1.0
}

// ---------------------------------------------------------------------------------------------
// Pure pixel helpers (no OS calls)
// ---------------------------------------------------------------------------------------------

/// Linear-light `[0, 1]` (4096 steps) → sRGB-encoded byte.
fn srgb_lut() -> Vec<u8> {
    (0..4096)
        .map(|i| {
            let l = i as f32 / 4095.0;
            let e = if l <= 0.003_130_8 {
                l * 12.92
            } else {
                1.055 * l.powf(1.0 / 2.4) - 0.055
            };
            (e * 255.0 + 0.5) as u8
        })
        .collect()
}

/// IEEE half → f32 (normals, subnormals, inf/nan) without a lookup table.
fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h & 0x8000) as u32) << 16;
    if h & 0x7C00 == 0x7C00 {
        // Inf / NaN: max exponent, mantissa carried over.
        return f32::from_bits(sign | 0x7F80_0000 | (((h & 0x03FF) as u32) << 13));
    }
    // Normals and subnormals: shift into place and rebias by 2^112.
    let mag = f32::from_bits(((h & 0x7FFF) as u32) << 13) * f32::from_bits(0x7780_0000);
    f32::from_bits(sign | mag.to_bits())
}

/// scRGB `R16G16B16A16_FLOAT` row → opaque BGRA8: scale to SDR white, clamp (a clip is the
/// tone curve: SDR content maps 1:1, HDR highlights above SDR white saturate), sRGB-encode.
fn f16_row_to_bgra(src: &[u8], scale: f32, lut: &[u8], out: &mut [u8]) {
    let enc = |lo: u8, hi: u8| {
        let v = (f16_to_f32(u16::from_le_bytes([lo, hi])) * scale).clamp(0.0, 1.0);
        lut[(v * 4095.0 + 0.5) as usize & 4095]
    };
    for (s, d) in src
        .as_chunks::<8>()
        .0
        .iter()
        .zip(out.as_chunks_mut::<4>().0)
    {
        d[0] = enc(s[4], s[5]);
        d[1] = enc(s[2], s[3]);
        d[2] = enc(s[0], s[1]);
        d[3] = 255;
    }
}

/// BGRA8 row with alpha forced to 255.
fn bgra_row_opaque(src: &[u8], out: &mut [u8]) {
    for (s, d) in src
        .as_chunks::<4>()
        .0
        .iter()
        .zip(out.as_chunks_mut::<4>().0)
    {
        *d = [s[0], s[1], s[2], 255];
    }
}

/// Write row `ty` of a `tw`x`th` source image into `canvas` at (`ox`, `oy`), rotating the
/// source by `rot` quarter turns clockwise (DXGI_MODE_ROTATION_ROTATE90 = 1, 180 = 2, 270 = 3).
#[allow(clippy::too_many_arguments)]
fn place_row(
    row: &[u8],
    ty: usize,
    tw: usize,
    th: usize,
    rot: u8,
    canvas: &mut [u8],
    cstride: usize,
    ox: usize,
    oy: usize,
) {
    let at = |x: usize, y: usize| (oy + y) * cstride + (ox + x) * 4;
    match rot {
        0 => {
            let i = at(0, ty);
            canvas[i..i + tw * 4].copy_from_slice(row);
        }
        2 => {
            for (x, px) in row.as_chunks::<4>().0.iter().enumerate() {
                let i = at(tw - 1 - x, th - 1 - ty);
                canvas[i..i + 4].copy_from_slice(px);
            }
        }
        1 => {
            for (x, px) in row.as_chunks::<4>().0.iter().enumerate() {
                let i = at(th - 1 - ty, x);
                canvas[i..i + 4].copy_from_slice(px);
            }
        }
        _ => {
            for (x, px) in row.as_chunks::<4>().0.iter().enumerate() {
                let i = at(ty, tw - 1 - x);
                canvas[i..i + 4].copy_from_slice(px);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats() {
        assert_eq!(f16_to_f32(0x3C00), 1.0);
        assert_eq!(f16_to_f32(0xC000), -2.0);
        assert_eq!(f16_to_f32(0x0000), 0.0);
        assert_eq!(f16_to_f32(0x3800), 0.5);
        assert_eq!(f16_to_f32(0x7C00), f32::INFINITY);
        assert_eq!(f16_to_f32(0xFC00), f32::NEG_INFINITY);
        assert!(f16_to_f32(0x7E00).is_nan());
        assert_eq!(f16_to_f32(0x7BFF), 65504.0);
        assert_eq!(f16_to_f32(0x0001), 2f32.powi(-24));
    }

    #[test]
    fn hdr_row() {
        let lut = srgb_lut();
        let h = |v: u16| v.to_le_bytes();
        // pixel (r=1.0, g=0.0, b=2.0 (clipped), a=1.0) at scale 1.0
        let mut src = Vec::new();
        for c in [0x3C00, 0x0000, 0x4000, 0x3C00] {
            src.extend_from_slice(&h(c));
        }
        let mut out = [0u8; 4];
        f16_row_to_bgra(&src, 1.0, &lut, &mut out);
        assert_eq!(out, [255, 0, 255, 255]);
        // half-bright linear at scale 1.0 is sRGB ~188
        let mut src = Vec::new();
        for c in [0x3800, 0x3800, 0x3800, 0x3C00] {
            src.extend_from_slice(&h(c));
        }
        f16_row_to_bgra(&src, 1.0, &lut, &mut out);
        assert!((186..=189).contains(&out[0]));
    }

    #[test]
    fn rotation_places_pixels() {
        // 3x2 source (x 0..3, y 0..2), pixel value = 10*y + x in the first byte.
        let src: Vec<[u8; 4]> = (0..6)
            .map(|i| [(10 * (i / 3) + i % 3) as u8, 0, 0, 255])
            .collect();
        let render = |rot: u8| {
            let (dw, dh) = if rot & 1 == 1 { (2, 3) } else { (3, 2) };
            let mut canvas = vec![0u8; dw * dh * 4];
            for y in 0..2 {
                let row: Vec<u8> = src[y * 3..y * 3 + 3].iter().flatten().copied().collect();
                place_row(&row, y, 3, 2, rot, &mut canvas, dw * 4, 0, 0);
            }
            canvas
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[0])
                .collect::<Vec<_>>()
        };
        assert_eq!(render(0), [0, 1, 2, 10, 11, 12]);
        assert_eq!(render(2), [12, 11, 10, 2, 1, 0]);
        // 90 CW: top-left of the source ends up top-right.
        assert_eq!(render(1), [10, 0, 11, 1, 12, 2]);
        assert_eq!(render(3), [2, 12, 1, 11, 0, 10]);
    }
}
