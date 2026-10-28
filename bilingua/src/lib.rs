//! Bilingua: open a source file for translation, segment it, and write it back.
//!
//! ```ignore
//! let opts = ExtractOptions { target_lang: Some("es-ES".into()), ..Default::default() };
//! let mut doc = bilingua::extract("page.html", &bytes, &opts)?;
//! doc.set_target("p3#1", vec![Inline::text("Hola.")])?;
//! let out: Vec<u8> = bilingua::merge(&doc)?;
//! ```
//!
//! Formats register in [`filters`]: DOCX, HTML, Markdown, JSON, PO, SRT, WebVTT and plain text. `extract` also applies sentence
//! segmentation (`bilingua-seg`) for the granularity in the options.

pub use bilingua_model::*;
pub use bilingua_seg::{join, split, Rules};

/// All available filters, most specific first. Plain text goes last because it accepts any UTF-8 file.
pub fn filters() -> Vec<Box<dyn Filter>> {
    vec![
        Box::new(bilingua_docx::Docx),
        Box::new(bilingua_subtitle::Vtt),
        Box::new(bilingua_subtitle::Srt),
        Box::new(bilingua_po::Po),
        Box::new(bilingua_json::JsonFilter),
        Box::new(bilingua_markdown::MarkdownFilter),
        // HTML sniffs loosely and plain text accepts any UTF-8, so they go last.
        Box::new(bilingua_html::Html),
        Box::new(bilingua_text::TextFilter),
    ]
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
    // A file with an extension no parser claims is refused rather than guessed: plain text and HTML sniff
    // loosely, and opening a YAML or config file as plain text would let translators change its keys.
    // Sniffing only decides for files with no extension.
    if by_ext.is_none() && !ext.is_empty() {
        return None;
    }
    by_ext.or(sniffed)
}

/// Extracts and segments a file. The returned document's `format` names the filter that `merge` will use.
/// The source language comes from `opts.source_lang` (default "en"); it picks the sentence rules.
pub fn extract(name: &str, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let f = detect(name, bytes).ok_or_else(|| Error::Unsupported(format!("no parser for {name}")))?;
    let mut doc = f.extract(bytes, opts)?;
    doc.name = Some(name.to_string());
    let lang = opts.source_lang.as_deref().unwrap_or("en");
    bilingua_seg::segment(&mut doc, lang, opts.granularity, Rules::bundled());
    Ok(doc)
}

/// Changes the granularity of an extracted document (sentence, paragraph or document). Targets already set
/// survive only for units whose segments all had one.
pub fn resegment(doc: &mut Document, source_lang: &str, mode: Granularity) {
    bilingua_seg::segment(doc, source_lang, mode, Rules::bundled());
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
