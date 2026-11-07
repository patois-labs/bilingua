//! Writing units back as Markdown, and the HTML preview.

use crate::extract::{map_markers, options, FM_KEY, RAW_KEY};
use crate::frontmatter::{self, Style};
use bilingua_model::*;
use pulldown_cmark::{CowStr, Event, Parser, Tag};

/// Markdown for a list of inlines of `unit`, with alt and title markers filled from the other units.
pub fn inlines_md(doc: &Document, unit: &Unit, inl: &[Inline]) -> String {
    let mut out = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => out.push_str(t),
            Inline::Tag { id, kind, .. } => {
                let piece = unit.codes.get(id).and_then(|c| match kind {
                    TagKind::Open => c.open.as_deref(),
                    TagKind::Close => c.close.as_deref(),
                    TagKind::Standalone => c.standalone.as_deref(),
                });
                if let Some(p) = piece {
                    out.push_str(&map_markers(p, |kind, idx| Some(side_text(doc, kind, idx))));
                }
            }
        }
    }
    out
}

/// Markdown for a translation. Like `inlines_md`, but an emphasis whose `*` or `_` marks would no longer read as
/// emphasis next to the translated text is written as `<em>` / `<strong>` instead. CommonMark only lets a `*` run
/// open emphasis when the character after it is not blank (and not punctuation unless a blank or punctuation
/// comes before it), and the mirror rule closes it; `_` is stricter. The source had text the rules allowed, but a
/// translation can put a letter next to a mark that follows punctuation (`**（1）x？**a` or `_(x)_b`), and the
/// marks would then print as stars. `before` is the character written just before the unit.
pub fn inlines_md_translated(doc: &Document, unit: &Unit, inl: &[Inline], before: Option<char>) -> String {
    let mut parts: Vec<(Option<(&str, TagKind)>, String)> = Vec::with_capacity(inl.len());
    for i in inl {
        match i {
            Inline::Text(t) => parts.push((None, t.clone())),
            Inline::Tag { id, kind, .. } => {
                let piece = unit.codes.get(id).and_then(|c| match kind {
                    TagKind::Open => c.open.as_deref(),
                    TagKind::Close => c.close.as_deref(),
                    TagKind::Standalone => c.standalone.as_deref(),
                });
                let s = piece.map(|p| map_markers(p, |kind, idx| Some(side_text(doc, kind, idx)))).unwrap_or_default();
                parts.push((Some((id.as_str(), *kind)), s));
            }
        }
    }
    let is_mark = |s: &str| !s.is_empty() && s.len() <= 2 && (s.bytes().all(|b| b == b'*') || s.bytes().all(|b| b == b'_'));
    let mut bad: Vec<&str> = Vec::new();
    let mut keep: Vec<&str> = Vec::new();
    // a backtick in the text may open a code span that swallows a tag: such a unit keeps its marks as they were
    let ticks = parts.iter().any(|(t, p)| t.is_none() && p.contains('`'));
    for (n, (tag, s)) in parts.iter().enumerate() {
        if ticks {
            break;
        }
        let Some((id, kind)) = tag else { continue };
        if !matches!(kind, TagKind::Open | TagKind::Close) || !is_mark(s) {
            continue;
        }
        let prev = parts[..n].iter().rev().find_map(|(_, p)| p.chars().next_back()).or(before);
        let next = parts[n + 1..].iter().find_map(|(_, p)| p.chars().next());
        // a backslash right before the mark is the translation's own escape: the mark is left as it was
        let last = parts[..n].iter().rev().find(|(_, p)| !p.is_empty()).map_or("", |(_, p)| p.as_str());
        if last.chars().rev().take_while(|&c| c == '\\').count() % 2 == 1 {
            keep.push(id);
            continue;
        }
        let (left, right) = md_flanks(prev, next);
        let punct_before = prev.is_some_and(md_punct);
        let punct_after = next.is_some_and(md_punct);
        let ok = match (s.as_bytes()[0], kind) {
            (b'*', TagKind::Open) => left,
            (b'*', _) => right,
            (_, TagKind::Open) => left && (!right || punct_before),
            (_, _) => right && (!left || punct_after),
        };
        if !ok && !bad.contains(id) {
            bad.push(id);
        }
    }
    bad.retain(|id| !keep.contains(id));
    let mut out = String::new();
    for (tag, s) in parts.iter() {
        match tag {
            Some((id, kind)) if bad.contains(id) && is_mark(s) && matches!(kind, TagKind::Open | TagKind::Close) => {
                let name = if s.len() == 2 { "strong" } else { "em" };
                out.push_str(&if *kind == TagKind::Open { format!("<{name}>") } else { format!("</{name}>") });
            }
            _ => out.push_str(s),
        }
    }
    out
}

/// Punctuation as CommonMark counts it for emphasis: ASCII punctuation and the Unicode P and S categories.
fn md_punct(c: char) -> bool {
    c.is_ascii_punctuation() || (!c.is_alphanumeric() && !c.is_whitespace() && !c.is_control() && !c.is_ascii() && !matches!(c, '\u{200B}'..='\u{200F}') && is_unicode_punct(c))
}

fn is_unicode_punct(c: char) -> bool {
    matches!(c,
        '\u{00A1}'..='\u{00BF}' | '\u{2010}'..='\u{2027}' | '\u{2030}'..='\u{205E}' | '\u{20A0}'..='\u{20CF}' | '\u{2190}'..='\u{2BFF}'
        | '\u{3001}'..='\u{3003}' | '\u{3008}'..='\u{3011}' | '\u{3014}'..='\u{301F}' | '\u{3030}' | '\u{30FB}'
        | '\u{FE10}'..='\u{FE1F}' | '\u{FE30}'..='\u{FE6F}' | '\u{FF01}'..='\u{FF0F}' | '\u{FF1A}'..='\u{FF20}'
        | '\u{FF3B}'..='\u{FF40}' | '\u{FF5B}'..='\u{FF65}' | '\u{0964}' | '\u{0965}' | '\u{060C}' | '\u{061B}' | '\u{061F}'
        | '\u{1F000}'..='\u{1FAFF}')
}

/// (left-flanking, right-flanking) of a delimiter run between `prev` and `next` (None = line edge = blank).
fn md_flanks(prev: Option<char>, next: Option<char>) -> (bool, bool) {
    let blank = |c: Option<char>| c.is_none_or(|c| c.is_whitespace());
    let punct = |c: Option<char>| c.is_some_and(md_punct);
    let left = !blank(next) && (!punct(next) || blank(prev) || punct(prev));
    let right = !blank(prev) && (!punct(prev) || blank(next) || punct(next));
    (left, right)
}

/// The current text of an alt or title unit, escaped for its place. Untouched text is written as it was.
fn side_text(doc: &Document, kind: char, idx: usize) -> String {
    let Some(u) = doc.units.get(idx) else { return String::new() };
    let touched = u.segments.iter().any(|s| s.has_target());
    let text = plain_text(&u.output());
    if !touched {
        return text;
    }
    let specials: &[char] = if kind == 't' { &['"', '\'', '(', ')'] } else { &['[', ']'] };
    let mut out = String::new();
    let mut prev_backslash = false;
    for c in text.chars() {
        if specials.contains(&c) && !prev_backslash {
            out.push('\\');
        }
        prev_backslash = c == '\\' && !prev_backslash;
        out.push(c);
    }
    out
}

/// The bytes a unit writes in the skeleton.
pub fn render_unit(doc: &Document, unit: &Unit) -> String {
    if let Some(style) = unit.codes.get(FM_KEY) {
        let touched = unit.segments.iter().any(|s| s.has_target());
        if !touched {
            if let Some(raw) = unit.codes.get(RAW_KEY).and_then(|c| c.standalone.clone()) {
                return raw;
            }
        }
        let text = plain_text(&unit.output());
        return frontmatter::write(&text, Style::from_name(style.label.as_deref().unwrap_or("plain")));
    }
    inlines_md(doc, unit, &unit.output())
}

/// The block markers at the start of `line` (a block start): each a kind letter and the byte where a backslash
/// makes it text. `f` code fence (three or more backticks with no backtick after them on the line, or three or more
/// tildes), `h` ATX heading, `b` bullet, `n` list number (escaped before its `.` / `)`), `q` block quote, `r` rule
/// or setext underline (only `=`, only `-`, three or more `*` or `_`). Quote marks are followed by the markers
/// after them; up to three blanks of indent are allowed.
fn block_markers(line: &str, first: bool) -> Vec<(char, usize)> {
    let b = line.as_bytes();
    let blank = |i: usize| matches!(b.get(i), None | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r'));
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        let st = i;
        while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
            i += 1;
        }
        if i - st > 3 {
            return out;
        }
        if b.get(i) == Some(&b'>') {
            out.push(('q', i));
            i += 1;
            continue;
        }
        break;
    }
    let rest = &b[i..];
    let body: Vec<u8> = rest.iter().copied().filter(|c| !matches!(c, b' ' | b'\t' | b'\n' | b'\r')).collect();
    let run = |c: u8| rest.iter().take_while(|&&x| x == c).count();
    if let Some(&c) = body.first() {
        let same = body.iter().all(|&x| x == c);
        if same && ((matches!(c, b'=' | b'-') && !first) || (matches!(c, b'-' | b'*' | b'_') && body.len() >= 3)) {
            out.push(('r', i));
            return out;
        }
    }
    let ticks = run(b'`');
    if (ticks >= 3 && !rest[ticks..].contains(&b'`')) || run(b'~') >= 3 {
        out.push(('f', i));
    } else if (1..=6).contains(&run(b'#')) && blank(i + run(b'#')) {
        out.push(('h', i));
    } else if matches!(b.get(i), Some(b'-') | Some(b'+') | Some(b'*')) && blank(i + 1) {
        out.push(('b', i));
    } else {
        let d = rest.iter().take_while(|c| c.is_ascii_digit()).count();
        if (1..=9).contains(&d) && matches!(b.get(i + d), Some(b'.') | Some(b')')) && blank(i + d + 1) {
            out.push(('n', i + d));
        }
    }
    out
}

/// The lines of `text` that start a block, with whether each one does: every line after a line break, and the
/// first line when only blanks and container marks (quote, bullet, list number) come before the unit on its line.
fn block_lines<'a>(line_prefix: &str, text: &'a str) -> impl Iterator<Item = (&'a str, bool, bool)> {
    let container = |c: char| matches!(c, ' ' | '\t' | '>' | '-' | '+' | '*' | '.' | ')') || c.is_ascii_digit();
    let first = line_prefix.chars().all(container);
    text.split_inclusive('\n').enumerate().map(move |(n, l)| (l, n > 0 || first, n == 0))
}

/// A translated unit must not start a block its source did not (LocOS trackers #704 and #790): a code fence would
/// swallow the rest of the file, a `# `, `- `, `1. ` or `> ` would turn a paragraph or list item into a heading,
/// list or quote. A marker of a kind the source has at none of its block starts gets a backslash, so it reads as
/// text and prints the same. Heading text and table cells are inline, so they are left alone.
pub fn guard_block_starts(line_prefix: &str, src: &str, text: &str) -> String {
    let mut seen: Vec<char> = Vec::new();
    for (line, at_start, first) in block_lines(line_prefix, src) {
        if at_start {
            seen.extend(block_markers(line, first).into_iter().map(|(k, _)| k));
        }
    }
    let cols = |s: &str| s.chars().take_while(|c| matches!(c, ' ' | '\t')).fold(0, |w, c| if c == '\t' { w + 4 - w % 4 } else { w + 1 });
    let mut out = String::with_capacity(text.len() + 4);
    for (line, at_start, first) in block_lines(line_prefix, text) {
        // four columns of indent at the unit's start would make it an indented code block: the blanks go
        let line = if first && at_start && cols(line) >= 4 && cols(src) < 4 { line.trim_start_matches([' ', '\t']) } else { line };
        let marks: Vec<usize> = if at_start { block_markers(line, first).into_iter().filter(|(k, _)| !seen.contains(k)).map(|(_, at)| at).collect() } else { Vec::new() };
        let mut pos = 0;
        for at in marks {
            out.push_str(&line[pos..at]);
            out.push('\\');
            pos = at;
        }
        out.push_str(&line[pos..]);
    }
    out
}

/// A translated ATX heading must not end in a closing sequence (`# w #`): the reader drops it, so the translated
/// `#` would be lost. Its first `#` gets a backslash (structure fuzz).
pub fn guard_atx_close(text: &str) -> String {
    let body = text.trim_end_matches([' ', '\t']);
    let before = body.trim_end_matches('#');
    if before.len() == body.len() || !(before.is_empty() || before.ends_with([' ', '\t'])) {
        return text.to_string();
    }
    format!("{before}\\{}", &text[before.len()..])
}

/// A bracket in the translated text of a link would end its label early or open another one (`[p[](url)` is the
/// text `[p` and an empty link): inside a link a `[` or `]` with no backslash before it gets one (structure fuzz).
pub fn escape_link_text(unit: &Unit, inl: &[Inline]) -> Vec<Inline> {
    let mut links: Vec<&str> = Vec::new();
    inl.iter()
        .map(|i| match i {
            Inline::Tag { id, kind: TagKind::Open, .. } if unit.codes.get(id).and_then(|c| c.open.as_deref()).is_some_and(|o| o.starts_with('[')) => {
                links.push(id);
                i.clone()
            }
            Inline::Tag { id, kind: TagKind::Close, .. } if links.last() == Some(&id.as_str()) => {
                links.pop();
                i.clone()
            }
            Inline::Text(t) if !links.is_empty() => {
                let mut out = String::with_capacity(t.len() + 2);
                let mut escaped = false;
                for c in t.chars() {
                    if matches!(c, '[' | ']') && !escaped {
                        out.push('\\');
                    }
                    out.push(c);
                    escaped = c == '\\' && !escaped;
                }
                Inline::Text(out)
            }
            _ => i.clone(),
        })
        .collect()
}

/// A translated table cell must not split its cell: a `|` with no backslash before it gets one.
pub fn guard_cell(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    let mut escaped = false;
    for c in text.chars() {
        if c == '|' && !escaped {
            out.push('\\');
        }
        out.push(c);
        escaped = c == '\\' && !escaped;
    }
    out
}

// Markers used only while building the preview. They are punctuation-like (Ps/Pe) so that Markdown emphasis
// next to them behaves as it does next to a bracket.
const SEG_OPEN: char = '\u{27E6}';
const SEG_ID_END: char = '\u{27E7}';
const SEG_CLOSE: char = '\u{27E8}';

fn clean(s: &str) -> String {
    s.replace([SEG_OPEN, SEG_ID_END, SEG_CLOSE], "\u{fffd}")
}

pub fn preview(doc: &Document) -> Option<String> {
    let Skeleton::Text { parts, .. } = &doc.skeleton else { return None };
    let mut md = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => md.push_str(&clean(s)),
            SkelPart::Unit(i) => {
                let unit = doc.units.get(*i)?;
                if unit.codes.contains_key(FM_KEY) || !unit.translate {
                    md.push_str(&clean(&render_unit(doc, unit)));
                    continue;
                }
                for (n, seg) in unit.segments.iter().enumerate() {
                    if n > 0 {
                        md.push_str(&unit.separators.get(n - 1).cloned().unwrap_or_default());
                    }
                    let inl = if !seg.has_target() { &seg.source } else { &seg.target };
                    md.push(SEG_OPEN);
                    md.push_str(&seg.id);
                    md.push(SEG_ID_END);
                    md.push_str(&clean(&inlines_md(doc, unit, inl)));
                    md.push(SEG_CLOSE);
                }
            }
        }
    }
    let body = match frontmatter::parse(&md) {
        Some(fm) => &md[fm.end..],
        None => &md[..],
    };
    let html = to_html(body);
    Some(format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"></head><body dir=\"auto\">\n{}</body></html>\n",
        put_spans(&html)
    ))
}

pub fn render_segment(doc: &Document, seg_id: &str) -> Option<String> {
    let (unit, seg) = doc.find(seg_id)?;
    let inl = if !seg.has_target() { &seg.source } else { &seg.target };
    let md = clean(&inlines_md(doc, unit, inl));
    let html = to_html(&md);
    let t = html.trim_end();
    let inner = t.strip_prefix("<p>").and_then(|x| x.strip_suffix("</p>")).unwrap_or(t);
    Some(inner.to_string())
}

/// Replaces the segment markers in rendered HTML by `<span data-bl-seg>` elements.
fn put_spans(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(a) = rest.find([SEG_OPEN, SEG_CLOSE]) {
        out.push_str(&rest[..a]);
        let c = rest[a..].chars().next().unwrap_or(SEG_CLOSE);
        let after = &rest[a + c.len_utf8()..];
        if c == SEG_CLOSE {
            out.push_str("</span>");
            rest = after;
        } else if let Some(z) = after.find(SEG_ID_END) {
            out.push_str("<span data-bl-seg=\"");
            out.push_str(&escape_attr(&after[..z]));
            out.push_str("\">");
            rest = &after[z + SEG_ID_END.len_utf8()..];
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Markdown to HTML with no raw HTML and no dangerous URLs.
pub fn to_html(md: &str) -> String {
    let events = Parser::new_ext(md, options()).map(|e| match e {
        Event::Html(h) | Event::InlineHtml(h) => Event::Text(h),
        Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
            Event::Start(Tag::Link { link_type, dest_url: safe_url(dest_url), title, id })
        }
        Event::Start(Tag::Image { link_type, dest_url, title, id }) => {
            Event::Start(Tag::Image { link_type, dest_url: safe_url(dest_url), title, id })
        }
        other => other,
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    html
}

fn safe_url(u: CowStr<'_>) -> CowStr<'_> {
    let lower: String = u.chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect::<String>().to_ascii_lowercase();
    let scheme_end = lower.find([':', '/', '?', '#']);
    match scheme_end {
        Some(i) if lower.as_bytes()[i] == b':' => {
            let scheme = &lower[..i];
            if ["http", "https", "mailto", "tel"].contains(&scheme) {
                u
            } else {
                CowStr::Borrowed("#")
            }
        }
        _ => u,
    }
}
