//! Bilingua wiki filters: MediaWiki wikitext (`wiki`, `mediawiki`, `wikitext`) and DokuWiki (`dokuwiki`). See the
//! README.
//!
//! A line-grammar walk over the source, as the Localization OS filter does: nothing is expanded, executed or
//! transcluded. Headings, paragraphs, list items, definition terms, table cells, link labels, file captions and
//! footnotes are units; comments, `<nowiki>`, code and math blocks, templates, magic words, categories, redirects and
//! preformatted lines stay in the skeleton. Links, templates, URLs, HTML tags and bold / italic marks are tags.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod block;
mod inline;
mod lex;
mod regions;
mod render;

use bilingua_model::*;
use std::collections::{HashMap, HashSet};

// Hidden codes on a unit (names start with `~`, never a tag id).
const RAW: &str = "raw"; // the source text of the span
const CELL: &str = "cell"; // how the translation is escaped: tpl, mw or dw

/// The MediaWiki filter. `template_params` is the install's allow-list of translatable template parameters
/// (LocOS `WIKI_TEMPLATE_PARAMS`); empty by default, which protects every template.
#[derive(Debug, Clone, Default)]
pub struct MediaWiki {
    allow: HashSet<(String, String)>,
}

/// The `ExtractOptions::format_options` key that lists translatable template parameters for one call, in the
/// [`MediaWiki::with_template_params`] spelling (`Quote:text,Note:1`); it adds to the filter's own list.
pub const TEMPLATE_PARAMS_KEY: &str = "wiki.template_params";

/// The `format_options` keys of the MediaWiki filter.
pub const FORMAT_OPTIONS: &[FormatOptionKey] =
    &[FormatOptionKey::text(TEMPLATE_PARAMS_KEY, "Template parameters whose values are translated (see TEMPLATE_PARAMS_KEY).")];

/// The DokuWiki filter.
#[derive(Debug, Clone, Copy, Default)]
pub struct DokuWiki;

impl MediaWiki {
    /// A filter that translates the listed template parameters: comma-separated `Template:param` pairs, for example
    /// `Quote:text,Note:1`. Template names compare like MediaWiki titles (first letter case-insensitive, `_` = space).
    pub fn with_template_params(spec: &str) -> MediaWiki {
        let mut allow = HashSet::new();
        for item in spec.split(',') {
            let item = lex::strip(item);
            let Some((name, param)) = item.rsplit_once(':') else { continue };
            if lex::strip(name).is_empty() || lex::strip(param).is_empty() {
                continue;
            }
            let name = if name.len() >= 9 && name.is_char_boundary(9) && name[..9].eq_ignore_ascii_case("template:") { &name[9..] } else { name };
            allow.insert((regions::norm_title(name), lex::strip(param).to_string()));
        }
        MediaWiki { allow }
    }
}

fn strip_edges(inl: Vec<Inline>) -> Vec<Inline> {
    let mut v = merge_text(inl);
    if let Some(Inline::Text(t)) = v.first_mut() {
        *t = t.trim_start_matches(lex::is_space).to_string();
    }
    if let Some(Inline::Text(t)) = v.last_mut() {
        *t = t.trim_end_matches(lex::is_space).to_string();
    }
    v.retain(|i| !matches!(i, Inline::Text(t) if t.is_empty()));
    v
}

/// The Localization OS markup budget: markup characters bound the regions, brackets and tags the walk builds.
fn check_budget(text: &str, l: &Limits) -> Result<(), Error> {
    let n = text.bytes().filter(|c| matches!(c, b'{' | b'[' | b'<' | b'\'' | b'%')).count();
    if n > 40 * l.max_units {
        return Err(LimitError::new(
            LimitKind::Units,
            format!("This wiki page has far more markup than {} segments allow. Split it into smaller files.", l.max_units),
        )
        .into());
    }
    Ok(())
}

fn extract_wiki(id: &'static str, dw: bool, allow: &HashSet<(String, String)>, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let limits = opts.limits();
    let mut dec = enc::decode_text(bytes, &enc::DecodeOptions::new(opts))?;
    let text = std::mem::take(&mut dec.text);
    check_budget(&text, &limits)?;
    let (regs, holes) = if dw { (regions::dw_regions(&text), Vec::new()) } else { regions::mw_regions(&text, allow) };
    let mask = regions::masked(&text, &regs);
    let starts: HashMap<usize, usize> = regs.iter().copied().collect();
    let links = if dw { HashMap::new() } else { inline::link_match(&mask) };
    let cx = inline::Ctx { text: &text, mask: &mask, starts: &starts, links: &links, dw };
    let mut walk = block::Walk::new(&cx, limits);
    if dw {
        block::dw_slots(&mut walk)?;
    } else {
        let mut h = holes;
        h.sort_unstable();
        block::mw_slots(&mut walk, &h)?;
    }
    let slots = walk.slots;
    for sl in &slots {
        guard::check_unit_chars(lex::char_count(&text, sl.start, sl.end), &limits)?;
    }
    let mut units: Vec<Unit> = Vec::new();
    let mut parts: Vec<SkelPart> = Vec::with_capacity(slots.len() * 2 + 1);
    let mut pos = 0usize;
    let (mut line, mut line_pos) = (1usize, 0usize);
    let mut heading: Option<String> = None;
    let (mut n_tr, mut n_lock) = (0usize, 0usize);
    let tb = text.as_bytes();
    for slot in &slots {
        if slot.start < pos {
            return Err(Error::Parse { format: id.into(), msg: "overlapping units".into() });
        }
        let mut tok = inline::Tok::new(&cx);
        let mut inl = Vec::new();
        tok.run(slot.start, slot.end, 0, &mut inl);
        let inl = strip_edges(inl);
        let translatable = lex::any_letter_in(&inl);
        let raw = lex::sl(&text, slot.start, slot.end);
        if slot.kind == "heading" {
            heading = Some(lex::strip(raw).to_string());
        }
        if !translatable && !opts.include_locked {
            continue;
        }
        while line_pos < slot.start {
            if tb[line_pos] == b'\n' || (tb[line_pos] == b'\r' && tb.get(line_pos + 1) != Some(&b'\n')) {
                line += 1;
            }
            line_pos += 1;
        }
        let uid = if translatable {
            n_tr += 1;
            format!("u{n_tr}")
        } else {
            n_lock += 1;
            format!("k{n_lock}")
        };
        let mut u = Unit::new(uid, inl);
        u.codes = tok.codes;
        u.set_hidden(RAW, raw);
        if let Some(c) = slot.cell {
            u.set_hidden(CELL, c);
        }
        if !translatable {
            u.translate = false;
            u.lock_reason = Some("no translatable text".into());
        }
        u.context.kind = Some(slot.kind.to_string());
        if slot.kind == "heading" {
            u.context.extra.insert("level".into(), slot.level.to_string());
        } else {
            u.context.heading = heading.clone();
        }
        u.context.location = Some(format!("line {line}"));
        parts.push(SkelPart::Raw(lex::sl(&text, pos, slot.start).to_string()));
        parts.push(SkelPart::Unit(units.len()));
        units.push(u);
        pos = slot.end;
    }
    parts.push(SkelPart::Raw(text.get(pos..).unwrap_or("").to_string()));
    parts.retain(|p| !matches!(p, SkelPart::Raw(s) if s.is_empty()));
    let doc = Document::new(id, units, Skeleton::text(parts, dec.encoding.clone(), bilingua_model::parser_info!())).with_options(opts);
    guard::finish(doc, std::mem::take(&mut dec.warnings), &limits)
}

fn merge_wiki(id: &str, dw: bool, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write(format!("{id} needs a text skeleton")));
    };
    let raw_of = |u: &Unit| u.hidden(RAW).map(str::to_string).unwrap_or_default();
    let mut out = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => out.push_str(s),
            SkelPart::Unit(i) => {
                let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                let raw = raw_of(u);
                if !u.translate || u.segments.iter().all(|s| !s.has_target()) {
                    out.push_str(&raw);
                    continue;
                }
                let (src, new) = (u.source(), u.output());
                if src == new {
                    out.push_str(&raw);
                } else {
                    let cell = u.hidden(CELL);
                    out.push_str(&render::render(&new, &u.codes, cell, u.context.kind.as_deref(), dw));
                }
            }
        }
    }
    // Neither wiki declares a page language, so `stamp_target_always` has nothing to stamp; `output_encoding`
    // UTF-8 writes UTF-8 whatever the file was (LocOS behaviour).
    enc::encode_text_with(&out, encoding, opts)
}

impl Filter for MediaWiki {
    fn id(&self) -> &'static str {
        "wiki"
    }
    fn format_option_keys(&self) -> &'static [FormatOptionKey] {
        FORMAT_OPTIONS
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["wiki", "mediawiki", "wikitext"]
    }
    fn sniff(&self, _bytes: &[u8]) -> bool {
        false // wikitext is plain text; only the extension says so
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        // The filter's allow-list plus `format_options["wiki.template_params"]` (same spelling) for this call.
        match opts.format_option(TEMPLATE_PARAMS_KEY) {
            Some(spec) if !lex::strip(spec).is_empty() => {
                let mut allow = self.allow.clone();
                allow.extend(MediaWiki::with_template_params(spec).allow);
                extract_wiki("wiki", false, &allow, bytes, opts)
            }
            _ => extract_wiki("wiki", false, &self.allow, bytes, opts),
        }
    }
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        merge_wiki("wiki", false, doc, opts)
    }
}

impl Filter for DokuWiki {
    fn id(&self) -> &'static str {
        "dokuwiki"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["dokuwiki"]
    }
    fn sniff(&self, _bytes: &[u8]) -> bool {
        false // a .txt file is never guessed to be DokuWiki
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        extract_wiki("dokuwiki", true, &HashSet::new(), bytes, opts)
    }
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        merge_wiki("dokuwiki", true, doc, opts)
    }
}
