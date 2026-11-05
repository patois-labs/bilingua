//! One XLIFF member: how it is decoded, opened, stamped with a target language and encoded again.

use bilingua_model::Error;
use bilingua_xliff::{Doc, Format};
use quick_xml::events::Event;
use quick_xml::Reader;

/// How the member's bytes are stored, so a rewritten member keeps its encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enc {
    /// UTF-8, with or without a byte order mark (the bytes go to the XLIFF reader as they are).
    Utf8,
    /// UTF-16 with a byte order mark. The XLIFF reader gets UTF-8 text whose declaration says `UTF-8`; `label` is
    /// what the declaration said, written back on encode.
    Utf16 { le: bool, label: Option<String> },
}

/// The span of the encoding label inside the XML declaration, if there is one.
fn label_span(text: &str) -> Option<(usize, usize)> {
    if !text.trim_start_matches('\u{feff}').trim_start().starts_with("<?xml") {
        return None;
    }
    let decl_end = text.find("?>")?;
    let p = text[..decl_end].find("encoding")?;
    let rest = &text[p + 8..decl_end];
    let eq = rest.find('=')?;
    let after = &rest[eq + 1..];
    let q_at = after.find(|c: char| !c.is_whitespace())?;
    let q = after[q_at..].chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let start = p + 8 + eq + 1 + q_at + 1;
    let end = text[start..decl_end].find(q)? + start;
    Some((start, end))
}

pub fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "xlz".into(), msg: msg.into() }
}

fn declared_encoding(head: &[u8]) -> Option<String> {
    let s: String = head.iter().take(200).map(|b| *b as char).collect();
    let decl = &s[..s.find("?>")?];
    if !decl.trim_start().starts_with("<?xml") {
        return None;
    }
    let p = decl.find("encoding")?;
    let rest = decl[p + 8..].trim_start().strip_prefix('=')?.trim_start();
    let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let end = rest[1..].find(q)?;
    Some(rest[1..1 + end].to_string())
}

/// UTF-8 (with or without BOM) and UTF-16 with a BOM only, as LocOS does: the writer must put the file back in its
/// own encoding. Returns the bytes the XLIFF reader gets (UTF-8) and the encoding to write back in.
pub fn decode(name: &str, raw: &[u8]) -> Result<(Vec<u8>, Enc), Error> {
    let utf16 = |le: bool| -> Result<(Vec<u8>, Enc), Error> {
        let body = &raw[2..];
        if !body.len().is_multiple_of(2) {
            return Err(perr(format!("The XLIFF file {name:?} has a truncated UTF-16 body.")));
        }
        let units = body.chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) });
        let text: Result<String, _> = char::decode_utf16(units).collect();
        let mut text = text.map_err(|_| perr(format!("The XLIFF file {name:?} is not valid UTF-16.")))?;
        let mut label = None;
        if let Some((a, b)) = label_span(&text) {
            label = Some(text[a..b].to_string());
            text.replace_range(a..b, "UTF-8");
        }
        Ok((text.into_bytes(), Enc::Utf16 { le, label }))
    };
    if raw.starts_with(&[0xFF, 0xFE]) {
        return utf16(true);
    }
    if raw.starts_with(&[0xFE, 0xFF]) {
        return utf16(false);
    }
    let body = raw.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(raw);
    if let Some(label) = declared_encoding(body) {
        let l = label.to_ascii_lowercase().replace('_', "-");
        if l != "utf-8" && l != "utf8" {
            return Err(perr(format!(
                "The XLIFF file {name:?} is encoded as {label}; only UTF-8 and UTF-16 are supported. Re-save it as UTF-8 in the CAT tool."
            )));
        }
    }
    if std::str::from_utf8(body).is_err() {
        return Err(perr(format!("The XLIFF file {name:?} is not valid UTF-8. Re-save it as UTF-8.")));
    }
    Ok((raw.to_vec(), Enc::Utf8))
}

/// Writes UTF-8 text back in the member's encoding (UTF-16 gets its byte order mark).
pub fn encode(utf8: &[u8], enc: &Enc) -> Result<Vec<u8>, Error> {
    let Enc::Utf16 { le, label } = enc else {
        return Ok(utf8.to_vec());
    };
    let le = *le;
    let text = std::str::from_utf8(utf8).map_err(|_| Error::Write("the merged XLIFF member is not valid UTF-8".into()))?;
    let mut text = text.to_string();
    if let (Some(l), Some((a, b))) = (label, label_span(&text)) {
        text.replace_range(a..b, l);
    }
    let text = text.as_str();
    let mut out: Vec<u8> = if le { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
    for u in text.encode_utf16() {
        out.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
    }
    Ok(out)
}

/// Opens a member for reading or writing. Only XLIFF 1.2 (with its vendor dialects) is accepted, as LocOS.
pub fn open(name: &str, utf8: Vec<u8>) -> Result<Doc, Error> {
    let doc = Doc::from_bytes_hint(utf8, name.to_ascii_lowercase().ends_with(".mxliff"))
        .map_err(|e| perr(format!("The XLIFF file {name:?} could not be read: {e}")))?;
    if doc.format() == Format::Xliff2 {
        return Err(perr(format!("The XLIFF file {name:?} is XLIFF 2; only XLIFF 1.2 files are read inside a package.")));
    }
    Ok(doc)
}

fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;")
}

/// Sets `target-language` on every `<file>` that has none (or a blank one), keeping every other byte. A file's own
/// language is never changed. Returns `None` when nothing needed it.
pub fn stamp_language(text: &str, tag: &str) -> Option<String> {
    let mut rd = Reader::from_str(text);
    rd.config_mut().check_end_names = false;
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    loop {
        let before = rd.buffer_position() as usize;
        let ev = match rd.read_event() {
            Ok(e) => e,
            Err(_) => return None,
        };
        let after = rd.buffer_position() as usize;
        match ev {
            Event::Eof => break,
            Event::Start(ref e) | Event::Empty(ref e) => {
                let local = e.local_name();
                if !local.as_ref().eq_ignore_ascii_case(b"file") {
                    continue;
                }
                let Some(start_tag) = text.get(before..after) else { continue };
                if let Some(new) = with_target_language(start_tag, tag) {
                    edits.push((before, after, new));
                }
            }
            _ => {}
        }
    }
    if edits.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(text.len() + edits.len() * 32);
    let mut at = 0;
    for (s, e, new) in edits {
        out.push_str(&text[at..s]);
        out.push_str(&new);
        at = e;
    }
    out.push_str(&text[at..]);
    Some(out)
}

/// The start tag with `target-language` set, or `None` when it already has a non-blank one.
fn with_target_language(tag: &str, value: &str) -> Option<String> {
    let b = tag.as_bytes();
    // find ` target-language` as a whole attribute name outside quotes
    let mut i = 0;
    let mut quote = 0u8;
    while i < b.len() {
        let c = b[i];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c.is_ascii_whitespace() && tag[i + 1..].starts_with("target-language") {
            let mut j = i + 1 + "target-language".len();
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if b.get(j) == Some(&b'=') {
                j += 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                let q = *b.get(j)?;
                if q != b'"' && q != b'\'' {
                    return None;
                }
                let end = tag[j + 1..].find(q as char)? + j + 1;
                if !tag[j + 1..end].trim().is_empty() {
                    return None;
                }
                let v = if q == b'"' { esc_attr(value) } else { esc_attr(value).replace('\'', "&apos;") };
                return Some(format!("{}{}{}", &tag[..j + 1], v, &tag[end..]));
            }
        }
        i += 1;
    }
    let mut close = tag.len() - if tag.ends_with("/>") { 2 } else { 1 };
    while close > 0 && b[close - 1].is_ascii_whitespace() {
        close -= 1;
    }
    Some(format!("{} target-language=\"{}\"{}", &tag[..close], esc_attr(value), &tag[close..]))
}

/// The canonical BCP 47 tag of `code` (`fr_fr` gives `fr-FR`), or `None` when it is not a usable tag.
pub fn html_lang(code: &str) -> Option<String> {
    let tag = code.trim().replace('_', "-");
    if tag.is_empty() || tag.len() > 64 {
        return None;
    }
    let mut parts = tag.split('-');
    let first = parts.next()?;
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let mut out = vec![first.to_ascii_lowercase()];
    let mut ext = false;
    for sub in parts {
        if sub.is_empty() || sub.len() > 8 || !sub.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        let alpha = sub.bytes().all(|b| b.is_ascii_alphabetic());
        if sub.len() == 1 {
            ext = true;
            out.push(sub.to_ascii_lowercase());
        } else if ext {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let mut s = sub.to_ascii_lowercase();
            s[..1].make_ascii_uppercase();
            out.push(s);
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && sub.bytes().all(|b| b.is_ascii_digit())) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_only_blank() {
        assert_eq!(with_target_language("<file a=\"1\">", "fr").unwrap(), "<file a=\"1\" target-language=\"fr\">");
        assert_eq!(with_target_language("<file a=\"1\" />", "fr").unwrap(), "<file a=\"1\" target-language=\"fr\" />");
        assert!(with_target_language("<file target-language=\"de\">", "fr").is_none());
        assert_eq!(with_target_language("<file target-language='' x='1'>", "fr").unwrap(), "<file target-language='fr' x='1'>");
        assert!(with_target_language("<file x=\" target-language='x'\">", "fr").unwrap().ends_with("target-language=\"fr\">"));
    }

    #[test]
    fn tags() {
        assert_eq!(html_lang("fr_fr").unwrap(), "fr-FR");
        assert_eq!(html_lang("zh-hant-tw").unwrap(), "zh-Hant-TW");
        assert!(html_lang("a b").is_none());
        assert!(html_lang("-").is_none());
    }
}
