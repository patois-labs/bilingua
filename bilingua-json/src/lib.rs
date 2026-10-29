//! Bilingua JSON filter.
//!
//! Every string value is a unit (`context.key` is its JSON pointer); keys are never translated. The file is not
//! re-serialised: the skeleton holds every byte between string values, and only the string token of a
//! translated value is rewritten. A value that was not translated is written back as its original token, so
//! `\/` and `é` spellings survive. Placeholders (`{name}`, `{{count}}`, `%s`, `%1$d`, `${x}`) and HTML tags
//! inside a string become inline tags. Values that are not prose (URLs, ids, numbers, ...) stay in the skeleton.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod inline;
mod scan;

use bilingua_model::*;

pub struct JsonFilter;

const ID: &str = "json";
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
/// Key in `Unit::codes` that holds the original string token (a hidden code, not a tag id).
const RAW_KEY: &str = "$raw";

impl Filter for JsonFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["json"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
        let Ok(text) = std::str::from_utf8(body) else { return false };
        matches!(text.trim_start().as_bytes().first(), Some(b'{' | b'[')) && scan::scan(text).is_ok()
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
            return Err(Error::Unsupported("UTF-16 JSON files; save the file as UTF-8".into()));
        }
        let (body, encoding) = match bytes.strip_prefix(BOM) {
            Some(rest) => (rest, "utf-8-bom"),
            None => (bytes, "utf-8"),
        };
        let text = std::str::from_utf8(body)
            .map_err(|e| Error::Parse { format: ID.into(), msg: format!("the file is not valid UTF-8 (at byte {})", e.valid_up_to()) })?;
        let vals = scan::scan(text)?;

        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        for v in vals {
            let token = &text[v.start..v.end];
            let Some(value) = scan::unescape(&token[1..token.len() - 1]) else { continue };
            let (inl, codes) = inline::tag_placeholders(&value);
            let skip = inline::looks_untranslatable(&value, &inl);
            if skip && (!opts.include_locked || value.trim().is_empty()) {
                continue;
            }
            if v.start > pos {
                parts.push(SkelPart::Raw(text[pos..v.start].to_string()));
            }
            pos = v.end;
            let mut unit = Unit::new(format!("u{}", units.len() + 1), inl);
            unit.codes = codes;
            unit.codes.insert(RAW_KEY.into(), CodeData { standalone: Some(token.to_string()), ..Default::default() });
            unit.context.kind = Some("string".into());
            unit.context.key = Some(v.pointer);
            unit.translate = !skip;
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
            skeleton: Skeleton::text(parts, encoding, bilingua_model::parser_info!()),
        })
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("json needs a text skeleton".into()));
        };
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    out.push_str(&render_token(unit));
                }
            }
        }
        let mut bytes = Vec::with_capacity(out.len() + 3);
        if encoding == "utf-8-bom" {
            bytes.extend_from_slice(BOM);
        }
        bytes.extend_from_slice(out.as_bytes());
        Ok(bytes)
    }
}

/// The string token to write for a unit.
fn render_token(unit: &Unit) -> String {
    let untouched = unit.segments.iter().all(|s| s.target.is_empty());
    if untouched {
        if let Some(raw) = unit.codes.get(RAW_KEY).and_then(|c| c.standalone.as_deref()) {
            return raw.to_string();
        }
    }
    let mut text = String::new();
    for i in unit.output() {
        match i {
            Inline::Text(t) => text.push_str(&t),
            Inline::Tag { id, kind, .. } => {
                let code: Option<&CodeData> = unit.codes.get(&id);
                let piece = code.and_then(|c| match kind {
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
    scan::quote(&text)
}

/// Exposed for tests and tools: the JSON pointers of all units, in order.
pub fn pointers(doc: &Document) -> Vec<String> {
    doc.units.iter().filter_map(|u| u.context.key.clone()).collect()
}

#[cfg(test)]
mod tests;
