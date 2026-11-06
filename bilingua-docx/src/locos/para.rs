//! One paragraph as LocOS's `docx_filter.py` (walk 6) reads and writes it: the atoms (`_walk_paragraph_v6`), the
//! unit text (`_tokenize_paragraph`, `_tokenize_objects`) and the rebuild from a translation (`_set_paragraph_text`,
//! `_set_paragraph_objects`).

use super::tree::*;
use bilingua_model::pytext::py_strip;
use bilingua_model::Error;
use bilingua_xmltree::{Dom, Kind};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Inline wrappers nested deeper than this stay one object (LocOS `MAX_INLINE_DEPTH`).
pub const MAX_INLINE_DEPTH: usize = 32;
const W6_WRAPPERS: [&str; 7] = ["hyperlink", "ins", "moveTo", "smartTag", "customXml", "dir", "bdo"];
const SDT_FIXED_KINDS: [&str; 11] = ["checkbox", "dropDownList", "comboBox", "date", "picture", "docPartObj", "docPartList", "citation", "bibliography", "equation", "group"];
const MARKERS: [&str; 19] = [
    "bookmarkStart",
    "bookmarkEnd",
    "proofErr",
    "commentRangeStart",
    "commentRangeEnd",
    "permStart",
    "permEnd",
    "moveFromRangeStart",
    "moveFromRangeEnd",
    "moveToRangeStart",
    "moveToRangeEnd",
    "customXmlInsRangeStart",
    "customXmlInsRangeEnd",
    "customXmlDelRangeStart",
    "customXmlDelRangeEnd",
    "customXmlMoveFromRangeStart",
    "customXmlMoveFromRangeEnd",
    "customXmlMoveToRangeStart",
    "customXmlMoveToRangeEnd",
];
const TEXT_RUN_CHILDREN: [&str; 5] = ["t", "tab", "cr", "noBreakHyphen", "ptab"];
const DROPPABLE_RUN_CHILDREN: [&str; 3] = ["rPr", "lastRenderedPageBreak", "softHyphen"];
/// Paragraph children holding text outside the direct runs (`_TEXT_CHILDREN`).
const TEXT_CHILDREN: [&str; 4] = ["hyperlink", "ins", "smartTag", "fldSimple"];

fn is_any_w(d: &Dom, id: usize, names: &[&str]) -> bool {
    names.iter().any(|n| is_w(d, id, n))
}

pub fn is_marker(d: &Dom, id: usize) -> bool {
    is_any_w(d, id, &MARKERS)
}

/// `_w6_content`: the element whose children walk 6 reads for a run-level wrapper, or None.
pub fn w6_content(d: &Dom, el: usize) -> Option<usize> {
    if is_any_w(d, el, &W6_WRAPPERS) {
        return Some(el);
    }
    if !is_w(d, el, "sdt") {
        return None;
    }
    if let Some(pr) = find_w(d, el, "sdtPr") {
        if d.element_children(pr).any(|c| SDT_FIXED_KINDS.contains(&d.local_name(c))) {
            return None;
        }
    }
    find_w(d, el, "sdtContent")
}

/// `_w6_paragraph_of_run`: the `w:p` whose walk-6 text includes run `r`, or None.
pub fn w6_paragraph_of_run(d: &Dom, r: usize) -> Option<usize> {
    let mut a = parent(d, r);
    let mut depth = 0;
    while let Some(x) = a {
        if is_w(d, x, "p") {
            break;
        }
        let mut cur = x;
        if is_w(d, x, "sdtContent") {
            let sdt = parent(d, x)?;
            if w6_content(d, sdt) != Some(x) {
                return None;
            }
            cur = sdt;
        } else if !is_any_w(d, x, &W6_WRAPPERS) {
            return None;
        }
        depth += 1;
        if depth > MAX_INLINE_DEPTH {
            return None;
        }
        a = parent(d, cur);
    }
    a
}

/// `_w6_runs`: every run walk 6 reads in paragraph `p`.
pub fn w6_runs(d: &Dom, p: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut stack: Vec<(usize, usize, usize)> = vec![(p, 0, 0)];
    while let Some(&(node, i, depth)) = stack.last() {
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
        if is_w(d, el, "r") {
            out.push(el);
            continue;
        }
        let bx = if depth < MAX_INLINE_DEPTH { w6_content(d, el) } else { None };
        if let Some(b) = bx {
            stack.push((b, 0, depth + 1));
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AKind {
    Text,
    Obj,
    Marker,
}

/// LocOS `_Atom`.
#[derive(Clone, Debug)]
pub struct Atom {
    pub kind: AKind,
    /// The innermost wrapper holding it.
    pub ctx: Option<usize>,
    /// The original `w:r` (text atoms and run-child objects).
    pub run: Option<usize>,
    /// The element itself (objects and markers).
    pub el: Option<usize>,
    pub text: String,
    /// Markers that preceded this atom, with their wrapper.
    pub markers: Vec<(usize, Option<usize>)>,
    pub group: usize,
    pub rkey: usize,
    /// The wrappers holding it, outermost first.
    pub path: Vec<usize>,
}

impl Atom {
    fn new(kind: AKind, ctx: Option<usize>, run: Option<usize>, el: Option<usize>, text: String) -> Atom {
        Atom { kind, ctx, run, el, text, markers: Vec::new(), group: 0, rkey: 0, path: Vec::new() }
    }
}

fn is_text_child(d: &Dom, el: usize) -> bool {
    if is_any_w(d, el, &TEXT_RUN_CHILDREN) {
        return true;
    }
    is_w(d, el, "br") && get_w(d, el, "type").filter(|t| !t.is_empty()).unwrap_or("textWrapping") == "textWrapping"
}

/// `str(child)` of a run's text child.
fn text_of(d: &Dom, el: usize) -> String {
    match d.local_name(el) {
        "t" => d.lead_text(el),
        "tab" | "ptab" => "\t".into(),
        "cr" | "br" => "\n".into(),
        "noBreakHyphen" => "-".into(),
        _ => String::new(),
    }
}

fn walk_runs(d: &Dom, r: usize, ctx: Option<usize>, out: &mut Vec<Atom>) {
    let mut buf = String::new();
    for child in d.element_children(r) {
        if is_any_w(d, child, &DROPPABLE_RUN_CHILDREN) {
            continue;
        }
        if is_text_child(d, child) {
            buf.push_str(&text_of(d, child));
        } else {
            if !buf.is_empty() {
                out.push(Atom::new(AKind::Text, ctx, Some(r), None, std::mem::take(&mut buf)));
            }
            out.push(Atom::new(AKind::Obj, ctx, Some(r), Some(child), String::new()));
        }
    }
    if !buf.is_empty() {
        out.push(Atom::new(AKind::Text, ctx, Some(r), None, buf));
    }
}

/// `_walk_paragraph_v6`: the atoms of one `w:p` in document order.
pub fn walk6(d: &Dom, p: usize) -> Vec<Atom> {
    let mut out: Vec<Atom> = Vec::new();
    // (node, next child, ctx, path)
    let mut stack: Vec<(usize, usize, Option<usize>, Vec<usize>)> = vec![(p, 0, None, Vec::new())];
    while let Some(f) = stack.last() {
        let (node, i, ctx) = (f.0, f.1, f.2);
        let kids = &d.nodes[node].children;
        if i >= kids.len() {
            stack.pop();
            continue;
        }
        let el = kids[i];
        stack.last_mut().unwrap().1 += 1;
        if !d.is_el(el) || is_w(d, el, "smartTagPr") || is_w(d, el, "customXmlPr") {
            continue;
        }
        if ctx.is_none() && is_w(d, el, "pPr") {
            continue;
        }
        let start = out.len();
        if is_w(d, el, "r") {
            walk_runs(d, el, ctx, &mut out);
        } else if is_marker(d, el) {
            out.push(Atom::new(AKind::Marker, ctx, None, Some(el), String::new()));
        } else {
            let depth = stack.last().unwrap().3.len();
            let bx = if depth < MAX_INLINE_DEPTH { w6_content(d, el) } else { None };
            if let Some(b) = bx {
                let mut path = stack.last().unwrap().3.clone();
                path.push(el);
                stack.push((b, 0, Some(el), path));
                continue;
            }
            out.push(Atom::new(AKind::Obj, ctx, None, Some(el), String::new()));
        }
        let path = stack.last().unwrap().3.clone();
        for a in &mut out[start..] {
            a.path = path.clone();
        }
    }
    out.retain(|a| a.kind != AKind::Text || !a.text.is_empty());
    out
}

/// python-docx `Run.text`.
pub fn run_text(d: &Dom, r: usize) -> String {
    let mut s = String::new();
    for c in d.element_children(r) {
        match d.local_name(c) {
            "br" if is_w(d, c, "br") => {
                if get_w(d, c, "type").unwrap_or("textWrapping") == "textWrapping" {
                    s.push('\n');
                }
            }
            "cr" | "noBreakHyphen" | "ptab" | "t" | "tab" if d.nodes[c].ns.as_deref() == Some(W) => s.push_str(&text_of(d, c)),
            _ => {}
        }
    }
    s
}

/// python-docx `Paragraph.text`: its direct runs and the runs of its direct hyperlinks.
pub fn para_text(d: &Dom, p: usize) -> String {
    let mut s = String::new();
    for c in d.element_children(p) {
        if is_w(d, c, "r") {
            s.push_str(&run_text(d, c));
        } else if is_w(d, c, "hyperlink") {
            for r in d.element_children(c).filter(|&r| is_w(d, r, "r")) {
                s.push_str(&run_text(d, r));
            }
        }
    }
    s
}

fn direct_runs(d: &Dom, p: usize) -> Vec<usize> {
    d.element_children(p).filter(|&c| is_w(d, c, "r")).collect()
}

fn content_runs(d: &Dom, p: usize) -> Vec<usize> {
    direct_runs(d, p).into_iter().filter(|&r| !run_text(d, r).is_empty()).collect()
}

fn runs_cover(d: &Dom, p: usize) -> bool {
    direct_runs(d, p).iter().map(|&r| run_text(d, r)).collect::<String>() == para_text(d, p)
}

/// `_has_text` (walk 6).
pub fn has_text(d: &Dom, p: usize) -> bool {
    !py_strip(&para_text(d, p)).is_empty() || walk6(d, p).iter().any(|a| a.kind == AKind::Text && !py_strip(&a.text).is_empty())
}

/// An object-mode paragraph (`_object_layout`).
#[derive(Clone, Debug)]
pub struct Layout {
    pub atoms: Vec<Atom>,
    pub texts: Vec<usize>,
    pub groups: Vec<Vec<usize>>,
    pub tail: Vec<(usize, Option<usize>)>,
    pub runs: Vec<Vec<usize>>,
}

pub fn object_layout(d: &Dom, p: usize) -> Option<Layout> {
    let mut atoms = walk6(d, p);
    if !atoms.iter().any(|a| a.kind == AKind::Obj || (a.kind == AKind::Text && a.ctx.is_some())) {
        return None;
    }
    let (mut texts, mut groups, mut runs): (Vec<usize>, Vec<Vec<usize>>, Vec<Vec<usize>>) = (Vec::new(), Vec::new(), Vec::new());
    let mut pending: Vec<(usize, Option<usize>)> = Vec::new();
    let mut prev: Option<usize> = None;
    for i in 0..atoms.len() {
        if atoms[i].kind == AKind::Marker {
            pending.push((atoms[i].el.unwrap(), atoms[i].ctx));
            continue;
        }
        atoms[i].markers = std::mem::take(&mut pending);
        if atoms[i].kind == AKind::Text {
            texts.push(i);
            let same = runs.last().is_some_and(|r: &Vec<usize>| atoms[r[0]].run == atoms[i].run);
            if same {
                runs.last_mut().unwrap().push(i);
            } else {
                runs.push(vec![i]);
            }
            atoms[i].rkey = runs.len() - 1;
        } else {
            if prev.is_some_and(|q| atoms[q].kind == AKind::Obj) {
                groups.last_mut().unwrap().push(i);
            } else {
                groups.push(vec![i]);
            }
            atoms[i].group = groups.len() - 1;
        }
        prev = Some(i);
    }
    Some(Layout { atoms, texts, groups, tail: pending, runs })
}

fn x_base(n_runs: usize) -> usize {
    if n_runs > 1 {
        n_runs + 1
    } else {
        1
    }
}

/// A piece of LocOS unit text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Open(usize),
    Close(usize),
    Point(usize),
}

/// The pieces as LocOS text.
pub fn pieces_text(p: &[Piece]) -> String {
    let mut s = String::new();
    for x in p {
        match x {
            Piece::Text(t) => s.push_str(t),
            Piece::Open(n) => s.push_str(&format!("\u{27e6}r{n}\u{27e7}")),
            Piece::Close(n) => s.push_str(&format!("\u{27e6}/r{n}\u{27e7}")),
            Piece::Point(n) => s.push_str(&format!("\u{27e6}x{n}\u{27e7}")),
        }
    }
    s
}

/// What `_tokenize_paragraph` gives: the unit text and, for each run pair, the run's `w:rPr` (its inline map).
pub struct Tokenized {
    pub pieces: Vec<Piece>,
    /// Pair number (as LocOS's map key) to the run's `w:rPr`; empty for a paragraph without run tokens.
    pub rprs: BTreeMap<String, Option<usize>>,
    pub layout: Option<Layout>,
}

/// `_tokenize_paragraph` (walk 6).
pub fn tokenize(d: &Dom, p: usize) -> Tokenized {
    if let Some(layout) = object_layout(d, p) {
        let wrap = layout.runs.len() > 1;
        let base = x_base(layout.runs.len());
        let mut pieces = Vec::new();
        let mut rprs = BTreeMap::new();
        let mut seen: HashSet<usize> = HashSet::new();
        for (i, a) in layout.atoms.iter().enumerate() {
            match a.kind {
                AKind::Text => {
                    let members = &layout.runs[a.rkey];
                    let n = a.rkey + 1;
                    if wrap && members[0] == i {
                        rprs.insert(n.to_string(), find_w(d, a.run.unwrap(), "rPr"));
                        pieces.push(Piece::Open(n));
                    }
                    pieces.push(Piece::Text(a.text.clone()));
                    if wrap && *members.last().unwrap() == i {
                        pieces.push(Piece::Close(n));
                    }
                }
                AKind::Obj if seen.insert(a.group) => pieces.push(Piece::Point(base + a.group)),
                _ => {}
            }
        }
        return Tokenized { pieces: join_texts(pieces), rprs, layout: Some(layout) };
    }
    if !runs_cover(d, p) {
        return Tokenized { pieces: vec![Piece::Text(para_text(d, p))], rprs: BTreeMap::new(), layout: None };
    }
    let runs = content_runs(d, p);
    if runs.len() <= 1 {
        return Tokenized { pieces: vec![Piece::Text(para_text(d, p))], rprs: BTreeMap::new(), layout: None };
    }
    let mut pieces = Vec::new();
    let mut rprs = BTreeMap::new();
    for (k, &r) in runs.iter().enumerate() {
        let n = k + 1;
        rprs.insert(n.to_string(), find_w(d, r, "rPr"));
        pieces.push(Piece::Open(n));
        pieces.push(Piece::Text(run_text(d, r)));
        pieces.push(Piece::Close(n));
    }
    Tokenized { pieces, rprs, layout: None }
}

fn join_texts(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    for p in pieces {
        if let (Piece::Text(t), Some(Piece::Text(last))) = (&p, out.last_mut()) {
            last.push_str(t);
            continue;
        }
        out.push(p);
    }
    out
}

// ------------------------------------------------------------------------------------------ writing

/// python-docx's `Run.text = text`: every child but `w:rPr` removed (lxml drops the text that follows a removed
/// element with it), then `w:t` / `w:tab` / `w:br` appended for the text.
pub fn set_run_text(d: &mut Dom, r: usize, text: &str) {
    let mut drop_tail = false;
    for c in d.nodes[r].children.clone() {
        match d.nodes[c].kind {
            Kind::Element if is_w(d, c, "rPr") => drop_tail = false,
            Kind::Element => {
                d.detach(c);
                drop_tail = true;
            }
            Kind::Text if drop_tail => d.detach(c),
            _ => drop_tail = false,
        }
    }
    let mut buf = String::new();
    for ch in text.chars() {
        match ch {
            '\t' => {
                add_t(d, r, &std::mem::take(&mut buf));
                let t = new_w(d, r, "tab");
                d.append(r, t);
            }
            '\r' | '\n' => {
                add_t(d, r, &std::mem::take(&mut buf));
                let b = new_w(d, r, "br");
                d.append(r, b);
            }
            c => buf.push(c),
        }
    }
    add_t(d, r, &buf);
}

fn add_t(d: &mut Dom, r: usize, text: &str) {
    if text.is_empty() {
        return;
    }
    let t = new_w(d, r, "t");
    d.append_text(t, text);
    if py_strip(text).len() < text.len() {
        d.set(t, "xml:space", "preserve");
    }
    d.append(r, t);
}

/// `para.add_run(text)`: a new `w:r` at the end of the paragraph.
fn add_run(d: &mut Dom, p: usize, text: &str) -> usize {
    let r = new_w(d, p, "r");
    d.append(p, r);
    if !text.is_empty() {
        set_run_text(d, r, text);
    }
    r
}

/// Every well-formed LocOS token (`⟦/?[a-z]?\d+⟧`, ASCII digits): (start, end, the text between the brackets).
pub fn find_tokens(text: &str) -> Vec<(usize, usize, &str)> {
    const OPEN: &str = "\u{27e6}";
    const CLOSE: &str = "\u{27e7}";
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = text[i..].find(OPEN) {
        let s = i + off;
        let inner = s + OPEN.len();
        let mut j = inner;
        if j < b.len() && b[j] == b'/' {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_lowercase() {
            j += 1;
        }
        let digits = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > digits && text[j..].starts_with(CLOSE) {
            out.push((s, j + CLOSE.len(), &text[inner..j]));
            i = j + CLOSE.len();
        } else {
            i = inner;
        }
    }
    out
}

/// `_split_on_run_tokens`: (key, text) per stretch between tokens.
fn split_on_run_tokens(text: &str) -> Vec<(Option<String>, String)> {
    let mut spans = Vec::new();
    let mut cur: Option<String> = None;
    let mut pos = 0;
    for (s, e, inner) in find_tokens(text) {
        spans.push((cur.clone(), text[pos..s].to_string()));
        pos = e;
        if inner.starts_with('/') {
            cur = None;
        } else {
            let key = inner.strip_prefix('r').unwrap_or(inner);
            cur = (!key.is_empty() && key.bytes().all(|b| b.is_ascii_digit())).then(|| key.to_string());
        }
    }
    spans.push((cur, text[pos..].to_string()));
    spans
}

/// Python's `round` (half to even) of `before * n_new / n_old`.
fn py_round_ratio(before: usize, n_new: usize, n_old: usize) -> i64 {
    let x = before as f64 * n_new as f64 / n_old as f64;
    let f = x.floor();
    let diff = x - f;
    let r = if diff > 0.5 {
        f + 1.0
    } else if diff < 0.5 || (f as i64) % 2 == 0 {
        f
    } else {
        f + 1.0
    };
    r as i64
}

/// `_set_paragraph_text`.
fn set_paragraph_text(d: &mut Dom, p: usize, text: &str, rprs: &BTreeMap<String, Option<usize>>) {
    let spans = split_on_run_tokens(text);
    let has_tokens = spans.iter().any(|(k, _)| k.is_some());
    if !has_tokens || rprs.is_empty() {
        let plain: String = spans.iter().map(|(_, t)| t.as_str()).collect();
        if !runs_cover(d, p) {
            for c in kids(d, p) {
                if is_any_w(d, c, &TEXT_CHILDREN) {
                    remove_with_tail(d, c);
                }
            }
        }
        let content = content_runs(d, p);
        let keep = content.first().copied().or_else(|| direct_runs(d, p).first().copied());
        match keep {
            Some(k) => {
                set_run_text(d, k, &plain);
                for r in direct_runs(d, p) {
                    if r != k {
                        set_run_text(d, r, "");
                    }
                }
            }
            None => {
                add_run(d, p, &plain);
            }
        }
        return;
    }
    let content: HashSet<usize> = content_runs(d, p).into_iter().collect();
    let n_content = content.len();
    let mut markers: Vec<(usize, usize)> = Vec::new();
    let mut seen = 0;
    for c in kids(d, p) {
        if is_w(d, c, "r") {
            if content.contains(&c) {
                seen += 1;
            }
        } else if is_marker(d, c) {
            markers.push((seen, c));
            remove_with_tail(d, c);
        }
    }
    for r in direct_runs(d, p) {
        remove_with_tail(d, r);
    }
    let mut new_runs = Vec::new();
    for (key, s) in &spans {
        if s.is_empty() {
            continue;
        }
        let run = add_run(d, p, s);
        new_runs.push(run);
        if let Some(Some(rpr)) = key.as_ref().and_then(|k| rprs.get(k)) {
            let copy = d.deep_copy(*rpr);
            d.insert(run, 0, copy);
        }
    }
    restore_range_markers(d, p, &new_runs, &markers, n_content);
}

/// lxml's `remove`: the element and the text that follows it (its tail).
fn remove_with_tail(d: &mut Dom, el: usize) {
    if let Some(par) = parent(d, el) {
        let kids = &d.nodes[par].children;
        let at = kids.iter().position(|&k| k == el).unwrap();
        let tail: Vec<usize> = kids[at + 1..].iter().copied().take_while(|&k| d.nodes[k].kind == Kind::Text).collect();
        for t in tail {
            d.detach(t);
        }
    }
    d.detach(el);
}

/// `_restore_range_markers`.
fn restore_range_markers(d: &mut Dom, p: usize, new_runs: &[usize], markers: &[(usize, usize)], n_old: usize) {
    if markers.is_empty() {
        return;
    }
    if new_runs.is_empty() {
        for &(_, m) in markers {
            d.append(p, m);
        }
        return;
    }
    let n_new = new_runs.len();
    let mut anchor_after: HashMap<usize, usize> = HashMap::new();
    for &(before, m) in markers {
        let b = if n_old == 0 { before as i64 } else { py_round_ratio(before, n_new, n_old) };
        let b = b.clamp(0, n_new as i64) as usize;
        if b == 0 {
            d.add_previous(new_runs[0], m);
        } else {
            let anchor = *anchor_after.get(&b).unwrap_or(&new_runs[b - 1]);
            d.add_next(anchor, m);
            anchor_after.insert(b, m);
        }
    }
}

// ------------------------------------------------------------------------------- object-mode writing

/// Fresh `w:id` values for the extra pieces of a split tracked change (`_revision_ids`).
pub struct IdGen {
    /// The largest numeric `w:id` seen, or the value Python's `int()` refuses (LocOS raises on first use).
    top: Result<i128, String>,
    next: Option<i128>,
}

impl IdGen {
    /// Scans the given roots (the main document and every part the walk opened).
    pub fn scan(doms: &[&Dom]) -> IdGen {
        let mut top: Result<i128, String> = Ok(-1);
        'outer: for d in doms {
            for el in d.iter(d.root) {
                let Some(v) = get_w(d, el, "id") else { continue };
                let digits = v.trim_start_matches('-');
                if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                    continue;
                }
                // Python: int(v) of "--5" raises ValueError
                let parsed = if v.len() - digits.len() > 1 { None } else { v.parse::<i128>().ok() };
                match parsed {
                    Some(n) => {
                        if let Ok(t) = &mut top {
                            *t = (*t).max(n);
                        }
                    }
                    None if v.len() - digits.len() > 1 => {
                        top = Err(format!("invalid literal for int() with base 10: {v:?}"));
                        break 'outer;
                    }
                    None => {}
                }
            }
        }
        IdGen { top, next: None }
    }

    fn take(&mut self) -> Result<String, Error> {
        if self.next.is_none() {
            let top = self.top.clone().map_err(Error::Write)?;
            self.next = Some(top + 1);
        }
        let n = self.next.unwrap();
        self.next = Some(n + 1);
        Ok(n.to_string())
    }
}

#[derive(Clone, Debug)]
enum Item {
    Text(Option<usize>, String),
    X(usize),
}

/// A text piece's home when the translation put it outside every pair (`_rehome_loose_text`): its wrapper and the
/// atom whose run lends the formatting.
type Home = Option<(Option<usize>, Option<usize>)>;

#[derive(Clone, Debug)]
enum Seq {
    Text(Option<usize>, String, Home),
    Group(usize),
}

#[derive(Clone, Debug)]
enum Fin {
    Marker(usize, Option<usize>),
    Obj(usize),
    Text(Option<usize>, String, Home),
}

enum Raw {
    T(String),
    X(Option<usize>),
    R(Option<usize>),
    Reset,
}

/// `_split_object_tokens`.
fn split_object_tokens(text: &str, lay: &Layout) -> Vec<Item> {
    let mut raw: Vec<Raw> = Vec::new();
    let mut pos = 0;
    for (s, e, inner) in find_tokens(text) {
        if s > pos {
            raw.push(Raw::T(text[pos..s].to_string()));
        }
        pos = e;
        let num = |rest: &str| -> Option<Option<usize>> { (!rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())).then(|| rest.parse::<usize>().ok()) };
        if let Some(n) = inner.strip_prefix('x').and_then(num) {
            raw.push(Raw::X(n));
        } else if let Some(n) = inner.strip_prefix('r').and_then(num) {
            raw.push(Raw::R(n));
        } else {
            raw.push(Raw::Reset);
        }
    }
    if pos < text.len() {
        raw.push(Raw::T(text[pos..].to_string()));
    }
    let ngroups = lay.groups.len();
    let x_ok = |v: Option<usize>, base: usize| v.is_some_and(|v| v >= base && v - base < ngroups);
    let r_ok = |v: Option<usize>, n: usize| v.is_some_and(|v| (1..=n).contains(&v));
    let fits = |n: usize| {
        let base = x_base(n);
        raw.iter().all(|k| match k {
            Raw::R(v) => r_ok(*v, n),
            Raw::X(v) => x_ok(*v, base),
            _ => true,
        })
    };
    let owners: Vec<Vec<usize>> = if lay.texts.len() != lay.runs.len() && !fits(lay.runs.len()) && fits(lay.texts.len()) {
        lay.texts.iter().map(|&t| vec![t]).collect()
    } else {
        lay.runs.clone()
    };
    let base = x_base(owners.len());
    let has_r = raw.iter().any(|k| matches!(k, Raw::R(v) if r_ok(*v, owners.len())));
    let mut default = None;
    if !has_r && !lay.texts.is_empty() {
        let first = lay.texts.iter().copied().find(|&t| lay.atoms[t].ctx.is_none()).unwrap_or(lay.texts[0]);
        default = owners.iter().position(|o| o.contains(&first));
    }
    let mut used: HashMap<usize, usize> = HashMap::new();
    let mut items = Vec::new();
    let mut cur = default;
    for k in raw {
        match k {
            Raw::T(v) => match cur {
                None => items.push(Item::Text(None, v)),
                Some(c) => {
                    let n = used.entry(c).or_insert(0);
                    let at = (*n).min(owners[c].len() - 1);
                    *n += 1;
                    items.push(Item::Text(Some(owners[c][at]), v));
                }
            },
            Raw::X(v) if x_ok(v, base) => items.push(Item::X(v.unwrap() - base)),
            Raw::R(v) if r_ok(v, owners.len()) => cur = Some(v.unwrap() - 1),
            _ if has_r => cur = None,
            _ => {}
        }
    }
    items
}

fn fld_types(d: &Dom, lay: &Layout, g: usize) -> Vec<Option<String>> {
    lay.groups[g].iter().filter_map(|&a| lay.atoms[a].el.filter(|&e| is_w(d, e, "fldChar")).map(|e| get_w(d, e, "fldCharType").map(str::to_string))).collect()
}

fn fld_residue(types: &[Option<String>]) -> Vec<&'static str> {
    let mut stack: Vec<&'static str> = Vec::new();
    for t in types {
        match t.as_deref() {
            Some("begin") => stack.push("b"),
            Some("separate") if stack.last() == Some(&"b") => *stack.last_mut().unwrap() = "bs",
            Some("end") if matches!(stack.last(), Some(&"b") | Some(&"bs")) => {
                stack.pop();
            }
            Some("separate") => stack.push("s"),
            Some("end") => stack.push("e"),
            _ => {}
        }
    }
    stack
}

/// `_fix_field_order`.
fn fix_field_order(d: &Dom, lay: &Layout, seq: Vec<Seq>) -> Vec<Seq> {
    let slots: Vec<usize> = seq.iter().enumerate().filter_map(|(i, s)| matches!(s, Seq::Group(g) if !fld_types(d, lay, *g).is_empty()).then_some(i)).collect();
    if slots.len() < 2 {
        return seq;
    }
    let gid = |s: &Seq| if let Seq::Group(g) = s { *g } else { unreachable!() };
    let emitted: Vec<Option<String>> = slots.iter().flat_map(|&i| fld_types(d, lay, gid(&seq[i]))).collect();
    let mut ordered: Vec<usize> = slots.iter().map(|&i| gid(&seq[i])).collect();
    ordered.sort();
    let original: Vec<Option<String>> = ordered.iter().flat_map(|&g| fld_types(d, lay, g)).collect();
    if fld_residue(&emitted) == fld_residue(&original) {
        return seq;
    }
    let mut out = seq;
    for (i, g) in slots.into_iter().zip(ordered) {
        out[i] = Seq::Group(g);
    }
    out
}

/// `_fields_in_links`: begin atom -> (end atom, wrappers they share).
fn fields_in_links(d: &Dom, atoms: &[Atom]) -> HashMap<usize, (usize, usize)> {
    let mut out = HashMap::new();
    let mut stack: Vec<usize> = Vec::new();
    for (i, a) in atoms.iter().enumerate() {
        let Some(el) = a.el.filter(|&e| a.kind == AKind::Obj && is_w(d, e, "fldChar")) else { continue };
        match get_w(d, el, "fldCharType") {
            Some("begin") => stack.push(i),
            Some("end") => {
                if let Some(b) = stack.pop() {
                    let (x, y) = (&atoms[b].path, &a.path);
                    let n = x.iter().zip(y.iter()).take_while(|(p, q)| p == q).count();
                    if n > 0 {
                        out.insert(b, (i, n));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// `_rehome_loose_text`.
fn rehome_loose_text(lay: &Layout, seq: Vec<Seq>, paths: &HashMap<usize, Vec<usize>>) -> Vec<Seq> {
    let chain = |ctx: Option<usize>| -> Vec<usize> {
        match ctx {
            None => Vec::new(),
            Some(c) => paths.get(&c).cloned().unwrap_or_else(|| vec![c]),
        }
    };
    let edge = |s: &Seq, last: bool| -> (Option<usize>, Option<usize>) {
        match s {
            Seq::Group(g) => {
                let m = &lay.groups[*g];
                (lay.atoms[if last { *m.last().unwrap() } else { m[0] }].ctx, None)
            }
            Seq::Text(Some(a), _, _) => (lay.atoms[*a].ctx, Some(*a)),
            Seq::Text(None, _, Some(h)) => *h,
            Seq::Text(None, _, None) => (None, None),
        }
    };
    let mut out = seq;
    for i in 0..out.len() {
        let Seq::Text(None, s, _) = &out[i] else { continue };
        let s = s.clone();
        let (prev_ctx, prev_fmt) = if i > 0 { edge(&out[i - 1], true) } else { (None, None) };
        let mut j = i + 1;
        while j < out.len() && matches!(out[j], Seq::Text(None, _, _)) {
            j += 1;
        }
        let (next_ctx, next_fmt) = if j < out.len() { edge(&out[j], false) } else { (None, None) };
        let (a, b) = (chain(prev_ctx), chain(next_ctx));
        let k = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
        if k > 0 {
            out[i] = Seq::Text(None, s, Some((Some(a[k - 1]), prev_fmt.or(next_fmt))));
        }
    }
    out
}

/// `_marker_items`.
fn marker_items(markers: &[(usize, Option<usize>)], ctx: Option<usize>, path: Option<&[usize]>) -> Vec<Fin> {
    match path {
        None => markers.iter().map(|&(el, mctx)| Fin::Marker(el, if mctx == ctx { mctx } else { None })).collect(),
        Some(path) => markers.iter().map(|&(el, mctx)| Fin::Marker(el, if mctx.is_some_and(|m| path.contains(&m)) { ctx } else { None })).collect(),
    }
}

/// `_fix_marker_order`.
fn fix_marker_order(d: &Dom, fin: Vec<Fin>) -> Vec<Fin> {
    let mut result: Vec<(usize, Fin)> = fin.into_iter().enumerate().collect();
    let snapshot: Vec<(usize, Option<usize>)> = result.iter().map(|(u, f)| (*u, if let Fin::Marker(el, _) = f { Some(*el) } else { None })).collect();
    for (uid, el) in snapshot {
        let Some(el) = el else { continue };
        let name = d.local_name(el).to_string();
        let Some(stem) = name.strip_suffix("End") else { continue };
        let mid = get_w(d, el, "id").map(str::to_string);
        let start = format!("{stem}Start");
        let s_pos = result.iter().position(|(_, f)| matches!(f, Fin::Marker(e, _) if d.local_name(*e) == start && get_w(d, *e, "id").map(str::to_string) == mid));
        let e_pos = result.iter().position(|(u, _)| *u == uid).unwrap();
        if let Some(s_pos) = s_pos {
            if e_pos < s_pos {
                let item = result.remove(e_pos);
                result.insert(s_pos, item);
            }
        }
    }
    result.into_iter().map(|(_, f)| f).collect()
}

/// `_strong_direction`: "ltr" / "rtl" from the first strong character, "" when there is none. The bidi classes
/// are approximated: right-to-left letters are those of the Hebrew, Arabic, Syriac, Thaana, NKo, Samaritan,
/// Mandaic and Arabic presentation blocks and the RTL supplementary blocks; any other alphabetic character is
/// left-to-right.
pub fn strong_direction(text: &str) -> &'static str {
    for c in text.chars() {
        let u = c as u32;
        let rtl = (0x0590..=0x08FF).contains(&u) || (0xFB1D..=0xFDFF).contains(&u) || (0xFE70..=0xFEFF).contains(&u) || (0x10800..=0x10FFF).contains(&u) || (0x1E800..=0x1EFFF).contains(&u);
        if rtl && c.is_alphabetic() {
            return "rtl";
        }
        if !rtl && c.is_alphabetic() && !is_combining(u) {
            return "ltr";
        }
    }
    ""
}

fn is_combining(u: u32) -> bool {
    (0x0300..=0x036F).contains(&u) || (0x1AB0..=0x1AFF).contains(&u) || (0x1DC0..=0x1DFF).contains(&u) || (0x20D0..=0x20FF).contains(&u) || (0xFE20..=0xFE2F).contains(&u)
}

struct Mat<'a> {
    p: usize,
    paths: &'a HashMap<usize, Vec<usize>>,
    fits_source: &'a HashSet<usize>,
    ctx: Option<usize>,
    bx: usize,
    wrap_for: Option<usize>,
    wrap: Option<usize>,
    open: Vec<(usize, usize)>,
    open_ends: Vec<usize>,
    floors: HashMap<usize, usize>,
    cloned: HashSet<usize>,
    fit: Vec<usize>,
}

impl Mat<'_> {
    fn box_for(&mut self, d: &mut Dom, ctx: Option<usize>, ids: &mut IdGen) -> Result<usize, Error> {
        if ctx == self.ctx {
            return Ok(self.bx);
        }
        let want: Vec<usize> = match ctx {
            None => Vec::new(),
            Some(c) => self.paths.get(&c).cloned().unwrap_or_else(|| vec![c]),
        };
        if !self.open_ends.is_empty() {
            let floor = self.open_ends.iter().map(|e| self.floors[e]).max().unwrap();
            if want.len() < floor || self.open.len() < floor {
                return Ok(self.bx);
            }
            if (0..floor).any(|i| self.open[i].0 != want[i]) {
                return Ok(self.bx);
            }
        }
        self.wrap_for = None;
        self.wrap = None;
        let k = self.open.iter().zip(want.iter()).take_while(|((o, _), w)| o == *w).count();
        self.open.truncate(k);
        let mut bx = self.open.last().map(|o| o.1).unwrap_or(self.p);
        for &orig in &want[k..] {
            let again = self.cloned.contains(&orig);
            let (clone, inner) = clone_container(d, orig, again, ids)?;
            self.cloned.insert(orig);
            if self.fits_source.contains(&orig) {
                self.fit.push(clone);
            }
            d.append(bx, clone);
            self.open.push((orig, inner));
            bx = inner;
        }
        self.bx = bx;
        self.ctx = ctx;
        Ok(self.bx)
    }
}

/// `_clone_container`: (element to append, element that receives the content).
fn clone_container(d: &mut Dom, ctx: usize, again: bool, ids: &mut IdGen) -> Result<(usize, usize), Error> {
    let name = d.nodes[ctx].name.clone();
    let scope = d.nodes[ctx].scope;
    let clone = d.new_element(&name, scope, Some(W.to_string()));
    let attrs: Vec<(String, String)> = d.nodes[ctx].attrs.iter().filter(|(k, _)| !(k == "xmlns" || k.starts_with("xmlns:"))).cloned().collect();
    d.nodes[clone].attrs = attrs;
    if is_w(d, ctx, "sdt") {
        for tag in ["sdtPr", "sdtEndPr"] {
            if let Some(pr) = find_w(d, ctx, tag) {
                let pr = d.deep_copy(pr);
                if again && tag == "sdtPr" {
                    for old in kids(d, pr) {
                        if is_w(d, old, "id") {
                            remove_with_tail(d, old);
                        }
                    }
                }
                d.append(clone, pr);
            }
        }
        let content = new_w(d, clone, "sdtContent");
        d.append(clone, content);
        return Ok((clone, content));
    }
    let pr_tag = if is_w(d, ctx, "smartTag") { "smartTagPr" } else { "customXmlPr" };
    if is_w(d, ctx, "smartTag") || is_w(d, ctx, "customXml") {
        if let Some(pr) = find_w(d, ctx, pr_tag) {
            let pr = d.deep_copy(pr);
            d.append(clone, pr);
        }
    }
    if (is_w(d, ctx, "ins") || is_w(d, ctx, "moveTo")) && again {
        let id = ids.take()?;
        set_w(d, clone, "id", &id);
    }
    Ok((clone, clone))
}

/// `_set_paragraph_objects`.
fn set_paragraph_objects(d: &mut Dom, p: usize, text: &str, lay: &Layout, ids: &mut IdGen) -> Result<(), Error> {
    let items = split_object_tokens(text, lay);
    let atoms = &lay.atoms;
    // 1. the emission sequence
    let mut seq: Vec<Seq> = Vec::new();
    let mut placed: HashSet<usize> = HashSet::new();
    for it in items {
        match it {
            Item::X(g) => {
                if placed.insert(g) {
                    seq.push(Seq::Group(g));
                }
            }
            Item::Text(a, s) if !s.is_empty() => seq.push(Seq::Text(a, s, None)),
            _ => {}
        }
    }
    // 2. groups the translation lost: back after the text they followed
    for (g, members) in lay.groups.iter().enumerate() {
        if placed.contains(&g) {
            continue;
        }
        let first = members[0];
        let anchor = lay.texts.iter().copied().rfind(|&t| t < first);
        let mut at;
        match anchor {
            None => {
                at = 0;
                while at < seq.len() && matches!(seq[at], Seq::Group(h) if placed.contains(&h) && h < g) {
                    at += 1;
                }
            }
            Some(anchor) => {
                let mut hits: Vec<usize> = seq.iter().enumerate().filter_map(|(i, s)| matches!(s, Seq::Text(Some(a), _, _) if *a == anchor).then_some(i)).collect();
                if hits.is_empty() {
                    hits = seq.iter().enumerate().filter_map(|(i, s)| matches!(s, Seq::Text(Some(a), _, _) if atoms[*a].run == atoms[anchor].run).then_some(i)).collect();
                }
                at = hits.last().map(|h| h + 1).unwrap_or(seq.len());
                while at < seq.len() && matches!(seq[at], Seq::Group(h) if h < g) {
                    at += 1;
                }
            }
        }
        seq.insert(at, Seq::Group(g));
        placed.insert(g);
    }
    let seq = fix_field_order(d, lay, seq);
    let paths: HashMap<usize, Vec<usize>> = atoms.iter().filter_map(|a| a.ctx.map(|c| (c, a.path.clone()))).collect();
    let seq = rehome_loose_text(lay, seq, &paths);
    // 3. markers travel before the first emission of the atom they preceded
    let mut fin: Vec<Fin> = Vec::new();
    let mut marked: HashSet<usize> = HashSet::new();
    for s in seq {
        let owner = match &s {
            Seq::Text(a, _, _) => *a,
            Seq::Group(g) => Some(lay.groups[*g][0]),
        };
        match owner {
            Some(o) if !marked.contains(&o) => {
                marked.insert(o);
                let here: Vec<usize> = match &s {
                    Seq::Text(..) => vec![o],
                    Seq::Group(g) => lay.groups[*g].clone(),
                };
                for &a in &here {
                    fin.extend(marker_items(&atoms[a].markers, atoms[a].ctx, Some(&atoms[a].path)));
                    if matches!(s, Seq::Group(_)) {
                        fin.push(Fin::Obj(a));
                    }
                }
                if let Seq::Text(a, t, h) = s {
                    fin.push(Fin::Text(a, t, h));
                }
            }
            _ => match s {
                Seq::Group(g) => fin.extend(lay.groups[g].iter().map(|&a| Fin::Obj(a))),
                Seq::Text(a, t, h) => fin.push(Fin::Text(a, t, h)),
            },
        }
    }
    for &t in &lay.texts {
        if !marked.contains(&t) {
            fin.extend(marker_items(&atoms[t].markers, None, None));
        }
    }
    fin.extend(marker_items(&lay.tail, None, None));
    let fin = fix_marker_order(d, fin);

    // walk 6: a w:bdo / w:dir whose direction matched its source text follows the translation's
    let mut fits_source: HashSet<usize> = HashSet::new();
    for chain in paths.values() {
        for &w in chain {
            if (is_w(d, w, "bdo") || is_w(d, w, "dir")) && matches!(get_w(d, w, "val"), Some("ltr") | Some("rtl")) && Some(strong_direction(&t_text(d, w))) == get_w(d, w, "val") {
                fits_source.insert(w);
            }
        }
    }
    // 4. materialise
    let mut keep_text = true;
    for c in d.nodes[p].children.clone() {
        match d.nodes[c].kind {
            Kind::Text => {
                if !keep_text {
                    d.detach(c);
                }
            }
            Kind::Element if is_w(d, c, "pPr") => keep_text = true,
            _ => {
                d.detach(c);
                keep_text = false;
            }
        }
    }
    let field_ends = fields_in_links(d, atoms);
    let mut m = Mat {
        p,
        paths: &paths,
        fits_source: &fits_source,
        ctx: None,
        bx: p,
        wrap_for: None,
        wrap: None,
        open: Vec::new(),
        open_ends: Vec::new(),
        floors: HashMap::new(),
        cloned: HashSet::new(),
        fit: Vec::new(),
    };
    for item in fin {
        match item {
            Fin::Marker(el, ctx) => {
                let bx = m.box_for(d, ctx, ids)?;
                d.append(bx, el);
                m.wrap_for = None;
                m.wrap = None;
            }
            Fin::Obj(ai) => {
                let a = &atoms[ai];
                let bx = m.box_for(d, a.ctx, ids)?;
                let el = a.el.unwrap();
                let Some(run) = a.run else {
                    d.append(bx, el);
                    m.wrap_for = None;
                    m.wrap = None;
                    continue;
                };
                if m.wrap_for != Some(run) {
                    let r = new_w(d, bx, "r");
                    if let Some(rpr) = find_w(d, run, "rPr") {
                        let c = d.deep_copy(rpr);
                        d.append(r, c);
                    }
                    d.append(bx, r);
                    m.wrap_for = Some(run);
                    m.wrap = Some(r);
                }
                d.append(m.wrap.unwrap(), el);
                if let (Some(&(end, floor)), true) = (field_ends.get(&ai), m.ctx.is_some()) {
                    m.open_ends.push(end);
                    m.floors.insert(end, floor);
                } else if let Some(at) = m.open_ends.iter().position(|&e| e == ai) {
                    m.open_ends.truncate(at);
                }
            }
            Fin::Text(a, s, home) => {
                let (ctx, fmt) = match home {
                    Some((c, f)) => (c, f),
                    None => (a.and_then(|a| atoms[a].ctx), a),
                };
                let bx = m.box_for(d, ctx, ids)?;
                let r = new_w(d, bx, "r");
                if let Some(f) = fmt {
                    if let Some(rpr) = find_w(d, atoms[f].run.unwrap(), "rPr") {
                        let c = d.deep_copy(rpr);
                        d.append(r, c);
                    }
                }
                d.append(bx, r);
                set_run_text(d, r, &s);
                m.wrap_for = None;
                m.wrap = None;
            }
        }
    }
    for el in m.fit {
        let want = strong_direction(&t_text(d, el));
        if !want.is_empty() && Some(want) != get_w(d, el, "val") {
            set_w(d, el, "val", want);
        }
    }
    Ok(())
}

/// `DocxFilter._write`: one paragraph rebuilt from its translation (walk 6).
pub fn write(d: &mut Dom, p: usize, text: &str, ids: &mut IdGen) -> Result<(), Error> {
    let tok = tokenize(d, p);
    match &tok.layout {
        Some(lay) => set_paragraph_objects(d, p, text, lay, ids),
        None => {
            set_paragraph_text(d, p, text, &tok.rprs);
            Ok(())
        }
    }
}
