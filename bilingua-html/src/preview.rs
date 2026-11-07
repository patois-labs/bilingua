//! The live-preview page: the translated document (target where present, else source) with every segment of a
//! body unit in `<span data-bl-seg="ID">`, and `data-bl-attr-seg` / `data-bl-attr` markers on the elements that
//! carry attribute units (or the text of a `<title>`, `<option>` or `<textarea>`, which cannot hold a span).
//! Built on the same token stream and inline maps as the merge, then sanitized.

use crate::page::{self, Cfg};
use crate::sanitize::sanitize_html;
use bilingua_htmlcore::html::{self, Tok};
use bilingua_htmlcore::pyx::*;
use bilingua_model::*;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};

/// Elements whose text cannot hold a span (raw text in a browser).
const NO_SPAN: [&str; 3] = ["title", "option", "textarea"];

fn seg_inlines(seg: &Segment) -> &[Inline] {
    if seg.has_target() {
        &seg.target
    } else {
        &seg.source
    }
}

fn ids_of(u: &Unit) -> String {
    u.segments.iter().map(|s| s.id.as_str()).collect::<Vec<_>>().join(",")
}

fn escaped(s: &str, quotes: bool) -> String {
    let mut o = String::new();
    escape_text_with(s, quotes, &mut o);
    o
}

/// Inserts the markers right after the tag name of a start-tag token.
fn mark(tok: &str, marks: &[(String, String)]) -> String {
    let b = tok.as_bytes();
    let mut e = 1;
    while e < b.len() && !matches!(b[e], b'\t' | b'\n' | 0x0c | b'\r' | b' ' | b'/' | b'>') {
        e += 1;
    }
    let ids: Vec<&str> = marks.iter().map(|m| m.0.as_str()).collect();
    let names: Vec<&str> = marks.iter().map(|m| m.1.as_str()).collect();
    format!("{} data-bl-attr-seg=\"{}\" data-bl-attr=\"{}\"{}", &tok[..e], html_escape(&ids.join(" ")), html_escape(&names.join(" ")), &tok[e..])
}

/// The preview page (`want` = None) or the inner HTML of one segment's span (`want` = its id).
pub(crate) fn render(doc: &Document, cfg: Cfg, want: Option<&str>) -> Option<String> {
    let text = page::skeleton_page(doc)?;
    let mut toks: Vec<Tok> = page::preview_tokens(&text, cfg)?;
    // the target language and direction are declared as the merge declares them: once something is translated,
    // a right-to-left target previews right to left
    let translated = doc.units.iter().any(|u| page::translated_value(u).is_some());
    if let Some(tag) = MergeOptions::default().stamp_lang(doc, translated).and_then(html::html_lang) {
        html::declare_language(&mut toks, &tag, html::text_direction(&tag));
    }
    let quotes = page::quotes_for(&text, cfg);
    let attr_tis: Vec<usize> = html::iter_page_attr_units_with(&toks, cfg.page_meta).into_iter().map(|a| a.ti).collect();
    let n_body = page::apply_attrs(&mut toks, doc, cfg.page_meta, page::translated_value).ok()?;
    let wanted = want.and_then(|id| doc.find(id)).map(|(u, s)| (doc.units.iter().position(|x| std::ptr::eq(x, u)), s));
    if let Some((Some(k), _)) = wanted {
        if k >= n_body {
            return Some(html_escape(&plain_text(&doc.units[k].output())));
        }
    } else if want.is_some() {
        return None;
    }
    // markers of the attribute units, on their start tags (inline tags included, so the spans carry them)
    let mut marks: BTreeMap<usize, Vec<(String, String)>> = BTreeMap::new();
    for (j, ti) in attr_tis.iter().enumerate() {
        let u = &doc.units[n_body + j];
        marks.entry(*ti).or_default().push((ids_of(u), u.context.key.clone().unwrap_or_default()));
    }
    for (ti, m) in &marks {
        toks[*ti].s = Cow::Owned(mark(&toks[*ti].s, m));
    }
    let body = html::iter_units(&toks, None).ok()?;
    let starts: HashSet<usize> = body.iter().map(|u| u.start).collect();
    let places = page::places(&toks, &starts);
    let mut text_marks: BTreeMap<usize, Vec<(String, String)>> = BTreeMap::new();
    let mut repl: Vec<(usize, usize, String)> = Vec::new();
    for (k, bu) in body.iter().enumerate() {
        let unit = doc.units.get(k)?;
        let place = places.get(&bu.start);
        let no_span = place.is_some_and(|p| NO_SPAN.contains(&p.elem.as_str()));
        let protected = page::protected_for(cfg, bu);
        if let Some((Some(wk), seg)) = wanted {
            if wk == k {
                let inner = page::materialize(seg_inlines(seg), &bu.map, quotes, &protected);
                return Some(if no_span { escaped(&plain_text(&unit.output()), quotes) } else { sanitize_html(&inner, false) });
            }
            continue;
        }
        let content = if no_span {
            if let Some(ti) = place.and_then(|p| p.elem_ti) {
                text_marks.entry(ti).or_default().push((ids_of(unit), "#text".into()));
            }
            page::materialize(&unit.output(), &bu.map, quotes, &protected)
        } else {
            let mut c = String::new();
            for (i, seg) in unit.segments.iter().enumerate() {
                if i > 0 {
                    if let Some(sep) = unit.separators.get(i - 1) {
                        c.push_str(&escaped(sep, quotes));
                    }
                }
                c.push_str(&format!("<span data-bl-seg=\"{}\">", html_escape(&seg.id)));
                c.push_str(&page::materialize(seg_inlines(seg), &bu.map, quotes, &protected));
                c.push_str("</span>");
            }
            c
        };
        repl.push((bu.start, bu.end, format!("{}{}{}", bu.lead, content, bu.trail)));
    }
    if want.is_some() {
        return None;
    }
    for (ti, m) in &text_marks {
        toks[*ti].s = Cow::Owned(mark(&toks[*ti].s, m));
    }
    for (s, e, v) in repl {
        toks[s].s = Cow::Owned(v);
        for t in &mut toks[s + 1..=e] {
            t.s = Cow::Borrowed("");
        }
    }
    let out: String = toks.iter().map(|t| t.s.as_ref()).collect();
    Some(sanitize_html(&out, true))
}
