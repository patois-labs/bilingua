//! YAML front matter: a block between `---` lines at the very start of the file. Only the keys in `KEYS`, or the
//! keys `markdown.fm_keys` lists (case-insensitive), as top-level one-line scalars, become units; everything else
//! stays in the skeleton.

use std::ops::Range;

/// Front matter keys that are translated.
pub const KEYS: &[&str] = &["title", "description"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Plain,
    Single,
    Double,
}

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Style::Plain => "plain",
            Style::Single => "single",
            Style::Double => "double",
        }
    }
    pub fn from_name(s: &str) -> Style {
        match s {
            "single" => Style::Single,
            "double" => Style::Double,
            _ => Style::Plain,
        }
    }
}

pub struct Value {
    pub key: String,
    /// The bytes that are replaced when the value is written: the scalar for plain, the inside of the quotes
    /// for quoted values.
    pub range: Range<usize>,
    pub style: Style,
    /// The text as a person reads it (quotes and escapes removed).
    pub text: String,
}

pub struct FrontMatter {
    /// Byte offset just after the closing line.
    pub end: usize,
    pub values: Vec<Value>,
}

pub fn parse(text: &str) -> Option<FrontMatter> {
    parse_with(text, None)
}

/// [`parse`] with the keys of `markdown.fm_keys` (lower case) in place of `KEYS`.
pub fn parse_with(text: &str, keys: Option<&[String]>) -> Option<FrontMatter> {
    let first_len = if text.starts_with("---\r\n") {
        5
    } else if text.starts_with("---\n") {
        4
    } else {
        return None;
    };
    let mut pos = first_len;
    let mut values = Vec::new();
    loop {
        if pos >= text.len() {
            return None; // never closed: not front matter
        }
        let line_end = text[pos..].find('\n').map(|n| pos + n + 1).unwrap_or(text.len());
        let line = &text[pos..line_end];
        let bare = line.trim_end_matches(['\n', '\r']);
        if bare == "---" || bare == "..." {
            return Some(FrontMatter { end: line_end, values });
        }
        if let Some(v) = parse_line(text, pos, bare, keys) {
            // a plain value that goes on over the next indented line is folded text: leave it alone
            if !(v.style == Style::Plain && plain_continues(&text[line_end..])) {
                values.push(v);
            }
        }
        pos = line_end;
    }
}

fn parse_line(text: &str, line_start: usize, line: &str, keys: Option<&[String]>) -> Option<Value> {
    let colon = line.find(':')?;
    let key = &line[..colon];
    let allowed = match keys {
        Some(k) => k.iter().any(|k| *k == key.to_lowercase()),
        None => KEYS.contains(&key),
    };
    if !allowed {
        return None;
    }
    let after = &line[colon + 1..];
    if !after.starts_with(' ') {
        return None;
    }
    let lead = after.len() - after.trim_start_matches(' ').len();
    let vstart = line_start + colon + 1 + lead;
    let rest = &text[vstart..line_start + line.len()];
    let first = rest.chars().next()?;
    match first {
        '"' => {
            let mut esc = false;
            let mut close = None;
            for (i, c) in rest.char_indices().skip(1) {
                if esc {
                    esc = false;
                } else if c == '\\' {
                    esc = true;
                } else if c == '"' {
                    close = Some(i);
                    break;
                }
            }
            let close = close?;
            if !tail_ok(&rest[close + 1..]) {
                return None;
            }
            let inner = &rest[1..close];
            // only \" and \\ are understood; any other escape means we leave the value alone
            let mut out = String::new();
            let mut it = inner.chars();
            while let Some(c) = it.next() {
                if c == '\\' {
                    match it.next()? {
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        _ => return None,
                    }
                } else {
                    out.push(c);
                }
            }
            Some(Value { key: key.into(), range: vstart + 1..vstart + close, style: Style::Double, text: out })
        }
        '\'' => {
            let b = rest.as_bytes();
            let mut i = 1;
            let close = loop {
                if i >= b.len() {
                    return None;
                }
                if b[i] == b'\'' {
                    if b.get(i + 1) == Some(&b'\'') {
                        i += 2;
                        continue;
                    }
                    break i;
                }
                i += 1;
            };
            if !tail_ok(&rest[close + 1..]) {
                return None;
            }
            Some(Value {
                key: key.into(),
                range: vstart + 1..vstart + close,
                style: Style::Single,
                text: rest[1..close].replace("''", "'"),
            })
        }
        '|' | '>' | '[' | '{' | '&' | '*' | '!' | '%' | '@' | '`' | '#' => None,
        _ => {
            // plain scalar up to a " #" comment
            let cut = rest.find(" #").unwrap_or(rest.len());
            let scalar = rest[..cut].trim_end();
            if scalar.is_empty() {
                return None;
            }
            Some(Value { key: key.into(), range: vstart..vstart + scalar.len(), style: Style::Plain, text: scalar.to_string() })
        }
    }
}

/// Whether the next non-blank line of `rest` is an indented, non-comment line (a plain scalar's continuation).
fn plain_continues(rest: &str) -> bool {
    rest.lines()
        .find(|l| !l.trim().is_empty())
        .is_some_and(|l| l.starts_with([' ', '\t']) && !l.trim_start().starts_with('#'))
}

fn tail_ok(t: &str) -> bool {
    let t = t.trim_start();
    t.is_empty() || t.starts_with('#')
}

/// Text to write for a value in the original style (plain values switch to double quotes when needed).
/// Returns the new bytes for `Value::range`; a plain value that needs quotes includes them.
pub fn write(text: &str, style: Style) -> String {
    match style {
        Style::Double => escape_double(text),
        Style::Single => text.replace('\'', "''").replace(['\n', '\r'], " "),
        Style::Plain => {
            if needs_quotes(text) {
                format!("\"{}\"", escape_double(text))
            } else {
                text.to_string()
            }
        }
    }
}

fn escape_double(t: &str) -> String {
    t.replace('\\', "\\\\").replace('"', "\\\"").replace(['\n', '\r'], " ")
}

fn needs_quotes(t: &str) -> bool {
    if t.is_empty() || t != t.trim() || t.contains(": ") || t.contains(" #") || t.ends_with(':') || t.contains('\n') || t.contains('\r') {
        return true;
    }
    let first = t.chars().next().unwrap_or(' ');
    if "-?:,[]{}#&*!|>'\"%@`".contains(first) {
        return true;
    }
    let l = t.to_ascii_lowercase();
    if ["true", "false", "yes", "no", "on", "off", "null", "~", "y", "n"].contains(&l.as_str()) {
        return true;
    }
    t.parse::<f64>().is_ok()
}
