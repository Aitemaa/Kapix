//! Launch-at-startup via `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (no elevation).

const VALUE_NAME: &str = "Kapix";

/// True if the Run value exists and points at the current executable.
pub fn is_enabled() -> bool {
    let _ = VALUE_NAME;
    todo!()
}

/// Create (`"<current exe path>"` quoted) or delete the Run value.
pub fn set_enabled(enabled: bool) -> crate::Result<()> {
    todo!()
}
