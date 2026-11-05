//! What a browser makes of a sitemap (Localization OS `html_guard`, #676), as far as the export check needs it.
//!
//! The sitemap walker (`sitemap.rs`) is a lenient scanner, not an HTML5 tokenizer, and on some markup the two part
//! ways: a tag the walker does not accept (`<a b="x"c="...">`, `<a.b ...>`, a value with a backtick) whose quoted
//! attribute holds text the walker then reads as a `<param>`, Unicode whitespace that the walker takes as an
//! attribute separator and a browser does not (`name=Name\u{a0}value=X`), raw-text elements, abrupt comments,
//! foreign content. Where they part ways, a translated value the walker writes as an attribute value can be outside
//! any attribute in the browser's reading, and `x" onmouseover=alert(1)` becomes live markup.
//!
//! Two layers, as in Localization OS:
//! 1. [`agreed_value_spans`]: the value spans every browser mode reads as an attribute value. A translated value
//!    outside them is written opaque (`strict_escape`): no character a tag state reacts to stays literal.
//! 2. [`check_build`]: the built file is parsed again in every mode and the merge is refused unless every element
//!    the browser builds already exists in the source with the same attributes and values (translated values, the
//!    `param` `value`, and the language attributes may differ) and every script / style body already exists.
//!
//! The tokenizer covers all tag, attribute, comment, bogus comment, CDATA, doctype, RCDATA / RAWTEXT / script-data /
//! PLAINTEXT states, driven by the parts of tree construction that switch them (SVG / MathML with integration
//! points and breakout tags, `<select>`, `<noscript>`, `<frameset>`), with scripting on and off and the legacy and
//! 2025 `<select>` rules. Offsets are byte offsets; every delimiter is ASCII, so they line up with the walker's.

use crate::text::{py_trim, unescape};
use std::collections::HashSet;

const RCDATA: &[&str] = &["title", "textarea"];
const RAWTEXT: &[&str] = &["style", "xmp", "iframe", "noembed", "noframes"];
const VOID: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "image", "img", "input", "keygen", "link", "meta", "param",
    "source", "track", "wbr",
];
const BREAKOUT: &[&str] = &[
    "b", "big", "blockquote", "body", "br", "center", "code", "dd", "div", "dl", "dt", "em", "embed", "h1", "h2", "h3", "h4", "h5", "h6",
    "head", "hr", "i", "img", "li", "listing", "menu", "meta", "nobr", "ol", "p", "pre", "ruby", "s", "small", "span", "strong", "strike",
    "sub", "sup", "table", "tt", "u", "ul", "var",
];
const SVG_HTML_IP: &[&str] = &["foreignobject", "desc", "title"];
const MATH_TEXT_IP: &[&str] = &["mi", "mo", "mn", "ms", "mtext"];
const SELECT_KEEP: &[&str] = &["option", "optgroup", "hr", "script", "template"];
const SELECT_CLOSE: &[&str] = &["input", "keygen", "textarea", "select"];
/// Attribute values the filters translate or the language declaration sets.
const FREE_ATTRS: &[&str] = &["alt", "title", "aria-label", "lang", "xml:lang", "dir"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ip {
    None,
    Html,
    MathText,
    Annotation,
}

type Attrs = Vec<(String, Option<String>)>;

struct StartTag {
    name: String,
    ns: &'static str,
    start: usize,
    attrs: Attrs,
}

/// A value span: (start, end, quote) with quote `"`, `'` or empty.
pub type ValueSpan = (usize, usize, &'static str);

#[derive(Default)]
struct View {
    starts: Vec<StartTag>,
    spans: HashSet<ValueSpan>,
    bodies: Vec<(String, String)>,
    uncertain: bool,
}

struct Frame {
    name: String,
    ns: &'static str,
    ip: Ip,
    body: Option<usize>,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

fn ws_run(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && is_ws(b[i]) {
        i += 1;
    }
    i
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// Tag name and attributes from `p` (the first name character) to the end of the tag; `None` when the tag runs into
/// the end of the file (the browser drops it). The first of duplicate attribute names wins; every value span goes
/// into `spans`.
fn read_tag(text: &str, p: usize, spans: &mut HashSet<ValueSpan>) -> Option<(String, Attrs, bool, usize)> {
    let b = text.as_bytes();
    let n = b.len();
    let mut i = p;
    while i < n && !(is_ws(b[i]) || b[i] == b'/' || b[i] == b'>') {
        i += 1;
    }
    let name = lower(&text[p..i]);
    let mut attrs: Attrs = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    loop {
        if i >= n {
            return None;
        }
        let c = b[i];
        if is_ws(c) {
            i = ws_run(b, i);
            continue;
        }
        if c == b'>' {
            return Some((name, attrs, false, i + 1));
        }
        if c == b'/' {
            if i + 1 < n && b[i + 1] == b'>' {
                return Some((name, attrs, true, i + 2));
            }
            i += 1;
            continue;
        }
        // attribute name: a leading "=" is part of the name
        let start = i;
        i = if c == b'=' { i + 1 } else { i };
        while i < n && !(is_ws(b[i]) || matches!(b[i], b'/' | b'>' | b'=')) {
            i += 1;
        }
        let aname = lower(&text[start..i]);
        let mut value: Option<String> = None;
        let j = ws_run(b, i);
        if j < n && b[j] == b'=' {
            i = ws_run(b, j + 1);
            if i >= n {
                return None;
            }
            let q = b[i];
            if q == b'"' || q == b'\'' {
                let close = text[i + 1..].find(q as char).map(|k| i + 1 + k)?;
                value = Some(text[i + 1..close].to_string());
                spans.insert((i + 1, close, if q == b'"' { "\"" } else { "'" }));
                i = close + 1;
            } else if q == b'>' {
                value = Some(String::new());
            } else {
                let mut e = i;
                while e < n && !(is_ws(b[e]) || b[e] == b'>') {
                    e += 1;
                }
                value = Some(text[i..e].to_string());
                spans.insert((i, e, ""));
                i = e;
            }
        } else {
            i = j;
        }
        if seen.insert(aname.clone()) {
            attrs.push((aname, value));
        }
    }
}

/// `p` just after `<!--`: the index after the comment as HTML5 ends it.
fn comment_end(text: &str, p: usize) -> usize {
    let rest = &text[p..];
    if rest.starts_with('>') {
        return p + 1;
    }
    if rest.starts_with("->") {
        return p + 2;
    }
    let a = rest.find("-->").map(|x| p + x + 3);
    let b = rest.find("--!>").map(|x| p + x + 4);
    match (a, b) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => text.len(),
    }
}

/// One character against an ASCII lowercase letter the way Python's `re.IGNORECASE` matches it (which also takes
/// the long s for `s`, the Kelvin sign for `k` and the dotted / dotless i for `i`).
fn ci_char(c: char, want: u8) -> bool {
    if c.is_ascii() {
        return (c as u8).to_ascii_lowercase() == want;
    }
    matches!((c, want), ('\u{17f}', b's') | ('\u{212a}', b'k') | ('\u{130}', b'i') | ('\u{131}', b'i'))
}

/// Does `text[at..]` start with `word` (ASCII lowercase) under Python's `re.I`? The byte length matched, if so.
fn ci_prefix(text: &str, at: usize, word: &str) -> Option<usize> {
    let mut len = 0;
    let mut it = text[at..].chars();
    for w in word.bytes() {
        let c = it.next()?;
        if !ci_char(c, w) {
            return None;
        }
        len += c.len_utf8();
    }
    Some(len)
}

/// Python `re.search(pattern, text, re.I)` for `<` + `word`.
fn ci_contains_tag(text: &str, word: &str) -> bool {
    text.match_indices('<').any(|(i, _)| ci_prefix(text, i + 1, word).is_some())
}

/// The index of the `<` of the end tag of an RCDATA / RAWTEXT element (`</name` then a delimiter).
fn raw_close(text: &str, p: usize, name: &str) -> usize {
    let b = text.as_bytes();
    let mut from = p;
    while let Some(k) = text[from..].find("</") {
        let at = from + k;
        if let Some(len) = ci_prefix(text, at + 2, name) {
            let e = at + 2 + len;
            if e < b.len() && (is_ws(b[e]) || b[e] == b'/' || b[e] == b'>') {
                return at;
            }
        }
        from = at + 1;
    }
    text.len()
}

/// The end of a script body, with the script data escaped and double-escaped states (Localization OS
/// `html_filter._raw_end`).
fn raw_end(text: &str, pos: usize, name: &str) -> usize {
    #[derive(PartialEq)]
    enum S {
        Data,
        Escaped,
        Double,
    }
    let b = text.as_bytes();
    let size = b.len();
    let nl = name.len();
    let mut state = S::Data;
    let mut i = pos;
    while i < size {
        if b[i] != b'<' && (state == S::Data || !text[i..].starts_with("-->")) {
            let mut nxt = text[i..].find('<').map_or(size, |k| i + k);
            if state != S::Data {
                let window = &b[i..(nxt + 2).min(size)];
                if let Some(c) = window.windows(3).position(|w| w == b"-->") {
                    if i + c < nxt {
                        nxt = i + c;
                    }
                }
            }
            i = nxt;
            continue;
        }
        if name == "script" && state == S::Data && text[i..].starts_with("<!--") {
            state = S::Escaped;
            i += 2;
            continue;
        }
        if state != S::Data && text[i..].starts_with("-->") {
            state = S::Data;
            i += 3;
            continue;
        }
        if b[i] == b'<' {
            let closing = text[i..].starts_with("</");
            let start = i + if closing { 2 } else { 1 };
            let end = start + nl;
            if end < size && b[start..end].eq_ignore_ascii_case(name.as_bytes()) && matches!(b[end], b' ' | b'\t' | b'\n' | b'\r' | 0x0c | b'/' | b'>') {
                if closing {
                    if state != S::Double {
                        return i;
                    }
                    state = S::Escaped;
                } else if name == "script" && state == S::Escaped {
                    state = S::Double;
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    size
}

fn has(list: &[&str], s: &str) -> bool {
    list.contains(&s)
}

struct Parser<'a> {
    text: &'a str,
    view: View,
    stack: Vec<Frame>,
    select_open: bool,
}

impl Parser<'_> {
    fn foreign_top(&self) -> bool {
        self.stack.last().is_some_and(|f| f.ns != "html")
    }

    fn pop_to(&mut self, idx: usize, at: usize) {
        while self.stack.len() > idx {
            let f = self.stack.pop().expect("non-empty stack");
            if let Some(body) = f.body {
                let body_text = self.text.get(body..at.max(body)).unwrap_or("").to_string();
                self.view.bodies.push((f.name, body_text));
            }
        }
    }

    fn html_mode_for_start(&self, name: &str) -> bool {
        let Some(top) = self.stack.last() else { return true };
        if top.ns == "html" || top.ip == Ip::Html {
            return true;
        }
        if top.ip == Ip::MathText && name != "mglyph" && name != "malignmark" {
            return true;
        }
        top.ip == Ip::Annotation && name == "svg"
    }

    fn pop_foreign(&mut self, at: usize) {
        let mut k = self.stack.len();
        while k > 0 && self.stack[k - 1].ns != "html" && !matches!(self.stack[k - 1].ip, Ip::Html | Ip::MathText) {
            k -= 1;
        }
        self.pop_to(k, at);
    }

    fn html_end(&mut self, name: &str, at: usize) {
        if name == "select" {
            self.select_open = false;
        }
        for k in (0..self.stack.len()).rev() {
            if self.stack[k].ns == "html" && self.stack[k].name == name {
                self.pop_to(k, at);
                return;
            }
            if self.stack[k].ip != Ip::None {
                return;
            }
        }
    }
}

fn breakout(name: &str, attrs: &Attrs) -> bool {
    has(BREAKOUT, name) || (name == "font" && attrs.iter().any(|(a, _)| a == "color" || a == "face" || a == "size"))
}

fn browser_view(text: &str, scripting: bool, legacy_select: bool) -> View {
    let b = text.as_bytes();
    let n = b.len();
    let mut ps = Parser { text, view: View::default(), stack: Vec::new(), select_open: false };
    let mut i = 0usize;
    while i < n {
        let Some(k) = text[i..].find('<') else { break };
        i += k;
        if i + 1 >= n {
            break;
        }
        let c = b[i + 1];
        if c == b'!' {
            if text[i + 2..].starts_with("--") {
                i = comment_end(text, i + 4);
            } else if b.len() >= i + 9 && b[i + 2..i + 9].eq_ignore_ascii_case(b"doctype") {
                i = text[i + 9..].find('>').map_or(n, |e| i + 9 + e + 1);
            } else if text[i + 2..].starts_with("[CDATA[") && ps.foreign_top() {
                i = text[i + 9..].find("]]>").map_or(n, |e| i + 9 + e + 3);
            } else {
                i = text[i + 2..].find('>').map_or(n, |e| i + 2 + e + 1);
            }
            continue;
        }
        if c == b'?' {
            i = text[i + 2..].find('>').map_or(n, |e| i + 2 + e + 1);
            continue;
        }
        if c == b'/' {
            let Some(&d) = b.get(i + 2) else { break };
            if d == b'>' {
                i += 3;
                continue;
            }
            if !d.is_ascii_alphabetic() {
                i = text[i + 2..].find('>').map_or(n, |e| i + 2 + e + 1);
                continue;
            }
            let Some((name, _attrs, _sc, end)) = read_tag(text, i + 2, &mut ps.view.spans) else { break };
            if ps.foreign_top() {
                if name == "br" || name == "p" {
                    ps.pop_foreign(i);
                    ps.html_end(&name, i);
                } else {
                    let mut k = ps.stack.len() as isize - 1;
                    loop {
                        if ps.stack[k as usize].name == name {
                            ps.pop_to(k as usize, i);
                            break;
                        }
                        k -= 1;
                        if k < 0 {
                            break;
                        }
                        if ps.stack[k as usize].ns == "html" {
                            ps.html_end(&name, i);
                            break;
                        }
                    }
                }
            } else {
                ps.html_end(&name, i);
            }
            i = end;
            continue;
        }
        if !c.is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let Some((mut name, attrs, sc, end)) = read_tag(text, i + 1, &mut ps.view.spans) else { break };
        let start = i;
        i = end;
        loop {
            if !ps.html_mode_for_start(&name) {
                if breakout(&name, &attrs) {
                    ps.pop_foreign(start);
                    continue;
                }
                let ns = ps.stack.last().map_or("html", |f| f.ns);
                let mut ip = Ip::None;
                if ns == "svg" && has(SVG_HTML_IP, &name) {
                    ip = Ip::Html;
                } else if ns == "math" && has(MATH_TEXT_IP, &name) {
                    ip = Ip::MathText;
                } else if ns == "math" && name == "annotation-xml" {
                    let enc = attrs.iter().find(|(a, _)| a == "encoding").and_then(|(_, v)| v.clone()).unwrap_or_default();
                    let e = py_trim(&enc).to_lowercase();
                    ip = if e == "text/html" || e == "application/xhtml+xml" { Ip::Html } else { Ip::Annotation };
                }
                ps.view.starts.push(StartTag { name: name.clone(), ns, start, attrs: attrs.clone() });
                if !sc {
                    let body = (name == "script" || name == "style").then_some(end);
                    ps.stack.push(Frame { name: name.clone(), ns, ip, body });
                }
                break;
            }
            if legacy_select && ps.select_open && !has(SELECT_KEEP, &name) {
                if has(SELECT_CLOSE, &name) {
                    ps.select_open = false;
                    for k in (0..ps.stack.len()).rev() {
                        if ps.stack[k].name == "select" && ps.stack[k].ns == "html" {
                            ps.pop_to(k, start);
                            break;
                        }
                    }
                    if name == "select" {
                        break;
                    }
                } else {
                    break;
                }
            }
            if name == "svg" || name == "math" {
                let ns: &'static str = if name == "svg" { "svg" } else { "math" };
                ps.view.starts.push(StartTag { name: name.clone(), ns, start, attrs: attrs.clone() });
                if !sc {
                    ps.stack.push(Frame { name: name.clone(), ns, ip: Ip::None, body: None });
                }
                break;
            }
            if name == "image" {
                name = "img".into();
            }
            ps.view.starts.push(StartTag { name: name.clone(), ns: "html", start, attrs: attrs.clone() });
            if name == "select" {
                ps.select_open = true;
            }
            if name == "frameset" {
                ps.view.uncertain = true;
            }
            let raw = has(RCDATA, &name) || has(RAWTEXT, &name) || (name == "noscript" && scripting);
            if raw || name == "script" {
                let close = if name == "script" { raw_end(text, i, "script") } else { raw_close(text, i, &name) };
                if name == "script" || name == "style" {
                    ps.view.bodies.push((name.clone(), text[i..close].to_string()));
                }
                i = close;
                if close < n {
                    i = match read_tag(text, close + 2, &mut ps.view.spans) {
                        None => n,
                        Some(t) => t.3,
                    };
                }
            } else if name == "plaintext" {
                i = n;
            } else if !has(VOID, &name) {
                ps.stack.push(Frame { name: name.clone(), ns: "html", ip: Ip::None, body: None });
            }
            break;
        }
    }
    ps.pop_to(0, n);
    ps.view
}

/// (scripting, legacy select) modes, as Localization OS orders them.
const MODES: [(bool, bool); 4] = [(true, false), (false, true), (false, false), (true, true)];

fn modes_for(text: &str) -> &'static [(bool, bool)] {
    let k = usize::from(ci_contains_tag(text, "noscript")) + usize::from(ci_contains_tag(text, "select"));
    &MODES[..[1, 2, 4][k]]
}

fn views(text: &str) -> Vec<View> {
    modes_for(text).iter().map(|&(s, l)| browser_view(text, s, l)).collect()
}

/// Value spans every browser mode reads as an attribute value of a tag; empty when the page holds a construct whose
/// parse depends on the browser (`<frameset>`).
pub fn agreed_value_spans(text: &str) -> HashSet<ValueSpan> {
    let vs = views(text);
    if vs.iter().any(|v| v.uncertain) {
        return HashSet::new();
    }
    let mut it = vs.into_iter();
    let mut out = it.next().map(|v| v.spans).unwrap_or_default();
    for v in it {
        out.retain(|s| v.spans.contains(s));
    }
    out
}

const META_STYLES: &[&str] = &["description", "keywords"];
const META_SOCIAL: &[&str] = &["og:title", "og:description", "twitter:title", "twitter:description"];

fn meta_is_text(attrs: &Attrs) -> bool {
    if attrs.iter().any(|(a, _)| a == "http-equiv") {
        return false;
    }
    let key = |want: &str| -> String {
        let v = attrs.iter().find(|(a, _)| a == want).and_then(|(_, v)| v.clone()).unwrap_or_default();
        py_trim(&unescape(&v)).to_lowercase()
    };
    let name = key("name");
    let prop = key("property");
    has(META_STYLES, &name) || has(META_SOCIAL, &prop) || has(META_SOCIAL, &name)
}

/// The sitemap extra free pair is (`param`, `value`).
fn free(tag: &str, attr: &str, attrs: &Attrs) -> bool {
    if has(FREE_ATTRS, attr) || (tag == "param" && attr == "value") {
        return true;
    }
    if tag == "meta" && attr == "content" {
        let equiv = attrs.iter().find(|(a, _)| a == "http-equiv").and_then(|(_, v)| v.clone()).unwrap_or_default();
        if py_trim(&equiv).to_lowercase() == "content-language" {
            return true;
        }
        return meta_is_text(attrs);
    }
    false
}

type Signature = (&'static str, String, Vec<(String, Option<Option<String>>)>);

fn signature(st: &StartTag) -> Signature {
    let mut a: Vec<(String, Option<Option<String>>)> =
        st.attrs.iter().map(|(k, v)| (k.clone(), if free(&st.name, k, &st.attrs) { None } else { Some(v.clone()) })).collect();
    a.sort();
    (st.ns, st.name.clone(), a)
}

fn line_of(text: &str, pos: usize) -> usize {
    text.as_bytes()[..pos.min(text.len())].iter().filter(|&&c| c == b'\n').count() + 1
}

fn refusal(after: &str, pos: usize, what: &str) -> String {
    format!(
        "This file cannot be exported safely: in the translated file a web browser would read {what} at line {} differently from the source file, so a translation could add markup or an attribute the source does not have. The source file has markup that browsers and the translation filter read differently (for example an abrupt comment <!-->, a <style> or <script> inside SVG, a stray </ x>, or an unclosed <textarea> or <xmp>). Correct that markup in the source file and export again.",
        line_of(after, pos)
    )
}

/// `Err(message)` unless `after`, the built sitemap, gives a browser only elements and script / style bodies that
/// `before` gives it too (translated values may differ).
pub fn check_build(before: &str, after: &str) -> Result<(), String> {
    if before == after {
        return Ok(());
    }
    let modes = modes_for(before);
    for &(s, l) in modes {
        let vb = browser_view(before, s, l);
        let va = browser_view(after, s, l);
        let known: HashSet<Signature> = vb.starts.iter().map(signature).collect();
        if let Some(st) = va.starts.iter().find(|st| !known.contains(&signature(st))) {
            return Err(refusal(after, st.start, &format!("the <{}> tag", st.name)));
        }
        let bodies: HashSet<&(String, String)> = vb.bodies.iter().collect();
        if let Some((name, _)) = va.bodies.iter().find(|bd| !bodies.contains(bd)) {
            return Err(refusal(after, 0, &format!("the content of a <{name}> element")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_and_states() {
        let t = r#"<a b="x"c="<param name=Name value=X>"><param name="Name" value="Y">"#;
        let s = agreed_value_spans(t);
        let y = t.find("Y").unwrap();
        assert!(s.contains(&(y, y + 1, "\"")));
        let x = t.find("X").unwrap();
        assert!(!s.iter().any(|sp| sp.0 == x));
        assert_eq!(raw_end("a</script>", 0, "script"), 1);
        assert_eq!(raw_end("<!--<script></script>--></script>", 0, "script"), 24);
        assert_eq!(raw_close("x</TITLE >", 0, "title"), 1);
        assert_eq!(raw_close("x</\u{17f}tyle>", 0, "style"), 1);
        assert!(check_build("<p title=a>", "<p title=b>").is_ok());
        assert!(check_build("<p class=a>", "<p class=b>").is_err());
    }
}
