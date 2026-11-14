//! One XHTML content document (a spine chapter or the navigation document), read through `bilingua-html`.
//!
//! The chapter text is decoded here (UTF-8, UTF-16 or its declared charset, see [`crate::text`]) and handed to the
//! HTML filter as UTF-8, so that filter's block and inline-tag rules are the ones every Bilingua HTML file gets.
//! On top of that this module applies what LocOS does for EPUB: only the accessibility attributes LocOS offers
//! (`alt`, `title`, `aria-label` on `img`/`area`, `title`, `aria-label` on `a`) come after the body units, XHTML
//! stays well-formed whatever a target contains, and the language of a translated book is declared on `<html>`.

use crate::lang;
use crate::text::{self, Text};
use bilingua_html::{HtmlFilter, VALUE};
use bilingua_model::*;
use std::collections::HashMap;

/// LocOS `_html()`: the HTML filter without page metadata units (chapters are joined into one document).
fn html() -> HtmlFilter {
    HtmlFilter::new().with_page_meta(false)
}

pub struct Chapter {
    pub path: String,
    pub text: Text,
    /// The HTML filter's document (units `u1`, `u2`, ... in document order).
    pub sub: Document,
    /// Epub unit order: indexes into `sub.units`. Body units first, then the LocOS accessibility attributes.
    pub order: Vec<usize>,
}

pub fn translated(u: &Unit) -> bool {
    u.translate && u.segments.iter().any(|s| s.has_target())
}

fn is_attr(u: &Unit) -> bool {
    u.hidden(VALUE) == Some("attr")
}

/// The LocOS style of an attribute unit (`a11y:img-alt` and the like): the HTML filter gives it.
fn a11y_style(u: &Unit) -> Option<&str> {
    u.context.style.as_deref().filter(|s| s.starts_with("a11y:"))
}

const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr", "command", "keygen"];

/// Entity declarations, the element count, and nesting depth (void elements and `/>` do not nest, so a sloppy but
/// otherwise readable chapter is not refused for its unclosed `<br>`).
fn check_xhtml(text: &str, l: &Limits) -> Result<(), Error> {
    bilingua_model::guard::check_markup(text, l)?;
    let b = text.as_bytes();
    let (mut i, mut depth) = (0usize, 0usize);
    while let Some(p) = text.get(i..).and_then(|t| t.find('<')) {
        i += p;
        let rest = &text[i..];
        if rest.starts_with("<!--") {
            i = text[i + 4..].find("-->").map_or(b.len(), |e| i + 4 + e + 3);
        } else if rest.starts_with("<![CDATA[") {
            i = text[i + 9..].find("]]>").map_or(b.len(), |e| i + 9 + e + 3);
        } else if rest.starts_with("<?") {
            i = text[i + 2..].find("?>").map_or(b.len(), |e| i + 2 + e + 2);
        } else if rest.starts_with("<!") {
            // DOCTYPE: to the closing '>' outside quotes and the internal subset
            let (mut j, mut q, mut sub) = (i + 2, 0u8, 0usize);
            while j < b.len() {
                let c = b[j];
                if q != 0 {
                    if c == q {
                        q = 0;
                    }
                } else if c == b'"' || c == b'\'' {
                    q = c;
                } else if c == b'[' {
                    sub += 1;
                } else if c == b']' {
                    sub = sub.saturating_sub(1);
                } else if c == b'>' && sub == 0 {
                    break;
                }
                j += 1;
            }
            i = j + 1;
        } else if rest.starts_with("</") {
            depth = depth.saturating_sub(1);
            i = text[i + 2..].find('>').map_or(b.len(), |e| i + 2 + e + 1);
        } else if rest.as_bytes().get(1).is_some_and(|c| c.is_ascii_alphabetic()) {
            let (mut j, mut q) = (i + 1, 0u8);
            while j < b.len() {
                let c = b[j];
                if q != 0 {
                    if c == q {
                        q = 0;
                    }
                } else if c == b'"' || c == b'\'' {
                    q = c;
                } else if c == b'>' {
                    break;
                }
                j += 1;
            }
            let name_end = text[i + 1..].find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/').map_or(b.len(), |e| i + 1 + e);
            let name = text[i + 1..name_end.min(j)].to_ascii_lowercase();
            let selfclose = j > 0 && j <= b.len() && b.get(j.wrapping_sub(1)) == Some(&b'/');
            if !selfclose && !VOID.contains(&name.as_str()) {
                depth += 1;
                if depth > l.max_xml_depth {
                    return Err(LimitError::new(
                        LimitKind::XmlDepth,
                        format!("This file nests XML elements more than {} deep. It was not processed.", l.max_xml_depth),
                    )
                    .into());
                }
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    Ok(())
}

impl Chapter {
    pub fn open(bytes: &[u8], path: &str, opts: &ExtractOptions) -> Result<Chapter, Error> {
        let limits = opts.limits();
        let text = text::read(bytes, path, true)?;
        check_xhtml(&text.text, &limits)?;
        let mut o = opts.clone();
        o.target_lang = None;
        o.granularity = Granularity::Paragraph;
        let sub = html().extract(text.text.as_bytes(), &o).map_err(|e| match e {
            Error::Parse { msg, .. } => Error::Parse { format: "epub".into(), msg: format!("{path}: {msg}") },
            e => e,
        })?;
        // body units, then the accessibility attributes (the filter's own order)
        let order = (0..sub.units.len()).filter(|&i| !is_attr(&sub.units[i]) || a11y_style(&sub.units[i]).is_some()).collect();
        Ok(Chapter { path: path.to_string(), text, sub, order })
    }

    pub fn prefix(c: usize) -> String {
        format!("h{c}-")
    }

    /// The chapter's units as epub units (ids `h<n>-u<k>`).
    pub fn units(&self, c: usize) -> Vec<Unit> {
        let prefix = Self::prefix(c);
        self.order
            .iter()
            .map(|&si| {
                let mut u = self.sub.units[si].clone();
                u.id = format!("{prefix}{}", u.id);
                for s in &mut u.segments {
                    s.id = format!("{prefix}{}", s.id);
                }
                let loc = u.context.location.take();
                u.context.location = Some(match loc {
                    Some(l) => format!("{} {l}", self.path),
                    None => self.path.clone(),
                });
                if let Some(st) = a11y_style(&u).map(str::to_string) {
                    u.context.extra.insert("locos-style".into(), st);
                }
                u
            })
            .collect()
    }

    /// The chapter's bytes with the translations of `doc_units` (by epub unit id) written in and, when `lang` is
    /// set, the language declared. `None` when there is nothing to change.
    pub fn render(&mut self, c: usize, by_id: &HashMap<&str, &Unit>, lang: Option<(&str, bool)>) -> Result<Option<Vec<u8>>, Error> {
        let prefix = Self::prefix(c);
        let mut picked: Vec<(usize, &Unit)> = Vec::new();
        for (k, &si) in self.order.iter().enumerate() {
            let _ = k;
            let id = format!("{prefix}{}", self.sub.units[si].id);
            if let Some(u) = by_id.get(id.as_str()) {
                if translated(u) && self.sub.units[si].translate {
                    picked.push((si, u));
                }
            }
        }
        if picked.is_empty() && lang.is_none() {
            return Ok(None);
        }
        for (si, u) in picked {
            let unit = &mut self.sub.units[si];
            // LocOS `_xhtml_safe`: XML 1.0 characters only; body text (entities as typed) also gets XML-safe
            // character references, attribute values are escaped in full by the HTML filter
            let body = !is_attr(unit);
            unit.segments = u
                .segments
                .iter()
                .map(|s| {
                    let mut c = s.clone();
                    c.id = s.id.strip_prefix(&prefix).unwrap_or(&s.id).to_string();
                    c.target = safe(&s.target, body);
                    c
                })
                .collect();
            unit.separators = u.separators.iter().map(|x| if body { bilingua_html::xml_named_refs(&text::xml_target_text(x)) } else { text::xml_target_text(x) }).collect();
        }
        let merged = html().merge(&self.sub)?;
        let mut merged = String::from_utf8(merged).map_err(|_| Error::Write(format!("{}: the merged chapter is not UTF-8", self.path)))?;
        if let Some((tag, rtl)) = lang {
            merged = stamp(&merged, tag, rtl);
        }
        Ok(Some(self.text.encode(&merged)))
    }
}

/// A target for an XHTML chapter (LocOS `_xhtml_safe`): XML 1.0 characters only, and in body text the HTML-only
/// named references as numeric ones.
fn safe(v: &[Inline], body: bool) -> Vec<Inline> {
    v.iter()
        .map(|i| match i {
            Inline::Text(t) if body => Inline::Text(bilingua_html::xml_named_refs(&text::xml_target_text(t))),
            Inline::Text(t) => Inline::Text(text::xml_target_text(t)),
            t => t.clone(),
        })
        .collect()
}

struct A {
    name: String,
    /// Span of the value including its quotes, if the attribute has one.
    value: Option<(usize, usize)>,
}

fn parse_attrs(tag: &str) -> (usize, Vec<A>) {
    let b = tag.as_bytes();
    let mut i = 1;
    while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
        i += 1;
    }
    let name_end = i;
    let mut out = Vec::new();
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] == b'>' || b[i] == b'/' {
            break;
        }
        let s = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        if i == s {
            i += 1;
            continue;
        }
        let name = tag[s..i].to_ascii_lowercase();
        let mut j = i;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < b.len() && b[j] == b'=' {
            j += 1;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let vs = j;
            if j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
                let q = b[j];
                j += 1;
                while j < b.len() && b[j] != q {
                    j += 1;
                }
                j = (j + 1).min(b.len());
            } else {
                while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' {
                    j += 1;
                }
            }
            out.push(A { name, value: Some((vs, j)) });
            i = j;
        } else {
            out.push(A { name, value: None });
        }
    }
    (name_end, out)
}

fn set_attr(tag: &str, name: &str, value: &str, add: bool) -> String {
    let (name_end, attrs) = parse_attrs(tag);
    match attrs.iter().find(|a| a.name == name && a.value.is_some()) {
        Some(a) => {
            let (s, e) = a.value.expect("checked");
            format!("{}\"{value}\"{}", &tag[..s], &tag[e..])
        }
        None if add => format!("{} {name}=\"{value}\"{}", &tag[..name_end], &tag[name_end..]),
        None => tag.to_string(),
    }
}

fn attr_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let (_, attrs) = parse_attrs(tag);
    let (s, e) = attrs.iter().find(|a| a.name == name)?.value?;
    Some(tag[s..e].trim_matches(['"', '\'']))
}

/// The start tags `<html ...>` and the head's `<meta http-equiv="content-language">` get the target language: `lang`
/// and `xml:lang` set (added when missing), `dir` set to `rtl` (added) or, for a left-to-right target, only changed
/// when present.
pub fn stamp(text: &str, tag: &str, rtl: bool) -> String {
    let b = text.as_bytes();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut i = 0;
    while let Some(p) = text.get(i..).and_then(|t| t.find('<')) {
        i += p;
        let rest = &text[i..];
        if rest.starts_with("<!--") {
            i = text[i + 4..].find("-->").map_or(b.len(), |e| i + 4 + e + 3);
            continue;
        }
        if rest.starts_with("<?") {
            i = text[i + 2..].find("?>").map_or(b.len(), |e| i + 2 + e + 2);
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            i = text[i + 9..].find("]]>").map_or(b.len(), |e| i + 9 + e + 3);
            continue;
        }
        if rest.starts_with("<!") {
            // DOCTYPE with an optional internal subset
            let (mut j, mut q, mut sub) = (i + 2, 0u8, 0usize);
            while j < b.len() {
                let c = b[j];
                if q != 0 {
                    if c == q {
                        q = 0;
                    }
                } else if c == b'"' || c == b'\'' {
                    q = c;
                } else if c == b'[' {
                    sub += 1;
                } else if c == b']' {
                    sub = sub.saturating_sub(1);
                } else if c == b'>' && sub == 0 {
                    break;
                }
                j += 1;
            }
            i = j + 1;
            continue;
        }
        if rest.starts_with("</") {
            i += 2;
            continue;
        }
        // a start tag: to '>' outside quotes
        let (mut j, mut q) = (i + 1, 0u8);
        while j < b.len() {
            let c = b[j];
            if q != 0 {
                if c == q {
                    q = 0;
                }
            } else if c == b'"' || c == b'\'' {
                q = c;
            } else if c == b'>' {
                break;
            }
            j += 1;
        }
        if j >= b.len() {
            break;
        }
        let end = j + 1;
        let t = &text[i..end];
        let name: String = t[1..].chars().take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/').collect::<String>().to_ascii_lowercase();
        if name == "html" {
            let mut n = set_attr(t, "lang", tag, true);
            n = set_attr(&n, "xml:lang", tag, true);
            n = if rtl { set_attr(&n, "dir", "rtl", true) } else { set_attr(&n, "dir", "ltr", false) };
            edits.push((i, end, n));
        } else if name == "meta" && attr_value(t, "http-equiv").is_some_and(|v| v.trim().eq_ignore_ascii_case("content-language")) {
            let n = set_attr(t, "content", tag, false);
            edits.push((i, end, n));
        } else if name == "body" {
            break;
        }
        i = end;
    }
    if edits.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 64);
    let mut last = 0;
    for (a, e, r) in edits {
        out.push_str(&text[last..a]);
        out.push_str(&r);
        last = e;
    }
    out.push_str(&text[last..]);
    out
}

/// `(tag, rtl)` for a target locale, or `None` when it is not a usable tag.
pub fn target(locale: Option<&str>) -> Option<(String, bool)> {
    let tag = lang::canonical_tag(locale?)?;
    let rtl = lang::is_rtl(&tag);
    Some((tag, rtl))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_html_and_meta() {
        let s = "<?xml version=\"1.0\"?>\n<!DOCTYPE html>\n<html xmlns=\"x\" lang='en' xml:lang=\"en\"><head><meta http-equiv=\"Content-Language\" content=\"en\"/></head><body><p>x</p></body></html>";
        let o = stamp(s, "ar", true);
        assert!(o.contains("lang=\"ar\" xml:lang=\"ar\" dir=\"rtl\"") || o.contains("dir=\"rtl\""), "{o}");
        assert!(o.contains("content=\"ar\""));
        assert!(!o.contains("lang='en'"));
        let o = stamp("<html><body/></html>", "fr-FR", false);
        assert_eq!(o, "<html xml:lang=\"fr-FR\" lang=\"fr-FR\"><body/></html>");
    }

    #[test]
    fn depth_ignores_void_elements() {
        let l = Limits::new().with_max_xml_depth(5);
        let t = format!("<html><body>{}</body></html>", "<br>".repeat(100));
        assert!(check_xhtml(&t, &l).is_ok());
        let deep = format!("{}{}", "<div>".repeat(6), "</div>".repeat(6));
        assert!(check_xhtml(&deep, &l).is_err());
    }
}
