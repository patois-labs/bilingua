//! MIF string escapes.
//!
//! Read: `\t` tab, `\>` `>`, `\q` `'`, `\Q` `` ` ``, `\\` backslash, `\xNN ` a byte of the legacy FrameMaker
//! character set (Mac Roman based; one space follows and is part of the escape), `\uXXXX` a UTF-16 unit as a
//! character. Any other backslash pair is kept as written.
//! Write (MIF 8 and later is UTF-8): backslash, `>`, `'`, `` ` `` and tab are escaped, other control characters
//! are dropped, everything else is written as is.

use bilingua_model::encoding_rs;

fn hex(b: &[u8]) -> Option<u32> {
    let mut v = 0u32;
    for &d in b {
        v = v * 16 + (d as char).to_digit(16)?;
    }
    Some(v)
}

/// The text inside `` `...' ``.
pub fn decode_string(raw: &str) -> String {
    let b = raw.as_bytes();
    let n = b.len();
    let mut out = String::with_capacity(n);
    let mut i = 0;
    while i < n {
        match raw[i..].find('\\') {
            None => {
                out.push_str(&raw[i..]);
                break;
            }
            Some(k) => {
                out.push_str(&raw[i..i + k]);
                i += k;
            }
        }
        if i + 1 >= n {
            out.push('\\');
            i += 1;
            continue;
        }
        let e = b[i + 1];
        match e {
            b't' => out.push('\t'),
            b'>' => out.push('>'),
            b'q' => out.push('\''),
            b'Q' => out.push('`'),
            b'\\' => out.push('\\'),
            b'x' if i + 4 <= n && hex(&b[i + 2..i + 4]).is_some() => {
                let byte = hex(&b[i + 2..i + 4]).expect("checked") as u8;
                let one = [byte];
                let (s, _) = encoding_rs::MACINTOSH.decode_without_bom_handling(&one);
                out.push_str(&s);
                i += 4;
                if i < n && b[i] == b' ' {
                    i += 1;
                }
                continue;
            }
            b'u' if i + 6 <= n && hex(&b[i + 2..i + 6]).is_some() => {
                if let Some(c) = char::from_u32(hex(&b[i + 2..i + 6]).expect("checked")) {
                    out.push(c);
                } else {
                    out.push_str(&raw[i..i + 6]);
                }
                i += 6;
                continue;
            }
            _ => {
                out.push('\\');
                let ch = raw[i + 1..].chars().next().expect("char");
                out.push(ch);
                i += 1 + ch.len_utf8();
                continue;
            }
        }
        i += 2;
    }
    out
}

/// Text as the inside of a MIF string.
pub fn encode_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '>' => out.push_str("\\>"),
            '\'' => out.push_str("\\q"),
            '`' => out.push_str("\\Q"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {}
            c => out.push(c),
        }
    }
    out
}
