//! Bilingua JSON filter.
//!
//! Every string value is a unit (`context.key` is its JSON pointer); keys are never translated. The file is not
//! re-serialised: the skeleton holds every byte between string values, and only the string token of a
//! translated value is rewritten. A value that was not translated is written back as its original token, so
//! `\/` and `é` spellings survive. Placeholders (`{name}`, `{{count}}`, `%s`, `%1$d`, `${x}`) and HTML tags
//! inside a string become inline tags. Values that are not prose (URLs, ids, numbers, ...) stay in the skeleton.
//!
//! Resource dialects (option `json.dialect`, on by default under the LocOS profile): a file shaped as Chrome
//! `messages.json`, FormatJS, Transifex Structured JSON, go-i18n or ARB gives only its message strings as units
//! (ICU plural and select branches one unit each), and a merge for a target language makes the plurals follow the
//! target's CLDR categories. Generic JSON gets i18next plural keys filled for the target (`json.i18next_plurals`).

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod cldr;
mod dialect;
mod i18next;
mod icu;
mod inline;
mod plan;
mod rebuild;
mod scan;
mod tree;

pub use cldr::{categories as plural_categories, BABEL_VERSION};

use bilingua_model::*;
use dialect::Dialect;

pub struct JsonFilter;

const ID: &str = "json";
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
/// Key in `Unit::codes` that holds the original string token (a hidden code, not a tag id).
const RAW_KEY: &str = "$raw";
/// `UnitContext::extra` key naming the dialect a unit's file was read as (units of a dialect file only).
const DIALECT_KEY: &str = "json.dialect";

/// The `format_options` keys of the JSON filter.
pub const FORMAT_OPTIONS: &[bilingua_model::FormatOptionKey] = &[
    bilingua_model::FormatOptionKey::text(
        "json.dialect",
        "At extract: plain (every string is a unit; the default), auto (read the file's dialect from its shape; the default under the LocOS profile), or chrome, formatjs, structured, go, arb (the file must have that shape). A merge follows the dialect the document was read with and ignores it.",
    ),
    bilingua_model::FormatOptionKey::bool(
        "json.i18next_plurals",
        "At merge for a target language: add the CLDR plural keys the target needs to i18next-style groups (item_one, item_other) of generic JSON, holding the _other value. Default off; on under the LocOS profile, which also writes the file the way LocOS does (re-indented, two spaces).",
    ),
];

/// What `json.dialect` asks for at extract.
enum Want {
    Plain,
    Auto,
    Named(Dialect),
}

const DIALECT_WORDS: &str = "plain, auto, chrome, formatjs, structured, go or arb";

fn want_dialect(opts: &ExtractOptions, locos: bool) -> Result<Want, Error> {
    match opts.format_option("json.dialect").map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(if locos { Want::Auto } else { Want::Plain }),
        Some("plain") => Ok(Want::Plain),
        Some("auto") => Ok(Want::Auto),
        Some(n) => Dialect::parse(n)
            .map(Want::Named)
            .ok_or_else(|| Error::FormatOption { key: "json.dialect".into(), msg: format!("{n:?} is not {DIALECT_WORDS}") }),
    }
}

/// A merge accepts the same words and ignores them (the document knows its dialect).
fn check_merge_dialect(opts: &bilingua_model::MergeOptions) -> Result<(), Error> {
    match opts.format_options.get("json.dialect").map(|v| v.trim()).filter(|v| !v.is_empty()) {
        None | Some("plain" | "auto") => Ok(()),
        Some(n) if Dialect::parse(n).is_some() => Ok(()),
        Some(n) => Err(Error::FormatOption { key: "json.dialect".into(), msg: format!("{n:?} is not {DIALECT_WORDS}") }),
    }
}

impl Filter for JsonFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn format_option_keys(&self) -> &'static [bilingua_model::FormatOptionKey] {
        FORMAT_OPTIONS
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
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let mut dec = bilingua_model::enc::decode_text(bytes, &bilingua_model::enc::DecodeOptions::new(opts))?;
        let text_owned = std::mem::take(&mut dec.text);
        let text = text_owned.as_str();
        let encoding = dec.encoding.as_str();
        let locos = opts.is_profile(profile::LOCOS);
        let vals = scan::scan_with(text, locos)?;
        let want = want_dialect(opts, locos)?;
        if !matches!(want, Want::Plain) {
            let found = tree::scan(text).ok().and_then(|root| dialect::detect(&root).map(|d| (root, d)));
            match (want, found) {
                (Want::Named(n), Some((_, d))) if n != d => {
                    return Err(Error::FormatOption { key: "json.dialect".into(), msg: format!("the file is a {} file, not {}", d.name(), n.name()) });
                }
                (Want::Named(n), None) => {
                    return Err(Error::FormatOption { key: "json.dialect".into(), msg: format!("the file does not have the shape of a {} file", n.name()) });
                }
                (_, Some((root, d))) => {
                    let doc = extract_dialect(text, encoding, &root, d, opts, locos)?;
                    return bilingua_model::guard::finish(doc, std::mem::take(&mut dec.warnings), &opts.limits());
                }
                _ => {}
            }
        }
        // LocOS walks `json.loads`'s dict: the last of a duplicate key at the first one's place, every string
        // with a letter or digit (placeholder-only and URL values included); p_index is that walk's order
        let mut order: Vec<usize> = (0..vals.len()).filter(|&k| !vals[k].dead).collect();
        order.sort_by(|&a, &b| vals[a].ord.cmp(&vals[b].ord));
        let mut walk_at = vec![usize::MAX; vals.len()];
        let mut live = 0usize;
        for k in order {
            let (a, b) = (vals[k].start, vals[k].end);
            let alnum = scan::unescape(&text[a + 1..b - 1]).is_some_and(|v| py_has_alnum(&v));
            if alnum {
                walk_at[k] = live;
                live += 1;
            }
        }

        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        for (k, v) in vals.into_iter().enumerate() {
            let token = &text[v.start..v.end];
            let Some(value) = scan::unescape(&token[1..token.len() - 1]) else { continue };
            let (inl, codes) = inline::tag_placeholders(&value);
            let skip = if locos { walk_at[k] == usize::MAX } else { inline::looks_untranslatable(&value, &inl) };
            if skip && (locos || !opts.include_locked || value.trim().is_empty()) {
                continue;
            }
            if v.start > pos {
                parts.push(SkelPart::Raw(text[pos..v.start].to_string()));
            }
            pos = v.end;
            let mut unit = Unit::new(format!("u{}", units.len() + 1), inl);
            unit.codes = codes;
            unit.codes.insert(RAW_KEY.into(), CodeData::new().with_standalone(token.to_string()));
            unit.context.kind = Some("string".into());
            unit.context.key = Some(v.pointer);
            unit.translate = !skip;
            if locos {
                unit.context.extra.insert(profile::P_INDEX.into(), walk_at[k].to_string());
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        if locos {
            // LocOS lists its units in walk order: a key repeated later stands at its first place
            in_walk_order(&mut units, &mut parts);
        }

        bilingua_model::guard::finish(Document::new(ID, units, Skeleton::text(parts, encoding, bilingua_model::parser_info!())).with_options(opts), std::mem::take(&mut dec.warnings), &opts.limits())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        check_merge_dialect(opts)?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("json needs a text skeleton".into()));
        };
        let locos = opts.is_profile(profile::LOCOS);
        let translated = doc.units.iter().any(|u| u.segments.iter().any(|s| s.has_target()));
        let target = opts.stamp_lang(doc, translated);
        // the original text: the skeleton with every string token as it was
        let mut orig = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => orig.push_str(s),
                SkelPart::Unit(i) => {
                    let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    // a unit without its original token (built by hand) stands for its source, as before
                    match unit.codes.get(RAW_KEY).and_then(|c| c.standalone.as_deref()) {
                        Some(raw) => orig.push_str(raw),
                        None => orig.push_str(&render_inlines(unit, unit.source())),
                    }
                }
            }
        }
        if let Some(d) = doc_dialect(doc, &orig, locos) {
            let out = if !translated && target.is_none() { orig } else { rebuild::splice(&orig, doc, d, target, locos)? };
            // LocOS splices `
` lines (a new `@@locale`, plural keys) into a CRLF file as they are: keep the text's breaks
            let enc = if locos { bilingua_model::enc::keep_newlines(encoding) } else { encoding.to_string() };
            return bilingua_model::enc::encode_text_with(&out, &enc, opts);
        }
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => out.push_str(&render_token(&doc.units[*i], locos)),
            }
        }
        let fill = opts.format_options.get("json.i18next_plurals").and_then(|v| match v.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        });
        if let (true, Some(t)) = (fill.unwrap_or(locos), target) {
            if let Some(filled) = i18next::fill(&out, t, locos, &orig) {
                out = filled;
            }
        }
        bilingua_model::enc::encode_text_with(&out, encoding, opts)
    }
}

/// The dialect a document was read as: the one its units name; a document without units under the LocOS profile
/// is read again by shape.
fn doc_dialect(doc: &Document, orig: &str, locos: bool) -> Option<Dialect> {
    if let Some(n) = doc.units.iter().find_map(|u| u.context.extra.get(DIALECT_KEY)) {
        return Dialect::parse(n);
    }
    if doc.units.is_empty() && locos {
        return tree::scan(orig).ok().and_then(|r| dialect::detect(&r));
    }
    None
}

/// The document of a resource-dialect file: its message strings, ICU branches as separate units.
fn extract_dialect(text: &str, encoding: &str, root: &tree::Node, d: Dialect, opts: &ExtractOptions, locos: bool) -> Result<Document, Error> {
    let mut parts: Vec<SkelPart> = Vec::new();
    let mut units: Vec<Unit> = Vec::new();
    let mut pos = 0usize;
    for m in dialect::messages(root, d) {
        let plan = plan::plan_message(m.value, m.syntax);
        if plan.units.is_empty() {
            continue;
        }
        if m.start > pos {
            parts.push(SkelPart::Raw(text[pos..m.start].to_string()));
        }
        pos = m.end;
        parts.push(SkelPart::Unit(units.len()));
        for (k, up) in plan.units.into_iter().enumerate() {
            let mut unit = Unit::new(format!("u{}", units.len() + 1), up.inlines);
            unit.codes = up.codes;
            if k == 0 {
                unit.codes.insert(RAW_KEY.into(), CodeData::new().with_standalone(text[m.start..m.end].to_string()));
            }
            unit.context.kind = Some("message".into());
            unit.context.key = Some(m.pointer.clone());
            unit.context.notes = m.notes.clone();
            if !up.path.is_empty() {
                unit.context.extra.insert("icu".into(), up.path);
            }
            unit.context.extra.insert(DIALECT_KEY.into(), d.name().into());
            if locos {
                unit.context.extra.insert(profile::P_INDEX.into(), units.len().to_string());
            }
            units.push(unit);
        }
    }
    if pos < text.len() {
        parts.push(SkelPart::Raw(text[pos..].to_string()));
    }
    Ok(Document::new(ID, units, Skeleton::text(parts, encoding, bilingua_model::parser_info!())).with_options(opts))
}

/// Sorts the units by their `locos.p_index` (stable), renumbers their ids and points the skeleton at the new
/// places.
fn in_walk_order(units: &mut Vec<Unit>, parts: &mut [SkelPart]) {
    let walk = |u: &Unit| u.context.extra.get(profile::P_INDEX).and_then(|p| p.parse::<usize>().ok()).unwrap_or(usize::MAX);
    let mut order: Vec<usize> = (0..units.len()).collect();
    order.sort_by_key(|&i| walk(&units[i]));
    if order.iter().enumerate().all(|(k, &i)| k == i) {
        return;
    }
    let mut new_at = vec![0usize; units.len()];
    for (k, &i) in order.iter().enumerate() {
        new_at[i] = k;
    }
    for p in parts.iter_mut() {
        if let SkelPart::Unit(i) = p {
            *i = new_at[*i];
        }
    }
    let mut old: Vec<Option<Unit>> = std::mem::take(units).into_iter().map(Some).collect();
    for (k, &i) in order.iter().enumerate() {
        let mut u = old[i].take().expect("each unit once");
        let was = std::mem::replace(&mut u.id, format!("u{}", k + 1));
        for s in u.segments.iter_mut().filter(|s| s.id == was) {
            s.id = u.id.clone();
        }
        units.push(u);
    }
}

/// The string token to write for a unit. Under the LocOS profile a translation equal to the source keeps the
/// original token too, as LocOS's splice does.
fn render_token(unit: &Unit, locos: bool) -> String {
    let untouched = unit.segments.iter().all(|s| !s.has_target()) || (locos && unit.output() == unit.source());
    if untouched {
        if let Some(raw) = unit.codes.get(RAW_KEY).and_then(|c| c.standalone.as_deref()) {
            return raw.to_string();
        }
    }
    render_inlines(unit, unit.output())
}

/// The JSON string token of `inl`, the unit's tags written back as their codes.
fn render_inlines(unit: &Unit, inl: Vec<Inline>) -> String {
    let mut text = String::new();
    for i in inl {
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
