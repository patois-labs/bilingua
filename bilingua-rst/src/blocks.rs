//! The block walk: a line-grammar pass over the source that finds every slot of translatable text (LocOS `_Walk`).

use crate::pyu::*;
use crate::rx;
use crate::table::{simple_ends, Grid, Simple};
use bilingua_model::{guard, Error, Limits};

const MAX_DEPTH: usize = 40;
const PUNCT: &str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

#[derive(Clone, Copy)]
pub struct Line<'a> {
    pub off: usize,
    pub s: &'a str,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Para,
    Span,
    Title,
    Grid,
    Simple,
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    /// For a title: display width of the title text and the adornment lines (start, end, character).
    pub width: usize,
    pub adorn: Vec<(usize, usize, char)>,
    /// For a table: the raw text of each cell (None when the cell cannot map to a unit) and its first line.
    pub cells: Vec<Option<String>>,
    pub line: usize,
    pub style: Option<&'static str>,
}

impl Slot {
    fn new(kind: Kind, start: usize, end: usize) -> Slot {
        Slot { kind, start, end, width: 0, adorn: Vec::new(), cells: Vec::new(), line: 0, style: None }
    }
}

pub struct Walk<'a> {
    pub text: &'a str,
    pub slots: Vec<Slot>,
    pub meta: Vec<Slot>,
    pub skipped: Vec<(usize, String)>,
    count: usize,
    limits: Limits,
    nl: Vec<usize>,
}

/// `_lines(text)`.
pub fn lines(text: &str) -> Vec<Line<'_>> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let (mut start, mut i) = (0usize, 0usize);
    while i < b.len() {
        match b[i] {
            b'\n' => {
                out.push(Line { off: start, s: &text[start..i] });
                i += 1;
                start = i;
            }
            b'\r' => {
                out.push(Line { off: start, s: &text[start..i] });
                i += if b.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < b.len() {
        out.push(Line { off: start, s: &text[start..] });
    }
    out
}

fn is_adorn(s: &str) -> bool {
    let r = rstrip(s);
    let mut it = r.chars();
    match it.next() {
        Some(c) => PUNCT.contains(c) && it.all(|x| x == c),
        None => false,
    }
}

fn end_indented(vl: &[Line], i: usize) -> usize {
    let mut j = i;
    while j < vl.len() && (blank(vl[j].s) || indent(vl[j].s) >= 1) {
        j += 1;
    }
    while j > i && blank(vl[j - 1].s) {
        j -= 1;
    }
    j
}

fn dedent<'a>(vl: &[Line<'a>]) -> Vec<Line<'a>> {
    let m = vl.iter().filter(|l| !blank(l.s)).map(|l| indent(l.s)).min().unwrap_or(0);
    vl.iter()
        .map(|l| if !blank(l.s) { Line { off: l.off + m, s: &l.s[m..] } } else { Line { off: l.off + l.s.len(), s: "" } })
        .collect()
}

type R<T> = Result<T, Error>;

impl<'a> Walk<'a> {
    pub fn new(text: &'a str, limits: Limits) -> Walk<'a> {
        let nl = text.bytes().enumerate().filter(|(_, b)| *b == b'\n').map(|(i, _)| i).collect();
        Walk { text, slots: Vec::new(), meta: Vec::new(), skipped: Vec::new(), count: 0, limits, nl }
    }

    pub fn line_no(&self, off: usize) -> usize {
        self.nl.partition_point(|&p| p < off) + 1
    }

    fn bump(&mut self, n: usize) -> R<()> {
        self.count += n;
        guard::check_unit_count(self.count, &self.limits)
    }

    fn trim(&self, start: usize, end: usize) -> (usize, usize) {
        let seg = &self.text[start..end];
        let lead = seg.len() - lstrip(seg).len();
        let t = rstrip(&seg[lead..]);
        (start + lead, start + lead + t.len())
    }

    fn span(&mut self, start: usize, end: usize, kind: Kind) -> R<bool> {
        let (s, e) = self.trim(start.min(end), end);
        if s < e {
            self.slots.push(Slot::new(kind, s, e));
            self.bump(1)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn walk(&mut self, vl: &[Line<'a>], depth: usize) -> R<()> {
        if depth > MAX_DEPTH {
            return Ok(());
        }
        let n = vl.len();
        let mut i = 0usize;
        let mut literal_next = false;
        let mut ends: Vec<usize> = Vec::new();
        while i < n {
            let Line { off, s } = vl[i];
            if blank(s) {
                i += 1;
                continue;
            }
            let ind = indent(s);
            if literal_next {
                literal_next = false;
                if ind > 0 {
                    i = end_indented(vl, i);
                    continue;
                }
                let c0 = s.chars().next().unwrap_or(' ');
                if PUNCT.contains(c0) && !is_adorn(s) {
                    while i < n && !blank(vl[i].s) && vl[i].s.starts_with(c0) {
                        i += 1;
                    }
                    continue;
                }
            }
            if ind > 0 {
                let j = end_indented(vl, i);
                let d = dedent(&vl[i..j]);
                self.walk(&d, depth + 1)?;
                i = j;
                continue;
            }
            if s.starts_with("..") && (rstrip(s) == ".." || matches!(s.as_bytes().get(2), Some(b' ' | b'\t'))) {
                i = self.explicit(vl, i, depth)?;
                continue;
            }
            if s.starts_with("__ ") || rstrip(s) == "__" {
                i = end_indented(vl, i + 1);
                continue;
            }
            if rx::grid_top_full(s) {
                let mut j = i;
                while j < n && !blank(vl[j].s) {
                    j += 1;
                }
                self.grid_table(&vl[i..j])?;
                i = j;
                continue;
            }
            if rx::simple_border(s) {
                i = self.simple_table(vl, i, &mut ends)?;
                continue;
            }
            // overline + title + underline
            if is_adorn(s)
                && i + 2 < n
                && !blank(vl[i + 1].s)
                && !is_adorn(vl[i + 1].s)
                && is_adorn(vl[i + 2].s)
                && vl[i + 2].s.chars().next() == s.chars().next()
            {
                self.title(vl[i + 1], &[vl[i], vl[i + 2]])?;
                i += 3;
                continue;
            }
            if is_adorn(s) && rstrip(s).len() >= 4 {
                i += 1;
                continue;
            }
            let bm = rx::bullet(s);
            let mut em = if bm.is_none() { rx::enum_match(s) } else { None };
            if em.as_ref().is_some_and(|e| e.paren && e.g3 != ')') {
                em = None;
            }
            if em.as_ref().is_some_and(|e| !e.g2.is_empty() && e.g2.chars().all(is_alpha)) && i + 1 < n {
                let nxt = vl[i + 1].s;
                if !blank(nxt) && indent(nxt) == 0 && rx::enum_match(nxt).is_none() {
                    em = None;
                }
            }
            if let Some(k) = bm.or(em.as_ref().map(|e| e.end)) {
                let j = end_indented(vl, i + 1);
                let mut body = vec![Line { off: off + k, s: &s[k..] }];
                body.extend(dedent(&vl[i + 1..j]));
                self.walk(&body, depth + 1)?;
                i = j;
                continue;
            }
            if rx::field(s) {
                i = end_indented(vl, i + 1);
                continue;
            }
            if s.starts_with(">>>") {
                while i < n && !blank(vl[i].s) {
                    i += 1;
                }
                continue;
            }
            if let Some(g1) = rx::option_list(s) {
                if g1 || (i + 1 < n && indent(vl[i + 1].s) > 0) {
                    while i < n && !blank(vl[i].s) {
                        i += 1;
                    }
                    if i < n && indent(vl[i].s) > 0 {
                        i = end_indented(vl, i);
                    }
                    continue;
                }
            }
            if rx::line_block(s).is_some() {
                while i < n && !blank(vl[i].s) {
                    let Line { off: o2, s: s2 } = vl[i];
                    let k = rx::line_block(s2).unwrap_or(0);
                    self.span(o2 + k, o2 + s2.len(), Kind::Span)?;
                    i += 1;
                }
                continue;
            }
            // a single line underlined: a section title
            if i + 1 < n && is_adorn(vl[i + 1].s) && !is_adorn(s) {
                let ul = rstrip(vl[i + 1].s);
                if ul.len() >= 4 || ul.len() >= column_width(rstrip(s)) {
                    self.title(vl[i], &[vl[i + 1]])?;
                    i += 2;
                    continue;
                }
            }
            // paragraph
            let mut j = i;
            while j < n && !blank(vl[j].s) && indent(vl[j].s) == 0 {
                j += 1;
            }
            if j < n && !blank(vl[j].s) && j == i + 1 {
                // definition list item
                self.span(off, off + s.len(), Kind::Span)?;
                let k = end_indented(vl, j);
                let d = dedent(&vl[j..k]);
                self.walk(&d, depth + 1)?;
                i = k;
                continue;
            }
            let Line { off: last_off, s: last } = vl[j - 1];
            let mut end = last_off + rstrip(last).len();
            if rstrip(last).ends_with("::") {
                literal_next = true;
                end -= 2;
            }
            self.span(off, end, Kind::Para)?;
            i = j;
        }
        Ok(())
    }

    fn title(&mut self, tline: Line<'a>, adorn: &[Line<'a>]) -> R<()> {
        let before = self.slots.len();
        self.span(tline.off, tline.off + tline.s.len(), Kind::Title)?;
        if self.slots.len() > before {
            let text = self.text;
            let slot = self.slots.last_mut().expect("just pushed");
            slot.width = column_width(&text[slot.start..slot.end]);
            slot.adorn = adorn
                .iter()
                .map(|l| {
                    let r = rstrip(l.s);
                    (l.off, l.off + r.len(), r.chars().next().unwrap_or('='))
                })
                .collect();
        }
        Ok(())
    }

    fn explicit(&mut self, vl: &[Line<'a>], i: usize, depth: usize) -> R<usize> {
        let Line { off, s } = vl[i];
        let j = end_indented(vl, i + 1);
        let k3 = adv(s, 3);
        let rest = if s.chars().count() > 2 { &s[k3..] } else { "" };
        let base = off + k3;
        if blank(rest) && (i + 1 >= vl.len() || blank(vl[i + 1].s)) {
            return Ok(i + 1);
        }
        if let Some(e) = rx::footnote(rest) {
            let mut body = vec![Line { off: base + e, s: &rest[e..] }];
            body.extend(dedent(&vl[i + 1..j]));
            self.walk(&body, depth + 1)?;
            return Ok(j);
        }
        if rest.starts_with('|') || rest.starts_with('_') {
            return Ok(j);
        }
        let Some((raw_name, dend)) = rx::directive(rest) else { return Ok(j) };
        let name = raw_name.to_lowercase();
        let arg = Line { off: base + dend, s: &rest[dend..] };
        let follow = dedent(&vl[i + 1..j]);
        let hdr_len = follow.iter().position(|l| blank(l.s)).unwrap_or(follow.len());
        let after = &follow[hdr_len..];
        let non_opt: Vec<Line<'a>> = follow[..hdr_len].iter().copied().filter(|l| !rx::option_line(l.s)).collect();
        let n = name.as_str();
        if ADMONITIONS.contains(&n) || VERSIONED.contains(&n) {
            let mut first = arg;
            if VERSIONED.contains(&n) {
                let a = arg.s;
                let word = a.find(is_space).unwrap_or(a.len());
                let cut = if word == 0 { 0 } else { word + (a.len() - word - a[word..].trim_start_matches([' ', '\t']).len()) };
                first = Line { off: arg.off + cut, s: &arg.s[cut..] };
            }
            let mut content: Vec<Line<'a>> = Vec::new();
            if !blank(first.s) {
                content.push(first);
            }
            content.extend(non_opt.iter().copied());
            if !content.is_empty() && !after.is_empty() {
                content.push(Line { off: after[0].off, s: "" });
            }
            let mut all = dedent(&content[..content.len().min(1)]);
            all.extend(content.iter().skip(1).copied());
            all.extend(after.iter().copied());
            self.walk(&all, depth + 1)?;
        } else if TITLED.contains(&n) {
            if !blank(arg.s) {
                let last = non_opt.last().copied().unwrap_or(arg);
                self.span(arg.off, last.off + last.s.len(), Kind::Span)?;
            }
            if n != "rubric" && n != "centered" {
                self.walk(after, depth + 1)?;
            }
        } else if CONTAINERS.contains(&n) || n.contains(':') {
            self.walk(after, depth + 1)?;
        } else if n == "meta" {
            self.meta_fields(&follow)?;
        }
        Ok(j)
    }

    fn meta_fields(&mut self, vl: &[Line<'a>]) -> R<()> {
        let mut i = 0usize;
        while i < vl.len() {
            let Line { off, s } = vl[i];
            i += 1;
            let fm = if !blank(s) && indent(s) == 0 { rx::meta_field(s) } else { None };
            let Some((name, fend)) = fm else { continue };
            let mut parts: Vec<Line<'a>> = Vec::new();
            if !blank(&s[fend..]) {
                parts.push(Line { off: off + fend, s: &s[fend..] });
            }
            while i < vl.len() && !blank(vl[i].s) && indent(vl[i].s) > 0 {
                parts.push(vl[i]);
                i += 1;
            }
            let (Some(f), Some(l)) = (parts.first(), parts.last()) else { continue };
            let (st, en) = self.trim(f.off, l.off + l.s.len());
            if st < en {
                let mut slot = Slot::new(Kind::Para, st, en);
                slot.style = Some(if name == "description" { "meta:description" } else { "meta:keywords" });
                self.meta.push(slot);
                self.bump(1)?;
            }
        }
        Ok(())
    }

    fn grid_table(&mut self, vl: &[Line<'a>]) -> R<()> {
        let ls: Vec<&str> = vl.iter().map(|l| l.s).collect();
        let mut g = Grid::new(&ls);
        let ok = g.parse().unwrap_or(false);
        if !ok {
            let line = self.line_no(vl[0].off);
            self.skipped.push((line, "grid table layout".into()));
            return Ok(());
        }
        let cells: Vec<Option<String>> = g.cells.iter().map(|&c| g.cell_text(c)).collect();
        self.bump(cells.len())?;
        let last = vl[vl.len() - 1];
        let mut slot = Slot::new(Kind::Grid, vl[0].off, last.off + last.s.len());
        slot.cells = cells;
        slot.line = self.line_no(vl[0].off);
        self.slots.push(slot);
        Ok(())
    }

    fn simple_table(&mut self, vl: &[Line<'a>], i: usize, ends: &mut Vec<usize>) -> R<usize> {
        let n = vl.len();
        if ends.len() != n + 1 {
            let ls: Vec<&str> = vl.iter().map(|l| l.s).collect();
            *ends = simple_ends(&ls);
        }
        let end = if i + 1 < n && ends[i + 1] < n { Some(ends[i + 1] + 1) } else { None };
        let Some(end) = end else {
            let mut i = i;
            while i < n && !blank(vl[i].s) {
                i += 1;
            }
            return Ok(i);
        };
        let ls: Vec<&str> = vl[i..end].iter().map(|l| l.s).collect();
        let mut t = Simple::new(&ls);
        let line = self.line_no(vl[i].off);
        if !t.parse() {
            let reason = if t.reason.is_empty() { "simple table layout" } else { t.reason };
            self.skipped.push((line, reason.into()));
            return Ok(end);
        }
        self.bump(t.cells.len())?;
        let last = vl[end - 1];
        let mut slot = Slot::new(Kind::Simple, vl[i].off, last.off + last.s.len());
        slot.cells = t.cells.iter().map(|c| c.raw.clone()).collect();
        slot.line = line;
        self.slots.push(slot);
        Ok(end)
    }
}

const ADMONITIONS: &[&str] =
    &["attention", "caution", "danger", "error", "hint", "important", "note", "tip", "warning", "seealso", "todo"];
const VERSIONED: &[&str] = &["versionadded", "versionchanged", "deprecated", "versionremoved"];
const TITLED: &[&str] = &["admonition", "topic", "sidebar", "table", "list-table", "rubric", "centered"];
const CONTAINERS: &[&str] = &[
    "figure",
    "container",
    "only",
    "ifconfig",
    "compound",
    "epigraph",
    "highlights",
    "pull-quote",
    "class",
    "rst-class",
    "cssclass",
    "glossary",
    "hlist",
    "line-block",
    "parsed-literal-text",
];
