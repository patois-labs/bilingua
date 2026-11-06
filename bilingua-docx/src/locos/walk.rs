//! LocOS's DOCX numbering (walk stamp 6, which numbers as stamps 4 and 5): the main body in reading order (tables
//! and content controls looked through, text boxes after their anchor paragraph), then the headers, footers,
//! footnotes, endnotes and comments (`_v4_walk`, `_blocks`, `_stream`, `_text_boxes`).

use super::pkg::{Pkg, Target};
use super::tree::*;
use bilingua_model::Error;
use bilingua_xmltree::Dom;
use std::collections::HashMap;

/// Table nesting deeper than this is not descended into (LocOS `MAX_TABLE_DEPTH`).
const MAX_TABLE_DEPTH: usize = 32;
/// Text boxes nested inside text boxes are followed this deep (`MAX_BOX_DEPTH`).
const MAX_BOX_DEPTH: usize = 8;
const EXTRA_KINDS: [&str; 5] = ["header", "footer", "footnotes", "endnotes", "comments"];
const NOTE_SKIP_TYPES: [&str; 3] = ["separator", "continuationSeparator", "continuationNotice"];

/// A paragraph: (part index, `w:p` node).
pub type P = (usize, usize);

/// What the walk numbers (LocOS `_Walk`).
#[derive(Default)]
pub struct Walk {
    pub paras: Vec<P>,
    /// The part of each paragraph.
    pub parts: Vec<usize>,
    /// p_index -> the Fallback copies of that text-box paragraph.
    pub mirrors: HashMap<usize, Vec<P>>,
    /// The other parts the walk opened, in the order it opened them.
    pub roots: Vec<usize>,
}

/// `_through_wrappers`: the children of `parent` named one of `tags` (in `w:`), looking through `w:sdt` /
/// `w:sdtContent` and `w:customXml`, in document order.
pub fn through_wrappers(d: &Dom, parent: usize, tags: &[&str]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut stack: Vec<(usize, usize)> = vec![(parent, 0)];
    while let Some(&(node, i)) = stack.last() {
        let kids = &d.nodes[node].children;
        if i >= kids.len() {
            stack.pop();
            continue;
        }
        let c = kids[i];
        stack.last_mut().unwrap().1 += 1;
        if !d.is_el(c) {
            continue;
        }
        if tags.iter().any(|t| is_w(d, c, t)) {
            out.push(c);
        } else if is_w(d, c, "sdt") {
            if let Some(content) = find_w(d, c, "sdtContent") {
                stack.push((content, 0));
            }
        } else if is_w(d, c, "customXml") {
            stack.push((c, 0));
        }
    }
    out
}

fn p_has_text(d: &Dom, p: usize) -> bool {
    d.iter(p).into_iter().any(|t| is_w(d, t, "t") && !d.lead_text(t).is_empty())
}

fn vmerge_continuation(d: &Dom, tc: usize) -> bool {
    let Some(pr) = find_w(d, tc, "tcPr") else { return false };
    match find_w(d, pr, "vMerge") {
        Some(vm) => get_w(d, vm, "val").unwrap_or("continue") != "restart",
        None => false,
    }
}

fn cells_of(d: &Dom, tbl: usize) -> Vec<usize> {
    through_wrappers(d, tbl, &["tr"]).into_iter().flat_map(|tr| through_wrappers(d, tr, &["tc"])).collect()
}

/// `_blocks`: the paragraphs of `tops` (body-level `w:p` / `w:tbl`) in reading order.
pub fn blocks(d: &Dom, tops: &[usize]) -> Vec<usize> {
    enum Frame {
        Table(Vec<usize>, usize, usize),
        Cell(Vec<usize>, usize, bool, usize),
    }
    let mut out = Vec::new();
    for &top in tops {
        if is_w(d, top, "p") {
            out.push(top);
            continue;
        }
        let mut stack = vec![Frame::Table(cells_of(d, top), 0, 1)];
        while let Some(f) = stack.last_mut() {
            match f {
                Frame::Table(cells, i, depth) => {
                    let Some(&tc) = cells.get(*i) else {
                        stack.pop();
                        continue;
                    };
                    *i += 1;
                    let depth = *depth;
                    let content = through_wrappers(d, tc, &["p", "tbl"]);
                    let keep = !vmerge_continuation(d, tc) || content.iter().any(|&x| is_w(d, x, "p") && p_has_text(d, x));
                    stack.push(Frame::Cell(content, 0, keep, depth));
                }
                Frame::Cell(items, i, keep, depth) => {
                    let Some(&el) = items.get(*i) else {
                        stack.pop();
                        continue;
                    };
                    *i += 1;
                    let (keep, depth) = (*keep, *depth);
                    if is_w(d, el, "p") {
                        if keep {
                            out.push(el);
                        }
                    } else if depth < MAX_TABLE_DEPTH {
                        stack.push(Frame::Table(cells_of(d, el), 0, depth + 1));
                    }
                }
            }
        }
    }
    out
}

fn outer_boxes(d: &Dom, root: usize) -> Vec<usize> {
    let boxes: Vec<usize> = d.iter(root).into_iter().filter(|&b| is_w(d, b, "txbxContent")).collect();
    let mut inner = std::collections::HashSet::new();
    for &b in &boxes {
        for x in d.iter(b).into_iter().skip(1) {
            if is_w(d, x, "txbxContent") {
                inner.insert(x);
            }
        }
    }
    boxes.into_iter().filter(|b| !inner.contains(b)).collect()
}

/// `_ac_boxes`: [(box, mirror)] for one `mc:AlternateContent`.
fn ac_boxes(d: &Dom, ac: usize) -> Vec<(usize, Option<usize>)> {
    let mut cboxes = Vec::new();
    for c in d.element_children(ac) {
        if d.is(c, MC, "Choice") {
            cboxes = outer_boxes(d, c);
            if !cboxes.is_empty() {
                break;
            }
        }
    }
    let fboxes = d.find(ac, MC, "Fallback").map(|fb| outer_boxes(d, fb)).unwrap_or_default();
    if cboxes.is_empty() {
        return fboxes.into_iter().map(|b| (b, None)).collect();
    }
    if fboxes.len() == cboxes.len() {
        return cboxes.into_iter().zip(fboxes).map(|(c, f)| (c, Some(f))).collect();
    }
    cboxes.into_iter().map(|b| (b, None)).collect()
}

/// `_text_boxes`: the text boxes anchored in paragraph `p`, in document order.
fn text_boxes(d: &Dom, p: usize) -> Vec<(usize, Option<usize>)> {
    let mut out = Vec::new();
    let mut stack: Vec<(usize, usize)> = vec![(p, 0)];
    while let Some(&(node, i)) = stack.last() {
        let kids = &d.nodes[node].children;
        if i >= kids.len() {
            stack.pop();
            continue;
        }
        let el = kids[i];
        stack.last_mut().unwrap().1 += 1;
        if !d.is_el(el) {
            continue;
        }
        if is_w(d, el, "txbxContent") {
            out.push((el, None));
        } else if d.is(el, MC, "AlternateContent") {
            out.extend(ac_boxes(d, el));
        } else if is_w(d, el, "del") || is_w(d, el, "moveFrom") {
            continue;
        } else if d.nodes[el].children.iter().any(|&c| d.nodes[c].kind != bilingua_xmltree::Kind::Text) {
            stack.push((el, 0));
        }
    }
    out
}

fn paras_of(d: &Dom, bx: usize) -> Vec<usize> {
    blocks(d, &through_wrappers(d, bx, &["p", "tbl"]))
}

/// `_stream`: (paragraph, mirror paragraphs) in reading order with text boxes after their anchor.
fn stream(d: &Dom, tops: &[usize]) -> Vec<(usize, Vec<usize>)> {
    let mut out = Vec::new();
    let first: Vec<(usize, Vec<usize>)> = blocks(d, tops).into_iter().map(|p| (p, Vec::new())).collect();
    type Frame = (Vec<(usize, Vec<usize>)>, usize);
    let mut stack: Vec<Frame> = vec![(first, 0)];
    while let Some((items, i)) = stack.last_mut() {
        let Some(item) = items.get(*i).cloned() else {
            stack.pop();
            continue;
        };
        *i += 1;
        out.push(item.clone());
        if stack.len() > MAX_BOX_DEPTH {
            continue;
        }
        let (para, mirrors) = item;
        let boxes = text_boxes(d, para);
        if boxes.is_empty() {
            continue;
        }
        let in_mirrors: Vec<Vec<(usize, Option<usize>)>> = mirrors.iter().map(|&m| text_boxes(d, m)).collect();
        let mut frames = Vec::new();
        for (k, &(bx, fbox)) in boxes.iter().enumerate() {
            let prim = paras_of(d, bx);
            let mut copies: Vec<usize> = fbox.into_iter().collect();
            for found in &in_mirrors {
                if found.len() == boxes.len() {
                    copies.push(found[k].0);
                    if let Some(m) = found[k].1 {
                        copies.push(m);
                    }
                }
            }
            let lists: Vec<Vec<usize>> = copies.iter().map(|&c| paras_of(d, c)).filter(|l| l.len() == prim.len()).collect();
            frames.push(prim.iter().enumerate().map(|(j, &q)| (q, lists.iter().map(|l| l[j]).collect())).collect::<Vec<_>>());
        }
        for f in frames.into_iter().rev() {
            stack.push((f, 0));
        }
    }
    out
}

/// Python's sort key `_natural`: text and numbers alternate.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    fn parts(s: &str) -> Vec<(bool, &str)> {
        let bytes = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let digit = bytes[i].is_ascii_digit();
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_digit() == digit {
                j += 1;
            }
            out.push((digit, &s[i..j]));
            i = j;
        }
        out
    }
    let (x, y) = (parts(a), parts(b));
    // re.split(r"(\d+)") starts with a (possibly empty) text part: compare text with text, numbers with numbers
    let norm = |v: Vec<(bool, &str)>| -> Vec<(bool, String)> {
        let mut out = Vec::new();
        if v.first().is_some_and(|p| p.0) {
            out.push((false, String::new()));
        }
        for (dg, t) in v {
            out.push((dg, t.to_string()));
        }
        if out.last().is_some_and(|p| p.0) {
            out.push((false, String::new()));
        }
        out
    };
    let (x, y) = (norm(x), norm(y));
    for (p, q) in x.iter().zip(y.iter()) {
        let o = if p.0 && q.0 {
            let (a, b) = (p.1.trim_start_matches('0'), q.1.trim_start_matches('0'));
            a.len().cmp(&b.len()).then_with(|| a.cmp(b))
        } else {
            p.1.cmp(&q.1)
        };
        if o != std::cmp::Ordering::Equal {
            return o;
        }
    }
    x.len().cmp(&y.len())
}

/// `_extra_parts`: (kind, part) of every header, footer, footnotes, endnotes and comments part the main document
/// relates to, once each, by kind then the natural order of the part name.
pub fn extra_parts(pkg: &Pkg) -> Vec<(&'static str, usize)> {
    let mut found: Vec<(&'static str, usize)> = Vec::new();
    for rel in &pkg.parts[pkg.main].rels {
        let Target::Part(t) = rel.target else { continue };
        let kind = rel.reltype.rsplit('/').next().unwrap_or("");
        let Some(&k) = EXTRA_KINDS.iter().find(|&&k| k == kind) else { continue };
        if !found.iter().any(|&(_, p)| pkg.parts[p].name == pkg.parts[t].name) {
            found.push((k, t));
        }
    }
    let rank = |k: &str| EXTRA_KINDS.iter().position(|&x| x == k).unwrap();
    found.sort_by(|a, b| rank(a.0).cmp(&rank(b.0)).then_with(|| natural_cmp(&pkg.parts[a.1].name, &pkg.parts[b.1].name)));
    found
}

fn story_tops(d: &Dom, kind: &str, root: usize) -> Vec<Vec<usize>> {
    if kind == "header" || kind == "footer" {
        return vec![through_wrappers(d, root, &["p", "tbl"])];
    }
    let item = match kind {
        "footnotes" => "footnote",
        "endnotes" => "endnote",
        _ => "comment",
    };
    d.element_children(root)
        .filter(|&el| is_w(d, el, item) && !get_w(d, el, "type").is_some_and(|t| NOTE_SKIP_TYPES.contains(&t)))
        .map(|el| through_wrappers(d, el, &["p", "tbl"]))
        .collect()
}

/// `_v4_walk` (stamp 6 numbers the same way). Opens (parses) the blob-kept parts it walks; a part that is not XML
/// is left out.
pub fn walk(pkg: &mut Pkg) -> Result<Walk, Error> {
    let mut w = Walk::default();
    let main = pkg.main;
    let add = |w: &mut Walk, d: &Dom, part: usize, tops: &[usize]| {
        for (para, mirrors) in stream(d, tops) {
            if !mirrors.is_empty() {
                w.mirrors.insert(w.paras.len(), mirrors.into_iter().map(|m| (part, m)).collect());
            }
            w.paras.push((part, para));
            w.parts.push(part);
        }
    };
    {
        let d = pkg.parts[main].dom.as_ref().unwrap();
        let body = find_w(d, d.root, "body").ok_or_else(|| super::perr("the main document has no w:body"))?;
        let tops = through_wrappers(d, body, &["p", "tbl"]);
        add(&mut w, d, main, &tops);
    }
    for (kind, part) in extra_parts(pkg) {
        if !pkg.open(part) {
            continue;
        }
        w.roots.push(part);
        let d = pkg.parts[part].dom.as_ref().unwrap();
        for tops in story_tops(d, kind, d.root) {
            add(&mut w, d, part, &tops);
        }
    }
    Ok(w)
}
