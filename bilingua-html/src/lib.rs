//! Bilingua HTML filter: the Localization OS HTML filter (`html_filter.HtmlFilter`), unit for unit and byte for
//! byte, on the engine in `bilingua-htmlcore`, plus a live preview. See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
/// 2: the skeleton is the page itself and units are LocOS's (a schema 1 document with units is refused at merge;
/// open the file again).
pub const SKELETON_SCHEMA: u32 = 2;

mod page;
mod preview;
mod sanitize;
mod tok;

pub use page::VALUE;
pub use sanitize::sanitize_html;

/// LocOS `_xml_named_refs`: HTML-only named references in a target (`&nbsp;`) written as numeric references, an
/// unknown one as text, so the target stays XML in an XHTML page read without a DTD (EPUB chapters, Flare topics).
pub fn xml_named_refs(s: &str) -> String {
    bilingua_htmlcore::pyx::xml_named_refs(s)
}

use bilingua_model::*;
use regex::Regex;

/// The HTML filter with LocOS's defaults (page metadata offered, no template pattern). The same as
/// [`HtmlFilter::new()`].
pub struct Html;

/// An HTML filter with options (LocOS `HtmlFilter(template_pattern=None, page_meta=True)`):
///
/// * `page_meta` (default on): offer the `<meta>` description, keywords, `og:` and `twitter:` text after the
///   accessibility attributes. Off for callers that join several pages into one document (EPUB chapters, e-mail
///   parts), as LocOS does.
/// * `template_pattern` (default none): spans of a template language (`{{ ... }}`, `{% ... %}`) that stay in their
///   text: a `<` inside one never starts a tag and a translation writes them unescaped. With a pattern, quotes in
///   text are always escaped and server code is not routed to the server-page filters.
///
/// A plain `.html` page with closed PHP or `<% %>` islands is read through `bilingua-serverpages` (LocOS
/// `server_page_for_html`); its document says `html` and merges back through the same dialect.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct HtmlFilter {
    pub page_meta: bool,
    pub template_pattern: Option<Regex>,
}

impl Default for HtmlFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl HtmlFilter {
    /// LocOS's defaults: page metadata on, no template pattern.
    pub fn new() -> Self {
        HtmlFilter { page_meta: true, template_pattern: None }
    }

    /// Offers the page metadata units (or not).
    pub fn with_page_meta(mut self, on: bool) -> Self {
        self.page_meta = on;
        self
    }

    /// Spans of a template language that stay in their text (see the type docs).
    pub fn with_template_pattern(mut self, re: Regex) -> Self {
        self.template_pattern = Some(re);
        self
    }

    fn cfg(&self) -> page::Cfg<'_> {
        page::Cfg { page_meta: self.page_meta, template: self.template_pattern.as_ref() }
    }
}

fn sniff_html(bytes: &[u8]) -> bool {
    let head: String = bytes.iter().take(1024).map(|b| b.to_ascii_lowercase() as char).collect();
    let t = head.trim_start_matches('\u{ef}').trim_start_matches(['\u{bb}', '\u{bf}']).trim_start();
    t.starts_with("<!doctype html") || t.contains("<html") || t.contains("<body") || t.contains("<head>")
}

impl Filter for HtmlFilter {
    fn id(&self) -> &'static str {
        "html"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["html", "htm", "xhtml"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        sniff_html(bytes)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        page::extract(bytes, opts, self.cfg())
    }

    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        page::merge(doc, opts, self.cfg())
    }

    fn preview(&self, doc: &Document) -> Option<String> {
        preview::render(doc, self.cfg(), None)
    }

    fn render_segment(&self, doc: &Document, seg_id: &str) -> Option<String> {
        preview::render(doc, self.cfg(), Some(seg_id))
    }
}

impl Filter for Html {
    fn id(&self) -> &'static str {
        "html"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["html", "htm", "xhtml"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        sniff_html(bytes)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        HtmlFilter::new().extract(bytes, opts)
    }

    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        HtmlFilter::new().merge_with(doc, opts)
    }

    fn preview(&self, doc: &Document) -> Option<String> {
        HtmlFilter::new().preview(doc)
    }

    fn render_segment(&self, doc: &Document, seg_id: &str) -> Option<String> {
        HtmlFilter::new().render_segment(doc, seg_id)
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
