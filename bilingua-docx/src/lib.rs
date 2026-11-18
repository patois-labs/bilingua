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
#[doc(hidden)]
pub mod locos;
mod preview;
mod render;
mod storyline;
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

/// A member's XML refused by `xml::parse`: the nesting cap is a limit (`LimitKind::Depth`), like every other
/// crate's; anything else is a parse error.
pub(crate) fn xml_member_err(member: &str, m: &str) -> Error {
    if m.starts_with("elements nested deeper than") {
        return bilingua_model::LimitError::new(bilingua_model::LimitKind::Depth, format!("{member}: {m}")).into();
    }
    perr(format!("{member}: {m}"))
}

/// Parts never parsed into units: `customXml/` (data a Word add-in keeps). The XML guard skips them, so a deep or
/// broken one never refuses the document (W4 docx-2, as LocOS `zip_guard.refuse_doctype` skips them).
pub(crate) const UNPARSED_PARTS: &[&str] = &["customXml/"];

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

fn too_many_paragraphs(l: &Limits) -> Error {
    too_many_paragraphs_n(l.max_paragraphs)
}

fn too_many_paragraphs_n(max: usize) -> Error {
    bilingua_model::LimitError::new(
        bilingua_model::LimitKind::Paragraphs,
        format!("This file has more than {max} paragraphs. Split it into smaller files."),
    )
    .into()
}

/// True for an entry the XML guard covers by its name alone (`bilingua_zip::check_xml_parts_except`).
pub(crate) fn xml_guarded_by_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    !UNPARSED_PARTS.iter().any(|p| name.starts_with(p)) && [".xml", ".rels", ".vml", ".xlf", ".xliff", ".sdlxliff"].iter().any(|x| n.ends_with(x))
}

/// True for an entry whose paragraphs `guard_named_parts` counts by its name alone.
fn paragraphs_counted_by_name(name: &str) -> bool {
    name.starts_with("word/") && name.ends_with(".xml")
}

/// Counts the paragraphs of `word/*.xml` while the package inflates (BILI-115), so a file over `max_paragraphs` is
/// refused before the rest of it is unpacked, instead of after the whole package sits in memory.
///
/// It cannot refuse a file the full read accepts: it counts exactly what [`guard_named_parts`] counts (the same
/// entries, with `guard::ElementCounter`, which gives `count_elements`'s number for a whole part and never more
/// than that for part of one) and refuses with the same error once the running total passes the same limit, so
/// whenever it refuses, the count after the read would have refused too. A file that breaks another limit as well
/// may be refused for its paragraphs where the full read would have named the other limit first.
struct ParagraphWatch {
    max: usize,
    /// Paragraphs of the entries already read.
    done: usize,
    /// The entry being read, when it is one whose paragraphs are counted.
    part: Option<bilingua_model::guard::ElementCounter>,
    /// False when an entry was not counted (a UTF-16 part): `guard_named_parts` then counts them all again.
    exact: bool,
}

impl ParagraphWatch {
    fn new(l: &Limits) -> ParagraphWatch {
        ParagraphWatch { max: l.max_paragraphs, done: 0, part: None, exact: true }
    }
    /// The exact paragraph count of every `word/*.xml` entry, when every one was counted.
    fn total(&self) -> Option<usize> {
        self.exact.then_some(self.done)
    }
}

impl bilingua_zip::Watch for ParagraphWatch {
    fn bytes(&mut self, name: &str, chunk: &[u8]) -> Result<(), Error> {
        if !paragraphs_counted_by_name(name) {
            return Ok(());
        }
        let c = self.part.get_or_insert_with(|| bilingua_model::guard::ElementCounter::new("p"));
        c.feed(chunk);
        if self.done.saturating_add(c.count()) > self.max {
            return Err(too_many_paragraphs_n(self.max));
        }
        Ok(())
    }
    fn done(&mut self, name: &str) -> Result<(), Error> {
        if !paragraphs_counted_by_name(name) {
            return Ok(());
        }
        // an empty entry sent no bytes: it holds no paragraph
        if let Some(mut c) = self.part.take() {
            c.finish();
            self.exact &= c.exact();
            self.done = self.done.saturating_add(c.count());
        }
        if self.done > self.max {
            return Err(too_many_paragraphs_n(self.max));
        }
        Ok(())
    }
}

/// Reads the package, counting its paragraphs as it inflates ([`ParagraphWatch`]). Returns the entries and, when
/// every counted part could be counted while streaming, the paragraph count for [`guard_named_parts`].
pub(crate) fn read_package(bytes: &[u8], l: &Limits) -> Result<(Vec<zipio::RawEntry>, Option<usize>), Error> {
    let mut watch = ParagraphWatch::new(l);
    let entries = zipio::read_zip_watched(bytes, &ZipLimits::from(l), &mut watch)?;
    Ok((entries, watch.total()))
}

/// The guards that go by entry name, run before anything is parsed: the XML guard over every XML-named entry
/// outside `customXml/`, and the paragraph count over `word/*.xml` (`counted`, when [`read_package`] already
/// counted them as they inflated). Returns the paragraphs counted so far.
pub(crate) fn guard_named_parts(entries: &[zipio::RawEntry], l: &Limits, counted: Option<usize>) -> Result<usize, Error> {
    bilingua_zip::check_xml_parts_except(entries, l, UNPARSED_PARTS)?;
    // empty paragraphs count too (LocOS DOCX_MAX_PARAGRAPHS), before any tree is built
    if let Some(n) = counted {
        if n > l.max_paragraphs {
            return Err(too_many_paragraphs(l));
        }
        return Ok(n);
    }
    let mut paragraphs = 0usize;
    for e in entries.iter().filter(|e| paragraphs_counted_by_name(&e.name)) {
        paragraphs += bilingua_model::guard::count_elements(&e.data, "p");
        if paragraphs > l.max_paragraphs {
            return Err(too_many_paragraphs(l));
        }
    }
    Ok(paragraphs)
}

/// The same guards for one part that is about to be parsed, whatever its name. A package chooses its parts by
/// content type and relationship, so the main document may be called `word/document.bin` or sit under
/// `customXml/`, where the name-keyed guards do not look; a part they did cover is not scanned twice.
pub(crate) fn guard_parsed_part(name: &str, data: &[u8], l: &Limits, paragraphs: &mut usize) -> Result<(), Error> {
    if !xml_guarded_by_name(name) {
        bilingua_model::guard::check_xml(data, l)?;
    }
    if !paragraphs_counted_by_name(name) {
        *paragraphs += bilingua_model::guard::count_elements(data, "p");
        if *paragraphs > l.max_paragraphs {
            return Err(too_many_paragraphs(l));
        }
    }
    Ok(())
}

fn extract_doc(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let l = opts.limits();
    bilingua_model::guard::check_input(bytes, &l)?;
    bilingua_model::guard::finish(extract_doc_inner(bytes, opts)?, vec![], &l)
}

fn extract_doc_inner(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let l = opts.limits();
    let (mut entries, counted) = read_package(bytes, &l)?;
    let mut paragraphs = guard_named_parts(&entries, &l, counted)?;
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

    // The guards above go by entry name; the parts read below are chosen by content type and by the main part's
    // folder. Whatever is about to be parsed and was not covered by name is guarded here, before any tree is built.
    let styles_name = format!("{dir}styles.xml");
    let comments_name = format!("{dir}comments.xml");
    for e in entries.iter() {
        if e.name == styles_name || e.name == comments_name || classify(&e.name, &main, &dir).is_some() {
            guard_parsed_part(&e.name, &e.data, &l, &mut paragraphs)?;
        }
    }

    // Styles and comments first: paragraphs need them for their context.
    let mut styles = Styles::default();
    if let Some(e) = entries
        .iter()
        .find(|e| e.name == styles_name)
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
        .find(|e| e.name == comments_name)
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

    let mut ex = Extractor::new(styles, comments).with_limits(l);
    let mut templates: HashMap<usize, String> = HashMap::new();
    let mut wp = "w:".to_string();
    let mut bilingual: Option<storyline::Found> = None;
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
            Err(m) if is_main => return Err(xml_member_err(&e.name, &m)),
            Err(_) => continue,
        };
        let ns = xml::Ns::detect(&nodes);
        if is_main {
            wp = ns.w.clone();
        }
        let mut counter = 0;
        xml::number(&mut nodes, &format!("{}p", ns.w), &mut counter);
        if is_main {
            bilingual = storyline::translation_paragraphs(&nodes, &ns);
        }
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
        // the walk frees the tree as it goes
        let template = ex.run_part(nodes);
        // a comment copied into more units than the context budget allows: refused as soon as the part is walked
        ex.check_refused()?;
        // the units only grow: once over the limit, `guard::finish` would refuse with this error (BILI-115)
        bilingua_model::guard::check_unit_count(ex.unit_count(), &l)?;
        if template.contains(M_UNIT) {
            // the skeleton keeps this part as its template: its unpacked and deflated bytes are not read again
            let e = &mut entries[i];
            e.data = Vec::new();
            e.packed = None;
        }
        templates.insert(i, template);
    }
    let mut units = ex.finish();
    // A Storyline "Export to Word" table: only the Translation cells are translated (LocOS docx_bilingual)
    let mut warnings = Vec::new();
    if let Some(f) = &bilingual {
        let keep: std::collections::HashSet<String> = f.keep.iter().map(|o| format!("p{o}")).collect();
        for u in units.iter_mut().filter(|u| !keep.contains(&u.id)) {
            u.translate = false;
            u.lock_reason = Some("bilingual table: only the Translation column is translated".into());
        }
        warnings.push(bilingua_model::Warning::new("docx_bilingual_table", main.clone(), storyline::notice(f)));
    }
    let idx: HashMap<&str, usize> = units
        .iter()
        .enumerate()
        .map(|(i, u)| (u.id.as_str(), i))
        .collect();

    let mut zentries = Vec::with_capacity(entries.len() + 1);
    let mut packed = std::collections::BTreeMap::new();
    let metas: Vec<EntryMeta> = entries.iter().map(|e| e.meta.clone()).collect();
    // each template and each entry's bytes are freed as soon as the skeleton has its own copy
    for (i, e) in entries.iter_mut().enumerate() {
        let data = match templates.remove(&i) {
            Some(t) if t.contains(M_UNIT) => ZipData::Parts(split_template(&t, &idx)?),
            _ => ZipData::Raw(e.take_kept(&mut packed)),
        };
        zentries.push(ZipEntry::new(e.name.clone(), data));
    }
    drop(entries);
    let meta = Meta {
        wp,
        main: main.clone(),
        entries: metas,
        packed,
    };
    let meta_json = serde_json::to_vec(&meta).map_err(|e| perr(e.to_string()))?;
    zentries.push(ZipEntry::raw(META_NAME, meta_json));

    Ok(Document::new("docx", units, Skeleton::zip(zentries, bilingua_model::parser_info!())).with_source_lang(opts.source_lang.clone().or(lang_default)).with_target_lang(opts.target_lang.clone()).with_granularity(Granularity::Paragraph).with_warnings(warnings))
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
                // An unreadable record must not be skipped: without its list of packed entries, their deflated
                // bytes would be written as if they were plain, and the file would be broken.
                meta = Some(serde_json::from_slice::<Meta>(b).map_err(|e| {
                    Error::Write(format!("the docx skeleton's zip record is unreadable: {e}"))
                })?);
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
            ZipData::Raw(b) => zipio::raw_body(meta.as_ref().map(|m| &m.packed), &e.name, b),
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

/// The `format_options` keys of the DOCX filter.
pub const FORMAT_OPTIONS: &[bilingua_model::FormatOptionKey] = &[
    bilingua_model::FormatOptionKey::bool("docx.translate_comments", "Translate Word comments (the default); false keeps them as locked context."),
    bilingua_model::FormatOptionKey::text("docx.walk", "Accepted and ignored: LocOS's numbering walk stamp; Bilingua has one walk, the current one."),
];

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
    fn format_option_keys(&self) -> &'static [bilingua_model::FormatOptionKey] {
        FORMAT_OPTIONS
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        if opts.is_profile(bilingua_model::profile::LOCOS) {
            // Track D: LocOS's units and numbering (comments left out with the option off, as LocOS does)
            let l = opts.limits();
            bilingua_model::guard::check_input(bytes, &l)?;
            return bilingua_model::guard::finish(locos::extract(bytes, opts)?, vec![], &l);
        }
        let mut doc = extract_doc(bytes, opts)?;
        // Word comments are part of the document walk (as in LocOS): with the option off they stay in the
        // document as locked context, so unit ids and the skeleton do not change
        if opts.format_flag("docx.translate_comments") == Some(false) {
            for u in doc.units.iter_mut().filter(|u| u.context.kind.as_deref() == Some("comment")) {
                u.translate = false;
                u.lock_reason = Some("comments are not translated (docx.translate_comments)".into());
            }
        }
        Ok(doc)
    }
    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        if let Skeleton::Binary { .. } = &doc.skeleton {
            // a document extracted under the Localization OS profile
            bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
            return locos::merge(doc, opts);
        }
        // The parts are UTF-8 XML, and the language is set on the runs this merge rewrites (translated text), so neither
        // option changes anything: an untranslated run is never rewritten.
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

/// B-SIDE 16: a member nested past the cap is a limit (`LimitKind::Depth`), not a parse error; other XML damage
/// stays a parse error.
#[cfg(test)]
mod depth_is_a_limit {
    #[test]
    fn nesting_cap_is_coded_limit() {
        let deep = "<a>".repeat(xml::MAX_DEPTH + 5) + &"</a>".repeat(xml::MAX_DEPTH + 5);
        let m = crate::xml::parse(&deep).unwrap_err();
        match crate::xml_member_err("word/document.xml", &m) {
            bilingua_model::Error::Limit(e) => {
                assert_eq!(e.kind, bilingua_model::LimitKind::Depth);
                assert!(e.msg.starts_with("word/document.xml: elements nested deeper than"), "{}", e.msg);
            }
            other => panic!("{other:?}"),
        }
        let bad = crate::xml::parse("<a><b></a>").unwrap_err();
        assert!(matches!(crate::xml_member_err("m.xml", &bad), bilingua_model::Error::Parse { .. }));
    }
    use crate::xml;
}

/// BILI-115: the paragraphs are counted while the package inflates, with the count and the error the whole-part
/// count gives.
#[cfg(test)]
mod fail_early {
    use crate::tests::{body, doc_of, make, p, part};
    use bilingua_model::{guard::count_elements, Error, ExtractOptions, Filter, LimitKind, Limits};

    fn paragraphs(n: usize) -> String {
        // empty paragraphs, properties and pictures that must not count, a comment that hides one
        let mut s = String::new();
        for i in 0..n {
            match i % 4 {
                0 => s.push_str(&p("Text")),
                1 => s.push_str("<w:p/>"),
                2 => s.push_str("<w:p><w:pPr><w:pStyle w:val=\"x\"/></w:pPr><w:r><w:pict/><w:t>Pic</w:t></w:r></w:p>"),
                _ => s.push_str("<!-- <w:p/> --><w:p w:rsidR=\"1\"><w:r><w:t>More</w:t></w:r></w:p>"),
            }
        }
        s
    }

    fn outcome(bytes: &[u8], l: Limits) -> Result<usize, String> {
        crate::Docx.extract(bytes, &ExtractOptions::default().with_limits(l)).map(|d| d.units.len()).map_err(|e: Error| e.to_string())
    }

    #[test]
    fn streamed_count_matches_the_whole_part_count() {
        let bytes = make(&[
            ("word/document.xml", &body(&paragraphs(40))),
            ("word/footer1.xml", &part("ftr", &paragraphs(3))),
            ("word/charts/chart1.xml", "<c:chartSpace xmlns:c=\"c\" xmlns:a=\"a\"><a:p/><a:p/></c:chartSpace>"),
        ]);
        let (entries, counted) = crate::read_package(&bytes, &Limits::new()).unwrap();
        let whole: usize = entries.iter().filter(|e| super::paragraphs_counted_by_name(&e.name)).map(|e| count_elements(&e.data, "p")).sum();
        assert_eq!(counted, Some(whole));
        assert_eq!(whole, 45);
        for k in 40..50 {
            let l = Limits::new().with_max_paragraphs(k);
            let r = crate::read_package(&bytes, &l);
            assert_eq!(r.is_err(), whole > k, "limit {k}");
            if let Err(Error::Limit(e)) = r {
                assert_eq!(e.kind, LimitKind::Paragraphs);
                assert_eq!(e.msg, format!("This file has more than {k} paragraphs. Split it into smaller files."));
            }
            // the extraction with the same limit: refused exactly when over it
            assert_eq!(outcome(&bytes, l).is_err(), whole > k, "limit {k}");
        }
    }

    /// A part larger than one inflated piece is refused before it has all been read.
    #[test]
    fn a_long_document_is_refused_while_it_inflates() {
        let bytes = doc_of(&paragraphs(60_000));
        let e = outcome(&bytes, Limits::new()).unwrap_err();
        assert_eq!(e, "This file has more than 50000 paragraphs. Split it into smaller files.");
        let mut watch = super::ParagraphWatch::new(&Limits::new().with_max_paragraphs(1_000));
        let mut seen = 0usize;
        struct Counting<'a>(&'a mut super::ParagraphWatch, &'a mut usize);
        impl bilingua_zip::Watch for Counting<'_> {
            fn bytes(&mut self, name: &str, chunk: &[u8]) -> Result<(), Error> {
                *self.1 += chunk.len();
                self.0.bytes(name, chunk)
            }
            fn done(&mut self, name: &str) -> Result<(), Error> {
                self.0.done(name)
            }
        }
        let r = crate::zipio::read_zip_watched(&bytes, &bilingua_zip::ZipLimits::from(&Limits::new()), &mut Counting(&mut watch, &mut seen));
        assert!(r.is_err());
        let full = body(&paragraphs(60_000)).len();
        assert!(seen < full / 2, "read {seen} of {full} bytes before the refusal");
    }

    /// A UTF-16 part is counted as before, after the read.
    #[test]
    fn a_utf16_part_falls_back_to_the_whole_part_count() {
        let mut v = vec![0xFF, 0xFE];
        v.extend(part("hdr", &paragraphs(8)).encode_utf16().flat_map(|u| u.to_le_bytes()));
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let o = zip::write::SimpleFileOptions::default();
        use std::io::Write;
        w.start_file("word/header1.xml", o).unwrap();
        w.write_all(&v).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        let (entries, counted) = crate::read_package(&bytes, &Limits::new()).unwrap();
        assert_eq!(counted, None);
        assert_eq!(crate::guard_named_parts(&entries, &Limits::new(), counted).unwrap(), 8);
        assert!(crate::guard_named_parts(&entries, &Limits::new().with_max_paragraphs(7), counted).is_err());
    }
}
