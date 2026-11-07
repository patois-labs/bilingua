//! Bilingua filters for server pages: ASP.NET Web Forms (`.aspx`, `.ascx`), JSP (`.jsp`, `.jspf`), PHP templates
//! (`.phtml`), server-side includes (`.shtml`, `.shtm`, `.stm`) and ERB (`.erb`). See the README.
//!
//! These are HTML pages with server code mixed in. The code is never translated, never executed and never
//! re-ordered. A dialect lexer finds every code island and masks it, so the HTML scanner never sees code; in
//! running text an OUTPUT island (`<?= $n ?>`, `<%= x %>`, `${x}`) is an inline code inside its sentence and a
//! STATEMENT island (`<?php if ... ?>`, `<% } %>`, directives, comments) is a unit boundary like a block tag.
//! Merge re-derives the islands from the original text and writes every one back byte for byte; a translation
//! can neither drop, duplicate, inject nor reorder code. A file whose code cannot be tokenised safely is refused.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod lex;

use bilingua_htmlcore::{browser, html, pyx};
use browser::{attr_strictness, Strict};

use bilingua_model::*;
use html::{Entry, Piece, Tok, K};
use pyx::*;
use std::borrow::Cow;

const S_OPEN: char = '\u{FDD0}';
const S_CLOSE: char = '\u{FDD1}';

/// The server-page dialects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Dialect {
    Phtml,
    Jsp,
    Aspx,
    Erb,
    Shtml,
}

impl Dialect {
    /// The format id (LocOS format name).
    pub fn id(self) -> &'static str {
        match self {
            Dialect::Phtml => "phtml",
            Dialect::Jsp => "jsp",
            Dialect::Aspx => "aspx",
            Dialect::Erb => "erb",
            Dialect::Shtml => "shtml",
        }
    }

    fn extensions(self) -> &'static [&'static str] {
        match self {
            Dialect::Phtml => &["phtml"],
            Dialect::Jsp => &["jsp", "jspf"],
            Dialect::Aspx => &["aspx", "ascx"],
            Dialect::Erb => &["erb"],
            Dialect::Shtml => &["shtml", "shtm", "stm"],
        }
    }

    fn label(self) -> &'static str {
        match self {
            Dialect::Phtml => "PHP template",
            Dialect::Jsp => "JSP",
            Dialect::Aspx => "ASP.NET page",
            Dialect::Erb => "ERB template",
            Dialect::Shtml => "SSI page",
        }
    }

    fn islands(self, text: &str) -> Result<Vec<lex::Island>, lex::Refusal> {
        match self {
            Dialect::Phtml => lex::php_islands(text),
            Dialect::Jsp => lex::percent_islands(text, lex::Percent::Jsp),
            Dialect::Aspx => lex::percent_islands(text, lex::Percent::Aspx),
            Dialect::Erb => lex::percent_islands(text, lex::Percent::Erb),
            Dialect::Shtml => lex::ssi_islands(text),
        }
    }

    /// The text of every code island of `text`, in order (what a merge must write back byte for byte).
    pub fn code_islands(self, text: &str) -> Result<Vec<String>, Error> {
        let islands = self.islands(text).map_err(|r| refused_lex(self, text, r))?;
        Ok(islands.into_iter().map(|(s, e, _)| text[s..e].to_string()).collect())
    }

    /// The dialect for a file extension (without the dot, any case).
    pub fn from_extension(ext: &str) -> Option<Dialect> {
        let e = ext.to_ascii_lowercase();
        [Dialect::Phtml, Dialect::Jsp, Dialect::Aspx, Dialect::Erb, Dialect::Shtml].into_iter().find(|d| d.extensions().contains(&e.as_str()))
    }

    /// The filter object of this dialect.
    pub fn filter(self) -> Box<dyn Filter> {
        match self {
            Dialect::Phtml => Box::new(Phtml),
            Dialect::Jsp => Box::new(Jsp),
            Dialect::Aspx => Box::new(Aspx),
            Dialect::Erb => Box::new(Erb),
            Dialect::Shtml => Box::new(Shtml),
        }
    }
}

const HINT: &str = "Move the code out of the tag or close it, then upload again.";

fn line_of(text: &str, pos: usize) -> usize {
    text.as_bytes()[..pos.min(text.len())].iter().filter(|&&c| c == b'\n').count() + 1
}

fn refused(d: Dialect, text: &str, pos: usize, what: &str, hint: &str) -> Error {
    Error::Parse {
        format: d.id().into(),
        msg: format!("This server page cannot be translated safely: {what} (line {}). {hint}", line_of(text, pos)),
    }
}

fn refused_lex(d: Dialect, text: &str, r: lex::Refusal) -> Error {
    refused(d, text, r.pos, &r.what, HINT)
}

// --- masking ------------------------------------------------------------------------------------------------

struct Masked {
    /// The page with every island replaced by `FDD0 index FDD1`.
    text: String,
    /// Byte range of each island in the original.
    codes: Vec<(usize, usize)>,
    inline: Vec<bool>,
}

impl Masked {
    fn unmask(&self, s: &str, orig: &str) -> String {
        if !s.contains(S_OPEN) {
            return s.to_string();
        }
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(p) = rest.find(S_OPEN) {
            out.push_str(&rest[..p]);
            let after = &rest[p + S_OPEN.len_utf8()..];
            match after.find(S_CLOSE) {
                Some(q) => {
                    if let Some(&(a, b)) = after[..q].parse::<usize>().ok().and_then(|k| self.codes.get(k)) {
                        out.push_str(&orig[a..b]);
                    }
                    rest = &after[q + S_CLOSE.len_utf8()..];
                }
                None => {
                    out.push_str(&rest[p..]);
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out
    }
}

/// A quoted string in an island that STARTS an HTML tag the island never ends: the tag would straddle code.
fn check_island_body(d: Dialect, text: &str, s: usize, e: usize) -> Result<(), Error> {
    let island = &text[s..e];
    let body = if let Some(b) = island.strip_suffix("-->") {
        b
    } else if let Some(b) = island.strip_suffix("?>").or_else(|| island.strip_suffix("%>")) {
        b
    } else if let Some(p) = jsp_closer_start(island) {
        &island[..p]
    } else {
        island
    };
    let b = body.as_bytes();
    let mut last_end: Option<usize> = None;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'"' || b[i] == b'\'' {
            let rest = &body[i + 1..];
            let t = trim_start(rest);
            let mut j = i + 1 + (rest.len() - t.len());
            if b.get(j) == Some(&b'<') {
                j += 1;
                if b.get(j) == Some(&b'/') {
                    j += 1;
                }
                if b.get(j).is_some_and(u8::is_ascii_alphabetic) {
                    j += 1;
                    while b.get(j).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b':' || *c == b'-') {
                        j += 1;
                    }
                    let la = body[j..].chars().next();
                    if la.is_some_and(|c| is_space(c) || c == '"' || c == '\'' || c == '/') {
                        last_end = Some(j);
                        i = j;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    if let Some(le) = last_end {
        if !body[le..].contains('>') {
            return Err(refused(d, text, s, "code prints the start of an HTML tag that the page finishes", HINT));
        }
    }
    Ok(())
}

/// `</jsp:[a-z]+\s*>\Z` (case-insensitive): where the closer starts.
fn jsp_closer_start(s: &str) -> Option<usize> {
    let t = s.strip_suffix('>')?;
    let t = trim_end(t);
    let p = t.to_ascii_lowercase().rfind("</jsp:")?;
    let name = &t[p + 6..];
    (!name.is_empty() && name.bytes().all(|c| c.is_ascii_alphabetic())).then_some(p)
}

fn mask(d: Dialect, text: &str, limits: &Limits) -> Result<Masked, Error> {
    if let Some(p) = text.find([S_OPEN, S_CLOSE]) {
        return Err(refused(d, text, p, "the file contains reserved characters U+FDD0/U+FDD1", "Remove those characters, then upload again."));
    }
    let islands = d.islands(text).map_err(|r| refused_lex(d, text, r))?;
    if islands.len() > limits.max_units.saturating_mul(10) {
        return Err(LimitError::new(
            LimitKind::Units,
            format!(
                "This server page has far more code blocks than {} segments allow. Split it into smaller files.",
                limits.max_units
            ),
        )
        .into());
    }
    let mut out = String::with_capacity(text.len());
    let mut codes = Vec::with_capacity(islands.len());
    let mut inline = Vec::with_capacity(islands.len());
    let mut pos = 0;
    for (k, &(s, e, il)) in islands.iter().enumerate() {
        check_island_body(d, text, s, e)?;
        out.push_str(&text[pos..s]);
        out.push(S_OPEN);
        out.push_str(&k.to_string());
        out.push(S_CLOSE);
        codes.push((s, e));
        inline.push(il);
        pos = e;
    }
    out.push_str(&text[pos..]);
    Ok(Masked { text: out, codes, inline })
}

/// A tag library's script or style element (`s:script`, `aui:script`, `x:style`).
fn prefixed_script(name: &str) -> bool {
    name.split_once(':').is_some_and(|(p, l)| !p.is_empty() && matches!(l, "script" | "style"))
}

/// Sentinels in a tag token may only sit inside attribute VALUES.
fn check_tag(d: Dialect, tok: &str, orig: &str, m: &Masked, masked_pos: usize) -> Result<(), Error> {
    if !tok.contains(S_OPEN) || tok.starts_with("<!") || tok.starts_with("<?") {
        return Ok(());
    }
    // `=\s*("[^"]*"|'[^']*'|[^\s>"']+)`: the value spans
    let b = tok.as_bytes();
    let mut allowed: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'=' {
            i += 1;
            continue;
        }
        let rest = &tok[i + 1..];
        let v = i + 1 + (rest.len() - trim_start(rest).len());
        let r = &tok[v..];
        let len = if let Some(x) = r.strip_prefix('"').and_then(|x| x.find('"')) {
            Some(x + 2)
        } else if let Some(x) = r.strip_prefix('\'').and_then(|x| x.find('\'')) {
            Some(x + 2)
        } else {
            let l = r.find(|c: char| is_space(c) || matches!(c, '>' | '"' | '\'')).unwrap_or(r.len());
            (l > 0).then_some(l)
        };
        match len {
            Some(l) => {
                allowed.push((v, v + l));
                i = v + l;
            }
            None => i += 1,
        }
    }
    let mut from = 0;
    while let Some(p) = tok[from..].find(S_OPEN) {
        let s = from + p;
        let e = tok[s..].find(S_CLOSE).map_or(tok.len(), |q| s + q + S_CLOSE.len_utf8());
        if !allowed.iter().any(|&(a, b)| a <= s && e <= b) {
            // Code BETWEEN attributes (after the tag name, between two attributes, before `>` or `/>`, or around a bare
            // attribute) is an opaque code of the tag. It must not build a tag or attribute name, and the rest of the
            // tag must read cleanly.
            if !(bare_code_ok(tok, s, e, &allowed, m) && tag_is_clean(tok, &allowed)) {
                let prefix = m.unmask(&m.text[..masked_pos + s], orig);
                return Err(refused(d, &prefix, prefix.len(), "code is used as a tag or attribute name", HINT));
            }
        }
        from = e;
    }
    Ok(())
}

const KNOWN_ELEMENTS: [&str; 40] = [
    "a", "b", "i", "u", "p", "br", "hr", "li", "ul", "ol", "dl", "dt", "dd", "tr", "td", "th", "div", "span", "img", "input",
    "label", "select", "option", "button", "table", "tbody", "thead", "tfoot", "form", "textarea", "section", "article",
    "nav", "header", "footer", "main", "aside", "fieldset", "iframe", "link",
];

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '-')
}

/// The code at `s..e` of `tok` (outside every attribute value) does not touch a tag name or an attribute name: before
/// it comes white space, a closing quote, another code, or a bare attribute name that itself starts after a code; after
/// it comes white space, `>`, `/`, the end, another code or a name that is not followed by `=`.
fn bare_code_ok(tok: &str, s: usize, e: usize, allowed: &[(usize, usize)], m: &Masked) -> bool {
    let before = &tok[..s];
    let run = before.len() - before.trim_end_matches(is_name_char).len();
    let head = &before[..before.len() - run];
    let before_ok = if run == 0 {
        before.chars().next_back().is_some_and(|c| is_space(c) || c == S_CLOSE) || allowed.iter().any(|&(_, b)| b == s)
    } else if head.ends_with(S_CLOSE) {
        true
    } else {
        // a name glued to the code: `<th<?php echo $attrs ?> id="a">` (a known element name), or a bare attribute
        // followed by a STATEMENT (`selected<?php endif; ?>`). An output code could be the rest of the name.
        let name = &before[before.len() - run..];
        let k = tok[s + S_OPEN.len_utf8()..e - S_CLOSE.len_utf8()].parse::<usize>().ok();
        let statement = k.and_then(|k| m.inline.get(k)).is_some_and(|il| !il);
        (head == "<" && KNOWN_ELEMENTS.contains(&name.to_ascii_lowercase().as_str())) || (head != "<" && statement)
    };
    let after = &tok[e..];
    let name = after.len() - after.trim_start_matches(is_name_char).len();
    let after_ok = match after[name..].chars().next() {
        None => true,
        Some('=') => name > 0,
        Some(c) => is_space(c) || matches!(c, '>' | '/' | S_OPEN),
    };
    before_ok && after_ok
}

/// A tag that holds code between its attributes must read cleanly: outside the attribute values and the codes there
/// is no quote and no `<` (a tag nested inside a quoted value, such as `href="a<x:url value="${v}"/>b"`, would make the
/// HTML tokenizer read junk, so such a page stays refused).
fn tag_is_clean(tok: &str, allowed: &[(usize, usize)]) -> bool {
    let mut i = 1;
    for (j, c) in tok.char_indices().skip(1) {
        if j < i {
            continue;
        }
        i = j + c.len_utf8();
        if let Some(&(_, b)) = allowed.iter().find(|&&(a, b)| a <= j && j < b) {
            i = b;
        } else if c == S_OPEN {
            i = tok[j..].find(S_CLOSE).map_or(tok.len(), |q| j + q + S_CLOSE.len_utf8());
        } else if matches!(c, '"' | '\'' | '<') {
            return false;
        }
    }
    true
}

/// The HTML tokens of the masked text, with islands in text split out as island tokens.
fn tokens<'m>(d: Dialect, m: &'m Masked, orig: &str, budget: &mut Option<i64>, limits: &Limits) -> Result<Vec<Tok<'m>>, Error> {
    let raw = html::tokenize(&m.text, budget).map_err(|_| {
        Error::from(LimitError::new(
            LimitKind::Units,
            format!("This server page has far more markup than {} segments allow. Split it into smaller files.", limits.max_units),
        ))
    })?;
    let mut out: Vec<Tok<'m>> = Vec::with_capacity(raw.len() + m.codes.len() * 2);
    let mut skip_name = String::new();
    let mut unclosed: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut pos = 0;
    let mut ri = 0;
    while ri < raw.len() {
        let (kind, len) = (raw[ri].k, raw[ri].s.len());
        let s: &'m str = match raw[ri].s {
            Cow::Borrowed(s) => s,
            Cow::Owned(_) => unreachable!("tokenizer borrows"),
        };
        ri += 1;
        if kind == K::Markup {
            check_tag(d, s, orig, m, pos)?;
            check_jsp_nested_tag(d, s, orig, m, pos)?;
            let (closing, name, sc) = html::parse_tag_str(s);
            if !skip_name.is_empty() {
                if closing && name == skip_name {
                    skip_name.clear();
                }
            } else if html::SKIP_CONTAINERS.contains(&name.as_str()) && !closing {
                skip_name = name;
            } else if !closing && !sc && prefixed_script(&name) && !unclosed.contains(&name) {
                // `<s:script>` / `<aui:script>`: a tag library's script or style element. Its body is code for a
                // browser, not text, so it becomes one opaque run (no units, no attributes) up to the matching end tag.
                // Without an end tag the page does not compile on its server: read it as HTML (as 0.9.0 did) rather
                // than lose the rest of the page's text. `unclosed` keeps a page of many such tags linear.
                let mut j = ri;
                let mut body_len = 0;
                while j < raw.len() {
                    if raw[j].k == K::Markup {
                        let (c, n, _) = html::parse_tag_str(&raw[j].s);
                        if c && n == name {
                            break;
                        }
                    }
                    body_len += raw[j].s.len();
                    j += 1;
                }
                if j == raw.len() {
                    unclosed.insert(name);
                    out.push(Tok { k: K::Markup, s: Cow::Borrowed(s) });
                    pos += len;
                    continue;
                }
                out.push(Tok { k: K::Markup, s: Cow::Borrowed(s) });
                pos += len;
                let body_start = pos;
                pos += body_len;
                ri = j;
                let mut body: &'m str = &m.text[body_start..pos];
                // a run that starts with `<` would be read as a tag name: hand those over one by one
                while body.starts_with('<') {
                    out.push(Tok { k: K::Island, s: Cow::Borrowed(&body[..1]) });
                    body = &body[1..];
                }
                if !body.is_empty() {
                    out.push(Tok { k: K::Island, s: Cow::Borrowed(body) });
                }
                continue;
            }
            out.push(Tok { k: K::Markup, s: Cow::Borrowed(s) });
        } else if s.contains(S_OPEN) {
            let mut last = 0;
            while let Some(p) = s[last..].find(S_OPEN) {
                let st = last + p;
                let after = st + S_OPEN.len_utf8();
                let digits = s[after..].bytes().take_while(u8::is_ascii_digit).count();
                let close_at = after + digits;
                if digits == 0 || !s[close_at..].starts_with(S_CLOSE) {
                    // not one of ours (cannot happen: the file holds no reserved characters)
                    last = after;
                    continue;
                }
                let end = close_at + S_CLOSE.len_utf8();
                let before = &s[last..st];
                if skip_name.is_empty() && (before.ends_with('<') || before.ends_with("</")) {
                    let prefix = m.unmask(&m.text[..pos + st], orig);
                    return Err(refused(d, &prefix, prefix.len(), "code is used as a tag name", HINT));
                }
                out.push(Tok { k: K::Text, s: Cow::Borrowed(before) });
                let k: usize = s[after..close_at].parse().unwrap_or(usize::MAX);
                let il = m.inline.get(k).copied().unwrap_or(false);
                out.push(Tok { k: if il { K::InlineIsland } else { K::Island }, s: Cow::Borrowed(&s[st..end]) });
                last = end;
            }
            out.push(Tok { k: K::Text, s: Cow::Borrowed(&s[last..]) });
        } else {
            out.push(Tok { k: kind, s: Cow::Borrowed(s) });
        }
        pos += len;
    }
    Ok(out)
}

// --- extraction ---------------------------------------------------------------------------------------------

fn has_sentinel(s: &str) -> bool {
    s.contains(S_OPEN)
}

fn cap_display(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n).collect();
        t.push('\u{2026}');
        t
    }
}

fn label_for(name: &str) -> String {
    match name {
        "b" | "strong" => "bold",
        "i" | "em" | "cite" => "italic",
        "a" => "link",
        "u" | "ins" => "underline",
        "s" | "strike" | "del" => "strikethrough",
        "code" | "kbd" | "samp" | "tt" => "code",
        "br" | "wbr" => "line break",
        "img" => "image",
        "sup" => "superscript",
        "sub" => "subscript",
        other => other,
    }
    .to_string()
}

fn body_unit(id: String, u: &html::BodyUnit, m: &Masked, orig: &str) -> Unit {
    let mut inl: Vec<Inline> = Vec::new();
    let mut codes = std::collections::BTreeMap::new();
    for p in &u.parts {
        match p {
            Piece::Text(t) => inl.push(Inline::text(*t)),
            Piece::Open(n) | Piece::Close(n) | Piece::Point(n) => {
                let e = &u.map[*n as usize - 1];
                let key = n.to_string();
                match p {
                    Piece::Open(_) => {
                        inl.push(Inline::tag(key.clone(), TagKind::Open, format!("<{}>", e.name)));
                        codes.insert(
                            key,
                            CodeData::new().with_open(e.open.as_deref().map(|s| m.unmask(s, orig))).with_close(e.close.as_deref().map(|s| m.unmask(s, orig))).with_label(label_for(&e.name)),
                        );
                    }
                    Piece::Close(_) => inl.push(Inline::tag(key, TagKind::Close, format!("</{}>", e.name))),
                    _ => {
                        let raw = m.unmask(e.point.as_deref().unwrap_or(""), orig);
                        let (display, label) = if e.island {
                            (cap_display(&raw, 200), "server code".to_string())
                        } else if e.name.is_empty() {
                            (cap_display(&raw, 80), "not translated".to_string())
                        } else {
                            (format!("<{}>", e.name), label_for(&e.name))
                        };
                        inl.push(Inline::tag(key.clone(), TagKind::Standalone, display));
                        codes.insert(key, CodeData::new().with_standalone(raw).with_label(label));
                    }
                }
            }
        }
    }
    let mut unit = Unit::new(id, merge_text(inl));
    unit.codes = codes;
    unit.context.kind = Some("paragraph".into());
    unit
}

fn attr_kind(style: &str) -> &'static str {
    if style.ends_with("alt") {
        "alt"
    } else if style.ends_with("title") && style.starts_with("a11y") {
        "title"
    } else if style.ends_with("aria-label") {
        "label"
    } else {
        "meta"
    }
}

/// The attribute units a page offers: the accessibility and metadata attributes, without code in the value.
fn attr_units(d: Dialect, toks: &[Tok]) -> Vec<html::AttrUnit> {
    html::iter_page_attr_units(toks)
        .into_iter()
        .filter(|a| {
            let v = &toks[a.ti].s[a.start..a.end];
            // a JSP custom tag in a value (`title="<fmt:message key='x'/>"`) is code the server runs, not text
            !has_sentinel(v) && !(d == Dialect::Jsp && has_prefixed_tag(v))
        })
        .collect()
}

/// `<prefix:name` or `</prefix:name` somewhere in `s` (a JSP custom action such as `<c:if`, `<fmt:message`).
fn has_prefixed_tag(s: &str) -> bool {
    s.match_indices('<').any(|(i, _)| starts_prefixed_tag(&s[i..]))
}

/// `s` starts with `<prefix:name` or `</prefix:name`.
fn starts_prefixed_tag(s: &str) -> bool {
    let Some(r) = s.strip_prefix('<') else { return false };
    let r = r.strip_prefix('/').unwrap_or(r);
    let p = r.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))).unwrap_or(r.len());
    p > 0 && r.as_bytes()[0].is_ascii_alphabetic() && r[p..].starts_with(':') && r[p + 1..].starts_with(|c: char| c.is_ascii_alphabetic())
}

/// JSP: a custom tag between the attributes of an HTML tag (`<input <c:if test="${x}">checked</c:if>>`). The HTML
/// scanner ends the outer tag at the inner tag's `>` and would offer the rest of the outer tag as text, so the page is
/// refused. A custom tag inside a quoted value is read correctly and is not offered (see `attr_units`).
fn check_jsp_nested_tag(d: Dialect, tok: &str, orig: &str, m: &Masked, masked_pos: usize) -> Result<(), Error> {
    if d != Dialect::Jsp || tok.starts_with("<!") || tok.starts_with("<?") || tok.starts_with("</") {
        return Ok(());
    }
    let mut quote: Option<char> = None;
    for (j, c) in tok.char_indices().skip(1) {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '<' && starts_prefixed_tag(&tok[j..]) => {
                let prefix = m.unmask(&m.text[..masked_pos + j], orig);
                return Err(refused(d, &prefix, prefix.len(), "code is used as a tag or attribute name", HINT));
            }
            None => {}
        }
    }
    Ok(())
}

fn load(bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<enc::Decoded, Error> {
    guard::check_input(bytes, limits)?;
    let dec = enc::decode_text(bytes, &enc::DecodeOptions::with_limits(*limits).with_source_lang(opts.source_lang.as_deref()).declared())?;
    if !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") && enc::encode_text(&dec.text, &dec.encoding)? != bytes {
        return Err(Error::Encoding(
            "This file uses byte sequences of its legacy encoding that cannot be written back unchanged. Re-save the file as UTF-8.".into(),
        ));
    }
    Ok(dec)
}

fn extract_with(d: Dialect, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
    let dec = load(bytes, opts, limits)?;
    let text = dec.text.as_str();
    // LocOS: ten markup characters per allowed segment, and a hundred scan steps per allowed segment
    if text.bytes().filter(|&c| c == b'<').count() > limits.max_units.saturating_mul(10) {
        return Err(LimitError::new(
            LimitKind::Units,
            format!("This {} file has far more tags than {} segments allow. Split it into smaller files.", d.label(), limits.max_units),
        )
        .into());
    }
    guard::check_markup(text, limits)?;
    let m = mask(d, text, limits)?;
    let mut budget = Some(i64::try_from(limits.max_units).unwrap_or(i64::MAX).saturating_mul(100));
    let toks = tokens(d, &m, text, &mut budget, limits)?;
    let body = match html::iter_units(&toks, Some(limits.max_units)) {
        Ok(b) => b,
        Err(n) => {
            guard::check_unit_count(n, limits)?;
            Vec::new()
        }
    };
    let mut units: Vec<Unit> = Vec::with_capacity(body.len());
    for (i, u) in body.iter().enumerate() {
        units.push(body_unit(format!("u{}", i + 1), u, &m, text));
    }
    for a in attr_units(d, &toks) {
        guard::check_unit_count(units.len() + 1, limits)?;
        let mut u = Unit::new(format!("u{}", units.len() + 1), vec![Inline::text(a.text.clone())]);
        u.context.kind = Some(attr_kind(&a.style).into());
        u.context.style = Some(a.style.clone());
        u.context.key = html::parse_tag(&toks[a.ti]).1.into();
        units.push(u);
    }
    let doc = Document::new(d.id(), units, Skeleton::text(vec![SkelPart::Raw(text.to_string())], dec.encoding.clone(), bilingua_model::parser_info!())).with_options(opts);
    guard::finish(doc, dec.warnings, limits)
}

// --- merge --------------------------------------------------------------------------------------------------

/// A target the page writes: the unit's output when it is translated and different from the source.
fn target_of(u: &Unit) -> Option<Vec<Inline>> {
    if u.segments.iter().all(|s| !s.has_target()) {
        return None;
    }
    let out = u.output();
    (out != u.source()).then_some(out)
}

fn escape_el(s: &str, out: &mut String) {
    // Template text "${" / "#{" would be evaluated as EL; "\${" prints it literally.
    let b = s.as_bytes();
    let mut last = 0;
    for i in 0..b.len() {
        if (b[i] == b'$' || b[i] == b'#') && b.get(i + 1) == Some(&b'{') && (i == 0 || b[i - 1] != b'\\') {
            out.push_str(&s[last..i]);
            out.push('\\');
            last = i;
        }
    }
    out.push_str(&s[last..]);
}

fn strip_sentinels(s: &str) -> Cow<'_, str> {
    if s.contains([S_OPEN, S_CLOSE]) {
        Cow::Owned(s.chars().filter(|c| *c != S_OPEN && *c != S_CLOSE).collect())
    } else {
        Cow::Borrowed(s)
    }
}

/// The markup text of a translated unit: text escaped, every inline code rebuilt from the original markup, every
/// output island exactly once (a dropped one is appended, a repeated one written once).
fn materialize(d: Dialect, target: &[Inline], map: &[Entry], m: &Masked, orig: &str) -> String {
    let mut out = String::new();
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let entry = |id: &str| id.parse::<u32>().ok().filter(|n| *n >= 1).and_then(|n| map.get(n as usize - 1).map(|e| (n, e)));
    // Text runs are escaped as one string up to the next code that writes something: a dropped tag, an empty close
    // or two adjacent text pieces must not join "$" and "{" (or "#" and "{") into a JSP expression after escaping.
    let mut pending = String::new();
    let mut seen_code: std::collections::HashSet<(u32, TagKind)> = std::collections::HashSet::new();
    let flush = |pending: &mut String, out: &mut String| {
        if pending.is_empty() {
            return;
        }
        if d == Dialect::Jsp {
            let mut el = String::with_capacity(pending.len());
            escape_el(pending, &mut el);
            escape_text(&el, out);
        } else {
            escape_text(pending, out);
        }
        pending.clear();
    };
    for i in target {
        match i {
            Inline::Text(t) => pending.push_str(&strip_sentinels(t)),
            Inline::Tag { id, kind, .. } => {
                let Some((n, e)) = entry(id) else { continue };
                let code = match kind {
                    TagKind::Standalone => e.point.as_deref().unwrap_or(""),
                    TagKind::Open => e.open.as_deref().unwrap_or(""),
                    TagKind::Close => e.close.as_deref().unwrap_or(""),
                };
                if *kind == TagKind::Standalone && e.island {
                    if seen.insert(n) {
                        flush(&mut pending, &mut out);
                        push_island(d, &mut out, code, m, orig);
                    }
                } else if !code.is_empty() {
                    // a tag that carries code (`<a href="<?php track(); ?>">`) is written once, so the code never runs twice
                    if has_sentinel(code) && !seen_code.insert((n, *kind)) {
                        continue;
                    }
                    flush(&mut pending, &mut out);
                    out.push_str(code);
                }
            }
        }
    }
    flush(&mut pending, &mut out);
    for (k, e) in map.iter().enumerate() {
        if e.island && !seen.contains(&(k as u32 + 1)) {
            push_island(d, &mut out, e.point.as_deref().unwrap_or(""), m, orig);
        }
    }
    out
}

/// Writes an output island. In JSP a backslash right before `${` or `#{` makes the expression literal text, so a
/// translation that ends in a backslash must not touch it: the backslashes become character references.
fn push_island(d: Dialect, out: &mut String, code: &str, m: &Masked, orig: &str) {
    let real = m.unmask(code, orig);
    if d == Dialect::Jsp && (real.starts_with("${") || real.starts_with("#{")) && out.ends_with('\\') {
        let keep = out.trim_end_matches('\\').len();
        let n = out.len() - keep;
        out.truncate(keep);
        for _ in 0..n {
            out.push_str("&#92;");
        }
    }
    out.push_str(code);
}

fn target_plain(t: &[Inline]) -> String {
    plain_text(t)
}

fn merge_with(d: Dialect, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write(format!("{} needs a text skeleton", d.id())));
    };
    let mut text = String::new();
    for p in parts {
        if let SkelPart::Raw(s) = p {
            text.push_str(s);
        }
    }
    let targets: Vec<Option<Vec<Inline>>> = doc.units.iter().map(target_of).collect();
    let translated = targets.iter().any(Option::is_some);
    // the language is stamped when a unit is translated, or always with `stamp_target_always` (LocOS)
    let tag = if translated || opts.stamp_target_always { doc.target_lang.as_deref().and_then(html::html_lang) } else { None };
    if !translated && tag.is_none() {
        return enc::encode_text_with(&text, encoding, opts);
    }
    let limits = Limits::default();
    let m = mask(d, &text, &limits)?;
    let mut toks = tokens(d, &m, &text, &mut None, &limits)?;
    if let Some(tag) = tag {
        let saved = toks.clone();
        html::declare_language(&mut toks, &tag, html::text_direction(&tag));
        for (old, new) in saved.iter().zip(toks.iter_mut()) {
            if old.s != new.s && has_sentinel(&old.s) {
                new.s = old.s.clone(); // <html lang="<?= $lang ?>">: a dynamic language stays dynamic
            }
        }
    }
    let before: String = toks.iter().map(|t| t.s.as_ref()).collect();
    let n_body = html::iter_units(&toks, None).map_err(|_| Error::Write("unit count".into()))?.len();
    let attrs = attr_units(d, &toks);
    if doc.units.len() != n_body + attrs.len() {
        return Err(Error::Write(format!(
            "the document has {} units but its page holds {}; open the file again",
            doc.units.len(),
            n_body + attrs.len()
        )));
    }
    // attribute values first: only values change, so the walk below sees the same structure
    let mut edits: std::collections::BTreeMap<usize, Vec<(usize, usize, String)>> = std::collections::BTreeMap::new();
    let mut strict: Option<Strict> = None;
    for (j, a) in attrs.iter().enumerate() {
        let Some(t) = &targets[n_body + j] else { continue };
        let new = target_plain(t);
        if new == a.text {
            continue;
        }
        let strict = strict.get_or_insert_with(|| attr_strictness(&toks));
        let mut val = html_escape(&strip_sentinels(&new));
        if d == Dialect::Jsp {
            let mut el = String::new();
            escape_el(&val, &mut el);
            val = el;
        }
        if strict(a.ti, a.start, a.end) {
            val = strict_escape(&val);
        }
        edits.entry(a.ti).or_default().push((a.start, a.end, val));
    }
    for (ti, mut spans) in edits {
        // one pass over the token (a replace per value would move the rest of a tag of thousands of values each time)
        spans.sort_by_key(|x| x.0);
        let old = toks[ti].s.to_string();
        let mut tok = String::with_capacity(old.len());
        let mut last = 0;
        for (s, e, v) in spans {
            tok.push_str(&old[last..s]);
            tok.push_str(&v);
            last = e;
        }
        tok.push_str(&old[last..]);
        toks[ti].s = Cow::Owned(tok);
    }
    // body units, walked on the edited tokens
    let mut repl: Vec<(usize, usize, String)> = Vec::new();
    {
        let body = html::iter_units(&toks, None).map_err(|_| Error::Write("unit count".into()))?;
        if body.len() != n_body {
            return Err(Error::Write("the page changed shape while its attributes were written".into()));
        }
        for (k, u) in body.iter().enumerate() {
            if let Some(t) = &targets[k] {
                let mat = materialize(d, t, &u.map, &m, &text);
                repl.push((u.start, u.end, format!("{}{}{}", u.lead, mat, u.trail)));
            }
        }
    }
    for (s, e, v) in repl {
        toks[s].s = Cow::Owned(v);
        for t in &mut toks[s + 1..=e] {
            t.s = Cow::Borrowed("");
        }
    }
    let out: String = toks.iter().map(|t| t.s.as_ref()).collect();
    // a browser-equivalent re-parse must find only the source's elements and attributes
    if before != out {
        let sources = browser::browser_views(&before);
        // LocOS `check_build(before, out)`: the browser views, then the XML cross-check
        if let Err(u) = browser::check_build(&before, &out, &sources, true) {
            return Err(Error::Write(u.message()));
        }
    }
    enc::encode_text_with(&m.unmask(&out, &text), encoding, opts)
}

// --- the filters --------------------------------------------------------------------------------------------

fn head_text(bytes: &[u8]) -> String {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(&b[..b.len().min(4096)]).into_owned()
}

fn sniff(d: Dialect, bytes: &[u8]) -> bool {
    let h = head_text(bytes);
    let l = h.to_ascii_lowercase();
    match d {
        Dialect::Phtml => l.contains("<?php") || l.contains("<?="),
        Dialect::Jsp => l.contains("<%@ page") || l.contains("<%@page") || l.contains("<%@ taglib") || l.contains("<jsp:") || l.contains("<%@ include"),
        Dialect::Aspx => {
            ["<%@ page", "<%@ control", "<%@ master", "<%@ register", "<%@ import", "<%@ webservice", "<%@ implements"]
                .iter()
                .any(|k| l.contains(k))
                && !l.contains("language=\"java\"")
        }
        Dialect::Shtml => l.contains("<!--#include") || l.contains("<!--#echo") || l.contains("<!--#config") || l.contains("<!--#exec") || l.contains("<!--#set"),
        Dialect::Erb => l.contains("<%=") && !l.contains("<%@"),
    }
}

macro_rules! dialect_filter {
    ($(#[$m:meta])* $name:ident, $d:expr) => {
        $(#[$m])*
        pub struct $name;

        impl $name {
            /// `extract` with explicit size limits.
            pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
                extract_with($d, bytes, opts, limits)
            }
        }

        impl Filter for $name {
            fn id(&self) -> &'static str {
                $d.id()
            }
            fn extensions(&self) -> &'static [&'static str] {
                $d.extensions()
            }
            fn sniff(&self, bytes: &[u8]) -> bool {
                sniff($d, bytes)
            }
            fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
                bilingua_model::check_own_options(self, &opts.format_options)?;
                extract_with($d, bytes, opts, &opts.limits())
            }
            fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
                bilingua_model::check_own_options(self, &opts.format_options)?;
                merge_with($d, doc, opts)
            }
        }
    };
}

dialect_filter!(
    /// PHP templates (`.phtml`; also a `.php` page that is not a language array).
    Phtml,
    Dialect::Phtml
);
dialect_filter!(
    /// JSP (`.jsp`, `.jspf`).
    Jsp,
    Dialect::Jsp
);
dialect_filter!(
    /// ASP.NET Web Forms (`.aspx`, `.ascx`).
    Aspx,
    Dialect::Aspx
);
dialect_filter!(
    /// ERB templates (`.erb`).
    Erb,
    Dialect::Erb
);
dialect_filter!(
    /// Server-side includes (`.shtml`, `.shtm`, `.stm`).
    Shtml,
    Dialect::Shtml
);

// --- .html with server code, and .php routing (LocOS #672) -----------------------------------------------------

/// The dialect for an `.html`/`.htm` page that holds server code, else `None`: a CLOSED PHP island (`<?php ... ?>`,
/// `<?= ... ?>`) opens as PHP; a closed `<% ... %>` opens as JSP when the page says so (a taglib, a `<jsp:` element
/// or `language="java"`), otherwise as ASP.NET. An opener with no closer is ordinary text. A page that mixes PHP and
/// `<% %>` is refused. The facade routes `.html` through this (LocOS `server_page_for_html`).
pub fn server_page_for_html(text: &str) -> Result<Option<Dialect>, Error> {
    fn closed_after(text: &str, open: impl Fn(&str, usize) -> Option<usize>, closer: &str) -> bool {
        open(text, 0).is_some_and(|e| text[e..].contains(closer))
    }
    let php_open = |t: &str, from: usize| -> Option<usize> {
        let mut i = from;
        while let Some(p) = find_bytes(t.as_bytes(), i, b"<?") {
            let a = p + 2;
            let tb = t.as_bytes();
            if tb.get(a) == Some(&b'=') {
                return Some(a + 1);
            }
            if tb.len() >= a + 3 && tb[a..a + 3].eq_ignore_ascii_case(b"php") && t[a + 3..].chars().next().is_some_and(is_space) {
                return Some(a + 3);
            }
            i = p + 1;
        }
        None
    };
    let percent_open = |t: &str, from: usize| find_bytes(t.as_bytes(), from, b"<%").map(|p| p + 2);
    let php = closed_after(text, php_open, "?>");
    let percent = closed_after(text, percent_open, "%>");
    if php && percent {
        let p = php_open(text, 0).unwrap_or(0);
        return Err(refused(
            Dialect::Phtml,
            text,
            p.saturating_sub(2),
            "this .html file mixes PHP code and <% %> code",
            "Upload it with its server-page extension (.php, .jsp, .aspx or .erb).",
        ));
    }
    if php {
        return Ok(Some(Dialect::Phtml));
    }
    if percent {
        let l = text.to_ascii_lowercase();
        let jsp = l.contains("<jsp:") || {
            let mut hint = false;
            let mut from = 0;
            while let Some(p) = find_bytes(l.as_bytes(), from, b"<%@") {
                let rest = trim_start(&l[p + 3..]);
                if rest.starts_with("taglib") && !rest[6..].starts_with(bilingua_model::py_is_word_char) {
                    hint = true;
                    break;
                }
                // `<%@\s*page\b[^%]*?\blanguage\s*=\s*["']?java\b`
                let word = bilingua_model::py_is_word_char;
                if rest.starts_with("page") && !rest[4..].starts_with(word) {
                    let end = rest.find('%').unwrap_or(rest.len());
                    let seg = &rest[..end];
                    for (q, _) in seg.match_indices("language") {
                        if seg[..q].chars().next_back().is_some_and(word) {
                            continue;
                        }
                        let after = trim_start(&seg[q + 8..]);
                        if let Some(v) = after.strip_prefix('=') {
                            let v = trim_start(v);
                            let v = v.strip_prefix(['"', '\'']).unwrap_or(v);
                            if v.starts_with("java") && !v[4..].starts_with(word) {
                                hint = true;
                                break;
                            }
                        }
                    }
                    if hint {
                        break;
                    }
                }
                from = p + 3;
            }
            hint
        };
        return Ok(Some(if jsp { Dialect::Jsp } else { Dialect::Aspx }));
    }
    Ok(None)
}

/// LocOS `_has_page_text`: true when a `.php` file holds page text outside its PHP islands (otherwise it is PHP
/// code, not a template). The facade uses it to route `.php` between the language-array filter and [`Phtml`].
pub fn php_has_page_text(bytes: &[u8]) -> Result<bool, Error> {
    let dec = enc::decode_text(bytes, &enc::DecodeOptions::with_limits(Limits::default()).declared())?;
    let text = dec.text;
    let islands = lex::php_islands(&text).map_err(|r| refused_lex(Dialect::Phtml, &text, r))?;
    let mut pos = 0;
    for (s, e, _) in islands {
        if !trim(&text[pos..s]).is_empty() {
            return Ok(true);
        }
        pos = e;
    }
    Ok(!trim(&text[pos..]).is_empty())
}
