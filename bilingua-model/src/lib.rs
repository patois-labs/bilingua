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
pub mod locos;

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
}

fn yes() -> bool {
    true
}

impl Unit {
    /// A unit with one segment holding `source`.
    pub fn new(id: impl Into<String>, source: Vec<Inline>) -> Unit {
        let id = id.into();
        Unit {
            segments: vec![Segment { id: id.clone(), source, target: vec![], state: SegState::New, origin: None }],
            id,
            separators: vec![],
            codes: BTreeMap::new(),
            context: UnitContext::default(),
            translate: true,
            notes: vec![],
            lock_reason: None,
        }
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

/// Which parser made a skeleton: the crate name and its version. A skeleton only fits the parser line that made
/// it, so `merge` refuses one from an incompatible version (see [`check_parser`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParserInfo {
    /// The producing crate, e.g. "bilingua-docx".
    pub name: String,
    /// Its semver version, e.g. "0.1.0".
    pub version: String,
}

impl ParserInfo {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> ParserInfo {
        ParserInfo { name: name.into(), version: version.into() }
    }

    /// True when a skeleton made by `self` can be written by `current`: same crate and the same compatibility
    /// line. The line is the major version, or the minor version while the major is 0 (semver for 0.x).
    pub fn compatible_with(&self, current: &ParserInfo) -> bool {
        self.name == current.name && line(&self.version).is_some() && line(&self.version) == line(&current.version)
    }
}

fn line(v: &str) -> Option<(u64, u64)> {
    let mut it = v.split(['.', '-', '+']);
    let major: u64 = it.next()?.parse().ok()?;
    if major > 0 {
        return Some((major, 0));
    }
    let minor: u64 = it.next()?.parse().ok()?;
    Some((0, minor))
}

/// The producing parser of the running crate: `parser_info!()` expands to a [`ParserInfo`] with the calling
/// crate's name and version.
#[macro_export]
macro_rules! parser_info {
    () => {
        $crate::ParserInfo::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
    };
}

/// Refuses a document whose skeleton came from an incompatible parser. A skeleton with no record (written
/// before parser versions existed) is accepted.
pub fn check_parser(doc: &Document, current: &ParserInfo) -> Result<(), Error> {
    match doc.skeleton.parser() {
        Some(found) if !found.compatible_with(current) => Err(Error::ParserVersion {
            found: format!("{} {}", found.name, found.version),
            expected: format!("{} {}", current.name, current.version),
        }),
        _ => Ok(()),
    }
}

/// Everything in the file that is not translatable text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    /// The parser that made this skeleton, if recorded.
    pub fn parser(&self) -> Option<&ParserInfo> {
        match self {
            Skeleton::Text { parser, .. } | Skeleton::Zip { parser, .. } => parser.as_ref(),
        }
    }
}

/// How finely units are split into segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

impl Document {
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
    pub fn set_target(&mut self, seg_id: &str, target: Vec<Inline>) -> Result<(), Error> {
        let unit = self
            .units
            .iter_mut()
            .find(|u| u.segments.iter().any(|s| s.id == seg_id))
            .ok_or_else(|| Error::UnknownSegment(seg_id.to_string()))?;
        if !unit.translate {
            return Err(Error::Locked(seg_id.to_string()));
        }
        let seg = unit.segments.iter_mut().find(|s| s.id == seg_id).expect("found above");
        check_tags(&seg.source, &target)?;
        let target = merge_text(target);
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractOptions {
    pub source_lang: Option<String>,
    pub target_lang: Option<String>,
    /// Parsers always extract one segment per unit; `bilingua-seg` applies the granularity afterwards.
    pub granularity: Granularity,
    /// Extract text the format marks as not translatable as locked units (for context), instead of skipping it.
    pub include_locked: bool,
}

#[derive(Debug, thiserror::Error)]
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
    /// Source file to document. One segment per unit; no sentence splitting here.
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error>;
    /// Document back to a file in the original format, targets in place of sources.
    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error>;
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
    fn set_target_checks_tags() {
        let mut d = Document {
            format: "test".into(),
            name: None,
            source_lang: None,
            target_lang: None,
            granularity: Granularity::Paragraph,
            units: vec![Unit::new("u1", vec![Inline::tag("1", TagKind::Open, "<b>"), Inline::text("x"), Inline::tag("1", TagKind::Close, "</b>")])],
            skeleton: Skeleton::Text { parts: vec![SkelPart::Unit(0)], encoding: "utf-8".into(), parser: None },
        };
        assert!(d.set_target("u1", vec![Inline::text("y")]).is_ok());
        assert!(matches!(d.set_target("u1", vec![Inline::tag("2", TagKind::Open, "<i>")]), Err(Error::InventedTag(_))));
        assert!(matches!(
            d.set_target("u1", vec![Inline::tag("1", TagKind::Open, ""), Inline::tag("1", TagKind::Open, "")]),
            Err(Error::DuplicatedTag(_))
        ));
        assert_eq!(d.find("u1").unwrap().1.state, SegState::Draft);
    }
}
