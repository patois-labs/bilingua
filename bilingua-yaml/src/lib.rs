//! Bilingua YAML filter (generic YAML, Rails i18n and Symfony translation files). See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod inline;
mod locale;
mod locale_data;
mod render;
mod resolve;
#[doc(hidden)]
pub mod scan;
mod text;
mod walk;

pub use walk::Limits;

use bilingua_model::*;
use scan::Style;
use text::Enc;
use walk::{py_space, walk, Walk};

/// The YAML filter.
pub struct Yaml;

const RAW: &str = "raw"; // the scalar's original text (its whole span)
const LEAD: &str = "lead"; // white space of the value before the unit text
const TRAIL: &str = "trail"; // and after it
const STY: &str = "sty"; // style: p ' " | >
const FLOW: &str = "flow"; // "1" inside a flow collection

fn style_char(s: Style) -> &'static str {
    match s {
        Style::Plain => "p",
        Style::Single => "'",
        Style::Double => "\"",
        Style::Literal => "|",
        Style::Folded => ">",
    }
}

fn style_from(c: &str) -> Style {
    match c {
        "'" => Style::Single,
        "\"" => Style::Double,
        "|" => Style::Literal,
        ">" => Style::Folded,
        _ => Style::Plain,
    }
}

fn style_name(s: Style) -> &'static str {
    match s {
        Style::Plain => "plain",
        Style::Single => "single-quoted",
        Style::Double => "double-quoted",
        Style::Literal => "literal",
        Style::Folded => "folded",
    }
}

fn parse_err(text: &str, e: &scan::ScanError) -> Error {
    if e.limit {
        return Error::Unsupported(format!("This YAML file is over a safety limit: {}.", e.msg));
    }
    let pos = e.pos.min(text.len());
    let line = text.as_bytes()[..pos].iter().filter(|&&c| c == b'\n').count() + 1;
    let col = pos - text.as_bytes()[..pos].iter().rposition(|&c| c == b'\n').map_or(0, |i| i + 1) + 1;
    Error::Parse { format: "yaml".into(), msg: format!("This YAML file could not be read: {} (line {line}, column {col}).", e.msg) }
}

fn code(u: &Unit, key: &str) -> String {
    u.hidden(key).map(str::to_string).unwrap_or_default()
}

fn out_text(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind, .. } => {
                if let Some(c) = u.codes.get(id) {
                    let piece = match kind {
                        TagKind::Open => c.open.as_deref(),
                        TagKind::Close => c.close.as_deref(),
                        TagKind::Standalone => c.standalone.as_deref(),
                    };
                    s.push_str(piece.unwrap_or(""));
                }
            }
        }
    }
    s
}

fn split_ws(s: &str) -> (&str, &str, &str) {
    let core = s.trim_matches(py_space);
    if core.is_empty() {
        return (s, "", "");
    }
    let lead = &s[..s.len() - s.trim_start_matches(py_space).len()];
    let trail = &s[s.trim_end_matches(py_space).len()..];
    (lead, core, trail)
}

fn has_target(u: &Unit) -> bool {
    u.segments.iter().any(|s| !s.target.is_empty())
}

/// The new value of a unit (white space around it restored) when it has a translation.
fn new_value(u: &Unit) -> Option<String> {
    if !has_target(u) {
        return None;
    }
    Some(format!("{}{}{}", code(u, LEAD), out_text(u, &u.output()), code(u, TRAIL)))
}

/// What goes in the unit's place: the original text unless the translation differs.
fn replacement(u: &Unit) -> String {
    let raw = code(u, RAW);
    let Some(nv) = new_value(u) else { return raw };
    if out_text(u, &u.output()) == out_text(u, &u.source()) {
        return raw;
    }
    render::render(&nv, style_from(&code(u, STY)), code(u, FLOW) == "1", &raw)
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(text.bytes().enumerate().filter(|(_, c)| *c == b'\n').map(|(i, _)| i + 1));
    v
}

fn comment_above(text: &str, starts: &[usize], line: usize) -> String {
    let mut got: Vec<&str> = Vec::new();
    let mut j = line;
    while j > 0 {
        j -= 1;
        let end = starts.get(j + 1).copied().unwrap_or(text.len());
        let l = text[starts[j]..end].trim_matches(py_space);
        if let Some(c) = l.strip_prefix('#') {
            got.push(c.trim_start_matches('#').trim_matches(py_space));
        } else {
            break;
        }
    }
    got.reverse();
    got.join("\n")
}

fn count_budget(text: &str, limits: &Limits) -> Result<(), Error> {
    let lines = text.bytes().filter(|&c| c == b'\n' || c == b'\r').count();
    let cap = limits.max_units.saturating_mul(20);
    if lines > cap {
        return Err(Error::Unsupported(format!("This YAML file has more than {cap} lines; split it into smaller files.")));
    }
    let flow = text.bytes().filter(|&c| matches!(c, b',' | b'[' | b'{')).count();
    if flow > cap {
        return Err(Error::Unsupported("This YAML file has far more values than the segment limit allows; split it into smaller files.".into()));
    }
    Ok(())
}

impl Yaml {
    /// Like `extract`, with explicit limits (`Limits::unlimited()` lifts the segment and size caps).
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        if bytes.len() > limits.max_bytes {
            return Err(Error::Unsupported("This YAML file is larger than the size limit.".into()));
        }
        let (text, enc) = text::decode(bytes)?;
        count_budget(&text, limits)?;
        if let Some(p) = text::first_forbidden(&text) {
            return Err(parse_err(&text, &scan::ScanError { msg: "control characters are not allowed".into(), pos: p, limit: false }));
        }
        let events = scan::scan(&text, limits.max_depth).map_err(|e| parse_err(&text, &e))?;
        let w = walk(&events, &text, limits).map_err(|e| parse_err(&text, &e))?;
        drop(events);
        let starts = line_starts(&text);
        let strip = w.rails_root.as_ref().map(|r| r.key.len() + 1);
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::with_capacity(w.units.len());
        let mut pos = 0usize;
        for (i, wu) in w.units.iter().enumerate() {
            let (lead, core, trail) = split_ws(&wu.value);
            let (inl, codes) = inline::tag_placeholders(core);
            let mut u = Unit::new(format!("u{}", i + 1), inl);
            u.codes = codes;
            u.set_hidden(RAW, &text[wu.start..wu.end]);
            u.set_hidden(LEAD, lead);
            u.set_hidden(TRAIL, trail);
            u.set_hidden(STY, style_char(wu.style));
            if wu.flow {
                u.set_hidden(FLOW, "1");
            }
            u.context.kind = Some("string".into());
            let key = match strip {
                Some(n) if wu.path.len() > n => wu.path[n..].to_string(),
                _ => wu.path.clone(),
            };
            u.context.key = Some(key);
            u.context.style = Some(style_name(wu.style).into());
            u.context.location = Some(format!("line {}", starts.partition_point(|&p| p <= wu.start)));
            if let Some(kl) = wu.key_line {
                let c = comment_above(&text, &starts, kl);
                if !c.is_empty() {
                    u.context.notes.push(c);
                }
            }
            if let Some(p) = &wu.plural {
                u.context.extra.insert("plural".into(), p.clone());
            }
            if wu.start > pos {
                parts.push(SkelPart::Raw(text[pos..wu.start].to_string()));
            }
            parts.push(SkelPart::Unit(i));
            pos = wu.end;
            units.push(u);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        if let Some(r) = &w.rails_root {
            if let Some(u) = units.first_mut() {
                u.context.extra.insert("rails_locale".into(), r.key.clone());
            }
        }
        Ok(Document::new("yaml", units, Skeleton::text(parts, enc.label(), bilingua_model::parser_info!())).with_options(opts))
    }
}

fn reconstruct(parts: &[SkelPart], units: &[Unit]) -> Result<String, Error> {
    let mut s = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(r) => s.push_str(r),
            SkelPart::Unit(i) => s.push_str(&code(units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?, RAW)),
        }
    }
    Ok(s)
}

fn target_root_key(source_key: &str, target: &str) -> String {
    let t = target.trim();
    if source_key.contains('_') {
        t.replace('-', "_")
    } else {
        t.replace('_', "-")
    }
}

/// Edits for a localized Rails file: the root key and the plural categories the target needs.
fn rails_edits(text: &str, w: &Walk, doc: &Document, target: &str, edits: &mut Vec<(usize, usize, String)>) {
    let Some(root) = &w.rails_root else { return };
    let new_key = target_root_key(&root.key, target);
    if !new_key.is_empty() && new_key != root.key {
        edits.push((root.start, root.end, render::render_key(&new_key, root.style)));
    }
    let starts = line_starts(text);
    for pm in &w.plural_maps {
        let Some(ou) = pm.other_unit else { continue };
        let Some(value) = doc.units.get(ou).and_then(new_value) else { continue };
        let wu = &w.units[ou];
        let Some(kl) = wu.key_line else { continue };
        if wu.flow || kl >= starts.len() {
            continue;
        }
        let missing = locale::missing_categories(&pm.cats, target);
        if missing.is_empty() {
            continue;
        }
        let bol = starts[kl];
        let eol_at = text[bol..].find('\n').map_or(text.len(), |i| bol + i);
        let line = &text[bol..eol_at];
        let indent_len = line.len() - line.trim_start_matches(' ').len();
        let after = &line[indent_len..];
        let is_other = ["other", "'other'", "\"other\""].iter().any(|k| after.strip_prefix(k).is_some_and(|r| r.trim_start().starts_with(':')));
        if !is_other {
            continue;
        }
        let rendered = if render::plain_ok(&value, false) { value.clone() } else { render::dq(&value) };
        let eol = if line.ends_with('\r') { "\r\n" } else { "\n" };
        let indent = &line[..indent_len];
        let ins: String = missing.iter().map(|c| format!("{indent}{c}: {rendered}{eol}")).collect();
        edits.push((bol, bol, ins));
    }
}

impl Filter for Yaml {
    fn id(&self) -> &'static str {
        "yaml"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["yaml", "yml"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(4096)];
        let head = head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head);
        let Ok(s) = std::str::from_utf8(head).or_else(|e| std::str::from_utf8(&head[..e.valid_up_to()])) else { return false };
        for l in s.lines() {
            let t = l.trim_end();
            if t.is_empty() || t.trim_start().starts_with('#') {
                continue;
            }
            if t.starts_with("%YAML") || t == "---" || t.starts_with("--- ") {
                return true;
            }
            if t.starts_with(['{', '[', '<']) {
                return false;
            }
            let t = t.trim_start();
            // `key = "a: b"` (TOML, .strings, JS) and `// note: x` comments are not YAML keys
            if t.starts_with('/') {
                return false;
            }
            return t.starts_with("- ") || (t.find(':').is_some_and(|i| !t[..i].contains(['=', ';']) && t[..i].chars().all(|c| !c.is_whitespace() || c == ' ') && (t.len() == i + 1 || t.as_bytes()[i + 1] == b' ') && i > 0));
        }
        false
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &Limits::default())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("yaml needs a text skeleton".into()));
        };
        let enc = Enc::from_label(encoding).ok_or_else(|| Error::Write(format!("unknown encoding {encoding}")))?;
        let mut changed = false;
        let out: String = // The root key and plural forms follow the target language only once something is translated (an untouched
        // document comes back byte for byte, like PO).
        match opts.stamp_lang(doc, doc.units.iter().any(has_target)) {
            None => {
                let mut out = String::new();
                for p in parts {
                    match p {
                        SkelPart::Raw(s) => out.push_str(s),
                        SkelPart::Unit(i) => {
                            let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                            let r = replacement(u);
                            changed |= r != code(u, RAW);
                            out.push_str(&r);
                        }
                    }
                }
                out
            }
            Some(target) => {
                let text = reconstruct(parts, &doc.units)?;
                let events = scan::scan(&text, 200).map_err(|e| parse_err(&text, &e))?;
                let w = walk(&events, &text, &Limits::unlimited()).map_err(|e| parse_err(&text, &e))?;
                if w.units.len() != doc.units.len() {
                    return Err(Error::Write("the document no longer matches its skeleton".into()));
                }
                let mut edits: Vec<(usize, usize, String)> = Vec::new();
                for (wu, u) in w.units.iter().zip(&doc.units) {
                    let r = replacement(u);
                    if r != code(u, RAW) {
                        edits.push((wu.start, wu.end, r));
                    }
                }
                rails_edits(&text, &w, doc, target, &mut edits);
                changed = !edits.is_empty();
                edits.sort_by_key(|e| (e.0, e.1));
                let mut out = String::with_capacity(text.len());
                let mut pos = 0;
                for (s, e, r) in edits {
                    out.push_str(&text[pos..s]);
                    out.push_str(&r);
                    pos = e;
                }
                out.push_str(&text[pos..]);
                out
            }
        };
        // A converted (UTF-16, Windows-1252) file comes back in its own encoding when nothing changed and as
        // UTF-8 when it did, like LocOS.
        let bytes = match enc {
            Enc::Utf8 | Enc::Utf8Bom => text::encode(&out, enc),
            _ if opts.output_encoding == bilingua_model::OutputEncoding::Utf8 => text::encode(&out, Enc::Utf8),
            _ if !changed => text::encode(&out, enc),
            _ => text::encode(&out, Enc::Utf8),
        };
        bytes.ok_or_else(|| Error::Write("the text does not fit the file's encoding".into()))
    }
}

