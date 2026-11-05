//! Bilingua Windows `.resjson` filter.
//!
//! A `.resjson` file is one JSON object of resource id to string (Windows Store and WinJS apps). The format adds
//! two things to JSON: `_<id>.comment` members, which are translator notes and never text to translate (any
//! other `_<id>.<attr>` member is metadata too), and JavaScript comments (`//`, `/* */`), which Visual Studio
//! templates carry. Nested objects are walked; the path of the objects above a string is its context.
//!
//! Behaviour follows Localization OS (`resjson_filter.py`, `kv_resource.py`):
//!
//! - every string value that has a letter or digit outside its placeholders is a unit; `{0}`, `%s`, `---` and
//!   the like stay in the skeleton (or come as locked units with `include_locked`);
//! - white space around the text of a value is kept outside the unit;
//! - placeholders such as `{0}` stay plain text: the LocOS filter does not tokenise them (its placeholder
//!   checks run in a later layer), and Okapi's JSON filter does not either;
//! - the file is never re-serialised. A value that was not changed is written back as its original spelling
//!   (`é`, `\/`); a changed one is JSON-escaped once. Comments, key order, spacing, line endings and the
//!   byte order mark survive byte for byte;
//! - UTF-8 (with or without BOM) and UTF-16 with a BOM are read, and written back the same way. UTF-32 and
//!   other encodings are refused with a typed error.
//!
//! The note of a unit (`_id.comment`) is in `context.notes`; `_id.maxLength` sets `context.max_len`; other
//! `_id.attr` members go to `context.extra[attr]`. `context.key` is the JSON pointer; the LocOS context and key
//! are in `extra["path"]` and `extra["name"]`.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod scan;

use bilingua_model::*;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub use scan::{decode as decode_string, is_meta, quote_inner};

const ID: &str = "resjson";
const BOM8: &[u8] = &[0xEF, 0xBB, 0xBF];
/// Hidden codes on a unit: the original spelling of the value, and the white space around its text.
const RAW_KEY: &str = "$raw";
const LEAD_KEY: &str = "$lead";
const TRAIL_KEY: &str = "$trail";

/// Size limits. The defaults are the Localization OS ones.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Most units in one file (LocOS `MAX_DOCUMENT_SEGMENTS`).
    pub max_units: usize,
    /// Longest unit text in characters (LocOS `MAX_SEGMENT_CHARS`).
    pub max_unit_chars: usize,
    /// Deepest nesting of objects and arrays, the top-level object included.
    pub max_depth: usize,
    /// Largest file in bytes.
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_units: 50_000, max_unit_chars: 100_000, max_depth: 64, max_bytes: 256 * 1024 * 1024 }
    }
}

/// The Windows `.resjson` filter.
#[derive(Debug, Default, Clone, Copy)]
pub struct Resjson;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Enc {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

impl Enc {
    fn label(self) -> &'static str {
        match self {
            Enc::Utf8 => "utf-8",
            Enc::Utf8Bom => "utf-8-bom",
            Enc::Utf16Le => "utf-16le",
            Enc::Utf16Be => "utf-16be",
        }
    }
    fn from_label(s: &str) -> Option<Enc> {
        Some(match s {
            "utf-8" => Enc::Utf8,
            "utf-8-bom" => Enc::Utf8Bom,
            "utf-16le" => Enc::Utf16Le,
            "utf-16be" => Enc::Utf16Be,
            _ => return None,
        })
    }
}

fn decode_bytes(bytes: &[u8]) -> Result<(String, Enc), Error> {
    if bytes.starts_with(&[0xFF, 0xFE, 0, 0]) || bytes.starts_with(&[0, 0, 0xFE, 0xFF]) {
        return Err(Error::Unsupported("UTF-32 files; save the file as UTF-8".into()));
    }
    let utf16 = |body: &[u8], le: bool| -> Result<String, Error> {
        if !body.len().is_multiple_of(2) {
            return Err(Error::Unsupported("the file is not valid UTF-16 (it ends in the middle of a character)".into()));
        }
        let units = body.chunks_exact(2).map(|p| if le { u16::from_le_bytes([p[0], p[1]]) } else { u16::from_be_bytes([p[0], p[1]]) });
        char::decode_utf16(units)
            .collect::<Result<String, _>>()
            .map_err(|_| Error::Unsupported("the file is not valid UTF-16 (unpaired surrogate)".into()))
    };
    if let Some(body) = bytes.strip_prefix(&[0xFF, 0xFE][..]) {
        return Ok((utf16(body, true)?, Enc::Utf16Le));
    }
    if let Some(body) = bytes.strip_prefix(&[0xFE, 0xFF][..]) {
        return Ok((utf16(body, false)?, Enc::Utf16Be));
    }
    let (body, enc) = match bytes.strip_prefix(BOM8) {
        Some(r) => (r, Enc::Utf8Bom),
        None => (bytes, Enc::Utf8),
    };
    let text = std::str::from_utf8(body).map_err(|e| {
        Error::Unsupported(format!("the file is not valid UTF-8 (byte {}); save the file as UTF-8", e.valid_up_to() + (bytes.len() - body.len())))
    })?;
    Ok((text.to_string(), enc))
}

fn encode(text: &str, enc: Enc) -> Vec<u8> {
    match enc {
        Enc::Utf8 => text.as_bytes().to_vec(),
        Enc::Utf8Bom => {
            let mut v = Vec::with_capacity(text.len() + 3);
            v.extend_from_slice(BOM8);
            v.extend_from_slice(text.as_bytes());
            v
        }
        Enc::Utf16Le | Enc::Utf16Be => {
            let le = enc == Enc::Utf16Le;
            let mut v = Vec::with_capacity(text.len() * 2 + 2);
            v.extend_from_slice(if le { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] });
            for u in text.encode_utf16() {
                v.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
            }
            v
        }
    }
}

/// The LocOS pre-scan budgets: lines, quotes and structure characters, relative to the unit limit.
fn check_budget(text: &str, limits: &Limits) -> Result<(), Error> {
    let (mut lines, mut quotes, mut structure) = (0usize, 0usize, 0usize);
    for &c in text.as_bytes() {
        match c {
            b'\n' | b'\r' => lines += 1,
            b'"' => quotes += 1,
            b',' | b'[' | b'{' => structure += 1,
            _ => {}
        }
    }
    let over = |what: &str, n: usize, max: usize| {
        if n > max {
            Err(Error::Unsupported(format!("this .resjson file has more {what} than {} units allow; split it into smaller files", limits.max_units)))
        } else {
            Ok(())
        }
    };
    over("lines", lines, 20usize.saturating_mul(limits.max_units))?;
    over("strings", quotes, 40usize.saturating_mul(limits.max_units))?;
    over("values", structure, 20usize.saturating_mul(limits.max_units))
}

/// Python `str.isspace` (what LocOS strips): White_Space plus U+001C to U+001F.
fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn split_ws(s: &str) -> (&str, &str, &str) {
    let core = s.trim_matches(is_space);
    if core.is_empty() {
        return (s, "", "");
    }
    let lead_len = s.len() - s.trim_start_matches(is_space).len();
    let trail_start = s.trim_end_matches(is_space).len();
    (&s[..lead_len], core, &s[trail_start..])
}

fn placeholder_only() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{[^{}]*\}|%(?:\d+\$)?[-#+0-9.]*(?:hh|ll|[hlqLzjt])?[a-zA-Z@]").expect("static pattern"))
}

/// A value with a letter or digit outside its placeholders (LocOS `is_translatable`).
fn is_translatable(v: &str) -> bool {
    if !v.chars().any(char::is_alphanumeric) {
        return false;
    }
    placeholder_only().replace_all(v, "").chars().any(char::is_alphanumeric)
}

fn code(s: &str) -> CodeData {
    CodeData { standalone: Some(s.to_string()), ..Default::default() }
}

impl Resjson {
    /// Like `extract`, with explicit limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        if bytes.len() > limits.max_bytes {
            return Err(Error::Unsupported(format!("this .resjson file is larger than {} bytes", limits.max_bytes)));
        }
        let (text, enc) = decode_bytes(bytes)?;
        check_budget(&text, limits)?;
        let scanned = scan::scan(&text, limits)?;

        // Notes and metadata, joined to their entry by (context, id).
        let mut notes: HashMap<(&str, &str), &str> = HashMap::new();
        for e in &scanned.entries {
            if let (true, Some(v)) = (scan::is_meta(&e.key), e.value.as_deref()) {
                if let Some(id) = e.key[1..].strip_suffix(".comment") {
                    notes.insert((e.context.as_str(), id), v);
                }
            }
        }
        let mut attrs: HashMap<(&str, &str), Vec<(&str, &str)>> = HashMap::new();
        for e in &scanned.entries {
            if let (true, Some(v)) = (scan::is_meta(&e.key), e.value.as_deref()) {
                if let Some((id, attr)) = e.key[1..].rsplit_once('.') {
                    if attr != "comment" {
                        attrs.entry((e.context.as_str(), id)).or_default().push((attr, v));
                    }
                }
            }
        }
        for l in &scanned.literals {
            if let Some((id, attr)) = l.key[1..].rsplit_once('.') {
                if attr != "comment" {
                    attrs.entry((l.context.as_str(), id)).or_default().push((attr, l.text.as_str()));
                }
            }
        }

        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut seen: HashSet<(&str, &str)> = HashSet::new();
        let mut pos = 0usize;
        let mut counted = 0usize;
        for e in &scanned.entries {
            let duplicate = !seen.insert((e.context.as_str(), e.key.as_str()));
            let Some(value) = e.value.as_deref() else { continue };
            let meta = scan::is_meta(&e.key);
            let translatable = !meta && is_translatable(value);
            let (lead, core, trail) = split_ws(value);
            if core.is_empty() || (!translatable && !opts.include_locked) {
                continue;
            }
            if translatable {
                if core.len() > limits.max_unit_chars && core.chars().count() > limits.max_unit_chars {
                    return Err(Error::Unsupported(format!("this .resjson file has a value longer than {} characters", limits.max_unit_chars)));
                }
                if counted >= limits.max_units {
                    return Err(Error::Unsupported(format!("this .resjson file has more than {} units", limits.max_units)));
                }
            }
            if translatable {
                counted += 1;
            }
            let raw = &text[e.start..e.end];
            if e.start > pos {
                parts.push(SkelPart::Raw(text[pos..e.start].to_string()));
            }
            pos = e.end;
            let mut unit = Unit::new(format!("u{}", units.len() + 1), vec![Inline::text(core)]);
            if !lead.is_empty() {
                unit.codes.insert(LEAD_KEY.into(), code(lead));
            }
            if !trail.is_empty() {
                unit.codes.insert(TRAIL_KEY.into(), code(trail));
            }
            // Keep the original spelling only where re-quoting would change it.
            if quote_inner(value) != raw {
                unit.codes.insert(RAW_KEY.into(), code(raw));
            }
            unit.context.kind = Some(if meta { "metadata" } else { "string" }.into());
            unit.context.key = Some(e.pointer.clone());
            unit.context.extra.insert("path".into(), e.context.clone());
            unit.context.extra.insert("name".into(), e.key.clone());
            if duplicate {
                unit.context.extra.insert("duplicate".into(), "true".into());
            }
            if !meta {
                if let Some(n) = notes.get(&(e.context.as_str(), e.key.as_str())) {
                    unit.context.notes.push((*n).to_string());
                }
                for (attr, v) in attrs.get(&(e.context.as_str(), e.key.as_str())).into_iter().flatten() {
                    match (*attr, v.parse::<usize>()) {
                        ("maxLength", Ok(n)) => unit.context.max_len = Some(n),
                        _ => {
                            unit.context.extra.insert((*attr).to_string(), (*v).to_string());
                        }
                    }
                }
            }
            unit.translate = translatable;
            if !translatable {
                unit.lock_reason = Some(if meta { "metadata, not text to translate" } else { "no translatable text" }.into());
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        Ok(Document {
            format: ID.into(),
            name: None,
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(parts, enc.label(), bilingua_model::parser_info!()),
        })
    }
}

fn text_of(v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { display, .. } => s.push_str(display),
        }
    }
    s
}

/// What goes between the quotes for a unit.
fn render(unit: &Unit) -> String {
    let lead = unit.codes.get(LEAD_KEY).and_then(|c| c.standalone.as_deref()).unwrap_or("");
    let trail = unit.codes.get(TRAIL_KEY).and_then(|c| c.standalone.as_deref()).unwrap_or("");
    let out = text_of(&unit.output());
    if out == text_of(&unit.source()) {
        if let Some(raw) = unit.codes.get(RAW_KEY).and_then(|c| c.standalone.as_deref()) {
            return raw.to_string();
        }
    }
    let mut full = String::with_capacity(lead.len() + out.len() + trail.len());
    full.push_str(lead);
    full.push_str(&out);
    full.push_str(trail);
    quote_inner(&full)
}

impl Filter for Resjson {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["resjson"]
    }

    /// Only a file that is a .resjson and not plain JSON: it has a JavaScript comment or a `_id.comment` member.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let limits = Limits::default();
        let Ok((text, _)) = decode_bytes(bytes) else { return false };
        if check_budget(&text, &limits).is_err() {
            return false;
        }
        match scan::scan(&text, &limits) {
            Ok(s) => s.saw_comment || s.entries.iter().any(|e| scan::is_meta(&e.key) && e.key.ends_with(".comment")),
            Err(_) => false,
        }
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &Limits::default())
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("resjson needs a text skeleton".into()));
        };
        let enc = Enc::from_label(encoding).ok_or_else(|| Error::Write(format!("resjson cannot write the encoding {encoding}")))?;
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
        Ok(encode(&out, enc))
    }
}
