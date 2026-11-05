//! Wordfast Pro TXLF: XLIFF 1.2 with the gs4tr extension namespace, read the way LocOS's XLIFF engine reads it
//! (one unit per `<source>`, or per `seg-source` segment), with the gs4tr protection rules.

use crate::engine::{Ctx, Item, Region, Status, Walked};
use crate::inline::{build_source, build_target, CodePiece, Piece};
use crate::xml::{perr, py_strip, with_attr, Node, Tree};
use bilingua_model::{Error, LimitError, LimitKind, Note, Origin, OriginKind, SegState, TagKind};
use std::collections::HashMap;

pub const FORMAT: &str = "txlf";
/// Key base of `seg-source` segments, as LocOS numbers them.
pub const SEG_BASE: usize = 1_000_000_000;
const POINT: [&str; 9] = ["x", "ph", "bpt", "ept", "it", "sc", "ec", "sm", "em"];
const MQ_NS: &str = "MQXliff";
const M_NS: &str = "http://www.memsource.com/mxlf/2.0";

fn lower(v: Option<&str>) -> String {
    v.unwrap_or("").trim().to_ascii_lowercase()
}

fn has_content(c: &Ctx, i: Option<usize>) -> bool {
    let Some(i) = i else { return false };
    c.tree.els[i].nodes > 0 || !py_strip(&c.tree.text_of(c.text, i)).is_empty()
}

fn translate_no(tree: &Tree, tu: usize) -> bool {
    let mut cur = Some(tu);
    while let Some(i) = cur {
        let e = &tree.els[i];
        if matches!(e.local.as_str(), "trans-unit" | "group" | "unit" | "file") {
            if let Some(v) = e.attr("translate") {
                return v.trim().eq_ignore_ascii_case("no");
            }
        }
        cur = e.parent;
    }
    false
}

fn iws_status(tree: &Tree, tu: usize) -> Option<usize> {
    let meta = tree.els[tu].kids.iter().copied().find(|&k| tree.els[k].qname.starts_with("iws:") && tree.els[k].local == "segment-metadata")?;
    tree.els[meta].kids.iter().copied().find(|&k| tree.els[k].qname.starts_with("iws:") && tree.els[k].local == "status")
}

/// Why a whole trans-unit is protected, as (reason, finished work).
fn unit_skip(c: &Ctx, tu: usize, target: Option<usize>) -> Option<(&'static str, bool)> {
    let tree = c.tree;
    let t = &tree.els[tu];
    let tstate = target.map(|x| lower(tree.els[x].attr("state"))).unwrap_or_default();
    if translate_no(tree, tu) {
        return Some(("translate-no", false));
    }
    if lower(t.attr("approved")) == "yes" {
        return Some(("approved", true));
    }
    if target.is_some() && (tstate == "final" || tstate == "signed-off") {
        return Some(("final", true));
    }
    if t.local == "segment" && lower(t.attr("state")) == "final" {
        return Some(("final", true));
    }
    if matches!(lower(t.attr_ns("locked", MQ_NS)).as_str(), "locked" | "true") {
        return Some(("locked", false));
    }
    if lower(t.attr_ns("locked", M_NS)) == "true" {
        return Some(("locked", false));
    }
    if t.gs4tr {
        let g = |e: Option<usize>| e.map(|x| lower(tree.els[x].attr_in("locked", "gs4tr"))).unwrap_or_default();
        if g(Some(tu)) == "true" || g(target) == "true" {
            return Some(("locked", false));
        }
        if has_content(c, target) && tstate != "new" && !tstate.starts_with("needs-") {
            return Some(("confirmed", true)); // Wordfast: a target past "needs-*" is the translator's
        }
    }
    let mq = lower(t.attr_ns("status", MQ_NS));
    if matches!(mq.as_str(), "proofread" | "reviewer1confirmed" | "reviewer2confirmed") {
        return Some(("approved", true));
    }
    if mq == "manuallyconfirmed" && has_content(c, target) {
        return Some(("confirmed", true));
    }
    let conf = t.attr_ns("confirmed", M_NS).unwrap_or("").trim();
    if !conf.is_empty() && conf != "0" && has_content(c, target) {
        return Some(("confirmed", true));
    }
    if let Some(s) = iws_status(tree, tu) {
        let s = &tree.els[s];
        if lower(s.attr("lock_status")) == "locked" {
            return Some(("locked", false));
        }
        if lower(s.attr("translation_status")) == "finished" {
            return Some(("final", true));
        }
    }
    None
}

fn seg_mrks(tree: &Tree, container: Option<usize>) -> Vec<usize> {
    let Some(c) = container else { return vec![] };
    let n = tree.descendants(c);
    (c + 1..=c + n).filter(|&k| tree.els[k].local == "mrk" && tree.els[k].attr("mtype") == Some("seg")).collect()
}

/// An unnumbered printf argument or an empty `{}`: the argument order is the code's meaning.
fn order_bound(code: &str) -> bool {
    if code == "{}" {
        return true;
    }
    let b = code.as_bytes();
    if b.first() != Some(&b'%') {
        return false;
    }
    let mut i = 1;
    let d0 = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i > d0 && b.get(i) == Some(&b'$') {
        return false; // numbered: %1$s
    }
    i = 1;
    while i < b.len() && matches!(b[i], b'-' | b'+' | b' ' | b'#' | b'0' | b'\'') {
        i += 1;
    }
    if b.get(i) == Some(&b'*') {
        i += 1;
    } else {
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        if b.get(i) == Some(&b'*') {
            i += 1;
        } else {
            let s = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i == s {
                return false;
            }
        }
    }
    for m in ["hh", "ll", "I64", "I32", "h", "l", "q", "L", "z", "t", "j", "I"] {
        if code[i..].starts_with(m) && code.len() - i > m.len() {
            i += m.len();
            break;
        }
    }
    code.len() == i + 1 && b"@diouxXcCsSpaAeEfFgG".contains(&b[i])
}

/// An `<x id>` / `<ph id>` placeholder a translation may move.
fn free_point(c: &Ctx, el: usize) -> bool {
    let tree = c.tree;
    let e = &tree.els[el];
    if e.local != "x" && e.local != "ph" {
        return false;
    }
    let Some(id) = e.attr("id") else { return false };
    let side = |s: &str| s.starts_with("START_") || s.starts_with("CLOSE_");
    if side(id) || e.attr("equiv").is_some_and(side) {
        return false;
    }
    if matches!(lower(e.attr("ctype")).as_str(), "bold" | "italic" | "underline" | "link") {
        return false;
    }
    let first_text = match tree.content(c.text, el).into_iter().next() {
        Some(Node::Text(t)) => Some(t),
        _ => None,
    };
    for code in [e.attr("equiv-text").map(str::to_string), e.attr("equiv").map(str::to_string), e.attr("disp").map(str::to_string), first_text].into_iter().flatten() {
        let code = py_strip(&code);
        if code.is_empty() {
            continue;
        }
        let markup_side = code.starts_with('<') && !code.ends_with("/>");
        if markup_side || order_bound(code) {
            return false;
        }
    }
    true
}

struct Walker<'a, 'c> {
    c: &'a Ctx<'c>,
    source: bool,
    n: usize,
    out: Vec<Piece>,
    occ: HashMap<String, usize>,
}

impl Walker<'_, '_> {
    fn raw(&self, k: usize) -> &str {
        let e = &self.c.tree.els[k];
        &self.c.text[e.start..e.end]
    }

    fn key(&mut self, k: usize) -> String {
        let e = &self.c.tree.els[k];
        let base = format!("{}|{}", e.local, e.attr("id").or_else(|| e.attr("rid")).unwrap_or(""));
        let o = self.occ.entry(base.clone()).or_insert(0);
        *o += 1;
        format!("{base}|{o}")
    }

    /// Depth first with an explicit stack (no recursion: the depth is bounded by the parser, but a thread's stack
    /// may be small). Each frame is the rest of an element's content and the close code written after it.
    fn visit(&mut self, parent: usize) {
        let tree = self.c.tree;
        let mut stack: Vec<(std::vec::IntoIter<Node>, Option<Piece>)> = vec![(tree.content(self.c.text, parent).into_iter(), None)];
        while let Some((rest, _)) = stack.last_mut() {
            let k = match rest.next() {
                None => {
                    if let Some((_, Some(close))) = stack.pop() {
                        self.out.push(close);
                    }
                    continue;
                }
                Some(Node::Text(t)) => {
                    self.out.push(Piece::Text(t));
                    continue;
                }
                Some(Node::El(k)) => k,
            };
            let e = &tree.els[k];
            let key = self.key(k);
            if POINT.contains(&e.local.as_str()) {
                let free = free_point(self.c, k);
                let display = {
                    let t = e.attr("equiv-text").map(str::to_string).unwrap_or_else(|| tree.text_of(self.c.text, k));
                    if py_strip(&t).is_empty() {
                        self.raw(k).to_string()
                    } else {
                        t
                    }
                };
                self.n += 1;
                let id = if self.source { self.n } else { 0 };
                self.out.push(Piece::Code(CodePiece { key, kind: TagKind::Standalone, free, display, id, raw: self.raw(k).to_string(), raw_close: String::new() }));
            } else {
                let (open, close) = if e.empty {
                    (format!("{}>", self.c.text[e.start..e.start_end].trim_end_matches("/>").trim_end()), format!("</{}>", e.qname))
                } else {
                    (self.c.text[e.start..e.start_end].to_string(), self.c.text[e.end_start..e.end].to_string())
                };
                self.n += 1;
                let id = if self.source { self.n } else { 0 };
                self.out.push(Piece::Code(CodePiece {
                    key: key.clone(),
                    kind: TagKind::Open,
                    free: false,
                    display: open.clone(),
                    id,
                    raw: open,
                    raw_close: close.clone(),
                }));
                let close = Piece::Code(CodePiece { key: format!("{key}/"), kind: TagKind::Close, free: false, display: close.clone(), id, raw: close, raw_close: String::new() });
                stack.push((tree.content(self.c.text, k).into_iter(), Some(close)));
            }
        }
    }
}

fn pieces(c: &Ctx, el: usize, source: bool) -> Vec<Piece> {
    let mut w = Walker { c, source, n: 0, out: Vec::new(), occ: HashMap::new() };
    w.visit(el);
    w.out
}

fn limit(msg: String) -> Error {
    LimitError::new(LimitKind::Units, msg).into()
}

fn target_state(c: &Ctx, t: usize, status: &Status) -> SegState {
    if matches!(status, Status::Locked(_)) {
        return SegState::Locked;
    }
    match lower(c.tree.els[t].attr("state")).as_str() {
        "signed-off" => SegState::SignedOff,
        "final" => SegState::Confirmed,
        "translated" => SegState::Translated,
        "new" | "needs-translation" | "needs-adaptation" | "needs-l10n" | "needs-review-translation" | "needs-review-adaptation" | "needs-review-l10n" => SegState::Draft,
        _ => match status {
            Status::Open => SegState::Draft,
            Status::Confirmed(_) => SegState::Translated,
            Status::Locked(_) => SegState::Locked,
        },
    }
}

/// Region of an existing `<target>` element: its content, or the whole element when it is written `<target/>`;
/// the start tag becomes part of the region when its `state` is to change.
fn target_region(c: &Ctx, tu: usize, t: usize) -> Region {
    let e = &c.tree.els[t];
    let state = lower(e.attr("state"));
    let resets = state == "new" || state == "needs-translation" || (c.tree.els[tu].gs4tr && state.starts_with("needs-"));
    let start_tag = &c.text[e.start..e.start_end];
    if e.empty {
        let mut open = format!("{}>", start_tag.trim_end_matches("/>").trim_end());
        if resets {
            open = with_attr(&open, "state", "translated");
        }
        return Region { start: e.start, end: e.end, pre: open, post: format!("</{}>", e.qname) };
    }
    if resets {
        return Region { start: e.start, end: e.end_start, pre: with_attr(start_tag, "state", "translated"), post: String::new() };
    }
    Region { start: e.start_end, end: e.end_start, pre: String::new(), post: String::new() }
}

pub fn walk(c: &Ctx) -> Result<Walked, Error> {
    let tree = c.tree;
    let root = &tree.els[0];
    if root.local != "xliff" || root.attr("version").unwrap_or("1.2").starts_with('2') {
        return Err(perr(FORMAT, "This is not an XLIFF 1.2 file, so it cannot be read as .txlf."));
    }
    let mut items: Vec<Item> = Vec::new();
    let mut budget: i64 = 10 * c.limits.max_units as i64;
    let (mut idx, mut seg_n) = (0usize, 0usize);
    for si in 0..tree.els.len() {
        let src = &tree.els[si];
        if src.local != "source" {
            continue;
        }
        let Some(tu) = src.parent else { continue };
        if !matches!(tree.els[tu].local.as_str(), "trans-unit" | "segment") {
            continue;
        }
        let key = idx;
        idx += 1;
        let target = tree.kid(tu, "target");
        let skip = unit_skip(c, tu, target);
        let seg_source = tree.kid(tu, "seg-source").filter(|_| tree.els[tu].local == "trans-unit");
        let mrks = seg_mrks(tree, seg_source);
        let tgt_mrks: HashMap<&str, usize> = seg_mrks(tree, target).into_iter().map(|m| (tree.els[m].attr("mid").unwrap_or(""), m)).collect();
        // (slot key, element holding the source text, element holding the seg target)
        let slots: Vec<(usize, usize, Option<usize>)> = if mrks.is_empty() {
            vec![(key, si, target)]
        } else {
            mrks.iter()
                .map(|&m| {
                    let k = SEG_BASE + seg_n;
                    seg_n += 1;
                    (k, m, tgt_mrks.get(tree.els[m].attr("mid").unwrap_or("")).copied())
                })
                .collect()
        };
        let whole = mrks.is_empty();
        let mut status_of: Vec<Status> = Vec::with_capacity(slots.len());
        for _ in &slots {
            status_of.push(match skip {
                Some((r, true)) => Status::Confirmed(r),
                Some((r, false)) => Status::Locked(r),
                None => Status::Open,
            });
        }
        // the owner of a mirrored target that has to be created (the first open segment)
        let owner = if !whole && target.is_none() { status_of.iter().position(|s| matches!(s, Status::Open)) } else { None };
        let mut template = String::new();
        if owner.is_some() {
            let ss = &tree.els[seg_source.expect("mrks come from seg-source")];
            let mut pos = ss.start_end;
            for (n, &m) in mrks.iter().enumerate() {
                let me = &tree.els[m];
                let k = if matches!(status_of[n], Status::Open) { slots[n].0.to_string() } else { String::new() };
                if me.empty {
                    let open = format!("{}>", c.text[me.start..me.start_end].trim_end_matches("/>").trim_end());
                    template.push_str(&c.text[pos..me.start]);
                    template.push_str(&open);
                    template.push('\u{1}');
                    template.push_str(&k);
                    template.push('\u{1}');
                    template.push_str(&format!("</{}>", me.qname));
                    pos = me.end;
                } else {
                    template.push_str(&c.text[pos..me.start_end]);
                    template.push('\u{1}');
                    template.push_str(&k);
                    template.push('\u{1}');
                    pos = me.end_start;
                }
            }
            template.push_str(&c.text[pos..ss.end_start]);
        }
        // the trans-unit's notes, read once for all its segments (a seg-source unit can have many)
        let mut notes = Vec::new();
        for &nk in &tree.els[tu].kids {
            let ne = &tree.els[nk];
            if ne.local == "note" {
                let text = py_strip(&tree.text_of(c.text, nk)).to_string();
                if !text.is_empty() {
                    notes.push(Note::new(text).with_author(ne.attr("from").map(str::to_string)));
                }
            }
        }
        // every segment gets the notes, unless that would copy them a quadratic number of times (a crafted file
        // with thousands of notes and segments): then the first listed segment carries them
        let notes_everywhere = notes.len().saturating_mul(slots.len()) <= 10_000;
        let mut notes = Some(notes);
        for (n, &(k, el, tel)) in slots.iter().enumerate() {
            let status = std::mem::replace(&mut status_of[n], Status::Open);
            let open = matches!(status, Status::Open);
            if open {
                budget -= 2 + tree.descendants(el) as i64;
                if budget < 0 {
                    return Err(limit(format!(
                        "This TXLF file has far more inline tags than {} segments allow. Split it into smaller files.",
                        c.limits.max_units
                    )));
                }
            } else if !c.want_protected {
                continue;
            }
            let s = build_source(pieces(c, el, true));
            if s.inlines.is_empty() {
                continue;
            }
            let status = if open && !whole && target.is_some() && tel.is_none() {
                Status::Locked("segmentation does not match seg-source")
            } else {
                status
            };
            let (tgt, state, origin) = match tel.filter(|_| has_content(c, tel)) {
                Some(t) => {
                    let tg = build_target(pieces(c, t, false), &s);
                    let st = if tg.is_empty() { SegState::New } else { target_state(c, tu_target(tel, target), &status) };
                    let origin = tree.els[tu_target(tel, target)]
                        .attr_in("score", "gs4tr")
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .map(|v| Origin::new(OriginKind::Tm).with_match(v.clamp(0.0, 255.0) as u8));
                    (tg, st, origin)
                }
                None => (Vec::new(), SegState::New, None),
            };
            let mut extra = Vec::new();
            let region = if !matches!(status, Status::Open | Status::Confirmed("confirmed")) {
                None
            } else if whole {
                Some(match target {
                    Some(t) => target_region(c, tu, t),
                    None => {
                        let anchor = &tree.els[seg_source.unwrap_or(si)];
                        let q = &tree.els[si].qname;
                        let tname = format!("{}target", &q[..q.len() - 6]);
                        Region { start: anchor.end, end: anchor.end, pre: format!("<{tname}>"), post: format!("</{tname}>") }
                    }
                })
            } else if let Some(t) = tel {
                let e = &tree.els[t];
                Some(if e.empty {
                    Region {
                        start: e.start,
                        end: e.end,
                        pre: format!("{}>", c.text[e.start..e.start_end].trim_end_matches("/>").trim_end()),
                        post: format!("</{}>", e.qname),
                    }
                } else {
                    Region { start: e.start_end, end: e.end_start, pre: String::new(), post: String::new() }
                })
            } else if !template.is_empty() {
                let ss = &tree.els[seg_source.expect("seg-source")];
                let q = &ss.qname;
                let tname = format!("{}target", &q[..q.len() - "seg-source".len()]);
                extra.push(("tmpl", std::mem::take(&mut template)));
                Some(Region { start: ss.end, end: ss.end, pre: format!("<{tname}>"), post: format!("</{tname}>") })
            } else {
                None // written by the owner of the mirrored target
            };
            items.push(Item {
                key: k,
                status,
                lead: s.lead,
                trail: s.trail,
                source: s.inlines,
                codes: s.codes,
                target: tgt,
                state,
                origin,
                region,
                extra,
                ctx_key: tree.els[tu].attr("id").map(str::to_string),
                style: Some("trans-unit".into()),
                notes: if notes_everywhere { notes.clone().unwrap_or_default() } else { notes.take().unwrap_or_default() },
            });
        }
    }
    let file = tree.kid(0, "file");
    let lang = |a: &str| file.and_then(|f| tree.els[f].attr(a)).unwrap_or("").trim().to_string();
    Ok(Walked { items, source_lang: lang("source-language"), target_lang: lang("target-language") })
}

/// The element whose attributes describe the target: the trans-unit's `<target>` (a segment target `mrk` is
/// described by its trans-unit target).
fn tu_target(tel: Option<usize>, target: Option<usize>) -> usize {
    target.or(tel).expect("a target exists")
}
