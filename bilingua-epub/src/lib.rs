//! Bilingua EPUB filter (EPUB 2 and EPUB 3), matching the Localization OS `epub_filter`.
//!
//! An EPUB is a zip (OCF) with a stored `mimetype` first, `META-INF/container.xml` naming the package document
//! (OPF), and the resources its manifest lists. Units come in this fixed order:
//!
//! 1. the OPF `dc:title` and `dc:description` (ids `opf-1`, ...; style `epub:title`, `epub:description`);
//! 2. every XHTML content document in spine order, read through `bilingua-html` (ids `h1-u1`, ...; body units
//!    first, then the accessibility attributes LocOS offers), then the EPUB 3 navigation document when it is not
//!    in the spine (it takes the next chapter number);
//! 3. the EPUB 2 NCX labels (`ncx-1`, ...; style `epub:toc-label`).
//!
//! Everything else (CSS, fonts, images, audio, scripts, other META-INF files) is never touched. Drm is refused,
//! font obfuscation is fine. The skeleton holds the whole original file; merge opens it again and writes only
//! what changed:
//!
//! * nothing translated: the original bytes, exactly;
//! * something translated: `mimetype` first, stored, no extra field; every other member in its original order with
//!   its original compression (members that did not change are copied without recompression); changed members
//!   rewritten with only the text spans that changed (OPF and NCX) or through the HTML filter (XHTML);
//! * with a `target_lang` and at least one translation: `dc:language`, the package `xml:lang`, `page-progression-
//!   direction` for a right-to-left target, `lang`/`xml:lang`/`dir` on every XHTML `<html>` and the NCX `xml:lang`.
//!
//! Every member keeps its own encoding (UTF-8 with or without a mark, UTF-16, a declared legacy charset). An XML
//! member that is not UTF-8 or UTF-16 and declares nothing is refused. Targets are plain text: `&` is escaped,
//! characters XML 1.0 forbids are dropped.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod chapter;
mod dup;
mod lang;
mod pkg;
mod text;
mod xml;

use bilingua_model::*;
use bilingua_zip::{write_zip_parts, Body, ZipLimits};
use chapter::{translated, Chapter};
use pkg::Package;
use std::collections::HashMap;
use xml::Edits;

/// The skeleton entry that holds the whole original file.
const ARCHIVE: &str = "epub";
/// The skeleton entry that holds the options the file was read with. The unit ids depend on `include_locked`
/// (locked text is numbered too) and the limits decide what may be read again, so merge must use the same ones.
const OPTIONS: &str = "epub-options";

#[derive(serde::Serialize, serde::Deserialize)]
struct Opts {
    include_locked: bool,
    limits: Limits,
}

/// The EPUB filter.
pub struct Epub;

fn read_package(bytes: &[u8], limits: &Limits) -> Result<Vec<bilingua_zip::RawEntry>, Error> {
    if dup::has_duplicates(bytes) {
        return Err(pkg::refuse("lists the same file twice"));
    }
    bilingua_zip::read_zip("epub", bytes, &ZipLimits::from(limits))
}

fn dc_unit(id: String, el: &pkg::DcEl, path: &str, style: &str) -> Unit {
    let mut u = Unit::new(id, vec![Inline::text(el.core.clone())]);
    u.context.kind = Some(el.name.clone());
    u.context.style = Some(style.to_string());
    u.context.location = Some(format!("{path} {}", el.name));
    u
}

fn extract_epub(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let limits = opts.limits();
    bilingua_model::guard::check_input(bytes, &limits)?;
    let entries = read_package(bytes, &limits)?;
    let pkg = Package::open(&entries, &limits)?;
    let mut units: Vec<Unit> = Vec::new();

    for (k, el) in pkg.dc_elements().iter().enumerate() {
        let mut u = dc_unit(format!("opf-{}", k + 1), el, &pkg.opf_path, &format!("epub:{}", el.name));
        if !pkg.version.is_empty() {
            u.context.extra.insert("epub-version".into(), pkg.version.clone());
        }
        units.push(u);
    }
    guard::check_unit_count(units.len(), &limits)?;

    let chapters: Vec<usize> = pkg.xhtml.iter().copied().chain(pkg.nav).collect();
    for (c, &ei) in chapters.iter().enumerate() {
        let ch = Chapter::open(&entries[ei].data, &entries[ei].name, opts)?;
        units.extend(ch.units(c + 1));
        guard::check_unit_count(units.len(), &limits)?;
    }

    if let Some(ni) = pkg.ncx {
        let part = pkg::load_xml(&entries[ni].data, "its NCX table of contents", &limits)?;
        for (k, el) in pkg::ncx_elements(&part).iter().enumerate() {
            units.push(dc_unit(format!("ncx-{}", k + 1), el, &entries[ni].name, "epub:toc-label"));
        }
        guard::check_unit_count(units.len(), &limits)?;
    }

    let lang = pkg.first_language().and_then(|i| xml::text_only(&pkg.opf.dom, &pkg.opf.t.text, i)).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let doc = Document {
        format: "epub".into(),
        source_lang: opts.source_lang.clone().or(lang),
        target_lang: opts.target_lang.clone(),
        granularity: Granularity::Paragraph,
        units,
        skeleton: Skeleton::zip(
            vec![
                ZipEntry { name: ARCHIVE.into(), data: ZipData::Raw(bytes.to_vec()) },
                ZipEntry {
                    name: OPTIONS.into(),
                    data: ZipData::Raw(serde_json::to_vec(&Opts { include_locked: opts.include_locked, limits }).map_err(|e| Error::Write(e.to_string()))?),
                },
            ],
            bilingua_model::parser_info!(),
        ),
        ..Default::default()
    };
    guard::finish(doc, vec![], &limits)
}

/// Replaces the text of a metadata or label element when its unit was translated to something different.
fn set_text(edits: &mut Edits, p: &pkg::Part, el: &pkg::DcEl, unit: Option<&&Unit>) {
    let Some(u) = unit else { return };
    if !translated(u) {
        return;
    }
    let new = text::xml_target_text(&plain_text(&u.output()));
    if new == el.core {
        return;
    }
    let n = p.dom.node(el.node);
    edits.replace(n.open_end, n.close_start, xml::escape_text(&format!("{}{new}{}", el.lead, el.trail)));
}

fn stamp_opf(edits: &mut Edits, pkg: &Package, tag: &str, rtl: bool) {
    let d = &pkg.opf.dom;
    if let Some(i) = pkg.first_language() {
        if let Some(t) = xml::text_only(d, &pkg.opf.t.text, i) {
            let (lead, core, trail) = xml::split_ws(&t);
            if core != tag {
                let n = d.node(i);
                edits.replace(n.open_end, n.close_start, xml::escape_text(&format!("{lead}{tag}{trail}")));
            }
        }
    }
    let root = d.node(d.root);
    if root.attr_ns(xml::XML_NS, "lang").is_some() {
        xml::set_attr(edits, root, "xml", "lang", xml::XML_NS, tag, false);
    }
    if let Some(s) = pkg.spine {
        let sp = d.node(s);
        let cur = sp.attr("page-progression-direction").map(|a| a.value.as_str());
        if rtl && cur != Some("rtl") {
            xml::set_attr(edits, sp, "", "page-progression-direction", "", "rtl", true);
        } else if !rtl && cur == Some("rtl") {
            xml::set_attr(edits, sp, "", "page-progression-direction", "", "ltr", true);
        }
    }
}

fn merge_epub(doc: &Document) -> Result<Vec<u8>, Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Zip { entries: skel, .. } = &doc.skeleton else {
        return Err(Error::Write("epub needs a zip skeleton".into()));
    };
    let original = skel
        .iter()
        .find_map(|e| match (&e.data, e.name == ARCHIVE) {
            (ZipData::Raw(b), true) => Some(b.as_slice()),
            _ => None,
        })
        .ok_or_else(|| Error::Write("epub skeleton has no archive".into()))?;
    // Nothing translated: the original bytes, without opening the package again.
    if !doc.units.iter().any(translated) {
        return Ok(original.to_vec());
    }
    let read_with = skel
        .iter()
        .find_map(|e| match (&e.data, e.name == OPTIONS) {
            (ZipData::Raw(b), true) => Some(serde_json::from_slice::<Opts>(b)),
            _ => None,
        })
        .transpose()
        .map_err(|e| Error::Write(format!("the epub skeleton's options are unreadable: {e}")))?
        .unwrap_or(Opts { include_locked: false, limits: Limits::default() });
    let limits = read_with.limits;
    let entries = read_package(original, &limits)?;
    let pkg = Package::open(&entries, &limits)?;
    let by_id: HashMap<&str, &Unit> = doc.units.iter().map(|u| (u.id.as_str(), u)).collect();
    // The language is declared only when something was translated (an untouched merge returned above).
    let lang = doc.target_lang.as_deref().and_then(|l| chapter::target(Some(l)));
    let lang_ref = lang.as_ref().map(|(t, r)| (t.as_str(), *r));
    let mut replaced: HashMap<usize, Vec<u8>> = HashMap::new();

    // 1. OPF
    let mut edits = Edits::default();
    for (k, el) in pkg.dc_elements().iter().enumerate() {
        set_text(&mut edits, &pkg.opf, el, by_id.get(format!("opf-{}", k + 1).as_str()));
    }
    if let Some((tag, rtl)) = lang_ref {
        stamp_opf(&mut edits, &pkg, tag, rtl);
    }
    if !edits.is_empty() {
        let out = pkg.opf.t.encode(&edits.apply(&pkg.opf.t.text));
        if out != entries[pkg.opf_idx].data {
            replaced.insert(pkg.opf_idx, out);
        }
    }

    // 2. XHTML content documents in spine order, then the navigation document
    let copts = ExtractOptions { include_locked: read_with.include_locked, limits: Some(limits), ..Default::default() };
    let chapters: Vec<usize> = pkg.xhtml.iter().copied().chain(pkg.nav).collect();
    for (c, &ei) in chapters.iter().enumerate() {
        let mut ch = Chapter::open(&entries[ei].data, &entries[ei].name, &copts)?;
        if let Some(out) = ch.render(c + 1, &by_id, lang_ref)? {
            if out != entries[ei].data {
                replaced.insert(ei, out);
            }
        }
    }

    // 3. NCX
    if let Some(ni) = pkg.ncx {
        let part = pkg::load_xml(&entries[ni].data, "its NCX table of contents", &limits)?;
        let mut edits = Edits::default();
        for (k, el) in pkg::ncx_elements(&part).iter().enumerate() {
            set_text(&mut edits, &part, el, by_id.get(format!("ncx-{}", k + 1).as_str()));
        }
        if let Some((tag, _)) = lang_ref {
            let root = part.dom.node(part.dom.root);
            if root.attr_ns(xml::XML_NS, "lang").is_some() {
                xml::set_attr(&mut edits, root, "xml", "lang", xml::XML_NS, tag, false);
            }
        }
        if !edits.is_empty() {
            let out = part.t.encode(&edits.apply(&part.t.text));
            if out != entries[ni].data {
                replaced.insert(ni, out);
            }
        }
    }

    if replaced.is_empty() {
        return Ok(original.to_vec());
    }
    write_epub(&entries, replaced)
}

/// `mimetype` first, stored, with no extra field; every other member in its original order, compression and time
/// stamp, content replaced only where `replaced` says so.
fn write_epub(entries: &[bilingua_zip::RawEntry], mut replaced: HashMap<usize, Vec<u8>>) -> Result<Vec<u8>, Error> {
    let mt = entries
        .iter()
        .position(|e| e.name == "mimetype" && !e.meta.dir)
        .ok_or_else(|| pkg::refuse("is missing mimetype"))?;
    let mut parts: Vec<(bilingua_zip::EntryMeta, Body<'_>)> = Vec::with_capacity(entries.len());
    let mut meta = entries[mt].meta.clone();
    meta.stored = true;
    parts.push((meta, Body::Owned(pkg::MIMETYPE.to_vec())));
    for (i, e) in entries.iter().enumerate() {
        if i == mt {
            continue;
        }
        let body = match (replaced.remove(&i), &e.packed) {
            (Some(b), _) => Body::Owned(b),
            (None, Some((p, bytes))) => Body::Packed(p, bytes),
            (None, None) => Body::Plain(&e.data),
        };
        parts.push((e.meta.clone(), body));
    }
    write_zip_parts(&parts)
}

impl Filter for Epub {
    fn id(&self) -> &'static str {
        "epub"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["epub"]
    }

    /// A zip whose first member is `mimetype` holding `application/epub+zip` (the OCF form), or one that names
    /// `META-INF/container.xml` and an `.opf` package. The mimetype must be the first member: a Word file that
    /// stores a part mentioning `application/epub+zip` is not an EPUB. Strict enough to sit anywhere in the list.
    fn sniff(&self, bytes: &[u8]) -> bool {
        if !bytes.starts_with(b"PK\x03\x04") {
            return false;
        }
        let first = || -> Option<bool> {
            let n = u16::from_le_bytes([*bytes.get(26)?, *bytes.get(27)?]) as usize;
            let x = u16::from_le_bytes([*bytes.get(28)?, *bytes.get(29)?]) as usize;
            let name = bytes.get(30..30 + n)?;
            let data = bytes.get(30 + n + x..)?;
            Some(name == b"mimetype" && data.starts_with(pkg::MIMETYPE))
        };
        let has = |w: &[u8]| bytes.windows(w.len()).any(|x| x == w);
        first().unwrap_or(false) || (has(pkg::CONTAINER.as_bytes()) && has(b".opf"))
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        extract_epub(bytes, opts)
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        merge_epub(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_wants_the_mimetype_first_or_a_container() {
        use std::io::Write;
        let zip = |members: &[(&str, &[u8])]| {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for (n, d) in members {
                w.start_file(*n, o).unwrap();
                w.write_all(d).unwrap();
            }
            w.finish().unwrap().into_inner()
        };
        assert!(Epub.sniff(&zip(&[("mimetype", pkg::MIMETYPE), ("a.xhtml", b"<p/>")])));
        assert!(Epub.sniff(&zip(&[("META-INF/container.xml", b"<container/>"), ("OEBPS/content.opf", b"<package/>"), ("mimetype", pkg::MIMETYPE)])));
        // a stored Word part that only mentions the EPUB mimetype
        assert!(!Epub.sniff(&zip(&[("[Content_Types].xml", b"<Types/>"), ("word/document.xml", b"<w:document>about application/epub+zip</w:document>")])));
        assert!(!Epub.sniff(b"PK\x03\x04"));
    }

    #[test]
    fn reserved_html_code_id_is_still_the_attribute_marker() {
        // chapter.rs reads `bilingua-html`'s reserved "~v" code to tell attribute units from body units.
        let d = bilingua_html::Html
            .extract(b"<p><img src=\"a\" alt=\"Cover art\"/></p>", &ExtractOptions::default())
            .unwrap();
        assert_eq!(d.units.len(), 1);
        assert!(d.units[0].codes.get("~v").is_some_and(|c| c.label.as_deref() == Some("attr")));
    }
}
