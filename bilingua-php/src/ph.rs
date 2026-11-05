//! Placeholders inside a decoded value: Laravel `:name`, plural conditions (`{0}`, `[1,19]`) and the `|` separator,
//! printf (`%s`, `%1$s`), simple ICU arguments (`{name}`, `{0, number}`) and HTML tags. Same rules and the same
//! order of preference as the LocOS filter, written as a scanner (the `regex` crate has no look-behind).

use crate::lex::is_space;
use bilingua_model::{CodeData, Inline, TagKind};
use std::collections::BTreeMap;

fn skip_ws(s: &str, mut i: usize) -> usize {
    for c in s[i..].chars() {
        if !is_space(c) {
            break;
        }
        i += c.len_utf8();
    }
    i
}

fn digits(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i
}

/// `-?\d+` or `\*`.
fn bound(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    if b.get(i) == Some(&b'*') {
        return Some(i + 1);
    }
    let j = if b.get(i) == Some(&b'-') { i + 1 } else { i };
    let e = digits(b, j);
    (e > j).then_some(e)
}

/// A plural condition `{0}` or `[1,19]` after optional blanks. Only valid at the start of the string or after `|`.
fn plural_cond(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut j = i;
    while matches!(b.get(j), Some(b' ' | b'\t')) {
        j += 1;
    }
    match b.get(j)? {
        b'{' => {
            let k = if b.get(j + 1) == Some(&b'-') { j + 2 } else { j + 1 };
            let e = digits(b, k);
            (e > k && b.get(e) == Some(&b'}')).then_some(e + 1)
        }
        b'[' => {
            let k = skip_ws(s, j + 1);
            let k = bound(s, k)?;
            let k = skip_ws(s, k);
            if b.get(k) != Some(&b',') {
                return None;
            }
            let k = skip_ws(s, k + 1);
            let k = bound(s, k)?;
            let k = skip_ws(s, k);
            (b.get(k) == Some(&b']')).then_some(k + 1)
        }
        _ => None,
    }
}

/// `⟦` up to 20 characters that are not brackets, `⟧`.
fn marker(s: &str, i: usize) -> Option<usize> {
    let mut j = i + '\u{27E6}'.len_utf8();
    for n in 0.. {
        let c = s[j..].chars().next()?;
        if c == '\u{27E7}' {
            return Some(j + c.len_utf8());
        }
        if c == '\u{27E6}' || n >= 20 {
            return None;
        }
        j += c.len_utf8();
    }
    None
}

/// `</?[A-Za-z][^<>]{0,500}>`
fn html_tag(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut j = i + 1;
    if b.get(j) == Some(&b'/') {
        j += 1;
    }
    if !b.get(j).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    j += 1;
    let mut n = 0;
    for c in s[j..].chars() {
        match c {
            '>' => return Some(j + 1),
            '<' => return None,
            _ => {
                n += 1;
                if n > 500 {
                    return None;
                }
                j += c.len_utf8();
            }
        }
    }
    None
}

/// `\{[A-Za-z0-9_]+(?:\s*,\s*[A-Za-z]+(?:\s*,\s*[^{}]*)?)?\}`
fn icu_arg(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut j = i + 1;
    while b.get(j).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
        j += 1;
    }
    if j == i + 1 {
        return None;
    }
    if b.get(j) == Some(&b'}') {
        return Some(j + 1);
    }
    let k = skip_ws(s, j);
    if b.get(k) != Some(&b',') {
        return None;
    }
    let k = skip_ws(s, k + 1);
    let mut e = k;
    while b.get(e).is_some_and(u8::is_ascii_alphabetic) {
        e += 1;
    }
    if e == k {
        return None;
    }
    if b.get(e) == Some(&b'}') {
        return Some(e + 1);
    }
    let m = skip_ws(s, e);
    if b.get(m) != Some(&b',') {
        return None;
    }
    let m = skip_ws(s, m + 1);
    let mut f = m;
    while f < b.len() && b[f] != b'{' && b[f] != b'}' {
        f += 1;
    }
    (b.get(f) == Some(&b'}')).then_some(f + 1)
}

/// `%(?:\d+\$)?[-+0#']*\d*(?:\.\d+)?[bcdeEfFgGosuxX%]`
fn printf(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut j = i + 1;
    let d = digits(b, j);
    if d > j && b.get(d) == Some(&b'$') {
        j = d + 1;
    }
    while matches!(b.get(j), Some(b'-' | b'+' | b'0' | b'#' | b'\'')) {
        j += 1;
    }
    j = digits(b, j);
    if b.get(j) == Some(&b'.') {
        let e = digits(b, j + 1);
        if e > j + 1 {
            j = e;
        }
    }
    matches!(b.get(j), Some(b'b' | b'c' | b'd' | b'e' | b'E' | b'f' | b'F' | b'g' | b'G' | b'o' | b's' | b'u' | b'x' | b'X' | b'%'))
        .then_some(j + 1)
}

/// `(?<![\w:]):[A-Za-z_][A-Za-z0-9_]*`
fn named(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    if let Some(p) = s[..i].chars().next_back() {
        if p == ':' || crate::lex::is_word(p) {
            return None;
        }
    }
    if !b.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') {
        return None;
    }
    let mut j = i + 2;
    while b.get(j).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
        j += 1;
    }
    Some(j)
}

/// The placeholder that starts at `i`, if any (the first alternative that matches wins).
fn match_at(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    let c = b[i];
    if matches!(c, b' ' | b'\t' | b'{' | b'[') && (i == 0 || b[i - 1] == b'|') {
        if let Some(e) = plural_cond(s, i) {
            return Some(e);
        }
    }
    match c {
        0xE2 if s[i..].starts_with('\u{27E6}') => marker(s, i),
        b'<' => html_tag(s, i),
        b'{' => icu_arg(s, i),
        b'%' => printf(s, i),
        b':' => named(s, i),
        b'|' => Some(i + 1),
        _ => None,
    }
}

/// Names its argument, so a translation may move it (LocOS `FREE_PH`: `:x` and `%1$s`).
fn is_free(p: &str) -> bool {
    let b = p.as_bytes();
    if b[0] == b':' {
        return b.get(1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_');
    }
    if b[0] == b'%' {
        let n = digits(b, 1);
        return n > 1 && b.get(n) == Some(&b'$');
    }
    false
}

/// The unit's inline content and its hidden codes (tag id to the original placeholder text).
pub fn unit_inlines(value: &str) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    let mut inl: Vec<Inline> = Vec::new();
    let mut codes = BTreeMap::new();
    let triggers = value
        .bytes()
        .any(|c| matches!(c, b'{' | b'[' | b'<' | b'%' | b':' | b'|' | 0xE2));
    if !triggers {
        if !value.is_empty() {
            inl.push(Inline::Text(value.to_string()));
        }
        return (inl, codes);
    }
    let (mut pos, mut i, mut n) = (0usize, 0usize, 0usize);
    let b = value.as_bytes();
    while i < b.len() {
        if !value.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let hit = match b[i] {
            b' ' | b'\t' | b'{' | b'[' | b'<' | b'%' | b':' | b'|' | 0xE2 => match_at(value, i),
            _ => None,
        };
        let Some(end) = hit else {
            i += 1;
            continue;
        };
        if i > pos {
            inl.push(Inline::Text(value[pos..i].to_string()));
        }
        n += 1;
        let id = n.to_string();
        let raw = &value[i..end];
        codes.insert(
            id.clone(),
            CodeData {
                standalone: Some(raw.to_string()),
                label: Some("placeholder".into()),
                ..Default::default()
            },
        );
        inl.push(if is_free(raw) {
            Inline::free_tag(id, raw)
        } else {
            Inline::tag(id, TagKind::Standalone, raw)
        });
        pos = end;
        i = end;
    }
    if pos < value.len() {
        inl.push(Inline::Text(value[pos..].to_string()));
    }
    (bilingua_model::merge_text(inl), codes)
}
