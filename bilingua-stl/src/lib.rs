//! Bilingua EBU STL filter (Tech 3264-E binary subtitles, `.stl`). See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
/// 2: the file is a `Skeleton::Binary` (schema 1 kept it as a one-entry zip skeleton, still read).
pub const SKELETON_SCHEMA: u32 = 2;

mod codec;
mod lang;
mod read;
mod tables;
mod write;

use bilingua_model::*;
use read::{Atom, Sub};
use std::collections::BTreeMap;

pub use lang::language_code;

/// The EBU STL filter.
pub struct Stl;

/// Name of the one skeleton member that holds the file as it came in.
const MEMBER: &str = "subtitles.stl";

fn label(b: u8) -> &'static str {
    match b {
        0x00 => "colour black",
        0x01 => "colour red",
        0x02 => "colour green",
        0x03 => "colour yellow",
        0x04 => "colour blue",
        0x05 => "colour magenta",
        0x06 => "colour cyan",
        0x07 => "colour white",
        0x08 => "flash",
        0x09 => "steady",
        0x0A => "end box",
        0x0B => "start box",
        0x0C => "normal height",
        0x0D => "double height",
        0x0E => "double width",
        0x0F => "double size",
        0x80 => "italics on",
        0x81 => "italics off",
        0x82 => "underline on",
        0x83 => "underline off",
        0x84 => "boxing on",
        0x85 => "boxing off",
        _ => "control code",
    }
}

/// Atoms to inlines: one standalone tag per control code, numbered from 1 in order.
fn inlines(atoms: &[Atom]) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    let mut out = Vec::new();
    let mut codes = BTreeMap::new();
    let mut text = String::new();
    let mut n = 0usize;
    for a in atoms {
        match a {
            Atom::C(c) => text.push(*c),
            Atom::T(b) => {
                if !text.is_empty() {
                    out.push(Inline::Text(std::mem::take(&mut text)));
                }
                n += 1;
                let id = n.to_string();
                let display = format!("<{b:02X}>");
                codes.insert(id.clone(), CodeData::new().with_standalone(display.clone()).with_label(label(*b).to_string()));
                out.push(Inline::tag(id, TagKind::Standalone, display));
            }
            Atom::U => {}
        }
    }
    if !text.is_empty() {
        out.push(Inline::Text(text));
    }
    (out, codes)
}

/// LocOS's text form (`⟦xN⟧` for a code), used to tell whether a unit changed.
fn text_of(inl: &[Inline]) -> String {
    let mut s = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, .. } => {
                s.push_str("\u{27E6}x");
                s.push_str(id);
                s.push('\u{27E7}');
            }
        }
    }
    s
}

/// The atoms of a translation: a tag takes the control code byte of the original code with its number; a tag the
/// original does not have is dropped (as LocOS drops an unknown token).
fn atoms_of(inl: &[Inline], tokens: &[u8]) -> Vec<Atom> {
    let mut out = Vec::new();
    for i in inl {
        match i {
            Inline::Text(t) => out.extend(t.chars().map(Atom::C)),
            Inline::Tag { id, .. } => match id.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|n| tokens.get(n)) {
                Some(b) => out.push(Atom::T(*b)),
                None => out.push(Atom::U),
            },
        }
    }
    out
}

fn blank(inl: &[Inline]) -> bool {
    !inl.iter().any(|i| match i {
        Inline::Tag { .. } => true,
        Inline::Text(t) => t.chars().any(|c| !py_isspace_char(c)),
    })
}

fn timecode(b: &[u8]) -> String {
    format!("{:02}:{:02}:{:02}:{:02}", b[0], b[1], b[2], b[3])
}

impl Filter for Stl {
    fn id(&self) -> &'static str {
        "stl"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["stl"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        bytes.len() >= 11 && matches!(&bytes[3..11], b"STL25.01" | b"STL30.01")
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let limits = opts.limits();
        guard::check_input(bytes, &limits)?;
        let p = read::parse(bytes, Some(&limits))?;
        let mnc = read::mnc(bytes);
        let mut units: Vec<Unit> = Vec::new();
        for g in p.groups.iter().filter(|g| !g.comment) {
            let sub = Sub::new(&g.tf, p.cct);
            let (atoms, _) = sub.atoms();
            let (inl, codes) = inlines(&atoms);
            if blank(&inl) {
                continue;
            }
            guard::check_unit_count(units.len() + 1, &limits)?;
            let n = units.len() + 1;
            let first = read::block(bytes, g.blocks[0]);
            let mut u = Unit::new(format!("c{n}"), inl);
            u.codes = codes;
            u.context.kind = Some("subtitle".into());
            u.context.key = Some(u16::from_le_bytes([g.sn[0], g.sn[1]]).to_string());
            u.context.location = Some(format!("block {}", g.blocks[0] + 1));
            u.context.extra.insert("start".into(), timecode(&first[5..9]));
            u.context.extra.insert("end".into(), timecode(&first[9..13]));
            u.context.extra.insert("max_cpl".into(), mnc.to_string());
            units.push(u);
        }
        let skeleton = Skeleton::binary(bytes.to_vec(), parser_info!());
        guard::finish(
            Document::new("stl", units, skeleton).with_options(opts),
            vec![],
            &limits,
        )
    }
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        merge(doc, opts)
    }
}

fn merge(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    check_parser(doc, &parser_info!())?;
    let raw = match &doc.skeleton {
        Skeleton::Binary { data, .. } => Some(data.as_slice()),
        // schema 1
        Skeleton::Zip { entries, .. } => entries.iter().find(|e| e.name == MEMBER).and_then(|e| match &e.data {
            ZipData::Raw(b) => Some(b.as_slice()),
            ZipData::Parts(_) => None,
        }),
        _ => None,
    }
    .ok_or_else(|| Error::Write("the skeleton does not hold an EBU STL file".into()))?;
    let parsed = read::parse(raw, None)?;
    let mut infos: Vec<write::UnitInfo> = Vec::new();
    let mut tokens: Vec<Vec<u8>> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    for (gi, g) in parsed.groups.iter().enumerate().filter(|(_, g)| !g.comment) {
        let sub = Sub::new(&g.tf, parsed.cct);
        let (atoms, toks) = sub.atoms();
        let (inl, _) = inlines(&atoms);
        if blank(&inl) {
            continue;
        }
        texts.push(text_of(&inl));
        tokens.push(toks);
        infos.push(write::UnitInfo { group: gi, sub, atoms });
    }
    let ids_match = doc.units.iter().enumerate().all(|(k, u)| u.id == format!("c{}", k + 1));
    if doc.units.len() != infos.len() || !ids_match {
        return Err(Error::Write("the document does not match its skeleton (units added, removed or reordered)".into()));
    }
    let mut changed = BTreeMap::new();
    for (k, u) in doc.units.iter().enumerate() {
        let out = u.output();
        if text_of(&out) != texts[k] {
            changed.insert(k, atoms_of(&out, &tokens[k]));
        }
    }
    let stamp = opts.should_stamp(doc);
    // an empty target language stamps nothing, as LocOS (`language_code(t) if t else None`)
    let lc = if stamp { doc.target_lang.as_deref().filter(|t| !t.is_empty()).map(language_code) } else { None };
    if changed.is_empty() && lc.is_none() {
        return Ok(raw.to_vec());
    }
    write::rebuild(raw, &parsed, &infos, &changed, lc, !opts.is_profile(bilingua_model::profile::LOCOS))
}
