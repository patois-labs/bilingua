//! Placeholder finder for PO strings: printf (`%s`, `%1$d`, `%-5.2f`), Python (`%(name)s`, `{name}`, `{0}`, `{}`).

#[derive(Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    Text(&'a str),
    Ph(&'a str),
}

pub fn split(s: &str) -> Vec<Piece<'_>> {
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
            b'{' => brace(b, i),
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

fn digits(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i
}

/// End of a `%...` placeholder starting at `i`, if it is one.
fn percent(b: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    // Python mapping key: %(name)s
    if b.get(i) == Some(&b'(') {
        let mut j = i + 1;
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
            j += 1;
        }
        if j == i + 1 || b.get(j) != Some(&b')') {
            return None;
        }
        i = j + 1;
    } else {
        // Positional argument: %1$s
        let d = digits(b, i);
        if d > i && b.get(d) == Some(&b'$') {
            i = d + 1;
        }
    }
    while i < b.len() && matches!(b[i], b'#' | b'0' | b'-' | b'+' | b'\'') {
        i += 1;
    }
    if b.get(i) == Some(&b'*') {
        i += 1;
    } else {
        i = digits(b, i);
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        if b.get(i) == Some(&b'*') {
            i += 1;
        } else {
            i = digits(b, i);
        }
    }
    while i < b.len() && matches!(b[i], b'h' | b'l' | b'L' | b'q' | b'j' | b'z' | b't') {
        i += 1;
    }
    match b.get(i) {
        Some(c) if b"diouxXeEfFgGaAcspnr".contains(c) => Some(i + 1),
        _ => None,
    }
}

/// End of a `{...}` placeholder: `{}`, `{0}`, `{name}`, `{name!r}`, `{0:>5}`.
fn brace(b: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'.' | b'[' | b']')) {
        i += 1;
    }
    if b.get(i) == Some(&b'!') && b.get(i + 1).is_some_and(|c| matches!(c, b'r' | b's' | b'a')) {
        i += 2;
    }
    if b.get(i) == Some(&b':') {
        i += 1;
        while i < b.len() && !matches!(b[i], b'{' | b'}' | b'"' | b'\n' | b' ') {
            i += 1;
        }
    }
    if b.get(i) == Some(&b'}') {
        Some(i + 1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phs(s: &str) -> Vec<&str> {
        split(s).into_iter().filter_map(|p| if let Piece::Ph(x) = p { Some(x) } else { None }).collect()
    }

    #[test]
    fn finds_placeholders() {
        assert_eq!(phs("Hello %s, you have %d new %1$s"), ["%s", "%d", "%1$s"]);
        assert_eq!(phs("%(name)s has %(count)d items"), ["%(name)s", "%(count)d"]);
        assert_eq!(phs("{name} and {0} and {} and {x!r} and {0:>5}"), ["{name}", "{0}", "{}", "{x!r}", "{0:>5}"]);
        assert_eq!(phs("%-5.2f %ld %5s %*d"), ["%-5.2f", "%ld", "%5s", "%*d"]);
    }

    #[test]
    fn leaves_look_alikes_alone() {
        assert!(phs("100%% sure, 50% off, 5 % of {total, ok").is_empty());
        assert!(phs("{\"json\": 1} and { spaced }").is_empty());
    }
}
