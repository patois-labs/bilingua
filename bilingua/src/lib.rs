//! Bilingua: open a source file for translation, segment it, and write it back.
//!
//! ```ignore
//! let opts = ExtractOptions { target_lang: Some("es-ES".into()), ..Default::default() };
//! let mut doc = bilingua::extract("page.html", &bytes, &opts)?;
//! doc.set_target("p3#1", vec![Inline::text("Hola.")])?;
//! let out: Vec<u8> = bilingua::merge(&doc)?;
//! ```
//!
//! Formats register in [`filters`]. Night build 10-28: the format crates are wired in by Opus at merge time;
//! until then a small built-in plain-text filter keeps the API usable.

pub use bilingua_model::*;

mod plain;

/// All available filters, most specific first.
pub fn filters() -> Vec<Box<dyn Filter>> {
    vec![Box::new(plain::PlainText)]
}

/// The filter for a file: by extension first, then by sniffing the bytes.
pub fn detect(name: &str, bytes: &[u8]) -> Option<Box<dyn Filter>> {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let all = filters();
    let mut by_ext = None;
    let mut sniffed = None;
    for f in all {
        if by_ext.is_none() && f.extensions().contains(&ext.as_str()) {
            by_ext = Some(f);
        } else if sniffed.is_none() && f.sniff(bytes) {
            sniffed = Some(f);
        }
    }
    by_ext.or(sniffed)
}

/// Extracts and segments a file. The returned document's `format` names the filter that `merge` will use.
pub fn extract(name: &str, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let f = detect(name, bytes).ok_or_else(|| Error::Unsupported(format!("no parser for {name}")))?;
    let mut doc = f.extract(bytes, opts)?;
    doc.name = Some(name.to_string());
    doc.granularity = opts.granularity;
    // Sentence segmentation (bilingua-seg) is applied here once that crate is merged.
    Ok(doc)
}

fn filter_for(doc: &Document) -> Result<Box<dyn Filter>, Error> {
    filters()
        .into_iter()
        .find(|f| f.id() == doc.format)
        .ok_or_else(|| Error::Unsupported(format!("no writer for format {}", doc.format)))
}

/// Writes the document back in its original format.
pub fn merge(doc: &Document) -> Result<Vec<u8>, Error> {
    filter_for(doc)?.merge(doc)
}

/// Translated HTML preview with `data-bl-seg` spans, for formats that have one.
pub fn preview(doc: &Document) -> Option<String> {
    filter_for(doc).ok()?.preview(doc)
}

/// Inner HTML for one segment's preview span, for live patches.
pub fn render_segment(doc: &Document, seg_id: &str) -> Option<String> {
    filter_for(doc).ok()?.render_segment(doc, seg_id)
}
