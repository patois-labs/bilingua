//! Bilingua STF filter: Salesforce Translation Workbench `.stf` files (Source, Outdated and untranslated, Bilingual).
//!
//! An STF file is tab-separated text, one translatable item per line (`KEY<TAB>LABEL`, and in the TRANSLATED
//! section of a Bilingual file `KEY<TAB>LABEL<TAB>TRANSLATION<TAB>OUT OF DATE`). Every data row whose LABEL has text
//! outside its placeholders is a unit, in file order. The KEY is never a unit and is never changed.
//!
//! On merge a translation goes into the TRANSLATION column when the section has one, else in place of the LABEL, as
//! the Workbench import expects. A unit whose text did not change is not touched (an existing reviewed translation
//! stays). Every byte outside a changed cell is identical, and an unchanged file comes back byte for byte.
//!
//! Placeholders are inline tags (standalone codes, label `placeholder`, display = the source text), the LocOS `x` token, because
//! Localization OS keeps them as point tokens: the escapes `\t \n \r \\`, `{0}` style arguments and `{!merge.fields}`.
//! A raw tab, line feed or carriage return in a translation is written back escaped, so a row can never split.
//!
//! With a target language and at least one translation (or `MergeOptions::stamp_target_always`) the `Language code:`
//! header is set to the target in Salesforce's code form (`de`, `es_MX`, `zh_CN`, `iw`, ...).
//!
//! Salesforce's own label limits for the common key types are reported as `context.max_len` and by [`over_limit`];
//! nothing is truncated.
//!
//! Encodings: UTF-8 (BOM kept), UTF-16 with a BOM, and the legacy code page of the file (default Windows-1252); a
//! merge keeps the file's encoding unless `MergeOptions::output_encoding` asks for UTF-8 (Localization OS does).

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod scan;

use bilingua_model::enc::{self, DecodeOptions};
use bilingua_model::*;
use scan::Walker;

pub use bilingua_model::Limits;

/// The STF filter.
pub struct StfFilter;

const ID: &str = "stf";
/// The one hidden code of a unit: `standalone` = the raw cell the unit's translation replaces, `open` = tabs that
/// must be added first when the row has no TRANSLATION cell yet.
const HIDDEN: &str = "$stf";

/// Salesforce label limits by key type, never enforced by truncation.
pub fn max_length_for_key(key: &str) -> Option<usize> {
    if let Some(r) = key.strip_prefix("CustomField.") {
        if r.ends_with(".FieldLabel") {
            return Some(40);
        }
        if r.ends_with(".RelationshipLabel") {
            return Some(80);
        }
        if r.ends_with(".HelpText") {
            return Some(510);
        }
    }
    if key.starts_with("CustomLabel.") {
        return Some(1000);
    }
    if key.starts_with("PicklistValue.") {
        return Some(255);
    }
    if key.strip_prefix("ValidationFormula.").is_some_and(|r| r.ends_with(".ErrorMessage")) {
        return Some(255);
    }
    if key.strip_prefix("RecordType.").is_some_and(|r| r.ends_with(".Name")) {
        return Some(80);
    }
    None
}

/// BCP 47 tag (`de-DE`, `zh-Hant`, `pt-BR`) to the Salesforce Translation Workbench code.
pub fn salesforce_language_code(locale: &str) -> String {
    let parts: Vec<&str> = py_strip(locale).split(['-', '_']).filter(|p| !p.is_empty()).collect();
    let Some(first) = parts.first() else { return String::new() };
    let lang = first.to_lowercase();
    let script = parts[1..].iter().find(|p| p.chars().count() == 4 && py_isalpha(p));
    let region = parts[1..].iter().find(|p| (2..=3).contains(&p.chars().count()) && py_isalnum(p));
    let mapped = |k: &str| -> Option<&'static str> {
        Some(match k {
            "he" => "iw",
            "id" => "in",
            "yi" => "ji",
            "nb" | "nn" => "no",
            "nl" => "nl_NL",
            "zh" | "zh_hans" | "zh_sg" => "zh_CN",
            "zh_hant" | "zh_mo" => "zh_TW",
            _ => return None,
        })
    };
    if lang == "zh" {
        if let Some(s) = script {
            if let Some(m) = mapped(&format!("zh_{}", s.to_lowercase())) {
                return m.to_string();
            }
        }
    }
    if let Some(r) = region {
        if let Some(m) = mapped(&format!("{}_{}", lang, r.to_lowercase())) {
            return m.to_string();
        }
        let base = mapped(&lang).unwrap_or(&lang);
        let base = base.split('_').next().unwrap_or(base);
        return format!("{}_{}", base, r.to_uppercase());
    }
    mapped(&lang).map_or(lang, str::to_string)
}

/// One row as a key-based project sees it (unit or not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub key: String,
    /// The source LABEL.
    pub value: String,
    /// The TRANSLATION cell (Bilingual TRANSLATED section only).
    pub translation: Option<String>,
    /// The OUT OF DATE cell is `*`.
    pub out_of_date: bool,
    pub section: String,
    pub max_length: Option<usize>,
}

/// A translation longer than Salesforce allows for its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverLimit {
    pub unit_index: usize,
    pub key: String,
    pub length: usize,
    pub max_length: usize,
}

fn decode(bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<enc::Decoded, Error> {
    let o = DecodeOptions { declared: false, source_lang: opts.source_lang.as_deref(), limits: *limits };
    let dec = enc::decode_text(bytes, &o)?;
    if dec.text.contains('\0') {
        return Err(Error::Encoding(
            "This .stf file contains NUL bytes, so it is not a Translation Workbench text export. Re-export it from Translation Workbench or re-save it as UTF-8."
                .into(),
        ));
    }
    Ok(dec)
}

/// Where the walk starts: a second byte-order mark inside a UTF-16 file is skipped, as Localization OS does.
fn lead(text: &str, encoding: &str) -> usize {
    if text.starts_with('\u{feff}') && enc::base(encoding).starts_with("utf-16") {
        3
    } else {
        0
    }
}

/// Every row of the file in order, unit or not.
pub fn records(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    let opts = ExtractOptions::default();
    let dec = decode(bytes, &opts, &opts.limits())?;
    let mut w = Walker::new(&dec.text, lead(&dec.text, &dec.encoding));
    let mut out = Vec::new();
    while let Some(step) = w.step() {
        let Some(r) = step else { continue };
        let own = r.dst != r.src;
        out.push(Record {
            key: r.key.to_string(),
            value: r.cells[r.src].to_string(),
            translation: own.then(|| r.cells.get(r.dst).map(|c| c.to_string())).flatten(),
            out_of_date: own && r.cells.get(r.dst + 1).is_some_and(|c| py_strip(c) == "*"),
            section: r.section.to_string(),
            max_length: max_length_for_key(r.key),
        });
    }
    Ok(out)
}

fn escape(s: &str, out: &mut String) {
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\r' => {
                if it.peek() == Some(&'\n') {
                    it.next();
                    out.push_str("\\n");
                } else {
                    out.push_str("\\r");
                }
            }
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
}

/// A translation as a cell: text escaped, placeholders as they were; unknown tags are dropped.
fn render_inlines(unit: &Unit, inl: &[Inline]) -> String {
    let mut s = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => escape(t, &mut s),
            Inline::Tag { id, .. } => {
                if let Some(raw) = unit.codes.get(id).and_then(|c| c.standalone.as_deref()) {
                    s.push_str(raw);
                }
            }
        }
    }
    s
}

/// Text with a tag as the Localization OS point token, to tell whether a translation differs from the source.
fn canon(inl: &[Inline]) -> String {
    let mut s = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, .. } => {
                s.push_str("\u{27E6}x");
                s.push_str(id);
                s.push('\u{27E7}');
            }
        }
    }
    s
}

fn has_target(u: &Unit) -> bool {
    u.translate && u.segments.iter().any(|s| !s.target.is_empty())
}

/// The cell text to write for a unit.
fn render(unit: &Unit) -> String {
    let Some(h) = unit.codes.get(HIDDEN) else { return String::new() };
    let raw = h.standalone.as_deref().unwrap_or("");
    if !has_target(unit) {
        return raw.to_string();
    }
    let out = unit.output();
    if canon(&out) == canon(&unit.source()) {
        return raw.to_string();
    }
    format!("{}{}", h.open.as_deref().unwrap_or(""), render_inlines(unit, &out))
}

/// Translations longer than the Salesforce limit of their key (the rendered cell, escapes and placeholders counted).
pub fn over_limit(doc: &Document) -> Vec<OverLimit> {
    let mut out = Vec::new();
    for (i, u) in doc.units.iter().enumerate() {
        let (Some(max), Some(key)) = (u.context.max_len, u.context.key.as_ref()) else { continue };
        if !has_target(u) {
            continue;
        }
        let n = render_inlines(u, &u.output()).chars().count();
        if n > max {
            out.push(OverLimit { unit_index: i, key: key.clone(), length: n, max_length: max });
        }
    }
    out
}

impl StfFilter {
    /// `extract` with explicit size limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        let dec = decode(bytes, opts, limits)?;
        let text = dec.text.as_str();
        let mut w = Walker::new(text, lead(text, &dec.encoding));
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        while let Some(step) = w.step() {
            let Some(r) = step else { continue };
            if !r.has_text {
                continue;
            }
            guard::check_unit_count(units.len() + 1, limits)?;
            let label = r.cells[r.src];
            let mut inl: Vec<Inline> = Vec::new();
            let mut codes = std::collections::BTreeMap::new();
            let (mut at, mut chars) = (0usize, 0usize);
            for (n, (s, e)) in scan::token_spans(label).into_iter().enumerate() {
                if s > at {
                    chars += label[at..s].chars().count();
                    inl.push(Inline::text(&label[at..s]));
                }
                let id = (n + 1).to_string();
                chars += 1;
                codes.insert(id.clone(), CodeData { standalone: Some(label[s..e].to_string()), label: Some("placeholder".into()), ..Default::default() });
                inl.push(Inline::tag(id, TagKind::Standalone, &label[s..e]));
                at = e;
            }
            if at < label.len() {
                chars += label[at..].chars().count();
                inl.push(Inline::text(&label[at..]));
            }
            guard::check_unit_chars(chars, limits)?;
            let (a, b) = r.cell_span(r.dst);
            if a < pos {
                return Err(Error::Parse { format: ID.into(), msg: "overlapping cells".into() });
            }
            if a > pos {
                parts.push(SkelPart::Raw(text[pos..a].to_string()));
            }
            pos = b;
            let mut unit = Unit::new(format!("u{}", units.len() + 1), inl);
            let pad = if r.dst >= r.cells.len() { "\t".repeat(r.dst - r.cells.len() + 1) } else { String::new() };
            codes.insert(HIDDEN.into(), CodeData { standalone: Some(text[a..b].to_string()), open: (!pad.is_empty()).then_some(pad), ..Default::default() });
            unit.codes = codes;
            unit.context.kind = Some("key".into());
            unit.context.key = Some(r.key.to_string());
            unit.context.style = Some(format!("stf:{}", r.section));
            unit.context.max_len = max_length_for_key(r.key);
            if r.dst != r.src {
                if let Some(t) = r.cells.get(r.dst).filter(|t| !t.is_empty()) {
                    unit.context.extra.insert("translation".into(), t.to_string());
                    if r.cells.get(r.dst + 1).is_some_and(|c| py_strip(c) == "*") {
                        unit.context.extra.insert("out_of_date".into(), "true".into());
                    }
                }
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let doc = Document {
            format: ID.into(),
            name: None,
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(parts, dec.encoding, bilingua_model::parser_info!()),
            ..Default::default()
        };
        guard::finish(doc, dec.warnings, limits)
    }
}

fn merge(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write("stf needs a text skeleton".into()));
    };
    let mut out = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => out.push_str(s),
            SkelPart::Unit(i) => {
                let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                out.push_str(&render(unit));
            }
        }
    }
    if opts.should_stamp(doc) {
        let code = doc.target_lang.as_deref().map(salesforce_language_code).unwrap_or_default();
        if !code.is_empty() {
            if let Some(h) = scan::find_header(&out, lead(&out, encoding), "language code") {
                if h.value != code {
                    out.replace_range(h.start..h.end, &code);
                }
            }
        }
    }
    enc::encode_text_with(&out, encoding, opts)
}

impl Filter for StfFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["stf"]
    }

    /// Strict: a `Language code:` header plus either a `# KEY<TAB>...` column line or a `Type:` header, in the first
    /// 8 KB. Safe anywhere in the facade list.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(8192)];
        let text: String = if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
            let be = head[0] == 0xFE;
            let units = head[2..].chunks_exact(2).map(|p| if be { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) });
            char::decode_utf16(units).map(|r| r.unwrap_or('\u{fffd}')).collect()
        } else {
            String::from_utf8_lossy(head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head)).into_owned()
        };
        if text.contains('\0') {
            return false;
        }
        let (mut lang, mut other) = (false, false);
        for line in text.lines() {
            if line.starts_with('#') {
                other |= scan::parse_columns(line).is_some_and(|c| c.iter().any(|n| n == "LABEL"));
            } else if let Some((name, _, _)) = scan::parse_header(line) {
                match name.as_str() {
                    "language code" => lang = true,
                    "type" => other = true,
                    _ => {}
                }
            }
        }
        lang && other
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &opts.limits())
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        merge(doc, &MergeOptions::default())
    }

    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        merge(doc, opts)
    }
}
