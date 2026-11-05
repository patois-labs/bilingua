//! The INI scan: every `key = value` entry with the span of its raw value, its section and its comment.

use regex::Regex;
use std::rc::Rc;
use std::sync::OnceLock;

/// How a quoted value spells a quote inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quote {
    /// The value is not wrapped in double quotes.
    None,
    /// Wrapped in quotes, a quote inside is `\"`.
    Backslash,
    /// Wrapped in quotes, a quote inside is the legacy Joomla `"_QQ_"`.
    Qq,
}

impl Quote {
    pub fn code(self) -> &'static str {
        match self {
            Quote::None => "",
            Quote::Backslash => "bs",
            Quote::Qq => "qq",
        }
    }
    pub fn from_code(s: &str) -> Quote {
        match s {
            "bs" => Quote::Backslash,
            "qq" => Quote::Qq,
            _ => Quote::None,
        }
    }
}

/// One entry of the file.
#[derive(Debug, Clone)]
pub struct Entry {
    pub key: String,
    /// The value with its quote escapes resolved.
    pub value: String,
    /// Byte span of the raw value in the text (inside the quotes for a quoted value).
    pub start: usize,
    pub end: usize,
    /// Comment lines above the entry (each without its `;` or `#`, empty ones left out).
    pub comments: Vec<String>,
    pub section: Rc<str>,
    pub quote: Quote,
}

/// Python's `str.isspace` for one character: Unicode white space plus the four information separators.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

pub fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// Splits a value into (leading white space, core, trailing white space). A value with no core is all "lead".
pub fn split_ws(s: &str) -> (&str, &str, &str) {
    let core = trim(s);
    if core.is_empty() {
        return (s, "", "");
    }
    let lead_len = s.len() - s.trim_start_matches(is_space).len();
    let trail_start = s.trim_end_matches(is_space).len();
    (&s[..lead_len], core, &s[trail_start..])
}

fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{[^{}]*\}|%(?:\d+\$)?[-#+0-9.]*(?:hh|ll|[hlqLzjt])?[a-zA-Z@]").expect("placeholder pattern"))
}

/// A value with a letter or digit outside its placeholders. `{0}`, `%1$@` and `---` are not translatable.
pub fn is_translatable(value: &str) -> bool {
    if !value.chars().any(char::is_alphanumeric) {
        return false;
    }
    placeholder_re().replace_all(value, "").chars().any(char::is_alphanumeric)
}

/// Natural lines as (start, end) byte spans, terminator excluded. `\r\n`, `\r` and `\n` end a line.
struct Lines<'a> {
    b: &'a [u8],
    pos: usize,
}

impl Iterator for Lines<'_> {
    type Item = (usize, usize);
    fn next(&mut self) -> Option<(usize, usize)> {
        let n = self.b.len();
        if self.pos >= n {
            return None;
        }
        let start = self.pos;
        let mut eol = start;
        while eol < n && self.b[eol] != b'\n' && self.b[eol] != b'\r' {
            eol += 1;
        }
        self.pos = if eol + 1 < n && self.b[eol] == b'\r' && self.b[eol + 1] == b'\n' { eol + 2 } else { eol + 1 };
        Some((start, eol))
    }
}

pub fn scan(text: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut comments: Vec<String> = Vec::new();
    let mut section: Rc<str> = Rc::from("");
    for (s, e) in (Lines { b: text.as_bytes(), pos: 0 }) {
        let line = &text[s..e];
        let body = trim(line);
        let Some(first) = body.chars().next() else {
            comments.clear();
            continue;
        };
        if first == ';' || first == '#' {
            let c = trim(&body[1..]);
            if !c.is_empty() {
                comments.push(c.to_string());
            }
            continue;
        }
        if body.starts_with('[') && body.ends_with(']') && body.len() >= 2 {
            section = Rc::from(trim(&body[1..body.len() - 1]));
            comments.clear();
            continue;
        }
        let Some(eq) = line.find('=') else {
            comments.clear();
            continue;
        };
        let key = trim(&line[..eq]).to_string();
        let bytes = line.as_bytes();
        let mut vs = eq + 1;
        let mut ve = bytes.len();
        while vs < ve && (bytes[vs] == b' ' || bytes[vs] == b'\t') {
            vs += 1;
        }
        while ve > vs && (bytes[ve - 1] == b' ' || bytes[ve - 1] == b'\t') {
            ve -= 1;
        }
        let raw = &line[vs..ve];
        let (value, start, end, quote) = if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
            let inner = &raw[1..raw.len() - 1];
            let value = inner.replace("\"_QQ_\"", "\"").replace("\\\"", "\"");
            let quote = if inner.contains("\"_QQ_\"") && !inner.contains("\\\"") { Quote::Qq } else { Quote::Backslash };
            (value, s + vs + 1, s + ve - 1, quote)
        } else {
            (raw.to_string(), s + vs, s + ve, Quote::None)
        };
        entries.push(Entry { key, value, start, end, comments: std::mem::take(&mut comments), section: Rc::clone(&section), quote });
    }
    entries
}

/// A translated value written back in the source value's own quote dialect. INI values are one line, so a line
/// break in the translation becomes a space.
pub fn encode_value(new: &str, quote: Quote) -> String {
    let one_line = new.replace("\r\n", "\n").replace('\r', "\n").replace('\n', " ");
    match quote {
        Quote::None => one_line,
        Quote::Qq => one_line.replace('"', "\"_QQ_\""),
        // Undo an already-escaped quote first so `\"` in a translation is not doubled.
        Quote::Backslash => one_line.replace("\\\"", "\"").replace('"', "\\\""),
    }
}
