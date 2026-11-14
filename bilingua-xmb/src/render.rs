//! Translated inline text back to message content. Tolerant, like Localization OS: an unknown or repeated tag and
//! a stray close are dropped (never written as literal brackets) and an unclosed pair is closed at the end.

use crate::scan;
use bilingua_model::*;
use std::collections::{BTreeMap, HashSet};

/// Keeps XML 1.0 characters, turning pasted Word breaks into newlines.
pub fn xml_target_text(s: &str) -> String {
    s.chars()
        .map(|c| if matches!(c, '\u{0B}' | '\u{0C}') { '\n' } else { c })
        .filter(|c| !matches!(*c as u32, 0..=8 | 0x0E..=0x1F | 0xD800..=0xDFFF | 0xFFFE | 0xFFFF))
        .collect()
}

pub fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn escape_attr(s: &str, quote: u8) -> String {
    let s = escape_text(s);
    if quote == b'\'' {
        s.replace('\'', "&apos;")
    } else {
        s.replace('"', "&quot;")
    }
}

/// The `name` of a `<ph name="..">` as written.
pub fn name_of_raw(raw: &str) -> String {
    scan::start_tag(raw, 0).ok().and_then(|t| t.attrs.into_iter().find(|a| a.name == "name").map(|a| a.value)).unwrap_or_default()
}

fn ph(name: &str, raw: Option<&str>, original_raw: bool) -> String {
    match raw {
        Some(r) if original_raw && !r.is_empty() => r.to_string(),
        _ => format!("<ph name=\"{}\"/>", escape_attr(name, b'"')),
    }
}

/// `original_raw`: an XTB splice keeps the original `<ph>` markup; an XMB to XTB conversion writes each as an empty
/// `<ph name=".."/>` (XTB carries no examples) and drops the unknown elements and comments.
pub fn render(codes: &BTreeMap<String, CodeData>, v: &[Inline], original_raw: bool) -> String {
    let mut out = String::new();
    let mut used: HashSet<&str> = HashSet::new();
    let mut stack: Vec<(&str, Option<String>)> = Vec::new();
    for i in v {
        match i {
            Inline::Text(t) => out.push_str(&escape_text(&xml_target_text(t))),
            Inline::Tag { id, kind: TagKind::Standalone, .. } => {
                let Some(c) = codes.get(id) else { continue };
                let Some(raw) = c.standalone.as_deref() else { continue };
                if c.open.is_some() || !used.insert(id) {
                    continue;
                }
                match &c.label {
                    Some(name) => out.push_str(&ph(name, Some(raw), original_raw)),
                    None => {
                        if original_raw {
                            out.push_str(raw);
                        }
                    }
                }
            }
            Inline::Tag { id, kind: TagKind::Open, .. } => match codes.get(id) {
                Some(c) if c.open.is_some() && used.insert(id) => {
                    let name = c.label.clone().unwrap_or_default();
                    out.push_str(&ph(&name, c.open.as_deref(), original_raw));
                    let close = ph(&c.close.as_deref().map(name_of_raw).unwrap_or_default(), c.close.as_deref(), original_raw);
                    stack.push((id, Some(close)));
                }
                _ => stack.push((id, None)),
            },
            Inline::Tag { id, kind: TagKind::Close, .. } => {
                if stack.last().is_some_and(|(k, _)| k == id) {
                    if let Some((_, Some(close))) = stack.pop() {
                        out.push_str(&close);
                    }
                }
            }
        }
    }
    for (_, close) in stack.into_iter().rev() {
        if let Some(c) = close {
            out.push_str(&c);
        }
    }
    out
}
