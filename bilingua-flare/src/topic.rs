//! One topic or snippet (XHTML), read through `bilingua-html`.
//!
//! The MadCap phrasing elements are masked first (see [`crate::mask`]) so the HTML filter keeps them inside the
//! sentence as inline codes, as Localization OS does. On top of the HTML filter's units this module offers what
//! LocOS offers for a Flare topic: block text, then the accessibility attributes (`alt`, `title`, `aria-label` on
//! `img` and `area`; `title`, `aria-label` on `a`), then the index keywords of `MadCap:keyword term="..."`.
//! XHTML stays well-formed whatever a target contains, and the language of a translated project is declared on
//! `<html>`.

use crate::lang;
use crate::mask;
use crate::text::{self, Text};
use bilingua_html::Html;
use bilingua_model::*;
use std::collections::HashMap;

/// Reserved code ids of `bilingua-html` (see its `extract.rs`): the attribute marker.
const VALUE: &str = "~v";

pub const STYLE_KEYWORD: &str = "flare:index-keyword";

pub fn translated(u: &Unit) -> bool {
    u.translate && u.segments.iter().any(|s| !s.target.is_empty())
}

fn is_attr(u: &Unit) -> bool {
    u.codes.get(VALUE).and_then(|c| c.label.as_deref()) == Some("attr")
}

/// The LocOS style of an attribute unit, when LocOS offers that attribute (`a11y:img-alt` and the like).
fn a11y_style(u: &Unit) -> Option<&'static str> {
    let elem = u.context.style.as_deref()?;
    let key = u.context.key.as_deref()?.to_ascii_lowercase();
    // LocOS needs a letter in the value (the HTML crate wants a letter or a digit)
    if !bilingua_model::py_has_alpha(&plain_text(&u.segments.first()?.source)) {
        return None;
    }
    match (elem, key.as_str()) {
        ("img" | "area", "alt") => Some("a11y:img-alt"),
        ("img" | "area", "title") => Some("a11y:img-title"),
        ("img" | "area", "aria-label") => Some("a11y:img-aria-label"),
        ("a", "title") => Some("a11y:link-title"),
        ("a", "aria-label") => Some("a11y:link-aria-label"),
        _ => None,
    }
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

/// A private-use character that is in neither the chapter nor any target; `&` is swapped for it before the HTML
/// filter writes a target (which would keep an `&name;` that looks like an unknown entity as it is, invalid XML)
/// and swapped back for `&amp;` afterwards.
fn pick_sentinel(chapter: &str, picked: &[(usize, &Unit)]) -> char {
    let used = |c: char| {
        chapter.contains(c)
            || picked.iter().any(|(_, u)| {
                u.segments.iter().any(|s| s.source.iter().chain(&s.target).any(|i| matches!(i, Inline::Text(t) if t.contains(c))))
                    || u.separators.iter().any(|x| x.contains(c))
            })
    };
    ('\u{E000}'..='\u{F8FF}').find(|&c| !used(c)).unwrap_or('\u{F8FF}')
}

fn safe(v: &[Inline], sentinel: char) -> Vec<Inline> {
    v.iter()
        .map(|i| match i {
            Inline::Text(t) => Inline::Text(text::xml_target_text(t).replace('&', &sentinel.to_string())),
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
            n = set_attr(&n, "xml:lang", tag, false);
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


/// One index keyword (or sub-keyword level) of a `term` attribute.
#[derive(Debug, Clone)]
struct Kw {
    /// Index into the masked topic's keyword attributes.
    attr: usize,
    /// Which piece of the split value (even indexes of the split list).
    piece: usize,
    core: String,
}

/// Splits `s` at `;` and at `:` that is not preceded by a backslash, keeping the separators (so the result
/// alternates piece, separator, piece, ...).
fn kw_split(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev = ' ';
    for c in s.chars() {
        if c == ';' || (c == ':' && prev != '\\') {
            out.push(std::mem::take(&mut cur));
            out.push(c.to_string());
        } else {
            cur.push(c);
        }
        prev = c;
    }
    out.push(cur);
    out
}

pub struct Topic {
    pub path: String,
    pub text: Text,
    masked: mask::Masked,
    /// The HTML filter's document of the masked topic.
    sub: Document,
    /// Topic unit order: indexes into `sub.units`. Body units first, then the accessibility attributes.
    order: Vec<usize>,
    kws: Vec<Kw>,
}

/// Gives the codes that stand for MadCap elements their real name as label and tag text.
fn relabel(unit: &mut Unit, m: &mask::Masked) {
    let mut shown: HashMap<String, String> = HashMap::new();
    for (id, code) in unit.codes.iter_mut() {
        let found = [code.open.as_deref(), code.close.as_deref(), code.standalone.as_deref()]
            .into_iter()
            .flatten()
            .find_map(|p| mask::find_mask(p, &m.key));
        let Some((n, _)) = found else { continue };
        let Some(orig) = m.table.get(n) else { continue };
        let name: String = orig
            .trim_start_matches('<')
            .trim_start_matches('/')
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>')
            .collect();
        code.label = Some(name.rsplit(':').next().unwrap_or(&name).to_ascii_lowercase());
        shown.insert(id.clone(), name);
    }
    if shown.is_empty() {
        return;
    }
    for seg in &mut unit.segments {
        for side in [&mut seg.source, &mut seg.target] {
            for inl in side.iter_mut() {
                if let Inline::Tag { id, kind, display, .. } = inl {
                    if let Some(name) = shown.get(id.as_str()) {
                        *display = match kind {
                            TagKind::Open => format!("<{name}>"),
                            TagKind::Close => format!("</{name}>"),
                            TagKind::Standalone => format!("<{name}/>"),
                        };
                    }
                }
            }
        }
    }
}

impl Topic {
    pub fn open(bytes: &[u8], path: &str, opts: &ExtractOptions) -> Result<Topic, Error> {
        let text = text::read(bytes, path, true)?;
        Topic::from_text(text, path, opts)
    }

    pub fn from_text(text: Text, path: &str, opts: &ExtractOptions) -> Result<Topic, Error> {
        let limits = opts.limits();
        check_xhtml(&text.text, &limits)?;
        let masked = mask::mask(&text.text)?;
        let mut o = opts.clone();
        o.target_lang = None;
        o.granularity = Granularity::Paragraph;
        let mut sub = Html.extract(masked.text.as_bytes(), &o).map_err(|e| match e {
            Error::Parse { msg, .. } => Error::Parse { format: "flprjzip".into(), msg: format!("{path}: {msg}") },
            e => e,
        })?;
        let mut body = Vec::new();
        let mut attrs = Vec::new();
        for (i, u) in sub.units.iter_mut().enumerate() {
            if !is_attr(u) {
                // the text of a <title> that holds markup cannot be offered safely: it stays as it is
                if u.context.kind.as_deref() == Some("title") && plain_text(&u.source()).contains('<') {
                    u.translate = false;
                }
                relabel(u, &masked);
                body.push(i);
            } else if a11y_style(u).is_some() {
                attrs.push(i);
            }
        }
        body.extend(attrs);
        let mut kws = Vec::new();
        for (ai, k) in masked.keywords.iter().enumerate() {
            let raw = &masked.table[k.entry][k.vs..k.ve];
            let parts = kw_split(&text::unescape(raw));
            for (pi, p) in parts.iter().enumerate().step_by(2) {
                let core = bilingua_model::py_strip(p).replace("\\:", ":");
                if !core.is_empty() && py_has_alpha(&core) {
                    kws.push(Kw { attr: ai, piece: pi, core });
                }
            }
        }
        Ok(Topic { path: path.to_string(), text, masked, sub, order: body, kws })
    }

    pub fn prefix(idx: usize) -> String {
        format!("m{idx}-")
    }

    /// The topic's units (ids `m<member>-u<k>`, keywords `m<member>-k<n>`).
    pub fn units(&self, idx: usize) -> Vec<Unit> {
        let prefix = Self::prefix(idx);
        let mut out: Vec<Unit> = self
            .order
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
                if let Some(st) = a11y_style(&u) {
                    u.context.extra.insert("locos-style".into(), st.into());
                }
                u
            })
            .collect();
        for (j, k) in self.kws.iter().enumerate() {
            let mut u = Unit::new(format!("{prefix}k{}", j + 1), vec![Inline::text(k.core.clone())]);
            u.context.kind = Some("keyword".into());
            u.context.style = Some(STYLE_KEYWORD.into());
            u.context.location = Some(format!("{} index keyword {}", self.path, j + 1));
            out.push(u);
        }
        out
    }

    pub fn source_lang(&self) -> Option<String> {
        self.sub.source_lang.clone()
    }

    /// The topic's bytes with the translations of `by_id` (by Flare unit id) written in and, when `lang` is set,
    /// the language declared. `None` when there is nothing to change.
    pub fn render(&mut self, idx: usize, by_id: &HashMap<&str, &Unit>, lang: Option<(&str, bool)>) -> Result<Option<Vec<u8>>, Error> {
        let prefix = Self::prefix(idx);
        let mut picked: Vec<(usize, &Unit)> = Vec::new();
        for &si in &self.order {
            let id = format!("{prefix}{}", self.sub.units[si].id);
            if let Some(u) = by_id.get(id.as_str()) {
                if translated(u) && self.sub.units[si].translate && u.output() != self.sub.units[si].source() {
                    picked.push((si, u));
                }
            }
        }
        // keyword edits: attribute -> (piece -> new text)
        let mut kw_edits: HashMap<usize, HashMap<usize, String>> = HashMap::new();
        for (j, k) in self.kws.iter().enumerate() {
            let id = format!("{prefix}k{}", j + 1);
            let Some(u) = by_id.get(id.as_str()) else { continue };
            if !translated(u) {
                continue;
            }
            let new = plain_text(&u.output());
            if new == k.core {
                continue;
            }
            // ";" would start a new keyword and ":" a new level: the translation stays one entry
            let val = text::collapse_ws(&text::xml_target_text(&new)).replace(';', ",").replace(':', "\\:");
            kw_edits.entry(k.attr).or_default().insert(k.piece, val);
        }
        if picked.is_empty() && kw_edits.is_empty() && lang.is_none() {
            return Ok(None);
        }
        let mut table = self.masked.table.clone();
        for (ai, pieces) in &kw_edits {
            let a = &self.masked.keywords[*ai];
            let entry = table[a.entry].clone();
            let mut parts = kw_split(&text::unescape(&entry[a.vs..a.ve]));
            for (pi, val) in pieces {
                let (lead, _core, trail) = text::split_ws(&parts[*pi]);
                parts[*pi] = format!("{lead}{val}{trail}");
            }
            let val = text::xml_escape(&parts.concat(), true);
            // a second `term` attribute on the same tag would have a different span in the new text: the first
            // edited one wins and the others stay as they were
            if table[a.entry] == self.masked.table[a.entry] {
                table[a.entry] = format!("{}{}{}", &entry[..a.vs], val, &entry[a.ve..]);
            }
        }
        let mut rendered = if picked.is_empty() {
            self.masked.text.clone()
        } else {
            let sentinel = pick_sentinel(&self.masked.text, &picked);
            for (si, u) in picked {
                let unit = &mut self.sub.units[si];
                unit.segments = u
                    .segments
                    .iter()
                    .map(|s| Segment {
                        id: s.id.strip_prefix(&prefix).unwrap_or(&s.id).to_string(),
                        source: safe(&s.source, sentinel),
                        target: safe(&s.target, sentinel),
                        state: s.state,
                        origin: s.origin.clone(),
                    })
                    .collect();
                unit.separators = u.separators.iter().map(|x| x.replace('&', &sentinel.to_string())).collect();
            }
            let merged = Html.merge(&self.sub)?;
            let merged = String::from_utf8(merged).map_err(|_| Error::Write(format!("{}: the merged topic is not UTF-8", self.path)))?;
            merged.replace(sentinel, "&amp;")
        };
        rendered = mask::unmask(&rendered, &self.masked.key, &table);
        if let Some((tag, rtl)) = lang {
            rendered = stamp(&rendered, tag, rtl);
        }
        Ok(Some(self.text.encode(&rendered)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_split_keeps_separators_and_escaped_colons() {
        assert_eq!(kw_split("A;B:c\\:d"), ["A", ";", "B", ":", "c\\:d"]);
    }

    #[test]
    fn stamps_html_and_meta() {
        let s = "<?xml version=\"1.0\"?>\n<!DOCTYPE html>\n<html xmlns=\"x\" lang='en' xml:lang=\"en\"><head><meta http-equiv=\"Content-Language\" content=\"en\"/></head><body><p>x</p></body></html>";
        let o = stamp(s, "ar", true);
        assert!(o.contains("dir=\"rtl\"") && o.contains("xml:lang=\"ar\""), "{o}");
        assert!(o.contains("content=\"ar\""));
        let o = stamp("<html><body/></html>", "fr-FR", false);
        assert_eq!(o, "<html lang=\"fr-FR\"><body/></html>");
    }

    #[test]
    fn depth_ignores_void_elements() {
        let l = Limits { max_xml_depth: 5, ..Limits::default() };
        let t = format!("<html><body>{}</body></html>", "<br>".repeat(100));
        assert!(check_xhtml(&t, &l).is_ok());
        let deep = format!("{}{}", "<div>".repeat(6), "</div>".repeat(6));
        assert!(check_xhtml(&deep, &l).is_err());
    }
}
