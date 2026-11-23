//! Codecs that would be expensive to bundle in Rust, via built-in Windows Imaging Component.

use crate::Error;
use kapix_core::Image;
use std::mem::ManuallyDrop;
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_WICPixelFormat24bppBGR,
    IWICBitmapEncoder, IWICBitmapFrameEncode, IWICImagingFactory, WICBitmapEncoderNoCache,
};
use windows::Win32::System::Com::StructuredStorage::{
    CreateStreamOnHGlobal, GetHGlobalFromStream, IPropertyBag2, PROPBAG2,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, STATFLAG_NONAME, STATSTG,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_R4};
use windows::core::PWSTR;

/// JPEG-encode via WIC (`IWICImagingFactory` → `CreateEncoder(GUID_ContainerFormatJpeg)` with
/// `ImageQuality` property). `quality` 0.0–1.0. No metadata is written.
pub fn jpeg(img: &Image, quality: f32) -> crate::Result<Vec<u8>> {
    let (w, h) = (img.width as usize, img.height as usize);
    if w == 0 || h == 0 || img.data.len() != w * h * 4 {
        return Err(Error("empty image".into()));
    }
    crate::com_init(false);
    // BGRA -> BGR (JPEG has no alpha; WIC would otherwise need a format converter).
    let mut bgr = Vec::with_capacity(w * h * 3);
    for px in img.data.as_chunks::<4>().0 {
        bgr.extend_from_slice(&px[..3]);
    }
    // SAFETY: WIC COM calls on interfaces we own; every pointer passed refers to live locals
    // and `bgr` outlives `WritePixels`.
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
        let encoder: IWICBitmapEncoder =
            factory.CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let (mut frame, mut props): (Option<IWICBitmapFrameEncode>, Option<IPropertyBag2>) =
            (None, None);
        encoder.CreateNewFrame(&mut frame, &mut props)?;
        let (frame, props) = frame
            .zip(props)
            .ok_or_else(|| Error("WIC returned no frame".into()))?;

        let mut name = crate::wide("ImageQuality");
        let bag = PROPBAG2 {
            pstrName: PWSTR(name.as_mut_ptr()),
            vt: VT_R4,
            ..Default::default()
        };
        let value = VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_R4,
                    Anonymous: VARIANT_0_0_0 {
                        fltVal: quality.clamp(0.0, 1.0),
                    },
                    ..Default::default()
                }),
            },
        };
        props.Write(1, &bag, &value)?;
        frame.Initialize(&props)?;
        frame.SetSize(img.width, img.height)?;
        let mut fmt = GUID_WICPixelFormat24bppBGR;
        frame.SetPixelFormat(&mut fmt)?;
        frame.WritePixels(img.height, (w * 3) as u32, &bgr)?;
        frame.Commit()?;
        encoder.Commit()?;

        let mut st = STATSTG::default();
        stream.Stat(&mut st, STATFLAG_NONAME)?;
        let hg = GetHGlobalFromStream(&stream)?;
        let p = GlobalLock(hg).cast::<u8>();
        if p.is_null() {
            return Err(Error("GlobalLock failed".into()));
        }
        let out = std::slice::from_raw_parts(p, st.cbSize as usize).to_vec();
        let _ = GlobalUnlock(hg);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn jpeg_has_soi_marker() {
        let img = kapix_core::Image::new(33, 17);
        let bytes = super::jpeg(&img, 0.9).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
        assert_eq!(&bytes[bytes.len() - 2..], &[0xFF, 0xD9]);
    }
}
