//! Bilingua document model: what every file parser extracts and what it writes back.
//!
//! A parser (`Filter`) turns a source file into a [`Document`]: translatable [`Unit`]s (a paragraph, a cell, a
//! string) and a [`Skeleton`] that holds everything else byte for byte. Each unit holds one or more
//! [`Segment`]s (one before segmentation; sentences after `bilingua-seg` splits it). Formatting inside the text
//! is kept as inline [`Inline::Tag`] codes whose original markup lives in [`Unit::codes`], so a writer can put
//! it back exactly.
//!
//! JSON shape: `Inline` and `TagKind` serialise exactly like `catcore::Inline` and `catcore::TagKind`
//! (`{"text": ".."}` / `{"tag": {"id", "kind": "open|close|standalone", "display"}}`), so one editor UI shows
//! bilingual files (catcore) and source files (Bilingua) the same way. Every field added since is optional and
//! left out of the JSON when empty: a tag's `free` flag, a segment's `origin`, a unit's `notes` and `lock_reason`,
//! a skeleton's `parser`.
//!
//! Bilingual files (XLIFF and the like) use the same model: a segment may carry a target, a state and an origin
//! (with a match percentage), and a unit may carry notes and a lock reason. Monolingual formats leave them empty.
//!
//! [`locos`] converts inline codes to and from the text tokens Localization OS uses, one token per tag.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod b64;
pub mod codepage;
pub mod enc;
pub mod garble;
pub mod guard;
pub mod locos;
mod pytables;
pub mod pytext;

pub use encoding_rs;
pub use guard::{LimitError, LimitKind, Limits};
pub use pytext::{py_is_word_char, py_is_word_start_char, py_isdecimal_char, py_isalnum_char, py_isalpha_char, py_isspace_char, py_has_alnum, py_has_alpha, py_isalnum, py_isalpha, py_isspace, py_lstrip, py_rstrip, py_strip};

/// What a builder takes for an optional text field: `"de-DE"`, a `String`, or an `Option` of either (`None`
/// clears the field).
pub trait IntoOptionString {
    fn into_option_string(self) -> Option<String>;
}

impl IntoOptionString for &str {
    fn into_option_string(self) -> Option<String> {
        Some(self.to_string())
    }
}
impl IntoOptionString for String {
    fn into_option_string(self) -> Option<String> {
        Some(self)
    }
}
impl IntoOptionString for &String {
    fn into_option_string(self) -> Option<String> {
        Some(self.clone())
    }
}
impl IntoOptionString for Option<String> {
    fn into_option_string(self) -> Option<String> {
        self
    }
}
impl IntoOptionString for Option<&str> {
    fn into_option_string(self) -> Option<String> {
        self.map(str::to_string)
    }
}
impl IntoOptionString for Option<&String> {
    fn into_option_string(self) -> Option<String> {
        self.cloned()
    }
}

/// One piece of segment content, in logical order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Inline {
    Text(String),
    /// An inline code. `free` is true for a standalone code that names its argument (`%1$s`, `{name}`, `{0}`, an
    /// XLIFF `<x id>` or `<ph id>`): a translation may move it, because the writer finds it again by `id`. It is
    /// false for markup and for unnumbered placeholders (`%s`), whose order is the argument order. Only a
    /// standalone code can be free.
    Tag {
        id: String,
        kind: TagKind,
        display: String,
        #[serde(default, skip_serializing_if = "is_false")]
        free: bool,
    },
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Inline {
    pub fn text(s: impl Into<String>) -> Inline {
        Inline::Text(s.into())
    }
    pub fn tag(id: impl Into<String>, kind: TagKind, display: impl Into<String>) -> Inline {
        Inline::Tag { id: id.into(), kind, display: display.into(), free: false }
    }
    /// A standalone code that may move freely in the target (see [`Inline::Tag`]).
    pub fn free_tag(id: impl Into<String>, display: impl Into<String>) -> Inline {
        Inline::Tag { id: id.into(), kind: TagKind::Standalone, display: display.into(), free: true }
    }
    /// True for a free-order standalone code.
    pub fn is_free(&self) -> bool {
        matches!(self, Inline::Tag { free: true, .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TagKind {
    Open,
    Close,
    Standalone,
}

/// The original markup of one inline code, kept so the writer can reproduce it.
/// A paired code fills `open` and `close`; a standalone code fills `standalone`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeData {
    pub open: Option<String>,
    pub close: Option<String>,
    pub standalone: Option<String>,
    /// Human label for the tag chip, e.g. "bold", "link".
    pub label: Option<String>,
}

/// Where a segment is in its life, from untouched to signed off. Monolingual files only use `New`, `Draft`,
/// `Translated` and `Confirmed`; bilingual files and review workflows use the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SegState {
    /// No target yet.
    #[default]
    #[serde(rename = "new")]
    New,
    /// A target exists but nobody accepted it (machine output, TM match, work in progress).
    #[serde(rename = "draft")]
    Draft,
    /// The translator accepted the target.
    #[serde(rename = "translated")]
    Translated,
    /// Confirmed or approved by the translator (XLIFF 2 `final`, Trados `ApprovedTranslation`).
    #[serde(rename = "confirmed")]
    Confirmed,
    /// A reviewer checked it.
    #[serde(rename = "reviewed")]
    Reviewed,
    /// Final approval (XLIFF 1.2 `signed-off`).
    #[serde(rename = "signed-off")]
    SignedOff,
    /// A reviewer sent it back; it needs work again.
    #[serde(rename = "rejected")]
    Rejected,
    /// Frozen: nobody edits it (context match, project lock).
    #[serde(rename = "locked")]
    Locked,
}

impl SegState {
    /// Position in the life cycle, used when segments are joined (the joined segment is as far along as its
    /// least advanced part). `Rejected` ranks just above `New`: it has a target but needs work.
    pub fn rank(self) -> u8 {
        match self {
            SegState::New => 0,
            SegState::Rejected => 1,
            SegState::Draft => 2,
            SegState::Translated => 3,
            SegState::Confirmed => 4,
            SegState::Reviewed => 5,
            SegState::SignedOff => 6,
            SegState::Locked => 7,
        }
    }

    /// Inverse of [`SegState::rank`]; anything above the top rank is `Locked`.
    pub fn from_rank(r: u8) -> SegState {
        match r {
            0 => SegState::New,
            1 => SegState::Rejected,
            2 => SegState::Draft,
            3 => SegState::Translated,
            4 => SegState::Confirmed,
            5 => SegState::Reviewed,
            6 => SegState::SignedOff,
            _ => SegState::Locked,
        }
    }

    /// True when a person accepted the target (translated or further along).
    pub fn is_accepted(self) -> bool {
        self.rank() >= SegState::Translated.rank()
    }
}

/// Where a segment's target came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OriginKind {
    /// Translation memory.
    Tm,
    /// Machine translation engine.
    Mt,
    /// Language model translation or post-edit.
    Ai,
    /// Typed by a person.
    Human,
    /// Anything else; `Origin::name` says more. An unknown kind in a JSON file also reads as `Other`.
    #[default]
    #[serde(other)]
    Other,
}

/// How a target came to be, with the match percentage when it came from memory.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub kind: OriginKind,
    /// Engine, memory or person: "Acme TM", "engine-2", "ana". Free text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Match percentage as the file states it (0 to 100; some tools write 101 for a context match).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_pct: Option<u8>,
}

impl Origin {
    pub fn new(kind: OriginKind) -> Origin {
        Origin { kind, name: None, match_pct: None }
    }
    pub fn with_match(mut self, pct: u8) -> Origin {
        self.match_pct = Some(pct);
        self
    }
    pub fn with_name(mut self, name: impl Into<String>) -> Origin {
        self.name = Some(name.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Segment {
    /// Unique within the document: `<unit id>` when the unit has one segment, else `<unit id>#<n>` (1-based).
    pub id: String,
    pub source: Vec<Inline>,
    /// Empty = untranslated.
    #[serde(default)]
    pub target: Vec<Inline>,
    #[serde(default)]
    pub state: SegState,
    /// Where the target came from. `None` for monolingual files and for targets nobody labelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
}

impl Segment {
    /// An untranslated segment (no target, state `New`, no origin).
    pub fn new(id: impl Into<String>, source: Vec<Inline>) -> Segment {
        Segment { id: id.into(), source, target: vec![], state: SegState::New, origin: None }
    }
    /// Sets the target as it is (no tag check: use [`Document::set_target`] for edits).
    pub fn with_target(mut self, target: Vec<Inline>) -> Segment {
        self.target = target;
        self
    }
    pub fn with_state(mut self, state: SegState) -> Segment {
        self.state = state;
        self
    }
    pub fn with_origin(mut self, origin: impl Into<Option<Origin>>) -> Segment {
        self.origin = origin.into();
        self
    }
}

/// A comment on a unit: a translator query, a developer instruction, a reviewer remark.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub text: String,
    /// Kind as the file has it: "comment", "instruction", "developer", "reviewer", ... Free text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// What helps a translator, an AI lane or a preview understand a unit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitContext {
    /// What the unit is: "heading", "paragraph", "list-item", "cell", "button", "title", "alt", "key",
    /// "subtitle", ... Free text; parsers use the most specific word they know.
    pub kind: Option<String>,
    /// Style or element name as the file has it, e.g. "Heading 2", "h2", "td".
    pub style: Option<String>,
    /// Resource key or path for key/value formats (JSON pointer, PO msgctxt + msgid, subtitle number).
    pub key: Option<String>,
    /// Nearest heading text above the unit, when the format has headings.
    pub heading: Option<String>,
    /// Location in the original, readable: "word/document.xml paragraph 12", "/html/body/main/p[3]".
    pub location: Option<String>,
    /// Maximum length in characters, when the file states one.
    pub max_len: Option<usize>,
    /// Comments or notes attached in the file (translator comments, PO comments, DOCX comments).
    #[serde(default)]
    pub notes: Vec<String>,
    /// Subtitle timing and similar extras the editor may show.
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "UnitRepr")]
#[non_exhaustive]
pub struct Unit {
    /// Unique within the document, stable for the same input file.
    pub id: String,
    pub segments: Vec<Segment>,
    /// Text between segments as it appeared in the source (spaces after a full stop). Always
    /// `segments.len() - 1` entries; the writer joins segment targets with these.
    #[serde(default)]
    pub separators: Vec<String>,
    /// Original markup per inline code id.
    #[serde(default)]
    pub codes: BTreeMap<String, CodeData>,
    #[serde(default)]
    pub context: UnitContext,
    /// False for content shown for context only (locked).
    #[serde(default = "yes")]
    pub translate: bool,
    /// Comments attached to the unit in a bilingual file. (Source files keep theirs in `context.notes`.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
    /// Why the unit is locked ("context match", "locked by the project"). Free text, usually with
    /// `translate == false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_reason: Option<String>,
    /// What the parser needs to write the unit back and nobody else sees: the value as written in the file, its
    /// white space, a layout flag. Keyed by a short name the parser picks ("raw", "lead"). It is not an inline code,
    /// so editors, the LocOS token map and tag checks never see it. Left out of the JSON when empty. (Parsers used
    /// to keep this in `codes` under `~` names; such codes in an old JSON document are moved here when it is read.)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hidden: BTreeMap<String, String>,
}

fn yes() -> bool {
    true
}

/// The JSON form of a [`Unit`] as read: the same fields, with old `~` codes moved into `hidden`.
#[derive(Deserialize)]
struct UnitRepr {
    id: String,
    segments: Vec<Segment>,
    #[serde(default)]
    separators: Vec<String>,
    #[serde(default)]
    codes: BTreeMap<String, CodeData>,
    #[serde(default)]
    context: UnitContext,
    #[serde(default = "yes")]
    translate: bool,
    #[serde(default)]
    notes: Vec<Note>,
    #[serde(default)]
    lock_reason: Option<String>,
    #[serde(default)]
    hidden: BTreeMap<String, String>,
}

impl From<UnitRepr> for Unit {
    fn from(r: UnitRepr) -> Unit {
        let mut hidden = r.hidden;
        let mut codes = BTreeMap::new();
        for (k, c) in r.codes {
            match k.strip_prefix('~') {
                // an old parser's hidden value: `standalone` (or, for a flag, `label`) held it
                Some(name) if c.open.is_none() && c.close.is_none() => {
                    hidden.entry(name.to_string()).or_insert_with(|| c.standalone.or(c.label).unwrap_or_default());
                }
                _ => {
                    codes.insert(k, c);
                }
            }
        }
        Unit {
            id: r.id,
            segments: r.segments,
            separators: r.separators,
            codes,
            context: r.context,
            translate: r.translate,
            notes: r.notes,
            lock_reason: r.lock_reason,
            hidden,
        }
    }
}

impl Unit {
    /// A unit with one segment holding `source`.
    pub fn new(id: impl Into<String>, source: Vec<Inline>) -> Unit {
        let id = id.into();
        let seg = Segment::new(id.clone(), source);
        Unit::from_segments(id, vec![seg])
    }

    /// A unit made of the given segments (their ids are kept as they are). Separators start empty, one per gap.
    pub fn from_segments(id: impl Into<String>, segments: Vec<Segment>) -> Unit {
        Unit {
            separators: vec![String::new(); segments.len().saturating_sub(1)],
            segments,
            id: id.into(),
            codes: BTreeMap::new(),
            context: UnitContext::default(),
            translate: true,
            notes: vec![],
            lock_reason: None,
            hidden: BTreeMap::new(),
        }
    }
    /// A hidden value (see [`Unit::hidden`]).
    pub fn hidden(&self, key: &str) -> Option<&str> {
        self.hidden.get(key).map(String::as_str)
    }
    /// Sets a hidden value.
    pub fn set_hidden(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.hidden.insert(key.into(), value.into());
    }
    /// True when the hidden value is set (a flag).
    pub fn has_hidden(&self, key: &str) -> bool {
        self.hidden.contains_key(key)
    }
    pub fn with_hidden(mut self, key: impl Into<String>, value: impl Into<String>) -> Unit {
        self.set_hidden(key, value);
        self
    }
    pub fn with_separators(mut self, separators: Vec<String>) -> Unit {
        self.separators = separators;
        self
    }
    pub fn with_codes(mut self, codes: BTreeMap<String, CodeData>) -> Unit {
        self.codes = codes;
        self
    }
    /// Adds (or replaces) the original markup of one inline code.
    pub fn with_code(mut self, id: impl Into<String>, code: CodeData) -> Unit {
        self.codes.insert(id.into(), code);
        self
    }
    pub fn with_context(mut self, context: UnitContext) -> Unit {
        self.context = context;
        self
    }
    pub fn with_translate(mut self, translate: bool) -> Unit {
        self.translate = translate;
        self
    }
    pub fn with_notes(mut self, notes: Vec<Note>) -> Unit {
        self.notes = notes;
        self
    }
    pub fn with_lock_reason(mut self, reason: impl IntoOptionString) -> Unit {
        self.lock_reason = reason.into_option_string();
        self
    }

    /// The whole unit's source, segments joined with their separators.
    pub fn source(&self) -> Vec<Inline> {
        self.join(|s| &s.source)
    }

    /// The text to write for this unit: each segment's target, or its source when the target is empty
    /// (an untranslated segment is written in the source language, as every CAT tool does).
    pub fn output(&self) -> Vec<Inline> {
        self.join(|s| if s.target.is_empty() { &s.source } else { &s.target })
    }

    fn join<'a>(&'a self, pick: impl Fn(&'a Segment) -> &'a Vec<Inline>) -> Vec<Inline> {
        let mut out = Vec::new();
        for (i, seg) in self.segments.iter().enumerate() {
            if i > 0 {
                if let Some(sep) = self.separators.get(i - 1) {
                    if !sep.is_empty() {
                        out.push(Inline::Text(sep.clone()));
                    }
                }
            }
            out.extend(pick(seg).iter().cloned());
        }
        merge_text(out)
    }
}

/// Joins neighbouring `Text` pieces.
pub fn merge_text(v: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::with_capacity(v.len());
    for i in v {
        match (out.last_mut(), i) {
            (Some(Inline::Text(a)), Inline::Text(b)) => a.push_str(&b),
            (_, Inline::Text(t)) if t.is_empty() => {}
            (_, x) => out.push(x),
        }
    }
    out
}

/// Plain text with the tags removed.
pub fn plain_text(v: &[Inline]) -> String {
    v.iter()
        .filter_map(|i| match i {
            Inline::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect()
}

/// One piece of a text skeleton: literal output, or the place where a unit's text goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkelPart {
    Raw(String),
    /// Index into `Document::units`. The parser decides how the unit's inlines are rendered (escaping, codes).
    Unit(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ZipData {
    /// Copied unchanged. In JSON this is a base64 string; the old array-of-integers form still reads.
    Raw(#[serde(with = "b64")] Vec<u8>),
    /// A text entry rebuilt from parts.
    Parts(Vec<SkelPart>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZipEntry {
    pub name: String,
    pub data: ZipData,
}

/// Which parser made a skeleton: the crate name, its version (for information) and the skeleton schema number.
/// The schema is an integer each format crate bumps only when its skeleton layout changes, so a new crate release
/// that did not touch the layout still merges old skeletons (see [`check_parser`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParserInfo {
    /// The producing crate, e.g. "bilingua-docx".
    pub name: String,
    /// Its semver version, e.g. "0.1.0". Informational only; never compared.
    pub version: String,
    /// The skeleton schema number. Absent in stamps written before schemas existed, which read as schema 1.
    #[serde(default = "legacy_schema")]
    pub schema: u32,
}

fn legacy_schema() -> u32 {
    1
}

impl ParserInfo {
    pub fn new(name: impl Into<String>, version: impl Into<String>, schema: u32) -> ParserInfo {
        ParserInfo { name: name.into(), version: version.into(), schema }
    }

    /// True when a skeleton made by `self` can be read by `current`: same crate and a schema no newer than the
    /// current one.
    pub fn compatible_with(&self, current: &ParserInfo) -> bool {
        self.name == current.name && self.schema <= current.schema
    }
}

/// The producing parser of the running crate: `parser_info!()` expands to a [`ParserInfo`] with the calling
/// crate's name, version and `crate::SKELETON_SCHEMA`. Every format crate defines that constant
/// (`pub const SKELETON_SCHEMA: u32 = 1;`) and bumps it only when its skeleton changes.
/// `parser_info!(n)` stamps an explicit schema instead.
// `crate::SKELETON_SCHEMA` must resolve in the calling crate: that is the point.
#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! parser_info {
    () => {
        $crate::parser_info!(crate::SKELETON_SCHEMA)
    };
    ($schema:expr) => {
        $crate::ParserInfo::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), $schema)
    };
}

/// Refuses a document whose skeleton came from another crate or from a newer schema than `current`. Older
/// schemas and skeletons with no record (written before stamps existed) are accepted; a crate that stops
/// reading some old schema uses [`check_parser_min`].
pub fn check_parser(doc: &Document, current: &ParserInfo) -> Result<(), Error> {
    check_parser_min(doc, current, 1)
}

/// Like [`check_parser`], but also refuses schemas older than `oldest`, the oldest one the crate still reads.
pub fn check_parser_min(doc: &Document, current: &ParserInfo, oldest: u32) -> Result<(), Error> {
    match doc.skeleton.parser() {
        Some(found) if !found.compatible_with(current) || found.schema < oldest => Err(Error::ParserVersion {
            found: format!("{} {} (skeleton schema {})", found.name, found.version, found.schema),
            expected: if found.name != current.name {
                current.name.clone()
            } else if found.schema > current.schema {
                format!("{} {} (reads skeleton schema {} and older)", current.name, current.version, current.schema)
            } else {
                format!("{} {} (reads skeleton schema {} to {})", current.name, current.version, oldest, current.schema)
            },
        }),
        _ => Ok(()),
    }
}

/// Everything in the file that is not translatable text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Skeleton {
    /// A text file: parts in order. `encoding` is what the writer must produce ("utf-8", "utf-8-bom", ...).
    Text {
        parts: Vec<SkelPart>,
        encoding: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parser: Option<ParserInfo>,
    },
    /// A zip container (DOCX, XLSX, PPTX, ODF, IDML): entries in their original order.
    Zip {
        entries: Vec<ZipEntry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parser: Option<ParserInfo>,
    },
    /// A binary file kept whole (a compiled MO catalog): the writer rebuilds the file from these bytes and the
    /// units. In JSON the bytes are a base64 string.
    Binary {
        #[serde(with = "b64")]
        data: Vec<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parser: Option<ParserInfo>,
    },
}

impl Default for Skeleton {
    /// An empty UTF-8 text skeleton.
    fn default() -> Self {
        Skeleton::Text { parts: Vec::new(), encoding: "utf-8".into(), parser: None }
    }
}

impl Skeleton {
    /// A text skeleton stamped with the producing parser.
    pub fn text(parts: Vec<SkelPart>, encoding: impl Into<String>, parser: ParserInfo) -> Skeleton {
        Skeleton::Text { parts, encoding: encoding.into(), parser: Some(parser) }
    }
    /// A zip skeleton stamped with the producing parser.
    pub fn zip(entries: Vec<ZipEntry>, parser: ParserInfo) -> Skeleton {
        Skeleton::Zip { entries, parser: Some(parser) }
    }
    /// A binary skeleton stamped with the producing parser.
    pub fn binary(data: Vec<u8>, parser: ParserInfo) -> Skeleton {
        Skeleton::Binary { data, parser: Some(parser) }
    }
    /// The parser that made this skeleton, if recorded.
    pub fn parser(&self) -> Option<&ParserInfo> {
        match self {
            Skeleton::Text { parser, .. } | Skeleton::Zip { parser, .. } | Skeleton::Binary { parser, .. } => parser.as_ref(),
        }
    }
}

/// How finely units are split into segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Granularity {
    /// SRX sentence segmentation (the classic CAT default).
    #[default]
    Sentence,
    /// One segment per unit (paragraph, cell, string).
    Paragraph,
    /// One segment per unit, flagged so AI lanes translate the whole document in one pass. The editor still
    /// edits per unit.
    Document,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Document {
    /// Parser id, e.g. "html", "docx", "json".
    pub format: String,
    /// Original file name, when known.
    pub name: Option<String>,
    pub source_lang: Option<String>,
    pub target_lang: Option<String>,
    pub granularity: Granularity,
    pub units: Vec<Unit>,
    pub skeleton: Skeleton,
    /// Things the reader noticed but did not refuse the file for (text that looks garbled in the code page it was
    /// read with, ...). Left out of the JSON when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
}

/// A notice about a file that was read anyway: it never stops a merge. `kind` is a stable word for programs
/// ("text_encoding"); `message` is plain text safe to show the person who uploaded the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub kind: String,
    /// Which part of the file the notice is about ("text encoding", an entry name, ...).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub part: String,
    pub message: String,
}

impl Warning {
    pub fn new(kind: impl Into<String>, part: impl Into<String>, message: impl Into<String>) -> Warning {
        Warning { kind: kind.into(), part: part.into(), message: message.into() }
    }
}

impl Document {
    /// A document of `format` (the filter id) with its units and skeleton; no name, no languages, sentence
    /// granularity, no warnings.
    pub fn new(format: impl Into<String>, units: Vec<Unit>, skeleton: Skeleton) -> Document {
        Document { format: format.into(), name: None, source_lang: None, target_lang: None, granularity: Granularity::default(), units, skeleton, warnings: vec![] }
    }
    /// Takes the languages and the granularity from the options the file was opened with (what every parser does).
    pub fn with_options(mut self, opts: &ExtractOptions) -> Document {
        self.source_lang = opts.source_lang.clone();
        self.target_lang = opts.target_lang.clone();
        self.granularity = opts.granularity;
        self
    }
    pub fn with_name(mut self, name: impl IntoOptionString) -> Document {
        self.name = name.into_option_string();
        self
    }
    pub fn with_source_lang(mut self, lang: impl IntoOptionString) -> Document {
        self.source_lang = lang.into_option_string();
        self
    }
    pub fn with_target_lang(mut self, lang: impl IntoOptionString) -> Document {
        self.target_lang = lang.into_option_string();
        self
    }
    pub fn with_granularity(mut self, granularity: Granularity) -> Document {
        self.granularity = granularity;
        self
    }
    pub fn with_warnings(mut self, warnings: Vec<Warning>) -> Document {
        self.warnings = warnings;
        self
    }

    pub fn segments(&self) -> impl Iterator<Item = (&Unit, &Segment)> {
        self.units.iter().flat_map(|u| u.segments.iter().map(move |s| (u, s)))
    }

    pub fn segment_count(&self) -> usize {
        self.units.iter().map(|u| u.segments.len()).sum()
    }

    pub fn find(&self, seg_id: &str) -> Option<(&Unit, &Segment)> {
        self.segments().find(|(_, s)| s.id == seg_id)
    }

    /// Sets a segment's target. Rejects unknown segments, untranslatable units, and tags the source lacks or
    /// repeats. A missing tag is allowed (the writer leaves it out), as in catcore.
    ///
    /// This looks the segment up by a scan of the document; for many segments use [`Document::set_targets`].
    pub fn set_target(&mut self, seg_id: &str, target: Vec<Inline>) -> Result<(), Error> {
        let at = self
            .units
            .iter()
            .enumerate()
            .find_map(|(ui, u)| u.segments.iter().position(|s| s.id == seg_id).map(|si| (ui, si)))
            .ok_or_else(|| Error::UnknownSegment(seg_id.to_string()))?;
        self.apply_target(at, seg_id, target)
    }

    /// Sets many targets in one pass: one id index is built for the whole call, so 100 000 segments take
    /// milliseconds where 100 000 [`Document::set_target`] calls scan the document each time. Items are applied in
    /// order with the same rules as `set_target`; the first error stops the call and is returned, items before it
    /// stay applied. When an id appears more than once, the segment is set by each item in turn (the last wins).
    pub fn set_targets<I, S>(&mut self, items: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = (S, Vec<Inline>)>,
        S: AsRef<str>,
    {
        let index = self.segment_index();
        for (id, target) in items {
            let id = id.as_ref();
            let at = *index.get(id).ok_or_else(|| Error::UnknownSegment(id.to_string()))?;
            self.apply_target(at, id, target)?;
        }
        Ok(())
    }

    /// Sets many states in one pass (see [`Document::set_targets`]).
    pub fn set_states<I, S>(&mut self, items: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = (S, SegState)>,
        S: AsRef<str>,
    {
        let index = self.segment_index();
        for (id, state) in items {
            let id = id.as_ref();
            let (ui, si) = *index.get(id).ok_or_else(|| Error::UnknownSegment(id.to_string()))?;
            self.units[ui].segments[si].state = state;
        }
        Ok(())
    }

    /// Segment id to (unit index, segment index); the first segment with an id wins, as in `set_target`.
    fn segment_index(&self) -> std::collections::HashMap<String, (usize, usize)> {
        let mut m = std::collections::HashMap::with_capacity(self.segment_count());
        for (ui, u) in self.units.iter().enumerate() {
            for (si, s) in u.segments.iter().enumerate() {
                m.entry(s.id.clone()).or_insert((ui, si));
            }
        }
        m
    }

    fn apply_target(&mut self, (ui, si): (usize, usize), seg_id: &str, target: Vec<Inline>) -> Result<(), Error> {
        let unit = &mut self.units[ui];
        if !unit.translate {
            return Err(Error::Locked(seg_id.to_string()));
        }
        let seg = &mut unit.segments[si];
        check_tags(&seg.source, &target)?;
        // The free flag belongs to the source code: a target tag takes it from the source tag with its id, so a
        // caller (an editor, a LocOS adapter) that does not know the flag cannot drop or invent it.
        let target = merge_text(
            target
                .into_iter()
                .map(|i| match i {
                    Inline::Tag { id, kind, display, .. } => {
                        let free = kind == TagKind::Standalone
                            && seg.source.iter().any(|s| matches!(s, Inline::Tag { id: sid, kind: TagKind::Standalone, free: true, .. } if *sid == id));
                        Inline::Tag { id, kind, display, free }
                    }
                    t => t,
                })
                .collect(),
        );
        let empty = plain_text(&target).trim().is_empty() && !target.iter().any(|i| matches!(i, Inline::Tag { .. }));
        seg.target = if empty { vec![] } else { target };
        seg.state = if seg.target.is_empty() { SegState::New } else { SegState::Draft };
        Ok(())
    }

    pub fn set_state(&mut self, seg_id: &str, state: SegState) -> Result<(), Error> {
        let seg = self
            .units
            .iter_mut()
            .flat_map(|u| u.segments.iter_mut())
            .find(|s| s.id == seg_id)
            .ok_or_else(|| Error::UnknownSegment(seg_id.to_string()))?;
        seg.state = state;
        Ok(())
    }
}

fn check_tags(source: &[Inline], target: &[Inline]) -> Result<(), Error> {
    let mut allowed: BTreeMap<(&str, TagKind), usize> = BTreeMap::new();
    for i in source {
        if let Inline::Tag { id, kind, .. } = i {
            *allowed.entry((id.as_str(), *kind)).or_default() += 1;
        }
    }
    let mut used: BTreeMap<(&str, TagKind), usize> = BTreeMap::new();
    for i in target {
        if let Inline::Tag { id, kind, .. } = i {
            let n = used.entry((id.as_str(), *kind)).or_default();
            *n += 1;
            match allowed.get(&(id.as_str(), *kind)) {
                None => return Err(Error::InventedTag(id.clone())),
                Some(max) if *n > *max => return Err(Error::DuplicatedTag(id.clone())),
                _ => {}
            }
        }
    }
    Ok(())
}

/// Options for [`Filter::extract`]. Build them with [`ExtractOptions::new`] and the `with_*` methods
/// (`ExtractOptions::new().with_target_lang("de-DE")`); new options are added without breaking callers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ExtractOptions {
    pub source_lang: Option<String>,
    pub target_lang: Option<String>,
    /// Parsers always extract one segment per unit; `bilingua-seg` applies the granularity afterwards.
    pub granularity: Granularity,
    /// Extract text the format marks as not translatable as locked units (for context), instead of skipping it.
    pub include_locked: bool,
    /// Safety limits for this call; `None` uses [`Limits::default`] (Localization OS's numbers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<Limits>,
    /// Options only one format knows, as `"<format>.<name>"` to a text value (`"ftl.translate_terms"` to
    /// `"true"`). Each filter declares its keys and their kind ([`Filter::format_option_keys`]); the `bilingua`
    /// facade refuses a key no filter declares, or a value of the wrong kind, with [`Error::FormatOption`].
    /// Booleans are `true`/`false`/`1`/`0`/`yes`/`no`/`on`/`off`; integers are decimal.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub format_options: BTreeMap<String, String>,
}

impl ExtractOptions {
    /// The defaults: no languages, sentence granularity, locked text skipped, Localization OS's limits.
    pub fn new() -> ExtractOptions {
        ExtractOptions::default()
    }
    pub fn with_source_lang(mut self, lang: impl IntoOptionString) -> ExtractOptions {
        self.source_lang = lang.into_option_string();
        self
    }
    pub fn with_target_lang(mut self, lang: impl IntoOptionString) -> ExtractOptions {
        self.target_lang = lang.into_option_string();
        self
    }
    pub fn with_granularity(mut self, granularity: Granularity) -> ExtractOptions {
        self.granularity = granularity;
        self
    }
    pub fn with_include_locked(mut self, include_locked: bool) -> ExtractOptions {
        self.include_locked = include_locked;
        self
    }
    /// Limits for this call (`Limits` or `Option<Limits>`; `None` is the defaults).
    pub fn with_limits(mut self, limits: impl Into<Option<Limits>>) -> ExtractOptions {
        self.limits = limits.into();
        self
    }
    /// Sets one format option (`"<format>.<name>"`); the value is text (`true`, `42`, `name=Name`).
    pub fn with_format_option(mut self, key: impl Into<String>, value: impl ToString) -> ExtractOptions {
        self.format_options.insert(key.into(), value.to_string());
        self
    }
    /// Replaces all format options.
    pub fn with_format_options(mut self, options: BTreeMap<String, String>) -> ExtractOptions {
        self.format_options = options;
        self
    }

    /// A format option as text, if set.
    pub fn format_option(&self, key: &str) -> Option<&str> {
        self.format_options.get(key).map(String::as_str)
    }

    /// A format option as a boolean (`true`, `1`, `yes`, `on` are true; `false`, `0`, `no`, `off` are false; any
    /// other text or no entry gives `None`).
    pub fn format_flag(&self, key: &str) -> Option<bool> {
        match self.format_option(key)?.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        }
    }

    /// A format option as an integer (`None` when unset or not a decimal integer).
    pub fn format_int(&self, key: &str) -> Option<i64> {
        self.format_option(key)?.trim().parse().ok()
    }

    /// The limits in force: the caller's, or the defaults.
    pub fn limits(&self) -> Limits {
        self.limits.unwrap_or_default()
    }
}

/// The kind of value a format option takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OptionKind {
    /// `true`/`false` (also `1`/`0`, `yes`/`no`, `on`/`off`).
    Bool,
    /// A decimal integer.
    Int,
    /// Any text.
    Text,
}

/// One option a format understands in `ExtractOptions::format_options`: its full key (`"csv.columns"`), the kind
/// of value, and a line of help for tools that list the options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct FormatOptionKey {
    pub key: &'static str,
    pub kind: OptionKind,
    pub help: &'static str,
}

impl FormatOptionKey {
    pub const fn new(key: &'static str, kind: OptionKind, help: &'static str) -> FormatOptionKey {
        FormatOptionKey { key, kind, help }
    }
    pub const fn bool(key: &'static str, help: &'static str) -> FormatOptionKey {
        FormatOptionKey::new(key, OptionKind::Bool, help)
    }
    pub const fn int(key: &'static str, help: &'static str) -> FormatOptionKey {
        FormatOptionKey::new(key, OptionKind::Int, help)
    }
    pub const fn text(key: &'static str, help: &'static str) -> FormatOptionKey {
        FormatOptionKey::new(key, OptionKind::Text, help)
    }

    /// Refuses a value that is not of this key's kind.
    pub fn check(&self, value: &str) -> Result<(), Error> {
        let v = value.trim();
        let ok = match self.kind {
            OptionKind::Bool => matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on" | "false" | "0" | "no" | "off"),
            OptionKind::Int => v.parse::<i64>().is_ok(),
            OptionKind::Text => true,
        };
        if ok {
            Ok(())
        } else {
            let want = match self.kind {
                OptionKind::Bool => "true or false",
                OptionKind::Int => "an integer",
                OptionKind::Text => "text",
            };
            Err(Error::FormatOption { key: self.key.to_string(), msg: format!("the value {value:?} is not {want}") })
        }
    }
}

/// Checks every `format_options` entry against the keys the filters declare: an unknown key or a value of the
/// wrong kind is an [`Error::FormatOption`]. The `bilingua` facade calls this with every filter's keys, so an
/// option meant for another format (the adapter sends `docx.translate_comments` with every file) is accepted.
pub fn check_format_options(opts: &ExtractOptions, known: &[FormatOptionKey]) -> Result<(), Error> {
    for (k, v) in &opts.format_options {
        match known.iter().find(|f| f.key == k) {
            Some(f) => f.check(v)?,
            None => {
                let family = k.split('.').next().unwrap_or("");
                let same: Vec<&str> = known.iter().filter(|f| f.key.split('.').next() == Some(family)).map(|f| f.key).collect();
                let msg = if same.is_empty() { "no format has this option".to_string() } else { format!("unknown option; {family} knows {}", same.join(", ")) };
                return Err(Error::FormatOption { key: k.clone(), msg });
            }
        }
    }
    Ok(())
}

/// How a merge writes text files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OutputEncoding {
    /// The file's own encoding, BOM and line endings (the default; an untouched merge is byte-identical).
    #[default]
    #[serde(rename = "keep")]
    Keep,
    /// Always UTF-8 (Localization OS behaviour). An XML, HTML or PO charset declaration is rewritten to say UTF-8,
    /// so the file stays truthful; a UTF-8 BOM the file had is kept, a UTF-16 BOM is dropped.
    #[serde(rename = "utf-8", alias = "utf8")]
    Utf8,
}

/// Options for [`Filter::merge_with`]. The default is what [`Filter::merge`] does. They live here, once, and
/// every format that stamps a language, fills plurals or writes text honours them:
///
/// - `output_encoding`: [`OutputEncoding::Keep`] or [`OutputEncoding::Utf8`] (through `enc::encode_text_with`, so
///   every text format that keeps its encoding in the skeleton gets it).
/// - `stamp_target_always`: by default a merge in which nothing is translated returns the original bytes (no target
///   language stamp, no plural fill, no format conversion). With `true` the target language is stamped and
///   missing plural forms are filled even then, as Localization OS does. It only has an effect when
///   `Document::target_lang` is set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct MergeOptions {
    pub output_encoding: OutputEncoding,
    pub stamp_target_always: bool,
}

impl MergeOptions {
    /// The defaults: keep the file's encoding, no stamp when nothing is translated (what [`Filter::merge`] does).
    pub fn new() -> MergeOptions {
        MergeOptions::default()
    }
    pub fn with_output_encoding(mut self, output_encoding: OutputEncoding) -> MergeOptions {
        self.output_encoding = output_encoding;
        self
    }
    pub fn with_stamp_target_always(mut self, stamp_target_always: bool) -> MergeOptions {
        self.stamp_target_always = stamp_target_always;
        self
    }
    /// The Localization OS behaviour: UTF-8 out, stamp and fill always.
    pub fn locos() -> MergeOptions {
        MergeOptions { output_encoding: OutputEncoding::Utf8, stamp_target_always: true }
    }

    /// True when the document has at least one segment with a target.
    pub fn any_translated(doc: &Document) -> bool {
        doc.segments().any(|(_, s)| !s.target.is_empty())
    }

    /// Should a format stamp the target language and fill plurals for this document? `target_lang` must be set (and
    /// not blank) and something translated, or `stamp_target_always`. "Translated" is [`MergeOptions::any_translated`];
    /// a format with its own notion of a change uses [`MergeOptions::stamp_lang`].
    pub fn should_stamp(&self, doc: &Document) -> bool {
        self.stamp_lang(doc, Self::any_translated(doc)).is_some()
    }

    /// The one stamp rule every format uses: the document's `target_lang` (as written, never blank) when
    /// `translated` (the format's own "something was written" test) or `stamp_target_always`, else `None`.
    pub fn stamp_lang<'a>(&self, doc: &'a Document, translated: bool) -> Option<&'a str> {
        doc.target_lang.as_deref().filter(|t| !t.trim().is_empty()).filter(|_| translated || self.stamp_target_always)
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("not a {format} file: {msg}")]
    Parse { format: String, msg: String },
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("unknown segment {0}")]
    UnknownSegment(String),
    #[error("segment {0} is locked")]
    Locked(String),
    #[error("tag {0} is not in the source")]
    InventedTag(String),
    #[error("tag {0} appears more often than in the source")]
    DuplicatedTag(String),
    #[error("this skeleton was made by {found}, but this is {expected}; open the file again with the current parser")]
    ParserVersion { found: String, expected: String },
    #[error("cannot write: {0}")]
    Write(String),
    /// The file is over a safety limit or holds something unsafe (a zip bomb, an XML entity bomb, too many
    /// units). The message is short, names the limit and is safe to show the person who uploaded the file.
    #[error("{0}")]
    Limit(LimitError),
    /// The bytes cannot be read as text in any encoding Bilingua knows.
    #[error("{0}")]
    Encoding(String),
    /// A `format_options` key no format declares, or a value of the wrong kind.
    #[error("format option {key}: {msg}")]
    FormatOption { key: String, msg: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A file parser. Implementations live in one crate per format (`bilingua-html`, `bilingua-docx`, ...).
pub trait Filter: Send + Sync {
    /// Short id, e.g. "html".
    fn id(&self) -> &'static str;
    /// Lowercase extensions without the dot, e.g. `["html", "htm"]`.
    fn extensions(&self) -> &'static [&'static str];
    /// True when the bytes look like this format (used when the extension is missing or ambiguous).
    fn sniff(&self, bytes: &[u8]) -> bool;
    /// True when the format carries source and target (PO, XLIFF and the like): its extract may fill segment
    /// targets and states. False for source files.
    fn bilingual(&self) -> bool {
        false
    }
    /// The `ExtractOptions::format_options` keys this filter reads (`"csv.columns"`, ...). The facade refuses keys
    /// no filter declares. Default: none.
    fn format_option_keys(&self) -> &'static [FormatOptionKey] {
        &[]
    }
    /// Source file to document. One segment per unit; no sentence splitting here.
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error>;
    /// Document back to a file in the original format, targets in place of sources, with [`MergeOptions`]. Every
    /// format honours both options: `output_encoding` (keep the file's encoding, or write UTF-8) and
    /// `stamp_target_always` (stamp the target language and fill plurals even when nothing is translated; the
    /// rule is [`MergeOptions::should_stamp`]).
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error>;
    /// [`Filter::merge_with`] with the default options: the file's own encoding, and no stamp when nothing is
    /// translated (an untouched merge gives the original bytes).
    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        self.merge_with(doc, &MergeOptions::default())
    }
    /// For formats a browser can show: the translated document as HTML in which every segment's text sits in
    /// `<span data-bl-seg="SEGMENT_ID">..</span>`, so a live preview can patch one segment without reloading.
    /// Scripts must be stripped. `None` when the format has no preview.
    fn preview(&self, _doc: &Document) -> Option<String> {
        None
    }
    /// The inner HTML of one segment's span in `preview()` (its current target, or source when empty), so a
    /// live preview can patch that span in place. `None` when the format has no preview or the id is unknown.
    fn render_segment(&self, _doc: &Document, _seg_id: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape_matches_catcore() {
        let v = vec![Inline::text("Hi "), Inline::tag("1", TagKind::Open, "<b>")];
        let j = serde_json::to_string(&v).unwrap();
        assert_eq!(j, r#"[{"text":"Hi "},{"tag":{"id":"1","kind":"open","display":"<b>"}}]"#);
    }

    #[test]
    fn output_joins_segments_and_falls_back_to_source() {
        let mut u = Unit::new("p1", vec![]);
        u.segments = vec![
            Segment { id: "p1#1".into(), source: vec![Inline::text("One.")], target: vec![Inline::text("Uno.")], state: SegState::Draft, origin: None },
            Segment { id: "p1#2".into(), source: vec![Inline::text("Two.")], target: vec![], state: SegState::New, origin: None },
        ];
        u.separators = vec![" ".into()];
        assert_eq!(u.output(), vec![Inline::text("Uno. Two.")]);
        assert_eq!(plain_text(&u.source()), "One. Two.");
    }

    #[test]
    fn set_target_takes_the_free_flag_from_the_source() {
        let mut d = Document {
            format: "test".into(),
            name: None,
            source_lang: None,
            target_lang: None,
            granularity: Granularity::Paragraph,
            units: vec![Unit::new("u1", vec![Inline::text("Hi "), Inline::free_tag("1", "{name}"), Inline::tag("2", TagKind::Standalone, "%s")])],
            skeleton: Skeleton::Text { parts: vec![SkelPart::Unit(0)], encoding: "utf-8".into(), parser: None },
            warnings: vec![],
        };
        // An editor that does not know the flag sends both codes as not free, and claims the %s is free.
        let t = vec![
            Inline::Tag { id: "2".into(), kind: TagKind::Standalone, display: "%s".into(), free: true },
            Inline::text(" hola "),
            Inline::tag("1", TagKind::Standalone, "{name}"),
        ];
        d.set_target("u1", t).unwrap();
        let got: Vec<bool> = d.units[0].segments[0].target.iter().filter(|i| matches!(i, Inline::Tag { .. })).map(Inline::is_free).collect();
        assert_eq!(got, vec![false, true]);
    }

    #[test]
    fn set_target_checks_tags() {
        let mut d = Document {
            format: "test".into(),
            name: None,
            source_lang: None,
            target_lang: None,
            granularity: Granularity::Paragraph,
            units: vec![Unit::new("u1", vec![Inline::tag("1", TagKind::Open, "<b>"), Inline::text("x"), Inline::tag("1", TagKind::Close, "</b>")])],
            skeleton: Skeleton::Text { parts: vec![SkelPart::Unit(0)], encoding: "utf-8".into(), parser: None },
            warnings: vec![],
        };
        assert!(d.set_target("u1", vec![Inline::text("y")]).is_ok());
        assert!(matches!(d.set_target("u1", vec![Inline::tag("2", TagKind::Open, "<i>")]), Err(Error::InventedTag(_))));
        assert!(matches!(
            d.set_target("u1", vec![Inline::tag("1", TagKind::Open, ""), Inline::tag("1", TagKind::Open, "")]),
            Err(Error::DuplicatedTag(_))
        ));
        assert_eq!(d.find("u1").unwrap().1.state, SegState::Draft);
    }

    fn big_doc(n: usize) -> Document {
        Document {
            format: "test".into(),
            units: (0..n).map(|i| Unit::new(format!("u{i}"), vec![Inline::text(format!("Source {i}"))])).collect(),
            skeleton: Skeleton::Text { parts: vec![], encoding: "utf-8".into(), parser: None },
            ..Default::default()
        }
    }

    #[test]
    fn set_targets_sets_many_and_matches_set_target() {
        let mut a = big_doc(50);
        let mut b = big_doc(50);
        a.units[7].translate = false;
        b.units[7].translate = false;
        for i in (0..50).filter(|i| *i != 7) {
            a.set_target(&format!("u{i}"), vec![Inline::text(format!("T{i}"))]).unwrap();
        }
        b.set_targets((0..50).filter(|i| *i != 7).map(|i| (format!("u{i}"), vec![Inline::text(format!("T{i}"))]))).unwrap();
        assert_eq!(a, b);
        assert_eq!(b.find("u49").unwrap().1.state, SegState::Draft);
    }

    #[test]
    fn set_targets_errors_like_set_target_and_keeps_earlier_items() {
        let mut d = big_doc(3);
        d.units[1].translate = false;
        let r = d.set_targets(vec![("u0", vec![Inline::text("a")]), ("u1", vec![Inline::text("b")]), ("u2", vec![Inline::text("c")])]);
        assert!(matches!(r, Err(Error::Locked(_))));
        assert_eq!(d.units[0].segments[0].target, vec![Inline::text("a")]);
        assert!(d.units[2].segments[0].target.is_empty());
        assert!(matches!(d.set_targets(vec![("nope", vec![])]), Err(Error::UnknownSegment(_))));
        let r = d.set_targets(vec![("u0", vec![Inline::tag("9", TagKind::Open, "<i>")])]);
        assert!(matches!(r, Err(Error::InventedTag(_))));
        // an empty target clears the segment, as set_target does
        d.set_targets(vec![("u0", vec![Inline::text("  ")])]).unwrap();
        assert_eq!(d.units[0].segments[0].state, SegState::New);
    }

    #[test]
    fn set_states_batch() {
        let mut d = big_doc(4);
        d.set_states((0..4).map(|i| (format!("u{i}"), SegState::Confirmed))).unwrap();
        assert!(d.segments().all(|(_, s)| s.state == SegState::Confirmed));
        assert!(matches!(d.set_states(vec![("zzz", SegState::New)]), Err(Error::UnknownSegment(_))));
    }

    #[test]
    fn set_targets_100k_is_fast() {
        let n = 100_000;
        let mut d = big_doc(n);
        let t = std::time::Instant::now();
        d.set_targets((0..n).map(|i| (format!("u{i}"), vec![Inline::text("x")]))).unwrap();
        d.set_states((0..n).map(|i| (format!("u{i}"), SegState::Translated))).unwrap();
        let took = t.elapsed();
        // well under a second in release; a debug build gets a loose bound that still rules out a quadratic scan
        let limit = if cfg!(debug_assertions) { 10_000 } else { 1_000 };
        assert!(took.as_millis() < limit, "100k targets took {took:?}");
        assert_eq!(d.units[n - 1].segments[0].state, SegState::Translated);
    }
}
