//! Reading and writing the file's own encoding, the way the Localization OS XAML filter does: UTF-8 (with or
//! without a BOM), UTF-16 with a BOM, or a common legacy charset the XML declaration names. Anything else is
//! refused, never guessed. The envelope goes into the skeleton as a short tag.

use bilingua_model::Error;
use encoding_rs::Encoding;
use regex::Regex;
use std::sync::OnceLock;

pub struct Decoded {
    pub text: String,
    /// `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be` or an `encoding_rs` name such as `windows-1252`.
    pub tag: String,
}

fn err(msg: impl Into<String>) -> Error {
    Error::Parse { format: "xaml".into(), msg: msg.into() }
}

fn decl_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"\A<\?xml[^>]*?\bencoding\s*=\s*["']([A-Za-z0-9._:-]+)["']"#).expect("declaration pattern"))
}

/// The legacy text encodings a file may declare and be read as.
fn allowed(e: &'static Encoding) -> bool {
    use encoding_rs::*;
    [
        WINDOWS_1250,
        WINDOWS_1251,
        WINDOWS_1252,
        WINDOWS_1253,
        WINDOWS_1254,
        WINDOWS_1255,
        WINDOWS_1256,
        WINDOWS_1257,
        WINDOWS_1258,
        ISO_8859_2,
        ISO_8859_5,
        ISO_8859_7,
        ISO_8859_15,
        KOI8_R,
        KOI8_U,
        SHIFT_JIS,
        EUC_JP,
        GBK,
        GB18030,
        BIG5,
        EUC_KR,
    ]
    .contains(&e)
}

/// The declared charset in the first 200 bytes, if any.
fn declared(raw: &[u8]) -> Option<String> {
    let head: String = raw.iter().take(200).map(|b| if b.is_ascii() { *b as char } else { '?' }).collect();
    decl_re().captures(&head).map(|c| c[1].to_string())
}

fn strict(e: &'static Encoding, body: &[u8]) -> Option<String> {
    if e == encoding_rs::WINDOWS_1252 && body.iter().any(|b| matches!(b, 0x81 | 0x8D | 0x8F | 0x90 | 0x9D)) {
        return None;
    }
    e.decode_without_bom_handling_and_without_replacement(body).map(|c| c.into_owned())
}

pub fn decode(raw: &[u8]) -> Result<Decoded, Error> {
    if let Some(body) = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        let text = std::str::from_utf8(body).map_err(|e| err(format!("This XAML file is not valid utf-8 (byte at position {}); re-save the file as UTF-8.", e.valid_up_to() + 3)))?;
        return Ok(Decoded { text: text.to_string(), tag: "utf-8-bom".into() });
    }
    for (bom, enc, tag) in [([0xFFu8, 0xFE], encoding_rs::UTF_16LE, "utf-16le"), ([0xFE, 0xFF], encoding_rs::UTF_16BE, "utf-16be")] {
        if raw.starts_with(&bom) {
            let text = enc
                .decode_without_bom_handling_and_without_replacement(&raw[2..])
                .ok_or_else(|| err(format!("This XAML file is not valid {tag}; re-save the file as UTF-8.")))?;
            return Ok(Decoded { text: text.into_owned(), tag: tag.into() });
        }
    }
    let label = declared(raw);
    let lower = label.as_deref().map(str::to_ascii_lowercase);
    let enc = match lower.as_deref() {
        None | Some("utf-8" | "utf8" | "us-ascii" | "ascii") => encoding_rs::UTF_8,
        Some(l) => match Encoding::for_label(l.as_bytes()) {
            Some(e) if e == encoding_rs::UTF_8 => e,
            Some(e) if allowed(e) => e,
            _ => {
                return Err(err(format!(
                    "This XAML file declares the encoding {:?}, which is not supported; re-save the file as UTF-8.",
                    label.unwrap_or_default()
                )))
            }
        },
    };
    if enc == encoding_rs::UTF_8 {
        let text = std::str::from_utf8(raw).map_err(|e| err(format!("This XAML file is not valid utf-8 (byte at position {}); re-save the file as UTF-8.", e.valid_up_to())))?;
        return Ok(Decoded { text: text.to_string(), tag: "utf-8".into() });
    }
    let name = enc.name().to_ascii_lowercase();
    let text = strict(enc, raw).ok_or_else(|| err(format!("This XAML file is not valid {name}; re-save the file as UTF-8.")))?;
    // the writer encodes the text back; a file that would not come back byte for byte is refused, never altered
    let (back, _, lossy) = enc.encode(&text);
    if lossy || back.as_ref() != raw {
        return Err(Error::Unsupported(format!("This XAML file uses {name} byte sequences that cannot be written back unchanged; re-save the file as UTF-8.")));
    }
    Ok(Decoded { text, tag: name })
}

/// Writes `text` in the envelope `tag`. A character the legacy charset cannot hold becomes a numeric character
/// reference (legal in XML content and attribute values).
pub fn encode(text: &str, tag: &str) -> Result<Vec<u8>, Error> {
    Ok(match tag {
        "utf-8" => text.as_bytes().to_vec(),
        "utf-8-bom" => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(text.as_bytes());
            v
        }
        "utf-16le" | "utf-16be" => {
            let le = tag == "utf-16le";
            let mut v = Vec::with_capacity(text.len() * 2 + 2);
            v.extend_from_slice(if le { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] });
            for u in text.encode_utf16() {
                v.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
            }
            v
        }
        other => {
            let e = Encoding::for_label(other.as_bytes()).filter(|e| allowed(e)).ok_or_else(|| Error::Write(format!("unknown encoding {other}")))?;
            e.encode(text).0.into_owned()
        }
    })
}
