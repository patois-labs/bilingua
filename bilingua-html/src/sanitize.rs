//! `sanitize_html`: scripts, event handlers, script URLs, `<base>` and meta refresh removed from HTML (the
//! preview and any other caller that shows translated HTML in a browser).
//!
//! The tokenizer here is small and a browser's parser is not, so the output is written in a way that leaves the two
//! nothing to disagree about: **every `<` in the output starts a tag this pass has checked** (or the doctype, or sits
//! in a style sheet where it cannot start a tag). Text and attribute values get `&lt;` for `<`, comments and
//! declarations are left out, end tags are written bare. Whatever state a browser is in when it meets one of those
//! tags (inside a raw-text element, inside SVG, after a comment this pass read differently), it either ignores the
//! tag as text or reads the same checked tag; and removing a token cannot join its neighbours into a new tag.

use crate::tok::*;
use bilingua_htmlcore::pyx::unescape;

/// `s` with the edits (start, end, replacement) applied; they do not overlap.
fn apply_edits(s: &str, mut edits: Vec<(usize, usize, &str)>) -> String {
    edits.sort_by_key(|e| e.0);
    let mut out = String::with_capacity(s.len() + edits.len() * 3);
    let mut pos = 0;
    for (a, b, r) in edits {
        if pos <= a && a <= b && b <= s.len() && s.is_char_boundary(a) && s.is_char_boundary(b) {
            out.push_str(&s[pos..a]);
            out.push_str(r);
            pos = b;
        }
    }
    out.push_str(&s[pos..]);
    out
}

const PREVIEW_STYLE: &str = "<style>[data-bl-seg]{outline:none}[data-bl-seg].bl-active{outline:2px solid #3b82f6;outline-offset:2px}</style>";

fn unsafe_url(v: &str) -> bool {
    let d: String = unescape(v).chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect::<String>().to_ascii_lowercase();
    d.starts_with("javascript:") || d.starts_with("vbscript:") || d.starts_with("data:text/html")
}

/// An attribute that must not reach a browser: an event handler, `srcdoc`, a script URL, or a script URL that an SVG
/// animation element would write into a link (`<set attributeName=href to=javascript:...>`).
fn bad_attr(html: &str, elem: &str, at: &Attr) -> bool {
    if (at.name.starts_with("on") && at.name.len() > 2) || at.name == "srcdoc" {
        return true;
    }
    let value = at.val.map(|(x, y)| &html[x..y]);
    match at.name.as_str() {
        "href" | "src" | "action" | "formaction" | "xlink:href" | "data" | "poster" => value.is_some_and(unsafe_url),
        "to" | "from" | "by" | "values" if matches!(elem, "set" | "animate" | "animatetransform" | "animatemotion") => {
            value.is_some_and(|v| v.split(';').any(unsafe_url))
        }
        _ => false,
    }
}

/// Text: a `<` is written `&lt;`, so it starts no tag wherever a browser reads it.
fn push_text(out: &mut String, s: &str) {
    let mut last = 0;
    for (i, _) in s.match_indices('<') {
        out.push_str(&s[last..i]);
        out.push_str("&lt;");
        last = i + 1;
    }
    out.push_str(&s[last..]);
}

/// A style sheet: in HTML it is raw text and must stay as typed (`<!--` wrappers included), but inside SVG or MathML
/// a browser reads it as markup. A `<` that could start a tag there is written as the CSS escape `\3c `, which means
/// the same to a style sheet.
fn push_css(out: &mut String, s: &str) {
    let b = s.as_bytes();
    let mut last = 0;
    for (i, _) in s.match_indices('<') {
        if b.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'/') {
            out.push_str(&s[last..i]);
            out.push_str("\\3c ");
            last = i + 1;
        }
    }
    out.push_str(&s[last..]);
}

/// A start tag with its unwanted attributes cut, or `None` when it cannot be written so that it holds no second `<`
/// (one in the tag name or an attribute name): the caller then shows it as text.
fn start_tag(tag: &str, name: &str, attrs: &[Attr], a: usize, html: &str) -> Option<String> {
    if name.contains('<') {
        return None;
    }
    let mut edits: Vec<(usize, usize, &str)> = Vec::new();
    let mut cut = false;
    for at in attrs {
        if bad_attr(html, name, at) {
            let end = at.full.map(|f| f.1).unwrap_or(at.name_end);
            edits.push((at.name_start - a, end - a, ""));
            cut = true;
            continue;
        }
        if at.name.contains('<') {
            return None;
        }
        if let Some((x, y)) = at.val {
            // a `<` inside a value means the same as `&lt;`, and cannot end a raw-text element a browser may be in
            edits.extend(html[x..y].match_indices('<').map(|(i, _)| (x + i - a, x + i + 1 - a, "&lt;")));
        }
    }
    if edits.is_empty() {
        return Some(tag.to_string());
    }
    let out = apply_edits(tag, edits);
    if cut {
        // what is left must still read as this one tag, with nothing unwanted in it
        let ok = match markup(&out, 0, &mut false) {
            Some(Tok::Start { end, name: n2, attrs: a2, closed: true, .. }) => {
                end == out.len() && n2 == name && !out[1..].contains('<') && !a2.iter().any(|at| bad_attr(&out, name, at))
            }
            _ => false,
        };
        if !ok {
            return None;
        }
    }
    Some(out)
}

/// Removes scripts, event handler attributes, script URLs, `<base>` and meta refresh from HTML. With
/// `inject`, also adds the preview `<style>`.
///
/// Comments, processing instructions and CDATA markers are left out (the text of a CDATA section stays), a `<`
/// that starts no tag is written `&lt;`, and end tags are written bare (`</p>`).
pub fn sanitize_html(html: &str, inject: bool) -> String {
    let toks = tokenize(html);
    let mut insert_at: Option<usize> = None; // token index to insert before
    if inject {
        let mut after_html: Option<usize> = None;
        let mut first_content: Option<usize> = None;
        for (i, t) in toks.iter().enumerate() {
            match t {
                Tok::End { name, .. } if name == "head" && insert_at.is_none() => insert_at = Some(i),
                Tok::Start { name, .. } if name == "html" && after_html.is_none() => after_html = Some(i + 1),
                Tok::Start { .. } | Tok::Text(..) if first_content.is_none() => first_content = Some(i),
                _ => {}
            }
        }
        insert_at = insert_at.or(after_html).or(first_content).or(Some(0));
    }
    let mut out = String::with_capacity(html.len() + 200);
    let mut in_script = false;
    for (i, t) in toks.iter().enumerate() {
        if insert_at == Some(i) {
            out.push_str(PREVIEW_STYLE);
        }
        let (a, b) = t.range();
        match t {
            Tok::Start { name, attrs, closed, .. } => {
                if name == "script" {
                    in_script = true;
                    continue;
                }
                // a tag that runs into the end of the input: a browser drops it
                if in_script || !closed || name == "base" {
                    continue;
                }
                if name == "meta" && attr_value(html, attrs, "http-equiv").is_some_and(|v| v.trim().eq_ignore_ascii_case("refresh")) {
                    continue;
                }
                match start_tag(&html[a..b], name, attrs, a, html) {
                    Some(tag) => out.push_str(&tag),
                    None => push_text(&mut out, &html[a..b]),
                }
            }
            Tok::End { name, .. } if name == "script" => {
                in_script = false;
            }
            _ if in_script => {}
            Tok::End { name, .. } => {
                if name == "base" {
                    continue;
                }
                let tag = &html[a..b];
                let typed = &tag[2..2 + name.len()];
                if typed.contains('<') {
                    push_text(&mut out, tag);
                } else if tag.strip_suffix('>').is_some_and(|t| t[2 + name.len()..].bytes().all(|c| c.is_ascii_whitespace())) {
                    out.push_str(tag);
                } else {
                    // junk after the name (a browser reads it as attributes and ignores them)
                    out.push_str("</");
                    out.push_str(typed);
                    out.push('>');
                }
            }
            Tok::Text(..) => push_text(&mut out, &html[a..b]),
            Tok::Raw(..) => push_css(&mut out, &html[a..b]),
            Tok::CData(..) => push_text(&mut out, &html[a + 9..b - 3]),
            Tok::Decl(..) => {
                // only the doctype is kept: without it the preview would render in quirks mode
                let tag = &html[a..b];
                if tag.len() > 9 && tag.as_bytes()[2..9].eq_ignore_ascii_case(b"doctype") && tag.ends_with('>') && !tag[1..].contains('<') {
                    out.push_str(tag);
                }
            }
            Tok::Comment(..) => {}
        }
    }
    out
}
