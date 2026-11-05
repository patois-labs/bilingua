//! The block grammar: which spans of the file are translatable (`_slots` in the LocOS filter).

use crate::lex::*;
use bilingua_model::guard::{self, Limits};
use bilingua_model::Error;
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct Slot {
    pub start: usize,
    pub end: usize,
    /// The table cell separator for a cell (escaped on merge).
    pub sep: Option<char>,
    pub kind: &'static str,
    /// Section level for a heading.
    pub level: usize,
    /// 1-based line of the start.
    pub line: usize,
}

struct Para {
    start: usize,
    end: usize,
    lines: Option<usize>,
    kind: &'static str,
    line: usize,
}

struct Table {
    close: String,
    sep: char,
    style: char, // '\0' = none
}

struct Walk<'a> {
    text: &'a str,
    slots: Vec<Slot>,
    limits: &'a Limits,
}

impl Walk<'_> {
    fn add(&mut self, mut start: usize, mut end: usize, sep: Option<char>, kind: &'static str, level: usize, line: usize) -> Result<(), Error> {
        let t = &self.text[start..end];
        let lead = t.len() - t.trim_start_matches(is_space).len();
        start += lead;
        let t = &self.text[start..end];
        end = start + rstrip(t).len();
        if start < end {
            let chars = self.text[start..end].chars().count();
            guard::check_unit_chars(chars, self.limits)?;
            self.slots.push(Slot { start, end, sep, kind, level, line });
            guard::check_unit_count(self.slots.len(), self.limits)?;
        }
        Ok(())
    }

    fn flush(&mut self, para: &mut Option<Para>) -> Result<(), Error> {
        if let Some(p) = para.take() {
            self.add(p.start, p.end, None, p.kind, 0, p.line)?;
        }
        Ok(())
    }

    /// `_cells`: one table line split into cell spans.
    fn cells(&mut self, s: usize, body: &str, table: &mut Table, line: usize) -> Result<(), Error> {
        let sep = table.sep;
        let b = body.as_bytes();
        let seps: Vec<usize> = (0..b.len()).filter(|&k| b[k] == sep as u8 && (k == 0 || b[k - 1] != b'\\')).collect();
        if seps.is_empty() {
            if table.style != 'l' {
                self.add(s, s + body.len(), Some(sep), "cell", 0, line)?;
            }
            return Ok(());
        }
        let mut bounds: Vec<(isize, usize)> = vec![(-1, seps[0])];
        for j in 0..seps.len() - 1 {
            bounds.push((seps[j] as isize, seps[j + 1]));
        }
        bounds.push((seps[seps.len() - 1] as isize, body.len()));
        let last = bounds.len() - 1;
        for (j, (a, bnd)) in bounds.iter().enumerate() {
            let start = (*a + 1) as usize;
            let mut end = *bnd;
            let mut nxt: Option<char> = None;
            if j < last {
                let seg = &body[start..end];
                match cell_spec(seg) {
                    Some((p, spec)) if !spec.is_empty() => {
                        end = start + p;
                        let l = spec.chars().last().unwrap_or(' ');
                        nxt = Some(if "adehlmsv".contains(l) { l } else { '\0' });
                    }
                    _ => nxt = Some('\0'),
                }
            }
            if table.style != 'l' {
                self.add(s + start, s + end, Some(sep), "cell", 0, line)?;
            }
            if let Some(n) = nxt {
                table.style = n;
            }
        }
        Ok(())
    }
}

/// `_CELL_SPEC.search`: a cell specifier at the end of a segment: (start, specifier).
fn cell_spec(seg: &str) -> Option<(usize, &str)> {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| {
        Regex::new(r"^(?:\d+\*)?(?:\d+(?:\.\d+)?\+|\.\d+\+)?[<^>]?(?:\.[<^>])?[adehlmsv]?$").expect("cell spec pattern")
    });
    let b = seg.as_bytes();
    // the longest suffix of specifier characters; a match can only start at its beginning
    let allowed = |c: u8| c.is_ascii_digit() || b"*+.<^>adehlmsv".contains(&c);
    let mut st = b.len();
    while st > 0 && allowed(b[st - 1]) {
        st -= 1;
    }
    let candidate = st == 0 || b[st - 1] == b' ' || b[st - 1] == b'\t';
    if candidate && re.is_match(&seg[st..]) {
        Some((st, &seg[st..]))
    } else {
        None
    }
}

/// `_setext`: a one-line plain paragraph underlined to within one character of its length is a section title.
fn setext(para: &Para, text: &str, underline: &str) -> bool {
    if para.lines != Some(1) {
        return false;
    }
    let title = rstrip(&text[para.start..para.end]);
    let Some(first) = title.chars().next() else { return false };
    if first == '.' || first == '[' || !title.chars().any(is_alnum) {
        return false;
    }
    let (tl, ul) = (title.chars().count(), underline.chars().count());
    ul >= 2 && tl.abs_diff(ul) <= 1
}

/// Every translatable span in document order.
pub fn slots(text: &str, limits: &Limits) -> Result<Vec<Slot>, Error> {
    let lines = lines(text);
    let mut w = Walk { text, slots: Vec::new(), limits };
    let mut para: Option<Para> = None;
    let mut verbatim: Option<String> = None;
    let mut table: Option<Table> = None;
    let mut nested: Option<Table> = None;
    let mut pending = String::new();
    let mut skip_para = false;
    let mut header = false;
    let mut seen_block = false;
    let mut attr_cont = false;
    for (idx, &(s, e)) in lines.iter().enumerate() {
        let line = idx + 1;
        let body = &text[s..e];
        let rs = rstrip(body);
        if let Some(v) = &verbatim {
            if rs == v {
                verbatim = None;
            }
            continue;
        }
        let continues = rs.ends_with(" \\") || rs.ends_with("\t\\");
        if attr_cont {
            attr_cont = continues;
            continue;
        }
        if is_preproc(body) {
            w.flush(&mut para)?;
            continue;
        }
        if rs.is_empty() {
            w.flush(&mut para)?;
            skip_para = false;
            header = false;
            continue;
        }
        if skip_para {
            continue;
        }
        if is_setext_line(rs) && para.as_ref().is_some_and(|p| setext(p, text, rs)) {
            w.flush(&mut para)?;
            continue;
        }
        if para.is_none() && is_attr_entry(body) {
            attr_cont = continues;
            continue;
        }
        if is_verbatim_delim(rs) || is_fence_open(rs) {
            w.flush(&mut para)?;
            verbatim = Some(if rs.starts_with("```") { "```".to_string() } else { rs.to_string() });
            pending.clear();
            seen_block = true;
            continue;
        }
        if is_line_comment(body) {
            w.flush(&mut para)?;
            continue;
        }
        let tm = table_delim(rs);
        if nested.is_some() || table.is_some() {
            let close_hit = {
                let cur = nested.as_ref().or(table.as_ref()).expect("a table is open");
                rs == cur.close
            };
            if close_hit {
                if nested.is_some() {
                    nested = None;
                } else {
                    table = None;
                }
                continue;
            }
            if tm == Some('!') && nested.is_none() {
                nested = Some(Table { close: rs.to_string(), sep: '!', style: '\0' });
                continue;
            }
            let cur = if nested.is_some() { nested.as_mut() } else { table.as_mut() }.expect("a table is open");
            w.cells(s, body, cur, line)?;
            continue;
        }
        if let Some(sep) = tm {
            w.flush(&mut para)?;
            seen_block = true;
            if sep == ',' || sep == ':' || is_table_format_skip(&pending) {
                verbatim = Some(rs.to_string());
            } else {
                table = Some(Table { close: rs.to_string(), sep, style: '\0' });
            }
            pending.clear();
            continue;
        }
        if is_compound_delim(rs) {
            w.flush(&mut para)?;
            seen_block = true;
            if !pending.is_empty() && is_verbatim_style(&block_style(&pending)) {
                verbatim = Some(rs.to_string());
            }
            pending.clear();
            continue;
        }
        if header {
            continue;
        }
        if para.is_none() && is_block_attr(rs) {
            pending = rs.to_string();
            continue;
        }
        if para.is_none() && is_block_macro(rs) {
            pending.clear();
            seen_block = true;
            continue;
        }
        if para.is_none() && is_break(rs) {
            continue;
        }
        if rs == "+" {
            w.flush(&mut para)?;
            continue;
        }
        if para.is_none() {
            if let Some((mch, mlen, mend)) = section_match(body) {
                let mut core_end = e;
                if mch == '=' {
                    // a symmetric trailing marker stays outside the unit
                    let t = body.trim_end_matches([' ', '\t']);
                    let marker = &body[..mlen];
                    if let Some(before) = t.strip_suffix(marker) {
                        let tail_start = before.trim_end_matches([' ', '\t']).len();
                        if tail_start < before.len() && tail_start >= mend {
                            core_end = s + tail_start;
                        }
                    }
                }
                w.add(s + mend, core_end, None, "heading", mlen, line)?;
                if mlen == 1 && !seen_block {
                    header = true;
                }
                seen_block = true;
                pending.clear();
                continue;
            }
        }
        seen_block = true;
        if para.is_none() && is_block_title(body) {
            w.add(s + 1, e, None, "block-title", 0, line)?;
            continue;
        }
        if let Some(end) = list_item_end(body) {
            w.flush(&mut para)?;
            pending.clear();
            para = Some(Para { start: s + end, end: e, lines: None, kind: "list-item", line });
            continue;
        }
        let first_space = body.chars().next().is_some_and(is_space);
        let dm = if para.is_some() || !first_space { dlist_match(rs) } else { None };
        if let Some((ts, te, me)) = dm {
            w.flush(&mut para)?;
            w.add(s + ts, s + te, None, "term", 0, line)?;
            if me < rs.len() {
                para = Some(Para { start: s + me, end: e, lines: None, kind: "description", line });
            }
            pending.clear();
            continue;
        }
        if para.is_none() {
            let style = if pending.is_empty() { String::new() } else { block_style(&pending) };
            pending.clear();
            if is_verbatim_style(&style) || body.starts_with([' ', '\t']) {
                skip_para = true;
                continue;
            }
            let (off, kind) = match admonition_end(body) {
                Some(n) => (n, "admonition"),
                None => (0, "paragraph"),
            };
            para = Some(Para { start: s + off, end: e, lines: Some(1), kind, line });
            continue;
        }
        if let Some(p) = para.as_mut() {
            p.end = e;
            p.lines = Some(p.lines.unwrap_or(1) + 1);
        }
    }
    w.flush(&mut para)?;
    Ok(w.slots)
}
