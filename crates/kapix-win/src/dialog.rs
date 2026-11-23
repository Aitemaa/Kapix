//! Native file dialogs.

use crate::{hwnd, wide};
use std::path::{Path, PathBuf};
use windows::Win32::UI::Controls::Dialogs::{
    GetSaveFileNameW, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::core::{PCWSTR, PWSTR, w};

/// `GetSaveFileNameW` with PNG/JPEG filters, `OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST`.
/// `initial_dir` and `default_name` (without extension) pre-fill the dialog. Returns the chosen
/// path with the extension appended if the user omitted it, or `None` if cancelled.
pub fn save_image_dialog(
    owner: isize,
    initial_dir: &Path,
    default_name: &str,
    jpeg_default: bool,
) -> Option<PathBuf> {
    let ext = if jpeg_default { "jpg" } else { "png" };
    let dir = wide(&initial_dir.to_string_lossy());
    let mut file = vec![0u16; 32 * 1024];
    for (d, s) in file.iter_mut().zip(
        default_name
            .encode_utf16()
            .take(260)
            .chain(".".encode_utf16())
            .chain(ext.encode_utf16()),
    ) {
        *d = s;
    }
    let filter = w!("PNG image\0*.png\0JPEG image\0*.jpg;*.jpeg\0\0");
    let mut ofn = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd(owner),
        lpstrFilter: filter,
        nFilterIndex: if jpeg_default { 2 } else { 1 },
        lpstrFile: PWSTR(file.as_mut_ptr()),
        nMaxFile: file.len() as u32,
        lpstrInitialDir: PCWSTR(dir.as_ptr()),
        lpstrDefExt: if jpeg_default { w!("jpg") } else { w!("png") },
        Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    // SAFETY: all pointers in `ofn` reference buffers that outlive the (modal) call.
    if !unsafe { GetSaveFileNameW(&mut ofn) }.as_bool() {
        return None;
    }
    let n = file.iter().position(|&c| c == 0).unwrap_or(file.len());
    let mut path = PathBuf::from(String::from_utf16_lossy(&file[..n]));
    if path.extension().is_none() {
        path.as_mut_os_string().push(if ofn.nFilterIndex == 2 {
            ".jpg"
        } else {
            ".png"
        });
    }
    Some(path)
}
