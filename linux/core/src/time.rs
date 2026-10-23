//! Server times are RFC 3339; the app keeps epoch milliseconds, as the phone does.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// `2026-10-12T10:00:00.5+05:30` as epoch milliseconds; 0 for anything else.
pub fn parse(s: &str) -> i64 {
    try_parse(s).unwrap_or(0)
}

fn try_parse(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let mut i = 19;
    let mut ms = 0;
    if b.get(i) == Some(&b'.') {
        let start = i + 1;
        i = start;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let frac = &s[start..i];
        ms = format!("{frac:0<3}")[..3].parse().ok()?;
    }
    let offset = match b.get(i)? {
        b'Z' | b'z' => 0,
        sign @ (b'+' | b'-') => {
            let (oh, om) = (num(i + 1..i + 3)?, num(i + 4..i + 6)?);
            let o = (oh * 60 + om) * 60_000;
            if *sign == b'+' { o } else { -o }
        }
        _ => return None,
    };
    let days = days_from_civil(y, mo, d);
    Some(((days * 24 + h) * 60 + mi) * 60_000 + sec * 1000 + ms - offset)
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    #[test]
    fn server_times_with_and_without_fractions_and_offsets() {
        assert_eq!(super::parse("1970-01-01T00:00:00Z"), 0);
        assert_eq!(super::parse("2026-10-12T10:00:00.5Z"), 1_791_799_200_500);
        assert_eq!(super::parse("2026-10-12T15:30:00+05:30"), 1_791_799_200_000);
        assert_eq!(super::parse(""), 0);
        assert_eq!(super::parse("yesterday"), 0);
    }
}
