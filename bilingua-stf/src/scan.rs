//! The STF line walker: headers, sections, column headers, data rows and the escape and placeholder spans of a
//! label. Written by hand (no regex) after the Localization OS filter; where Python's `re` and `str` rules differ
//! from Rust's (case folding of `i`, `s`, `k`; `strip`; `\d`) the Python rule is followed.

use bilingua_model::{py_isdecimal_char, py_isspace_char, py_strip};
use std::rc::Rc;

/// A header line value: where it sits in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub start: usize,
    pub end: usize,
    pub value: String,
}

/// One data row of the file.
#[derive(Debug)]
pub struct Row<'t> {
    pub key: &'t str,
    pub section: Rc<str>,
    /// Index of the LABEL cell and of the cell a translation goes into.
    pub src: usize,
    pub dst: usize,
    /// Start of the line and end of its content (before the line break).
    pub start: usize,
    pub line_end: usize,
    pub cells: Vec<&'t str>,
    /// The label has text outside its placeholders: the row is a unit.
    pub has_text: bool,
}

impl Row<'_> {
    /// Absolute byte span of cell `idx`; a missing cell is an empty span at the end of the line.
    pub fn cell_span(&self, idx: usize) -> (usize, usize) {
        let mut off = self.start;
        for (i, c) in self.cells.iter().enumerate() {
            if i == idx {
                return (off, off + c.len());
            }
            off += c.len() + 1;
        }
        (self.line_end, self.line_end)
    }
}

/// `re.I` equivalence of one character with the ASCII lowercase letter `l` (Python also folds the Kelvin sign to
/// `k`, long s to `s`, and the dotted and dotless i to `i`).
fn ci(c: char, l: char) -> bool {
    c == l
        || c == l.to_ascii_uppercase()
        || match l {
            'i' => c == '\u{130}' || c == '\u{131}',
            's' => c == '\u{17f}',
            'k' => c == '\u{212a}',
            _ => false,
        }
}

/// Byte length of the prefix of `s` that matches `word` (lowercase ASCII) ignoring case, as `re.I` does.
fn match_ci(s: &str, word: &str) -> Option<usize> {
    let mut it = s.char_indices();
    let mut end = 0;
    for w in word.chars() {
        let (i, c) = it.next()?;
        if !ci(c, w) {
            return None;
        }
        end = i + c.len_utf8();
    }
    Some(end)
}

fn skip_blanks(s: &str, from: usize) -> usize {
    from + s.as_bytes()[from..].iter().take_while(|b| matches!(b, b' ' | b'\t')).count()
}

/// `^(Language code|Type|Translation type)[ \t]*:[ \t]*(.*?)[ \t]*$`, case-insensitive. Returns the header name as
/// Python's `lower()` makes it, and the span of the value.
pub fn parse_header(line: &str) -> Option<(String, usize, usize)> {
    for word in ["language code", "type", "translation type"] {
        let Some(p) = match_ci(line, word) else { continue };
        let q = skip_blanks(line, p);
        if line.as_bytes().get(q) != Some(&b':') {
            continue;
        }
        let vstart = skip_blanks(line, q + 1);
        let mut vend = line.len();
        while vend > vstart && matches!(line.as_bytes()[vend - 1], b' ' | b'\t') {
            vend -= 1;
        }
        return Some((line[..p].to_lowercase(), vstart, vend));
    }
    None
}

/// `^-{3,}[ \t]*([A-Za-z][A-Za-z ]{0,60}?)[ \t]*-{3,}[ \t]*$`: the section name, lowercased with runs of spaces
/// collapsed.
pub fn parse_section(line: &str) -> Option<String> {
    let b = line.as_bytes();
    let dashes = b.iter().take_while(|&&c| c == b'-').count();
    if dashes < 3 {
        return None;
    }
    let s = skip_blanks(line, dashes);
    if !b.get(s).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let run = b[s..].iter().take_while(|&&c| c.is_ascii_alphabetic() || c == b' ').count();
    let mut e = s + run;
    while e > s && b[e - 1] == b' ' {
        e -= 1;
    }
    if e - s > 61 {
        return None;
    }
    let after = skip_blanks(line, s + run);
    let d2 = b[after..].iter().take_while(|&&c| c == b'-').count();
    if d2 < 3 || !b[after + d2..].iter().all(|&c| c == b' ' || c == b'\t') {
        return None;
    }
    let mut name = String::with_capacity(e - s);
    let mut prev_space = false;
    for &c in &b[s..e] {
        if c == b' ' {
            if !prev_space {
                name.push(' ');
            }
            prev_space = true;
        } else {
            name.push(c.to_ascii_lowercase() as char);
            prev_space = false;
        }
    }
    Some(name)
}

/// `^#[ \t]*KEY\t` (case-insensitive) on a comment line, then the upper-cased, stripped column names.
pub fn parse_columns(line: &str) -> Option<Vec<String>> {
    let rest = line.strip_prefix('#')?;
    let q = skip_blanks(rest, 0);
    let k = match_ci(&rest[q..], "key")?;
    if rest.as_bytes().get(q + k) != Some(&b'\t') {
        return None;
    }
    Some(rest.split('\t').map(|c| py_strip(c).to_uppercase()).collect())
}

/// One walk over the decoded text: header spans and data rows in file order.
pub struct Walker<'t> {
    text: &'t str,
    pos: usize,
    section: Rc<str>,
    /// LABEL and TRANSLATION cell indexes of the current column header, when it names TRANSLATION. Worked out once
    /// per column line, not per row: a column line of 200 000 names over a million rows must stay linear.
    columns: Option<(usize, usize)>,
    pub seen_data: bool,
    pub type_: String,
    /// First span of each header, by lowercased name.
    pub headers: Vec<(String, Header)>,
}

impl<'t> Walker<'t> {
    pub fn new(text: &'t str, pos: usize) -> Walker<'t> {
        Walker { text, pos, section: Rc::from(""), columns: None, seen_data: false, type_: String::new(), headers: Vec::new() }
    }

    pub fn header(&self, name: &str) -> Option<&Header> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, h)| h)
    }

    /// Reads the next line. `None` at the end of the text; `Some(None)` for a line that is not a data row;
    /// `Some(Some(row))` for a data row.
    pub fn step(&mut self) -> Option<Option<Row<'t>>> {
        let text = self.text;
        let b = text.as_bytes();
        if self.pos >= b.len() {
            return None;
        }
        let start = self.pos;
        let end = b[start..].iter().position(|&c| c == b'\r' || c == b'\n').map_or(b.len(), |p| start + p);
        self.pos = if end < b.len() {
            if b[end] == b'\r' && b.get(end + 1) == Some(&b'\n') {
                end + 2
            } else {
                end + 1
            }
        } else {
            end
        };
        let line = &text[start..end];
        if blank(line) {
            return Some(None);
        }
        if line.starts_with('#') {
            if let Some(c) = parse_columns(line) {
                self.columns = c.iter().position(|n| n == "TRANSLATION").map(|t| (c.iter().position(|n| n == "LABEL").unwrap_or(1), t));
            }
            return Some(None);
        }
        if let Some(name) = parse_section(line) {
            self.section = Rc::from(name);
            self.columns = None;
            return Some(None);
        }
        if !self.seen_data {
            if let Some((name, vs, ve)) = parse_header(line) {
                // Checked before the tab test: `Language code:<TAB>de` is a header, not a data row.
                let value = line[vs..ve].to_string();
                if name == "type" {
                    self.type_ = py_strip(&value).to_string();
                }
                if !self.headers.iter().any(|(n, _)| *n == name) {
                    self.headers.push((name, Header { start: start + vs, end: start + ve, value }));
                }
                return Some(None);
            }
        }
        if !line.contains('\t') {
            return Some(None); // stray line: kept byte for byte, never a unit
        }
        self.seen_data = true;
        let cells: Vec<&str> = line.split('\t').collect();
        let (mut src, mut dst) = (1usize, 1usize);
        if let Some((s, d)) = self.columns.filter(|_| &*self.section != "untranslated") {
            (src, dst) = (s, d);
        } else if &*self.section == "translated" && cells.len() >= 3 {
            src = 1;
            dst = 2;
        }
        if src >= cells.len() || blank(cells[0]) {
            return Some(None);
        }
        let section: Rc<str> = if self.section.is_empty() {
            Rc::from(if self.type_.is_empty() { "source".to_string() } else { self.type_.to_lowercase() })
        } else {
            self.section.clone()
        };
        let has_text = has_text(cells[src]);
        Some(Some(Row { key: cells[0], section, src, dst, start, line_end: end, cells, has_text }))
    }
}

/// The first header called `name` (lowercase) in `text`, found the way the walker finds it: only lines before the
/// first data row count. `pos` is where the walk starts (after a byte-order mark).
pub fn find_header(text: &str, pos: usize, name: &str) -> Option<Header> {
    let mut w = Walker::new(text, pos);
    while !w.seen_data {
        if w.step().is_none() {
            break;
        }
    }
    w.header(name).cloned()
}

// ---- escapes and placeholders -------------------------------------------------------------------------------

/// End of the placeholder starting at byte `i` of `s` (which must be `\` or `{`), if one starts there:
/// `\n \t \r \\`, `{0}` / `{12,number}` arguments and `{!merge.fields}`.
fn token_end(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    match b[i] {
        b'\\' => matches!(b.get(i + 1), Some(b'n' | b't' | b'r' | b'\\')).then_some(i + 2),
        b'{' => {
            let rest = &s[i + 1..];
            if let Some(body) = rest.strip_prefix('!') {
                let mut n = 0usize;
                for (j, c) in body.char_indices() {
                    match c {
                        '}' => return (n >= 1).then_some(i + 2 + j + 1),
                        '{' | '\r' | '\n' => return None,
                        _ => {
                            n += 1;
                            if n > 200 {
                                return None;
                            }
                        }
                    }
                }
                return None;
            }
            let (mut digits, mut q) = (0usize, 0usize);
            for c in rest.chars() {
                if !py_isdecimal_char(c) {
                    break;
                }
                digits += 1;
                q += c.len_utf8();
                if digits > 3 {
                    return None;
                }
            }
            if digits == 0 {
                return None;
            }
            let tail = &rest[q..];
            if tail.starts_with('}') {
                return Some(i + 1 + q + 1);
            }
            let args = tail.strip_prefix(',')?;
            let mut n = 0usize;
            for (j, c) in args.char_indices() {
                match c {
                    '}' => return Some(i + 1 + q + 1 + j + 1),
                    '{' | '\r' | '\n' => return None,
                    _ => {
                        n += 1;
                        if n > 60 {
                            return None;
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// The placeholder spans of a label, in order.
pub fn token_spans(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if matches!(b[i], b'\\' | b'{') {
            if let Some(e) = token_end(s, i) {
                out.push((i, e));
                i = e;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// True when the label has text (not only white space) outside its placeholders.
pub fn has_text(s: &str) -> bool {
    let b = s.as_bytes();
    let (mut pos, mut i) = (0usize, 0usize);
    while i < b.len() {
        if matches!(b[i], b'\\' | b'{') {
            if let Some(e) = token_end(s, i) {
                if !blank(&s[pos..i]) {
                    return true;
                }
                pos = e;
                i = e;
                continue;
            }
        }
        i += 1;
    }
    !blank(&s[pos..])
}

/// Python's `not s.strip()`: empty or only white space.
fn blank(s: &str) -> bool {
    s.chars().all(py_isspace_char)
}
