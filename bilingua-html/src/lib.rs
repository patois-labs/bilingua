//! Bilingua HTML filter. See the README for what is extracted and how the preview works.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod ent;
mod extract;
mod merge;
mod tok;

pub use merge::sanitize_html;

use bilingua_model::*;

/// The HTML filter.
pub struct Html;

impl Filter for Html {
    fn id(&self) -> &'static str {
        "html"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["html", "htm", "xhtml"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let head: String = bytes.iter().take(1024).map(|b| b.to_ascii_lowercase() as char).collect();
        let t = head.trim_start_matches('\u{ef}').trim_start_matches(['\u{bb}', '\u{bf}']).trim_start();
        t.starts_with("<!doctype html") || t.contains("<html") || t.contains("<body") || t.contains("<head>")
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        extract::extract_html(bytes, opts)
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        self.merge_with(doc, &bilingua_model::MergeOptions::default())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        merge::merge_html(doc, opts)
    }

    fn preview(&self, doc: &Document) -> Option<String> {
        merge::preview_html(doc)
    }

    fn render_segment(&self, doc: &Document, seg_id: &str) -> Option<String> {
        merge::render_segment_html(doc, seg_id)
    }
}

#[cfg(test)]
mod smoke {
    use super::*;

    #[test]
    fn smoke() {
        let src = "<!DOCTYPE html>\n<html lang=\"en\"><head><title>Hi there</title></head><body><p>Hello <b>big</b> world &amp; co.</p></body></html>\n";
        let d = Html.extract(src.as_bytes(), &ExtractOptions::default()).unwrap();
        assert_eq!(d.units.len(), 2);
        assert_eq!(Html.merge(&d).unwrap(), src.as_bytes());
    }
}
