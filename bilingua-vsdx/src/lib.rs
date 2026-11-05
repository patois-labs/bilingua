//! Bilingua Visio filter (`.vsdx`, macro-enabled `.vsdm`): page names, shape text and shape data labels to units
//! and back. See the README for what is and is not supported.
//!
//! ```ignore
//! let f = bilingua_vsdx::Vsdx;
//! let mut doc = f.extract(&bytes, &ExtractOptions::default())?;
//! doc.set_target("page1-3", vec![Inline::text("Hallo")])?;
//! let out = f.merge(&doc)?;
//! ```

/// Skeleton schema number: bump only when this crate's skeleton layout changes.
pub const SKELETON_SCHEMA: u32 = 1;

mod extract;
mod lang;
mod lcid_table;
mod render;
mod xml;
mod zipdir;

use bilingua_model::*;
use bilingua_zip::{EntryMeta, Packed, ZipLimits};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The pseudo entry that carries zip details; it holds a NUL, which no real zip entry name can.
const META_NAME: &str = "\0bilingua-vsdx-meta";

/// Markers inside skeleton text. XML 1.0 cannot contain these control characters, so they never occur in a
/// part that was parsed.
pub(crate) const M_OPEN: char = '\u{1}';
pub(crate) const M_END: char = '\u{3}';

/// Reserved code names (unit tags are "1", "2", ...).
pub(crate) const C_KIND: &str = "_k";
pub(crate) const C_QUOTE: &str = "_q";
pub(crate) const C_ORIG: &str = "_orig";
pub(crate) const C_PRE: &str = "_pre";
pub(crate) const C_SUF: &str = "_suf";
pub(crate) const C_CRLF: &str = "_crlf";
pub(crate) const C_SHAPE: &str = "_shape";

pub(crate) const DEFAULT_MAX_DEPTH_CAP: usize = 256;

#[derive(Serialize, Deserialize)]
pub(crate) struct Meta {
    pub entries: Vec<EntryMeta>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub packed: BTreeMap<String, Packed>,
    /// Encoding tag of every XML part that is not plain UTF-8 without BOM.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub enc: BTreeMap<String, String>,
    /// True when comments were extracted.
    #[serde(default)]
    pub comments: bool,
}

pub(crate) fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "vsdx".into(), msg: msg.into() }
}

/// The Visio filter. Comments are not extracted by default (LocOS: the project's "translate comments" option);
/// use [`Vsdx::with_comments`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Vsdx;

impl Vsdx {
    /// Like `extract`, and also the text of the `CommentEntry` elements of `visio/comments.xml` as units
    /// (`comment-N`, style "comment").
    pub fn extract_with_comments(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        extract::extract_doc(bytes, opts, true)
    }
}

impl Filter for Vsdx {
    fn id(&self) -> &'static str {
        "vsdx"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["vsdx", "vsdm"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
        bytes.starts_with(b"PK\x03\x04") && has(b"[Content_Types].xml") && has(b"visio/")
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        extract::extract_doc(bytes, opts, false)
    }
    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        render::merge(doc)
    }
}

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_gate;
#[cfg(test)]
mod tests_more;
