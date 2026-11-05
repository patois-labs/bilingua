//! Bilingua IDML filter: Adobe InDesign packages to units and back. See the README for the details.
//!
//! ```ignore
//! let f = bilingua_idml::Idml;
//! let mut doc = f.extract(&bytes, &ExtractOptions::new().with_target_lang("de-DE"))?;
//! doc.set_target("p0", vec![Inline::text("Hallo.")])?;
//! let out = f.merge(&doc)?;
//! ```
//!
//! The story markup (paragraphs, style ranges as inline tags, rebuild) lives in the public [`story`] module on top
//! of the lossless tree in [`xml`], so the InCopy ICML filter can use it without the package code.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

pub mod lang;
mod package;
// the story and XML layers, shared with other Bilingua crates and used by this crate's tests: not part of the
// semver API (they change with the ICML move onto this tree)
#[doc(hidden)]
pub mod story;
#[doc(hidden)]
pub mod xml;

use bilingua_model::*;
use bilingua_zip::{Body, EntryMeta, ZipLimits};
use package::{encode_xml, parse_xml, perr, Budget};
use std::collections::{BTreeMap, HashMap};

/// The skeleton holds the original package under this name (it contains a NUL, which no real zip entry name can).
const ORIGINAL: &str = "\0bilingua-idml-original";

pub struct Idml;

fn read(bytes: &[u8], l: &Limits) -> Result<Vec<bilingua_zip::RawEntry>, Error> {
    package::check_member_names(bytes)?;
    let entries = bilingua_zip::read_zip("idml", bytes, &ZipLimits::from(l))?;
    bilingua_zip::check_xml_parts(&entries, l)?;
    Ok(entries)
}

fn extract_doc(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let l = opts.limits();
    guard::check_input(bytes, &l)?;
    let entries = read(bytes, &l)?;
    let mut budget = Budget::new(&l);
    let pkg = package::open(&entries, &l, &mut budget)?;
    let mut units: Vec<Unit> = vec![];
    let mut idx = 0usize;
    for member in &pkg.stories {
        let Some(e) = package::find(&entries, member) else { continue };
        let (mut tree, _) = parse_xml(&e.data, member, Some(&mut budget), &l)?;
        let so = story::Opts { hidden_conditions: pkg.hidden_conditions.clone() };
        let paras = story::collect(&mut tree, &so);
        for (local, para) in paras.iter().enumerate() {
            let tok = story::tokenize(&tree, para);
            if tok.has_text() {
                let mut u = Unit::new(format!("p{idx}"), tok.core_inlines());
                u.codes = tok.codes().clone();
                u.context = UnitContext::new().with_kind("paragraph".to_string()).with_style(Some(story::style(&tree, para)).filter(|s| !s.is_empty())).with_location(format!("{member} paragraph {}", local + 1)).with_extra(BTreeMap::from([("story".to_string(), member.clone())]));
                units.push(u);
                guard::check_unit_count(units.len(), &l)?;
            }
            idx += 1;
        }
    }
    let skeleton = Skeleton::zip(vec![ZipEntry::raw(ORIGINAL, bytes.to_vec())], parser_info!());
    let doc = Document::new("idml", units, skeleton).with_options(opts);
    guard::finish(doc, vec![], &l)
}

fn paragraph_index(id: &str) -> Option<usize> {
    id.strip_prefix('p')?.parse().ok()
}

fn merge_doc(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    check_parser(doc, &parser_info!())?;
    let Skeleton::Zip { entries: skel, .. } = &doc.skeleton else {
        return Err(Error::Write("idml needs a zip skeleton".into()));
    };
    let original = skel
        .iter()
        .find(|e| e.name == ORIGINAL)
        .and_then(|e| if let ZipData::Raw(b) = &e.data { Some(b) } else { None })
        .ok_or_else(|| Error::Write("the idml skeleton lost the original package".into()))?;
    // paragraph index -> the translated text; an untranslated unit keeps the original markup
    let mut wanted: HashMap<usize, Vec<Inline>> = HashMap::new();
    for u in &doc.units {
        if !u.translate || !u.segments.iter().any(|s| s.has_target()) {
            continue;
        }
        if let Some(i) = paragraph_index(&u.id) {
            wanted.insert(i, u.output());
        }
    }
    if wanted.is_empty() {
        return Ok(original.clone());
    }
    let l = Limits::default();
    let entries = read(original, &l)?;
    let pkg = package::open(&entries, &l, &mut Budget::new(&l))?;
    // InDesign keeps the language per paragraph: the stories this merge rewrites (the translated ones) get it, so
    // `stamp_target_always` adds nothing (an untranslated story is never rewritten). The stories are UTF-8 XML, so
    // `output_encoding` has nothing to change either.
    let tag = opts.stamp_lang(doc, true);
    let language = tag.and_then(|t| lang::indesign_language(&pkg.languages, t));
    let rtl = tag.is_some_and(lang::is_rtl);
    let so = story::Opts { hidden_conditions: pkg.hidden_conditions.clone() };
    let mut replaced: HashMap<&str, Vec<u8>> = HashMap::new();
    let mut idx = 0usize;
    for member in &pkg.stories {
        let Some(e) = package::find(&entries, member) else { continue };
        let (mut tree, enc) = parse_xml(&e.data, member, None, &l)?;
        let paras = story::collect(&mut tree, &so);
        let hit = (idx..idx + paras.len()).any(|k| wanted.contains_key(&k));
        if hit {
            story::normalize(&mut tree, &paras);
            let paras2 = story::collect(&mut tree, &so);
            if paras2.len() != paras.len() {
                return Err(perr(format!("{member}: the paragraph structure changed on rebuild")));
            }
            let mut plans = vec![];
            for (k, para) in paras2.iter().enumerate() {
                let Some(new) = wanted.get(&(idx + k)) else { continue };
                let tok = story::tokenize(&tree, para);
                if tok.has_text() {
                    plans.push((para, tok, new));
                }
            }
            if !plans.is_empty() {
                for (para, tok, new) in plans {
                    let ranges = story::rebuild(&mut tree, para, &tok, new);
                    if let Some(lg) = &language {
                        story::stamp_language(&mut tree, &ranges, lg);
                    }
                    if rtl {
                        story::stamp_rtl(&mut tree, &ranges);
                    }
                }
                replaced.insert(member.as_str(), encode_xml(&tree.to_xml(), enc));
            }
        }
        idx += paras.len();
    }
    if replaced.is_empty() {
        return Ok(original.clone());
    }
    // Every member the translation did not touch is written back as it was; `mimetype` is first and stored.
    let mut order: Vec<&bilingua_zip::RawEntry> = entries.iter().collect();
    order.sort_by_key(|e| if e.name == "mimetype" { 0 } else { 1 });
    let mut out: Vec<(EntryMeta, Body)> = Vec::with_capacity(order.len());
    for e in order {
        let mut meta = e.meta.clone();
        let body = if let Some(b) = replaced.remove(e.name.as_str()) {
            Body::Owned(b)
        } else if e.name == "mimetype" {
            meta.stored = true;
            Body::Plain(&e.data)
        } else {
            match &e.packed {
                Some((p, bytes)) if !meta.stored && !meta.dir => Body::Packed(p, bytes),
                _ => Body::Plain(&e.data),
            }
        };
        out.push((meta, body));
    }
    bilingua_zip::write_zip_parts(&out)
}

impl Filter for Idml {
    fn id(&self) -> &'static str {
        "idml"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["idml"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
        bytes.starts_with(b"PK\x03\x04") && (has(b"application/vnd.adobe.indesign-idml-package") || (has(b"designmap.xml") && has(b"Stories/")))
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        extract_doc(bytes, opts)
    }
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        merge_doc(doc, opts)
    }
}
