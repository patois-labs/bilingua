//! Reading and writing a TTX file in its own encoding. TTX is usually UTF-16 LE with a BOM; UTF-16 without a BOM and
//! UTF-8 (BOM or not) are read too, and the file is written back the same way. Anything else is refused, as LocOS does.
//!
//! The skeleton encoding tag is `base[;nl=crlf|cr][;us=N]`: `base` is `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be`
//! (both with a BOM) or `utf-16le-nobom`, `utf-16be-nobom`; `us` is the index of the skeleton part that holds the
//! `<UserSettings>` start tag (so the writer can stamp the target language).

use bilingua_model::encoding_rs;
use bilingua_model::Error;

pub struct Decoded {
    pub text: String,
    pub base: &'static str,
}

fn refuse(msg: String) -> Error {
    Error::Encoding(msg)
}

/// The encoding named by the first `encoding=` in the first 200 bytes, as LocOS reads it.
fn declared(raw: &[u8]) -> Option<String> {
    let head: String = raw.iter().take(200).map(|b| if b.is_ascii() { *b as char } else { '?' }).collect();
    let mut from = 0;
    while let Some(p) = head[from..].find("encoding") {
        let mut rest = head[from + p + 8..].trim_start_matches([' ', '\t', '\r', '\n']);
        from += p + 8;
        let Some(r) = rest.strip_prefix('=') else { continue };
        rest = r.trim_start_matches([' ', '\t', '\r', '\n']);
        let Some(r) = rest.strip_prefix(['"', '\'']) else { continue };
        let name: String = r.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')).collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

pub fn decode(raw: &[u8]) -> Result<Decoded, Error> {
    let (base, body, e16): (&'static str, &[u8], Option<&'static encoding_rs::Encoding>) = if let Some(r) = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        ("utf-8-bom", r, None)
    } else if let Some(r) = raw.strip_prefix(&[0xFF, 0xFE]) {
        ("utf-16le", r, Some(encoding_rs::UTF_16LE))
    } else if let Some(r) = raw.strip_prefix(&[0xFE, 0xFF]) {
        ("utf-16be", r, Some(encoding_rs::UTF_16BE))
    } else if raw.starts_with(b"<\0") {
        ("utf-16le-nobom", raw, Some(encoding_rs::UTF_16LE))
    } else if raw.starts_with(b"\0<") {
        ("utf-16be-nobom", raw, Some(encoding_rs::UTF_16BE))
    } else {
        if let Some(name) = declared(raw) {
            if !matches!(name.to_ascii_lowercase().replace('_', "-").as_str(), "utf-8" | "utf8") {
                return Err(refuse(format!(
                    "This TTX file is encoded as {name}; only UTF-16 and UTF-8 are supported. Re-save it from TagEditor (TTX files are normally UTF-16)."
                )));
            }
        }
        ("utf-8", raw, None)
    };
    let text = match e16 {
        Some(e) => e.decode_without_bom_handling_and_without_replacement(body).map(|c| c.into_owned()),
        None => std::str::from_utf8(body).ok().map(str::to_string),
    };
    match text {
        Some(text) => Ok(Decoded { text, base }),
        None => Err(refuse(format!(
            "This TTX file is not valid {}. Re-save it from TagEditor.",
            if e16.is_some() { "UTF-16" } else { "UTF-8" }
        ))),
    }
}

pub struct Env {
    pub base: String,
    pub crlf: bool,
    pub us: Option<usize>,
}

pub fn tag(base: &str, text: &str, us: Option<usize>) -> String {
    let mut t = base.to_string();
    if bilingua_model::enc::newline_style(text) == "\r\n" {
        t.push_str(";nl=crlf");
    }
    if let Some(u) = us {
        t.push_str(&format!(";us={u}"));
    }
    t
}

pub fn parse_tag(tag: &str) -> Env {
    let mut it = tag.split(';');
    let base = it.next().unwrap_or("utf-8").to_string();
    let (mut crlf, mut us) = (false, None);
    for p in it {
        if p == "nl=crlf" {
            crlf = true;
        } else if let Some(n) = p.strip_prefix("us=") {
            us = n.parse().ok();
        }
    }
    Env { base, crlf, us }
}

pub fn encode(text: &str, base: &str) -> Result<Vec<u8>, Error> {
    let u16s = |be: bool, bom: bool| {
        let mut v = Vec::with_capacity(text.len() * 2 + 2);
        if bom {
            v.extend_from_slice(if be { &[0xFE, 0xFF] } else { &[0xFF, 0xFE] });
        }
        for u in text.encode_utf16() {
            v.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
        }
        v
    };
    Ok(match base {
        "utf-8" => text.as_bytes().to_vec(),
        "utf-8-bom" => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(text.as_bytes());
            v
        }
        "utf-16le" => u16s(false, true),
        "utf-16be" => u16s(true, true),
        "utf-16le-nobom" => u16s(false, false),
        "utf-16be-nobom" => u16s(true, false),
        other => return Err(Error::Write(format!("unknown encoding {other}"))),
    })
}
