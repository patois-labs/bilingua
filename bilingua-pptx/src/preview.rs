//! A simple HTML preview: one section per slide (title, bullets, tables), notes after, layouts and masters at
//! the end. Every segment sits in `<span data-bl-seg="ID">`. No scripts, not pixel perfect.

use crate::render::is_rtl_lang;
use bilingua_model::*;

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => {}
            c => o.push(c),
        }
    }
    o
}

/// Inner HTML of one segment: its target, or its source when there is none.
fn seg_inner(u: &Unit, seg: &Segment) -> String {
    let inl = if seg.target.is_empty() { &seg.source } else { &seg.target };
    let mut s = String::new();
    let mut stack: Vec<&'static str> = vec![];
    for i in inl {
        match i {
            Inline::Text(t) => s.push_str(&esc(t).replace('\n', "<br>")),
            Inline::Tag { id, kind, display } => {
                let label = u.codes.get(id).and_then(|c| c.label.as_deref()).unwrap_or("");
                match kind {
                    TagKind::Open => {
                        let tag = if label.contains("bold") && !label.contains(' ') {
                            "b"
                        } else if label == "italic" {
                            "i"
                        } else if label == "underline" {
                            "u"
                        } else if label == "superscript" {
                            "sup"
                        } else if label == "subscript" {
                            "sub"
                        } else {
                            "span"
                        };
                        stack.push(tag);
                        s.push_str(&format!("<{tag}>"));
                    }
                    TagKind::Close => {
                        if let Some(t) = stack.pop() {
                            s.push_str(&format!("</{t}>"));
                        }
                    }
                    TagKind::Standalone => {
                        if label == "line break" {
                            s.push_str("<br>");
                        } else {
                            s.push_str(&format!("<span class=\"bl-code\">{}</span>", esc(display)));
                        }
                    }
                }
            }
        }
    }
    while let Some(t) = stack.pop() {
        s.push_str(&format!("</{t}>"));
    }
    s
}

fn unit_html(u: &Unit) -> String {
    let mut s = String::new();
    for (k, seg) in u.segments.iter().enumerate() {
        if k > 0 {
            if let Some(sep) = u.separators.get(k - 1) {
                s.push_str(&esc(sep));
            }
        }
        s.push_str(&format!("<span data-bl-seg=\"{}\">{}</span>", esc(&seg.id), seg_inner(u, seg)));
    }
    s
}

pub fn render_segment(doc: &Document, seg_id: &str) -> Option<String> {
    let (u, seg) = doc.find(seg_id)?;
    Some(seg_inner(u, seg))
}

fn extra<'a>(u: &'a Unit, k: &str) -> &'a str {
    u.context.extra.get(k).map(String::as_str).unwrap_or("")
}

fn slide_html(units: &[&Unit]) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        let kind = u.context.kind.as_deref().unwrap_or("");
        let part = extra(u, "part");
        match (part, kind) {
            ("notes", _) => {
                s.push_str("<aside class=\"bl-notes\">");
                while i < units.len() && extra(units[i], "part") == "notes" {
                    s.push_str(&format!("<p>{}</p>", unit_html(units[i])));
                    i += 1;
                }
                s.push_str("</aside>");
                continue;
            }
            (_, "title") => s.push_str(&format!("<h2>{}</h2>", unit_html(u))),
            (_, "subtitle") => s.push_str(&format!("<h3>{}</h3>", unit_html(u))),
            (_, "cell") => {
                let table = extra(u, "table").to_string();
                s.push_str("<table class=\"bl-table\">");
                let mut row = 0usize;
                while i < units.len() && extra(units[i], "part") == part && units[i].context.kind.as_deref() == Some("cell") && extra(units[i], "table") == table {
                    let r: usize = extra(units[i], "row").parse().unwrap_or(0);
                    if r != row {
                        if row != 0 {
                            s.push_str("</tr>");
                        }
                        s.push_str("<tr>");
                        row = r;
                    }
                    s.push_str(&format!("<td>{}</td>", unit_html(units[i])));
                    i += 1;
                }
                s.push_str("</tr></table>");
                continue;
            }
            (_, "body") | ("diagram", _) => {
                s.push_str("<ul>");
                while i < units.len() && (units[i].context.kind.as_deref() == Some("body") || extra(units[i], "part") == "diagram") && extra(units[i], "part") != "notes" {
                    s.push_str(&format!("<li>{}</li>", unit_html(units[i])));
                    i += 1;
                }
                s.push_str("</ul>");
                continue;
            }
            _ => s.push_str(&format!("<p>{}</p>", unit_html(u))),
        }
        i += 1;
    }
    s
}

pub fn preview(doc: &Document) -> Option<String> {
    let rtl = doc.target_lang.as_deref().is_some_and(is_rtl_lang);
    let mut body = String::new();
    // Slides in order of their number.
    let mut slides: Vec<(u32, Vec<&Unit>)> = vec![];
    let mut other: Vec<&Unit> = vec![];
    for u in &doc.units {
        match extra(u, "slide").parse::<u32>() {
            Ok(n) => match slides.iter_mut().find(|(k, _)| *k == n) {
                Some((_, v)) => v.push(u),
                None => slides.push((n, vec![u])),
            },
            Err(_) => other.push(u),
        }
    }
    slides.sort_by_key(|(n, _)| *n);
    for (n, units) in &slides {
        body.push_str(&format!("<section class=\"bl-slide\" data-slide=\"{n}\"><h1>Slide {n}</h1>{}</section>", slide_html(units)));
    }
    if !other.is_empty() {
        body.push_str("<section class=\"bl-layouts\"><h1>Layouts and masters</h1>");
        for u in other {
            body.push_str(&format!(
                "<p><small>{}</small><br>{}</p>",
                esc(u.context.location.as_deref().unwrap_or("")),
                unit_html(u)
            ));
        }
        body.push_str("</section>");
    }
    let dir = if rtl { " dir=\"rtl\"" } else { "" };
    Some(format!(
        "<!doctype html><html{dir}><head><meta charset=\"utf-8\"><title>Preview</title><style>body{{font-family:sans-serif;max-width:60rem;margin:1rem auto}}section.bl-slide{{border:1px solid #ccc;padding:1rem;margin:1rem 0;aspect-ratio:auto}}h1{{font-size:.9rem;color:#666}}.bl-notes{{background:#f6f6f0;padding:.5rem;font-size:.9rem}}.bl-code{{background:#eee;font-family:monospace;font-size:.8em}}table{{border-collapse:collapse}}td{{border:1px solid #ccc;padding:.2rem .5rem}}</style></head><body>{body}</body></html>"
    ))
}
