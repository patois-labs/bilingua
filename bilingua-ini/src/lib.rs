//! Bilingua INI filter: generic Windows-style INI files and Joomla language files.
//!
//! Every `key = value` entry whose value has a letter or digit outside its placeholders is a unit. Keys, sections,
//! comments, blank lines, separators and line endings stay in the skeleton byte for byte, and so does a value that
//! was not changed. A value wrapped in double quotes is written back in the quote dialect of the source (`\"` or the
//! legacy Joomla `"_QQ_"`); INI values are one line, so a line break in a translation becomes a space.
//! The unit text is plain: like Localization OS, this filter makes no inline tags.
//!
//! Encodings: UTF-8 (BOM kept), UTF-16 with a BOM, and Windows-1252 (written back in Windows-1252 when every
//! character fits, else as UTF-8).

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod enc;
mod scan;

use bilingua_model::*;
use scan::{is_translatable, split_ws, Quote};
use std::collections::HashSet;

/// The INI filter.
pub struct IniFilter;

const ID: &str = "ini";
/// The one hidden code of a unit: `standalone` = the original raw value, `open` = white space before the text,
/// `close` = white space after it, `label` = the quote dialect (absent for an unquoted value).
const HIDDEN: &str = "$ini";

/// Size limits (the Localization OS defaults). `bilingua-model` has no limits in this tree, so the crate caps itself.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Units in one file.
    pub max_units: usize,
    /// Characters in one value.
    pub max_value_chars: usize,
    /// Line breaks in one file.
    pub max_lines: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_units: 50_000, max_value_chars: 100_000, max_lines: 1_000_000 }
    }
}

/// One entry as a key-based project sees it (translatable or not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub key: String,
    pub value: String,
    /// Comment lines above the entry, joined with newlines.
    pub comment: String,
    pub section: String,
    /// Index of the entry's unit, `None` when the value is not translatable.
    pub unit_index: Option<usize>,
    /// The same key in the same section appeared earlier.
    pub duplicate: bool,
}

fn limit_err(msg: String) -> Error {
    Error::Unsupported(msg)
}

fn read(bytes: &[u8], limits: &Limits) -> Result<(String, &'static str, Vec<scan::Entry>), Error> {
    let (text, encoding) = enc::decode(bytes)?;
    let lines = text.bytes().filter(|&b| b == b'\n' || b == b'\r').count();
    if lines > limits.max_lines {
        return Err(limit_err(format!("this file has more than {} lines; split it into smaller files", limits.max_lines)));
    }
    let entries = scan::scan(&text);
    Ok((text, encoding, entries))
}

/// Every entry of the file in order, translatable or not.
pub fn records(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    let (_, _, entries) = read(bytes, &Limits::default())?;
    let dups = duplicates(&entries);
    let mut k = 0;
    let mut out = Vec::with_capacity(entries.len());
    for (e, duplicate) in entries.into_iter().zip(dups) {
        let unit_index = if is_translatable(&e.value) {
            k += 1;
            Some(k - 1)
        } else {
            None
        };
        out.push(Record { key: e.key, value: e.value, comment: e.comments.join("\n"), section: e.section.to_string(), unit_index, duplicate });
    }
    Ok(out)
}

/// For each entry: does the same key already appear earlier in the same section?
fn duplicates(entries: &[scan::Entry]) -> Vec<bool> {
    let mut seen: HashSet<(&str, &str)> = HashSet::with_capacity(entries.len());
    entries.iter().map(|e| !seen.insert((&*e.section, e.key.as_str()))).collect()
}

fn owned(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

impl IniFilter {
    /// `extract` with explicit size limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        let (text, encoding, entries) = read(bytes, limits)?;
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        let dups = duplicates(&entries);
        for (e, duplicate) in entries.into_iter().zip(dups) {
            let translatable = is_translatable(&e.value);
            if !translatable && (!opts.include_locked || e.value.trim().is_empty()) {
                continue;
            }
            let (lead, core, trail) = split_ws(&e.value);
            if core.chars().take(limits.max_value_chars + 1).count() > limits.max_value_chars {
                return Err(limit_err(format!("this file has a value longer than {} characters; break it into shorter values", limits.max_value_chars)));
            }
            if translatable && units.len() >= limits.max_units {
                return Err(limit_err(format!("this file has more than {} segments; split it into smaller files", limits.max_units)));
            }
            if e.start > pos {
                parts.push(SkelPart::Raw(text[pos..e.start].to_string()));
            }
            pos = e.end;
            let mut unit = Unit::new(format!("u{}", units.len() + 1), vec![Inline::text(core)]);
            let hidden = CodeData {
                standalone: Some(text[e.start..e.end].to_string()),
                open: owned(lead),
                close: owned(trail),
                label: owned(e.quote.code()),
            };
            unit.codes.insert(HIDDEN.into(), hidden);
            unit.translate = translatable;
            unit.context.kind = Some("key".into());
            unit.context.key = Some(e.key);
            unit.context.notes = e.comments;
            if !e.section.is_empty() {
                unit.context.extra.insert("section".into(), e.section.to_string());
            }
            if duplicate {
                unit.context.extra.insert("duplicate".into(), "true".into());
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        Ok(Document::new(ID, units, Skeleton::text(parts, encoding, bilingua_model::parser_info!())).with_options(opts))
    }
}

impl Filter for IniFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ini"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(4096)];
        let body = head.strip_prefix(enc::BOM8).unwrap_or(head);
        // A cut in the middle of a character is fine.
        let text = match std::str::from_utf8(body) {
            Ok(t) => t,
            Err(e) if e.error_len().is_none() => std::str::from_utf8(&body[..e.valid_up_to()]).unwrap_or(""),
            Err(_) => return false,
        };
        if text.contains('\0') {
            return false;
        }
        let (mut section, mut pairs, mut other) = (false, 0usize, 0usize);
        for line in text.lines().map(scan::trim).filter(|l| !l.is_empty() && !l.starts_with([';', '#'])) {
            if line.starts_with('[') && line.ends_with(']') && line.len() > 2 {
                section = true;
            } else if line.find('=').is_some_and(|i| i > 0 && !scan::trim(&line[..i]).contains(bilingua_model::py_isspace_char)) {
                pairs += 1;
            } else {
                other += 1;
            }
        }
        (section || pairs > 0) && other <= pairs.max(1) / 4
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &Limits::default())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("ini needs a text skeleton".into()));
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
        match bilingua_model::enc::utf8_output(&out, encoding, opts) {
            Some(b) => Ok(b),
            None => enc::encode(&out, encoding),
        }
    }
}

/// The raw value to write for a unit: the original spelling when nothing changed, else the translation in the
/// source's quote dialect.
fn render(unit: &Unit) -> String {
    let Some(h) = unit.codes.get(HIDDEN) else { return String::new() };
    let raw = h.standalone.as_deref().unwrap_or("");
    if !unit.translate || unit.segments.iter().all(|s| s.target.is_empty()) {
        return raw.to_string();
    }
    let text = plain_text(&unit.output());
    if text == plain_text(&unit.source()) {
        return raw.to_string();
    }
    let quote = Quote::from_code(h.label.as_deref().unwrap_or(""));
    scan::encode_value(&format!("{}{}{}", h.open.as_deref().unwrap_or(""), text, h.close.as_deref().unwrap_or("")), quote)
}
