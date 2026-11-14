//! Bilingua PPTX filter: PowerPoint presentations to units and back. See the README for what is and is not
//! supported.
//!
//! ```ignore
//! let f = bilingua_pptx::Pptx;
//! let mut doc = f.extract(&bytes, &ExtractOptions::default())?;
//! doc.set_target("slide1-p1", vec![Inline::text("Hola.")])?;
//! let (out, report) = bilingua_pptx::merge_with_report(&doc)?;
//! ```

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 2;

mod extract;
pub mod locos;
mod preview;
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
const META_NAME: &str = "\0bilingua-pptx-meta";

/// Markers inside skeleton text. Parts that already contain these characters are refused or left alone.
pub(crate) const M_UNIT: char = '\u{E000}';
pub(crate) const M_END: char = '\u{E001}';
/// A SmartArt drawing paragraph that mirrors a data paragraph: `M_MIRROR id M_END original M_ORIG`.
pub(crate) const M_MIRROR: char = '\u{E002}';
pub(crate) const M_ORIG: char = '\u{E004}';

/// Reserved code names (besides those in `rich`).
pub(crate) const C_PFX: &str = "_p";
pub(crate) const C_LEAD: &str = "_lead";

#[derive(Serialize, Deserialize)]
pub(crate) struct Meta {
    pub entries: Vec<EntryMeta>,
    /// Entries whose skeleton bytes are the original deflated bytes (merge copies them without recompressing).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub packed: std::collections::BTreeMap<String, zipio::Packed>,
}

pub struct Pptx;

/// A member's XML refused by `xml::parse`: the nesting cap is a limit (`LimitKind::Depth`), like every other
/// crate's; anything else is a parse error.
pub(crate) fn xml_member_err(member: &str, m: &str) -> Error {
    if m.starts_with("elements nested deeper than") {
        return bilingua_model::LimitError::new(bilingua_model::LimitKind::Depth, format!("{member}: {m}")).into();
    }
    perr(format!("{member}: {m}"))
}

pub(crate) fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "pptx".into(), msg: msg.into() }
}

type Split<'a> = (Option<Meta>, Vec<&'a ZipEntry>);

fn split_meta(doc: &Document) -> Result<Split<'_>, Error> {
    let Skeleton::Zip { entries, .. } = &doc.skeleton else {
        return Err(Error::Write("pptx needs a zip skeleton".into()));
    };
    let mut meta = None;
    let mut real = Vec::new();
    for e in entries {
        if e.name == META_NAME {
            if let ZipData::Raw(b) = &e.data {
                // An unreadable record must not be skipped: without its list of packed entries, their deflated
                // bytes would be written as if they were plain, and the file would be broken.
                meta = Some(serde_json::from_slice::<Meta>(b).map_err(|e| {
                    Error::Write(format!("the pptx skeleton's zip record is unreadable: {e}"))
                })?);
            }
        } else {
            real.push(e);
        }
    }
    Ok((meta, real))
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

/// The `format_options` keys of the PPTX filter. `pptx.translate_comments`: under the LocOS profile the slide
/// comments become units as LocOS gives them; without the profile they are not extracted yet and `true` adds a
/// warning.
pub const FORMAT_OPTIONS: &[bilingua_model::FormatOptionKey] = &[
    bilingua_model::FormatOptionKey::bool("pptx.translate_comments", "Translate slide comments (LocOS profile; otherwise not supported yet and true adds a warning)."),
    bilingua_model::FormatOptionKey::text("pptx.walk", "Accepted and ignored: LocOS's numbering walk stamp; Bilingua has one walk, the current one."),
];

impl Filter for Pptx {
    fn id(&self) -> &'static str {
        "pptx"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["pptx", "pptm", "potx", "potm", "ppsx", "ppsm"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
        bytes.starts_with(b"PK\x03\x04") && has(b"[Content_Types].xml") && has(b"ppt/")
    }
    fn format_option_keys(&self) -> &'static [bilingua_model::FormatOptionKey] {
        FORMAT_OPTIONS
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        // Track D: under the Localization OS profile, LocOS's own walk, numbers and writer
        let mut doc = if opts.is_profile(bilingua_model::profile::LOCOS) { locos::extract(bytes, opts)? } else { extract::extract_doc(bytes, opts)? };
        let locos = opts.is_profile(bilingua_model::profile::LOCOS);
        if !locos && opts.format_flag("pptx.translate_comments") == Some(true) {
            doc.warnings.push(Warning::new(
                "format_option",
                "pptx.translate_comments",
                "Slide comments are not extracted by this version, so they stay as they are.",
            ));
        }
        Ok(doc)
    }
    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        if let Skeleton::Binary { .. } = &doc.skeleton {
            // a document extracted under the Localization OS profile
            bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
            return locos::merge(doc, opts);
        }
        // The parts are UTF-8 XML, and the language is set on the runs this merge rewrites (translated text), so neither
        // option changes anything: an untranslated run is never rewritten.
        merge_with_report(doc).map(|(b, _)| b)
    }
    fn preview(&self, doc: &Document) -> Option<String> {
        preview::preview(doc)
    }
    fn render_segment(&self, doc: &Document, seg_id: &str) -> Option<String> {
        preview::render_segment(doc, seg_id)
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
