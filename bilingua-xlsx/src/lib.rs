//! Bilingua XLSX filter: Excel workbooks to units and back. See the README for what is and is not supported.
//!
//! ```ignore
//! let f = bilingua_xlsx::Xlsx::default();
//! let mut doc = f.extract(&bytes, &ExtractOptions::default())?;
//! doc.set_target("ss3", vec![Inline::text("Hola.")])?;
//! let (out, report) = bilingua_xlsx::merge_with_report(&doc)?;
//! ```

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 2;

mod extract;
mod locos;
mod refs;
mod render;
mod rich;
mod xml;
mod zipio;

pub use render::MergeReport;

use bilingua_model::*;
use serde::{Deserialize, Serialize};
use zipio::EntryMeta;

/// The pseudo entry that carries zip details (compression, times). It holds a NUL, which no real zip entry name
/// can, and it is never written to the output file.
const META_NAME: &str = "\0bilingua-xlsx-meta";

/// Markers inside skeleton text. Workbooks that already contain these characters are refused.
pub(crate) const M_UNIT: char = '\u{E000}';
pub(crate) const M_END: char = '\u{E001}';
/// Sheet name in `workbook.xml`: `M_NAME id M_END original M_ORIG`.
pub(crate) const M_NAME: char = '\u{E002}';
/// Sheet name inside a formula.
pub(crate) const M_SREF: char = '\u{E003}';
pub(crate) const M_ORIG: char = '\u{E004}';
/// Table column name (mirrors the header cell).
pub(crate) const M_COL: char = '\u{E005}';

pub(crate) const C_PFX: &str = "_p";

/// Options that `ExtractOptions` has no field for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct XlsxOptions {
    /// Make sheet names units. Off by default: formulas, charts and macros refer to sheet names.
    /// Formulas and defined names in the workbook are rewritten; charts and external links are not.
    pub translate_sheet_names: bool,
    /// Leave out text that is only shown in hidden sheets, rows or columns. Off by default: hidden text is
    /// extracted with the note "hidden".
    pub skip_hidden: bool,
    /// Make cell comments (legacy notes and threaded comments) units. Off by default: they are notes on the
    /// cell's unit.
    pub translate_comments: bool,
    /// Column mode (LocOS's `columns=`): `source=English, target.de=German, key=id, context=notes, maxlen=limit,
    /// header=yes|no|auto`; columns are header names (any case) or 1-based numbers. Each sheet is planned from its
    /// first non-empty row; the source column's text cells become units and the translation is written into the
    /// target column of the same row (cells and rows are created where there are none). Comments, text boxes and
    /// the other options do not apply in column mode. Off by default (every text cell is a unit, in place).
    pub columns: Option<String>,
    /// The project's target locales, so a header cell may name a language (`de`, `fr-FR`) as a target column.
    /// `ExtractOptions::target_lang` is added to the list; it also picks the column this document writes into.
    pub target_locales: Vec<String>,
}

impl XlsxOptions {
    /// These options with `xlsx.columns` and `xlsx.target_locales` from `format_options` applied (the mapping is
    /// replaced, the locales are added) and the three flags.
    fn with_format_options(&self, opts: &ExtractOptions) -> XlsxOptions {
        let mut xo = self.clone();
        let set = |key: &str, field: &mut bool| {
            if let Some(on) = opts.format_flag(key) {
                *field = on;
            }
        };
        set("xlsx.translate_comments", &mut xo.translate_comments);
        set("xlsx.translate_sheet_names", &mut xo.translate_sheet_names);
        set("xlsx.skip_hidden", &mut xo.skip_hidden);
        if let Some(c) = opts.format_option("xlsx.columns") {
            xo.columns = Some(c.to_string());
        }
        if let Some(l) = opts.format_option("xlsx.target_locales") {
            for t in l.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                if !xo.target_locales.iter().any(|x| x == t) {
                    xo.target_locales.push(t.to_string());
                }
            }
        }
        xo
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Meta {
    pub entries: Vec<EntryMeta>,
    /// Entries whose skeleton bytes are the original deflated bytes (merge copies them without recompressing).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub packed: std::collections::BTreeMap<String, zipio::Packed>,
}

/// The `format_options` keys of the XLSX filter. When set they win over the filter's own `XlsxOptions`.
pub const FORMAT_OPTIONS: &[FormatOptionKey] = &[
    FormatOptionKey::bool("xlsx.translate_comments", "Make cell comments (notes and threaded comments) units."),
    FormatOptionKey::bool("xlsx.translate_sheet_names", "Make sheet names units (formulas and defined names are rewritten)."),
    FormatOptionKey::bool("xlsx.skip_hidden", "Leave out text that is only shown in hidden sheets, rows or columns."),
    FormatOptionKey::text("xlsx.walk", "Accepted and ignored: LocOS's numbering walk stamp; Bilingua has one walk, the current one."),
    FormatOptionKey::text("xlsx.columns", "Column mode: the column mapping, as XlsxOptions::columns: source=English, target.de=German, key=id, ..."),
    FormatOptionKey::text("xlsx.target_locales", "The project's target locales, comma separated (de, fr-FR), so a header may name a language."),
    FormatOptionKey::text("xlsx.table", "Accepted and ignored: LocOS's stored column plan; Bilingua plans the columns again from xlsx.columns."),
];

#[derive(Default)]
#[non_exhaustive]
pub struct Xlsx {
    pub options: XlsxOptions,
}

impl Xlsx {
    pub fn with_options(options: XlsxOptions) -> Xlsx {
        Xlsx { options }
    }
}

/// A member's XML refused by `xml::parse`: the nesting cap is a limit (`LimitKind::Depth`), like every other
/// crate's; anything else is a parse error.
pub(crate) fn xml_member_err(member: &str, m: &str) -> Error {
    if m.starts_with("elements nested deeper than") {
        return bilingua_model::LimitError::new(bilingua_model::LimitKind::Depth, format!("{member}: {m}")).into();
    }
    perr(format!("{member}: {m}"))
}

pub(crate) fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "xlsx".into(), msg: msg.into() }
}

type Split<'a> = (Option<Meta>, Vec<&'a ZipEntry>);

fn split_meta(doc: &Document) -> Result<Split<'_>, Error> {
    let Skeleton::Zip { entries, .. } = &doc.skeleton else {
        return Err(Error::Write("xlsx needs a zip skeleton".into()));
    };
    let mut meta = None;
    let mut real = Vec::new();
    for e in entries {
        if e.name == META_NAME {
            if let ZipData::Raw(b) = &e.data {
                // An unreadable record must not be skipped: without its list of packed entries, their deflated
                // bytes would be written as if they were plain, and the file would be broken.
                meta = Some(serde_json::from_slice::<Meta>(b).map_err(|e| {
                    Error::Write(format!("the xlsx skeleton's zip record is unreadable: {e}"))
                })?);
            }
        } else {
            real.push(e);
        }
    }
    Ok((meta, real))
}

/// Extracts with explicit options.
pub fn extract_with(bytes: &[u8], opts: &ExtractOptions, xo: &XlsxOptions) -> Result<Document, Error> {
    if opts.is_profile(bilingua_model::profile::LOCOS) || locos::table_mode(xo) {
        return locos::extract(bytes, opts, xo);
    }
    extract::extract_doc(bytes, opts, xo)
}

/// Writes the document and reports what the caller should know.
pub fn merge_with_report(doc: &Document) -> Result<(Vec<u8>, MergeReport), Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    if let Skeleton::Binary { .. } = &doc.skeleton {
        // a document extracted under the Localization OS profile
        let opts = locos::report_options(doc);
        return locos::merge(doc, &opts).map(|b| (b, MergeReport { warnings: vec![] }));
    }
    let (meta, real) = split_meta(doc)?;
    let mut r = render::Renderer::new(doc);
    let mut out: Vec<(EntryMeta, zipio::Body)> = Vec::with_capacity(real.len());
    for e in real {
        let data = match &e.data {
            ZipData::Raw(b) => zipio::raw_body(meta.as_ref().map(|m| &m.packed), &e.name, b),
            ZipData::Parts(parts) => {
                let mut s = String::new();
                for p in parts {
                    match p {
                        SkelPart::Raw(t) => s.push_str(&r.resolve(t)?),
                        SkelPart::Unit(i) => s.push_str(&r.unit(*i)?),
                    }
                }
                zipio::Body::Owned(s.into_bytes())
            }
        };
        let em = meta
            .as_ref()
            .and_then(|m| m.entries.iter().find(|x| x.name == e.name).cloned())
            .unwrap_or(EntryMeta {
                name: e.name.clone(),
                stored: false,
                dir: e.name.ends_with('/'),
                modified: None,
            });
        out.push((em, data));
    }
    let bytes = zipio::write_zip_parts(&out)?;
    Ok((bytes, MergeReport { warnings: r.warnings }))
}

impl Filter for Xlsx {
    fn id(&self) -> &'static str {
        "xlsx"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["xlsx", "xlsm", "xltx", "xltm"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
        bytes.starts_with(b"PK\x03\x04") && has(b"[Content_Types].xml") && has(b"xl/")
    }
    fn format_option_keys(&self) -> &'static [FormatOptionKey] {
        FORMAT_OPTIONS
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let xo = self.options.with_format_options(opts);
        if opts.is_profile(bilingua_model::profile::LOCOS) || locos::table_mode(&xo) {
            // LocOS's walk (cells, text boxes, comments when asked) or its column mode: see `locos`
            return locos::extract(bytes, opts, &xo);
        }
        extract::extract_doc(bytes, opts, &xo)
    }
    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        if let Skeleton::Binary { .. } = &doc.skeleton {
            // a document extracted under the Localization OS profile
            bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
            return locos::merge(doc, opts);
        }
        // The parts are UTF-8 XML and a workbook has no language to stamp, so neither option changes anything.
        merge_with_report(doc).map(|(b, _)| b)
    }
}

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_oracle;

/// B-SIDE 16: a member nested past the cap is a limit (`LimitKind::Depth`), not a parse error; other XML damage
/// stays a parse error.
#[cfg(test)]
mod depth_is_a_limit {
    #[test]
    fn nesting_cap_is_coded_limit() {
        let deep = "<a>".repeat(xml::MAX_DEPTH + 5) + &"</a>".repeat(xml::MAX_DEPTH + 5);
        let m = crate::xml::parse(&deep).unwrap_err();
        match crate::xml_member_err("word/document.xml", &m) {
            bilingua_model::Error::Limit(e) => {
                assert_eq!(e.kind, bilingua_model::LimitKind::Depth);
                assert!(e.msg.starts_with("word/document.xml: elements nested deeper than"), "{}", e.msg);
            }
            other => panic!("{other:?}"),
        }
        let bad = crate::xml::parse("<a><b></a>").unwrap_err();
        assert!(matches!(crate::xml_member_err("m.xml", &bad), bilingua_model::Error::Parse { .. }));
    }
    use crate::xml;
}
