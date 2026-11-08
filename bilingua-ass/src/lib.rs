//! Bilingua filter for Advanced SubStation Alpha subtitles (`.ass`) and SubStation Alpha v4 (`.ssa`).
//!
//! Only two things are translated, as in Localization OS: the `Text` field of every `Dialogue:` line in
//! `[Events]` (the last field named by the section's `Format:` line, so it may hold commas) and the `Title:` header
//! of `[Script Info]`. Everything else is skeleton, byte for byte: `Comment:` events, styles, fonts, graphics, other
//! headers, timing, the other Dialogue fields, line endings and the byte-order mark. See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod lang;
mod tok;

use bilingua_model::guard;
use bilingua_model::*;
use std::collections::HashSet;

/// The ASS / SSA filter.
pub struct AssFilter;

/// Hidden code of every unit: `standalone` = the original core text, `label` = `title` or `event`.
const RAW: &str = "raw";
const KIND: &str = "kind"; // "title" (a [Script Info] title) or "event" (a Dialogue line)
/// Characters per line a QA check may use as a default (the same hint the SRT and VTT filters carry).
pub const DEFAULT_MAX_CPL: &str = "42";

/// The `format_options` keys of the ASS/SSA filter.
pub const FORMAT_OPTIONS: &[bilingua_model::FormatOptionKey] =
    &[bilingua_model::FormatOptionKey::bool("ass.include_locked", "List commented-out and other locked lines as locked units too.")];

impl Filter for AssFilter {
    fn format_option_keys(&self) -> &'static [bilingua_model::FormatOptionKey] {
        FORMAT_OPTIONS
    }
    fn id(&self) -> &'static str {
        "ass"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["ass", "ssa"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        sniff(bytes)
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        extract(bytes, opts)
    }
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        merge(doc, opts)
    }
}

// ---------- lines and keywords ----------

/// Every line as (start, end of body, end including its terminator). Lines end only at CR, LF or CRLF: U+2028,
/// U+0085 and form feed inside a Text field do not cut a subtitle in two (libass splits on CR and LF only).
fn lines(text: &str) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
    let b = text.as_bytes();
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        if pos >= b.len() {
            return None;
        }
        let start = pos;
        let body_end = b[pos..].iter().position(|c| *c == b'\r' || *c == b'\n').map_or(b.len(), |p| pos + p);
        let end = if body_end >= b.len() {
            b.len()
        } else if b[body_end] == b'\r' && b.get(body_end + 1) == Some(&b'\n') {
            body_end + 2
        } else {
            body_end + 1
        };
        pos = end;
        Some((start, body_end, end))
    })
}

fn ws_len(s: &str) -> usize {
    s.bytes().take_while(|c| *c == b' ' || *c == b'\t').count()
}

/// Case-insensitive keyword as Python's `re.IGNORECASE` reads it (`i` also matches the Turkish dotted and
/// dotless capital and small i).
fn ci_char(c: char, k: char) -> bool {
    c == k || c.to_ascii_lowercase() == k || (k == 'i' && (c == '\u{130}' || c == '\u{131}'))
}

/// Length of `kw` at the start of `s` (ignoring case when `ci`) and then `[ \t]*:`: the offset just after the colon.
fn colon_end(s: &str, kw: &str, ci: bool) -> Option<usize> {
    let mut it = s.char_indices();
    let mut at = 0usize;
    for k in kw.chars() {
        let (i, c) = it.next()?;
        let ok = if ci { ci_char(c, k) } else { c == k };
        if !ok {
            return None;
        }
        at = i + c.len_utf8();
    }
    at += ws_len(&s[at..]);
    s[at..].starts_with(':').then_some(at + 1)
}

/// The head of a line: `kw[ \t]*:[ \t]*`, as a length.
fn head_len(s: &str, kw: &str, ci: bool) -> Option<usize> {
    let at = colon_end(s, kw, ci)?;
    Some(at + ws_len(&s[at..]))
}

/// `[Name]` section header: the lower-cased, stripped name.
fn section_name(body: &str) -> Option<String> {
    let st = py_strip(body);
    if st.starts_with('[') && st.ends_with(']') && st.len() >= 2 {
        Some(py_strip(&st[1..st.len() - 1]).to_lowercase())
    } else {
        None
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Other,
    ScriptInfo,
    Events,
}

fn section_of(name: &str) -> Section {
    match name {
        "script info" => Section::ScriptInfo,
        "events" => Section::Events,
        _ => Section::Other,
    }
}

// ---------- sniff ----------

fn sniff(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    let text: String = if head.starts_with(&[0xFF, 0xFE]) {
        bilingua_model::encoding_rs::UTF_16LE.decode_without_bom_handling(&head[2..]).0.into_owned()
    } else if head.starts_with(&[0xFE, 0xFF]) {
        bilingua_model::encoding_rs::UTF_16BE.decode_without_bom_handling(&head[2..]).0.into_owned()
    } else {
        String::from_utf8_lossy(head).into_owned()
    };
    let text = text.trim_start_matches('\u{feff}');
    let hit = lines(text).take(40).any(|(a, b, _)| {
        matches!(
            py_strip(&text[a..b]).to_ascii_lowercase().as_str(),
            "[script info]" | "[v4+ styles]" | "[v4 styles]" | "[events]" | "[v4++ styles]"
        )
    });
    hit
}

// ---------- extract ----------

struct Found {
    /// Byte range of the unit's core in the text.
    core: (usize, usize),
    unit: Unit,
}

fn extract(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let limits = opts.limits();
    guard::check_input(bytes, &limits)?;
    let mut dec = enc::decode_text(bytes, &enc::DecodeOptions::new(opts).mark_before_legacy())?;
    let text = std::mem::take(&mut dec.text);
    let include_locked = opts.include_locked || opts.format_flag("ass.include_locked").unwrap_or(false);

    let mut found: Vec<Found> = Vec::new();
    let mut section = Section::Other;
    let mut names: Vec<String> = default_names();
    let mut n_fields = names.len();
    let mut text_last = true;
    let mut event_no = 0usize;
    for (lineno, (start, body_end, _end)) in lines(&text).enumerate() {
        let body = &text[start..body_end];
        if let Some(name) = section_name(body) {
            section = section_of(&name);
            if section == Section::Events {
                names = default_names();
                n_fields = names.len();
                text_last = true;
            }
            continue;
        }
        match section {
            Section::ScriptInfo => {
                if let Some(h) = head_len(body, "title", true) {
                    let rest = &body[h..];
                    let tail = ws_len_back(rest);
                    let core = &rest[..rest.len() - tail];
                    let stripped = py_strip(core);
                    if !stripped.is_empty() && stripped.to_lowercase() != "<untitled>" {
                        let mut u = Unit::new(format!("l{}", lineno + 1), vec![Inline::Text(core.to_string())]);
                        u.context.kind = Some("title".into());
                        u.context.key = Some("Title".into());
                        u.context.location = Some(format!("line {}", lineno + 1));
                        u.set_hidden(RAW, core.to_string());
                        u.set_hidden(KIND, "title");
                        let a = start + h;
                        found.push(Found { core: (a, a + core.len()), unit: u });
                        guard::check_unit_count(found.len(), &limits)?;
                    }
                }
            }
            Section::Events => {
                if let Some(c) = colon_end(body, "format", true) {
                    names = body[c..].split(',').map(|f| py_strip(f).to_lowercase()).collect();
                    n_fields = names.len().max(1);
                    text_last = names.last().is_some_and(|n| n == "text");
                    continue;
                }
                let (is_comment, h) = if let Some(h) = head_len(body, "Dialogue", false) {
                    (false, h)
                } else if let Some(h) = head_len(body, "Comment", false) {
                    (true, h)
                } else {
                    continue;
                };
                event_no += 1;
                if !text_last || (is_comment && !include_locked) {
                    continue;
                }
                let rest = &body[h..];
                let Some(field_at) = split_text_field(rest, n_fields) else { continue };
                let field = &rest[field_at..];
                if !is_comment {
                    guard::check_unit_chars(field.chars().count(), &limits)?;
                } else if guard::check_unit_chars(field.chars().count(), &limits).is_err() {
                    continue; // an over-long locked Comment stays skeleton (and is never tokenized)
                }
                if tok::in_drawing_mode(field) {
                    continue;
                }
                let Some(tk) = tok::tokenize(field) else { continue };
                let mut u = Unit::new(format!("l{}", lineno + 1), tk.inlines);
                u.codes = tk.codes;
                let core = &field[tk.prefix_len..field.len() - tk.suffix_len];
                u.set_hidden(RAW, core.to_string());
                u.set_hidden(KIND, "event");
                u.context.kind = Some("subtitle".into());
                u.context.key = Some(event_no.to_string());
                u.context.location = Some(format!("line {}", lineno + 1));
                fields_context(&mut u, &rest[..field_at], &names);
                u.context.extra.insert("max_cpl".into(), DEFAULT_MAX_CPL.into());
                if is_comment {
                    u.translate = false;
                    u.lock_reason = Some("Comment line".into());
                }
                let a = start + h + field_at + tk.prefix_len;
                found.push(Found { core: (a, a + core.len()), unit: u });
                guard::check_unit_count(found.len(), &limits)?;
            }
            Section::Other => {}
        }
    }

    let mut units = Vec::with_capacity(found.len());
    let mut parts = Vec::new();
    let mut cursor = 0usize;
    for (i, f) in found.into_iter().enumerate() {
        if f.core.0 > cursor {
            parts.push(SkelPart::Raw(text[cursor..f.core.0].to_string()));
        }
        parts.push(SkelPart::Unit(i));
        cursor = f.core.1;
        units.push(f.unit);
    }
    if cursor < text.len() {
        parts.push(SkelPart::Raw(text[cursor..].to_string()));
    }
    guard::finish(
        Document::new("ass", units, Skeleton::text(parts, dec.encoding.as_str(), parser_info!())).with_options(opts),
        std::mem::take(&mut dec.warnings),
        &limits,
    )
}

/// Layer|Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
fn default_names() -> Vec<String> {
    ["layer", "start", "end", "style", "name", "marginl", "marginr", "marginv", "effect", "text"].iter().map(|s| s.to_string()).collect()
}

fn ws_len_back(s: &str) -> usize {
    s.bytes().rev().take_while(|c| *c == b' ' || *c == b'\t').count()
}

/// Byte offset in `rest` where the last of `n_fields` comma-separated fields starts, or None with too few fields.
fn split_text_field(rest: &str, n_fields: usize) -> Option<usize> {
    if n_fields <= 1 {
        return Some(0);
    }
    let mut seen = 1usize;
    for (i, c) in rest.bytes().enumerate() {
        if c == b',' {
            seen += 1;
            if seen == n_fields {
                return Some(i + 1);
            }
        }
    }
    None
}

fn fields_context(u: &mut Unit, before_text: &str, names: &[String]) {
    for (name, value) in names.iter().zip(before_text.split(',')) {
        let key = match name.as_str() {
            "start" | "end" | "style" | "name" | "effect" => name.as_str(),
            "layer" | "marked" => "layer",
            _ => continue,
        };
        let v = py_strip(value);
        if !v.is_empty() {
            u.context.extra.insert(key.into(), v.to_string());
        }
    }
    if let Some(style) = u.context.extra.get("style") {
        u.context.style = Some(style.clone());
    }
}

// ---------- merge ----------

fn merge(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    check_parser(doc, &parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write("ass needs a text skeleton".into()));
    };
    let mut out = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => out.push_str(s),
            SkelPart::Unit(i) => {
                let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                let original = u.hidden(RAW).unwrap_or("");
                let translated = u.translate && u.segments.iter().any(|s| s.has_target());
                if !translated || u.output() == u.source() {
                    out.push_str(original);
                } else if u.hidden(KIND) == Some("title") {
                    out.push_str(&render_title(u));
                } else {
                    out.push_str(&render_event(u));
                }
            }
        }
    }
    if opts.should_stamp(doc) {
        if let Some(target) = doc.target_lang.as_deref() {
            out = stamp_language(&out, target);
        }
    }
    let bytes = enc::encode_text_with(&out, encoding, opts)?;
    // UTF-8 out (LocOS `ass_filter.build`): a file that started with a UTF-16 byte order mark keeps a mark, the
    // UTF-8 one (B-SIDE W6 ass-3)
    if opts.output_encoding == OutputEncoding::Utf8 && enc::base(encoding).starts_with("utf-16") && !bytes.starts_with(b"\xef\xbb\xbf") {
        let mut v = b"\xef\xbb\xbf".to_vec();
        v.extend_from_slice(&bytes);
        return Ok(v);
    }
    Ok(bytes)
}

fn newlines_to(s: &str, with: &str) -> String {
    s.replace("\r\n", with).replace(['\r', '\n'], with)
}

/// A header is one line of text: a line break becomes a space (and so does a typed `\N`).
fn render_title(u: &Unit) -> String {
    let t = plain_text(&u.output());
    newlines_to(&t, "\\N").replace("\\N", " ")
}

/// Text typed by a translator can never become a control code: `{` (which would open an override block) and a
/// backslash that would make `\N`, `\n`, `\h` or `\{` are written as their full-width look-alikes. A real line break
/// becomes `\N`.
fn safe_text(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut it = t.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' => out.push('\u{ff5b}'),
            '\\' if it.peek().is_none_or(|n| matches!(n, 'N' | 'n' | 'h' | '{' | '}')) => out.push('\u{ff3c}'),
            c => out.push(c),
        }
    }
    newlines_to(&out, "\\N")
}

struct Out {
    text: bool,
    s: String,
}

fn render_event(u: &Unit) -> String {
    let mut out: Vec<Out> = Vec::new();
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut used: HashSet<(String, TagKind)> = HashSet::new();
    let close = |out: &mut Vec<Out>, id: &str, mark: usize| {
        if out.len() == mark {
            out.truncate(mark - 1); // nothing between: no switch around nothing
        } else if let Some(c) = u.codes.get(id).and_then(|c| c.close.clone()) {
            out.push(Out { text: false, s: c });
        }
    };
    for i in u.output() {
        match i {
            Inline::Text(t) => {
                if !t.is_empty() {
                    out.push(Out { text: true, s: t });
                }
            }
            Inline::Tag { id, kind, .. } => {
                let Some(c) = u.codes.get(&id) else { continue };
                match kind {
                    TagKind::Open => {
                        if let Some(o) = c.open.as_ref() {
                            if used.insert((id.clone(), kind)) {
                                out.push(Out { text: false, s: o.clone() });
                                stack.push((id, out.len()));
                            }
                        }
                    }
                    TagKind::Close => {
                        if stack.last().is_some_and(|(sid, _)| *sid == id) {
                            if let Some((sid, mark)) = stack.pop() {
                                close(&mut out, &sid, mark);
                            }
                        }
                    }
                    TagKind::Standalone => {
                        if let Some(s) = c.standalone.as_ref() {
                            if used.insert((id, kind)) {
                                out.push(Out { text: false, s: s.clone() });
                            }
                        }
                    }
                }
            }
        }
    }
    while let Some((sid, mark)) = stack.pop() {
        close(&mut out, &sid, mark);
    }
    let mut res = String::new();
    let mut run = String::new();
    for p in out {
        if p.text {
            run.push_str(&p.s);
        } else {
            if !run.is_empty() {
                res.push_str(&safe_text(&std::mem::take(&mut run)));
            }
            res.push_str(&p.s);
        }
    }
    if !run.is_empty() {
        res.push_str(&safe_text(&run));
    }
    res
}

// ---------- language header ----------

/// Sets the `Language:` header of `[Script Info]` (the libass shaping hint, two letters) to the target language,
/// adding one after the section header when the script has none. A target with no two-letter code drops a stale
/// header; a target that is no language tag at all changes nothing.
fn stamp_language(text: &str, target: &str) -> String {
    let Some(tag) = lang::html_lang(target) else { return text.to_string() };
    let lg = tag.split('-').next().unwrap_or("").to_ascii_lowercase();
    let two = if lg.len() == 2 && lg.bytes().all(|c| c.is_ascii_alphabetic()) { lg } else { String::new() };
    let mut out: Vec<String> = Vec::new();
    let mut section = Section::Other;
    let mut info_at: Option<usize> = None;
    let mut found = false;
    for (start, body_end, end) in lines(text) {
        let body = &text[start..body_end];
        let eol = &text[body_end..end];
        if let Some(name) = section_name(body) {
            section = section_of(&name);
            out.push(text[start..end].to_string());
            if section == Section::ScriptInfo && info_at.is_none() {
                info_at = Some(out.len());
            }
            continue;
        }
        if section == Section::ScriptInfo {
            if let Some(h) = head_len(body, "language", true) {
                found = true;
                if !two.is_empty() {
                    let rest = &body[h..];
                    let tail = ws_len_back(rest);
                    out.push(format!("{}{}{}{}", &body[..h], two, &rest[rest.len() - tail..], eol));
                }
                continue;
            }
        }
        out.push(text[start..end].to_string());
    }
    if !found && !two.is_empty() {
        if let Some(at) = info_at {
            let header = &mut out[at - 1];
            let eol = if header.ends_with("\r\n") {
                "\r\n"
            } else if header.ends_with('\r') {
                "\r"
            } else if header.ends_with('\n') {
                "\n"
            } else {
                header.push('\n'); // a header on the last line without a line end
                "\n"
            };
            out.insert(at, format!("Language: {two}{eol}"));
        }
    }
    out.concat()
}

#[cfg(test)]
mod tests;
