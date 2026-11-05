//! The hand-written DTD scanner. Not an XML or DTD parser: nothing is expanded, resolved or fetched.
//!
//! Grammar: XML 1.0 section 4.2 (EntityDecl), 4.1 (references) and 2.5 (comments). Only the value of a general
//! internal entity (`<!ENTITY name "value">`) is of interest; every other declaration is skipped quote-aware.

use bilingua_model::Error;
use regex::Regex;
use std::sync::OnceLock;

/// One general internal entity declaration.
#[derive(Debug, Clone)]
pub struct Entity {
    pub name: String,
    /// Byte span of the raw value between the quotes.
    pub start: usize,
    pub end: usize,
    pub quote: char,
    /// Comments directly above the declaration (no blank line between).
    pub notes: Vec<String>,
}

fn err(text: &str, pos: usize, what: &str) -> Error {
    let line = text.as_bytes()[..pos].iter().filter(|&&b| b == b'\n').count() + 1;
    Error::Parse { format: "dtd".into(), msg: format!("this .dtd file has {what} (line {line})") }
}

/// Python `str.isspace` (what the LocOS regex `\s` accepts), so the two scanners agree on the same files.
pub fn is_ws(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == ':'
}

/// The LocOS name class `[-A-Za-z0-9._:·-￿]`.
fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | ':') || ('\u{b7}'..='\u{ffff}').contains(&c)
}

/// End of a Name starting at `pos`, or `None`.
fn name_end(text: &str, pos: usize) -> Option<usize> {
    let mut it = text[pos..].char_indices();
    let (_, c) = it.next()?;
    if !name_start(c) {
        return None;
    }
    for (i, c) in it {
        if !name_char(c) {
            return Some(pos + i);
        }
    }
    Some(text.len())
}

fn skip_ws(text: &str, pos: usize) -> usize {
    for (i, c) in text[pos..].char_indices() {
        if !is_ws(c) {
            return pos + i;
        }
    }
    text.len()
}

/// End (exclusive) of a markup declaration starting at `pos` (`<!`), quote-aware.
fn skip_decl(text: &str, pos: usize) -> Result<usize, Error> {
    let b = text.as_bytes();
    let mut i = pos + 2;
    let mut quote = 0u8;
    while i < b.len() {
        let c = b[i];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return Ok(i + 1);
        }
        i += 1;
    }
    Err(err(text, pos, "an unterminated declaration"))
}

/// Number of line breaks (`\r\n` counts once) in a run of white space.
fn breaks(run: &[u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < run.len() {
        match run[i] {
            b'\r' => {
                n += 1;
                if run.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
            }
            b'\n' => n += 1,
            _ => {}
        }
        i += 1;
    }
    n
}

/// Every general internal entity in order. Errors are typed `Error::Parse`.
pub fn scan(text: &str) -> Result<Vec<Entity>, Error> {
    let b = text.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut i = 0;
    while i < n {
        let c = b[i];
        if matches!(c, b' ' | b'\t' | b'\r' | b'\n') {
            let mut j = i;
            while j < n && matches!(b[j], b' ' | b'\t' | b'\r' | b'\n') {
                j += 1;
            }
            if breaks(&b[i..j]) >= 2 {
                notes.clear(); // a blank line detaches a note from the next entity
            }
            i = j;
            continue;
        }
        if text[i..].starts_with("<!--") {
            let Some(rel) = text[i + 4..].find("-->") else { return Err(err(text, i, "an unterminated comment")) };
            let body = text[i + 4..i + 4 + rel].trim();
            if !body.is_empty() {
                notes.push(body.to_string());
            }
            i += 4 + rel + 3;
            continue;
        }
        if text[i..].starts_with("<!ENTITY") {
            let malformed = || err(text, i, "a malformed ENTITY declaration");
            let mut j = i + 8;
            let after = skip_ws(text, j);
            if after == j {
                return Err(malformed());
            }
            j = after;
            let mut pe = false;
            if b.get(j) == Some(&b'%') {
                let k = skip_ws(text, j + 1);
                if k == j + 1 {
                    return Err(malformed());
                }
                pe = true;
                j = k;
            }
            let Some(ne) = name_end(text, j) else { return Err(malformed()) };
            let name = &text[j..ne];
            let k = skip_ws(text, ne);
            if k == ne {
                return Err(malformed());
            }
            j = k;
            let q = b.get(j).copied().unwrap_or(0);
            if pe || (q != b'"' && q != b'\'') {
                // parameter entity, or an external entity (SYSTEM / PUBLIC): kept as is
                i = skip_decl(text, i)?;
                notes.clear();
                continue;
            }
            let Some(rel) = b[j + 1..].iter().position(|&x| x == q) else {
                return Err(err(text, i, "an unterminated entity value"));
            };
            let close_q = j + 1 + rel;
            let mut close = close_q + 1;
            while close < n && matches!(b[close], b' ' | b'\t' | b'\r' | b'\n') {
                close += 1;
            }
            if close >= n || b[close] != b'>' {
                return Err(malformed());
            }
            out.push(Entity { name: name.to_string(), start: j + 1, end: close_q, quote: q as char, notes: std::mem::take(&mut notes) });
            i = close + 1;
            continue;
        }
        if text[i..].starts_with("<?") {
            let Some(rel) = text[i + 2..].find("?>") else { return Err(err(text, i, "an unterminated processing instruction")) };
            i += 2 + rel + 2;
            notes.clear();
            continue;
        }
        if text[i..].starts_with("<!") {
            i = skip_decl(text, i)?;
            notes.clear();
            continue;
        }
        if c == b'%' {
            if let Some(e) = name_end(text, i + 1) {
                if b.get(e) == Some(&b';') {
                    i = e + 1;
                    notes.clear();
                    continue;
                }
            }
        }
        return Err(err(text, i, "unexpected text outside a declaration"));
    }
    Ok(out)
}

const NAME: &str = r"[A-Za-z_:][-A-Za-z0-9._:\u{b7}-\u{ffff}]*";

fn ref_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!("&(#[0-9]+|#x[0-9A-Fa-f]+|{NAME});")).expect("ref regex"))
}

fn protected_ref() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!("&{NAME};")).expect("protected regex"))
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"</?[A-Za-z][^<>]*>").expect("tag regex"))
}

/// What the translator sees as code: markup, escaped markup (`&lt;i>`), or an entity reference.
pub fn token_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"</?[A-Za-z][^<>]*>|&lt;/?[A-Za-z][^<>&]*>|&{NAME};")).expect("token regex"))
}

fn char_ref(r: &str) -> Option<char> {
    let cp = match r.strip_prefix("#x") {
        Some(h) => u32::from_str_radix(h, 16).ok()?,
        None => r[1..].parse::<u32>().ok()?,
    };
    if matches!(cp, 0x9 | 0xA | 0xD) || (0x20..=0xD7FF).contains(&cp) || (0xE000..=0xFFFD).contains(&cp) || (0x10000..=0x10FFFF).contains(&cp) {
        char::from_u32(cp)
    } else {
        None
    }
}

/// Raw EntityValue to translator text: character references and `&amp; &quot; &apos;` become characters; every
/// other reference stays as written.
pub fn unit_text(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut pos = 0;
    for m in ref_re().captures_iter(raw) {
        let whole = m.get(0).expect("match");
        out.push_str(&raw[pos..whole.start()]);
        let r = &m[1];
        let rep = if r.starts_with('#') {
            char_ref(r)
        } else {
            match r {
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => None,
            }
        };
        match rep {
            Some(c) => out.push(c),
            None => out.push_str(whole.as_str()),
        }
        pos = whole.end();
    }
    out.push_str(&raw[pos..]);
    out
}

/// Translator text to raw EntityValue. Entity references the text carries stay references; every other `&` is
/// `&amp;`, `%` is `&#037;` (a bare `%` would start a parameter-entity reference) and the value's own quote is
/// `&quot;` / `&apos;`.
pub fn encode_value(new: &str, quote: char) -> String {
    let mut out = String::with_capacity(new.len() + 8);
    let mut pos = 0;
    for m in protected_ref().find_iter(new) {
        escape(&new[pos..m.start()], quote, &mut out);
        out.push_str(m.as_str());
        pos = m.end();
    }
    escape(&new[pos..], quote, &mut out);
    out
}

fn escape(s: &str, quote: char, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '%' => out.push_str("&#037;"),
            c if c == quote => out.push_str(if quote == '"' { "&quot;" } else { "&apos;" }),
            c => out.push(c),
        }
    }
}

/// A value with a letter or digit outside references and markup.
pub fn is_translatable(value: &str) -> bool {
    let no_refs = protected_ref().replace_all(value, "");
    tag_re().replace_all(&no_refs, "").chars().any(char::is_alphanumeric)
}

/// Splits leading and trailing white space (Python `str.strip` semantics) from the core.
pub fn split_ws(s: &str) -> (&str, &str, &str) {
    let core = s.trim_matches(is_ws);
    if core.is_empty() {
        return (s, "", "");
    }
    let lead = s.len() - s.trim_start_matches(is_ws).len();
    let trail = s.len() - s.trim_end_matches(is_ws).len();
    (&s[..lead], core, &s[s.len() - trail..])
}
