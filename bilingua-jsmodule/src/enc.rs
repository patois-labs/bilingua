//! Reading and writing the encodings a JavaScript module may have: UTF-8 (BOM kept), UTF-16 with a BOM, Windows-1252.

use bilingua_model::Error;

pub const BOM8: &[u8] = &[0xEF, 0xBB, 0xBF];
pub const BOM16LE: &[u8] = &[0xFF, 0xFE];
pub const BOM16BE: &[u8] = &[0xFE, 0xFF];

/// The skeleton's encoding names.
pub const UTF8: &str = "utf-8";
pub const UTF8_BOM: &str = "utf-8-bom";
pub const UTF16LE: &str = "utf-16le-bom";
pub const UTF16BE: &str = "utf-16be-bom";
pub const CP1252: &str = "cp1252";

/// A decode refusal: code `encoding` (LocOS UnsupportedEncodingError).
fn enc_err(msg: String) -> Error {
    Error::Encoding(msg)
}

/// Windows-1252 bytes 0x80..=0x9F as characters (`None` for the five bytes the code page leaves undefined).
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

fn cp1252_decode(data: &[u8]) -> Result<String, Error> {
    let mut s = String::with_capacity(data.len());
    for (i, &b) in data.iter().enumerate() {
        match b {
            0x80..=0x9F => match CP1252_HIGH[(b - 0x80) as usize] {
                Some(c) => s.push(c),
                None => {
                    return Err(enc_err(format!(
                        "file is not valid UTF-8 and could not be read as Windows-1252 either (byte 0x{b:02x} at position {i}); re-save the file as UTF-8"
                    )))
                }
            },
            _ => s.push(b as char),
        }
    }
    Ok(s)
}

/// Windows-1252 bytes for `text`, or `None` when a character does not exist in the code page.
fn cp1252_encode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        let u = c as u32;
        match u {
            0..=0x7F | 0xA0..=0xFF => out.push(u as u8),
            0x80..=0x9F => return None,
            _ => out.push(
                CP1252_HIGH
                    .iter()
                    .position(|h| *h == Some(c))
                    .map(|p| 0x80 + p as u8)?,
            ),
        }
    }
    Some(out)
}

/// C0 controls a text file never carries (TAB, LF, VT, FF, CR, SUB and ESC are allowed).
fn is_binary_control(b: u8) -> bool {
    b < 0x20 && !matches!(b, 0x09..=0x0D | 0x1A | 0x1B)
}

/// A file that is not UTF-8 and holds a NUL or many control bytes is binary, not a legacy text file.
fn looks_binary(data: &[u8]) -> bool {
    if data.contains(&0) {
        return true;
    }
    let bad = data.iter().filter(|&&b| is_binary_control(b)).count();
    bad > 2 + data.len() / 100
}

fn utf16(body: &[u8], big: bool) -> Result<String, Error> {
    if !body.len().is_multiple_of(2) {
        return Err(enc_err(
            "the UTF-16 file has an odd number of bytes; re-save the file as UTF-8".into(),
        ));
    }
    let units = body.chunks_exact(2).map(|p| {
        if big {
            u16::from_be_bytes([p[0], p[1]])
        } else {
            u16::from_le_bytes([p[0], p[1]])
        }
    });
    let mut s = String::with_capacity(body.len() / 2);
    for (n, r) in char::decode_utf16(units).enumerate() {
        match r {
            Ok(c) => s.push(c),
            Err(_) => {
                return Err(enc_err(format!(
                "file is not valid UTF-16 (bad surrogate near unit {n}); re-save the file as UTF-8"
            )))
            }
        }
    }
    Ok(s)
}

/// Bytes to text and the encoding name the writer must use.
pub fn decode(data: &[u8]) -> Result<(String, &'static str), Error> {
    // UTF-32 BOMs start with a UTF-16 one; nobody ships UTF-32 resources.
    if data.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) || data.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        return Err(Error::Encoding("UTF-32 files are not supported; re-save the file as UTF-8.".into()));
    }
    if let Some(body) = data.strip_prefix(BOM8) {
        return utf8(body, 0).map(|t| (t, UTF8_BOM));
    }
    if let Some(body) = data.strip_prefix(BOM16LE) {
        return utf16(body, false).map(|t| (t, UTF16LE));
    }
    if let Some(body) = data.strip_prefix(BOM16BE) {
        return utf16(body, true).map(|t| (t, UTF16BE));
    }
    if let Ok(t) = std::str::from_utf8(data) {
        return Ok((t.to_string(), UTF8));
    }
    if looks_binary(data) {
        return Err(enc_err("file is not valid UTF-8 and looks like a binary file, not text; re-save the file as UTF-8".into()));
    }
    cp1252_decode(data).map(|t| (t, CP1252))
}

fn utf8(body: &[u8], skipped: usize) -> Result<String, Error> {
    std::str::from_utf8(body).map(str::to_string).map_err(|e| {
        enc_err(format!(
            "file is not valid UTF-8 (at byte {}); re-save the file as UTF-8",
            e.valid_up_to() + skipped
        ))
    })
}

/// Text back to bytes in the named encoding. A Windows-1252 file goes back in Windows-1252 when every character
/// fits, else as UTF-8 (no BOM).
pub fn encode(text: &str, encoding: &str) -> Result<Vec<u8>, Error> {
    match encoding {
        UTF8 => Ok(text.as_bytes().to_vec()),
        UTF8_BOM => {
            let mut v = BOM8.to_vec();
            v.extend_from_slice(text.as_bytes());
            Ok(v)
        }
        UTF16LE | UTF16BE => {
            let big = encoding == UTF16BE;
            let mut v = if big { BOM16BE } else { BOM16LE }.to_vec();
            for u in text.encode_utf16() {
                v.extend_from_slice(&if big {
                    u.to_be_bytes()
                } else {
                    u.to_le_bytes()
                });
            }
            Ok(v)
        }
        CP1252 => Ok(cp1252_encode(text).unwrap_or_else(|| text.as_bytes().to_vec())),
        other => Err(Error::Write(format!(
            "unknown encoding {other:?} in the js skeleton"
        ))),
    }
}
