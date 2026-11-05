//! Wordfast Pro TXML: `<txml>` with `<translatable>` / `<localizable>` blocks of `<segment>`s, each with a
//! `<source>`, an optional `<target>`, and `<ws>`, `<comments>`, `<revisions>` around them.

use crate::engine::{Ctx, Item, Region, Status, Walked};
use crate::inline::{build_source, build_target, CodePiece, Piece};
use crate::xml::{perr, py_strip, Node, Tree};
use bilingua_model::{Error, LimitError, LimitKind, Note, Origin, OriginKind, SegState, TagKind};
use std::collections::HashMap;

pub const FORMAT: &str = "txml";

fn is_true(v: Option<&str>) -> bool {
    v.unwrap_or("").trim().eq_ignore_ascii_case("true")
}

fn has_content(c: &Ctx, i: Option<usize>) -> bool {
    let Some(i) = i else { return false };
    c.tree.els[i].nodes > 0 || !py_strip(&c.tree.text_of(c.text, i)).is_empty()
}

/// The inline pieces of `src` (a `<source>` or `<target>`). For a source, `ut` start/end pairs with the same `x`
/// become a paired code and every other element a point; the element's own bytes are kept.
fn pieces(c: &Ctx, src: usize, source: bool) -> Vec<Piece> {
    let tree: &Tree = c.tree;
    let kids = &tree.els[src].kids;
    // pair <ut type="start" x="n"> with the next <ut type="end" x="n">
    let mut pair_of: HashMap<usize, usize> = HashMap::new();
    if source {
        let mut open_x: HashMap<&str, usize> = HashMap::new();
        for (i, &k) in kids.iter().enumerate() {
            let e = &tree.els[k];
            if e.local != "ut" {
                continue;
            }
            let Some(x) = e.attr("x") else { continue };
            match e.attr("type").unwrap_or("").to_ascii_lowercase().as_str() {
                "start" => {
                    open_x.insert(x, i);
                }
                "end" => {
                    if let Some(j) = open_x.remove(x) {
                        pair_of.insert(i, j);
                        pair_of.insert(j, i);
                    }
                }
                _ => {}
            }
        }
    }
    let raw = |k: usize| c.text[tree.els[k].start..tree.els[k].end].to_string();
    let show = |k: usize| {
        let t = tree.text_of(c.text, k);
        if t.is_empty() {
            raw(k)
        } else {
            t
        }
    };
    let mut out = Vec::new();
    let mut n = 0usize;
    let mut num_of: HashMap<usize, usize> = HashMap::new();
    let mut ordinal: HashMap<&str, usize> = HashMap::new();
    let mut i = 0usize;
    for node in tree.content(c.text, src) {
        match node {
            Node::Text(t) => out.push(Piece::Text(t)),
            Node::El(k) => {
                let e = &tree.els[k];
                let ord = ordinal.entry(e.local.as_str()).or_insert(0);
                *ord += 1;
                let key = match (e.local.as_str(), e.attr("x")) {
                    ("ut", Some(x)) => format!("ut|{}|{x}", e.attr("type").unwrap_or("").to_ascii_lowercase()),
                    (l, _) => format!("{l}|#{ord}"),
                };
                let mk = |kind, id, raw: String, raw_close: String| {
                    Piece::Code(CodePiece { key: key.clone(), kind, free: false, display: show(k), id, raw, raw_close })
                };
                if !source {
                    out.push(mk(TagKind::Standalone, 0, String::new(), String::new()));
                } else if let Some(&j) = pair_of.get(&i) {
                    if j > i {
                        n += 1;
                        num_of.insert(i, n);
                        out.push(mk(TagKind::Open, n, raw(k), raw(kids[j])));
                    } else {
                        out.push(mk(TagKind::Close, num_of[&j], raw(k), String::new()));
                    }
                } else {
                    n += 1;
                    out.push(mk(TagKind::Standalone, n, raw(k), String::new()));
                }
                i += 1;
            }
        }
    }
    out
}

fn limit(msg: String) -> Error {
    LimitError::new(LimitKind::Units, msg).into()
}

pub fn walk(c: &Ctx) -> Result<Walked, Error> {
    let tree = c.tree;
    let root = &tree.els[0];
    if root.local != "txml" {
        return Err(perr(FORMAT, "This is not a Wordfast TXML file (the root element is not <txml>)."));
    }
    let mut items = Vec::new();
    let mut key = 0usize;
    let mut budget: i64 = 10 * c.limits.max_units as i64;
    for i in 0..tree.els.len() {
        let seg = &tree.els[i];
        if seg.local != "segment" {
            continue;
        }
        let Some(b) = seg.parent else { continue };
        let block = &tree.els[b];
        if block.local != "translatable" && block.local != "localizable" {
            continue; // e.g. history inside <revisions>
        }
        let Some(src) = tree.kid(i, "source") else { continue };
        let target = tree.kid(i, "target");
        let k = key;
        key += 1;
        let modified = is_true(seg.attr("modified"));
        let mut origin = None;
        let status = if block.local == "localizable" {
            Status::Locked("localizable")
        } else if is_true(seg.attr("locked")) || is_true(block.attr("locked")) {
            Status::Locked("locked")
        } else if !has_content(c, target) {
            Status::Open
        } else {
            let t = &tree.els[target.expect("has content")];
            let score = t.attr("score").or_else(|| seg.attr("score"));
            if is_true(seg.attr("gtmt")) && !modified {
                origin = Some(Origin::new(OriginKind::Mt));
                Status::Open // raw machine translation: re-translate
            } else if let Some(s) = score.filter(|_| !modified).and_then(|s| s.trim().parse::<f64>().ok()).filter(|s| *s < 100.0) {
                origin = Some(Origin::new(OriginKind::Tm).with_match(s.clamp(0.0, 255.0) as u8));
                Status::Open // a fuzzy match: re-translate
            } else {
                if let Some(s) = score.and_then(|s| s.trim().parse::<f64>().ok()) {
                    origin = Some(Origin::new(OriginKind::Tm).with_match(s.clamp(0.0, 255.0) as u8));
                }
                Status::Confirmed("confirmed")
            }
        };
        if matches!(status, Status::Open) {
            budget -= 2 + tree.descendants(src) as i64;
            if budget < 0 {
                return Err(limit(format!(
                    "This TXML file has far more inline tags than {} segments allow. Split it into smaller files.",
                    c.limits.max_units
                )));
            }
        } else if !c.want_protected {
            continue;
        }
        let s = build_source(pieces(c, src, true));
        if s.inlines.is_empty() {
            continue;
        }
        let (tgt, state) = match target.filter(|_| has_content(c, target)) {
            Some(t) => {
                let tg = build_target(pieces(c, t, false), &s);
                let st = match status {
                    Status::Open => SegState::Draft,
                    Status::Confirmed(_) => SegState::Translated,
                    Status::Locked(_) => SegState::Locked,
                };
                let st = if tg.is_empty() { SegState::New } else { st };
                (tg, st)
            }
            None => (Vec::new(), SegState::New),
        };
        let region = if matches!(status, Status::Open | Status::Confirmed("confirmed")) {
            Some(match target {
                Some(t) => {
                    let e = &tree.els[t];
                    if e.empty {
                        let open = c.text[e.start..e.start_end].trim_end_matches("/>").trim_end();
                        Region { start: e.start, end: e.end, pre: format!("{open}>"), post: format!("</{}>", e.qname) }
                    } else {
                        Region { start: e.start_end, end: e.end_start, pre: String::new(), post: String::new() }
                    }
                }
                None => {
                    let anchor = match tree.next_sibling(src) {
                        Some(n) if tree.els[n].local == "ws" => n,
                        _ => src,
                    };
                    let q = &tree.els[src].qname;
                    let tname = format!("{}target", &q[..q.len() - 6]);
                    let at = tree.els[anchor].end;
                    Region { start: at, end: at, pre: format!("<{tname}>"), post: format!("</{tname}>") }
                }
            })
        } else {
            None
        };
        let mut notes = Vec::new();
        if let Some(cm) = tree.kid(i, "comments") {
            for &ck in &tree.els[cm].kids {
                let ce = &tree.els[ck];
                if ce.local == "comment" {
                    let text = py_strip(&tree.text_of(c.text, ck)).to_string();
                    if !text.is_empty() {
                        notes.push(Note { author: ce.attr("creationid").map(str::to_string), text, kind: ce.attr("type").map(str::to_string) });
                    }
                }
            }
        }
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
            extra: Vec::new(),
            ctx_key: Some(format!("{}/{}", block.attr("blockId").unwrap_or(""), seg.attr("segmentId").unwrap_or(""))),
            style: Some(block.local.clone()),
            notes,
        });
    }
    Ok(Walked {
        items,
        source_lang: root.attr("locale").unwrap_or("").to_string(),
        target_lang: root.attr("targetlocale").unwrap_or("").trim().to_string(),
    })
}
