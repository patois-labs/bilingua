//! The `.hhc` / `.hhk` sitemap walk: a lenient HTML tag scanner that follows the Localization OS one exactly
//! (same tag grammar, atomic attributes, no backtracking across the file).

use crate::text::{has_alnum, py_space, py_trim, unescape};
use bilingua_model::guard::check_unit_count;
use bilingua_model::{Error, Limits};

#[derive(Debug, Clone)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    /// The unescaped value.
    pub text: String,
    /// `"` or `'` or empty for an unquoted value.
    pub quote: &'static str,
    /// `name` or `keyword` for units.
    pub kind: &'static str,
}

#[derive(Default)]
pub struct Walk {
    pub units: Vec<Span>,
    pub see_also: Vec<Span>,
    pub fonts: Vec<Span>,
}

struct Attr {
    name: String,
    start: usize,
    end: usize,
    quote: &'static str,
}

struct Tag {
    end: usize,
    closing: bool,
    name: String,
    attrs: Vec<Attr>,
}

fn skip_ws(t: &str, mut i: usize) -> usize {
    for c in t[i..].chars() {
        if !py_space(c) {
            break;
        }
        i += c.len_utf8();
    }
    i
}

fn is_name_char(c: char) -> bool {
    !(py_space(c) || matches!(c, '=' | '<' | '>' | '/' | '"' | '\''))
}

/// One attribute at `j`: name, then an optional `= value`. `None` when there is no name.
fn attr_at(t: &str, j: usize) -> Option<(Attr, usize)> {
    let mut k = j;
    for c in t[j..].chars() {
        if !is_name_char(c) {
            break;
        }
        k += c.len_utf8();
    }
    if k == j {
        return None;
    }
    let name = t[j..k].to_lowercase();
    let after_name = k;
    let eq = skip_ws(t, k);
    if t[eq..].starts_with('=') {
        let v = skip_ws(t, eq + 1);
        let rest = &t[v..];
        let (start, end, quote, next) = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => match rest[1..].find(q) {
                Some(p) => (v + 1, v + 1 + p, if q == '"' { "\"" } else { "'" }, v + p + 2),
                None => (v, v, "", v),
            },
            _ => {
                let mut e = v;
                for c in rest.chars() {
                    if py_space(c) || matches!(c, '<' | '>' | '"' | '\'') {
                        break;
                    }
                    e += c.len_utf8();
                }
                (v, e, "", e)
            }
        };
        return Some((Attr { name, start, end, quote }, next));
    }
    Some((Attr { name, start: after_name, end: after_name, quote: "" }, after_name))
}

fn match_tag(t: &str, lt: usize) -> Option<Tag> {
    let mut i = lt + 1;
    let closing = t[i..].starts_with('/');
    if closing {
        i += 1;
    }
    let first = t[i..].chars().next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let ns = i;
    i += 1;
    for c in t[i..].chars() {
        if !(c.is_alphanumeric() || matches!(c, '_' | ':' | '-')) {
            break;
        }
        i += c.len_utf8();
    }
    let name = t[ns..i].to_lowercase();
    let mut attrs = Vec::new();
    loop {
        let j = skip_ws(t, i);
        if j == i {
            break;
        }
        match attr_at(t, j) {
            Some((a, next)) => {
                attrs.push(a);
                i = next;
            }
            None => break,
        }
    }
    let mut j = skip_ws(t, i);
    if t[j..].starts_with('/') {
        j += 1;
    }
    if !t[j..].starts_with('>') {
        return None;
    }
    Some(Tag { end: j + 1, closing, name, attrs })
}

fn first_attr<'a>(t: &'a Tag, name: &str) -> Option<&'a Attr> {
    t.attrs.iter().find(|a| a.name == name)
}

fn span(a: &Attr, text: &str, kind: &'static str) -> Span {
    Span { start: a.start, end: a.end, text: unescape(&text[a.start..a.end]), quote: a.quote, kind }
}

/// (units, See Also spans, Font spans) in document order.
pub fn walk(text: &str, limits: &Limits) -> Result<Walk, Error> {
    let mut w = Walk::default();
    let mut obj: Option<String> = None;
    let mut pos = 0;
    while let Some(off) = text[pos..].find('<') {
        let lt = pos + off;
        if text[lt..].starts_with("<!--") {
            pos = match text[lt + 4..].find("-->") {
                Some(e) => lt + 4 + e + 3,
                None => text.len(),
            };
            continue;
        }
        let Some(tag) = match_tag(text, lt) else {
            pos = lt + 1;
            continue;
        };
        pos = tag.end;
        if tag.name == "object" {
            obj = if tag.closing {
                None
            } else {
                Some(first_attr(&tag, "type").map(|a| py_trim(&unescape(&text[a.start..a.end])).to_lowercase()).unwrap_or_default())
            };
            continue;
        }
        let Some(kind) = obj.as_deref() else { continue };
        if tag.name != "param" || tag.closing {
            continue;
        }
        let (Some(n), Some(v)) = (first_attr(&tag, "name"), first_attr(&tag, "value")) else { continue };
        let pname = py_trim(&unescape(&text[n.start..n.end])).to_lowercase();
        let sp = span(v, text, "");
        if kind == "text/sitemap" {
            if (pname == "name" || pname == "keyword") && has_alnum(&sp.text) {
                w.units.push(Span { kind: if pname == "name" { "name" } else { "keyword" }, ..sp });
                check_unit_count(w.units.len(), limits)?;
            } else if pname == "see also" && !py_trim(&sp.text).is_empty() {
                w.see_also.push(sp);
            }
        } else if kind == "text/site properties" && pname == "font" {
            w.fonts.push(sp);
        }
    }
    Ok(w)
}

/// `_escape_attr`: a plain value as an attribute value of the given quote style (unquoted gets double quotes).
pub fn escape_attr(s: &str, quote: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' if quote == "'" => o.push_str("&#39;"),
            '\r' | '\n' => o.push(' '),
            c => o.push(c),
        }
    }
    if quote.is_empty() {
        format!("\"{o}\"")
    } else {
        o
    }
}

/// Makes an escaped value opaque to every HTML5 tag state: each character a tokenizer reacts to becomes a numeric
/// reference.
pub fn strict_escape(escaped: &str) -> String {
    let mut o = String::with_capacity(escaped.len());
    for c in escaped.chars() {
        if matches!(c, '\t' | '\n' | '\u{0c}' | '\r' | ' ' | '/' | '=' | '`') {
            o.push_str(&format!("&#{};", c as u32));
        } else {
            o.push(c);
        }
    }
    o
}

/// The parts of the file where this walker and a browser may read the markup differently: from a raw-text or template
/// element (`script`, `style`, `title`, `textarea`, `select`, `svg`, ...) to the end of the file, a processing
/// instruction, CDATA section or bogus comment up to its `>`, and a comment a browser ends earlier than the walker
/// does (`<!-->`, `<!--->`, `--!>`). A value inside such a part is written opaque (see `strict_escape`), because a
/// browser may not read it as the attribute value the walker found. An unterminated comment swallows the rest of the
/// file in both, so it is no risk.
pub fn risky_regions(text: &str) -> Vec<(usize, usize)> {
    let b = text.as_bytes();
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while let Some(off) = text[i..].find('<') {
        let p = i + off;
        let rest = &b[p + 1..];
        let l: Vec<u8> = rest.iter().take(12).map(|c| c.to_ascii_lowercase()).collect();
        let rawtext = [&b"script"[..], b"style", b"textarea", b"title", b"xmp", b"iframe", b"noembed", b"noframes", b"plaintext", b"noscript", b"select", b"template", b"svg", b"math"];
        if rawtext.iter().any(|w| l.starts_with(w)) {
            out.push((p, text.len()));
            return out;
        }
        if rest.starts_with(b"!--") {
            let walker_end = text[p + 4..].find("-->").map(|e| p + 4 + e + 3);
            let Some(end) = walker_end else { return out };
            let early = rest[3..].starts_with(b">") || rest[3..].starts_with(b"->");
            let bang = text[p + 4..end].find("--!>").map(|k| p + 4 + k);
            if early {
                out.push((p, end));
            } else if let Some(k) = bang {
                out.push((k, end));
            }
            i = end;
            continue;
        }
        if rest.first() == Some(&b'?') || (rest.first() == Some(&b'!') && !l.starts_with(b"!doctype")) {
            let end = text[p..].find('>').map_or(text.len(), |e| p + e + 1);
            out.push((p, end));
            i = end;
            continue;
        }
        i = p + 1;
    }
    out
}

pub fn in_regions(regions: &[(usize, usize)], pos: usize) -> bool {
    regions.iter().any(|&(s, e)| s <= pos && pos < e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lenient_tags() {
        let l = Limits::default();
        let w = walk("< <<< <OBJECT type='text/sitemap'><param name='Name' value='a < b'>", &l).unwrap();
        assert_eq!(w.units.len(), 1);
        assert_eq!(w.units[0].text, "a < b");
        let w = walk("<OBJECT type=text/sitemap><param name=Name value=a/b=c></OBJECT>", &l).unwrap();
        assert_eq!(w.units[0].text, "a/b=c");
        let w = walk(r#"<OBJECT type=text/sitemap><param name=Name value="unterminated></OBJECT>"#, &l).unwrap();
        assert!(w.units.is_empty());
    }
}
