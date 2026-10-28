//! Line-level PO parser. Keeps byte ranges so the writer can replace only `msgstr` fields.

use bilingua_model::Error;

#[derive(Debug, Clone)]
pub struct Field {
    /// Start of the keyword line.
    pub start: usize,
    /// End of the last string line, without its line terminator.
    pub end: usize,
    pub value: String,
    /// Number of lines the field occupies (more than one = multi-line layout).
    pub lines: usize,
    /// 1-based line number of the keyword.
    pub line_no: usize,
}

impl Field {
    pub fn multiline(&self) -> bool {
        self.lines > 1
    }
}

#[derive(Debug, Clone, Default)]
pub struct Entry {
    pub translator: Vec<String>,
    pub extracted: Vec<String>,
    pub refs: Vec<String>,
    pub flags: Vec<String>,
    /// Line range of the first `#,` line, terminator included.
    pub flag_line: Option<(usize, usize)>,
    pub ctxt: Option<Field>,
    pub id: Option<Field>,
    pub id_plural: Option<Field>,
    /// `msgstr` (index None) or `msgstr[N]`.
    pub strs: Vec<(Option<usize>, Field)>,
}

#[derive(Debug)]
pub struct Parsed {
    pub entries: Vec<Entry>,
}

#[derive(PartialEq, Clone, Copy)]
enum Last {
    None,
    Ctxt,
    Id,
    IdPlural,
    Str,
}

pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('a') => out.push('\u{7}'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('v') => out.push('\u{b}'),
            Some('x') => {
                let mut v = 0u32;
                let mut n = 0;
                while let Some(d) = it.peek().and_then(|c| c.to_digit(16)) {
                    if n == 2 {
                        break;
                    }
                    v = v * 16 + d;
                    n += 1;
                    it.next();
                }
                if n == 0 {
                    out.push_str("\\x");
                } else {
                    out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
                }
            }
            Some(d @ '0'..='7') => {
                let mut v = d.to_digit(8).unwrap_or(0);
                let mut n = 1;
                while let Some(d) = it.peek().and_then(|c| c.to_digit(8)) {
                    if n == 3 {
                        break;
                    }
                    v = v * 8 + d;
                    n += 1;
                    it.next();
                }
                out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\u{b}' => out.push_str("\\v"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\{:03o}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn perr(line: usize, msg: &str) -> Error {
    Error::Parse { format: "po".into(), msg: format!("line {line}: {msg}") }
}

/// The quoted string at the start of `rest` (after trimming), unescaped. `None` when it is not a string.
fn quoted(rest: &str) -> Option<String> {
    let t = rest.trim();
    let inner = t.strip_prefix('"')?.strip_suffix('"')?;
    if t.len() < 2 {
        return None;
    }
    Some(unescape(inner))
}

#[allow(unused_assignments)]
pub fn parse(text: &str) -> Result<Parsed, Error> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut cur = Entry::default();
    let mut last = Last::None;
    let mut has_content = false;
    let mut after_str = false;
    let mut pos = 0;
    let mut line_no = 0;

    macro_rules! finish {
        () => {
            if has_content {
                entries.push(std::mem::take(&mut cur));
            }
            cur = Entry::default();
            last = Last::None;
            has_content = false;
            after_str = false;
        };
    }

    for raw in text.split_inclusive('\n') {
        line_no += 1;
        let start = pos;
        pos += raw.len();
        let full_end = pos;
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let end = start + line.len();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            finish!();
            continue;
        }
        if trimmed.starts_with("#~") {
            // Obsolete entry: skeleton only.
            finish!();
            continue;
        }
        if let Some(c) = trimmed.strip_prefix('#') {
            if after_str {
                finish!();
            }
            has_content = true;
            let body = c.get(1..).unwrap_or("").trim().to_string();
            match c.chars().next() {
                Some('.') => cur.extracted.push(body),
                Some(':') => cur.refs.push(body),
                Some(',') => {
                    let flags: Vec<String> = body.split(',').map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect();
                    if cur.flag_line.is_none() && flags.iter().any(|f| f == "fuzzy") {
                        cur.flag_line = Some((start, full_end));
                    }
                    cur.flags.extend(flags);
                }
                Some('|') => {}
                _ => cur.translator.push(c.trim().to_string()),
            }
            continue;
        }
        if trimmed.starts_with('"') {
            let v = quoted(trimmed).ok_or_else(|| perr(line_no, "unterminated string"))?;
            let field = match last {
                Last::Ctxt => cur.ctxt.as_mut(),
                Last::Id => cur.id.as_mut(),
                Last::IdPlural => cur.id_plural.as_mut(),
                Last::Str => cur.strs.last_mut().map(|(_, f)| f),
                Last::None => return Err(perr(line_no, "string without a keyword")),
            };
            if let Some(f) = field {
                f.value.push_str(&v);
                f.end = end;
                f.lines += 1;
            }
            continue;
        }
        let (kw, rest) = match trimmed.find(|c: char| c.is_whitespace() || c == '"') {
            Some(p) => (&trimmed[..p], &trimmed[p..]),
            None => (trimmed, ""),
        };
        let v = quoted(rest).ok_or_else(|| perr(line_no, "expected a quoted string"))?;
        let field = Field { start, end, value: v, lines: 1, line_no };
        match kw {
            "msgctxt" | "msgid" => {
                if after_str {
                    finish!();
                }
                has_content = true;
                if kw == "msgctxt" {
                    cur.ctxt = Some(field);
                    last = Last::Ctxt;
                } else {
                    if cur.id.is_some() {
                        return Err(perr(line_no, "second msgid in one entry"));
                    }
                    cur.id = Some(field);
                    last = Last::Id;
                }
            }
            "msgid_plural" => {
                if cur.id.is_none() {
                    return Err(perr(line_no, "msgid_plural before msgid"));
                }
                cur.id_plural = Some(field);
                last = Last::IdPlural;
            }
            "msgstr" => {
                has_content = true;
                after_str = true;
                cur.strs.push((None, field));
                last = Last::Str;
            }
            k if k.starts_with("msgstr[") && k.ends_with(']') => {
                let n: usize = k[7..k.len() - 1].parse().map_err(|_| perr(line_no, "bad plural index"))?;
                has_content = true;
                after_str = true;
                cur.strs.push((Some(n), field));
                last = Last::Str;
            }
            other => return Err(perr(line_no, &format!("unknown keyword {other}"))),
        }
    }
    finish!();
    Ok(Parsed { entries })
}
