//! Declaring the target language in a merged file, as Localization OS does: XLIFF 1.2 (and its dialects)
//! `target-language` on every `<file>`, XLIFF 2 `trgLang` on the root `<xliff>`. Nothing else changes; the edit is a
//! byte patch of the start tag.

use crate::attrs::TagInfo;
use crate::patch::{apply, AttrSlot, Patch};
use crate::Format;
use quick_xml::events::Event;
use quick_xml::Reader;

/// A BCP 47 tag in its usual case (`es-ES`, `zh-Hant-TW`), or `None` when `locale` is not a plausible tag.
pub(crate) fn canonical_tag(locale: &str) -> Option<String> {
    let tag = locale.trim().replace('_', "-");
    if tag.is_empty() || tag.len() > 64 {
        return None;
    }
    let mut parts = tag.split('-');
    let first = parts.next()?;
    if !(2..=8).contains(&first.len()) || !first.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut out = vec![first.to_ascii_lowercase()];
    let mut extension = false;
    for sub in parts {
        if sub.is_empty() || sub.len() > 8 || !sub.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        let alpha = sub.chars().all(|c| c.is_ascii_alphabetic());
        let digit = sub.chars().all(|c| c.is_ascii_digit());
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let mut c = sub.to_ascii_lowercase();
            c[..1].make_ascii_uppercase();
            out.push(c);
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && digit) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

fn local(name: &[u8]) -> &[u8] {
    name.rsplit(|b| *b == b':').next().unwrap_or(name)
}

/// `bytes` (a whole XLIFF file, BOM allowed) with the target language declared. A file that cannot be scanned, or
/// a `locale` that is not a language tag, comes back unchanged.
pub(crate) fn stamp_target_language(bytes: &[u8], format: Format, locale: &str) -> Vec<u8> {
    let Some(tag) = canonical_tag(locale) else { return bytes.to_vec() };
    let bom = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
    let body = &bytes[bom..];
    let (element, attr): (&[u8], &str) = if format == Format::Xliff2 { (b"xliff", "trgLang") } else { (b"file", "target-language") };
    let mut reader = Reader::from_reader(body);
    let mut patches: Vec<Patch> = Vec::new();
    loop {
        let start = reader.buffer_position() as usize;
        let ev = match reader.read_event() {
            Ok(ev) => ev,
            Err(_) => return bytes.to_vec(),
        };
        let end = reader.buffer_position() as usize;
        match ev {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) if local(e.name().as_ref()) == element => {
                let t = TagInfo::parse(body, start, end);
                if let Some(p) = AttrSlot::from_tag(body, &t, attr).patch(attr, Some(&tag), None) {
                    patches.push(p);
                }
                if format == Format::Xliff2 {
                    break; // only the root
                }
            }
            _ => {}
        }
    }
    let mut out = bytes[..bom].to_vec();
    out.extend(apply(body, patches));
    out
}
