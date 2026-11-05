//! XLIFF 1.2 family: SDLXLIFF (`seg-source` / `mrk mtype="seg"` / `sdl:seg-defs`),
//! MQXLIFF (`mq:status`) and plain XLIFF 1.2 (`trans-unit` / `target state`).

use crate::attrs::TagInfo;
use crate::doc::{Layout, Loc, Meta, ParsedDoc, SegRec};
use crate::error::Error;
use crate::inline::{serialize, Flavor, TagDef, TagDefs};
use crate::patch::{apply, line_prefix, AttrSlot, Patch};
use crate::{Format, MxMeta};
use bilingua_model::{Note, Origin, OriginKind, SegState};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::ops::Range;

/// The `<target>` element of a trans-unit.
#[derive(Debug, Clone)]
pub enum UTarget {
    /// No target element; insert one at `pos`.
    Missing { pos: usize, prefix: String },
    /// `<target/>`.
    EmptyElem { range: Range<usize>, name: String },
    Present { content: Range<usize>, close_start: usize },
}

#[derive(Debug, Clone)]
pub enum Defs {
    Absent,
    EmptyElem { range: Range<usize>, name: String },
    Open { close_start: usize },
}

#[derive(Debug, Clone)]
pub struct UnitInfo {
    pub target: UTarget,
    /// SDL: inner range of `<seg-source>` and the source content range of each segment `mrk`,
    /// used to build a target skeleton.
    pub seg_source: Option<Range<usize>>,
    pub src_ranges: Vec<Range<usize>>,
    pub defs: Defs,
    /// Prefix used by the seg-defs element, `"sdl:"` by default.
    pub defs_prefix: String,
    pub tu_close: usize,
    pub first: usize,
    pub count: usize,
    /// The trans-unit's `id`.
    pub id: String,
    pub notes: Vec<Note>,
    /// `translation_status` of the WorldServer `iws:status` element, when the unit has one.
    pub iws: Option<AttrSlot>,
}

/// WorldServer (GlobalLink) `<iws:segment-metadata><iws:status translation_status=".." lock_status=".."/>`.
#[derive(Debug, Clone)]
struct IwsStatus {
    translation: String,
    lock: String,
    slot: AttrSlot,
}

impl IwsStatus {
    fn finished(&self) -> bool {
        self.translation.trim().eq_ignore_ascii_case("finished")
    }
    fn locked(&self) -> bool {
        self.lock.trim().eq_ignore_ascii_case("locked")
    }
}

#[derive(Debug, Clone)]
pub enum TgtSlot {
    Content(Range<usize>),
    /// Self-closed `<mrk mtype="seg" mid=".."/>`.
    Empty { range: Range<usize>, name: String },
    /// The target exists but has no `mrk` for this segment: insert one at `pos`.
    InTarget { pos: usize },
    /// Use the unit's `UTarget`.
    Unit,
}

/// The `sdl:seg` element of a segment, as found in `sdl:seg-defs`.
#[derive(Debug, Clone)]
pub struct SdlDef {
    pub conf: AttrSlot,
    pub origin: AttrSlot,
    /// The whole start tag (or empty element).
    pub tag: Range<usize>,
    pub empty: bool,
    pub name: String,
    /// `<sdl:prev-origin ./>` that Trados writes when a human edits a TM or MT match; built from
    /// the current origin attributes. `None` when the origin is absent or already interactive.
    pub prev_origin: Option<String>,
}

#[derive(Debug, Clone)]
pub enum StateLoc {
    /// SDL: `conf` on `sdl:seg`. `None` slot = no `sdl:seg` element for this mid yet.
    Sdl(Option<SdlDef>),
    /// memoQ: `mq:status` on the trans-unit.
    Mq(AttrSlot),
    /// Phrase: `m:confirmed` on the trans-unit.
    Mx(AttrSlot),
    /// Plain: `state` on the target element (`None` when there is no target element).
    TargetState(Option<AttrSlot>),
}

#[derive(Debug, Clone)]
pub struct X12Loc {
    pub mid: Option<String>,
    pub tgt: TgtSlot,
    pub state: StateLoc,
    /// Plain XLIFF: `approved` attribute on the trans-unit.
    pub approved: Option<(Range<usize>, String)>,
    /// Phrase metadata of the trans-unit (MXLIFF only).
    pub mx: Option<Box<MxMeta>>,
}

fn local(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|&b| b == b':') {
        Some(p) => &name[p + 1..],
        None => name,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum K {
    Group,
    Tu,
    Source,
    SegSource,
    Target,
    SegDefs,
    MrkSeg,
    Note,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Region {
    SegSource,
    Target,
}

struct DefSeg {
    mid: String,
    locked: bool,
    def: SdlDef,
}

struct Acc {
    tag: TagInfo,
    id: String,
    /// `translate="no"` inherited from an enclosing `<group>` (nearest explicit value wins).
    group_locked: bool,
    source: Option<(Range<usize>, usize, usize)>,
    seg_source: Option<Range<usize>>,
    ss_elem: Option<(usize, usize)>,
    src_mrks: Vec<(String, Range<usize>)>,
    target: Option<UTarget>,
    target_tag: Option<TagInfo>,
    tgt_mrks: Vec<(String, TgtSlot)>,
    defs: Defs,
    defs_prefix: String,
    def_segs: Vec<DefSeg>,
    notes: Vec<(TagInfo, Range<usize>)>,
    iws: Option<IwsStatus>,
}

/// Prefixes that mean WorldServer (GlobalLink): `iws` (what Okapi keys on) and any prefix bound to an
/// `idiominc.com` namespace in the file header.
fn iws_prefixes(buf: &[u8]) -> Vec<String> {
    let head = String::from_utf8_lossy(&buf[..buf.len().min(16 * 1024)]).into_owned();
    let mut out = vec!["iws".to_owned()];
    let mut rest = head.as_str();
    while let Some(p) = rest.find("xmlns:") {
        rest = &rest[p + 6..];
        let Some(eq) = rest.find('=') else { break };
        let name = rest[..eq].trim().to_owned();
        let after = rest[eq + 1..].trim_start();
        let Some(q) = after.chars().next().filter(|c| matches!(c, '"' | '\'')) else { continue };
        let Some(end) = after[1..].find(q) else { break };
        if after[1..1 + end].contains("idiominc.com") && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// True for an `iws:status` element name.
fn is_iws_status(name: &str, prefixes: &[String]) -> bool {
    match name.split_once(':') {
        Some((p, "status")) => prefixes.iter().any(|x| x == p),
        _ => false,
    }
}

fn iws_of(buf: &[u8], start: usize, end: usize) -> IwsStatus {
    let tag = TagInfo::parse(buf, start, end);
    IwsStatus {
        translation: tag.value(buf, "translation_status").unwrap_or_default(),
        lock: tag.value(buf, "lock_status").unwrap_or_default(),
        slot: AttrSlot::from_tag(buf, &tag, "translation_status"),
    }
}

fn mid_of(buf: &[u8], tag: &TagInfo) -> Option<String> {
    tag.value(buf, "mid")
}

fn is_seg_mrk(buf: &[u8], tag: &TagInfo) -> bool {
    tag.attr(buf, "mtype").is_some_and(|a| &buf[a.value.clone()] == b"seg")
}

fn empty_state(target_empty: bool) -> SegState {
    if target_empty {
        SegState::New
    } else {
        SegState::Draft
    }
}

/// Trados `conf` to a state. `ApprovedTranslation` is `Confirmed`, `ApprovedSignOff` is `SignedOff` and a
/// rejection is `Rejected` (while there is a target to reject).
pub fn sdl_state_from(conf: &str, target_empty: bool) -> SegState {
    match conf {
        "Translated" => SegState::Translated,
        "ApprovedTranslation" => SegState::Confirmed,
        "ApprovedSignOff" => SegState::SignedOff,
        "RejectedTranslation" | "RejectedSignOff" if !target_empty => SegState::Rejected,
        _ => empty_state(target_empty),
    }
}

/// `None` = no `conf` attribute (unspecified).
pub fn sdl_conf_to(s: SegState) -> Option<&'static str> {
    match s {
        SegState::New => None,
        SegState::Draft => Some("Draft"),
        SegState::Translated => Some("Translated"),
        SegState::Confirmed | SegState::Reviewed => Some("ApprovedTranslation"),
        SegState::SignedOff | SegState::Locked => Some("ApprovedSignOff"),
        SegState::Rejected => Some("RejectedTranslation"),
        // a state added later: accepted or not
        other => if other.is_accepted() { Some("Translated") } else { Some("Draft") },
    }
}

/// Real memoQ files (document.mqxliff, mt.mqxliff) show `ManuallyConfirmed`, `MachineTranslated` and
/// `PreTranslated` on trans-units, `NotStarted` and `PartiallyEdited` in `mq:historical-unit`. A
/// confirmed unit with an EMPTY target exists in a real file (memoQ allows confirming an empty
/// segment), so `ManuallyConfirmed` stays `Translated` whatever the target holds. The names
/// `Proofread`, `Reviewer1Confirmed` and `Reviewer2Confirmed` come from memoQ documentation, not
/// from a sample file here.
pub fn mq_state_from(status: &str, target_empty: bool) -> SegState {
    match status {
        "NotStarted" => SegState::New,
        "ManuallyConfirmed" => SegState::Translated,
        "Reviewer1Confirmed" => SegState::Confirmed,
        "Proofread" => SegState::Reviewed,
        "Reviewer2Confirmed" => SegState::SignedOff,
        _ => empty_state(target_empty),
    }
}

pub fn mq_state_to(s: SegState) -> &'static str {
    match s {
        SegState::New => "NotStarted",
        SegState::Draft | SegState::Rejected => "PartiallyEdited",
        SegState::Translated => "ManuallyConfirmed",
        SegState::Confirmed => "Reviewer1Confirmed",
        SegState::Reviewed => "Proofread",
        SegState::SignedOff | SegState::Locked => "Reviewer2Confirmed",
        // a state added later: accepted or not
        other => if other.is_accepted() { "ManuallyConfirmed" } else { "PartiallyEdited" },
    }
}

/// Prefix bound to the Phrase namespace in the root element (`m` when absent).
fn mx_prefix(buf: &[u8]) -> String {
    let head = String::from_utf8_lossy(&buf[..buf.len().min(16 * 1024)]);
    if let Some(p) = head.find("=\"http://www.memsource.com/mxlf") {
        if let Some(x) = head[..p].rfind("xmlns:") {
            let name = &head[x + 6..p];
            if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
                return name.to_owned();
            }
        }
    }
    "m".to_owned()
}

fn mx_flag(v: Option<String>) -> bool {
    matches!(v.as_deref().map(str::trim), Some("1" | "true" | "True" | "locked" | "yes"))
}

fn mx_meta(buf: &[u8], tag: &TagInfo, p: &str) -> MxMeta {
    let pre = format!("{p}:");
    let mut meta = MxMeta::default();
    for a in &tag.attrs {
        let full = String::from_utf8_lossy(&buf[a.name.clone()]).into_owned();
        let Some(name) = full.strip_prefix(&pre) else { continue };
        let val = crate::attrs::unescape_attr(&buf[a.value.clone()]);
        match name {
            "confirmed" => meta.confirmed = mx_flag(Some(val)),
            "locked" => meta.locked = mx_flag(Some(val)),
            "score" => meta.score = val.trim().parse().ok(),
            "gross-score" => meta.gross_score = val.trim().parse().ok(),
            "trans-origin" => meta.trans_origin = Some(val),
            "para-id" => meta.para_id = Some(val),
            "level" => meta.level = Some(val),
            _ => meta.other.push((name.to_owned(), val)),
        }
    }
    meta
}

pub fn plain_state_from(state: &str, target_empty: bool) -> SegState {
    match state {
        "translated" => SegState::Translated,
        "final" => SegState::Confirmed,
        "signed-off" => SegState::SignedOff,
        _ => empty_state(target_empty),
    }
}

pub fn plain_state_to(s: SegState) -> &'static str {
    match s {
        SegState::New => "new",
        SegState::Draft => "needs-translation",
        SegState::Rejected => "needs-review-translation",
        SegState::Translated => "translated",
        SegState::Confirmed | SegState::Reviewed | SegState::Locked => "final",
        SegState::SignedOff => "signed-off",
        // a state added later: accepted or not
        other => if other.is_accepted() { "translated" } else { "needs-translation" },
    }
}

/// Display strings from SDLXLIFF `<tag-defs>`.
fn parse_tag_defs(buf: &[u8]) -> TagDefs {
    let mut defs = TagDefs::new();
    let Some(a) = find(buf, b"<tag-defs") else { return defs };
    let Some(b) = find(&buf[a..], b"</tag-defs>") else { return defs };
    let slice = &buf[a..a + b + 11];
    let mut rd = Reader::from_reader(slice);
    let mut cur_id = String::new();
    let mut cur_kind: Option<u8> = None;
    let mut text = String::new();
    loop {
        let start = rd.buffer_position() as usize;
        let Ok(ev) = rd.read_event() else { break };
        let end = rd.buffer_position() as usize;
        match ev {
            Event::Eof => break,
            Event::Start(e) => match local(e.name().as_ref()) {
                b"tag" => {
                    cur_id = TagInfo::parse(slice, start, end).value(slice, "id").unwrap_or_default();
                }
                n @ (b"bpt" | b"ept" | b"ph") => {
                    cur_kind = Some(n[0]);
                    text.clear();
                }
                _ => {}
            },
            Event::Text(t) => {
                if cur_kind.is_some() {
                    text.push_str(&t.decode().unwrap_or_default());
                }
            }
            Event::GeneralRef(r) => {
                if cur_kind.is_some() {
                    if let Some(c) = r.decode().ok().and_then(|n| crate::attrs::decode_entity(&n)) {
                        text.push(c);
                    }
                }
            }
            Event::End(e) => {
                if let Some(k) = cur_kind.take() {
                    if matches!(local(e.name().as_ref()), b"bpt" | b"ept" | b"ph") {
                        let d: &mut TagDef = defs.entry(cur_id.clone()).or_default();
                        let t = Some(std::mem::take(&mut text));
                        match k {
                            b'b' => d.open = t,
                            b'e' => d.close = t,
                            _ => d.standalone = t,
                        }
                    } else {
                        cur_kind = Some(k);
                    }
                }
            }
            _ => {}
        }
    }
    defs
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub fn parse(buf: &[u8], format: Format) -> Result<ParsedDoc, Error> {
    let (doc, segmented) = parse_inner(buf, format, false)?;
    if format == Format::Sdlxliff && !segmented {
        // A Trados file saved before segmentation: no `mrk mtype="seg"` anywhere. Every trans-unit
        // becomes one segment, so the translator is not shown an empty file.
        return Ok(parse_inner(buf, format, true)?.0);
    }
    Ok(doc)
}

/// Returns the parsed document and whether any trans-unit carried a segmentation `mrk`.
fn parse_inner(buf: &[u8], format: Format, unsegmented: bool) -> Result<(ParsedDoc, bool), Error> {
    let mut any_mrk = false;
    let tag_defs = if format == Format::Sdlxliff { Some(parse_tag_defs(buf)) } else { None };
    let iws_names = iws_prefixes(buf);
    let mut note_open: Option<(TagInfo, usize)> = None;
    let mut rd = Reader::from_reader(buf);
    let mut stack: Vec<K> = Vec::new();
    let mut recs: Vec<SegRec> = Vec::new();
    let mut units: Vec<UnitInfo> = Vec::new();
    let mut acc: Option<Acc> = None;
    let mut tu_depth = 0usize;
    let mut region: Option<Region> = None;
    let mut content_start = 0usize;
    let mut elem_start = 0usize;
    let mut ss_start = 0usize;
    let mut mrk_mid = String::new();
    let mut src_lang = String::new();
    let mut groups: Vec<Option<bool>> = Vec::new();
    let mut tgt_lang = String::new();
    loop {
        let start = rd.buffer_position() as usize;
        let ev = rd.read_event().map_err(|e| Error::Xml { pos: start, msg: e.to_string() })?;
        let end = rd.buffer_position() as usize;
        match ev {
            Event::Eof => break,
            Event::Start(e) => {
                let name = e.name();
                let nm = local(name.as_ref());
                let direct = acc.is_some() && stack.len() == tu_depth;
                let k = if nm == b"group" && acc.is_none() {
                    let tag = TagInfo::parse(buf, start, end);
                    groups.push(match tag.value(buf, "translate").as_deref() {
                        Some("no") => Some(true),
                        Some("yes") => Some(false),
                        _ => None,
                    });
                    K::Group
                } else if nm == b"trans-unit" {
                    let tag = TagInfo::parse(buf, start, end);
                    let id = tag.value(buf, "id").unwrap_or_default();
                    acc = Some(Acc {
                        tag,
                        id,
                        group_locked: groups.iter().rev().find_map(|g| *g).unwrap_or(false),
                        source: None,
                        seg_source: None,
                        ss_elem: None,
                        src_mrks: Vec::new(),
                        target: None,
                        target_tag: None,
                        tgt_mrks: Vec::new(),
                        defs: Defs::Absent,
                        defs_prefix: "sdl:".to_owned(),
                        def_segs: Vec::new(),
                        notes: Vec::new(),
                        iws: None,
                    });
                    tu_depth = stack.len() + 1;
                    K::Tu
                } else if nm == b"file" && acc.is_none() {
                    if src_lang.is_empty() {
                        let tag = TagInfo::parse(buf, start, end);
                        src_lang = tag.value(buf, "source-language").unwrap_or_default();
                        tgt_lang = tag.value(buf, "target-language").unwrap_or_default();
                    }
                    K::Other
                } else if direct && nm == b"source" {
                    content_start = end;
                    elem_start = start;
                    K::Source
                } else if direct && nm == b"seg-source" {
                    ss_start = end;
                    elem_start = start;
                    region = Some(Region::SegSource);
                    K::SegSource
                } else if direct && nm == b"target" {
                    content_start = end;
                    region = Some(Region::Target);
                    if let Some(a) = acc.as_mut() {
                        a.target_tag = Some(TagInfo::parse(buf, start, end));
                    }
                    K::Target
                } else if direct && nm == b"seg-defs" {
                    if let Some(a) = acc.as_mut() {
                        let n = name.as_ref();
                        a.defs_prefix = match n.iter().position(|&b| b == b':') {
                            Some(p) => String::from_utf8_lossy(&n[..=p]).into_owned(),
                            None => String::new(),
                        };
                    }
                    K::SegDefs
                } else if region.is_some() && nm == b"mrk" {
                    let tag = TagInfo::parse(buf, start, end);
                    if is_seg_mrk(buf, &tag) {
                        mrk_mid = mid_of(buf, &tag).unwrap_or_default();
                        content_start = end;
                        K::MrkSeg
                    } else {
                        K::Other
                    }
                } else if stack.last() == Some(&K::SegDefs) && nm == b"seg" {
                    if let Some(a) = acc.as_mut() {
                        a.def_segs.push(def_seg(buf, start, end, false, &a.defs_prefix.clone()));
                    }
                    K::Other
                } else if direct && nm == b"note" {
                    note_open = Some((TagInfo::parse(buf, start, end), end));
                    K::Note
                } else if acc.as_ref().is_some_and(|a| a.iws.is_none()) && is_iws_status(&String::from_utf8_lossy(name.as_ref()), &iws_names) {
                    if let Some(a) = acc.as_mut() {
                        a.iws = Some(iws_of(buf, start, end));
                    }
                    K::Other
                } else {
                    K::Other
                };
                stack.push(k);
            }
            Event::Empty(e) => {
                let name = e.name();
                let nm = local(name.as_ref());
                let direct = acc.is_some() && stack.len() == tu_depth;
                if let Some(a) = acc.as_mut() {
                    if direct && nm == b"target" {
                        let tag = TagInfo::parse(buf, start, end);
                        a.target = Some(UTarget::EmptyElem { range: start..end, name: tag.name_str(buf).to_owned() });
                        a.target_tag = Some(tag);
                    } else if direct && nm == b"seg-defs" {
                        let tag = TagInfo::parse(buf, start, end);
                        a.defs_prefix = match tag.name_str(buf).find(':') {
                            Some(p) => tag.name_str(buf)[..=p].to_owned(),
                            None => String::new(),
                        };
                        a.defs = Defs::EmptyElem { range: start..end, name: tag.name_str(buf).to_owned() };
                    } else if direct && nm == b"source" {
                        a.source = Some((end..end, end, start));
                    } else if a.iws.is_none() && is_iws_status(&String::from_utf8_lossy(name.as_ref()), &iws_names) {
                        a.iws = Some(iws_of(buf, start, end));
                    } else if stack.last() == Some(&K::SegDefs) && nm == b"seg" {
                        a.def_segs.push(def_seg(buf, start, end, true, &a.defs_prefix.clone()));
                    } else if region.is_some() && nm == b"mrk" {
                        let tag = TagInfo::parse(buf, start, end);
                        if is_seg_mrk(buf, &tag) {
                            let mid = mid_of(buf, &tag).unwrap_or_default();
                            match region {
                                Some(Region::SegSource) => a.src_mrks.push((mid, end..end)),
                                _ => a.tgt_mrks.push((
                                    mid,
                                    TgtSlot::Empty { range: start..end, name: tag.name_str(buf).to_owned() },
                                )),
                            }
                        }
                    }
                }
            }
            Event::End(_) => {
                let k = stack.pop().unwrap_or(K::Other);
                match k {
                    K::Source => {
                        if let Some(a) = acc.as_mut() {
                            a.source = Some((content_start..start, end, elem_start));
                        }
                    }
                    K::SegSource => {
                        region = None;
                        if let Some(a) = acc.as_mut() {
                            a.seg_source = Some(ss_start..start);
                            a.ss_elem = Some((elem_start, end));
                        }
                    }
                    K::Target => {
                        region = None;
                        if let Some(a) = acc.as_mut() {
                            a.target = Some(UTarget::Present { content: content_start..start, close_start: start });
                        }
                    }
                    K::SegDefs => {
                        if let Some(a) = acc.as_mut() {
                            a.defs = Defs::Open { close_start: start };
                        }
                    }
                    K::MrkSeg => {
                        if let Some(a) = acc.as_mut() {
                            let mid = std::mem::take(&mut mrk_mid);
                            match region {
                                Some(Region::SegSource) => a.src_mrks.push((mid, content_start..start)),
                                _ => a.tgt_mrks.push((mid, TgtSlot::Content(content_start..start))),
                            }
                        }
                    }
                    K::Note => {
                        if let (Some((tag, from)), Some(a)) = (note_open.take(), acc.as_mut()) {
                            a.notes.push((tag, from..start));
                        }
                    }
                    K::Tu => {
                        if let Some(a) = acc.take() {
                            any_mrk |= !a.src_mrks.is_empty();
                            finish_unit(buf, format, a, start, unsegmented, &mut recs, &mut units)?;
                        }
                    }
                    K::Group => {
                        groups.pop();
                    }
                    K::Other => {}
                }
            }
            _ => {}
        }
    }
    let unit_ids = units.iter().map(|u| u.id.clone()).collect();
    let unit_notes = units.iter().map(|u| u.notes.clone()).collect();
    let unit_data = vec![Vec::new(); units.len()];
    Ok((
        ParsedDoc {
            format,
            source_lang: src_lang,
            target_lang: tgt_lang,
            recs,
            layout: Layout::X12(units),
            tag_defs,
            unit_ids,
            unit_notes,
            unit_data,
        },
        any_mrk,
    ))
}

/// True when inline XML holds text that a translator would see: text outside tags, and outside the
/// payload of `bpt`, `ept`, `ph` and `it` elements. Whitespace does not count.
fn has_text(xml: &[u8]) -> bool {
    let mut i = 0;
    let mut payload = 0usize;
    while i < xml.len() {
        if xml[i] == b'<' {
            if xml[i..].starts_with(b"<!--") {
                i += xml[i..].windows(3).position(|w| w == b"-->").map_or(xml.len() - i, |p| p + 3);
                continue;
            }
            let close = xml.get(i + 1) == Some(&b'/');
            let mut j = i + 1 + usize::from(close);
            let ns = j;
            while j < xml.len() && !(xml[j].is_ascii_whitespace() || matches!(xml[j], b'>' | b'/')) {
                j += 1;
            }
            let name = local(&xml[ns..j]).to_vec();
            let mut quote = 0u8;
            while j < xml.len() {
                match (quote, xml[j]) {
                    (0, b'"' | b'\x27') => quote = xml[j],
                    (q, c) if q != 0 && q == c => quote = 0,
                    (0, b'>') => break,
                    _ => {}
                }
                j += 1;
            }
            let self_closed = j > 0 && xml.get(j - 1) == Some(&b'/');
            if matches!(name.as_slice(), b"bpt" | b"ept" | b"ph" | b"it") && !self_closed {
                if close {
                    payload = payload.saturating_sub(1);
                } else {
                    payload += 1;
                }
            }
            i = j + 1;
        } else {
            if payload == 0 && !xml[i].is_ascii_whitespace() {
                return true;
            }
            i += 1;
        }
    }
    false
}

fn def_seg(buf: &[u8], start: usize, end: usize, empty: bool, prefix: &str) -> DefSeg {
    let tag = TagInfo::parse(buf, start, end);
    let origin = AttrSlot::from_tag(buf, &tag, "origin");
    let prev_origin = match &origin {
        AttrSlot::Existing { raw, .. } if !raw.is_empty() && raw != "interactive" => {
            let mut s = format!("<{prefix}prev-origin");
            for name in ["origin", "origin-system", "percent", "struct-match", "text-match"] {
                if let Some(a) = tag.attr(buf, name) {
                    // Raw (still escaped) bytes are copied as they are.
                    s.push_str(&format!(" {name}=\"{}\"", String::from_utf8_lossy(&buf[a.value.clone()])));
                }
            }
            s.push_str("/>");
            Some(s)
        }
        _ => None,
    };
    DefSeg {
        mid: tag.value(buf, "id").unwrap_or_default(),
        locked: tag.value(buf, "locked").as_deref() == Some("true"),
        def: SdlDef {
            conf: AttrSlot::from_tag(buf, &tag, "conf"),
            origin,
            tag: start..end,
            empty,
            name: tag.name_str(buf).to_owned(),
            prev_origin,
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_unit(
    buf: &[u8],
    format: Format,
    a: Acc,
    tu_close: usize,
    unsegmented: bool,
    recs: &mut Vec<SegRec>,
    units: &mut Vec<UnitInfo>,
) -> Result<(), Error> {
    let first = recs.len();
    let unit_ix = units.len();
    let notes = unit_notes(buf, &a);
    let iws_finished = a.iws.as_ref().is_some_and(IwsStatus::finished);
    let iws_locked = a.iws.as_ref().is_some_and(IwsStatus::locked);
    let iws_slot = a.iws.as_ref().map(|i| i.slot.clone());
    let (src_elem_end, src_elem_start) = a.source.as_ref().map_or((tu_close, tu_close), |s| (s.1, s.2));
    let utarget = a.target.clone().unwrap_or_else(|| match a.ss_elem {
        // SDL: the target follows seg-source; otherwise it follows source.
        Some((s, e)) => UTarget::Missing { pos: e, prefix: line_prefix(buf, s) },
        None => UTarget::Missing { pos: src_elem_end, prefix: line_prefix(buf, src_elem_start) },
    });
    let tu_locked = match a.tag.value(buf, "translate").as_deref() {
        Some("no") => true,
        Some("yes") => false,
        _ => a.group_locked,
    };

    if let Some(ss) = a.seg_source.clone() {
        // SDL-style segmentation. A seg-source with no `mrk mtype="seg"` that holds TEXT is one
        // segment spanning the whole seg-source (a file that was never segmented); its id is the
        // trans-unit id and its target is the whole `<target>` content. A seg-source that holds
        // only tags (`<x id="3"/>`) is not a segment: real Trados output (32415989.sdlxliff has 15
        // such units next to 68 segmented ones) leaves these out of the editor, and so do we.
        // In a file that has no segmentation at all (`unsegmented`), a tag-only unit is a segment too.
        let whole = a.src_mrks.is_empty()
            && (has_text(&buf[ss.clone()]) || (unsegmented && !buf[ss.clone()].iter().all(u8::is_ascii_whitespace)));
        let src_mrks: Vec<(String, Range<usize>)> =
            if whole { vec![(a.id.clone(), ss.clone())] } else { a.src_mrks.clone() };
        for (mid, src_range) in &src_mrks {
            let tslot = match (&utarget, a.tgt_mrks.iter().find(|(m, _)| m == mid)) {
                (UTarget::Present { content, .. }, None) if whole => TgtSlot::Content(content.clone()),
                (UTarget::Present { .. }, Some((_, s))) => s.clone(),
                (UTarget::Present { close_start, .. }, None) => TgtSlot::InTarget { pos: *close_start },
                _ => TgtSlot::Unit,
            };
            let tgt_span = match &tslot {
                TgtSlot::Content(r) => Some(r.clone()),
                _ => None,
            };
            let def = a.def_segs.iter().find(|d| &d.mid == mid);
            let (conf_raw, slot, locked) = match def {
                Some(d) => {
                    let raw = match &d.def.conf {
                        AttrSlot::Existing { raw, .. } => raw.clone(),
                        AttrSlot::Missing { .. } => String::new(),
                    };
                    (raw, Some(d.def.clone()), d.locked)
                }
                None => (String::new(), None, false),
            };
            let mut state = sdl_state_from(&conf_raw, tgt_span.as_ref().is_none_or(|r| r.is_empty()));
            if iws_finished && state.rank() < SegState::Translated.rank() {
                state = SegState::Translated;
            }
            let id = mid.clone();
            let lock_reason = if tu_locked {
                Some("translate=no".to_owned())
            } else if locked {
                Some("locked in Trados (sdl:seg)".to_owned())
            } else if iws_locked {
                Some("locked in WorldServer (iws:status)".to_owned())
            } else {
                None
            };
            let origin = def.and_then(|d| sdl_origin(buf, d));
            recs.push(SegRec {
                seg: Meta { id, state, native: (!conf_raw.is_empty()).then(|| conf_raw.clone()) },
                locked: lock_reason.is_some(),
                lock_reason,
                origin,
                unit_ix,
                src: src_range.clone(),
                tgt: tgt_span,
                content: Default::default(),
                orig_state: state,
                target_dirty: false,
                loc: Loc::X12(X12Loc {
                    mid: Some(mid.clone()),
                    tgt: tslot,
                    state: StateLoc::Sdl(slot),
                    approved: None,
                    mx: None,
                }),
            });
        }
        units.push(UnitInfo {
            target: utarget,
            seg_source: Some(ss),
            src_ranges: src_mrks.iter().map(|(_, r)| r.clone()).collect(),
            defs: a.defs.clone(),
            defs_prefix: a.defs_prefix.clone(),
            tu_close,
            first,
            count: recs.len() - first,
            id: a.id.clone(),
            notes,
            iws: iws_slot,
        });
        return Ok(());
    }

    // One segment per trans-unit (MQXLIFF and plain XLIFF 1.2).
    let Some((src_range, _, _)) = a.source.clone() else {
        return Ok(());
    };
    if format == Format::Sdlxliff && (!unsegmented || buf[src_range.clone()].iter().all(u8::is_ascii_whitespace)) {
        // No `seg-source` inside a segmented file (`translate="no"`, or not segmented yet): not editable here.
        return Ok(());
    }
    let (tgt_span, tslot) = match &utarget {
        UTarget::Present { content, .. } => (Some(content.clone()), TgtSlot::Content(content.clone())),
        _ => (None, TgtSlot::Unit),
    };
    let empty = tgt_span.as_ref().is_none_or(|r| r.is_empty());
    let approved = a.tag.attr(buf, "approved").map(|at| (at.value.clone(), crate::attrs::unescape_attr(&buf[at.value.clone()])));
    let mut mx: Option<Box<MxMeta>> = None;
    let (state, native, sloc) = if format == Format::Mxliff {
        let p = mx_prefix(buf);
        let meta = mx_meta(buf, &a.tag, &p);
        let slot = AttrSlot::from_tag(buf, &a.tag, &format!("{p}:confirmed"));
        let st = if meta.confirmed { SegState::Translated } else { empty_state(empty) };
        mx = Some(Box::new(meta));
        (st, None, StateLoc::Mx(slot))
    } else if a.tag.attr(buf, "mq:status").is_some() || format == Format::Mqxliff {
        let slot = AttrSlot::from_tag(buf, &a.tag, "mq:status");
        let raw = match &slot {
            AttrSlot::Existing { raw, .. } => raw.clone(),
            AttrSlot::Missing { .. } => String::new(),
        };
        (mq_state_from(&raw, empty), (!raw.is_empty()).then(|| raw.clone()), StateLoc::Mq(slot))
    } else {
        let slot = a.target_tag.as_ref().map(|t| AttrSlot::from_tag(buf, t, "state"));
        let raw = match &slot {
            Some(AttrSlot::Existing { raw, .. }) => raw.clone(),
            _ => String::new(),
        };
        let mut st = plain_state_from(&raw, empty);
        if approved.as_ref().is_some_and(|(_, v)| v == "yes") && st.rank() < SegState::Confirmed.rank() {
            st = SegState::Confirmed;
        }
        (st, (!raw.is_empty()).then(|| raw.clone()), StateLoc::TargetState(slot))
    };
    let (state, native) = if iws_finished && state.rank() < SegState::Translated.rank() {
        (SegState::Translated, native)
    } else {
        (state, native)
    };
    let lock_reason = if tu_locked {
        Some("translate=no".to_owned())
    } else if a.tag.value(buf, "mq:locked").as_deref() == Some("locked") {
        Some("locked in memoQ (mq:locked)".to_owned())
    } else if mx.as_ref().is_some_and(|m| m.locked) {
        Some("locked in Phrase (m:locked)".to_owned())
    } else if iws_locked {
        Some("locked in WorldServer (iws:status)".to_owned())
    } else {
        None
    };
    let origin = plain_origin(buf, &a, mx.as_deref(), native.as_deref());
    let id = a.id.clone();
    recs.push(SegRec {
        seg: Meta { id, state, native },
        locked: lock_reason.is_some(),
        lock_reason,
        origin,
        unit_ix,
        src: src_range,
        tgt: tgt_span,
        content: Default::default(),
        orig_state: state,
        target_dirty: false,
        loc: Loc::X12(X12Loc { mid: None, tgt: tslot, state: sloc, approved, mx }),
    });
    units.push(UnitInfo {
        target: utarget,
        seg_source: None,
        src_ranges: Vec::new(),
        defs: Defs::Absent,
        defs_prefix: String::new(),
        tu_close,
        first,
        count: 1,
        id: a.id.clone(),
        notes,
        iws: iws_slot,
    });
    Ok(())
}

/// The trans-unit's `<note>` elements as notes (`from` is the author, `annotates` the kind).
fn unit_notes(buf: &[u8], a: &Acc) -> Vec<Note> {
    a.notes
        .iter()
        .filter_map(|(tag, r)| {
            let text = crate::inline::parse_inlines(&buf[r.clone()], Flavor::X12, None, None)
                .map(|p| crate::inline::plain(&p.inlines))
                .unwrap_or_else(|_| String::from_utf8_lossy(&buf[r.clone()]).into_owned());
            (!text.trim().is_empty()).then(|| Note::new(text).with_author(tag.value(buf, "from").filter(|f| !f.is_empty())).with_kind(tag.value(buf, "annotates").filter(|f| !f.is_empty())))
        })
        .collect()
}

fn pct_of(v: Option<String>) -> Option<u8> {
    v.and_then(|p| p.trim().trim_end_matches('%').parse::<f64>().ok()).filter(|p| *p >= 0.0).map(|p| p.round().min(255.0) as u8)
}

/// Where a Trados segment's target came from: `sdl:seg origin` (`tm`, `mt`, `interactive`, `auto-propagated`,
/// `document-match`, ...) with `origin-system` and `percent`.
fn sdl_origin(buf: &[u8], d: &DefSeg) -> Option<Origin> {
    let AttrSlot::Existing { raw, .. } = &d.def.origin else { return None };
    if raw.is_empty() {
        return None;
    }
    let tag = TagInfo::parse(buf, d.def.tag.start, d.def.tag.end);
    let system = tag.value(buf, "origin-system").filter(|s| !s.is_empty());
    let (kind, name) = match raw.as_str() {
        "tm" => (OriginKind::Tm, system),
        "mt" => (OriginKind::Mt, system),
        "interactive" => (OriginKind::Human, system),
        other => (OriginKind::Other, Some(other.to_owned())),
    };
    Some(Origin::new(kind).with_name(name).with_match_pct(pct_of(tag.value(buf, "percent"))))
}

/// Origin of a one-segment trans-unit: memoQ machine and pre-translation, Phrase `m:trans-origin` and
/// `m:score`, XLIFF 1.2 `state-qualifier`.
fn plain_origin(buf: &[u8], a: &Acc, mx: Option<&MxMeta>, native: Option<&str>) -> Option<Origin> {
    if let Some(m) = mx {
        let t = m.trans_origin.clone()?;
        let low = t.to_ascii_lowercase();
        let kind = if low.contains("mt") || low.contains("machine") {
            OriginKind::Mt
        } else if low.contains("tm") {
            OriginKind::Tm
        } else {
            OriginKind::Other
        };
        let match_pct = m.score.filter(|s| *s >= 0.0).map(|s| (if s <= 1.0 { s * 100.0 } else { s }).round().min(255.0) as u8);
        return Some(Origin::new(kind).with_name(t).with_match_pct(match_pct));
    }
    if a.tag.attr(buf, "mq:status").is_some() {
        let kind = match native {
            Some("MachineTranslated") => OriginKind::Mt,
            Some("PreTranslated") => OriginKind::Tm,
            _ => return None,
        };
        return Some(Origin::new(kind).with_match_pct(pct_of(a.tag.value(buf, "mq:percent"))));
    }
    let q = a.target_tag.as_ref()?.value(buf, "state-qualifier")?;
    let (kind, pct) = match q.as_str() {
        "exact-match" | "id-match" => (OriginKind::Tm, Some(100)),
        "fuzzy-match" | "leveraged-tm" | "leveraged-repository" | "tm-suggestion" | "leveraged-inherited" => (OriginKind::Tm, None),
        "mt-suggestion" | "leveraged-mt" => (OriginKind::Mt, None),
        _ => return None,
    };
    Some(Origin::new(kind).with_match_pct(pct))
}

/// Patches for every edited segment.
pub fn patches(buf: &[u8], recs: &[SegRec], units: &[UnitInfo]) -> Vec<Patch> {
    let mut out = Vec::new();
    for unit in units {
        let segs = &recs[unit.first..unit.first + unit.count];
        if segs.iter().all(|r| !r.target_dirty && r.seg.state == r.orig_state) {
            continue;
        }
        if unit.seg_source.is_some() {
            sdl_unit(buf, unit, segs, &mut out);
        } else if let Some(r) = segs.first() {
            plain_unit(buf, unit, r, &mut out);
        }
        // GlobalLink: a segment that became finished work says so in its `iws:status`.
        if let Some(slot) = &unit.iws {
            if segs.iter().any(|r| r.seg.state != r.orig_state && r.seg.state.is_accepted()) {
                out.extend(slot.patch("translation_status", Some("finished"), None));
            }
        }
    }
    out
}

fn sdl_unit(buf: &[u8], unit: &UnitInfo, segs: &[SegRec], out: &mut Vec<Patch>) {
    // Targets.
    let any_target = segs.iter().any(|r| r.target_dirty);
    if any_target {
        match &unit.target {
            UTarget::Present { .. } => {
                for r in segs.iter().filter(|r| r.target_dirty) {
                    let Loc::X12(loc) = &r.loc else { continue };
                    let Some(c) = r.content.get() else { continue };
                    let body = serialize(&c.target, Flavor::X12, &c.raws);
                    match &loc.tgt {
                        TgtSlot::Content(range) => out.push(Patch::replace(range.clone(), body)),
                        TgtSlot::Empty { range, name } => {
                            let head = String::from_utf8_lossy(&buf[range.clone()]).into_owned();
                            let head = head.trim_end_matches('>').trim_end_matches('/').trim_end().to_owned();
                            out.push(Patch::replace(range.clone(), format!("{head}>{body}</{name}>")));
                        }
                        TgtSlot::InTarget { pos } => {
                            let mid = loc.mid.clone().unwrap_or_default();
                            out.push(Patch::insert(*pos, format!("<mrk mtype=\"seg\" mid=\"{mid}\">{body}</mrk>")));
                        }
                        TgtSlot::Unit => {}
                    }
                }
            }
            UTarget::Missing { .. } | UTarget::EmptyElem { .. } => {
                let skeleton = skeleton(buf, unit, segs);
                match &unit.target {
                    UTarget::Missing { pos, prefix } => {
                        out.push(Patch::insert(*pos, format!("{prefix}<target>{skeleton}</target>")));
                    }
                    UTarget::EmptyElem { range, name } => {
                        let head = String::from_utf8_lossy(&buf[range.clone()]).into_owned();
                        let head = head.trim_end_matches('>').trim_end_matches('/').trim_end().to_owned();
                        out.push(Patch::replace(range.clone(), format!("{head}>{skeleton}</{name}>")));
                    }
                    UTarget::Present { .. } => {}
                }
            }
        }
    }
    // States and origin. A human edit of a target is what Trados records as
    // `origin="interactive"`; when the segment came from a TM or MT match, the old origin moves
    // into a `prev-origin` child (real file: Okapi prev-origin-sdl.sdlxliff).
    let mut new_defs: Vec<String> = Vec::new();
    for r in segs.iter().filter(|r| r.target_dirty || r.seg.state != r.orig_state) {
        let Loc::X12(loc) = &r.loc else { continue };
        let StateLoc::Sdl(def) = &loc.state else { continue };
        let state_changed = r.seg.state != r.orig_state;
        let want = crate::x12::sdl_conf_to(r.seg.state);
        let confirmed_now = state_changed && r.seg.state.is_accepted();
        // A human edit, or a human confirming a segment that has no origin yet (the real return package
        // test-packages.sdlrpx shows `<sdl:seg id="1"/>` becoming `conf="Translated" origin="interactive"`).
        let human_edit = (r.target_dirty && r.seg.state != SegState::New)
            || (confirmed_now && matches!(def, None | Some(SdlDef { origin: AttrSlot::Missing { .. }, .. })));
        match def {
            Some(d) => {
                let mut attr_patches: Vec<Patch> = Vec::new();
                if state_changed {
                    attr_patches.extend(d.conf.patch("conf", want, None));
                }
                let mut prev: Option<&str> = None;
                if human_edit {
                    attr_patches.extend(d.origin.patch("origin", Some("interactive"), None));
                    prev = d.prev_origin.as_deref();
                }
                if d.empty && (prev.is_some() || !attr_patches.is_empty()) {
                    // `<sdl:seg .../>`: rebuild the element with the attribute changes applied.
                    let rel = attr_patches
                        .into_iter()
                        .map(|p| Patch { range: p.range.start - d.tag.start..p.range.end - d.tag.start, bytes: p.bytes })
                        .collect();
                    let head = apply(&buf[d.tag.clone()], rel);
                    let head = String::from_utf8_lossy(&head).into_owned();
                    let head = head.trim_end_matches('>').trim_end_matches('/').trim_end().to_owned();
                    let text = match prev {
                        Some(child) => format!("{head}>{child}</{}>", d.name),
                        None => format!("{head}/>"),
                    };
                    out.push(Patch::replace(d.tag.clone(), text));
                } else {
                    out.extend(attr_patches);
                    if let Some(p) = prev {
                        out.push(Patch::insert(d.tag.end, p));
                    }
                }
            }
            None => {
                let mid = loc.mid.clone().unwrap_or_default();
                let mut attrs = String::new();
                if state_changed {
                    if let Some(w) = want {
                        attrs.push_str(&format!(" conf=\"{w}\""));
                    }
                }
                if human_edit {
                    attrs.push_str(" origin=\"interactive\"");
                }
                if !attrs.is_empty() {
                    new_defs.push(format!("<{p}seg id=\"{mid}\"{attrs}/>", p = unit.defs_prefix));
                }
            }
        }
    }
    if !new_defs.is_empty() {
        let joined = new_defs.concat();
        match &unit.defs {
            Defs::Open { close_start } => out.push(Patch::insert(*close_start, joined)),
            Defs::EmptyElem { range, name } => {
                let head = String::from_utf8_lossy(&buf[range.clone()]).into_owned();
                let head = head.trim_end_matches('>').trim_end_matches('/').trim_end().to_owned();
                out.push(Patch::replace(range.clone(), format!("{head}>{joined}</{name}>")));
            }
            Defs::Absent => {
                let p = &unit.defs_prefix;
                out.push(Patch::insert(unit.tu_close, format!("<{p}seg-defs>{joined}</{p}seg-defs>")));
            }
        }
    }
}

/// The new `<target>` content for a unit that had none: `seg-source` with each segment
/// `mrk` filled from the in-memory target.
fn skeleton(buf: &[u8], unit: &UnitInfo, segs: &[SegRec]) -> String {
    let Some(inner) = &unit.seg_source else { return String::new() };
    let mut out = String::new();
    let mut at = inner.start;
    for (i, range) in unit.src_ranges.iter().enumerate() {
        out.push_str(&String::from_utf8_lossy(&buf[at..range.start]));
        if let Some(c) = segs.get(i).and_then(|r| r.content.get()) {
            out.push_str(&serialize(&c.target, Flavor::X12, &c.raws));
        }
        at = range.end;
    }
    out.push_str(&String::from_utf8_lossy(&buf[at..inner.end]));
    out
}

fn plain_unit(buf: &[u8], unit: &UnitInfo, r: &SegRec, out: &mut Vec<Patch>) {
    let Loc::X12(loc) = &r.loc else { return };
    let state_changed = r.seg.state != r.orig_state;
    // Attribute written into a generated or rewritten target element.
    let state_attr_for_new = |name_state: Option<&str>| name_state.map(|s| format!(" state=\"{s}\"")).unwrap_or_default();
    let body = r.content.get().map(|c| serialize(&c.target, Flavor::X12, &c.raws)).unwrap_or_default();
    match &unit.target {
        UTarget::Present { content, .. } => {
            if r.target_dirty {
                out.push(Patch::replace(content.clone(), body));
            }
            if state_changed {
                if let StateLoc::TargetState(Some(slot)) = &loc.state {
                    if let Some(p) = slot.patch("state", Some(plain_state_to(r.seg.state)), None) {
                        out.push(p);
                    }
                }
            }
        }
        UTarget::EmptyElem { range, name } => {
            if r.target_dirty || state_changed {
                let mut head_bytes = buf[range.clone()].to_vec();
                let mut rel: Vec<Patch> = Vec::new();
                if state_changed {
                    if let StateLoc::TargetState(Some(slot)) = &loc.state {
                        if let Some(p) = slot.patch("state", Some(plain_state_to(r.seg.state)), None) {
                            rel.push(Patch { range: p.range.start - range.start..p.range.end - range.start, bytes: p.bytes });
                        }
                    }
                }
                head_bytes = apply(&head_bytes, rel);
                let head = String::from_utf8_lossy(&head_bytes).into_owned();
                let head = head.trim_end_matches('>').trim_end_matches('/').trim_end().to_owned();
                if r.target_dirty {
                    out.push(Patch::replace(range.clone(), format!("{head}>{body}</{name}>")));
                } else {
                    out.push(Patch::replace(range.clone(), format!("{head}/>")));
                }
            }
        }
        UTarget::Missing { pos, prefix } => {
            if r.target_dirty || state_changed {
                let st = match &loc.state {
                    StateLoc::TargetState(_) => state_attr_for_new(Some(plain_state_to(r.seg.state))),
                    _ => String::new(),
                };
                out.push(Patch::insert(*pos, format!("{prefix}<target{st}>{body}</target>")));
            }
        }
    }
    if state_changed {
        match &loc.state {
            StateLoc::Mq(slot) => {
                if let Some(p) = slot.patch("mq:status", Some(mq_state_to(r.seg.state)), None) {
                    out.push(p);
                }
            }
            StateLoc::Mx(slot) => {
                // Only a translator's confirmation sets the flag; a draft never does.
                let on = r.seg.state.is_accepted();
                let words = matches!(slot, AttrSlot::Existing { raw, .. } if raw == "true" || raw == "false");
                let want = match (on, words) {
                    (true, true) => "true",
                    (true, false) => "1",
                    (false, true) => "false",
                    (false, false) => "0",
                };
                let name = format!("{}:confirmed", mx_prefix(buf));
                if let Some(p) = slot.patch(&name, Some(want), Some("0")) {
                    out.push(p);
                }
            }
            StateLoc::TargetState(_) => {
                if let Some((range, raw)) = &loc.approved {
                    let want = if r.seg.state.rank() >= SegState::Confirmed.rank() { "yes" } else { "no" };
                    if raw != want {
                        out.push(Patch::replace(range.clone(), want));
                    }
                }
            }
            StateLoc::Sdl(_) => {}
        }
    }
}
