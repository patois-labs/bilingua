//! Bilingua JSON5 and JSONC filter (`.json5`, `.jsonc`).
//!
//! JSON with comments and trailing commas (JSONC, as VS Code reads it) and JSON5 1.0.0 (unquoted and single-quoted
//! keys, single-quoted strings, hex numbers, `Infinity` and `NaN`, a leading `+`, `\` line continuations). One
//! scanner reads both. Every string VALUE with at least one letter or digit is a unit (the Localization OS rule);
//! keys, numbers, booleans and `null` never are. `context.key` is the dotted path (`menu.items[0].label`) and the
//! comment block directly above the key goes to `context.notes`.
//!
//! The file is not re-serialised. The skeleton holds every byte between string values: comments, keys, commas,
//! number spellings, whitespace, line endings and the BOM. A translated value is written back into its own span in
//! the quote style it had, escaping only what that style needs; an untranslated (or unchanged) value keeps its
//! original spelling, escapes included. Placeholders (`{name}`, `{{user}}`, `%1$s`, `${x}`) and HTML tags inside
//! a string become inline tags.
//!
//! Encodings: UTF-8 (with or without BOM), UTF-16 with BOM, and, as Localization OS reads them, the Windows code
//! page of the source language (Windows-1252 by default). The output keeps the encoding when every character fits.
//!
//! Limits (see [`Limits`]): nesting 256, 500 000 strings, 100 000 characters per string, 256 MiB. Malformed,
//! truncated, deep and oversized files return [`Error::Parse`] or [`Error::Unsupported`]; nothing panics.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod encoding;
mod inline;
mod scan;

pub use scan::Limits;

use bilingua_model::*;

const ID: &str = "json5";
/// Key in `Unit::codes` that holds the original string token (a hidden code, not a tag id).
const RAW_KEY: &str = "$raw";

/// The JSON5 / JSONC filter.
#[derive(Debug, Clone, Default)]
pub struct Json5Filter {
    pub limits: Limits,
}

impl Json5Filter {
    pub fn with_limits(limits: Limits) -> Json5Filter {
        Json5Filter { limits }
    }
}

impl Filter for Json5Filter {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["json5", "jsonc"]
    }

    /// Plain `.json` belongs to `bilingua-json`; a JSON5 file is recognised by its extension.
    fn sniff(&self, _bytes: &[u8]) -> bool {
        false
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        if bytes.len() > self.limits.max_bytes {
            return Err(Error::Parse { format: ID.into(), msg: format!("the file is larger than {} bytes", self.limits.max_bytes) });
        }
        let (text, enc) = encoding::decode(bytes, opts.source_lang.as_deref())?;
        let vals = scan::scan(&text, &self.limits)?;

        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        for v in vals {
            if !v.translatable && (!opts.include_locked || v.value.trim().is_empty()) {
                continue;
            }
            let (inl, codes) = inline::tag_placeholders(&v.value);
            if v.start > pos {
                parts.push(SkelPart::Raw(text[pos..v.start].to_string()));
            }
            pos = v.end;
            let mut unit = Unit::new(format!("u{}", units.len() + 1), inl);
            unit.codes = codes;
            unit.codes.insert(RAW_KEY.into(), CodeData { standalone: Some(text[v.start..v.end].to_string()), ..Default::default() });
            unit.context.kind = Some("string".into());
            if !v.key.is_empty() {
                unit.context.key = Some(v.key);
            }
            if !v.comment.is_empty() {
                unit.context.notes.push(v.comment);
            }
            unit.translate = v.translatable;
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
            skeleton: Skeleton::text(parts, enc, bilingua_model::parser_info!()),
        })
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("json5 needs a text skeleton".into()));
        };
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    out.push_str(&render_token(unit)?);
                }
            }
        }
        Ok(encoding::encode(&out, encoding))
    }
}

/// The text of inlines with every tag replaced by its original markup.
fn expand(unit: &Unit, inl: &[Inline]) -> String {
    let mut text = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => text.push_str(t),
            Inline::Tag { id, kind, .. } => {
                let piece = unit.codes.get(id).and_then(|c| match kind {
                    TagKind::Open => c.open.as_deref(),
                    TagKind::Close => c.close.as_deref(),
                    TagKind::Standalone => c.standalone.as_deref(),
                });
                if let Some(p) = piece {
                    text.push_str(p);
                }
            }
        }
    }
    text
}

/// The string token to write for a unit: the original spelling when the text is unchanged, else the new text in
/// the original quote style.
fn render_token(unit: &Unit) -> Result<String, Error> {
    let raw = unit
        .codes
        .get(RAW_KEY)
        .and_then(|c| c.standalone.as_deref())
        .ok_or_else(|| Error::Write(format!("unit {} lost its original string", unit.id)))?;
    if unit.segments.iter().all(|s| s.target.is_empty()) {
        return Ok(raw.to_string());
    }
    let new = expand(unit, &unit.output());
    if new == expand(unit, &unit.source()) {
        return Ok(raw.to_string());
    }
    let q = match raw.as_bytes().first() {
        Some(&q @ (b'"' | b'\'')) => q,
        _ => b'"',
    };
    Ok(scan::quote(&new, q))
}

/// Exposed for tests and tools: the dotted paths of all units, in order.
pub fn keys(doc: &Document) -> Vec<String> {
    doc.units.iter().map(|u| u.context.key.clone().unwrap_or_default()).collect()
}

#[cfg(test)]
mod tests;
