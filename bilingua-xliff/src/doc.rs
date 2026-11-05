//! The open-file handle: scan a bilingual file for byte spans, read segments on demand, change targets and
//! states, and write the file back by patching the spans.
//!
//! Opening only scans for byte spans. The inline content of a segment (text and tags) is parsed the first time
//! something asks for it, so a 50 MB file opens quickly and an editor that shows 40 rows only pays for 40 rows.
//! Saving splices the new target text and state attributes into the original bytes, so everything else in the
//! file (declaration, namespaces, quoting, line endings, vendor markup) comes back as it was.

use crate::error::Error;
use crate::inline::{parse_inlines, plain, split_brace_codes, Flavor, Parsed, RawTag, TagDefs};
use crate::patch::{apply, Patch};
use crate::{x12, x2, Format, MxMeta};
use bilingua_model::{merge_text, Inline, Note, Origin, SegState};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

pub(crate) enum Loc {
    X2(x2::X2Loc),
    X12(x12::X12Loc),
}

pub(crate) enum Layout {
    X2,
    X12(Vec<x12::UnitInfo>),
}

/// The always-loaded facts about a segment.
pub(crate) struct Meta {
    pub id: String,
    pub state: SegState,
    /// The state value exactly as the file wrote it (`conf`, `mq:status`, `state` [/ `subState`]).
    pub native: Option<String>,
}

/// Parsed inline content, loaded on demand.
pub(crate) struct Content {
    pub source: Vec<Inline>,
    pub target: Vec<Inline>,
    pub raws: Vec<RawTag>,
}

/// One segment plus everything needed to write it back.
pub(crate) struct SegRec {
    pub seg: Meta,
    pub locked: bool,
    pub lock_reason: Option<String>,
    pub origin: Option<Origin>,
    /// Position of the trans-unit or unit this segment belongs to.
    pub unit_ix: usize,
    /// Byte range of the source content (relative to the bytes after any BOM).
    pub src: Range<usize>,
    /// Byte range of the target content, if the target has any.
    pub tgt: Option<Range<usize>>,
    pub content: OnceLock<Content>,
    pub orig_state: SegState,
    pub target_dirty: bool,
    pub loc: Loc,
}

pub(crate) struct ParsedDoc {
    pub format: Format,
    pub source_lang: String,
    pub target_lang: String,
    pub recs: Vec<SegRec>,
    pub layout: Layout,
    pub tag_defs: Option<TagDefs>,
    /// Per unit: its id, its notes, and (XLIFF 2) the byte ranges of its `originalData` entries.
    pub unit_ids: Vec<String>,
    pub unit_notes: Vec<Vec<Note>>,
    pub unit_data: Vec<Vec<(String, Range<usize>)>>,
}

/// Combine tag markup learned from the source and the target (source wins).
fn merge_raws(mut src: Vec<RawTag>, tgt: Vec<RawTag>) -> Vec<RawTag> {
    for t in tgt {
        if !src.iter().any(|s| s.id == t.id && s.kind == t.kind) {
            src.push(t);
        }
    }
    src
}

/// What the file says about one segment, borrowed from the open file.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SegView<'a> {
    /// Position in the file (0-based).
    pub index: usize,
    /// Unique within the file: the trans-unit id, the Trados `mid`, or `unit#N` for XLIFF 2 units with several
    /// segments. A repeated id gets `~2`, `~3`, ... so a segment can always be addressed by it.
    pub id: &'a str,
    /// Position of the trans-unit or unit (several segments can share one).
    pub unit_ix: usize,
    pub unit_id: &'a str,
    pub state: SegState,
    /// The state exactly as the file wrote it, `None` once the state was edited here.
    pub native_state: Option<&'a str>,
    pub locked: bool,
    pub lock_reason: Option<&'a str>,
    pub origin: Option<&'a Origin>,
    pub source: &'a [Inline],
    pub target: &'a [Inline],
    /// The original markup of every tag in source and target.
    pub raws: &'a [RawTag],
}

/// An open bilingual file.
pub struct Doc {
    bytes: Vec<u8>,
    /// Length of a UTF-8 byte order mark at the start (0 or 3). Spans are relative to the bytes after it.
    bom: usize,
    format: Format,
    source_lang: String,
    target_lang: String,
    recs: Vec<SegRec>,
    layout: Layout,
    tag_defs: Option<TagDefs>,
    index: HashMap<String, usize>,
    unit_ids: Vec<String>,
    unit_notes: Vec<Vec<Note>>,
    unit_data: Vec<Vec<(String, Range<usize>)>>,
}

/// The dialect of a file from its header.
pub fn detect(buf: &[u8]) -> Result<Format, Error> {
    let head = &buf[..buf.len().min(16 * 1024)];
    let text = String::from_utf8_lossy(head);
    if text.trim_start().starts_with("<?xml") {
        let lower = text.to_ascii_lowercase();
        if lower.contains("encoding=\"utf-16") || lower.contains("encoding='utf-16") {
            return Err(Error::Unsupported("UTF-16 files are not supported; convert to UTF-8".into()));
        }
    }
    let Some(p) = text.find("<xliff") else {
        return Err(Error::Unsupported("not an XLIFF file (no <xliff> element)".into()));
    };
    let tag_end = text[p..].find('>').map_or(text.len(), |e| p + e);
    let tag = &text[p..tag_end];
    if tag.contains("version=\"2.") || tag.contains("version='2.") {
        Ok(Format::Xliff2)
    } else if tag.contains("SdlXliff") || tag.contains("xmlns:sdl") {
        Ok(Format::Sdlxliff)
    } else if tag.contains("www.memsource.com/mxlf") {
        Ok(Format::Mxliff)
    } else if tag.contains("MQXliff") || tag.contains("xmlns:mq") {
        Ok(Format::Mqxliff)
    } else {
        Ok(Format::Xliff12)
    }
}

impl Doc {
    /// Open a file held in memory.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Doc, Error> {
        Doc::from_bytes_hint(bytes, false)
    }

    /// Open a file held in memory. `mxliff_ext` is true when the file name ends in `.mxliff`: such a file is read
    /// as MXLIFF even when its header has no Phrase namespace.
    pub fn from_bytes_hint(bytes: Vec<u8>, mxliff_ext: bool) -> Result<Doc, Error> {
        let bom = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
        let mut format = detect(&bytes[bom..])?;
        if mxliff_ext && format == Format::Xliff12 {
            format = Format::Mxliff;
        }
        let parsed = match format {
            Format::Xliff2 => x2::parse(&bytes[bom..])?,
            f => x12::parse(&bytes[bom..], f)?,
        };
        let mut recs = parsed.recs;
        let mut index = HashMap::with_capacity(recs.len());
        for (i, r) in recs.iter_mut().enumerate() {
            // Ids must be unique so a segment can be addressed by id.
            if index.contains_key(&r.seg.id) {
                let base = r.seg.id.clone();
                let mut n = 1;
                while index.contains_key(&r.seg.id) {
                    n += 1;
                    r.seg.id = format!("{base}~{n}");
                }
            }
            index.insert(r.seg.id.clone(), i);
        }
        Ok(Doc {
            bytes,
            bom,
            format: parsed.format,
            source_lang: parsed.source_lang,
            target_lang: parsed.target_lang,
            recs,
            layout: parsed.layout,
            tag_defs: parsed.tag_defs,
            index,
            unit_ids: parsed.unit_ids,
            unit_notes: parsed.unit_notes,
            unit_data: parsed.unit_data,
        })
    }

    fn body(&self) -> &[u8] {
        &self.bytes[self.bom..]
    }

    /// True when the file starts with a UTF-8 byte order mark.
    pub fn has_bom(&self) -> bool {
        self.bom > 0
    }

    /// The dialect the file was read as.
    pub fn format(&self) -> Format {
        self.format
    }

    /// Source language from the file header (empty when the file does not say).
    pub fn source_lang(&self) -> &str {
        &self.source_lang
    }

    /// Target language from the file header (empty when the file does not say).
    pub fn target_lang(&self) -> &str {
        &self.target_lang
    }

    fn flavor(&self) -> Flavor {
        if self.format == Format::Xliff2 {
            Flavor::X2
        } else {
            Flavor::X12
        }
    }

    /// `originalData` of a unit as data id to text (XLIFF 2 `dataRef`).
    fn data_table(&self, unit_ix: usize) -> Option<HashMap<String, String>> {
        let list = self.unit_data.get(unit_ix).filter(|l| !l.is_empty())?;
        let body = self.body();
        Some(
            list.iter()
                .map(|(id, r)| {
                    let text = parse_inlines(&body[r.clone()], Flavor::X2, None, None)
                        .map(|p| plain(&p.inlines))
                        .unwrap_or_default();
                    (id.clone(), text)
                })
                .collect(),
        )
    }

    /// Parse one byte span of the body as inline content (MXLIFF brace codes split out).
    fn parse_span(&self, range: &Range<usize>, data: Option<&HashMap<String, String>>) -> Parsed {
        let body = self.body();
        let p = parse_inlines(&body[range.clone()], self.flavor(), self.tag_defs.as_ref(), data).unwrap_or_else(|_| {
            // Malformed inline markup: show it as text rather than hide the segment.
            Parsed { inlines: vec![Inline::Text(String::from_utf8_lossy(&body[range.clone()]).into_owned())], raws: Vec::new() }
        });
        if self.format == Format::Mxliff {
            split_brace_codes(p)
        } else {
            p
        }
    }

    /// The target of segment `i` as the file has it (not as edited here).
    fn original_target(&self, i: usize) -> Vec<Inline> {
        let r = &self.recs[i];
        let data = self.data_table(r.unit_ix);
        r.tgt.as_ref().map(|t| self.parse_span(t, data.as_ref()).inlines).unwrap_or_default()
    }

    /// Parse the inline content of segment `i` if that has not happened yet.
    fn content(&self, i: usize) -> &Content {
        let r = &self.recs[i];
        r.content.get_or_init(|| {
            let data = self.data_table(r.unit_ix);
            let src = self.parse_span(&r.src, data.as_ref());
            let tgt = r.tgt.as_ref().map(|t| self.parse_span(t, data.as_ref())).unwrap_or_default();
            Content { source: src.inlines, target: tgt.inlines, raws: merge_raws(src.raws, tgt.raws) }
        })
    }

    /// Number of segments.
    pub fn len(&self) -> usize {
        self.recs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recs.is_empty()
    }

    /// Index of the segment with this id.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.index.get(id).copied()
    }

    /// Everything about segment `i` (parses its content on first use). Panics when `i` is out of range.
    pub fn seg(&self, i: usize) -> SegView<'_> {
        let r = &self.recs[i];
        let c = self.content(i);
        SegView {
            index: i,
            id: &r.seg.id,
            unit_ix: r.unit_ix,
            unit_id: self.unit_ids.get(r.unit_ix).map_or("", String::as_str),
            state: r.seg.state,
            native_state: if r.seg.state == r.orig_state { r.seg.native.as_deref() } else { None },
            locked: r.locked,
            lock_reason: r.lock_reason.as_deref(),
            origin: r.origin.as_ref(),
            source: &c.source,
            target: &c.target,
            raws: &c.raws,
        }
    }

    /// Notes of a trans-unit or unit.
    pub fn unit_notes(&self, unit_ix: usize) -> &[Note] {
        self.unit_notes.get(unit_ix).map_or(&[], Vec::as_slice)
    }

    /// Number of trans-units or units that hold segments.
    pub fn unit_count(&self) -> usize {
        self.unit_ids.len()
    }

    /// Phrase metadata of segment `i` (MXLIFF only), as the file had it when opened.
    pub fn mxliff_meta(&self, i: usize) -> Option<&MxMeta> {
        match &self.recs.get(i)?.loc {
            Loc::X12(l) => l.mx.as_deref(),
            _ => None,
        }
    }

    /// Replaces the target of segment `i` without checking tags (the caller's contract decides what is allowed).
    /// Locked segments are refused (`false`). An unchanged target changes nothing. Otherwise the segment becomes
    /// `Draft` (`New` for an empty target; a `Rejected` one stays so); tags take their display text and `free` flag from the source tag with
    /// the same id and kind.
    pub fn put_target(&mut self, i: usize, target: Vec<Inline>) -> bool {
        if self.recs[i].locked {
            return false;
        }
        self.content(i);
        let original = self.original_target(i);
        let r = &mut self.recs[i];
        let Some(c) = r.content.get_mut() else { return false };
        let target = normalize(target, &c.source);
        if target == c.target {
            return true;
        }
        // Back to what the file holds (an undo): the target bytes need no rewrite.
        let back_to_file = target == original;
        // A rejected segment stays rejected while its target is edited (the file keeps its `RejectedTranslation`
        // until someone sets a new state).
        if target.is_empty() {
            r.seg.state = SegState::New;
        } else if r.seg.state != SegState::Rejected {
            r.seg.state = SegState::Draft;
        }
        c.target = target;
        r.target_dirty = !back_to_file;
        true
    }

    /// The state segment `i` had when the file was opened.
    pub fn orig_state(&self, i: usize) -> SegState {
        self.recs[i].orig_state
    }

    /// True when the target of segment `i` differs from the file's.
    pub fn target_changed(&self, i: usize) -> bool {
        self.recs[i].target_dirty
    }

    /// Like [`Doc::put_target`] but leaves the state alone (for writing a whole document back, where the state is
    /// set separately).
    pub fn put_target_keep_state(&mut self, i: usize, target: Vec<Inline>) -> bool {
        let before = self.recs[i].seg.state;
        let ok = self.put_target(i, target);
        self.recs[i].seg.state = before;
        ok
    }

    /// Sets the state of segment `i`.
    pub fn put_state(&mut self, i: usize, state: SegState) {
        self.recs[i].seg.state = state;
    }

    /// Row indices (0-based, document order) of the segments whose source or target contains `text` (plain
    /// substring, case sensitive, tags ignored).
    pub fn search_indices(&self, text: &str) -> Vec<usize> {
        if text.is_empty() {
            return Vec::new();
        }
        let body = self.body();
        (0..self.recs.len())
            .filter(|&i| {
                let r = &self.recs[i];
                if let Some(c) = r.content.get() {
                    return plain(&c.source).contains(text) || plain(&c.target).contains(text);
                }
                // Fast path: spans with no markup and no entities are their own text.
                let mut slow = false;
                for range in std::iter::once(&r.src).chain(r.tgt.iter()) {
                    let raw = &body[range.clone()];
                    if raw.iter().any(|&b| b == b'<' || b == b'&') {
                        slow = true;
                        break;
                    }
                    if std::str::from_utf8(raw).is_ok_and(|s| s.contains(text)) {
                        return true;
                    }
                }
                if !slow {
                    return false;
                }
                let c = self.content(i);
                plain(&c.source).contains(text) || plain(&c.target).contains(text)
            })
            .collect()
    }

    /// Cheap test on the raw bytes: could segment `i` hold a bidi control character? Bidi controls are
    /// U+200E/F, U+202A-E, U+2066-9 (all start with 0xE2 in UTF-8) or U+061C (starts with 0xD8); numeric
    /// references (`&`) can hide them, so those count as "maybe". A segment whose content is already parsed
    /// is always "maybe".
    pub fn maybe_bidi(&self, i: usize) -> bool {
        let r = &self.recs[i];
        let raw_has = |range: &Range<usize>| self.body()[range.clone()].iter().any(|&b| b == 0xE2 || b == 0xD8 || b == b'&');
        raw_has(&r.src) || r.tgt.as_ref().is_some_and(raw_has) || r.content.get().is_some()
    }

    /// True if anything was edited since opening.
    pub fn is_dirty(&self) -> bool {
        self.recs.iter().any(|r| r.target_dirty || r.seg.state != r.orig_state)
    }

    /// The file bytes as they would be saved now. With no edits, the input byte for byte.
    pub fn to_bytes(&self) -> Vec<u8> {
        if !self.is_dirty() {
            return self.bytes.clone();
        }
        let body = self.body();
        let patches: Vec<Patch> = match &self.layout {
            Layout::X2 => x2::patches(body, &self.recs),
            Layout::X12(units) => x12::patches(body, &self.recs, units),
        };
        let mut out = self.bytes[..self.bom].to_vec();
        out.extend(apply(body, patches));
        out
    }

    /// Size of the loaded file in bytes.
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    /// The original bytes as read.
    pub fn original_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Merge adjacent text and drop empty text so equal content compares equal; tags take display and `free`
/// from the source tag with the same id and kind.
fn normalize(inlines: Vec<Inline>, source: &[Inline]) -> Vec<Inline> {
    let fixed: Vec<Inline> = inlines
        .into_iter()
        .map(|i| match i {
            Inline::Tag { id, kind, display, .. } => {
                let hit = source.iter().find_map(|s| match s {
                    Inline::Tag { id: i, kind: k, display: d, free: f, .. } if *i == id && *k == kind => Some((d.clone(), *f)),
                    _ => None,
                });
                match hit {
                    Some((d, f)) => Inline::tag(id, kind, d).with_free(f),
                    None => Inline::tag(id, kind, display),
                }
            }
            t => t,
        })
        .collect();
    merge_text(fixed)
}
