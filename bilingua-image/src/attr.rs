//! Start-tag attribute editing: the target language on the root `<svg>` and the right-to-left layout of translated
//! `<text>` elements (LocOS `_declare_svg_language`, `_svg_rtl`).

use crate::tok::{is_markup, parse_tag};
use bilingua_model::{py_isspace_char, py_rstrip};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

fn name_end(tok: &str) -> Option<(usize, usize)> {
    // `_TAG_NAME_RE.search`: the first run of name characters anywhere in the token
    let s = tok.char_indices().find(|(_, c)| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-'))?.0;
    let e = tok[s..].char_indices().find(|(_, c)| !(c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-'))).map_or(tok.len(), |(i, _)| s + i);
    Some((s, e))
}

/// `{attribute name: (start, end) of its value}` of a start tag, quotes included, first occurrence of a name.
pub fn attr_spans(tok: &str) -> HashMap<&str, (usize, usize)> {
    let mut spans = HashMap::new();
    let mut pos = name_end(tok).map_or(tok.len(), |(_, e)| e);
    loop {
        // \s+
        let mut p = pos;
        for c in tok[p..].chars() {
            if py_isspace_char(c) {
                p += c.len_utf8();
            } else {
                break;
            }
        }
        if p == pos {
            break;
        }
        // name: [^\s=/>"']+
        let ns = p;
        for c in tok[p..].chars() {
            if py_isspace_char(c) || matches!(c, '=' | '/' | '>' | '"' | '\'') {
                break;
            }
            p += c.len_utf8();
        }
        if p == ns {
            break;
        }
        let name = &tok[ns..p];
        let end_name = p;
        // optional: \s*=\s*value
        let mut q = p;
        let skip_ws = |mut q: usize| {
            for c in tok[q..].chars() {
                if py_isspace_char(c) {
                    q += c.len_utf8();
                } else {
                    break;
                }
            }
            q
        };
        q = skip_ws(q);
        let mut matched = None;
        if tok[q..].starts_with('=') {
            q = skip_ws(q + 1);
            let rest = &tok[q..];
            if let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
                if let Some(close) = rest[1..].find(quote) {
                    matched = Some((q, q + 1 + close + 1));
                }
            } else {
                let mut e = q;
                for c in rest.chars() {
                    if py_isspace_char(c) || matches!(c, '>' | '"' | '\'') {
                        break;
                    }
                    e += c.len_utf8();
                }
                if e > q {
                    matched = Some((q, e));
                }
            }
        }
        match matched {
            Some(sp) => {
                spans.entry(name).or_insert(sp);
                pos = sp.1;
            }
            None => pos = end_name,
        }
    }
    spans
}

/// Sets `attr` (any quoting) on a start tag, or inserts it after the element name.
pub fn set_attr(tok: &str, attr: &str, value: &str, add_if_missing: bool) -> String {
    if let Some(&(s, e)) = attr_spans(tok).get(attr) {
        return format!("{}\"{}\"{}", &tok[..s], value, &tok[e..]);
    }
    if !add_if_missing {
        return tok.to_string();
    }
    match name_end(tok) {
        Some((_, e)) => format!("{} {}=\"{}\"{}", &tok[..e], attr, value, &tok[e..]),
        None => tok.to_string(),
    }
}

/// The target language on the root element: its `xml:lang` and a plain `lang` when present; a root with neither
/// gets `xml:lang`. A first element that is not `svg` is left alone.
pub fn declare_language(toks: &mut [Cow<'_, str>], tag: &str) {
    for slot in toks.iter_mut() {
        let (closing, name, _) = parse_tag(slot);
        if name.is_empty() || closing {
            continue;
        }
        if name != "svg" {
            return;
        }
        let mut tok = slot.to_string();
        let attrs = attr_spans(&tok);
        let (has_xml, has_plain) = (attrs.contains_key("xml:lang"), attrs.contains_key("lang"));
        if has_xml || !has_plain {
            tok = set_attr(&tok, "xml:lang", tag, true);
        }
        if has_plain {
            tok = set_attr(&tok, "lang", tag, false);
        }
        *slot = Cow::Owned(tok);
        return;
    }
}

fn swap(v: &str) -> Option<&'static str> {
    match v {
        "start" => Some("end"),
        "end" => Some("start"),
        _ => None,
    }
}

/// Span of `prop`'s value inside the inline `style` of a start tag.
fn style_prop_span(tok: &str, prop: &str) -> Option<(usize, usize)> {
    let &(mut start, mut end) = attr_spans(tok).get("style")?;
    if tok[start..].starts_with(['\'', '"']) {
        start += 1;
        end = end.saturating_sub(1).max(start);
    }
    let s = &tok[start..end];
    let sb = s.as_bytes();
    let pb = prop.as_bytes();
    let mut p = 0usize;
    while p <= s.len() {
        if !s.is_char_boundary(p) {
            p += 1;
            continue;
        }
        let ok_before = p == 0 || {
            let c = s[..p].chars().next_back().unwrap_or(' ');
            c == ';' || py_isspace_char(c)
        };
        if ok_before && p + pb.len() <= s.len() && sb[p..p + pb.len()].eq_ignore_ascii_case(pb) {
            let mut q = p + pb.len();
            q = skip_ws(s, q);
            if s[q..].starts_with(':') {
                q = skip_ws(s, q + 1);
                let stop = s[q..].find([';', '!']).map_or(s.len(), |i| q + i);
                let val = py_rstrip(&s[q..stop]);
                if val.is_empty() {
                    return None;
                }
                return Some((start + q, start + q + val.len()));
            }
        }
        p += 1;
    }
    None
}

fn skip_ws(s: &str, mut q: usize) -> usize {
    for c in s[q..].chars() {
        if py_isspace_char(c) {
            q += c.len_utf8();
        } else {
            break;
        }
    }
    q
}

/// `(value, where)` of a presentation property: inline style wins over the attribute.
fn own_prop(tok: &str, prop: &str) -> Option<String> {
    if let Some((s, e)) = style_prop_span(tok, prop) {
        return Some(tok[s..e].trim().to_lowercase());
    }
    let &(s, e) = attr_spans(tok).get(prop)?;
    Some(tok[s..e].trim_matches(['"', '\'']).trim().to_lowercase())
}

fn set_prop(tok: &str, prop: &str, value: &str) -> String {
    if let Some((s, e)) = style_prop_span(tok, prop) {
        return format!("{}{}{}", &tok[..s], value, &tok[e..]);
    }
    set_attr(tok, prop, value, true)
}

/// `direction="rtl"` and a mirrored `text-anchor` on every `<text>` that holds a translated unit.
pub fn rtl(toks: &mut [Cow<'_, str>], unit_tis: &HashSet<usize>) {
    let mut stack: Vec<(String, String)> = Vec::new();
    let mut text_ti: Option<usize> = None;
    let mut text_anchor = String::from("start");
    let mut inner: Vec<(usize, String)> = Vec::new();
    let mut hit = false;
    let mut edits: Vec<(usize, &'static str, String)> = Vec::new();
    for (ti, tok) in toks.iter().enumerate() {
        if !is_markup(tok) {
            if text_ti.is_some() && unit_tis.contains(&ti) {
                hit = true;
            }
            continue;
        }
        let (closing, name, self_closing) = parse_tag(tok);
        if name.is_empty() {
            continue;
        }
        if closing {
            while let Some(top) = stack.pop() {
                if top.0 == name {
                    break;
                }
            }
            if name == "text" {
                if let Some(t) = text_ti {
                    if hit {
                        edits.push((t, "direction", "rtl".into()));
                        if let Some(m) = swap(&text_anchor) {
                            edits.push((t, "text-anchor", m.into()));
                        }
                        for (k, own) in &inner {
                            if let Some(m) = swap(own) {
                                edits.push((*k, "text-anchor", m.into()));
                            }
                        }
                    }
                    text_ti = None;
                    inner.clear();
                    hit = false;
                }
            }
            continue;
        }
        let own = own_prop(tok, "text-anchor").filter(|s| !s.is_empty());
        let effective = own.clone().unwrap_or_else(|| stack.last().map_or_else(|| "start".to_string(), |s| s.1.clone()));
        if name == "text" && text_ti.is_none() && !self_closing {
            text_ti = Some(ti);
            text_anchor = effective.clone();
            inner.clear();
            hit = false;
        } else if text_ti.is_some() && (name == "tspan" || name == "textpath") {
            if let Some(o) = own {
                inner.push((ti, o));
            }
        }
        if !self_closing {
            stack.push((name, effective));
        }
    }
    for (ti, prop, value) in edits {
        let new = set_prop(&toks[ti], prop, &value);
        toks[ti] = Cow::Owned(new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_and_set() {
        let t = r#"<svg aria-label="set lang=en" data-n=' xml:lang=x' xml:lang="en">"#;
        let a = attr_spans(t);
        assert_eq!(&t[a["xml:lang"].0..a["xml:lang"].1], "\"en\"");
        assert!(!a.contains_key("lang"));
        assert_eq!(set_attr(t, "xml:lang", "de", true), t.replace("\"en\">", "\"de\">"));
        assert_eq!(set_attr("<svg>", "xml:lang", "ar", true), "<svg xml:lang=\"ar\">");
    }

    #[test]
    fn style_prop() {
        let t = r#"<text style="font-size:12px; text-anchor: end">"#;
        let (s, e) = style_prop_span(t, "text-anchor").unwrap();
        assert_eq!(&t[s..e], "end");
        assert_eq!(set_prop(t, "text-anchor", "start"), r#"<text style="font-size:12px; text-anchor: start">"#);
    }
}
