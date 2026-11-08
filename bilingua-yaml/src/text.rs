//! Reading and writing the file's bytes: UTF-8 (with or without BOM), UTF-16 with BOM, and Windows-1252 as the
//! legacy fallback (the same order LocOS tries).

use bilingua_model::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enc {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Cp1252,
    /// The code page of the project's source language (Windows-1251 for Russian, ...).
    Legacy(&'static bilingua_model::encoding_rs::Encoding),
}

impl Enc {
    pub fn label(self) -> &'static str {
        match self {
            Enc::Utf8 => "utf-8",
            Enc::Utf8Bom => "utf-8-bom",
            Enc::Utf16Le => "utf-16le",
            Enc::Utf16Be => "utf-16be",
            Enc::Cp1252 => "cp1252",
            Enc::Legacy(e) => e.name(),
        }
    }
    pub fn from_label(s: &str) -> Option<Enc> {
        Some(match s {
            "utf-8" => Enc::Utf8,
            "utf-8-bom" => Enc::Utf8Bom,
            "utf-16le" => Enc::Utf16Le,
            "utf-16be" => Enc::Utf16Be,
            "cp1252" => Enc::Cp1252,
            other => Enc::Legacy(bilingua_model::encoding_rs::Encoding::for_label(other.as_bytes())?),
        })
    }
}

const CP1252_HI: [u16; 32] = [
    0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161,
    0x203A, 0x0153, 0, 0x017E, 0x0178,
];

fn looks_binary(d: &[u8]) -> bool {
    if d.contains(&0) {
        return true;
    }
    let bad = d.iter().filter(|&&b| b < 0x20 && !matches!(b, 9..=13 | 0x1A | 0x1B)).count();
    bad > 2 + d.len() / 100
}

/// A decode refusal: code `encoding` (LocOS UnsupportedEncodingError).
fn err(msg: impl Into<String>) -> Error {
    Error::Encoding(msg.into())
}

/// Bytes to text, the encoding the writer must use, and what the reader noticed. A file that is neither UTF-8 nor
/// UTF-16 is read in the code page of `source_lang` (LocOS decision 22), Windows-1252 when the language has none.
pub fn decode(bytes: &[u8], source_lang: Option<&str>) -> Result<(String, Enc, Vec<bilingua_model::Warning>), Error> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        if let Ok(s) = std::str::from_utf8(rest) {
            return Ok((s.to_string(), Enc::Utf8Bom, Vec::new()));
        }
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok((s.to_string(), Enc::Utf8, Vec::new()));
    }
    let le = bytes.starts_with(&[0xFF, 0xFE]);
    if le || bytes.starts_with(&[0xFE, 0xFF]) {
        let body = &bytes[2..];
        if body.len().is_multiple_of(2) {
            let units = body.chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) });
            if let Ok(s) = char::decode_utf16(units).collect::<Result<String, _>>() {
                return Ok((s, if le { Enc::Utf16Le } else { Enc::Utf16Be }, Vec::new()));
            }
        }
    }
    if looks_binary(bytes) {
        return Err(err("file is not valid UTF-8 and looks like a binary file, not text; re-save the file as UTF-8."));
    }
    if let Some(r) = bilingua_model::enc::decode_source_legacy(bytes, source_lang) {
        let (text, e, warnings) = r?;
        let enc = if e == bilingua_model::encoding_rs::WINDOWS_1252 { Enc::Cp1252 } else { Enc::Legacy(e) };
        return Ok((text, enc, warnings));
    }
    let mut s = String::with_capacity(bytes.len());
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            0x80..=0x9F => match CP1252_HI[(b - 0x80) as usize] {
                0 => return Err(err(format!("the file is not valid UTF-8 or Windows-1252 (byte 0x{b:02x} at {i}); save it as UTF-8"))),
                c => s.push(char::from_u32(c as u32).unwrap_or('?')),
            },
            _ => s.push(b as char),
        }
    }
    Ok((s, Enc::Cp1252, Vec::new()))
}

/// Writes `text` in `enc`. Returns None when the text does not fit (Windows-1252 only).
pub fn encode(text: &str, enc: Enc) -> Option<Vec<u8>> {
    Some(match enc {
        Enc::Utf8 => text.as_bytes().to_vec(),
        Enc::Utf8Bom => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(text.as_bytes());
            v
        }
        Enc::Utf16Le | Enc::Utf16Be => {
            let le = enc == Enc::Utf16Le;
            let mut v = if le { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
            for u in text.encode_utf16() {
                v.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
            }
            v
        }
        Enc::Legacy(e) => {
            let (bytes, _, bad) = e.encode(text);
            if bad {
                return None;
            }
            bytes.into_owned()
        }
        Enc::Cp1252 => {
            let mut v = Vec::with_capacity(text.len());
            for c in text.chars() {
                let cp = c as u32;
                if cp < 0x80 || (0xA0..=0xFF).contains(&cp) {
                    v.push(cp as u8);
                } else {
                    let i = CP1252_HI.iter().position(|&x| x != 0 && x as u32 == cp)?;
                    v.push(0x80 + i as u8);
                }
            }
            v
        }
    })
}

/// YAML forbids most control characters, U+0080-U+009F (except U+0085), U+FFFE and U+FFFF.
pub fn first_forbidden(text: &str) -> Option<usize> {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c < 0x80 {
            if (c < 0x20 && !matches!(c, 9 | 10 | 13)) || c == 0x7F {
                return Some(i);
            }
        } else if c == 0xC2 && b.get(i + 1).is_some_and(|&n| (0x80..=0x9F).contains(&n) && n != 0x85) || c == 0xEF && b.get(i + 1) == Some(&0xBF) && b.get(i + 2).is_some_and(|&n| n == 0xBE || n == 0xBF) {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for (bytes, enc) in [
            (b"a: 1".to_vec(), Enc::Utf8),
            (b"\xEF\xBB\xBFa: 1".to_vec(), Enc::Utf8Bom),
            (encode("a: \u{fc}", Enc::Utf16Le).unwrap(), Enc::Utf16Le),
            (encode("a: \u{fc}", Enc::Utf16Be).unwrap(), Enc::Utf16Be),
            (b"a: caf\xE9 \x80".to_vec(), Enc::Cp1252),
        ] {
            let (t, e, _) = decode(&bytes, None).unwrap();
            assert_eq!(e, enc);
            assert_eq!(encode(&t, e).unwrap(), bytes);
        }
        assert!(decode(b"\xFF\xD8\xFF\x00\x81binary\x00").is_err());
        assert!(decode(b"a: \x81").is_err());
    }
}
