//! Reading and writing the file's text (UTF-8 with or without a BOM, UTF-16 with a BOM; everything else is
//! refused, as LocOS does for Wordfast files), XML escaping, and the target-language tag.

use bilingua_model::enc::{self, DecodeOptions};
use bilingua_model::{Error, ExtractOptions, Limits, Warning};

pub struct Decoded {
    pub text: String,
    pub encoding: String,
    pub warnings: Vec<Warning>,
}

/// The `encoding` named by the XML declaration in the first 200 bytes, if any.
fn declared(bytes: &[u8]) -> Option<String> {
    let head: String = bytes.iter().take(200).map(|b| if b.is_ascii() { *b as char } else { '?' }).collect();
    let at = head.find("encoding")?;
    let rest = head[at + 8..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let q = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let body = &rest[1..];
    let end = body.find(q)?;
    Some(body[..end].to_string())
}

pub fn decode(fmt_name: &str, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Decoded, Error> {
    let upper = fmt_name.to_ascii_uppercase();
    let has_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]) || bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]);
    if !has_bom {
        if let Some(d) = declared(bytes) {
            let n = d.to_ascii_lowercase().replace('_', "-");
            if n != "utf-8" && n != "utf8" {
                return Err(Error::Encoding(format!(
                    "This {upper} file is encoded as {d}; only UTF-8 and UTF-16 are supported. Re-save it as UTF-8 in the CAT tool."
                )));
            }
        }
    }
    let o = DecodeOptions::with_limits(*limits).with_source_lang(opts.source_lang.as_deref()).declared();
    let dec = enc::decode_text(bytes, &o)?;
    if !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") {
        return Err(Error::Encoding(format!("This {upper} file is not valid UTF-8. Re-save it as UTF-8.")));
    }
    Ok(Decoded { text: dec.text, encoding: dec.encoding, warnings: dec.warnings })
}

/// Escapes character data: `&` and `<`, and `]]>`. A bare `>` stays as written.
pub fn esc_text(s: &str, out: &mut String) {
    let b = s.as_bytes();
    let mut last = 0;
    for (i, c) in b.iter().enumerate() {
        let rep = match c {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' if i >= 2 && &b[i - 2..i] == b"]]" => "&gt;",
            _ => continue,
        };
        out.push_str(&s[last..i]);
        out.push_str(rep);
        last = i + 1;
    }
    out.push_str(&s[last..]);
}

/// Keeps XML 1.0 characters, turning pasted Word breaks into newlines.
pub fn xml_target_text(s: &str) -> String {
    s.chars()
        .map(|c| if matches!(c, '\u{0B}' | '\u{0C}') { '\n' } else { c })
        .filter(|c| !matches!(*c as u32, 0..=8 | 0x0E..=0x1F | 0xFFFE | 0xFFFF))
        .collect()
}

/// The canonical BCP 47 tag for a stored locale code, or None when it is not usable (LocOS `html_lang`).
pub fn html_lang(code: &str) -> Option<String> {
    let t = code.trim().replace('_', "-");
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    let parts: Vec<&str> = t.split('-').collect();
    let first = parts[0];
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    for sub in &parts[1..] {
        if sub.is_empty() || sub.len() > 8 || !sub.bytes().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
    }
    let mut out = vec![first.to_ascii_lowercase()];
    let mut extension = false;
    for sub in &parts[1..] {
        let alpha = sub.bytes().all(|c| c.is_ascii_alphabetic());
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let l = sub.to_ascii_lowercase();
            out.push(format!("{}{}", l[..1].to_ascii_uppercase(), &l[1..]));
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && sub.bytes().all(|c| c.is_ascii_digit())) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}
