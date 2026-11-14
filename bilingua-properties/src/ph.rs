//! Placeholders in a value: MessageFormat arguments (`{0}`, `{1,number}`), named braces (`{name}`), printf
//! (`%s`, `%1$d`, Mozilla `%S` / `%1$S`), `${var}` and, in the Mozilla dialect, the plural `#1`.

#[derive(Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    Text(&'a str),
    Ph(&'a str),
}

fn digits(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i
}

/// `{ 0 }`, `{0}`, `{1,number,#.##}`, `{0,choice,0#none|1<{0} items}` (braces balanced).
fn mf_arg(b: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while b.get(i).is_some_and(|c| matches!(c, b' ' | b'\t')) {
        i += 1;
    }
    let d = digits(b, i);
    if d == i {
        return None;
    }
    i = d;
    while b.get(i).is_some_and(|c| matches!(c, b' ' | b'\t')) {
        i += 1;
    }
    match b.get(i) {
        Some(b'}') => Some(i + 1),
        Some(b',') => {
            let mut depth = 1;
            i += 1;
            while i < b.len() {
                match b[i] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    b'\n' | b'\r' => return None,
                    _ => {}
                }
                i += 1;
            }
            None
        }
        _ => None,
    }
}

/// `{name}`, `{user.name}`, `{a-b}`.
fn named(b: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    if !b.get(i).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') {
        return None;
    }
    while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'.' | b'-')) {
        i += 1;
    }
    (b.get(i) == Some(&b'}')).then_some(i + 1)
}

/// `${name}` (no nested braces).
fn dollar(b: &[u8], start: usize) -> Option<usize> {
    if b.get(start + 1) != Some(&b'{') {
        return None;
    }
    let mut i = start + 2;
    while i < b.len() && !matches!(b[i], b'{' | b'}' | b'\n' | b'\r') {
        i += 1;
    }
    (b.get(i) == Some(&b'}') && i > start + 2).then_some(i + 1)
}

fn percent(b: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let d = digits(b, i);
    if d > i && b.get(d) == Some(&b'$') {
        i = d + 1;
    }
    while b.get(i).is_some_and(|c| matches!(c, b'-' | b'#' | b'+' | b'0')) {
        i += 1;
    }
    i = digits(b, i);
    if b.get(i) == Some(&b'.') {
        i = digits(b, i + 1);
    }
    while b.get(i).is_some_and(|c| matches!(c, b'h' | b'l' | b'q' | b'L' | b'z' | b'j' | b't')) {
        i += 1;
    }
    match b.get(i) {
        Some(c) if b"diouxXeEfFgGaAcCsSp@".contains(c) => Some(i + 1),
        _ => None,
    }
}

pub fn split(s: &str, mozilla: bool) -> Vec<Piece<'_>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut text_start = 0;
    let mut i = 0;
    while i < b.len() {
        let found = match b[i] {
            b'%' => {
                if b.get(i + 1) == Some(&b'%') {
                    i += 2;
                    continue;
                }
                percent(b, i)
            }
            b'{' => mf_arg(b, i).or_else(|| named(b, i)),
            b'$' => dollar(b, i),
            b'#' if mozilla => {
                let prev_ok = s[..i].chars().next_back().is_none_or(|p| !(p == '&' || bilingua_model::py_isalnum_char(p) || p == '_'));
                if prev_ok && b.get(i + 1).is_some_and(|c| (b'1'..=b'9').contains(c)) && !b.get(i + 2).is_some_and(u8::is_ascii_digit) {
                    Some(i + 2)
                } else {
                    None
                }
            }
            _ => None,
        };
        match found {
            Some(end) => {
                if text_start < i {
                    out.push(Piece::Text(&s[text_start..i]));
                }
                out.push(Piece::Ph(&s[i..end]));
                i = end;
                text_start = end;
            }
            None => i += 1,
        }
    }
    if text_start < b.len() {
        out.push(Piece::Text(&s[text_start..]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phs(s: &str) -> Vec<&str> {
        split(s, true).into_iter().filter_map(|p| if let Piece::Ph(x) = p { Some(x) } else { None }).collect()
    }

    #[test]
    fn finds_placeholders() {
        assert_eq!(phs("Hello {0}, you have {1,number} of {name}"), ["{0}", "{1,number}", "{name}"]);
        assert_eq!(phs("Open %s %1$S %d%% done ${x}"), ["%s", "%1$S", "%d", "${x}"]);
        assert_eq!(phs("#1 file;#1 files"), ["#1", "#1"]);
        assert_eq!(phs("{0,choice,0#none|1<{0} items}"), ["{0,choice,0#none|1<{0} items}"]);
    }

    #[test]
    fn leaves_look_alikes() {
        assert!(phs("50% off, 100%% sure, a#1b, &#1;, {} and { 0x }").is_empty());
        assert!(phs("{0").is_empty());
    }
}
