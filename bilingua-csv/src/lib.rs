//! Bilingua CSV and TSV filter. A port of the Localization OS tabular filter (the baseline for this format; Okapi's
//! `okf_table_csv` is the second reference).
//!
//! * The file is read record by record (RFC 4180 with quote doubling, the way Python's `csv.reader` reads it) and
//!   the raw text of every record is kept. A record that no unit writes into stays byte for byte; in a changed
//!   record only the written cell changes, and every other field keeps its exact quoting. Delimiter, quote
//!   doubling and each record's line ending are kept.
//! * Which cells are units follows the column mapping ([`plan`]): a spec such as
//!   `source=English, target.de=German, key=id` or, without one, the header row (`source`/`text` or the source
//!   locale, `target`/`translation` or a target locale). With neither, every text cell is a unit and is
//!   translated where it stands.
//! * A cell is a unit when it has a letter and is not a formula (`=SUM(A1)`, Excel's `="0012"`). Every cell this
//!   filter writes that starts with `= + - @` (or TAB, CR, an apostrophe) gets an apostrophe in front (OWASP).
//! * A target column that already holds text keeps it when the segment has no translation of its own.
//!
//! Encodings: UTF-8 (BOM kept), UTF-16 with a BOM, and the legacy code pages `bilingua-model` reads (the source
//! language picks the page, Windows-1252 by default). A legacy file goes back in its own code page when every
//! character fits, else as UTF-8 with a BOM (so Excel still opens it).

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

pub mod plan;
mod rec;
mod sniff;

use bilingua_model::enc::{self, DecodeOptions};
use bilingua_model::*;
use plan::{is_translatable, skip_reason, Plan};
use rec::{blank, parse_row, rewrite, Rec, Records};
use std::collections::HashMap;

pub use plan::Spec;

/// The code that carries a unit's place in its record. On the first unit of a record: `standalone` = the raw text of
/// the record, `open` = the delimiter. On every unit: `label` = `first:write:source` (the first unit of the record,
/// the column the translation is written into, the column the text came from).
const HIDDEN: &str = "$csv";

/// What the facade cannot say: the column mapping and the project's target locales.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CsvOptions {
    /// `source=English, target.de=German, key=id, context=notes, maxlen=limit, header=yes|no|auto,
    /// delimiter=comma|semicolon|tab|pipe`. Columns are header names (any case) or 1-based numbers.
    pub columns: Option<String>,
    /// The project's target locales: they let the header row name a language (`de`, `fr-FR`) as a target column.
    /// `ExtractOptions::target_lang` is added to the list; it also picks the column this document writes into.
    pub target_locales: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Csv,
    Tsv,
}

/// The CSV filter (delimiter detected: comma, semicolon, TAB or pipe).
#[derive(Debug, Clone, Default)]
pub struct CsvFilter {
    pub options: CsvOptions,
}

/// The TSV filter (`tsv`, `tab`): the delimiter is TAB unless the mapping says `delimiter=`.
#[derive(Debug, Clone, Default)]
pub struct TsvFilter {
    pub options: CsvOptions,
}

struct Meta {
    first: usize,
    write: usize,
    source: usize,
}

fn meta(u: &Unit) -> Option<Meta> {
    let label = u.codes.get(HIDDEN)?.label.as_deref()?;
    let mut it = label.split(':').map(|p| p.parse::<usize>().ok());
    Some(Meta { first: it.next()??, write: it.next()??, source: it.next()?? })
}

fn extract_impl(kind: Kind, bytes: &[u8], opts: &ExtractOptions, copts: &CsvOptions) -> Result<Document, Error> {
    let lim = opts.limits();
    let dec = enc::decode_text(bytes, &DecodeOptions::new(opts))?;
    let text = dec.text;
    let spec = plan::parse_spec(copts.columns.as_deref())?;
    let delim = spec.delimiter.or((kind == Kind::Tsv).then_some(b'\t')).unwrap_or_else(|| sniff::sniff_delimiter(&text, b','));
    let source_locale = opts.source_lang.clone().unwrap_or_default();
    let mut targets = copts.target_locales.clone();
    if let Some(t) = opts.target_lang.as_ref().filter(|t| !t.is_empty()) {
        if !targets.contains(t) {
            targets.push(t.clone());
        }
    }

    let mut recs = Records::new(&text, delim);
    let mut peek: Vec<Rec> = Vec::new();
    let mut first: Vec<String> = Vec::new();
    for r in recs.by_ref() {
        let r = r?;
        let non_blank = r.row.iter().any(|c| !blank(c));
        if non_blank {
            first = r.row.clone();
        }
        peek.push(r);
        if non_blank {
            break;
        }
    }
    let plan: Plan = plan::resolve(&spec, &first, first.len(), &source_locale, &targets)?;
    let tcol = if plan.inplace { None } else { plan::target_col(&plan, opts.target_lang.as_deref()) };
    let note_cols: Vec<usize> = plan.notes.iter().map(|(_, i)| *i).collect();

    let mut units: Vec<Unit> = Vec::new();
    let mut parts: Vec<SkelPart> = Vec::new();
    let mut pos = 0usize;
    let (mut n_tr, mut n_lock) = (0usize, 0usize);
    let mut seen_header = false;
    for (n, r) in peek.into_iter().map(Ok).chain(recs).enumerate() {
        let r = r?;
        let mut is_header = false;
        if plan.header && !seen_header && r.row.iter().any(|c| !blank(c)) {
            seen_header = true;
            is_header = true;
        }
        if is_header {
            continue;
        }
        let cells: Vec<usize> = if plan.inplace {
            (0..r.row.len()).filter(|i| !note_cols.contains(i)).collect()
        } else {
            plan.source.filter(|s| *s < r.row.len()).into_iter().collect()
        };
        let mut members: Vec<(usize, bool)> = Vec::new();
        for c in cells {
            let v = &r.row[c];
            if is_translatable(v) {
                members.push((c, true));
            } else if opts.include_locked && skip_reason(v).is_some() {
                members.push((c, false));
            }
        }
        if members.is_empty() {
            continue;
        }
        let notes = plan::note_parts(&plan, &r.row);
        let first_idx = units.len();
        if r.start > pos {
            parts.push(SkelPart::Raw(text[pos..r.start].to_string()));
        }
        pos = r.end;
        for (c, tr) in members {
            let v = &r.row[c];
            let mut unit = if tr {
                n_tr += 1;
                guard::check_unit_count(n_tr, &lim)?;
                Unit::new(format!("u{n_tr}"), vec![Inline::text(v.as_str())])
            } else {
                n_lock += 1;
                let mut u = Unit::new(format!("k{n_lock}"), vec![Inline::text(v.as_str())]);
                u.translate = false;
                u.lock_reason = skip_reason(v).map(|s| s.to_string());
                u
            };
            let write = if plan.inplace { c } else { tcol.unwrap_or(c) };
            let mut code = CodeData { label: Some(format!("{first_idx}:{write}:{c}")), ..Default::default() };
            if units.len() == first_idx {
                code.standalone = Some(text[r.start..r.end].to_string());
                code.open = Some((delim as char).to_string());
            }
            unit.codes.insert(HIDDEN.into(), code);
            unit.context.kind = Some("cell".into());
            unit.context.location = Some(format!("row {}", n + 1));
            unit.context.notes = notes.clone();
            if let Some((_, i)) = plan.notes.iter().find(|(l, _)| l == "Key") {
                if let Some(k) = r.row.get(*i).filter(|k| !blank(k)) {
                    unit.context.key = Some(k.trim().to_string());
                }
            }
            if let Some((_, i)) = plan.notes.iter().find(|(l, _)| l == "Max length") {
                unit.context.max_len = r.row.get(*i).and_then(|m| m.trim().parse().ok());
            }
            if plan.header {
                if let Some(h) = first.get(c).filter(|h| !blank(h)) {
                    unit.context.extra.insert("column".into(), h.trim().to_string());
                }
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
        }
    }
    if pos < text.len() {
        parts.push(SkelPart::Raw(text[pos..].to_string()));
    }
    let doc = Document::new(match kind {
            Kind::Csv => "csv",
            Kind::Tsv => "tsv",
        }
        , units, Skeleton::text(parts, dec.encoding, bilingua_model::parser_info!())).with_options(opts);
    guard::finish(doc, dec.warnings, &lim)
}

/// Text back to bytes. A legacy file goes back in its own code page when every character fits, else as UTF-8 with a
/// BOM. Line endings are written as they are in the merged text (a written line break stays `\n`).
fn to_bytes(text: &str, tag: &str, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    let base = enc::base(tag);
    if enc::is_legacy(tag) && opts.output_encoding != OutputEncoding::Utf8 {
        if let Some(e) = encoding_rs::Encoding::for_label(base.as_bytes()) {
            let writer = if e == encoding_rs::GBK { encoding_rs::GB18030 } else { e };
            let (bytes, _, unmappable) = writer.encode(text);
            return if unmappable { enc::encode_text(text, "utf-8-bom") } else { Ok(bytes.into_owned()) };
        }
    }
    // `encode_text` drops one leading U+FEFF (the byte-order mark it writes itself); a file whose first cell
    // really starts with U+FEFF after its BOM keeps it.
    if text.starts_with('\u{feff}') {
        return enc::encode_text_with(&format!("\u{feff}{text}"), utf8_base(base, opts), opts);
    }
    enc::encode_text_with(text, utf8_base(base, opts), opts)
}

/// With UTF-8 out a legacy file is written as plain UTF-8; a UTF-8 BOM the file had is kept.
fn utf8_base<'a>(base: &'a str, opts: &MergeOptions) -> &'a str {
    if opts.output_encoding == OutputEncoding::Utf8 && base != "utf-8-bom" {
        "utf-8"
    } else {
        base
    }
}

fn merge_impl(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    // no language stamp in CSV: the target column is chosen at extract time
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write("csv needs a text skeleton".into()));
    };
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, u) in doc.units.iter().enumerate() {
        if let Some(m) = meta(u) {
            groups.entry(m.first).or_default().push(i);
        }
    }
    let mut out = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => out.push_str(s),
            SkelPart::Unit(i) => {
                let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                // a unit without its record code would drop the record silently: refuse instead
                let m = meta(unit).ok_or_else(|| Error::Write(format!("unit {i} has lost its {HIDDEN} code")))?;
                if m.first == *i {
                    out.push_str(&render_record(doc, *i, groups.get(i).map(Vec::as_slice).unwrap_or(&[]))?);
                }
            }
        }
    }
    to_bytes(&out, encoding, opts)
}

fn render_record(doc: &Document, first: usize, group: &[usize]) -> Result<String, Error> {
    let code = doc.units[first].codes.get(HIDDEN);
    let raw = code
        .and_then(|c| c.standalone.as_deref())
        .ok_or_else(|| Error::Write(format!("unit {first} has lost its record text")))?;
    // only the four delimiters this filter reads (a multi-byte one would split a character)
    let delim = match code.and_then(|c| c.open.as_deref()) {
        Some(",") => b',',
        Some(";") => b';',
        Some("\t") => b'\t',
        Some("|") => b'|',
        other => return Err(Error::Write(format!("unit {first} has an unknown delimiter {other:?}"))),
    };
    let changed = |u: &Unit| u.translate && u.segments.iter().any(|s| !s.target.is_empty());
    if !group.iter().any(|&ui| changed(&doc.units[ui])) {
        return Ok(raw.to_string());
    }
    let row = parse_row(raw, delim)?;
    let mut writes: Vec<(usize, String)> = Vec::new();
    for &ui in group {
        let u = &doc.units[ui];
        let Some(m) = meta(u) else { continue };
        // a column past the limit can only come from an edited document (and would pad the row without bound)
        if m.write >= rec::MAX_COLUMNS || m.source >= rec::MAX_COLUMNS {
            return Err(Error::Write(format!("unit {ui} names a column past {}", rec::MAX_COLUMNS)));
        }
        if !changed(u) {
            continue;
        }
        let tr = plain_text(&u.output());
        let cur = row.get(m.source).map(String::as_str).unwrap_or("");
        if m.write == m.source {
            if tr != cur {
                writes.push((m.source, tr));
            }
        } else {
            let existing = row.get(m.write).map(String::as_str).unwrap_or("");
            if tr == cur && !blank(existing) {
                continue; // untranslated: keep a target someone already filled
            }
            if tr != existing {
                writes.push((m.write, tr));
            }
        }
    }
    if writes.is_empty() {
        return Ok(raw.to_string());
    }
    Ok(rewrite(raw, &row, delim, &writes))
}

fn head_text(bytes: &[u8]) -> Option<String> {
    let head = &bytes[..bytes.len().min(8192)];
    let body = head.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(head);
    let text = match std::str::from_utf8(body) {
        Ok(t) => t,
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&body[..e.valid_up_to()]).ok()?,
        Err(_) => return None,
    };
    (!text.contains('\0')).then(|| text.to_string())
}

macro_rules! filter_impl {
    ($ty:ty, $id:expr, $exts:expr, $kind:expr, $only:expr) => {
        impl $ty {
            /// A filter with a column mapping.
            pub fn with_options(options: CsvOptions) -> Self {
                Self { options }
            }
            /// Like [`Filter::extract`] with an explicit column mapping.
            pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, options: &CsvOptions) -> Result<Document, Error> {
                extract_impl($kind, bytes, opts, options)
            }
        }
        impl Filter for $ty {
            fn id(&self) -> &'static str {
                $id
            }
            fn extensions(&self) -> &'static [&'static str] {
                $exts
            }
            fn sniff(&self, bytes: &[u8]) -> bool {
                head_text(bytes).is_some_and(|t| sniff::looks_tabular(&t, $only))
            }
            fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
                extract_impl($kind, bytes, opts, &self.options)
            }
            fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
                merge_impl(doc, opts)
            }
        }
    };
}

filter_impl!(CsvFilter, "csv", &["csv"], Kind::Csv, None);
filter_impl!(TsvFilter, "tsv", &["tsv", "tab"], Kind::Tsv, Some(b'\t'));
