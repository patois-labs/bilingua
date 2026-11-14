//! Placeholder finder for Apple strings: printf style (`%@`, `%d`, `%1$@`, `%ld`, `%.2f`) and the
//! stringsdict variable `%#@name@`. `%%` is a literal percent sign and stays text.

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
        if b[i] == b'%' {
            if b.get(i + 1) == Some(&b'%') {
                i += 2;
                continue;
            }
            if let Some(end) = percent(b, i) {
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

/// A placeholder that names its argument (`%1$@`, `%#@items@`) may move in a translation.
pub fn is_named(p: &str) -> bool {
    p.contains('$') || p.starts_with("%#@")
}

fn digits(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i
}

/// End of a `%...` placeholder starting at `start`, if it is one.
fn percent(b: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    if b.get(i) == Some(&b'#') && b.get(i + 1) == Some(&b'@') {
        let mut j = i + 2;
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
            j += 1;
        }
        return if j > i + 2 && b.get(j) == Some(&b'@') { Some(j + 1) } else { None };
    }
    let d = digits(b, i);
    if d > i && b.get(d) == Some(&b'$') {
        i = d + 1;
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
        Some(c) if b"@diouxXeEfFgGaAcCsSpnDUO".contains(c) => Some(i + 1),
        _ => None,
    }
}

fn letter_or_at(c: Option<&u8>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphabetic() || *c == b'@')
}

/// LocOS rule (`kv_resource.is_translatable`): true when the value has a letter or digit outside its `{...}`
/// and `%...` placeholders. `{0}`, `%1$@` and `---` are not translatable.
pub fn has_text(value: &str) -> bool {
    let b = value.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' => {
                if let Some(rel) = b[i + 1..].iter().position(|&c| c == b'{' || c == b'}') {
                    if b[i + 1 + rel] == b'}' {
                        i += rel + 2;
                        continue;
                    }
                }
                i += 1;
            }
            b'%' => match loose_percent(b, i) {
                Some(end) => i = end,
                None => i += 1,
            },
            c if c.is_ascii() => {
                if c.is_ascii_alphanumeric() {
                    return true;
                }
                i += 1;
            }
            _ => {
                let ch = value[i..].chars().next().unwrap_or('\u{fffd}');
                if bilingua_model::py_isalnum_char(ch) {
                    return true;
                }
                i += ch.len_utf8();
            }
        }
    }
    false
}

/// `%(?:\d+\$)?[-#+0-9.]*(?:hh|ll|[hlqLzjt])?[a-zA-Z@]`
fn loose_percent(b: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let d = digits(b, i);
    if d > i && b.get(d) == Some(&b'$') {
        i = d + 1;
    }
    while i < b.len() && matches!(b[i], b'-' | b'#' | b'+' | b'0'..=b'9' | b'.') {
        i += 1;
    }
    let two = b.get(i..i + 2).is_some_and(|p| p == b"hh" || p == b"ll");
    if two && letter_or_at(b.get(i + 2)) {
        return Some(i + 3);
    }
    if b.get(i).is_some_and(|c| b"hlqLzjt".contains(c)) && letter_or_at(b.get(i + 1)) {
        return Some(i + 2);
    }
    if letter_or_at(b.get(i)) {
        return Some(i + 1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phs(s: &str) -> Vec<&str> {
        split(s).into_iter().filter_map(|p| if let Piece::Ph(x) = p { Some(x) } else { None }).collect()
    }

    #[test]
    fn finds_placeholders() {
        assert_eq!(phs("Hello %@, you have %1$@ items"), ["%@", "%1$@"]);
        assert_eq!(phs("%d of %ld, %.2f, %-5s %lu %2$lld"), ["%d", "%ld", "%.2f", "%-5s", "%lu", "%2$lld"]);
        assert_eq!(phs("%#@items@ left"), ["%#@items@"]);
    }

    #[test]
    fn leaves_look_alikes_alone() {
        assert!(phs("100%% sure, 50% off, 5 % of").is_empty());
        assert!(phs("%").is_empty() && phs("%#@x").is_empty() && phs("%y").is_empty());
    }

    #[test]
    fn translatable_rule() {
        assert!(!has_text("%@") && !has_text("{0}") && !has_text("---") && !has_text("") && !has_text("%1$@ %d"));
        assert!(has_text("Save %@") && has_text("\u{65e5}\u{672c}") && has_text("100%"));
    }
}
