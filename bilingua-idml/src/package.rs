//! The IDML package: which stories there are, in which order, and what the design map says about conditions,
//! layers and languages.

use crate::xml::{ParseLimits, Tree};
use bilingua_model::{Error, LimitError, LimitKind, Limits};
use bilingua_zip::RawEntry;
use std::collections::HashSet;

pub const IDML_MIMETYPE: &str = "application/vnd.adobe.indesign-idml-package";

pub fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "idml".into(), msg: msg.into() }
}

/// Markup budget across every XML part: forty `<` per allowed segment, counted before a tree is built.
pub struct Budget {
    left: i64,
}

impl Budget {
    pub fn new(l: &Limits) -> Budget {
        Budget { left: 40i64.saturating_mul(l.max_units as i64) }
    }
    pub fn spend(&mut self, raw: &[u8], l: &Limits) -> Result<(), Error> {
        self.left -= raw.iter().filter(|b| **b == b'<').count() as i64;
        if self.left < 0 {
            return Err(LimitError::new(
                LimitKind::XmlElements,
                format!("This IDML package has far more markup than {} segments allow. Split the document.", l.max_units),
            )
            .into());
        }
        Ok(())
    }
}

/// How an XML part is stored, so a rewritten part keeps its encoding.
#[derive(Debug, Clone, Copy)]
pub enum XmlEnc {
    Utf8,
    Utf16Le,
    Utf16Be,
    Other(&'static bilingua_model::encoding_rs::Encoding),
}

fn declared_label(head: &[u8]) -> Option<String> {
    let s: String = head.iter().take(200).map(|b| *b as char).collect();
    if !s.trim_start_matches('\u{feff}').trim_start().starts_with("<?xml") {
        return None;
    }
    let decl = &s[..s.find("?>")?];
    let p = decl.find("encoding")?;
    let rest = decl[p + 8..].trim_start().strip_prefix('=')?.trim_start();
    let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let end = rest[1..].find(q)?;
    Some(rest[1..1 + end].to_string())
}

/// Decodes an XML part: a UTF-8 or UTF-16 byte order mark, a declared charset, else UTF-8. Bytes that do not
/// decode are an error (never silently replaced).
pub fn decode_xml(bytes: &[u8], name: &str) -> Result<(String, XmlEnc), Error> {
    use bilingua_model::encoding_rs as er;
    let bad = |what: &str| perr(format!("{name} is not valid {what}"));
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[b'<', 0]) {
        let (s, _) = er::UTF_16LE.decode_without_bom_handling(bytes);
        if s.contains('\u{FFFD}') {
            return Err(bad("UTF-16"));
        }
        return Ok((s.into_owned(), XmlEnc::Utf16Le));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) || bytes.starts_with(&[0, b'<']) {
        let (s, _) = er::UTF_16BE.decode_without_bom_handling(bytes);
        if s.contains('\u{FFFD}') {
            return Err(bad("UTF-16"));
        }
        return Ok((s.into_owned(), XmlEnc::Utf16Be));
    }
    let enc = declared_label(bytes).and_then(|l| er::Encoding::for_label(l.as_bytes()));
    match enc {
        Some(e) if e != er::UTF_8 && e != er::UTF_16LE && e != er::UTF_16BE => {
            let s = bilingua_model::enc::decode_strict(e, bytes).map_err(|_| bad(e.name()))?;
            Ok((s, XmlEnc::Other(e)))
        }
        _ => {
            let s = std::str::from_utf8(bytes).map_err(|_| bad("UTF-8"))?;
            Ok((s.to_string(), XmlEnc::Utf8))
        }
    }
}

/// Encodes a rewritten part the way it was read.
pub fn encode_xml(s: &str, enc: XmlEnc) -> Vec<u8> {
    match enc {
        XmlEnc::Utf8 => s.as_bytes().to_vec(),
        XmlEnc::Utf16Le => s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect(),
        XmlEnc::Utf16Be => s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect(),
        XmlEnc::Other(e) => e.encode(s).0.into_owned(),
    }
}

pub fn parse_xml(raw: &[u8], name: &str, budget: Option<&mut Budget>, l: &Limits) -> Result<(Tree, XmlEnc), Error> {
    if let Some(b) = budget {
        b.spend(raw, l)?;
    }
    let (text, enc) = decode_xml(raw, name)?;
    let lim = ParseLimits { max_depth: l.max_xml_depth, max_elements: l.max_xml_elements };
    let tree = Tree::parse(&text, &lim).map_err(|m| perr(format!("{name} is not well-formed XML: {m}")))?;
    Ok((tree, enc))
}

/// What the design map and the spreads say.
pub struct Pkg {
    /// Story members in design map order, printable stories only.
    pub stories: Vec<String>,
    pub hidden_conditions: HashSet<String>,
    /// `$ID/...` names of the languages the package declares.
    pub languages: HashSet<String>,
}

fn story_id(member: &str) -> String {
    let seg = member.rsplit('/').next().unwrap_or(member);
    let mut from = 0;
    while let Some(p) = seg[from..].find("Story_") {
        let start = from + p + 6;
        if let Some(inner) = seg[start..].strip_suffix(".xml") {
            if !inner.is_empty() {
                return inner.to_string();
            }
        }
        from = start;
    }
    member.to_string()
}

pub fn find<'a>(entries: &'a [RawEntry], name: &str) -> Option<&'a RawEntry> {
    entries.iter().find(|e| e.name == name)
}

pub fn open(entries: &[RawEntry], l: &Limits, budget: &mut Budget) -> Result<Pkg, Error> {
    let names: HashSet<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    let Some(dm) = find(entries, "designmap.xml") else {
        return Err(perr("this ZIP is not an IDML package (no designmap.xml)"));
    };
    if let Some(mt) = find(entries, "mimetype") {
        let text: String = mt.data.iter().map(|b| if b.is_ascii() { *b as char } else { '\u{FFFD}' }).collect();
        if text.trim() != IDML_MIMETYPE {
            return Err(perr(format!("this package is not IDML (mimetype {:?})", text.trim().chars().take(80).collect::<String>())));
        }
    }
    let (tree, _) = parse_xml(&dm.data, "designmap.xml", Some(budget), l)?;
    let mut hidden_conditions = HashSet::new();
    let mut hidden_layers: HashSet<String> = HashSet::new();
    let mut languages = HashSet::new();
    let mut story_order: Vec<String> = vec![];
    for el in tree.iter(tree.root()) {
        if !tree.is_element(el) {
            continue;
        }
        match tree.local(el) {
            "Condition" => {
                if tree.attr(el, "Visible").is_some_and(|v| v.eq_ignore_ascii_case("false")) {
                    if let Some(s) = tree.attr(el, "Self") {
                        hidden_conditions.insert(s.to_string());
                    }
                }
            }
            "Layer" => {
                let off = |a: &str| tree.attr(el, a).is_some_and(|v| v.eq_ignore_ascii_case("false"));
                if off("Visible") || off("Printable") {
                    if let Some(s) = tree.attr(el, "Self") {
                        hidden_layers.insert(s.to_string());
                    }
                }
            }
            "Language" => {
                if let Some(n) = tree.attr(el, "Name") {
                    languages.insert(n.to_string());
                }
            }
            "Story" => {
                if let Some(src) = tree.attr(el, "src") {
                    if names.contains(src) && !story_order.iter().any(|s| s == src) {
                        story_order.push(src.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    let silent = nonprinting_stories(entries, &hidden_layers, l, budget)?;
    let stories = story_order.into_iter().filter(|s| !silent.contains(&story_id(s))).collect();
    Ok(Pkg { stories, hidden_conditions, languages })
}

/// Stories whose every text frame on a spread or master spread is hidden, non-printing or on a hidden or
/// non-printing layer. A story with no frame found (an anchored frame lives inside another story) is kept.
fn nonprinting_stories(entries: &[RawEntry], hidden_layers: &HashSet<String>, l: &Limits, budget: &mut Budget) -> Result<HashSet<String>, Error> {
    let mut printing: HashSet<String> = HashSet::new();
    let mut silent: HashSet<String> = HashSet::new();
    let mut spreads: Vec<&RawEntry> = entries
        .iter()
        .filter(|e| (e.name.starts_with("Spreads/") || e.name.starts_with("MasterSpreads/")) && e.name.ends_with(".xml") && !e.meta.dir)
        .collect();
    spreads.sort_by(|a, b| a.name.cmp(&b.name));
    for e in spreads {
        let (tree, _) = parse_xml(&e.data, &e.name, Some(budget), l)?;
        for el in tree.iter(tree.root()) {
            if tree.local(el) != "TextFrame" {
                continue;
            }
            let Some(story) = tree.attr(el, "ParentStory").filter(|s| !s.is_empty()) else { continue };
            let is = |a: &str, v: &str| tree.attr(el, a).is_some_and(|x| x.eq_ignore_ascii_case(v));
            let hidden = is("Nonprinting", "true") || is("Visible", "false") || tree.attr(el, "ItemLayer").is_some_and(|x| hidden_layers.contains(x));
            if hidden {
                silent.insert(story.to_string());
            } else {
                printing.insert(story.to_string());
            }
        }
    }
    Ok(silent.difference(&printing).cloned().collect())
}

/// Refuses a package whose central directory lists the same member twice (or a member name with a backslash).
/// The zip reader indexes members by name, so a repeated name would silently show only one of the two copies; the
/// directory is read here, before that.
pub fn check_member_names(bytes: &[u8]) -> Result<(), Error> {
    let u16at = |o: usize| bytes.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let sig = [0x50u8, 0x4b, 5, 6];
    let from = bytes.len().saturating_sub(22 + 65535);
    let Some(eocd) = bytes.get(from..).and_then(|t| t.windows(4).rposition(|w| w == sig)).map(|p| p + from) else { return Ok(()) };
    let (Some(count), Some(mut off)) = (u16at(eocd + 10), u32at(eocd + 16)) else { return Ok(()) };
    let mut seen: HashSet<String> = HashSet::new();
    for _ in 0..count {
        if bytes.get(off..off + 4) != Some(&[0x50, 0x4b, 1, 2]) {
            return Ok(()); // damaged directory: the zip reader reports it
        }
        let (Some(n), Some(x), Some(c)) = (u16at(off + 28), u16at(off + 30), u16at(off + 32)) else { return Ok(()) };
        let Some(name) = bytes.get(off + 46..off + 46 + n) else { return Ok(()) };
        let name = String::from_utf8_lossy(name).to_ascii_lowercase();
        if name.contains('\\') {
            return Err(perr(format!("a package member has an unsafe name: {name:?}")));
        }
        if !seen.insert(name.clone()) {
            return Err(perr(format!("the package repeats the member {name:?}")));
        }
        off += 46 + n + x + c;
    }
    Ok(())
}
