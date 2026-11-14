//! Validation of the non-string values of a TOML 1.0 document (numbers, booleans, dates and times). They are
//! never translated; they are checked only so that an invalid file is refused like the reference parser does.

/// True when `s` (already trimmed) is a valid TOML 1.0 boolean, integer, float, date or time.
pub fn valid_scalar(s: &str) -> bool {
    match s {
        "true" | "false" | "inf" | "+inf" | "-inf" | "nan" | "+nan" | "-nan" => return true,
        _ => {}
    }
    let b = s.as_bytes();
    is_datetime(b) || number(b)
}

fn digits(b: &[u8], radix: u32) -> bool {
    if b.is_empty() {
        return false;
    }
    let mut prev_digit = false;
    for (i, &c) in b.iter().enumerate() {
        if c == b'_' {
            if !prev_digit || i + 1 == b.len() {
                return false;
            }
            prev_digit = false;
        } else if (c as char).is_digit(radix) {
            prev_digit = true;
        } else {
            return false;
        }
    }
    true
}

fn number(b: &[u8]) -> bool {
    if b.len() > 2 && b[0] == b'0' {
        let radix = match b[1] {
            b'x' => 16,
            b'o' => 8,
            b'b' => 2,
            _ => 0,
        };
        if radix != 0 {
            return digits(&b[2..], radix);
        }
    }
    let b = match b.first() {
        Some(b'+' | b'-') => &b[1..],
        _ => b,
    };
    let end_int = b.iter().position(|&c| c == b'.' || c == b'e' || c == b'E').unwrap_or(b.len());
    let int = &b[..end_int];
    if !digits(int, 10) || (int.len() > 1 && int[0] == b'0') {
        return false;
    }
    let mut rest = &b[end_int..];
    if rest.is_empty() {
        return true;
    }
    if rest[0] == b'.' {
        let end_frac = rest.iter().position(|&c| c == b'e' || c == b'E').unwrap_or(rest.len());
        if !digits(&rest[1..end_frac], 10) {
            return false;
        }
        rest = &rest[end_frac..];
    }
    if rest.is_empty() {
        return true;
    }
    // exponent
    let e = match rest[1..].first() {
        Some(b'+' | b'-') => &rest[2..],
        _ => &rest[1..],
    };
    digits(e, 10)
}

fn num(b: &[u8], at: usize, len: usize) -> Option<u32> {
    let s = b.get(at..at + len)?;
    if !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(s.iter().fold(0u32, |a, &c| a * 10 + u32::from(c - b'0')))
}

fn date(b: &[u8]) -> bool {
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let (Some(y), Some(m), Some(d)) = (num(b, 0, 4), num(b, 5, 2), num(b, 8, 2)) else { return false };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let dim = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => 28 + u32::from(leap),
        _ => return false,
    };
    (1..=dim).contains(&d)
}

/// Checks `hh:mm:ss[.frac]` at the start of `b`; returns its length.
fn time(b: &[u8]) -> Option<usize> {
    if b.len() < 8 || b[2] != b':' || b[5] != b':' {
        return None;
    }
    let (h, m, s) = (num(b, 0, 2)?, num(b, 3, 2)?, num(b, 6, 2)?);
    if h > 23 || m > 59 || s > 60 {
        return None;
    }
    let mut n = 8;
    if b.get(8) == Some(&b'.') {
        let f = b[9..].iter().take_while(|c| c.is_ascii_digit()).count();
        if f == 0 {
            return None;
        }
        n = 9 + f;
    }
    Some(n)
}

fn is_datetime(b: &[u8]) -> bool {
    if b.len() >= 3 && b[2] == b':' {
        return time(b) == Some(b.len());
    }
    if !date(b) {
        return false;
    }
    if b.len() == 10 {
        return true;
    }
    if !matches!(b[10], b'T' | b't' | b' ') {
        return false;
    }
    let Some(tl) = time(&b[11..]) else { return false };
    let rest = &b[11 + tl..];
    match rest {
        [] => true,
        [b'Z' | b'z'] => true,
        [b'+' | b'-', ..] => rest.len() == 6 && rest[3] == b':' && num(rest, 1, 2).is_some_and(|h| h < 24) && num(rest, 4, 2).is_some_and(|m| m < 60),
        _ => false,
    }
}
