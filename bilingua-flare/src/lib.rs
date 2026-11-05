//! Bilingua MadCap Flare project filter (`flprjzip`), matching the Localization OS `flare_filter`.
//!
//! A Flare project travels as a zip (Flare's own "zip project" writes a `.flprjzip`; a plain `.zip` of the project
//! folder renamed to `.flprjzip` works too). Units come in zip member order, and inside each member in this order:
//!
//! * topics and snippets (`.htm .html .xhtml .flsnp`), read through `bilingua-html` with the MadCap phrasing
//!   elements (`MadCap:xref`, `conditionalText`, `glossaryTerm`, ...) kept inside the sentence as inline codes and
//!   `MadCap:variable` (with its cached content), `snippetText`, `keyword` and `concept` as point codes (ids
//!   `m<member>-u<k>`): block text, then the accessibility attributes LocOS offers, then the index keywords of
//!   `MadCap:keyword term="..."`, one unit per keyword or sub-keyword level (`m<member>-k<n>`, style
//!   `flare:index-keyword`);
//! * variable sets (`.flvar`): the text of each text `Variable` (style `flare:variable`); a matching
//!   `EvaluatedDefinition` follows it;
//! * TOCs (`.fltoc`): the `Title` of each `TocEntry` (style `flare:toc-title`), `[%=System.LinkedTitle%]` tokens
//!   are standalone codes;
//! * glossaries (`.flglo`): `Term` and plain-text `Definition` (styles `flare:glossary-term` and
//!   `flare:glossary-definition`).
//!
//! Everything else (the `.flprj`, targets, condition tag sets, page layouts, style sheets, images, skins) is copied
//! as it is, and so is every attribute (`MadCap:conditions` included). The skeleton holds the whole original file;
//! merge opens it again and writes only what changed:
//!
//! * nothing translated: the original bytes, exactly;
//! * something translated: every member in its original order with its original compression and time stamp
//!   (members that did not change are copied without recompression), the archive comment kept;
//! * with a `target_lang` and at least one translation (or `MergeOptions::stamp_target_always`): `lang`,
//!   `xml:lang` (when present) and `dir` on every topic's `<html>` and the content-language `<meta>`.
//!
//! Flare saves UTF-8 and LocOS reads nothing else: a member that is not UTF-8 is a typed [`Error::Encoding`]. A
//! DTD or entity declaration, a topic or part that is not UTF-8, a structured part that is not well-formed XML,
//! an unsafe or repeated member name and a link are refused. Targets are plain text: `&`, `<` and `>` are
//! escaped and characters XML 1.0 forbids are dropped.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod lang;
mod mask;
mod parts;
mod scan;
mod text;
mod topic;

use bilingua_model::*;
use bilingua_zip::{write_zip_parts, Body, RawEntry, ZipLimits};
use parts::PartKind;
use std::collections::HashMap;
use topic::{translated, Topic};

/// The skeleton entry that holds the whole original file.
const ARCHIVE: &str = "flprjzip";
/// The skeleton entry that holds the options the file was read with: unit ids depend on `include_locked` and the
/// limits decide what may be read again, so merge uses the same ones.
const OPTIONS: &str = "flprjzip-options";

#[derive(serde::Serialize, serde::Deserialize)]
struct Opts {
    include_locked: bool,
    limits: Limits,
}

/// The Flare project filter.
pub struct Flare;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Topic,
    Part(PartKind),
}

fn refuse(msg: impl Into<String>) -> Error {
    Error::Parse { format: "flprjzip".into(), msg: msg.into() }
}

fn kind_of(name: &str) -> Option<Kind> {
    let low = name.replace('\\', "/").to_lowercase();
    if low.ends_with('/') || low.starts_with("__macosx/") || low.rsplit('/').next().unwrap_or("").starts_with("._") {
        return None;
    }
    if [".htm", ".html", ".xhtml", ".flsnp"].iter().any(|e| low.ends_with(e)) {
        return Some(Kind::Topic);
    }
    if low.ends_with(".flvar") {
        return Some(Kind::Part(PartKind::Variables));
    }
    if low.ends_with(".fltoc") {
        return Some(Kind::Part(PartKind::Toc));
    }
    if low.ends_with(".flglo") {
        return Some(Kind::Part(PartKind::Glossary));
    }
    None
}

struct Package {
    entries: Vec<RawEntry>,
    kinds: Vec<Option<Kind>>,
}

impl Package {
    fn open(bytes: &[u8], limits: &Limits) -> Result<Package, Error> {
        // read_zip refuses a name listed twice (exactly, by case, or `\` against `/`), however the directory is found
        let entries = bilingua_zip::read_zip("flprjzip", bytes, &ZipLimits::from(limits))?;
        let kinds: Vec<Option<Kind>> = entries.iter().map(|e| if e.meta.dir { None } else { kind_of(&e.name) }).collect();
        let flare = entries.iter().zip(&kinds).any(|(e, k)| {
            let low = e.name.to_lowercase();
            !e.meta.dir && (low.ends_with(".flprj") || low.ends_with(".flsnp") || matches!(k, Some(Kind::Part(_))))
        }) || entries.iter().zip(&kinds).any(|(e, k)| {
            *k == Some(Kind::Topic) && String::from_utf8_lossy(&e.data[..e.data.len().min(4096)]).to_lowercase().contains("xmlns:madcap")
        });
        if !flare {
            return Err(refuse(
                "This zip is not a MadCap Flare project: it has no .flprj project file, no Flare snippets, variables, TOCs or glossaries, and no topics with the MadCap namespace. It was not processed.",
            ));
        }
        Ok(Package { entries, kinds })
    }
}

fn part_units(idx: usize, path: &str, items: &[parts::Item]) -> Vec<Unit> {
    items
        .iter()
        .enumerate()
        .map(|(j, it)| {
            let mut u = Unit::new(format!("m{idx}-x{}", j + 1), it.inl.clone());
            u.context.kind = Some(
                match it.style {
                    parts::STYLE_VARIABLE => "variable",
                    parts::STYLE_TOC => "toc-entry",
                    parts::STYLE_TERM => "term",
                    _ => "definition",
                }
                .into(),
            );
            u.context.style = Some(it.style.into());
            u.context.key = it.key.clone();
            u.context.location = Some(format!("{path} item {}", j + 1));
            for (id, tok) in &it.tokens {
                u.codes.insert(id.clone(), CodeData { standalone: Some(tok.clone()), label: Some("variable".into()), open: None, close: None });
            }
            u
        })
        .collect()
}

fn extract_flare(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let limits = opts.limits();
    guard::check_input(bytes, &limits)?;
    let pkg = Package::open(bytes, &limits)?;
    let mut units: Vec<Unit> = Vec::new();
    let mut markup = 0usize;
    let mut first_lang: Option<String> = None;
    for (idx, e) in pkg.entries.iter().enumerate() {
        let Some(kind) = pkg.kinds[idx] else { continue };
        let is_topic = kind == Kind::Topic;
        let t = text::read(&e.data, &e.name, is_topic)?;
        markup += t.text.bytes().filter(|b| *b == b'<').count();
        if markup > 10usize.saturating_mul(limits.max_units) {
            return Err(LimitError::new(
                LimitKind::XmlElements,
                format!("This Flare project has far more markup than {} segments allow. Split it into smaller ones.", limits.max_units),
            )
            .into());
        }
        match kind {
            Kind::Topic => {
                let tp = Topic::from_text(t, &e.name, opts)?;
                if first_lang.is_none() {
                    first_lang = tp.source_lang();
                }
                units.extend(tp.units(idx));
            }
            Kind::Part(pk) => {
                parts::check(&t.text, &e.name, &limits)?;
                units.extend(part_units(idx, &e.name, &parts::items(pk, &t.text)));
            }
        }
        guard::check_unit_count(units.len(), &limits)?;
    }
    let skeleton = Skeleton::zip(
            vec![
                ZipEntry { name: ARCHIVE.into(), data: ZipData::Raw(bytes.to_vec()) },
                ZipEntry {
                    name: OPTIONS.into(),
                    data: ZipData::Raw(
                        serde_json::to_vec(&Opts { include_locked: opts.include_locked, limits }).map_err(|e| Error::Write(e.to_string()))?,
                    ),
                },
            ],
        bilingua_model::parser_info!(),
    );
    let doc = Document::new("flprjzip", units, skeleton)
        .with_options(opts)
        .with_source_lang(opts.source_lang.clone().or(first_lang))
        .with_granularity(Granularity::Paragraph);
    guard::finish(doc, vec![], &limits)
}

fn merge_flare(doc: &Document, mopts: &MergeOptions) -> Result<Vec<u8>, Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Zip { entries: skel, .. } = &doc.skeleton else {
        return Err(Error::Write("flprjzip needs a zip skeleton".into()));
    };
    let original = skel
        .iter()
        .find_map(|e| match (&e.data, e.name == ARCHIVE) {
            (ZipData::Raw(b), true) => Some(b.as_slice()),
            _ => None,
        })
        .ok_or_else(|| Error::Write("flprjzip skeleton has no archive".into()))?;
    let any = doc.units.iter().any(translated);
    let lang = mopts.stamp_lang(doc, any).and_then(|l| topic::target(Some(l)));
    // Nothing translated and nothing to stamp: the original bytes, without opening the project again.
    if !any && lang.is_none() {
        return Ok(original.to_vec());
    }
    let read_with = skel
        .iter()
        .find_map(|e| match (&e.data, e.name == OPTIONS) {
            (ZipData::Raw(b), true) => Some(serde_json::from_slice::<Opts>(b)),
            _ => None,
        })
        .transpose()
        .map_err(|e| Error::Write(format!("the flprjzip skeleton's options are unreadable: {e}")))?
        .unwrap_or(Opts { include_locked: false, limits: Limits::default() });
    // a stored limit set from an older build may lack new fields: serde fills them from `Limits::default()`
    let limits = read_with.limits;
    let pkg = Package::open(original, &limits)?;
    let by_id: HashMap<&str, &Unit> = doc.units.iter().map(|u| (u.id.as_str(), u)).collect();
    let lang_ref = lang.as_ref().map(|(t, r)| (t.as_str(), *r));
    let copts = ExtractOptions::new().with_include_locked(read_with.include_locked).with_limits(limits);
    let mut replaced: HashMap<usize, Vec<u8>> = HashMap::new();
    for (idx, e) in pkg.entries.iter().enumerate() {
        let Some(kind) = pkg.kinds[idx] else { continue };
        match kind {
            Kind::Topic => {
                let mut tp = Topic::open(&e.data, &e.name, &copts)?;
                if let Some(out) = tp.render(idx, &by_id, lang_ref)? {
                    if out != e.data {
                        replaced.insert(idx, out);
                    }
                }
            }
            Kind::Part(pk) => {
                let t = text::read(&e.data, &e.name, false)?;
                let items = parts::items(pk, &t.text);
                let mut targets = Vec::new();
                for j in 0..items.len() {
                    if let Some(u) = by_id.get(format!("m{idx}-x{}", j + 1).as_str()) {
                        if translated(u) {
                            targets.push((j, u.output()));
                        }
                    }
                }
                if targets.is_empty() {
                    continue;
                }
                let new = parts::render(pk, &t.text, &items, &targets);
                if new != t.text {
                    replaced.insert(idx, t.encode(&new));
                }
            }
        }
    }
    if replaced.is_empty() {
        return Ok(original.to_vec());
    }
    write_project(&pkg.entries, replaced, original)
}

/// Every member in its original order, compression and time stamp, content replaced only where `replaced` says so.
fn write_project(entries: &[RawEntry], mut replaced: HashMap<usize, Vec<u8>>, original: &[u8]) -> Result<Vec<u8>, Error> {
    let mut parts: Vec<(bilingua_zip::EntryMeta, Body<'_>)> = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        let body = match (replaced.remove(&i), &e.packed) {
            (Some(b), _) => Body::Owned(b),
            (None, Some((p, bytes))) => Body::Packed(p, bytes),
            (None, None) => Body::Plain(&e.data),
        };
        parts.push((e.meta.clone(), body));
    }
    let out = write_zip_parts(&parts)?;
    Ok(keep_comment(out, original))
}

/// The archive comment of `z` (the end-of-central-directory record is at most 22 + 65535 bytes from the end).
fn comment_of(z: &[u8]) -> Option<&[u8]> {
    let from = z.len().saturating_sub(22 + 65535);
    let mut i = z.len().checked_sub(22)?;
    loop {
        if z[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06]) {
            let n = u16::from_le_bytes([z[i + 20], z[i + 21]]) as usize;
            return z.get(i + 22..i + 22 + n);
        }
        if i <= from {
            return None;
        }
        i -= 1;
    }
}

/// The zip writer does not write an archive comment; the original's is appended to the new end record.
fn keep_comment(mut out: Vec<u8>, original: &[u8]) -> Vec<u8> {
    let Some(c) = comment_of(original).filter(|c| !c.is_empty()) else { return out };
    let n = out.len();
    if n >= 22 && out[n - 22..].starts_with(&[0x50, 0x4b, 0x05, 0x06]) && out[n - 2] == 0 && out[n - 1] == 0 {
        let len = (c.len() as u16).to_le_bytes();
        out[n - 2] = len[0];
        out[n - 1] = len[1];
        out.extend_from_slice(c);
    }
    out
}

/// True when the central directory names a Flare project file (`.flprj`, `.flvar`, `.fltoc`, `.flglo`, `.flsnp`).
fn names_a_flare_file(z: &[u8]) -> bool {
    let from = z.len().saturating_sub(22 + 65535);
    let Some(mut i) = z.len().checked_sub(22) else { return false };
    while !z[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06]) {
        if i <= from {
            return false;
        }
        i -= 1;
    }
    let count = u16::from_le_bytes([z[i + 10], z[i + 11]]) as usize;
    let mut at = u32::from_le_bytes([z[i + 16], z[i + 17], z[i + 18], z[i + 19]]) as usize;
    for _ in 0..count {
        let Some(names_at) = at.checked_add(46) else { return false };
        let Some(h) = z.get(at..names_at) else { return false };
        if !h.starts_with(&[0x50, 0x4b, 0x01, 0x02]) {
            return false;
        }
        let n = u16::from_le_bytes([h[28], h[29]]) as usize;
        let skip = n + u16::from_le_bytes([h[30], h[31]]) as usize + u16::from_le_bytes([h[32], h[33]]) as usize;
        let Some(name) = names_at.checked_add(n).and_then(|e| z.get(names_at..e)) else { return false };
        let low = String::from_utf8_lossy(name).to_lowercase();
        if [".flprj", ".flvar", ".fltoc", ".flglo", ".flsnp"].iter().any(|e| low.ends_with(e)) {
            return true;
        }
        let Some(next) = names_at.checked_add(skip) else { return false };
        at = next;
    }
    false
}

impl Filter for Flare {
    fn id(&self) -> &'static str {
        "flprjzip"
    }

    /// Only `.flprjzip`: a plain `.zip` is not registered, so every other zip keeps its "unsupported" answer.
    fn extensions(&self) -> &'static [&'static str] {
        &["flprjzip"]
    }

    /// A zip whose directory names a Flare project file (`.flprj`, `.flvar`, `.fltoc`, `.flglo`, `.flsnp`). Strict:
    /// it never takes an Office, EPUB or other package, so its place in the list does not matter.
    fn sniff(&self, bytes: &[u8]) -> bool {
        bytes.starts_with(b"PK\x03\x04") && names_a_flare_file(bytes)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        extract_flare(bytes, opts)
    }

    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        merge_flare(doc, opts)
    }
}
