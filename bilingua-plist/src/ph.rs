//! Apple format specifiers (`%@`, `%1$@`, `%lld`, `%.2f`), the stringsdict variable `%#@files@` and the String
//! Catalog argument `%arg`, found the way Localization OS finds them. `%%` is a literal percent sign.

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Piece<'a> {
    Text(&'a str),
    Ph(&'a str),
}

fn digits(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i
}

/// End of a specifier that starts at the `%` at `start`, or `None`.
fn specifier(b: &[u8], start: usize) -> Option<usize> {
    let after = start + 1;
    // optional positional prefix `n$`
    let d = digits(b, after);
    let pos_end = if d > after && b.get(d) == Some(&b'$') { d + 1 } else { after };
    // `%#@name@` (with or without a position)
    if b.get(pos_end) == Some(&b'#') && b.get(pos_end + 1) == Some(&b'@') {
        let mut j = pos_end + 2;
        if b.get(j).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') {
            j += 1;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            if b.get(j) == Some(&b'@') {
                return Some(j + 1);
            }
        }
    }
    // `%arg`, not followed by a word character
    if b[after..].starts_with(b"arg") {
        let e = after + 3;
        if !b.get(e).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
            return Some(e);
        }
    }
    let mut i = pos_end;
    while i < b.len() && matches!(b[i], b'-' | b'+' | b' ' | b'#' | b'0' | b'\'') {
        i += 1;
    }
    if b.get(i) == Some(&b'*') {
        i += 1;
    } else {
        i = digits(b, i);
    }
    if b.get(i) == Some(&b'.') {
        let j = i + 1;
        if b.get(j) == Some(&b'*') {
            i = j + 1;
        } else {
            let e = digits(b, j);
            if e > j {
                i = e;
            }
        }
    }
    if b[i..].starts_with(b"hh") || b[i..].starts_with(b"ll") {
        i += 2;
    } else if b.get(i).is_some_and(|c| b"hlqLztj".contains(c)) {
        i += 1;
    }
    match b.get(i) {
        Some(c) if b"@diouxXcCsSpaAeEfFgGD".contains(c) => Some(i + 1),
        _ => None,
    }
}

/// Splits a value into text and specifiers.
pub(crate) fn split(s: &str) -> Vec<Piece<'_>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let (mut text_start, mut i) = (0, 0);
    while i < b.len() {
        if b[i] == b'%' {
            if b.get(i + 1) == Some(&b'%') {
                i += 2;
                continue;
            }
            if let Some(end) = specifier(b, i) {
                if text_start < i {
                    out.push(Piece::Text(&s[text_start..i]));
                }
                out.push(Piece::Ph(&s[i..end]));
                i = end;
                text_start = end;
                continue;
            }
        }
        i += 1;
    }
    if text_start < b.len() {
        out.push(Piece::Text(&s[text_start..]));
    }
    out
}

/// A specifier that names its argument (`%1$@`, `%2$#@x@`) may move in a translation; an unnumbered one keeps its
/// order (LocOS point token `p` against `x`).
pub(crate) fn is_positional(p: &str) -> bool {
    let b = p.as_bytes();
    let d = digits(b, 1);
    d > 1 && b.get(d) == Some(&b'$')
}

/// LocOS `is_translatable`: a letter or digit outside the specifiers (after `%%` is dropped).
pub(crate) fn is_translatable(value: &str) -> bool {
    if !value.contains('%') {
        return value.chars().any(is_alnum);
    }
    let stripped = value.replace("%%", "");
    split(&stripped).into_iter().any(|p| matches!(p, Piece::Text(t) if t.chars().any(is_alnum)))
}

/// Python `str.isalnum`, exactly (`bilingua_model::py_isalnum_char`).
pub(crate) fn is_alnum(c: char) -> bool {
    bilingua_model::py_isalnum_char(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phs(s: &str) -> Vec<&str> {
        split(s).into_iter().filter_map(|p| if let Piece::Ph(x) = p { Some(x) } else { None }).collect()
    }

    #[test]
    fn finds_specifiers() {
        assert_eq!(phs("Hello %@, you have %1$@ items"), ["%@", "%1$@"]);
        assert_eq!(phs("%d of %ld, %.2f, %-5s %lu %2$lld %hhd %05.1f"), ["%d", "%ld", "%.2f", "%-5s", "%lu", "%2$lld", "%hhd", "%05.1f"]);
        assert_eq!(phs("%#@items@ left %1$#@a_b@ %arg %args"), ["%#@items@", "%1$#@a_b@", "%arg", "%a"]);
        assert_eq!(phs("% d %'d %*d %.*f"), ["% d", "%'d", "%*d", "%.*f"]);
    }

    #[test]
    fn leaves_look_alikes_alone() {
        assert!(phs("100%% sure, 50%, 5 %").is_empty());
        // LocOS quirk kept: a space is a printf flag, so "50% off" holds the specifier "% o"
        assert_eq!(phs("50% off"), ["% o"]);
        assert!(phs("%").is_empty() && phs("%y").is_empty() && phs("%5.f").is_empty());
        assert_eq!(phs("%%%d"), ["%d"]);
        // LocOS quirk kept: an unfinished `%#@x` is read as the printf form `%#@`
        assert_eq!(phs("%#@x"), ["%#@"]);
    }

    #[test]
    fn positional_ones_are_free() {
        assert!(is_positional("%1$@") && is_positional("%12$#@v@") && !is_positional("%@") && !is_positional("%#@v@"));
    }

    #[test]
    fn translatable_rule() {
        assert!(!is_translatable("%@") && !is_translatable("%#@files@") && !is_translatable("%lld") && !is_translatable("---"));
        assert!(!is_translatable("") && !is_translatable("%1$@ %d"));
        assert!(is_translatable("Save %@") && is_translatable("\u{65e5}\u{672c}") && is_translatable("100%"));
        // `%%` is removed before the specifiers are
        assert!(is_translatable("%%d") && !is_translatable("%%%d"));
    }
}
