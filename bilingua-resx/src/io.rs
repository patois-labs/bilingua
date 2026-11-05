//! Limits, the XML guard and the text envelope (encoding) of one file.
//!
//! This is a crate-local stand-in for the shared guards and encodings of `bilingua-model` (B6). Every function
//! here has the shape of its shared twin, so the swap is local to this file.

use bilingua_model::Error;

/// Size limits. Hostile files are refused with a typed error before any large allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Largest input in bytes.
    pub max_input_bytes: usize,
    /// Most units (translatable or locked) one file may hold.
    pub max_units: usize,
    /// Most characters in one value.
    pub max_unit_chars: usize,
    /// Deepest element nesting.
    pub max_xml_depth: usize,
    /// Most elements in the file.
    pub max_xml_elements: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_input_bytes: 100 << 20, max_units: 50_000, max_unit_chars: 100_000, max_xml_depth: 256, max_xml_elements: 4_000_000 }
    }
}

pub fn limit(msg: String) -> Error {
    Error::Unsupported(format!("input limit: {msg}"))
}

pub fn parse_err(msg: impl Into<String>) -> Error {
    Error::Parse { format: "resx".into(), msg: msg.into() }
}

pub fn check_input(bytes: &[u8], l: &Limits) -> Result<(), Error> {
    if bytes.len() > l.max_input_bytes {
        return Err(limit(format!("the file is larger than {} bytes. It was not processed.", l.max_input_bytes)));
    }
    Ok(())
}

pub fn check_unit_count(n: usize, l: &Limits) -> Result<(), Error> {
    if n > l.max_units {
        return Err(limit(format!("the file has more than {} units. Split it into smaller files.", l.max_units)));
    }
    Ok(())
}

fn find(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= hay.len() {
        return None;
    }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn unsafe_xml(msg: &str) -> Error {
    Error::Unsupported(format!("unsafe XML: {msg}"))
}

/// Scans the markup once and refuses a DTD internal subset or any `<!ENTITY` (entity bombs, external entities),
/// nesting deeper than `max_xml_depth` and more than `max_xml_elements` elements. Comments, CDATA and processing
/// instructions are skipped, so markup inside them is not counted.
pub fn check_xml(text: &str, l: &Limits) -> Result<(), Error> {
    let b = text.as_bytes();
    let (mut i, mut depth, mut elements) = (0usize, 0usize, 0usize);
    while let Some(p) = b.get(i..).and_then(|r| r.iter().position(|c| *c == b'<')) {
        i += p;
        let rest = &b[i..];
        if rest.starts_with(b"<!--") {
            match find(b, i + 4, b"-->") {
                Some(e) => i = e + 3,
                None => break,
            }
        } else if rest.starts_with(b"<![CDATA[") {
            match find(b, i + 9, b"]]>") {
                Some(e) => i = e + 3,
                None => break,
            }
        } else if rest.starts_with(b"<?") {
            match find(b, i + 2, b"?>") {
                Some(e) => i = e + 2,
                None => break,
            }
        } else if rest.len() >= 9 && rest[..9].eq_ignore_ascii_case(b"<!DOCTYPE") {
            // up to the closing '>' outside quotes; a '[' outside quotes opens an internal subset
            let (mut j, mut sq) = (i + 9, 0u8);
            while j < b.len() {
                let c = b[j];
                if sq != 0 {
                    if c == sq {
                        sq = 0;
                    }
                } else if c == b'"' || c == b'\'' {
                    sq = c;
                } else if c == b'[' {
                    return Err(unsafe_xml("the file declares a DTD internal subset (an entity-expansion and XXE vector). It was not processed."));
                } else if c == b'>' {
                    break;
                }
                j += 1;
            }
            i = j + 1;
        } else if rest.starts_with(b"<!") {
            if rest.len() >= 8 && rest[..8].eq_ignore_ascii_case(b"<!ENTITY") {
                return Err(unsafe_xml("the file declares XML entities, which are not allowed. It was not processed."));
            }
            match find(b, i + 2, b">") {
                Some(e) => i = e + 1,
                None => break,
            }
        } else if rest.starts_with(b"</") {
            depth = depth.saturating_sub(1);
            match find(b, i + 2, b">") {
                Some(e) => i = e + 1,
                None => break,
            }
        } else {
            let (mut j, mut sq) = (i + 1, 0u8);
            let mut end = None;
            while j < b.len() {
                let c = b[j];
                if sq != 0 {
                    if c == sq {
                        sq = 0;
                    }
                } else if c == b'"' || c == b'\'' {
                    sq = c;
                } else if c == b'>' {
                    end = Some(j);
                    break;
                }
                j += 1;
            }
            let Some(e) = end else { break };
            elements += 1;
            if elements > l.max_xml_elements {
                return Err(limit(format!("the file has more than {} XML elements. It was not processed.", l.max_xml_elements)));
            }
            if b[e - 1] != b'/' {
                depth += 1;
                if depth > l.max_xml_depth {
                    return Err(limit(format!("the file nests XML elements more than {} deep. It was not processed.", l.max_xml_depth)));
                }
            }
            i = e + 1;
        }
    }
    Ok(())
}

// ---- encodings -------------------------------------------------------------------------------------------------

const BOM8: &[u8] = &[0xEF, 0xBB, 0xBF];
pub const UTF8: &str = "utf-8";
pub const UTF8_BOM: &str = "utf-8-bom";
pub const UTF16LE: &str = "utf-16le";
pub const UTF16BE: &str = "utf-16be";
pub const CP1252: &str = "windows-1252";

const CP1252_HIGH: [Option<char>; 32] = [
    Some('\u{20AC}'),
    None,
    Some('\u{201A}'),
    Some('\u{0192}'),
    Some('\u{201E}'),
    Some('\u{2026}'),
    Some('\u{2020}'),
    Some('\u{2021}'),
    Some('\u{02C6}'),
    Some('\u{2030}'),
    Some('\u{0160}'),
    Some('\u{2039}'),
    Some('\u{0152}'),
    None,
    Some('\u{017D}'),
    None,
    None,
    Some('\u{2018}'),
    Some('\u{2019}'),
    Some('\u{201C}'),
    Some('\u{201D}'),
    Some('\u{2022}'),
    Some('\u{2013}'),
    Some('\u{2014}'),
    Some('\u{02DC}'),
    Some('\u{2122}'),
    Some('\u{0161}'),
    Some('\u{203A}'),
    Some('\u{0153}'),
    None,
    Some('\u{017E}'),
    Some('\u{0178}'),
];

fn utf16(body: &[u8], big: bool) -> Result<String, Error> {
    if !body.len().is_multiple_of(2) {
        return Err(parse_err("the UTF-16 file has an odd number of bytes; re-save the file as UTF-8"));
    }
    let units = body.chunks_exact(2).map(|p| if big { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) });
    let mut s = String::with_capacity(body.len() / 2);
    for (n, r) in char::decode_utf16(units).enumerate() {
        match r {
            Ok(c) => s.push(c),
            Err(_) => return Err(parse_err(format!("the file is not valid UTF-16 (bad surrogate near unit {n}); re-save the file as UTF-8"))),
        }
    }
    Ok(s)
}

fn binary(data: &[u8]) -> bool {
    data.contains(&0) || data.iter().filter(|&&b| b < 0x20 && !matches!(b, 0x09..=0x0D | 0x1A | 0x1B)).count() > 2 + data.len() / 100
}

/// The lower-case encoding name in the XML declaration of single-byte input.
fn declared_name(data: &[u8]) -> Option<String> {
    let head = &data[..data.len().min(200)];
    let h: String = head.iter().map(|&b| if b < 0x80 { b as char } else { '?' }).collect();
    let h = h.to_ascii_lowercase();
    let k = h.find("encoding")?;
    let rest = h[k + 8..].trim_start().strip_prefix('=')?.trim_start();
    let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &rest[1..];
    Some(rest[..rest.find(q)?].trim().to_string())
}

/// Bytes to text and the envelope tag the writer must use.
pub fn decode(data: &[u8]) -> Result<(String, String), Error> {
    if data.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) || data.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        return Err(Error::Unsupported("UTF-32 files; re-save the file as UTF-8".into()));
    }
    if let Some(body) = data.strip_prefix(BOM8) {
        let t = std::str::from_utf8(body).map_err(|e| parse_err(format!("the file is not valid UTF-8 (at byte {}); re-save the file as UTF-8", e.valid_up_to() + 3)))?;
        return Ok((t.to_string(), UTF8_BOM.into()));
    }
    if let Some(body) = data.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(body, false).map(|t| (t, UTF16LE.into()));
    }
    if let Some(body) = data.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(body, true).map(|t| (t, UTF16BE.into()));
    }
    if let Ok(t) = std::str::from_utf8(data) {
        return Ok((t.to_string(), UTF8.into()));
    }
    if binary(data) {
        return Err(parse_err("the file is not valid UTF-8 and looks like a binary file, not text; re-save the file as UTF-8"));
    }
    if let Some(name) = declared_name(data) {
        if !matches!(name.as_str(), "iso-8859-1" | "latin1" | "latin-1" | "windows-1252" | "cp1252" | "us-ascii" | "ascii") {
            return Err(Error::Unsupported(format!("the declared encoding {name} is not available in this build")));
        }
    }
    let mut s = String::with_capacity(data.len());
    for (i, &b) in data.iter().enumerate() {
        match b {
            0x80..=0x9F => match CP1252_HIGH[(b - 0x80) as usize] {
                Some(c) => s.push(c),
                None => {
                    return Err(parse_err(format!(
                        "the file is not valid UTF-8 and could not be read as Windows-1252 either (byte 0x{b:02x} at position {i}); re-save the file as UTF-8"
                    )))
                }
            },
            _ => s.push(b as char),
        }
    }
    Ok((s, CP1252.into()))
}

/// Text back to bytes in the envelope. `None` when a legacy code page cannot hold every character.
pub fn encode(text: &str, tag: &str) -> Result<Option<Vec<u8>>, Error> {
    Ok(Some(match tag {
        UTF8 => text.as_bytes().to_vec(),
        UTF8_BOM => {
            let mut v = BOM8.to_vec();
            v.extend_from_slice(text.as_bytes());
            v
        }
        UTF16LE | UTF16BE => {
            let big = tag == UTF16BE;
            let mut v = if big { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
            for u in text.encode_utf16() {
                v.extend_from_slice(&if big { u.to_be_bytes() } else { u.to_le_bytes() });
            }
            v
        }
        CP1252 => {
            let mut out = Vec::with_capacity(text.len());
            for c in text.chars() {
                let u = c as u32;
                match u {
                    0..=0x7F | 0xA0..=0xFF => out.push(u as u8),
                    0x80..=0x9F => return Ok(None),
                    _ => match CP1252_HIGH.iter().position(|h| *h == Some(c)) {
                        Some(p) => out.push(0x80 + p as u8),
                        None => return Ok(None),
                    },
                }
            }
            out
        }
        other => return Err(Error::Write(format!("unknown encoding {other:?} in the resx skeleton"))),
    }))
}
