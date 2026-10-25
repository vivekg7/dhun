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
    let (mut i, mut ms) = (19, 0);
    if b[19] == b'.' {
        i = 20 + b[20..].iter().take_while(|c| c.is_ascii_digit()).count();
        ms = format!("{:0<3}", &s[20..i])[..3].parse().ok()?;
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

/// Epoch milliseconds as `2026-10-12T10:00:00.500Z`, as the server reads them.
pub fn format(ms: i64) -> String {
    let (days, rest) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let (y, m, d) = civil_from_days(days);
    let s = rest / 1000;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", s / 3600, s / 60 % 60, s % 60, rest % 1000)
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
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
        for ms in [0, 1_791_799_200_500, 951_782_400_000] {
            assert_eq!(super::parse(&super::format(ms)), ms);
        }
        assert_eq!(super::format(1_791_799_200_500), "2026-10-12T10:00:00.500Z");
    }
}
