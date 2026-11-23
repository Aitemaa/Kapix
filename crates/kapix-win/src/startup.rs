//! Launch-at-startup via `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (no elevation).

use crate::{Error, wide};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW,
    RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::core::{PCWSTR, w};

const VALUE_NAME: PCWSTR = w!("Kapix");
const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");

/// Open the Run key and close it again on drop.
struct Key(HKEY);

impl Key {
    fn open(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> crate::Result<Key> {
        let mut k = HKEY::default();
        // SAFETY: `k` is a valid out pointer; the key name is a static NUL-terminated string.
        let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, access, &mut k) };
        if r == ERROR_SUCCESS {
            Ok(Key(k))
        } else {
            Err(Error(format!("cannot open Run key (error {})", r.0)))
        }
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by `Key::open`.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn exe_path() -> Option<String> {
    Some(std::env::current_exe().ok()?.to_string_lossy().into_owned())
}

/// True if the Run value exists and points at the current executable.
pub fn is_enabled() -> bool {
    let (Some(exe), Ok(key)) = (exe_path(), Key::open(KEY_QUERY_VALUE)) else {
        return false;
    };
    let mut buf = [0u16; 1024];
    let mut len = std::mem::size_of_val(&buf) as u32;
    // SAFETY: `buf`/`len` describe a valid writable buffer.
    let r = unsafe {
        RegQueryValueExW(
            key.0,
            VALUE_NAME,
            None,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        )
    };
    if r != ERROR_SUCCESS {
        return false;
    }
    let n = (len as usize / 2).min(buf.len());
    let val = String::from_utf16_lossy(&buf[..n]);
    let val = val.trim_end_matches('\0').trim().trim_matches('"');
    val.eq_ignore_ascii_case(&exe)
}

/// Create (`"<current exe path>"` quoted) or delete the Run value.
pub fn set_enabled(enabled: bool) -> crate::Result<()> {
    let key = Key::open(KEY_SET_VALUE)?;
    // SAFETY: valid key handle; the data slice outlives the call.
    let r = unsafe {
        if enabled {
            let exe = exe_path().ok_or_else(|| Error("cannot determine executable path".into()))?;
            let data = wide(&format!("\"{exe}\""));
            let bytes = std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2);
            RegSetValueExW(key.0, VALUE_NAME, None, REG_SZ, Some(bytes))
        } else {
            RegDeleteValueW(key.0, VALUE_NAME)
        }
    };
    // Deleting a value that does not exist (ERROR_FILE_NOT_FOUND = 2) is fine.
    if r == ERROR_SUCCESS || (!enabled && r.0 == 2) {
        Ok(())
    } else {
        Err(Error(format!("registry write failed (error {})", r.0)))
    }
}
