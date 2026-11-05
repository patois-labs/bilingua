//! Writing a translated unit back: tags to their markup, a line break folded to a space, a table separator in the
//! translated words escaped, and any wiki markup the translated words would create written inert (inside
//! `<nowiki>`), so a translation can never add a template, a category, a link, raw HTML, a list or a redirect.

use crate::lex::*;
use bilingua_model::{CodeData, Inline, TagKind};
use std::collections::BTreeMap;

const LINE_START_MW: &str = "*#:;=-{ ";
const LINE_START_DW: &str = "*->^|= ";

/// Length of the live-markup match at `i`, if any. MediaWiki: `[[ ]] {{ }} {| |} '{2,} ~{3,} __WORD__ <x [http:`.
fn live_mw(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    let rest = &b[i..];
    for lit in [&b"[["[..], b"]]", b"{{", b"}}", b"{|", b"|}"] {
        if rest.starts_with(lit) {
            return Some(2);
        }
    }
    match rest[0] {
        b'\'' if rest.get(1) == Some(&b'\'') => Some(rest.iter().take_while(|c| **c == b'\'').count()),
        b'~' => {
            let n = rest.iter().take_while(|c| **c == b'~').count();
            (n >= 3).then_some(n)
        }
        b'_' if rest.get(1) == Some(&b'_') => {
            let cls = |c: &u8| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b'_';
            let run: Vec<u8> = rest[2..].iter().copied().take_while(cls).collect();
            (1..run.len().saturating_sub(1)).rev().find(|&k| run[k] == b'_' && run[k + 1] == b'_').map(|k| 2 + k + 2)
        }
        b'<' => rest.get(1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'/' || *c == b'!').then_some(1),
        b'[' => {
            let r = &rest[1..];
            (r.starts_with(b"http:") || r.starts_with(b"https:") || r.starts_with(b"ftp:") || r.starts_with(b"mailto:") || r.starts_with(b"//")).then_some(1)
        }
        _ => None,
    }
}

/// DokuWiki: `[[ ]] {{ }} (( )) ** (?<!:)// __ '' ~~ %% \\ <x`.
fn live_dw(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    let rest = &b[i..];
    for lit in [&b"[["[..], b"]]", b"{{", b"}}", b"((", b"))", b"**"] {
        if rest.starts_with(lit) {
            return Some(2);
        }
    }
    if rest.starts_with(b"//") && (i == 0 || b[i - 1] != b':') {
        return Some(2);
    }
    for lit in [&b"__"[..], b"''", b"~~", b"%%", b"\\\\"] {
        if rest.starts_with(lit) {
            return Some(2);
        }
    }
    (rest[0] == b'<' && rest.get(1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'/' || *c == b'!')).then_some(1)
}

fn inert(s: &str, dw: bool, at_start: bool) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let hit = if dw { live_dw(s, i) } else { live_mw(s, i) };
        match hit {
            Some(n) => {
                out.push_str("<nowiki>");
                out.push_str(&s[i..i + n]);
                out.push_str("</nowiki>");
                i += n;
            }
            None => {
                let c = s[i..].chars().next().unwrap_or(' ');
                out.push(c);
                i += c.len_utf8();
            }
        }
    }
    let starts = if dw { LINE_START_DW } else { LINE_START_MW };
    if at_start {
        if let Some(c) = out.chars().next() {
            if starts.contains(c) {
                let rest = out[c.len_utf8()..].to_string();
                return format!("<nowiki>{c}</nowiki>{rest}");
            }
        }
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

fn text_part(s: &str, cell: Option<&str>, dw: bool, at_start: bool) -> String {
    let s = inert(&fold_breaks(s), dw, at_start);
    match cell {
        Some("tpl") => s.replace('|', "{{!}}").replace('=', "{{=}}"),
        Some("mw") => s.replace('|', "{{!}}"),
        Some("dw") => s.replace('|', "%%|%%").replace('^', "%%^%%"),
        _ => s,
    }
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

/// Renders a translated unit (its output inlines) to wiki markup.
pub fn render(new: &[Inline], codes: &BTreeMap<String, CodeData>, cell: Option<&str>, dw: bool) -> String {
    let mut items: Vec<Inline> = new.to_vec();
    // strip the white space at both ends of the whole translation
    if let Some(Inline::Text(t)) = items.first_mut() {
        *t = t.trim_start_matches(is_space).to_string();
    }
    if let Some(Inline::Text(t)) = items.last_mut() {
        *t = t.trim_end_matches(is_space).to_string();
    }
    let mut out: Vec<String> = Vec::new();
    let mut urls = Vec::new();
    let mut breaks = Vec::new();
    let mut pending = String::new();
    for it in &items {
        match it {
            Inline::Text(t) => pending.push_str(t),
            Inline::Tag { id, kind, .. } => {
                let at_start = out.is_empty();
                out.push(text_part(&std::mem::take(&mut pending), cell, dw, at_start));
                let Some(cd) = codes.get(id) else { continue };
                let closing = *kind == TagKind::Close;
                if cd.open.is_some() || cd.close.is_some() {
                    let s = if closing { cd.close.clone() } else { cd.open.clone() };
                    out.push(s.unwrap_or_default());
                } else if !closing {
                    let v = cd.standalone.clone().unwrap_or_default();
                    if !dw && bare_url(&v) {
                        urls.push(out.len());
                    }
                    if dw && v == "\\\\" {
                        breaks.push(out.len());
                    }
                    out.push(v);
                }
            }
        }
    }
    let at_start = out.is_empty();
    out.push(text_part(&pending, cell, dw, at_start));
    for i in breaks {
        let nxt = out[i + 1..].iter().find_map(|p| p.chars().next());
        if nxt.is_some_and(|c| !is_space(c)) {
            out[i].push(' ');
        }
    }
    separate_urls(&mut out, &urls);
    out.concat()
}
