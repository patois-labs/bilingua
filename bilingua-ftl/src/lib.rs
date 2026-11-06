//! Bilingua Fluent filter: Mozilla Project Fluent `.ftl` files (syntax 1.0).
//!
//! Units: a message value and each message attribute are translatable patterns, cut into runs of text and simple
//! placeables. A select expression ends a run and each of its variants is walked on its own (`ftl:emails[one]`,
//! `ftl:shared[male][other]`), so plural variants are separate units. Terms (`-brand`) are not translated unless
//! [`Limits::translate_terms`] is set; term attributes never are. Comments, identifiers, selectors, variant keys,
//! Junk, indentation of untouched text, line endings and the BOM stay in the skeleton byte for byte.
//!
//! Placeables (`{ $name }`, `{ -brand }`, `{ NUMBER($n) }`) are inline tags (free standalone codes, label
//! `placeholder`, display = the source text), because Localization OS keeps them as plain text in the unit.
//! `{ "{" }` style brace literals are shown as the plain character. On merge a literal brace in a translation is
//! written as `{"{"}` and a continuation line starting with `[`, `*` or `.` is escaped the same way, so a
//! translation can never change the structure of the file.
//!
//! `FtlFilter::merge_localized` also adds, to every plural select, a variant for each CLDR category the target
//! language uses that the file lacks, holding the translated `[other]` variant (Localization OS decision 13). Plain
//! `merge` never adds units.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod parser;
mod plural_data;

use bilingua_model::*;
use parser::{El, Item, PKind};

/// The Fluent filter.
pub struct FtlFilter;

const ID: &str = "ftl";
/// The one hidden code of a unit: `standalone` = the original source text of the run, `open` = white space before
/// the text, `close` = white space after it, `label` = `l` or `c` (line ends: LF or CRLF), `z`, `b` or `i` (the text starts at column 0, after spaces at the line start, or follows something on its
/// line), then the indent.
const HIDDEN: &str = "$ftl";
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF8: &str = "utf-8";
const UTF8_BOM: &str = "utf-8-bom";
const MAX_NESTING: usize = 40;

/// Size limits and the one option. The numbers are `bilingua_model::Limits` (Localization OS's configuration):
/// `Filter::extract` takes them from `ExtractOptions::limits`, and over-limit files fail with `Error::Limit`.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Limits {
    /// Units in one file (`MAX_DOCUMENT_SEGMENTS`); twenty times this is also the placeable budget.
    pub max_units: usize,
    /// Characters in one unit (`MAX_SEGMENT_CHARS`).
    pub max_value_chars: usize,
    /// Bytes in one file (`MAX_UPLOAD_BYTES`).
    pub max_input_bytes: usize,
    /// Lines in one file (twenty per allowed unit, LocOS's line budget).
    pub max_lines: usize,
    /// Translate terms (`-brand-name = Firefox`) too. Localization OS: the `FTL_TRANSLATE_TERMS` environment variable.
    pub translate_terms: bool,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits::from_model(&bilingua_model::Limits::default())
    }
}

impl Limits {
    /// The model's limits (as `ExtractOptions::limits` carries them), terms not translated.
    pub fn from_model(l: &bilingua_model::Limits) -> Limits {
        Limits {
            max_units: l.max_units,
            max_value_chars: l.max_unit_chars,
            max_input_bytes: usize::try_from(l.max_input_bytes).unwrap_or(usize::MAX),
            max_lines: l.max_lines,
            translate_terms: false,
        }
    }

    fn model(&self) -> bilingua_model::Limits {
        bilingua_model::Limits::new().with_max_units(self.max_units).with_max_unit_chars(self.max_value_chars).with_max_input_bytes(self.max_input_bytes as u64).with_max_lines(self.max_lines)
    }
}

fn limit_err(msg: String) -> Error {
    Error::Unsupported(msg)
}

/// Python `str.isspace` (Rust's `White_Space` plus the four separators U+001C to U+001F).
fn py_space(c: char) -> bool {
    bilingua_model::py_isspace_char(c)
}

fn py_strip(s: &str) -> &str {
    s.trim_matches(py_space)
}

fn decode(bytes: &[u8]) -> Result<(&str, bool), Error> {
    let (body, bom) = match bytes.strip_prefix(BOM) {
        Some(b) => (b, true),
        None => (bytes, false),
    };
    let text = std::str::from_utf8(body).map_err(|e| {
        if body.contains(&0) {
            limit_err("this file is not valid UTF-8 and looks like a binary file, not text; re-save it as UTF-8".into())
        } else {
            limit_err(format!(
                "Fluent .ftl files are UTF-8; byte 0x{:02x} at position {} is not. Re-save the file as UTF-8.",
                body[e.valid_up_to()],
                e.valid_up_to() + if bom { 3 } else { 0 }
            ))
        }
    })?;
    if text.contains('\0') {
        return Err(limit_err("this file is not valid UTF-8 text (it holds NUL bytes); re-save it as UTF-8".into()));
    }
    Ok((text, bom))
}

/// Line count, placeable count and nesting depth, before the parser builds anything.
fn check_budget(text: &str, l: &Limits) -> Result<(), Error> {
    bilingua_model::guard::check_lines(text, &l.model())?;
    let budget = l.max_units.saturating_mul(20);
    let (mut opens, mut depth) = (0usize, 0usize);
    for &b in text.as_bytes() {
        match b {
            b'{' => {
                opens += 1;
                depth += 1;
                if depth > MAX_NESTING {
                    return Err(limit_err(format!("this Fluent file nests placeables more than {MAX_NESTING} deep")));
                }
            }
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    if opens > budget {
        return Err(limit_err(format!("this Fluent file has more than {budget} placeables; split it into smaller files")));
    }
    Ok(())
}

enum Piece {
    Text(String),
    Ph(String),
}

struct Run {
    start: usize,
    end: usize,
    pieces: Vec<Piece>,
    label: String,
    indent: String,
}

fn push_text(pieces: &mut Vec<Piece>, s: &str) {
    if let Some(Piece::Text(t)) = pieces.last_mut() {
        t.push_str(s);
    } else {
        pieces.push(Piece::Text(s.to_string()));
    }
}

/// Indentation for continuation lines: the run's own, else its line's plus four spaces.
fn indent_of(src: &str, start: usize, end: usize) -> String {
    let seg = &src[start..end];
    let b = seg.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c != b'\n' {
            continue;
        }
        let mut j = i + 1;
        while j < b.len() && b[j] == b' ' {
            j += 1;
        }
        if j > i + 1 && j < b.len() {
            if let Some(ch) = seg[j..].chars().next() {
                if !py_space(ch) {
                    return seg[i + 1..j].to_string();
                }
            }
        }
    }
    let bol = src.as_bytes()[..start].iter().rposition(|&c| c == b'\n' || c == b'\r').map_or(0, |p| p + 1);
    let line = &src[bol..start];
    let lead = line.len() - line.trim_start_matches(' ').len();
    format!("{}    ", &line[..lead])
}

fn walk(els: &[El], label: &str, src: &str, out: &mut Vec<Run>) {
    let mut run: Vec<&El> = Vec::new();
    fn flush(run: &mut Vec<&El>, label: &str, src: &str, out: &mut Vec<Run>) {
        if run.is_empty() {
            return;
        }
        let bounds = |e: &El| match e {
            El::Text { start, end, .. } | El::Place { start, end, .. } => (*start, *end),
        };
        let (start, end) = (bounds(run[0]).0, bounds(run[run.len() - 1]).1);
        let mut pieces: Vec<Piece> = Vec::new();
        for e in run.iter() {
            match e {
                El::Text { value, .. } => push_text(&mut pieces, value),
                El::Place { kind: PKind::Str(v), .. } if matches!(v.as_str(), "{" | "}" | "[" | "*" | ".") => push_text(&mut pieces, v),
                El::Place { start, end, .. } => pieces.push(Piece::Ph(src[*start..*end].to_string())),
            }
        }
        out.push(Run { start, end, pieces, label: label.to_string(), indent: indent_of(src, start, end) });
        run.clear();
    }
    for el in els {
        if let El::Place { kind: PKind::Select(sel), .. } = el {
            flush(&mut run, label, src, out);
            for v in &sel.variants {
                walk(&v.value, &format!("{label}[{}]", v.key), src, out);
            }
            continue;
        }
        run.push(el);
    }
    flush(&mut run, label, src, out);
}

fn runs(src: &str, items: &[Item], terms: bool) -> Vec<(Run, Option<String>)> {
    let mut out: Vec<Run> = Vec::new();
    let mut comments: Vec<Option<String>> = Vec::new();
    for it in items {
        let before = out.len();
        if !it.is_term {
            if let Some(v) = &it.value {
                walk(v, &it.id, src, &mut out);
            }
            for a in &it.attrs {
                walk(&a.value, &format!("{}.{}", it.id, a.name), src, &mut out);
            }
        } else if terms {
            if let Some(v) = &it.value {
                walk(v, &format!("-{}", it.id), src, &mut out);
            }
        }
        for _ in before..out.len() {
            comments.push(it.comment.clone());
        }
    }
    out.into_iter().zip(comments).collect()
}

/// A letter or digit outside the placeables (Python `str.isalnum`, exactly: `bilingua_model::py_has_alnum`).
fn translatable(pieces: &[Piece]) -> bool {
    let mut phs: Vec<&str> = pieces.iter().filter_map(|p| if let Piece::Ph(s) = p { Some(s.as_str()) } else { None }).collect();
    if phs.is_empty() {
        return pieces.iter().any(|p| matches!(p, Piece::Text(t) if bilingua_model::py_has_alnum(t)));
    }
    phs.sort_by_key(|s| std::cmp::Reverse(s.len()));
    phs.dedup();
    let mut s: String = pieces.iter().map(|p| match p { Piece::Text(t) | Piece::Ph(t) => t.as_str() }).collect();
    for p in phs {
        s = s.replace(p, "");
    }
    bilingua_model::py_has_alnum(&s)
}

/// What a Fluent file holds, for tools and tests: messages, terms and Junk entries (text the Fluent grammar
/// rejects, kept as is).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Structure {
    pub messages: usize,
    pub terms: usize,
    pub junk: usize,
    /// Attributes of messages and terms.
    pub attributes: usize,
}

/// Counts the entries of a Fluent file (UTF-8, optional BOM).
pub fn structure(bytes: &[u8]) -> Result<Structure, Error> {
    let (text, _) = decode(bytes)?;
    check_budget(text, &Limits::default())?;
    let (items, junk) = parser::parse_counted(text);
    Ok(Structure {
        messages: items.iter().filter(|i| !i.is_term).count(),
        terms: items.iter().filter(|i| i.is_term).count(),
        junk,
        attributes: items.iter().map(|i| i.attrs.len()).sum(),
    })
}

impl FtlFilter {
    /// `extract` with explicit limits and options.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        bilingua_model::guard::check_input(bytes, &limits.model())?;
        let (text, bom) = decode(bytes)?;
        check_budget(text, limits)?;
        let items = parser::parse(text);
        let crlf = text.contains("\r\n");
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        for (run, comment) in runs(text, &items, limits.translate_terms) {
            if !translatable(&run.pieces) {
                continue;
            }
            let mut pieces = run.pieces;
            let (mut lead, mut trail) = (String::new(), String::new());
            if let Some(Piece::Text(t)) = pieces.first_mut() {
                let s = py_strip_start(t);
                lead = t[..t.len() - s.len()].to_string();
                *t = s.to_string();
            }
            if let Some(Piece::Text(t)) = pieces.last_mut() {
                let s = py_strip_end(t);
                trail = t[s.len()..].to_string();
                t.truncate(s.len());
            }
            let mut chars = 0usize;
            let mut inl: Vec<Inline> = Vec::new();
            let mut codes = std::collections::BTreeMap::new();
            let mut n = 0usize;
            for p in pieces {
                match p {
                    Piece::Text(t) => {
                        chars += t.chars().count();
                        if !t.is_empty() {
                            inl.push(Inline::text(t));
                        }
                    }
                    Piece::Ph(raw) => {
                        chars += raw.chars().count();
                        n += 1;
                        let id = n.to_string();
                        codes.insert(id.clone(), CodeData::new().with_standalone(raw.clone()).with_label("placeholder".to_string()));
                        inl.push(Inline::free_tag(id, raw));
                    }
                }
            }
            bilingua_model::guard::check_unit_chars(chars, &limits.model())?;
            bilingua_model::guard::check_unit_count(units.len() + 1, &limits.model())?;
            if run.start > pos {
                parts.push(SkelPart::Raw(text[pos..run.start].to_string()));
            }
            pos = run.end;
            let mut unit = Unit::new(format!("u{}", units.len() + 1), inl);
            unit.codes = codes;
            unit.codes.insert(
                HIDDEN.into(),
                CodeData::new().with_open((!lead.is_empty()).then_some(lead)).with_close((!trail.is_empty()).then_some(trail)).with_standalone(text[run.start..run.end].to_string()).with_label(format!(
                        "{}{}{}",
                        if crlf { 'c' } else { 'l' },
                        match &text[line_start(text, run.start)..run.start] {
                            "" => 'z',
                            p if p.bytes().all(|b| b == b' ') => 'b',
                            _ => 'i',
                        },
                        run.indent
                    )),
            );
            unit.context.kind = Some("key".into());
            unit.context.style = Some(format!("ftl:{}", run.label));
            unit.context.key = Some(run.label);
            if let Some(c) = comment {
                unit.context.notes.push(c);
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        Ok(Document::new(ID, units, Skeleton::text(parts, if bom { UTF8_BOM } else { UTF8 }, bilingua_model::parser_info!())).with_options(opts))
    }
}

fn py_strip_start(s: &str) -> &str {
    s.trim_start_matches(py_space)
}

fn py_strip_end(s: &str) -> &str {
    s.trim_end_matches(py_space)
}

/// The `format_options` keys of the FTL filter.
pub const FORMAT_OPTIONS: &[bilingua_model::FormatOptionKey] =
    &[bilingua_model::FormatOptionKey::bool("ftl.translate_terms", "Extract term values (-term = ...) as units too (LocOS leaves them out).")];

impl Filter for FtlFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn format_option_keys(&self) -> &'static [bilingua_model::FormatOptionKey] {
        FORMAT_OPTIONS
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ftl"]
    }

    /// Loose: Fluent looks like `key = value`. Needs a Fluent-only marker (`{ $var }`, `{ -term }`, a select
    /// variant `*[other]`, a `-term =` entry, an indented `.attribute =`, a `##` group comment) and at least one
    /// parsed entry. Put it late in the facade list.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(4096)];
        let body = head.strip_prefix(BOM).unwrap_or(head);
        let text = match std::str::from_utf8(body) {
            Ok(t) => t,
            Err(e) if e.error_len().is_none() => std::str::from_utf8(&body[..e.valid_up_to()]).unwrap_or(""),
            Err(_) => return false,
        };
        if text.contains('\0') || check_budget(text, &Limits::default()).is_err() {
            return false;
        }
        let marker = text.contains("{ $")
            || text.contains("{ -")
            || text.contains("*[")
            || text.lines().any(|l| {
                l.starts_with("## ")
                    || l.starts_with("### ")
                    || (l.starts_with(' ') && l.trim_start().starts_with('.') && l.contains(" = "))
                    || (l.starts_with('-') && l[1..].split(" =").next().is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')) && l.contains(" ="))
            });
        marker && !parser::parse(text).is_empty()
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let mut limits = Limits::from_model(&opts.limits());
        // `format_options["ftl.translate_terms"]` is the generic spelling of `Limits::translate_terms`
        if let Some(on) = opts.format_flag("ftl.translate_terms") {
            limits.translate_terms = on;
        }
        self.extract_with(bytes, opts, &limits)
    }

    /// With `stamp_target_always` the plural selects are filled for the document's `target_lang` even when nothing
    /// is translated (LocOS `build_localized`); otherwise the same as `merge`.
    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        // plain `merge` never fills (see `merge_localized`), so only the "always" half of the stamp rule applies
        match opts.stamp_lang(doc, false) {
            Some(t) => self.merge_inner(doc, Some(t), true),
            None => self.merge_inner(doc, None, false),
        }
    }
}

impl FtlFilter {
    /// Like `merge`, and also adds to every plural select the CLDR categories `locale` (default: the document's
    /// `target_lang`) uses that the file lacks, each a copy of the translated `[other]` variant (LocOS
    /// `build_localized`, decision 13). Nothing is added when no unit has a target. The added variants are new
    /// units when the file is read again, which is why plain `merge` does not do this.
    pub fn merge_localized(&self, doc: &Document, locale: Option<&str>) -> Result<Vec<u8>, Error> {
        self.merge_inner(doc, locale.or(doc.target_lang.as_deref()), false)
    }

    fn merge_inner(&self, doc: &Document, fill: Option<&str>, even_untranslated: bool) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("ftl needs a text skeleton".into()));
        };
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    out.push_str(&render(unit));
                }
            }
        }
        let translated = doc.units.iter().any(|u| u.segments.iter().any(|s| s.has_target()));
        if let (Some(loc), true) = (fill, translated || even_untranslated) {
            check_budget(&out, &Limits::default()).map_err(|e| Error::Write(e.to_string()))?;
            out = fill_plurals(&out, loc);
        }
        let mut bytes = Vec::with_capacity(out.len() + 3);
        if encoding == UTF8_BOM {
            bytes.extend_from_slice(BOM);
        }
        bytes.extend_from_slice(out.as_bytes());
        Ok(bytes)
    }
}

/// Source form of an inline sequence: text as is, a tag as its original placeable.
fn flat(unit: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, display, .. } => s.push_str(unit.codes.get(id).and_then(|c| c.standalone.as_deref()).unwrap_or(display)),
        }
    }
    s
}

/// Under the LocOS profile a unit's placeables are plain text in its source (the profile flattens them and keeps
/// their codes). The translation is then read as LocOS's `_encode` does: every occurrence of one of the source's
/// placeables (the first one listed wins at a position) is written as it is, everything else is escaped. Returns
/// `out` unchanged for a unit whose source still has its placeable tags.
fn retag_flattened(unit: &Unit, out: Vec<Inline>) -> Vec<Inline> {
    let mut ph: Vec<(usize, &str, &str)> = unit
        .codes
        .iter()
        .filter(|(_, c)| c.label.as_deref() == Some("placeholder"))
        .filter_map(|(id, c)| Some((id.parse::<usize>().ok()?, id.as_str(), c.standalone.as_deref()?)))
        .filter(|(_, _, m)| !m.is_empty())
        .collect();
    let used = |id: &str| unit.segments.iter().any(|s| s.source.iter().any(|i| matches!(i, Inline::Tag { id: t, .. } if t == id)));
    if ph.is_empty() || ph.iter().any(|(_, id, _)| used(id)) {
        return out;
    }
    ph.sort();
    let mut v = Vec::with_capacity(out.len());
    for i in out {
        let Inline::Text(t) = i else {
            v.push(i);
            continue;
        };
        let mut pos = 0;
        // each placeable's next occurrence at or after `pos` (None: no more), searched again only once passed, so
        // a long translation is scanned once per placeable
        let mut next: Vec<Option<usize>> = ph.iter().map(|(_, _, m)| t.find(m)).collect();
        while pos < t.len() {
            for (k, (_, _, m)) in ph.iter().enumerate() {
                if next[k].is_some_and(|at| at < pos) {
                    next[k] = t[pos..].find(m).map(|at| pos + at);
                }
            }
            let hit = ph.iter().enumerate().filter_map(|(k, (_, id, m))| next[k].map(|at| (at, *id, *m))).min_by_key(|(at, ..)| *at);
            match hit {
                Some((at, id, m)) => {
                    if at > pos {
                        v.push(Inline::text(&t[pos..at]));
                    }
                    v.push(Inline::tag(id, TagKind::Standalone, m));
                    pos = at + m.len();
                }
                None => {
                    v.push(Inline::text(&t[pos..]));
                    pos = t.len();
                }
            }
        }
    }
    v
}

/// The source text to write for a unit: the original when nothing changed, else the encoded translation.
fn render(unit: &Unit) -> String {
    let Some(h) = unit.codes.get(HIDDEN) else { return String::new() };
    let raw = h.standalone.as_deref().unwrap_or("");
    if !unit.translate || unit.segments.iter().all(|s| !s.has_target()) {
        return raw.to_string();
    }
    let out = retag_flattened(unit, unit.output());
    if flat(unit, &out) == flat(unit, &unit.source()) {
        return raw.to_string();
    }
    let layout = h.label.as_deref().unwrap_or("li    ");
    let (newline, rest) = if let Some(rest) = layout.strip_prefix('c') { ("\r\n", rest) } else { ("\n", layout.strip_prefix('l').unwrap_or(layout)) };
    let block_start = rest.starts_with(['b', 'z']);
    let col0 = rest.starts_with('z');
    let indent = rest.get(1..).unwrap_or("    ");
    let mut seq: Vec<Inline> = Vec::with_capacity(out.len() + 2);
    seq.push(Inline::text(h.open.clone().unwrap_or_default()));
    seq.extend(out);
    seq.push(Inline::text(h.close.clone().unwrap_or_default()));
    let seq = merge_text(seq);
    // Continuation lines get the unit's indent, blank lines none. Only line breaks in the translator's text count:
    // a placeable is written as it was, even when it spans lines.
    let mut rebuilt = String::new();
    let (mut line, mut nonblank, mut first_line, mut after_nl) = (String::new(), false, true, false);
    let end_line = |rebuilt: &mut String, line: &mut String, nonblank: bool, first_line: &mut bool| {
        if *first_line {
            rebuilt.push_str(line);
            *first_line = false;
        } else {
            rebuilt.push_str(newline);
            if nonblank {
                rebuilt.push_str(indent);
                rebuilt.push_str(line);
            }
        }
        line.clear();
    };
    for (k, i) in seq.iter().enumerate() {
        match i {
            Inline::Text(t) => {
                let t = t.replace("\r\n", "\n").replace('\r', "\n");
                let mut esc = String::with_capacity(t.len());
                escape_text(&mut esc, &t, (k == 0 && block_start) || after_nl);
                for ch in esc.chars() {
                    if ch == '\n' {
                        end_line(&mut rebuilt, &mut line, nonblank, &mut first_line);
                        nonblank = false;
                    } else {
                        line.push(ch);
                        nonblank |= !py_space(ch);
                    }
                }
                after_nl = t.ends_with('\n');
            }
            Inline::Tag { id, display, .. } => {
                line.push_str(unit.codes.get(id).and_then(|c| c.standalone.as_deref()).unwrap_or(display));
                nonblank = true;
                after_nl = false;
            }
        }
    }
    end_line(&mut rebuilt, &mut line, nonblank, &mut first_line);
    if col0 && !rebuilt.starts_with(['{', ' ']) && !py_strip(&rebuilt).is_empty() {
        rebuilt.insert_str(0, indent);
    }
    rebuilt
}

/// Braces become `{"{"}`; a `[`, `*` or `.` at the start of a continuation line becomes `{"["}` and so on.
fn escape_text(out: &mut String, seg: &str, at_line_start: bool) {
    let mut line_start = at_line_start || out.ends_with('\n');
    for ch in seg.chars() {
        if ch == '{' || ch == '}' {
            out.push_str(&format!("{{\"{ch}\"}}"));
            line_start = false;
            continue;
        }
        if line_start && matches!(ch, '[' | '*' | '.') {
            out.push_str(&format!("{{\"{ch}\"}}"));
            line_start = false;
            continue;
        }
        out.push(ch);
        if ch == '\n' {
            line_start = true;
        } else if ch != ' ' {
            line_start = false;
        }
    }
}

// ---- plural fill ---------------------------------------------------------------------------------------------

const PLURAL_ORDER: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];
const PLURAL_LETTERS: &str = "zotfmx";

/// The CLDR plural (or ordinal) categories a locale uses, in CLDR order, always with `other`; `None` for no locale.
fn target_categories(locale: &str, ordinal: bool) -> Option<Vec<&'static str>> {
    let norm = locale.trim().replace('-', "_");
    let mut parts: Vec<String> = norm.split('_').map(str::to_string).collect();
    if parts[0].is_empty() || !parts[0].bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    parts[0] = parts[0].to_ascii_lowercase();
    // The table's subtags are ASCII: a non-ASCII one never matches (and must not be cut inside a character).
    for p in parts.iter_mut().skip(1).filter(|p| p.is_ascii()) {
        if p.len() == 4 {
            *p = format!("{}{}", p[..1].to_ascii_uppercase(), p[1..].to_ascii_lowercase());
        } else {
            *p = p.to_ascii_uppercase();
        }
    }
    let mut letters = "x";
    'search: for n in (1..=parts.len()).rev() {
        let id = parts[..n].join("_");
        if let Ok(i) = plural_data::TABLE.binary_search_by(|r| r.0.cmp(id.as_str())) {
            letters = if ordinal { plural_data::TABLE[i].2 } else { plural_data::TABLE[i].1 };
            break 'search;
        }
    }
    Some(PLURAL_LETTERS.char_indices().filter(|(_, c)| letters.contains(*c)).map(|(i, _)| PLURAL_ORDER[i]).collect())
}

fn select_list<'a>(els: &'a [El], out: &mut Vec<&'a parser::Select>) {
    for el in els {
        if let El::Place { kind: PKind::Select(s), .. } = el {
            out.push(s);
            for v in &s.variants {
                select_list(&v.value, out);
            }
        }
    }
}

fn line_start(text: &str, pos: usize) -> usize {
    text.as_bytes()[..pos].iter().rposition(|&c| c == b'\n' || c == b'\r').map_or(0, |p| p + 1)
}

/// Adds the target language's missing plural variants (see the crate docs). Returns `text` as is when there is
/// nothing to add.
fn fill_plurals(text: &str, locale: &str) -> String {
    let items = parser::parse(text);
    let mut selects: Vec<&parser::Select> = Vec::new();
    for it in &items {
        if let Some(v) = &it.value {
            select_list(v, &mut selects);
        }
        for a in &it.attrs {
            select_list(&a.value, &mut selects);
        }
    }
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut inserts: Vec<(usize, String)> = Vec::new();
    for sel in selects {
        let named: Vec<&parser::Variant> = sel.variants.iter().filter(|v| v.ident).collect();
        if named.len() < 2 || named.iter().any(|v| !PLURAL_ORDER.contains(&v.key.as_str())) {
            continue;
        }
        let other = named.iter().rev().find(|v| v.key == "other");
        let default = sel.variants.iter().find(|v| v.default);
        let (Some(other), Some(default)) = (other, default) else { continue };
        let Some(cats) = target_categories(locale, sel.ordinal) else { continue };
        let missing: Vec<&str> = cats.into_iter().filter(|c| !named.iter().any(|v| v.key == *c)).collect();
        if missing.is_empty() {
            continue;
        }
        let Some(rel) = text[other.key_end..].find(']') else { continue };
        let close = other.key_end + rel;
        if close >= other.end {
            continue;
        }
        let body = &text[close + 1..other.end];
        let bol = line_start(text, default.start);
        let indent = &text[bol..default.start];
        if !py_strip(indent).is_empty() {
            continue;
        }
        let first = &sel.variants[0];
        let lead0 = &text[line_start(text, first.start)..first.start];
        let lead = if !first.default && py_strip(lead0).is_empty() { lead0.to_string() } else { format!("{indent} ") };
        let mut s = String::new();
        for c in missing {
            s.push_str(&format!("{lead}[{c}]{body}{newline}"));
        }
        inserts.push((bol, s));
    }
    if inserts.is_empty() {
        return text.to_string();
    }
    inserts.sort_by_key(|i| i.0);
    let mut out = String::with_capacity(text.len() + inserts.iter().map(|i| i.1.len()).sum::<usize>());
    let mut pos = 0;
    for (at, s) in inserts {
        out.push_str(&text[pos..at]);
        out.push_str(&s);
        pos = at;
    }
    out.push_str(&text[pos..]);
    out
}

#[cfg(test)]
mod locos_tests {
    use super::*;

    #[test]
    fn retagging_a_long_translation_stays_linear() {
        // two placeables in the source, a translation with 200 000 of the first and none of the second: each
        // placeable is searched once per pass over the text, not once per hit
        let mut u = Unit::new("1", vec![Inline::text("x { $a } y { $b }")]);
        u.codes.insert("1".into(), CodeData::standalone("{ $a }").with_label("placeholder"));
        u.codes.insert("2".into(), CodeData::standalone("{ $b }").with_label("placeholder"));
        let t = "{ $a }".repeat(200_000);
        let out = retag_flattened(&u, vec![Inline::text(&t)]);
        assert_eq!(out.len(), 200_000);
        assert!(out.iter().all(|i| matches!(i, Inline::Tag { id, .. } if id == "1")));
    }
}
