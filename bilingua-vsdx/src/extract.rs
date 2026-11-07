//! Extraction. The walk is LocOS's: page names, then for each page (in `pages.xml` order, resolved through the
//! relationships) every shape `<Text>` and every shape data label, in document order; comments last when asked.
//! Each walked part is kept as a template: the original markup with a marker where a unit's text goes.

use crate::xml::*;
use crate::*;
use bilingua_zip::RawEntry;
use std::collections::{BTreeMap, HashMap, HashSet};

const PAGES: &str = "visio/pages/pages.xml";
const PAGES_RELS: &str = "visio/pages/_rels/pages.xml.rels";
const DOCUMENT_RELS: &str = "visio/_rels/document.xml.rels";
const COMMENTS: &str = "visio/comments.xml";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub const STYLE_PAGE: &str = "Visio page name";
pub const STYLE_TEXT: &str = "Visio shape text";
pub const STYLE_LABEL: &str = "Visio shape data label";
pub const STYLE_COMMENT: &str = "comment";

fn perr(msg: impl Into<String>) -> Error {
    crate::perr(msg)
}

fn page_part_number(name: &str) -> Option<u64> {
    let rest = name.strip_prefix("visio/pages/page")?.strip_suffix(".xml")?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(rest.parse().unwrap_or(u64::MAX))
}

pub(crate) fn normpath(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for s in p.split('/') {
        match s {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|l| *l != "..") {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// A part as text: UTF-8 (with or without BOM), UTF-16 with a BOM, or a legacy charset the XML declaration names.
/// Anything else is refused, as the XML specification and LocOS do. Returns the text and the encoding tag to
/// write it back with (`None` for plain UTF-8).
pub(crate) fn decode_part(name: &str, data: &[u8], opts: &ExtractOptions) -> Result<(String, Option<String>), Error> {
    use bilingua_model::enc;
    let (body, bom) = match data.strip_prefix(&[0xEF, 0xBB, 0xBF][..]) {
        Some(r) => (r, true),
        None => (data, false),
    };
    let declared_legacy = enc::declared_encoding(data)
        .filter(|e| *e != bilingua_model::encoding_rs::UTF_8 && *e != bilingua_model::encoding_rs::UTF_16LE && *e != bilingua_model::encoding_rs::UTF_16BE);
    if let Ok(s) = std::str::from_utf8(body) {
        let ascii_legacy = declared_legacy.is_some() && !bom && s.is_ascii();
        if !ascii_legacy {
            return Ok((s.to_string(), bom.then(|| "utf-8-bom".to_string())));
        }
    }
    let utf16 = data.starts_with(&[0xFF, 0xFE]) || data.starts_with(&[0xFE, 0xFF]);
    if !utf16 && declared_legacy.is_none() {
        return Err(perr(format!("{name} is not valid XML (it is not UTF-8 and declares no other encoding)")));
    }
    let o = enc::DecodeOptions::new(opts).declared();
    let d = enc::decode_text(data, &o).map_err(|e| match e {
        Error::Encoding(m) => perr(format!("{name} is not valid XML ({m})")),
        e => e,
    })?;
    Ok((d.text, (d.encoding != "utf-8").then_some(d.encoding)))
}

fn parse_part(name: &str, text: &str, depth: usize) -> Result<Parsed, Error> {
    xml::parse(text, depth).map_err(|m| {
        if m.contains("DOCTYPE") {
            perr(format!("This Visio file declares a DOCTYPE in {name}. It was not processed."))
        } else {
            perr(format!("This Visio file has a part that is not valid XML ({name}): {m}"))
        }
    })
}

fn root_of(nodes: &[Node]) -> Option<&El> {
    nodes.iter().find_map(|n| if let Node::El(e) = n { Some(e) } else { None })
}

/// What the skeleton does at one element.
enum Edit {
    /// An attribute value is the unit (page `Name`, label `V`); `nameu` adds `NameU` before the tag ends.
    Attr { unit: usize, name: &'static str, nameu: bool },
    /// The children of this element are the unit.
    Children { unit: usize },
    /// A shape with a text unit: the region from its start tag to the `Text` child (`text_id`) is transformed
    /// at merge when the unit is translated.
    Shape { unit: usize, text_id: u32 },
}

struct Emitter {
    parts: Vec<SkelPart>,
    cur: String,
}

impl Emitter {
    fn unit(&mut self, i: usize) {
        if !self.cur.is_empty() {
            self.parts.push(SkelPart::Raw(std::mem::take(&mut self.cur)));
        }
        self.parts.push(SkelPart::Unit(i));
    }
    fn finish(mut self) -> Vec<SkelPart> {
        if !self.cur.is_empty() {
            self.parts.push(SkelPart::Raw(self.cur));
        }
        self.parts
    }
}

fn emit(nodes: &[Node], edits: &HashMap<u32, Edit>, em: &mut Emitter) {
    for n in nodes {
        match n {
            Node::Text(s) | Node::Raw(s) => em.cur.push_str(s),
            Node::El(e) => emit_el(e, edits, em),
        }
    }
}

fn emit_el(e: &El, edits: &HashMap<u32, Edit>, em: &mut Emitter) {
    match edits.get(&e.id) {
        None => {
            em.cur.push_str(&e.start);
            emit(&e.children, edits, em);
            em.cur.push_str(&e.end);
        }
        Some(Edit::Attr { unit, name, nameu }) => {
            let a = find_attr(&e.start, name).expect("attribute checked at extraction");
            em.cur.push_str(&e.start[..a.vs]);
            em.unit(*unit);
            let close = if e.start.ends_with("/>") { e.start.len() - 2 } else { e.start.len() - 1 };
            let at = if *nameu { e.start[..close].trim_end().len() } else { a.ve };
            em.cur.push_str(&e.start[a.ve..at]);
            if *nameu {
                em.cur.push_str(&format!("{M_OPEN}N{unit}{M_END}"));
                em.cur.push_str(&e.start[at..]);
            } else {
                em.cur.push_str(&e.start[at..]);
            }
            emit(&e.children, edits, em);
            em.cur.push_str(&e.end);
        }
        Some(Edit::Children { unit }) => {
            em.cur.push_str(&e.start);
            em.unit(*unit);
            em.cur.push_str(&e.end);
        }
        Some(Edit::Shape { unit, text_id }) => {
            em.cur.push_str(&e.start);
            em.cur.push_str(&format!("{M_OPEN}B{unit}{M_END}"));
            for c in &e.children {
                if let Node::El(ch) = c {
                    if ch.id == *text_id {
                        em.cur.push_str(&format!("{M_OPEN}E{M_END}"));
                    }
                }
                match c {
                    Node::Text(s) | Node::Raw(s) => em.cur.push_str(s),
                    Node::El(ch) => emit_el(ch, edits, em),
                }
            }
            em.cur.push_str(&e.end);
        }
    }
}

// ------------------------------------------------------------------------------------------- run content

/// A piece of the content of a `Text` element, in order.
enum Item<'a> {
    Str { raw: &'a str, dec: String, cdata: bool },
    Mark(&'a El),
    Other(&'a str),
}

impl Item<'_> {
    fn raw(&self) -> &str {
        match self {
            Item::Str { raw, .. } => raw,
            Item::Other(r) => r,
            Item::Mark(_) => "",
        }
    }
}

fn items_of(children: &[Node]) -> Vec<Item<'_>> {
    children
        .iter()
        .map(|c| match c {
            Node::Text(raw) => Item::Str { raw, dec: decode(raw, false), cdata: raw.starts_with("<![CDATA[") },
            Node::El(e) => Item::Mark(e),
            Node::Raw(r) => Item::Other(r),
        })
        .collect()
}

/// Byte offsets of the start of every decoded character of a plain (non-CDATA) text node, plus its length.
fn atoms(raw: &str) -> Vec<usize> {
    let b = raw.as_bytes();
    let mut v = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < b.len() {
        v.push(i);
        if b[i] == b'&' {
            i += raw[i..].find(';').map(|p| p + 1).unwrap_or(1);
        } else if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') {
            i += 2;
        } else {
            i += raw[i..].chars().next().map(char::len_utf8).unwrap_or(1);
        }
    }
    v.push(b.len());
    v
}

pub(crate) struct Core {
    pub pre: String,
    pub suf: String,
    /// Core pieces: text, or a marker element (its raw markup).
    pub parts: Vec<CorePart>,
}

pub(crate) enum CorePart {
    Text(String),
    Mark(String),
}

/// Splits the content of a run-holding element into prefix, core and suffix as LocOS `_split_layout` does.
/// `fields_visible`: a `fld` marker counts as visible content (shape text yes, comments never have markers).
fn split_core(children: &[Node]) -> Option<Core> {
    let items = items_of(children);
    let visible = |it: &Item| match it {
        Item::Str { dec, .. } => dec.chars().any(|c| !py_space(c)),
        Item::Mark(e) => e.local() == "fld",
        Item::Other(_) => false,
    };
    let first = items.iter().position(visible)?;
    let last = items.iter().rposition(visible)?;
    let mut pre = String::new();
    let mut suf = String::new();
    for it in &items[..first] {
        match it {
            Item::Mark(e) => e.write_raw(&mut pre),
            o => pre.push_str(o.raw()),
        }
    }
    let mut parts = Vec::new();
    for (k, it) in items.iter().enumerate().take(last + 1).skip(first) {
        match it {
            Item::Str { raw, dec, cdata } => {
                let chars: Vec<char> = dec.chars().collect();
                let lead = if k == first && !cdata { chars.iter().take_while(|c| py_space(**c)).count() } else { 0 };
                let trail = if k == last && !cdata { chars.iter().rev().take_while(|c| py_space(**c)).count() } else { 0 };
                let (a, b);
                if lead > 0 || trail > 0 {
                    let at = atoms(raw);
                    a = at[lead];
                    b = at[at.len() - 1 - trail];
                    pre.push_str(&raw[..a]);
                    suf.insert_str(0, &raw[b..]);
                } else {
                    a = 0;
                    b = raw.len();
                }
                let _ = (a, b);
                let core: String = chars[lead..chars.len() - trail].iter().collect();
                if !core.is_empty() {
                    parts.push(CorePart::Text(core));
                }
            }
            Item::Mark(e) => parts.push(CorePart::Mark(e.raw())),
            Item::Other(_) => {}
        }
    }
    for it in &items[last + 1..] {
        match it {
            Item::Mark(e) => e.write_raw(&mut suf),
            o => suf.push_str(o.raw()),
        }
    }
    Some(Core { pre, suf, parts })
}

fn translatable(s: &str) -> bool {
    bilingua_model::py_has_alnum(s)
}

fn mark_label(raw: &str) -> &'static str {
    let name = raw[1..].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or("");
    match local_name(name) {
        "cp" => "character properties",
        "pp" => "paragraph properties",
        "tp" => "tab properties",
        "fld" => "field",
        _ => "marker",
    }
}

// ----------------------------------------------------------------------------------------------- the walk

struct Part {
    name: String,
    enc: Option<String>,
    parsed: Parsed,
    edits: HashMap<u32, Edit>,
    changed: bool,
}

struct Builder {
    units: Vec<Unit>,
    max_units: usize,
    slots: usize,
    /// Under the LocOS profile: record LocOS's p_index (the slot's index, empty and untranslatable slots counted)
    locos: bool,
}

/// LocOS `office_comments.COMMENT_BASE`: the p_index of the first comment entry.
const COMMENT_BASE: usize = 1_000_000;

impl Builder {
    /// Under the LocOS profile, `locos.p_index` = the current slot's index (LocOS `_walk` numbers every page name,
    /// shape text and data label, so the numbers have gaps).
    fn mark(&self, u: &mut Unit, p_index: usize) {
        if self.locos {
            u.context.extra.insert(bilingua_model::profile::P_INDEX.to_string(), p_index.to_string());
        }
    }
}

impl Builder {
    fn slot(&mut self) -> Result<(), Error> {
        self.slots += 1;
        if self.slots > self.max_units {
            return Err(Error::Limit(bilingua_model::LimitError::new(
                LimitKind::Units,
                format!("This Visio file has more than {} segments. Split it into smaller files.", self.max_units),
            )));
        }
        Ok(())
    }
}

fn code(s: impl Into<String>) -> CodeData {
    CodeData::standalone(s)
}

fn r_id(rel: &El, anc: &[&El]) -> Option<String> {
    for (k, v) in attrs_of(&rel.start) {
        if local_name(&k) == "id" && k.contains(':') {
            let p = prefix_of(&k);
            let key = format!("xmlns:{p}");
            let bound = anc.iter().rev().chain(std::iter::once(&rel)).any(|a| a.attr(&key).as_deref() == Some(R_NS));
            let _ = &v;
            if bound {
                return Some(v);
            }
        }
    }
    None
}

fn find_entry<'a>(entries: &'a [RawEntry], name: &str) -> Option<(usize, &'a RawEntry)> {
    entries.iter().enumerate().find(|(_, e)| e.name == name)
}

struct Rels(Vec<(String, String, String)>); // (Id, Type, Target)

fn read_rels(name: &str, e: &RawEntry, opts: &ExtractOptions, depth: usize) -> Result<Rels, Error> {
    let (text, _) = decode_part(name, &e.data, opts)?;
    let p = parse_part(name, &text, depth)?;
    let mut out = Vec::new();
    if let Some(root) = root_of(&p.nodes) {
        for rel in root.elements() {
            if rel.local() == "Relationship" && rel.attr("TargetMode").as_deref() != Some("External") {
                out.push((rel.attr("Id").unwrap_or_default(), rel.attr("Type").unwrap_or_default(), rel.attr("Target").unwrap_or_default()));
            }
        }
    }
    Ok(Rels(out))
}

pub(crate) fn extract_doc(bytes: &[u8], opts: &ExtractOptions, with_comments: bool) -> Result<Document, Error> {
    let l = opts.limits();
    bilingua_model::guard::check_input(bytes, &l)?;
    if let Some(name) = crate::zipdir::duplicate_name(bytes) {
        return Err(perr(format!("This Visio file lists the member {name:?} twice. It was not processed.")));
    }
    let doc = extract_inner(bytes, opts, with_comments)?;
    bilingua_model::guard::finish(doc, vec![], &l)
}

fn extract_inner(bytes: &[u8], opts: &ExtractOptions, with_comments: bool) -> Result<Document, Error> {
    let l = opts.limits();
    let depth = l.max_xml_depth.min(DEFAULT_MAX_DEPTH_CAP);
    let entries = bilingua_zip::read_zip("vsdx", bytes, &ZipLimits::from(&l))?;
    bilingua_zip::check_xml_parts(&entries, &l)?;
    let names: HashSet<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    if !names.contains(PAGES) && !entries.iter().any(|e| page_part_number(&e.name).is_some()) {
        return Err(perr("This Visio file has no drawing pages (visio/pages/)."));
    }
    // a pre-parse markup budget over the page parts, forty markup characters per allowed segment
    let mut lt = 0usize;
    for e in &entries {
        if e.name.starts_with("visio/pages/") && e.name.ends_with(".xml") {
            lt += e.data.iter().filter(|b| **b == b'<').count();
            if lt > 40 * l.max_units {
                return Err(Error::Limit(bilingua_model::LimitError::new(
                    LimitKind::XmlElements,
                    format!("This Visio file has far more shapes or markup than the limit allows ({} segments). Split it into smaller files.", l.max_units),
                )));
            }
        }
    }
    let mut b = Builder { units: vec![], max_units: l.max_units, slots: 0, locos: opts.is_profile(bilingua_model::profile::LOCOS) };
    let mut parts: Vec<Part> = Vec::new();
    let mut page_names: HashMap<String, (usize, String)> = HashMap::new();

    // 1. page names
    let mut page_order: Vec<String> = Vec::new();
    let mut pages_idx = None;
    if let Some((_, e)) = find_entry(&entries, PAGES) {
        let (text, enc) = decode_part(PAGES, &e.data, opts)?;
        let parsed = parse_part(PAGES, &text, depth)?;
        let mut part = Part { name: PAGES.into(), enc, parsed, edits: HashMap::new(), changed: false };
        let mut n = 0usize;
        let mut found: Vec<(u32, String, bool, String)> = Vec::new();
        if let Some(root) = root_of(&part.parsed.nodes) {
            for page in root.elements().filter(|p| p.local() == "Page") {
                if let Some(name) = page.attr("Name") {
                    found.push((page.id, name, page.has_attr("NameU"), page.quote_in_use().to_string()));
                }
            }
        }
        for (id, name, has_nameu, q) in found {
            n += 1;
            b.slot()?;
            if !translatable(&name) {
                continue;
            }
            let unit = b.units.len();
            let mut u = Unit::new(format!("name-{n}"), vec![Inline::text(name.clone())]);
            u.context.kind = Some("page-name".into());
            u.context.style = Some(STYLE_PAGE.into());
            u.context.location = Some(format!("pages.xml, page {n}"));
            u.codes.insert(C_KIND.into(), code("attr"));
            u.codes.insert(C_QUOTE.into(), code(q));
            let a = {
                let root = root_of(&part.parsed.nodes).expect("root");
                let page = root.elements().find(|p| p.id == id).expect("page");
                find_attr(&page.start, "Name").expect("Name")
            };
            let root = root_of(&part.parsed.nodes).expect("root");
            let page = root.elements().find(|p| p.id == id).expect("page");
            u.codes.insert(C_ORIG.into(), code(page.start[a.vs..a.ve].to_string()));
            b.mark(&mut u, b.slots - 1);
            b.units.push(u);
            part.edits.insert(id, Edit::Attr { unit, name: "Name", nameu: !has_nameu });
            part.changed = true;
        }
        // page parts by relationship
        if let Some((_, re)) = find_entry(&entries, PAGES_RELS) {
            let rels = read_rels(PAGES_RELS, re, opts, depth)?;
            let targets: HashMap<&str, String> = rels
                .0
                .iter()
                .map(|(id, _, t)| {
                    let path = if let Some(s) = t.strip_prefix('/') { s.trim_start_matches('/').to_string() } else { normpath(&format!("visio/pages/{t}")) };
                    (id.as_str(), path)
                })
                .collect();
            if let Some(root) = root_of(&part.parsed.nodes) {
                for page in root.elements().filter(|p| p.local() == "Page") {
                    for ch in page.elements().filter(|c| c.local() == "Rel") {
                        if let Some(path) = r_id(ch, &[root, page]).and_then(|id| targets.get(id.as_str()).cloned()) {
                            if names.contains(path.as_str()) && !page_order.contains(&path) && path.ends_with(".xml") {
                                page_names.insert(path.clone(), (page_order.len() + 1, page.attr("Name").unwrap_or_default()));
                                page_order.push(path);
                            }
                        }
                    }
                }
            }
        }
        pages_idx = Some(parts.len());
        parts.push(part);
    }
    if page_order.is_empty() {
        let mut v: Vec<(u64, String)> = entries.iter().filter_map(|e| page_part_number(&e.name).map(|n| (n, e.name.clone()))).collect();
        v.sort();
        for (k, (_, n)) in v.into_iter().enumerate() {
            page_names.insert(n.clone(), (k + 1, String::new()));
            page_order.push(n);
        }
    }

    // 2. shape text and data labels
    for pname in &page_order {
        let (_, e) = find_entry(&entries, pname).expect("listed");
        let (text, enc) = decode_part(pname, &e.data, opts)?;
        let parsed = parse_part(pname, &text, depth)?;
        let mut part = Part { name: pname.clone(), enc, parsed, edits: HashMap::new(), changed: false };
        let stem = pname.rsplit('/').next().unwrap_or(pname).trim_end_matches(".xml").to_string();
        let (pno, pnm) = page_names.get(pname).cloned().unwrap_or((0, String::new()));
        let mut w = PageWalk { b: &mut b, stem, pno, pnm, n: 0, edits: HashMap::new() };
        let mut anc: Vec<&El> = Vec::new();
        for node in &part.parsed.nodes {
            if let Node::El(e) = node {
                w.walk(e, &mut anc)?;
            }
        }
        let edits = w.edits;
        if !edits.is_empty() {
            part.edits = edits;
            part.changed = true;
        }
        parts.push(part);
    }

    // 3. comments, when asked
    let mut comments_part = None;
    if with_comments {
        let mut path = None;
        if let Some((_, re)) = find_entry(&entries, DOCUMENT_RELS) {
            for (_, typ, target) in read_rels(DOCUMENT_RELS, re, opts, depth)?.0 {
                if typ.rsplit('/').next() == Some("comments") {
                    let p = if let Some(s) = target.strip_prefix('/') { normpath(s) } else { normpath(&format!("visio/{target}")) };
                    if names.contains(p.as_str()) && !p.starts_with("..") {
                        path = Some(p);
                        break;
                    }
                }
            }
        }
        let path = path.or_else(|| names.contains(COMMENTS).then(|| COMMENTS.to_string()));
        if let Some(p) = path {
            let (_, e) = find_entry(&entries, &p).expect("listed");
            let (text, enc) = decode_part(&p, &e.data, opts)?;
            if text.bytes().filter(|b| *b == b'<').count() > 40 * l.max_units {
                return Err(Error::Limit(bilingua_model::LimitError::new(LimitKind::XmlElements, "This Visio file has far more markup than the limit allows.")));
            }
            let parsed = parse_part(&p, &text, depth)?;
            let mut part = Part { name: p.clone(), enc, parsed, edits: HashMap::new(), changed: false };
            let mut entries_found: Vec<&El> = Vec::new();
            fn collect<'a>(nodes: &'a [Node], out: &mut Vec<&'a El>) {
                for n in nodes {
                    if let Node::El(e) = n {
                        if e.local() == "CommentEntry" {
                            out.push(e);
                        }
                        collect(&e.children, out);
                    }
                }
            }
            collect(&part.parsed.nodes, &mut entries_found);
            let mut edits = HashMap::new();
            for (j, el) in entries_found.iter().enumerate() {
                if el.children.iter().any(|c| !matches!(c, Node::Text(_))) {
                    continue;
                }
                let Some(core) = split_core(&el.children) else { continue };
                let txt: String = core.parts.iter().map(|p| if let CorePart::Text(t) = p { t.as_str() } else { "" }).collect();
                if !translatable(&txt) {
                    continue;
                }
                b.slots += 1;
                let unit = b.units.len();
                let mut u = Unit::new(format!("comment-{}", j + 1), vec![Inline::text(txt)]);
                u.context.kind = Some("comment".into());
                u.context.style = Some(STYLE_COMMENT.into());
                u.context.location = Some(format!("comment {}", j + 1));
                set_run_codes(&mut u, &core, &el.children, "text");
                b.mark(&mut u, COMMENT_BASE + j);
                b.units.push(u);
                edits.insert(el.id, Edit::Children { unit });
            }
            if !edits.is_empty() {
                part.edits = edits;
                part.changed = true;
            }
            comments_part = Some(parts.len());
            parts.push(part);
        }
    }
    let _ = (pages_idx, comments_part);

    // the skeleton: untouched entries keep their (possibly deflated) bytes
    let mut by_name: HashMap<&str, &Part> = HashMap::new();
    let mut enc_map = BTreeMap::new();
    for p in &parts {
        if let Some(t) = &p.enc {
            enc_map.insert(p.name.clone(), t.clone());
        }
        if p.changed {
            by_name.insert(p.name.as_str(), p);
        }
    }
    let mut zentries = Vec::with_capacity(entries.len() + 1);
    let mut packed = BTreeMap::new();
    for e in &entries {
        let data = match by_name.get(e.name.as_str()) {
            Some(p) => {
                let mut em = Emitter { parts: vec![], cur: String::new() };
                emit(&p.parsed.nodes, &p.edits, &mut em);
                ZipData::Parts(em.finish())
            }
            None => ZipData::Raw(e.keep(&mut packed)),
        };
        zentries.push(ZipEntry::new(e.name.clone(), data));
    }
    let meta = Meta { entries: entries.iter().map(|e| e.meta.clone()).collect(), packed, enc: enc_map, comments: with_comments };
    let meta_json = serde_json::to_vec(&meta).map_err(|e| perr(e.to_string()))?;
    zentries.push(ZipEntry::raw(META_NAME, meta_json));
    Ok(Document::new("vsdx", b.units, Skeleton::zip(zentries, bilingua_model::parser_info!())).with_source_lang(opts.source_lang.clone()).with_target_lang(opts.target_lang.clone()).with_granularity(Granularity::Paragraph))
}

/// The codes a run-holding unit needs to be written back: original content, prefix, suffix, line-end style.
fn set_run_codes(u: &mut Unit, core: &Core, children: &[Node], kind: &str) {
    let orig = raw_of(children);
    if orig.contains("\r\n") {
        u.codes.insert(C_CRLF.into(), code("1"));
    }
    u.codes.insert(C_ORIG.into(), code(orig));
    u.codes.insert(C_PRE.into(), code(core.pre.clone()));
    u.codes.insert(C_SUF.into(), code(core.suf.clone()));
    u.codes.insert(C_KIND.into(), code(kind));
}

struct PageWalk<'b> {
    b: &'b mut Builder,
    stem: String,
    pno: usize,
    pnm: String,
    n: usize,
    edits: HashMap<u32, Edit>,
}

impl PageWalk<'_> {
    fn location(&self, shape: Option<&El>) -> String {
        let id = shape.and_then(|s| s.attr("ID")).map(|i| format!(", shape {i}")).unwrap_or_default();
        if self.pnm.is_empty() {
            format!("page {}{id}", self.pno)
        } else {
            format!("page {} \"{}\"{id}", self.pno, self.pnm)
        }
    }

    fn walk<'a>(&mut self, el: &'a El, anc: &mut Vec<&'a El>) -> Result<(), Error> {
        let local = el.local();
        if local == "Text" && anc.last().is_some_and(|p| p.local() == "Shape") {
            self.n += 1;
            self.b.slot()?;
            let shape = *anc.last().expect("parent");
            if let Some(core) = split_core(&el.children) {
                let has_text = core.parts.iter().any(|p| matches!(p, CorePart::Text(t) if translatable(t)));
                if has_text {
                    let unit = self.b.units.len();
                    let mut u = Unit::new(format!("{}-{}", self.stem, self.n), vec![]);
                    let mut src = Vec::new();
                    let mut tags = 0;
                    for p in &core.parts {
                        match p {
                            CorePart::Text(t) => src.push(Inline::text(t.clone())),
                            CorePart::Mark(raw) => {
                                tags += 1;
                                let id = tags.to_string();
                                src.push(Inline::tag(id.clone(), TagKind::Standalone, raw.clone()));
                                u.codes.insert(id, CodeData::new().with_standalone(raw.clone()).with_label(mark_label(raw).to_string()));
                            }
                        }
                    }
                    u.segments[0].source = merge_text(src);
                    u.context.kind = Some("shape-text".into());
                    u.context.style = Some(STYLE_TEXT.into());
                    u.context.location = Some(self.location(Some(shape)));
                    set_run_codes(&mut u, &core, &el.children, "text");
                    u.codes.insert(C_SHAPE.into(), code(shape.start.clone()));
                    self.b.mark(&mut u, self.b.slots - 1);
                    self.b.units.push(u);
                    self.edits.insert(el.id, Edit::Children { unit });
                    self.edits.insert(shape.id, Edit::Shape { unit, text_id: el.id });
                }
            }
        } else if local == "Cell" && label_cell(el, anc) {
            self.n += 1;
            self.b.slot()?;
            let v = el.attr("V").unwrap_or_default();
            if translatable(&v) {
                let unit = self.b.units.len();
                let mut u = Unit::new(format!("{}-{}", self.stem, self.n), vec![Inline::text(v)]);
                let shape = anc.iter().rev().find(|a| a.local() == "Shape").copied();
                u.context.kind = Some("data-label".into());
                u.context.style = Some(STYLE_LABEL.into());
                u.context.location = Some(self.location(shape));
                let a = find_attr(&el.start, "V").expect("V");
                u.codes.insert(C_KIND.into(), code("attr"));
                u.codes.insert(C_QUOTE.into(), code((a.quote as char).to_string()));
                u.codes.insert(C_ORIG.into(), code(el.start[a.vs..a.ve].to_string()));
                self.b.mark(&mut u, self.b.slots - 1);
                self.b.units.push(u);
                self.edits.insert(el.id, Edit::Attr { unit, name: "V", nameu: false });
            }
        }
        anc.push(el);
        for c in &el.children {
            if let Node::El(ch) = c {
                self.walk(ch, anc)?;
            }
        }
        anc.pop();
        Ok(())
    }
}

fn label_cell(el: &El, anc: &[&El]) -> bool {
    if el.attr("N").as_deref() != Some("Label") || el.has_attr("F") {
        return false;
    }
    let n = anc.len();
    n >= 2 && anc[n - 1].local() == "Row" && anc[n - 2].local() == "Section" && anc[n - 2].attr("N").as_deref() == Some("Property")
}
