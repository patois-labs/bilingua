//! Writing a translated unit back: tags to their markup, a line break folded to a space, a table separator in the
//! translated words escaped, and any wiki markup the translated words would create written inert (inside
//! `<nowiki>`), so a translation can never add a template, a category, a link, raw HTML, a list or a redirect.
//!
//! Beyond the LocOS filter (gate): markup that would form across a tag edge (`{` before a template, `'` after an
//! italic close, `[` before a bare URL) is broken with `<nowiki>`, the line-start set also covers table (`| ! } +`)
//! and DokuWiki quote (`>`) lines, a header cell escapes `!!`, a definition term escapes `:`, a DokuWiki cell that
//! would read `:::` is kept literal, a DokuWiki bare URL is ended before translated words that would extend it, and
//! a separator inside our own `<nowiki>` is left as it is (it is already inert there).

use crate::lex::*;
use bilingua_model::{CodeData, Inline, TagKind};
use std::collections::BTreeMap;

const LINE_START_MW: &str = "*#:;=-{ |!}+";
const LINE_START_DW: &str = "*->^|= ";

/// Length of the live-markup match at `i`, if any, and how far the pattern looks (a look-ahead counts).
/// MediaWiki: `[[ ]] {{ }} {| |} '{2,} ~{3,} __WORD__ <x [http:`.
fn live_mw(s: &str, i: usize) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let rest = &b[i..];
    for lit in [&b"[["[..], b"]]", b"{{", b"}}", b"{|", b"|}"] {
        if rest.starts_with(lit) {
            return Some((2, 2));
        }
    }
    match rest[0] {
        b'\'' if rest.get(1) == Some(&b'\'') => {
            let n = rest.iter().take_while(|c| **c == b'\'').count();
            Some((n, n))
        }
        b'~' => {
            let n = rest.iter().take_while(|c| **c == b'~').count();
            (n >= 3).then_some((n, n))
        }
        // A behaviour switch `__WORD__` (case-insensitive in MediaWiki: `__notoc__` works). Any `__` before an ASCII
        // letter or digit is written inert: that breaks every switch, keeps the words text and needs no scan for
        // the closing `__` (a scan for it at every `__` would be quadratic on a long `[A-Za-z0-9_]` run).
        b'_' if rest.get(1) == Some(&b'_') => rest.get(2).is_some_and(|c| c.is_ascii_alphanumeric()).then_some((2, 3)),
        b'<' => rest.get(1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'/' || *c == b'!').then_some((1, 2)),
        b'[' => {
            let r = &rest[1..];
            ["http:", "https:", "ftp:", "mailto:", "//"].iter().find(|p| r.starts_with(p.as_bytes())).map(|p| (1, 1 + p.len()))
        }
        c if c.is_ascii_alphabetic() => free_link(s, i, MW_PROTOCOLS),
        _ => None,
    }
}

/// MediaWiki's default `$wgUrlProtocols`: a bare URL with one of these becomes a link (a free external link).
const MW_PROTOCOLS: &[&str] = &[
    "bitcoin:", "ftp://", "ftps://", "geo:", "git://", "gopher://", "http://", "https://", "irc://", "ircs://", "magnet:", "mailto:", "matrix:",
    "mms://", "news:", "nntp://", "redis://", "sftp://", "sip:", "sips:", "sms:", "ssh://", "svn://", "tel:", "telnet://", "urn:",
    "worldwind://", "xmpp:",
];

/// DokuWiki's default schemes (`conf/scheme.conf`) and its `www.` / `ftp.` short forms.
const DW_PROTOCOLS: &[&str] =
    &["http://", "https://", "telnet://", "gopher://", "wais://", "ftp://", "ed2k://", "irc://", "ldap://", "news:", "www.", "ftp."];

/// A protocol followed by a URL character: the translated words would become a link. The
/// protocol is written inert, which breaks the link.
fn free_link(s: &str, i: usize, protocols: &[&str]) -> Option<(usize, usize)> {
    // no word-start test: a character before it may itself be written inert, and the wrapper is a word break
    let rest = &s.as_bytes()[i..];
    let p = protocols.iter().find(|p| rest.len() > p.len() && rest[..p.len()].eq_ignore_ascii_case(p.as_bytes()))?;
    let next = s[i + p.len()..].chars().next()?;
    (!is_space(next) && !matches!(next, '[' | ']' | '<' | '>' | '"' | '|')).then_some((p.len(), p.len() + 1))
}

/// DokuWiki: `[[ ]] {{ }} (( )) ** (?<!:)// __ '' ~~ %% \\ <x`.
fn live_dw(s: &str, i: usize) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let rest = &b[i..];
    for lit in [&b"[["[..], b"]]", b"{{", b"}}", b"((", b"))", b"**"] {
        if rest.starts_with(lit) {
            return Some((2, 2));
        }
    }
    // `//` after a colon is italic too when no URL claims it (`a://b`); a URL in the words is caught by its scheme
    if rest.starts_with(b"//") {
        return Some((2, 2));
    }
    for lit in [&b"__"[..], b"''", b"~~", b"%%", b"\\\\"] {
        if rest.starts_with(lit) {
            return Some((2, 2));
        }
    }
    if rest[0] == b'<' {
        return rest.get(1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'/' || *c == b'!').then_some((1, 2));
    }
    if rest[0].is_ascii_alphabetic() {
        return free_link(s, i, DW_PROTOCOLS);
    }
    None
}

fn live(s: &str, i: usize, dw: bool) -> Option<(usize, usize)> {
    if dw {
        live_dw(s, i)
    } else {
        live_mw(s, i)
    }
}

/// Whether live markup would form across the edge between `left` and `right` (it starts in `left` and reaches into
/// `right`). Only the few bytes next to the edge can take part: every pattern is short or a run of one character.
fn crosses(left: &str, right: &str, dw: bool) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }
    let mut a = left.len().saturating_sub(48);
    while !left.is_char_boundary(a) {
        a += 1;
    }
    let mut z = right.len().min(48);
    while !right.is_char_boundary(z) {
        z -= 1;
    }
    let tail = &left[a..];
    let w = format!("{tail}{}", &right[..z]);
    // a run of quotes or tildes that starts further left still crosses: the scan starts at each byte of the tail
    tail.char_indices().any(|(j, _)| live(&w, j, dw).is_some_and(|(_, reach)| j + reach > tail.len()))
}

/// The plain translated words of one piece, with `edge_start` / `edge_end` asking for the first / last character to
/// be written inert (markup would form across the tag next to it).
struct Piece<'a> {
    s: &'a str,
    at_start: bool,
    edge_start: bool,
    edge_end: bool,
    /// Characters that would end the label this piece sits in: `]` in an external link label, `|` in a
    /// MediaWiki link label (a file link takes its last part as the caption), `{` `}` in a DokuWiki media caption.
    enders: String,
}

fn escape_char(out: &mut String, c: char, cell: Option<&str>, kind: Option<&str>, dw: bool) {
    match (cell, c) {
        (Some("tpl"), '|') | (Some("mw"), '|') => out.push_str("{{!}}"),
        (Some("tpl"), '=') => out.push_str("{{=}}"),
        // a single brace would pair with the template's own braces when the page is read again
        (Some("tpl"), '{' | '}') => {
            out.push_str("<nowiki>");
            out.push(c);
            out.push_str("</nowiki>");
        }
        (Some("dw"), '|') => out.push_str("%%|%%"),
        (Some("dw"), '^') => out.push_str("%%^%%"),
        // a colon would end the term early; an open bracket would hide the colon that ends it
        (_, ':' | '[' | ']') if !dw && kind == Some("term") => {
            out.push_str("<nowiki>");
            out.push(c);
            out.push_str("</nowiki>");
        }
        _ => out.push(c),
    }
}

fn text_part(p: &Piece, cell: Option<&str>, kind: Option<&str>, dw: bool) -> String {
    let s = fold_breaks(p.s);
    let s = s.as_str();
    let mut out = String::with_capacity(s.len() + 16);
    let b = s.as_bytes();
    let starts = if dw { LINE_START_DW } else { LINE_START_MW };
    let last = s.char_indices().next_back().map(|(i, _)| i);
    let mut i = 0;
    while i < b.len() {
        let c = s[i..].chars().next().unwrap_or(' ');
        if (i == 0 && p.edge_start) || (Some(i) == last && p.edge_end) {
            out.push_str("<nowiki>");
            out.push(c);
            out.push_str("</nowiki>");
            i += c.len_utf8();
            continue;
        }
        let end = if p.edge_end { last.unwrap_or(b.len()) } else { b.len() };
        // a match that runs into the last character (written inert on its own) is cut there, but still inert
        if let Some(n) = live(s, i, dw).map(|(n, _)| n.min(end - i)).filter(|n| *n > 0) {
            out.push_str("<nowiki>");
            out.push_str(&s[i..i + n]);
            out.push_str("</nowiki>");
            i += n;
            continue;
        }
        if i == 0 && p.at_start && starts.contains(c) {
            out.push_str("<nowiki>");
            out.push(c);
            out.push_str("</nowiki>");
            i += c.len_utf8();
            continue;
        }
        if p.enders.contains(c) {
            out.push_str("<nowiki>");
            out.push(c);
            out.push_str("</nowiki>");
            i += c.len_utf8();
            continue;
        }
        if !dw && kind == Some("header-cell") && s[i..].starts_with("!!") {
            out.push_str("<nowiki>!!</nowiki>");
            i += 2;
            continue;
        }
        escape_char(&mut out, c, cell, kind, dw);
        i += c.len_utf8();
    }
    out
}

/// `[ \t]*(\r\n|\r|\n)[ \t]*` folded to one space.
fn fold_breaks(s: &str) -> String {
    if !s.contains(['\r', '\n']) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut floor = 0;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\r' || c == '\n' {
            if c == '\r' && it.peek() == Some(&'\n') {
                it.next();
            }
            while out.len() > floor && out.ends_with([' ', '\t']) {
                out.pop();
            }
            while matches!(it.peek(), Some(' ' | '\t')) {
                it.next();
            }
            out.push(' ');
            floor = out.len();
        } else {
            out.push(c);
        }
    }
    out
}

/// `(?:https?|ftps?)://[^\s\[\]<>"\x00]+` matching the whole string.
fn bare_url(v: &str) -> bool {
    let Some(r) = ["https://", "http://", "ftps://", "ftp://"].iter().find_map(|p| v.strip_prefix(p)) else { return false };
    !r.is_empty() && !r.chars().any(|c| is_space(c) || matches!(c, '[' | ']' | '<' | '>' | '"' | '\0'))
}

/// Whether the text that follows piece `i` starts with the run of a MediaWiki URL (anything before the first
/// space, bracket, quote or control character, once trailing `,;.:!?)` are dropped).
fn url_continues(out: &[String], i: usize) -> bool {
    let trim = |c: char| matches!(c, ',' | ';' | '.' | ':' | '!' | '?' | ')');
    for piece in &out[i + 1..] {
        for c in piece.chars() {
            let stop = is_space(c) || matches!(c, '[' | ']' | '<' | '>' | '"' | '\u{7f}' | '\u{fffd}') || (c as u32) <= 0x20;
            if stop {
                return false;
            }
            if !trim(c) {
                return true;
            }
        }
    }
    false
}

/// DokuWiki: whether the words after piece `i` would run on as part of the bare URL before them. The reader's own
/// URL class decides (anything up to a space, a bracket, a quote or `|`), so the merged file reads back with the
/// same URL; DokuWiki itself stops earlier, where `<nowiki></nowiki>` is harmless.
fn dw_url_continues(out: &[String], i: usize) -> bool {
    out[i + 1..]
        .iter()
        .find_map(|p| p.chars().next())
        .is_some_and(|c| !(is_space(c) || matches!(c, '[' | ']' | '<' | '>' | '"' | '|' | '\u{0}')))
}

fn separate_urls(out: &mut [String], urls: &[usize]) {
    for &i in urls {
        if url_continues(out, i) {
            out[i].push_str("<nowiki/>");
        }
        let before = out[..i].iter().rev().find_map(|p| p.chars().next_back());
        if before.is_some_and(|c| is_alnum(c) || c == '_') {
            out[i] = format!("<nowiki/>{}", out[i]);
        }
    }
}

enum Seg {
    Text(String),
    Code(String),
    Open(String),
    Close(String),
}

/// Renders a translated unit (its output inlines) to wiki markup. `kind` is the unit's `context.kind`.
pub fn render(new: &[Inline], codes: &BTreeMap<String, CodeData>, cell: Option<&str>, kind: Option<&str>, dw: bool) -> String {
    let mut items: Vec<Inline> = new.to_vec();
    // strip the white space at both ends of the whole translation
    if let Some(Inline::Text(t)) = items.first_mut() {
        *t = t.trim_start_matches(is_space).to_string();
    }
    if let Some(Inline::Text(t)) = items.last_mut() {
        *t = t.trim_end_matches(is_space).to_string();
    }
    // text and code pieces in order; a text piece always sits between two codes (maybe empty)
    let mut segs: Vec<Seg> = Vec::new();
    let mut pending = String::new();
    for it in &items {
        match it {
            Inline::Text(t) => pending.push_str(t),
            Inline::Tag { id, kind, .. } => {
                segs.push(Seg::Text(std::mem::take(&mut pending)));
                let Some(cd) = codes.get(id) else { continue };
                let closing = *kind == TagKind::Close;
                if cd.open.is_some() || cd.close.is_some() {
                    if closing {
                        segs.push(Seg::Close(cd.close.clone().unwrap_or_default()));
                    } else {
                        segs.push(Seg::Open(cd.open.clone().unwrap_or_default()));
                    }
                } else if !closing {
                    segs.push(Seg::Code(cd.standalone.clone().unwrap_or_default()));
                }
            }
        }
    }
    segs.push(Seg::Text(pending));
    // the code markup next to each text piece (codes with empty markup are skipped)
    let near = |j: usize, back: bool| -> Option<&str> {
        let mut k = j;
        loop {
            k = if back { k.checked_sub(1)? } else { k + 1 };
            match segs.get(k)? {
                Seg::Code(c) | Seg::Open(c) | Seg::Close(c) if !c.is_empty() => return Some(c.as_str()),
                Seg::Code(_) | Seg::Open(_) | Seg::Close(_) => continue,
                Seg::Text(t) if t.is_empty() => continue,
                Seg::Text(_) => return None,
            }
        }
    };
    let mut out: Vec<String> = Vec::new();
    let mut urls = Vec::new();
    let mut dw_urls = Vec::new();
    let mut breaks = Vec::new();
    // the labels open around the current piece (their opening markup)
    let mut open: Vec<&str> = Vec::new();
    let mut in_tpl = false;
    for (j, sg) in segs.iter().enumerate() {
        match sg {
            Seg::Text(t) => {
                let folded = fold_breaks(t);
                // the skeleton after a header cell can be `!!`: a last `!` would make it `!!!`
                let header_end = j + 1 == segs.len() && kind == Some("header-cell") && folded.ends_with('!');
                let p = Piece {
                    s: t,
                    at_start: out.is_empty(),
                    edge_start: near(j, true).is_some_and(|c| crosses(c, &folded, dw)),
                    edge_end: header_end || near(j, false).is_some_and(|c| crosses(&folded, c, dw)),
                    enders: open.iter().fold(String::new(), |mut acc, o| {
                        let e = if o.starts_with("[[") && !dw {
                            "|"
                        } else if o.starts_with('[') && !o.starts_with("[[") {
                            "]"
                        } else if o.starts_with("{{") && dw {
                            "{}"
                        } else {
                            ""
                        };
                        acc.push_str(e);
                        acc
                    }),
                };
                // an allow-listed template parameter translated in place sits between `{{Name|key=` and `}}`
                let cell = if in_tpl && !dw { Some("tpl") } else { cell };
                out.push(text_part(&p, cell, kind, dw));
            }
            Seg::Code(v) | Seg::Open(v) | Seg::Close(v) => {
                match sg {
                    Seg::Open(o) => open.push(o.as_str()),
                    Seg::Close(_) => {
                        open.pop();
                    }
                    _ => {
                        if v.starts_with("{{") && !v.ends_with("}}") {
                            in_tpl = true;
                        } else if v.ends_with("}}") && !v.starts_with("{{") {
                            in_tpl = false;
                        }
                    }
                }
                if !dw && bare_url(v) {
                    urls.push(out.len());
                }
                if dw && bare_url(v) {
                    dw_urls.push(out.len());
                }
                if dw && v == "\\\\" {
                    breaks.push(out.len());
                }
                out.push(v.clone());
            }
        }
    }
    for i in breaks {
        let nxt = out[i + 1..].iter().find_map(|p| p.chars().next());
        if nxt.is_some_and(|c| !is_space(c)) {
            out[i].push(' ');
        }
    }
    separate_urls(&mut out, &urls);
    for i in dw_urls {
        if dw_url_continues(&out, i) {
            out[i].push_str("<nowiki></nowiki>");
        }
    }
    let joined = out.concat();
    // a DokuWiki cell that reads `:::` would join the cell above
    if cell == Some("dw") && strip(&joined) == ":::" {
        return "%%:::%%".to_string();
    }
    joined
}
