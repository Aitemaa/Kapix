//! Pure-Rust local wall-clock time without external date/time crates or `unsafe`.
//!
//! Reads `/etc/localtime` (standard RFC 8536 TZif v1/v2/v3) to compute the local UTC offset,
//! falling back to UTC if unavailable.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use kapix_core::template::DateTime;

/// Current local wall-clock time.
pub fn local_now() -> DateTime {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let unix_secs = dur.as_secs() as i64;
    let millis = dur.subsec_millis() as u16;
    let offset = local_utc_offset(unix_secs).unwrap_or(0);
    from_unix_secs(unix_secs + offset, millis)
}

/// Convert `secs` since 1970-01-01T00:00:00 (already offset to local time) into [`DateTime`].
/// Uses Howard Hinnant's civil-from-days algorithm.
pub fn from_unix_secs(secs: i64, millis: u16) -> DateTime {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = (rem / 3600) as u8;
    let minute = ((rem % 3600) / 60) as u8;
    let second = (rem % 60) as u8;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (y + i64::from(month <= 2)).clamp(0, u16::MAX as i64) as u16;

    DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        millis,
    }
}

/// Look up the local UTC offset in seconds from `/etc/localtime` (TZif v1/v2/v3).
fn local_utc_offset(unix_secs: i64) -> Option<i64> {
    let bytes = fs::read("/etc/localtime").ok()?;
    parse_tzif_offset(&bytes, unix_secs)
}

fn read_be_u32(b: &[u8], off: usize) -> Option<usize> {
    Some(u32::from_be_bytes(b.get(off..off + 4)?.try_into().ok()?) as usize)
}

fn read_be_i32(b: &[u8], off: usize) -> Option<i32> {
    Some(i32::from_be_bytes(b.get(off..off + 4)?.try_into().ok()?))
}

fn read_be_i64(b: &[u8], off: usize) -> Option<i64> {
    Some(i64::from_be_bytes(b.get(off..off + 8)?.try_into().ok()?))
}

fn parse_tzif_offset(data: &[u8], unix_secs: i64) -> Option<i64> {
    if data.len() < 44 || &data[0..4] != b"TZif" {
        return None;
    }
    let version = data[4];
    let v1_isutcnt = read_be_u32(data, 20)?;
    let v1_isstdcnt = read_be_u32(data, 24)?;
    let v1_leapcnt = read_be_u32(data, 28)?;
    let v1_timecnt = read_be_u32(data, 32)?;
    let v1_typecnt = read_be_u32(data, 36)?;
    let v1_charcnt = read_be_u32(data, 40)?;

    // Prefer the 64-bit v2/v3 data block when present.
    if version >= b'2' {
        let v1_body = v1_timecnt * 4
            + v1_timecnt
            + v1_typecnt * 6
            + v1_charcnt
            + v1_leapcnt * 8
            + v1_isstdcnt
            + v1_isutcnt;
        let v2_start = 44 + v1_body;
        if data.get(v2_start..v2_start + 4) == Some(b"TZif")
            && let Some(off) = parse_block(&data[v2_start..], true, unix_secs)
        {
            return Some(off);
        }
    }

    parse_block(data, false, unix_secs)
}

fn parse_block(block: &[u8], is_64: bool, unix_secs: i64) -> Option<i64> {
    if block.len() < 44 {
        return None;
    }
    let timecnt = read_be_u32(block, 32)?;
    let typecnt = read_be_u32(block, 36)?;
    if typecnt == 0 {
        return None;
    }
    let ts_size = if is_64 { 8 } else { 4 };
    let times_off = 44;
    let indices_off = times_off + timecnt * ts_size;
    let ttinfos_off = indices_off + timecnt;

    let mut chosen_type = 0usize;
    for i in 0..timecnt {
        let t = if is_64 {
            read_be_i64(block, times_off + i * 8)?
        } else {
            i64::from(read_be_i32(block, times_off + i * 4)?)
        };
        if t <= unix_secs {
            chosen_type = usize::from(*block.get(indices_off + i)?);
        } else {
            break;
        }
    }
    if chosen_type >= typecnt {
        chosen_type = 0;
    }
    let utoff = read_be_i32(block, ttinfos_off + chosen_type * 6)?;
    Some(i64::from(utoff))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_and_known_date() {
        let ep = from_unix_secs(0, 0);
        assert_eq!(
            (ep.year, ep.month, ep.day, ep.hour, ep.minute, ep.second),
            (1970, 1, 1, 0, 0, 0)
        );
        // 2026-11-24 09:00:00 UTC = 1795510800
        let d = from_unix_secs(1_795_510_800, 42);
        assert_eq!(
            (d.year, d.month, d.day, d.hour, d.minute, d.second, d.millis),
            (2026, 11, 24, 9, 0, 0, 42)
        );
    }

    #[test]
    fn plausible_clock() {
        let t = local_now();
        assert!(t.year >= 2024 && (1..=12).contains(&t.month) && (1..=31).contains(&t.day));
    }
}
