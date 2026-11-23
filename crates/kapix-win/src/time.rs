//! Local time without a date/time crate.

use kapix_core::template::DateTime;
use windows::Win32::System::SystemInformation::GetLocalTime;

/// `GetLocalTime`.
pub fn local_now() -> DateTime {
    // SAFETY: no preconditions.
    let t = unsafe { GetLocalTime() };
    DateTime {
        year: t.wYear,
        month: t.wMonth as u8,
        day: t.wDay as u8,
        hour: t.wHour as u8,
        minute: t.wMinute as u8,
        second: t.wSecond as u8,
        millis: t.wMilliseconds,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn plausible_clock() {
        let t = super::local_now();
        assert!(t.year >= 2024 && (1..=12).contains(&t.month) && (1..=31).contains(&t.day));
    }
}
