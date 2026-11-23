//! Windows light/dark "app mode" detection.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, REG_DWORD, RegCloseKey, RegOpenKeyExW,
    RegQueryValueExW,
};
use windows::core::w;

/// True when Windows apps use the light theme
/// (`HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme` = 1).
/// A missing key or value (older Windows builds) means dark.
pub fn apps_use_light_theme() -> bool {
    let mut key = HKEY::default();
    // SAFETY: `key` is a valid out pointer; the key name is a static NUL-terminated string.
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            None,
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if opened != ERROR_SUCCESS {
        return false;
    }
    let mut value = 0u32;
    let mut len = std::mem::size_of::<u32>() as u32;
    let mut kind = REG_DWORD;
    // SAFETY: `value`/`len` describe a valid 4-byte writable buffer; the key is open.
    let r = unsafe {
        let r = RegQueryValueExW(
            key,
            w!("AppsUseLightTheme"),
            None,
            Some(&mut kind),
            Some((&raw mut value).cast()),
            Some(&mut len),
        );
        let _ = RegCloseKey(key);
        r
    };
    r == ERROR_SUCCESS && kind == REG_DWORD && value != 0
}
