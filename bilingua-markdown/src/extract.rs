//! Markdown to units. Only text ranges become units; every other byte stays in the skeleton.
//!
//! The parser (pulldown-cmark) gives each event a byte range. A unit covers the range from the first to the
//! last inline event of one block (paragraph, heading, list item, table cell). Inside it, text is kept as the
//! raw source slice (escapes and entities untouched), and every markup construct becomes a tag whose code holds
//! the original bytes. After a unit is built, its source is written back and compared with the original bytes;
//! a unit that does not match exactly is dropped to the skeleton, so the file always round-trips.

use crate::frontmatter;
use bilingua_model::*;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::BTreeMap;
use std::ops::Range;

/// Marks a place inside a code where another unit's text is written (alt text, link title).
pub const MARK_OPEN: char = '\u{E000}';
pub const MARK_CLOSE: char = '\u{E001}';
/// Key in `Unit::codes` holding the original bytes of the unit (front matter values).
pub const RAW_KEY: &str = "$raw";
/// Key in `Unit::codes` marking a front matter value; its `label` is the YAML style.
pub const FM_KEY: &str = "$fm";

pub fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_FOOTNOTES
}

struct Pending {
    text: String,
    title: bool,
}

struct Frame {
    id: String,
}

struct Block {
    kind: String,
    style: Option<String>,
    start: usize,
    end: usize,
    cursor: usize,
    inl: Vec<Inline>,
    text: String,
    codes: BTreeMap<String, CodeData>,
    next_id: usize,
    stack: Vec<Frame>,
    pending: Vec<Pending>,
    bad: bool,
}

impl Block {
    fn new(kind: &str, style: Option<String>, start: usize) -> Block {
        Block {
            kind: kind.into(),
            style,
            start,
            end: start,
            cursor: start,
            inl: Vec::new(),
            text: String::new(),
            codes: BTreeMap::new(),
            next_id: 1,
            stack: Vec::new(),
            pending: Vec::new(),
            bad: false,
        }
    }

    fn flush_text(&mut self) {
        if !self.text.is_empty() {
            self.inl.push(Inline::Text(std::mem::take(&mut self.text)));
        }
    }

    fn new_id(&mut self) -> String {
        let id = self.next_id.to_string();
        self.next_id += 1;
        id
    }

    /// Text from the cursor up to `to`.
    fn text_to(&mut self, src: &str, to: usize) {
        if to < self.cursor {
            self.bad = true;
            return;
        }
        match src.get(self.cursor..to) {
            Some(s) => self.text.push_str(s),
            None => self.bad = true,
        }
        self.cursor = to;
    }

    fn standalone(&mut self, display: &str, code: String, label: &str) {
        self.flush_text();
        let id = self.new_id();
        self.codes.insert(id.clone(), CodeData { standalone: Some(code), label: Some(label.into()), ..Default::default() });
        self.inl.push(Inline::tag(id, TagKind::Standalone, display));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Ctx {
    Heading(u8),
    Paragraph,
    Item,
    Cell,
    Other,
}

pub struct Extracted {
    pub units: Vec<Unit>,
    pub parts: Vec<SkelPart>,
}

struct Ex<'a> {
    full: &'a str,
    /// Offset of the body (after front matter) in `full`.
    off: usize,
    body: &'a str,
    units: Vec<Unit>,
    parts: Vec<SkelPart>,
    raw_cursor: usize,
    last_heading: Option<String>,
    newlines: Vec<usize>,
}

pub fn extract(full: &str, include_locked: bool) -> Extracted {
    let _ = include_locked;
    let mut newlines = vec![];
    for (i, b) in full.bytes().enumerate() {
        if b == b'\n' {
            newlines.push(i);
        }
    }
    let fm = frontmatter::parse(full);
    let off = fm.as_ref().map(|f| f.end).unwrap_or(0);
    let mut ex = Ex { full, off, body: &full[off..], units: vec![], parts: vec![], raw_cursor: 0, last_heading: None, newlines };
    if let Some(fm) = fm {
        for v in fm.values {
            let token = full[v.range.clone()].to_string();
            let mut u = Unit::new(format!("u{}", ex.units.len() + 1), vec![Inline::Text(v.text.clone())]);
            u.context.kind = Some(v.key.clone());
            u.context.key = Some(v.key.clone());
            u.context.location = Some(format!("line {}", ex.line(v.range.start)));
            u.codes.insert(RAW_KEY.into(), CodeData { standalone: Some(token), ..Default::default() });
            u.codes.insert(FM_KEY.into(), CodeData { label: Some(v.style.name().into()), ..Default::default() });
            ex.add_unit_in_skeleton(u, v.range.start, v.range.end);
        }
    }
    ex.parse_body();
    ex.parts.push(SkelPart::Raw(full[ex.raw_cursor..].to_string()));
    ex.parts.retain(|p| !matches!(p, SkelPart::Raw(s) if s.is_empty()));
    Extracted { units: ex.units, parts: ex.parts }
}

impl<'a> Ex<'a> {
    fn line(&self, abs: usize) -> usize {
        self.newlines.partition_point(|n| *n < abs) + 1
    }

    fn add_unit_in_skeleton(&mut self, unit: Unit, abs_start: usize, abs_end: usize) {
        self.parts.push(SkelPart::Raw(self.full[self.raw_cursor..abs_start].to_string()));
        self.parts.push(SkelPart::Unit(self.units.len()));
        self.units.push(unit);
        self.raw_cursor = abs_end;
    }

    fn kind_for(ctx: &[Ctx]) -> (&'static str, Option<String>) {
        match ctx.last() {
            Some(Ctx::Heading(n)) => ("heading", Some(format!("h{n}"))),
            Some(Ctx::Cell) => ("cell", None),
            Some(Ctx::Item) => ("list-item", None),
            Some(Ctx::Paragraph) => {
                if ctx.len() >= 2 && ctx[ctx.len() - 2] == Ctx::Item {
                    ("list-item", None)
                } else {
                    ("paragraph", None)
                }
            }
            _ => ("paragraph", None),
        }
    }

    fn parse_body(&mut self) {
        let src = self.body;
        let evs: Vec<(Event, Range<usize>)> = Parser::new_ext(src, options()).into_offset_iter().collect();
        let mut ctx: Vec<Ctx> = Vec::new();
        let mut cur: Option<Block> = None;
        let mut skip = 0usize; // inside a code block or HTML block
        let mut i = 0usize;
        while i < evs.len() {
            let (ev, r) = &evs[i];
            match ev {
                Event::Start(Tag::CodeBlock(_)) | Event::Start(Tag::HtmlBlock) => {
                    self.finish(cur.take());
                    skip += 1;
                }
                Event::End(TagEnd::CodeBlock) | Event::End(TagEnd::HtmlBlock) => skip = skip.saturating_sub(1),
                _ if skip > 0 => {}
                Event::Start(Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. }) => {
                    let consumed = self.inline_start(&evs, i, &mut cur, &ctx);
                    i = consumed;
                    continue;
                }
                Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link) => {
                    if let Some(b) = cur.as_mut() {
                        end_container(b, src, &evs, i);
                    }
                }
                Event::Start(tag) => {
                    self.finish(cur.take());
                    ctx.push(match tag {
                        Tag::Heading { level, .. } => Ctx::Heading(*level as u8),
                        Tag::Paragraph => Ctx::Paragraph,
                        Tag::Item => Ctx::Item,
                        Tag::TableCell => Ctx::Cell,
                        _ => Ctx::Other,
                    });
                }
                Event::End(_) => {
                    self.finish(cur.take());
                    ctx.pop();
                }
                Event::Rule => self.finish(cur.take()),
                Event::TaskListMarker(_) => {}
                Event::Text(_) if r.is_empty() => {}
                Event::Text(_) => {
                    let b = ensure(&mut cur, &ctx, r.start);
                    b.text_to(src, r.end);
                    b.end = b.end.max(r.end);
                }
                Event::Code(_) => {
                    let b = ensure(&mut cur, &ctx, r.start);
                    let shown = src.get(r.clone()).unwrap_or("code").to_string();
                    leaf_tag(b, src, r, &shown, "code");
                }
                Event::InlineHtml(_) => {
                    let b = ensure(&mut cur, &ctx, r.start);
                    leaf_tag(b, src, r, "html", "html");
                }
                Event::FootnoteReference(_) => {
                    let b = ensure(&mut cur, &ctx, r.start);
                    leaf_tag(b, src, r, "footnote", "footnote");
                }
                Event::SoftBreak | Event::HardBreak => {
                    let b = ensure(&mut cur, &ctx, r.start);
                    let hard = matches!(ev, Event::HardBreak);
                    let next = evs.get(i + 1).map(|e| e.1.start).unwrap_or(r.end);
                    if next > r.end {
                        // text between the break and the next piece is a line prefix such as "> " or indentation
                        b.text_to(src, r.start);
                        match src.get(r.start..next) {
                            Some(code) => b.standalone("\u{21b5}", code.to_string(), "line break"),
                            None => b.bad = true,
                        }
                        b.cursor = next;
                        b.end = b.end.max(next);
                    } else if hard {
                        b.text_to(src, r.start);
                        match src.get(r.start..r.end) {
                            Some(code) => b.standalone("<br>", code.to_string(), "line break"),
                            None => b.bad = true,
                        }
                        b.cursor = r.end;
                        b.end = b.end.max(r.end);
                    } else {
                        b.text_to(src, r.end);
                        b.end = b.end.max(r.end);
                    }
                }
                // anything else (math, html outside blocks) is left in the skeleton
                _ => {
                    self.finish(cur.take());
                }
            }
            i += 1;
        }
        self.finish(cur.take());
    }

    /// Handles Start of emphasis, strong, strikethrough, link and image. Returns the next event index.
    fn inline_start(&mut self, evs: &[(Event, Range<usize>)], i: usize, cur: &mut Option<Block>, ctx: &[Ctx]) -> usize {
        let src = self.body;
        let (ev, r) = &evs[i];
        let b = ensure(cur, ctx, r.start);
        match ev {
            Event::Start(Tag::Emphasis) => open_container(b, src, evs, i, "<i>", "italic"),
            Event::Start(Tag::Strong) => open_container(b, src, evs, i, "<b>", "bold"),
            Event::Start(Tag::Strikethrough) => open_container(b, src, evs, i, "<s>", "strikethrough"),
            Event::Start(Tag::Link { link_type, .. }) => {
                use pulldown_cmark::LinkType;
                if matches!(link_type, LinkType::Autolink | LinkType::Email) {
                    let end = matching_end(evs, i);
                    leaf_tag(b, src, r, &src.get(r.clone()).unwrap_or("<link>").to_string(), "link");
                    return end + 1;
                }
                open_container(b, src, evs, i, "<a>", "link")
            }
            Event::Start(Tag::Image { .. }) => {
                let end = matching_end(evs, i);
                image(b, src, evs, i, end);
                return end + 1;
            }
            _ => {}
        }
        i + 1
    }

    fn finish(&mut self, block: Option<Block>) {
        let Some(mut b) = block else { return };
        if b.bad || !b.stack.is_empty() {
            return;
        }
        if b.cursor < b.end {
            let to = b.end;
            b.text_to(self.body, to);
        }
        b.flush_text();
        let Some(region) = self.body.get(b.start..b.end) else { return };
        // The unit's source must write back to exactly the original bytes.
        if render_check(&b.inl, &b.codes, &b.pending) != region {
            return;
        }
        let has_text = plain_text(&b.inl).chars().any(|c| c.is_alphabetic());
        if !has_text && b.pending.is_empty() {
            return;
        }
        let abs_start = self.off + b.start;
        let abs_end = self.off + b.end;
        let base = self.units.len();
        // Global unit index of pending item j is base + 1 + j.
        for code in b.codes.values_mut() {
            for s in [&mut code.open, &mut code.close, &mut code.standalone].into_iter().flatten() {
                *s = globalise(s, base + 1);
            }
        }
        let line = format!("line {}", self.line(abs_start));
        let mut unit = Unit::new(format!("u{}", base + 1), b.inl);
        unit.codes = b.codes;
        unit.context.kind = Some(if has_text { b.kind.clone() } else { "image".into() });
        unit.context.style = b.style.clone();
        unit.context.location = Some(line.clone());
        unit.translate = has_text;
        if b.kind == "heading" {
            self.last_heading = Some(plain_text(&unit.segments[0].source).trim().to_string());
        } else {
            unit.context.heading = self.last_heading.clone();
        }
        let heading = unit.context.heading.clone();
        self.add_unit_in_skeleton(unit, abs_start, abs_end);
        for p in b.pending {
            let mut u = Unit::new(format!("u{}", self.units.len() + 1), vec![Inline::Text(p.text)]);
            u.context.kind = Some(if p.title { "title" } else { "alt" }.into());
            u.context.location = Some(line.clone());
            u.context.heading = heading.clone();
            self.units.push(u);
        }
    }
}

fn ensure<'b>(cur: &'b mut Option<Block>, ctx: &[Ctx], start: usize) -> &'b mut Block {
    cur.get_or_insert_with(|| {
        let (kind, style) = Ex::kind_for(ctx);
        Block::new(kind, style, start)
    })
}

/// A construct that is one tag: code span, inline HTML, footnote reference, autolink.
fn leaf_tag(b: &mut Block, src: &str, r: &Range<usize>, display: &str, label: &str) {
    b.text_to(src, r.start);
    match src.get(r.clone()) {
        Some(code) => b.standalone(display, code.to_string(), label),
        None => b.bad = true,
    }
    b.cursor = r.end;
    b.end = b.end.max(r.end);
}

/// Index of the End event that closes the Start at `i` (same kind, nesting counted).
fn matching_end(evs: &[(Event, Range<usize>)], i: usize) -> usize {
    let is_image = matches!(evs[i].0, Event::Start(Tag::Image { .. }));
    let mut depth = 0usize;
    for (j, (e, _)) in evs.iter().enumerate().skip(i) {
        match e {
            Event::Start(Tag::Image { .. }) if is_image => depth += 1,
            Event::Start(Tag::Link { .. }) if !is_image => depth += 1,
            Event::End(TagEnd::Image) if is_image => {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            Event::End(TagEnd::Link) if !is_image => {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            _ => {}
        }
    }
    evs.len() - 1
}

fn open_container(b: &mut Block, src: &str, evs: &[(Event, Range<usize>)], i: usize, display: &str, label: &str) {
    let r = &evs[i].1;
    let first = evs.get(i + 1).map(|e| e.1.start).unwrap_or(r.end);
    b.text_to(src, r.start);
    let Some(open) = src.get(r.start..first) else {
        b.bad = true;
        return;
    };
    if first <= r.start || first > r.end {
        b.bad = true;
        return;
    }
    b.flush_text();
    let id = b.new_id();
    b.codes.insert(id.clone(), CodeData { open: Some(open.to_string()), label: Some(label.into()), ..Default::default() });
    b.inl.push(Inline::tag(id.clone(), TagKind::Open, display));
    b.stack.push(Frame { id });
    b.cursor = first;
}

fn end_container(b: &mut Block, src: &str, evs: &[(Event, Range<usize>)], i: usize) {
    let r = &evs[i].1;
    let Some(frame) = b.stack.pop() else {
        b.bad = true;
        return;
    };
    // end of the last child: the previous non-empty event
    let last_end = evs[..i].iter().rev().find(|e| !e.1.is_empty()).map(|e| e.1.end).unwrap_or(r.start);
    b.text_to(src, last_end);
    let Some(close) = src.get(last_end..r.end) else {
        b.bad = true;
        return;
    };
    let mut close = close.to_string();
    // a link title becomes its own unit
    if let Event::End(TagEnd::Link) = &evs[i].0 {
        if let Some((a, z)) = find_title(&close) {
            let text = close[a..z].to_string();
            if !text.trim().is_empty() {
                let j = b.pending.len();
                close = format!("{}{}t{}{}{}", &close[..a], MARK_OPEN, j, MARK_CLOSE, &close[z..]);
                b.pending.push(Pending { text, title: true });
            }
        }
    }
    b.flush_text();
    if let Some(c) = b.codes.get_mut(&frame.id) {
        c.close = Some(close);
    }
    let display = match b.codes.get(&frame.id).and_then(|c| c.label.as_deref()) {
        Some("bold") => "</b>",
        Some("italic") => "</i>",
        Some("strikethrough") => "</s>",
        _ => "</a>",
    };
    b.inl.push(Inline::tag(frame.id, TagKind::Close, display));
    b.cursor = r.end;
    b.end = b.end.max(r.end);
}

/// An image is one standalone tag; its alt text is a separate unit written back through a marker.
fn image(b: &mut Block, src: &str, evs: &[(Event, Range<usize>)], i: usize, end: usize) {
    let r = &evs[i].1;
    b.text_to(src, r.start);
    let Some(whole) = src.get(r.clone()) else {
        b.bad = true;
        return;
    };
    let children: Vec<&Range<usize>> = evs[i + 1..end].iter().map(|e| &e.1).filter(|x| !x.is_empty()).collect();
    let mut code = whole.to_string();
    if let (Some(first), Some(last)) = (children.first(), children.iter().map(|c| c.end).max()) {
        if first.start >= r.start && last <= r.end && first.start < last {
            let alt = src[first.start..last].to_string();
            if !alt.trim().is_empty() {
                let j = b.pending.len();
                let (a, z) = (first.start - r.start, last - r.start);
                code = format!("{}{}a{}{}{}", &whole[..a], MARK_OPEN, j, MARK_CLOSE, &whole[z..]);
                b.pending.push(Pending { text: alt, title: false });
            }
        }
    }
    b.standalone("<img>", code, "image");
    b.cursor = r.end;
    b.end = b.end.max(r.end);
}

/// Finds the title inside the tail of an inline link, `](url "title")`. Returns the byte range of the title.
fn find_title(tail: &str) -> Option<(usize, usize)> {
    let b = tail.as_bytes();
    if !tail.starts_with("](") || !tail.ends_with(')') {
        return None;
    }
    let mut i = 2;
    let ws = |i: &mut usize| {
        while *i < b.len() && matches!(b[*i], b' ' | b'\t' | b'\n' | b'\r') {
            *i += 1;
        }
    };
    ws(&mut i);
    // destination
    if b.get(i) == Some(&b'<') {
        while i < b.len() && b[i] != b'>' {
            i += 1;
        }
        i += 1;
    } else {
        let mut depth = 0i32;
        while i < b.len() {
            match b[i] {
                b'\\' => i += 1,
                b'(' => depth += 1,
                b')' => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                b' ' | b'\t' | b'\n' | b'\r' => break,
                _ => {}
            }
            i += 1;
        }
    }
    let before_ws = i;
    ws(&mut i);
    if i == before_ws {
        return None; // a title must be separated from the destination
    }
    let open = *b.get(i)?;
    let close = match open {
        b'"' => b'"',
        b'\'' => b'\'',
        b'(' => b')',
        _ => return None,
    };
    let start = i + 1;
    let mut j = start;
    while j < b.len() {
        if b[j] == b'\\' {
            j += 2;
            continue;
        }
        if b[j] == close {
            break;
        }
        j += 1;
    }
    if j >= b.len() {
        return None;
    }
    let mut k = j + 1;
    ws(&mut k);
    if k + 1 != b.len() {
        return None;
    }
    tail.get(start..j).map(|_| (start, j))
}

/// Replaces local marker indexes (`a0`, `t1`) with global unit indexes.
fn globalise(s: &str, first: usize) -> String {
    map_markers(s, |kind, j| Some(format!("{MARK_OPEN}{kind}{}{MARK_CLOSE}", first + j)))
}

/// Calls `f(kind, index)` for every marker in `s` and substitutes its result. Text without markers is copied.
pub fn map_markers(s: &str, mut f: impl FnMut(char, usize) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(a) = rest.find(MARK_OPEN) {
        out.push_str(&rest[..a]);
        let after = &rest[a + MARK_OPEN.len_utf8()..];
        let Some(z) = after.find(MARK_CLOSE) else {
            out.push_str(&rest[a..]);
            return out;
        };
        let body = &after[..z];
        let mut chars = body.chars();
        let kind = chars.next().unwrap_or('a');
        let idx: usize = chars.as_str().parse().unwrap_or(0);
        match f(kind, idx) {
            Some(t) => out.push_str(&t),
            None => out.push_str(&rest[a..a + MARK_OPEN.len_utf8() + z + MARK_CLOSE.len_utf8()]),
        }
        rest = &after[z + MARK_CLOSE.len_utf8()..];
    }
    out.push_str(rest);
    out
}

/// What the unit writes back for its source, with pending alt and title text in place of markers.
fn render_check(inl: &[Inline], codes: &BTreeMap<String, CodeData>, pending: &[Pending]) -> String {
    let mut out = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => out.push_str(t),
            Inline::Tag { id, kind, .. } => {
                let piece = codes.get(id).and_then(|c| match kind {
                    TagKind::Open => c.open.as_deref(),
                    TagKind::Close => c.close.as_deref(),
                    TagKind::Standalone => c.standalone.as_deref(),
                });
                if let Some(p) = piece {
                    out.push_str(&map_markers(p, |_, j| pending.get(j).map(|x| x.text.clone())));
                }
            }
        }
    }
    out
}
