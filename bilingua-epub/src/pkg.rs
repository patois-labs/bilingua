//! The EPUB container: `mimetype`, `META-INF/container.xml`, the OPF package document, the spine, the navigation
//! document, the NCX and the encryption check. Extraction and merge both open the package the same way from the
//! same bytes, so they walk the same units in the same order.

use crate::text::{self, Text};
use crate::xml::{self, Dom, XmlError};
use bilingua_model::{Error, LimitError, LimitKind, Limits};
use bilingua_zip::RawEntry;
use std::collections::HashMap;

pub const MIMETYPE: &[u8] = b"application/epub+zip";
pub const CONTAINER: &str = "META-INF/container.xml";
pub const ENCRYPTION: &str = "META-INF/encryption.xml";
const OPF_MEDIA: &str = "application/oebps-package+xml";
const XHTML_MEDIA: &str = "application/xhtml+xml";
const NCX_MEDIA: &str = "application/x-dtbncx+xml";

pub const NS_CONTAINER: &str = "urn:oasis:names:tc:opendocument:xmlns:container";
pub const NS_OPF: &str = "http://www.idpf.org/2007/opf";
pub const NS_DC: &str = "http://purl.org/dc/elements/1.1/";
pub const NS_NCX: &str = "http://www.daisy.org/z3986/2005/ncx/";
const NS_ENC: &str = "http://www.w3.org/2001/04/xmlenc#";

/// Font obfuscation, not DRM (EPUB 3.3 section 4.4; the Adobe one is the legacy variant).
const OBFUSCATION: &[&str] = &["http://www.idpf.org/2008/embedding", "http://ns.adobe.com/pdf/enc#RC"];

pub fn refuse(msg: impl std::fmt::Display) -> Error {
    Error::Parse { format: "epub".into(), msg: format!("This EPUB file {msg}. It was not processed.") }
}

pub fn alnum(s: &str) -> bool {
    bilingua_model::py_has_alnum(s)
}

/// A parsed package XML part: the decoded text with its envelope, and the element table.
pub struct Part {
    pub t: Text,
    pub dom: Dom,
}

pub fn xml_err(e: XmlError, what: &str, limits: &Limits) -> Error {
    match e {
        XmlError::Syntax(m) => refuse(format!("has a damaged {what} ({})", m.chars().take(120).collect::<String>())),
        XmlError::Depth => LimitError::new(
            LimitKind::XmlDepth,
            format!("This file nests XML elements more than {} deep. It was not processed.", limits.max_xml_depth),
        )
        .into(),
    }
}

/// Opens one package XML part: the XML guard (entities, depth, element count), a markup budget, the decode and the
/// strict parse.
pub fn load_xml(bytes: &[u8], what: &str, limits: &Limits) -> Result<Part, Error> {
    bilingua_model::guard::check_xml(bytes, limits)?;
    let t = text::read(bytes, what, false)?;
    // forty markup characters per allowed unit bound the element table before it exists
    if t.text.bytes().filter(|b| *b == b'<').count() > 40 * limits.max_units {
        return Err(LimitError::new(
            LimitKind::XmlElements,
            format!("This EPUB file has far more markup in {what} than the {} unit limit allows. It was not processed.", limits.max_units),
        )
        .into());
    }
    let dom = xml::parse(&t.text, limits.max_xml_depth).map_err(|e| xml_err(e, what, limits))?;
    Ok(Part { t, dom })
}

fn unquote(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = ((b[i + 1] as char).to_digit(16), (b[i + 2] as char).to_digit(16)) {
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

/// Resolves a manifest href (a URL relative to the package document, EPUB 3.3 5.2) to a container member name, or
/// `None` for a remote, absolute or escaping reference.
pub fn href_path(base_dir: &str, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() {
        return None;
    }
    let has_scheme = href.find(':').is_some_and(|p| {
        p > 0 && href[..p].starts_with(|c: char| c.is_ascii_alphabetic()) && href[..p].chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    });
    if has_scheme || href.starts_with("//") {
        return None;
    }
    let path = href.split(['?', '#']).next().unwrap_or("");
    if path.starts_with('/') {
        return None;
    }
    let joined = if base_dir.is_empty() { unquote(path) } else { format!("{base_dir}/{}", unquote(path)) };
    let mut stack: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if stack.last().is_some_and(|l| *l != "..") {
                    stack.pop();
                } else {
                    stack.push("..");
                }
            }
            p => stack.push(p),
        }
    }
    if stack.is_empty() || stack[0] == ".." {
        return None;
    }
    Some(stack.join("/"))
}

pub struct Item {
    pub path: Option<String>,
    pub media: String,
    pub props: Vec<String>,
}

/// One translatable `dc:title` or `dc:description` element.
pub struct DcEl {
    pub node: usize,
    pub name: String,
    pub core: String,
    pub lead: String,
    pub trail: String,
}

pub struct Package {
    pub opf_idx: usize,
    pub opf_path: String,
    pub opf: Part,
    pub version: String,
    pub spine: Option<usize>,
    /// Entry indexes of the spine XHTML documents, in reading order.
    pub xhtml: Vec<usize>,
    pub nav: Option<usize>,
    pub ncx: Option<usize>,
}

impl Package {
    pub fn open(entries: &[RawEntry], limits: &Limits) -> Result<Package, Error> {
        let names: HashMap<&str, usize> = entries.iter().enumerate().filter(|(_, e)| !e.meta.dir).map(|(i, e)| (e.name.as_str(), i)).collect();
        let read = |name: &str, required: bool| -> Result<Option<usize>, Error> {
            match names.get(name) {
                Some(&i) => Ok(Some(i)),
                None if required => Err(refuse(format!("is missing {name}"))),
                None => Ok(None),
            }
        };
        let mt = read("mimetype", true)?.expect("required");
        if entries[mt].data.trim_ascii() != MIMETYPE {
            return Err(refuse("has no application/epub+zip mimetype"));
        }
        let cont = read(CONTAINER, true)?.expect("required");
        let c = load_xml(&entries[cont].data, CONTAINER, limits)?;
        let mut opf_path = None;
        for i in c.dom.all() {
            let n = c.dom.node(i);
            if !n.is(NS_CONTAINER, "rootfile") {
                continue;
            }
            let media = n.attr("media-type").map(|a| a.value.trim().to_lowercase()).unwrap_or_default();
            let path = href_path("", n.attr("full-path").map_or("", |a| a.value.as_str()));
            if media == OPF_MEDIA {
                if let Some(p) = path {
                    opf_path = Some(p);
                    break;
                }
            }
        }
        let opf_path = opf_path.ok_or_else(|| refuse("names no package document in META-INF/container.xml"))?;
        let opf_idx = read(&opf_path, true)?.expect("required");
        let opf = load_xml(&entries[opf_idx].data, "its package document", limits)?;
        let root = opf.dom.node(opf.dom.root);
        if !root.is(NS_OPF, "package") {
            return Err(refuse("has a package document that is not an OPF package"));
        }
        let version = root.attr("version").map(|a| a.value.trim().to_string()).unwrap_or_default();
        let base = opf_path.rsplit_once('/').map_or("", |(d, _)| d).to_string();

        // manifest: later items with the same id replace earlier ones but keep the first position
        let mut manifest: Vec<(String, Item)> = Vec::new();
        let mut by_id: HashMap<String, usize> = HashMap::new();
        for i in opf.dom.all() {
            let n = opf.dom.node(i);
            if !n.is(NS_OPF, "item") {
                continue;
            }
            let id = n.attr("id").map(|a| a.value.clone()).unwrap_or_default();
            let item = Item {
                path: href_path(&base, n.attr("href").map_or("", |a| a.value.as_str())),
                media: n.attr("media-type").map(|a| a.value.trim().to_lowercase()).unwrap_or_default(),
                props: n.attr("properties").map(|a| a.value.split_whitespace().map(str::to_string).collect()).unwrap_or_default(),
            };
            match by_id.get(&id) {
                Some(&k) => manifest[k].1 = item,
                None => {
                    by_id.insert(id.clone(), manifest.len());
                    manifest.push((id, item));
                }
            }
        }
        let spine = opf.dom.child(opf.dom.root, NS_OPF, "spine");
        let mut xhtml = Vec::new();
        let mut seen: Vec<usize> = Vec::new();
        if let Some(s) = spine {
            for d in opf.dom.descendants(s) {
                let n = opf.dom.node(d);
                if !n.is(NS_OPF, "itemref") {
                    continue;
                }
                let idref = n.attr("idref").map_or("", |a| a.value.as_str());
                let Some(&k) = by_id.get(idref) else { continue };
                let item = &manifest[k].1;
                let Some(path) = item.path.as_deref() else { continue };
                if item.media != XHTML_MEDIA {
                    continue;
                }
                let Some(&ei) = names.get(path) else { continue };
                if seen.contains(&ei) {
                    continue;
                }
                seen.push(ei);
                xhtml.push(ei);
            }
        }
        let mut nav = None;
        for (_, item) in &manifest {
            if item.props.iter().any(|p| p == "nav") {
                if let Some(&ei) = item.path.as_deref().and_then(|p| names.get(p)) {
                    if !seen.contains(&ei) && item.media == XHTML_MEDIA {
                        nav = Some(ei);
                    }
                    break;
                }
            }
        }
        let toc_id = spine.and_then(|s| opf.dom.node(s).attr("toc")).map(|a| a.value.clone());
        let mut cand = toc_id.as_deref().and_then(|t| by_id.get(t)).map(|&k| &manifest[k].1);
        if cand.is_none_or(|c| c.media != NCX_MEDIA) {
            cand = manifest.iter().map(|(_, i)| i).find(|i| i.media == NCX_MEDIA);
        }
        let ncx = cand
            .and_then(|c| c.path.as_deref())
            .and_then(|p| names.get(p).copied())
            .filter(|ei| !seen.contains(ei));
        let pkg = Package { opf_idx, opf_path, opf, version, spine, xhtml, nav, ncx };
        if let Some(enc) = read(ENCRYPTION, false)? {
            pkg.check_encryption(&entries[enc].data, entries, limits)?;
        }
        Ok(pkg)
    }

    /// Refuses DRM: anything encrypted that is not font obfuscation of a resource we do not translate.
    fn check_encryption(&self, raw: &[u8], entries: &[RawEntry], limits: &Limits) -> Result<(), Error> {
        let p = load_xml(raw, ENCRYPTION, limits)?;
        let mut ours: Vec<&str> = vec![self.opf_path.as_str()];
        for &i in self.xhtml.iter().chain(self.nav.iter()).chain(self.ncx.iter()) {
            ours.push(entries[i].name.as_str());
        }
        for i in p.dom.all() {
            let ed = p.dom.node(i);
            if !ed.is(NS_ENC, "EncryptedData") {
                continue;
            }
            let alg = p.dom.child(i, NS_ENC, "EncryptionMethod").and_then(|m| p.dom.node(m).attr("Algorithm")).map(|a| a.value.trim().to_string()).unwrap_or_default();
            let target = p
                .dom
                .child(i, NS_ENC, "CipherData")
                .and_then(|cd| p.dom.child(cd, NS_ENC, "CipherReference"))
                .and_then(|cr| p.dom.node(cr).attr("URI"))
                .and_then(|u| href_path("", &u.value));
            if !OBFUSCATION.contains(&alg.as_str()) || target.as_deref().is_none_or(|t| ours.contains(&t)) {
                return Err(Error::Unsupported(
                    "This EPUB file is encrypted (DRM protected). Only font obfuscation is supported; export a DRM-free EPUB from the source. It was not processed.".into(),
                ));
            }
        }
        Ok(())
    }

    /// The `dc:title` and `dc:description` elements with text to translate, in document order.
    pub fn dc_elements(&self) -> Vec<DcEl> {
        let d = &self.opf.dom;
        let Some(meta) = d.child(d.root, NS_OPF, "metadata") else { return vec![] };
        let mut out = Vec::new();
        for i in d.descendants(meta) {
            let n = d.node(i);
            if n.ns != NS_DC || !(n.local == "title" || n.local == "description") {
                continue;
            }
            let Some(t) = xml::text_only(d, &self.opf.t.text, i) else { continue };
            if !alnum(&t) {
                continue;
            }
            let (lead, core, trail) = xml::split_ws(&t);
            out.push(DcEl { node: i, name: n.local.clone(), core: core.to_string(), lead: lead.to_string(), trail: trail.to_string() });
        }
        out
    }

    /// The first `dc:language` element of the metadata.
    pub fn first_language(&self) -> Option<usize> {
        let d = &self.opf.dom;
        let meta = d.child(d.root, NS_OPF, "metadata")?;
        d.descendants(meta).find(|&i| d.node(i).is(NS_DC, "language"))
    }
}

/// An NCX table of contents: its label `text` elements.
pub fn ncx_elements(p: &Part) -> Vec<DcEl> {
    let d = &p.dom;
    let mut out = Vec::new();
    for i in d.all() {
        let n = d.node(i);
        if !n.is(NS_NCX, "text") {
            continue;
        }
        let Some(par) = n.parent else { continue };
        let pn = d.node(par);
        if !(pn.ns == NS_NCX && matches!(pn.local.as_str(), "docTitle" | "navLabel" | "navInfo")) {
            continue;
        }
        let Some(t) = xml::text_only(d, &p.t.text, i) else { continue };
        if !alnum(&t) {
            continue;
        }
        let (lead, core, trail) = xml::split_ws(&t);
        out.push(DcEl { node: i, name: "toc-label".into(), core: core.to_string(), lead: lead.to_string(), trail: trail.to_string() });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hrefs() {
        assert_eq!(href_path("OEBPS", "text/chapter%202.xhtml").as_deref(), Some("OEBPS/text/chapter 2.xhtml"));
        assert_eq!(href_path("OEBPS", "../x.xhtml#f").as_deref(), Some("x.xhtml"));
        for bad in ["../../x", "/x", "http://h/x", "C:/x", "//h/x", "", "..", "%2e%2e/%2e%2e/x"] {
            assert_eq!(href_path("OEBPS", bad), None, "{bad}");
        }
        assert_eq!(href_path("", "a/./b//c.xhtml").as_deref(), Some("a/b/c.xhtml"));
    }
}
