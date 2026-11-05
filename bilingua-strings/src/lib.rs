//! Bilingua Apple `.strings` filter. See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod ph;
mod scan;

use bilingua_model::*;
use scan::{decode, escape, scan, split_ws, Enc, Entry};
use std::collections::{BTreeMap, HashSet};

/// The Apple `.strings` filter.
pub struct Strings;

/// Size limits. The defaults match Localization OS (`MAX_DOCUMENT_SEGMENTS`, `MAX_SEGMENT_CHARS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Most translatable units in one file.
    pub max_units: usize,
    /// Most characters in one value.
    pub max_value_chars: usize,
    /// Largest input in bytes.
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_units: 50_000, max_value_chars: 100_000, max_bytes: 64 * 1024 * 1024 }
    }
}

// Reserved hidden code ids (start with `~`, never a numeric tag id).
const OV: &str = "~ov"; // original core text of the value (unescaped)
const RAW: &str = "~raw"; // original raw value as written in the file
const LEAD: &str = "~lead"; // white space before the core
const TRAIL: &str = "~trail"; // white space after the core
const UNQ: &str = "~unq"; // the value was an unquoted word

fn hidden(text: impl Into<String>) -> CodeData {
    CodeData { standalone: Some(text.into()), ..Default::default() }
}

fn code(u: &Unit, key: &str) -> Option<String> {
    u.codes.get(key).and_then(|c| c.standalone.clone())
}

fn limit_err(msg: String) -> Error {
    Error::Unsupported(msg)
}

fn read(bytes: &[u8], limits: &Limits) -> Result<(String, Enc, Vec<Entry>), Error> {
    if bytes.len() > limits.max_bytes {
        return Err(limit_err(format!("this .strings file is larger than {} bytes; split it into smaller files", limits.max_bytes)));
    }
    let (text, enc) = decode(bytes)?;
    let lines = text.bytes().filter(|&c| c == b'\n' || c == b'\r').count();
    if lines > 20 * limits.max_units {
        return Err(limit_err(format!("this .strings file has more than {} lines; split it into smaller files", 20 * limits.max_units)));
    }
    if text.bytes().filter(|&c| c == b'"').count() > 8 * limits.max_units {
        return Err(limit_err(format!("this .strings file has far more strings than the limit of {} units; split it", limits.max_units)));
    }
    let mut entries = Vec::new();
    scan(&text, &mut entries).map_err(|msg| Error::Parse { format: "strings".into(), msg })?;
    Ok((text, enc, entries))
}

type Source = (Vec<Inline>, BTreeMap<String, CodeData>, Vec<(String, String)>);

fn placeholder_tag(id: &str, p: &str) -> Inline {
    if ph::is_named(p) {
        Inline::free_tag(id, p)
    } else {
        Inline::tag(id, TagKind::Standalone, p)
    }
}

fn source_inlines(text: &str) -> Source {
    let mut inl = Vec::new();
    let mut codes = BTreeMap::new();
    let mut tags = Vec::new();
    for piece in ph::split(text) {
        match piece {
            ph::Piece::Text(t) => inl.push(Inline::text(t)),
            ph::Piece::Ph(p) => {
                let id = (tags.len() + 1).to_string();
                codes.insert(id.clone(), CodeData { standalone: Some(p.to_string()), label: Some("placeholder".into()), ..Default::default() });
                inl.push(placeholder_tag(&id, p));
                tags.push((id, p.to_string()));
            }
        }
    }
    (merge_text(inl), codes, tags)
}

fn out_text(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, .. } => {
                if let Some(c) = u.codes.get(id).and_then(|c| c.standalone.as_deref()) {
                    s.push_str(c);
                }
            }
        }
    }
    s
}

fn has_target(u: &Unit) -> bool {
    u.segments.iter().any(|s| !s.target.is_empty())
}

/// One entry of the file as a key-based project sees it (translatable or not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub key: String,
    pub value: String,
    /// The comments above the entry, joined with newlines.
    pub comment: String,
    /// Index of the entry's unit among the file's translatable units; None when it has no translatable text.
    pub unit_index: Option<usize>,
    /// The same key appeared earlier.
    pub duplicate: bool,
}

/// Every entry in file order, translatable or not.
pub fn records(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    let (_, _, entries) = read(bytes, &Limits::default())?;
    let mut seen: HashSet<&str> = HashSet::new();
    let mut k = 0;
    let mut out = Vec::with_capacity(entries.len());
    for e in &entries {
        let unit_index = if ph::has_text(&e.value) {
            k += 1;
            Some(k - 1)
        } else {
            None
        };
        out.push(Record {
            key: e.key.clone(),
            value: e.value.clone(),
            comment: e.comments.join("\n"),
            unit_index,
            duplicate: !seen.insert(e.key.as_str()),
        });
    }
    Ok(out)
}

impl Strings {
    /// Like `Filter::extract` with explicit size limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        let (text, enc, entries) = read(bytes, limits)?;
        let mut units: Vec<Unit> = Vec::new();
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut cursor = 0;
        let mut counted = 0usize;
        for (n, e) in entries.iter().enumerate() {
            let translatable = ph::has_text(&e.value);
            if !translatable && !opts.include_locked {
                continue;
            }
            let (lead, core, trail) = split_ws(&e.value);
            if translatable {
                counted += 1;
                if counted > limits.max_units {
                    return Err(limit_err(format!("this .strings file has more than {} units; split it into smaller files", limits.max_units)));
                }
                if core.chars().count() > limits.max_value_chars {
                    return Err(limit_err(format!("this .strings file has a value longer than {} characters; break it into shorter values", limits.max_value_chars)));
                }
            }
            let (src, codes, _) = source_inlines(core);
            let mut u = Unit::new(format!("s{}", n + 1), src);
            u.codes = codes;
            u.context.kind = Some("string".into());
            u.context.key = Some(e.key.clone());
            u.context.location = Some(format!("line {}", e.line));
            u.context.notes = e.comments.clone();
            u.translate = translatable;
            if !translatable {
                u.lock_reason = Some("no translatable text".into());
            }
            u.codes.insert(OV.into(), hidden(core));
            u.codes.insert(RAW.into(), hidden(&text[e.vstart..e.vend]));
            if !lead.is_empty() {
                u.codes.insert(LEAD.into(), hidden(lead));
            }
            if !trail.is_empty() {
                u.codes.insert(TRAIL.into(), hidden(trail));
            }
            if !e.quoted {
                u.codes.insert(UNQ.into(), hidden("1"));
            }
            if e.vstart > cursor {
                parts.push(SkelPart::Raw(text[cursor..e.vstart].to_string()));
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(u);
            cursor = e.vend;
        }
        if cursor < text.len() {
            parts.push(SkelPart::Raw(text[cursor..].to_string()));
        }
        Ok(Document::new("strings", units, Skeleton::text(parts, enc.name(), bilingua_model::parser_info!())).with_options(opts))
    }
}

impl Filter for Strings {
    fn id(&self) -> &'static str {
        "strings"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["strings"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        const HEAD: usize = 8192;
        let head = &bytes[..bytes.len().min(HEAD)];
        let truncated = bytes.len() > HEAD;
        // A cut head may end inside a character: drop an odd byte (UTF-16) or the broken tail (UTF-8).
        let decoded = match decode(head) {
            Ok(d) => Some(d.0),
            Err(_) if truncated => {
                let even = &head[..head.len() & !1];
                match decode(even) {
                    Ok(d) => Some(d.0),
                    Err(_) => std::str::from_utf8(head).err().and_then(|e| decode(&head[..e.valid_up_to()]).ok().map(|d| d.0)),
                }
            }
            Err(_) => None,
        };
        let Some(text) = decoded else { return false };
        let mut entries = Vec::new();
        let r = scan(&text, &mut entries);
        !entries.is_empty() && (r.is_ok() || truncated)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &Limits::default())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("strings needs a text skeleton".into()));
        };
        let enc = Enc::from_name(encoding).ok_or_else(|| Error::Write(format!("unknown encoding {encoding}")))?;
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    render(u, enc, &mut out)?;
                }
            }
        }
        // LocOS keeps a strings file's own encoding (UTF-16 included): that is the default `Keep`
        Ok(bilingua_model::enc::utf8_output(&out, encoding, opts).unwrap_or_else(|| enc.encode(&out)))
    }
}

/// The unit's place in the file. An untouched value comes back exactly as it was.
fn render(u: &Unit, enc: Enc, out: &mut String) -> Result<(), Error> {
    let raw = code(u, RAW).unwrap_or_default();
    if !u.translate || !has_target(u) {
        out.push_str(&raw);
        return Ok(());
    }
    let new = out_text(u, &u.output());
    if new == code(u, OV).unwrap_or_default() {
        out.push_str(&raw);
        return Ok(());
    }
    // A NUL would make a UTF-8 file (or a BOM-less UTF-16 one) unreadable by Xcode and by this parser.
    if new.contains('\0') && !matches!(enc, Enc::Utf16LeBom | Enc::Utf16BeBom) {
        return Err(Error::Write(format!("the translation of unit {} contains a NUL character", u.id)));
    }
    let value = format!("{}{}{}", code(u, LEAD).unwrap_or_default(), new, code(u, TRAIL).unwrap_or_default());
    let body = escape(&value);
    if u.codes.contains_key(UNQ) {
        out.push('"');
        out.push_str(&body);
        out.push('"');
    } else {
        out.push_str(&body);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
