//! A small ICU MessageFormat parser that keeps the raw text of every piece, so an unchanged message is written
//! back exactly. Plural, selectordinal and select arguments are split into branches; `{name}` and typed
//! arguments (`{n, number}`) are simple arguments; quotes (`'{'`, `''`) stay as raw text.

const MAX_DEPTH: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Raw message text (quote marks included).
    Text(String),
    /// A simple or typed argument, raw: `{name}`, `{n, number, ::compact}`.
    Simple(String),
    /// `#` inside a plural or selectordinal branch.
    Pound,
    Complex(Complex),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Plural,
    SelectOrdinal,
    Select,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Complex {
    /// Raw text from `{` through the whitespace before the first selector (and an `offset:n`).
    pub head: String,
    pub kind: Kind,
    pub branches: Vec<Branch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub selector: String,
    /// Whitespace between the selector and `{`.
    pub mid: String,
    pub msg: Vec<Part>,
    /// Whitespace after the closing brace.
    pub trail: String,
}

#[derive(Debug)]
pub struct IcuError;

type R<T> = Result<T, IcuError>;

/// Python `\s` for str patterns: Unicode white space plus the four information separators.
fn is_ws(c: char) -> bool {
    bilingua_model::py_isspace_char(c)
}

fn ch(s: &str, i: usize) -> Option<char> {
    s.get(i..).and_then(|r| r.chars().next())
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    while let Some(c) = ch(s, i) {
        if !is_ws(c) {
            break;
        }
        i += c.len_utf8();
    }
    i
}

/// Parses a whole message. Errors on unbalanced braces, bad arguments or nesting over 40 levels.
pub fn parse(s: &str) -> R<Vec<Part>> {
    // Nothing that could be markup: one text part.
    if !s.bytes().any(|b| matches!(b, b'{' | b'}' | b'\'')) {
        return Ok(if s.is_empty() { vec![] } else { vec![Part::Text(s.to_string())] });
    }
    let (parts, i) = message(s, 0, 0, false, false)?;
    if i != s.len() {
        return Err(IcuError);
    }
    Ok(parts)
}

fn message(s: &str, mut i: usize, depth: usize, in_plural: bool, nested: bool) -> R<(Vec<Part>, usize)> {
    if depth > MAX_DEPTH {
        return Err(IcuError);
    }
    let b = s.as_bytes();
    let n = b.len();
    let mut parts = Vec::new();
    let mut run = i;
    while i < n {
        match b[i] {
            b'{' => {
                flush(s, &mut parts, run, i);
                let (p, next) = argument(s, i, depth)?;
                parts.push(p);
                i = next;
                run = i;
            }
            b'}' => {
                if !nested {
                    return Err(IcuError);
                }
                break;
            }
            b'#' if in_plural => {
                flush(s, &mut parts, run, i);
                parts.push(Part::Pound);
                i += 1;
                run = i;
            }
            b'\'' => {
                let nxt = b.get(i + 1).copied();
                if nxt == Some(b'\'') {
                    i += 2;
                } else if matches!(nxt, Some(b'{' | b'}' | b'|')) || (nxt == Some(b'#') && in_plural) {
                    let mut j = i + 1;
                    while j < n {
                        if b[j] == b'\'' {
                            if b.get(j + 1) == Some(&b'\'') {
                                j += 2;
                                continue;
                            }
                            break;
                        }
                        j += 1;
                    }
                    i = j + 1;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    let end = i.min(n);
    flush(s, &mut parts, run, end);
    Ok((parts, i))
}

fn flush(s: &str, parts: &mut Vec<Part>, from: usize, to: usize) {
    if to > from {
        parts.push(Part::Text(s[from..to].to_string()));
    }
}

fn argument(s: &str, start: usize, depth: usize) -> R<(Part, usize)> {
    let mut j = skip_ws(s, start + 1);
    // name: one or more characters that are not white space or , { } # ' |
    let name_start = j;
    while let Some(c) = ch(s, j) {
        if is_ws(c) || matches!(c, ',' | '{' | '}' | '#' | '\'' | '|') {
            break;
        }
        j += c.len_utf8();
    }
    if j == name_start {
        return Err(IcuError);
    }
    j = skip_ws(s, j);
    let b = s.as_bytes();
    match b.get(j) {
        Some(b'}') => return Ok((Part::Simple(s[start..=j].to_string()), j + 1)),
        Some(b',') => {}
        _ => return Err(IcuError),
    }
    j = skip_ws(s, j + 1);
    let ty_start = j;
    while b.get(j).is_some_and(u8::is_ascii_alphabetic) {
        j += 1;
    }
    if j == ty_start {
        return Err(IcuError);
    }
    let kind_word = &s[ty_start..j];
    j = skip_ws(s, j);
    match b.get(j) {
        Some(b'}') => return Ok((Part::Simple(s[start..=j].to_string()), j + 1)),
        Some(b',') => {}
        _ => return Err(IcuError),
    }
    let kind = match kind_word {
        "plural" => Kind::Plural,
        "selectordinal" => Kind::SelectOrdinal,
        "select" => Kind::Select,
        _ => {
            // number, date, time, ...: opaque, brace balanced, quotes skipped
            let mut level = 1usize;
            let mut k = j + 1;
            while k < b.len() && level > 0 {
                match b[k] {
                    b'{' => level += 1,
                    b'}' => level -= 1,
                    b'\'' => match s[k + 1..].find('\'') {
                        Some(p) => k = k + 1 + p,
                        None => k = b.len(),
                    },
                    _ => {}
                }
                k += 1;
            }
            if level > 0 {
                return Err(IcuError);
            }
            return Ok((Part::Simple(s[start..k].to_string()), k));
        }
    };
    j = skip_ws(s, j + 1);
    if kind != Kind::Select {
        j = offset(s, j).unwrap_or(j);
    }
    let head = s[start..j].to_string();
    let mut branches = Vec::new();
    loop {
        if j >= b.len() {
            return Err(IcuError);
        }
        if b[j] == b'}' {
            break;
        }
        let sel_end = selector_end(s, j).ok_or(IcuError)?;
        let selector = s[j..sel_end].to_string();
        let k = skip_ws(s, sel_end);
        if b.get(k) != Some(&b'{') {
            return Err(IcuError);
        }
        let (msg, k2) = message(s, k + 1, depth + 1, kind != Kind::Select, true)?;
        if b.get(k2) != Some(&b'}') {
            return Err(IcuError);
        }
        let t = skip_ws(s, k2 + 1);
        branches.push(Branch { selector, mid: s[sel_end..k].to_string(), msg, trail: s[k2 + 1..t].to_string() });
        j = t;
    }
    if branches.is_empty() {
        return Err(IcuError);
    }
    Ok((Part::Complex(Complex { head, kind, branches }), j + 1))
}

/// `offset\s*:\s*\d+\s*` at `j`: the index after it.
fn offset(s: &str, j: usize) -> Option<usize> {
    let rest = s.get(j..)?.strip_prefix("offset")?;
    let mut k = skip_ws(s, s.len() - rest.len());
    if s.as_bytes().get(k) != Some(&b':') {
        return None;
    }
    k = skip_ws(s, k + 1);
    let d0 = k;
    while s.as_bytes().get(k).is_some_and(u8::is_ascii_digit) {
        k += 1;
    }
    if k == d0 {
        return None;
    }
    Some(skip_ws(s, k))
}

/// End of a selector at `j`: `=-?\d+(\.\d+)?` first, else any run without white space or braces.
fn selector_end(s: &str, j: usize) -> Option<usize> {
    let b = s.as_bytes();
    if b.get(j) == Some(&b'=') {
        let mut k = j + 1;
        if b.get(k) == Some(&b'-') {
            k += 1;
        }
        let d0 = k;
        while b.get(k).is_some_and(u8::is_ascii_digit) {
            k += 1;
        }
        if k > d0 {
            if b.get(k) == Some(&b'.') && b.get(k + 1).is_some_and(u8::is_ascii_digit) {
                k += 1;
                while b.get(k).is_some_and(u8::is_ascii_digit) {
                    k += 1;
                }
            }
            return Some(k);
        }
    }
    let mut k = j;
    while let Some(c) = ch(s, k) {
        if is_ws(c) || c == '{' || c == '}' {
            break;
        }
        k += c.len_utf8();
    }
    (k > j).then_some(k)
}

/// True when the level has a letter or digit in its text or simple arguments (tokens do not count). With `locos`
/// a literal `⟦x<digits>⟧` in the text is a token too and does not count (LocOS `_translatable`), so a message
/// that is only such a token has no unit.
pub fn translatable(parts: &[Part], locos: bool) -> bool {
    parts.iter().any(|p| match p {
        Part::Text(t) | Part::Simple(t) => has_words(t, locos),
        _ => false,
    })
}

/// A letter or digit in `t`, after removing LocOS argument tokens when `locos`.
pub fn has_words(t: &str, locos: bool) -> bool {
    const OPEN: &str = "\u{27E6}x";
    const CLOSE: char = '\u{27E7}';
    if !locos || !t.contains(OPEN) {
        return bilingua_model::py_has_alnum(t);
    }
    let mut rest = t;
    let mut kept = String::new();
    while let Some(at) = rest.find(OPEN) {
        let after = &rest[at + OPEN.len()..];
        let d = after.chars().take_while(|c| bilingua_model::py_isdecimal_char(*c)).map(char::len_utf8).sum::<usize>();
        if d > 0 && after[d..].starts_with(CLOSE) {
            kept.push_str(&rest[..at]);
            rest = &after[d + CLOSE.len_utf8()..];
        } else {
            let skip = at + '\u{27E6}'.len_utf8();
            kept.push_str(&rest[..skip]);
            rest = &rest[skip..];
        }
    }
    kept.push_str(rest);
    bilingua_model::py_has_alnum(&kept)
}
