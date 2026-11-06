//! The package as python-docx opens and saves it for LocOS: the parts reachable from the package relationships
//! (a depth-first walk), each with its content type; the XML parts python-docx keeps as trees (the document,
//! styles, settings, numbering, headers, footers, comments, core properties) parsed with its parser
//! (`remove_blank_text`), every other part as bytes. Saving writes what python-docx writes: a new
//! `[Content_Types].xml`, the relationship files, every part reachable (unreachable ones are dropped); an entry
//! that comes out as the same XML as the original keeps the original bytes. Strict Open XML packages go through
//! LocOS's `ooxml_strict` namespace swap both ways.

use super::tree;
use crate::zipio::{self, EntryMeta, RawEntry, ZipLimits};
use bilingua_model::{Error, Limits};
use bilingua_xmltree::Dom;
use std::collections::HashMap;

const RT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
pub const RT_CORE: &str = "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
const PKG_RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const WML_MAIN: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const WML_ALIASES: [&str; 3] = [
    "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml",
    "application/vnd.ms-word.document.macroEnabled.main+xml",
    "application/vnd.ms-word.template.macroEnabledTemplate.main+xml",
];
/// The content types python-docx loads as XML parts (its `PartFactory` table).
const XML_PARTS: [&str; 7] = [
    "application/vnd.openxmlformats-package.core-properties+xml",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
];
/// python-docx `default_content_types`: an (extension, type) pair written as a `Default`.
const DEFAULT_TYPES: [(&str, &str); 18] = [
    ("bin", "application/vnd.openxmlformats-officedocument.presentationml.printerSettings"),
    ("bin", "application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings"),
    ("bin", "application/vnd.openxmlformats-officedocument.wordprocessingml.printerSettings"),
    ("bmp", "image/bmp"),
    ("emf", "image/x-emf"),
    ("fntdata", "application/x-fontdata"),
    ("gif", "image/gif"),
    ("jpe", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("jpg", "image/jpeg"),
    ("png", "image/png"),
    ("rels", "application/vnd.openxmlformats-package.relationships+xml"),
    ("tif", "image/tiff"),
    ("tiff", "image/tiff"),
    ("wdp", "image/vnd.ms-photo"),
    ("wmf", "image/x-wmf"),
    ("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
    ("xml", "application/xml"),
];

pub fn rt(name: &str) -> String {
    format!("{RT}{name}")
}

#[derive(Clone, Debug)]
pub enum Target {
    Part(usize),
    External(String),
}

#[derive(Clone, Debug)]
pub struct Rel {
    pub id: String,
    pub reltype: String,
    pub target: Target,
}

pub struct Part {
    /// The part name without its leading slash (the zip entry name).
    pub name: String,
    pub ct: String,
    /// Kept by python-docx as a tree and written back from it.
    pub xml: bool,
    pub data: Vec<u8>,
    pub dom: Option<Dom>,
    /// A part kept as bytes whose tree LocOS wrote back (a footnotes or endnotes part a translation landed in).
    pub dirty: bool,
    pub rels: Vec<Rel>,
}

pub struct Pkg {
    pub parts: Vec<Part>,
    pub rels: Vec<Rel>,
    pub main: usize,
    /// The input entries as read (Strict names), and as normalised (Transitional names; the same when not Strict).
    original: Vec<RawEntry>,
    normalized: Vec<Vec<u8>>,
    strict: bool,
}

fn perr(msg: impl Into<String>) -> Error {
    super::perr(msg)
}

// ------------------------------------------------------------------------------------- Strict Open XML

const STRICT: &str = "http://purl.oclc.org/ooxml/";
const TRANS: &str = "http://schemas.openxmlformats.org/";
const STRICT_RELS: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
const TRANS_RELS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
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

fn refusal() -> Error {
    Error::Unsupported(
        "This DOCX file is saved as Strict Open XML with parts this version cannot read. Save it in the standard Office format (for example Word Document) and upload it again."
            .into(),
    )
}

fn map_strict(uri: &str) -> Option<String> {
    if uri == STRICT_RELS {
        return Some(TRANS_RELS.into());
    }
    if let Some(rest) = uri.strip_prefix(STRICT) {
        if let Some(&(_, t)) = NAMESPACES.iter().find(|(s, _)| *s == rest) {
            return Some(format!("{TRANS}{t}"));
        }
    }
    let name = uri.strip_prefix(STRICT_RELS)?.strip_prefix('/')?;
    let name = REL_RENAMES.iter().find(|(s, _)| *s == name).map(|(_, t)| *t).unwrap_or(name);
    Some(format!("{TRANS_RELS}/{name}"))
}

fn map_transitional(uri: &str) -> String {
    if uri == TRANS_RELS {
        return STRICT_RELS.into();
    }
    if let Some(rest) = uri.strip_prefix(TRANS) {
        if let Some(&(s, _)) = NAMESPACES.iter().find(|(_, t)| *t == rest) {
            return format!("{STRICT}{s}");
        }
    }
    if let Some(name) = uri.strip_prefix(TRANS_RELS).and_then(|r| r.strip_prefix('/')) {
        let name = REL_RENAMES.iter().find(|(_, t)| *t == name).map(|(s, _)| *s).unwrap_or(name);
        return format!("{STRICT_RELS}/{name}");
    }
    uri.to_string()
}

/// LocOS's quoted-URI substitution: every `"<prefix>..."` / `'<prefix>...'` standing alone in quotes (no quote,
/// `<`, `>` or white space inside) replaced by `f` of it; `f` returning None refuses the part.
fn swap_uris(xml: &[u8], prefix: &str, skip: &[&str], f: impl Fn(&str) -> Option<String>) -> Option<Vec<u8>> {
    let pre = prefix.as_bytes();
    let mut out = Vec::with_capacity(xml.len());
    let mut i = 0;
    while i < xml.len() {
        let q = xml[i];
        if (q == b'"' || q == b'\'') && xml[i + 1..].starts_with(pre) {
            let start = i + 1;
            let mut j = start + pre.len();
            while j < xml.len() && !matches!(xml[j], b'"' | b'\'' | b'<' | b'>' | b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
                j += 1;
            }
            let after = &xml[start + pre.len()..j];
            if j < xml.len() && xml[j] == q && !skip.iter().any(|s| after.starts_with(s.as_bytes())) {
                let uri = String::from_utf8_lossy(&xml[start..j]).to_string();
                out.push(q);
                out.extend_from_slice(f(&uri)?.as_bytes());
                out.push(q);
                i = j + 1;
                continue;
            }
        }
        out.push(q);
        i += 1;
    }
    Some(out)
}

fn xml_member(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".xml") || n.ends_with(".rels") || n.ends_with(".vml") || name == "[Content_Types].xml"
}

fn part_to_transitional(xml: &[u8]) -> Result<Vec<u8>, Error> {
    if !contains(xml, b"purl.oclc.org/ooxml/") {
        return Ok(xml.to_vec());
    }
    let out = swap_uris(xml, STRICT, &[], map_strict).ok_or_else(refusal)?;
    if contains(&out, b"\"http://purl.oclc.org/ooxml/") || contains(&out, b"'http://purl.oclc.org/ooxml/") {
        return Err(refusal());
    }
    Ok(out)
}

fn part_to_strict(xml: &[u8]) -> Vec<u8> {
    if !contains(xml, b"schemas.openxmlformats.org/") {
        return xml.to_vec();
    }
    swap_uris(xml, TRANS, &["package/", "markup-compatibility/"], |u| Some(map_transitional(u))).unwrap_or_else(|| xml.to_vec())
}

fn contains(h: &[u8], n: &[u8]) -> bool {
    h.windows(n.len()).any(|w| w == n)
}

// --------------------------------------------------------------------------------------------- paths

/// `posixpath.normpath` of an absolute path.
fn normpath(p: &str) -> String {
    let lead = if p.starts_with("//") && !p.starts_with("///") { "//" } else { "/" };
    let mut out: Vec<&str> = Vec::new();
    for c in p.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    format!("{lead}{}", out.join("/"))
}

/// `PackURI.from_rel_ref`.
fn from_rel_ref(base: &str, rel: &str) -> String {
    let joined = if rel.starts_with('/') {
        rel.to_string()
    } else if base.ends_with('/') {
        format!("{base}{rel}")
    } else {
        format!("{base}/{rel}")
    };
    normpath(&joined)
}

/// `PackURI.baseURI` (`posixpath.split(partname)[0]`): the directory of a part name, "/" at the top.
fn base_uri(partname: &str) -> String {
    let i = partname.rfind('/').map(|i| i + 1).unwrap_or(0);
    let head = &partname[..i];
    if !head.is_empty() && !head.bytes().all(|b| b == b'/') {
        head.trim_end_matches('/').to_string()
    } else {
        head.to_string()
    }
}

/// `PackURI.relative_ref`.
fn relative_ref(partname: &str, base: &str) -> String {
    if base == "/" {
        return partname[1..].to_string();
    }
    let p: Vec<&str> = partname.split('/').filter(|c| !c.is_empty()).collect();
    let b: Vec<&str> = base.split('/').filter(|c| !c.is_empty()).collect();
    let common = p.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    let mut out: Vec<&str> = vec![".."; b.len() - common];
    out.extend(&p[common..]);
    if out.is_empty() {
        ".".into()
    } else {
        out.join("/")
    }
}

/// The member name of a part's relationships (`PackURI.rels_uri`).
fn rels_member(partname: &str) -> String {
    if partname == "/" {
        return "_rels/.rels".into();
    }
    let (dir, file) = partname.rsplit_once('/').unwrap_or(("", partname));
    format!("{}/_rels/{file}.rels", dir).trim_start_matches('/').to_string()
}

/// `PackURI.ext`: what follows the last dot of the file name (a leading dot does not count).
fn ext(partname: &str) -> &str {
    let file = partname.rsplit('/').next().unwrap_or("");
    let core = file.trim_start_matches('.');
    match core.rfind('.') {
        Some(i) => &core[i + 1..],
        None => "",
    }
}

// --------------------------------------------------------------------------------------------- loading

#[derive(Clone)]
struct SRel {
    id: String,
    reltype: String,
    target: String,
    external: bool,
}

fn parse_xml(data: &[u8], what: &str) -> Result<Dom, Error> {
    let text = tree::decode(data).ok_or_else(|| perr(format!("{what} is not UTF-8 or UTF-16 XML")))?;
    bilingua_xmltree::parse_noblanks(&text).map_err(|e| perr(format!("{what}: {e}")))
}

impl Pkg {
    fn entry(&self, member: &str) -> Option<&[u8]> {
        self.original.iter().position(|e| e.name == member).map(|i| self.normalized[i].as_slice())
    }

    fn srels(&self, partname: &str) -> Result<Vec<SRel>, Error> {
        let member = rels_member(partname);
        let Some(data) = self.entry(&member) else { return Ok(Vec::new()) };
        let d = parse_xml(data, &member)?;
        let mut out = Vec::new();
        for r in d.element_children(d.root).filter(|&r| d.is(r, PKG_RELS_NS, "Relationship")) {
            let need = |k: &str| d.get(r, k).map(str::to_string).ok_or_else(|| perr(format!("{member}: a relationship has no {k}")));
            out.push(SRel { id: need("Id")?, reltype: need("Type")?, target: need("Target")?, external: d.get(r, "TargetMode") == Some("External") });
        }
        Ok(out)
    }

    /// Opens a package the way python-docx does.
    pub fn load(bytes: &[u8], limits: &Limits) -> Result<Pkg, Error> {
        let entries: Vec<RawEntry> = zipio::read_zip(bytes, &ZipLimits::from(limits))?.into_iter().filter(|e| !e.meta.dir).collect();
        bilingua_zip::check_xml_parts(&entries, limits)?;
        // empty paragraphs count too (LocOS DOCX_MAX_PARAGRAPHS), before any tree is built
        let mut paragraphs = 0usize;
        for e in entries.iter().filter(|e| e.name.starts_with("word/") && e.name.ends_with(".xml")) {
            paragraphs += bilingua_model::guard::count_elements(&e.data, "p");
            if paragraphs > limits.max_paragraphs {
                return Err(bilingua_model::LimitError::new(
                    bilingua_model::LimitKind::Paragraphs,
                    format!("This file has more than {} paragraphs. Split it into smaller files.", limits.max_paragraphs),
                )
                .into());
            }
        }
        let strict = entries.iter().any(|e| e.name == "_rels/.rels" && contains(&e.data, format!("{STRICT_RELS}/officeDocument").as_bytes()));
        let normalized = entries.iter().map(|e| if strict && xml_member(&e.name) { part_to_transitional(&e.data) } else { Ok(e.data.clone()) }).collect::<Result<Vec<_>, _>>()?;
        let mut pkg = Pkg { parts: Vec::new(), rels: Vec::new(), main: 0, original: entries, normalized, strict };

        // content types
        let ct = pkg.entry("[Content_Types].xml").ok_or_else(|| perr("not a Word document: no [Content_Types].xml"))?;
        let ctd = parse_xml(ct, "[Content_Types].xml")?;
        let mut overrides: HashMap<String, String> = HashMap::new();
        let mut defaults: HashMap<String, String> = HashMap::new();
        for c in ctd.element_children(ctd.root) {
            let need = |k: &str| ctd.get(c, k).map(str::to_string).ok_or_else(|| perr(format!("[Content_Types].xml: an entry has no {k}")));
            if ctd.is(c, CT_NS, "Override") {
                overrides.insert(need("PartName")?.to_lowercase(), need("ContentType")?);
            } else if ctd.is(c, CT_NS, "Default") {
                defaults.insert(need("Extension")?.to_lowercase(), need("ContentType")?);
            }
        }

        // the parts reachable from the package relationships, depth first
        let pkg_srels = pkg.srels("/")?;
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut part_srels: Vec<Vec<SRel>> = Vec::new();
        let mut stack: Vec<(String, Vec<SRel>, usize)> = vec![("/".into(), pkg_srels.clone(), 0)];
        while let Some(top) = stack.last_mut() {
            let Some(s) = top.1.get(top.2).cloned() else {
                stack.pop();
                continue;
            };
            top.2 += 1;
            if s.external {
                continue;
            }
            let partname = from_rel_ref(&top.0, &s.target);
            if index.contains_key(&partname) {
                continue;
            }
            let member = partname[1..].to_string();
            let data = pkg.entry(&member).ok_or_else(|| perr(format!("the package has no part {member}")))?.to_vec();
            let ct = overrides
                .get(&partname.to_lowercase())
                .or_else(|| defaults.get(&ext(&partname).to_lowercase()))
                .cloned()
                .ok_or_else(|| perr(format!("no content type for part {partname}")))?;
            let srels_here = pkg.srels(&partname)?;
            index.insert(partname.clone(), pkg.parts.len());
            // python-docx's PartFactory: an image relationship gives an image part, else the content type decides
            let xml = s.reltype != rt("image") && (ct == WML_MAIN || WML_ALIASES.contains(&ct.as_str()) || XML_PARTS.contains(&ct.as_str()));
            let dom = if xml { Some(parse_xml(&data, &member)?) } else { None };
            pkg.parts.push(Part { name: member, ct, xml, data, dom, dirty: false, rels: Vec::new() });
            part_srels.push(srels_here.clone());
            stack.push((base_uri(&partname), srels_here, 0));
        }
        let resolve = |base: &str, s: &SRel| -> Rel {
            let target = if s.external { Target::External(s.target.clone()) } else { Target::Part(index[&from_rel_ref(base, &s.target)]) };
            Rel { id: s.id.clone(), reltype: s.reltype.clone(), target }
        };
        let pkg_rels: Vec<Rel> = pkg_srels.iter().map(|s| resolve("/", s)).collect();
        for (i, srels) in part_srels.iter().enumerate() {
            let base = base_uri(&format!("/{}", pkg.parts[i].name));
            pkg.parts[i].rels = srels.iter().map(|s| resolve(&base, s)).collect();
        }
        pkg.rels = pkg_rels;
        let mains: Vec<&Rel> = pkg.rels.iter().filter(|r| r.reltype == rt("officeDocument")).collect();
        let main = match mains.as_slice() {
            [Rel { target: Target::Part(p), .. }] => *p,
            [] => return Err(perr("not a Word document: no main document part")),
            _ => return Err(perr("not a Word document: more than one main document part")),
        };
        let ct = &pkg.parts[main].ct;
        if ct != WML_MAIN && !WML_ALIASES.contains(&ct.as_str()) {
            return Err(perr(format!("not a Word file, content type is '{ct}'")));
        }
        pkg.main = main;
        Ok(pkg)
    }

    /// The tree of a part the walk reads (`_part_root`): an XML part's, or a bytes-kept part parsed now; false when
    /// it is not XML (left exactly as it was).
    pub fn open(&mut self, part: usize) -> bool {
        let p = &mut self.parts[part];
        if p.dom.is_some() {
            return true;
        }
        match tree::decode(&p.data).and_then(|t| bilingua_xmltree::parse_noblanks(&t).ok()) {
            Some(d) => {
                p.dom = Some(d);
                true
            }
            None => false,
        }
    }

    /// The first internal relationship of a part (`None`: the package) with this type.
    pub fn related(&self, from: Option<usize>, reltype: &str) -> Option<usize> {
        let rels = match from {
            Some(p) => &self.parts[p].rels,
            None => &self.rels,
        };
        rels.iter().find_map(|r| match r.target {
            Target::Part(t) if r.reltype == reltype => Some(t),
            _ => None,
        })
    }

    // ------------------------------------------------------------------------------------------ saving

    fn rels_xml(&self, rels: &[Rel], base: &str) -> Vec<u8> {
        let mut s = format!("<?xml version='1.0' encoding='UTF-8' standalone='yes'?>\n<Relationships xmlns=\"{PKG_RELS_NS}\"");
        if rels.is_empty() {
            s.push_str("/>");
            return s.into_bytes();
        }
        s.push('>');
        for r in rels {
            let (target, external) = match &r.target {
                Target::External(t) => (t.clone(), true),
                Target::Part(p) => (relative_ref(&format!("/{}", self.parts[*p].name), base), false),
            };
            s.push_str(&format!(
                "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"{}/>",
                bilingua_xmltree::esc_attr(&r.id),
                bilingua_xmltree::esc_attr(&r.reltype),
                bilingua_xmltree::esc_attr(&target),
                if external { " TargetMode=\"External\"" } else { "" }
            ));
        }
        s.push_str("</Relationships>");
        s.into_bytes()
    }

    fn content_types_xml(&self) -> Vec<u8> {
        let mut defaults: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        defaults.insert("rels".into(), "application/vnd.openxmlformats-package.relationships+xml".into());
        defaults.insert("xml".into(), "application/xml".into());
        let mut overrides: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        for p in &self.parts {
            let partname = format!("/{}", p.name);
            let e = ext(&partname).to_lowercase();
            if DEFAULT_TYPES.iter().any(|(x, c)| *x == e && *c == p.ct) {
                defaults.insert(e, p.ct.clone());
            } else {
                overrides.insert(partname, p.ct.clone());
            }
        }
        let mut s = format!("<?xml version='1.0' encoding='UTF-8' standalone='yes'?>\n<Types xmlns=\"{CT_NS}\">");
        for (e, c) in &defaults {
            s.push_str(&format!("<Default Extension=\"{}\" ContentType=\"{}\"/>", bilingua_xmltree::esc_attr(e), bilingua_xmltree::esc_attr(c)));
        }
        for (n, c) in &overrides {
            s.push_str(&format!("<Override PartName=\"{}\" ContentType=\"{}\"/>", bilingua_xmltree::esc_attr(n), bilingua_xmltree::esc_attr(c)));
        }
        s.push_str("</Types>");
        s.into_bytes()
    }

    /// The package as python-docx saves it, then mapped back to Strict names for a Strict original.
    pub fn save(&mut self) -> Result<Vec<u8>, Error> {
        let mut out: Vec<(String, Vec<u8>)> = Vec::new();
        out.push(("[Content_Types].xml".into(), self.content_types_xml()));
        out.push(("_rels/.rels".into(), self.rels_xml(&self.rels, "/")));
        for i in 0..self.parts.len() {
            let name = self.parts[i].name.clone();
            let bytes = {
                let p = &mut self.parts[i];
                match (&mut p.dom, p.xml || p.dirty) {
                    (Some(d), true) => tree::part_bytes(d),
                    _ => p.data.clone(),
                }
            };
            out.push((name.clone(), bytes));
            if !self.parts[i].rels.is_empty() {
                let partname = format!("/{name}");
                out.push((rels_member(&partname), self.rels_xml(&self.parts[i].rels, &base_uri(&partname))));
            }
        }
        let mut written: Vec<(EntryMeta, Vec<u8>)> = Vec::new();
        for (name, bytes) in out {
            let at = self.original.iter().position(|e| e.name == name);
            let mut bytes = bytes;
            if let Some(i) = at {
                let norm = &self.normalized[i];
                if *norm != bytes && xml_member(&name) && tree::canonical(norm).is_some_and(|c| Some(c) == tree::canonical(&bytes)) {
                    bytes = norm.clone();
                }
            }
            if self.strict {
                bytes = match at {
                    Some(i) if self.normalized[i] == bytes => self.original[i].data.clone(),
                    _ if xml_member(&name) => part_to_strict(&bytes),
                    _ => bytes,
                };
            }
            let meta = at.map(|i| self.original[i].meta.clone()).unwrap_or(EntryMeta { name: name.clone(), stored: false, dir: false, modified: None });
            written.push((meta, bytes));
        }
        bilingua_zip::write_zip(&written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_paths_like_python() {
        assert_eq!(from_rel_ref("/word", "styles.xml"), "/word/styles.xml");
        assert_eq!(from_rel_ref("/word", "../customXml/item1.xml"), "/customXml/item1.xml");
        assert_eq!(from_rel_ref("/", "word/document.xml"), "/word/document.xml");
        assert_eq!(from_rel_ref("/word", "/word/./a//b.xml"), "/word/a/b.xml");
        assert_eq!(base_uri("/word/document.xml"), "/word");
        assert_eq!(base_uri("/document.xml"), "/");
        assert_eq!(relative_ref("/customXml/item1.xml", "/word"), "../customXml/item1.xml");
        assert_eq!(relative_ref("/word/styles.xml", "/word"), "styles.xml");
        assert_eq!(rels_member("/word/document.xml"), "word/_rels/document.xml.rels");
        assert_eq!(rels_member("/"), "_rels/.rels");
        assert_eq!(ext("/word/media/image1.PNG"), "PNG");
        assert_eq!(ext("/_rels/.rels"), "");
    }

    #[test]
    fn strict_names_round_trip() {
        let s = br#"<w:document xmlns:w="http://purl.oclc.org/ooxml/wordprocessingml/main" xmlns:r='http://purl.oclc.org/ooxml/officeDocument/relationships'/>"#;
        let t = part_to_transitional(s).unwrap();
        assert!(contains(&t, b"\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\""));
        assert_eq!(part_to_strict(&t), s.to_vec());
        let rel = br#"<Relationship Type="http://purl.oclc.org/ooxml/officeDocument/relationships/extendedProperties"/>"#;
        let t = part_to_transitional(rel).unwrap();
        assert!(contains(&t, b"2006/relationships/extended-properties"));
        assert_eq!(part_to_strict(&t), rel.to_vec());
        assert!(part_to_transitional(br#"<a xmlns="http://purl.oclc.org/ooxml/unknown/thing"/>"#).is_err());
        // the package namespaces are never Strict names
        let pk = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#;
        assert_eq!(part_to_strict(pk), pk.to_vec());
    }
}
