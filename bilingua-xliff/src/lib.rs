//! Bilingua XLIFF: XLIFF 1.2 (with the SDLXLIFF, memoQ MQXLIFF and Phrase MXLIFF dialects) and XLIFF 2.
//!
//! Two ways in:
//! * [`Doc`], the open-file handle (lazy, byte-faithful): scan, read segments, change targets and states, write.
//!   CAT OS's editor core uses it.
//! * The [`Filter`] implementations ([`filters`]): `extract` gives a Bilingua [`Document`] with targets, states,
//!   origin, notes and lock reasons; `merge` writes the targets and states of a document back **by patching the
//!   original bytes**, which the skeleton holds whole.
//!
//! Segmentation is the file's own, so the facade does not split these documents into sentences.
//! Inline codes keep their original markup in `Unit::codes`; an `x` or `ph` placeholder is marked `free` by the
//! Localization OS rule (see `inline.rs`).

/// Version of the skeleton layout this crate writes (bumped when an old skeleton can no longer be merged).
/// Schema 2 (wave 12): a trans-unit with one segment names that segment by the unit id (see [`segment_ids`]);
/// a schema 1 document is merged by the file's own segment ids, as it was made.
pub const SKELETON_SCHEMA: u32 = 2;

mod attrs;
mod doc;
mod error;
mod inline;
mod lang;
#[doc(hidden)]
pub mod locos;
mod patch;
mod x12;
mod x2;

pub use doc::{detect, Doc, SegView};
pub use error::Error;
pub use inline::{RawTag, TagDef, TagDefs};

use bilingua_model::{Document, ExtractOptions, Filter, Granularity, Inline, MergeOptions, Segment, SkelPart, Skeleton, TagKind, Unit};
use std::collections::{HashMap, HashSet};

/// The unit ids and segment ids [`to_document`] gives an open file, one unit id per trans-unit (`Doc` unit
/// position) and one segment id per segment (`Doc` segment position).
///
/// A unit id is the trans-unit id (`~2`, `~3`, ... after a repeat). A unit with one segment names that segment by
/// its unit id, so the facade contract "segment id = unit id in paragraph mode" holds also for a Trados SDLXLIFF,
/// whose trans-unit ids are GUIDs and whose segment ids (`mrk mid`) are numbers. A unit with several segments keeps
/// the file's own segment ids ([`SegView::id`]). Ids stay unique in the document: an id already taken gets `~2`, ...
pub fn segment_ids(doc: &Doc) -> (Vec<String>, Vec<String>) {
    let mut unit_ids: Vec<Option<String>> = vec![None; doc.unit_count()];
    let mut seg_count = vec![0usize; doc.unit_count()];
    let mut seen: HashSet<String> = HashSet::new();
    for i in 0..doc.len() {
        let v = doc.seg(i);
        seg_count[v.unit_ix] += 1;
        if unit_ids[v.unit_ix].is_none() {
            let mut uid = v.unit_id.to_string();
            let mut n = 1;
            while !seen.insert(uid.clone()) {
                n += 1;
                uid = format!("{}~{n}", v.unit_id);
            }
            unit_ids[v.unit_ix] = Some(uid);
        }
    }
    // the file's own ids of multi-segment units first, so a single-segment unit never takes one of them
    let mut taken: HashSet<String> = HashSet::new();
    for i in 0..doc.len() {
        let v = doc.seg(i);
        if seg_count[v.unit_ix] > 1 {
            taken.insert(v.id.to_string());
        }
    }
    let mut seg_ids = Vec::with_capacity(doc.len());
    for i in 0..doc.len() {
        let v = doc.seg(i);
        if seg_count[v.unit_ix] > 1 {
            seg_ids.push(v.id.to_string());
            continue;
        }
        let base = unit_ids[v.unit_ix].clone().unwrap_or_default();
        let mut id = base.clone();
        let mut n = 1;
        while !taken.insert(id.clone()) {
            n += 1;
            id = format!("{base}~{n}");
        }
        seg_ids.push(id);
    }
    (unit_ids.into_iter().map(Option::unwrap_or_default).collect(), seg_ids)
}

/// True when `document` was made with the file's own segment ids ([`SegView::id`]) rather than [`segment_ids`]:
/// its skeleton has no stamp, or a stamp of crate `parser` with a schema below `first` (that crate's first schema
/// with [`segment_ids`]: 2 for `bilingua-xliff` and `bilingua-sdl`). A stamp of another crate answers false.
pub fn uses_file_segment_ids(document: &Document, parser: &str, first: u32) -> bool {
    document.skeleton.parser().is_none_or(|p| p.name == parser && p.schema < first)
}

/// Which bilingual dialect a file was read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// XLIFF 2.x (`unit` / `segment`).
    Xliff2,
    /// SDL Trados SDLXLIFF (XLIFF 1.2 plus `sdl:` extensions).
    Sdlxliff,
    /// memoQ MQXLIFF (XLIFF 1.2 plus `mq:` extensions).
    Mqxliff,
    /// Plain XLIFF 1.2 (`trans-unit`).
    Xliff12,
    /// Phrase TMS MXLIFF (XLIFF 1.2 plus `m:` extensions).
    Mxliff,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Xliff2 => "xliff2",
            Format::Sdlxliff => "sdlxliff",
            Format::Mqxliff => "mqxliff",
            Format::Xliff12 => "xliff12",
            Format::Mxliff => "mxliff",
        }
    }
}

/// Phrase (MXLIFF) metadata of one trans-unit, as the file had it when opened.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct MxMeta {
    /// `m:confirmed` is `1` / `true`.
    pub confirmed: bool,
    /// `m:locked` is `1` / `true`.
    pub locked: bool,
    /// `m:score`: the match score of the pre-translation, 0.0 to 1.0 as Phrase writes it.
    pub score: Option<f64>,
    /// `m:gross-score`.
    pub gross_score: Option<f64>,
    /// `m:trans-origin`, for example `memsource-tm`.
    pub trans_origin: Option<String>,
    /// `m:para-id`.
    pub para_id: Option<String>,
    /// `m:level`.
    pub level: Option<String>,
    /// Every other `m:` attribute of the trans-unit, name without the prefix, in file order.
    pub other: Vec<(String, String)>,
}

fn parse_err(id: &str, e: Error) -> bilingua_model::Error {
    bilingua_model::Error::Parse { format: id.to_string(), msg: e.to_string() }
}

/// Builds a Bilingua document from an open file. The file must be valid UTF-8 (the skeleton is text).
pub fn to_document(doc: &Doc, id: &str, opts: &ExtractOptions) -> Result<Document, bilingua_model::Error> {
    to_document_in(doc, id, opts, None, Vec::new())
}

/// [`to_document`] for a file that was transcoded to UTF-8 before it was opened: `encoding` is the envelope tag
/// from [`bilingua_model::enc::decode_text`] that [`merge_document`] writes it back in, `warnings` go on the
/// document. The XML guard runs over the file first.
pub fn to_document_in(
    doc: &Doc,
    id: &str,
    opts: &ExtractOptions,
    encoding: Option<String>,
    warnings: Vec<bilingua_model::Warning>,
) -> Result<Document, bilingua_model::Error> {
    let original = doc.original_bytes();
    let limits = opts.limits();
    bilingua_model::guard::check_xml(original, &limits)?;
    let (bom, body) = match original.strip_prefix(&[0xEF, 0xBB, 0xBF][..]) {
        Some(r) => (true, r),
        None => (false, original),
    };
    let text = std::str::from_utf8(body)
        .map_err(|e| bilingua_model::Error::Parse { format: id.to_string(), msg: format!("not valid UTF-8: {e}") })?;
    let mut units: Vec<Unit> = Vec::new();
    let mut unit_of: Vec<Option<usize>> = vec![None; doc.unit_count()];
    let (unit_ids, seg_ids) = segment_ids(doc);
    for (i, sid) in seg_ids.iter().enumerate() {
        let v = doc.seg(i);
        let ui = match unit_of[v.unit_ix] {
            Some(ui) => ui,
            None => {
                let mut u = Unit::new(unit_ids[v.unit_ix].clone(), vec![]);
                u.segments.clear();
                u.context.kind = Some("segment".into());
                u.context.key = Some(v.unit_id.to_string());
                u.context.extra.insert("xliff.dialect".into(), doc.format().name().into());
                u.notes = doc.unit_notes(v.unit_ix).to_vec();
                units.push(u);
                unit_of[v.unit_ix] = Some(units.len() - 1);
                units.len() - 1
            }
        };
        let u = &mut units[ui];
        if !u.segments.is_empty() {
            u.separators.push(String::new());
        }
        if let Some(n) = v.native_state {
            u.context.extra.insert(format!("xliff.state.{sid}"), n.to_string());
        }
        for r in v.raws {
            let c = u.codes.entry(r.id.clone()).or_default();
            match r.kind {
                TagKind::Open => c.open = Some(r.raw.clone()),
                TagKind::Close => c.close = Some(r.raw.clone()),
                TagKind::Standalone => c.standalone = Some(r.raw.clone()),
            }
        }
        if v.locked {
            u.translate = false;
            u.lock_reason = v.lock_reason.map(str::to_string);
        }
        u.segments.push(Segment::new(sid.clone(), v.source.to_vec()).with_target(v.target.to_vec()).with_state(v.state).with_origin(v.origin.cloned()));
    }
    let lang = |s: &str, fallback: &Option<String>| (!s.trim().is_empty()).then(|| s.to_string()).or_else(|| fallback.clone());
    bilingua_model::guard::finish(Document::new(id.to_string(), units, Skeleton::text(
            vec![SkelPart::Raw(text.to_string())],
            encoding.unwrap_or_else(|| if bom { "utf-8-bom" } else { "utf-8" }.to_string()),
            bilingua_model::parser_info!(),
        )).with_source_lang(lang(doc.source_lang(), &opts.source_lang)).with_target_lang(lang(doc.target_lang(), &opts.target_lang)).with_granularity(Granularity::Paragraph), warnings, &limits)
}

/// Writes the targets and states of `document` into an open file (matching segments by id). Locked segments
/// and units with `translate == false` are left as they are.
pub fn apply_document(doc: &mut Doc, document: &Document) {
    apply_document_prefixed(doc, document, "");
}

/// Like [`apply_document`] for a document that holds the segments of several files: only segments whose id
/// starts with `prefix` are used, and the id is matched without it.
pub fn apply_document_prefixed(doc: &mut Doc, document: &Document, prefix: &str) {
    if uses_file_segment_ids(document, env!("CARGO_PKG_NAME"), 2) {
        apply_units_prefixed_file_ids(doc, document.units.iter(), prefix);
    } else {
        apply_units_prefixed(doc, document.units.iter(), prefix);
    }
}

/// Like [`apply_document_prefixed`] for a chosen set of units (a package passes the units of one of its files).
/// Segments are matched by the ids [`to_document`] gives ([`segment_ids`]).
pub fn apply_units_prefixed<'a>(doc: &mut Doc, units: impl Iterator<Item = &'a Unit>, prefix: &str) {
    let ids: HashMap<String, usize> = segment_ids(doc).1.into_iter().enumerate().map(|(i, s)| (s, i)).collect();
    apply_units_with(doc, units, prefix, |_, id| ids.get(id).copied());
}

/// [`apply_units_prefixed`] for a document made before [`segment_ids`] (skeleton schema 1): segments are matched
/// by the file's own ids ([`SegView::id`], the Trados `mid` for an SDLXLIFF).
pub fn apply_units_prefixed_file_ids<'a>(doc: &mut Doc, units: impl Iterator<Item = &'a Unit>, prefix: &str) {
    apply_units_with(doc, units, prefix, |d, id| d.index_of(id));
}

fn apply_units_with<'a>(doc: &mut Doc, units: impl Iterator<Item = &'a Unit>, prefix: &str, find: impl Fn(&Doc, &str) -> Option<usize>) {
    for u in units.filter(|u| u.translate) {
        for s in &u.segments {
            let Some(id) = s.id.strip_prefix(prefix) else { continue };
            let Some(i) = find(&*doc, id) else { continue };
            if doc.seg(i).locked {
                continue;
            }
            doc.put_target_keep_state(i, s.target.clone());
            doc.put_state(i, s.state);
        }
    }
}

/// Writes a document back: the original bytes with the document's targets and states patched in.
pub fn merge_document(document: &Document, id: &str) -> Result<Vec<u8>, bilingua_model::Error> {
    merge_document_with(document, id, &MergeOptions::default())
}

/// [`merge_document`] with [`MergeOptions`]. When the document has a `target_lang` and a target was written (or
/// `stamp_target_always` is set), the language is declared in the file as Localization OS does: `target-language`
/// on every `<file>` of an XLIFF 1.2 family file, `trgLang` on the root of XLIFF 2. A file nothing was written to
/// comes back byte for byte.
pub fn merge_document_with(document: &Document, id: &str, opts: &MergeOptions) -> Result<Vec<u8>, bilingua_model::Error> {
    bilingua_model::check_parser(document, &bilingua_model::parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &document.skeleton else {
        return Err(bilingua_model::Error::Write(format!("{id} needs a text skeleton")));
    };
    let plain = matches!(encoding.as_str(), "utf-8" | "utf-8-bom");
    let mut bytes: Vec<u8> = if encoding == "utf-8-bom" { vec![0xEF, 0xBB, 0xBF] } else { Vec::new() };
    for p in parts {
        if let SkelPart::Raw(r) = p {
            bytes.extend_from_slice(r.as_bytes());
        }
    }
    let mut doc = Doc::from_bytes_hint(bytes, id == "mxliff").map_err(|e| parse_err(id, e))?;
    apply_document(&mut doc, document);
    let mut out = doc.to_bytes();
    let stamp = opts.stamp_lang(document, doc.is_dirty());
    if let Some(t) = stamp {
        out = lang::stamp_target_language(&out, doc.format(), t);
    }
    if plain {
        return Ok(out);
    }
    // a file that was read in another encoding goes back out in it (UTF-8 when the new text does not fit, or when
    // the options ask for UTF-8)
    let utf8 = String::from_utf8(out)
        .map_err(|e| bilingua_model::Error::Write(format!("{id}: the merged file is not valid UTF-8: {e}")))?;
    bilingua_model::enc::encode_text_with(&utf8, encoding, opts)
}

/// One of the XLIFF filters.
pub struct XliffFilter {
    id: &'static str,
    exts: &'static [&'static str],
    dialect: Option<Format>,
}

/// All XLIFF filters, specific dialects first.
pub fn filters() -> Vec<Box<dyn Filter>> {
    vec![
        Box::new(XliffFilter { id: "sdlxliff", exts: &["sdlxliff", "sdlxlf"], dialect: Some(Format::Sdlxliff) }),
        Box::new(XliffFilter { id: "mqxliff", exts: &["mqxliff"], dialect: Some(Format::Mqxliff) }),
        Box::new(XliffFilter { id: "mxliff", exts: &["mxliff"], dialect: Some(Format::Mxliff) }),
        Box::new(XliffFilter { id: "xliff", exts: &["xlf", "xliff"], dialect: None }),
    ]
}

impl Filter for XliffFilter {
    fn id(&self) -> &'static str {
        self.id
    }
    fn extensions(&self) -> &'static [&'static str] {
        self.exts
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(4096)];
        if !head.windows(6).any(|w| w == b"<xliff") {
            return false;
        }
        match self.dialect {
            Some(d) => detect(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes)).is_ok_and(|f| f == d),
            None => true,
        }
    }
    fn bilingual(&self) -> bool {
        true
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, bilingua_model::Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let limits = opts.limits();
        bilingua_model::guard::check_input(bytes, &limits)?;
        bilingua_model::guard::check_xml(bytes, &limits)?;
        if opts.is_profile(bilingua_model::profile::LOCOS) {
            return locos::extract(bytes, self.id, opts);
        }
        let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
        if std::str::from_utf8(body).is_ok() {
            let doc = Doc::from_bytes_hint(bytes.to_vec(), self.id == "mxliff").map_err(|e| parse_err(self.id, e))?;
            return to_document(&doc, self.id, opts);
        }
        // UTF-16 or a legacy code page: read it as text, hand the parser UTF-8
        let dec = bilingua_model::enc::decode_text(bytes, &bilingua_model::enc::DecodeOptions::new(opts).declared())?;
        bilingua_model::guard::check_xml(dec.text.as_bytes(), &limits)?;
        let doc = Doc::from_bytes_hint(dec.text.into_bytes(), self.id == "mxliff").map_err(|e| parse_err(self.id, e))?;
        to_document_in(&doc, self.id, opts, Some(dec.encoding), dec.warnings)
    }
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, bilingua_model::Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        if let Skeleton::Binary { data, .. } = &doc.skeleton {
            // a document extracted under the Localization OS profile
            bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
            bilingua_model::guard::check_xml(data, &bilingua_model::Limits::default())?;
            return locos::merge(doc, self.id, opts);
        }
        merge_document_with(doc, self.id, opts)
    }
}

/// Plain text of inlines, for callers that only need the words.
pub fn plain_text(v: &[Inline]) -> String {
    inline::plain(v)
}
