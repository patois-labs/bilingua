//! Bilingua Trados Studio project files (`.sdlproj`). See the README.
//!
//! LocOS reads a `.sdlproj` only as the manifest inside a Trados package: it takes the language directions, the
//! package GUID, type and name, and extracts no text from it. This filter does the same for a loose file: zero
//! units, the languages on the document, and an untouched merge that returns the original bytes.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

use bilingua_model::guard::{LimitError, LimitKind};
use bilingua_model::*;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

/// LocOS reads no `.sdlproj` over 16 MB (`MAX_SDLPROJ_BYTES`).
pub const MAX_SDLPROJ_BYTES: usize = 16 * 1024 * 1024;

pub use bilingua_model::Limits;

/// What a project file says about itself (the facts LocOS takes from a package manifest).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectInfo {
    /// Local name of the root element (`PackageProject` or `Project`).
    pub root: String,
    /// `PackageGuid`, else `Guid`, trimmed, at most 64 characters.
    pub package_guid: String,
    /// `PackageType` (`ProjectPackage`, `ReturnPackage`), trimmed, at most 40 characters.
    pub package_type: String,
    /// `Name`, trimmed, at most 200 characters (empty when absent).
    pub name: String,
    /// The one source language of the project.
    pub source_lang: String,
    /// The target languages, in order, each once (compared without case).
    pub target_langs: Vec<String>,
}

fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "sdlproj".into(), msg: msg.into() }
}

fn xml_char(c: char) -> bool {
    matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

/// Every `&` starts a predefined entity or a character reference to an XML character.
fn check_ref(name: &str) -> Result<(), Error> {
    let ok = match name {
        "lt" | "gt" | "amp" | "quot" | "apos" => true,
        _ => name.strip_prefix('#').is_some_and(|n| {
            let v = match n.strip_prefix('x') {
                Some(h) if !h.is_empty() && h.bytes().all(|c| c.is_ascii_hexdigit()) => u32::from_str_radix(h, 16).ok(),
                None if !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()) => n.parse().ok(),
                _ => None,
            };
            v.and_then(char::from_u32).is_some_and(xml_char)
        }),
    };
    if ok {
        Ok(())
    } else {
        Err(perr(format!("not well-formed XML: undefined entity &{name};")))
    }
}

fn check_refs(s: &str) -> Result<(), Error> {
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        let after = &rest[p + 1..];
        let Some(semi) = after.find(';').filter(|i| *i <= 12) else {
            return Err(perr("not well-formed XML: bad entity reference"));
        };
        check_ref(&after[..semi])?;
        rest = &after[semi + 1..];
    }
    Ok(())
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let after = &rest[p + 1..];
        let Some(semi) = after.find(';') else {
            out.push_str(&rest[p..]);
            return out;
        };
        let name = &after[..semi];
        match name {
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "amp" => out.push('&'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let v = match name.strip_prefix("#x") {
                    Some(h) => u32::from_str_radix(h, 16).ok(),
                    None => name.strip_prefix('#').and_then(|d| d.parse().ok()),
                };
                if let Some(c) = v.and_then(char::from_u32) {
                    out.push(c);
                }
            }
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    out
}

fn lookup<'a>(ns: &'a [(String, String)], prefix: &str) -> Option<&'a str> {
    if prefix == "xml" {
        return Some("http://www.w3.org/XML/1998/namespace");
    }
    ns.iter().rev().find(|(p, _)| p == prefix).map(|(_, u)| u.as_str())
}

/// The attributes of a tag as (qualified name, unescaped value); duplicates and bad references are errors.
fn attrs_of(e: &BytesStart<'_>) -> Result<Vec<(String, String)>, Error> {
    let mut out: Vec<(String, String)> = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(|e| perr(format!("not well-formed XML: {e}")))?;
        let name = std::str::from_utf8(a.key.as_ref()).map_err(|_| perr("not well-formed XML: attribute name"))?.to_string();
        let raw = std::str::from_utf8(&a.value).map_err(|_| perr("not well-formed XML: attribute value"))?;
        if raw.contains('<') {
            return Err(perr("not well-formed XML: '<' in an attribute value"));
        }
        check_refs(raw)?;
        if out.iter().any(|(n, _)| *n == name) {
            return Err(perr(format!("not well-formed XML: attribute {name} appears twice")));
        }
        out.push((name, unescape(raw)));
    }
    Ok(out)
}

fn limited(s: &str, n: usize) -> String {
    s.trim().chars().take(n).collect()
}

/// Reads the project facts from decoded text, checking it is well-formed XML on the way.
fn read_project(text: &str, limits: &Limits) -> Result<ProjectInfo, Error> {
    if let Some(c) = text.chars().find(|c| !xml_char(*c)) {
        return Err(perr(format!("not well-formed XML: character U+{:04X} is not allowed", c as u32)));
    }
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_end_names = true;
    reader.config_mut().check_comments = true;
    let mut ns: Vec<(String, String)> = Vec::new();
    let mut ns_marks: Vec<usize> = Vec::new();
    let mut depth = 0usize;
    let mut root_seen = false;
    let mut info = ProjectInfo::default();
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut elements = 0usize;
    loop {
        let before = reader.buffer_position() as usize;
        let ev = reader.read_event().map_err(|e| perr(format!("not well-formed XML: {e}")))?;
        match ev {
            Event::Eof => break,
            Event::Decl(_) => {
                if before != 0 {
                    return Err(perr("not well-formed XML: the XML declaration is not at the start"));
                }
            }
            Event::DocType(_) => return Err(perr("This .sdlproj file declares a DOCTYPE / DTD; refused (a project file never needs one).")),
            Event::PI(_) | Event::Comment(_) => {}
            Event::CData(_) => {
                if depth == 0 {
                    return Err(perr("not well-formed XML: text outside the root element"));
                }
            }
            Event::Text(t) => {
                let b: &[u8] = &t;
                if depth == 0 {
                    if !b.iter().all(|c| is_ws(*c)) {
                        return Err(perr("not well-formed XML: text outside the root element"));
                    }
                } else {
                    if b.windows(3).any(|w| w == b"]]>") {
                        return Err(perr("not well-formed XML: ']]>' in text"));
                    }
                    if b.contains(&b'&') {
                        check_refs(&String::from_utf8_lossy(b))?;
                    }
                }
            }
            Event::GeneralRef(r) => {
                if depth == 0 {
                    return Err(perr("not well-formed XML: reference outside the root element"));
                }
                let name = r.decode().map_err(|e| perr(format!("not well-formed XML: {e}")))?;
                check_ref(&name)?;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if let Some(m) = ns_marks.pop() {
                    ns.truncate(m);
                }
            }
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                if depth == 0 && root_seen {
                    return Err(perr("not well-formed XML: more than one root element"));
                }
                elements += 1;
                if elements > limits.max_xml_elements {
                    return Err(LimitError::new(LimitKind::XmlElements, "This file has too many XML elements. It was not processed.").into());
                }
                let attrs = attrs_of(e)?;
                let mark = ns.len();
                for (n, v) in &attrs {
                    if n == "xmlns" {
                        ns.push((String::new(), v.clone()));
                    } else if let Some(p) = n.strip_prefix("xmlns:") {
                        ns.push((p.to_string(), v.clone()));
                    }
                }
                let qname = std::str::from_utf8(e.name().into_inner()).map_err(|_| perr("not well-formed XML: element name"))?.to_string();
                let (prefix, local) = qname.split_once(':').unwrap_or(("", &qname));
                if !prefix.is_empty() && lookup(&ns, prefix).is_none() {
                    return Err(perr(format!("not well-formed XML: namespace prefix {prefix} is not defined")));
                }
                for (n, _) in &attrs {
                    if let Some((p, _)) = n.split_once(':') {
                        if p != "xmlns" && lookup(&ns, p).is_none() {
                            return Err(perr(format!("not well-formed XML: namespace prefix {p} is not defined")));
                        }
                    }
                }
                let get = |k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
                if !root_seen {
                    root_seen = true;
                    if local != "PackageProject" && local != "Project" {
                        return Err(perr(format!("This is not a Trados project file (root element {local:?}).")));
                    }
                    info.root = local.to_string();
                    info.package_guid = limited(get("PackageGuid").or(get("Guid")).unwrap_or(""), 64);
                    info.package_type = limited(get("PackageType").unwrap_or(""), 40);
                    info.name = limited(get("Name").unwrap_or(""), 200);
                }
                if local == "LanguageDirection" {
                    let s = get("SourceLanguageCode").unwrap_or("").trim();
                    let t = get("TargetLanguageCode").unwrap_or("").trim();
                    if !s.is_empty() && !t.is_empty() {
                        pairs.push((s.to_string(), t.to_string()));
                    }
                }
                if empty {
                    ns.truncate(mark);
                } else {
                    depth += 1;
                    if depth > limits.max_xml_depth {
                        return Err(LimitError::new(LimitKind::XmlDepth, "This file nests XML elements too deep. It was not processed.").into());
                    }
                    ns_marks.push(mark);
                }
            }
        }
    }
    if !root_seen {
        return Err(perr("not well-formed XML: no root element"));
    }
    if depth != 0 {
        return Err(perr("not well-formed XML: the file ends inside an element"));
    }
    if pairs.is_empty() {
        return Err(perr("This .sdlproj names no language direction (SourceLanguageCode / TargetLanguageCode)."));
    }
    let mut sources: Vec<String> = pairs.iter().map(|(s, _)| s.to_lowercase()).collect();
    sources.sort();
    sources.dedup();
    if sources.len() != 1 {
        return Err(perr("This project has more than one source language; a project here has one."));
    }
    info.source_lang = pairs[0].0.clone();
    for (_, t) in &pairs {
        if !info.target_langs.iter().any(|x| x.to_lowercase() == t.to_lowercase()) {
            info.target_langs.push(t.clone());
        }
    }
    Ok(info)
}

fn open(bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<(ProjectInfo, enc::Decoded), Error> {
    guard::check_input(bytes, limits)?;
    if bytes.len() > MAX_SDLPROJ_BYTES {
        return Err(LimitError::new(LimitKind::InputBytes, "This Trados .sdlproj file is too large to read (16 MB at most).").into());
    }
    guard::check_xml(bytes, limits)?;
    let dec = enc::decode_text(bytes, &enc::DecodeOptions::with_limits(*limits).with_source_lang(opts.source_lang.as_deref()).declared())?;
    // XML without a declaration is UTF-8 (or UTF-16 with a BOM); anything else is refused, as LocOS and the XML
    // specification do
    if !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") {
        let Some(declared) = enc::declared_encoding(bytes) else {
            return Err(Error::Encoding("This .sdlproj file is not UTF-8 and does not declare its encoding. Re-save the file as UTF-8.".into()));
        };
        // the bytes were read with a fallback code page: they are not valid in the encoding the file declares
        // (a file that says UTF-8 but is not, or bad Shift_JIS). An XML parser refuses that, and so does LocOS.
        let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        if declared == encoding_rs::UTF_8 || declared.decode_without_bom_handling_and_without_replacement(body).is_none() {
            return Err(Error::Encoding(format!(
                "This .sdlproj file declares {} but its bytes are not valid in that encoding. Re-save the file as UTF-8.",
                declared.name()
            )));
        }
    }
    let info = read_project(&dec.text, limits)?;
    Ok((info, dec))
}

/// The facts of a project file with the default limits: languages, GUID, package type and name.
pub fn project_info(bytes: &[u8]) -> Result<ProjectInfo, Error> {
    let opts = ExtractOptions::default();
    open(bytes, &opts, &opts.limits()).map(|(i, _)| i)
}

/// The manifest of a return package: `PackageType="ProjectPackage"` becomes `"ReturnPackage"` on the root element
/// only, every other byte unchanged. A manifest without that attribute value comes back as it is.
pub fn flip_package_type(sdlproj: &[u8]) -> Vec<u8> {
    const NAME: &[u8] = b"PackageProject";
    let find = |hay: &[u8], needle: &[u8], from: usize| hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from);
    let start = find(sdlproj, NAME, 0).unwrap_or(0);
    let end = sdlproj[start..].iter().position(|b| *b == b'>').map(|p| p + start);
    // as LocOS: with no '>' the head is empty and nothing changes
    let head_len = end.map_or(0, |e| e + 1);
    let (head, tail) = sdlproj.split_at(head_len);
    let mut out = Vec::with_capacity(sdlproj.len() + 4);
    match replace_package_type(head) {
        Some(h) => out.extend_from_slice(&h),
        None => out.extend_from_slice(head),
    }
    out.extend_from_slice(tail);
    out
}

/// `\bPackageType\s*=\s*(["'])ProjectPackage\1` -> `ReturnPackage`, first match only. As in LocOS's bytes regex,
/// `\b` and `\s` are ASCII: a non-ASCII byte before the name is a word boundary.
fn replace_package_type(head: &[u8]) -> Option<Vec<u8>> {
    const KEY: &[u8] = b"PackageType";
    const VAL: &[u8] = b"ProjectPackage";
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let ws = |b: &u8| matches!(*b, b' ' | b'\t' | b'\r' | b'\n' | 0x0B | 0x0C);
    let mut i = 0;
    while i + KEY.len() <= head.len() {
        if &head[i..i + KEY.len()] == KEY && (i == 0 || !word(head[i - 1])) {
            let mut j = i + KEY.len();
            while head.get(j).is_some_and(ws) {
                j += 1;
            }
            if head.get(j) == Some(&b'=') {
                j += 1;
                while head.get(j).is_some_and(ws) {
                    j += 1;
                }
                if let Some(&q) = head.get(j).filter(|q| **q == b'"' || **q == b'\'') {
                    let v = j + 1;
                    if head[v..].starts_with(VAL) && head.get(v + VAL.len()) == Some(&q) {
                        let mut out = head[..v].to_vec();
                        out.extend_from_slice(b"ReturnPackage");
                        out.extend_from_slice(&head[v + VAL.len()..]);
                        return Some(out);
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// The Trados project file filter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sdlproj;

impl Sdlproj {
    /// Like [`Filter::extract`] with explicit limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        let (info, dec) = open(bytes, opts, limits)?;
        if !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") && enc::encode_text(&dec.text, &dec.encoding)? != bytes {
            return Err(Error::Encoding("This .sdlproj file uses byte sequences of its legacy encoding that cannot be written back unchanged. Re-save the file as UTF-8.".into()));
        }
        let doc = Document::new("sdlproj", Vec::new(), Skeleton::text(vec![SkelPart::Raw(dec.text.clone())], dec.encoding.clone(), bilingua_model::parser_info!())).with_name(if info.name.is_empty() { None } else { Some(info.name.clone()) }).with_source_lang(info.source_lang.clone()).with_target_lang(info.target_langs.first().cloned()).with_granularity(opts.granularity);
        guard::finish(doc, dec.warnings, limits)
    }
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

impl Filter for Sdlproj {
    fn id(&self) -> &'static str {
        "sdlproj"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["sdlproj"]
    }

    /// Tight: XML whose first 4 KB has a `<PackageProject` root, or a `<Project` root with a `Guid` and either
    /// `GeneralProjectInfo` or `LanguageDirection`. The extension is the normal way in.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let h = head_text(bytes);
        h.trim_start().starts_with('<')
            && (h.contains("<PackageProject") || (h.contains("<Project ") && h.contains("Guid=") && (h.contains("GeneralProjectInfo") || h.contains("LanguageDirection"))))
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &opts.limits())
    }

    /// Every option gives the original bytes in the file's own encoding. A project file has no text to translate,
    /// so there is nothing to stamp (`stamp_target_always` changes nothing: the target languages are the project's
    /// own), and `output_encoding: Utf8` is not applied either: the file is a manifest Trados reads back as it wrote
    /// it, and LocOS keeps it byte for byte (its one edit is [`flip_package_type`]).
    fn merge_with(&self, doc: &Document, _opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("sdlproj needs a text skeleton".into()));
        };
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => return Err(Error::Write(format!("skeleton points at unit {i}, but a project file has none"))),
            }
        }
        enc::encode_text(&out, encoding)
    }
}
