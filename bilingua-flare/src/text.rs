//! Text helpers: reading a member as UTF-8, the character rules of XML, and character references.
//!
//! Flare saves every file as UTF-8 and Localization OS reads nothing else, so a member that is not UTF-8 is a typed
//! [`Error::Encoding`] (with or without a byte-order mark; UTF-16 is refused too, it is not UTF-8 text).

use bilingua_model::{py_isspace_char, Error};
use regex::Regex;
use std::sync::OnceLock;

/// How far into the text charset declarations are searched (they live in the head).
const HEAD: usize = 64 * 1024;

/// A decoded member: the text, whether it had a UTF-8 byte-order mark, and the charset labels that were rewritten
/// to `UTF-8` so the HTML reader is not misled by a stale `encoding="iso-8859-1"` on UTF-8 text.
#[derive(Debug, Clone)]
pub struct Text {
    pub text: String,
    pub bom: bool,
    pub labels: Vec<(usize, String)>,
}

fn enc_err(what: &str, msg: &str) -> Error {
    Error::Encoding(format!("{what}: {msg}. Flare projects are UTF-8 text; re-save the file as UTF-8."))
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

/// Reads `bytes` as UTF-8 (a byte-order mark is allowed and remembered). With `rewrite`, a declared charset that is
/// not UTF-8 is rewritten to `UTF-8` (and remembered), so [`Text::encode`] can put it back.
pub fn read(bytes: &[u8], what: &str, rewrite: bool) -> Result<Text, Error> {
    let (body, bom) = match bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(rest) => (rest, true),
        None => (bytes, false),
    };
    let s = std::str::from_utf8(body).map_err(|e| enc_err(what, &format!("it is not valid UTF-8 (byte {})", e.valid_up_to())))?;
    if s.contains('\0') {
        return Err(enc_err(what, "it holds NUL characters (UTF-16 text?)"));
    }
    let mut labels = Vec::new();
    let text = if rewrite { rewrite_labels(s, &mut labels) } else { s.to_string() };
    Ok(Text { text, bom, labels })
}

fn rewrite_labels(text: &str, labels: &mut Vec<(usize, String)>) -> String {
    let end = head_end(text);
    let head = &text[..end];
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (n, c) in decl_re().captures_iter(head).enumerate() {
        let m = c.get(2).expect("label group");
        if matches!(m.as_str().to_ascii_lowercase().as_str(), "utf-8" | "utf8") {
            continue;
        }
        out.push_str(&head[last..m.start()]);
        out.push_str("UTF-8");
        labels.push((n, m.as_str().to_string()));
        last = m.end();
    }
    if labels.is_empty() {
        return text.to_string();
    }
    out.push_str(&head[last..]);
    out.push_str(&text[end..]);
    out
}

impl Text {
    /// The member's bytes for `s`: the original charset labels put back, the byte-order mark kept.
    pub fn encode(&self, s: &str) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::with_capacity(s.len() + 3);
        if self.bom {
            out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        }
        if self.labels.is_empty() {
            out.extend_from_slice(s.as_bytes());
            return out;
        }
        let end = head_end(s);
        let head = &s[..end];
        let mut last = 0;
        let mut k = 0;
        let mut text = String::with_capacity(s.len() + 16);
        for (n, c) in decl_re().captures_iter(head).enumerate() {
            let m = c.get(2).expect("label group");
            if k < self.labels.len() && self.labels[k].0 == n {
                text.push_str(&head[last..m.start()]);
                text.push_str(&self.labels[k].1);
                k += 1;
                last = m.end();
            }
        }
        text.push_str(&head[last..]);
        text.push_str(&s[end..]);
        out.extend_from_slice(text.as_bytes());
        out
    }
}

/// Characters XML 1.0 allows; a target with others is cleaned (line and form feed become a newline). Lone
/// surrogates cannot occur in a Rust string.
pub fn xml_target_text(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\u{0b}' | '\u{0c}' => Some('\n'),
            '\u{0}'..='\u{8}' | '\u{0e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}' => None,
            c => Some(c),
        })
        .collect()
}

/// `&`, `<`, `>` escaped; with `quote`, `"` and `'` too (attribute values).
pub fn xml_escape(s: &str, quote: bool) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if quote => out.push_str("&quot;"),
            '\'' if quote => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// Python `s.strip()` split into (leading white space, core, trailing white space); the trailing part is empty
/// when the core is.
pub fn split_ws(s: &str) -> (&str, &str, &str) {
    let core = bilingua_model::py_strip(s);
    if core.is_empty() {
        return (s, "", "");
    }
    let lead_len = s.len() - bilingua_model::py_lstrip(s).len();
    let trail_len = s.len() - bilingua_model::py_rstrip(s).len();
    (&s[..lead_len], core, &s[s.len() - trail_len..])
}

/// Python `" ".join(s.split())`.
pub fn collapse_ws(s: &str) -> String {
    s.split(py_isspace_char).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
}

const LATIN1: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf", "laquo", "not",
    "shy", "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro", "para", "middot", "cedil", "sup1",
    "ordm", "raquo", "frac14", "frac12", "frac34", "iquest", "Agrave", "Aacute", "Acirc", "Atilde", "Auml",
    "Aring", "AElig", "Ccedil", "Egrave", "Eacute", "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH",
    "Ntilde", "Ograve", "Oacute", "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc",
    "Uuml", "Yacute", "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil",
    "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde", "ograve", "oacute",
    "ocirc", "otilde", "ouml", "divide", "oslash", "ugrave", "uacute", "ucirc", "uuml", "yacute", "thorn", "yuml",
];

const EXTRA: &[(&str, char)] = &[
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("hellip", '\u{2026}'),
    ("mdash", '\u{2014}'),
    ("ndash", '\u{2013}'),
    ("lsquo", '\u{2018}'),
    ("rsquo", '\u{2019}'),
    ("sbquo", '\u{201a}'),
    ("ldquo", '\u{201c}'),
    ("rdquo", '\u{201d}'),
    ("bdquo", '\u{201e}'),
    ("bull", '\u{2022}'),
    ("trade", '\u{2122}'),
    ("euro", '\u{20ac}'),
    ("larr", '\u{2190}'),
    ("uarr", '\u{2191}'),
    ("rarr", '\u{2192}'),
    ("darr", '\u{2193}'),
    ("ensp", '\u{2002}'),
    ("emsp", '\u{2003}'),
    ("thinsp", '\u{2009}'),
    ("zwnj", '\u{200c}'),
    ("zwj", '\u{200d}'),
    ("lrm", '\u{200e}'),
    ("rlm", '\u{200f}'),
];

fn named(name: &str) -> Option<char> {
    if let Some((_, c)) = EXTRA.iter().find(|(n, _)| *n == name) {
        return Some(*c);
    }
    LATIN1.iter().position(|n| *n == name).and_then(|i| char::from_u32(0xA0 + i as u32))
}

/// Character references decoded the way Python's `html.unescape` does for the common cases: the five XML names,
/// the Latin-1 and common typographic names, and decimal and hexadecimal numbers. A reference it does not know is
/// kept as typed.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        let tail = &rest[1..];
        let semi = tail.char_indices().take(34).find(|(_, c)| *c == ';').map(|(i, _)| i);
        let decoded = semi.and_then(|e| {
            let body = &tail[..e];
            let c = if let Some(num) = body.strip_prefix('#') {
                let v = match num.strip_prefix(['x', 'X']) {
                    Some(h) => u32::from_str_radix(h, 16).ok(),
                    None => num.parse::<u32>().ok(),
                };
                v.and_then(char::from_u32).filter(|c| *c != '\0')
            } else {
                named(body)
            };
            c.map(|c| (c, e + 2))
        });
        match decoded {
            Some((c, used)) => {
                out.push(c);
                rest = &rest[used..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_with_and_without_a_mark_and_refusals() {
        assert!(!read("héllo".as_bytes(), "x", false).unwrap().bom);
        let mut b = vec![0xEF, 0xBB, 0xBF];
        b.extend_from_slice(b"<a/>");
        assert!(read(&b, "x", false).unwrap().bom);
        assert!(read(b"<a>caf\xe9</a>", "x", false).is_err());
        assert!(read(&[0xFF, 0xFE, b'<', 0], "x", false).is_err());
        assert!(read(b"<\0a\0>\0", "x", false).is_err());
    }

    #[test]
    fn a_stale_charset_label_is_hidden_and_put_back() {
        let t = read(b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>x</a>", "x", true).unwrap();
        assert!(t.text.contains("encoding=\"UTF-8\""));
        assert_eq!(t.encode(&t.text), b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>x</a>");
    }

    #[test]
    fn references_and_white_space() {
        assert_eq!(unescape("a&amp;b &#160;&#xe9; &nbsp;&eacute; &bogus; & &#0;"), "a&b \u{a0}\u{e9} \u{a0}\u{e9} &bogus; & &#0;");
        assert_eq!(split_ws("  x y \n"), ("  ", "x y", " \n"));
        assert_eq!(split_ws("   "), ("   ", "", ""));
        assert_eq!(collapse_ws(" a \t b\n"), "a b");
        assert_eq!(xml_escape("<a href=\"x\">&'", true), "&lt;a href=&quot;x&quot;&gt;&amp;&apos;");
    }
}
