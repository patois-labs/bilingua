//! Bilingua `.properties` filter (Java and Mozilla). See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod moz;
mod ph;
mod scan;

use bilingua_model::*;
use regex::Regex;
use scan::{Entry, Mode};
use std::sync::OnceLock;

/// The `.properties` filter.
pub struct Properties;

/// Size limits. Hostile files are refused with a typed error before any large allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// Largest input in bytes.
    pub max_input_bytes: usize,
    /// Most translatable units (or locked ones) one file may hold.
    pub max_units: usize,
    /// Longest single value in characters.
    pub max_value_chars: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_input_bytes: 256 << 20, max_units: 1_000_000, max_value_chars: 1_000_000 }
    }
}

/// One entry as a key-based project sees it (translatable or not).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Record {
    pub key: String,
    pub value: String,
    /// The comment lines above the entry, joined with newlines.
    pub comment: String,
    /// Index among the translatable units; `None` when the entry is not translatable.
    pub unit_index: Option<usize>,
    /// The same key appeared earlier.
    pub duplicate: bool,
}

// Reserved code ids (start with `~`, never a numeric tag id).
const RAW: &str = "raw"; // the raw value text of the original
const LEAD: &str = "lead"; // white space the value started with
const TRAIL: &str = "trail"; // white space the value ended with
const MF: &str = "mf"; // "d": MessageFormat value with every apostrophe doubled; "e": other MessageFormat value

fn placeholder_only() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{[^{}]*\}|%(?:\d+\$)?[-#+0-9.]*(?:hh|ll|[hlqLzjt])?[a-zA-Z@]").expect("placeholder regex"))
}

/// A value with a letter or digit outside its placeholders. `{0}`, `%1$@` and `---` are skipped.
fn is_translatable(v: &str) -> bool {
    if !bilingua_model::py_has_alnum(v) {
        return false;
    }
    if !v.contains(['{', '%']) {
        return true;
    }
    placeholder_only().replace_all(v, "").chars().any(bilingua_model::py_isalnum_char)
}

/// MessageFormat value whose apostrophes are all doubled (or absent): the translator sees single ones.
fn mf_doubled(v: &str) -> bool {
    moz::mf_arg().is_match(v) && !v.replace("''", "").contains('\'')
}

fn limit_err(kind: guard::LimitKind, msg: String) -> Error {
    guard::LimitError::new(kind, format!("input limit: {msg}")).into()
}

fn check_limits(bytes: &[u8], l: &Limits) -> Result<(), Error> {
    if bytes.len() > l.max_input_bytes {
        return Err(limit_err(guard::LimitKind::InputBytes, format!("the file is larger than {} bytes", l.max_input_bytes)));
    }
    Ok(())
}

fn line_budget(text: &str, l: &Limits) -> Result<(), Error> {
    let limit = l.max_units.saturating_mul(20);
    let breaks = text.bytes().filter(|&b| b == b'\n' || b == b'\r').count();
    if breaks > limit {
        return Err(limit_err(guard::LimitKind::Lines, format!("the file has more than {limit} lines")));
    }
    Ok(())
}

fn tags_of(core: &str, mozilla: bool) -> (Vec<Inline>, std::collections::BTreeMap<String, CodeData>) {
    let mut inl = Vec::new();
    let mut codes = std::collections::BTreeMap::new();
    let mut n = 0usize;
    for piece in ph::split(core, mozilla) {
        match piece {
            ph::Piece::Text(t) => inl.push(Inline::text(t)),
            ph::Piece::Ph(p) => {
                n += 1;
                let id = n.to_string();
                codes.insert(id.clone(), CodeData::new().with_standalone(p.to_string()).with_label("placeholder".to_string()));
                inl.push(if profile::tokens::is_named_placeholder(p) { Inline::free_tag(id, p) } else { Inline::tag(id, TagKind::Standalone, p) });
            }
        }
    }
    (inl, codes)
}

fn code(u: &Unit, key: &str) -> Option<String> {
    u.hidden(key).map(str::to_string)
}

fn render(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, display, .. } => match u.codes.get(id).and_then(|c| c.standalone.as_deref()) {
                Some(c) => s.push_str(c),
                None => s.push_str(display),
            },
        }
    }
    s
}

/// MessageFormat: write `''` for a single apostrophe between two letters that would open a quote.
fn double_elisions(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 4);
    let mut quoted = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '\'' {
            out.push(c);
            i += 1;
            continue;
        }
        if chars.get(i + 1) == Some(&'\'') {
            out.push_str("''");
            i += 2;
            continue;
        }
        if !quoted && i > 0 && bilingua_model::py_isalpha_char(chars[i - 1]) && chars.get(i + 1).is_some_and(|c| bilingua_model::py_isalpha_char(*c)) {
            out.push_str("''");
        } else {
            out.push('\'');
            quoted = !quoted;
        }
        i += 1;
    }
    out
}

/// A translated value in `.properties` escaping.
fn encode_value(new: &str, mf: &str, mode: Mode) -> String {
    let adjusted;
    let new = match mf {
        "d" => {
            adjusted = new.replace("''", "'").replace('\'', "''");
            adjusted.as_str()
        }
        "e" => {
            adjusted = double_elisions(new);
            adjusted.as_str()
        }
        _ => new,
    };
    let limit: Option<u32> = match mode {
        Mode::Ascii => Some(0x7E),
        Mode::Latin1 => Some(0xFF),
        Mode::Utf8 => None,
    };
    let mut out = String::with_capacity(new.len() + 8);
    for (idx, ch) in new.chars().enumerate() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0C}' => out.push_str("\\f"),
            ' ' if idx == 0 => out.push_str("\\ "),
            '=' | ':' if idx == 0 => {
                out.push('\\');
                out.push(ch);
            }
            c if (c as u32) < 0x20 || c == '\u{7f}' || limit.is_some_and(|l| (c as u32) > l) => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04X}", unit));
                }
            }
            c => out.push(c),
        }
    }
    out
}

struct Parsed<'a> {
    text: std::borrow::Cow<'a, str>,
    mode: Mode,
    bom: bool,
    entries: Vec<Entry>,
    mozilla: bool,
}

fn parse<'a>(bytes: &'a [u8], l: &Limits) -> Result<Parsed<'a>, Error> {
    check_limits(bytes, l)?;
    let d = scan::decode(bytes)?;
    line_budget(&d.text, l)?;
    let entries = scan::scan(&d.text)?;
    let mozilla = d.mode == Mode::Utf8 && moz::looks_mozilla(&d.text);
    Ok(Parsed { text: d.text, mode: d.mode, bom: d.bom, entries, mozilla })
}

impl Properties {
    /// Like [`Filter::extract`] with explicit limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        let p = parse(bytes, limits)?;
        let mut units: Vec<Unit> = Vec::new();
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut pos = 0usize;
        let (mut n_tr, mut n_lock) = (0usize, 0usize);
        for e in &p.entries {
            let translatable = is_translatable(&e.value);
            if !translatable && (!opts.include_locked || scan::strip(&e.value).is_empty()) {
                continue;
            }
            if units.len() >= limits.max_units {
                return Err(limit_err(guard::LimitKind::Units, format!("the file has more than {} entries", limits.max_units)));
            }
            let mf_is_doubled = translatable && mf_doubled(&e.value);
            let shown: std::borrow::Cow<str> = if mf_is_doubled { e.value.replace("''", "'").into() } else { (&e.value[..]).into() };
            let (lead, core, trail) = scan::split_ws(&shown);
            if core.len() > limits.max_value_chars && core.chars().count() > limits.max_value_chars {
                return Err(limit_err(guard::LimitKind::UnitChars, format!("a value is longer than {} characters", limits.max_value_chars)));
            }
            let mut unit = if translatable {
                n_tr += 1;
                let (inl, codes) = tags_of(core, p.mozilla);
                let mut u = Unit::new(format!("u{n_tr}"), inl);
                u.codes = codes;
                u
            } else {
                n_lock += 1;
                let mut u = Unit::new(format!("k{n_lock}"), vec![Inline::text(core)]);
                u.translate = false;
                u
            };
            unit.set_hidden(RAW, &p.text[e.start..e.end]);
            if !lead.is_empty() {
                unit.set_hidden(LEAD, lead);
            }
            if !trail.is_empty() {
                unit.set_hidden(TRAIL, trail);
            }
            if translatable {
                if mf_is_doubled {
                    unit.set_hidden(MF, "d");
                } else if moz::mf_arg().is_match(&e.value) {
                    unit.set_hidden(MF, "e");
                }
            } else {
                unit.lock_reason = Some("no translatable text".into());
            }
            unit.context.kind = Some("string".into());
            unit.context.key = Some(e.key.clone());
            unit.context.location = Some(format!("line {}", e.line));
            unit.context.notes = e.comment.iter().filter(|c| !c.is_empty()).cloned().collect();
            if e.start > pos {
                parts.push(SkelPart::Raw(p.text[pos..e.start].to_string()));
            }
            pos = e.end;
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
        }
        if pos < p.text.len() {
            parts.push(SkelPart::Raw(p.text[pos..].to_string()));
        }
        mark_duplicates(&mut units);
        Ok(Document::new("properties", units, Skeleton::text(parts, p.mode.label(p.bom), bilingua_model::parser_info!())).with_options(opts))
    }

    /// Every entry in file order, translatable or not (the key-based view).
    pub fn records(&self, bytes: &[u8]) -> Result<Vec<Record>, Error> {
        let p = parse(bytes, &Limits::default())?;
        let mut seen = std::collections::HashSet::new();
        let mut k = 0usize;
        let mut out = Vec::with_capacity(p.entries.len());
        for e in &p.entries {
            let unit_index = is_translatable(&e.value).then(|| {
                k += 1;
                k - 1
            });
            out.push(Record {
                key: e.key.clone(),
                value: e.value.clone(),
                comment: e.comment.join("\n"),
                unit_index,
                duplicate: !seen.insert(e.key.clone()),
            });
        }
        Ok(out)
    }
}

/// Flags the second and later units that share a key (`extra["duplicate"] = "true"`).
fn mark_duplicates(units: &mut [Unit]) {
    let mut seen = std::collections::HashSet::new();
    for u in units.iter_mut() {
        if let Some(k) = &u.context.key {
            if !seen.insert(k.clone()) {
                u.context.extra.insert("duplicate".into(), "true".into());
            }
        }
    }
}

impl Filter for Properties {
    fn id(&self) -> &'static str {
        "properties"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["properties"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(4096)];
        let Ok(d) = scan::decode(head).or_else(|_| scan::decode(&head[..head.len().saturating_sub(3)])) else { return false };
        let (mut kv, mut other) = (0, 0);
        let mut cont = false;
        for l in d.text.lines() {
            let t = l.trim();
            if t.is_empty() || t.starts_with('#') || t.starts_with('!') || cont {
                cont = !t.starts_with('#') && !t.starts_with('!') && !t.is_empty() && t.ends_with('\\');
                continue;
            }
            cont = t.ends_with('\\');
            let key_end = t.find(['=', ':', ' ', '\t']).unwrap_or(t.len());
            let key = &t[..key_end];
            let key_ok = !key.is_empty() && key.chars().all(|c| bilingua_model::py_isalnum_char(c) || matches!(c, '.' | '_' | '-' | '[' | ']' | '/' | '$' | '\\'));
            let sep = t[key_end..].trim_start().starts_with(['=', ':']);
            if key_ok && sep && !t.starts_with(['{', '[', '<']) {
                kv += 1;
            } else {
                other += 1;
            }
        }
        kv > 0 && other * 5 <= kv
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        self.extract_with(bytes, opts, &Limits::default())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("properties needs a text skeleton".into()));
        };
        let (mode, bom) = Mode::from_label(encoding).ok_or_else(|| Error::Write(format!("unknown encoding {encoding}")))?;
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    write_unit(u, mode, &mut out);
                }
            }
        }
        let mut bytes = Vec::with_capacity(out.len() + 3);
        if bom {
            bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        }
        if mode == Mode::Latin1 && opts.output_encoding != bilingua_model::OutputEncoding::Utf8 {
            for c in out.chars() {
                let v = c as u32;
                if v > 0xFF {
                    return Err(Error::Unwritable { unit: None, reason: format!("U+{v:04X} does not fit ISO-8859-1") });
                }
                bytes.push(v as u8);
            }
        } else {
            bytes.extend_from_slice(out.as_bytes());
        }
        Ok(bytes)
    }
}

fn write_unit(u: &Unit, mode: Mode, out: &mut String) {
    let raw = u.hidden(RAW).unwrap_or("");
    if !u.translate || u.segments.iter().all(|s| !s.has_target()) {
        out.push_str(raw);
        return;
    }
    let new = render(u, &u.output());
    if new == render(u, &u.source()) {
        out.push_str(raw);
        return;
    }
    let full = format!("{}{}{}", code(u, LEAD).unwrap_or_default(), new, code(u, TRAIL).unwrap_or_default());
    out.push_str(&encode_value(&full, code(u, MF).as_deref().unwrap_or(""), mode));
}

#[cfg(test)]
mod tests;
