//! Records and fields: the reader follows Python's `csv.reader` (default dialect, non-strict, quote doubling) over
//! physical lines, keeps the raw text of every record, and finds the byte span of each field so that a record can be
//! rewritten with only the written cells changed.

use bilingua_model::{Error, LimitError, LimitKind};

/// Most columns a row may have (Localization OS `MAX_COLUMNS`).
pub const MAX_COLUMNS: usize = 1000;
/// Longest field in characters (Python's `csv.field_size_limit` default).
pub const FIELD_LIMIT: usize = 131_072;

/// One record: its byte range in the text (line ending included) and its field values.
pub struct Rec {
    pub start: usize,
    pub end: usize,
    pub row: Vec<String>,
}

fn too_wide() -> Error {
    LimitError::new(LimitKind::Spreadsheet, format!("This CSV file has a row with more than {MAX_COLUMNS} columns.")).into()
}

fn too_long() -> Error {
    LimitError::new(
        LimitKind::UnitChars,
        format!("This CSV file could not be read: field larger than field limit ({FIELD_LIMIT})."),
    )
    .into()
}

/// Python `str.isspace` for one character.
pub fn py_space(c: char) -> bool {
    bilingua_model::py_isspace_char(c)
}

/// Python `str.strip() == ""`.
pub fn blank(s: &str) -> bool {
    s.chars().all(py_space)
}

#[derive(Clone, Copy, PartialEq)]
enum S {
    Start,
    InField,
    InQuoted,
    QuoteIn,
}

/// Iterator over the records of a text. A record that ends the file inside an open quote runs to the end.
pub struct Records<'a> {
    text: &'a str,
    pos: usize,
    delim: u8,
}

impl<'a> Records<'a> {
    pub fn new(text: &'a str, delim: u8) -> Records<'a> {
        Records { text, pos: 0, delim }
    }
}

fn save(buf: &mut Vec<u8>, row: &mut Vec<String>) -> Result<(), Error> {
    let s = String::from_utf8(std::mem::take(buf)).unwrap_or_default();
    if s.len() > FIELD_LIMIT && s.chars().count() > FIELD_LIMIT {
        return Err(too_long());
    }
    row.push(s);
    if row.len() > MAX_COLUMNS {
        return Err(too_wide());
    }
    Ok(())
}

impl Iterator for Records<'_> {
    type Item = Result<Rec, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let b = self.text.as_bytes();
        let n = b.len();
        if self.pos >= n {
            return None;
        }
        let start = self.pos;
        let d = self.delim;
        let mut i = start;
        let mut s = S::Start;
        let mut row: Vec<String> = Vec::new();
        let mut buf: Vec<u8> = Vec::new();
        macro_rules! tr {
            ($e:expr) => {
                if let Err(e) = $e {
                    self.pos = n;
                    return Some(Err(e));
                }
            };
        }
        while i < n {
            let c = b[i];
            let term = c == b'\r' || c == b'\n';
            match s {
                S::Start | S::InField | S::QuoteIn if term => {
                    // a blank line is the empty row; any other record end saves the field it stands in
                    if !(s == S::Start && i == start) {
                        tr!(save(&mut buf, &mut row));
                    }
                    i += if c == b'\r' && b.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                    self.pos = i;
                    return Some(Ok(Rec { start, end: i, row }));
                }
                S::Start => {
                    if c == b'"' {
                        s = S::InQuoted;
                    } else if c == d {
                        tr!(save(&mut buf, &mut row));
                    } else {
                        buf.push(c);
                        s = S::InField;
                    }
                }
                S::InField => {
                    if c == d {
                        tr!(save(&mut buf, &mut row));
                        s = S::Start;
                    } else {
                        buf.push(c);
                    }
                }
                S::InQuoted => {
                    if c == b'"' {
                        s = S::QuoteIn;
                    } else {
                        buf.push(c);
                    }
                }
                S::QuoteIn => {
                    if c == b'"' {
                        buf.push(b'"');
                        s = S::InQuoted;
                    } else if c == d {
                        tr!(save(&mut buf, &mut row));
                        s = S::Start;
                    } else {
                        buf.push(c);
                        s = S::InField;
                    }
                }
            }
            if buf.len() > FIELD_LIMIT * 4 {
                self.pos = n;
                return Some(Err(too_long()));
            }
            i += 1;
        }
        // end of the text: the open field is saved (an unterminated quote keeps what it read)
        tr!(save(&mut buf, &mut row));
        self.pos = n;
        Some(Ok(Rec { start, end: n, row }))
    }
}

/// The row values of one record's raw text.
pub fn parse_row(raw: &str, delim: u8) -> Result<Vec<String>, Error> {
    match Records::new(raw, delim).next() {
        Some(r) => r.map(|r| r.row),
        None => Ok(Vec::new()),
    }
}

/// Splits a record's raw text into its body and its line ending.
pub fn split_ending(raw: &str) -> (&str, &str) {
    let body = raw.trim_end_matches(['\r', '\n']);
    (body, &raw[body.len()..])
}

/// Field byte spans of one record's text (RFC 4180 with quote doubling).
pub fn spans(body: &str, delim: u8) -> Vec<(usize, usize)> {
    let b = body.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        let start = i;
        if i < n && b[i] == b'"' {
            i += 1;
            while i < n {
                if b[i] == b'"' {
                    if i + 1 < n && b[i + 1] == b'"' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
        }
        while i < n && b[i] != delim {
            i += 1;
        }
        out.push((start, i));
        if i < n && b[i] == delim {
            i += 1;
            continue;
        }
        return out;
    }
}

/// A leading `= + - @`, TAB, CR or LF (after white space for the operators), or an apostrophe, gets an apostrophe in
/// front (OWASP; Localization OS `csv_safe_cell`). The full-width forms of the operators count too.
pub fn safe_cell(value: &str) -> String {
    let lead = value.trim_start_matches(py_space);
    if value.starts_with(['\'', '\t', '\r', '\n']) || lead.starts_with(['=', '+', '-', '@', '\u{ff1d}', '\u{ff0b}', '\u{ff0d}', '\u{ff20}']) {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

/// The value as a field: quoted (quotes doubled) when `force` or when it holds the delimiter, a quote or a line break.
pub fn quote(value: &str, delim: u8, force: bool) -> String {
    if force || value.bytes().any(|c| c == delim || c == b'"' || c == b'\r' || c == b'\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// The record with only the cells in `writes` changed; every other field keeps its exact bytes. A written cell
/// past the end of a short row pads the row with empty fields.
pub fn rewrite(raw: &str, row: &[String], delim: u8, writes: &[(usize, String)]) -> String {
    let (body, ending) = split_ending(raw);
    let sp = spans(body, delim);
    let mut fields: Vec<String> = if sp.len() == row.len() {
        sp.iter().map(|&(a, b)| body[a..b].to_string()).collect()
    } else {
        // the two readings disagree: never guess, write the row again
        row.iter().map(|v| quote(v, delim, false)).collect()
    };
    let width = writes.iter().map(|(i, _)| i + 1).chain([fields.len()]).max().unwrap_or(0);
    fields.resize(width, String::new());
    for (i, value) in writes {
        let was_quoted = fields[*i].starts_with('"');
        fields[*i] = quote(&safe_cell(value), delim, was_quoted);
    }
    let sep = (delim as char).to_string();
    let mut out = fields.join(&sep);
    out.push_str(ending);
    out
}
