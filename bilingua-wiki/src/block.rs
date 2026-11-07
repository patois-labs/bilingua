//! The line grammar: which spans of the page are units. Both walks work on the mask (protected regions are NUL
//! bytes), so a comment, a template or a `<code>` block never looks like a heading, a list or a cell.

use crate::inline::{Ctx, Tok};
use crate::lex::*;
use crate::regions::Region;
use bilingua_model::{guard, Error, Limits};

#[derive(Debug, Clone)]
pub struct Slot {
    pub start: usize,
    pub end: usize,
    /// How a translation is escaped: `tpl` (template parameter), `mw` / `dw` (table cell).
    pub cell: Option<&'static str>,
    pub kind: &'static str,
    pub level: usize,
}

pub struct Walk<'a> {
    pub cx: &'a Ctx<'a>,
    pub slots: Vec<Slot>,
    para: Option<(usize, usize)>,
    limits: Limits,
}

/// Lines of the mask: `(start, end)` without the terminator; `\r\n`, `\r` and `\n` end a line, and a final
/// terminator does not start another.
fn lines(mask: &str) -> Vec<(usize, usize)> {
    let b = mask.as_bytes();
    let mut out = Vec::new();
    let mut s = 0;
    while s < b.len() {
        let mut e = s;
        while e < b.len() && b[e] != b'\r' && b[e] != b'\n' {
            e += 1;
        }
        out.push((s, e));
        if e >= b.len() {
            break;
        }
        s = if b[e] == b'\r' && b.get(e + 1) == Some(&b'\n') { e + 2 } else { e + 1 };
    }
    out
}

fn blank(s: &str) -> bool {
    s.chars().all(|c| c == '\0' || is_space(c))
}

impl<'a> Walk<'a> {
    pub fn new(cx: &'a Ctx<'a>, limits: Limits) -> Walk<'a> {
        Walk { cx, slots: Vec::new(), para: None, limits }
    }

    fn add(&mut self, start: usize, end: usize, cell: Option<&'static str>, kind: &'static str, level: usize) -> Result<(), Error> {
        let t = self.cx.text;
        let (mut s, mut e) = (start, end);
        while s < e && char_at(t, s).is_some_and(is_space) {
            s += char_at(t, s).map_or(1, char::len_utf8);
        }
        while e > s && char_before(t, e).is_some_and(is_space) {
            e -= char_before(t, e).map_or(1, char::len_utf8);
        }
        if s < e {
            self.slots.push(Slot { start: s, end: e, cell, kind, level });
            guard::check_unit_count(self.slots.len(), &self.limits)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Error> {
        if let Some((s, e)) = self.para.take() {
            self.add(s, e, None, "paragraph", 0)?;
        }
        Ok(())
    }

    /// Whether `[a, b)` holds a translatable letter once markup is tokenized.
    fn words(&self, a: usize, b: usize) -> bool {
        if !has_alpha(sl(self.cx.mask, a, b)) {
            return false;
        }
        let mut tok = Tok::new(self.cx);
        let mut v = Vec::new();
        tok.run(a, b, 0, &mut v);
        crate::lex::any_letter_in(&v)
    }

    /// A line inside a file link that closes on a later line: it joins the open paragraph whatever it looks like
    /// (`----`, a list marker, a blank line), so the options and the closing brackets stay in the link.
    fn para_force(&mut self, s: usize, e: usize) {
        match &mut self.para {
            None => self.para = Some((s, e)),
            Some(p) => p.1 = e,
        }
    }

    fn para_line(&mut self, s: usize, e: usize) -> Result<(), Error> {
        if !self.words(s, e) {
            return self.flush();
        }
        match &mut self.para {
            None => self.para = Some((s, e)),
            Some(p) => p.1 = e,
        }
        Ok(())
    }
}

/// Splits `[a, b)` on any separator outside `[[ ]]` and `{{ }}`; the parts as `(start, end)`.
fn split_top(mask: &str, a: usize, b: usize, seps: &[&str]) -> Vec<(usize, usize)> {
    let by = mask.as_bytes();
    let mut out = Vec::new();
    let (mut dl, mut db) = (0usize, 0usize);
    let mut cur = a;
    let mut i = a;
    while i < b {
        let two = by.get(i..(i + 2).min(by.len())).unwrap_or(&[]);
        if two == b"[[" {
            dl += 1;
            i += 2;
            continue;
        }
        if two == b"]]" && dl > 0 {
            dl -= 1;
            i += 2;
            continue;
        }
        if two == b"{{" {
            db += 1;
            i += 2;
            continue;
        }
        if two == b"}}" && db > 0 {
            db -= 1;
            i += 2;
            continue;
        }
        if dl == 0 && db == 0 {
            if let Some(h) = seps.iter().find(|s| by[i..].starts_with(s.as_bytes())) {
                out.push((cur, i));
                i += h.len();
                cur = i;
                continue;
            }
        }
        i += 1;
    }
    out.push((cur, b));
    out
}

/// The `:` that ends a `; term` (outside brackets and URLs).
fn term_colon(mask: &str, a: usize, b: usize) -> Option<usize> {
    let by = mask.as_bytes();
    let mut depth = 0usize;
    for i in a..b {
        match by[i] {
            b'[' => depth += 1,
            b']' if depth > 0 => depth -= 1,
            b':' if depth == 0 && !by[i + 1..].starts_with(b"//") => return Some(i),
            _ => {}
        }
    }
    None
}

/// `[ \t]*#[ \t]*redirect\b` (case-insensitive)
fn is_redirect(body: &str) -> bool {
    let t = body.trim_start_matches([' ', '\t']);
    let Some(r) = t.strip_prefix('#') else { return false };
    let r = r.trim_start_matches([' ', '\t']);
    r.len() >= 8 && r.is_char_boundary(8) && r[..8].eq_ignore_ascii_case("redirect") && !char_at(r, 8).is_some_and(is_word)
}

pub fn mw_slots(w: &mut Walk, holes: &[Region]) -> Result<(), Error> {
    let mask = w.cx.mask;
    let mut table = 0usize;
    let mut hole_i = 0usize;
    // end of a file link that started on an earlier line and has not closed yet
    let mut link_open_until = 0usize;
    for (s, e) in lines(mask) {
        let body = &mask[s..e];
        let mut line_holes = Vec::new();
        while hole_i < holes.len() && holes[hole_i].0 < e {
            if holes[hole_i].0 >= s {
                line_holes.push(holes[hole_i]);
            }
            hole_i += 1;
        }
        if s < link_open_until {
            w.para_force(s, e);
            // the line that closes one caption may open the next one
            if let Some(end) = multiline_file_link(w.cx, link_open_until.min(e), e) {
                link_open_until = end;
            }
            continue;
        }
        let stripped = strip(body);
        if stripped.is_empty() {
            w.flush()?;
            continue;
        }
        if !line_holes.is_empty() {
            let mut rest = body.as_bytes().to_vec();
            for &(hs, he) in &line_holes {
                let hi = (he - s).min(rest.len());
                if let Some(r) = rest.get_mut(hs - s..hi) {
                    r.fill(0);
                }
            }
            if String::from_utf8(rest).map(|r| blank(&r)).unwrap_or(false) {
                w.flush()?;
                for (hs, he) in line_holes {
                    w.add(hs, he, Some("tpl"), "template parameter", 0)?;
                }
                continue;
            }
        }
        if blank(stripped) || is_redirect(body) {
            w.flush()?;
            continue;
        }
        let head_at = body.len() - body.trim_start_matches([' ', '\t', ':']).len();
        let (lead, head) = (&body[..head_at], &body[head_at..]);
        if head.starts_with("{|") {
            w.flush()?;
            table += 1;
            continue;
        }
        if table > 0 && head.starts_with("|}") {
            w.flush()?;
            table -= 1;
            continue;
        }
        if table > 0 && head.starts_with(['|', '!']) && lead.trim_matches([' ', '\t']).is_empty() {
            w.flush()?;
            mw_cells(w, s + lead.len(), head)?;
            continue;
        }
        if body.starts_with('=') {
            let core = body.trim_end_matches([' ', '\t', '\0']);
            let lead_eq = core.len() - core.trim_start_matches('=').len();
            let trail_eq = core.len() - core.trim_end_matches('=').len();
            let n = lead_eq.min(trail_eq).min(6);
            if n >= 1 && core.len() > 2 * n {
                w.flush()?;
                w.add(s + n, s + core.len() - n, None, "heading", n)?;
                continue;
            }
        }
        if body.starts_with("----") {
            w.flush()?;
            let d = body.len() - body.trim_start_matches('-').len();
            w.add(s + d, e, None, "paragraph", 0)?;
            continue;
        }
        if body.starts_with(['*', '#', ':', ';']) {
            w.flush()?;
            let p = body.len() - body.trim_start_matches(['*', '#', ':', ';']).len();
            if body[..p].contains(';') {
                if let Some(colon) = term_colon(mask, s + p, e) {
                    w.add(s + p, colon, None, "term", 0)?;
                    w.add(colon + 1, e, None, "definition", 0)?;
                    continue;
                }
                // a term without a definition: a colon in its translation would start one
                w.add(s + p, e, None, "term", 0)?;
                continue;
            }
            w.add(s + p, e, None, "list-item", 0)?;
            continue;
        }
        if body.starts_with([' ', '\t']) {
            w.flush()?;
            continue;
        }
        w.para_line(s, e)?;
        if let Some(end) = multiline_file_link(w.cx, s, e) {
            link_open_until = end;
            w.para_force(s, e);
        }
    }
    w.flush()
}

/// The end of a `[[File:..]]` link that starts on the line `[s, e)` and closes on a later line (a caption of several
/// lines). The link ends within 200 lines and 20000 bytes, or it is not taken for one.
fn multiline_file_link(cx: &Ctx, s: usize, e: usize) -> Option<usize> {
    let m = cx.mask.as_bytes();
    let mut i = s;
    while i + 1 < e {
        if m[i] == b'[' && m[i + 1] == b'[' {
            if let Some(&end) = cx.links.get(&i).filter(|end| **end > e) {
                let close = end + 2;
                let inner = sl(cx.mask, i + 2, end);
                let target = inner.split('|').next().unwrap_or("");
                let far = close - e > 20000 || m[e..close].iter().filter(|c| **c == 10).count() > 200;
                // a heading line between is a heading (MediaWiki reads headings before links), not caption text
                let heading = !far
                    && sl(cx.mask, e, close).lines().any(|l| {
                        let t = l.trim_end_matches([' ', '\t', '\r']);
                        t.len() >= 3 && t.starts_with('=') && t.ends_with('=')
                    });
                if inner.contains('|') && crate::inline::file_ns(target) && !far && !heading {
                    return Some(close);
                }
            }
        }
        i += 1;
    }
    None
}

/// One MediaWiki table line: caption, header or data cells; row lines are skipped.
fn mw_cells(w: &mut Walk, s: usize, head: &str) -> Result<(), Error> {
    let mask = w.cx.mask;
    if head.starts_with("|-") {
        return Ok(());
    }
    let (a, seps, kind): (usize, &[&str], &'static str) = if head.starts_with("|+") {
        (s + 2, &["\u{1}"], "caption")
    } else if head.starts_with('!') {
        (s + 1, &["!!", "||"], "header-cell")
    } else {
        (s + 1, &["||"], "cell")
    };
    let b = s + head.len();
    for (mut cs, ce) in split_top(mask, a, b, seps) {
        let bar = split_top(mask, cs, ce, &["|"]);
        if bar.len() > 1 {
            cs = bar[1].0;
        }
        w.add(cs, ce, Some("mw"), kind, 0)?;
    }
    Ok(())
}

/// `(?: {2,}|\t+)[*-][ \t]*`: the length of a DokuWiki list marker, if the line starts with one.
fn dw_list_marker(body: &str) -> Option<usize> {
    let by = body.as_bytes();
    let n = if by.first() == Some(&b' ') {
        let n = by.iter().take_while(|c| **c == b' ').count();
        if n < 2 {
            return None;
        }
        n
    } else if by.first() == Some(&b'\t') {
        by.iter().take_while(|c| **c == b'\t').count()
    } else {
        return None;
    };
    if !matches!(by.get(n), Some(b'*' | b'-')) {
        return None;
    }
    let ws = by[n + 1..].iter().take_while(|c| **c == b' ' || **c == b'\t').count();
    Some(n + 1 + ws)
}

pub fn dw_slots(w: &mut Walk) -> Result<(), Error> {
    let mask = w.cx.mask;
    for (s, e) in lines(mask) {
        let body = &mask[s..e];
        let stripped = strip(body);
        if stripped.is_empty() || blank(stripped) {
            w.flush()?;
            continue;
        }
        let rs = body.trim_end_matches(is_space);
        let core = stripped;
        if core.starts_with("==") && core.ends_with("==") && core.len() > 4 {
            let n = (core.len() - core.trim_start_matches('=').len()).min(core.len() - core.trim_end_matches('=').len());
            if n >= 2 && core.len() > 2 * n {
                w.flush()?;
                let off = body.find(core).unwrap_or(0);
                w.add(s + off + n, s + off + core.len() - n, None, "heading", n)?;
                continue;
            }
        }
        let trimmed = body.trim_matches([' ', '\t']);
        if trimmed.len() >= 4 && trimmed.bytes().all(|c| c == b'-') {
            w.flush()?;
            continue;
        }
        if let Some(m) = dw_list_marker(body) {
            w.flush()?;
            w.add(s + m, e, None, "list-item", 0)?;
            continue;
        }
        if body.starts_with("  ") || body.starts_with('\t') {
            w.flush()?;
            continue;
        }
        if body.starts_with(['^', '|']) {
            w.flush()?;
            for (cs, ce) in split_top(mask, s, s + rs.len(), &["^", "|"]).into_iter().skip(1) {
                if strip(sl(w.cx.text, cs, ce)) != ":::" {
                    w.add(cs, ce, Some("dw"), "cell", 0)?;
                }
            }
            continue;
        }
        if body.starts_with('>') {
            w.flush()?;
            let p = body.len() - body.trim_start_matches('>').len();
            w.add(s + p, e, None, "quote", 0)?;
            continue;
        }
        w.para_line(s, e)?;
    }
    w.flush()
}
