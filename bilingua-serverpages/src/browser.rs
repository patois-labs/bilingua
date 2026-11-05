//! What a browser makes of a built page (LocOS `html_guard`) and the export check built on it.
//!
//! The scanner in `html.rs` and a browser's HTML5 tokenizer part ways on a few malformed constructs. `browser_view`
//! is a WHATWG-style tokenizer driven by the parts of tree construction that switch tokenizer states. Three layers
//! use it: translated attribute values are written opaque on a page where the two disagree (`strict`), the build
//! is re-parsed and refused unless it holds only elements the source held (`check_build`), and text is always
//! escaped. The XML cross-check LocOS runs with lxml on well-formed pages is not ported (a page that holds code is
//! never well-formed XML, and a plain XHTML page is covered by the browser check).

use crate::html::{self, K};
use crate::pyx::*;
use std::collections::HashSet;

fn is_ws(c: u8) -> bool {
    matches!(c, b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

const RCDATA: [&str; 2] = ["title", "textarea"];
const RAWTEXT: [&str; 5] = ["style", "xmp", "iframe", "noembed", "noframes"];
const VOID: [&str; 19] = [
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "image", "img", "input", "keygen", "link", "meta", "param", "source",
    "track", "wbr",
];
const BREAKOUT: &[&str] = &[
    "b", "big", "blockquote", "body", "br", "center", "code", "dd", "div", "dl", "dt", "em", "embed", "h1", "h2", "h3", "h4", "h5", "h6", "head", "hr",
    "i", "img", "li", "listing", "menu", "meta", "nobr", "ol", "p", "pre", "ruby", "s", "small", "span", "strong", "strike", "sub", "sup", "table",
    "tt", "u", "ul", "var",
];
const SVG_HTML_IP: [&str; 3] = ["foreignobject", "desc", "title"];
const MATH_TEXT_IP: [&str; 5] = ["mi", "mo", "mn", "ms", "mtext"];
const SELECT_KEEP: [&str; 5] = ["option", "optgroup", "hr", "script", "template"];
const SELECT_CLOSE: [&str; 4] = ["input", "keygen", "textarea", "select"];
const FREE_ATTRS: [&str; 6] = ["alt", "title", "aria-label", "lang", "xml:lang", "dir"];

const IP_NONE: u8 = 0;
const IP_HTML: u8 = 1;
const IP_MATH_TEXT: u8 = 2;
const IP_ANNOTATION: u8 = 3;

pub(crate) type Attrs = Vec<(String, Option<String>)>;

pub(crate) struct StartTag {
    pub name: String,
    pub ns: &'static str,
    pub start: usize,
    pub attrs: Attrs,
}

#[derive(Default)]
pub(crate) struct View {
    pub starts: Vec<StartTag>,
    /// (value start, value end, quote byte or 0) of every attribute value.
    pub spans: HashSet<(usize, usize, u8)>,
    pub bodies: Vec<(String, usize, String)>,
    pub uncertain: bool,
}

fn ws_run(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && is_ws(b[i]) {
        i += 1;
    }
    i
}

fn run_until(b: &[u8], mut i: usize, stop: impl Fn(u8) -> bool) -> usize {
    while i < b.len() && !stop(b[i]) {
        i += 1;
    }
    i
}

type TagRead = (String, Attrs, bool, usize);

/// Tag name and attribute states from `p` (the first name character) to the end of the tag; `None` when the tag
/// runs into the end of the text (the browser drops it).
fn read_tag(text: &str, p: usize, mut view: Option<&mut View>) -> Option<TagRead> {
    let b = text.as_bytes();
    let n = b.len();
    let e = run_until(b, p, |c| is_ws(c) || c == b'/' || c == b'>');
    let name = text[p..e].to_lowercase();
    let mut i = e;
    let mut attrs: Attrs = Vec::new();
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
        let start = i;
        i = run_until(b, if c == b'=' { i + 1 } else { i }, |c| is_ws(c) || matches!(c, b'/' | b'>' | b'='));
        let aname = text[start..i].to_lowercase();
        let mut value = None;
        let j = ws_run(b, i);
        if j < n && b[j] == b'=' {
            i = ws_run(b, j + 1);
            if i >= n {
                return None;
            }
            let q = b[i];
            if q == b'"' || q == b'\'' {
                let close = b[i + 1..].iter().position(|&x| x == q)? + i + 1;
                value = Some(text[i + 1..close].to_string());
                if let Some(v) = view.as_deref_mut() {
                    v.spans.insert((i + 1, close, q));
                }
                i = close + 1;
            } else if q == b'>' {
                value = Some(String::new());
            } else {
                let e = run_until(b, i, |c| is_ws(c) || c == b'>');
                value = Some(text[i..e].to_string());
                if let Some(v) = view.as_deref_mut() {
                    v.spans.insert((i, e, 0));
                }
                i = e;
            }
        } else {
            i = j;
        }
        if !attrs.iter().any(|(a, _)| *a == aname) {
            attrs.push((aname, value));
        }
    }
}

fn comment_end(text: &str, p: usize) -> usize {
    let b = text.as_bytes();
    let n = b.len();
    if b[p..].starts_with(b">") {
        return p + 1;
    }
    if b[p..].starts_with(b"->") {
        return p + 2;
    }
    let a = find_bytes(b, p, b"-->").map(|x| x + 3);
    // `--!>` only counts when it ends the comment before `-->` does (and a full scan per comment would be quadratic)
    let c = find_bytes(&b[..a.unwrap_or(n)], p, b"--!>").map(|x| x + 4);
    match (a, c) {
        (Some(x), Some(y)) => x.min(y),
        (Some(x), None) | (None, Some(x)) => x,
        _ => n,
    }
}

/// Index of the `<` of the end tag of an RCDATA or RAWTEXT element.
fn raw_close(text: &str, p: usize, name: &str) -> usize {
    let b = text.as_bytes();
    let mut pat = b"</".to_vec();
    pat.extend_from_slice(name.as_bytes());
    let mut from = p;
    while let Some(i) = find_ci(b, from, &pat) {
        let after = i + pat.len();
        if after < b.len() && (is_ws(b[after]) || b[after] == b'/' || b[after] == b'>') {
            return i;
        }
        from = i + 1;
    }
    b.len()
}

struct Entry {
    name: String,
    ns: &'static str,
    ip: u8,
    body: Option<usize>,
}

/// Parse `text` as an HTML5 browser does, as far as tokenizer states go.
pub(crate) fn browser_view(text: &str, scripting: bool, legacy_select: bool) -> View {
    let b = text.as_bytes();
    let n = b.len();
    let mut view = View::default();
    let mut stack: Vec<Entry> = Vec::new();
    let mut select_open = false;
    let mut i = 0;

    fn pop_to(stack: &mut Vec<Entry>, view: &mut View, text: &str, idx: usize, at: usize) {
        while stack.len() > idx {
            let e = stack.pop().expect("non-empty");
            if let Some(body) = e.body {
                let s = if at >= body { text[body..at].to_string() } else { String::new() };
                view.bodies.push((e.name, body, s));
            }
        }
    }
    fn foreign_top(stack: &[Entry]) -> bool {
        stack.last().is_some_and(|e| e.ns != "html")
    }
    fn html_end(stack: &mut Vec<Entry>, view: &mut View, text: &str, select_open: &mut bool, name: &str, at: usize) {
        if name == "select" {
            *select_open = false;
        }
        for k in (0..stack.len()).rev() {
            if stack[k].ns == "html" && stack[k].name == name {
                pop_to(stack, view, text, k, at);
                return;
            }
            if stack[k].ip != IP_NONE {
                return;
            }
        }
    }
    fn pop_foreign(stack: &mut Vec<Entry>, view: &mut View, text: &str, at: usize) {
        let mut k = stack.len();
        while k > 0 && stack[k - 1].ns != "html" && !matches!(stack[k - 1].ip, IP_HTML | IP_MATH_TEXT) {
            k -= 1;
        }
        pop_to(stack, view, text, k, at);
    }
    fn html_mode_for_start(stack: &[Entry], name: &str) -> bool {
        let Some(top) = stack.last() else { return true };
        if top.ns == "html" || top.ip == IP_HTML {
            return true;
        }
        if top.ip == IP_MATH_TEXT && name != "mglyph" && name != "malignmark" {
            return true;
        }
        top.ip == IP_ANNOTATION && name == "svg"
    }
    fn breakout(name: &str, attrs: &Attrs) -> bool {
        if BREAKOUT.contains(&name) {
            return true;
        }
        name == "font" && attrs.iter().any(|(a, _)| matches!(a.as_str(), "color" | "face" | "size"))
    }

    while i < n {
        let Some(lt) = b[i..].iter().position(|&c| c == b'<') else { break };
        i += lt;
        if i + 1 >= n {
            break;
        }
        let c = b[i + 1];
        if c == b'!' {
            if b[i + 2..].starts_with(b"--") {
                i = comment_end(text, i + 4.min(n - i));
            } else if b.len() >= i + 9 && b[i + 2..i + 9].eq_ignore_ascii_case(b"doctype") {
                i = find_bytes(b, i + 9, b">").map_or(n, |e| e + 1);
            } else if b[i + 2..].starts_with(b"[CDATA[") && foreign_top(&stack) {
                i = find_bytes(b, i + 9, b"]]>").map_or(n, |e| e + 3);
            } else {
                i = find_bytes(b, i + 2, b">").map_or(n, |e| e + 1);
            }
            continue;
        }
        if c == b'?' {
            i = find_bytes(b, i + 2, b">").map_or(n, |e| e + 1);
            continue;
        }
        if c == b'/' {
            let Some(&d) = b.get(i + 2) else { break };
            if d == b'>' {
                i += 3;
                continue;
            }
            if !d.is_ascii_alphabetic() {
                i = find_bytes(b, i + 2, b">").map_or(n, |e| e + 1);
                continue;
            }
            let Some((name, _a, _sc, end)) = read_tag(text, i + 2, Some(&mut view)) else { break };
            if foreign_top(&stack) {
                if name == "br" || name == "p" {
                    pop_foreign(&mut stack, &mut view, text, i);
                    html_end(&mut stack, &mut view, text, &mut select_open, &name, i);
                } else {
                    let mut k = stack.len() as isize - 1;
                    loop {
                        if stack[k as usize].name == name {
                            pop_to(&mut stack, &mut view, text, k as usize, i);
                            break;
                        }
                        k -= 1;
                        if k < 0 {
                            break;
                        }
                        if stack[k as usize].ns == "html" {
                            html_end(&mut stack, &mut view, text, &mut select_open, &name, i);
                            break;
                        }
                    }
                }
            } else {
                html_end(&mut stack, &mut view, text, &mut select_open, &name, i);
            }
            i = end;
            continue;
        }
        if !c.is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let Some((mut name, attrs, sc, end)) = read_tag(text, i + 1, Some(&mut view)) else { break };
        let start = i;
        i = end;
        loop {
            if !html_mode_for_start(&stack, &name) {
                if breakout(&name, &attrs) {
                    pop_foreign(&mut stack, &mut view, text, start);
                    continue;
                }
                let ns = stack.last().map_or("html", |t| t.ns);
                let mut ip = IP_NONE;
                if ns == "svg" && SVG_HTML_IP.contains(&name.as_str()) {
                    ip = IP_HTML;
                } else if ns == "math" && MATH_TEXT_IP.contains(&name.as_str()) {
                    ip = IP_MATH_TEXT;
                } else if ns == "math" && name == "annotation-xml" {
                    let enc = attrs.iter().find(|(a, _)| a == "encoding").and_then(|(_, v)| v.clone()).unwrap_or_default();
                    let e = trim(&enc).to_lowercase();
                    ip = if e == "text/html" || e == "application/xhtml+xml" { IP_HTML } else { IP_ANNOTATION };
                }
                view.starts.push(StartTag { name: name.clone(), ns, start, attrs: attrs.clone() });
                if !sc {
                    let body = (name == "script" || name == "style").then_some(end);
                    stack.push(Entry { name: name.clone(), ns, ip, body });
                }
                break;
            }
            if legacy_select && select_open && !SELECT_KEEP.contains(&name.as_str()) {
                if SELECT_CLOSE.contains(&name.as_str()) {
                    select_open = false;
                    for k in (0..stack.len()).rev() {
                        if stack[k].name == "select" && stack[k].ns == "html" {
                            pop_to(&mut stack, &mut view, text, k, start);
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
                view.starts.push(StartTag { name: name.clone(), ns, start, attrs: attrs.clone() });
                if !sc {
                    stack.push(Entry { name: name.clone(), ns, ip: IP_NONE, body: None });
                }
                break;
            }
            if name == "image" {
                name = "img".to_string();
            }
            view.starts.push(StartTag { name: name.clone(), ns: "html", start, attrs: attrs.clone() });
            if name == "select" {
                select_open = true;
            }
            if name == "frameset" {
                view.uncertain = true;
            }
            let raw = RCDATA.contains(&name.as_str()) || RAWTEXT.contains(&name.as_str()) || (name == "noscript" && scripting);
            if raw || name == "script" {
                let close = if name == "script" { html::raw_end(b, i, "script") } else { raw_close(text, i, &name) };
                if name == "script" || name == "style" {
                    view.bodies.push((name.clone(), i, text[i..close].to_string()));
                }
                i = close;
                if close < n {
                    i = match read_tag(text, close + 2, Some(&mut view)) {
                        None => n,
                        Some(t) => t.3,
                    };
                }
            } else if name == "plaintext" {
                i = n;
            } else if !VOID.contains(&name.as_str()) {
                stack.push(Entry { name: name.clone(), ns: "html", ip: IP_NONE, body: None });
            }
            break;
        }
    }
    pop_to(&mut stack, &mut view, text, 0, n);
    view
}

/// (scripting, legacy select): the first mode decides a page with neither `<noscript>` nor `<select>`, the first two
/// cover both values of each flag, and a page with both needs all four.
const MODES: [(bool, bool); 4] = [(true, false), (false, true), (false, false), (true, true)];

pub(crate) fn browser_views(text: &str) -> Vec<View> {
    let n_modes = usize::from(contains_ci(text.as_bytes(), b"<noscript")) + usize::from(contains_ci(text.as_bytes(), b"<select"));
    let count = [1, 2, 4][n_modes];
    MODES[..count].iter().map(|&(sc, sel)| browser_view(text, sc, sel)).collect()
}

/// Value spans every browser mode reads as an attribute value; empty when the parse depends on the browser.
pub(crate) fn agreed_value_spans(text: &str) -> HashSet<(usize, usize, u8)> {
    let views = browser_views(text);
    if views.iter().any(|v| v.uncertain) {
        return HashSet::new();
    }
    let mut it = views.into_iter();
    let mut out = it.next().map(|v| v.spans).unwrap_or_default();
    for v in it {
        out.retain(|s| v.spans.contains(s));
    }
    out
}

// --- the scanner/browser agreement check (LocOS `_raw_quotes_safe`) --------------------------------------

const RAW_TEXT_ELEMENTS: [&str; 8] = ["xmp", "iframe", "noembed", "noframes", "noscript", "textarea", "title", "plaintext"];

/// Where an HTML5 tokenizer ends the tag at the start of `tok`, or `None` when it never ends inside `tok`.
fn html5_tag_end(tok: &str) -> Option<usize> {
    #[derive(PartialEq, Clone, Copy)]
    enum S {
        BeforeName,
        Name,
        AfterName,
        BeforeValue,
        Dq,
        Sq,
        Unquoted,
        AfterQuoted,
    }
    let b = tok.as_bytes();
    let n = b.len();
    let mut i = if tok.starts_with("</") { 2 } else { 1 };
    while i < n && !is_ws(b[i]) && b[i] != b'/' && b[i] != b'>' {
        i += 1;
    }
    let mut state = S::BeforeName;
    while i < n {
        let c = b[i];
        if state == S::Dq || state == S::Sq {
            let q = if state == S::Dq { b'"' } else { b'\'' };
            let close = b[i..].iter().position(|&x| x == q)?;
            i += close + 1;
            state = S::AfterQuoted;
            continue;
        }
        if c == b'>' {
            return Some(i + 1);
        }
        match state {
            S::BeforeName => {
                if !is_ws(c) && c != b'/' {
                    state = S::Name;
                }
            }
            S::Name => {
                if is_ws(c) {
                    state = S::AfterName;
                } else if c == b'/' {
                    state = S::BeforeName;
                } else if c == b'=' {
                    state = S::BeforeValue;
                }
            }
            S::AfterName => {
                if c == b'=' {
                    state = S::BeforeValue;
                } else if c == b'/' {
                    state = S::BeforeName;
                } else if !is_ws(c) {
                    state = S::Name;
                }
            }
            S::BeforeValue => {
                if c == b'"' {
                    state = S::Dq;
                } else if c == b'\'' {
                    state = S::Sq;
                } else if !is_ws(c) {
                    state = S::Unquoted;
                }
            }
            S::Unquoted => {
                if is_ws(c) {
                    state = S::BeforeName;
                }
            }
            S::AfterQuoted => {
                state = S::BeforeName;
                continue;
            }
            S::Dq | S::Sq => {}
        }
        i += 1;
    }
    None
}

/// `</(?![a-zA-Z>])` anywhere in `s`.
fn has_bogus_end_tag(s: &str) -> bool {
    let b = s.as_bytes();
    let mut from = 0;
    while let Some(p) = find_bytes(b, from, b"</") {
        match b.get(p + 2) {
            None => return true,
            Some(c) if !(c.is_ascii_alphabetic() || *c == b'>') => return true,
            _ => {}
        }
        from = p + 1;
    }
    false
}

/// True when every markup boundary of this page is where an HTML5 browser puts it.
pub(crate) fn raw_quotes_safe(text: &str) -> bool {
    let Ok(tokens) = html::tokenize(text, &mut None) else { return false };
    let mut starts: HashSet<usize> = HashSet::new();
    let mut pos = 0;
    for t in &tokens {
        starts.insert(pos);
        pos += t.s.len();
    }
    pos = 0;
    let mut raw_body = false;
    for t in &tokens {
        pos += t.s.len();
        if t.s.is_empty() {
            continue;
        }
        if t.k != K::Markup {
            if !raw_body && has_bogus_end_tag(&t.s) {
                return false;
            }
            raw_body = false;
            continue;
        }
        raw_body = false;
        let s: &str = &t.s;
        if s.starts_with("<!--") {
            if s.starts_with("<!-->") || s.starts_with("<!--->") || s.contains("--!>") {
                return false;
            }
            continue;
        }
        if s.starts_with("<!") || s.starts_with("<?") {
            // char-aware: a truncated token may end inside a multi-byte character
            if s[..s.len() - s.chars().next_back().map_or(0, char::len_utf8)].contains('>') {
                return false;
            }
            continue;
        }
        if html5_tag_end(s) != Some(s.len()) {
            return false;
        }
        let (closing, name, _) = html::parse_tag_str(s);
        if closing {
            continue;
        }
        raw_body = html::SKIP_CONTAINERS.contains(&name.as_str());
        if name == "svg" || name == "math" {
            return false;
        }
        if RAW_TEXT_ELEMENTS.contains(&name.as_str()) {
            let b = text.as_bytes();
            let mut pat = b"</".to_vec();
            pat.extend_from_slice(name.as_bytes());
            let mut from = pos;
            while let Some(i) = find_ci(b, from, &pat) {
                let after = i + pat.len();
                if after < b.len() && (is_ws(b[after]) || b[after] == b'/' || b[after] == b'>') {
                    if !starts.contains(&i) {
                        return false;
                    }
                    break;
                }
                from = i + 1;
            }
        }
    }
    true
}

// --- the export check -----------------------------------------------------------------------------------

fn free_attr(tag: &str, attr: &str, attrs: &Attrs) -> bool {
    if FREE_ATTRS.contains(&attr) {
        return true;
    }
    if tag == "meta" && attr == "content" {
        let equiv = attrs.iter().find(|(a, _)| a == "http-equiv").and_then(|(_, v)| v.clone()).unwrap_or_default();
        if trim(&equiv).to_lowercase() == "content-language" {
            return true;
        }
        return html::meta_text_style(attrs).is_some();
    }
    false
}

fn signature(st: &StartTag) -> String {
    let mut parts: Vec<String> = st
        .attrs
        .iter()
        .map(|(a, v)| {
            if free_attr(&st.name, a, &st.attrs) {
                format!("{a}\u{0}free")
            } else {
                format!("{a}\u{0}{}", v.as_deref().map_or("\u{1}none".to_string(), |v| format!("v{v}")))
            }
        })
        .collect();
    parts.sort();
    format!("{}\u{2}{}\u{2}{}", st.ns, st.name, parts.join("\u{3}"))
}

fn line_of(text: &str, pos: usize) -> usize {
    text.as_bytes()[..pos.min(text.len())].iter().filter(|&&c| c == b'\n').count() + 1
}

/// The message of the refusal and its line.
pub(crate) struct Unsafe {
    pub what: String,
    pub line: usize,
}

/// Refuse unless `after`, the built page, gives a browser only elements and script/style bodies `before` gives it
/// too; translated values (alt, title, aria-label, page metadata, the language declaration) may differ.
pub(crate) fn check_build(before: &str, after: &str, sources: &[View]) -> Result<(), Unsafe> {
    if before == after {
        return Ok(());
    }
    for ((scripting, legacy_select), vb) in MODES.iter().zip(sources) {
        let va = browser_view(after, *scripting, *legacy_select);
        let known: HashSet<String> = vb.starts.iter().map(signature).collect();
        for st in &va.starts {
            if !known.contains(&signature(st)) {
                return Err(Unsafe { what: format!("the <{}> tag", st.name), line: line_of(after, st.start) });
            }
        }
        let bodies: HashSet<(&str, &str)> = vb.bodies.iter().map(|(n, _, b)| (n.as_str(), b.as_str())).collect();
        for (name, pos, body) in &va.bodies {
            if !bodies.contains(&(name.as_str(), body.as_str())) {
                return Err(Unsafe { what: format!("the content of a <{name}> element"), line: line_of(after, *pos) });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refuses(before: &str, after: &str) -> bool {
        let v = browser_views(before);
        check_build(before, after, &v).is_err()
    }

    #[test]
    fn the_export_check_accepts_translated_values_and_refuses_new_elements() {
        let src = "<p>Hi</p><img src=\"a.png\" alt=\"A cat\"><script>var a = 1;</script>";
        assert!(!refuses(src, "<p>Salut</p><img src=\"a.png\" alt=\"Un chat\"><script>var a = 1;</script>"));
        // a new attribute, a new element, a changed script
        assert!(refuses(src, "<p>Hi</p><img src=\"a.png\" alt=\"A cat\" onerror=\"x\"><script>var a = 1;</script>"));
        assert!(refuses(src, "<p>Hi</p><b>x</b><img src=\"a.png\" alt=\"A cat\"><script>var a = 1;</script>"));
        assert!(refuses(src, "<p>Hi</p><img src=\"a.png\" alt=\"A cat\"><script>var a = 2;</script>"));
        // an attribute value that is not a free one may not change
        assert!(refuses(src, "<p>Hi</p><img src=\"b.png\" alt=\"A cat\"><script>var a = 1;</script>"));
        // page metadata and the language declaration are free
        let m = "<html lang=\"en\"><meta name=\"description\" content=\"About\"><meta name=\"robots\" content=\"all\">";
        assert!(!refuses(m, "<html lang=\"fr\"><meta name=\"description\" content=\"A propos\"><meta name=\"robots\" content=\"all\">"));
        assert!(refuses(m, "<html lang=\"en\"><meta name=\"description\" content=\"About\"><meta name=\"robots\" content=\"none\">"));
    }

    #[test]
    fn the_browser_and_the_scanner_part_ways_on_these() {
        // an abrupt comment: the browser sees the img, the scanner sees one comment
        let abrupt = "<p>x</p><!--><img alt=\"Real\">-->";
        let v = browser_view(abrupt, true, false);
        assert!(v.starts.iter().any(|s| s.name == "img"));
        assert!(!raw_quotes_safe(abrupt));
        // raw text elements and foreign content
        assert!(agreed_value_spans("<xmp><img alt=\"x\"></xmp>").is_empty());
        assert!(!raw_quotes_safe("<svg><style><img alt=\"x\"></style></svg>"));
        assert!(!raw_quotes_safe("<p>a</ b></p>"));
        assert!(raw_quotes_safe("<p class=\"a\">x</p><script>var s = '<img>';</script><img alt=\"x\">"));
        // inside <xmp> the browser sees no img
        let v = browser_view("<xmp><img alt=\"x\"></xmp>", true, false);
        assert!(!v.starts.iter().any(|s| s.name == "img"));
        // <noscript> is raw text with scripting on, markup with it off
        let ns = "<noscript><img alt=\"x\"></noscript>";
        assert!(!browser_view(ns, true, false).starts.iter().any(|s| s.name == "img"));
        assert!(browser_view(ns, false, true).starts.iter().any(|s| s.name == "img"));
        assert_eq!(browser_views(ns).len(), 2);
        // a frameset makes the parse uncertain: no value is agreed
        assert!(agreed_value_spans("<frameset><img alt=\"x\"></frameset>").is_empty());
        assert!(!agreed_value_spans("<img alt=\"x\">").is_empty());
    }

    #[test]
    fn comments_scan_linearly() {
        let t = "<!-- c -->".repeat(100_000);
        let start = std::time::Instant::now();
        let v = browser_view(&t, true, false);
        assert!(v.starts.is_empty());
        assert!(start.elapsed().as_secs() < 10);
    }
}
