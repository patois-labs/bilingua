//! XLIFF 2.x: `unit` / `segment` / `source` / `target`.
//!
//! One `Segment` per `<segment>`. The id is the unit id, or `unit#N` when a unit holds
//! several segments. `<ignorable>` content is not a segment.

use crate::attrs::TagInfo;
use crate::doc::{Layout, Loc, Meta, ParsedDoc, SegRec};
use crate::error::Error;
use crate::inline::{serialize, Flavor};
use crate::patch::{line_prefix, AttrSlot, Patch};
use crate::Format;
use bilingua_model::{Note, SegState};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::ops::Range;

#[derive(Debug, Clone)]
pub enum TSlot {
    /// `<target>…</target>`: replace the content.
    Content(Range<usize>),
    /// `<target/>`: replace the whole element.
    Empty { range: Range<usize>, name: String },
    /// No `<target>`: insert after `</source>`.
    Insert { pos: usize, prefix: String },
}

#[derive(Debug, Clone)]
pub struct X2Loc {
    pub tgt: TSlot,
    pub state: AttrSlot,
    /// ` subState="..."` (full range). It qualifies the old `state`, so it goes when the state changes.
    pub sub_state: Option<Range<usize>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum K {
    Group,
    Unit,
    Segment,
    Source,
    Target,
    Ignorable,
    Note,
    Data,
    Other,
}

/// `Some(true)` for `translate="no"` (locked), `Some(false)` for `"yes"`, `None` when absent.
fn translate_flag(buf: &[u8], tag: &TagInfo) -> Option<bool> {
    match tag.value(buf, "translate").as_deref() {
        Some("no") => Some(true),
        Some("yes") => Some(false),
        _ => None,
    }
}

fn local(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|&b| b == b':') {
        Some(p) => &name[p + 1..],
        None => name,
    }
}

/// XLIFF 2 `state` to a state: `translated`, `reviewed` and `final` are three steps.
pub fn state_from(raw: &str, target_empty: bool) -> SegState {
    match raw {
        "translated" => SegState::Translated,
        "reviewed" => SegState::Reviewed,
        "final" => SegState::Confirmed,
        _ => {
            if target_empty {
                SegState::New
            } else {
                SegState::Draft
            }
        }
    }
}

pub fn state_to(s: SegState) -> &'static str {
    match s {
        SegState::New | SegState::Draft | SegState::Rejected => "initial",
        SegState::Translated => "translated",
        SegState::Reviewed => "reviewed",
        SegState::Confirmed | SegState::SignedOff | SegState::Locked => "final",
        // a state added later: accepted or not
        other => if other.is_accepted() { "translated" } else { "initial" },
    }
}

struct CurSeg {
    tag: TagInfo,
    src: Option<(Range<usize>, usize, usize)>, // content, element end, element start
    tgt: Option<TSlot>,
}

pub fn parse(buf: &[u8]) -> Result<ParsedDoc, Error> {
    let mut rd = Reader::from_reader(buf);
    let mut stack: Vec<K> = Vec::new();
    let mut recs: Vec<SegRec> = Vec::new();
    let mut unit_id = String::new();
    let mut unit_locked = false;
    // `translate` of enclosing `file` and `group` elements (nearest explicit value wins).
    let mut scopes: Vec<Option<bool>> = Vec::new();
    let mut unit_first = 0usize;
    let mut cur: Option<CurSeg> = None;
    let mut content_start = 0usize;
    let mut elem_start = 0usize;
    let mut src_lang = String::new();
    let mut tgt_lang = String::new();
    let mut unit_ids: Vec<String> = Vec::new();
    let mut unit_notes: Vec<Vec<Note>> = Vec::new();
    let mut unit_data: Vec<Vec<(String, Range<usize>)>> = Vec::new();
    let mut cur_notes: Vec<Note> = Vec::new();
    let mut cur_data: Vec<(String, Range<usize>)> = Vec::new();
    let mut note_open: Option<(TagInfo, usize)> = None;
    let mut data_id = String::new();
    loop {
        let start = rd.buffer_position() as usize;
        let ev = rd.read_event().map_err(|e| Error::Xml { pos: start, msg: e.to_string() })?;
        let end = rd.buffer_position() as usize;
        match ev {
            Event::Eof => break,
            Event::Start(e) => {
                let parent = stack.last().copied();
                let k = match (local(e.name().as_ref()), parent) {
                    (b"group" | b"file", _) => {
                        let tag = TagInfo::parse(buf, start, end);
                        scopes.push(translate_flag(buf, &tag));
                        K::Group
                    }
                    (b"unit", _) => {
                        let tag = TagInfo::parse(buf, start, end);
                        unit_id = tag.value(buf, "id").unwrap_or_default();
                        unit_locked = translate_flag(buf, &tag)
                            .or_else(|| scopes.iter().rev().find_map(|g| *g))
                            .unwrap_or(false);
                        unit_first = recs.len();
                        cur_notes.clear();
                        cur_data.clear();
                        K::Unit
                    }
                    (b"xliff", _) => {
                        let tag = TagInfo::parse(buf, start, end);
                        src_lang = tag.value(buf, "srcLang").unwrap_or_default();
                        tgt_lang = tag.value(buf, "trgLang").unwrap_or_default();
                        K::Other
                    }
                    (b"segment", Some(K::Unit)) => {
                        cur = Some(CurSeg { tag: TagInfo::parse(buf, start, end), src: None, tgt: None });
                        K::Segment
                    }
                    (b"ignorable", Some(K::Unit)) => K::Ignorable,
                    (b"note", _) if stack.contains(&K::Unit) && !stack.contains(&K::Segment) => {
                        note_open = Some((TagInfo::parse(buf, start, end), end));
                        K::Note
                    }
                    (b"data", _) if stack.contains(&K::Unit) && !stack.contains(&K::Segment) => {
                        data_id = TagInfo::parse(buf, start, end).value(buf, "id").unwrap_or_default();
                        content_start = end;
                        K::Data
                    }
                    (b"source", Some(K::Segment)) => {
                        content_start = end;
                        elem_start = start;
                        K::Source
                    }
                    (b"target", Some(K::Segment)) => {
                        content_start = end;
                        elem_start = start;
                        K::Target
                    }
                    _ => K::Other,
                };
                stack.push(k);
            }
            Event::Empty(e) => {
                let parent = stack.last().copied();
                match (local(e.name().as_ref()), parent) {
                    (b"target", Some(K::Segment)) => {
                        if let Some(c) = cur.as_mut() {
                            let tag = TagInfo::parse(buf, start, end);
                            c.tgt = Some(TSlot::Empty { range: start..end, name: tag.name_str(buf).to_owned() });
                        }
                    }
                    (b"source", Some(K::Segment)) => {
                        if let Some(c) = cur.as_mut() {
                            c.src = Some((end..end, end, start));
                        }
                    }
                    _ => {}
                }
            }
            Event::End(_) => {
                let k = stack.pop().unwrap_or(K::Other);
                match k {
                    K::Source => {
                        if let Some(c) = cur.as_mut() {
                            c.src = Some((content_start..start, end, elem_start));
                        }
                    }
                    K::Target => {
                        if let Some(c) = cur.as_mut() {
                            c.tgt = Some(TSlot::Content(content_start..start));
                        }
                    }
                    K::Segment => {
                        if let Some(c) = cur.take() {
                            if let Some(rec) = build_seg(buf, c, &unit_id, unit_locked, unit_ids.len())? {
                                recs.push(rec);
                            }
                        }
                    }
                    K::Note => {
                        if let Some((tag, from)) = note_open.take() {
                            let text = crate::inline::parse_inlines(&buf[from..start], Flavor::X2, None, None)
                                .map(|p| crate::inline::plain(&p.inlines))
                                .unwrap_or_else(|_| String::from_utf8_lossy(&buf[from..start]).into_owned());
                            if !text.trim().is_empty() {
                                cur_notes.push(Note::new(text).with_author(tag.value(buf, "appliesTo").filter(|v| !v.is_empty()).map(|v| format!("applies to {v}"))).with_kind(tag.value(buf, "category").filter(|v| !v.is_empty())));
                            }
                        }
                    }
                    K::Data => cur_data.push((std::mem::take(&mut data_id), content_start..start)),
                    K::Unit => {
                        unit_ids.push(unit_id.clone());
                        unit_notes.push(std::mem::take(&mut cur_notes));
                        unit_data.push(std::mem::take(&mut cur_data));
                        let n = recs.len() - unit_first;
                        if n > 1 {
                            for (i, r) in recs[unit_first..].iter_mut().enumerate() {
                                r.seg.id = format!("{}#{}", unit_id, i + 1);
                            }
                        }
                    }
                    K::Group => {
                        scopes.pop();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(ParsedDoc {
        format: Format::Xliff2,
        source_lang: src_lang,
        target_lang: tgt_lang,
        recs,
        layout: Layout::X2,
        tag_defs: None,
        unit_ids,
        unit_notes,
        unit_data,
    })
}

fn build_seg(buf: &[u8], c: CurSeg, unit_id: &str, locked: bool, unit_ix: usize) -> Result<Option<SegRec>, Error> {
    let Some((src_range, src_elem_end, src_elem_start)) = c.src else {
        return Ok(None);
    };
    let (tgt_span, tslot) = match c.tgt {
        Some(TSlot::Content(r)) => (Some(r.clone()), TSlot::Content(r)),
        Some(slot) => (None, slot),
        None => (None, TSlot::Insert { pos: src_elem_end, prefix: line_prefix(buf, src_elem_start) }),
    };
    let state_slot = AttrSlot::from_tag(buf, &c.tag, "state");
    let raw_state = match &state_slot {
        AttrSlot::Existing { raw, .. } => raw.as_str(),
        AttrSlot::Missing { .. } => "initial",
    };
    let empty = tgt_span.as_ref().is_none_or(|r| r.is_empty());
    let state = state_from(raw_state, empty);
    let sub = c.tag.attr(buf, "subState");
    let native = match (&state_slot, sub) {
        (AttrSlot::Existing { raw, .. }, Some(s)) => Some(format!("{raw}/{}", crate::attrs::unescape_attr(&buf[s.value.clone()]))),
        (AttrSlot::Existing { raw, .. }, None) => Some(raw.clone()),
        _ => None,
    };
    let sub_state = sub.map(|s| s.full_range(buf));
    Ok(Some(SegRec {
        seg: Meta { id: unit_id.to_owned(), state, native },
        locked,
        lock_reason: locked.then(|| "translate=no".to_owned()),
        origin: None,
        unit_ix,
        src: src_range,
        tgt: tgt_span,
        content: Default::default(),
        orig_state: state,
        target_dirty: false,
        loc: Loc::X2(X2Loc { tgt: tslot, state: state_slot, sub_state }),
    }))
}

/// Patches for every edited segment.
pub fn patches(buf: &[u8], recs: &[SegRec]) -> Vec<Patch> {
    let mut out = Vec::new();
    for r in recs {
        let Loc::X2(loc) = &r.loc else { continue };
        if let (true, Some(c)) = (r.target_dirty, r.content.get()) {
            let body = serialize(&c.target, Flavor::X2, &c.raws);
            match &loc.tgt {
                TSlot::Content(range) => out.push(Patch::replace(range.clone(), body)),
                TSlot::Empty { range, name } => {
                    let head = String::from_utf8_lossy(&buf[range.clone()]);
                    let head = head.trim_end_matches('>').trim_end_matches('/').trim_end();
                    out.push(Patch::replace(range.clone(), format!("{head}>{body}</{name}>")));
                }
                TSlot::Insert { pos, prefix } => {
                    out.push(Patch::insert(*pos, format!("{prefix}<target>{body}</target>")));
                }
            }
        }
        if r.seg.state != r.orig_state {
            if let Some(p) = loc.state.patch("state", Some(state_to(r.seg.state)), Some("initial")) {
                out.push(p);
            }
            if let Some(full) = &loc.sub_state {
                out.push(Patch::replace(full.clone(), ""));
            }
        }
    }
    out
}
