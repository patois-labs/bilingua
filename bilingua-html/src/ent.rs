//! Entities and character encodings.

use bilingua_model::Error;

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
    ("hearts", '\u{2665}'),
    ("dagger", '\u{2020}'),
    ("Dagger", '\u{2021}'),
    ("permil", '\u{2030}'),
    ("lsaquo", '\u{2039}'),
    ("rsaquo", '\u{203a}'),
    ("ensp", '\u{2002}'),
    ("emsp", '\u{2003}'),
    ("thinsp", '\u{2009}'),
    ("zwnj", '\u{200c}'),
    ("zwj", '\u{200d}'),
    ("lrm", '\u{200e}'),
    ("rlm", '\u{200f}'),
    ("infin", '\u{221e}'),
    ("ne", '\u{2260}'),
    ("le", '\u{2264}'),
    ("ge", '\u{2265}'),
    ("asymp", '\u{2248}'),
    ("minus", '\u{2212}'),
    ("check", '\u{2713}'),
    ("star", '\u{2606}'),
];

pub fn named_entity(name: &str) -> Option<char> {
    if let Some(i) = LATIN1.iter().position(|n| *n == name) {
        return char::from_u32(160 + i as u32);
    }
    EXTRA.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// If `s` starts with an entity reference, returns its length in bytes and the character.
fn entity_at(s: &str) -> Option<(usize, char)> {
    let b = s.as_bytes();
    if b.first() != Some(&b'&') {
        return None;
    }
    // Search the bytes: byte 34 may fall inside a multi-byte character, and ';' is ASCII so `semi` is a char boundary.
    let semi = b[..b.len().min(34)].iter().position(|&c| c == b';')?;
    let body = &s[1..semi];
    if let Some(num) = body.strip_prefix('#') {
        let cp = if let Some(h) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
            u32::from_str_radix(h, 16).ok()?
        } else {
            num.parse::<u32>().ok()?
        };
        if cp == 0 {
            return None;
        }
        return char::from_u32(cp).map(|c| (semi + 1, c));
    }
    if body.is_empty() || !body.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    named_entity(body).map(|c| (semi + 1, c))
}

/// True when `s` starts with something that looks like an entity we do not know (kept literal on output).
fn unknown_entity_at(s: &str) -> bool {
    let b = s.as_bytes();
    if b.first() != Some(&b'&') || b.len() < 3 {
        return false;
    }
    let Some(semi) = b[..b.len().min(34)].iter().position(|&c| c == b';') else {
        return false;
    };
    let body = &s[1..semi];
    if body.is_empty() {
        return false;
    }
    if let Some(num) = body.strip_prefix('#') {
        return !num.is_empty() && num.bytes().all(|c| c.is_ascii_alphanumeric()) && entity_at(&s[..=semi]).is_none();
    }
    body.bytes().all(|c| c.is_ascii_alphanumeric()) && named_entity(body).is_none()
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        match entity_at(rest) {
            Some((len, c)) => {
                out.push(c);
                rest = &rest[len..];
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

/// Decodes entities and collapses runs of ASCII whitespace to one space (HTML rendering rules).
pub fn decode_collapse(s: &str) -> String {
    let d = decode_entities(s);
    let mut out = String::with_capacity(d.len());
    let mut prev_ws = false;
    for c in d.chars() {
        if matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{0c}') {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out
}

/// Encodes text for element content. Characters the target encoding cannot hold become numeric references.
pub fn esc_text(s: &str, enc: Enc) -> String {
    esc(s, enc, false)
}

pub fn esc_attr(s: &str, enc: Enc) -> String {
    esc(s, enc, true)
}

fn esc(s: &str, enc: Enc, attr: bool) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for (i, c) in s.char_indices() {
        match c {
            '&' => {
                if unknown_entity_at(&s[i..]) {
                    out.push('&');
                } else {
                    out.push_str("&amp;");
                }
            }
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attr => out.push_str("&quot;"),
            c if !enc.can_hold(c) => out.push_str(&format!("&#{};", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enc {
    Utf8,
    Utf8Bom,
    Latin1,
    Win1252,
    /// Any other legacy code page (Windows-125x, Shift_JIS, GBK, Big5, EUC-KR, KOI8, ...).
    Legacy(&'static bilingua_model::encoding_rs::Encoding),
    /// UTF-16 with a byte-order mark (`true`: big endian).
    Utf16(bool),
}

const W1252: [char; 32] = [
    '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{2c6}',
    '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}', '\u{8f}', '\u{90}', '\u{2018}', '\u{2019}',
    '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}',
    '\u{153}', '\u{9d}', '\u{17e}', '\u{178}',
];

impl Enc {
    pub fn name(self) -> &'static str {
        match self {
            Enc::Utf8 => "utf-8",
            Enc::Utf8Bom => "utf-8-bom",
            Enc::Latin1 => "iso-8859-1",
            Enc::Win1252 => "windows-1252",
            Enc::Legacy(e) => e.name(),
            Enc::Utf16(false) => "utf-16le",
            Enc::Utf16(true) => "utf-16be",
        }
    }

    /// The encoding of a skeleton tag (the part before any `;`).
    pub fn from_name(n: &str) -> Option<Enc> {
        match bilingua_model::enc::base(n) {
            "utf-16le" => return Some(Enc::Utf16(false)),
            "utf-16be" => return Some(Enc::Utf16(true)),
            _ => {}
        }
        match bilingua_model::enc::base(n) {
            "utf-8" => Some(Enc::Utf8),
            "utf-8-bom" => Some(Enc::Utf8Bom),
            "iso-8859-1" => Some(Enc::Latin1),
            "windows-1252" => Some(Enc::Win1252),
            // a GBK file that wrote its euro sign as A2 E3 keeps GB18030's spelling
            "gbk" if n.contains(";euro=a2e3") => Some(Enc::Legacy(bilingua_model::encoding_rs::GB18030)),
            other => bilingua_model::encoding_rs::Encoding::for_label(other.as_bytes()).map(Enc::Legacy),
        }
    }

    fn can_hold(self, c: char) -> bool {
        match self {
            Enc::Utf8 | Enc::Utf8Bom => true,
            Enc::Latin1 => (c as u32) < 256,
            Enc::Win1252 => (c as u32) < 128 || (160..256).contains(&(c as u32)) || W1252.contains(&c),
            Enc::Utf16(_) => true,
            Enc::Legacy(e) => !writer(e).encode(c.encode_utf8(&mut [0; 4])).2,
        }
    }

    pub fn encode(self, s: &str) -> Vec<u8> {
        match self {
            // GBK / GB2312: GB18030's bytes, but the euro sign is the single byte 0x80
            Enc::Legacy(e) if e == bilingua_model::encoding_rs::GBK => bilingua_model::enc::encode_gbk(s).0,
            Enc::Legacy(e) => writer(e).encode(s).0.into_owned(),
            Enc::Utf16(be) => {
                let mut v: Vec<u8> = if be { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
                for u in s.encode_utf16() {
                    v.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
                }
                v
            }
            Enc::Utf8 => s.as_bytes().to_vec(),
            Enc::Utf8Bom => {
                let mut v = vec![0xEF, 0xBB, 0xBF];
                v.extend_from_slice(s.as_bytes());
                v
            }
            Enc::Latin1 => s.chars().map(|c| if (c as u32) < 256 { c as u32 as u8 } else { b'?' }).collect(),
            Enc::Win1252 => s
                .chars()
                .map(|c| {
                    let u = c as u32;
                    if u < 128 || (160..256).contains(&u) {
                        u as u8
                    } else if let Some(p) = W1252.iter().position(|x| *x == c) {
                        0x80 + p as u8
                    } else {
                        b'?'
                    }
                })
                .collect(),
        }
    }
}

/// GB18030 writes everything GBK does and more.
fn writer(e: &'static bilingua_model::encoding_rs::Encoding) -> &'static bilingua_model::encoding_rs::Encoding {
    if e == bilingua_model::encoding_rs::GBK {
        bilingua_model::encoding_rs::GB18030
    } else {
        e
    }
}

/// Finds the declared charset in the first 2 KB (`<meta charset=..>` or `content="...; charset=.."`).
fn declared_charset(bytes: &[u8]) -> Option<String> {
    let head: String = bytes.iter().take(2048).map(|b| (*b as char).to_ascii_lowercase()).collect();
    let p = head.find("charset")?;
    let rest = head[p + 7..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.trim_start_matches(['"', '\'']);
    let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':')).unwrap_or(rest.len());
    let v = &rest[..end];
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// Reads an HTML file: a UTF-8 or UTF-16 byte-order mark, then the charset the file declares (UTF-8, Latin-1,
/// Windows-1252 and the legacy pages `bilingua_model::enc` reads), then UTF-8, and for an undeclared file that
/// is not UTF-8 the code page of the project's source language. The declaration stays as written: characters
/// the file's encoding cannot hold are written as numeric references on merge.
pub fn decode_input(bytes: &[u8], opts: &bilingua_model::ExtractOptions) -> Result<(String, Enc, Vec<bilingua_model::Warning>), Error> {
    bilingua_model::guard::check_input(bytes, &opts.limits())?;
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        let s = std::str::from_utf8(rest).map_err(|e| perr(e.to_string()))?;
        return Ok((s.to_string(), Enc::Utf8Bom, vec![]));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let d = bilingua_model::enc::decode_text(bytes, &bilingua_model::enc::DecodeOptions::new(opts))?;
        let enc = Enc::from_name(&d.encoding).ok_or_else(|| perr("cannot read this UTF-16 file".into()))?;
        return Ok((d.text, enc, d.warnings));
    }
    let declared = declared_charset(bytes);
    let enc = match declared.as_deref() {
        None | Some("utf-8") | Some("utf8") | Some("us-ascii") | Some("ascii") => None,
        Some("iso-8859-1") | Some("latin1") | Some("iso8859-1") | Some("l1") => Some(Enc::Latin1),
        Some("windows-1252") | Some("cp1252") | Some("x-cp1252") => Some(Enc::Win1252),
        Some(other) => match bilingua_model::encoding_rs::Encoding::for_label(other.as_bytes()).filter(|e| bilingua_model::enc::is_declared_legacy(e)) {
            Some(e) => {
                let s = bilingua_model::enc::decode_strict(e, bytes).map_err(|p| {
                    Error::Encoding(format!("This file declares {other} but is not valid in it (byte at position {p}). Re-save the file as UTF-8."))
                })?;
                return Ok((s, Enc::Legacy(e), vec![]));
            }
            None => return Err(Error::Unsupported(format!("HTML charset {other}"))),
        },
    };
    match enc {
        None => match std::str::from_utf8(bytes) {
            Ok(s) => Ok((s.to_string(), Enc::Utf8, vec![])),
            // the file declares UTF-8 (or ASCII) but is not: refused, never read with a fallback code page
            Err(e) if declared.is_some() => Err(Error::Encoding(format!(
                "This file declares the encoding {} but is not valid UTF-8 (byte at position {}). Re-save the file as UTF-8.",
                declared.as_deref().unwrap_or(""),
                e.valid_up_to()
            ))),
            Err(_) => {
                // undeclared and not UTF-8: the source language's code page (or Windows-1252), with a warning when it looks wrong
                let d = bilingua_model::enc::decode_text(bytes, &bilingua_model::enc::DecodeOptions::new(opts))?;
                let enc = Enc::from_name(&d.encoding).ok_or_else(|| perr(format!("unknown encoding {}", d.encoding)))?;
                Ok((d.text, enc, d.warnings))
            }
        },
        Some(Enc::Latin1) => Ok((bytes.iter().map(|b| *b as char).collect(), Enc::Latin1, vec![])),
        Some(e) => Ok((
            bytes
                .iter()
                .map(|b| if (0x80..0xa0).contains(b) { W1252[(*b - 0x80) as usize] } else { *b as char })
                .collect(),
            e,
            vec![],
        )),
    }
}

fn perr(msg: String) -> Error {
    Error::Parse { format: "html".into(), msg }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multibyte_text_after_an_ampersand_never_panics() {
        // Byte 34 after the '&' lands inside a multi-byte character; this used to panic on a str slice.
        for pad in 0..8 {
            let s = format!("&amp;{}日本語のテキストが続きます。日本語のテキスト", "a".repeat(pad));
            let out = decode_entities(&s);
            assert!(out.starts_with('&'));
            let s2 = format!("&{}日本語のテキストが続きます。日本語のテキスト;", "b".repeat(pad));
            let _ = decode_entities(&s2);
            let _ = esc_text(&s2, Enc::Utf8);
        }
    }

    #[test]
    fn latin1_table_is_complete() {
        assert_eq!(LATIN1.len(), 96);
        assert_eq!(named_entity("nbsp"), Some('\u{a0}'));
        assert_eq!(named_entity("yuml"), Some('\u{ff}'));
        assert_eq!(named_entity("eacute"), Some('\u{e9}'));
    }

    #[test]
    fn decode_and_encode() {
        assert_eq!(decode_entities("a &amp; b &lt;c&gt; &#233; &#x4e2d; &copy;"), "a & b <c> \u{e9} \u{4e2d} \u{a9}");
        assert_eq!(decode_entities("&bogus; & &#;"), "&bogus; & &#;");
        assert_eq!(esc_text("a & b <c>", Enc::Utf8), "a &amp; b &lt;c&gt;");
        assert_eq!(esc_text("&bogus; &amp;", Enc::Utf8), "&bogus; &amp;amp;");
        assert_eq!(esc_attr("say \"hi\"", Enc::Utf8), "say &quot;hi&quot;");
        assert_eq!(esc_text("\u{4e2d}", Enc::Latin1), "&#20013;");
    }

    #[test]
    fn collapse() {
        assert_eq!(decode_collapse("a \n\t b\u{a0} c"), "a b\u{a0} c");
    }
}
