//! Extraction: workbook, shared strings, sheets, comments, tables. Every part that holds text is parsed into a
//! lossless tree; the text of a unit is replaced by a marker and the tree is written back as the skeleton
//! template. Parts without text are copied as they are.

use crate::refs::*;
use crate::rich::*;
use crate::xml::*;
use crate::zipio::{self, RawEntry, ZipLimits};
use crate::*;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const SS_URIS: &[&str] = &[
    "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
    "http://purl.oclc.org/ooxml/spreadsheetml/main",
];

struct SheetInfo {
    idx: usize,
    name: String,
    file: String,
    hidden: bool,
}

struct Rel {
    typ: String,
    target: String,
    id: String,
}

fn dir_of(path: &str) -> String {
    path.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default()
}

fn resolve_target(base_dir: &str, target: &str) -> String {
    let t = target.replace('\\', "/");
    let mut parts: Vec<&str> = if t.starts_with('/') {
        vec![]
    } else {
        base_dir.split('/').filter(|p| !p.is_empty()).collect()
    };
    for p in t.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

fn find_entry<'a>(entries: &'a [RawEntry], name: &str) -> Option<(usize, &'a RawEntry)> {
    entries.iter().enumerate().find(|(_, e)| e.name == name)
}

fn parse_part(e: &RawEntry) -> Option<Vec<Node>> {
    let t = std::str::from_utf8(&e.data).ok()?;
    if has_marker_chars(t) {
        return None;
    }
    parse(t).ok()
}

fn has_marker_chars(s: &str) -> bool {
    s.chars().any(|c| ('\u{E000}'..='\u{E005}').contains(&c))
}

fn read_rels(entries: &[RawEntry], part: &str) -> Vec<Rel> {
    let file = part.rsplit('/').next().unwrap_or(part);
    let path = format!("{}_rels/{file}.rels", dir_of(part));
    let Some((_, e)) = find_entry(entries, &path) else {
        return vec![];
    };
    let Some(nodes) = parse_part(e) else {
        return vec![];
    };
    let base = dir_of(part);
    let mut out = vec![];
    fn walk(nodes: &[Node], base: &str, out: &mut Vec<Rel>) {
        for n in nodes {
            if let Node::El(e) = n {
                if local_name(&e.name) == "Relationship" {
                    if e.attr("TargetMode").as_deref() == Some("External") {
                        continue;
                    }
                    if let (Some(id), Some(t), Some(target)) = (e.attr("Id"), e.attr("Type"), e.attr("Target")) {
                        out.push(Rel { id, typ: t, target: resolve_target(base, &target) });
                    }
                }
                walk(&e.children, base, out);
            }
        }
    }
    walk(&nodes, &base, &mut out);
    out
}

fn workbook_part(content_types: &[u8]) -> Option<String> {
    let nodes = parse(std::str::from_utf8(content_types).ok()?).ok()?;
    fn walk(nodes: &[Node]) -> Option<String> {
        for n in nodes {
            if let Node::El(e) = n {
                if local_name(&e.name) == "Override" {
                    let ct = e.attr("ContentType").unwrap_or_default();
                    if ct.contains("spreadsheetml.") && ct.ends_with(".main+xml") {
                        return e.attr("PartName").map(|p| p.trim_start_matches('/').to_string());
                    }
                }
                if let Some(x) = walk(&e.children) {
                    return Some(x);
                }
            }
        }
        None
    }
    walk(&nodes)
}

fn root_mut(nodes: &mut [Node]) -> Option<&mut El> {
    nodes.iter_mut().find_map(|n| if let Node::El(e) = n { Some(e) } else { None })
}

fn marker(id: &str) -> Node {
    Node::Text(format!("{M_UNIT}{id}{M_END}"))
}

fn raw_children(e: &El) -> String {
    let mut s = String::new();
    for c in &e.children {
        c.write_raw(&mut s);
    }
    s
}

fn has_text(atoms: &[Atom]) -> bool {
    atoms
        .iter()
        .any(|a| matches!(a, Atom::Text { text, .. } if text.chars().any(|c| c.is_alphabetic())))
}

/// Replaces the value of an attribute in a start tag as written. `f` gets the raw value.
fn map_attr(start: &str, attr: &str, f: impl Fn(&str) -> String) -> String {
    let b = start.as_bytes();
    let pat = attr.as_bytes();
    let mut i = 1;
    while i + pat.len() + 2 < b.len() {
        if b[i - 1].is_ascii_whitespace() && &b[i..i + pat.len()] == pat {
            let mut j = i + pat.len();
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < b.len() && b[j] == b'=' {
                j += 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
                    let q = b[j];
                    let vs = j + 1;
                    if let Some(len) = b[vs..].iter().position(|c| *c == q) {
                        return format!("{}{}{}", &start[..vs], f(&start[vs..vs + len]), &start[vs + len..]);
                    }
                }
            }
        }
        i += 1;
    }
    start.to_string()
}

fn is_true(v: Option<String>) -> bool {
    matches!(v.as_deref(), Some("1") | Some("true"))
}

pub fn props_of(fmt: &str) -> BTreeSet<&'static str> {
    let mut s = BTreeSet::new();
    let Ok(nodes) = parse(fmt) else { return s };
    fn walk(nodes: &[Node], s: &mut BTreeSet<&'static str>) {
        for n in nodes {
            if let Node::El(e) = n {
                let off = matches!(e.attr("val").as_deref(), Some("0") | Some("false"));
                match local_name(&e.name) {
                    "b" if !off => {
                        s.insert("b");
                    }
                    "i" if !off => {
                        s.insert("i");
                    }
                    "u" if e.attr("val").as_deref() != Some("none") => {
                        s.insert("u");
                    }
                    "vertAlign" => match e.attr("val").as_deref() {
                        Some("superscript") => {
                            s.insert("sup");
                        }
                        Some("subscript") => {
                            s.insert("sub");
                        }
                        _ => {}
                    },
                    _ => {}
                }
                walk(&e.children, s);
            }
        }
    }
    walk(&nodes, &mut s);
    s
}

/// Atoms of a shared string item or comment text: `t` and `r` children. Phonetic runs are skipped (the second
/// value says they exist).
fn si_atoms(ns: &Ns, children: &[Node]) -> (Vec<Atom>, bool) {
    let mut atoms = vec![];
    let mut phonetic = false;
    for c in children {
        let Node::El(e) = c else { continue };
        if ns.is(&e.name, "t") {
            atoms.push(Atom::Text { fmt: String::new(), key: String::new(), text: e.text() });
        } else if ns.is(&e.name, "r") {
            let mut fmt = String::new();
            let mut text = String::new();
            for x in e.elements() {
                if ns.is(&x.name, "rPr") {
                    fmt = x.raw();
                } else if ns.is(&x.name, "t") {
                    text.push_str(&x.text());
                }
            }
            atoms.push(Atom::Text { key: fmt.clone(), fmt, text });
        } else if ns.is(&e.name, "rPh") || ns.is(&e.name, "phoneticPr") {
            phonetic = true;
        }
    }
    (atoms, phonetic)
}

fn plain_of(ns: &Ns, children: &[Node]) -> String {
    let (atoms, _) = si_atoms(ns, children);
    atoms
        .into_iter()
        .filter_map(|a| if let Atom::Text { text, .. } = a { Some(text) } else { None })
        .collect()
}

#[derive(Default)]
struct Use {
    loc: String,
    count: usize,
    all_hidden: bool,
    hidden_kind: Option<&'static str>,
    max_len: Option<usize>,
    notes: Vec<String>,
}

#[derive(Clone)]
enum CellRef {
    Sst(usize),
    Inline(String),
}

#[derive(Default)]
struct SheetCtx {
    hidden_cols: Vec<(u32, u32)>,
    validations: Vec<(String, usize)>,
}

impl SheetCtx {
    fn col_hidden(&self, col: u32) -> bool {
        self.hidden_cols.iter().any(|(a, b)| col >= *a && col <= *b)
    }
    fn max_len(&self, col: u32, row: u32) -> Option<usize> {
        self.validations
            .iter()
            .filter(|(sq, _)| in_sqref(sq, col, row))
            .map(|(_, n)| *n)
            .min()
    }
}

struct Extractor<'a> {
    xo: &'a XlsxOptions,
    /// (sheet name, unit id) of every sheet, only when sheet names are translated.
    sheet_units: Vec<(String, String)>,
    notes: HashMap<(usize, String), Vec<String>>,
    uses: BTreeMap<usize, Use>,
    cell_refs: HashMap<(usize, (u32, u32)), CellRef>,
    u_inline: Vec<Unit>,
    u_hf: Vec<Unit>,
    u_comments: Vec<Unit>,
}

fn make_unit(
    id: String,
    built: Built,
    orig: String,
    mode: &str,
    pfx: &str,
    ctx: UnitContext,
) -> Unit {
    let mut u = Unit::new(id, built.inlines);
    u.codes = built.codes;
    u.codes.insert(C_ORIG.into(), CodeData { standalone: Some(orig), ..Default::default() });
    u.codes.insert(C_MODE.into(), CodeData { standalone: Some(mode.into()), ..Default::default() });
    u.codes.insert(C_PFX.into(), CodeData { standalone: Some(pfx.into()), ..Default::default() });
    u.context = ctx;
    u
}

fn hidden_extra(ctx: &mut UnitContext, hidden: Option<&str>) {
    if let Some(k) = hidden {
        ctx.notes.insert(0, "hidden".into());
        ctx.extra.insert("hidden".into(), k.into());
    }
}

impl Extractor<'_> {
    fn sheet_cell(&self, sh: &SheetInfo, cell: &str) -> String {
        location(&sh.name, cell)
    }

    fn comments_for(&self, sh: &SheetInfo, cell: &str) -> Vec<String> {
        self.notes.get(&(sh.idx, cell.to_ascii_uppercase())).cloned().unwrap_or_default()
    }

    fn sheet_ctx(&self, ns: &Ns, root: &El) -> SheetCtx {
        let mut ctx = SheetCtx::default();
        for c in root.elements() {
            if ns.is(&c.name, "cols") {
                for col in c.elements().filter(|x| ns.is(&x.name, "col")) {
                    if is_true(col.attr("hidden")) {
                        let min = col.attr("min").and_then(|v| v.parse().ok()).unwrap_or(0);
                        let max = col.attr("max").and_then(|v| v.parse().ok()).unwrap_or(min);
                        ctx.hidden_cols.push((min, max));
                    }
                }
            } else if ns.is(&c.name, "dataValidations") {
                for dv in c.elements().filter(|x| ns.is(&x.name, "dataValidation")) {
                    if dv.attr("type").as_deref() != Some("textLength") {
                        continue;
                    }
                    let op = dv.attr("operator").unwrap_or_else(|| "between".into());
                    let num = |name: &str| {
                        dv.elements()
                            .find(|x| ns.is(&x.name, name))
                            .and_then(|x| x.text().trim().parse::<usize>().ok())
                    };
                    let max = match op.as_str() {
                        "lessThanOrEqual" | "equal" => num("formula1"),
                        "lessThan" => num("formula1").map(|n| n.saturating_sub(1)),
                        "between" => num("formula2"),
                        _ => None,
                    };
                    if let (Some(m), Some(sq)) = (max, dv.attr("sqref")) {
                        ctx.validations.push((sq, m));
                    }
                }
            }
        }
        ctx
    }

    fn process_sheet(&mut self, sh: &SheetInfo, nodes: &mut [Node]) {
        let Some(root) = root_mut(nodes) else { return };
        let nodes_for_ns = [Node::El(El { name: root.name.clone(), start: root.start.clone(), ..Default::default() })];
        let ns = Ns::detect(&nodes_for_ns, SS_URIS, "");
        let ctx = self.sheet_ctx(&ns, root);
        let mut row_no = 0u32;
        for child in root.children.iter_mut() {
            let Node::El(e) = child else { continue };
            if ns.is(&e.name, "sheetData") {
                for row in e.children.iter_mut() {
                    let Node::El(row) = row else { continue };
                    if !ns.is(&row.name, "row") {
                        continue;
                    }
                    let row_hidden = is_true(row.attr("hidden"));
                    // `r` is optional in the file format: a row without it follows the previous row, a cell
                    // without it follows the previous cell.
                    row_no = row.attr("r").and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(row_no + 1);
                    let mut col_no = 0u32;
                    for cell in row.children.iter_mut() {
                        if let Node::El(c) = cell {
                            if ns.is(&c.name, "c") {
                                col_no = match c.attr("r").and_then(|r| parse_ref(&r)) {
                                    Some((col, _)) => col,
                                    None => col_no + 1,
                                };
                                let name = match c.attr("r") {
                                    Some(r) if parse_ref(&r).is_some() => r,
                                    _ => cell_name(col_no, row_no),
                                };
                                self.cell(&ns, sh, row_hidden, c, &ctx, &name);
                            }
                        }
                    }
                }
            } else if ns.is(&e.name, "headerFooter") {
                self.header_footer(&ns, sh, e);
            }
        }
        if self.xo.translate_sheet_names {
            self.rewrite_formulas(&mut root.children);
        }
    }

    fn cell(&mut self, ns: &Ns, sh: &SheetInfo, row_hidden: bool, c: &mut El, ctx: &SheetCtx, r: &str) {
        let r = r.to_string();
        let Some((col, row)) = parse_ref(&r) else { return };
        if c.elements().any(|e| ns.is(&e.name, "f")) {
            return;
        }
        let t = c.attr("t").unwrap_or_default();
        let hidden: Option<&'static str> = if sh.hidden {
            Some("sheet")
        } else if row_hidden {
            Some("row")
        } else if ctx.col_hidden(col) {
            Some("column")
        } else {
            None
        };
        let max_len = ctx.max_len(col, row);
        let loc = self.sheet_cell(sh, &r);
        match t.as_str() {
            "s" => {
                let Some(idx) = c
                    .elements()
                    .find(|e| ns.is(&e.name, "v"))
                    .and_then(|e| e.text().trim().parse::<usize>().ok())
                else {
                    return;
                };
                let notes = self.comments_for(sh, &r);
                let u = self.uses.entry(idx).or_insert_with(|| Use {
                    loc,
                    all_hidden: true,
                    ..Default::default()
                });
                u.count += 1;
                if hidden.is_none() {
                    u.all_hidden = false;
                } else if u.hidden_kind.is_none() {
                    u.hidden_kind = hidden;
                }
                if let Some(m) = max_len {
                    u.max_len = Some(u.max_len.map_or(m, |o| o.min(m)));
                }
                for n in notes {
                    if !u.notes.contains(&n) {
                        u.notes.push(n);
                    }
                }
                self.cell_refs.insert((sh.idx, (col, row)), CellRef::Sst(idx));
            }
            "inlineStr" => {
                let notes = self.comments_for(sh, &r);
                for child in c.children.iter_mut() {
                    let Node::El(is) = child else { continue };
                    if !ns.is(&is.name, "is") {
                        continue;
                    }
                    let (atoms, _) = si_atoms(ns, &is.children);
                    if !has_text(&atoms) || (self.xo.skip_hidden && hidden.is_some()) {
                        return;
                    }
                    let id = format!("is{}-{}", sh.idx, r);
                    let built = build(atoms, props_of);
                    let mut cx = UnitContext {
                        kind: Some("cell".into()),
                        location: Some(loc),
                        max_len,
                        notes,
                        ..Default::default()
                    };
                    hidden_extra(&mut cx, hidden);
                    let orig = raw_children(is);
                    self.u_inline.push(make_unit(id.clone(), built, orig, "si", &ns.pfx, cx));
                    is.children = vec![marker(&id)];
                    self.cell_refs.insert((sh.idx, (col, row)), CellRef::Inline(id));
                    return;
                }
            }
            _ => {}
        }
    }

    fn header_footer(&mut self, ns: &Ns, sh: &SheetInfo, e: &mut El) {
        const NAMES: [&str; 6] = ["oddHeader", "oddFooter", "evenHeader", "evenFooter", "firstHeader", "firstFooter"];
        for child in e.children.iter_mut() {
            let Node::El(h) = child else { continue };
            let Some(name) = NAMES.iter().find(|n| ns.is(&h.name, n)) else {
                continue;
            };
            let atoms = hf_atoms(&h.text());
            if !has_text(&atoms) || (self.xo.skip_hidden && sh.hidden) {
                continue;
            }
            let id = format!("sh{}-{}", sh.idx, name);
            let kind = if name.ends_with("Header") { "header" } else { "footer" };
            let which = name.trim_end_matches("Header").trim_end_matches("Footer");
            let built = build(atoms, props_of);
            let mut cx = UnitContext {
                kind: Some(kind.into()),
                style: Some((*name).into()),
                location: Some(format!("{} {kind} ({which})", sh.name)),
                max_len: Some(255),
                ..Default::default()
            };
            hidden_extra(&mut cx, sh.hidden.then_some("sheet"));
            let orig = raw_children(h);
            self.u_hf.push(make_unit(id.clone(), built, orig, "hf", &ns.pfx, cx));
            h.children = vec![marker(&id)];
        }
    }

    /// Sheet names inside formulas and defined names become markers, so a renamed sheet keeps its references.
    fn rewrite_formulas(&self, nodes: &mut [Node]) {
        for n in nodes.iter_mut() {
            let Node::El(e) = n else { continue };
            let l = local_name(&e.name);
            let plain = e.children.iter().all(|c| matches!(c, Node::Text(_)));
            if matches!(l, "f" | "formula" | "formula1" | "formula2" | "definedName") && plain && !e.name.contains("xm:") {
                let raw = raw_children(e);
                if let Some(t) = self.rewrite_refs(&raw) {
                    e.children = vec![Node::Text(t)];
                }
            } else {
                self.rewrite_formulas(&mut e.children);
            }
        }
    }

    fn rewrite_refs(&self, raw: &str) -> Option<String> {
        let cs: Vec<char> = raw.chars().collect();
        let mut out = String::new();
        let mut last = 0usize;
        let mut changed = false;
        for k in 0..cs.len() {
            if cs[k] != '!' {
                continue;
            }
            let (start, token_raw, name): (usize, String, String) = if k > 0 && cs[k - 1] == '\'' {
                let close = k - 1;
                let mut p = close;
                let open = loop {
                    if p == 0 {
                        break None;
                    }
                    p -= 1;
                    if cs[p] == '\'' {
                        if p > 0 && cs[p - 1] == '\'' {
                            p -= 1;
                            continue;
                        }
                        break Some(p);
                    }
                };
                let Some(open) = open else { continue };
                let inner: String = cs[open + 1..close].iter().collect();
                (open, cs[open..=close].iter().collect(), unescape(&inner.replace("''", "'")))
            } else {
                let mut p = k;
                while p > 0 && (cs[p - 1].is_alphanumeric() || cs[p - 1] == '_' || cs[p - 1] == '.') {
                    p -= 1;
                }
                if p == k {
                    continue;
                }
                let tok: String = cs[p..k].iter().collect();
                (p, tok.clone(), tok)
            };
            if start < last {
                continue;
            }
            let Some((_, id)) = self.sheet_units.iter().find(|(n, _)| *n == name) else {
                continue;
            };
            out.extend(cs[last..start].iter());
            out.push(M_SREF);
            out.push_str(id);
            out.push(M_END);
            out.push_str(&token_raw);
            out.push(M_ORIG);
            last = k;
            changed = true;
        }
        if !changed {
            return None;
        }
        out.extend(cs[last..].iter());
        Some(out)
    }

    /// Comments of one sheet: notes for the cell units, and units when `translate_comments` is on.
    fn comments(&mut self, sh: &SheetInfo, nodes: &mut [Node], threaded: bool, persons: &HashMap<String, String>) {
        let Some(root) = root_mut(nodes) else { return };
        let probe = [Node::El(El { name: root.name.clone(), start: root.start.clone(), ..Default::default() })];
        let ns = if threaded {
            Ns { pfx: root.name.rsplit_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default() }
        } else {
            Ns::detect(&probe, SS_URIS, "")
        };
        if threaded {
            let mut k = 0;
            for c in root.children.iter_mut() {
                let Node::El(tc) = c else { continue };
                if local_name(&tc.name) != "threadedComment" {
                    continue;
                }
                k += 1;
                let Some(cell) = tc.attr("ref") else { continue };
                let author = tc.attr("personId").and_then(|p| persons.get(&p).cloned());
                let pfx = tc.name.rsplit_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
                for t in tc.children.iter_mut() {
                    let Node::El(te) = t else { continue };
                    if local_name(&te.name) != "text" {
                        continue;
                    }
                    let body = te.text();
                    self.add_note(sh, &cell, author.as_deref(), body.trim());
                    if self.xo.translate_comments && body.chars().any(|c| c.is_alphabetic()) {
                        let id = format!("threaded{}-{}-{}", sh.idx, cell, k);
                        let atoms = vec![Atom::Text { fmt: String::new(), key: String::new(), text: body.clone() }];
                        let cx = UnitContext {
                            kind: Some("comment".into()),
                            location: Some(self.sheet_cell(sh, &cell)),
                            notes: vec![],
                            ..Default::default()
                        };
                        self.u_comments.push(make_unit(id.clone(), build(atoms, props_of), raw_children(te), "plain", &pfx, cx));
                        te.children = vec![marker(&id)];
                    }
                }
            }
            return;
        }
        let mut authors: Vec<String> = vec![];
        for c in root.elements() {
            if ns.is(&c.name, "authors") {
                authors = c.elements().map(|a| a.text()).collect();
            }
        }
        for list in root.children.iter_mut() {
            let Node::El(list) = list else { continue };
            if !ns.is(&list.name, "commentList") {
                continue;
            }
            for c in list.children.iter_mut() {
                let Node::El(cm) = c else { continue };
                if !ns.is(&cm.name, "comment") {
                    continue;
                }
                let Some(cell) = cm.attr("ref") else { continue };
                let author = cm.attr("authorId").and_then(|a| a.parse::<usize>().ok()).and_then(|i| authors.get(i).cloned());
                for t in cm.children.iter_mut() {
                    let Node::El(te) = t else { continue };
                    if !ns.is(&te.name, "text") {
                        continue;
                    }
                    let body = plain_of(&ns, &te.children);
                    if body.starts_with("[Threaded comment]") {
                        continue;
                    }
                    self.add_note(sh, &cell, author.as_deref(), body.trim());
                    let (atoms, _) = si_atoms(&ns, &te.children);
                    if self.xo.translate_comments && has_text(&atoms) {
                        let id = format!("comment{}-{}", sh.idx, cell);
                        let cx = UnitContext {
                            kind: Some("comment".into()),
                            location: Some(self.sheet_cell(sh, &cell)),
                            ..Default::default()
                        };
                        self.u_comments.push(make_unit(id.clone(), build(atoms, props_of), raw_children(te), "si", &ns.pfx, cx));
                        te.children = vec![marker(&id)];
                    }
                }
            }
        }
    }

    fn add_note(&mut self, sh: &SheetInfo, cell: &str, author: Option<&str>, body: &str) {
        if body.is_empty() {
            return;
        }
        let note = match author {
            Some(a) if !a.is_empty() => format!("Comment ({a}): {body}"),
            _ => format!("Comment: {body}"),
        };
        self.notes.entry((sh.idx, cell.to_ascii_uppercase())).or_default().push(note);
    }
}

fn split_template(t: &str, idx: &HashMap<&str, usize>) -> Result<Vec<SkelPart>, Error> {
    let mut parts = Vec::new();
    let mut rest = t;
    while let Some(p) = rest.find(M_UNIT) {
        if p > 0 {
            parts.push(SkelPart::Raw(rest[..p].to_string()));
        }
        let after = &rest[p + M_UNIT.len_utf8()..];
        let end = after.find(M_END).ok_or_else(|| perr("internal: unterminated unit marker"))?;
        let id = &after[..end];
        let i = *idx.get(id).ok_or_else(|| perr(format!("internal: unknown unit {id}")))?;
        parts.push(SkelPart::Unit(i));
        rest = &after[end + M_END.len_utf8()..];
    }
    if !rest.is_empty() {
        parts.push(SkelPart::Raw(rest.to_string()));
    }
    Ok(parts)
}

pub fn extract_doc(bytes: &[u8], opts: &ExtractOptions, xo: &XlsxOptions) -> Result<Document, Error> {
    let entries = zipio::read_zip(bytes, &ZipLimits::default())?;
    let ct = find_entry(&entries, "[Content_Types].xml")
        .ok_or_else(|| perr("not an Excel workbook: no [Content_Types].xml"))?
        .1;
    let wb_name = workbook_part(&ct.data).unwrap_or_else(|| "xl/workbook.xml".into());
    let (wb_i, wb_e) = find_entry(&entries, &wb_name).ok_or_else(|| perr("not an Excel workbook: no workbook part"))?;
    let wb_text = std::str::from_utf8(&wb_e.data).map_err(|_| perr("the workbook part is not UTF-8 XML"))?;
    if has_marker_chars(wb_text) {
        return Err(Error::Unsupported("the workbook uses private-use characters U+E000 to U+E005".into()));
    }
    let mut wb_nodes = parse(wb_text).map_err(|m| perr(format!("{wb_name}: {m}")))?;
    let wb_ns = Ns::detect(&wb_nodes, SS_URIS, "");
    let rels = read_rels(&entries, &wb_name);

    // Sheets in workbook order.
    let mut sheets: Vec<SheetInfo> = vec![];
    let mut all_names: Vec<(usize, String, bool)> = vec![];
    {
        let root = wb_nodes
            .iter()
            .find_map(|n| if let Node::El(e) = n { Some(e) } else { None })
            .ok_or_else(|| perr("empty workbook"))?;
        for c in root.elements().filter(|c| wb_ns.is(&c.name, "sheets")) {
            for (k, s) in c.elements().filter(|s| wb_ns.is(&s.name, "sheet")).enumerate() {
                let idx = k + 1;
                let name = s.attr("name").unwrap_or_default();
                let hidden = matches!(s.attr("state").as_deref(), Some("hidden") | Some("veryHidden"));
                all_names.push((idx, name.clone(), hidden));
                let rid = s.attrs().into_iter().find(|(k, _)| local_name(k) == "id" && k.contains(':') && !k.starts_with("xml")).map(|(_, v)| v);
                let Some(rel) = rid.and_then(|r| rels.iter().find(|x| x.id == r)) else {
                    continue;
                };
                if !rel.typ.ends_with("/worksheet") {
                    continue;
                }
                sheets.push(SheetInfo { idx, name, file: rel.target.clone(), hidden });
            }
        }
    }

    let mut ex = Extractor {
        xo,
        sheet_units: vec![],
        notes: HashMap::new(),
        uses: BTreeMap::new(),
        cell_refs: HashMap::new(),
        u_inline: vec![],
        u_hf: vec![],
        u_comments: vec![],
    };

    // Sheet name units.
    let mut u_names: Vec<Unit> = vec![];
    if xo.translate_sheet_names {
        for (idx, name, hidden) in &all_names {
            let id = format!("sheetname-{idx}");
            let mut u = Unit::new(id.clone(), vec![Inline::text(name.clone())]);
            u.codes.insert(C_BASE.into(), CodeData { standalone: Some(String::new()), ..Default::default() });
            u.codes.insert(C_MODE.into(), CodeData { standalone: Some("attr".into()), ..Default::default() });
            u.context = UnitContext {
                kind: Some("sheet-name".into()),
                location: Some(format!("workbook, sheet {idx}")),
                max_len: Some(31),
                ..Default::default()
            };
            hidden_extra(&mut u.context, hidden.then_some("sheet"));
            u_names.push(u);
            ex.sheet_units.push((name.clone(), id));
        }
    }

    // Trees of every part we change.
    let mut trees: HashMap<usize, Vec<Node>> = HashMap::new();

    // Comments and tables hang off sheets.
    let persons: HashMap<String, String> = entries
        .iter()
        .find(|e| e.name.ends_with("persons/person.xml"))
        .and_then(parse_part)
        .map(|nodes| {
            let mut m = HashMap::new();
            fn walk(nodes: &[Node], m: &mut HashMap<String, String>) {
                for n in nodes {
                    if let Node::El(e) = n {
                        if local_name(&e.name) == "person" {
                            if let (Some(id), Some(name)) = (e.attr("id"), e.attr("displayName")) {
                                m.insert(id, name);
                            }
                        }
                        walk(&e.children, m);
                    }
                }
            }
            walk(&nodes, &mut m);
            m
        })
        .unwrap_or_default();
    let mut tables: Vec<(usize, usize)> = vec![]; // (sheet idx, entry index)
    let sheet_list: Vec<(usize, String, String, bool)> =
        sheets.iter().map(|s| (s.idx, s.name.clone(), s.file.clone(), s.hidden)).collect();
    for (idx, name, file, hidden) in &sheet_list {
        let sh = SheetInfo { idx: *idx, name: name.clone(), file: file.clone(), hidden: *hidden };
        for rel in read_rels(&entries, file) {
            let Some((i, e)) = find_entry(&entries, &rel.target) else { continue };
            if rel.typ.ends_with("/table") {
                tables.push((*idx, i));
            } else if rel.typ.ends_with("/comments") || rel.typ.ends_with("/threadedComment") {
                let Some(mut nodes) = parse_part(e) else { continue };
                ex.comments(&sh, &mut nodes, rel.typ.ends_with("/threadedComment"), &persons);
                trees.insert(i, nodes);
            }
        }
    }

    // Sheets.
    for (idx, name, file, hidden) in &sheet_list {
        let sh = SheetInfo { idx: *idx, name: name.clone(), file: file.clone(), hidden: *hidden };
        let Some((i, e)) = find_entry(&entries, file) else { continue };
        let text = std::str::from_utf8(&e.data).map_err(|_| perr(format!("{file} is not UTF-8 XML")))?;
        if has_marker_chars(text) {
            return Err(Error::Unsupported(format!("{file} uses private-use characters U+E000 to U+E005")));
        }
        let mut nodes = parse(text).map_err(|m| perr(format!("{file}: {m}")))?;
        ex.process_sheet(&sh, &mut nodes);
        trees.insert(i, nodes);
    }

    // Shared strings.
    let sst_name = rels
        .iter()
        .find(|r| r.typ.ends_with("/sharedStrings"))
        .map(|r| r.target.clone())
        .unwrap_or_else(|| "xl/sharedStrings.xml".into());
    let mut u_sst: Vec<Unit> = vec![];
    let mut final_ids: HashSet<String> = HashSet::new();
    if let Some((i, e)) = find_entry(&entries, &sst_name) {
        let text = std::str::from_utf8(&e.data).map_err(|_| perr("sharedStrings is not UTF-8 XML"))?;
        if has_marker_chars(text) {
            return Err(Error::Unsupported("sharedStrings uses private-use characters U+E000 to U+E005".into()));
        }
        let mut nodes = parse(text).map_err(|m| perr(format!("{sst_name}: {m}")))?;
        let ns = Ns::detect(&nodes, SS_URIS, "");
        if let Some(root) = root_mut(&mut nodes) {
            let mut n = 0usize;
            for c in root.children.iter_mut() {
                let Node::El(si) = c else { continue };
                if !ns.is(&si.name, "si") {
                    continue;
                }
                let idx = n;
                n += 1;
                let Some(u) = ex.uses.get(&idx) else { continue };
                if xo.skip_hidden && u.all_hidden {
                    continue;
                }
                let (atoms, phonetic) = si_atoms(&ns, &si.children);
                if !has_text(&atoms) {
                    continue;
                }
                let id = format!("ss{idx}");
                let mut built = build(atoms, props_of);
                if phonetic {
                    built.codes.insert("_ph".into(), CodeData::default());
                }
                let mut cx = UnitContext {
                    kind: Some("cell".into()),
                    location: Some(u.loc.clone()),
                    max_len: u.max_len,
                    notes: u.notes.clone(),
                    ..Default::default()
                };
                cx.extra.insert("cells".into(), u.count.to_string());
                if u.all_hidden {
                    hidden_extra(&mut cx, u.hidden_kind);
                }
                u_sst.push(make_unit(id.clone(), built, raw_children(si), "si", &ns.pfx, cx));
                si.children = vec![marker(&id)];
                final_ids.insert(id);
            }
        }
        trees.insert(i, nodes);
    }
    for u in &ex.u_inline {
        final_ids.insert(u.id.clone());
    }

    // Workbook: sheet names and defined names.
    if xo.translate_sheet_names {
        // Units are numbered by sheet position.
        fn sheets_only(nodes: &mut [Node], ns: &Ns, ex: &Extractor) {
            for n in nodes.iter_mut() {
                let Node::El(e) = n else { continue };
                if ns.is(&e.name, "sheets") {
                    let mut pos = 0;
                    for c in e.children.iter_mut() {
                        if let Node::El(s) = c {
                            if ns.is(&s.name, "sheet") {
                                pos += 1;
                                if let Some((_, id)) = ex.sheet_units.get(pos - 1) {
                                    s.start = map_attr(&s.start, "name", |raw| format!("{M_NAME}{id}{M_END}{raw}{M_ORIG}"));
                                }
                            }
                        }
                    }
                } else {
                    sheets_only(&mut e.children, ns, ex);
                }
            }
        }
        sheets_only(&mut wb_nodes, &wb_ns, &ex);
        ex.rewrite_formulas(&mut wb_nodes);
        trees.insert(wb_i, wb_nodes);
    }

    // Tables: column names follow the header cell.
    for (sheet_idx, i) in tables {
        let Some(mut nodes) = parse_part(&entries[i]) else { continue };
        let ns = Ns::detect(&nodes, SS_URIS, "");
        let mut changed = false;
        if let Some(root) = root_mut(&mut nodes) {
            let header_rows = root.attr("headerRowCount").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
            let Some((c1, r1, _, _)) = root.attr("ref").and_then(|r| parse_range(&r)) else { continue };
            if header_rows == 0 {
                continue;
            }
            for cols in root.children.iter_mut() {
                let Node::El(cols) = cols else { continue };
                if !ns.is(&cols.name, "tableColumns") {
                    continue;
                }
                let mut k = 0u32;
                for tc in cols.children.iter_mut() {
                    let Node::El(tc) = tc else { continue };
                    if !ns.is(&tc.name, "tableColumn") {
                        continue;
                    }
                    let cell = (c1 + k, r1);
                    k += 1;
                    let id = match ex.cell_refs.get(&(sheet_idx, cell)) {
                        Some(CellRef::Sst(n)) => format!("ss{n}"),
                        Some(CellRef::Inline(id)) => id.clone(),
                        None => continue,
                    };
                    if !final_ids.contains(&id) {
                        continue;
                    }
                    tc.start = map_attr(&tc.start, "name", |raw| format!("{M_COL}{id}{M_END}{raw}{M_ORIG}"));
                    changed = true;
                }
            }
        }
        if changed {
            trees.insert(i, nodes);
        }
    }

    // Units in a useful order.
    let mut units: Vec<Unit> = vec![];
    units.extend(u_names);
    units.extend(u_sst);
    units.append(&mut ex.u_inline);
    units.append(&mut ex.u_hf);
    units.append(&mut ex.u_comments);
    let idx: HashMap<&str, usize> = units.iter().enumerate().map(|(i, u)| (u.id.as_str(), i)).collect();

    let mut zentries = Vec::with_capacity(entries.len() + 1);
    for (i, e) in entries.iter().enumerate() {
        let data = match trees.get(&i) {
            Some(nodes) => {
                let mut t = String::new();
                for n in nodes {
                    n.write_raw(&mut t);
                }
                if has_marker_chars(&t) {
                    ZipData::Parts(split_template(&t, &idx)?)
                } else {
                    ZipData::Raw(e.data.clone())
                }
            }
            None => ZipData::Raw(e.data.clone()),
        };
        zentries.push(ZipEntry { name: e.name.clone(), data });
    }
    let meta = Meta { entries: entries.iter().map(|e| e.meta.clone()).collect() };
    let meta_json = serde_json::to_vec(&meta).map_err(|e| perr(e.to_string()))?;
    zentries.push(ZipEntry { name: META_NAME.into(), data: ZipData::Raw(meta_json) });

    Ok(Document {
        format: "xlsx".into(),
        name: None,
        source_lang: opts.source_lang.clone(),
        target_lang: opts.target_lang.clone(),
        granularity: Granularity::Paragraph,
        units,
        skeleton: Skeleton::Zip { entries: zentries },
    })
}
