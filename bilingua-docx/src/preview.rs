//! Approximate HTML preview. Headings, paragraphs, lists, tables, bold/italic/underline and image
//! placeholders; every segment sits in `<span data-bl-seg="ID">`. No scripts, no styles from the file, no page
//! layout. Parts other than the main document (headers, footers, notes, comments, text boxes, properties) are
//! listed in sections after the body.

use crate::extract::*;
use crate::xml::*;
use bilingua_model::*;
use std::collections::HashSet;

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
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

fn fmt_tags(label: &str) -> Vec<&'static str> {
    let mut v = Vec::new();
    if label.contains("bold") {
        v.push("b");
    }
    if label.contains("italic") {
        v.push("i");
    }
    if label.contains("underline") {
        v.push("u");
    }
    if label.contains("superscript") {
        v.push("sup");
    }
    if label.contains("subscript") {
        v.push("sub");
    }
    if label.contains("strike") {
        v.push("s");
    }
    v
}

/// The HTML for a run of inlines of one unit, with tags balanced.
pub fn inline_html(unit: &Unit, inlines: &[Inline]) -> String {
    let mut out = String::new();
    let mut stack: Vec<(String, String)> = Vec::new(); // id, closing html
    for i in inlines {
        match i {
            Inline::Text(t) => out.push_str(&esc(t)),
            Inline::Tag { id, kind, display, .. } => {
                let code = unit.codes.get(id);
                let label = code.and_then(|c| c.label.clone()).unwrap_or_default();
                match kind {
                    TagKind::Open => {
                        let wrapper = code.is_some_and(|c| c.standalone.is_some());
                        let (o, c) = if wrapper {
                            (
                                "<span class=\"bl-link\">".to_string(),
                                "</span>".to_string(),
                            )
                        } else {
                            let tags = fmt_tags(&label);
                            let o: String = tags.iter().map(|t| format!("<{t}>")).collect();
                            let c: String = tags.iter().rev().map(|t| format!("</{t}>")).collect();
                            (o, c)
                        };
                        out.push_str(&o);
                        stack.push((id.clone(), c));
                    }
                    TagKind::Close => {
                        if let Some(pos) = stack.iter().rposition(|(sid, _)| sid == id) {
                            while stack.len() > pos {
                                let (_, c) = stack.pop().expect("length checked");
                                out.push_str(&c);
                            }
                        }
                    }
                    TagKind::Standalone => match label.as_str() {
                        "tab" => out.push_str("&emsp;"),
                        "break" => out.push_str("<br>"),
                        "image" | "textbox" => {
                            out.push_str("<span class=\"bl-img\">[image]</span>")
                        }
                        "field" => {
                            let r = code.and_then(|c| c.open.clone()).unwrap_or_default();
                            out.push_str(&esc(&r));
                        }
                        "footnote-ref" => out.push_str(&format!("<sup>{}</sup>", esc(display))),
                        _ => out
                            .push_str(&format!("<span class=\"bl-code\">{}</span>", esc(display))),
                    },
                }
            }
        }
    }
    while let Some((_, c)) = stack.pop() {
        out.push_str(&c);
    }
    out
}

fn seg_inlines(s: &Segment) -> &Vec<Inline> {
    if s.target.is_empty() {
        &s.source
    } else {
        &s.target
    }
}

pub fn render_segment(doc: &Document, seg_id: &str) -> Option<String> {
    let (u, s) = doc.find(seg_id)?;
    Some(inline_html(u, seg_inlines(s)))
}

fn unit_html(u: &Unit) -> String {
    let mut out = String::new();
    for (i, s) in u.segments.iter().enumerate() {
        if i > 0 {
            if let Some(sep) = u.separators.get(i - 1) {
                out.push_str(&esc(sep));
            }
        }
        out.push_str(&format!(
            "<span data-bl-seg=\"{}\">{}</span>",
            esc(&s.id),
            inline_html(u, seg_inlines(s))
        ));
    }
    out
}

struct Pv<'a> {
    doc: &'a Document,
    ns: Ns,
    shown: HashSet<usize>,
}

impl Pv<'_> {
    fn marker_unit(&self, e: &El) -> Option<usize> {
        for c in &e.children {
            if let Node::Text(t) = c {
                if let Some(p) = t.find(M_UNIT) {
                    let rest = &t[p + M_UNIT.len_utf8()..];
                    let end = rest.find(M_END)?;
                    return rest[..end].parse().ok();
                }
            }
        }
        None
    }

    fn para(&mut self, e: &El) -> Option<(String, bool)> {
        let idx = self.marker_unit(e)?;
        let u = self.doc.units.get(idx)?;
        self.shown.insert(idx);
        let ppr = e.elements().find(|c| self.ns.is_w(&c.name, "pPr"));
        let rtl = ppr
            .and_then(|p| p.elements().find(|c| self.ns.is_w(&c.name, "bidi")))
            .is_some_and(|b| {
                !matches!(self.ns.attr(b, "val").as_deref(), Some("0") | Some("false"))
            });
        let dir = if rtl { " dir=\"rtl\"" } else { "" };
        let kind = u.context.kind.as_deref().unwrap_or("paragraph");
        let inner = unit_html(u);
        Some(match kind {
            "heading" => {
                let lvl = u
                    .context
                    .style
                    .as_deref()
                    .map(|s| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>())
                    .and_then(|d| d.parse::<u8>().ok())
                    .unwrap_or(1)
                    .clamp(1, 6);
                (format!("<h{lvl}{dir}>{inner}</h{lvl}>"), false)
            }
            "list-item" => (format!("<li{dir}>{inner}</li>"), true),
            _ => (format!("<p{dir}>{inner}</p>"), false),
        })
    }

    fn block(&mut self, nodes: &[Node], out: &mut String) {
        let mut in_list = false;
        for n in nodes {
            let Node::El(e) = n else { continue };
            if self.ns.is_w(&e.name, "p") {
                match self.para(e) {
                    Some((html, true)) => {
                        if !in_list {
                            out.push_str("<ul>");
                            in_list = true;
                        }
                        out.push_str(&html);
                    }
                    Some((html, false)) => {
                        if in_list {
                            out.push_str("</ul>");
                            in_list = false;
                        }
                        out.push_str(&html);
                    }
                    None => {}
                }
                continue;
            }
            if in_list {
                out.push_str("</ul>");
                in_list = false;
            }
            if self.ns.is_w(&e.name, "tbl") {
                out.push_str("<table>");
                self.block(&e.children, out);
                out.push_str("</table>");
            } else if self.ns.is_w(&e.name, "tr") {
                out.push_str("<tr>");
                self.block(&e.children, out);
                out.push_str("</tr>");
            } else if self.ns.is_w(&e.name, "tc") {
                out.push_str("<td>");
                self.block(&e.children, out);
                out.push_str("</td>");
            } else if self.ns.is_w(&e.name, "pPr")
                || self.ns.is_w(&e.name, "tblPr")
                || self.ns.is_w(&e.name, "trPr")
                || self.ns.is_w(&e.name, "tcPr")
            {
            } else {
                self.block(&e.children, out);
            }
        }
        if in_list {
            out.push_str("</ul>");
        }
    }
}

pub fn preview(doc: &Document, main: &str, w: &str) -> Option<String> {
    let Skeleton::Zip { entries, .. } = &doc.skeleton else {
        return None;
    };
    let entry = entries.iter().find(|e| e.name == main)?;
    let ZipData::Parts(parts) = &entry.data else {
        return None;
    };
    let mut tpl = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => tpl.push_str(s),
            SkelPart::Unit(i) => tpl.push_str(&format!("{M_UNIT}{i}{M_END}")),
        }
    }
    let nodes = parse(&tpl).ok()?;
    let mut pv = Pv {
        doc,
        ns: Ns {
            w: w.to_string(),
            mc: "mc:".into(),
        },
        shown: HashSet::new(),
    };
    let mut body = String::new();
    pv.block(&nodes, &mut body);

    let sections = [
        ("header", "Header"),
        ("footer", "Footer"),
        ("footnote", "Footnotes"),
        ("endnote", "Endnotes"),
        ("comment", "Comments"),
        ("textbox", "Text boxes"),
        ("title", "Document title"),
        ("subject", "Document subject"),
    ];
    let mut extra = String::new();
    for (kind, title) in sections {
        let mut items = String::new();
        for (i, u) in doc.units.iter().enumerate() {
            if pv.shown.contains(&i) || u.context.kind.as_deref() != Some(kind) {
                continue;
            }
            items.push_str(&format!("<p>{}</p>", unit_html(u)));
            pv.shown.insert(i);
        }
        if !items.is_empty() {
            extra.push_str(&format!(
                "<section class=\"bl-part\"><h6>{title}</h6>{items}</section>"
            ));
        }
    }
    // Anything else (a paragraph the walk did not reach) still gets its spans.
    let mut rest = String::new();
    for (i, u) in doc.units.iter().enumerate() {
        if !pv.shown.contains(&i) {
            rest.push_str(&format!("<p>{}</p>", unit_html(u)));
        }
    }
    if !rest.is_empty() {
        extra.push_str(&format!(
            "<section class=\"bl-part\"><h6>Other</h6>{rest}</section>"
        ));
    }
    Some(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Preview</title><style>\
body{{font-family:Georgia,serif;max-width:46rem;margin:2rem auto;line-height:1.5}}\
table{{border-collapse:collapse}}td{{border:1px solid #bbb;padding:.25rem .5rem;vertical-align:top}}\
.bl-img,.bl-code{{background:#eee;color:#555;font-size:.8em;padding:0 .25em;border-radius:3px}}\
.bl-part{{margin-top:2rem;border-top:1px solid #ccc;color:#444}}\
</style></head><body>{body}{extra}</body></html>"
    ))
}
