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
    FormatOptionKey::text("xlsx.table", "Accepted and ignored: LocOS's column plan (Bilingua has no column mode for workbooks)."),
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
    extract::extract_doc(bytes, opts, xo)
}

/// Writes the document and reports what the caller should know.
pub fn merge_with_report(doc: &Document) -> Result<(Vec<u8>, MergeReport), Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
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
        let mut xo = self.options.clone();
        let set = |key: &str, field: &mut bool| {
            if let Some(on) = opts.format_flag(key) {
                *field = on;
            }
        };
        set("xlsx.translate_comments", &mut xo.translate_comments);
        set("xlsx.translate_sheet_names", &mut xo.translate_sheet_names);
        set("xlsx.skip_hidden", &mut xo.skip_hidden);
        extract::extract_doc(bytes, opts, &xo)
    }
    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
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
