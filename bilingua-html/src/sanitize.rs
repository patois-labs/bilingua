//! `sanitize_html`: HTML for a browser to show (the preview, and any other caller that shows translated HTML), cut
//! down to an **allowlist**: the elements a translator needs to see (text formatting, headings, lists, tables, links,
//! images, simple form controls, style sheets), a short list of attributes per element, and links and images only to
//! http, https, mailto and tel addresses (images also to `data:image/...`, not SVG).
//!
//! Everything else is left out: scripts, SVG and MathML (with their content), frames, objects, embeds, templates,
//! forms, `<base>`, `<link>`, `<meta http-equiv>`, event handlers, `srcdoc`, and any URL with another scheme. Run 2 of the
//! security review (G3-8) showed the old blocklist could be passed through an SVG animation value or a meta
//! refresh written with character references; an allowlist has no list of bad things to keep complete.
//!
//! Tags are **rebuilt**, not copied: the element name from the allowlist, each kept attribute once (the first, as a
//! browser takes it), with its value decoded once and written back escaped. What is checked is then exactly what the
//! browser reads. The rest keeps the invariant of the BILI-73 rewrite: every `<` in the output starts a tag this pass
//! wrote (or the doctype, or sits in a style sheet where it cannot start a tag). Text gets `&lt;` for `<`, comments
//! and declarations are left out. With no SVG or MathML in the output, a browser never reads any of it as foreign
//! content.

use crate::tok::*;
use bilingua_htmlcore::pyx::unescape;

const PREVIEW_STYLE: &str = "<style>[data-bl-seg]{outline:none}[data-bl-seg].bl-active{outline:2px solid #3b82f6;outline-offset:2px}</style>";

/// The elements that are written (rebuilt), with the attributes each may keep besides the global ones.
const ELEMENTS: &[(&str, &[&str])] = &[
    ("html", &[]),
    ("head", &[]),
    ("body", &["bgcolor", "text", "link", "vlink", "alink"]),
    ("title", &[]),
    // a meta element only as a name and its content (a page description the preview marks as a segment), never
    // with `http-equiv`: an element that has one is left out whole
    ("meta", &["name", "content", "charset"]),
    ("style", &["media"]),
    // sections and blocks
    ("address", &[]),
    ("article", &[]),
    ("aside", &[]),
    ("footer", &[]),
    ("header", &[]),
    ("main", &[]),
    ("nav", &[]),
    ("section", &[]),
    ("hgroup", &[]),
    ("h1", &["align"]),
    ("h2", &["align"]),
    ("h3", &["align"]),
    ("h4", &["align"]),
    ("h5", &["align"]),
    ("h6", &["align"]),
    ("p", &["align"]),
    ("div", &["align"]),
    ("center", &[]),
    ("blockquote", &["cite"]),
    ("pre", &[]),
    ("hr", &["align", "size", "width", "noshade"]),
    ("br", &["clear"]),
    ("figure", &[]),
    ("figcaption", &[]),
    ("details", &["open"]),
    ("summary", &[]),
    // lists
    ("ul", &["type"]),
    ("ol", &["start", "type", "reversed"]),
    ("li", &["value", "type"]),
    ("dl", &[]),
    ("dt", &[]),
    ("dd", &[]),
    ("menu", &[]),
    // text
    ("a", &["href", "name", "rel", "hreflang"]),
    ("abbr", &[]),
    ("acronym", &[]),
    ("b", &[]),
    ("bdi", &[]),
    ("bdo", &[]),
    ("big", &[]),
    ("cite", &[]),
    ("code", &[]),
    ("data", &["value"]),
    ("del", &["cite", "datetime"]),
    ("dfn", &[]),
    ("em", &[]),
    ("font", &["color", "face", "size"]),
    ("i", &[]),
    ("ins", &["cite", "datetime"]),
    ("kbd", &[]),
    ("mark", &[]),
    ("nobr", &[]),
    ("q", &["cite"]),
    ("rp", &[]),
    ("rt", &[]),
    ("ruby", &[]),
    ("s", &[]),
    ("samp", &[]),
    ("small", &[]),
    ("span", &[]),
    ("strike", &[]),
    ("strong", &[]),
    ("sub", &[]),
    ("sup", &[]),
    ("time", &["datetime"]),
    ("tt", &[]),
    ("u", &[]),
    ("var", &[]),
    ("wbr", &[]),
    // images
    ("img", &["src", "alt", "width", "height", "border", "align", "hspace", "vspace"]),
    // tables
    ("table", &["border", "cellpadding", "cellspacing", "width", "align", "bgcolor", "summary", "frame", "rules"]),
    ("caption", &["align"]),
    ("colgroup", &["span", "width", "align", "valign"]),
    ("col", &["span", "width", "align", "valign"]),
    ("thead", &["align", "valign", "bgcolor"]),
    ("tbody", &["align", "valign", "bgcolor"]),
    ("tfoot", &["align", "valign", "bgcolor"]),
    ("tr", &["align", "valign", "bgcolor", "height"]),
    ("td", &["colspan", "rowspan", "headers", "scope", "abbr", "align", "valign", "width", "height", "bgcolor", "nowrap"]),
    ("th", &["colspan", "rowspan", "headers", "scope", "abbr", "align", "valign", "width", "height", "bgcolor", "nowrap"]),
    // form controls, shown but with nowhere to send to (no form, and no form or formaction attribute)
    ("label", &["for"]),
    ("fieldset", &["disabled"]),
    ("legend", &[]),
    ("button", &["type", "name", "value", "disabled"]),
    ("input", &["type", "name", "value", "placeholder", "checked", "disabled", "readonly", "size", "maxlength"]),
    ("select", &["name", "multiple", "size", "disabled"]),
    ("optgroup", &["label", "disabled"]),
    ("option", &["value", "label", "selected", "disabled"]),
    ("textarea", &["name", "rows", "cols", "placeholder", "disabled", "readonly"]),
];

/// Attributes any written element may keep (plus `aria-*` and `data-*`).
const GLOBAL_ATTRS: &[&str] = &["id", "class", "title", "lang", "xml:lang", "dir", "style", "translate", "role", "hidden"];

/// Elements left out together with everything inside them, up to their end tag.
const DROP_WITH_CONTENT: &[&str] = &["script", "svg", "math", "template", "iframe", "object", "applet", "noembed", "noframes", "xmp"];

/// Allowed elements that have no end tag.
const VOID: &[&str] = &["br", "hr", "img", "input", "col", "wbr", "meta"];

/// Image types a `data:` URL of an image may have.
const DATA_IMAGES: &[&str] = &["png", "jpeg", "jpg", "gif", "webp", "bmp", "avif", "x-icon"];

fn element(name: &str) -> Option<(&'static str, &'static [&'static str])> {
    ELEMENTS.iter().find(|(n, _)| *n == name).copied()
}

/// A name made only of ASCII lowercase letters, digits, `-`, `_` and `.` after the given prefix.
fn prefixed(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|r| !r.is_empty() && r.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'_' | b'.')))
}

fn allowed_attr(elem_attrs: &[&str], name: &str) -> bool {
    GLOBAL_ATTRS.contains(&name) || elem_attrs.contains(&name) || prefixed(name, "aria-") || prefixed(name, "data-")
}

/// A decoded URL with every blank and control character taken out, lowercase: a browser drops tabs and line breaks
/// anywhere in a URL and controls and spaces at its ends, so this holds whatever scheme a browser could read.
fn squeezed_url(decoded: &str) -> String {
    decoded.chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect::<String>().to_ascii_lowercase()
}

/// The scheme of a URL: what comes before a `:` that comes before any `/`, `?` or `#`. `None` for a relative URL.
fn scheme(squeezed: &str) -> Option<&str> {
    let end = squeezed.find([':', '/', '?', '#'])?;
    (squeezed.as_bytes()[end] == b':').then(|| &squeezed[..end])
}

/// Whether a decoded URL may be kept: relative, or http or https; for links also mailto and tel, for images also
/// `data:image/...` of a raster type.
fn url_ok(decoded: &str, image: bool) -> bool {
    let s = squeezed_url(decoded);
    match scheme(&s) {
        None | Some("http") | Some("https") => true,
        Some("mailto") | Some("tel") => !image,
        Some("data") if image => s
            .strip_prefix("data:image/")
            .is_some_and(|r| DATA_IMAGES.iter().any(|t| r.strip_prefix(t).is_some_and(|rest| rest.starts_with([';', ','])))),
        _ => false,
    }
}

/// CSS as a style sheet reads it: comments taken out and escapes decoded, lowercase.
fn css_plain(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        if c[i] == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i < c.len() && !(c[i] == '*' && c.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c[i] == '\\' {
            i += 1;
            let start = i;
            while i < c.len() && i - start < 6 && c[i].is_ascii_hexdigit() {
                i += 1;
            }
            if i > start {
                let v = c[start..i].iter().fold(0u32, |a, d| a * 16 + d.to_digit(16).unwrap_or(0));
                out.push(char::from_u32(v).filter(|&x| x != '\0').unwrap_or('\u{FFFD}'));
                if i < c.len() && c[i].is_whitespace() {
                    i += 1;
                }
            } else if i < c.len() {
                if c[i] != '\n' {
                    out.push(c[i]);
                }
                i += 1;
            }
            continue;
        }
        out.push(c[i]);
        i += 1;
    }
    out.to_lowercase()
}

/// CSS that holds nothing that runs code in any browser, read after comments and escapes: no `expression(`, no
/// script URL, no `-moz-binding` and no `behavior` property.
fn css_ok(s: &str) -> bool {
    let p: String = css_plain(s).chars().filter(|c| !c.is_whitespace()).collect();
    if ["expression(", "javascript:", "vbscript:", "-moz-binding"].iter().any(|w| p.contains(w)) {
        return false;
    }
    // `behavior:` as a property of its own (IE), not the end of `scroll-behavior`
    for w in ["behavior:", "behaviour:"] {
        for (i, _) in p.match_indices(w) {
            if !p[..i].ends_with(|c: char| c.is_alphanumeric() || c == '-' || c == '_') {
                return false;
            }
        }
    }
    true
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

/// A style sheet: raw text in HTML, kept as typed (`<!--` wrappers included), but with every `<` that could start a
/// tag written as the CSS escape `\3c `, which means the same to a style sheet. A sheet that fails [`css_ok`] is
/// left out.
fn push_css(out: &mut String, s: &str) {
    if !css_ok(s) {
        return;
    }
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

/// An attribute value, written between double quotes.
fn push_value(out: &mut String, v: &str) {
    for c in v.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

/// The start tag of an allowed element, rebuilt from the attributes it may keep.
fn start_tag(out: &mut String, html: &str, name: &str, elem_attrs: &[&str], attrs: &[Attr]) {
    out.push('<');
    out.push_str(name);
    let mut seen: Vec<&str> = Vec::new();
    for at in attrs {
        let an = at.name.as_str();
        if seen.contains(&an) {
            continue; // a browser keeps the first of two attributes with one name
        }
        seen.push(an);
        if !allowed_attr(elem_attrs, an) {
            continue;
        }
        let value = at.val.map(|(x, y)| unescape(&html[x..y]));
        let ok = match (an, value.as_deref()) {
            ("href", Some(v)) => url_ok(v, false),
            ("src", Some(v)) => url_ok(v, true),
            ("cite", Some(v)) => matches!(scheme(&squeezed_url(v)), None | Some("http") | Some("https")),
            ("style", Some(v)) => css_ok(v),
            ("href" | "src" | "cite", None) => false,
            _ => true,
        };
        if !ok {
            continue;
        }
        out.push(' ');
        out.push_str(an);
        if let Some(v) = value {
            out.push_str("=\"");
            push_value(out, &v);
            out.push('"');
        }
    }
    out.push('>');
}

/// Cuts HTML down to the allowlist of this module (see the module notes). With `inject`, also adds the preview
/// `<style>`.
///
/// Comments, processing instructions and CDATA markers are left out (the text of a CDATA section stays), a `<`
/// that starts no tag is written `&lt;`, tags are rebuilt (`<p class="x">`, `</p>`), and an element that is not
/// allowed is left out while its text stays (apart from those of [`DROP_WITH_CONTENT`], which go with everything in
/// them).
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
    // inside an element of DROP_WITH_CONTENT: its name, and how many more of that name are open inside it
    let mut dropping: Option<(&str, usize)> = None;
    for (i, t) in toks.iter().enumerate() {
        if insert_at == Some(i) {
            out.push_str(PREVIEW_STYLE);
        }
        let (a, b) = t.range();
        if let Some((dn, depth)) = dropping {
            match t {
                Tok::Start { name, closed: true, .. } if name == dn => dropping = Some((dn, depth + 1)),
                Tok::End { name, .. } if name == dn => dropping = depth.checked_sub(1).map(|d| (dn, d)),
                _ => {}
            }
            continue;
        }
        match t {
            Tok::Start { name, attrs, closed, .. } => {
                if let Some(d) = DROP_WITH_CONTENT.iter().copied().find(|d| *d == name.as_str()) {
                    // `<svg/>` and `<math/>` close themselves; every other element of the list runs to its end tag
                    let self_closed = matches!(d, "svg" | "math") && html[a..b].ends_with("/>");
                    if *closed && !self_closed {
                        dropping = Some((d, 0));
                    }
                    continue;
                }
                // a tag that runs into the end of the input: a browser drops it
                if !closed {
                    continue;
                }
                if name == "meta" && attrs.iter().any(|at| at.name == "http-equiv") {
                    continue;
                }
                if let Some((n, elem_attrs)) = element(name) {
                    start_tag(&mut out, html, n, elem_attrs, attrs);
                }
            }
            Tok::End { name, .. } => {
                if let Some((n, _)) = element(name) {
                    if !VOID.contains(&n) {
                        out.push_str("</");
                        out.push_str(n);
                        out.push('>');
                    }
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
