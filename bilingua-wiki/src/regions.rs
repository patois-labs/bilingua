//! Protected regions: the parts of a page that are never translated and become point tags (or are skipped when a
//! whole line is made of them). MediaWiki: comments and verbatim extension tags, templates, parameters, magic words,
//! signatures, categories. DokuWiki: `<nowiki>`, `%%..%%`, `<code>`, `<file>`, `<html>`, `<php>` and `~~MACRO~~`.

use crate::lex::*;
use std::collections::{HashMap, HashSet};

pub type Region = (usize, usize);

/// The text with every protected region replaced by NUL bytes of the same length (offsets stay valid).
pub fn masked(text: &str, regions: &[Region]) -> String {
    let mut v = text.as_bytes().to_vec();
    for &(s, e) in regions {
        if let Some(r) = v.get_mut(s..e) {
            r.fill(MASK);
        }
    }
    String::from_utf8(v).unwrap_or_else(|_| "\0".repeat(text.len()))
}

/// Sorted, with any region nested inside (or overlapping the end of) the previous one dropped.
pub fn merge(mut all: Vec<Region>) -> Vec<Region> {
    all.sort_by_key(|r| (r.0, std::cmp::Reverse(r.1)));
    let mut out: Vec<Region> = Vec::with_capacity(all.len());
    for (s, e) in all {
        if out.last().is_none_or(|l| s >= l.1) {
            out.push((s, e));
        }
    }
    out
}

// ------------------------------------------------------------------------------------------ MediaWiki

const MW_VERBATIM: &[&str] = &[
    "nowiki", "pre", "code", "syntaxhighlight", "source", "math", "chem", "ce", "score", "timeline", "graph", "templatedata", "mapframe",
    "maplink", "hiero", "gallery", "imagemap", "inputbox", "categorytree", "charinsert", "html",
];

fn starts_ci(b: &[u8], i: usize, name: &str) -> bool {
    b.get(i..i + name.len()).is_some_and(|w| w.eq_ignore_ascii_case(name.as_bytes()))
}

/// `</name\s*>` (case-insensitive) at or after `from`; the end of the closing tag.
fn find_close_ci(text: &str, from: usize, name: &str) -> Option<usize> {
    let b = text.as_bytes();
    let mut i = from;
    while i + 3 < b.len() + 1 {
        let off = b.get(i..)?.iter().position(|c| *c == b'<')?;
        i += off;
        if b.get(i + 1) == Some(&b'/') && starts_ci(b, i + 2, name) {
            let j = run(text, i + 2 + name.len(), b.len(), usize::MAX, is_space);
            if b.get(j) == Some(&b'>') {
                return Some(j + 1);
            }
        }
        i += 1;
    }
    None
}

/// Comments and verbatim extension tags. An unclosed comment runs to the end (as in MediaWiki); an unclosed tag
/// protects only itself (MediaWiki shows it as text).
pub fn mw_phase1(text: &str) -> Vec<Region> {
    let b = text.as_bytes();
    let mut regs = Vec::new();
    let mut pos = 0usize;
    // once a closing tag was not found after position p, none exists after any later position either
    let mut none_after: HashMap<usize, usize> = HashMap::new();
    while pos < b.len() {
        let Some(off) = b[pos..].iter().position(|c| *c == b'<') else { break };
        let s = pos + off;
        if b[s..].starts_with(b"<!--") {
            let e = text[s + 4..].find("-->").map_or(text.len(), |p| s + 4 + p + 3);
            regs.push((s, e));
            pos = e;
            continue;
        }
        let mut hit: Option<(usize, usize, bool)> = None; // (name index, end of the opening tag, self closing)
        for (idx, name) in MW_VERBATIM.iter().enumerate() {
            if !starts_ci(b, s + 1, name) {
                continue;
            }
            let j = s + 1 + name.len();
            if char_at(text, j).is_some_and(is_word) {
                continue; // \b fails
            }
            // [^<>]{0,2000}? then /? then >
            let mut q = j;
            let mut n = 0usize;
            let mut found = None;
            loop {
                if b.get(q) == Some(&b'/') && b.get(q + 1) == Some(&b'>') {
                    found = Some((q + 2, true));
                    break;
                }
                if b.get(q) == Some(&b'>') {
                    found = Some((q + 1, false));
                    break;
                }
                if n >= 2000 || q >= b.len() || b[q] == b'<' {
                    break;
                }
                q += utf8_len(b[q]);
                n += 1;
            }
            if let Some((e, sc)) = found {
                hit = Some((idx, e, sc));
            }
            break;
        }
        match hit {
            None => pos = s + 1,
            Some((idx, e, true)) => {
                let _ = idx;
                regs.push((s, e));
                pos = e;
            }
            Some((idx, open_end, false)) => {
                let name = MW_VERBATIM[idx];
                let known_none = none_after.get(&idx).is_some_and(|p| *p <= open_end);
                let close = if known_none { None } else { find_close_ci(text, open_end, name) };
                if close.is_none() && !known_none {
                    none_after.insert(idx, open_end);
                }
                let e = close.unwrap_or(open_end);
                regs.push((s, e));
                pos = e;
            }
        }
    }
    regs
}

fn brace_match(m1: &str) -> HashMap<usize, usize> {
    let mut map = HashMap::new();
    let mut stack: Vec<usize> = Vec::new();
    for (i, c) in m1.bytes().enumerate() {
        if c == b'{' {
            stack.push(i);
        } else if c == b'}' {
            if let Some(o) = stack.pop() {
                map.insert(o, i);
            }
        }
    }
    map
}

fn find_in(m1: &str, pat: &str, from: usize, to: usize) -> Option<usize> {
    if from >= to {
        return None;
    }
    m1.get(from..to)?.find(pat).map(|p| p + from)
}

/// (key, value start, value end) of every parameter of the template at `[p, end)`, and the name.
fn tpl_params(m1: &str, mt: &HashMap<usize, usize>, p: usize, end: usize) -> (String, Vec<(String, usize, usize)>) {
    let b = m1.as_bytes();
    let (a, e) = (p + 2, end - 2);
    let mut seps = Vec::new();
    let mut i = a;
    while i < e {
        let c = b[i];
        if c == b'{' {
            if let Some(&q) = mt.get(&i) {
                i = q + 1;
                continue;
            }
        }
        if c == b'[' && b.get(i + 1) == Some(&b'[') {
            match find_in(m1, "]]", i + 2, e) {
                Some(j) => {
                    i = j + 2;
                    continue;
                }
                None => break,
            }
        }
        if c == b'|' {
            seps.push(i);
        }
        i += 1;
    }
    let name = sl(m1, a, seps.first().copied().unwrap_or(e)).to_string();
    let mut params = Vec::new();
    let mut n = 0;
    let mut bounds = seps.clone();
    bounds.push(e);
    for k in 0..seps.len() {
        let (ps, pe) = (bounds[k] + 1, bounds[k + 1]);
        let seg = sl(m1, ps, pe);
        match seg.find('=') {
            Some(eq) if !seg[..eq].contains(['{', '[', '\0']) => params.push((strip(&seg[..eq]).to_string(), ps + eq + 1, pe)),
            _ => {
                n += 1;
                params.push((n.to_string(), ps, pe));
            }
        }
    }
    (name, params)
}

pub fn norm_title(name: &str) -> String {
    let joined = name.replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ");
    let mut it = joined.chars();
    match it.next() {
        Some(c) => c.to_uppercase().collect::<String>() + it.as_str(),
        None => String::new(),
    }
}

/// Template regions (to mask) and allow-listed parameter holes (left as text).
fn mw_templates(m1: &str, allow: &HashSet<(String, String)>) -> (Vec<Region>, Vec<Region>) {
    let mt = brace_match(m1);
    let mut regs = Vec::new();
    let mut holes = Vec::new();
    scan(m1, &mt, allow, 0, m1.len(), true, &mut regs, &mut holes);
    regs.retain(|r| r.0 < r.1);
    (regs, holes)
}

#[allow(clippy::too_many_arguments)]
fn scan(m1: &str, mt: &HashMap<usize, usize>, allow: &HashSet<(String, String)>, a: usize, b: usize, top: bool, regs: &mut Vec<Region>, holes: &mut Vec<Region>) {
    let mut pos = a;
    while let Some(p) = find_in(m1, "{{", pos, b) {
        let Some(&q) = mt.get(&p).filter(|q| **q < b) else {
            pos = p + 2;
            continue;
        };
        let end = q + 1;
        let hs = if top && !allow.is_empty() { holes_of(m1, mt, allow, p, end) } else { Vec::new() };
        let mut cur = p;
        for (hs_, he_) in hs {
            regs.push((cur, hs_));
            holes.push((hs_, he_));
            scan(m1, mt, allow, hs_, he_, false, regs, holes);
            cur = he_;
        }
        regs.push((cur, end));
        pos = end;
    }
}

fn holes_of(m1: &str, mt: &HashMap<usize, usize>, allow: &HashSet<(String, String)>, p: usize, end: usize) -> Vec<Region> {
    if !m1.get(end.saturating_sub(2)..).is_some_and(|r| r.starts_with("}}")) || m1[p..].starts_with("{{{") || end < p + 4 {
        return Vec::new();
    }
    let (name, params) = tpl_params(m1, mt, p, end);
    let name = strip(&name);
    if name.is_empty() || name.starts_with(['#', '{', '\0']) || name.contains(':') {
        return Vec::new();
    }
    let title = norm_title(name);
    let mut out = Vec::new();
    for (key, mut vs, mut ve) in params {
        if !allow.contains(&(title.clone(), key)) {
            continue;
        }
        while vs < ve && char_at(m1, vs).is_some_and(is_space) {
            vs += char_at(m1, vs).map_or(1, char::len_utf8);
        }
        while ve > vs && char_before(m1, ve).is_some_and(is_space) {
            ve -= char_before(m1, ve).map_or(1, char::len_utf8);
        }
        let val = sl(m1, vs, ve);
        if !val.is_empty() && !val.contains(['\n', '\r']) && has_alpha(val) {
            out.push((vs, ve));
        }
    }
    out
}

/// `__MAGIC__`, `~~~` signatures and `[[Category:..]]` in the text left after templates are masked.
fn mw_phase3(m2: &str) -> Vec<Region> {
    let b = m2.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let is_cls = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    while i < b.len() {
        let mut hit = None;
        match b[i] {
            b'_' if b.get(i + 1) == Some(&b'_') && b.get(i + 2).is_some_and(|c| c.is_ascii_alphabetic()) => {
                let after = i + 3;
                let mut kmax = 0;
                while kmax < 40 && b.get(after + kmax).is_some_and(|c| is_cls(*c)) {
                    kmax += 1;
                }
                for k in (1..=kmax).rev() {
                    if b.get(after + k) == Some(&b'_') && b.get(after + k + 1) == Some(&b'_') {
                        hit = Some(after + k + 2);
                        break;
                    }
                }
            }
            b'~' => {
                let mut n = 0;
                while n < 5 && b.get(i + n) == Some(&b'~') {
                    n += 1;
                }
                if n >= 3 {
                    hit = Some(i + n);
                }
            }
            b'[' if b.get(i + 1) == Some(&b'[') => {
                let mut j = i + 2;
                while matches!(b.get(j), Some(b' ' | b'\t')) {
                    j += 1;
                }
                if starts_ci(b, j, "category") {
                    j += 8;
                    while matches!(b.get(j), Some(b' ' | b'\t')) {
                        j += 1;
                    }
                    if b.get(j) == Some(&b':') {
                        j += 1;
                        let end = run(m2, j, b.len(), 1000, |c| !matches!(c, '[' | ']' | '\r' | '\n' | '\0'));
                        if b.get(end) == Some(&b']') && b.get(end + 1) == Some(&b']') {
                            hit = Some(end + 2);
                        }
                    }
                }
            }
            _ => {}
        }
        match hit {
            Some(e) => {
                if !sl(m2, i, e).contains('\0') {
                    out.push((i, e));
                }
                i = e;
            }
            None => i += 1,
        }
    }
    out
}

/// Regions and allow-listed template parameter holes of a MediaWiki page.
pub fn mw_regions(text: &str, allow: &HashSet<(String, String)>) -> (Vec<Region>, Vec<Region>) {
    let r1 = mw_phase1(text);
    let m1 = masked(text, &r1);
    let (r2, holes) = mw_templates(&m1, allow);
    let m2 = masked(&m1, &r2);
    let r3 = mw_phase3(&m2);
    let mut all = r1;
    all.extend(r2);
    let merged = merge(all);
    let mut all2 = merged;
    all2.extend(r3);
    (merge(all2), holes)
}

// ------------------------------------------------------------------------------------------ DokuWiki

const DW_NAMES: &[&str] = &["nowiki", "code", "file", "html", "HTML", "php", "PHP"];

/// `%%..%%` runs, `<code>`-like blocks and `~~MACRO~~` lines. A `<code>` without its closing tag is literal text in
/// DokuWiki: only the opening tag is protected then.
pub fn dw_regions(text: &str) -> Vec<Region> {
    let b = text.as_bytes();
    let mut regs = Vec::new();
    let mut pos = 0usize;
    let mut none_after: HashMap<usize, usize> = HashMap::new();
    while pos < b.len() {
        let Some(off) = b[pos..].iter().position(|c| matches!(c, b'%' | b'<' | b'~')) else { break };
        let s = pos + off;
        match b[s] {
            b'%' if b.get(s + 1) == Some(&b'%') => {
                let e = text[s + 2..].find("%%").map_or(s + 2, |p| s + 2 + p + 2);
                regs.push((s, e));
                pos = e;
            }
            b'<' => {
                let mut done = false;
                for (idx, name) in DW_NAMES.iter().enumerate() {
                    if !b[s + 1..].starts_with(name.as_bytes()) {
                        continue;
                    }
                    let j = s + 1 + name.len();
                    if !char_at(text, j).is_some_and(|c| c == '>' || is_space(c)) {
                        continue;
                    }
                    // [^<>\r\n]{0,500} then >
                    let end = run(text, j, b.len(), 500, |c| !matches!(c, '<' | '>' | '\r' | '\n'));
                    if b.get(end) != Some(&b'>') {
                        break;
                    }
                    let open_end = end + 1;
                    let close = format!("</{name}>");
                    let known_none = none_after.get(&idx).is_some_and(|p| *p <= open_end);
                    let found = if known_none { None } else { text[open_end..].find(&close).map(|p| open_end + p + close.len()) };
                    if found.is_none() && !known_none {
                        none_after.insert(idx, open_end);
                    }
                    let e = found.unwrap_or(open_end);
                    regs.push((s, e));
                    pos = e;
                    done = true;
                    break;
                }
                if !done {
                    pos = s + 1;
                }
            }
            b'~' if b.get(s + 1) == Some(&b'~') => match dw_macro(text, s) {
                Some(e) => {
                    regs.push((s, e));
                    pos = e;
                }
                None => pos = s + 1,
            },
            _ => pos = s + 1,
        }
    }
    regs
}

/// `~~[A-Z][A-Z0-9_]{1,40}(:[^~\r\n]{0,200})?~~`
fn dw_macro(text: &str, s: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut j = s + 2;
    if !b.get(j)?.is_ascii_uppercase() {
        return None;
    }
    j += 1;
    let mut n = 0;
    while n < 40 && b.get(j).is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b'_') {
        j += 1;
        n += 1;
    }
    if n < 1 {
        return None;
    }
    if b.get(j) == Some(&b':') {
        let end = run(text, j + 1, b.len(), 200, |c| !matches!(c, '~' | '\r' | '\n'));
        if b.get(end) == Some(&b'~') && b.get(end + 1) == Some(&b'~') {
            return Some(end + 2);
        }
        return None;
    }
    (b.get(j) == Some(&b'~') && b.get(j + 1) == Some(&b'~')).then_some(j + 2)
}
