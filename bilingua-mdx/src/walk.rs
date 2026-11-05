//! One pass over an MDX text (LocOS `_Walk`): the front matter values, the line units and the attribute units.

use crate::front::{self, FrontMatter};
use crate::mdline::{self, CommentBlocks};
use crate::pyx::{has_alnum, is_eol_char, is_space, split_lines, trim, trim_end};
use crate::scan::{Scanner, Tag};
use bilingua_model::{Error, LimitError, LimitKind};
use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

pub(crate) type Span = (usize, usize);

/// A piece of a line unit's text.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Item {
    Text(usize, usize),
    /// Index into `LineUnit::toks`.
    Open(usize),
    Close(usize),
    Point(usize),
}

#[derive(Debug, Clone)]
pub(crate) struct Tok {
    /// LocOS token number (1-based, one counter per unit).
    pub id: usize,
    pub open: Span,
    pub close: Option<Span>,
}

#[derive(Debug, Clone)]
pub(crate) struct LineUnit {
    pub start: usize,
    pub end: usize,
    pub items: Vec<Item>,
    pub toks: Vec<Tok>,
    /// A link reference definition's title: its opening delimiter (the text is the decoded title).
    pub delim: Option<char>,
    pub title: String,
    /// Heading prefix, list bullet and so on, for the unit's kind.
    pub prefix: String,
    /// A translated line start may need a block-start escape (#704).
    pub block_ok: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct AttrUnit {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub quote: u8,
    /// Index of the line unit whose core holds this attribute.
    pub line: Option<usize>,
}

pub(crate) struct Walk<'a> {
    pub text: &'a str,
    allow: &'a HashSet<String>,
    max_units: usize,
    sc: Scanner<'a>,
    pub starts: Vec<usize>,
    ends: Vec<usize>,
    pub fm: Option<FrontMatter>,
    pub units: Vec<LineUnit>,
    pub attr_units: Vec<AttrUnit>,
    pub tags: Vec<Tag>,
    bom: bool,
    nl_pos: usize,
    ticks: Option<crate::codespan::Spans>,
    last_nl: Option<usize>,
}

/// Longest text before a unit on its line that is still tested as a markup prefix.
const MAX_LEAD: usize = 4096;

fn placeholder_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\u{27E6}/?[a-z]?\d+\u{27E7}").expect("static pattern"))
}

fn unit_cap(count: usize, max: usize) -> Result<(), Error> {
    if count > max {
        return Err(LimitError::new(LimitKind::Units, format!("This file has more than {max} segments. Split it into smaller files.")).into());
    }
    Ok(())
}

fn tag_open_at(t: &str, i: usize) -> bool {
    t.as_bytes().get(i) == Some(&b'<') && t.as_bytes().get(i + 1).is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, b'_' | b'$' | b'/' | b'>'))
}

/// Kind and name of one JSX tag from its source text.
pub(crate) fn tag_raw(raw: &str) -> (bool, bool, String) {
    let closing = raw.starts_with("</");
    let self_closing = !closing && trim_end(&raw[..raw.len() - 1]).ends_with('/');
    let mut r = &raw[1..];
    r = r.strip_prefix('/').unwrap_or(r);
    r = r.trim_start_matches(is_space);
    let name: String = r.chars().take_while(|&c| !(is_space(c) || matches!(c, '/' | '>' | '{'))).collect();
    (closing, self_closing, name)
}

enum P {
    Text(usize, usize),
    Expr(usize, usize),
    Tag(Tag),
}

impl<'a> Walk<'a> {
    pub fn new(
        text: &'a str,
        allow: &'a HashSet<String>,
        fm_keys: &HashSet<String>,
        max_units: usize,
        bom: bool,
    ) -> Result<Walk<'a>, Error> {
        let lines = split_lines(text);
        let starts: Vec<usize> = lines.iter().map(|l| l.start).collect();
        let ends: Vec<usize> = lines.iter().map(|l| l.end).collect();
        let bodies: Vec<&str> = lines.iter().map(|l| &text[l.start..l.end]).collect();
        let fm = front::find_front_matter(&bodies, fm_keys);
        let mut w = Walk {
            text,
            allow,
            max_units,
            sc: Scanner::new(text),
            starts,
            ends,
            fm,
            units: Vec::new(),
            attr_units: Vec::new(),
            tags: Vec::new(),
            bom,
            nl_pos: 0,
            ticks: None,
            last_nl: None,
        };
        w.run(&bodies)?;
        Ok(w)
    }

    fn fm_len(&self) -> usize {
        self.fm.as_ref().map_or(0, |f| f.fields.len())
    }

    fn cap(&self) -> Result<(), Error> {
        unit_cap(self.fm_len() + self.units.len() + self.attr_units.len(), self.max_units)
    }

    fn line_end_at(&self, offset: usize) -> usize {
        let li = self.starts.partition_point(|&s| s <= offset);
        if li == 0 {
            0
        } else {
            self.ends[li - 1]
        }
    }

    fn at_eol(&self, mut j: usize) -> bool {
        let b = self.text.as_bytes();
        while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
            j += 1;
        }
        j >= b.len() || self.text[j..].chars().next().is_some_and(is_eol_char)
    }

    fn record_attrs(&mut self, tag: &Tag, line: Option<usize>) -> Result<(), Error> {
        self.tags.push(tag.clone());
        for (name, s, e, q) in &tag.attrs {
            if self.allow.contains(name) && has_alnum(&self.text[*s..*e]) {
                self.attr_units.push(AttrUnit { name: name.clone(), start: *s, end: *e, quote: *q, line });
                self.cap()?;
            }
        }
        Ok(())
    }

    fn run(&mut self, bodies: &[&str]) -> Result<(), Error> {
        let t = self.text;
        let n_lines = self.starts.len();
        let mut consumed = 0usize;
        let mut in_fence = false;
        let mut para = false;
        let mut li = self.fm.as_ref().map_or(0, |f| f.end);
        let mut comments = CommentBlocks::new(bodies);
        while li < n_lines {
            let (s, c) = (self.starts[li], self.ends[li]);
            if s < consumed {
                li += 1;
                continue;
            }
            let bs = if li == 0 && t.starts_with('\u{feff}') { s + 3 } else { s };
            let body = &t[bs..c];
            if mdline::is_fence(body) {
                in_fence = !in_fence;
                para = false;
                li += 1;
                continue;
            }
            if in_fence || trim(body).is_empty() || mdline::is_skip_line(body) {
                para = false;
                li += 1;
                continue;
            }
            if let Some(end) = comments.end(li) {
                para = false;
                li = end + 1;
                continue;
            }
            let rd = if para { None } else { mdline::link_ref_def(bodies, li) };
            if let Some(rd) = rd {
                if let Some((tl, ts, te, delim)) = rd.title {
                    let ls = self.starts[tl];
                    let title = mdline::decode_link_title(&t[ls + ts..ls + te]);
                    if has_alnum(&title) {
                        self.units.push(LineUnit {
                            start: ls + ts,
                            end: ls + te,
                            items: Vec::new(),
                            toks: Vec::new(),
                            delim: Some(delim),
                            title,
                            prefix: String::new(),
                            block_ok: false,
                        });
                        self.cap()?;
                    }
                }
                para = false;
                li = rd.end + 1;
                continue;
            }
            if !para && (body.starts_with("import ") || body.starts_with("import\t") || body.starts_with("export ") || body.starts_with("export\t")) {
                li += 1;
                while li < n_lines && !trim(&t[self.starts[li]..self.ends[li]]).is_empty() {
                    li += 1;
                }
                continue;
            }
            let first = bs + body.len() - body.trim_start_matches([' ', '\t']).len();
            if t.as_bytes()[first] == b'{' {
                if let Some(e) = self.sc.expr(first)? {
                    if self.at_eol(e) {
                        consumed = e;
                        para = false;
                        li += 1;
                        continue;
                    }
                }
            }
            if tag_open_at(t, first) {
                let tags = self.flow_tags(first)?;
                if !tags.is_empty() {
                    for tag in &tags {
                        self.record_attrs(tag, None)?;
                    }
                    consumed = tags.last().unwrap().end;
                    para = false;
                    li += 1;
                    continue;
                }
            }
            let mut plen = mdline::prefix_len(body);
            let heading = body[..plen].contains('#');
            if let Some(fe) = mdline::footnote_def(body, plen) {
                plen = fe;
            }
            consumed = self.text_line(bs + plen, c, &body[..plen], bs)?;
            para = !heading;
            li += 1;
        }
        Ok(())
    }

    /// Tags that fill a line up to its end (flow JSX), else empty.
    fn flow_tags(&mut self, mut j: usize) -> Result<Vec<Tag>, Error> {
        let mut tags = Vec::new();
        loop {
            let Some(tag) = self.sc.tag(j)? else { return Ok(Vec::new()) };
            j = tag.end;
            tags.push(tag);
            if self.at_eol(j) {
                return Ok(tags);
            }
            let b = self.text.as_bytes();
            while b[j] == b' ' || b[j] == b'\t' {
                j += 1;
            }
            if !tag_open_at(self.text, j) {
                return Ok(Vec::new());
            }
        }
    }

    fn text_line(&mut self, cs: usize, mut end: usize, prefix: &str, line_bs: usize) -> Result<usize, Error> {
        let t = self.text;
        let b = t.as_bytes();
        let mut parts: Vec<P> = Vec::new();
        let mut j = cs;
        while j < end {
            let Some(p) = b[j..end].iter().position(|c| matches!(c, b'\\' | b'`' | b'{' | b'<')) else {
                parts.push(P::Text(j, end));
                break;
            };
            let k = j + p;
            if k > j {
                parts.push(P::Text(j, k));
            }
            match b[k] {
                b'\\' => {
                    let nx = if k + 1 < end { t[k + 1..].chars().next().map_or(0, char::len_utf8) } else { 0 };
                    let e2 = (k + 1 + nx).min(end);
                    parts.push(P::Text(k, e2));
                    j = e2;
                }
                b'`' => {
                    let mut run = 0;
                    while k + run < end && b[k + run] == b'`' {
                        run += 1;
                    }
                    let sp = self.ticks.get_or_insert_with(|| crate::codespan::Spans::new(t, false));
                    let stop = sp.close_run(run, k + run, end).unwrap_or(k + run);
                    parts.push(P::Text(k, stop));
                    j = stop;
                }
                b'{' => match self.sc.expr(k)? {
                    None => {
                        parts.push(P::Text(k, k + 1));
                        j = k + 1;
                    }
                    Some(e) => {
                        parts.push(P::Expr(k, e));
                        end = end.max(self.line_end_at(e - 1));
                        j = e;
                    }
                },
                _ => {
                    let tag = if tag_open_at(t, k) { self.sc.tag(k)? } else { None };
                    match tag {
                        None => {
                            parts.push(P::Text(k, k + 1));
                            j = k + 1;
                        }
                        Some(tag) => {
                            end = end.max(self.line_end_at(tag.end - 1));
                            j = tag.end;
                            parts.push(P::Tag(tag));
                        }
                    }
                }
            }
        }
        let mut core_end = end;
        if let Some(P::Text(s, e)) = parts.last_mut() {
            let kept = trim_end(&t[*s..*e]).len();
            core_end -= *e - (*s + kept);
            *e = *s + kept;
        }
        let (items, toks) = self.tokens(&parts);
        let has_text = items.iter().any(|it| match it {
            Item::Text(s, e) => has_alnum(&placeholder_re().replace_all(&t[*s..*e], "")),
            _ => false,
        });
        let mut index = None;
        if has_text {
            index = Some(self.units.len());
            // text before the unit on its (LF delimited) line; units come in document order, so the last LF is
            // tracked with a cursor instead of searched for again (a file with no LF would be quadratic)
            while self.nl_pos < cs {
                if self.text.as_bytes()[self.nl_pos] == b'\n' {
                    self.last_nl = Some(self.nl_pos);
                }
                self.nl_pos += 1;
            }
            let lead_start = self.last_nl.map_or(0, |p| p + 1);
            let lead = &t[lead_start..cs];
            // a lead this long cannot be a markup prefix in any real file (LocOS would still test it)
            let block_ok = lead.len() <= MAX_LEAD && {
                let lead = mdline::strip_footnote_lead(lead);
                !(lead.contains('#') || self.bom && lead_start == 0) && mdline::prefix_full(&lead)
            };
            let _ = line_bs;
            self.units.push(LineUnit {
                start: cs,
                end: core_end,
                items,
                toks,
                delim: None,
                title: String::new(),
                prefix: prefix.to_string(),
                block_ok,
            });
            self.cap()?;
        }
        for p in &parts {
            if let P::Tag(tag) = p {
                self.record_attrs(tag, index)?;
            }
        }
        Ok(end)
    }

    /// Pairs inline tags by name; unpaired tags and expressions become point tokens.
    fn tokens(&self, parts: &[P]) -> (Vec<Item>, Vec<Tok>) {
        let mut partner: Vec<Option<usize>> = vec![None; parts.len()];
        let mut stack: Vec<(usize, String)> = Vec::new();
        for (pi, p) in parts.iter().enumerate() {
            if let P::Tag(tag) = p {
                let (closing, self_closing, name) = tag_raw(&self.text[tag.start..tag.end]);
                if closing {
                    if let Some(si) = stack.iter().rposition(|(_, n)| *n == name) {
                        let o = stack[si].0;
                        partner[o] = Some(pi);
                        partner[pi] = Some(o);
                        stack.truncate(si);
                    }
                } else if !self_closing {
                    stack.push((pi, name));
                }
            }
        }
        let mut items = Vec::new();
        let mut toks: Vec<Tok> = Vec::new();
        let mut tok_of: Vec<usize> = vec![usize::MAX; parts.len()];
        for (pi, p) in parts.iter().enumerate() {
            let (s, e) = match p {
                P::Text(s, e) => {
                    items.push(Item::Text(*s, *e));
                    continue;
                }
                P::Expr(s, e) => (*s, *e),
                P::Tag(t) => (t.start, t.end),
            };
            match partner[pi] {
                Some(o) if o < pi => {
                    let ti = tok_of[o];
                    toks[ti].close = Some((s, e));
                    items.push(Item::Close(ti));
                }
                other => {
                    let id = toks.len() + 1;
                    toks.push(Tok { id, open: (s, e), close: None });
                    tok_of[pi] = toks.len() - 1;
                    items.push(if other.is_some() { Item::Open(toks.len() - 1) } else { Item::Point(toks.len() - 1) });
                }
            }
        }
        (items, toks)
    }
}
