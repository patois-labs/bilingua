//! Bilingua DOCX filter: Word documents to units and back. See the README for what is and is not supported.
//!
//! ```ignore
//! let f = bilingua_docx::Docx;
//! let mut doc = f.extract(&bytes, &ExtractOptions::default())?;
//! doc.set_target("p3", vec![Inline::text("Hola.")])?;
//! let (out, report) = bilingua_docx::merge_with_report(&doc)?;
//! ```

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 2;

mod extract;
mod preview;
mod render;
mod xml;
mod zipio;

pub use render::MergeReport;

use bilingua_model::*;
use extract::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zipio::{EntryMeta, ZipLimits};

/// The name of the pseudo entry that carries zip details (compression, times) and the namespace prefix.
/// It holds a NUL, which no real zip entry name can (the reader refuses control characters), and it is never
/// written to the output file.
const META_NAME: &str = "\0bilingua-docx-meta";

#[derive(Serialize, Deserialize)]
struct Meta {
    /// Prefix of the WordprocessingML namespace, with the colon ("w:").
    wp: String,
    /// Name of the main document part.
    main: String,
    entries: Vec<EntryMeta>,
    /// Entries whose skeleton bytes are the original deflated bytes (merge copies them without recompressing).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    packed: std::collections::BTreeMap<String, zipio::Packed>,
}

pub struct Docx;

fn perr(msg: impl Into<String>) -> Error {
    Error::Parse {
        format: "docx".into(),
        msg: msg.into(),
    }
}

fn main_part(content_types: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(content_types).ok()?;
    let nodes = xml::parse(text).ok()?;
    fn walk(nodes: &[xml::Node]) -> Option<String> {
        for n in nodes {
            if let xml::Node::El(e) = n {
                if xml::local_name(&e.name) == "Override" {
                    let ct = e.attr("ContentType").unwrap_or_default();
                    if ct.contains("wordprocessingml.document.main")
                        || ct.contains("wordprocessingml.template.main")
                    {
                        return e
                            .attr("PartName")
                            .map(|p| p.trim_start_matches('/').to_string());
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

fn classify(name: &str, main: &str, dir: &str) -> Option<PartKind> {
    if name == main {
        return Some(PartKind::Main);
    }
    if name == "docProps/core.xml" {
        return Some(PartKind::Core);
    }
    let rest = name.strip_prefix(dir)?;
    if rest.contains('/') {
        return None;
    }
    let stem = rest.strip_suffix(".xml")?;
    let numbered = |p: &str| {
        stem.strip_prefix(p)
            .is_some_and(|d| d.chars().all(|c| c.is_ascii_digit()))
    };
    if numbered("header") {
        Some(PartKind::Header)
    } else if numbered("footer") {
        Some(PartKind::Footer)
    } else if stem == "footnotes" {
        Some(PartKind::Footnotes)
    } else if stem == "endnotes" {
        Some(PartKind::Endnotes)
    } else if stem == "comments" {
        Some(PartKind::Comments)
    } else {
        None
    }
}

fn has_marker_chars(s: &str) -> bool {
    s.chars().any(|c| ('\u{E000}'..='\u{E003}').contains(&c))
}

fn texts_of(ns: &xml::Ns, e: &xml::El, out: &mut String) {
    for c in e.elements() {
        if ns.is_w(&c.name, "t") {
            out.push_str(&c.text());
        } else {
            texts_of(ns, c, out);
        }
    }
}

fn read_comments(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(nodes) = xml::parse(text) else {
        return map;
    };
    let ns = xml::Ns::detect(&nodes);
    fn walk(ns: &xml::Ns, nodes: &[xml::Node], map: &mut HashMap<String, String>) {
        for n in nodes {
            if let xml::Node::El(e) = n {
                if ns.is_w(&e.name, "comment") {
                    let id = ns.attr(e, "id").unwrap_or_default();
                    let author = ns.attr(e, "author").unwrap_or_default();
                    let mut paras = Vec::new();
                    for p in e.elements().filter(|p| ns.is_w(&p.name, "p")) {
                        let mut t = String::new();
                        texts_of(ns, p, &mut t);
                        if !t.trim().is_empty() {
                            paras.push(t.trim().to_string());
                        }
                    }
                    let body = paras.join(" ");
                    let note = if author.is_empty() {
                        format!("Comment: {body}")
                    } else {
                        format!("Comment ({author}): {body}")
                    };
                    map.insert(id, note);
                } else {
                    walk(ns, &e.children, map);
                }
            }
        }
    }
    walk(&ns, &nodes, &mut map);
    map
}

fn split_template(t: &str, idx: &HashMap<&str, usize>) -> Result<Vec<SkelPart>, Error> {
    let mut parts = Vec::new();
    let mut rest = t;
    while let Some(p) = rest.find(M_UNIT) {
        if p > 0 {
            parts.push(SkelPart::Raw(rest[..p].to_string()));
        }
        let after = &rest[p + M_UNIT.len_utf8()..];
        let end = after
            .find(M_END)
            .ok_or_else(|| perr("internal: unterminated unit marker"))?;
        let id = &after[..end];
        let i = *idx
            .get(id)
            .ok_or_else(|| perr(format!("internal: unknown unit {id}")))?;
        parts.push(SkelPart::Unit(i));
        rest = &after[end + M_END.len_utf8()..];
    }
    if !rest.is_empty() {
        parts.push(SkelPart::Raw(rest.to_string()));
    }
    Ok(parts)
}

fn extract_doc(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let entries = zipio::read_zip(bytes, &ZipLimits::default())?;
    let ct = entries
        .iter()
        .find(|e| e.name == "[Content_Types].xml")
        .ok_or_else(|| perr("not a Word document: no [Content_Types].xml"))?;
    let main = main_part(&ct.data).unwrap_or_else(|| "word/document.xml".into());
    if !entries.iter().any(|e| e.name == main) {
        return Err(perr("not a Word document: no main document part"));
    }
    let dir = main
        .rsplit_once('/')
        .map(|(d, _)| format!("{d}/"))
        .unwrap_or_default();

    // Styles and comments first: paragraphs need them for their context.
    let mut styles = Styles::default();
    if let Some(e) = entries
        .iter()
        .find(|e| e.name == format!("{dir}styles.xml"))
    {
        if let Some(nodes) = std::str::from_utf8(&e.data)
            .ok()
            .and_then(|t| xml::parse(t).ok())
        {
            let ns = xml::Ns::detect(&nodes);
            styles = Styles::parse(&nodes, &ns);
        }
    }
    let mut comments = HashMap::new();
    if let Some(e) = entries
        .iter()
        .find(|e| e.name == format!("{dir}comments.xml"))
    {
        if let Ok(t) = std::str::from_utf8(&e.data) {
            comments = read_comments(t);
        }
    }
    let lang_default = styles.lang.clone();

    // Parts in processing order: the main part first.
    let mut order: Vec<(usize, PartKind)> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if let Some(k) = classify(&e.name, &main, &dir) {
            if k == PartKind::Main {
                order.insert(0, (i, k));
            } else {
                order.push((i, k));
            }
        }
    }

    let mut ex = Extractor::new(styles, comments);
    let mut templates: HashMap<usize, String> = HashMap::new();
    let mut wp = "w:".to_string();
    for (i, kind) in order {
        let e = &entries[i];
        let is_main = kind == PartKind::Main;
        let text = match std::str::from_utf8(&e.data) {
            Ok(t) => t,
            Err(_) if is_main => return Err(perr("the main document is not UTF-8 XML")),
            Err(_) => continue,
        };
        if has_marker_chars(text) {
            if is_main {
                return Err(Error::Unsupported(
                    "the document uses private-use characters U+E000 to U+E003".into(),
                ));
            }
            continue;
        }
        let mut nodes = match xml::parse(text) {
            Ok(n) => n,
            Err(m) if is_main => return Err(perr(format!("{}: {m}", e.name))),
            Err(_) => continue,
        };
        let ns = xml::Ns::detect(&nodes);
        if is_main {
            wp = ns.w.clone();
        }
        let mut counter = 0;
        xml::number(&mut nodes, &format!("{}p", ns.w), &mut counter);
        let prefix = if is_main {
            String::new()
        } else {
            let stem = e
                .name
                .rsplit('/')
                .next()
                .unwrap_or("part")
                .trim_end_matches(".xml")
                .to_string();
            format!("{stem}-")
        };
        ex.begin_part(kind, &e.name, prefix, ns);
        templates.insert(i, ex.run_part(&nodes));
    }
    let units = ex.finish();
    let idx: HashMap<&str, usize> = units
        .iter()
        .enumerate()
        .map(|(i, u)| (u.id.as_str(), i))
        .collect();

    let mut zentries = Vec::with_capacity(entries.len() + 1);
    let mut packed = std::collections::BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        let data = match templates.get(&i) {
            Some(t) if t.contains(M_UNIT) => ZipData::Parts(split_template(t, &idx)?),
            _ => ZipData::Raw(e.keep(&mut packed)),
        };
        zentries.push(ZipEntry {
            name: e.name.clone(),
            data,
        });
    }
    let meta = Meta {
        wp,
        main: main.clone(),
        entries: entries.iter().map(|e| e.meta.clone()).collect(),
        packed,
    };
    let meta_json = serde_json::to_vec(&meta).map_err(|e| perr(e.to_string()))?;
    zentries.push(ZipEntry {
        name: META_NAME.into(),
        data: ZipData::Raw(meta_json),
    });

    Ok(Document {
        format: "docx".into(),
        name: None,
        source_lang: opts.source_lang.clone().or(lang_default),
        target_lang: opts.target_lang.clone(),
        granularity: Granularity::Paragraph,
        units,
        skeleton: Skeleton::zip(zentries, bilingua_model::parser_info!()),
    })
}

type Split<'a> = (Option<Meta>, Vec<&'a ZipEntry>);

fn split_meta(doc: &Document) -> Result<Split<'_>, Error> {
    let Skeleton::Zip { entries, .. } = &doc.skeleton else {
        return Err(Error::Write("docx needs a zip skeleton".into()));
    };
    let mut meta = None;
    let mut real = Vec::new();
    for e in entries {
        if e.name == META_NAME {
            if let ZipData::Raw(b) = &e.data {
                meta = serde_json::from_slice::<Meta>(b).ok();
            }
        } else {
            real.push(e);
        }
    }
    Ok((meta, real))
}

/// Writes the document and reports what the caller should know (tracked changes accepted, and so on).
pub fn merge_with_report(doc: &Document) -> Result<(Vec<u8>, MergeReport), Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    let (meta, real) = split_meta(doc)?;
    let wp = meta
        .as_ref()
        .map(|m| m.wp.clone())
        .unwrap_or_else(|| "w:".into());
    let mut r = render::Renderer::new(doc, wp);
    let mut out: Vec<(EntryMeta, zipio::Body)> = Vec::with_capacity(real.len());
    for e in real {
        let data = match &e.data {
            ZipData::Raw(b) => match meta.as_ref().and_then(|m| m.packed.get(&e.name)) {
                Some(p) => zipio::Body::Packed(p, b),
                None => zipio::Body::Plain(b),
            },
            ZipData::Parts(parts) => {
                let mut s = String::new();
                for p in parts {
                    match p {
                        SkelPart::Raw(t) => s.push_str(&r.resolve(t, 0)?),
                        SkelPart::Unit(i) => s.push_str(&r.unit(*i)?),
                    }
                }
                zipio::Body::Owned(s.into_bytes())
            }
        };
        let em = meta
            .as_ref()
            .and_then(|m| m.entries.iter().find(|x| x.name == e.name).cloned())
            .unwrap_or(EntryMeta {
                name: e.name.clone(),
                stored: false,
                dir: e.name.ends_with('/'),
                modified: None,
            });
        out.push((em, data));
    }
    let bytes = zipio::write_zip_parts(&out)?;
    Ok((
        bytes,
        MergeReport {
            warnings: r.warnings,
        },
    ))
}

impl Filter for Docx {
    fn id(&self) -> &'static str {
        "docx"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["docx", "docm", "dotx", "dotm"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
        bytes.starts_with(b"PK\x03\x04") && has(b"[Content_Types].xml") && has(b"word/")
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        extract_doc(bytes, opts)
    }
    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        merge_with_report(doc).map(|(b, _)| b)
    }
    fn preview(&self, doc: &Document) -> Option<String> {
        let (meta, _) = split_meta(doc).ok()?;
        let meta = meta?;
        preview::preview(doc, &meta.main, &meta.wp)
    }
    fn render_segment(&self, doc: &Document, seg_id: &str) -> Option<String> {
        preview::render_segment(doc, seg_id)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_features;
