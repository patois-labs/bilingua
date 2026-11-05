//! Edits to the header entry's text (`Language`, `Plural-Forms`, the charset label).

use crate::read::charset_spans;

/// The range of the first line that starts with `prefix` (not counting the line break).
pub fn find_line(h: &str, prefix: &str) -> Option<(usize, usize)> {
    let mut start = 0;
    loop {
        let end = h[start..].find('\n').map_or(h.len(), |p| start + p);
        if h[start..end].starts_with(prefix) {
            return Some((start, end));
        }
        if end >= h.len() {
            return None;
        }
        start = end + 1;
    }
}

/// True when some line looks like `Name-Like-This: value`, which is what a real header is made of.
pub fn looks_like_header(h: &str) -> bool {
    h.split('\n').any(|l| {
        let b = l.as_bytes();
        let name = b.iter().take_while(|c| c.is_ascii_alphabetic() || **c == b'-').count();
        name > 0 && b[0].is_ascii_alphabetic() && b[name..].starts_with(b": ")
    })
}

/// Every charset label in the header becomes `UTF-8`.
pub fn labels_to_utf8(h: &str) -> String {
    let mut out = String::with_capacity(h.len());
    let mut last = 0;
    for (a, b) in charset_spans(h.as_bytes()) {
        out.push_str(&h[last..a]);
        out.push_str("UTF-8");
        last = b;
    }
    out.push_str(&h[last..]);
    out
}

/// The localized catalog is written as UTF-8, so its header says so: a header with no `Content-Type` gets one,
/// one naming another charset (or the template word `CHARSET`) is set to UTF-8.
pub fn utf8_charset(h: &str) -> String {
    let Some((s, e)) = find_line(h, "Content-Type:") else {
        return format!("Content-Type: text/plain; charset=UTF-8\n{h}");
    };
    let line = &h[s..e];
    let lower = line.to_ascii_lowercase();
    let lb = lower.as_bytes();
    let mut i = 0;
    while let Some(p) = lower[i..].find("charset") {
        let at = i + p;
        let boundary = at == 0 || !(lb[at - 1].is_ascii_alphanumeric() || lb[at - 1] == b'_');
        let mut j = at + 7;
        while lb.get(j).is_some_and(|c| c.is_ascii_whitespace() || *c == 0x0b) {
            j += 1;
        }
        if boundary && lb.get(j) == Some(&b'=') {
            j += 1;
            while lb.get(j).is_some_and(|c| c.is_ascii_whitespace() || *c == 0x0b) {
                j += 1;
            }
            let mut k = j;
            while k < line.len() && !matches!(lb[k], b';') && !lb[k].is_ascii_whitespace() && lb[k] != 0x0b {
                k += 1;
            }
            return format!("{}{}UTF-8{}{}", &h[..s], &line[..j], &line[k..], &h[e..]);
        }
        i = at + 7;
    }
    format!("{}{}; charset=UTF-8{}", &h[..s], line.trim_end().trim_end_matches(';'), &h[e..])
}

/// Sets (or adds) the `Language` field. Returns the language the header named before, trimmed.
pub fn set_language(h: &mut String, value: &str) -> String {
    match find_line(h, "Language:") {
        Some((s, e)) => {
            let old = h[s + 9..e].trim().to_string();
            h.replace_range(s..e, &format!("Language: {value}"));
            old
        }
        None => {
            let base = h.trim_end_matches('\n').to_string();
            *h = format!("{base}\nLanguage: {value}\n");
            String::new()
        }
    }
}

/// Sets (or adds) the `Plural-Forms` field.
pub fn set_plural_forms(h: &mut String, declaration: &str) {
    match find_line(h, "Plural-Forms:") {
        Some((s, e)) => h.replace_range(s..e, &format!("Plural-Forms: {declaration}")),
        None => {
            let base = h.trim_end_matches('\n').to_string();
            *h = format!("{base}\nPlural-Forms: {declaration}\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charset_rewrites() {
        assert_eq!(utf8_charset("Content-Type: text/plain; charset=ISO-8859-1\nX: y\n"), "Content-Type: text/plain; charset=UTF-8\nX: y\n");
        assert_eq!(utf8_charset("Content-Type: text/plain; charset=CHARSET\n"), "Content-Type: text/plain; charset=UTF-8\n");
        assert_eq!(utf8_charset("Content-Type: text/plain;\n"), "Content-Type: text/plain; charset=UTF-8\n");
        assert_eq!(utf8_charset("A: b\n"), "Content-Type: text/plain; charset=UTF-8\nA: b\n");
        assert_eq!(labels_to_utf8("Content-Type: text/plain; charset=latin1\n"), "Content-Type: text/plain; charset=UTF-8\n");
    }

    #[test]
    fn language_and_plural_fields() {
        let mut h = "A: b\n".to_string();
        assert_eq!(set_language(&mut h, "ru_RU"), "");
        assert_eq!(h, "A: b\nLanguage: ru_RU\n");
        assert_eq!(set_language(&mut h, "pl"), "ru_RU");
        set_plural_forms(&mut h, "nplurals=1; plural=0;");
        assert_eq!(h, "A: b\nLanguage: pl\nPlural-Forms: nplurals=1; plural=0;\n");
        let mut empty = String::new();
        set_language(&mut empty, "x");
        assert_eq!(empty, "\nLanguage: x\n");
    }

    #[test]
    fn header_detection() {
        assert!(looks_like_header("Project-Id-Version: x\n"));
        assert!(!looks_like_header("stray text"));
    }
}
