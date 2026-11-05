//! The Mozilla dialect rule: a UTF-8 `.properties` file with no Java MessageFormat argument and at least one
//! Mozilla marker (licence header, `LOCALIZATION NOTE`, `%S` / `%1$S`, a plural `#1`).

use regex::Regex;
use std::sync::OnceLock;

pub(crate) fn mf_arg() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\s*\d+\s*[,}]").expect("mf regex"))
}

fn license(text: &str) -> bool {
    let mut from = 0;
    while let Some(p) = text[from..].find("Mozilla Public") {
        let rest = &text[from + p + 14..];
        let trimmed = rest.trim_start_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
        if trimmed.len() < rest.len() && trimmed.starts_with("License") {
            return true;
        }
        from += p + 14;
    }
    false
}

/// `%S` or `%12$S` not followed by an ASCII letter.
fn percent_s(b: &[u8]) -> bool {
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let mut j = i + 1;
            let d = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j > d && b.get(j) == Some(&b'$') {
                j += 1;
            } else {
                j = d;
            }
            if b.get(j) == Some(&b'S') && !b.get(j + 1).is_some_and(u8::is_ascii_alphabetic) {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// `#1` to `#9` not followed by a digit and not preceded by `&` or a word character.
fn plural_hash(text: &str) -> bool {
    let mut prev: Option<char> = None;
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if c == '#' && !prev.is_some_and(|p| p == '&' || p.is_alphanumeric() || p == '_') {
            if let Some(&d) = it.peek() {
                if ('1'..='9').contains(&d) {
                    let mut look = it.clone();
                    look.next();
                    if !look.peek().is_some_and(|n| n.is_numeric()) {
                        return true;
                    }
                }
            }
        }
        prev = Some(c);
    }
    false
}

/// True when decoded UTF-8 text is the Mozilla dialect.
pub(crate) fn looks_mozilla(text: &str) -> bool {
    if mf_arg().is_match(text) {
        return false;
    }
    license(text) || text.contains("LOCALIZATION NOTE") || percent_s(text.as_bytes()) || plural_hash(text)
}
