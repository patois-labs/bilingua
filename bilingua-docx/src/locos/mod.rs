//! DOCX under the Localization OS profile (Track D group B): a port of LocOS's `docx_filter.py` (walk stamp 6) on
//! python-docx's view of the package, so the adapter gets LocOS's units and files.
//!
//! * Units: every paragraph with text, numbered by LocOS's walk (the main body in reading order with tables,
//!   content controls and text boxes, then headers, footers, footnotes, endnotes and comments); the alt text and
//!   title of every drawing after the last paragraph (`a11y:img-alt`, `a11y:img-title`). `locos.p_index` is LocOS's
//!   `p_index`; the style is the paragraph style's name as python-docx gives it ("Normal", "Heading 1").
//! * Inline codes: a paragraph with more than one content run gets one `r` pair per run (tag id = LocOS's run
//!   number, `Open`/`Close`); a paragraph holding objects (pictures, fields, note and comment references, breaks,
//!   tracked changes, content controls) or wrapper text (hyperlinks, smart tags, insertions) is in LocOS's object
//!   mode: each group of objects is one standalone code (tag id = LocOS's `x` number). A Storyline "Export to
//!   Word" file gives only its Translation column, as LocOS does.
//! * Writing is LocOS's rebuild of each translated paragraph (run per pair with the run's `w:rPr`, objects moved
//!   back, wrappers re-created), then for a target language its declaration (`w:lang`, `w:bidi`, `dc:language`,
//!   the complex-script twins), and the package saved the way python-docx saves it. Strict Open XML stays Strict.
//!
//! Documents made under the profile carry the original as a `Skeleton::Binary`; `merge` picks this writer by that.

mod lang;
mod para;
mod pkg;
mod tree;
mod walk;

use bilingua_model::pytext::{py_has_alnum, py_isspace_char, py_strip};
use bilingua_model::{profile, CodeData, Document, Error, ExtractOptions, Granularity, Inline, MergeOptions, Skeleton, TagKind, Unit, UnitContext};
use bilingua_xmltree::Dom;
use para::Piece;
use pkg::{rt, Pkg};
use std::collections::{BTreeMap, HashMap, HashSet};
use tree::*;
use walk::{Walk, P};

pub(crate) fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "docx".into(), msg: msg.into() }
}

/// `UnitContext::extra` key with the part a unit is written back to.
pub const PART: &str = "docx.part";

/// One LocOS unit.
struct LUnit {
    p: usize,
    pieces: Vec<Piece>,
    style: String,
    part: usize,
    rprs: BTreeMap<String, Option<String>>,
    alt: bool,
}

/// python-docx `BabelFish.internal2ui`.
fn ui_style_name(name: &str) -> String {
    match name {
        "caption" => "Caption".into(),
        "footer" => "Footer".into(),
        "header" => "Header".into(),
        n => match n.strip_prefix("heading ") {
            Some(d) if d.len() == 1 && ("1"..="9").contains(&d) => format!("Heading {d}"),
            _ => n.to_string(),
        },
    }
}

/// `_paragraph_style_names`: (styleId -> name of the paragraph styles, the default paragraph style's name). With no
/// styles part python-docx invents its default one, whose only paragraph style is Normal.
fn style_names(pkg: &Pkg) -> (HashMap<String, String>, Option<String>) {
    let Some(d) = pkg.related(Some(pkg.main), &rt("styles")).and_then(|s| pkg.parts[s].dom.as_ref()) else {
        return (HashMap::from([("Normal".to_string(), "Normal".to_string())]), Some("Normal".into()));
    };
    let mut names = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut default_name = None;
    for el in d.element_children(d.root).filter(|&e| is_w(d, e, "style")) {
        let is_para = get_w(d, el, "type") == Some("paragraph");
        let is_default = matches!(get_w(d, el, "default"), Some("1") | Some("true") | Some("on"));
        let sid = get_w(d, el, "styleId").filter(|s| !s.is_empty()).map(str::to_string);
        if !is_para {
            if let Some(s) = sid {
                seen.insert(s);
            }
            continue;
        }
        let name = find_w(d, el, "name").and_then(|n| get_w(d, n, "val")).map(ui_style_name).unwrap_or_default();
        if let Some(s) = sid {
            if seen.insert(s.clone()) {
                names.insert(s, name.clone());
            }
        }
        if is_default {
            default_name = Some(name);
        }
    }
    (names, default_name)
}

/// `docx_bilingual.translation_paragraphs`: the paragraphs of the Translation column of every Storyline
/// "Export to Word" table, or None when the document has none (or more prose outside its tables).
fn bilingual_paragraphs(d: &Dom) -> Option<HashSet<usize>> {
    let cell_text = |tc: usize| py_strip(&t_text(d, tc)).to_string();
    let norm = |t: &str| -> String {
        let joined = t.split(py_isspace_char).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
        joined.trim_end_matches(':').to_lowercase()
    };
    let mut keep: HashSet<usize> = HashSet::new();
    let mut src_chars = 0usize;
    for tbl in d.iter(d.root).into_iter().filter(|&t| is_w(d, t, "tbl")) {
        let trs: Vec<usize> = d.element_children(tbl).filter(|&tr| is_w(d, tr, "tr")).collect();
        if trs.len() < 2 {
            continue;
        }
        let head: Vec<usize> = d.element_children(trs[0]).filter(|&tc| is_w(d, tc, "tc")).collect();
        let header: Vec<String> = head.iter().map(|&tc| norm(&cell_text(tc))).collect();
        let mut cols = None;
        'find: for (j, name) in header.iter().enumerate() {
            if name == "translation" {
                for i in (0..j).rev() {
                    if ["original text", "original", "source text", "source"].contains(&header[i].as_str()) {
                        cols = Some((i, j));
                        break 'find;
                    }
                }
            }
        }
        let Some((src, tgt)) = cols else { continue };
        for &tr in &trs[1..] {
            let cells: Vec<usize> = d.element_children(tr).filter(|&tc| is_w(d, tc, "tc")).collect();
            if cells.len() != head.len() {
                continue;
            }
            if cells.iter().map(|&tc| norm(&cell_text(tc))).collect::<Vec<_>>() == header {
                continue;
            }
            if cell_text(cells[tgt]).is_empty() {
                continue;
            }
            keep.extend(d.iter(cells[tgt]).into_iter().filter(|&p| is_w(d, p, "p")));
            src_chars += cell_text(cells[src]).chars().count();
        }
    }
    if keep.is_empty() {
        return None;
    }
    // prose outside every table (headings aside) larger than the source column: an ordinary document
    let mut prose = 0usize;
    for p in d.iter(d.root).into_iter().filter(|&p| is_w(d, p, "p")) {
        let mut a = d.nodes[p].parent;
        let mut in_table = false;
        while let Some(x) = a {
            if is_w(d, x, "tbl") {
                in_table = true;
                break;
            }
            a = d.nodes[x].parent;
        }
        if in_table {
            continue;
        }
        let sid = find_w(d, p, "pPr").and_then(|ppr| find_w(d, ppr, "pStyle")).and_then(|s| get_w(d, s, "val")).unwrap_or("").to_lowercase();
        if sid.starts_with("heading") || sid.starts_with("title") {
            continue;
        }
        prose += t_text(d, p).chars().count();
    }
    if prose > src_chars {
        return None;
    }
    Some(keep)
}

/// One translatable alt text or title (`_alt_units`).
struct Alt {
    part: usize,
    dp: usize,
    attr: &'static str,
    value: String,
    style: &'static str,
}

fn alt_units(pkg: &Pkg, w: &Walk) -> Vec<Alt> {
    let mut out = Vec::new();
    let main = pkg.main;
    let md = pkg.parts[main].dom.as_ref().unwrap();
    let mut roots: Vec<(usize, usize)> = find_w(md, md.root, "body").map(|b| vec![(main, b)]).unwrap_or_default();
    roots.extend(w.roots.iter().map(|&p| (p, pkg.parts[p].dom.as_ref().unwrap().root)));
    for (part, root) in roots {
        let d = pkg.parts[part].dom.as_ref().unwrap();
        for dp in d.iter(root).into_iter().filter(|&x| d.is(x, WP, "docPr")) {
            for (attr, style) in [("descr", "a11y:img-alt"), ("title", "a11y:img-title")] {
                let value = py_strip(d.get(dp, attr).unwrap_or("")).to_string();
                if !value.is_empty() && py_has_alnum(&value) {
                    out.push(Alt { part, dp, attr, value, style });
                }
            }
        }
    }
    out
}

/// `_set_alt`.
fn set_alt(d: &mut Dom, dp: usize, attr: &str, new: &str) {
    let old = d.get(dp, attr).map(str::to_string);
    d.set(dp, attr, new);
    let Some(container) = d.nodes[dp].parent else { return };
    for cnv in d.iter(container) {
        if d.is(cnv, PIC, "cNvPr") && old.is_some() && d.get(cnv, attr).map(str::to_string) == old {
            d.set(cnv, attr, new);
        }
    }
}

/// `DocxFilter.parse`: the LocOS units of a package.
fn locos_units(pkg: &mut Pkg, comments: bool) -> Result<(Vec<LUnit>, Walk), Error> {
    let w = walk::walk(pkg)?;
    let (names, default_name) = style_names(pkg);
    let mut units = Vec::new();
    for (i, &(part, p)) in w.paras.iter().enumerate() {
        let d = pkg.parts[part].dom.as_ref().unwrap();
        if !para::has_text(d, p) {
            continue;
        }
        let sid = match find_w(d, p, "pPr").and_then(|ppr| find_w(d, ppr, "pStyle")) {
            Some(ps) => Some(get_w(d, ps, "val").ok_or_else(|| perr("a w:pStyle has no w:val"))?.to_string()),
            None => None,
        };
        let style = sid.as_ref().and_then(|s| names.get(s)).cloned().unwrap_or_else(|| default_name.clone().unwrap_or_default());
        let tok = para::tokenize(d, p);
        let rprs = tok.rprs.iter().map(|(k, v)| (k.clone(), v.map(|r| d.serialize_node(r)))).collect();
        units.push(LUnit { p: i, pieces: tok.pieces, style, part, rprs, alt: false });
    }
    let main = pkg.main;
    let bilingual = bilingual_paragraphs(pkg.parts[main].dom.as_ref().unwrap());
    if let Some(keep) = &bilingual {
        units.retain(|u| u.part == main && keep.contains(&w.paras[u.p].1));
    }
    if bilingual.is_none() {
        let base = w.paras.len();
        for (j, a) in alt_units(pkg, &w).into_iter().enumerate() {
            units.push(LUnit { p: base + j, pieces: vec![Piece::Text(a.value)], style: a.style.to_string(), part: a.part, rprs: BTreeMap::new(), alt: true });
        }
    }
    if !comments {
        let comment_parts: HashSet<&str> = walk::extra_parts(pkg).into_iter().filter(|(k, _)| *k == "comments").map(|(_, p)| pkg.parts[p].name.as_str()).collect();
        units.retain(|u| !comment_parts.contains(pkg.parts[u.part].name.as_str()));
    }
    Ok((units, w))
}

fn document_unit(u: LUnit, pkg: &Pkg) -> Unit {
    let mut source = Vec::new();
    let mut codes = BTreeMap::new();
    for piece in u.pieces {
        match piece {
            Piece::Text(t) => source.push(Inline::text(t)),
            Piece::Open(n) => {
                let rpr = u.rprs.get(&n.to_string()).cloned().flatten().unwrap_or_default();
                codes.insert(n.to_string(), CodeData::paired(format!("<w:r>{rpr}"), "</w:r>").with_label("run"));
                source.push(Inline::tag(n.to_string(), TagKind::Open, "<w:r>"));
            }
            Piece::Close(n) => source.push(Inline::tag(n.to_string(), TagKind::Close, "</w:r>")),
            Piece::Point(n) => {
                codes.insert(n.to_string(), CodeData::standalone("object").with_label("object"));
                source.push(Inline::tag(n.to_string(), TagKind::Standalone, "object"));
            }
        }
    }
    let mut unit = Unit::new(format!("p{}", u.p), source);
    unit.codes = codes;
    let mut ctx = UnitContext::new().with_kind(if u.alt { "alt-text" } else { "paragraph" });
    if !u.style.is_empty() {
        ctx = ctx.with_style(u.style.clone());
    }
    ctx.extra.insert(profile::P_INDEX.to_string(), u.p.to_string());
    ctx.extra.insert(PART.to_string(), pkg.parts[u.part].name.clone());
    unit.context = ctx;
    unit
}

/// Extracts a DOCX the way Localization OS does.
pub fn extract(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let mut pkg = Pkg::load(bytes, &opts.limits())?;
    let comments = opts.format_flag("docx.translate_comments") != Some(false);
    let (units, _) = locos_units(&mut pkg, comments)?;
    let units = units.into_iter().map(|u| document_unit(u, &pkg)).collect();
    Ok(Document::new("docx", units, Skeleton::binary(bytes.to_vec(), bilingua_model::parser_info!()))
        .with_source_lang(opts.source_lang.clone())
        .with_target_lang(opts.target_lang.clone())
        .with_granularity(Granularity::Paragraph))
}

/// The LocOS text of inline content: tag ids are LocOS's run (`r`) and object (`x`) numbers.
fn locos_text(inl: &[Inline]) -> String {
    let mut s = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => s.push_str(&para::esc_lit(t)),
            Inline::Tag { id, kind, .. } => s.push_str(&match kind {
                TagKind::Open => format!("\u{27e6}r{id}\u{27e7}"),
                TagKind::Close => format!("\u{27e6}/r{id}\u{27e7}"),
                TagKind::Standalone => format!("\u{27e6}x{id}\u{27e7}"),
            }),
        }
    }
    s
}

/// The texts the document writes, by LocOS's `p_index`, and whether each equals its source.
fn written_texts(doc: &Document) -> BTreeMap<usize, (String, bool)> {
    let mut out = BTreeMap::new();
    for u in doc.units.iter().filter(|u| u.translate) {
        let Some(p) = u.context.extra.get(profile::P_INDEX).and_then(|p| p.parse::<usize>().ok()) else { continue };
        if !u.segments.iter().any(|s| s.has_target()) {
            continue;
        }
        let (mut text, mut source) = (String::new(), String::new());
        for (i, s) in u.segments.iter().enumerate() {
            if i > 0 {
                let sep = u.separators.get(i - 1).map(String::as_str).unwrap_or("");
                text.push_str(sep);
                source.push_str(sep);
            }
            text.push_str(&locos_text(s.output()));
            source.push_str(&locos_text(&s.source));
        }
        let same = text == source;
        out.insert(p, (text, same));
    }
    out
}

/// `xml_target_text`: XML 1.0 characters only, Word's pasted breaks as newlines.
fn xml_target_text(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\u{b}' || c == '\u{c}' { '\n' } else { c })
        .filter(|&c| !matches!(c, '\u{0}'..='\u{8}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}'))
        .collect()
}

/// Writes a document extracted under the profile (its skeleton holds the original file).
pub fn merge(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    let Skeleton::Binary { data, .. } = &doc.skeleton else {
        return Err(Error::Write("docx: a LocOS-profile document needs its original file".into()));
    };
    let texts = written_texts(doc);
    let locale = opts.stamp_lang(doc, !texts.is_empty()).map(str::to_string);
    let limits = bilingua_model::Limits::default();
    // LocOS's B-bytes guard: a plain build that writes nothing, or only each unit's own source, is the upload
    if locale.is_none() && texts.values().all(|(_, same)| *same) {
        Pkg::load(data, &limits)?;
        return Ok(data.clone());
    }
    let mut pkg = Pkg::load(data, &limits)?;
    let w = walk::walk(&mut pkg)?;
    let alts = alt_units(&pkg, &w);
    let mut ids = {
        let mut doms: Vec<&Dom> = vec![pkg.parts[pkg.main].dom.as_ref().unwrap()];
        doms.extend(w.roots.iter().map(|&p| pkg.parts[p].dom.as_ref().unwrap()));
        para::IdGen::scan(&doms)
    };
    let mut translated: Vec<P> = Vec::new();
    let mut touched: HashSet<usize> = HashSet::new();
    for (&i, (text, _)) in &texts {
        let text = xml_target_text(text);
        if i < w.paras.len() {
            let (part, p) = w.paras[i];
            let twins: Vec<P> = match w.mirrors.get(&i) {
                Some(ms) => {
                    let d = pkg.parts[part].dom.as_ref().unwrap();
                    let shape = para::pieces_text(&para::tokenize(d, p).pieces);
                    ms.iter().copied().filter(|&(mp, m)| para::pieces_text(&para::tokenize(pkg.parts[mp].dom.as_ref().unwrap(), m).pieces) == shape).collect()
                }
                None => Vec::new(),
            };
            para::write(pkg.parts[part].dom.as_mut().unwrap(), p, &text, &mut ids).map_err(|e| Error::Write(format!("DOCX paragraph {} could not be written ({e}). Review its target text.", i + 1)))?;
            translated.push((part, p));
            touched.insert(w.parts[i]);
            for (tp, t) in twins {
                if para::write(pkg.parts[tp].dom.as_mut().unwrap(), t, &text, &mut ids).is_ok() {
                    translated.push((tp, t));
                }
            }
        } else if let Some(a) = alts.get(i - w.paras.len()) {
            set_alt(pkg.parts[a.part].dom.as_mut().unwrap(), a.dp, a.attr, &para::unesc_lit(&text));
            touched.insert(a.part);
        }
    }
    for t in touched {
        if !pkg.parts[t].xml && pkg.parts[t].dom.is_some() {
            pkg.parts[t].dirty = true;
        }
    }
    if let Some(tag) = locale.as_deref().and_then(profile::lang_tag) {
        lang::declare(&mut pkg, &tag, profile::is_rtl(&tag), &translated);
    }
    pkg.save()
}

#[cfg(test)]
mod tests;
