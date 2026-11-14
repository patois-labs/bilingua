//! Story markup: the paragraphs of an InDesign story (or an InCopy ICML story, which uses the same markup), their
//! inline codes, and the rewrite of a paragraph from a translation.
//!
//! This module has no zip, no file names and no package logic, so another crate (ICML) can use it on any tree whose
//! `Story` element holds `ParagraphStyleRange > CharacterStyleRange > Content | Br | ...`.
//!
//! ```ignore
//! let mut tree = Tree::parse(xml, &ParseLimits::default())?;
//! let paras = story::collect(&mut tree, &Opts::default());
//! let tok = story::tokenize(&tree, &paras[0]);
//! let unit_inlines = tok.core_inlines();          // what a translator sees
//! // when more than one paragraph of the story may change: story::normalize(&mut tree, &paras), then collect again
//! let ranges = story::rebuild(&mut tree, &paras[0], &tok, &translated);
//! story::stamp_language(&mut tree, &ranges, "$ID/de_DE_2006");
//! ```
//!
//! Inline codes follow the Localization OS token grammar, one numbered tag each:
//! * a paragraph that spans several CharacterStyleRanges keeps its most used range untagged and wraps every other
//!   range as a paired tag (label `run`);
//! * hyperlink sources, inline XML tags and tracked insertions are paired tags (labels `link`, `xml-tag`, `change`);
//! * tabs, forced line breaks, `<?ACE n?>` special characters, index markers, hyperlink destinations, text
//!   variables, notes, anchored frames, tables, footnotes, tracked deletions and hidden text are standalone tags
//!   that the rebuild puts back.

use crate::xml::{Id, Tree};
use bilingua_model::{CodeData, Inline, TagKind};
use std::collections::{BTreeMap, HashMap, HashSet};

const WRAPPERS: [&str; 2] = ["HyperlinkTextSource", "XMLElement"];
const SUB_STORIES: [&str; 3] = ["Footnote", "EndnoteRange", "ParagraphStyleRange"];
const KEEP_ON_CLONE: [&str; 2] = ["Properties", "XMLAttribute"];
const PARA_STOPS: [&str; 5] = ["Story", "Cell", "Footnote", "EndnoteRange", "Note"];
const PROTECTED: [char; 3] = ['\t', '\u{2028}', '\u{2029}'];

/// What the walk needs to know about the package.
#[derive(Debug, Clone, Default)]
pub struct Opts {
    /// `Self` ids of the conditions whose `Visible` is false (designmap.xml).
    pub hidden_conditions: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Where {
    Direct,
    Char,
    Content,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Point {
    El(Id),
    Ch(char),
}

#[derive(Debug, Clone)]
enum Item {
    T(String),
    P(Point, Where),
    O(Id),
    C(Id),
}

#[derive(Debug, Clone)]
struct Run {
    csr: Id,
    hidden: bool,
    items: Vec<Item>,
    members: Vec<Id>,
    br: Option<Id>,
}

impl Run {
    fn new(csr: Id) -> Run {
        Run { csr, hidden: false, items: vec![], members: vec![], br: None }
    }
}

/// One paragraph of a story: the ranges (or pieces of ranges) it is made of.
#[derive(Debug, Clone, Default)]
pub struct Para {
    runs: Vec<Run>,
}

fn unquote(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_range(t: &Tree, id: Id) -> bool {
    t.local(id) == "CharacterStyleRange"
}

fn is_wrapper(t: &Tree, id: Id) -> bool {
    let n = t.local(id);
    WRAPPERS.contains(&n) || (n == "Change" && matches!(t.attr(id, "ChangeType"), Some("InsertedText" | "MovedText")))
}

fn hidden_csr(t: &Tree, csr: Id, hidden: &HashSet<String>) -> bool {
    if hidden.is_empty() {
        return false;
    }
    t.attr(csr, "AppliedConditions").unwrap_or("").split_whitespace().any(|c| hidden.contains(&unquote(c)))
}

fn sibling_name(t: &Tree, like: Id, local: &str) -> String {
    match t.name(like).rsplit_once(':') {
        Some((p, _)) => format!("{p}:{local}"),
        None => local.to_string(),
    }
}

struct Walker<'a> {
    t: &'a mut Tree,
    hidden: &'a HashSet<String>,
    paras: Vec<Para>,
    cur: Option<Para>,
    subs: Vec<Id>,
}

/// The paragraphs of a story in unit order: the body first, then table cells, footnotes and the like (breadth
/// first). Bare `Content` and `Br` directly under a block are wrapped in a range in the tree (in memory).
pub fn collect(tree: &mut Tree, opts: &Opts) -> Vec<Para> {
    let root = tree.root();
    let story = tree.iter(root).into_iter().find(|i| tree.is_element(*i) && tree.name(*i) == "Story");
    let Some(story) = story else { return vec![] };
    let mut w = Walker { t: tree, hidden: &opts.hidden_conditions, paras: vec![], cur: None, subs: vec![] };
    let mut queue = vec![story];
    let mut i = 0;
    while i < queue.len() {
        w.subs = vec![];
        w.block(queue[i]);
        w.flush();
        let subs = std::mem::take(&mut w.subs);
        queue.extend(subs);
        i += 1;
    }
    w.paras
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
        wrap_bare(self.t, el);
        for child in self.t.children(el).to_vec() {
            let name = self.t.local(child).to_string();
            if name == "ParagraphStyleRange" {
                self.block(child);
                self.flush();
            } else if is_wrapper(self.t, child) {
                self.block(child);
            } else if name == "CharacterStyleRange" {
                self.csr(child);
            }
        }
    }

    fn csr(&mut self, csr: Id) {
        if hidden_csr(self.t, csr, self.hidden) {
            let mut r = Run::new(csr);
            r.hidden = true;
            self.para().runs.push(r);
            return;
        }
        let mut run = Run::new(csr);
        for child in self.t.children(csr).to_vec() {
            let name = self.t.local(child).to_string();
            if name == "Br" {
                run.br = Some(child);
                let done = std::mem::replace(&mut run, Run::new(csr));
                self.para().runs.push(done);
                self.flush();
                continue;
            }
            if KEEP_ON_CLONE.contains(&name.as_str()) {
                continue;
            }
            if !self.t.is_pi(child) && !self.t.is_element(child) {
                continue; // comments stay where they are
            }
            run.members.push(child);
            let mut items = std::mem::take(&mut run.items);
            self.inline(child, &mut items);
            run.items = items;
        }
        if !run.items.is_empty() {
            self.para().runs.push(run);
        }
    }

    fn inline(&mut self, el: Id, items: &mut Vec<Item>) {
        if self.t.is_pi(el) {
            items.push(Item::P(Point::El(el), Where::Direct));
            return;
        }
        let name = self.t.local(el).to_string();
        if name == "Content" {
            content_items(self.t, el, items);
            return;
        }
        if is_wrapper(self.t, el) || (name == "CharacterStyleRange" && !hidden_csr(self.t, el, self.hidden)) {
            items.push(Item::O(el));
            for child in self.t.children(el).to_vec() {
                let cname = self.t.local(child).to_string();
                if KEEP_ON_CLONE.contains(&cname.as_str()) || !(self.t.is_pi(child) || self.t.is_element(child)) {
                    continue;
                }
                if cname == "Br" {
                    items.push(Item::P(Point::El(child), Where::Direct));
                    continue;
                }
                self.inline(child, items);
            }
            items.push(Item::C(el));
            return;
        }
        items.push(Item::P(Point::El(el), Where::Direct));
        if SUB_STORIES.contains(&name.as_str()) {
            self.subs.push(el);
        } else if name == "Table" {
            let cells: Vec<Id> = self.t.children(el).iter().copied().filter(|c| self.t.local(*c) == "Cell").collect();
            self.subs.extend(cells);
        }
    }
}

fn wrap_bare(t: &mut Tree, block: Id) {
    let mut groups: Vec<Vec<Id>> = vec![];
    let mut cur: Vec<Id> = vec![];
    for c in t.children(block).to_vec() {
        if t.is_element(c) && matches!(t.local(c), "Content" | "Br") {
            cur.push(c);
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
        let name = sibling_name(t, group[0], "CharacterStyleRange");
        let csr = t.new_element(&name, &[]);
        let at = t.index_of(block, group[0]).unwrap_or(0);
        t.insert(block, at, csr);
        for c in group {
            t.append(csr, c);
        }
    }
}

fn content_items(t: &Tree, content: Id, items: &mut Vec<Item>) {
    fn text(s: &str, items: &mut Vec<Item>) {
        let mut buf = String::new();
        for c in s.chars() {
            if PROTECTED.contains(&c) {
                if !buf.is_empty() {
                    items.push(Item::T(std::mem::take(&mut buf)));
                }
                items.push(Item::P(Point::Ch(c), Where::Char));
            } else {
                buf.push(c);
            }
        }
        if !buf.is_empty() {
            items.push(Item::T(buf));
        }
    }
    text(&t.text(content), items);
    for child in t.children(content) {
        if t.is_pi(*child) {
            items.push(Item::P(Point::El(*child), Where::Content));
        }
        text(&t.tail(*child), items);
    }
}

/// One piece of a paragraph's text: characters, or a numbered inline code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part {
    Text(String),
    Open(usize),
    Close(usize),
    Point(usize),
}

fn push_part_text(parts: &mut Vec<Part>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(Part::Text(t)) = parts.last_mut() {
        t.push_str(s);
    } else {
        parts.push(Part::Text(s.to_string()));
    }
}

#[derive(Debug, Clone)]
enum Reg {
    Run(usize),
    Point(usize, Point, Where),
    Wrap(usize, Id),
}

impl Reg {
    fn run(&self) -> usize {
        match self {
            Reg::Run(r) | Reg::Point(r, ..) | Reg::Wrap(r, _) => *r,
        }
    }
}

/// A paragraph with its inline codes numbered.
#[derive(Debug, Clone)]
pub struct Tok {
    parts: Vec<Part>,
    reg: BTreeMap<usize, Reg>,
    dominant: Option<usize>,
    mode_a: bool,
    order: Vec<usize>,
    content_attrib: Vec<(String, String)>,
    codes: BTreeMap<String, CodeData>,
}

fn signature(t: &Tree, csr: Id) -> String {
    let mut attrs: Vec<&(String, String)> = t.attrs(csr).iter().collect();
    attrs.sort();
    let mut s = String::new();
    for (k, v) in attrs {
        s.push_str(k);
        s.push('\u{1}');
        s.push_str(v);
        s.push('\u{2}');
    }
    if let Some(p) = t.children(csr).iter().find(|c| t.local(**c) == "Properties") {
        s.push('\u{3}');
        s.push_str(&t.serialize_node(*p));
    }
    s
}

fn label_of(local: &str) -> &'static str {
    match local {
        "HyperlinkTextSource" => "link",
        "XMLElement" => "xml-tag",
        "Change" => "change",
        "CharacterStyleRange" => "range",
        _ => "inline",
    }
}

/// Numbers the inline codes of a paragraph and lays out its text.
pub fn tokenize(t: &Tree, para: &Para) -> Tok {
    let live: Vec<usize> = (0..para.runs.len()).filter(|i| !para.runs[*i].hidden && !para.runs[*i].items.is_empty()).collect();
    let parents: Vec<Option<Id>> = para.runs.iter().map(|r| t.parent(r.csr)).collect();
    let mode_a = para.runs.iter().all(|r| !r.hidden) && parents.iter().all(|p| *p == parents[0]);
    let mut dominant = None;
    let mut untagged: HashSet<usize> = HashSet::new();
    if live.len() == 1 {
        dominant = Some(live[0]);
        untagged.insert(live[0]);
    } else if mode_a && !live.is_empty() {
        let mut weight: HashMap<String, usize> = HashMap::new();
        let sigs: Vec<String> = live.iter().map(|i| signature(t, para.runs[*i].csr)).collect();
        for (k, i) in live.iter().enumerate() {
            let w: usize = para.runs[*i].items.iter().map(|it| if let Item::T(s) = it { s.chars().count() } else { 0 }).sum();
            *weight.entry(sigs[k].clone()).or_insert(0) += w;
        }
        let best = weight.values().copied().max().unwrap_or(0);
        let top = sigs.iter().find(|s| weight[*s] == best).cloned().unwrap_or_default();
        for (k, i) in live.iter().enumerate() {
            if sigs[k] == top {
                if dominant.is_none() {
                    dominant = Some(*i);
                }
                untagged.insert(*i);
            }
        }
    }
    let mut reg = BTreeMap::new();
    let mut order = vec![];
    let mut parts: Vec<Part> = vec![];
    let mut codes = BTreeMap::new();
    let mut n = 0usize;
    for &ri in &live {
        let r = &para.runs[ri];
        let mut rid = None;
        if !untagged.contains(&ri) {
            n += 1;
            rid = Some(n);
            reg.insert(n, Reg::Run(ri));
            codes.insert(
                n.to_string(),
                CodeData::new().with_open(t.start_tag(r.csr, false)).with_close(format!("</{}>", t.name(r.csr))).with_label("run".to_string()),
            );
            parts.push(Part::Open(n));
        }
        let mut open_ids: HashMap<Id, usize> = HashMap::new();
        for it in &r.items {
            match it {
                Item::T(s) => push_part_text(&mut parts, s),
                Item::P(obj, wh) => {
                    n += 1;
                    reg.insert(n, Reg::Point(ri, *obj, *wh));
                    order.push(n);
                    codes.insert(n.to_string(), point_code(t, *obj));
                    parts.push(Part::Point(n));
                }
                Item::O(el) => {
                    n += 1;
                    reg.insert(n, Reg::Wrap(ri, *el));
                    order.push(n);
                    open_ids.insert(*el, n);
                    codes.insert(
                        n.to_string(),
                        CodeData::new().with_open(t.start_tag(*el, false)).with_close(format!("</{}>", t.name(*el))).with_label(label_of(t.local(*el)).to_string()),
                    );
                    parts.push(Part::Open(n));
                }
                Item::C(el) => {
                    if let Some(k) = open_ids.get(el) {
                        parts.push(Part::Close(*k));
                    }
                }
            }
        }
        if let Some(k) = rid {
            parts.push(Part::Close(k));
        }
    }
    let mut content_attrib = vec![];
    'find: for &ri in &live {
        for m in &para.runs[ri].members {
            for el in t.iter(*m) {
                if t.is_element(el) && t.local(el) == "Content" {
                    content_attrib = t.attrs(el).to_vec();
                    break 'find;
                }
            }
        }
    }
    Tok { parts, reg, dominant, mode_a, order, content_attrib, codes }
}

fn point_code(t: &Tree, p: Point) -> CodeData {
    let (display, label) = match p {
        Point::Ch('\t') => ("\t".to_string(), "tab".to_string()),
        Point::Ch('\u{2028}') => ("\u{2028}".to_string(), "line-break".to_string()),
        Point::Ch(c) => (c.to_string(), "paragraph-separator".to_string()),
        Point::El(e) => match t.kind(e) {
            crate::xml::Kind::Pi => (t.start_tag(e, false), "special-char".to_string()),
            _ => {
                let l = t.local(e);
                let label = if l == "Br" { "br".to_string() } else { l.to_ascii_lowercase() };
                (t.start_tag(e, true), label)
            }
        },
    };
    CodeData::new().with_standalone(display).with_label(label)
}

/// True when the characters of a part list contain anything but white space.
fn has_text(parts: &[Part]) -> bool {
    parts.iter().any(|p| matches!(p, Part::Text(s) if !s.trim().is_empty()))
}

impl Tok {
    /// The paragraph holds text a person translates (not only white space and codes).
    pub fn has_text(&self) -> bool {
        has_text(&self.parts)
    }

    /// The white space around the text, kept by the rewrite: `(leading, trailing)`.
    pub fn edges(&self) -> (String, String) {
        let lead = match self.parts.first() {
            Some(Part::Text(s)) => s[..s.len() - s.trim_start().len()].to_string(),
            _ => String::new(),
        };
        let trail = match self.parts.last() {
            Some(Part::Text(s)) => s[s.trim_end().len()..].to_string(),
            _ => String::new(),
        };
        (lead, trail)
    }

    /// The text with its edge white space cut off, as inline pieces with the original markup in [`Tok::codes`].
    pub fn core_inlines(&self) -> Vec<Inline> {
        let mut parts = self.parts.clone();
        if let Some(Part::Text(s)) = parts.first_mut() {
            *s = s.trim_start().to_string();
        }
        if let Some(Part::Text(s)) = parts.last_mut() {
            *s = s.trim_end().to_string();
        }
        let mut out = vec![];
        for p in parts {
            match p {
                Part::Text(s) if s.is_empty() => {}
                Part::Text(s) => out.push(Inline::Text(s)),
                Part::Open(n) => out.push(Inline::tag(n.to_string(), TagKind::Open, self.disp(n, true))),
                Part::Close(n) => out.push(Inline::tag(n.to_string(), TagKind::Close, self.disp(n, false))),
                Part::Point(n) => out.push(Inline::tag(n.to_string(), TagKind::Standalone, self.disp(n, true))),
            }
        }
        out
    }

    fn disp(&self, n: usize, open: bool) -> String {
        let c = self.codes.get(&n.to_string());
        let s = c.and_then(|c| if open { c.open.clone().or(c.standalone.clone()) } else { c.close.clone() });
        s.unwrap_or_default()
    }

    /// Original markup of every inline code, keyed by tag id.
    pub fn codes(&self) -> &BTreeMap<String, CodeData> {
        &self.codes
    }
}

/// The paragraph style name, for context: `Heading 1`.
pub fn style(t: &Tree, para: &Para) -> String {
    let Some(first) = para.runs.first() else { return String::new() };
    let mut el = t.parent(first.csr);
    while let Some(e) = el {
        if t.local(e) == "ParagraphStyleRange" {
            let s = t.attr(e, "AppliedParagraphStyle").unwrap_or("");
            let s = s.split_once('/').map(|x| x.1).unwrap_or(s).replace("$ID/", "");
            return s.chars().take(80).collect();
        }
        el = t.parent(e);
    }
    String::new()
}

// ----------------------------------------------------------------------------------------------- rebuild

/// Splits every visible paragraph-level range after each `Br`, so each range holds the text of exactly one
/// paragraph. Same paragraphs, same order. Call before collecting again and rebuilding.
pub fn normalize(t: &mut Tree, paras: &[Para]) {
    let mut seen: Vec<Id> = vec![];
    let mut set = HashSet::new();
    for p in paras {
        for r in &p.runs {
            if !r.hidden && set.insert(r.csr) {
                seen.push(r.csr);
            }
        }
    }
    for csr in seen {
        let children = t.children(csr).to_vec();
        let is_br = |t: &Tree, c: Id| t.is_element(c) && t.local(c) == "Br";
        let brs: Vec<usize> = (0..children.len()).filter(|i| is_br(t, children[*i])).collect();
        let keep = |t: &Tree, c: Id| t.is_element(c) && KEEP_ON_CLONE.contains(&t.local(c));
        let tail_items = match brs.last() {
            Some(l) => children[l + 1..].iter().filter(|c| !keep(t, **c)).count(),
            None => 0,
        };
        if brs.is_empty() || (brs.len() == 1 && tail_items == 0) {
            continue;
        }
        let mut groups: Vec<Vec<Id>> = vec![vec![]];
        for c in &children {
            if keep(t, *c) {
                continue;
            }
            groups.last_mut().expect("one group").push(*c);
            if is_br(t, *c) {
                groups.push(vec![]);
            }
        }
        groups.retain(|g| !g.is_empty());
        let mut anchor = csr;
        for g in groups.iter().skip(1) {
            let clone = shallow(t, csr);
            for c in g {
                t.append(clone, *c);
            }
            t.add_next(anchor, clone);
            let tail = t.tail_raw(anchor).to_string();
            t.set_tail_raw(clone, &tail);
            anchor = clone;
        }
    }
}

/// LocOS `_shallow`: a copy of the element with only its `Properties` and `XMLAttribute` children. Those are
/// `deepcopy`'d, which keeps each one's tail, so the indentation between `</Properties>` and the content that follows
/// stays as LocOS writes it (W4 idml-2); the clone itself has no tail.
fn shallow(t: &mut Tree, el: Id) -> Id {
    let name = t.name(el).to_string();
    let attrs = t.attrs(el).to_vec();
    let clone = t.new_element(&name, &attrs);
    for c in t.children(el).to_vec() {
        if t.is_element(c) && KEEP_ON_CLONE.contains(&t.local(c)) {
            let tail = t.tail_raw(c).to_string();
            let cp = t.deep_copy(c);
            t.set_tail_raw(cp, &tail);
            t.append(clone, cp);
        }
    }
    clone
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PK {
    T,
    P,
    W,
}

struct Piece {
    run: Option<usize>,
    chain: Vec<usize>,
    kind: PK,
    text: String,
    key: usize,
    /// Inside a character-style run of the translation (not loose text before, between or after the runs).
    inside: bool,
}

enum Frame {
    Run(usize),
    Wrap,
    Phantom,
}

fn pieces(parts: &[Part], tok: &Tok) -> Vec<Piece> {
    let mut out: Vec<Piece> = vec![];
    let mut used: HashSet<usize> = HashSet::new();
    let mut stack: Vec<(usize, Frame)> = vec![];
    let first_run = tok.reg.values().find_map(|r| if let Reg::Run(i) = r { Some(*i) } else { None });
    let in_run = |stack: &[(usize, Frame)]| stack.iter().any(|(_, f)| matches!(f, Frame::Run(_)));
    let ctx = |stack: &[(usize, Frame)]| -> (Option<usize>, Vec<usize>) {
        let mut run = tok.dominant;
        let mut chain = vec![];
        for (k, f) in stack {
            match f {
                Frame::Run(r) => run = Some(*r),
                Frame::Wrap => chain.push(*k),
                Frame::Phantom => {}
            }
        }
        if run.is_none() {
            run = first_run;
        }
        (run, chain)
    };
    for part in parts {
        match part {
            Part::Text(s) => {
                if !s.is_empty() {
                    let (run, chain) = ctx(&stack);
                    let inside = in_run(&stack);
                    out.push(Piece { run, chain, kind: PK::T, text: s.clone(), key: 0, inside });
                }
            }
            Part::Close(k) => {
                if stack.last().is_some_and(|(sk, _)| sk == k) {
                    stack.pop();
                }
            }
            Part::Open(k) | Part::Point(k) => {
                let is_point = matches!(part, Part::Point(_));
                let entry = tok.reg.get(k);
                let bad = match entry {
                    None => true,
                    Some(e) => used.contains(k) || matches!(e, Reg::Point(..)) != is_point,
                };
                if bad {
                    if !is_point {
                        stack.push((*k, Frame::Phantom));
                    }
                    continue;
                }
                used.insert(*k);
                match entry.expect("checked") {
                    Reg::Point(..) => {
                        let (run, chain) = ctx(&stack);
                        let inside = in_run(&stack);
                        out.push(Piece { run, chain, kind: PK::P, text: String::new(), key: *k, inside });
                    }
                    Reg::Run(r) => stack.push((*k, Frame::Run(*r))),
                    Reg::Wrap(..) => {
                        stack.push((*k, Frame::Wrap));
                        let (run, chain) = ctx(&stack);
                        let inside = in_run(&stack);
                        out.push(Piece { run, chain, kind: PK::W, text: String::new(), key: *k, inside });
                    }
                }
            }
        }
    }
    // Nothing is lost: a code the translation dropped goes to the end of its own run.
    for k in &tok.order {
        if !used.contains(k) {
            let e = &tok.reg[k];
            let kind = if matches!(e, Reg::Point(..)) { PK::P } else { PK::W };
            out.push(Piece { run: Some(e.run()), chain: vec![], kind, text: String::new(), key: *k, inside: true });
        }
    }
    out
}

struct Filler {
    root: Id,
    chain: Vec<(usize, Id)>,
}

impl Filler {
    fn target(&self) -> Id {
        self.chain.last().map(|c| c.1).unwrap_or(self.root)
    }

    fn sync(&mut self, t: &mut Tree, tok: &Tok, opened: &mut HashSet<usize>, chain: &[usize]) {
        let mut common = 0;
        while common < self.chain.len().min(chain.len()) && self.chain[common].0 == chain[common] {
            common += 1;
        }
        self.chain.truncate(common);
        for key in &chain[common..] {
            if opened.contains(key) {
                break;
            }
            let Some(Reg::Wrap(_, el)) = tok.reg.get(key) else { break };
            if is_range(t, *el) && is_range(t, self.target()) {
                continue;
            }
            opened.insert(*key);
            let clone = shallow(t, *el);
            let tgt = self.target();
            t.append(tgt, clone);
            self.chain.push((*key, clone));
        }
    }

    fn content(&mut self, t: &mut Tree, tok: &Tok) -> Id {
        let tgt = self.target();
        if let Some(last) = t.children(tgt).last().copied() {
            if t.is_element(last) && t.local(last) == "Content" {
                return last;
            }
        }
        let name = sibling_name(t, self.root, "Content");
        let c = t.new_element(&name, &tok.content_attrib);
        t.append(tgt, c);
        c
    }

    fn text(&mut self, t: &mut Tree, tok: &Tok, opened: &mut HashSet<usize>, chain: &[usize], s: &str) {
        self.sync(t, tok, opened, chain);
        let c = self.content(t, tok);
        match t.children(c).last().copied() {
            Some(last) => t.push_tail(last, s),
            None => t.push_text(c, s),
        }
    }

    fn point(&mut self, t: &mut Tree, tok: &Tok, opened: &mut HashSet<usize>, chain: &[usize], key: usize) {
        self.sync(t, tok, opened, chain);
        let Some(Reg::Point(_, obj, wh)) = tok.reg.get(&key) else { return };
        match (obj, wh) {
            (Point::Ch(c), _) => {
                let s = c.to_string();
                self.text(t, tok, opened, chain, &s);
            }
            (Point::El(el), w) => {
                t.clear_tail(*el);
                if *w == Where::Content {
                    let c = self.content(t, tok);
                    t.append(c, *el);
                } else {
                    let tgt = self.target();
                    t.append(tgt, *el);
                }
            }
        }
    }

    fn wrap(&mut self, t: &mut Tree, tok: &Tok, opened: &mut HashSet<usize>, chain: &[usize], key: usize) {
        match chain.iter().position(|k| *k == key) {
            None => {
                self.sync(t, tok, opened, chain);
                if let Some(Reg::Wrap(_, el)) = tok.reg.get(&key) {
                    if !opened.contains(&key) && !is_range(t, *el) {
                        opened.insert(key);
                        let c = shallow(t, *el);
                        let tgt = self.target();
                        t.append(tgt, c);
                    }
                }
            }
            Some(p) => self.sync(t, tok, opened, &chain[..=p]),
        }
    }

    fn apply(&mut self, t: &mut Tree, tok: &Tok, opened: &mut HashSet<usize>, p: &Piece) {
        match p.kind {
            PK::T => self.text(t, tok, opened, &p.chain, &p.text),
            PK::P => self.point(t, tok, opened, &p.chain, p.key),
            PK::W => self.wrap(t, tok, opened, &p.chain, p.key),
        }
    }
}

/// Empty clones of the wrappers a badly nested translation never materialized (a hyperlink source or XML element
/// carries a `Self` id other parts point at, so it must survive; a nested range has no id and no text left).
fn unopened(t: &mut Tree, tok: &Tok, opened: &mut HashSet<usize>, run: Option<usize>) -> Vec<Id> {
    let mut out = vec![];
    for key in &tok.order {
        let Some(Reg::Wrap(r, el)) = tok.reg.get(key) else { continue };
        if opened.contains(key) || run.is_some_and(|x| x != *r) {
            continue;
        }
        opened.insert(*key);
        if !is_range(t, *el) {
            out.push(shallow(t, *el));
        }
    }
    out
}

/// Turns a translation into parts: invalid XML characters out, newlines to the forced line break, and the
/// paragraph's own edge white space back.
fn target_parts(target: &[Inline], lead: &str, trail: &str) -> Vec<Part> {
    let mut parts: Vec<Part> = vec![];
    for i in target {
        match i {
            Inline::Text(s) => {
                let s = clean_target(s);
                push_part_text(&mut parts, &s);
            }
            Inline::Tag { id, kind, .. } => {
                let Ok(n) = id.parse::<usize>() else { continue };
                parts.push(match kind {
                    TagKind::Open => Part::Open(n),
                    TagKind::Close => Part::Close(n),
                    TagKind::Standalone => Part::Point(n),
                });
            }
        }
    }
    if !lead.is_empty() {
        let ok = matches!(parts.first(), Some(Part::Text(s)) if s.starts_with(lead));
        if !ok {
            if let Some(Part::Text(s)) = parts.first_mut() {
                *s = s.trim_start().to_string();
            }
            parts.insert(0, Part::Text(lead.to_string()));
            merge_adjacent(&mut parts);
        }
    }
    if !trail.is_empty() {
        let ok = matches!(parts.last(), Some(Part::Text(s)) if s.ends_with(trail));
        if !ok {
            if let Some(Part::Text(s)) = parts.last_mut() {
                *s = s.trim_end().to_string();
            }
            parts.push(Part::Text(trail.to_string()));
            merge_adjacent(&mut parts);
        }
    }
    parts
}

fn merge_adjacent(parts: &mut Vec<Part>) {
    let mut out: Vec<Part> = Vec::with_capacity(parts.len());
    for p in parts.drain(..) {
        match (out.last_mut(), p) {
            (Some(Part::Text(a)), Part::Text(b)) => a.push_str(&b),
            (_, Part::Text(b)) if b.is_empty() => {}
            (_, p) => out.push(p),
        }
    }
    *parts = out;
}

fn clean_target(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\u{b}' | '\u{c}' | '\n' => out.push('\u{2028}'),
            '\r' => {
                if it.peek() == Some(&'\n') {
                    it.next();
                }
                out.push('\u{2028}');
            }
            c if crate::xml::is_xml_char(c) => out.push(c),
            _ => {}
        }
    }
    out
}

/// Rewrites one paragraph from a translation (the text of the unit, edge white space not included). Returns the
/// ranges that now hold the translated text, for [`stamp_language`] and [`stamp_rtl`]. Inline codes are moved,
/// never copied or dropped: a code the translation lost is appended to the end of its paragraph.
pub fn rebuild(t: &mut Tree, para: &Para, tok: &Tok, target: &[Inline]) -> Vec<Id> {
    let (lead, trail) = tok.edges();
    let parts = target_parts(target, &lead, &trail);
    let pcs = pieces(&parts, tok);
    let br = para.runs.iter().rev().find_map(|r| r.br);
    if para.runs.is_empty() {
        return vec![]; // `collect` never returns an empty paragraph; `Para::default()` is one
    }
    if tok.mode_a {
        let parent = t.parent(para.runs[0].csr).unwrap_or(t.root());
        let mut old: Vec<Id> = vec![];
        for r in &para.runs {
            if !old.contains(&r.csr) {
                old.push(r.csr);
            }
        }
        let index = t.index_of(parent, old[0]).unwrap_or(0);
        let base = tok.dominant.or_else(|| (0..para.runs.len()).find(|i| !para.runs[*i].items.is_empty())).unwrap_or(0);
        let mut new_csrs: Vec<Id> = vec![];
        let mut filler: Option<Filler> = None;
        let mut cur_run = usize::MAX;
        let mut opened = HashSet::new();
        for p in &pcs {
            let run = p.run.unwrap_or(base);
            if filler.is_none() || run != cur_run {
                let csr = shallow(t, para.runs[run].csr);
                new_csrs.push(csr);
                filler = Some(Filler { root: csr, chain: vec![] });
                cur_run = run;
            }
            if let Some(f) = filler.as_mut() {
                f.apply(t, tok, &mut opened, p);
            }
        }
        if new_csrs.is_empty() {
            new_csrs.push(shallow(t, para.runs[base].csr));
        }
        let last = *new_csrs.last().expect("one range");
        for el in unopened(t, tok, &mut opened, None) {
            t.append(last, el);
        }
        if let Some(b) = br {
            t.clear_tail(b);
            t.append(last, b);
        }
        let tail = t.tail_raw(*old.last().expect("one range")).to_string();
        for c in &old {
            t.detach(*c);
        }
        for (k, c) in new_csrs.iter().enumerate() {
            t.insert(parent, index + k, *c);
            t.set_tail_raw(*c, &tail);
        }
        return new_csrs;
    }
    // Mode B: rebuild each visible run in place, in its own range.
    let live: Vec<usize> = (0..para.runs.len()).filter(|i| !para.runs[*i].hidden && !para.runs[*i].items.is_empty()).collect();
    let Some(&first) = live.first() else { return vec![] };
    let mut by_run: HashMap<usize, Vec<&Piece>> = live.iter().map(|i| (*i, vec![])).collect();
    let mut opened = HashSet::new();
    // loose text after the last run of the translation stays after it (it would otherwise join the dominant run
    // and jump in front of the other runs)
    let mut last_in: Option<usize> = None;
    for p in &pcs {
        let mut target = match p.run {
            Some(r) if by_run.contains_key(&r) => r,
            _ => first,
        };
        if p.inside {
            last_in = Some(target);
        } else if let (PK::T, Some(l)) = (&p.kind, last_in) {
            target = l;
        }
        by_run.get_mut(&target).expect("live run").push(p);
    }
    for &ri in &live {
        let r = &para.runs[ri];
        for m in &r.members {
            if t.parent(*m) == Some(r.csr) {
                t.detach(*m);
            }
        }
        if let Some(b) = r.br {
            if t.parent(b) == Some(r.csr) {
                t.detach(b);
            }
        }
        let mut f = Filler { root: r.csr, chain: vec![] };
        for p in &by_run[&ri] {
            f.apply(t, tok, &mut opened, p);
        }
    }
    for &ri in &live {
        let r = &para.runs[ri];
        for el in unopened(t, tok, &mut opened, Some(ri)) {
            t.append(r.csr, el);
        }
        if let Some(b) = r.br {
            t.clear_tail(b);
            t.append(r.csr, b);
        }
    }
    live.iter().map(|i| para.runs[*i].csr).collect()
}

fn wrapped_only(t: &Tree, rng: Id, top: Id) -> bool {
    let mut el = t.parent(rng);
    while let Some(e) = el {
        if e == top {
            break;
        }
        if !(is_wrapper(t, e) || is_range(t, e)) {
            return false;
        }
        el = t.parent(e);
    }
    true
}

/// The ranges nested under `el` that the stamping walks reach (directly, or through wrappers only).
fn nested_ranges(t: &Tree, el: Id) -> Vec<Id> {
    let mut out = vec![];
    for child in t.children(el) {
        if !t.is_element(*child) {
            continue;
        }
        if is_range(t, *child) {
            out.push(*child);
        } else if is_wrapper(t, *child) {
            out.extend(t.iter(*child).into_iter().filter(|c| t.is_element(*c) && is_range(t, *c) && wrapped_only(t, *c, *child)));
        }
    }
    out
}

/// `AppliedLanguage` on every rebuilt range, nested ranges (inside a hyperlink or XML tag) included. Never
/// descends into a point element (footnote, table, note, hidden text, anchored frame).
pub fn stamp_language(t: &mut Tree, ranges: &[Id], language: &str) {
    for el in stamp_walk(t, ranges) {
        t.set_attr(el, "AppliedLanguage", language);
    }
}

/// The rebuilt ranges and the ranges nested under them, each once. A range under alternating wrappers and ranges is
/// reachable from every range above it, so a walk without the seen set grows exponentially with the nesting.
fn stamp_walk(t: &Tree, ranges: &[Id]) -> Vec<Id> {
    let mut seen: HashSet<Id> = HashSet::new();
    let mut out = vec![];
    let mut stack = ranges.to_vec();
    while let Some(el) = stack.pop() {
        if !seen.insert(el) {
            continue;
        }
        out.push(el);
        stack.extend(nested_ranges(t, el).into_iter().filter(|r| !seen.contains(r)));
    }
    out
}

const RTL_COMPOSER: &str = "HL Composer Optyca";
const RTL_SINGLE: &str = "HL Single Optyca";

/// Right-to-left target: the paragraph range holding each rebuilt range gets `ParagraphDirection` right to left and
/// a World-Ready composer; an explicit left-to-right `CharacterDirection` becomes `DefaultDirection`.
pub fn stamp_rtl(t: &mut Tree, ranges: &[Id]) {
    let mut paras: Vec<Id> = vec![];
    for el in stamp_walk(t, ranges) {
        if t.attr(el, "CharacterDirection") == Some("LeftToRightDirection") {
            t.set_attr(el, "CharacterDirection", "DefaultDirection");
        }
        let mut up = t.parent(el);
        while let Some(u) = up {
            if PARA_STOPS.contains(&t.local(u)) {
                break;
            }
            if t.local(u) == "ParagraphStyleRange" {
                if !paras.contains(&u) {
                    paras.push(u);
                }
                break;
            }
            up = t.parent(u);
        }
    }
    for psr in paras {
        t.set_attr(psr, "ParagraphDirection", "RightToLeftDirection");
        let composer = t.attr(psr, "Composer").unwrap_or("").to_string();
        if !composer.contains("Optyca") {
            t.set_attr(psr, "Composer", if composer.starts_with("HL Single") { RTL_SINGLE } else { RTL_COMPOSER });
        }
    }
}
