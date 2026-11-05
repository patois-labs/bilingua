//! Bilingua AutoCAD DXF filter (ASCII DXF). See the README.
//!
//! The drawing text of the ENTITIES and BLOCKS sections becomes units: TEXT, MTEXT (its 250-character chunks
//! joined into one unit), ATTRIB, ATTDEF (value and prompt), DIMENSION text overrides and MULTILEADER content.
//! Everything else stays in the skeleton byte for byte. The skeleton is bytes: each raw part holds one character
//! per file byte (U+0000..U+00FF), so a drawing in a legacy code page or with stray bytes outside its text merges
//! back exactly; the code page travels in the skeleton `encoding` as `dxf-bytes;codec=<Python codec name>`.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod codec;
mod lex;
mod printable;
mod sbcs;
mod walk;

use bilingua_model::*;
use codec::Codec;
use lex::Kind;

/// The DXF filter.
pub struct Dxf;

const ID: &str = "dxf";
/// The hidden code of a unit: `label` = `text`, `mtext` or `dim`; `standalone` = the original bytes of the first
/// span (the value for a single value, the whole line block for MTEXT), as one character per byte.
const HIDDEN: &str = "$dxf";
/// MTEXT only: `open` = the first code line up to its digits, `close` = the line break after the code,
/// `standalone` = the line break after the first value (empty at the end of the file).
const META: &str = "$dxf.m";
const TAG_PREFIX: &str = "dxf-bytes;codec=";
const RESAVE: &str = " Save the drawing as an ASCII DXF (AutoCAD 2007 or later is UTF-8).";

fn to_chars(b: &[u8]) -> String {
    if b.is_ascii() {
        String::from_utf8_lossy(b).into_owned()
    } else {
        b.iter().map(|&x| x as char).collect()
    }
}

fn push_bytes(s: &str, out: &mut Vec<u8>) -> Result<(), Error> {
    if s.is_ascii() {
        out.extend_from_slice(s.as_bytes());
        return Ok(());
    }
    for c in s.chars() {
        let u = c as u32;
        if u > 0xFF {
            return Err(Error::Write("this is not a dxf skeleton (a raw part holds a character above U+00FF)".into()));
        }
        out.push(u as u8);
    }
    Ok(())
}

fn refuse(msg: String) -> Error {
    Error::Encoding(format!("{msg}{RESAVE}"))
}

fn pick_codec(data: &[u8], w: &walk::Walked) -> Result<Codec, Error> {
    let ver = w
        .acadver
        .as_deref()
        .filter(|v| v.len() == 6 && v.starts_with("AC") && v[2..].bytes().all(|b| b.is_ascii_digit()))
        .and_then(|v| v[2..].parse::<u32>().ok());
    if ver.is_some_and(|v| v >= 1021) {
        return Ok(Codec::Utf8);
    }
    if let Some(cp) = w.codepage.as_deref().filter(|c| !c.is_empty()) {
        return Codec::from_dwg(cp)
            .ok_or_else(|| refuse(format!("This DXF file declares code page {cp} ($DWGCODEPAGE), which is not supported.")));
    }
    if ver.is_none() && !data.is_ascii() && std::str::from_utf8(data).is_ok() {
        return Ok(Codec::Utf8);
    }
    Ok(Codec::from_name("cp1252").unwrap_or(Codec::Utf8))
}

fn parse_tag(tag: &str) -> Result<Codec, Error> {
    tag.strip_prefix(TAG_PREFIX)
        .and_then(Codec::from_name)
        .ok_or_else(|| Error::Write(format!("unknown encoding {tag}")))
}

/// The hidden code's label: how the value is lexed and written. `mtext` is the chunked MTEXT form (groups 1 and 3);
/// `mleader` lexes as MTEXT but is one group-304 value, written in place like the single-value kinds.
fn kind_label(style: &str, k: Kind) -> &'static str {
    match (style, k) {
        ("dxf:mtext", _) => "mtext",
        ("dxf:mleader", _) => "mleader",
        (_, Kind::Dim) => "dim",
        _ => "text",
    }
}

fn kind_of(label: &str) -> Kind {
    match label {
        "mtext" | "mleader" => Kind::Mtext,
        "dim" => Kind::Dim,
        _ => Kind::Text,
    }
}

struct Slot {
    start: usize,
    end: usize,
    unit: usize,
}

fn decode_value(raw: &[u8], codec: Codec, at: usize) -> Result<String, Error> {
    codec.decode(raw).map_err(|p| {
        refuse(format!(
            "Byte 0x{:02x} at position {} is not valid in this drawing's encoding ({}).",
            raw[p],
            at + p,
            codec.name()
        ))
    })
}

impl Filter for Dxf {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["dxf"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(4096)];
        let head = head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head);
        let s = head.iter().position(|&c| !walk::is_py_ws(c)).unwrap_or(head.len());
        let t = &head[s..];
        if t.starts_with(b"AutoCAD Binary DXF") {
            return true;
        }
        let mut pos = 0;
        for _ in 0..64 {
            let Some(p) = walk::match_pair(t, pos) else { return false };
            pos = p.end;
            match p.code {
                999 => continue,
                0 if walk::py_strip(&t[p.val_s..p.val_e]) == b"SECTION" => {
                    return walk::match_pair(t, pos).is_some_and(|q| q.code == 2);
                }
                _ => return false,
            }
        }
        false
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        guard::check_input(bytes, &limits)?;
        let w = walk::walk(bytes, &limits)?;
        let codec = pick_codec(bytes, &w)?;
        let mut units: Vec<Unit> = Vec::new();
        let mut slots: Vec<Slot> = Vec::new();
        for ru in &w.units {
            let mut ordered: Vec<&walk::Pair> = ru.pairs.iter().collect();
            if ru.kind == Kind::Mtext && ordered.len() > 1 {
                ordered = ru.pairs.iter().filter(|p| p.code == 3).chain(ru.pairs.iter().filter(|p| p.code != 3)).collect();
            }
            let mut raw: Vec<u8> = Vec::new();
            for p in &ordered {
                raw.extend_from_slice(&bytes[p.val_s..p.val_e]);
            }
            let value = decode_value(&raw, codec, ordered[0].val_s)?;
            if py_strip(&value).is_empty() {
                continue;
            }
            let parsed = lex::parse_value(&value, ru.kind);
            if !parsed.alpha {
                continue;
            }
            let idx = units.len();
            let mut unit = Unit::new(format!("u{}", idx + 1), parsed.inlines);
            unit.codes = parsed.codes;
            let first = ru.pairs[0];
            let label = Some(kind_label(ru.style, ru.kind).to_string());
            if ru.style == "dxf:mtext" {
                // spans: whole line blocks, neighbours joined
                let mut spans: Vec<(usize, usize)> = Vec::new();
                for p in &ru.pairs {
                    match spans.last_mut() {
                        Some(l) if l.1 == p.start => l.1 = p.end,
                        _ => spans.push((p.start, p.end)),
                    }
                }
                for (k, (a, b)) in spans.iter().enumerate() {
                    let key = if k == 0 { HIDDEN.to_string() } else { format!("{HIDDEN}.{k}") };
                    unit.codes.insert(key, CodeData { standalone: Some(to_chars(&bytes[*a..*b])), label: label.clone(), ..Default::default() });
                    slots.push(Slot { start: *a, end: *b, unit: idx });
                }
                let nl1 = &bytes[first.code_e..first.val_s];
                let ws = nl1.iter().position(|&c| c != b' ' && c != b'\t').unwrap_or(nl1.len());
                unit.codes.insert(
                    META.into(),
                    CodeData {
                        open: Some(to_chars(&bytes[first.start..first.code_e])),
                        close: Some(to_chars(&nl1[ws..])),
                        standalone: Some(to_chars(&bytes[first.val_e..first.end])),
                        ..Default::default()
                    },
                );
            } else {
                unit.codes.insert(
                    HIDDEN.into(),
                    CodeData { standalone: Some(to_chars(&bytes[first.val_s..first.val_e])), label, ..Default::default() },
                );
                slots.push(Slot { start: first.val_s, end: first.val_e, unit: idx });
            }
            unit.context.kind = Some(ru.style.trim_start_matches("dxf:").to_string());
            unit.context.style = Some(ru.style.to_string());
            unit.context.location = Some(format!("line {}", 2 * ru.group + 1));
            units.push(unit);
        }
        slots.sort_by_key(|s| s.start);
        let mut parts: Vec<SkelPart> = Vec::with_capacity(slots.len() * 2 + 1);
        let mut pos = 0usize;
        for s in &slots {
            if s.start > pos {
                parts.push(SkelPart::Raw(to_chars(&bytes[pos..s.start])));
            }
            parts.push(SkelPart::Unit(s.unit));
            pos = s.end;
        }
        if pos < bytes.len() {
            parts.push(SkelPart::Raw(to_chars(&bytes[pos..])));
        }
        let doc = Document {
            format: ID.into(),
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(parts, format!("{TAG_PREFIX}{}", codec.name()), bilingua_model::parser_info!()),
            ..Default::default()
        };
        guard::finish(doc, Vec::new(), &limits)
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        self.merge_with(doc, &MergeOptions::default())
    }

    /// DXF has no language stamp and keeps its code page, so the merge options change nothing (LocOS keeps the
    /// code page too, so the adapter's UTF-8 option must not be passed for dxf either).
    fn merge_with(&self, doc: &Document, _opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("dxf needs a text skeleton".into()));
        };
        let codec = parse_tag(encoding)?;
        let mut out: Vec<u8> = Vec::new();
        let mut seen = vec![0usize; doc.units.len()];
        let mut changed = vec![false; doc.units.len()];
        for p in parts {
            match p {
                SkelPart::Raw(s) => push_bytes(s, &mut out)?,
                SkelPart::Unit(i) => {
                    let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    let occ = seen[*i];
                    seen[*i] += 1;
                    write_unit(unit, occ, &mut changed[*i], codec, &mut out)?;
                }
            }
        }
        Ok(out)
    }
}

fn write_unit(unit: &Unit, occ: usize, changed: &mut bool, codec: Codec, out: &mut Vec<u8>) -> Result<(), Error> {
    let key = if occ == 0 { HIDDEN.to_string() } else { format!("{HIDDEN}.{occ}") };
    let h = unit.codes.get(&key).ok_or_else(|| Error::Write(format!("unit {} has no dxf record", unit.id)))?;
    let original = h.standalone.as_deref().unwrap_or("");
    let label = unit.codes.get(HIDDEN).and_then(|c| c.label.as_deref()).unwrap_or("text");
    let kind = kind_of(label);
    if occ == 0 {
        *changed = unit.translate && unit.segments.iter().any(|s| !s.target.is_empty()) && unit.output() != unit.source();
    }
    if !*changed {
        return push_bytes(original, out);
    }
    if occ > 0 {
        return Ok(()); // the rest of an MTEXT's chunks: replaced by the block written at the first span
    }
    let text = lex::render(&unit.output(), &unit.codes, kind);
    let value = lex::fit_codec(&text, codec);
    let enc = |s: &str| -> Result<Vec<u8>, Error> {
        codec.encode(s).ok_or_else(|| Error::Write(format!("unit {}: cannot write the value in {}", unit.id, codec.name())))
    };
    if label != "mtext" {
        out.extend_from_slice(&enc(&value)?);
        return Ok(());
    }
    let m = unit.codes.get(META).ok_or_else(|| Error::Write(format!("unit {} has no dxf layout record", unit.id)))?;
    let template = m.open.as_deref().unwrap_or("");
    let nl1 = m.close.as_deref().unwrap_or("");
    let nl2_orig = m.standalone.as_deref().unwrap_or("");
    let nl2 = if nl2_orig.is_empty() { nl1 } else { nl2_orig };
    let pieces = lex::chunks(&value);
    for (k, piece) in pieces.iter().enumerate() {
        let code = if k == pieces.len() - 1 { 1 } else { 3 };
        let num = code.to_string();
        let line = if template.starts_with([' ', '\t']) { format!("{num:>w$}", w = template.chars().count()) } else { num };
        push_bytes(&line, out)?;
        push_bytes(nl1, out)?;
        out.extend_from_slice(&enc(piece)?);
        if k < pieces.len() - 1 || !nl2_orig.is_empty() {
            push_bytes(nl2, out)?;
        }
    }
    Ok(())
}
