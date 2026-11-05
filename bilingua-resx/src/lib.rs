//! Bilingua `.resx` and `.resw` filter (.NET and UWP / WinUI resources). See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod ph;
mod xml;

pub use bilingua_model::Limits;

use bilingua_model::*;
use bilingua_model::enc::{self, DecodeOptions};
use bilingua_model::guard;
use std::collections::BTreeMap;

/// The `.resx` / `.resw` filter.
pub struct Resx;

/// One resource entry as a key-based project sees it (translatable or not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub key: String,
    pub value: String,
    pub comment: String,
    /// Index among the translatable units; `None` when the entry is not translatable.
    pub unit_index: Option<usize>,
    pub translatable: bool,
}

// Reserved code ids (start with `~`, never a numeric tag id).
const RAW: &str = "~raw"; // the raw inner XML of the original <value>
const LEAD: &str = "~lead"; // white space the value started with (decoded)
const TRAIL: &str = "~trail"; // white space the value ended with (decoded)
const CDATA: &str = "~cdata"; // the original held a CDATA section
const NL: &str = "~nl"; // "crlf" when the file's line ends are all CRLF

/// (leading white space, core, trailing white space), as Python's `str.strip()` would cut it.
fn split_ws(s: &str) -> (&str, &str, &str) {
    let core = s.trim_matches(bilingua_model::py_isspace_char);
    if core.is_empty() {
        return (s, "", "");
    }
    let lead = &s[..s.len() - s.trim_start_matches(bilingua_model::py_isspace_char).len()];
    let trail = &s[s.trim_end_matches(bilingua_model::py_isspace_char).len()..];
    (lead, core, trail)
}

/// A value with a letter or digit. (Python's `isalnum`, exactly.)
fn has_text(s: &str) -> bool {
    bilingua_model::py_has_alnum(s)
}

struct Parsed {
    text: String,
    enc: String,
    warnings: Vec<Warning>,
    scan: xml::Scan,
}

fn parse(bytes: &[u8], opts: &ExtractOptions) -> Result<Parsed, Error> {
    let l = opts.limits();
    guard::check_input(bytes, &l)?;
    let dec = enc::decode_text(bytes, &DecodeOptions::new(opts).declared())?;
    guard::check_xml(dec.text.as_bytes(), &l)?;
    let scan = xml::scan(&dec.text)?;
    Ok(Parsed { text: dec.text, enc: dec.encoding, warnings: dec.warnings, scan })
}

fn hidden(text: impl Into<String>) -> CodeData {
    CodeData { standalone: Some(text.into()), ..Default::default() }
}

fn code(u: &Unit, key: &str) -> Option<String> {
    u.codes.get(key).and_then(|c| c.standalone.clone())
}

fn tags_of(core: &str) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    let mut inl = Vec::new();
    let mut codes = BTreeMap::new();
    let mut n = 0usize;
    for piece in ph::split(core) {
        match piece {
            ph::Piece::Text(t) => inl.push(Inline::text(t)),
            ph::Piece::Ph(p) => {
                n += 1;
                let id = n.to_string();
                codes.insert(id.clone(), CodeData { standalone: Some(p.to_string()), label: Some("placeholder".into()), ..Default::default() });
                inl.push(if locos::is_named_placeholder(p) { Inline::free_tag(id, p) } else { Inline::tag(id, TagKind::Standalone, p) });
            }
        }
    }
    (inl, codes)
}

fn render(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, display, .. } => match u.codes.get(id).and_then(|c| c.standalone.as_deref()) {
                Some(c) => s.push_str(c),
                None => s.push_str(display),
            },
        }
    }
    s
}

/// Keeps XML 1.0 characters; pasted Word breaks become newlines (LocOS `xml_target_text`).
fn clean(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\u{0B}' | '\u{0C}' => Some('\n'),
            '\u{0}'..='\u{8}' | '\u{0E}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}' => None,
            c => Some(c),
        })
        .collect()
}

/// A translated value as inner XML.
fn encode_value(new: &str, cdata: bool, crlf: bool) -> String {
    let new = clean(new);
    let nl = |s: String| if crlf { s.replace('\n', "\r\n") } else { s };
    if cdata && !new.contains("]]>") && !new.contains('\r') {
        return format!("<![CDATA[{}]]>", nl(new));
    }
    let mut out = String::with_capacity(new.len() + 8);
    for c in new.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    nl(out)
}

impl Resx {
    /// Like [`Filter::extract`] with explicit limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        let opts = &ExtractOptions { limits: Some(*limits), ..opts.clone() };
        let limits = &opts.limits();
        let p = parse(bytes, opts)?;
        let mut units: Vec<Unit> = Vec::new();
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut pos = 0usize;
        let (mut n_tr, mut n_lock) = (0usize, 0usize);
        let (mut line, mut line_at) = (1usize, 0usize);
        for e in &p.scan.entries {
            if e.typed || e.complex {
                continue;
            }
            let (lead, core, trail) = split_ws(&e.display);
            let translatable = !e.name.starts_with(">>") && has_text(core);
            let Some((a, b)) = e.inner else { continue };
            if !translatable && (!opts.include_locked || core.is_empty()) {
                continue;
            }
            guard::check_unit_count(units.len() + 1, limits)?;
            let mut unit = if translatable {
                n_tr += 1;
                let (inl, codes) = tags_of(core);
                let mut u = Unit::new(format!("u{n_tr}"), inl);
                u.codes = codes;
                u
            } else {
                n_lock += 1;
                let mut u = Unit::new(format!("k{n_lock}"), vec![Inline::text(core)]);
                u.translate = false;
                u.lock_reason = Some(if e.name.starts_with(">>") { "designer metadata".into() } else { "no translatable text".into() });
                u
            };
            unit.codes.insert(RAW.into(), hidden(&p.text[a..b]));
            if !lead.is_empty() {
                unit.codes.insert(LEAD.into(), hidden(lead));
            }
            if !trail.is_empty() {
                unit.codes.insert(TRAIL.into(), hidden(trail));
            }
            if e.cdata {
                unit.codes.insert(CDATA.into(), hidden("1"));
            }
            if p.scan.crlf {
                unit.codes.insert(NL.into(), hidden("crlf"));
            }
            line += p.text.as_bytes()[line_at..e.start].iter().filter(|c| **c == b'\n').count();
            line_at = e.start;
            unit.context.kind = Some("string".into());
            unit.context.key = Some(e.name.clone());
            unit.context.location = Some(format!("line {line}"));
            if !e.comment.is_empty() {
                unit.context.notes = vec![e.comment.clone()];
            }
            if a > pos {
                parts.push(SkelPart::Raw(p.text[pos..a].to_string()));
            }
            pos = b;
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
        }
        if pos < p.text.len() {
            parts.push(SkelPart::Raw(p.text[pos..].to_string()));
        }
        guard::finish(Document {
            format: "resx".into(),
            name: None,
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(parts, p.enc, bilingua_model::parser_info!()),
            warnings: Vec::new(),
        }, p.warnings, limits)
    }

    /// Every `<data>` with a `<value>` in file order, translatable or not (the key-based view).
    pub fn records(&self, bytes: &[u8]) -> Result<Vec<Record>, Error> {
        let p = parse(bytes, &ExtractOptions::default())?;
        let mut k = 0usize;
        let mut out = Vec::with_capacity(p.scan.entries.len());
        for e in &p.scan.entries {
            let translatable = !e.typed && !e.complex && !e.name.starts_with(">>") && e.inner.is_some() && has_text(split_ws(&e.display).1);
            let unit_index = translatable.then(|| {
                k += 1;
                k - 1
            });
            out.push(Record { key: e.name.clone(), value: e.display.clone(), comment: e.comment.clone(), unit_index, translatable });
        }
        Ok(out)
    }
}

impl Filter for Resx {
    fn id(&self) -> &'static str {
        "resx"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["resx", "resw"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(8192)];
        let o = ExtractOptions::default();
        let d = DecodeOptions::new(&o);
        let Ok(t) = enc::decode_text(head, &d).or_else(|_| enc::decode_text(&head[..head.len().saturating_sub(3)], &d)) else { return false };
        let t = t.text.to_ascii_lowercase();
        t.contains("<root") && (t.contains("<resheader") || t.contains("<data name=") || t.contains("microsoft-resx"))
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &Limits::default())
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("resx needs a text skeleton".into()));
        };
        let mut out = String::new();
        let mut changed = false;
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    changed |= write_unit(u, &mut out);
                }
            }
        }
        finish_bytes(out, encoding, changed)
    }
}

fn write_unit(u: &Unit, out: &mut String) -> bool {
    let raw = u.codes.get(RAW).and_then(|c| c.standalone.as_deref()).unwrap_or("");
    if !u.translate || u.segments.iter().all(|s| s.target.is_empty()) {
        out.push_str(raw);
        return false;
    }
    let new = render(u, &u.output());
    if new == render(u, &u.source()) {
        out.push_str(raw);
        return false;
    }
    let full = format!("{}{}{}", code(u, LEAD).unwrap_or_default(), new, code(u, TRAIL).unwrap_or_default());
    out.push_str(&encode_value(&full, code(u, CDATA).is_some(), code(u, NL).as_deref() == Some("crlf")));
    true
}

/// The encoding named in the XML declaration: byte span of the name and the name.
fn declared(text: &str) -> Option<(usize, usize, String)> {
    let head_end = text.char_indices().nth(300).map_or(text.len(), |(i, _)| i);
    let head = &text[..head_end];
    if !head.starts_with("<?xml") {
        return None;
    }
    let decl_end = head.find("?>")?;
    let decl = &head[..decl_end];
    let k = decl.find("encoding")?;
    let rest = &decl[k + 8..];
    let eq = rest.find('=')?;
    let after = &rest[eq + 1..];
    let q = after.find(['"', '\''])?;
    let quote = after.as_bytes()[q] as char;
    let vstart = k + 8 + eq + 1 + q + 1;
    let vlen = decl[vstart..].find(quote)?;
    Some((vstart, vstart + vlen, decl[vstart..vstart + vlen].to_string()))
}

fn utf8_name(n: &str) -> bool {
    matches!(n.trim().to_ascii_lowercase().replace('_', "-").as_str(), "utf-8" | "utf8")
}

fn rewrite_decl(text: &str) -> String {
    match declared(text) {
        Some((a, b, _)) => format!("{}UTF-8{}", &text[..a], &text[b..]),
        None => text.to_string(),
    }
}

fn finish_bytes(mut out: String, encoding: &str, changed: bool) -> Result<Vec<u8>, Error> {
    let utf8_family = matches!(encoding, "utf-8" | "utf-8-bom");
    if changed && utf8_family && !out.is_ascii() {
        if let Some((_, _, name)) = declared(&out) {
            if !utf8_name(&name) {
                out = rewrite_decl(&out);
            }
        }
    }
    enc::encode_text(&out, encoding)
}
