//! Bilingua rich text JSON filters: Contentful Rich Text (`contentful`), Sanity Portable Text (`sanity`), Jira ADF
//! (`adf`) and Notion blocks (`notion`), as Localization OS reads them in `app/filters/` and `app/services/connectors/`.
//!
//! A file holds the value of one rich text field (a Contentful `document`, a Portable Text array, an ADF `doc`, a
//! Notion block list). Text in the tree is the unit's text; marks (bold, a link) are paired inline tags, embedded
//! entries, mentions and code spans standalone tags. The file is parsed into a tree that keeps key order and number
//! spellings; a merge that translates nothing returns the original bytes, and a merge that translates rewrites only
//! the containers that were translated (everything else is copied from the source tree) in the file's layout
//! (indent, separators, trailing newline, `\u` escapes).
//!
//! Limits: nesting 256 (`Error::Parse`), the shared `bilingua_model::Limits` for size and unit count. Nothing panics.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod adf;
mod common;
mod contentful;
mod json;
mod notion;
mod sanity;

use bilingua_model::*;
use common::*;

fn sniff_with(bytes: &[u8], shape: fn(&json::Value) -> bool) -> bool {
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
    let Ok(text) = std::str::from_utf8(body) else { return false };
    matches!(text.trim_start().as_bytes().first(), Some(b'{' | b'[')) && json::parse(text).is_ok_and(|p| shape(&p.value))
}

/// Contentful Rich Text (`{"nodeType": "document", ...}`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Contentful;

/// Sanity Portable Text (an array of blocks).
#[derive(Debug, Clone, Copy, Default)]
pub struct Sanity;

/// Jira Atlassian Document Format (`{"type": "doc", ...}`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Adf;

/// Notion blocks (an array of block objects, or a list response with `results`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Notion;

/// The `format_options` keys of the ADF filter.
pub const ADF_OPTIONS: &[FormatOptionKey] =
    &[FormatOptionKey::text("adf.units", "`inline` (default): one unit per paragraph with marks as tag pairs; `leaf`: one unit per text leaf, no tags.")];

impl Filter for Contentful {
    fn id(&self) -> &'static str {
        "contentful"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["contentful"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        sniff_with(bytes, contentful::is_document)
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        check_own_options(self, &opts.format_options)?;
        extract_doc("contentful", "a Contentful Rich Text document", bytes, opts, "", contentful::is_document, contentful::plan)
    }
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        check_own_options(self, &opts.format_options)?;
        merge_doc("contentful", doc, opts, contentful::plan, contentful::apply)
    }
}

impl Filter for Sanity {
    fn id(&self) -> &'static str {
        "sanity"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["portabletext"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        sniff_with(bytes, sanity::is_portable_text)
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        check_own_options(self, &opts.format_options)?;
        extract_doc("sanity", "Sanity Portable Text (an array with a text block)", bytes, opts, "", sanity::is_portable_text, sanity::plan)
    }
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        check_own_options(self, &opts.format_options)?;
        merge_doc("sanity", doc, opts, sanity::plan, sanity::apply)
    }
}

impl Filter for Adf {
    fn id(&self) -> &'static str {
        "adf"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["adf"]
    }
    fn format_option_keys(&self) -> &'static [FormatOptionKey] {
        ADF_OPTIONS
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        sniff_with(bytes, adf::is_adf)
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        check_own_options(self, &opts.format_options)?;
        if let Some(v) = opts.format_options.get("adf.units") {
            if !matches!(v.trim().to_ascii_lowercase().as_str(), "inline" | "leaf") {
                return Err(Error::FormatOption { key: "adf.units".into(), msg: "use inline or leaf".into() });
            }
        }
        extract_doc("adf", "a Jira ADF document", bytes, opts, mode_option(&opts.format_options), adf::is_adf, adf::plan)
    }
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        check_own_options(self, &opts.format_options)?;
        merge_doc("adf", doc, opts, adf::plan, adf::apply)
    }
}

impl Filter for Notion {
    fn id(&self) -> &'static str {
        "notion"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["notion"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        sniff_with(bytes, notion::is_notion)
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        check_own_options(self, &opts.format_options)?;
        extract_doc("notion", "a Notion block list", bytes, opts, "", notion::is_notion, notion::plan)
    }
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        check_own_options(self, &opts.format_options)?;
        merge_doc("notion", doc, opts, notion::plan, notion::apply)
    }
}
