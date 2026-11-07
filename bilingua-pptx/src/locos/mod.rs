//! PowerPoint under the Localization OS profile (Track D group B): a port of LocOS's `pptx_filter.py` (and the
//! part of `ooxml_strict.py` it uses), so the adapter gets LocOS's units, numbers and files.
//!
//! * The walk is LocOS's: for each slide in `p:sldIdLst` order (the label counts every `p:sldId`, a skipped one
//!   included), every `a:p` of the slide outside `mc:Fallback` and merged table cells; then the SmartArt data parts
//!   and charts in the order the slide references them (`"slide N SmartArt"`, `"slide N chart"`); then the first
//!   notes slide (`"slide N notes"`); a hidden slide is `"slide N hidden"`. Slide layouts and masters are not walked.
//!   `locos.p_index` is the paragraph's position in that walk, empty paragraphs counted, so the numbers have gaps.
//!   Then the alt text (`p:cNvPr` `descr` and `title`) of every slide, numbered after the last paragraph, styled
//!   `a11y:img-alt` / `a11y:img-title`.
//! * A paragraph with one text run and nothing else is plain text. Otherwise every `a:r` is a pair whose tag id is
//!   its LocOS run number (`1..N`), and every `a:br` and `a:fld` a standalone code numbered after the runs
//!   (`N+1..`); `profile::tokens::to_locos_text_numbered` turns that into LocOS's `⟦r1⟧..⟦/r1⟧⟦x3⟧`.
//! * Writing is LocOS's `_rebuild`: the target is turned back into LocOS text and split with LocOS's token rules
//!   (a run token sets the run whose `a:rPr` is copied, a point token puts its break or field back once, a dropped
//!   one is appended, new lines become `a:br`); the language is set on every run, break and field of a written
//!   paragraph (and `rtl="1"` for a right-to-left target); the SmartArt drawing cache follows its data part; the
//!   parts that changed are re-serialised, every other entry keeps its bytes. A Strict Open XML file is read and
//!   written through LocOS's namespace swap.
//!
//! Not ported: slide comments (LocOS's `comments=True` option, off by default), LocOS's segment and markup
//! budgets (the facade's limits apply instead), XML parts in an encoding other than UTF-8 or UTF-16.
//!
//! The XML tree is the internal crate `bilingua-xmltree` the XLIFF and DOCX ports share (`dom` here).

#![allow(dead_code)]

use bilingua_xmltree as dom;
#[cfg(test)]
mod tests;

use bilingua_model::pytext::{py_has_alnum, py_isdecimal_char, py_strip};
use bilingua_model::*;
use bilingua_zip::{Body, EntryMeta, RawEntry, ZipLimits};
use dom::Dom;
use std::collections::{BTreeMap, HashMap, HashSet};

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const DGM: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
const DSP: &str = "http://schemas.microsoft.com/office/drawing/2008/diagram";
const C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const ALT_ATTRS: [(&str, &str); 2] = [("descr", "a11y:img-alt"), ("title", "a11y:img-title")];
/// `UnitContext::extra` key with the package part a unit comes from.
pub const PART: &str = "pptx.part";

fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "pptx".into(), msg: msg.into() }
}

// ------------------------------------------------------------------------------------- Strict Open XML

const STRICT_PREFIX: &str = "http://purl.oclc.org/ooxml/";
const TRANS: &str = "http://schemas.openxmlformats.org/";
const STRICT_RELS: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
const TRANS_RELS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// `ooxml_strict.NAMESPACES`: Strict suffix (after `STRICT_PREFIX`) to Transitional suffix (after `TRANS`).
const NAMESPACES: [(&str, &str); 22] = [
    ("spreadsheetml/main", "spreadsheetml/2006/main"),
    ("wordprocessingml/main", "wordprocessingml/2006/main"),
    ("presentationml/main", "presentationml/2006/main"),
    ("drawingml/main", "drawingml/2006/main"),
    ("drawingml/chart", "drawingml/2006/chart"),
    ("drawingml/chartDrawing", "drawingml/2006/chartDrawing"),
    ("drawingml/diagram", "drawingml/2006/diagram"),
    ("drawingml/picture", "drawingml/2006/picture"),
    ("drawingml/spreadsheetDrawing", "drawingml/2006/spreadsheetDrawing"),
    ("drawingml/wordprocessingDrawing", "drawingml/2006/wordprocessingDrawing"),
    ("drawingml/lockedCanvas", "drawingml/2006/lockedCanvas"),
    ("drawingml/compatibility", "drawingml/2006/compatibility"),
    ("drawingml/table", "drawingml/2006/table"),
    ("officeDocument/math", "officeDocument/2006/math"),
    ("officeDocument/bibliography", "officeDocument/2006/bibliography"),
    ("officeDocument/characteristics", "officeDocument/2006/characteristics"),
    ("officeDocument/customXml", "officeDocument/2006/customXml"),
    ("officeDocument/docPropsVTypes", "officeDocument/2006/docPropsVTypes"),
    ("officeDocument/extendedProperties", "officeDocument/2006/extended-properties"),
    ("officeDocument/customProperties", "officeDocument/2006/custom-properties"),
    ("officeDocument/sharedTypes", "officeDocument/2006/sharedTypes"),
    ("schemaLibrary/main", "schemaLibrary/2006/main"),
];
const REL_RENAMES: [(&str, &str); 2] = [("extendedProperties", "extended-properties"), ("customProperties", "custom-properties")];

/// Python's `bytes.decode("ascii", "replace")`.
fn ascii_replace(b: &[u8]) -> String {
    b.iter().map(|&c| if c < 0x80 { c as char } else { '\u{fffd}' }).collect()
}

fn map_strict(uri: &str) -> Option<String> {
    if uri == STRICT_RELS {
        return Some(TRANS_RELS.to_string());
    }
    if let Some(s) = uri.strip_prefix(STRICT_PREFIX) {
        if let Some((_, t)) = NAMESPACES.iter().find(|(x, _)| *x == s) {
            return Some(format!("{TRANS}{t}"));
        }
    }
    let name = uri.strip_prefix(STRICT_RELS)?.strip_prefix('/')?;
    let name = REL_RENAMES.iter().find(|(s, _)| *s == name).map(|(_, t)| *t).unwrap_or(name);
    Some(format!("{TRANS_RELS}/{name}"))
}

fn map_transitional(uri: &str) -> String {
    if uri == TRANS_RELS {
        return STRICT_RELS.to_string();
    }
    if let Some(t) = uri.strip_prefix(TRANS) {
        if let Some((s, _)) = NAMESPACES.iter().find(|(_, x)| *x == t) {
            return format!("{STRICT_PREFIX}{s}");
        }
    }
    if let Some(name) = uri.strip_prefix(TRANS_RELS).and_then(|n| n.strip_prefix('/')) {
        let name = REL_RENAMES.iter().find(|(_, t)| *t == name).map(|(s, _)| *s).unwrap_or(name);
        return format!("{STRICT_RELS}/{name}");
    }
    uri.to_string()
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Python's bytes `\s`.
fn py_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// Every `(["'])(<prefix>[^"'<>\s]*)\1` in `xml` replaced by `f(uri)`; `skip` lists what may not follow the prefix
/// (the regex's negative look-ahead). `f` returning `None` refuses the part.
fn swap_quoted(xml: &[u8], prefix: &[u8], skip: &[&[u8]], f: &dyn Fn(&str) -> Option<String>) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(xml.len());
    let mut i = 0;
    while i < xml.len() {
        let q = xml[i];
        if (q == b'"' || q == b'\'') && xml[i + 1..].starts_with(prefix) && !skip.iter().any(|s| xml[i + 1 + prefix.len()..].starts_with(s)) {
            let mut end = i + 1;
            while end < xml.len() && !matches!(xml[end], b'"' | b'\'' | b'<' | b'>') && !py_space(xml[end]) {
                end += 1;
            }
            if end < xml.len() && xml[end] == q {
                out.push(q);
                out.extend_from_slice(f(&ascii_replace(&xml[i + 1..end]))?.as_bytes());
                out.push(q);
                i = end + 1;
                continue;
            }
        }
        out.push(q);
        i += 1;
    }
    Some(out)
}

/// `part_to_transitional`: `None` when the part names a Strict URI LocOS does not know (LocOS refuses the file).
fn part_to_transitional(xml: &[u8]) -> Option<Vec<u8>> {
    if !contains(xml, b"purl.oclc.org/ooxml/") {
        return Some(xml.to_vec());
    }
    let out = swap_quoted(xml, STRICT_PREFIX.as_bytes(), &[], &map_strict)?;
    if contains(&out, b"\"http://purl.oclc.org/ooxml/") || contains(&out, b"'http://purl.oclc.org/ooxml/") {
        return None;
    }
    Some(out)
}

/// `part_to_strict`, for a part this port re-serialised.
fn part_to_strict(xml: &[u8]) -> Vec<u8> {
    if !contains(xml, b"schemas.openxmlformats.org/") {
        return xml.to_vec();
    }
    swap_quoted(xml, TRANS.as_bytes(), &[b"package/", b"markup-compatibility/"], &|u| Some(map_transitional(u))).unwrap_or_else(|| xml.to_vec())
}

fn is_xml_name(name: &str) -> bool {
    let l = name.to_lowercase();
    [".xml", ".rels", ".vml"].iter().any(|s| l.ends_with(s)) || name == "[Content_Types].xml"
}

// ------------------------------------------------------------------------------------------ package

/// Python's `posixpath.normpath` for the relative paths a relationship resolves to.
fn normpath(path: &str) -> String {
    if path.is_empty() {
        return ".".into();
    }
    let lead = if path.starts_with("//") && !path.starts_with("///") {
        "//"
    } else if path.starts_with('/') {
        "/"
    } else {
        ""
    };
    let mut parts: Vec<&str> = Vec::new();
    for c in path.split('/') {
        match c {
            "" | "." => {}
            ".." if !lead.is_empty() && parts.is_empty() => {}
            ".." if parts.last().is_some_and(|l| *l != "..") => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    let s = format!("{lead}{}", parts.join("/"));
    if s.is_empty() {
        ".".into()
    } else {
        s
    }
}

/// `posixpath.dirname`.
fn dirname(p: &str) -> &str {
    let head = &p[..p.rfind('/').map(|i| i + 1).unwrap_or(0)];
    let t = head.trim_end_matches('/');
    if t.is_empty() {
        head
    } else {
        t
    }
}

fn join(d: &str, f: &str) -> String {
    if d.is_empty() || f.starts_with('/') {
        f.to_string()
    } else if d.ends_with('/') {
        format!("{d}{f}")
    } else {
        format!("{d}/{f}")
    }
}

/// `_resolve`: the package path of a relationship target, `None` when it leaves the package.
fn resolve(part: &str, target: &str) -> Option<String> {
    let path = if target.starts_with('/') { normpath(target.trim_start_matches('/')) } else { normpath(&join(dirname(part), target)) };
    (!(path.starts_with("..") || path.starts_with('/') || path == ".")).then_some(path)
}

fn rels_name(part: &str) -> String {
    let i = part.rfind('/').map(|i| i + 1).unwrap_or(0);
    let (head, tail) = (&part[..i], &part[i..]);
    let h = head.trim_end_matches('/');
    let head = if h.is_empty() { head } else { h };
    join(&join(head, "_rels"), &format!("{tail}.rels"))
}

/// A relationship: (type suffix, resolved path).
type RelTarget = (String, String);
/// `rId -> (type, path)` in Python dict order (first insertion keeps its place, the last value wins).
type Rels = Vec<(Option<String>, RelTarget)>;

fn rel_get<'a>(rels: &'a Rels, id: Option<&str>) -> Option<&'a RelTarget> {
    rels.iter().find(|(k, _)| k.as_deref() == id).map(|(_, v)| v)
}

/// The package: its entries, the (Transitional) bytes of every part and the parsed parts.
struct Pkg {
    entries: Vec<RawEntry>,
    names: HashSet<String>,
    /// Normalised bytes of a Strict package's XML parts (absent: the entry's own bytes).
    normal: HashMap<String, Vec<u8>>,
    strict: bool,
    doms: HashMap<String, Dom>,
    /// The decoded text of each parsed part (the element spans point into it).
    texts: HashMap<String, String>,
}

fn decode(raw: &[u8]) -> Option<String> {
    if let Some(b) = raw.strip_prefix(&[0xEF, 0xBB, 0xBF][..]) {
        return String::from_utf8(b.to_vec()).ok();
    }
    let utf16 = |b: &[u8], le: bool| -> Option<String> {
        if !b.len().is_multiple_of(2) {
            return None;
        }
        let u: Vec<u16> = b.chunks(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        String::from_utf16(&u).ok()
    };
    if let Some(b) = raw.strip_prefix(&[0xFF, 0xFE][..]) {
        return utf16(b, true);
    }
    if let Some(b) = raw.strip_prefix(&[0xFE, 0xFF][..]) {
        return utf16(b, false);
    }
    String::from_utf8(raw.to_vec()).ok()
}

impl Pkg {
    fn open(bytes: &[u8], limits: &Limits) -> Result<Pkg, Error> {
        let entries = bilingua_zip::read_zip("pptx", bytes, &ZipLimits::from(limits))?;
        bilingua_zip::check_xml_parts(&entries, limits)?;
        let names = entries.iter().filter(|e| !e.meta.dir).map(|e| e.name.clone()).collect();
        let strict = entries.iter().any(|e| e.name == "_rels/.rels" && contains(&e.data, format!("{STRICT_RELS}/officeDocument").as_bytes()));
        let mut normal = HashMap::new();
        if strict {
            for e in entries.iter().filter(|e| !e.meta.dir && is_xml_name(&e.name)) {
                let t = part_to_transitional(&e.data).ok_or_else(|| {
                    perr("This PowerPoint file is saved as Strict Open XML with parts this version cannot read. Save it in the standard Office format and upload it again.")
                })?;
                if t != e.data {
                    normal.insert(e.name.clone(), t);
                }
            }
        }
        Ok(Pkg { entries, names, normal, strict, doms: HashMap::new(), texts: HashMap::new() })
    }

    fn bytes(&self, name: &str) -> Option<&[u8]> {
        if let Some(b) = self.normal.get(name) {
            return Some(b);
        }
        self.entries.iter().find(|e| e.name == name && !e.meta.dir).map(|e| e.data.as_slice())
    }

    /// `_Package.xml`: the parsed part, `None` when there is no such entry.
    fn load(&mut self, name: &str) -> Result<bool, Error> {
        if self.doms.contains_key(name) {
            return Ok(true);
        }
        let Some(raw) = self.bytes(name) else { return Ok(false) };
        if contains(&raw[..raw.len().min(4096)], b"<!DOCTYPE") || contains(raw, b"<!ENTITY") {
            return Err(perr("This PowerPoint file contains an unsupported XML document type."));
        }
        let bad = || perr(format!("This PowerPoint file has invalid XML in {name}. Save it again in PowerPoint."));
        let text = decode(raw).ok_or_else(bad)?;
        let d = dom::parse(&text).map_err(|_| bad())?;
        self.doms.insert(name.to_string(), d);
        self.texts.insert(name.to_string(), text);
        Ok(true)
    }

    fn dom(&self, name: &str) -> &Dom {
        &self.doms[name]
    }

    /// `_Package.rels`: the internal relationships of `part`.
    fn rels(&mut self, part: &str) -> Result<Rels, Error> {
        let rn = rels_name(part);
        let mut out: Rels = Vec::new();
        if !self.load(&rn)? {
            return Ok(out);
        }
        let d = self.dom(&rn);
        for rel in d.iter(d.root).into_iter().filter(|&e| d.is(e, PKG_REL, "Relationship")) {
            if d.get(rel, "TargetMode").unwrap_or("").to_lowercase() == "external" {
                continue;
            }
            let Some(path) = resolve(part, d.get(rel, "Target").unwrap_or("")) else { continue };
            let kind = d.get(rel, "Type").unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
            let id = d.get(rel, "Id").map(str::to_string);
            match out.iter_mut().find(|(k, _)| *k == id) {
                Some((_, v)) => *v = (kind, path),
                None => out.push((id, (kind, path))),
            }
        }
        Ok(out)
    }
}

fn main_part(pkg: &mut Pkg) -> Result<String, Error> {
    let mut rels = pkg.rels("")?;
    rels.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    for (_, (kind, path)) in rels {
        if kind == "officeDocument" && pkg.names.contains(&path) && pkg.load(&path)? {
            let d = pkg.dom(&path);
            if d.is(d.root, P, "presentation") {
                return Ok(path);
            }
        }
    }
    Err(perr("This PowerPoint file is not a PowerPoint presentation (no presentation part)."))
}

// --------------------------------------------------------------------------------------------- walk

fn in_fallback_or_merged(d: &Dom, p: usize) -> bool {
    let mut a = d.nodes[p].parent;
    while let Some(x) = a {
        if d.is(x, MC, "Fallback") {
            return true;
        }
        if d.is(x, A, "tc") && (matches!(d.get(x, "hMerge"), Some("1" | "true")) || matches!(d.get(x, "vMerge"), Some("1" | "true"))) {
            return true;
        }
        a = d.nodes[x].parent;
    }
    false
}

fn paragraphs(d: &Dom, root: usize) -> Vec<usize> {
    d.iter(root).into_iter().filter(|&e| d.is(e, A, "p") && !in_fallback_or_merged(d, e)).collect()
}

struct Alt {
    part: String,
    node: usize,
    attr: &'static str,
    value: String,
    style: &'static str,
}

/// `_Walk`: the ordered paragraphs and alt-text slots of a package.
struct Walk {
    paras: Vec<(String, usize, String)>,
    alts: Vec<Alt>,
    /// diagramData part and its cached drawing part, in walk order.
    drawings: Vec<(String, String)>,
}

impl Walk {
    fn new(pkg: &mut Pkg) -> Result<Walk, Error> {
        let mut w = Walk { paras: Vec::new(), alts: Vec::new(), drawings: Vec::new() };
        let main = main_part(pkg)?;
        let prels = pkg.rels(&main)?;
        let mut seen: HashSet<String> = HashSet::new();
        let slide_ids: Vec<Option<String>> = {
            let d = pkg.dom(&main);
            match d.find(d.root, P, "sldIdLst") {
                None => Vec::new(),
                Some(lst) => d.iter(lst).into_iter().skip(1).filter(|&e| d.is(e, P, "sldId")).map(|e| d.get_ns(e, R, "id").map(str::to_string)).collect(),
            }
        };
        for (n, rid) in slide_ids.iter().enumerate() {
            let n = n + 1;
            let Some((kind, path)) = rel_get(&prels, rid.as_deref()).cloned() else { continue };
            if kind != "slide" || seen.contains(&path) || !pkg.names.contains(&path) {
                continue;
            }
            seen.insert(path.clone());
            pkg.load(&path)?;
            let hidden = {
                let d = pkg.dom(&path);
                matches!(d.get(d.root, "show"), Some("0" | "false"))
            };
            let label = format!("slide {n}{}", if hidden { " hidden" } else { "" });
            w.add(pkg, &path, &label);
            let srels = pkg.rels(&path)?;
            // SmartArt and charts, in the order the slide references them
            let refs: Vec<(Option<String>, &str)> = {
                let d = pkg.dom(&path);
                d.iter(d.root)
                    .into_iter()
                    .filter_map(|e| {
                        if d.is(e, DGM, "relIds") {
                            Some((d.get_ns(e, R, "dm").map(str::to_string), "SmartArt"))
                        } else if d.is(e, C, "chart") {
                            Some((d.get_ns(e, R, "id").map(str::to_string), "chart"))
                        } else {
                            None
                        }
                    })
                    .collect()
            };
            for (rid, what) in refs {
                let Some((kind2, sub)) = rel_get(&srels, rid.as_deref()).cloned() else { continue };
                if sub.is_empty() || seen.contains(&sub) || !pkg.names.contains(&sub) {
                    continue;
                }
                if kind2 != "diagramData" && kind2 != "chart" {
                    continue;
                }
                seen.insert(sub.clone());
                pkg.load(&sub)?;
                w.add(pkg, &sub, &format!("{label} {what}"));
                if kind2 == "diagramData" {
                    let d = pkg.dom(&sub);
                    let drawing = d.iter(d.root).into_iter().filter(|&e| d.is(e, DSP, "dataModelExt")).find_map(|e| {
                        rel_get(&srels, d.get(e, "relId")).filter(|(k, p)| k == "diagramDrawing" && pkg.names.contains(p)).map(|(_, p)| p.clone())
                    });
                    if let Some(drawing) = drawing {
                        w.drawings.push((sub.clone(), drawing));
                    }
                }
            }
            for (_, (kind2, sub)) in &srels {
                if kind2 == "notesSlide" && !seen.contains(sub) && pkg.names.contains(sub) {
                    seen.insert(sub.clone());
                    pkg.load(sub)?;
                    w.add(pkg, sub, &format!("{label} notes"));
                    break;
                }
            }
            let d = pkg.dom(&path);
            for cnv in d.iter(d.root).into_iter().filter(|&e| d.is(e, P, "cNvPr")) {
                let mut a = d.nodes[cnv].parent;
                let mut fallback = false;
                while let Some(x) = a {
                    fallback |= d.is(x, MC, "Fallback");
                    a = d.nodes[x].parent;
                }
                if fallback {
                    continue; // the Fallback copy of a shape is not walked
                }
                for (attr, style) in ALT_ATTRS {
                    let value = py_strip(d.get(cnv, attr).unwrap_or(""));
                    if !value.is_empty() && py_has_alnum(value) {
                        w.alts.push(Alt { part: path.clone(), node: cnv, attr, value: value.to_string(), style });
                    }
                }
            }
        }
        Ok(w)
    }

    fn add(&mut self, pkg: &Pkg, part: &str, style: &str) {
        let d = pkg.dom(part);
        for p in paragraphs(d, d.root) {
            self.paras.push((part.to_string(), p, style.to_string()));
        }
    }
}

// --------------------------------------------------------------------------------------- tokenizing

/// `_items`: the runs (`true`) and the breaks and fields (`false`) of a paragraph, in order.
fn items(d: &Dom, p: usize) -> Vec<(bool, usize)> {
    d.nodes[p]
        .children
        .iter()
        .filter_map(|&c| {
            if d.is(c, A, "r") {
                Some((true, c))
            } else if d.is(c, A, "br") || d.is(c, A, "fld") {
                Some((false, c))
            } else {
                None
            }
        })
        .collect()
}

/// `_run_text`: lxml's `.text` of the run's first `a:t`.
fn run_text(d: &Dom, r: usize) -> String {
    d.find(r, A, "t").map(|t| d.lead_text(t)).unwrap_or_default()
}

fn has_text(d: &Dom, p: usize) -> bool {
    items(d, p).into_iter().any(|(run, el)| run && !py_strip(&run_text(d, el)).is_empty())
}

/// Literal text that looks like a LocOS token (`⟦x2⟧` written by the author or the translator) must not be read as
/// one when a translation is written. Literal text is escaped on its way to the writer (the open bracket becomes
/// U+E000 U+E001, U+E000 becomes U+E000 U+E000) and unescaped where the spans are cut, so only real codes are tokens.
pub(crate) fn esc_lit(s: &str) -> String {
    if !s.contains(['\u{27e6}', '\u{e000}']) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\u{27e6}' => out.push_str("\u{e000}\u{e001}"),
            '\u{e000}' => out.push_str("\u{e000}\u{e000}"),
            c => out.push(c),
        }
    }
    out
}

/// The inverse of `esc_lit`.
pub(crate) fn unesc_lit(s: &str) -> String {
    if !s.contains('\u{e000}') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{e000}' {
            match it.peek() {
                Some('\u{e001}') => {
                    it.next();
                    out.push('\u{27e6}');
                    continue;
                }
                Some('\u{e000}') => {
                    it.next();
                }
                _ => {}
            }
        }
        out.push(c)
    }
    out
}

/// `tokenize(p)[0]`: the paragraph as LocOS text.
fn tok_text(d: &Dom, p: usize) -> String {
    let it = items(d, p);
    let runs = it.iter().filter(|(r, _)| *r).count();
    if it.len() == 1 && runs == 1 {
        return esc_lit(&run_text(d, it[0].1));
    }
    let (mut rn, mut xn) = (0, runs);
    let mut s = String::new();
    for (run, el) in it {
        if run {
            rn += 1;
            s.push_str(&format!("\u{27e6}r{rn}\u{27e7}{}\u{27e6}/r{rn}\u{27e7}", esc_lit(&run_text(d, el))));
        } else {
            xn += 1;
            s.push_str(&format!("\u{27e6}x{xn}\u{27e7}"));
        }
    }
    s
}

/// The paragraph as Bilingua inlines: tag ids are LocOS's token numbers (runs first, breaks and fields after).
fn tok_inlines(d: &Dom, text: &str, p: usize) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    let it = items(d, p);
    let runs = it.iter().filter(|(r, _)| *r).count();
    let mut out = Vec::new();
    let mut codes = BTreeMap::new();
    if it.len() == 1 && runs == 1 {
        let t = run_text(d, it[0].1);
        if !t.is_empty() {
            out.push(Inline::text(t));
        }
        return (out, codes);
    }
    let raw = |id: usize| d.nodes[id].span.map(|s| (&text[s.start..s.start_end], &text[s.end_start..s.end], &text[s.start..s.end]));
    let (mut rn, mut xn) = (0usize, runs);
    for (run, el) in it {
        let (open, close, whole) = raw(el).unwrap_or(("", "", ""));
        if run {
            rn += 1;
            let id = rn.to_string();
            out.push(Inline::tag(id.clone(), TagKind::Open, open));
            let t = run_text(d, el);
            if !t.is_empty() {
                out.push(Inline::text(t));
            }
            out.push(Inline::tag(id.clone(), TagKind::Close, close));
            codes.insert(id, CodeData::paired(open, close).with_label(if d.find(el, A, "rPr").is_some() { "run" } else { "plain run" }));
        } else {
            xn += 1;
            let id = xn.to_string();
            let label = if d.is(el, A, "br") { "line break" } else { "field" };
            out.push(Inline::tag(id.clone(), TagKind::Standalone, whole));
            codes.insert(id, CodeData::standalone(whole).with_label(label));
        }
    }
    (out, codes)
}

/// One LocOS token at the start of `s` (`⟦/?[a-z]?\d+⟧`, `\d` being any Unicode decimal digit): (closing, letter,
/// value or `None` past `u64`, byte length).
fn token_at(s: &str) -> Option<(bool, Option<char>, Option<u64>, usize)> {
    let rest = s.strip_prefix('\u{27e6}')?;
    let (close, rest) = match rest.strip_prefix('/') {
        Some(r) => (true, r),
        None => (false, rest),
    };
    let (letter, rest) = match rest.chars().next() {
        Some(c) if c.is_ascii_lowercase() => (Some(c), &rest[1..]),
        _ => (None, rest),
    };
    let mut value: Option<u64> = Some(0);
    let mut len = 0;
    for c in rest.chars() {
        if !py_isdecimal_char(c) {
            break;
        }
        value = value.and_then(|v| v.checked_mul(10)).and_then(|v| v.checked_add(digit_value(c)));
        len += c.len_utf8();
    }
    if len == 0 || !rest[len..].starts_with('\u{27e7}') {
        return None;
    }
    Some((close, letter, value, s.len() - rest.len() + len + '\u{27e7}'.len_utf8()))
}

/// The value of a Unicode decimal digit (`int()` of it): Unicode keeps every decimal digit set as ten code points
/// in a row from zero.
fn digit_value(c: char) -> u64 {
    if let Some(v) = c.to_digit(10) {
        return v as u64;
    }
    let mut k = 0u32;
    while k < 64 && char::from_u32(c as u32 - k - 1).is_some_and(py_isdecimal_char) {
        k += 1;
    }
    (k % 10) as u64
}

#[derive(Debug, PartialEq)]
enum Span {
    /// Text in the run with this index (`None`: no run).
    T(Option<usize>, String),
    /// The point with this index (may be out of range: then it writes nothing).
    X(i128),
}

/// `_spans`: translated LocOS text to runs of text and points.
fn spans(text: &str, n_runs: usize) -> Vec<Span> {
    let mut out = Vec::new();
    let mut cur: Option<usize> = None;
    let mut lit = String::new();
    let mut i = 0;
    while i < text.len() {
        if let Some((close, letter, value, len)) = token_at(&text[i..]) {
            if !lit.is_empty() {
                out.push(Span::T(cur, unesc_lit(&std::mem::take(&mut lit))));
            }
            match (letter, value) {
                (Some('r'), Some(v)) if (1..=n_runs as u64).contains(&v) => cur = if close { None } else { Some(v as usize - 1) },
                (Some('x'), v) if !close => out.push(Span::X(v.map(|v| v as i128 - n_runs as i128 - 1).unwrap_or(i128::MAX))),
                _ => out.push(Span::T(cur, text[i..i + len].to_string())),
            }
            i += len;
            continue;
        }
        let c = text[i..].chars().next().unwrap();
        lit.push(c);
        i += c.len_utf8();
    }
    if !lit.is_empty() {
        out.push(Span::T(cur, unesc_lit(&lit)));
    }
    out
}

/// The name for a new element in namespace A inside `ctx`.
fn a_name(d: &Dom, ctx: usize, local: &str) -> String {
    match d.inner_prefix(ctx, A) {
        Some(p) if !p.is_empty() => format!("{p}:{local}"),
        _ => local.to_string(),
    }
}

fn new_el(d: &mut Dom, ctx: usize, local: &str) -> usize {
    let name = a_name(d, ctx, local);
    let scope = d.nodes[ctx].scope;
    d.new_element(&name, scope, Some(A.to_string()))
}

/// `_new_run`: an `a:r` with a copy of `rpr` and an `a:t` holding `text`.
fn new_run(d: &mut Dom, p: usize, text: &str, rpr: Option<usize>) -> Vec<usize> {
    let r = new_el(d, p, "r");
    if let Some(rpr) = rpr {
        for c in d.copy_with_tail(rpr, p) {
            d.append(r, c);
        }
    }
    let t = new_el(d, p, "t");
    d.append_text(t, text);
    d.append(r, t);
    vec![r]
}

fn new_br(d: &mut Dom, p: usize, rpr: Option<usize>) -> Vec<usize> {
    let br = new_el(d, p, "br");
    if let Some(rpr) = rpr {
        for c in d.copy_with_tail(rpr, p) {
            d.append(br, c);
        }
    }
    vec![br]
}

/// `write_paragraph`: replaces the content of one `a:p` with LocOS text.
fn write_paragraph(d: &mut Dom, p: usize, text: &str) {
    let it = items(d, p);
    let runs: Vec<usize> = it.iter().filter(|(r, _)| *r).map(|(_, e)| *e).collect();
    let points: Vec<usize> = it.iter().filter(|(r, _)| !*r).map(|(_, e)| *e).collect();
    let rprs: Vec<Option<usize>> = runs.iter().map(|&r| d.find(r, A, "rPr")).collect();
    let fallback = rprs.first().copied().flatten();
    let mut used: HashSet<usize> = HashSet::new();
    let mut new: Vec<Vec<usize>> = Vec::new();
    for span in spans(text, runs.len()) {
        match span {
            Span::X(i) => {
                if i >= 0 && (i as usize) < points.len() && used.insert(i as usize) {
                    new.push(d.copy_with_tail(points[i as usize], p));
                }
            }
            Span::T(ri, s) => {
                if s.is_empty() {
                    continue;
                }
                let rpr = match ri {
                    Some(i) => rprs[i],
                    None => fallback,
                };
                let s = s.replace("\r\n", "\n").replace('\r', "\n");
                for (j, line) in s.split('\n').enumerate() {
                    if j > 0 {
                        new.push(new_br(d, p, rpr));
                    }
                    if !line.is_empty() {
                        new.push(new_run(d, p, line, rpr));
                    }
                }
            }
        }
    }
    for (i, &el) in points.iter().enumerate() {
        if !used.contains(&i) {
            new.push(d.copy_with_tail(el, p)); // a dropped break or field is kept, never deleted
        }
    }
    if !new.iter().any(|v| d.is(v[0], A, "r")) {
        let r = new_run(d, p, "", fallback);
        new.insert(0, r);
    }
    for (_, el) in it {
        d.remove_with_tail(el);
    }
    let flat: Vec<usize> = new.into_iter().flatten().collect();
    let at = match d.find(p, A, "endParaRPr") {
        Some(end) => d.nodes[p].children.iter().position(|&c| c == end).unwrap(),
        None => d.nodes[p].children.len(),
    };
    d.insert_at(p, at, &flat);
}

/// `_stamp_language`.
fn stamp_language(d: &mut Dom, p: usize, tag: &str, rtl: bool) {
    let kids: Vec<usize> = d.nodes[p].children.clone();
    for r in kids {
        if d.is(r, A, "r") || d.is(r, A, "fld") || d.is(r, A, "br") {
            let rpr = match d.find(r, A, "rPr") {
                Some(x) => x,
                None => {
                    let x = new_el(d, r, "rPr");
                    let at = d.first_slot(r);
                    d.insert_at(r, at, &[x]);
                    x
                }
            };
            d.set(rpr, "lang", tag);
        }
    }
    if let Some(end) = d.find(p, A, "endParaRPr") {
        d.set(end, "lang", tag);
    }
    if rtl {
        let ppr = match d.find(p, A, "pPr") {
            Some(x) => x,
            None => {
                let x = new_el(d, p, "pPr");
                let at = d.first_slot(p);
                d.insert_at(p, at, &[x]);
                x
            }
        };
        d.set(ppr, "rtl", "1");
    }
}

/// `_point_paragraphs`: modelId -> the `a:p` of every SmartArt point with text, in dict order.
fn point_paragraphs(d: &Dom) -> Vec<(String, Vec<usize>)> {
    let mut out: Vec<(String, Vec<usize>)> = Vec::new();
    for pt in d.iter(d.root).into_iter().filter(|&e| d.is(e, DGM, "pt")) {
        let Some(t) = d.find(pt, DGM, "t") else { continue };
        let Some(mid) = d.get(pt, "modelId").filter(|m| !m.is_empty()) else { continue };
        let ps: Vec<usize> = d.iter(t).into_iter().filter(|&e| d.is(e, A, "p")).collect();
        match out.iter_mut().find(|(m, _)| m == mid) {
            Some((_, v)) => *v = ps,
            None => out.push((mid.to_string(), ps)),
        }
    }
    out
}

/// Each SmartArt point's paragraphs as LocOS text, by modelId.
type PointTexts = Vec<(String, Vec<String>)>;

/// `_mirror_drawing`: `translated` is each point's translated paragraphs as LocOS text.
fn mirror_drawing(source: &PointTexts, translated: &PointTexts, dr: &mut Dom) -> bool {
    let mut changed = false;
    let sps: Vec<usize> = dr.iter(dr.root).into_iter().filter(|&e| dr.is(e, DSP, "sp")).collect();
    for sp in sps {
        let Some(mid) = dr.get(sp, "modelId").map(str::to_string) else { continue };
        let Some((_, src)) = source.iter().find(|(m, _)| *m == mid) else { continue };
        let Some(body) = dr.find(sp, DSP, "txBody") else { continue };
        let cached: Vec<usize> = dr.iter(body).into_iter().filter(|&e| dr.is(e, A, "p")).collect();
        let tr = translated.iter().find(|(m, _)| *m == mid).map(|(_, v)| v.as_slice()).unwrap_or(&[]);
        let now: Vec<String> = cached.iter().map(|&p| tok_text(dr, p)).collect();
        if now != *src || cached.len() != tr.len() {
            continue;
        }
        for (&cp, t) in cached.iter().zip(tr) {
            write_paragraph(dr, cp, t);
        }
        changed = true;
    }
    changed
}

/// `xml_target_text`: XML 1.0 characters only, Word's pasted breaks as newlines.
fn xml_target_text(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\u{b}' || c == '\u{c}' { '\n' } else { c })
        .filter(|&c| !matches!(c, '\u{0}'..='\u{8}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}'))
        .collect()
}

/// `_serialize_part`.
fn serialize_part(d: &Dom, original: &[u8]) -> Vec<u8> {
    let standalone = contains(&original[..original.len().min(200)], b"standalone=\"yes\"");
    let decl = if standalone { "<?xml version='1.0' encoding='UTF-8' standalone='yes'?>\n" } else { "<?xml version='1.0' encoding='UTF-8'?>\n" };
    let mut out = decl.as_bytes().to_vec();
    out.extend_from_slice(d.serialize().as_bytes());
    out
}

// ------------------------------------------------------------------------------------ extract, merge

/// Extracts a presentation the way Localization OS does.
pub fn extract(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let mut pkg = Pkg::open(bytes, &opts.limits())?;
    let walk = Walk::new(&mut pkg)?;
    let mut units = Vec::new();
    for (i, (part, p, style)) in walk.paras.iter().enumerate() {
        let d = pkg.dom(part);
        if !has_text(d, *p) {
            continue;
        }
        let (source, codes) = tok_inlines(d, &pkg.texts[part], *p);
        units.push(unit(i, source, codes, part, style, "paragraph"));
    }
    let base = walk.paras.len();
    for (j, a) in walk.alts.iter().enumerate() {
        units.push(unit(base + j, vec![Inline::text(a.value.clone())], BTreeMap::new(), &a.part, a.style, "alt"));
    }
    Ok(Document::new("pptx".to_string(), units, Skeleton::binary(bytes.to_vec(), bilingua_model::parser_info!()))
        .with_source_lang(opts.source_lang.clone())
        .with_target_lang(opts.target_lang.clone())
        .with_granularity(Granularity::Paragraph))
}

fn unit(p_index: usize, source: Vec<Inline>, codes: BTreeMap<String, CodeData>, part: &str, style: &str, kind: &str) -> Unit {
    let mut u = Unit::new(p_index.to_string(), source);
    u.codes = codes;
    let mut ctx = UnitContext::new().with_kind(kind).with_location(format!("{part} {kind} {p_index}"));
    ctx.extra.insert(profile::P_INDEX.to_string(), p_index.to_string());
    ctx.extra.insert(profile::STYLE.to_string(), style.to_string());
    ctx.extra.insert(PART.to_string(), part.to_string());
    u.context = ctx;
    u
}

/// The LocOS text of a target: run tags are `r` pairs, every other code an `x` point, numbered by tag id.
pub fn target_text(target: &[Inline]) -> String {
    let mut s = String::new();
    for i in target {
        match i {
            Inline::Text(t) => s.push_str(&esc_lit(t)),
            Inline::Tag { id, kind: TagKind::Open, .. } => s.push_str(&format!("\u{27e6}r{id}\u{27e7}")),
            Inline::Tag { id, kind: TagKind::Close, .. } => s.push_str(&format!("\u{27e6}/r{id}\u{27e7}")),
            Inline::Tag { id, free: true, .. } => s.push_str(&format!("\u{27e6}p{id}\u{27e7}")),
            Inline::Tag { id, .. } => s.push_str(&format!("\u{27e6}x{id}\u{27e7}")),
        }
    }
    s
}

/// The LocOS texts a document writes, by `p_index`: every unit whose segments all have a target.
fn written_texts(doc: &Document) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    for u in doc.units.iter().filter(|u| u.translate) {
        let Some(p) = u.context.extra.get(profile::P_INDEX).and_then(|p| p.parse::<i64>().ok()) else { continue };
        if u.segments.is_empty() || !u.segments.iter().all(|s| s.has_target()) {
            continue;
        }
        out.push((p, target_text(&u.output())));
    }
    out
}

/// `PptxFilter._rebuild`: the package with `texts` (`p_index` -> LocOS text) written.
pub fn rebuild(original: &[u8], texts: &[(i64, String)], target: Option<&str>) -> Result<Vec<u8>, Error> {
    let werr = |e: Error| match e {
        Error::Parse { msg, .. } => Error::Write(msg),
        e => e,
    };
    let mut pkg = Pkg::open(original, &Limits::default()).map_err(werr)?;
    let walk = Walk::new(&mut pkg).map_err(werr)?;
    let tag = target.and_then(profile::lang_tag);
    let rtl = tag.is_some() && target.is_some_and(profile::is_rtl);
    // SmartArt source text per point, read before any paragraph is rewritten
    let mut smart_src: Vec<(String, PointTexts)> = Vec::new();
    for (data, _) in &walk.drawings {
        let d = pkg.dom(data);
        let pts = point_paragraphs(d).into_iter().map(|(m, ps)| (m, ps.iter().map(|&p| tok_text(d, p)).collect())).collect();
        smart_src.push((data.clone(), pts));
    }
    let mut touched: HashSet<String> = HashSet::new();
    let base = walk.paras.len() as i64;
    for (i, text) in texts {
        let text = xml_target_text(text);
        if (0..base).contains(i) {
            let (part, p, _) = &walk.paras[*i as usize];
            let d = pkg.doms.get_mut(part).unwrap();
            write_paragraph(d, *p, &text);
            if let Some(tag) = &tag {
                stamp_language(d, *p, tag, rtl);
            }
            touched.insert(part.clone());
        } else if (base..base + walk.alts.len() as i64).contains(i) {
            let a = &walk.alts[(i - base) as usize];
            pkg.doms.get_mut(&a.part).unwrap().set(a.node, a.attr, &unesc_lit(&text));
            touched.insert(a.part.clone());
        }
    }
    for ((data, drawing), (_, src)) in walk.drawings.iter().zip(&smart_src) {
        if !touched.contains(data) || !pkg.load(drawing).map_err(werr)? {
            continue;
        }
        let translated: PointTexts = {
            let d = pkg.dom(data);
            point_paragraphs(d).into_iter().map(|(m, ps)| (m, ps.iter().map(|&p| tok_text(d, p)).collect())).collect()
        };
        let dr = pkg.doms.get_mut(drawing).unwrap();
        if mirror_drawing(src, &translated, dr) {
            if let Some(tag) = &tag {
                let sps: Vec<usize> = dr.iter(dr.root).into_iter().filter(|&e| dr.is(e, DSP, "sp")).collect();
                for sp in sps {
                    if dr.get(sp, "modelId").is_some_and(|m| src.iter().any(|(x, _)| x == m)) {
                        let ps: Vec<usize> = dr.iter(sp).into_iter().filter(|&e| dr.is(e, A, "p")).collect();
                        for p in ps {
                            stamp_language(dr, p, tag, rtl);
                        }
                    }
                }
            }
            touched.insert(drawing.clone());
        }
    }
    if touched.is_empty() {
        return Ok(original.to_vec());
    }
    let mut changed: HashMap<String, Vec<u8>> = HashMap::new();
    for name in &touched {
        let out = serialize_part(pkg.dom(name), pkg.bytes(name).unwrap_or_default());
        changed.insert(name.clone(), if pkg.strict { part_to_strict(&out) } else { out });
    }
    let mut parts: Vec<(EntryMeta, Body)> = Vec::with_capacity(pkg.entries.len());
    for e in &pkg.entries {
        let body = match (changed.remove(&e.name), &e.packed) {
            (Some(b), _) if !e.meta.dir => {
                parts.push((EntryMeta { stored: false, ..e.meta.clone() }, Body::Owned(b)));
                continue;
            }
            (_, Some((p, raw))) if !e.meta.stored && !e.meta.dir => Body::Packed(p, raw),
            _ => Body::Plain(&e.data),
        };
        parts.push((e.meta.clone(), body));
    }
    bilingua_zip::write_zip_parts(&parts)
}

/// Writes a document extracted under the profile (its skeleton holds the original file).
pub fn merge(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    let Skeleton::Binary { data, .. } = &doc.skeleton else {
        return Err(Error::Write("pptx: a LocOS-profile document needs its original file".into()));
    };
    let texts = written_texts(doc);
    rebuild(data, &texts, opts.stamp_lang(doc, true))
}
