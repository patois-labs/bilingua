//! Bilingua image filters: SVG text and the text layer of raster images. See the README.
//!
//! * [`SvgFilter`] (`svg`): the text of `<text>`, `<tspan>` and `<textPath>`, plus the `<title>` and `<desc>`
//!   accessibility text (appended after the text units, style `a11y:svg-title` / `a11y:svg-desc`). Everything else
//!   is kept byte for byte. A merge for a target language stamps `xml:lang` on the root and, for a right-to-left
//!   target, sets `direction="rtl"` with a mirrored `text-anchor` on every translated `<text>`.
//! * [`ImageFilter`] (`image`): what Localization OS stores for a raster image. LocOS reads the regions of a PNG
//!   from a JSON sidecar in a text chunk (`loc_tms_regions`), one unit per region in reading order. OCR and vision
//!   stay in LocOS: an image with no sidecar (and every JPEG, GIF, TIFF, BMP or WebP) is a document with no units.
//!   A merge writes the translated texts back into the sidecar; pixels are never touched (LocOS draws the
//!   translation onto the pixels, which needs fonts and stays there). Given SVG bytes it behaves as [`SvgFilter`],
//!   as the LocOS `image` filter does.

/// Skeleton schema number: bump only when this crate's skeleton layout changes.
pub const SKELETON_SCHEMA: u32 = 1;

mod attr;
mod json;
mod lang;
mod png;
mod probe;
mod region;
mod tok;
mod xmlscan;

use bilingua_model::enc::{self, DecodeOptions};
use bilingua_model::guard::{self, LimitError, LimitKind};
use bilingua_model::*;
use probe::Kind;
use std::borrow::Cow;
use std::collections::HashSet;

pub(crate) fn refuse(msg: &str) -> Error {
    Error::Parse { format: "image".into(), msg: msg.to_string() }
}

pub(crate) fn refused_frames(msg: &str) -> Error {
    Error::Unsupported(msg.to_string())
}

/// Unit-private values: the text token as written, and (for a file whose encoding does not round trip) the bytes.
const SRC: &str = "svg.src";
const RAW: &str = "svg.raw";

/// The SVG filter (id `svg`, extension `svg`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SvgFilter;

/// The raster image filter (id `image`): the PNG region sidecar, and SVG bytes as [`SvgFilter`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageFilter;

// ---------------------------------------------------------------------------------------------------------------
// SVG
// ---------------------------------------------------------------------------------------------------------------

struct Found {
    ranges: Vec<(usize, usize)>,
    nodes: Vec<tok::Node>,
    left_alone: usize,
}

fn has_entity_declaration(text: &str) -> bool {
    text.as_bytes().windows(8).any(|w| w.eq_ignore_ascii_case(b"<!ENTITY"))
}

/// True when the text holds `&name;` for any name but the five predefined XML entities.
fn uses_custom_entity(s: &str) -> bool {
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        rest = &rest[p + 1..];
        let Some(semi) = rest.find(';') else { return false };
        let name = &rest[..semi];
        if !name.is_empty()
            && !name.starts_with('#')
            && !matches!(name, "amp" | "lt" | "gt" | "quot" | "apos")
            && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
        {
            return true;
        }
    }
    false
}

/// The units of an SVG text, as LocOS finds them, minus the text nodes that lie inside a comment, CDATA section,
/// processing instruction, DOCTYPE or attribute value (see `xmlscan`).
fn find_nodes(text: &str, limits: Option<&Limits>) -> Result<Found, Error> {
    let ranges = tok::split_tags(text);
    let toks: Vec<Cow<str>> = ranges.iter().map(|&(s, e)| Cow::Borrowed(&text[s..e])).collect();
    let mut nodes = match limits {
        Some(l) => {
            let mut cap = |n: usize| guard::check_unit_count(n, l);
            let mut v = tok::text_nodes(&toks, Some(&mut cap))?;
            let a = tok::a11y_nodes(&toks, v.len(), Some(&mut cap))?;
            v.extend(a);
            v
        }
        None => {
            let mut v = tok::text_nodes(&toks, None)?;
            let a = tok::a11y_nodes(&toks, v.len(), None)?;
            v.extend(a);
            v
        }
    };
    let regions = xmlscan::Regions::scan(text);
    let before = nodes.len();
    nodes.retain(|n| {
        let (s, e) = ranges[n.ti];
        !regions.overlaps(s, e)
    });
    // A DOCTYPE with entity declarations: a text node that uses one of them (`&name;`) is not edited, because a
    // translation written back would turn the reference into the letters `&amp;name;`.
    if has_entity_declaration(text) {
        nodes.retain(|n| {
            let (s, e) = ranges[n.ti];
            !uses_custom_entity(&text[s..e])
        });
    }
    let left_alone = before - nodes.len();
    Ok(Found { ranges, nodes, left_alone })
}

fn too_many_tags(limits: &Limits) -> Error {
    LimitError::new(
        LimitKind::Units,
        format!("This SVG file has far more tags than the limit of {} segments allows. Split it into smaller files.", limits.max_units),
    )
    .into()
}

fn svg_extract(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    svg_read(bytes, opts, true)
}

/// `keep_whole`: a file with no units whose code page does not give its bytes back keeps them in a binary skeleton
/// (there is no unit to hold them), so an untouched merge is still the original.
fn svg_read(bytes: &[u8], opts: &ExtractOptions, keep_whole: bool) -> Result<Document, Error> {
    let limits = opts.limits();
    guard::check_input(bytes, &limits)?;
    guard::check_xml_internal_entities(bytes, &limits)?;
    let dec = enc::decode_text(bytes, &DecodeOptions::new(opts).declared())?;
    let text = &dec.text;
    // twenty markup characters per allowed segment, as LocOS does before it splits a tag at a time
    if text.bytes().filter(|&b| b == b'<').count() > 20usize.saturating_mul(limits.max_units) {
        return Err(too_many_tags(&limits));
    }
    let found = find_nodes(text, Some(&limits))?;
    let mut warnings = dec.warnings.clone();
    if found.left_alone > 0 {
        warnings.push(Warning::new(
            "svg-markup-text",
            "",
            format!(
                "{} piece(s) of text inside a comment, CDATA section, DOCTYPE or attribute value (a `>` there ends a tag early in a simple reader) were left alone.",
                found.left_alone
            ),
        ));
    }
    // legacy code pages whose decode is not one to one cannot give the same bytes back from the text: keep them
    let keep_raw = !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom") && enc::encode_text(text, &dec.encoding).ok().as_deref() != Some(bytes);
    let mut units: Vec<Unit> = Vec::with_capacity(found.nodes.len());
    let mut order: Vec<(usize, usize)> = Vec::with_capacity(found.nodes.len());
    for (k, n) in found.nodes.iter().enumerate() {
        let mut u = Unit::new(format!("u{}", k + 1), vec![Inline::text(n.core.clone())]);
        let (s, e) = found.ranges[n.ti];
        u.set_hidden(SRC, &text[s..e]);
        if keep_raw && k == 0 {
            u.set_hidden(RAW, b64::encode(bytes));
        }
        u.context.style = n.style.map(str::to_string);
        u.context.kind = Some(match n.style {
            None => "text",
            Some("a11y:svg-title") => "title",
            Some(_) => "desc",
        }
        .into());
        order.push((n.ti, k));
        units.push(u);
    }
    order.sort_unstable();
    let mut parts: Vec<SkelPart> = Vec::with_capacity(order.len() * 2 + 1);
    let mut pos = 0usize;
    for (ti, k) in order {
        let (s, e) = found.ranges[ti];
        parts.push(SkelPart::Raw(text[pos..s].to_string()));
        parts.push(SkelPart::Unit(k));
        pos = e;
    }
    parts.push(SkelPart::Raw(text[pos..].to_string()));
    let skeleton = if keep_raw && keep_whole && units.is_empty() {
        Skeleton::binary(bytes.to_vec(), parser_info!())
    } else {
        Skeleton::text(parts, dec.encoding, parser_info!())
    };
    guard::finish(Document::new("svg", units, skeleton).with_options(opts), warnings, &limits)
}

/// The text a unit now holds, when it differs from `old` and is translated (a translation to nothing counts).
fn changed(unit: &Unit, old: &str) -> Option<String> {
    if !unit.is_translated() {
        return None;
    }
    let new = plain_text(&unit.output());
    (new != old).then_some(new)
}

fn svg_merge(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    check_parser(doc, &parser_info!())?;
    if let (Skeleton::Binary { data, .. }, "svg") = (&doc.skeleton, doc.format.as_str()) {
        // a file kept whole (no units, a code page that does not give its bytes back)
        if !doc.units.is_empty() {
            return Err(Error::Write("the document does not match its skeleton (different number of units)".into()));
        }
        if opts.stamp_lang(doc, false).and_then(lang::canonical_tag).is_none() && opts.output_encoding != OutputEncoding::Utf8 {
            return Ok(data.to_vec());
        }
        let mut inner = svg_read(data, &ExtractOptions::new(), false)?;
        inner.target_lang = doc.target_lang.clone();
        return svg_merge(&inner, opts);
    }
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write("svg needs the skeleton made by its own extract".into()));
    };
    let mut text = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => text.push_str(s),
            SkelPart::Unit(i) => {
                let u = doc.units.get(*i).ok_or_else(|| Error::Write("the skeleton names a unit the document does not have".into()))?;
                let orig = u.hidden(SRC).ok_or_else(|| Error::Write("a unit lost its original text".into()))?;
                text.push_str(orig);
            }
        }
    }
    let found = find_nodes(&text, None)?;
    if found.nodes.len() != doc.units.len() {
        return Err(Error::Write("the document does not match its skeleton (different number of units)".into()));
    }
    let mut toks: Vec<Cow<str>> = found.ranges.iter().map(|&(s, e)| Cow::Borrowed(&text[s..e])).collect();
    let mut translated: HashSet<usize> = HashSet::new();
    let mut any = false;
    for (n, u) in found.nodes.iter().zip(&doc.units) {
        if let Some(new) = changed(u, &n.core) {
            let orig = &text[found.ranges[n.ti].0..found.ranges[n.ti].1];
            let (lead, trail) = tok::lead_trail(orig);
            toks[n.ti] = Cow::Owned(format!("{lead}{}{trail}", tok::xml_escape(&new)));
            any = true;
            if n.style.is_none() {
                translated.insert(n.ti);
            }
        }
    }
    let tag = opts.stamp_lang(doc, MergeOptions::any_translated(doc)).and_then(lang::canonical_tag);
    let stamped = tag.is_some();
    if !any && !stamped {
        // nothing to write: the original bytes, whatever its encoding (unless UTF-8 out was asked for)
        if opts.output_encoding == OutputEncoding::Utf8 {
            return enc::encode_text_with(&text, encoding, opts);
        }
        let raw = doc.units.first().and_then(|u| u.hidden(RAW)).and_then(|s| b64::decode(s).ok());
        return match raw {
            Some(r) => Ok(r),
            None => enc::encode_text(&text, encoding),
        };
    }
    if let (true, Some(tag)) = (stamped, tag.as_deref()) {
        attr::declare_language(&mut toks, tag);
        if lang::is_rtl(tag) && !translated.is_empty() {
            attr::rtl(&mut toks, &translated);
        }
    }
    let out: String = toks.concat();
    enc::encode_text_with(&out, encoding, opts)
}

// ---------------------------------------------------------------------------------------------------------------
// Raster
// ---------------------------------------------------------------------------------------------------------------

struct Sidecar {
    png: png::Png,
    text: png::Text,
    regions: Vec<region::Region>,
}

fn sidecar(bytes: &[u8]) -> Result<Option<Sidecar>, Error> {
    let p = png::walk(bytes, true)?;
    let texts = png::texts(bytes, &p)?;
    let Some(t) = png::last(&texts, region::KEY).filter(|t| !t.value.is_empty()).cloned() else { return Ok(None) };
    let regions = region::regions(&t.value)?;
    Ok(Some(Sidecar { png: p, text: t, regions }))
}

fn raster_extract(bytes: &[u8], kind: Kind, opts: &ExtractOptions) -> Result<Document, Error> {
    let limits = opts.limits();
    guard::check_input(bytes, &limits)?;
    let mut units = Vec::new();
    let mut warnings = Vec::new();
    if kind == Kind::Png {
        if let Some(sc) = sidecar(bytes)? {
            for (i, r) in sc.regions.iter().enumerate() {
                let mut u = Unit::new(format!("u{}", i + 1), vec![Inline::text(r.text.clone())]);
                u.context.kind = Some("region".into());
                u.context.location = Some(format!("{},{},{},{}", r.bbox[0], r.bbox[1], r.bbox[2], r.bbox[3]));
                u.context.extra.insert("image_region".into(), region::contract_json(i, r));
                units.push(u);
            }
        }
    } else {
        probe::check_raster(bytes, kind)?;
    }
    if units.is_empty() {
        warnings.push(Warning::new(
            "no-text-layer",
            "",
            "This image carries no text layer (no region sidecar). Reading the text of the pixels is OCR, which stays in Localization OS.",
        ));
    }
    guard::finish(Document::new("image", units, Skeleton::binary(bytes.to_vec(), parser_info!())).with_options(opts), warnings, &limits)
}

fn raster_merge(doc: &Document) -> Result<Vec<u8>, Error> {
    check_parser(doc, &parser_info!())?;
    let Skeleton::Binary { data: original, .. } = &doc.skeleton else {
        return Err(Error::Write("image needs the skeleton made by its own extract".into()));
    };
    let original: &[u8] = original;
    let edits: Vec<(usize, String)> = doc
        .units
        .iter()
        .enumerate()
        .filter_map(|(i, u)| changed(u, &plain_text(&u.source())).map(|n| (i, n)))
        .collect();
    if edits.is_empty() {
        return Ok(original.to_vec());
    }
    let sc = sidecar(original)?.ok_or_else(|| Error::Write("the image has no text layer to write to".into()))?;
    if sc.regions.len() != doc.units.len() {
        return Err(Error::Write("the document does not match its skeleton (different number of regions)".into()));
    }
    let mut spans: Vec<((usize, usize), String)> = edits.into_iter().map(|(i, n)| (sc.regions[i].text_span, json::ascii_literal(&n))).collect();
    spans.sort_by_key(|s| s.0 .0);
    let mut value = String::with_capacity(sc.text.value.len() + 64);
    let mut last = 0usize;
    for ((s, e), lit) in spans {
        value.push_str(&sc.text.value[last..s]);
        value.push_str(&lit);
        last = e;
    }
    value.push_str(&sc.text.value[last..]);
    png::replace_text(original, &sc.png, &sc.text, &value)
}

fn raster_magic(bytes: &[u8]) -> bool {
    match probe::sniff(bytes) {
        Some(Kind::Bmp) => matches!(bytes.get(14..18), Some([12 | 40 | 52 | 56 | 64 | 108 | 124, 0, 0, 0])),
        // every JPEG marker segment starts with 0xFF: SOI alone (two bytes) is too loose for format detection
        Some(Kind::Jpeg) => bytes.get(2) == Some(&0xFF),
        Some(Kind::Svg) | None => false,
        Some(_) => true,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Filters
// ---------------------------------------------------------------------------------------------------------------

impl Filter for SvgFilter {
    fn id(&self) -> &'static str {
        "svg"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["svg"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        probe::is_svg_strict(bytes)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        guard::check_input(bytes, &opts.limits())?;
        if !probe::looks_like_svg(bytes) {
            return Err(Error::Parse { format: "svg".into(), msg: "the content is not SVG".into() });
        }
        svg_extract(bytes, opts)
    }

    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        svg_merge(doc, opts)
    }
}

impl Filter for ImageFilter {
    fn id(&self) -> &'static str {
        "image"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["png", "jpg", "jpeg", "tif", "tiff", "bmp", "gif", "webp"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        raster_magic(bytes)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        guard::check_input(bytes, &opts.limits())?;
        match probe::sniff(bytes) {
            None if probe::looks_like_svg(bytes) => svg_extract(bytes, opts),
            Some(Kind::Svg) => svg_extract(bytes, opts),
            Some(k) => raster_extract(bytes, k, opts),
            None => Err(refuse("not a decodable image: the content is not PNG, JPEG, TIFF, BMP, GIF or WebP")),
        }
    }

    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        match &doc.skeleton {
            Skeleton::Text { .. } => svg_merge(doc, opts),
            Skeleton::Binary { .. } if doc.format == "svg" => svg_merge(doc, opts),
            Skeleton::Binary { .. } => raster_merge(doc),
            _ => Err(Error::Write("image needs the skeleton made by its own extract".into())),
        }
    }
}
