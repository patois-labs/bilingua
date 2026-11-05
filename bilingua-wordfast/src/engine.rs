//! What the two dialect walkers hand to the shared extract and merge code.

use crate::xml::Tree;
use bilingua_model::{CodeData, Inline, Limits, Note, Origin, SegState};
use std::collections::BTreeMap;

pub enum Status {
    /// A segment the pipeline translates.
    Open,
    /// Finished work somebody confirmed: loaded as its target. A translator's confirmation (`"confirmed"`) stays
    /// editable and is only rewritten when its target changes; reviewer sign-off (`"final"`, `"approved"`) is
    /// read-only.
    Confirmed(&'static str),
    /// Locked, or not for translation.
    Locked(&'static str),
}

/// Where a translation is written: replace `text[start..end]` with `pre + content + post`.
pub struct Region {
    pub start: usize,
    pub end: usize,
    pub pre: String,
    pub post: String,
}

pub struct Item {
    pub key: usize,
    pub status: Status,
    pub lead: String,
    pub trail: String,
    pub source: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
    pub target: Vec<Inline>,
    pub state: SegState,
    pub origin: Option<Origin>,
    pub region: Option<Region>,
    /// Hidden extras of the unit (a mirrored `seg-source` template).
    pub extra: Vec<(&'static str, String)>,
    pub ctx_key: Option<String>,
    pub style: Option<String>,
    pub notes: Vec<Note>,
}

pub struct Walked {
    pub items: Vec<Item>,
    pub source_lang: String,
    pub target_lang: String,
}

pub struct Ctx<'a> {
    pub text: &'a str,
    pub tree: &'a Tree,
    pub limits: &'a Limits,
    /// Tokenise segments that are not for translation (they are listed).
    pub want_protected: bool,
}
