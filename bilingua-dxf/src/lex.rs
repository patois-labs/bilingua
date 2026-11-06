//! DXF text values: the inline codes (MTEXT format codes, `%%` specials, `^` controls, `\U+` escapes), the escaping
//! of a translation, the re-chunking of long MTEXT and the fit to a code page. Ports `dxf_filter.py` rule by rule.

use crate::codec::Codec;
use crate::printable::NOT_PRINTABLE;
use bilingua_model::*;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// TEXT semantics: a backslash is a literal character.
    Text,
    /// MTEXT codes.
    Mtext,
    /// MTEXT codes plus `<>` and `[]`.
    Dim,
}

/// Python `str.isprintable` for one character.
pub fn is_printable(c: char) -> bool {
    let u = c as u32;
    if (0x20..0x7F).contains(&u) {
        return true;
    }
    NOT_PRINTABLE
        .binary_search_by(|&(lo, hi)| {
            if u < lo {
                std::cmp::Ordering::Greater
            } else if u > hi {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_err()
}

fn hex4(s: &[char]) -> Option<u32> {
    if s.len() == 4 && s.iter().all(|c| c.is_ascii_hexdigit()) {
        let t: String = s.iter().collect();
        u32::from_str_radix(&t, 16).ok()
    } else {
        None
    }
}

fn slice(s: &[char], a: usize, b: usize) -> &[char] {
    let a = a.min(s.len());
    let b = b.min(s.len());
    &s[a..b]
}

fn is(s: &[char], a: usize, pat: &str) -> bool {
    let mut k = a;
    for p in pat.chars() {
        if s.get(k) != Some(&p) {
            return false;
        }
        k += 1;
    }
    true
}

/// At `s[i] == '\\'`: a `\U+XXXX` or `\M+NXXXX` escape gives (the character or None, its length); length 0 = none.
fn unicode_escape(s: &[char], i: usize) -> (Option<char>, usize) {
    let d = slice(s, i + 1, i + 3);
    if (d == ['U', '+'] || d == ['u', '+']) && hex4(slice(s, i + 3, i + 7)).is_some() {
        let cp = hex4(slice(s, i + 3, i + 7)).unwrap_or(0);
        if (0xD800..=0xDFFF).contains(&cp) {
            if cp <= 0xDBFF && (is(s, i + 7, "\\U+") || is(s, i + 7, "\\u+")) {
                if let Some(lo) = hex4(slice(s, i + 10, i + 14)) {
                    if (0xDC00..=0xDFFF).contains(&lo) {
                        let full = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                        return (char::from_u32(full), 14);
                    }
                }
            }
            return (None, 7);
        }
        return (char::from_u32(cp), 7);
    }
    if d == ['M', '+'] || d == ['m', '+'] {
        let k = slice(s, i + 3, i + 4);
        let codec = match k {
            ['1'] => Some("cp932"),
            ['2'] => Some("cp950"),
            ['3'] => Some("cp949"),
            ['5'] => Some("gbk"),
            _ => None,
        };
        if let (Some(name), Some(v)) = (codec, hex4(slice(s, i + 4, i + 8))) {
            let raw = [(v >> 8) as u8, (v & 0xFF) as u8];
            if let Some(Ok(t)) = Codec::from_name(name).map(|c| c.decode(&raw)) {
                let mut it = t.chars();
                if let (Some(ch), None) = (it.next(), it.next()) {
                    return (Some(ch), 8);
                }
            }
            return (None, 8);
        }
    }
    (None, 0)
}

#[derive(Debug, PartialEq)]
enum Item {
    T(String),
    X(String),
    O,
    C,
}

fn lex(s: &[char], kind: Kind) -> Vec<Item> {
    let mtext = kind != Kind::Text;
    let mut items: Vec<Item> = Vec::new();
    let mut buf = String::new();
    let n = s.len();
    let mut i = 0;
    macro_rules! tok {
        ($it:expr) => {{
            if !buf.is_empty() {
                items.push(Item::T(std::mem::take(&mut buf)));
            }
            items.push($it);
        }};
    }
    let raw = |a: usize, b: usize| -> String { s[a..b.min(n)].iter().collect() };
    while i < n {
        let c = s[i];
        if c == '\\' && i + 1 < n {
            let (ch, ln) = unicode_escape(s, i);
            if ln > 0 {
                match ch {
                    Some(ch) if is_printable(ch) => buf.push(ch),
                    _ => tok!(Item::X(raw(i, i + ln))),
                }
                i += ln;
                continue;
            }
            if !mtext {
                buf.push(c);
                i += 1;
                continue;
            }
            let d = s[i + 1];
            if d == '\\' || d == '{' || d == '}' {
                buf.push(d);
                i += 2;
            } else if d == '~' {
                buf.push('\u{a0}');
                i += 2;
            } else if d == 'S' {
                let mut j = i + 2;
                while j < n && s[j] != ';' {
                    j += if s[j] == '\\' { 2 } else { 1 };
                }
                let end = (j + 1).min(n);
                tok!(Item::X(raw(i, end)));
                i = end;
            } else if "ACcFfHpQTW".contains(d) {
                let end = match s[i + 2..].iter().position(|&x| x == ';') {
                    Some(p) => i + 2 + p + 1,
                    None => n,
                };
                tok!(Item::X(raw(i, end)));
                i = end;
            } else {
                tok!(Item::X(raw(i, i + 2)));
                i += 2;
            }
            continue;
        }
        if c == '\\' {
            if mtext {
                tok!(Item::X("\\".into()));
            } else {
                buf.push(c);
            }
            i += 1;
            continue;
        }
        if c == '%' && is(s, i, "%%") {
            if let Some(&e) = s.get(i + 2) {
                if "dDcCpPuUoOkK%".contains(e) {
                    tok!(Item::X(raw(i, i + 3)));
                    i += 3;
                    continue;
                }
            }
            let t = slice(s, i + 2, i + 5);
            if t.len() == 3 && t.iter().all(|&x| py_isdecimal_char(x)) {
                tok!(Item::X(raw(i, i + 5)));
                i += 5;
                continue;
            }
        }
        if c == '^' && i + 1 < n {
            tok!(Item::X(raw(i, i + 2)));
            i += 2;
            continue;
        }
        if mtext && c == '{' {
            tok!(Item::O);
            i += 1;
            continue;
        }
        if mtext && c == '}' {
            tok!(Item::C);
            i += 1;
            continue;
        }
        if kind == Kind::Dim && (is(s, i, "<>") || is(s, i, "[]")) {
            tok!(Item::X(raw(i, i + 2)));
            i += 2;
            continue;
        }
        buf.push(c);
        i += 1;
    }
    if !buf.is_empty() {
        items.push(Item::T(buf));
    }
    // pair the braces; an unmatched one is a point code
    let mut stack: Vec<usize> = Vec::new();
    let mut paired = vec![false; items.len()];
    for idx in 0..items.len() {
        match items[idx] {
            Item::O => stack.push(idx),
            Item::C => {
                if let Some(o) = stack.pop() {
                    paired[o] = true;
                    paired[idx] = true;
                }
            }
            _ => {}
        }
    }
    for (idx, it) in items.iter_mut().enumerate() {
        if !paired[idx] {
            match it {
                Item::O => *it = Item::X("{".into()),
                Item::C => *it = Item::X("}".into()),
                _ => {}
            }
        }
    }
    items
}

/// What a value parses to: inline text and codes, the code table, and whether any letter is outside the codes.
pub struct Parsed {
    pub inlines: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
    pub alpha: bool,
}

fn label_of(raw: &str) -> &'static str {
    if raw.starts_with("%%") {
        "symbol"
    } else if raw.starts_with("\\U+") || raw.starts_with("\\M+") {
        "character"
    } else if raw == "<>" || raw == "[]" {
        "measurement"
    } else if raw.starts_with('\\') {
        "mtext code"
    } else if raw.starts_with('^') {
        "control"
    } else {
        "code"
    }
}

pub fn parse_value(value: &str, kind: Kind) -> Parsed {
    let chars: Vec<char> = value.chars().collect();
    let items = lex(&chars, kind);
    let mut inlines = Vec::new();
    let mut codes = BTreeMap::new();
    let mut alpha = false;
    let mut n = 0usize;
    let mut open_ids: Vec<String> = Vec::new();
    for it in items {
        match it {
            Item::T(t) => {
                alpha = alpha || py_has_alpha(&t);
                inlines.push(Inline::Text(t));
            }
            Item::X(raw) => {
                n += 1;
                let id = n.to_string();
                codes.insert(id.clone(), CodeData::standalone(raw.clone()).with_label(label_of(&raw)));
                inlines.push(Inline::tag(id, TagKind::Standalone, raw));
            }
            Item::O => {
                n += 1;
                let id = n.to_string();
                codes.insert(id.clone(), CodeData::paired("{", "}").with_label("group"));
                inlines.push(Inline::tag(id.clone(), TagKind::Open, "{"));
                open_ids.push(id);
            }
            Item::C => {
                let id = open_ids.pop().unwrap_or_default();
                inlines.push(Inline::tag(id, TagKind::Close, "}"));
            }
        }
    }
    Parsed { inlines, codes, alpha }
}

fn is_ctrl(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{8}' | '\u{b}'..='\u{1f}' | '\u{7f}')
}

fn escape(s: &str, kind: Kind, out: &mut String) {
    // newlines unified, control characters dropped
    let mut t = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\r' {
            if it.peek() == Some(&'\n') {
                it.next();
            }
            t.push('\n');
        } else if !is_ctrl(c) {
            t.push(c);
        }
    }
    let mut run = 0usize;
    let flush = |run: &mut usize, out: &mut String| {
        let k = if *run >= 2 { *run * 3 } else { *run };
        for _ in 0..k {
            out.push('%');
        }
        *run = 0;
    };
    for c in t.chars() {
        if c == '%' {
            run += 1;
            continue;
        }
        flush(&mut run, out);
        match c {
            '^' => out.push_str("^ "),
            '\n' | '\t' if kind == Kind::Text => out.push(' '),
            '\\' if kind != Kind::Text => out.push_str("\\\\"),
            '{' if kind != Kind::Text => out.push_str("\\{"),
            '}' if kind != Kind::Text => out.push_str("\\}"),
            '\u{a0}' if kind != Kind::Text => out.push_str("\\~"),
            '\n' => out.push_str("\\P"),
            '\t' => out.push_str("^I"),
            c => out.push(c),
        }
    }
    flush(&mut run, out);
}

/// The text value for a translation: escaped text and the original code spellings. A paired brace survives only
/// with its partner, so braces stay balanced; a code the unit does not know is dropped.
pub fn render(target: &[Inline], codes: &BTreeMap<String, CodeData>, kind: Kind) -> String {
    let mut opens: HashSet<&str> = HashSet::new();
    let mut closes: HashSet<&str> = HashSet::new();
    for i in target {
        if let Inline::Tag { id, kind: k, .. } = i {
            match k {
                TagKind::Open => {
                    opens.insert(id);
                }
                TagKind::Close => {
                    closes.insert(id);
                }
                TagKind::Standalone => {}
            }
        }
    }
    let mut out = String::new();
    // What to write after an open-ended code before anything follows it (see `open_end`).
    let mut seal: Option<&'static str> = None;
    let mut piece = String::new();
    for i in target {
        piece.clear();
        let mut code = None;
        match i {
            Inline::Text(t) => escape(t, kind, &mut piece),
            Inline::Tag { id, kind: k, .. } => {
                let Some(c) = codes.get(id) else { continue };
                if let Some(raw) = &c.standalone {
                    piece.push_str(raw);
                    code = Some(raw.as_str());
                } else if *k == TagKind::Open && closes.contains(id.as_str()) {
                    piece.push_str(c.open.as_deref().unwrap_or("{"));
                } else if *k == TagKind::Close && opens.contains(id.as_str()) {
                    piece.push_str(c.close.as_deref().unwrap_or("}"));
                }
            }
        }
        if piece.is_empty() {
            continue;
        }
        if let Some(s) = seal.take() {
            if !piece.starts_with(char::is_whitespace) {
                out.push_str(s);
            }
        }
        out.push_str(&piece);
        seal = code.and_then(|raw| open_end(raw, kind));
    }
    out
}

/// A code that only ended where its value ended (a lone trailing `\`, a `\A1` with no `;`) would join whatever a
/// translation puts after it (`\` + `q` reads back as the `\q` code). What closes it then: a second `\` makes the
/// lone backslash the escaped one it shows as, `;` ends the format code. A code at the end of the value is written as is,
/// so untouched and normal translations keep their bytes. Kept for LocOS byte parity (its goldens pin it): before
/// white space nothing is added, so `\` + space still reads back as the `\ ` code and an open `\A1` still runs on to
/// the next `;` (LocOS writes the joined form everywhere).
fn open_end(raw: &str, kind: Kind) -> Option<&'static str> {
    if kind == Kind::Text {
        return None;
    }
    if raw == "\\" {
        return Some("\\");
    }
    let mut c = raw.chars();
    if c.next() != Some('\\') || raw.ends_with(';') || raw.ends_with('\\') {
        return None;
    }
    c.next().filter(|d| "ACcFfHpQTWS".contains(*d)).map(|_| ";")
}

/// Characters the code page cannot hold become `\U+XXXX` (a surrogate pair above U+FFFF), as AutoCAD writes them.
pub fn fit_codec(s: &str, codec: Codec) -> String {
    if codec.can_write(s) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 16);
    let mut scratch = Vec::new();
    for c in s.chars() {
        scratch.clear();
        if codec.encode_char(c, &mut scratch) {
            out.push(c);
        } else {
            let mut cp = c as u32;
            if cp > 0xFFFF {
                cp -= 0x10000;
                out.push_str(&format!("\\U+{:04X}\\U+{:04X}", 0xD800 + (cp >> 10), 0xDC00 + (cp & 0x3FF)));
            } else {
                out.push_str(&format!("\\U+{cp:04X}"));
            }
        }
    }
    out
}

fn atom_len(c: &[char], i: usize) -> usize {
    if c[i] == '\\' {
        if is(c, i, "\\U+") && hex4(slice(c, i + 3, i + 7)).is_some() {
            return 7;
        }
        if is(c, i, "\\M+") && c.get(i + 3).is_some_and(|&d| py_isdecimal_char(d)) && hex4(slice(c, i + 4, i + 8)).is_some() {
            return 8;
        }
        return if i + 1 < c.len() { 2 } else { 1 };
    }
    if c[i] == '^' && i + 1 < c.len() {
        return 2;
    }
    1
}

pub const CHUNK: usize = 250;

/// An MTEXT value as pieces of at most 250 characters, never inside an escape; the last (group 1) is shorter.
pub fn chunks(s: &str) -> Vec<String> {
    let c: Vec<char> = s.chars().collect();
    if c.len() < CHUNK {
        return vec![s.to_string()];
    }
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut size = 0usize;
    let mut i = 0;
    while i < c.len() {
        let l = atom_len(&c, i);
        if size + l > CHUNK {
            out.push(std::mem::take(&mut cur));
            size = 0;
        }
        cur.extend(&c[i..i + l]);
        size += l;
        i += l;
    }
    out.push(cur);
    if out.last().is_some_and(|l| l.chars().count() >= CHUNK) {
        out.push(String::new());
    }
    out
}
