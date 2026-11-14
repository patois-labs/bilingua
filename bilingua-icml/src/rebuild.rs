//! Rebuilding translated paragraphs and stamping the target language (port of the LocOS IDML `_pieces`,
//! `_Filler`, `_rebuild`, `_stamp`, `_stamp_rtl`).

use crate::dom::*;
use crate::walk::*;
use std::collections::HashSet;

/// A translated paragraph as a flat list: text, tag opens, tag closes and points. Tag ids are the token numbers
/// of the source paragraph, as text.
#[derive(Clone, Debug)]
pub enum TP {
    Text(String),
    Open(String),
    Close(String),
    Point(String),
}

enum Pk {
    T(String),
    P(usize),
    W(usize),
}

struct Piece {
    run: Option<usize>,
    chain: Vec<usize>,
    kind: Pk,
}

enum Sk {
    R(usize),
    G,
    Ph,
}

fn key_of(tok: &Tok, key: &str) -> Option<usize> {
    let n: usize = key.parse().ok()?;
    if n >= 1 && n <= tok.reg.len() && n.to_string() == key {
        Some(n)
    } else {
        None
    }
}

fn pieces(tok: &Tok, text: &[TP]) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut used: HashSet<usize> = HashSet::new();
    let mut stack: Vec<(String, Sk, usize)> = Vec::new();
    let mut last: Option<usize> = None;

    // `last` is the run tag closed most recently: text typed outside every run tag after it stays in that run, so
    // text after a hidden range is not moved in front of it (LocOS sends it to the first run).
    fn ctx(tok: &Tok, stack: &[(String, Sk, usize)], last: Option<usize>) -> (Option<usize>, Vec<usize>) {
        let mut run = tok.dominant.or(last);
        let mut chain = Vec::new();
        for (_, k, n) in stack {
            match k {
                Sk::R(r) => run = Some(*r),
                Sk::G => chain.push(*n),
                Sk::Ph => {}
            }
        }
        if run.is_none() {
            run = tok.reg.iter().find_map(|e| if let Entry::Run(r) = e { Some(*r) } else { None });
        }
        (run, chain)
    }

    for tp in text {
        match tp {
            TP::Text(s) => {
                if !s.is_empty() {
                    let (run, chain) = ctx(tok, &stack, last);
                    out.push(Piece { run, chain, kind: Pk::T(s.clone()) });
                }
            }
            TP::Close(key) => {
                if stack.last().is_some_and(|(k, _, _)| k == key) {
                    if let Some((_, Sk::R(r), _)) = stack.pop() {
                        last = Some(r);
                    }
                }
            }
            TP::Open(key) | TP::Point(key) => {
                let is_point = matches!(tp, TP::Point(_));
                let n = key_of(tok, key);
                let entry = n.map(|n| &tok.reg[n - 1]);
                let ok = match (entry, n) {
                    (Some(e), Some(n)) => !used.contains(&n) && matches!(e, Entry::Point { .. }) == is_point,
                    _ => false,
                };
                if !ok {
                    if !is_point {
                        stack.push((key.clone(), Sk::Ph, 0));
                    }
                    continue;
                }
                let (Some(n), Some(entry)) = (n, entry) else { continue };
                used.insert(n);
                match entry {
                    Entry::Point { .. } => {
                        let (run, chain) = ctx(tok, &stack, last);
                        out.push(Piece { run, chain, kind: Pk::P(n) });
                    }
                    Entry::Run(r) => stack.push((key.clone(), Sk::R(*r), n)),
                    Entry::Wrap { .. } => {
                        stack.push((key.clone(), Sk::G, n));
                        let (run, chain) = ctx(tok, &stack, last);
                        out.push(Piece { run, chain, kind: Pk::W(n) });
                    }
                }
            }
        }
    }
    for &n in &tok.order {
        if !used.contains(&n) {
            match &tok.reg[n - 1] {
                Entry::Point { run, .. } => out.push(Piece { run: Some(*run), chain: Vec::new(), kind: Pk::P(n) }),
                Entry::Wrap { run, .. } => out.push(Piece { run: Some(*run), chain: Vec::new(), kind: Pk::W(n) }),
                Entry::Run(_) => {}
            }
        }
    }
    out
}

fn wrap_el(tok: &Tok, key: usize) -> Id {
    match &tok.reg[key - 1] {
        Entry::Wrap { el, .. } => *el,
        _ => NIL,
    }
}

struct Filler {
    root: Id,
    chain: Vec<(usize, Id)>,
}

impl Filler {
    fn new(root: Id) -> Filler {
        Filler { root, chain: Vec::new() }
    }

    fn target(&self) -> Id {
        self.chain.last().map_or(self.root, |c| c.1)
    }

    fn sync(&mut self, dom: &mut Dom, tok: &Tok, opened: &mut HashSet<usize>, chain: &[usize]) {
        let mut common = 0;
        while common < self.chain.len().min(chain.len()) && self.chain[common].0 == chain[common] {
            common += 1;
        }
        self.chain.truncate(common);
        for &key in &chain[common..] {
            if opened.contains(&key) {
                break;
            }
            let el = wrap_el(tok, key);
            if is_range(dom, el) && is_range(dom, self.target()) {
                continue;
            }
            opened.insert(key);
            let clone = shallow(dom, el);
            let t = self.target();
            dom.append(t, clone);
            self.chain.push((key, clone));
        }
    }

    fn content(&mut self, dom: &mut Dom, tok: &Tok) -> Id {
        let tgt = self.target();
        if let Some(&last) = dom.kids(tgt).last() {
            if dom.is_elem(last) && dom.local(last) == "Content" {
                return last;
            }
        }
        let c = sibling_elem(dom, self.root, "Content", tok.content_attrib.clone());
        dom.append(tgt, c);
        c
    }

    fn text(&mut self, dom: &mut Dom, tok: &Tok, opened: &mut HashSet<usize>, chain: &[usize], s: &str) {
        self.sync(dom, tok, opened, chain);
        let c = self.content(dom, tok);
        dom.append_text(c, s);
    }

    fn point(&mut self, dom: &mut Dom, tok: &Tok, opened: &mut HashSet<usize>, chain: &[usize], key: usize) {
        self.sync(dom, tok, opened, chain);
        let Entry::Point { obj, wh, .. } = &tok.reg[key - 1] else { return };
        match (obj, wh) {
            (Pt::Ch(s), _) => self.text(dom, tok, opened, chain, s),
            (Pt::Node(n), Wh::Content) => {
                let c = self.content(dom, tok);
                dom.append(c, *n);
            }
            (Pt::Node(n), _) => {
                let t = self.target();
                dom.append(t, *n);
            }
        }
    }

    fn wrap(&mut self, dom: &mut Dom, tok: &Tok, opened: &mut HashSet<usize>, chain: &[usize], key: usize) {
        if let Some(i) = chain.iter().position(|k| *k == key) {
            self.sync(dom, tok, opened, &chain[..=i]);
        } else {
            self.sync(dom, tok, opened, chain);
            let el = wrap_el(tok, key);
            if !opened.contains(&key) && !is_range(dom, el) {
                opened.insert(key);
                let clone = shallow(dom, el);
                let t = self.target();
                dom.append(t, clone);
            }
        }
    }

    fn apply(&mut self, dom: &mut Dom, tok: &Tok, opened: &mut HashSet<usize>, p: &Piece) {
        match &p.kind {
            Pk::T(s) => self.text(dom, tok, opened, &p.chain, s),
            Pk::P(k) => self.point(dom, tok, opened, &p.chain, *k),
            Pk::W(k) => self.wrap(dom, tok, opened, &p.chain, *k),
        }
    }
}

fn unopened(dom: &mut Dom, tok: &Tok, opened: &mut HashSet<usize>, run: Option<usize>) -> Vec<Id> {
    let mut out = Vec::new();
    for &n in &tok.order {
        let Entry::Wrap { run: rr, el } = &tok.reg[n - 1] else { continue };
        if opened.contains(&n) || run.is_some_and(|r| r != *rr) {
            continue;
        }
        opened.insert(n);
        if dom.local(*el) != "CharacterStyleRange" {
            out.push(shallow(dom, *el));
        }
    }
    out
}

/// Rebuilds one paragraph from translated text; returns the ranges that now hold it.
pub fn rebuild(dom: &mut Dom, para: &Para, tok: &Tok, text: &[TP]) -> Vec<Id> {
    let pcs = pieces(tok, text);
    let br = para.runs.iter().rev().find_map(|r| r.br);
    let mut opened: HashSet<usize> = HashSet::new();
    if tok.mode_a {
        let parent = dom.n(para.runs[0].csr).parent;
        let mut old: Vec<Id> = Vec::new();
        for r in &para.runs {
            if !old.contains(&r.csr) {
                old.push(r.csr);
            }
        }
        let index = dom.index_of(parent, old[0]).unwrap_or(0);
        let base = tok.dominant.or_else(|| para.runs.iter().position(|r| !r.items.is_empty())).unwrap_or(0);
        let mut new_csrs: Vec<Id> = Vec::new();
        let mut filler: Option<Filler> = None;
        let mut cur_run: Option<usize> = None;
        for p in &pcs {
            let run = p.run.unwrap_or(base);
            if filler.is_none() || cur_run != Some(run) {
                let csr = shallow(dom, para.runs[run].csr);
                new_csrs.push(csr);
                filler = Some(Filler::new(csr));
                cur_run = Some(run);
            }
            if let Some(f) = filler.as_mut() {
                f.apply(dom, tok, &mut opened, p);
            }
        }
        if new_csrs.is_empty() {
            new_csrs.push(shallow(dom, para.runs[base].csr));
        }
        let last = new_csrs[new_csrs.len() - 1];
        for el in unopened(dom, tok, &mut opened, None) {
            dom.append(last, el);
        }
        if let Some(b) = br {
            dom.append(last, b);
        }
        let mut tail = None;
        for c in &old {
            tail = dom.remove_with_tail(*c);
        }
        let mut pos = index;
        for c in &new_csrs {
            dom.insert(parent, pos, *c);
            pos += 1;
            if let Some(t) = tail.as_deref().filter(|t| !t.is_empty()) {
                let tn = dom.new_text(t);
                dom.insert(parent, pos, tn);
                pos += 1;
            }
        }
        return new_csrs;
    }
    // Mode B: each visible run is rebuilt in place, in its own range.
    let live: Vec<usize> = (0..para.runs.len()).filter(|i| !para.runs[*i].hidden && !para.runs[*i].items.is_empty()).collect();
    let Some(&first) = live.first() else { return Vec::new() };
    let mut by_run: Vec<Vec<&Piece>> = (0..para.runs.len()).map(|_| Vec::new()).collect();
    for p in &pcs {
        let target = match p.run {
            Some(r) if live.contains(&r) => r,
            _ => first,
        };
        by_run[target].push(p);
    }
    for &ri in &live {
        let r = &para.runs[ri];
        for &m in &r.members {
            if dom.n(m).parent == r.csr {
                dom.remove_with_tail(m);
            }
        }
        if let Some(b) = r.br {
            if dom.n(b).parent == r.csr {
                dom.remove_with_tail(b);
            }
        }
        let mut filler = Filler::new(r.csr);
        for p in &by_run[ri] {
            filler.apply(dom, tok, &mut opened, p);
        }
    }
    for &ri in &live {
        let r = &para.runs[ri];
        for el in unopened(dom, tok, &mut opened, Some(ri)) {
            dom.append(r.csr, el);
        }
        if let Some(b) = r.br {
            dom.append(r.csr, b);
        }
    }
    live.iter().map(|ri| para.runs[*ri].csr).collect()
}

// --------------------------------------------------------------------------- stamping
fn wrapped_only(dom: &Dom, rng: Id, top: Id) -> bool {
    let mut el = dom.n(rng).parent;
    while el != NIL && el != top {
        if !(is_wrapper(dom, el) || is_range(dom, el)) {
            return false;
        }
        el = dom.n(el).parent;
    }
    true
}

/// Ranges inside the wrapper `top` (itself included in the walk) that sit under wrappers only.
fn wrapped_ranges(dom: &Dom, top: Id) -> Vec<Id> {
    let mut out = Vec::new();
    let mut stack = vec![top];
    while let Some(e) = stack.pop() {
        if is_range(dom, e) && wrapped_only(dom, e, top) {
            out.push(e);
        }
        stack.extend(dom.kids(e).into_iter().filter(|c| dom.is_elem(*c)));
    }
    out
}

fn range_children(dom: &Dom, el: Id) -> Vec<Id> {
    let mut out = Vec::new();
    for child in dom.kids(el) {
        if !dom.is_elem(child) {
            continue;
        }
        if is_range(dom, child) {
            out.push(child);
        } else if is_wrapper(dom, child) {
            out.extend(wrapped_ranges(dom, child));
        }
    }
    out
}

/// AppliedLanguage on every range that now holds translated text, nested ranges included.
pub fn stamp(dom: &mut Dom, ranges: &[Id], language: &str) {
    let mut stack: Vec<Id> = ranges.to_vec();
    while let Some(el) = stack.pop() {
        dom.set_attr(el, "AppliedLanguage", language);
        stack.extend(range_children(dom, el));
    }
}

const RTL_COMPOSER: &str = "HL Composer Optyca";
const RTL_SINGLE: &str = "HL Single Optyca";
const PARA_STOPS: [&str; 5] = ["Story", "Cell", "Footnote", "EndnoteRange", "Note"];

/// Right-to-left target: paragraph direction and the World-Ready composer on every translated paragraph.
pub fn stamp_rtl(dom: &mut Dom, ranges: &[Id]) {
    let mut paras: Vec<Id> = Vec::new();
    let mut seen: HashSet<Id> = HashSet::new();
    let mut stack: Vec<Id> = ranges.to_vec();
    while let Some(el) = stack.pop() {
        if dom.attr(el, "CharacterDirection") == Some("LeftToRightDirection") {
            dom.set_attr(el, "CharacterDirection", "DefaultDirection");
        }
        let mut up = dom.n(el).parent;
        while up != NIL && !PARA_STOPS.contains(&dom.local(up)) {
            if dom.local(up) == "ParagraphStyleRange" {
                if seen.insert(up) {
                    paras.push(up);
                }
                break;
            }
            up = dom.n(up).parent;
        }
        stack.extend(range_children(dom, el));
    }
    for psr in paras {
        dom.set_attr(psr, "ParagraphDirection", "RightToLeftDirection");
        let composer = dom.attr(psr, "Composer").unwrap_or("").to_string();
        if !composer.contains("Optyca") {
            let v = if composer.starts_with("HL Single") { RTL_SINGLE } else { RTL_COMPOSER };
            dom.set_attr(psr, "Composer", v);
        }
    }
}
