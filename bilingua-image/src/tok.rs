//! The SVG tokenizer and unit finder, a port of Localization OS's `app/filters/image_filter.py`.
//!
//! The text is cut into alternating text and tag tokens exactly as LocOS cuts it (`_split_tags`): a tag runs from a
//! `<` to the first `>` after it with at least one character between; `<>` is not a tag. Only the text inside a
//! `<text>`, `<tspan>` or `<textPath>` container is a unit, plus the plain text of `<title>` and `<desc>` (appended
//! after the text units, styled `a11y:svg-title` / `a11y:svg-desc`).

use bilingua_model::{py_has_alnum, py_lstrip, py_rstrip, py_strip};
use std::borrow::Cow;

/// Token ranges of `text`: even positions are text (possibly empty), odd positions are tags. The same list
/// `re.split(r"(<[^>]+>)", text)` gives.
pub fn split_tags(text: &str) -> Vec<(usize, usize)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let (mut pos, mut i) = (0usize, 0usize);
    while let Some(lt) = find_byte(b, b'<', i) {
        let Some(gt) = find_byte(b, b'>', lt + 1) else { break };
        if gt == lt + 1 {
            i = gt;
            continue;
        }
        out.push((pos, lt));
        out.push((lt, gt + 1));
        pos = gt + 1;
        i = gt + 1;
    }
    out.push((pos, b.len()));
    out
}

fn find_byte(b: &[u8], x: u8, from: usize) -> Option<usize> {
    if from >= b.len() {
        return None;
    }
    b[from..].iter().position(|&c| c == x).map(|p| p + from)
}

/// A token that the unit finder treats as markup (`tok.startswith("<") and tok.endswith(">")`).
pub fn is_markup(tok: &str) -> bool {
    tok.starts_with('<') && tok.ends_with('>')
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-')
}

/// End byte offset of the name `[a-zA-Z0-9:_-]+` that starts at the beginning of `s`, if any.
fn name_len(s: &str) -> usize {
    s.char_indices().find(|(_, c)| !is_name_char(*c)).map_or(s.len(), |(i, _)| i)
}

/// `(closing, lowercase name, self_closing)` of a token; `(false, "", false)` when it names no element.
pub fn parse_tag(tok: &str) -> (bool, String, bool) {
    if !tok.starts_with('<') {
        return (false, String::new(), false);
    }
    let mut head = py_lstrip(&tok[1..]);
    let closing = head.starts_with('/');
    if closing {
        head = py_lstrip(&head[1..]);
    }
    let n = name_len(head);
    if n == 0 {
        return (false, String::new(), false);
    }
    let name = head[..n].to_ascii_lowercase();
    let self_closing = py_rstrip(tok.trim_end_matches('>')).ends_with('/');
    (closing, name, self_closing)
}

/// Decodes the five predefined XML entities and numeric character references; any other `&name;` stays as typed.
pub fn xml_unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        match entity(rest) {
            Some((ch, len)) => {
                out.push(ch);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// One entity at the start of `s` (which begins with `&`): the character and the byte length consumed.
fn entity(s: &str) -> Option<(char, usize)> {
    let b = s.as_bytes();
    if b.len() < 3 {
        return None;
    }
    if b[1] == b'#' {
        let (hex, from) = if matches!(b[2], b'x' | b'X') { (true, 3) } else { (false, 2) };
        let max = if hex { 6 } else { 7 };
        let mut n = 0usize;
        let mut cp: u32 = 0;
        while from + n < b.len() && n < max {
            let d = match (b[from + n] as char).to_digit(if hex { 16 } else { 10 }) {
                Some(d) => d,
                None => break,
            };
            cp = cp * if hex { 16 } else { 10 } + d;
            n += 1;
        }
        if n == 0 || b.get(from + n) != Some(&b';') {
            return None;
        }
        let ok = matches!(cp, 0x9 | 0xA | 0xD) || (0x20..=0xD7FF).contains(&cp) || (0xE000..=0xFFFD).contains(&cp) || (0x10000..=0x10FFFF).contains(&cp);
        if !ok {
            return None;
        }
        return char::from_u32(cp).map(|c| (c, from + n + 1));
    }
    for (name, ch) in [("amp;", '&'), ("lt;", '<'), ("gt;", '>'), ("quot;", '"'), ("apos;", '\'')] {
        if s[1..].starts_with(name) {
            return Some((ch, 1 + name.len()));
        }
    }
    None
}

/// `xml.sax.saxutils.escape`: `&`, `<` and `>` only. A control character XML 1.0 forbids is dropped (a vertical
/// tab or form feed becomes a newline), so a translation cannot make the SVG ill-formed.
pub fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{0B}' | '\u{0C}' => out.push('\n'),
            '\u{0}'..='\u{8}' | '\u{0E}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// A translatable text node: its token index and the token as written.
#[derive(Debug, Clone)]
pub struct Node {
    pub ti: usize,
    /// The unescaped text (whitespace stripped).
    pub core: String,
    /// `Some("a11y:svg-title")` / `Some("a11y:svg-desc")` for an accessibility unit.
    pub style: Option<&'static str>,
}

const TEXT_CONTAINERS: [&str; 3] = ["text", "tspan", "textpath"];
const SKIP_CONTAINERS: [&str; 6] = ["style", "script", "defs", "metadata", "title", "desc"];
const A11Y_OPAQUE: [&str; 3] = ["style", "script", "metadata"];

/// The text nodes in document order (`_svg_units`).
pub fn text_nodes(toks: &[Cow<'_, str>], cap: Option<&mut dyn FnMut(usize) -> Result<(), bilingua_model::Error>>) -> Result<Vec<Node>, bilingua_model::Error> {
    let mut cap = cap;
    let mut out = Vec::new();
    let (mut text_depth, mut skip_depth) = (0usize, 0usize);
    for (ti, tok) in toks.iter().enumerate() {
        if is_markup(tok) {
            let (closing, name, self_closing) = parse_tag(tok);
            if SKIP_CONTAINERS.contains(&name.as_str()) {
                if closing {
                    skip_depth = skip_depth.saturating_sub(1);
                } else if !self_closing {
                    skip_depth += 1;
                }
                continue;
            }
            if TEXT_CONTAINERS.contains(&name.as_str()) && !self_closing {
                text_depth = if closing { text_depth.saturating_sub(1) } else { text_depth + 1 };
            }
            continue;
        }
        if text_depth == 0 || skip_depth > 0 || tok.is_empty() {
            continue;
        }
        let core = py_strip(tok);
        if core.is_empty() || !py_has_alnum(core) {
            continue;
        }
        out.push(Node { ti, core: xml_unescape(core), style: None });
        if let Some(c) = cap.as_mut() {
            c(out.len())?;
        }
    }
    Ok(out)
}

/// The `<title>` and `<desc>` nodes (`_svg_a11y_units`): an element whose whole content is one text token.
pub fn a11y_nodes(toks: &[Cow<'_, str>], already: usize, cap: Option<&mut dyn FnMut(usize) -> Result<(), bilingua_model::Error>>) -> Result<Vec<Node>, bilingua_model::Error> {
    let mut cap = cap;
    let mut out = Vec::new();
    let mut opaque = 0usize;
    let n = toks.len();
    for (ti, tok) in toks.iter().enumerate() {
        if !is_markup(tok) {
            continue;
        }
        let (closing, name, self_closing) = parse_tag(tok);
        if A11Y_OPAQUE.contains(&name.as_str()) && !self_closing {
            opaque = if closing { opaque.saturating_sub(1) } else { opaque + 1 };
            continue;
        }
        let style = match name.as_str() {
            "title" => "a11y:svg-title",
            "desc" => "a11y:svg-desc",
            _ => continue,
        };
        if opaque > 0 || closing || self_closing || ti + 2 >= n {
            continue;
        }
        let (text, end) = (&toks[ti + 1], &toks[ti + 2]);
        if is_markup(text) {
            continue;
        }
        let (end_closing, end_name, _) = parse_tag(end);
        if !(end_closing && end_name == name) {
            continue;
        }
        let core = py_strip(text);
        if core.is_empty() || !py_has_alnum(core) {
            continue;
        }
        out.push(Node { ti: ti + 1, core: xml_unescape(core), style: Some(style) });
        if let Some(c) = cap.as_mut() {
            c(already + out.len())?;
        }
    }
    Ok(out)
}

/// The leading and trailing whitespace of a text token.
pub fn lead_trail(tok: &str) -> (&str, &str) {
    let l = tok.len() - py_lstrip(tok).len();
    let t = tok.len() - py_rstrip(tok).len();
    (&tok[..l], &tok[tok.len() - t.min(tok.len() - l)..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(s: &str) -> Vec<&str> {
        split_tags(s).into_iter().map(|(a, b)| &s[a..b]).collect()
    }

    #[test]
    fn split_matches_the_regex() {
        let s = r#"<svg x="1"><text>Hi <tspan>there</tspan> a < b</text><!-- c<d --><></svg>"#;
        assert_eq!(
            split(s),
            ["", r#"<svg x="1">"#, "", "<text>", "Hi ", "<tspan>", "there", "</tspan>", " a ", "< b</text>", "", "<!-- c<d -->", "<>", "</svg>", ""]
        );
        assert_eq!(split("a<>b"), ["a<>b"]);
        assert_eq!(split("<a"), ["<a"]);
    }

    #[test]
    fn entities() {
        assert_eq!(xml_unescape("&#169; 2024 &quot;Acme&quot; &#x41; &amp; Co"), "\u{a9} 2024 \"Acme\" A & Co");
        assert_eq!(xml_unescape("&foo; &#0; &#xD800; &#1234567; &amp"), "&foo; &#0; &#xD800; &#1234567; &amp");
        assert_eq!(xml_escape("a < b & c > d"), "a &lt; b &amp; c &gt; d");
    }

    #[test]
    fn tags() {
        assert_eq!(parse_tag("<text/>"), (false, "text".into(), true));
        assert_eq!(parse_tag("</ TSpan >"), (true, "tspan".into(), false));
        assert_eq!(parse_tag("<!-- x -->").1, "");
        assert_eq!(parse_tag("<?xml version='1.0'?>").1, "");
    }
}
