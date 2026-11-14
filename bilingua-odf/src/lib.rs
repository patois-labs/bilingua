//! Bilingua OpenDocument filters: Writer (`odt`, `ott`), Calc (`ods`, `ots`), Impress (`odp`, `otp`) and Draw
//! (`odg`, `otg`). One walker serves all four, because every OpenDocument family keeps its text in `text:p` and
//! `text:h`. See the README for the rules; they follow the Localization OS filter (`odf_filter.py`).
//!
//! ```ignore
//! let f = bilingua_odf::Odt::default();
//! let mut doc = f.extract(&bytes, &ExtractOptions::new().with_target_lang("de-DE"))?;
//! doc.set_target("p0", vec![Inline::text("Erste Schritte")])?;
//! let out = f.merge(&doc)?;
//! ```

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod edit;
mod lang;
mod stamp;
mod walk;
mod xml;
mod zipdir;

use bilingua_model::*;
use bilingua_zip::{Body, EntryMeta, RawEntry, ZipLimits};
use edit::Edits;
use std::collections::BTreeMap;
use xml::{Kind, Tree, NS_TABLE};

/// The skeleton's only entry: the whole original archive. It holds a NUL, which no real zip entry name can.
const ARCHIVE: &str = "\0bilingua-odf-archive";
/// Comment units are numbered from here, as in Localization OS.
const COMMENT_BASE: usize = 1_000_000;
const PARTS: [&str; 2] = ["content.xml", "styles.xml"];

/// The `format_options` keys of the OpenDocument filters: one key for odt, ods, odp and odg.
pub const FORMAT_OPTIONS: &[FormatOptionKey] =
    &[FormatOptionKey::bool("odf.translate_comments", "Make the paragraphs of comments (office:annotation) units.")];

/// Options that `ExtractOptions` has no field for. `format_options["odf.translate_comments"]`, when set, wins
/// over the filter's own `translate_comments`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct OdfOptions {
    /// Make the paragraphs of comments (`office:annotation`) units (`c0`, `c1`, ...). Off by default: the comment
    /// rides in its host paragraph as an untranslated inline code.
    pub translate_comments: bool,
}

fn perr(id: &str, msg: impl Into<String>) -> Error {
    Error::Parse { format: id.into(), msg: msg.into() }
}

// ------------------------------------------------------------------------------------------ package

fn open_package(bytes: &[u8], id: &str, l: &Limits) -> Result<Vec<RawEntry>, Error> {
    guard::check_input(bytes, l)?;
    if let Some(name) = zipdir::duplicate_name(bytes) {
        return Err(perr(id, format!("the zip lists the member {name:?} twice. It was not processed.")));
    }
    let entries = bilingua_zip::read_zip(id, bytes, &ZipLimits::from(l))?;
    bilingua_zip::check_xml_parts(&entries, l)?;
    if !entries.iter().any(|e| e.name == "content.xml") {
        return Err(perr(id, "this OpenDocument file has no content.xml. Save it again in LibreOffice."));
    }
    if let Some(m) = entries.iter().find(|e| e.name == "mimetype") {
        let head = &m.data[..m.data.len().min(200)];
        if !String::from_utf8_lossy(head).trim().starts_with("application/vnd.oasis.opendocument.") {
            return Err(perr(id, "this file is not an OpenDocument document (its mimetype is not an OpenDocument type)."));
        }
    }
    if let Some(m) = entries.iter().find(|e| e.name == "META-INF/manifest.xml") {
        if m.data.windows(15).any(|w| w == b"encryption-data") {
            return Err(Error::Unsupported("this OpenDocument file is password-protected. Save a copy without a password and upload that.".into()));
        }
    }
    Ok(entries)
}

struct Loaded {
    name: &'static str,
    tree: Tree,
    /// Encoding envelope (no newline part) for writing the part back.
    enc: String,
    paras: Vec<u32>,
    warnings: Vec<Warning>,
}

fn decode_part(bytes: &[u8], l: &Limits) -> Result<(String, String, Vec<Warning>), Error> {
    // no line budget: an OpenDocument part is one long line or pretty printed, either way the tree limits apply
    let lim = l.with_max_lines(usize::MAX);
    let d = enc::decode_text(bytes, &enc::DecodeOptions::with_limits(lim).declared())?;
    let tag: Vec<&str> = d.encoding.split(';').filter(|p| !p.starts_with("nl=")).collect();
    let tag = tag.join(";");
    if enc::is_legacy(&tag) && !tag.contains(";label=") {
        return Err(Error::Encoding(
            "An XML part of this OpenDocument file is not UTF-8 and declares no encoding. Save the file again in LibreOffice.".into(),
        ));
    }
    Ok((d.text, tag, d.warnings))
}

fn load_part(entries: &[RawEntry], name: &'static str, l: &Limits, budget: bool) -> Result<Option<Loaded>, Error> {
    let Some(e) = entries.iter().find(|e| e.name == name) else { return Ok(None) };
    if budget && e.data.iter().filter(|b| **b == b'<').count() > 40 * l.max_units {
        return Err(Error::Limit(LimitError::new(
            LimitKind::XmlElements,
            format!("This OpenDocument file has far more markup than the {} unit limit allows. Split it into smaller files.", l.max_units),
        )));
    }
    let (text, enc, warnings) = decode_part(&e.data, l)?;
    let tree = Tree::parse(text, l)?;
    let paras = walk::paragraphs(&tree, name == "styles.xml");
    Ok(Some(Loaded { name, tree, enc, paras, warnings }))
}

// ------------------------------------------------------------------------------------------ extract

fn label_of(local: &str) -> &str {
    match local {
        "a" => "link",
        "tab" => "tab",
        "line-break" => "line break",
        "soft-page-break" => "page break",
        "annotation" => "comment",
        "bookmark" | "bookmark-start" | "bookmark-end" => "bookmark",
        l => l,
    }
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut o: String = s.chars().take(max).collect();
        o.push('\u{2026}');
        o
    }
}

fn code_for(t: &Tree, node: u32, part: &walk::Part) -> (String, CodeData) {
    let n = t.node(node);
    match part {
        walk::Part::Open(_) | walk::Part::Close(_) => {
            let open = clip(t.start_tag(node), 200);
            let close = if t.self_closing(node) { format!("</{}>", t.qname(node)) } else { t.end_tag(node).to_string() };
            let label = label_of(t.local(node)).to_string();
            let disp = if matches!(part, walk::Part::Open(_)) { open.clone() } else { close.clone() };
            (disp, CodeData::new().with_open(open).with_close(close).with_label(label))
        }
        _ => {
            let (disp, label) = if n.kind == Kind::Elem {
                let st = clip(t.start_tag(node), 200);
                let d = if t.self_closing(node) { st } else { format!("{st}\u{2026}{}", t.end_tag(node)) };
                (d, label_of(t.local(node)).to_string())
            } else {
                (clip(t.raw(node), 200), if n.kind == Kind::Comment { "xml comment".to_string() } else { "instruction".to_string() })
            };
            (disp.clone(), CodeData::new().with_standalone(disp).with_label(label))
        }
    }
}

fn make_unit(t: &Tree, p: u32, tok: &walk::Tok, id: String, kind: &str) -> Unit {
    let mut inl: Vec<Inline> = Vec::with_capacity(tok.parts.len());
    let mut codes = BTreeMap::new();
    for part in &tok.parts {
        match part {
            walk::Part::Text(s) => inl.push(Inline::Text(s.clone())),
            walk::Part::Open(n) | walk::Part::Close(n) | walk::Part::Point(n) => {
                let node = tok.nodes[*n - 1];
                let (disp, data) = code_for(t, node, part);
                let k = match part {
                    walk::Part::Open(_) => TagKind::Open,
                    walk::Part::Close(_) => TagKind::Close,
                    _ => TagKind::Standalone,
                };
                let key = n.to_string();
                codes.entry(key.clone()).or_insert(data);
                inl.push(Inline::tag(key, k, disp));
            }
        }
    }
    let mut u = Unit::new(id, inl);
    u.codes = codes;
    u.context.kind = Some(kind.to_string());
    u.context.style = t.attr(p, xml::NS_TEXT, "style-name");
    u
}

fn extract_doc(bytes: &[u8], opts: &ExtractOptions, id: &'static str, o: &OdfOptions) -> Result<Document, Error> {
    let l = opts.limits();
    let entries = open_package(bytes, id, &l)?;
    let mut units: Vec<Unit> = Vec::new();
    let mut warnings = Vec::new();
    let mut parts: Vec<Loaded> = Vec::new();
    for name in PARTS {
        if let Some(p) = load_part(&entries, name, &l, true)? {
            parts.push(p);
        }
    }
    let mut idx = 0usize;
    for part in &parts {
        warnings.extend(part.warnings.iter().cloned());
        let t = &part.tree;
        let mut heading: Option<String> = None;
        for &p in &part.paras {
            let tok = walk::tokenize(t, p, l.max_unit_chars)?;
            if tok.has_letter() {
                let is_h = t.local(p) == "h";
                let in_cell = t.ancestors(p).any(|a| t.is(a, NS_TABLE, "table-cell"));
                let kind = if is_h {
                    "heading"
                } else if in_cell {
                    "cell"
                } else {
                    "paragraph"
                };
                let mut u = make_unit(t, p, &tok, format!("p{idx}"), kind);
                u.context.location = Some(format!("{} paragraph {idx}", part.name));
                u.context.heading = heading.clone();
                u.context.extra.insert("p_index".into(), idx.to_string());
                u.context.extra.insert("part".into(), part.name.to_string());
                if is_h {
                    heading = Some(tok.parts.iter().filter_map(|x| if let walk::Part::Text(s) = x { Some(s.as_str()) } else { None }).collect());
                }
                units.push(u);
                guard::check_unit_count(units.len(), &l)?;
            }
            idx += 1;
        }
    }
    if o.translate_comments {
        let mut j = 0usize;
        for part in &parts {
            for p in walk::annotation_paragraphs(&part.tree) {
                let tok = walk::tokenize(&part.tree, p, l.max_unit_chars)?;
                if tok.has_letter() {
                    let mut u = make_unit(&part.tree, p, &tok, format!("c{j}"), "comment");
                    u.context.location = Some(format!("{} comment paragraph {j}", part.name));
                    u.context.extra.insert("p_index".into(), (COMMENT_BASE + j).to_string());
                    u.context.extra.insert("part".into(), part.name.to_string());
                    units.push(u);
                    guard::check_unit_count(units.len(), &l)?;
                }
                j += 1;
            }
        }
    }
    let skeleton = Skeleton::zip(vec![ZipEntry::raw(ARCHIVE, bytes.to_vec())], parser_info!());
    let doc = Document::new(id, units, skeleton).with_options(opts);
    guard::finish(doc, warnings, &l)
}

// ------------------------------------------------------------------------------------------ merge

/// A target is a translation when any segment has one.
fn translated(u: &Unit) -> bool {
    u.segments.iter().any(|s| s.has_target())
}

fn same_content(a: &[Inline], b: &[Inline]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| match (x, y) {
            (Inline::Text(p), Inline::Text(q)) => p == q,
            (Inline::Tag { id: i, kind: k, .. }, Inline::Tag { id: j, kind: m, .. }) => i == j && k == m,
            _ => false,
        })
}

/// A part Localization OS rewrites, as its `_serialize` writes it (`etree.tostring(root, xml_declaration=True,
/// encoding="UTF-8")`): UTF-8 with a single-quoted declaration and a new line, then the tree as lxml serialises it
/// (namespace declarations first, double-quoted attributes with `&apos;` as a plain `'` and `"` as `&quot;`, `<x/>`
/// for an empty element, no white space after the root). A part with a CDATA section, or a comment or instruction outside the root,
/// gets the declaration only, so nothing is lost (LocOS keeps a CDATA section too, `strip_cdata=False`, and drops what
/// lies outside the root; there the bytes differ, the content does not).
fn lxml_part(text: &str) -> Vec<u8> {
    const DECL: &str = "<?xml version='1.0' encoding='UTF-8'?>
";
    let body = match text.trim_start().strip_prefix("<?xml") {
        Some(rest) => rest.find("?>").map_or(text, |i| &rest[i + 2..]),
        None => text,
    };
    if !body.contains("<![CDATA[") {
        if let Ok(dom) = bilingua_xmltree::parse(body) {
            let outside_blank = dom.nodes[dom.root].span.is_some_and(|s| body[..s.start].trim().is_empty() && body[s.end..].trim().is_empty());
            if outside_blank {
                return format!("{DECL}{}", dom.serialize()).into_bytes();
            }
        }
    }
    format!("{DECL}{}", body.trim_start()).into_bytes()
}

fn merge_doc(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    check_parser(doc, &parser_info!())?;
    let Skeleton::Zip { entries: skel, .. } = &doc.skeleton else {
        return Err(Error::Write("an OpenDocument skeleton must be a zip skeleton".into()));
    };
    let Some(ZipData::Raw(archive)) = skel.iter().find(|e| e.name == ARCHIVE).map(|e| &e.data) else {
        return Err(Error::Write("the OpenDocument skeleton has lost its archive".into()));
    };
    let work: Vec<&Unit> = doc.units.iter().filter(|u| u.translate && translated(u)).collect();
    let locos = opts.is_profile(bilingua_model::profile::LOCOS);
    let stamp = opts.stamp_lang(doc, !work.is_empty()).and_then(lang::html_lang);
    if work.is_empty() && stamp.is_none() {
        return Ok(archive.clone());
    }
    let l = Limits::default();
    let entries = bilingua_zip::read_zip("odf", archive, &ZipLimits::from(&l)).map_err(|e| Error::Write(format!("the stored archive is unreadable: {e}")))?;
    let mut parts: Vec<Loaded> = Vec::new();
    for name in PARTS {
        if let Some(p) = load_part(&entries, name, &l.with_max_xml_elements(usize::MAX).with_max_xml_depth(4096), false)? {
            parts.push(p);
        }
    }
    let body: Vec<(usize, u32)> = parts.iter().enumerate().flat_map(|(pi, p)| p.paras.iter().map(move |n| (pi, *n))).collect();
    let notes: Vec<(usize, u32)> = parts.iter().enumerate().flat_map(|(pi, p)| walk::annotation_paragraphs(&p.tree).into_iter().map(move |n| (pi, n))).collect();
    let mut edits: Vec<Edits> = parts.iter().map(|p| Edits::new(&p.tree)).collect();
    for e in &mut edits {
        e.lxml_prefixes = locos;
    }
    // Localization OS rebuilds every paragraph it is given text for, also when that text is the paragraph's own
    // (spaces become `text:s` the way its writer makes them, white space between inline elements goes, and the part
    // is written again). Its build skips all of that only when no target locale is set and every text is unchanged
    // (the identity build returns the upload).
    let rebuild_same = locos && (stamp.is_some() || work.iter().any(|u| !same_content(&u.output(), &u.source())));
    for u in work {
        let (list, num) = if let Some(n) = u.id.strip_prefix('p').and_then(|n| n.parse::<usize>().ok()) {
            (&body, n)
        } else if let Some(n) = u.id.strip_prefix('c').and_then(|n| n.parse::<usize>().ok()) {
            (&notes, n)
        } else {
            return Err(Error::Write(format!("unit {} does not belong to an OpenDocument file", u.id)));
        };
        let Some(&(pi, node)) = list.get(num) else {
            return Err(Error::Write(format!("unit {} is not in this file", u.id)));
        };
        let out = u.output();
        if !rebuild_same && same_content(&out, &u.source()) {
            continue;
        }
        let t = &parts[pi].tree;
        let tok = walk::tokenize(t, node, usize::MAX)?;
        walk::rebuild(t, &mut edits[pi], node, &tok, &out);
    }
    if let Some(tag) = &stamp {
        let lg = stamp::Lang::new(tag);
        for (i, p) in parts.iter().enumerate() {
            stamp::stamp_part(&p.tree, &mut edits[i], p.name == "styles.xml", p.name == "content.xml", &lg);
        }
    }
    let mut replaced: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (i, p) in parts.iter().enumerate() {
        if edits[i].is_edited() {
            let text = edits[i].emit(&p.tree);
            let bytes = if locos { lxml_part(&text) } else { enc::encode_text_with(&text, &p.enc, opts)? };
            replaced.insert(p.name.to_string(), bytes);
        }
    }
    if let Some(tag) = &stamp {
        if let Some(e) = entries.iter().find(|e| e.name == "meta.xml") {
            let (text, enc_tag, _) = decode_part(&e.data, &l)?;
            let tree = Tree::parse(text, &l)?;
            let mut ed = Edits::new(&tree);
            ed.lxml_prefixes = locos;
            if stamp::stamp_meta(&tree, &mut ed, tag) {
                let text = ed.emit(&tree);
                replaced.insert("meta.xml".into(), if locos { lxml_part(&text) } else { enc::encode_text_with(&text, &enc_tag, opts)? });
            }
        }
    }
    if replaced.is_empty() {
        return Ok(archive.clone());
    }
    // mimetype first and stored (ODF 1.3 Part 2, 3.3); every other member keeps its order, bytes and compression
    let mut order: Vec<&RawEntry> = entries.iter().filter(|e| e.name == "mimetype").collect();
    order.extend(entries.iter().filter(|e| e.name != "mimetype"));
    let mut out: Vec<(EntryMeta, Body)> = Vec::with_capacity(order.len());
    for e in order {
        let mut meta = e.meta.clone();
        if e.name == "mimetype" {
            // a deflated mimetype is written stored from its plain bytes, never as a packed copy
            meta.stored = true;
        }
        let body = if let Some(b) = replaced.remove(&e.name) {
            Body::Owned(b)
        } else {
            match &e.packed {
                Some((p, b)) if !meta.stored && !meta.dir => Body::Packed(p, b),
                _ => Body::Plain(&e.data),
            }
        };
        out.push((meta, body));
    }
    bilingua_zip::write_zip_parts(&out)
}

// ------------------------------------------------------------------------------------------ filters

fn sniff_mimetype(bytes: &[u8], wanted: &[&str]) -> bool {
    const KEY: &[u8] = b"mimetypeapplication/vnd.oasis.opendocument.";
    if !bytes.starts_with(b"PK") {
        return false;
    }
    let head = &bytes[..bytes.len().min(4096)];
    let Some(p) = head.windows(KEY.len()).position(|w| w == KEY) else { return false };
    let rest = &head[p + KEY.len()..];
    let end = rest.iter().position(|b| !(b.is_ascii_lowercase() || *b == b'-')).unwrap_or(rest.len());
    wanted.iter().any(|w| w.as_bytes() == &rest[..end])
}

macro_rules! odf_filter {
    ($name:ident, $id:literal, $exts:expr, $mimes:expr) => {
        #[derive(Debug, Clone, Copy, Default)]
        pub struct $name {
            pub options: OdfOptions,
        }

        impl $name {
            pub fn with_options(options: OdfOptions) -> $name {
                $name { options }
            }
        }

        impl Filter for $name {
            fn id(&self) -> &'static str {
                $id
            }
            fn extensions(&self) -> &'static [&'static str] {
                &$exts
            }
            fn sniff(&self, bytes: &[u8]) -> bool {
                sniff_mimetype(bytes, &$mimes)
            }
            fn format_option_keys(&self) -> &'static [FormatOptionKey] {
                FORMAT_OPTIONS
            }
            fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
                bilingua_model::check_own_options(self, &opts.format_options)?;
                let mut o = self.options;
                if let Some(on) = opts.format_flag("odf.translate_comments") {
                    o.translate_comments = on;
                }
                extract_doc(bytes, opts, $id, &o)
            }
            fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
                bilingua_model::check_own_options(self, &opts.format_options)?;
                merge_doc(doc, opts)
            }
        }
    };
}

odf_filter!(Odt, "odt", ["odt", "ott"], ["text", "text-template", "text-master", "text-master-template", "text-web"]);
odf_filter!(Ods, "ods", ["ods", "ots"], ["spreadsheet", "spreadsheet-template"]);
odf_filter!(Odp, "odp", ["odp", "otp"], ["presentation", "presentation-template"]);
odf_filter!(Odg, "odg", ["odg", "otg"], ["graphics", "graphics-template"]);

/// The four filters, for the facade.
pub fn filters() -> Vec<Box<dyn Filter>> {
    vec![Box::new(Odt::default()), Box::new(Ods::default()), Box::new(Odp::default()), Box::new(Odg::default())]
}

