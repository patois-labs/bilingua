//! The story walk and the tokenizer: the same paragraph order for extract and merge (port of the LocOS IDML story
//! model that the ICML filter reuses). One paragraph becomes one unit; ranges, wrappers and points become tags.

use crate::dom::*;
use std::collections::{BTreeSet, HashMap, HashSet};

const WRAPPERS: [&str; 2] = ["HyperlinkTextSource", "XMLElement"];
const SUB_STORIES: [&str; 3] = ["Footnote", "EndnoteRange", "ParagraphStyleRange"];
const KEEP_ON_CLONE: [&str; 2] = ["Properties", "XMLAttribute"];
const PARA_CHARS: [char; 3] = ['\t', '\u{2028}', '\u{2029}'];

#[derive(Clone, Debug)]
pub enum Pt {
    Node(Id),
    Ch(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wh {
    Direct,
    Content,
    Char,
}

#[derive(Clone, Debug)]
pub enum Item {
    T(String),
    P(Pt, Wh),
    O(Id),
    C(Id),
}

#[derive(Clone, Debug)]
pub struct Run {
    pub csr: Id,
    pub hidden: bool,
    pub items: Vec<Item>,
    pub members: Vec<Id>,
    pub br: Option<Id>,
}

impl Run {
    fn new(csr: Id) -> Run {
        Run { csr, hidden: false, items: Vec::new(), members: Vec::new(), br: None }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Para {
    pub runs: Vec<Run>,
}

pub fn py_space(c: char) -> bool {
    bilingua_model::py_isspace_char(c)
}

fn unquote(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let h = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|x| u8::from_str_radix(x, 16).ok());
            if let Some(v) = h {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn hidden_csr(dom: &Dom, csr: Id, hidden: &BTreeSet<String>) -> bool {
    match dom.attr(csr, "AppliedConditions") {
        Some(a) => a.split(py_space).filter(|x| !x.is_empty()).any(|c| hidden.contains(&unquote(c))),
        None => false,
    }
}

pub fn is_range(dom: &Dom, id: Id) -> bool {
    dom.local(id) == "CharacterStyleRange"
}

pub fn is_wrapper(dom: &Dom, id: Id) -> bool {
    let name = dom.local(id);
    WRAPPERS.contains(&name) || (name == "Change" && matches!(dom.attr(id, "ChangeType"), Some("InsertedText") | Some("MovedText")))
}

/// New element named `name` in the namespace and prefix of `like`.
pub fn sibling_elem(dom: &mut Dom, like: Id, name: &str, attrs: Vec<(String, String)>) -> Id {
    let n = dom.n(like);
    let qn = match n.name.rsplit_once(':') {
        Some((p, _)) => format!("{p}:{name}"),
        None => name.to_string(),
    };
    let ns = n.ns;
    dom.new_elem(&qn, ns, attrs, Vec::new())
}

/// The story walk. `hidden` is the set of hidden condition ids of the story's document.
pub fn walk(dom: &mut Dom, hidden: &BTreeSet<String>, story: Id) -> Vec<Para> {
    let mut w = Walker { dom, hidden, paras: Vec::new(), cur: None, subs: Vec::new() };
    let mut queue = vec![story];
    let mut i = 0;
    while i < queue.len() {
        w.subs = Vec::new();
        w.block(queue[i]);
        w.flush();
        let subs = std::mem::take(&mut w.subs);
        queue.extend(subs);
        i += 1;
    }
    w.paras
}

struct Walker<'a> {
    dom: &'a mut Dom,
    hidden: &'a BTreeSet<String>,
    paras: Vec<Para>,
    cur: Option<Para>,
    subs: Vec<Id>,
}

impl Walker<'_> {
    fn flush(&mut self) {
        if let Some(p) = self.cur.take() {
            if !p.runs.is_empty() {
                self.paras.push(p);
            }
        }
    }

    fn para(&mut self) -> &mut Para {
        self.cur.get_or_insert_with(Para::default)
    }

    fn block(&mut self, el: Id) {
        wrap_bare(self.dom, el);
        for child in self.dom.kids(el) {
            let name = self.dom.local(child).to_string();
            if name == "ParagraphStyleRange" {
                self.block(child);
                self.flush();
            } else if is_wrapper(self.dom, child) {
                self.block(child);
            } else if name == "CharacterStyleRange" {
                self.csr(child);
            }
        }
    }

    fn csr(&mut self, csr: Id) {
        if hidden_csr(self.dom, csr, self.hidden) {
            let mut r = Run::new(csr);
            r.hidden = true;
            self.para().runs.push(r);
            return;
        }
        let mut run = Run::new(csr);
        for child in self.dom.kids(csr) {
            let name = self.dom.local(child).to_string();
            if name == "Br" {
                run.br = Some(child);
                self.para().runs.push(run);
                self.flush();
                run = Run::new(csr);
                continue;
            }
            if KEEP_ON_CLONE.contains(&name.as_str()) || self.dom.n(child).kind == Kind::Comment {
                continue;
            }
            run.members.push(child);
            self.inline(child, &mut run.items, Wh::Direct);
        }
        if !run.items.is_empty() {
            self.para().runs.push(run);
        }
    }

    fn inline(&mut self, el: Id, items: &mut Vec<Item>, wh: Wh) {
        if self.dom.is_pi(el) {
            items.push(Item::P(Pt::Node(el), wh));
            return;
        }
        let name = self.dom.local(el).to_string();
        if name == "Content" {
            content_items(self.dom, el, items);
            return;
        }
        if is_wrapper(self.dom, el) || (name == "CharacterStyleRange" && !hidden_csr(self.dom, el, self.hidden)) {
            items.push(Item::O(el));
            for child in self.dom.kids(el) {
                let cname = self.dom.local(child).to_string();
                if KEEP_ON_CLONE.contains(&cname.as_str()) || self.dom.n(child).kind == Kind::Comment {
                    continue;
                }
                if cname == "Br" {
                    items.push(Item::P(Pt::Node(child), Wh::Direct));
                    continue;
                }
                self.inline(child, items, Wh::Direct);
            }
            items.push(Item::C(el));
            return;
        }
        items.push(Item::P(Pt::Node(el), Wh::Direct));
        if SUB_STORIES.contains(&name.as_str()) {
            self.subs.push(el);
        } else if name == "Table" {
            let cells: Vec<Id> = self.dom.kids(el).into_iter().filter(|c| self.dom.local(*c) == "Cell").collect();
            self.subs.extend(cells);
        }
    }
}

fn wrap_bare(dom: &mut Dom, block: Id) {
    let mut groups: Vec<Vec<Id>> = Vec::new();
    let mut cur: Vec<Id> = Vec::new();
    for child in dom.kids(block) {
        if dom.is_elem(child) && matches!(dom.local(child), "Content" | "Br") {
            cur.push(child);
            continue;
        }
        if !cur.is_empty() {
            groups.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        groups.push(cur);
    }
    for group in groups {
        let csr = sibling_elem(dom, group[0], "CharacterStyleRange", Vec::new());
        dom.insert_before(group[0], csr);
        for c in group {
            // lxml's append moves an element's tail text with it
            let p = dom.n(c).parent;
            let tail = dom.index_of(p, c).and_then(|i| dom.n(p).children.get(i + 1).copied()).filter(|t| dom.n(*t).kind == Kind::Text);
            dom.append(csr, c);
            if let Some(t) = tail {
                dom.append(csr, t);
            }
        }
    }
}

fn content_items(dom: &Dom, content: Id, items: &mut Vec<Item>) {
    fn text(s: &str, items: &mut Vec<Item>) {
        let mut last = 0;
        for (i, c) in s.char_indices() {
            if PARA_CHARS.contains(&c) {
                if i > last {
                    items.push(Item::T(s[last..i].to_string()));
                }
                items.push(Item::P(Pt::Ch(c.to_string()), Wh::Char));
                last = i + c.len_utf8();
            }
        }
        if last < s.len() {
            items.push(Item::T(s[last..].to_string()));
        }
    }
    for &c in &dom.n(content).children {
        match dom.n(c).kind {
            Kind::Text => text(&dom.text_value(c), items),
            Kind::Pi => items.push(Item::P(Pt::Node(c), Wh::Content)),
            _ => {}
        }
    }
}

// --------------------------------------------------------------------------- tokens
#[derive(Clone, Debug)]
pub enum Part {
    Text(String),
    ROpen(usize),
    RClose(usize),
    Point(usize),
    WOpen(usize),
    WClose(usize),
}

#[derive(Clone, Debug)]
pub enum Entry {
    Run(usize),
    Point { run: usize, obj: Pt, wh: Wh },
    Wrap { run: usize, el: Id },
}

pub struct Tok {
    pub parts: Vec<Part>,
    /// Token number n is `reg[n - 1]`.
    pub reg: Vec<Entry>,
    pub dominant: Option<usize>,
    pub mode_a: bool,
    /// Every point and wrapper number in source order.
    pub order: Vec<usize>,
    pub content_attrib: Vec<(String, String)>,
}

fn signature(dom: &Dom, csr: Id) -> (Vec<(String, String)>, String) {
    let mut attrs = dom.n(csr).attrs.clone();
    attrs.sort();
    let mut props = String::new();
    if let Some(p) = dom.kids(csr).into_iter().find(|c| dom.is_elem(*c) && dom.local(*c) == "Properties") {
        dom.ser(p, &mut props, true);
    }
    (attrs, props)
}

pub fn tokenize(dom: &Dom, para: &Para) -> Tok {
    let live: Vec<usize> = (0..para.runs.len()).filter(|i| !para.runs[*i].hidden && !para.runs[*i].items.is_empty()).collect();
    let parents: Vec<Id> = para.runs.iter().map(|r| dom.n(r.csr).parent).collect();
    let mode_a = para.runs.iter().all(|r| !r.hidden) && parents.iter().all(|p| *p == parents[0]);
    let mut dominant = None;
    let mut untagged = vec![false; para.runs.len()];
    if live.len() == 1 {
        dominant = Some(live[0]);
        untagged[live[0]] = true;
    } else if mode_a && !live.is_empty() {
        let sigs: Vec<_> = live.iter().map(|i| signature(dom, para.runs[*i].csr)).collect();
        let mut weight: Vec<(usize, usize)> = Vec::new(); // (first index with this signature, weight)
        for (k, i) in live.iter().enumerate() {
            let w: usize = para.runs[*i]
                .items
                .iter()
                .map(|it| match it {
                    Item::T(t) => t.chars().count(),
                    _ => 0,
                })
                .sum();
            match weight.iter_mut().find(|(f, _)| sigs[*f] == sigs[k]) {
                Some((_, tot)) => *tot += w,
                None => weight.push((k, w)),
            }
        }
        let best = weight.iter().map(|(_, w)| *w).max().unwrap_or(0);
        let top = (0..live.len()).find(|k| weight.iter().any(|(f, w)| *w == best && sigs[*f] == sigs[*k])).unwrap_or(0);
        let group: Vec<usize> = (0..live.len()).filter(|k| sigs[*k] == sigs[top]).collect();
        dominant = Some(live[group[0]]);
        for k in group {
            untagged[live[k]] = true;
        }
    }
    let mut reg: Vec<Entry> = Vec::new();
    let mut order = Vec::new();
    let mut parts: Vec<Part> = Vec::new();
    for &ri in &live {
        let r = &para.runs[ri];
        let mut rid = None;
        if !untagged[ri] {
            reg.push(Entry::Run(ri));
            rid = Some(reg.len());
            parts.push(Part::ROpen(reg.len()));
        }
        let mut open_ids: Vec<(Id, usize)> = Vec::new();
        for it in &r.items {
            match it {
                Item::T(t) => match parts.last_mut() {
                    Some(Part::Text(prev)) => prev.push_str(t),
                    _ => parts.push(Part::Text(t.clone())),
                },
                Item::P(obj, wh) => {
                    reg.push(Entry::Point { run: ri, obj: obj.clone(), wh: *wh });
                    order.push(reg.len());
                    parts.push(Part::Point(reg.len()));
                }
                Item::O(el) => {
                    reg.push(Entry::Wrap { run: ri, el: *el });
                    order.push(reg.len());
                    open_ids.push((*el, reg.len()));
                    parts.push(Part::WOpen(reg.len()));
                }
                Item::C(el) => {
                    if let Some((_, n)) = open_ids.iter().rev().find(|(e, _)| e == el) {
                        parts.push(Part::WClose(*n));
                    }
                }
            }
        }
        if let Some(n) = rid {
            parts.push(Part::RClose(n));
        }
    }
    let mut content_attrib = Vec::new();
    'find: for &ri in &live {
        for &m in &para.runs[ri].members {
            let mut stack = vec![m];
            while let Some(e) = stack.pop() {
                if dom.is_elem(e) && dom.local(e) == "Content" {
                    content_attrib = dom.n(e).attrs.clone();
                    break 'find;
                }
                if dom.is_elem(e) {
                    stack.extend(dom.kids(e).into_iter().rev());
                }
            }
        }
    }
    Tok { parts, reg, dominant, mode_a, order, content_attrib }
}

pub fn has_text(parts: &[Part]) -> bool {
    parts.iter().any(|p| matches!(p, Part::Text(t) if t.chars().any(|c| !py_space(c))))
}

/// Splits the leading and trailing white space off a token text (LocOS `_split_ws`). Returns (lead, core, trail).
pub fn split_ws(parts: &[Part]) -> (String, Vec<Part>, String) {
    let mut core: Vec<Part> = parts.to_vec();
    let mut lead = String::new();
    let mut trail = String::new();
    if let Some(Part::Text(t)) = core.first_mut() {
        let trimmed = t.trim_start_matches(py_space);
        lead = t[..t.len() - trimmed.len()].to_string();
        *t = trimmed.to_string();
    }
    if let Some(Part::Text(t)) = core.last_mut() {
        let trimmed = t.trim_end_matches(py_space);
        trail = t[trimmed.len()..].to_string();
        *t = trimmed.to_string();
    }
    core.retain(|p| !matches!(p, Part::Text(t) if t.is_empty()));
    (lead, core, trail)
}

/// A shallow clone: same name, attributes and namespace declarations, plus copies of the children that stay on a
/// clone (`Properties`, `XMLAttribute`).
pub fn shallow(dom: &mut Dom, el: Id) -> Id {
    let keeps: Vec<Id> = dom.kids(el).into_iter().filter(|c| dom.is_elem(*c) && KEEP_ON_CLONE.contains(&dom.local(*c))).collect();
    shallow_with(dom, el, &keeps)
}

fn shallow_with(dom: &mut Dom, el: Id, keeps: &[Id]) -> Id {
    let n = dom.n(el);
    let (name, ns, attrs, decls) = (n.name.clone(), n.ns, n.attrs.clone(), n.nsdecls.clone());
    let clone = dom.new_elem(&name, ns, attrs, decls);
    for &child in keeps {
        let c = dom.deep_copy(child);
        dom.append(clone, c);
    }
    clone
}

/// Splits every visible paragraph-level range after each `Br`, so each range holds exactly one paragraph.
pub fn normalize(dom: &mut Dom, paras: &[Para]) {
    let mut seen: Vec<Id> = Vec::new();
    let mut set: HashSet<Id> = HashSet::new();
    for p in paras {
        for r in &p.runs {
            if !r.hidden && set.insert(r.csr) {
                seen.push(r.csr);
            }
        }
    }
    for csr in seen {
        let children = dom.kids(csr);
        let is_br = |d: &Dom, c: Id| d.is_elem(c) && d.local(c) == "Br";
        let keep = |d: &Dom, c: Id| d.is_elem(c) && KEEP_ON_CLONE.contains(&d.local(c));
        let brs: Vec<usize> = (0..children.len()).filter(|i| is_br(dom, children[*i])).collect();
        let tail_items = match brs.last() {
            Some(&l) => children[l + 1..].iter().filter(|c| !keep(dom, **c)).count(),
            None => 0,
        };
        if brs.is_empty() || (brs.len() == 1 && tail_items == 0) {
            continue;
        }
        let mut groups: Vec<Vec<Id>> = vec![Vec::new()];
        for &c in &children {
            if keep(dom, c) {
                continue;
            }
            if let Some(g) = groups.last_mut() {
                g.push(c);
            }
            if is_br(dom, c) {
                groups.push(Vec::new());
            }
        }
        groups.retain(|g| !g.is_empty());
        // one pass: the first group stays in `csr`, every later group moves into a clone placed right after it
        let mut clones: Vec<Id> = Vec::new();
        let keeps: Vec<Id> = children.iter().copied().filter(|c| keep(dom, *c)).collect();
        let mut owner: HashMap<Id, usize> = HashMap::new();
        for (gi, g) in groups.iter().enumerate().skip(1) {
            clones.push(shallow_with(dom, csr, &keeps));
            for c in g {
                owner.insert(*c, gi - 1);
            }
        }
        let old = std::mem::take(&mut dom.nm(csr).children);
        let mut stay = Vec::with_capacity(old.len());
        // a text node is the tail of the node before it (lxml): it moves with that node
        let mut last: Option<usize> = None;
        for c in old {
            let to = if dom.n(c).kind == Kind::Text {
                last
            } else {
                last = owner.get(&c).copied();
                last
            };
            match to.as_ref() {
                Some(&k) => {
                    dom.nm(c).parent = clones[k];
                    dom.nm(clones[k]).children.push(c);
                }
                None => stay.push(c),
            }
        }
        dom.nm(csr).children = stay;
        let p = dom.n(csr).parent;
        if p != NIL {
            // LocOS: `anchor.addnext(clone); clone.tail = anchor.tail`: the range keeps its tail and every clone
            // follows it with a copy of that tail
            let i = dom.index_of(p, csr);
            let tail = i.and_then(|i| dom.n(p).children.get(i + 1).copied()).filter(|t| dom.n(*t).kind == Kind::Text);
            let at = match (i, tail) {
                (Some(i), Some(_)) => i + 2,
                (Some(i), None) => i + 1,
                (None, _) => dom.n(p).children.len(),
            };
            let mut seq: Vec<Id> = Vec::with_capacity(clones.len() * 2);
            for c in &clones {
                dom.nm(*c).parent = p;
                seq.push(*c);
                if let Some(t) = tail {
                    // a copy keeps the source bytes (line ends, references) of the tail
                    let tn = dom.deep_copy(t);
                    dom.nm(tn).parent = p;
                    seq.push(tn);
                }
            }
            dom.nm(p).children.splice(at..at, seq);
            dom.touch(p);
        }
        dom.touch(csr);
    }
}

/// The paragraph style name of a paragraph, for context.
pub fn style_of(dom: &Dom, para: &Para) -> String {
    let Some(r) = para.runs.first() else { return String::new() };
    let mut el = dom.n(r.csr).parent;
    while el != NIL && dom.local(el) != "ParagraphStyleRange" {
        el = dom.n(el).parent;
    }
    if el == NIL {
        return String::new();
    }
    let style = dom.attr(el, "AppliedParagraphStyle").unwrap_or("");
    let after = style.split_once('/').map_or(style, |x| x.1);
    after.replace("$ID/", "").chars().take(80).collect()
}
