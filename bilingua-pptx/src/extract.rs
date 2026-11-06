//! Extraction: slides, layouts, masters, notes and SmartArt. Every part that holds text is parsed into a
//! lossless tree; the content of a paragraph is replaced by a marker and the tree is written back as the
//! skeleton template. Parts without text are copied as they are.

use crate::rich::*;
use crate::xml::*;
use crate::zipio::{self, RawEntry, ZipLimits};
use crate::*;
use std::collections::{BTreeSet, HashMap};

const A_URIS: &[&str] = &[
    "http://schemas.openxmlformats.org/drawingml/2006/main",
    "http://purl.oclc.org/ooxml/drawingml/main",
];
const P_URIS: &[&str] = &[
    "http://schemas.openxmlformats.org/presentationml/2006/main",
    "http://purl.oclc.org/ooxml/presentationml/main",
];
const MC_URIS: &[&str] = &["http://schemas.openxmlformats.org/markup-compatibility/2006"];

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

fn has_marker_chars(s: &str) -> bool {
    s.chars().any(|c| ('\u{E000}'..='\u{E004}').contains(&c))
}

fn parse_part(e: &RawEntry) -> Option<Vec<Node>> {
    let t = std::str::from_utf8(&e.data).ok()?;
    if has_marker_chars(t) {
        return None;
    }
    parse(t).ok()
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

fn presentation_part(content_types: &[u8]) -> Option<String> {
    let nodes = parse(std::str::from_utf8(content_types).ok()?).ok()?;
    fn walk(nodes: &[Node]) -> Option<String> {
        for n in nodes {
            if let Node::El(e) = n {
                if local_name(&e.name) == "Override" {
                    let ct = e.attr("ContentType").unwrap_or_default();
                    if ct.contains("presentationml.") && ct.ends_with(".main+xml") {
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

fn raw_of(nodes: &[Node]) -> String {
    let mut s = String::new();
    for c in nodes {
        c.write_raw(&mut s);
    }
    s
}

fn marker(id: &str) -> Node {
    Node::Text(format!("{M_UNIT}{id}{M_END}"))
}

fn rid_attr(e: &El) -> Option<String> {
    e.attrs()
        .into_iter()
        .find(|(k, _)| local_name(k) == "id" && k.contains(':') && !k.starts_with("xml"))
        .map(|(_, v)| v)
}

/// Removes attributes from a start tag as written.
pub fn remove_attrs(start: &str, names: &[&str]) -> String {
    let mut s = start.to_string();
    for n in names {
        loop {
            let pat = format!(" {n}=");
            let Some(p) = s.find(&pat) else { break };
            let vs = p + pat.len();
            let Some(q) = s[vs..].chars().next().filter(|c| *c == '"' || *c == '\'') else {
                break;
            };
            let Some(end) = s[vs + 1..].find(q) else { break };
            s.replace_range(p..vs + 1 + end + 1, "");
        }
    }
    s
}

pub fn props_of(fmt: &str) -> BTreeSet<&'static str> {
    let mut s = BTreeSet::new();
    let Ok(nodes) = parse(fmt) else { return s };
    for n in &nodes {
        let Node::El(e) = n else { continue };
        if is_true(e.attr("b")) {
            s.insert("b");
        }
        if is_true(e.attr("i")) {
            s.insert("i");
        }
        if e.attr("u").is_some_and(|u| u != "none") {
            s.insert("u");
        }
        match e.attr("baseline").and_then(|b| b.parse::<i32>().ok()) {
            Some(b) if b > 0 => {
                s.insert("sup");
            }
            Some(b) if b < 0 => {
                s.insert("sub");
            }
            _ => {}
        }
        if e.elements().any(|c| local_name(&c.name) == "hlinkClick") {
            s.insert("link");
        }
    }
    s
}

fn is_true(v: Option<String>) -> bool {
    matches!(v.as_deref(), Some("1") | Some("true"))
}

/// Formatting key: the run properties without spell-check and language marks.
fn fmt_key(rpr: &El) -> String {
    let start = remove_attrs(&rpr.start, &["lang", "altLang", "dirty", "err", "smtClean", "noProof", "bmk"]);
    let mut s = start;
    for c in &rpr.children {
        c.write_raw(&mut s);
    }
    s.push_str(&rpr.end);
    s
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Slide,
    Layout,
    Master,
    Notes,
    Diagram,
    Drawing,
}

struct PartInfo {
    idx: usize,
    kind: Kind,
    stem: String,
    num: u32,
    slide_no: u32,
    layout_name: String,
}

#[derive(Clone, Default)]
struct Shape {
    name: String,
    ph: Option<String>,
    in_ph: bool,
}

#[derive(Clone)]
struct Cell {
    row: usize,
    col: usize,
    table: String,
}

struct Nss {
    a: Ns,
    p: Ns,
    mc: Ns,
}

struct Extractor {
    units: Vec<((u32, u8, u32), Unit)>,
    para_no: usize,
    seq: u32,
    titles: HashMap<u32, String>,
    lang: Option<String>,
    /// Plain text of data paragraphs by (slide, text) for SmartArt drawing mirrors.
    smart: HashMap<(u32, String), String>,
}

fn shape_info(e: &El, parent: &Shape) -> Shape {
    let mut sh = parent.clone();
    let l = local_name(&e.name);
    if !matches!(l, "sp" | "pic" | "graphicFrame" | "cxnSp" | "grpSp") {
        return sh;
    }
    sh.ph = None;
    sh.in_ph = false;
    for c in e.elements() {
        let cl = local_name(&c.name);
        if cl.starts_with("nv") && cl.ends_with("Pr") {
            for x in c.elements() {
                match local_name(&x.name) {
                    "cNvPr" => sh.name = x.attr("name").unwrap_or_default(),
                    "nvPr" => {
                        if let Some(ph) = x.elements().find(|y| local_name(&y.name) == "ph") {
                            sh.ph = Some(ph.attr("type").unwrap_or_else(|| "body".into()));
                            sh.in_ph = true;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    sh
}

impl Extractor {
    fn walk(&mut self, pi: &PartInfo, ns: &Nss, nodes: &mut [Node], shape: &Shape, cell: Option<&Cell>) {
        for n in nodes.iter_mut() {
            let Node::El(e) = n else { continue };
            if ns.mc.is(&e.name, "AlternateContent") {
                for c in e.children.iter_mut() {
                    if let Node::El(ch) = c {
                        if ns.mc.is(&ch.name, "Choice") {
                            self.walk(pi, ns, &mut ch.children, shape, cell);
                        }
                    }
                }
                continue;
            }
            if ns.a.is(&e.name, "p") {
                self.paragraph(pi, ns, e, shape, cell);
                continue;
            }
            if ns.a.is(&e.name, "tbl") {
                self.table(pi, ns, e, shape);
                continue;
            }
            let sh = shape_info(e, shape);
            self.walk(pi, ns, &mut e.children, &sh, cell);
        }
    }

    fn table(&mut self, pi: &PartInfo, ns: &Nss, tbl: &mut El, shape: &Shape) {
        let mut row = 0;
        for tr in tbl.children.iter_mut() {
            let Node::El(tr) = tr else { continue };
            if !ns.a.is(&tr.name, "tr") {
                continue;
            }
            row += 1;
            let mut col = 0;
            for tc in tr.children.iter_mut() {
                let Node::El(tc) = tc else { continue };
                if !ns.a.is(&tc.name, "tc") {
                    continue;
                }
                col += 1;
                // The covered cells of a merged range are not shown; their text is leftover. Paragraph numbers
                // still count them, so the ids of the other units do not depend on this rule.
                let covered = ["hMerge", "vMerge"].iter().any(|a| matches!(tc.attr(a).as_deref(), Some("1") | Some("true")));
                if covered {
                    let n: usize = tc
                        .elements()
                        .filter(|b| ns.a.is(&b.name, "txBody"))
                        .map(|b| b.elements().filter(|p| ns.a.is(&p.name, "p")).count())
                        .sum();
                    self.para_no += n;
                    continue;
                }
                let cell = Cell { row, col, table: shape.name.clone() };
                self.walk(pi, ns, &mut tc.children, shape, Some(&cell));
            }
        }
    }

    fn paragraph(&mut self, pi: &PartInfo, ns: &Nss, p: &mut El, shape: &Shape, cell: Option<&Cell>) {
        self.para_no += 1;
        let n = self.para_no;
        if matches!(pi.kind, Kind::Layout | Kind::Master) && shape.in_ph {
            return;
        }
        let mut atoms: Vec<Atom> = vec![];
        let mut lead = String::new();
        let mut tail: Vec<Node> = vec![];
        let mut orig_nodes: Vec<Node> = vec![];
        for c in p.children.drain(..) {
            match &c {
                Node::El(e) if ns.a.is(&e.name, "endParaRPr") => {
                    tail.push(c);
                    continue;
                }
                Node::El(e) if ns.a.is(&e.name, "pPr") => lead = e.raw(),
                Node::El(e) if ns.a.is(&e.name, "r") => {
                    let mut fmt = String::new();
                    let mut key = String::new();
                    let mut text = String::new();
                    for x in e.elements() {
                        if ns.a.is(&x.name, "rPr") {
                            fmt = x.raw();
                            key = fmt_key(x);
                            if self.lang.is_none() {
                                self.lang = x.attr("lang");
                            }
                        } else if ns.a.is(&x.name, "t") {
                            text.push_str(&x.text());
                        }
                    }
                    atoms.push(Atom::Text { fmt, key, text });
                }
                Node::El(e) if ns.a.is(&e.name, "br") => {
                    atoms.push(Atom::Code { xml: e.raw(), display: "<br/>".into(), label: "line break".into() });
                }
                Node::El(e) if ns.a.is(&e.name, "fld") => {
                    let ty = e.attr("type").unwrap_or_else(|| "field".into());
                    atoms.push(Atom::Code { xml: e.raw(), display: format!("{{{ty}}}"), label: format!("field {ty}") });
                }
                Node::El(e) => {
                    let l = local_name(&e.name).to_string();
                    atoms.push(Atom::Code { xml: e.raw(), display: format!("{{{l}}}"), label: format!("{l} element") });
                }
                _ => {}
            }
            orig_nodes.push(c);
        }
        let translatable = atoms
            .iter()
            .any(|a| matches!(a, Atom::Text { text, .. } if text.chars().any(|c| c.is_alphabetic())));
        if !translatable {
            // Put the paragraph back exactly as it was.
            let mut all = orig_nodes;
            all.extend(tail);
            p.children = all;
            return;
        }
        let id = format!("{}-p{n}", pi.stem);
        let built = build(atoms, props_of);

        let (kind, style) = match (pi.kind, cell, shape.ph.as_deref()) {
            (Kind::Notes, _, _) => ("notes", None),
            (_, Some(_), _) => ("cell", None),
            (_, None, Some("title")) | (_, None, Some("ctrTitle")) => ("title", shape.ph.clone()),
            (_, None, Some("subTitle")) => ("subtitle", shape.ph.clone()),
            (_, None, Some(_)) => ("body", shape.ph.clone()),
            (_, None, None) => ("textbox", None),
        };
        let slide = pi.slide_no;
        let mut cx = UnitContext::new().with_kind(kind.to_string()).with_style(style);
        let shape_part = format!("shape \"{}\"", shape.name);
        let loc = match pi.kind {
            Kind::Slide => format!("slide {slide}, {shape_part}"),
            Kind::Notes => format!("slide {slide}, notes, {shape_part}"),
            Kind::Diagram => format!("slide {slide}, diagram"),
            Kind::Drawing => format!("slide {slide}, diagram"),
            Kind::Layout => format!("layout {} \"{}\", {shape_part}", pi.num, pi.layout_name),
            Kind::Master => format!("master {}, {shape_part}", pi.num),
        };
        cx.location = Some(match cell {
            Some(c) => format!("{loc}, row {}, column {}", c.row, c.col),
            None => loc,
        });
        cx.key = Some(pi.stem.clone());
        let part_name = match pi.kind {
            Kind::Slide => "slide",
            Kind::Notes => "notes",
            Kind::Diagram | Kind::Drawing => "diagram",
            Kind::Layout => "layout",
            Kind::Master => "master",
        };
        cx.extra.insert("part".into(), part_name.into());
        if matches!(pi.kind, Kind::Slide | Kind::Notes | Kind::Diagram) {
            cx.extra.insert("slide".into(), slide.to_string());
            if let Some(t) = self.titles.get(&slide) {
                if kind != "title" {
                    cx.heading = Some(t.clone());
                }
            }
        }
        if let Some(c) = cell {
            cx.extra.insert("table".into(), c.table.clone());
            cx.extra.insert("row".into(), c.row.to_string());
            cx.extra.insert("col".into(), c.col.to_string());
        }
        if kind == "title" && matches!(pi.kind, Kind::Slide) {
            let t: String = built
                .inlines
                .iter()
                .filter_map(|i| if let Inline::Text(t) = i { Some(t.as_str()) } else { None })
                .collect();
            self.titles.entry(slide).or_insert(t.trim().to_string());
        }

        let orig = raw_of(&orig_nodes);
        let mut u = Unit::new(id.clone(), built.inlines);
        u.codes = built.codes;
        u.codes.insert(C_ORIG.into(), CodeData::new().with_standalone(orig));
        u.codes.insert(C_LEAD.into(), CodeData::new().with_standalone(lead));
        u.codes.insert(C_PFX.into(), CodeData::new().with_standalone(ns.a.pfx.clone()));
        u.context = cx;
        let plain = plain_text(&u.source());
        if matches!(pi.kind, Kind::Diagram) {
            self.smart.entry((slide, plain.trim().to_string())).or_insert_with(|| id.clone());
        }
        let rank = match pi.kind {
            Kind::Slide => 0,
            Kind::Diagram | Kind::Drawing => 1,
            Kind::Notes => 2,
            Kind::Master => 3,
            Kind::Layout => 4,
        };
        let key_slide = if matches!(pi.kind, Kind::Layout | Kind::Master) { u32::MAX } else { slide };
        self.seq += 1;
        self.units.push(((key_slide, rank, self.seq), u));
        let mut ch = vec![marker(&id)];
        ch.extend(tail);
        p.children = ch;
    }

    /// SmartArt drawing paragraphs follow the data paragraph with the same text.
    fn drawing_paragraphs(&mut self, pi: &PartInfo, ns: &Nss, nodes: &mut [Node]) {
        for n in nodes.iter_mut() {
            let Node::El(e) = n else { continue };
            if ns.a.is(&e.name, "p") {
                let mut text = String::new();
                for c in e.elements() {
                    if ns.a.is(&c.name, "r") {
                        for t in c.elements().filter(|t| ns.a.is(&t.name, "t")) {
                            text.push_str(&t.text());
                        }
                    }
                }
                let Some(id) = self.smart.get(&(pi.slide_no, text.trim().to_string())).cloned() else {
                    continue;
                };
                let mut keep = vec![];
                let mut orig = vec![];
                for c in e.children.drain(..) {
                    if matches!(&c, Node::El(x) if ns.a.is(&x.name, "endParaRPr")) {
                        keep.push(c);
                    } else {
                        orig.push(c);
                    }
                }
                let mut ch = vec![Node::Text(format!("{M_MIRROR}{id}{M_END}{}{M_ORIG}", raw_of(&orig)))];
                ch.extend(keep);
                e.children = ch;
            } else {
                self.drawing_paragraphs(pi, ns, &mut e.children);
            }
        }
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

fn numbered<'a>(name: &'a str, dir: &str, prefix: &str) -> Option<(&'a str, u32)> {
    let rest = name.strip_prefix(dir)?;
    let stem = rest.strip_suffix(".xml")?;
    let digits = stem.strip_prefix(prefix)?;
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((stem, digits.parse().ok()?))
}

pub fn extract_doc(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let l = opts.limits();
    bilingua_model::guard::check_input(bytes, &l)?;
    bilingua_model::guard::finish(extract_doc_inner(bytes, opts)?, vec![], &l)
}

fn extract_doc_inner(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let entries = zipio::read_zip(bytes, &ZipLimits::from(&opts.limits()))?;
    bilingua_zip::check_xml_parts(&entries, &opts.limits())?;
    let ct = find_entry(&entries, "[Content_Types].xml")
        .ok_or_else(|| perr("not a PowerPoint file: no [Content_Types].xml"))?
        .1;
    let pres = presentation_part(&ct.data).unwrap_or_else(|| "ppt/presentation.xml".into());
    let (_, pres_e) = find_entry(&entries, &pres).ok_or_else(|| perr("not a PowerPoint file: no presentation part"))?;
    let pres_text = std::str::from_utf8(&pres_e.data).map_err(|_| perr("the presentation part is not UTF-8 XML"))?;
    let pres_nodes = parse(pres_text).map_err(|m| perr(format!("{pres}: {m}")))?;
    let prels = read_rels(&entries, &pres);

    // Slide order.
    let mut slide_no: HashMap<String, u32> = HashMap::new();
    {
        let mut n = 0;
        fn walk(nodes: &[Node], f: &mut impl FnMut(&El)) {
            for x in nodes {
                if let Node::El(e) = x {
                    if local_name(&e.name) == "sldId" {
                        f(e);
                    }
                    walk(&e.children, f);
                }
            }
        }
        walk(&pres_nodes, &mut |e| {
            if let Some(rel) = rid_attr(e).and_then(|r| prels.iter().find(|x| x.id == r)) {
                n += 1;
                slide_no.entry(rel.target.clone()).or_insert(n);
            }
        });
    }

    // Parts.
    let mut parts: Vec<PartInfo> = vec![];
    for (i, e) in entries.iter().enumerate() {
        let (kind, stem, num) = if let Some((s, n)) = numbered(&e.name, "ppt/slides/", "slide") {
            (Kind::Slide, s, n)
        } else if let Some((s, n)) = numbered(&e.name, "ppt/slideLayouts/", "slideLayout") {
            (Kind::Layout, s, n)
        } else if let Some((s, n)) = numbered(&e.name, "ppt/slideMasters/", "slideMaster") {
            (Kind::Master, s, n)
        } else if let Some((s, n)) = numbered(&e.name, "ppt/notesSlides/", "notesSlide") {
            (Kind::Notes, s, n)
        } else if let Some((s, n)) = numbered(&e.name, "ppt/diagrams/", "data") {
            (Kind::Diagram, s, n)
        } else if let Some((s, n)) = numbered(&e.name, "ppt/diagrams/", "drawing") {
            (Kind::Drawing, s, n)
        } else {
            continue;
        };
        parts.push(PartInfo { idx: i, kind, stem: stem.to_string(), num, slide_no: 0, layout_name: String::new() });
    }
    // Slides without a place in the slide list come after the others.
    let mut next = slide_no.values().copied().max().unwrap_or(0);
    for p in parts.iter_mut().filter(|p| p.kind == Kind::Slide) {
        let name = &entries[p.idx].name;
        p.slide_no = match slide_no.get(name) {
            Some(n) => *n,
            None => {
                next += 1;
                slide_no.insert(name.clone(), next);
                next
            }
        };
    }
    // Notes, SmartArt data and drawings belong to the slide that points at them. A part that two
    // slides point at goes to the first in presentation order (as the LocOS profile does), so the
    // slides are walked in slide-number order, not in HashMap order.
    let mut owner: HashMap<String, u32> = HashMap::new();
    let mut in_order: Vec<(String, u32)> = slide_no.clone().into_iter().collect();
    in_order.sort_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)));
    for (name, n) in in_order {
        for rel in read_rels(&entries, &name) {
            if rel.typ.ends_with("/notesSlide") || rel.typ.ends_with("/diagramData") || rel.typ.ends_with("/diagramDrawing") {
                owner.entry(rel.target).or_insert(n);
            }
        }
    }
    for p in parts.iter_mut() {
        if matches!(p.kind, Kind::Notes | Kind::Diagram | Kind::Drawing) {
            p.slide_no = owner.get(&entries[p.idx].name).copied().unwrap_or(0);
        }
    }
    // Slides first, then data before drawings, so mirrors can find their data paragraph.
    parts.sort_by_key(|p| (p.kind as u8, p.slide_no, p.num));

    let mut ex = Extractor {
        units: vec![],
        para_no: 0,
        seq: 0,
        titles: HashMap::new(),
        lang: None,
        smart: HashMap::new(),
    };
    let mut trees: HashMap<usize, Vec<Node>> = HashMap::new();
    for pi in parts.iter_mut() {
        let e = &entries[pi.idx];
        let Ok(text) = std::str::from_utf8(&e.data) else {
            if pi.kind == Kind::Slide {
                return Err(perr(format!("{} is not UTF-8 XML", e.name)));
            }
            continue;
        };
        if has_marker_chars(text) {
            if pi.kind == Kind::Slide {
                return Err(Error::Unsupported(format!("{} uses private-use characters U+E000 to U+E004", e.name)));
            }
            continue;
        }
        let mut nodes = match parse(text) {
            Ok(n) => n,
            Err(m) if pi.kind == Kind::Slide => return Err(perr(format!("{}: {m}", e.name))),
            Err(_) => continue,
        };
        let ns = Nss {
            a: Ns::detect(&nodes, A_URIS, "a:"),
            p: Ns::detect(&nodes, P_URIS, "p:"),
            mc: Ns::detect(&nodes, MC_URIS, "mc:"),
        };
        let _ = &ns.p;
        if pi.kind == Kind::Layout {
            if let Some(Node::El(root)) = nodes.iter().find(|n| matches!(n, Node::El(_))) {
                if let Some(c) = root.elements().find(|c| local_name(&c.name) == "cSld") {
                    pi.layout_name = c.attr("name").unwrap_or_default();
                }
            }
        }
        ex.para_no = 0;
        if pi.kind == Kind::Drawing {
            ex.drawing_paragraphs(pi, &ns, &mut nodes);
        } else {
            ex.walk(pi, &ns, &mut nodes, &Shape::default(), None);
        }
        trees.insert(pi.idx, nodes);
    }

    ex.units.sort_by_key(|(k, _)| *k);
    let units: Vec<Unit> = ex.units.into_iter().map(|(_, u)| u).collect();
    let idx: HashMap<&str, usize> = units.iter().enumerate().map(|(i, u)| (u.id.as_str(), i)).collect();

    let mut zentries = Vec::with_capacity(entries.len() + 1);
    let mut packed = std::collections::BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        let data = match trees.get(&i) {
            Some(nodes) => {
                let t = raw_of(nodes);
                if has_marker_chars(&t) {
                    ZipData::Parts(split_template(&t, &idx)?)
                } else {
                    ZipData::Raw(e.keep(&mut packed))
                }
            }
            None => ZipData::Raw(e.keep(&mut packed)),
        };
        zentries.push(ZipEntry::new(e.name.clone(), data));
    }
    let meta = Meta { entries: entries.iter().map(|e| e.meta.clone()).collect(), packed };
    let meta_json = serde_json::to_vec(&meta).map_err(|e| perr(e.to_string()))?;
    zentries.push(ZipEntry::raw(META_NAME, meta_json));

    Ok(Document::new("pptx", units, Skeleton::zip(zentries, bilingua_model::parser_info!())).with_source_lang(opts.source_lang.clone().or(ex.lang)).with_target_lang(opts.target_lang.clone()).with_granularity(Granularity::Paragraph))
}
