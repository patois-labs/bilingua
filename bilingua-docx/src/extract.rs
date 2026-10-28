//! Paragraph extraction. Walks one XML part and produces a template string (the part with every unit's text
//! replaced by a marker) plus the units.
//!
//! Markers: `U+E000 id U+E001` stands for the inner content of unit `id` (everything after `w:pPr`), and
//! `U+E003 id:k U+E001` for a mirror (the AlternateContent fallback copy of the same text box paragraph).
//! Markers also appear inside the stored XML of a tag when a drawing holds a text box with paragraphs of its
//! own; the writer resolves them recursively.

use crate::xml::*;
use bilingua_model::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

pub const M_UNIT: char = '\u{E000}';
pub const M_END: char = '\u{E001}';
pub const M_MIRROR: char = '\u{E003}';

/// Reserved code names in `Unit::codes` (they never appear in an inline).
pub const C_ORIG: &str = "_o";
pub const C_BASE: &str = "_base";
pub const C_LEAD: &str = "_lead";
pub const C_TAIL: &str = "_tail";
pub const C_SIMPLE: &str = "_simple";
pub const C_TRACKED: &str = "_tracked";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PartKind {
    Main,
    Header,
    Footer,
    Footnotes,
    Endnotes,
    Comments,
    Core,
}

#[derive(Clone, Copy, Default)]
struct Flags {
    in_cell: bool,
    in_textbox: bool,
}

enum Atom {
    Text {
        rpr: String,
        key: String,
        text: String,
    },
    Code {
        xml: String,
        display: String,
        label: &'static str,
        open: Option<String>,
    },
    Keep(String),
    Drop,
    Open {
        open: String,
        close: String,
        display: String,
        close_display: String,
        label: &'static str,
    },
    Close,
}

#[derive(Default)]
struct FieldState {
    depth: usize,
    buf: String,
    instr: String,
    result: String,
    in_result: bool,
}

#[derive(Default)]
struct ParaState {
    tracked: bool,
    comment_ids: Vec<String>,
}

#[derive(Default)]
pub struct Styles {
    pub names: HashMap<String, String>,
    pub numbered: HashSet<String>,
    pub lang: Option<String>,
}

impl Styles {
    pub fn parse(nodes: &[Node], ns: &Ns) -> Styles {
        let mut s = Styles::default();
        fn walk(nodes: &[Node], ns: &Ns, s: &mut Styles) {
            for n in nodes {
                if let Node::El(e) = n {
                    if ns.is_w(&e.name, "style") {
                        if let Some(id) = ns.attr(e, "styleId") {
                            for c in e.elements() {
                                if ns.is_w(&c.name, "name") {
                                    if let Some(v) = ns.attr(c, "val") {
                                        s.names.insert(id.clone(), v);
                                    }
                                }
                                if ns.is_w(&c.name, "pPr")
                                    && c.elements().any(|x| ns.is_w(&x.name, "numPr"))
                                {
                                    s.numbered.insert(id.clone());
                                }
                            }
                        }
                    } else if ns.is_w(&e.name, "lang") && s.lang.is_none() {
                        s.lang = ns.attr(e, "val");
                    } else {
                        walk(&e.children, ns, s);
                    }
                }
            }
        }
        walk(nodes, ns, &mut s);
        s
    }
}

struct Pending {
    part: usize,
    ord: usize,
    unit: Unit,
}

pub struct Extractor {
    ns: Rc<Ns>,
    styles: Styles,
    comments: HashMap<String, String>,
    part_seq: usize,
    part_kind: PartKind,
    part_name: String,
    prefix: String,
    pending: Vec<Pending>,
    index: HashMap<String, usize>,
    memo: HashMap<usize, String>,
    alt_memo: HashMap<usize, String>,
    nesting: usize,
    field: FieldState,
    last_heading: Option<String>,
    core_ord: usize,
}

fn is_wrapper_local(l: &str) -> Option<(&'static str, &'static str)> {
    match l {
        "hyperlink" => Some(("<a>", "link")),
        "smartTag" => Some(("<x>", "smart tag")),
        "customXml" => Some(("<x>", "custom xml")),
        "bdo" | "dir" => Some(("<x>", "direction")),
        _ => None,
    }
}

impl Extractor {
    pub fn new(styles: Styles, comments: HashMap<String, String>) -> Extractor {
        Extractor {
            ns: Rc::new(Ns::default_ns()),
            styles,
            comments,
            part_seq: 0,
            part_kind: PartKind::Main,
            part_name: String::new(),
            prefix: String::new(),
            pending: Vec::new(),
            index: HashMap::new(),
            memo: HashMap::new(),
            alt_memo: HashMap::new(),
            nesting: 0,
            field: FieldState::default(),
            last_heading: None,
            core_ord: 0,
        }
    }

    pub fn begin_part(&mut self, kind: PartKind, name: &str, prefix: String, ns: Ns) {
        self.part_seq += 1;
        self.part_kind = kind;
        self.part_name = name.to_string();
        self.prefix = prefix;
        self.ns = Rc::new(ns);
        self.memo.clear();
        self.alt_memo.clear();
        self.field = FieldState::default();
        self.last_heading = None;
        self.nesting = 0;
    }

    pub fn run_part(&mut self, nodes: &[Node]) -> String {
        let mut out = String::new();
        for n in nodes {
            out.push_str(&self.emit(n, Flags::default()));
        }
        out
    }

    /// Units in document order (part order, then paragraph order).
    pub fn finish(mut self) -> Vec<Unit> {
        self.pending.sort_by_key(|p| (p.part, p.ord));
        self.pending.into_iter().map(|p| p.unit).collect()
    }

    fn emit(&mut self, node: &Node, fl: Flags) -> String {
        match node {
            Node::Text(t) | Node::Raw(t) => t.clone(),
            Node::El(e) => self.emit_el(e, fl),
        }
    }

    fn emit_el_nested(&mut self, e: &El, fl: Flags) -> String {
        self.nesting += 1;
        let r = self.emit_el(e, fl);
        self.nesting -= 1;
        r
    }

    fn emit_el(&mut self, e: &El, fl: Flags) -> String {
        let ns = self.ns.clone();
        if ns.is_w(&e.name, "p") {
            return self.paragraph(e, fl);
        }
        if ns.is_mc(&e.name, "AlternateContent") {
            return self.alt_content(e, fl);
        }
        if self.part_kind == PartKind::Core && (e.name == "dc:title" || e.name == "dc:subject") {
            return self.core_simple(e);
        }
        self.emit_default(e, fl)
    }

    fn emit_default(&mut self, e: &El, fl: Flags) -> String {
        let ns = self.ns.clone();
        let mut fl2 = fl;
        if ns.is_w(&e.name, "tc") {
            fl2.in_cell = true;
        }
        if ns.is_w(&e.name, "txbxContent") {
            fl2.in_textbox = true;
            fl2.in_cell = false;
        }
        let mut s = e.start.clone();
        for c in &e.children {
            s.push_str(&self.emit(c, fl2));
        }
        s.push_str(&e.end);
        s
    }

    fn core_simple(&mut self, e: &El) -> String {
        let all_text = e.children.iter().all(|c| matches!(c, Node::Text(_)));
        let text = e.text();
        if e.end.is_empty() || !all_text || text.trim().is_empty() {
            return e.raw();
        }
        self.core_ord += 1;
        let local = local_name(&e.name).to_string();
        let id = format!("meta-{local}");
        let mut inner = String::new();
        for c in &e.children {
            c.write_raw(&mut inner);
        }
        let mut u = Unit::new(id.clone(), vec![Inline::text(text)]);
        u.context.kind = Some(local);
        u.context.location = Some(format!("{} {}", self.part_name, e.name));
        u.codes.insert(
            C_ORIG.into(),
            CodeData {
                standalone: Some(inner),
                ..Default::default()
            },
        );
        u.codes.insert(C_SIMPLE.into(), CodeData::default());
        self.add_unit(u, self.core_ord);
        format!("{}{M_UNIT}{id}{M_END}{}", e.start, e.end)
    }

    fn add_unit(&mut self, unit: Unit, ord: usize) {
        if self.index.contains_key(&unit.id) {
            return;
        }
        self.index.insert(unit.id.clone(), self.pending.len());
        self.pending.push(Pending {
            part: self.part_seq,
            ord,
            unit,
        });
    }

    // ---------------------------------------------------------------- AlternateContent

    fn alt_content(&mut self, e: &El, fl: Flags) -> String {
        let ns = self.ns.clone();
        let choice = e.elements().find(|c| ns.is_mc(&c.name, "Choice"));
        let fallback = e.elements().find(|c| ns.is_mc(&c.name, "Fallback"));
        let (Some(choice), Some(fallback)) = (choice, fallback) else {
            return self.emit_default(e, fl);
        };
        let key = e as *const El as usize;
        if let Some(s) = self.alt_memo.get(&key) {
            return s.clone();
        }
        let choice_out = self.emit_default(choice, fl);
        let mut out = e.start.clone();
        for n in &e.children {
            match n {
                Node::El(c) if std::ptr::eq(c, choice) => out.push_str(&choice_out),
                Node::El(c) if std::ptr::eq(c, fallback) => {
                    let s = self.fallback(fallback, choice);
                    out.push_str(&s);
                }
                other => out.push_str(&self.emit(other, fl)),
            }
        }
        out.push_str(&e.end);
        self.alt_memo.insert(key, out.clone());
        out
    }

    fn collect_paras<'a>(&self, e: &'a El, out: &mut Vec<&'a El>) {
        for c in e.elements() {
            if self.ns.is_w(&c.name, "p") {
                out.push(c);
            }
            self.collect_paras(c, out);
        }
    }

    fn para_text(&self, e: &El) -> String {
        let mut s = String::new();
        fn walk(ns: &Ns, e: &El, s: &mut String) {
            for c in e.elements() {
                if ns.is_w(&c.name, "t") {
                    s.push_str(&c.text());
                } else {
                    walk(ns, c, s);
                }
            }
        }
        walk(&self.ns, e, &mut s);
        s
    }

    /// The fallback copy of a text box: each paragraph that pairs with a paragraph of the choice (same
    /// position, same text) becomes a mirror of that unit, so both copies are translated together. When the
    /// two do not line up the fallback is left as it was (it stays in the source language).
    fn fallback(&mut self, fb: &El, choice: &El) -> String {
        let mut cp = Vec::new();
        let mut fp = Vec::new();
        self.collect_paras(choice, &mut cp);
        self.collect_paras(fb, &mut fp);
        let mut map: HashMap<usize, String> = HashMap::new();
        if cp.len() == fp.len()
            && cp
                .iter()
                .zip(&fp)
                .all(|(a, b)| self.para_text(a) == self.para_text(b))
        {
            for (a, b) in cp.iter().zip(&fp) {
                let id = format!("{}p{}", self.prefix, a.ord);
                if self.index.contains_key(&id) {
                    map.insert(b.ord, id);
                }
            }
        }
        self.emit_fallback(fb, &map)
    }

    fn emit_fallback(&mut self, e: &El, map: &HashMap<usize, String>) -> String {
        let ns = self.ns.clone();
        if ns.is_w(&e.name, "p") {
            if let Some(uid) = map.get(&e.ord) {
                let ppr_pos = e
                    .children
                    .iter()
                    .position(|n| matches!(n, Node::El(x) if ns.is_w(&x.name, "pPr")));
                let mut shell = e.start.clone();
                let from = match ppr_pos {
                    Some(k) => {
                        for n in &e.children[..=k] {
                            n.write_raw(&mut shell);
                        }
                        k + 1
                    }
                    None => 0,
                };
                let mut inner = String::new();
                for n in &e.children[from..] {
                    n.write_raw(&mut inner);
                }
                if let Some(&pi) = self.index.get(uid) {
                    let unit = &mut self.pending[pi].unit;
                    let k = unit.codes.keys().filter(|k| k.starts_with("_m")).count();
                    unit.codes.insert(
                        format!("_m{k}"),
                        CodeData {
                            standalone: Some(inner),
                            ..Default::default()
                        },
                    );
                    return format!("{shell}{M_MIRROR}{uid}:{k}{M_END}{}", e.end);
                }
            }
        }
        let mut s = e.start.clone();
        for n in &e.children {
            match n {
                Node::El(c) => s.push_str(&self.emit_fallback(c, map)),
                other => other.write_raw(&mut s),
            }
        }
        s.push_str(&e.end);
        s
    }

    // ---------------------------------------------------------------- paragraphs

    fn paragraph(&mut self, p: &El, fl: Flags) -> String {
        if self.nesting > 0 {
            if let Some(s) = self.memo.get(&p.ord) {
                return s.clone();
            }
        }
        let saved = if self.nesting > 0 {
            Some(std::mem::take(&mut self.field))
        } else {
            None
        };
        let r = self.paragraph_inner(p, fl);
        if let Some(f) = saved {
            self.field = f;
        }
        if self.nesting > 0 {
            self.memo.insert(p.ord, r.clone());
        }
        r
    }

    fn paragraph_inner(&mut self, p: &El, fl: Flags) -> String {
        if p.end.is_empty() {
            return p.start.clone();
        }
        self.nesting += 1;
        let ns = self.ns.clone();
        let ppr_pos = p
            .children
            .iter()
            .position(|n| matches!(n, Node::El(e) if ns.is_w(&e.name, "pPr")));
        let mut shell = p.start.clone();
        let from = match ppr_pos {
            Some(k) => {
                for n in &p.children[..=k] {
                    n.write_raw(&mut shell);
                }
                k + 1
            }
            None => 0,
        };
        let content = &p.children[from..];
        let ppr = ppr_pos.and_then(|k| {
            if let Node::El(e) = &p.children[k] {
                Some(e)
            } else {
                None
            }
        });

        let mut st = ParaState::default();
        let mut atoms = Vec::new();
        self.atoms(content, &mut atoms, &mut st, fl);
        if self.field.depth > 0 && !self.field.buf.is_empty() {
            self.flush_field(&mut atoms);
        }
        let result = if self.build_unit(p, ppr, atoms, st, fl, content) {
            format!("{shell}{M_UNIT}{}p{}{M_END}{}", self.prefix, p.ord, p.end)
        } else {
            let mut s = shell;
            for c in content {
                s.push_str(&self.emit(c, fl));
            }
            s.push_str(&p.end);
            s
        };
        self.nesting -= 1;
        result
    }

    // ---------------------------------------------------------------- atoms

    fn has_fldchar(&self, e: &El) -> bool {
        let ns = &self.ns;
        if ns.is_w(&e.name, "fldChar") {
            return true;
        }
        if ["drawing", "pict", "object", "txbxContent"]
            .iter()
            .any(|l| ns.is_w(&e.name, l))
            || ns.is_mc(&e.name, "AlternateContent")
        {
            return false;
        }
        e.elements().any(|c| self.has_fldchar(c))
    }

    fn scan_fields(&mut self, e: &El) {
        let ns = self.ns.clone();
        if ns.is_w(&e.name, "fldChar") {
            match ns.attr(e, "fldCharType").as_deref() {
                Some("begin") => {
                    self.field.depth += 1;
                    if self.field.depth == 1 {
                        self.field.instr.clear();
                        self.field.result.clear();
                        self.field.in_result = false;
                    }
                }
                Some("separate") => {
                    if self.field.depth == 1 {
                        self.field.in_result = true;
                    }
                }
                Some("end") => {
                    self.field.depth = self.field.depth.saturating_sub(1);
                }
                _ => {}
            }
            return;
        }
        if ns.is_w(&e.name, "instrText") {
            if self.field.depth == 1 && !self.field.in_result {
                let t = e.text();
                self.field.instr.push_str(&t);
            }
            return;
        }
        if ns.is_w(&e.name, "t") {
            if self.field.depth == 1 && self.field.in_result {
                let t = e.text();
                self.field.result.push_str(&t);
            }
            return;
        }
        if ["drawing", "pict", "object"]
            .iter()
            .any(|l| ns.is_w(&e.name, l))
            || ns.is_mc(&e.name, "AlternateContent")
        {
            return;
        }
        for c in e.elements() {
            self.scan_fields(c);
        }
    }

    fn flush_field(&mut self, out: &mut Vec<Atom>) {
        let f = &mut self.field;
        let name = f
            .instr
            .split_whitespace()
            .next()
            .unwrap_or("field")
            .to_uppercase();
        let result = f.result.trim().to_string();
        let shown: String = result.chars().take(24).collect();
        let display = if shown.is_empty() {
            format!("{{{name}}}")
        } else {
            format!("{{{name} {shown}}}")
        };
        let xml = std::mem::take(&mut f.buf);
        if f.depth == 0 {
            f.instr.clear();
            f.result.clear();
            f.in_result = false;
        }
        out.push(Atom::Code {
            xml,
            display,
            label: "field",
            open: Some(result),
        });
    }

    fn atoms(&mut self, children: &[Node], out: &mut Vec<Atom>, st: &mut ParaState, fl: Flags) {
        for n in children {
            let e = match n {
                Node::El(e) => e,
                Node::Text(t) | Node::Raw(t) => {
                    if self.field.depth > 0 {
                        self.field.buf.push_str(t);
                    }
                    continue;
                }
            };
            if self.field.depth > 0 || self.has_fldchar(e) {
                self.scan_fields(e);
                let raw = self.emit_el_nested(e, fl);
                self.field.buf.push_str(&raw);
                if self.field.depth == 0 {
                    self.flush_field(out);
                }
                continue;
            }
            self.atom_el(e, out, st, fl);
        }
    }

    fn code(&mut self, e: &El, display: &str, label: &'static str, fl: Flags, out: &mut Vec<Atom>) {
        let xml = self.emit_el_nested(e, fl);
        out.push(Atom::Code {
            xml,
            display: display.into(),
            label,
            open: None,
        });
    }

    fn atom_el(&mut self, e: &El, out: &mut Vec<Atom>, st: &mut ParaState, fl: Flags) {
        let ns = self.ns.clone();
        let local = e.name.strip_prefix(ns.w.as_str()).unwrap_or("");
        if ns.is_w(&e.name, "r") {
            self.run(e, out, st, fl);
        } else if let Some((display, label)) = is_wrapper_local(local) {
            out.push(Atom::Open {
                open: e.start.clone(),
                close: e.end.clone(),
                display: display.into(),
                close_display: display.replace('<', "</"),
                label,
            });
            self.atoms(&e.children, out, st, fl);
            out.push(Atom::Close);
        } else if local == "sdt" {
            let content = e.elements().find(|c| ns.is_w(&c.name, "sdtContent"));
            match content {
                Some(c) if !c.end.is_empty() => {
                    let mut open = e.start.clone();
                    for n in &e.children {
                        match n {
                            Node::El(x) if std::ptr::eq(x, c) => break,
                            other => other.write_raw(&mut open),
                        }
                    }
                    open.push_str(&c.start);
                    let close = format!("{}{}", c.end, e.end);
                    out.push(Atom::Open {
                        open,
                        close,
                        display: "<x>".into(),
                        close_display: "</x>".into(),
                        label: "content control",
                    });
                    self.atoms(&c.children, out, st, fl);
                    out.push(Atom::Close);
                }
                _ => self.code(e, "<code>", "code", fl, out),
            }
        } else if local == "ins" || local == "moveTo" {
            st.tracked = true;
            self.atoms(&e.children, out, st, fl);
        } else if local == "del" || local == "moveFrom" {
            st.tracked = true;
            out.push(Atom::Drop);
        } else if local == "fldSimple" {
            let instr = ns.attr(e, "instr").unwrap_or_default();
            let name = instr
                .split_whitespace()
                .next()
                .unwrap_or("field")
                .to_uppercase();
            let mut result = String::new();
            fn texts(ns: &Ns, e: &El, s: &mut String) {
                for c in e.elements() {
                    if ns.is_w(&c.name, "t") {
                        s.push_str(&c.text());
                    } else {
                        texts(ns, c, s);
                    }
                }
            }
            texts(&ns, e, &mut result);
            let result = result.trim().to_string();
            let shown: String = result.chars().take(24).collect();
            let display = if shown.is_empty() {
                format!("{{{name}}}")
            } else {
                format!("{{{name} {shown}}}")
            };
            let xml = self.emit_el_nested(e, fl);
            out.push(Atom::Code {
                xml,
                display,
                label: "field",
                open: Some(result),
            });
        } else if local == "commentRangeStart" {
            if let Some(id) = ns.attr(e, "id") {
                if !st.comment_ids.contains(&id) {
                    st.comment_ids.push(id);
                }
            }
            out.push(Atom::Keep(e.raw()));
        } else if local == "proofErr" {
            out.push(Atom::Drop);
        } else if local == "bookmarkStart"
            || local == "bookmarkEnd"
            || local == "commentRangeEnd"
            || local == "permStart"
            || local == "permEnd"
            || local.ends_with("RangeStart")
            || local.ends_with("RangeEnd")
        {
            out.push(Atom::Keep(e.raw()));
        } else if local_name(&e.name) == "oMath" || local_name(&e.name) == "oMathPara" {
            self.code(e, "{math}", "math", fl, out);
        } else {
            self.code(e, "<code>", "code", fl, out);
        }
    }

    fn run(&mut self, e: &El, out: &mut Vec<Atom>, st: &mut ParaState, fl: Flags) {
        let ns = self.ns.clone();
        // An empty `<w:rPr/>` carries nothing: drop it so plain runs are written without one.
        let rpr_el = e
            .elements()
            .find(|c| ns.is_w(&c.name, "rPr"))
            .filter(|r| r.elements().next().is_some());
        let rpr = rpr_el.map(|r| r.raw()).unwrap_or_default();
        let key = rpr_el.map(canon_rpr).unwrap_or_default();
        let wrap = |inner: &str| format!("<{}r>{rpr}{inner}</{}r>", ns.w, ns.w);
        for c in e.elements() {
            let local = c.name.strip_prefix(ns.w.as_str()).unwrap_or("");
            match local {
                "rPr" | "delText" | "instrText" => {}
                "t" => {
                    let text = c.text();
                    if !text.is_empty() {
                        out.push(Atom::Text {
                            rpr: rpr.clone(),
                            key: key.clone(),
                            text,
                        });
                    }
                }
                "noBreakHyphen" => out.push(Atom::Text {
                    rpr: rpr.clone(),
                    key: key.clone(),
                    text: "\u{2011}".into(),
                }),
                "softHyphen" => out.push(Atom::Text {
                    rpr: rpr.clone(),
                    key: key.clone(),
                    text: "\u{AD}".into(),
                }),
                "tab" => out.push(Atom::Code {
                    xml: wrap(&c.raw()),
                    display: "<tab>".into(),
                    label: "tab",
                    open: None,
                }),
                "br" | "cr" => {
                    let page = ns.attr(c, "type").as_deref() == Some("page");
                    let display = if page { "<page-break>" } else { "<br>" };
                    out.push(Atom::Code {
                        xml: wrap(&c.raw()),
                        display: display.into(),
                        label: "break",
                        open: None,
                    });
                }
                "lastRenderedPageBreak" => out.push(Atom::Drop),
                "footnoteReference" | "endnoteReference" => {
                    let id = ns.attr(c, "id").unwrap_or_default();
                    let short = if local == "footnoteReference" {
                        "fn"
                    } else {
                        "en"
                    };
                    out.push(Atom::Code {
                        xml: wrap(&c.raw()),
                        display: format!("[{short} {id}]"),
                        label: "footnote-ref",
                        open: None,
                    });
                }
                "commentReference" => {
                    if let Some(id) = ns.attr(c, "id") {
                        if !st.comment_ids.contains(&id) {
                            st.comment_ids.push(id);
                        }
                    }
                    out.push(Atom::Keep(wrap(&c.raw())));
                }
                "footnoteRef"
                | "endnoteRef"
                | "annotationRef"
                | "separator"
                | "continuationSeparator" => out.push(Atom::Keep(wrap(&c.raw()))),
                "drawing" | "pict" | "object" => {
                    let inner = self.emit_el_nested(c, fl);
                    let (display, label) = if inner.contains(M_UNIT) {
                        ("<textbox>", "textbox")
                    } else {
                        ("<img>", "image")
                    };
                    out.push(Atom::Code {
                        xml: wrap(&inner),
                        display: display.into(),
                        label,
                        open: None,
                    });
                }
                "sym" => out.push(Atom::Code {
                    xml: wrap(&c.raw()),
                    display: "<sym>".into(),
                    label: "symbol",
                    open: None,
                }),
                _ if ns.is_mc(&c.name, "AlternateContent") => {
                    let inner = self.emit_el_nested(c, fl);
                    let (display, label) = if inner.contains(M_UNIT) {
                        ("<textbox>", "textbox")
                    } else {
                        ("<img>", "image")
                    };
                    out.push(Atom::Code {
                        xml: wrap(&inner),
                        display: display.into(),
                        label,
                        open: None,
                    });
                }
                _ => {
                    let inner = self.emit_el_nested(c, fl);
                    out.push(Atom::Code {
                        xml: wrap(&inner),
                        display: "<code>".into(),
                        label: "code",
                        open: None,
                    });
                }
            }
        }
    }

    // ---------------------------------------------------------------- units

    /// Builds the unit for a paragraph. False when the paragraph has no text to translate.
    fn build_unit(
        &mut self,
        p: &El,
        ppr: Option<&El>,
        atoms: Vec<Atom>,
        st: ParaState,
        fl: Flags,
        content: &[Node],
    ) -> bool {
        let has_text = atoms.iter().any(
            |a| matches!(a, Atom::Text { text, .. } if text.chars().any(|c| !c.is_whitespace())),
        );
        if !has_text {
            return false;
        }
        // Split invisible marks off: before the first visible piece they lead, later ones trail.
        let mut vis: Vec<Atom> = Vec::new();
        let mut lead = String::new();
        let mut tail = String::new();
        for a in atoms {
            match a {
                Atom::Keep(x) => {
                    if vis.is_empty() {
                        lead.push_str(&x)
                    } else {
                        tail.push_str(&x)
                    }
                }
                Atom::Drop => {}
                other => vis.push(other),
            }
        }
        // Merge neighbouring text pieces with the same formatting.
        let mut merged: Vec<Atom> = Vec::new();
        for a in vis {
            match (merged.last_mut(), a) {
                (
                    Some(Atom::Text {
                        key: k0, text: t0, ..
                    }),
                    Atom::Text { key, text, .. },
                ) if *k0 == key => t0.push_str(&text),
                (_, a) => merged.push(a),
            }
        }
        let vis = merged;

        // The base formatting of each scope (the paragraph, then each link or content control): the most
        // common one, which needs no tag.
        let mut tallies: Vec<Vec<(String, String, usize)>> = vec![vec![]];
        let mut stack = vec![0usize];
        for a in &vis {
            match a {
                Atom::Open { .. } => {
                    tallies.push(vec![]);
                    stack.push(tallies.len() - 1);
                }
                Atom::Close => {
                    if stack.len() > 1 {
                        stack.pop();
                    }
                }
                Atom::Text { rpr, key, text } => {
                    let t = &mut tallies[*stack.last().unwrap_or(&0)];
                    let n = text.chars().count();
                    match t.iter_mut().find(|x| x.0 == *key) {
                        Some(x) => x.2 += n,
                        None => t.push((key.clone(), rpr.clone(), n)),
                    }
                }
                _ => {}
            }
        }
        let bases: Vec<(String, String)> = tallies
            .iter()
            .map(|t| {
                let mut best: Option<&(String, String, usize)> = None;
                for x in t {
                    if best.is_none_or(|b| x.2 > b.2) {
                        best = Some(x);
                    }
                }
                best.map(|b| (b.0.clone(), b.1.clone())).unwrap_or_default()
            })
            .collect();

        let mut inl: Vec<Inline> = Vec::new();
        let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
        let mut n = 0usize;
        let mut scope: Vec<usize> = vec![0];
        let mut next_scope = 1usize;
        let mut cur: Option<(String, String, String)> = None; // key, id, close display
        let mut open_wrappers: Vec<(String, String)> = Vec::new();
        macro_rules! close_cur {
            () => {
                if let Some((_, id, disp)) = cur.take() {
                    inl.push(Inline::tag(id, TagKind::Close, disp));
                }
            };
        }
        for (i, a) in vis.iter().enumerate() {
            match a {
                Atom::Text { rpr, key, text } => {
                    let base_key = &bases[*scope.last().unwrap_or(&0)].0;
                    if key == base_key {
                        close_cur!();
                    } else if !matches!(&cur, Some((k, _, _)) if k == key) {
                        close_cur!();
                        n += 1;
                        let id = n.to_string();
                        let (d, cd, label) = fmt_names(rpr);
                        codes.insert(
                            id.clone(),
                            CodeData {
                                open: Some(rpr.clone()),
                                close: Some(String::new()),
                                standalone: None,
                                label: Some(label),
                            },
                        );
                        inl.push(Inline::tag(id.clone(), TagKind::Open, d));
                        cur = Some((key.clone(), id, cd));
                    }
                    inl.push(Inline::text(text.clone()));
                }
                Atom::Code {
                    xml,
                    display,
                    label,
                    open,
                } => {
                    if let Some((k, _, _)) = &cur {
                        let next_key = vis[i + 1..]
                            .iter()
                            .take_while(|x| !matches!(x, Atom::Open { .. } | Atom::Close))
                            .find_map(|x| {
                                if let Atom::Text { key, .. } = x {
                                    Some(key)
                                } else {
                                    None
                                }
                            });
                        if next_key != Some(k) {
                            close_cur!();
                        }
                    }
                    n += 1;
                    let id = n.to_string();
                    codes.insert(
                        id.clone(),
                        CodeData {
                            open: open.clone(),
                            close: None,
                            standalone: Some(xml.clone()),
                            label: Some((*label).into()),
                        },
                    );
                    inl.push(Inline::tag(id, TagKind::Standalone, display.clone()));
                }
                Atom::Open {
                    open,
                    close,
                    display,
                    close_display,
                    label,
                } => {
                    close_cur!();
                    n += 1;
                    let id = n.to_string();
                    let base = bases
                        .get(next_scope)
                        .map(|b| b.1.clone())
                        .unwrap_or_default();
                    codes.insert(
                        id.clone(),
                        CodeData {
                            open: Some(open.clone()),
                            close: Some(close.clone()),
                            standalone: Some(base),
                            label: Some((*label).into()),
                        },
                    );
                    scope.push(next_scope);
                    next_scope += 1;
                    inl.push(Inline::tag(id.clone(), TagKind::Open, display.clone()));
                    open_wrappers.push((id, close_display.clone()));
                }
                Atom::Close => {
                    close_cur!();
                    if let Some((id, d)) = open_wrappers.pop() {
                        scope.pop();
                        inl.push(Inline::tag(id, TagKind::Close, d));
                    }
                }
                Atom::Keep(_) | Atom::Drop => {}
            }
        }
        close_cur!();
        let source = merge_text(inl);

        codes.insert(
            C_BASE.into(),
            CodeData {
                standalone: Some(bases[0].1.clone()),
                ..Default::default()
            },
        );
        if !lead.is_empty() {
            codes.insert(
                C_LEAD.into(),
                CodeData {
                    standalone: Some(lead),
                    ..Default::default()
                },
            );
        }
        if !tail.is_empty() {
            codes.insert(
                C_TAIL.into(),
                CodeData {
                    standalone: Some(tail),
                    ..Default::default()
                },
            );
        }
        let mut orig = String::new();
        for c in content {
            orig.push_str(&self.emit(c, fl));
        }
        codes.insert(
            C_ORIG.into(),
            CodeData {
                standalone: Some(orig),
                ..Default::default()
            },
        );
        if st.tracked {
            codes.insert(C_TRACKED.into(), CodeData::default());
        }

        // Context.
        let ns = self.ns.clone();
        let (style, has_num, outline) = para_info(ppr, &ns);
        let style_name = style
            .as_ref()
            .and_then(|s| self.styles.names.get(s))
            .cloned();
        let heading_level = heading_level(style.as_deref(), style_name.as_deref(), outline);
        let list = has_num
            || style
                .as_ref()
                .is_some_and(|s| self.styles.numbered.contains(s));
        let kind = match self.part_kind {
            PartKind::Header => "header",
            PartKind::Footer => "footer",
            PartKind::Footnotes => "footnote",
            PartKind::Endnotes => "endnote",
            PartKind::Comments => "comment",
            _ if fl.in_textbox => "textbox",
            _ if heading_level.is_some() => "heading",
            _ if list => "list-item",
            _ if fl.in_cell => "cell",
            _ => "paragraph",
        };
        let id = format!("{}p{}", self.prefix, p.ord);
        let mut unit = Unit::new(id, source);
        unit.codes = codes;
        unit.context.kind = Some(kind.into());
        unit.context.style = style;
        unit.context.heading = self.last_heading.clone();
        unit.context.location = Some(format!("{} paragraph {}", self.part_name, p.ord));
        for cid in &st.comment_ids {
            if let Some(t) = self.comments.get(cid) {
                unit.context.notes.push(t.clone());
            }
        }
        if st.tracked {
            unit.context
                .notes
                .push("This paragraph has tracked changes".into());
        }
        if heading_level.is_some() && self.part_kind == PartKind::Main {
            let text = plain_text(&unit.segments[0].source).trim().to_string();
            self.last_heading = Some(text);
        }
        self.add_unit(unit, p.ord);
        true
    }
}

fn para_info(ppr: Option<&El>, ns: &Ns) -> (Option<String>, bool, Option<u8>) {
    let mut style = None;
    let mut num = false;
    let mut outline = None;
    if let Some(ppr) = ppr {
        for c in ppr.elements() {
            if ns.is_w(&c.name, "pStyle") {
                style = ns.attr(c, "val");
            } else if ns.is_w(&c.name, "numPr") {
                let numid = c
                    .elements()
                    .find(|x| ns.is_w(&x.name, "numId"))
                    .and_then(|x| ns.attr(x, "val"));
                num = numid.as_deref() != Some("0");
            } else if ns.is_w(&c.name, "outlineLvl") {
                outline = ns
                    .attr(c, "val")
                    .and_then(|v| v.parse::<u8>().ok())
                    .filter(|v| *v < 9);
            }
        }
    }
    (style, num, outline)
}

fn heading_level(
    style_id: Option<&str>,
    style_name: Option<&str>,
    outline: Option<u8>,
) -> Option<u8> {
    fn from(s: &str) -> Option<u8> {
        let l = s.to_lowercase().replace(' ', "");
        if l == "title" {
            return Some(1);
        }
        l.strip_prefix("heading")
            .and_then(|d| d.parse::<u8>().ok())
            .filter(|d| (1..=9).contains(d))
    }
    style_name
        .and_then(from)
        .or_else(|| style_id.and_then(from))
        .or(outline.map(|o| o + 1))
}

/// Formatting key for comparing runs: language marks, spell-check marks and revision ids do not count.
pub fn canon_rpr(r: &El) -> String {
    let mut parts = Vec::new();
    for c in r.elements() {
        let local = local_name(&c.name);
        if matches!(local, "lang" | "noProof") {
            continue;
        }
        let mut attrs: Vec<String> = c
            .attrs()
            .into_iter()
            .filter(|(k, _)| !local_name(k).starts_with("rsid"))
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        attrs.sort();
        let mut inner = String::new();
        for n in &c.children {
            n.write_raw(&mut inner);
        }
        parts.push(format!("{local}[{}]{inner}", attrs.join(",")));
    }
    parts.join("|")
}

/// (open display, close display, label) for a formatting tag made from a run's `w:rPr`.
fn fmt_names(rpr: &str) -> (String, String, String) {
    let mut features: Vec<String> = Vec::new();
    if let Ok(nodes) = parse(rpr) {
        for n in &nodes {
            if let Node::El(r) = n {
                for c in r.elements() {
                    let val = c
                        .attrs()
                        .into_iter()
                        .find(|(k, _)| local_name(k) == "val")
                        .map(|(_, v)| v);
                    let off = matches!(val.as_deref(), Some("0") | Some("false") | Some("none"));
                    match local_name(&c.name) {
                        "b" if !off => features.push("bold".into()),
                        "i" if !off => features.push("italic".into()),
                        "u" if !off => features.push("underline".into()),
                        "strike" | "dstrike" if !off => features.push("strike".into()),
                        "vertAlign" => match val.as_deref() {
                            Some("superscript") => features.push("superscript".into()),
                            Some("subscript") => features.push("subscript".into()),
                            _ => {}
                        },
                        "rStyle" => features.push(format!("style {}", val.unwrap_or_default())),
                        "color" => features.push("color".into()),
                        "highlight" | "shd" => features.push("highlight".into()),
                        "sz" => features.push("size".into()),
                        "rFonts" => features.push("font".into()),
                        "caps" | "smallCaps" if !off => features.push("caps".into()),
                        _ => {}
                    }
                }
            }
        }
    }
    let label = if rpr.is_empty() {
        "no formatting".to_string()
    } else if features.is_empty() {
        "formatting".to_string()
    } else {
        features.join(" ")
    };
    let single = match features.as_slice() {
        [f] if f == "bold" => Some("b"),
        [f] if f == "italic" => Some("i"),
        [f] if f == "underline" => Some("u"),
        [f] if f == "superscript" => Some("sup"),
        [f] if f == "subscript" => Some("sub"),
        _ => None,
    };
    let t = single.unwrap_or("fmt");
    (format!("<{t}>"), format!("</{t}>"), label)
}
