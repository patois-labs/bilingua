//! Bilingua subtitle filters: SRT and WebVTT. See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

use bilingua_model::*;
use std::collections::BTreeMap;

/// Default characters per line stored in `context.extra["max_cpl"]`. A unit's QA can check against it.
pub const DEFAULT_MAX_CPL: &str = "42";

const RAW: &str = "raw";
const LINE_BREAK_DISPLAY: &str = "\u{21b5}";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Srt,
    Vtt,
}

/// The SRT filter.
pub struct Srt;
/// The WebVTT filter.
pub struct Vtt;

impl Filter for Srt {
    fn id(&self) -> &'static str {
        "srt"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["srt"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);
        let head = head.trim_start_matches('\u{feff}');
        !head.trim_start().starts_with("WEBVTT") && head.lines().take(6).any(|l| l.contains(" --> ") && l.contains(','))
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        extract(Kind::Srt, bytes, opts)
    }
    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        merge(Kind::Srt, doc, opts)
    }
}

impl Filter for Vtt {
    fn id(&self) -> &'static str {
        "vtt"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["vtt"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(16)]);
        head.trim_start_matches('\u{feff}').starts_with("WEBVTT")
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        extract(Kind::Vtt, bytes, opts)
    }
    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        merge(Kind::Vtt, doc, opts)
    }
}

fn fmt_name(k: Kind) -> &'static str {
    match k {
        Kind::Srt => "srt",
        Kind::Vtt => "vtt",
    }
}

fn perr(k: Kind, msg: impl Into<String>) -> Error {
    Error::Parse { format: fmt_name(k).into(), msg: msg.into() }
}

struct Line<'a> {
    start: usize,
    /// End without the terminator.
    end: usize,
    text: &'a str,
}

fn lines(s: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut pos = 0;
    for raw in s.split_inclusive('\n') {
        let t = raw.strip_suffix('\n').unwrap_or(raw);
        let t = t.strip_suffix('\r').unwrap_or(t);
        out.push(Line { start: pos, end: pos + t.len(), text: t });
        pos += raw.len();
    }
    out
}

fn extract(k: Kind, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let mut dec = bilingua_model::enc::decode_text(bytes, &bilingua_model::enc::DecodeOptions::new(opts).lossy_after_utf8_bom())?;
    let text_owned = std::mem::take(&mut dec.text);
    let text = text_owned.as_str();
    let ls = lines(text);
    if k == Kind::Vtt && !ls.first().is_some_and(|l| l.text.starts_with("WEBVTT")) {
        return Err(perr(k, "a WebVTT file must start with WEBVTT"));
    }

    // Blocks: runs of non-blank lines.
    let mut blocks: Vec<(usize, usize)> = Vec::new(); // line index range [a, b)
    let mut i = 0;
    while i < ls.len() {
        if ls[i].text.trim().is_empty() {
            i += 1;
            continue;
        }
        let a = i;
        while i < ls.len() && !ls[i].text.trim().is_empty() {
            i += 1;
        }
        blocks.push((a, i));
    }

    let mut units: Vec<Unit> = Vec::new();
    let mut spans: Vec<(usize, usize)> = Vec::new(); // text range per unit
    let mut n = 0usize;
    for (bi, &(a, b)) in blocks.iter().enumerate() {
        let first = ls[a].text;
        if k == Kind::Vtt {
            let word = first.split([' ', '\t']).next().unwrap_or("");
            if (bi == 0 && first.starts_with("WEBVTT")) || matches!(word, "NOTE" | "STYLE" | "REGION") {
                continue;
            }
        }
        // The timing line is the first line with "-->" (VTT allows one identifier line before it).
        let Some(ti) = (a..b).find(|&j| ls[j].text.contains("-->")) else { continue };
        if ti + 1 >= b {
            continue; // a cue with no text
        }
        if k == Kind::Vtt && ti > a + 1 {
            continue;
        }
        let ident = if ti > a { Some(ls[a].text.trim().to_string()) } else { None };
        let timing = ls[ti].text;
        let (st, rest) = timing.split_once("-->").unwrap_or((timing, ""));
        let rest = rest.trim_start();
        let (en, settings) = match rest.find([' ', '\t']) {
            Some(p) => (&rest[..p], rest[p..].trim()),
            None => (rest, ""),
        };
        let t0 = ls[ti + 1].start;
        let t1 = ls[b - 1].end;
        let raw = &text[t0..t1];
        n += 1;
        let (inl, codes) = cue_inlines(k, raw);
        let mut u = Unit::new(format!("c{n}"), inl);
        u.codes = codes;
        u.set_hidden(RAW, raw.to_string());
        u.context.kind = Some("subtitle".into());
        u.context.key = Some(ident.unwrap_or_else(|| n.to_string()));
        u.context.location = Some(format!("line {}", ti + 1));
        u.context.extra.insert("start".into(), st.trim().to_string());
        u.context.extra.insert("end".into(), en.trim().to_string());
        if !settings.is_empty() {
            u.context.extra.insert("settings".into(), settings.to_string());
        }
        u.context.extra.insert("max_cpl".into(), DEFAULT_MAX_CPL.into());
        if plain_text(&u.segments[0].source).trim().is_empty() && !u.segments[0].source.iter().any(|i| matches!(i, Inline::Tag { .. })) {
            n -= 1;
            continue;
        }
        units.push(u);
        spans.push((t0, t1));
    }

    let mut parts = Vec::new();
    let mut cursor = 0;
    for (i, (s, e)) in spans.iter().enumerate() {
        if *s > cursor {
            parts.push(SkelPart::Raw(text[cursor..*s].to_string()));
        }
        parts.push(SkelPart::Unit(i));
        cursor = *e;
    }
    if cursor < text.len() {
        parts.push(SkelPart::Raw(text[cursor..].to_string()));
    }
    bilingua_model::guard::finish(Document::new(fmt_name(k), units, Skeleton::text(parts, dec.encoding.as_str(), bilingua_model::parser_info!())).with_options(opts), std::mem::take(&mut dec.warnings), &opts.limits())
}

// ---------- cue text ----------

enum Tok<'a> {
    Text(String),
    Open { raw: &'a str, name: String },
    Close { raw: &'a str, name: String },
    Single { raw: &'a str, name: String },
    Break(&'a str),
}

fn srt_tag(name: &str) -> bool {
    matches!(name, "i" | "b" | "u" | "s" | "font" | "strike" | "em" | "strong")
}

fn decode_vtt(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        let hit = [("&amp;", '&'), ("&lt;", '<'), ("&gt;", '>'), ("&nbsp;", '\u{a0}'), ("&lrm;", '\u{200e}'), ("&rlm;", '\u{200f}')]
            .iter()
            .find(|(e, _)| rest.starts_with(e));
        match hit {
            Some((e, c)) => {
                out.push(*c);
                rest = &rest[e.len()..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// True when `s` starts like an entity this module does not decode (`&copy;`): kept as it is on output.
fn foreign_entity(s: &str) -> bool {
    let b = s.as_bytes();
    if b.first() != Some(&b'&') {
        return false;
    }
    let mut i = 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'#') && i < 12 {
        i += 1;
    }
    i > 1 && b.get(i) == Some(&b';') && !["&amp;", "&lt;", "&gt;", "&nbsp;", "&lrm;", "&rlm;"].iter().any(|e| s.starts_with(e))
}

fn encode_vtt(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.char_indices() {
        match c {
            '&' if foreign_entity(&s[i..]) => out.push('&'),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            c => out.push(c),
        }
    }
    out
}

fn scan(k: Kind, raw: &str) -> Vec<Tok<'_>> {
    let b = raw.as_bytes();
    let mut out = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    let flush = |text: &mut String, out: &mut Vec<Tok<'_>>| {
        if !text.is_empty() {
            let t = std::mem::take(text);
            out.push(Tok::Text(if k == Kind::Vtt { decode_vtt(&t) } else { t }));
        }
    };
    while i < b.len() {
        let c = b[i];
        if c == b'\r' || c == b'\n' {
            let end = if c == b'\r' && b.get(i + 1) == Some(&b'\n') { i + 2 } else { i + 1 };
            flush(&mut text, &mut out);
            out.push(Tok::Break(&raw[i..end]));
            i = end;
            continue;
        }
        if c == b'{' && k == Kind::Srt && b.get(i + 1) == Some(&b'\\') {
            if let Some(p) = raw[i..].find('}') {
                flush(&mut text, &mut out);
                out.push(Tok::Single { raw: &raw[i..i + p + 1], name: "override".into() });
                i += p + 1;
                continue;
            }
        }
        if c == b'<' {
            if let Some(p) = raw[i..].find('>') {
                let inner = &raw[i + 1..i + p];
                let whole = &raw[i..i + p + 1];
                let (closing, body) = match inner.strip_prefix('/') {
                    Some(r) => (true, r),
                    None => (false, inner),
                };
                let name: String = body
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == ':')
                    .collect::<String>()
                    .to_ascii_lowercase();
                let is_stamp = !closing && body.as_bytes().first().is_some_and(|c| c.is_ascii_digit()) && body.contains(':');
                let ok = match k {
                    Kind::Srt => srt_tag(&name) && !name.is_empty(),
                    Kind::Vtt => is_stamp || (!name.is_empty() && body.as_bytes().first().is_some_and(|c| c.is_ascii_alphabetic())),
                };
                if ok && !inner.contains('\n') {
                    flush(&mut text, &mut out);
                    if is_stamp {
                        out.push(Tok::Single { raw: whole, name: "timestamp".into() });
                    } else if closing {
                        out.push(Tok::Close { raw: whole, name });
                    } else {
                        out.push(Tok::Open { raw: whole, name });
                    }
                    i += p + 1;
                    continue;
                }
            }
        }
        let ch = raw[i..].chars().next().unwrap_or('\u{fffd}');
        text.push(ch);
        i += ch.len_utf8();
    }
    flush(&mut text, &mut out);
    out
}

fn label(name: &str) -> &str {
    match name {
        "i" | "em" => "italic",
        "b" | "strong" => "bold",
        "u" => "underline",
        "s" | "strike" => "strikethrough",
        "font" => "font",
        "c" => "class",
        "v" => "voice",
        "lang" => "language",
        "ruby" | "rt" => "ruby",
        "timestamp" => "timestamp",
        "override" => "style override",
        other => other,
    }
}

fn display_open(raw: &str, name: &str) -> String {
    match name {
        "v" | "c" | "lang" => {
            if raw.chars().count() <= 24 {
                raw.to_string()
            } else {
                format!("<{name}>")
            }
        }
        _ => format!("<{name}>"),
    }
}

fn cue_inlines(k: Kind, raw: &str) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    let toks = scan(k, raw);
    let n = toks.len();
    let mut pair: Vec<Option<usize>> = vec![None; n];
    let mut stack: Vec<usize> = Vec::new();
    for idx in 0..n {
        match &toks[idx] {
            Tok::Open { name, .. } => {
                let _ = name;
                stack.push(idx);
            }
            Tok::Close { name, .. } => {
                let found = stack.iter().rposition(|&o| matches!(&toks[o], Tok::Open { name: on, .. } if on == name));
                if let Some(p) = found {
                    let o = stack[p];
                    pair[o] = Some(idx);
                    pair[idx] = Some(o);
                    stack.truncate(p);
                }
            }
            _ => {}
        }
    }
    let mut inl = Vec::new();
    let mut codes = BTreeMap::new();
    let mut ids: Vec<Option<String>> = vec![None; n];
    let mut next = 1;
    for idx in 0..n {
        match &toks[idx] {
            Tok::Text(t) => inl.push(Inline::Text(t.clone())),
            Tok::Open { raw, name } if pair[idx].is_some() => {
                let id = next.to_string();
                next += 1;
                let close = pair[idx].and_then(|c| if let Tok::Close { raw, .. } = &toks[c] { Some(raw.to_string()) } else { None });
                codes.insert(id.clone(), CodeData::new().with_open(raw.to_string()).with_close(close).with_label(label(name).to_string()));
                inl.push(Inline::tag(id.clone(), TagKind::Open, display_open(raw, name)));
                ids[idx] = Some(id);
            }
            Tok::Close { name, .. } if pair[idx].is_some() => {
                if let Some(id) = pair[idx].and_then(|o| ids[o].clone()) {
                    inl.push(Inline::tag(id, TagKind::Close, format!("</{name}>")));
                }
            }
            tok => {
                let id = next.to_string();
                next += 1;
                let (raw, name, display) = match tok {
                    Tok::Open { raw, name } => (raw.to_string(), name.as_str(), display_open(raw, name)),
                    Tok::Close { raw, name } => (raw.to_string(), name.as_str(), format!("</{name}>")),
                    Tok::Single { raw, name } => (raw.to_string(), name.as_str(), if name == "timestamp" || name == "override" { raw.to_string() } else { format!("<{name}>") }),
                    Tok::Break(b) => (b.to_string(), "line break", LINE_BREAK_DISPLAY.to_string()),
                    Tok::Text(_) => continue,
                };
                codes.insert(id.clone(), CodeData::new().with_standalone(raw).with_label(label(name).to_string()));
                inl.push(Inline::tag(id, TagKind::Standalone, display));
            }
        }
    }
    (merge_text(inl), codes)
}

fn merge(k: Kind, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write(format!("{} needs a text skeleton", fmt_name(k))));
    };
    let eol = if parts.iter().any(|p| matches!(p, SkelPart::Raw(s) if s.contains("\r\n"))) { "\r\n" } else { "\n" };
    let mut out = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => out.push_str(s),
            SkelPart::Unit(i) => {
                let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                let raw = u.hidden(RAW);
                let translated = u.translate && u.segments.iter().any(|s| s.has_target());
                match raw {
                    Some(r) if !translated => out.push_str(r),
                    _ => out.push_str(&render(k, u, eol)),
                }
            }
        }
    }
    bilingua_model::enc::encode_text_with(&out, encoding, opts)
}

fn render(k: Kind, u: &Unit, eol: &str) -> String {
    let mut out = String::new();
    for i in u.output() {
        match i {
            Inline::Text(t) => {
                let t = if k == Kind::Vtt { encode_vtt(&t) } else { t };
                out.push_str(&t.replace("\r\n", "\n").replace('\n', eol));
            }
            Inline::Tag { id, kind, .. } => {
                let Some(c) = u.codes.get(&id) else { continue };
                let s = match kind {
                    TagKind::Open => c.open.as_deref(),
                    TagKind::Close => c.close.as_deref(),
                    TagKind::Standalone => c.standalone.as_deref(),
                };
                if let Some(s) = s {
                    // A line break keeps the file's own line ending.
                    if c.label.as_deref() == Some("line break") {
                        out.push_str(eol);
                    } else {
                        out.push_str(s);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
