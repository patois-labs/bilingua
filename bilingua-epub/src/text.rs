//! Reading and writing the text of one XML or XHTML member in the encoding it came in.
//!
//! XML formats refuse undeclared non-UTF-8 input (as the XML specification and LocOS do): a member is UTF-8 (with or
//! without a byte-order mark), UTF-16 (with a mark, or recognised from `<` in 16-bit form), or declares another
//! charset in its XML declaration or `<meta charset>`; anything else is a typed [`Error::Encoding`].

use bilingua_model::encoding_rs::{self, Encoding};
use bilingua_model::Error;
use regex::Regex;
use std::sync::OnceLock;

/// How far into the text charset declarations are searched (they live in the head).
const HEAD: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Env {
    Utf8,
    Utf8Bom,
    /// `(big endian, has a byte-order mark)`
    Utf16(bool, bool),
    Legacy(&'static Encoding),
}

/// A decoded member: the text, the envelope to write it back in, and the charset labels that were rewritten to
/// UTF-8 so a reader of the text (the HTML crate) is not misled by them.
#[derive(Debug, Clone)]
pub struct Text {
    pub text: String,
    pub env: Env,
    pub labels: Vec<(usize, String)>,
}

fn enc_err(what: &str, msg: &str) -> Error {
    Error::Encoding(format!("{what}: {msg}. EPUB content is UTF-8 or UTF-16 text; re-save the file as UTF-8."))
}

fn decl_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r#"(?i)(<\?xml[^>]{0,200}?\bencoding\s*=\s*["']\s*|<meta\b[^>]{0,400}?\bcharset\s*=\s*["']?\s*)([A-Za-z0-9_.:-]{1,40})"#)
            .expect("charset declaration pattern")
    })
}

fn head_end(s: &str) -> usize {
    let mut n = s.len().min(HEAD);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    n
}

fn declared(bytes: &[u8]) -> Option<&'static Encoding> {
    bilingua_model::enc::declared_encoding(bytes)
}

fn is_utf8_label(l: &str) -> bool {
    matches!(l.to_ascii_lowercase().as_str(), "utf-8" | "utf8")
}

/// Reads `bytes`. With `rewrite`, every declared charset that is not UTF-8 is rewritten to `UTF-8` in the text
/// (and remembered in `labels`), so [`Text::encode`] can put it back.
pub fn read(bytes: &[u8], what: &str, rewrite: bool) -> Result<Text, Error> {
    let (text, env) = if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        let s = std::str::from_utf8(rest).map_err(|e| enc_err(what, &format!("invalid UTF-8 after the byte-order mark (byte {})", e.valid_up_to())))?;
        (s.to_string(), Env::Utf8Bom)
    } else if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let be = bytes[0] == 0xFE;
        (utf16(&bytes[2..], be, what)?, Env::Utf16(be, true))
    } else if bytes.starts_with(&[b'<', 0]) || bytes.starts_with(&[0, b'<']) {
        let be = bytes[0] == 0;
        (utf16(bytes, be, what)?, Env::Utf16(be, false))
    } else {
        match declared(bytes) {
            Some(e) if e != encoding_rs::UTF_8 => {
                if e == encoding_rs::UTF_16LE || e == encoding_rs::UTF_16BE {
                    return Err(enc_err(what, "it declares UTF-16 but is not UTF-16 text"));
                }
                let s = bilingua_model::enc::decode_strict(e, bytes)
                    .map_err(|p| enc_err(what, &format!("it declares {} but is not valid in it (byte at position {p})", e.name())))?;
                (s, Env::Legacy(e))
            }
            _ => match std::str::from_utf8(bytes) {
                Ok(s) => (s.to_string(), Env::Utf8),
                Err(e) => return Err(enc_err(what, &format!("it is not valid UTF-8 (byte {}) and declares no other encoding", e.valid_up_to()))),
            },
        }
    };
    let mut labels = Vec::new();
    let text = if rewrite { rewrite_labels(text, &mut labels) } else { text };
    Ok(Text { text, env, labels })
}

fn utf16(body: &[u8], be: bool, what: &str) -> Result<String, Error> {
    let e = if be { encoding_rs::UTF_16BE } else { encoding_rs::UTF_16LE };
    e.decode_without_bom_handling_and_without_replacement(body)
        .map(|c| c.into_owned())
        .ok_or_else(|| enc_err(what, "it is not valid UTF-16"))
}

fn rewrite_labels(text: String, labels: &mut Vec<(usize, String)>) -> String {
    let end = head_end(&text);
    let head = &text[..end];
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (n, c) in decl_re().captures_iter(head).enumerate() {
        let m = c.get(2).expect("label group");
        if is_utf8_label(m.as_str()) {
            continue;
        }
        out.push_str(&head[last..m.start()]);
        out.push_str("UTF-8");
        labels.push((n, m.as_str().to_string()));
        last = m.end();
    }
    if labels.is_empty() {
        return text;
    }
    out.push_str(&head[last..]);
    out.push_str(&text[end..]);
    out
}

impl Text {
    /// The member's bytes for `s`, in the envelope it was read in.
    pub fn encode(&self, s: &str) -> Vec<u8> {
        let restored;
        let s = if self.labels.is_empty() {
            s
        } else {
            let end = head_end(s);
            let head = &s[..end];
            let mut out = String::with_capacity(s.len() + 16);
            let mut last = 0;
            let mut k = 0;
            for (n, c) in decl_re().captures_iter(head).enumerate() {
                let m = c.get(2).expect("label group");
                if k < self.labels.len() && self.labels[k].0 == n {
                    out.push_str(&head[last..m.start()]);
                    out.push_str(&self.labels[k].1);
                    k += 1;
                    last = m.end();
                }
            }
            out.push_str(&head[last..]);
            out.push_str(&s[end..]);
            restored = out;
            &restored
        };
        match self.env {
            Env::Utf8 => s.as_bytes().to_vec(),
            Env::Utf8Bom => {
                let mut v = vec![0xEF, 0xBB, 0xBF];
                v.extend_from_slice(s.as_bytes());
                v
            }
            Env::Utf16(be, bom) => {
                let mut v: Vec<u8> = if !bom {
                    vec![]
                } else if be {
                    vec![0xFE, 0xFF]
                } else {
                    vec![0xFF, 0xFE]
                };
                for u in s.encode_utf16() {
                    v.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
                }
                v
            }
            Env::Legacy(e) => {
                // GB18030 writes everything GBK does; a character the code page lacks becomes a numeric reference.
                let w = if e == encoding_rs::GBK { encoding_rs::GB18030 } else { e };
                w.encode(s).0.into_owned()
            }
        }
    }
}

/// Characters XML 1.0 allows; a target with others is cleaned (line and form feed become a newline).
pub fn xml_target_text(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\u{0b}' | '\u{0c}' => Some('\n'),
            '\u{0}'..='\u{8}' | '\u{0e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}' => None,
            c => Some(c),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_utf16_and_declared_legacy() {
        assert_eq!(read("héllo".as_bytes(), "x", false).unwrap().env, Env::Utf8);
        let mut b = vec![0xEF, 0xBB, 0xBF];
        b.extend_from_slice(b"<a/>");
        assert_eq!(read(&b, "x", false).unwrap().env, Env::Utf8Bom);
        let t = read(b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>caf\xe9</a>", "x", true).unwrap();
        assert!(t.text.contains("café") && t.text.contains("encoding=\"UTF-8\""));
        let back = t.encode(&t.text);
        assert_eq!(back, b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>caf\xe9</a>");
        assert!(read(b"<a>caf\xe9</a>", "x", false).is_err());
    }
}
