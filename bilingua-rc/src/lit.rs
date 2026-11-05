//! String literals: splitting one into text and codes (escapes, printf and FormatMessage placeholders), and
//! writing translated text back as a literal.

use regex::Regex;
use std::sync::OnceLock;

/// One piece of a literal's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    /// An escape or a placeholder, as written in the file.
    Code(String),
}

fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"%%|%[1-9]\d?(?:![^!\n]{1,12}!)?|%[-+#0]*(?:\d+|\*)?(?:\.(?:\d+|\*))?(?:hh|h|ll|l|L|z|j|t|I64|I32|I|w)?[diouxXeEfFgGaAcCsSpn]",
        )
        .expect("placeholder pattern")
    })
}

/// Python's `str.isprintable` for one character, as far as std can tell: controls, format characters, private
/// use and every white space but the plain space are not printable. (Unassigned code points are treated as
/// printable; Python says no. Known difference.)
pub fn is_printable(c: char) -> bool {
    if c == ' ' {
        return true;
    }
    if c.is_control() || c.is_whitespace() {
        return false;
    }
    let u = c as u32;
    let format = matches!(
        u,
        0xAD | 0x600..=0x605 | 0x61C | 0x6DD | 0x70F | 0x890..=0x891 | 0x8E2 | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E
            | 0x2060..=0x2064 | 0x2066..=0x206F | 0xFEFF | 0xFFF9..=0xFFFB | 0x110BD | 0x110CD | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0xE0001 | 0xE0020..=0xE007F
    );
    let private = matches!(u, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD);
    !(format || private)
}

/// The text of a literal split into pieces. `lit` is the whole token (optional `L`, quotes). The second value is
/// true for a wide (`L"..."`) literal.
pub fn pieces(lit: &str) -> (Vec<Piece>, bool) {
    let wide = lit.starts_with('L');
    let body = if wide { &lit[2..lit.len() - 1] } else { &lit[1..lit.len() - 1] };
    let mut raw: Vec<Piece> = Vec::new();
    let mut buf = String::new();
    let n = body.len();
    let bs = body.as_bytes();
    let mut i = 0usize;
    let flush = |buf: &mut String, raw: &mut Vec<Piece>| {
        if !buf.is_empty() {
            raw.push(Piece::Text(std::mem::take(buf)));
        }
    };
    while i < n {
        // copy the run up to the next quote or backslash
        let j = bs[i..].iter().position(|&c| c == b'"' || c == b'\\').map_or(n, |p| i + p);
        if j > i {
            buf.push_str(&body[i..j]);
            i = j;
            continue;
        }
        if bs[i] == b'"' {
            buf.push('"');
            i = (i + 2).min(n);
            continue;
        }
        // a backslash
        if i + 1 >= n {
            buf.push('\\');
            i += 1;
            continue;
        }
        let d = body[i + 1..].chars().next().unwrap_or('\\');
        let dl = d.len_utf8();
        match d {
            '\\' | '"' => {
                buf.push(d);
                i += 2;
                continue;
            }
            '\'' | '?' => {
                if !wide {
                    buf.push('\\');
                }
                buf.push(d);
                i += 2;
                continue;
            }
            '\n' | '\r' if d == '\n' || bs.get(i + 2) == Some(&b'\n') => {
                // line continuation: rc.exe also drops the next line's leading white space
                i += if d == '\n' { 2 } else { 3 };
                while let Some(c) = body[i..].chars().next() {
                    if matches!(c, '\r' | '\n' | '\t' | ' ' | '\x0b' | '\x0c' | '\u{a0}') {
                        i += c.len_utf8();
                    } else {
                        break;
                    }
                }
                continue;
            }
            _ => {}
        }
        let code: &str = if d == 'x' || d == 'X' {
            let maxd = if wide { 4 } else { 2 };
            let digits = bs[i + 2..].iter().take(maxd).take_while(|c| c.is_ascii_hexdigit()).count();
            let code = &body[i..i + 2 + digits];
            if wide && digits > 0 {
                if let Some(ch) = u32::from_str_radix(&body[i + 2..i + 2 + digits], 16).ok().and_then(char::from_u32) {
                    if is_printable(ch) {
                        buf.push(ch);
                        i += code.len();
                        continue;
                    }
                }
            }
            code
        } else if ('0'..='7').contains(&d) {
            let digits = bs[i + 1..].iter().take(3).take_while(|c| (b'0'..=b'7').contains(c)).count();
            &body[i..i + 1 + digits]
        } else {
            &body[i..i + 1 + dl]
        };
        flush(&mut buf, &mut raw);
        raw.push(Piece::Code(code.to_string()));
        i += code.len();
    }
    flush(&mut buf, &mut raw);
    // printf / FormatMessage placeholders inside the text pieces become codes too
    let mut out: Vec<Piece> = Vec::with_capacity(raw.len());
    for p in raw {
        match p {
            Piece::Code(_) => out.push(p),
            Piece::Text(t) => {
                let mut pos = 0;
                for m in placeholder_re().find_iter(&t) {
                    if m.start() > pos {
                        out.push(Piece::Text(t[pos..m.start()].to_string()));
                    }
                    out.push(Piece::Code(m.as_str().to_string()));
                    pos = m.end();
                }
                if pos == 0 {
                    out.push(Piece::Text(t));
                } else if pos < t.len() {
                    out.push(Piece::Text(t[pos..].to_string()));
                }
            }
        }
    }
    (out, wide)
}

/// True when a code names its argument (`%1`, `%2!s!`): a translation may move it.
pub fn is_free(code: &str) -> bool {
    let b = code.as_bytes();
    b.len() >= 2 && b[0] == b'%' && (b'1'..=b'9').contains(&b[1])
}

/// True for a printf or FormatMessage placeholder (not an escape).
pub fn is_format(code: &str) -> bool {
    code.starts_with('%')
}

/// One character of translated text as it is written inside a literal.
pub fn escape_char(ch: char, wide: bool, out: &mut String) {
    match ch {
        '\\' => out.push_str("\\\\"),
        '"' => out.push_str("\"\""),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        c if (c as u32) < 0x20 || c as u32 == 0x7F => {
            if wide {
                out.push_str(&format!("\\x{:04x}", c as u32));
            } else {
                out.push_str(&format!("\\{:03o}", c as u32));
            }
        }
        c => out.push(c),
    }
}
