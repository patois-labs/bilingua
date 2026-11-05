//! Bilingua Wordfast filters: Wordfast Pro TXML (`txml`) and TXLF (`txlf`). See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod engine;
mod inline;
mod text;
mod txlf;
mod txml;
mod xml;

use bilingua_model::*;
use engine::{Ctx, Item, Status, Walked};
use std::collections::HashMap;

pub use bilingua_model::Limits;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dialect {
    Txml,
    Txlf,
}

impl Dialect {
    fn id(self) -> &'static str {
        match self {
            Dialect::Txml => "txml",
            Dialect::Txlf => "txlf",
        }
    }
}

/// The Wordfast Pro TXML filter. `load_targets` (default true) lists segments somebody already confirmed as
/// read-only units with their translation as the target; the Localization OS adapter turns it off, which leaves
/// exactly the segments LocOS translates. `include_locked` in the extract options lists the locked and
/// non-translatable segments too.
#[derive(Debug, Clone, Copy)]
pub struct Txml {
    pub load_targets: bool,
}

/// The Wordfast Pro TXLF filter (XLIFF 1.2 with the gs4tr extension). Options as for [`Txml`].
#[derive(Debug, Clone, Copy)]
pub struct Txlf {
    pub load_targets: bool,
}

impl Default for Txml {
    fn default() -> Self {
        Txml { load_targets: true }
    }
}

impl Default for Txlf {
    fn default() -> Self {
        Txlf { load_targets: true }
    }
}

/// Both filters, TXLF before the generic XLIFF one when you register them.
pub fn filters() -> Vec<Box<dyn Filter>> {
    vec![Box::new(Txml::default()), Box::new(Txlf::default())]
}

// Hidden codes (ids start with `~`, never a tag id).
const RAW: &str = "~raw"; // the region as the file had it
const PRE: &str = "~pre"; // text before a written translation
const POST: &str = "~post"; // text after it
const LEAD: &str = "~lead"; // white space the source had before / after its text
const TRAIL: &str = "~trail";
const T0: &str = "~t0"; // the target as it was loaded (comparison string)
const TMPL: &str = "~tmpl"; // a mirrored seg-source target to create
const KEY: &str = "~key"; // the LocOS unit key

fn hidden(text: impl Into<String>) -> CodeData {
    CodeData { standalone: Some(text.into()), ..Default::default() }
}

fn code(u: &Unit, key: &str) -> Option<String> {
    u.codes.get(key).and_then(|c| c.standalone.clone())
}

fn head_text(bytes: &[u8]) -> String {
    let head = &bytes[..bytes.len().min(4096)];
    if head.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    String::from_utf8_lossy(head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head)).into_owned()
}

fn extract(d: Dialect, load_targets: bool, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
    let name = d.id();
    guard::check_input(bytes, limits)?;
    guard::check_xml(bytes, limits)?;
    let dec = text::decode(name, bytes, opts, limits)?;
    let text = &dec.text;
    // the markup budget: far more markup than the unit cap allows is refused before a tree is built
    if text.bytes().filter(|b| *b == b'<').count() > limits.max_units.saturating_mul(40) {
        return Err(guard::LimitError::new(
            LimitKind::XmlElements,
            format!("This {} file has far more markup than {} segments allow. Split it into smaller files.", name.to_uppercase(), limits.max_units),
        )
        .into());
    }
    let tree = xml::parse(name, text)?;
    let ctx = Ctx { text, tree: &tree, limits, want_protected: load_targets || opts.include_locked };
    let Walked { items, source_lang, target_lang } = match d {
        Dialect::Txml => txml::walk(&ctx)?,
        Dialect::Txlf => txlf::walk(&ctx)?,
    };
    let mut units: Vec<Unit> = Vec::new();
    let mut regions: Vec<(usize, usize)> = Vec::new(); // (start, unit index)
    let mut ends: Vec<usize> = Vec::new();
    for it in items {
        let keep = match it.status {
            Status::Open => true,
            Status::Confirmed(_) => load_targets || opts.include_locked,
            Status::Locked(_) => opts.include_locked,
        };
        if !keep {
            continue;
        }
        guard::check_unit_count(units.len() + 1, limits)?;
        let chars: usize = it.source.iter().map(|i| if let Inline::Text(t) = i { t.chars().count() } else { 1 }).sum();
        guard::check_unit_chars(chars, limits)?;
        let idx = units.len();
        if let Some(r) = &it.region {
            regions.push((r.start, idx));
            ends.push(r.end);
        }
        units.push(unit_of(it, text));
    }
    // the skeleton in file order
    let mut order: Vec<(usize, usize, usize)> = regions.iter().zip(&ends).map(|(&(s, i), &e)| (s, e, i)).collect();
    order.sort_unstable();
    let mut parts = Vec::with_capacity(order.len() * 2 + 1);
    let mut pos = 0usize;
    for (s, e, i) in order {
        if s < pos {
            return Err(xml::perr(name, "overlapping segments"));
        }
        if s > pos {
            parts.push(SkelPart::Raw(text[pos..s].to_string()));
        }
        parts.push(SkelPart::Unit(i));
        pos = e;
    }
    if pos < text.len() {
        parts.push(SkelPart::Raw(text[pos..].to_string()));
    }
    let lang = |file: &str, fallback: &Option<String>| (!file.is_empty()).then(|| file.to_string()).or_else(|| fallback.clone());
    let doc = Document {
        format: name.into(),
        name: None,
        source_lang: lang(&source_lang, &opts.source_lang),
        target_lang: lang(&target_lang, &opts.target_lang),
        granularity: opts.granularity,
        units,
        skeleton: Skeleton::text(parts, dec.encoding, bilingua_model::parser_info!()),
        warnings: Vec::new(),
    };
    guard::finish(doc, dec.warnings, limits)
}

fn unit_of(it: Item, text: &str) -> Unit {
    let mut u = Unit::new(it.key.to_string(), it.source);
    let seg = &mut u.segments[0];
    seg.target = it.target;
    seg.state = it.state;
    seg.origin = it.origin;
    let t0 = (!seg.target.is_empty()).then(|| inline::flat(&seg.target));
    u.codes = it.codes;
    u.notes = it.notes;
    u.context.kind = Some("segment".into());
    u.context.key = it.ctx_key;
    u.context.style = it.style;
    match it.status {
        Status::Open => {}
        Status::Confirmed(r) | Status::Locked(r) => {
            u.translate = false;
            u.lock_reason = Some(r.to_string());
        }
    }
    u.codes.insert(KEY.into(), hidden(it.key.to_string()));
    if let Some(r) = it.region {
        u.codes.insert(RAW.into(), hidden(&text[r.start..r.end]));
        u.codes.insert(PRE.into(), hidden(r.pre));
        u.codes.insert(POST.into(), hidden(r.post));
        u.codes.insert(LEAD.into(), hidden(it.lead));
        u.codes.insert(TRAIL.into(), hidden(it.trail));
        if let Some(t) = t0 {
            u.codes.insert(T0.into(), hidden(t));
        }
        for (k, v) in it.extra {
            u.codes.insert(k.into(), hidden(v));
        }
    }
    u
}

/// The translation to write for a unit, or None when it has none or it is the one the file already had.
fn content_of(u: &Unit) -> Option<String> {
    if !u.translate || u.segments.iter().all(|s| s.target.is_empty()) {
        return None;
    }
    let target = u.output();
    if code(u, T0).is_some_and(|t0| t0 == inline::flat(&target)) {
        return None;
    }
    Some(inline::render(&target, &u.codes, &code(u, LEAD).unwrap_or_default(), &code(u, TRAIL).unwrap_or_default()))
}

fn merge(d: Dialect, doc: &Document) -> Result<Vec<u8>, Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write(format!("{} needs a text skeleton", d.id())));
    };
    let mut by_key: Option<HashMap<&str, &Unit>> = None;
    let mut out = String::new();
    let mut written = false;
    for p in parts {
        match p {
            SkelPart::Raw(s) => out.push_str(s),
            SkelPart::Unit(i) => {
                let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                let raw = code(u, RAW).unwrap_or_default();
                if let Some(tmpl) = code(u, TMPL) {
                    // a target mirroring seg-source, created once for all the segments of the trans-unit
                    let map = by_key.get_or_insert_with(|| doc.units.iter().filter_map(|u| u.codes.get(KEY).and_then(|c| c.standalone.as_deref()).map(|k| (k, u))).collect());
                    let mut body = String::new();
                    let mut any = false;
                    for (n, piece) in tmpl.split('\u{1}').enumerate() {
                        if n % 2 == 0 {
                            body.push_str(piece);
                        } else if let Some(c) = map.get(piece).and_then(|u| content_of(u)) {
                            any = true;
                            body.push_str(&c);
                        }
                    }
                    if any {
                        written = true;
                        out.push_str(&code(u, PRE).unwrap_or_default());
                        out.push_str(&body);
                        out.push_str(&code(u, POST).unwrap_or_default());
                    } else {
                        out.push_str(&raw);
                    }
                    continue;
                }
                match content_of(u) {
                    Some(c) => {
                        written = true;
                        out.push_str(&code(u, PRE).unwrap_or_default());
                        out.push_str(&c);
                        out.push_str(&code(u, POST).unwrap_or_default());
                    }
                    None => out.push_str(&raw),
                }
            }
        }
    }
    if written {
        if let Some(tag) = doc.target_lang.as_deref().and_then(text::html_lang) {
            stamp(d, &mut out, &tag);
        }
    }
    enc::encode_text(&out, encoding)
}

/// Declares the target language where the file has none: `targetlocale` on `<txml>` (in the file's own
/// `en_US` style when its `locale` is written that way), `target-language` on each `<file>`.
fn stamp(d: Dialect, out: &mut String, tag: &str) {
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for st in xml::start_tags(out) {
        let local = st.qname.rsplit(':').next().unwrap_or("").to_ascii_lowercase();
        let tag_text = &out[st.start..st.end];
        match d {
            Dialect::Txml if local == "txml" => {
                if xml::tag_attr(tag_text, "targetlocale").unwrap_or_default().trim().is_empty() {
                    let value = if xml::tag_attr(tag_text, "locale").is_some_and(|l| l.contains('_')) { tag.replace('-', "_") } else { tag.to_string() };
                    edits.push((st.start, st.end, xml::with_attr(tag_text, "targetlocale", &value)));
                }
                break;
            }
            Dialect::Txlf if local == "file" => {
                if xml::tag_attr(tag_text, "target-language").unwrap_or_default().trim().is_empty() {
                    edits.push((st.start, st.end, xml::with_attr(tag_text, "target-language", tag)));
                }
            }
            _ => {}
        }
    }
    for (s, e, new) in edits.into_iter().rev() {
        out.replace_range(s..e, &new);
    }
}

macro_rules! filter_impl {
    ($ty:ident, $d:expr, $ext:literal, $sniff:expr) => {
        impl $ty {
            /// Like [`Filter::extract`] with explicit limits.
            pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
                extract($d, self.load_targets, bytes, opts, limits)
            }
            /// The filter that leaves out segments somebody already confirmed (what LocOS translates).
            pub fn without_targets() -> $ty {
                $ty { load_targets: false }
            }
        }

        impl Filter for $ty {
            fn id(&self) -> &'static str {
                $d.id()
            }
            fn extensions(&self) -> &'static [&'static str] {
                &[$ext]
            }
            fn sniff(&self, bytes: &[u8]) -> bool {
                let h = head_text(bytes);
                $sniff(h.as_str())
            }
            fn bilingual(&self) -> bool {
                true
            }
            fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
                self.extract_with(bytes, opts, &opts.limits())
            }
            fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
                merge($d, doc)
            }
        }
    };
}

filter_impl!(Txml, Dialect::Txml, "txml", |h: &str| h.contains("<txml"));
filter_impl!(Txlf, Dialect::Txlf, "txlf", |h: &str| h.contains("<xliff") && h.contains("gs4tr"));
