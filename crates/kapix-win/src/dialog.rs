//! Native file dialogs.

use std::path::{Path, PathBuf};

/// `GetSaveFileNameW` with PNG/JPEG filters, `OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST`.
/// `initial_dir` and `default_name` (without extension) pre-fill the dialog. Returns the chosen
/// path with the extension appended if the user omitted it, or `None` if cancelled.
pub fn save_image_dialog(owner: isize, initial_dir: &Path, default_name: &str, jpeg_default: bool) -> Option<PathBuf> {
    todo!()
}
