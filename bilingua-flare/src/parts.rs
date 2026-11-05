//! The structured XML parts of a project: variable sets (`.flvar`), TOCs (`.fltoc`) and glossaries (`.flglo`).
//!
//! Units are found by the tag scanner; a translation is written over the exact span of the old text, so every
//! other byte of the part (attributes, white space, comments, the XML declaration) stays as it was.

use crate::scan::{next_tag, Tag};
use crate::text::{split_ws, unescape, xml_escape, xml_target_text};
use bilingua_model::{guard, py_has_alpha, Error, Inline, Limits, TagKind};

pub const STYLE_VARIABLE: &str = "flare:variable";
pub const STYLE_TOC: &str = "flare:toc-title";
pub const STYLE_TERM: &str = "flare:glossary-term";
pub const STYLE_DEFINITION: &str = "flare:glossary-definition";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    Variables,
    Toc,
    Glossary,
}

/// One translatable piece of a part.
#[derive(Debug, Clone)]
pub struct Item {
    pub style: &'static str,
    /// The unit's source.
    pub inl: Vec<Inline>,
    /// Original text of each placeholder (`[%=...%]`) in `inl`, by tag id.
    pub tokens: Vec<(String, String)>,
    pub key: Option<String>,
    /// The span rewritten on a change: element content, or an attribute value.
    pub span: (usize, usize),
    pub lead: String,
    pub trail: String,
    /// `EvaluatedDefinition` value span, when it holds the same text (variable sets).
    pub evaluated: Option<(usize, usize)>,
}

fn malformed(name: &str) -> Error {
    Error::Parse { format: "flprjzip".into(), msg: format!("file {name} is not well-formed XML. It was not processed.") }
}

fn dtd(name: &str) -> Error {
    guard::LimitError::new(
        guard::LimitKind::XmlEntities,
        format!("This Flare project's file {name} declares a DTD or entities. It was not processed."),
    )
    .into()
}

/// A structured part must be well-formed XML without a DTD (no entity can be defined, none is resolved).
pub fn check(text: &str, name: &str, l: &Limits) -> Result<(), Error> {
    use quick_xml::events::Event;
    guard::check_xml(text.as_bytes(), l)?;
    let mut r = quick_xml::Reader::from_str(text);
    let (mut depth, mut roots) = (0usize, 0usize);
    let attrs_ok = |e: &quick_xml::events::BytesStart| -> bool {
        e.attributes().all(|a| match a {
            Ok(a) => a.unescape_value().is_ok(),
            Err(_) => false,
        })
    };
    loop {
        match r.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::DocType(_)) => return Err(dtd(name)),
            Ok(Event::Start(e)) => {
                if depth == 0 {
                    roots += 1;
                }
                depth += 1;
                if !attrs_ok(&e) {
                    return Err(malformed(name));
                }
            }
            Ok(Event::Empty(e)) => {
                if depth == 0 {
                    roots += 1;
                }
                if !attrs_ok(&e) {
                    return Err(malformed(name));
                }
            }
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Text(t)) => {
                let s = t.decode().map_err(|_| malformed(name))?;
                if depth == 0 && !s.trim().is_empty() {
                    return Err(malformed(name));
                }
                quick_xml::escape::unescape(&s).map_err(|_| malformed(name))?;
            }
            Ok(Event::GeneralRef(r)) => {
                let n = r.decode().map_err(|_| malformed(name))?;
                let ok = match n.strip_prefix('#') {
                    Some(num) => {
                        let v = match num.strip_prefix(['x', 'X']) {
                            Some(h) => u32::from_str_radix(h, 16).ok(),
                            None => num.parse::<u32>().ok(),
                        };
                        v.and_then(char::from_u32).is_some_and(|c| matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}'))
                    }
                    None => matches!(n.as_ref(), "amp" | "lt" | "gt" | "quot" | "apos"),
                };
                if !ok || depth == 0 {
                    return Err(malformed(name));
                }
            }
            Ok(Event::CData(_)) if depth == 0 => return Err(malformed(name)),
            Ok(_) => {}
            Err(_) => return Err(malformed(name)),
        }
        if roots > 1 {
            return Err(malformed(name));
        }
    }
    if depth != 0 || roots != 1 {
        return Err(malformed(name));
    }
    Ok(())
}

fn attr<'a>(text: &'a str, t: &Tag, name: &str) -> Option<(usize, usize, &'a str)> {
    t.attrs
        .iter()
        .find(|a| a.name == name && a.quoted)
        .and_then(|a| a.value)
        .map(|(s, e)| (s, e, &text[s..e]))
}

/// Elements in `names` whose content is a single run of text: (open tag, content span, name).
fn text_elements<'a>(text: &str, names: &[&'a str]) -> Vec<(Tag, (usize, usize), &'a str)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(open) = next_tag(text, i) {
        i = open.resume;
        if open.closing || open.self_closing {
            continue;
        }
        let Some(&nm) = names.iter().find(|n| **n == open.name) else { continue };
        let Some(close) = next_tag(text, open.end) else { continue };
        // content must be text only: the very next tag is this element's end tag
        if !close.closing || close.name != open.name || text[open.end..close.start].contains('<') {
            continue;
        }
        let span = (open.end, close.start);
        out.push((open, span, nm));
    }
    out
}

/// `[%=...%]` ranges in `s` (byte offsets): `[%=`, up to 200 characters that are neither `%` nor `]`, then `%]`.
pub fn flare_tokens(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while let Some(p) = s[i..].find("[%=") {
        let start = i + p;
        let body = start + 3;
        let run = s[body..].find(['%', ']']).unwrap_or(s.len() - body);
        let end_run = body + run;
        if s[body..end_run].chars().count() <= 200 && b.get(end_run) == Some(&b'%') && b.get(end_run + 1) == Some(&b']') {
            out.push((start, end_run + 2));
            i = end_run + 2;
        } else {
            i = start + 1;
        }
    }
    out
}

pub fn items(kind: PartKind, text: &str) -> Vec<Item> {
    match kind {
        PartKind::Variables => variable_items(text),
        PartKind::Glossary => glossary_items(text),
        PartKind::Toc => toc_items(text),
    }
}

fn plain_item(text: &str, style: &'static str, span: (usize, usize), key: Option<String>, evaluated: Option<(usize, usize)>) -> Option<Item> {
    let body = &text[span.0..span.1];
    let (lead, core, trail) = split_ws(body);
    let value = unescape(core);
    if core.is_empty() || !py_has_alpha(&value) {
        return None;
    }
    Some(Item {
        style,
        inl: vec![Inline::text(value)],
        tokens: vec![],
        key,
        span,
        lead: lead.to_string(),
        trail: trail.to_string(),
        evaluated,
    })
}

fn variable_items(text: &str) -> Vec<Item> {
    let mut out = Vec::new();
    for (open, span, _) in text_elements(text, &["variable"]) {
        let vtype = attr(text, &open, "type").map(|a| unescape(a.2).trim().to_ascii_lowercase()).unwrap_or_default();
        if !(vtype.is_empty() || vtype == "text") {
            continue;
        }
        let name = attr(text, &open, "name").map(|a| unescape(a.2));
        let Some(mut it) = plain_item(text, STYLE_VARIABLE, span, name, None) else { continue };
        if let Some((s, e, raw)) = attr(text, &open, "evaluateddefinition") {
            if let Some(Inline::Text(v)) = it.inl.first() {
                if bilingua_model::py_strip(&unescape(raw)) == v {
                    it.evaluated = Some((s, e));
                }
            }
        }
        out.push(it);
    }
    out
}

fn glossary_items(text: &str) -> Vec<Item> {
    text_elements(text, &["term", "definition"])
        .into_iter()
        .filter_map(|(_, span, nm)| plain_item(text, if nm == "term" { STYLE_TERM } else { STYLE_DEFINITION }, span, None, None))
        .collect()
}

fn toc_items(text: &str) -> Vec<Item> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(t) = next_tag(text, i) {
        i = t.resume;
        if t.closing || t.name != "tocentry" {
            continue;
        }
        for a in &t.attrs {
            let (true, true, Some((s, e))) = (a.name == "title", a.quoted, a.value) else { continue };
            let value = unescape(&text[s..e]);
            let (lead, core, trail) = split_ws(&value);
            // the core as text and placeholders
            let mut inl = Vec::new();
            let mut tokens = Vec::new();
            let mut pos = 0;
            for (ts, te) in flare_tokens(core) {
                if ts > pos {
                    inl.push(Inline::text(&core[pos..ts]));
                }
                let id = (tokens.len() + 1).to_string();
                inl.push(Inline::tag(id.clone(), TagKind::Standalone, &core[ts..te]));
                tokens.push((id, core[ts..te].to_string()));
                pos = te;
            }
            if pos < core.len() {
                inl.push(Inline::text(&core[pos..]));
            }
            let has_letter = inl.iter().any(|x| matches!(x, Inline::Text(t) if py_has_alpha(t)));
            if core.is_empty() || !has_letter {
                continue;
            }
            out.push(Item {
                style: STYLE_TOC,
                inl,
                tokens,
                key: None,
                span: (s, e),
                lead: lead.to_string(),
                trail: trail.to_string(),
                evaluated: None,
            });
        }
    }
    out
}

/// The replacement text of an item for a target (`None` when the target changes nothing).
fn new_value(kind: PartKind, it: &Item, out: &[Inline]) -> Option<String> {
    if out == it.inl.as_slice() {
        return None;
    }
    if kind == PartKind::Toc {
        let mut s = String::new();
        for x in out {
            match x {
                Inline::Text(t) => s.push_str(&xml_escape(&xml_target_text(t), true)),
                Inline::Tag { id, .. } => {
                    if let Some((_, orig)) = it.tokens.iter().find(|(i, _)| i == id) {
                        s.push_str(&xml_escape(orig, true));
                    }
                }
            }
        }
        return Some(format!("{}{}{}", it.lead, s, it.trail));
    }
    let clean = xml_target_text(&bilingua_model::plain_text(out));
    Some(format!("{}{}{}", it.lead, xml_escape(&clean, false), it.trail))
}

/// `text` with the targets in `targets` (item number to its output) written in.
pub fn render(kind: PartKind, text: &str, its: &[Item], targets: &[(usize, Vec<Inline>)]) -> String {
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for (j, out) in targets {
        let Some(it) = its.get(*j) else { continue };
        let Some(val) = new_value(kind, it, out) else { continue };
        if let Some((s, e)) = it.evaluated {
            let clean = xml_target_text(&bilingua_model::plain_text(out));
            edits.push((s, e, xml_escape(&clean, true)));
        }
        edits.push((it.span.0, it.span.1, val));
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut s = text.to_string();
    for (a, b, r) in edits {
        s.replace_range(a..b, &r);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        assert_eq!(flare_tokens("a [%=System.LinkedTitle%] b [%=x%]"), vec![(2, 25), (28, 34)]);
        assert!(flare_tokens("[%=a]b%]").is_empty());
    }

    #[test]
    fn variables_skip_typed_ones() {
        let t = "<S><Variable Name=\"A\" EvaluatedDefinition=\"Hi\">Hi</Variable><Variable Name=\"D\" Type=\"DateTime\">Today</Variable><Variable Name=\"N\">123</Variable></S>";
        let it = items(PartKind::Variables, t);
        assert_eq!(it.len(), 1);
        assert!(it[0].evaluated.is_some());
        let r = render(PartKind::Variables, t, &it, &[(0, vec![Inline::text("Yo & co")])]);
        assert!(r.contains("EvaluatedDefinition=\"Yo &amp; co\">Yo &amp; co</Variable>"), "{r}");
    }

    #[test]
    fn well_formedness() {
        let l = Limits::default();
        assert!(check("<a><b/></a>", "x", &l).is_ok());
        assert!(check("<a><b></a>", "x", &l).is_err());
        assert!(check("<a>&nbsp;</a>", "x", &l).is_err());
        assert!(check("<!DOCTYPE a []><a/>", "x", &l).is_err());
        assert!(check("<a/><b/>", "x", &l).is_err());
        assert!(check("", "x", &l).is_err());
    }
}
