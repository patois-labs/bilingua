//! Writing units back as Markdown, and the HTML preview.

use crate::extract::{map_markers, options, FM_KEY, RAW_KEY};
use crate::frontmatter::{self, Style};
use bilingua_model::*;
use pulldown_cmark::{CowStr, Event, Parser, Tag};

/// Markdown for a list of inlines of `unit`, with alt and title markers filled from the other units.
pub fn inlines_md(doc: &Document, unit: &Unit, inl: &[Inline]) -> String {
    let mut out = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => out.push_str(t),
            Inline::Tag { id, kind, .. } => {
                let piece = unit.codes.get(id).and_then(|c| match kind {
                    TagKind::Open => c.open.as_deref(),
                    TagKind::Close => c.close.as_deref(),
                    TagKind::Standalone => c.standalone.as_deref(),
                });
                if let Some(p) = piece {
                    out.push_str(&map_markers(p, |kind, idx| Some(side_text(doc, kind, idx))));
                }
            }
        }
    }
    out
}

/// The current text of an alt or title unit, escaped for its place. Untouched text is written as it was.
fn side_text(doc: &Document, kind: char, idx: usize) -> String {
    let Some(u) = doc.units.get(idx) else { return String::new() };
    let touched = u.segments.iter().any(|s| s.has_target());
    let text = plain_text(&u.output());
    if !touched {
        return text;
    }
    let specials: &[char] = if kind == 't' { &['"', '\'', '(', ')'] } else { &['[', ']'] };
    let mut out = String::new();
    let mut prev_backslash = false;
    for c in text.chars() {
        if specials.contains(&c) && !prev_backslash {
            out.push('\\');
        }
        prev_backslash = c == '\\' && !prev_backslash;
        out.push(c);
    }
    out
}

/// The bytes a unit writes in the skeleton.
pub fn render_unit(doc: &Document, unit: &Unit) -> String {
    if let Some(style) = unit.codes.get(FM_KEY) {
        let touched = unit.segments.iter().any(|s| s.has_target());
        if !touched {
            if let Some(raw) = unit.codes.get(RAW_KEY).and_then(|c| c.standalone.clone()) {
                return raw;
            }
        }
        let text = plain_text(&unit.output());
        return frontmatter::write(&text, Style::from_name(style.label.as_deref().unwrap_or("plain")));
    }
    inlines_md(doc, unit, &unit.output())
}

// Markers used only while building the preview. They are punctuation-like (Ps/Pe) so that Markdown emphasis
// next to them behaves as it does next to a bracket.
const SEG_OPEN: char = '\u{27E6}';
const SEG_ID_END: char = '\u{27E7}';
const SEG_CLOSE: char = '\u{27E8}';

fn clean(s: &str) -> String {
    s.replace([SEG_OPEN, SEG_ID_END, SEG_CLOSE], "\u{fffd}")
}

pub fn preview(doc: &Document) -> Option<String> {
    let Skeleton::Text { parts, .. } = &doc.skeleton else { return None };
    let mut md = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => md.push_str(&clean(s)),
            SkelPart::Unit(i) => {
                let unit = doc.units.get(*i)?;
                if unit.codes.contains_key(FM_KEY) || !unit.translate {
                    md.push_str(&clean(&render_unit(doc, unit)));
                    continue;
                }
                for (n, seg) in unit.segments.iter().enumerate() {
                    if n > 0 {
                        md.push_str(&unit.separators.get(n - 1).cloned().unwrap_or_default());
                    }
                    let inl = if !seg.has_target() { &seg.source } else { &seg.target };
                    md.push(SEG_OPEN);
                    md.push_str(&seg.id);
                    md.push(SEG_ID_END);
                    md.push_str(&clean(&inlines_md(doc, unit, inl)));
                    md.push(SEG_CLOSE);
                }
            }
        }
    }
    let body = match frontmatter::parse(&md) {
        Some(fm) => &md[fm.end..],
        None => &md[..],
    };
    let html = to_html(body);
    Some(format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"></head><body dir=\"auto\">\n{}</body></html>\n",
        put_spans(&html)
    ))
}

pub fn render_segment(doc: &Document, seg_id: &str) -> Option<String> {
    let (unit, seg) = doc.find(seg_id)?;
    let inl = if !seg.has_target() { &seg.source } else { &seg.target };
    let md = clean(&inlines_md(doc, unit, inl));
    let html = to_html(&md);
    let t = html.trim_end();
    let inner = t.strip_prefix("<p>").and_then(|x| x.strip_suffix("</p>")).unwrap_or(t);
    Some(inner.to_string())
}

/// Replaces the segment markers in rendered HTML by `<span data-bl-seg>` elements.
fn put_spans(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(a) = rest.find([SEG_OPEN, SEG_CLOSE]) {
        out.push_str(&rest[..a]);
        let c = rest[a..].chars().next().unwrap_or(SEG_CLOSE);
        let after = &rest[a + c.len_utf8()..];
        if c == SEG_CLOSE {
            out.push_str("</span>");
            rest = after;
        } else if let Some(z) = after.find(SEG_ID_END) {
            out.push_str("<span data-bl-seg=\"");
            out.push_str(&escape_attr(&after[..z]));
            out.push_str("\">");
            rest = &after[z + SEG_ID_END.len_utf8()..];
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Markdown to HTML with no raw HTML and no dangerous URLs.
pub fn to_html(md: &str) -> String {
    let events = Parser::new_ext(md, options()).map(|e| match e {
        Event::Html(h) | Event::InlineHtml(h) => Event::Text(h),
        Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
            Event::Start(Tag::Link { link_type, dest_url: safe_url(dest_url), title, id })
        }
        Event::Start(Tag::Image { link_type, dest_url, title, id }) => {
            Event::Start(Tag::Image { link_type, dest_url: safe_url(dest_url), title, id })
        }
        other => other,
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    html
}

fn safe_url(u: CowStr<'_>) -> CowStr<'_> {
    let lower: String = u.chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect::<String>().to_ascii_lowercase();
    let scheme_end = lower.find([':', '/', '?', '#']);
    match scheme_end {
        Some(i) if lower.as_bytes()[i] == b':' => {
            let scheme = &lower[..i];
            if ["http", "https", "mailto", "tel"].contains(&scheme) {
                u
            } else {
                CowStr::Borrowed("#")
            }
        }
        _ => u,
    }
}
