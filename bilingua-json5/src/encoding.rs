//! Reading and writing the byte encodings a JSON5 / JSONC file may have, the way LocOS reads text uploads:
//! UTF-8 (with or without a BOM), UTF-16 with a BOM, otherwise the Windows code page of the source language
//! (Windows-1252 when the language has none). A file that looks binary, or whose legacy bytes do not decode and
//! encode back to the same bytes, is refused with a typed error. Output keeps the input's encoding when every
//! character fits; otherwise the whole file is written as UTF-8.

use bilingua_model::Error;
use encoding_rs::Encoding;

pub const UTF8: &str = "utf-8";
pub const UTF8_BOM: &str = "utf-8-bom";
pub const UTF16LE: &str = "utf-16le";
pub const UTF16BE: &str = "utf-16be";

/// A decode refusal: code `encoding` (LocOS UnsupportedEncodingError).
fn refuse(msg: impl Into<String>) -> Error {
    Error::Encoding(msg.into())
}

fn looks_binary(data: &[u8]) -> bool {
    let mut bad = 0usize;
    for &b in data {
        if b == 0 {
            return true;
        }
        if b < 0x20 && !matches!(b, 0x09 | 0x0a | 0x0b | 0x0c | 0x0d | 0x1a | 0x1b) {
            bad += 1;
        }
    }
    bad > 2 + data.len() / 100
}

/// The Windows ANSI code page of a language tag (LocOS `target_code_page`), as an encoding.
fn code_page_for(lang: Option<&str>) -> Option<&'static Encoding> {
    let tag = lang?.replace('_', "-");
    let mut parts = tag.split('-');
    let l = parts.next()?.to_ascii_lowercase();
    let rest: Vec<String> = parts.map(|p| p.to_string()).collect();
    let script = rest.iter().find(|p| p.len() == 4).map(|s| s.as_str());
    let region = rest.iter().find(|p| p.len() == 2).map(|s| s.to_ascii_uppercase());
    let label = if l == "zh" {
        if script == Some("Hant") || matches!(region.as_deref(), Some("TW" | "HK" | "MO")) {
            "big5"
        } else {
            "gbk"
        }
    } else if script == Some("Cyrl") && matches!(l.as_str(), "sr" | "bs" | "az" | "uz" | "mn" | "tg") {
        "windows-1251"
    } else if script == Some("Latn") && l == "sr" {
        "windows-1250"
    } else {
        match l.as_str() {
            "cs" | "pl" | "hu" | "hr" | "sk" | "sl" | "ro" | "sq" | "bs" | "hsb" | "dsb" => "windows-1250",
            "ru" | "uk" | "bg" | "be" | "mk" | "sr" | "kk" | "ky" | "tt" | "mn" | "ba" | "sah" | "tg" => "windows-1251",
            "el" => "windows-1253",
            "tr" | "az" | "uz" => "windows-1254",
            "he" | "yi" => "windows-1255",
            "ar" | "fa" | "ur" | "ps" | "ug" | "sd" => "windows-1256",
            "et" | "lv" | "lt" => "windows-1257",
            "vi" => "windows-1258",
            "th" => "windows-874",
            "ja" => "shift_jis",
            "ko" => "euc-kr",
            _ => return None,
        }
    };
    Encoding::for_label(label.as_bytes())
}

fn strict_decode(enc: &'static Encoding, data: &[u8]) -> Option<String> {
    if enc.name() == "windows-1252" && data.iter().any(|b| matches!(b, 0x81 | 0x8d | 0x8f | 0x90 | 0x9d)) {
        return None; // undefined in the Windows code page; the WHATWG table maps them to C1 controls
    }
    enc.decode_without_bom_handling_and_without_replacement(data).map(|c| c.into_owned())
}

/// Decodes `data` to text. Returns the text and the encoding name to record in the skeleton.
pub fn decode(data: &[u8], source_lang: Option<&str>) -> Result<(String, String), Error> {
    if let Some(rest) = data.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        if let Ok(s) = std::str::from_utf8(rest) {
            return Ok((s.to_string(), UTF8_BOM.into()));
        }
    } else if let Ok(s) = std::str::from_utf8(data) {
        return Ok((s.to_string(), UTF8.into()));
    }
    if data.len() >= 2 && (data[..2] == [0xFF, 0xFE] || data[..2] == [0xFE, 0xFF]) {
        let le = data[0] == 0xFF;
        let body = &data[2..];
        if body.len().is_multiple_of(2) {
            let units: Vec<u16> =
                body.chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
            if let Ok(s) = String::from_utf16(&units) {
                return Ok((s, if le { UTF16LE } else { UTF16BE }.into()));
            }
        }
    }
    if looks_binary(data) {
        return Err(refuse("file is not valid UTF-8 and looks like a binary file, not text; re-save the file as UTF-8."));
    }
    let primary = code_page_for(source_lang).unwrap_or(encoding_rs::WINDOWS_1252);
    let attempts = if primary == encoding_rs::WINDOWS_1252 { vec![primary] } else { vec![primary, encoding_rs::WINDOWS_1252] };
    for enc in attempts {
        if let Some(text) = strict_decode(enc, data) {
            // Byte-faithful merge needs an exact round trip; refuse the rare file that has none.
            let (back, _, bad) = enc.encode(&text);
            if !bad && back.as_ref() == data {
                return Ok((text, enc.name().to_ascii_lowercase()));
            }
        }
    }
    Err(refuse("the file is not valid UTF-8 and could not be read as a Windows code page either; re-save it as UTF-8"))
}

/// Encodes `text` for the recorded encoding name. A legacy page that cannot hold every character gives UTF-8.
pub fn encode(text: &str, name: &str) -> Vec<u8> {
    match name {
        UTF8 => text.as_bytes().to_vec(),
        UTF8_BOM => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(text.as_bytes());
            v
        }
        UTF16LE | UTF16BE => {
            let le = name == UTF16LE;
            let mut v = if le { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
            for u in text.encode_utf16() {
                v.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
            }
            v
        }
        other => match Encoding::for_label(other.as_bytes()) {
            Some(enc) => {
                let (bytes, _, bad) = enc.encode(text);
                if bad {
                    text.as_bytes().to_vec()
                } else {
                    bytes.into_owned()
                }
            }
            None => text.as_bytes().to_vec(),
        },
    }
}
