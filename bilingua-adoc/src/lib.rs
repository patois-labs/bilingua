//! Bilingua AsciiDoc filter (`.adoc`, `.asciidoc`, `.asc`). See the README.
//!
//! A line-grammar walk over the source, as the Localization OS filter does: nothing is executed or followed (an
//! `include::` line is kept as it is). Paragraphs, titles, list items, description list terms, admonition
//! paragraphs, block titles, table cells and the header's description and keywords are units; listing, literal,
//! passthrough and comment blocks, attribute entries, block attribute lines, macros and directives stay in the
//! skeleton. Inline formatting, links, macros and attribute references are tags.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod block;
mod inline;
mod lex;
mod meta;
mod render;

use bilingua_model::*;
use std::collections::BTreeMap;

/// The AsciiDoc filter.
pub struct Adoc;

const ID: &str = "adoc";
// Keys of the unit's hidden values (`Unit::hidden`).
const RAW: &str = "raw"; // the source text of the span
const SEP: &str = "sep"; // table cell separator
const LS: &str = "ls"; // the span starts a line
const META: &str = "meta"; // a header description / keywords value

struct Item {
    start: usize,
    end: usize,
    unit: usize,
}

fn flag(u: &Unit, key: &str) -> bool {
    u.has_hidden(key)
}

impl Filter for Adoc {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["adoc", "asciidoc", "asc"]
    }

    fn sniff(&self, _bytes: &[u8]) -> bool {
        false // AsciiDoc is plain text; only the extension says so
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        let mut dec = enc::decode_text(bytes, &enc::DecodeOptions::new(opts))?;
        let text = std::mem::take(&mut dec.text);
        if lex::strip(&text).starts_with("-----BEGIN PGP") {
            return Err(Error::Parse {
                format: ID.into(),
                msg: "This .asc file is an OpenPGP signature or key, not an AsciiDoc document.".into(),
            });
        }
        let slots = block::slots(&text, &limits)?;
        let mut units: Vec<Unit> = Vec::new();
        let mut items: Vec<Item> = Vec::new();
        let mut locked: Vec<(&block::Slot, Vec<Inline>, BTreeMap<String, CodeData>)> = Vec::new();
        let mut heading: Option<String> = None;
        for sl in &slots {
            let raw = &text[sl.start..sl.end];
            let (inl, codes) = inline::unit_of(raw);
            let translatable = inl.iter().any(|i| matches!(i, Inline::Text(t) if lex::any_letter(t)));
            if sl.kind == "heading" {
                heading = Some(lex::strip(raw).to_string());
            }
            if !translatable {
                if opts.include_locked {
                    locked.push((sl, inl, codes));
                }
                continue;
            }
            let mut u = Unit::new(format!("u{}", units.len() + 1), inl);
            u.codes = codes;
            u.set_hidden(RAW, raw.to_string());
            if let Some(sep) = sl.sep {
                u.set_hidden(SEP, sep.to_string());
            }
            if sl.start == 0 || matches!(text.as_bytes()[sl.start - 1], b'\r' | b'\n') {
                u.set_hidden(LS, "1");
            }
            u.context.kind = Some(sl.kind.to_string());
            if sl.kind == "heading" {
                u.context.extra.insert("level".into(), sl.level.to_string());
            } else {
                u.context.heading = heading.clone();
            }
            u.context.location = Some(format!("line {}", sl.line));
            items.push(Item { start: sl.start, end: sl.end, unit: units.len() });
            units.push(u);
        }
        // #733: description / keywords of the header come after every body unit
        let body_items = items.len();
        for m in meta::header_meta(&text) {
            let overlaps = items[..body_items].iter().any(|it| it.start < m.end && m.start < it.end);
            if overlaps {
                continue;
            }
            let (inl, codes) = meta::meta_unit(&m.value);
            let mut u = Unit::new(format!("u{}", units.len() + 1), inl);
            u.codes = codes;
            u.set_hidden(RAW, text[m.start..m.end].to_string());
            u.set_hidden(META, "1");
            u.context.kind = Some("metadata".into());
            u.context.style = Some(m.style.to_string());
            u.context.location = Some(format!("line {}", m.line));
            items.push(Item { start: m.start, end: m.end, unit: units.len() });
            units.push(u);
        }
        let taken = items.len();
        for (n, (sl, inl, codes)) in locked.into_iter().enumerate() {
            if items[..taken].iter().any(|it| it.start < sl.end && sl.start < it.end) {
                continue;
            }
            let mut u = Unit::new(format!("k{}", n + 1), inl);
            u.codes = codes;
            u.set_hidden(RAW, text[sl.start..sl.end].to_string());
            u.translate = false;
            u.lock_reason = Some("no translatable text".into());
            u.context.kind = Some(sl.kind.to_string());
            u.context.location = Some(format!("line {}", sl.line));
            items.push(Item { start: sl.start, end: sl.end, unit: units.len() });
            units.push(u);
        }
        items.sort_by_key(|i| i.start);
        let mut parts: Vec<SkelPart> = Vec::with_capacity(items.len() * 2 + 1);
        let mut pos = 0usize;
        for it in &items {
            if it.start > pos {
                parts.push(SkelPart::Raw(text[pos..it.start].to_string()));
            }
            parts.push(SkelPart::Unit(it.unit));
            pos = it.end;
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let doc = Document::new(ID, units, Skeleton::text(parts, dec.encoding.clone(), bilingua_model::parser_info!())).with_options(opts);
        guard::finish(doc, std::mem::take(&mut dec.warnings), &limits)
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("adoc needs a text skeleton".into()));
        };
        let raw_of = |u: &Unit| u.hidden(RAW).map(str::to_string).unwrap_or_default();
        // the original text, for the URL attributes (built only when a link needs it)
        let original = || {
            let mut s = String::new();
            for p in parts {
                match p {
                    SkelPart::Raw(r) => s.push_str(r),
                    SkelPart::Unit(i) => {
                        if let Some(u) = doc.units.get(*i) {
                            s.push_str(&raw_of(u));
                        }
                    }
                }
            }
            s
        };
        let mut attrs = render::Attrs::new(original);
        let mut out = String::new();
        let mut translated = false;
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    let raw = raw_of(u);
                    if !u.translate || u.segments.iter().all(|s| s.target.is_empty()) {
                        out.push_str(&raw);
                        continue;
                    }
                    translated = true;
                    let (src, new) = (u.source(), u.output());
                    if src == new {
                        out.push_str(&raw);
                    } else if flag(u, META) {
                        let t = meta::meta_render(u, &new);
                        out.push_str(if t.is_empty() { &raw } else { &t });
                    } else {
                        let sep = u.hidden(SEP).and_then(|s| s.chars().next());
                        let mut r = render::render(u, &new, sep, &mut attrs);
                        // a translation that opens with block syntax would turn the paragraph into another block
                        if flag(u, LS) && sep.is_none() && lex::is_block_start(&r) {
                            r.insert_str(0, "{empty}");
                        }
                        out.push_str(&r);
                    }
                }
            }
        }
        if let Some(lang) = opts.stamp_lang(doc, translated) {
            out = meta::set_lang(&out, lang);
        }
        enc::encode_text_with(&out, encoding, opts)
    }
}
