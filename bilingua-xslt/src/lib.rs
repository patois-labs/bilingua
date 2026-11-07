//! Bilingua XSLT filter: the literal result text of XSLT 1.0, 2.0 and 3.0 stylesheets (`.xsl`, `.xslt`). See the
//! README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod lex;
mod plan;
mod textenc;

use bilingua_model::encoding_rs::{Encoding, REPLACEMENT, UTF_8};
use bilingua_model::*;
use lex::{single_start_tag, Lexed};
use plan::{lit_display, PUnit, Piece};
use std::collections::BTreeMap;

pub use bilingua_model::Limits;

/// The XSLT filter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Xslt;

// Keys of the unit's hidden values (`Unit::hidden`).
const RAW: &str = "raw"; // the source span as written
const META: &str = "meta"; // kind (t text, a attribute), quote, crlf, braces doubled
const EDGE: &str = "edge"; // `ba` flags (1/0) then the `xsl:` prefix: the unit touches a stripped whitespace node
const NEST: &str = "nest"; // attribute units that sit inside the span: `from-to-unit;...` (offsets in the span)
const TVT: &str = "v"; // + token id: braces doubled inside this paired element
const TNEST: &str = "n"; // + token id: attribute units inside the token's opening markup

fn hidden(text: impl Into<String>) -> CodeData {
    CodeData::standalone(text)
}

fn code(u: &Unit, key: &str) -> Option<String> {
    u.hidden(key).map(str::to_string)
}

/// Keeps XML 1.0 characters, turning pasted Word breaks into newlines.
fn xml_target_text(s: &str) -> String {
    s.chars()
        .map(|c| if matches!(c, '\u{0B}' | '\u{0C}') { '\n' } else { c })
        .filter(|c| !matches!(*c as u32, 0..=8 | 0x0E..=0x1F | 0xFFFE | 0xFFFF))
        .collect()
}

/// `keep_cr`: the source text of the unit holds a carriage return (it came from `&#13;`, since an XML reader turns a
/// literal CR into LF), so the translation's CRs are the author's and are written back as `&#13;`.
fn escape_lit(s: &str, tvt: bool, attr: bool, quote: u8, crlf: bool, keep_cr: bool) -> String {
    let mut s = s.to_string();
    let keep_cr = keep_cr && !attr && s.contains('\r');
    if !attr && !keep_cr {
        s = s.replace("\r\n", "\n").replace('\r', "\n");
    }
    s = s.replace('&', "&amp;").replace('<', "&lt;");
    if attr {
        s = if quote == b'"' { s.replace('"', "&quot;") } else { s.replace('\'', "&apos;") };
        s = s.replace('\t', "&#9;").replace('\n', "&#10;").replace('\r', "&#13;");
    } else {
        s = s.replace('>', "&gt;");
        if keep_cr {
            s = s.replace('\r', "&#13;");
        } else if crlf {
            s = s.replace('\n', "\r\n");
        }
    }
    if tvt {
        s = s.replace('{', "{{").replace('}', "}}");
    }
    s
}

// ---------------------------------------------------------------- reading

fn unreadable(msg: &str) -> Error {
    Error::Encoding(msg.to_string())
}

/// The encoding the XML declaration names, if any; refuses a name nobody can read.
fn declared_encoding(bytes: &[u8]) -> Result<Option<&'static Encoding>, Error> {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Ok(None);
    }
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if !body.starts_with(b"<?xml") {
        return Ok(None);
    }
    let head = &body[..body.len().min(200)];
    let end = head.windows(2).position(|w| w == b"?>").unwrap_or(head.len());
    let decl = &head[..end];
    let lower = decl.to_ascii_lowercase();
    let Some(p) = lower.windows(8).position(|w| w == b"encoding") else { return Ok(None) };
    let mut i = p + 8;
    let skip = |i: &mut usize| {
        while *i < decl.len() && decl[*i].is_ascii_whitespace() {
            *i += 1;
        }
    };
    skip(&mut i);
    if decl.get(i) != Some(&b'=') {
        return Ok(None);
    }
    i += 1;
    skip(&mut i);
    let Some(&q) = decl.get(i).filter(|q| matches!(**q, b'"' | b'\'')) else { return Ok(None) };
    i += 1;
    let label: Vec<u8> = decl[i..].iter().copied().take_while(|c| *c != q).collect();
    let name = String::from_utf8_lossy(&label).trim().to_string();
    let lowered = name.to_ascii_lowercase();
    let wide = lowered.starts_with("utf-16") || lowered.starts_with("utf16") || lowered.starts_with("utf-32") || lowered.starts_with("utf32") || lowered.starts_with("ucs");
    match Encoding::for_label(name.as_bytes()) {
        Some(e) if e != REPLACEMENT && !wide => Ok(Some(e)),
        _ => Err(unreadable(&format!("This XSLT file declares the encoding {name:?}, which is not supported or does not match the file; re-save the file as UTF-8."))),
    }
}

/// Where each token's markup is: units are `PUnit` spans in the text.
struct Built {
    unit: Unit,
    /// Open and point tokens: (start, end, id).
    tokens: Vec<(usize, usize, String)>,
}

fn build_unit(text: &str, idx: usize, p: &PUnit) -> Built {
    let attr = p.attr.is_some();
    let mut inl: Vec<Inline> = Vec::new();
    let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
    let mut hidden_vals: Vec<(String, &'static str)> = Vec::new();
    let mut tokens = Vec::new();
    let mut n = 0usize;
    let mut open_ids: BTreeMap<usize, String> = BTreeMap::new();
    for piece in &p.pieces {
        match *piece {
            Piece::Lit { s, e, tvt, preserve } => {
                let d = lit_display(&text[s..e], tvt, preserve, attr);
                if !d.is_empty() {
                    inl.push(Inline::text(d));
                }
            }
            Piece::Point { s, e } => {
                n += 1;
                let id = n.to_string();
                inl.push(Inline::tag(id.clone(), TagKind::Standalone, &text[s..e]));
                codes.insert(id.clone(), hidden(&text[s..e]));
                tokens.push((s, e, id));
            }
            Piece::Open { s, e, tvt, key } => {
                n += 1;
                let id = n.to_string();
                open_ids.insert(key, id.clone());
                inl.push(Inline::tag(id.clone(), TagKind::Open, &text[s..e]));
                codes.insert(id.clone(), CodeData::new().with_open(text[s..e].to_string()));
                hidden_vals.push((format!("{TVT}{id}"), if tvt { "1" } else { "0" }));
                tokens.push((s, e, id));
            }
            Piece::Close { s, e, key } => {
                let id = open_ids.get(&key).cloned().unwrap_or_default();
                inl.push(Inline::tag(id.clone(), TagKind::Close, &text[s..e]));
                if let Some(c) = codes.get_mut(&id) {
                    c.close = Some(text[s..e].to_string());
                }
            }
        }
    }
    let mut unit = Unit::new(format!("u{}", idx + 1), merge_text(inl));
    unit.codes = codes;
    for (k, v) in hidden_vals {
        unit.set_hidden(k, v);
    }
    unit.set_hidden(RAW, &text[p.s..p.e]);
    let quote = if p.quote == 0 { '-' } else { p.quote as char };
    unit.set_hidden(META, format!("{}{}{}{}", if attr { 'a' } else { 't' }, quote, u8::from(p.crlf), u8::from(p.tvt)));
    if let Some((b, a, prefix)) = &p.edge {
        unit.set_hidden(EDGE, format!("{}{}{prefix}", u8::from(*b), u8::from(*a)));
    }
    match &p.attr {
        Some(a) => {
            unit.context.kind = Some(a.clone());
            unit.context.style = Some(format!("attr:{a}"));
        }
        None => unit.context.kind = Some("text".into()),
    }
    Built { unit, tokens }
}

/// Attribute units inside a span: (from, to, unit index).
type Nests = Vec<(usize, usize, usize)>;

fn nest_list(items: &[(usize, usize, usize)], base: usize) -> String {
    items.iter().map(|(s, e, i)| format!("{}-{}-{}", s - base, e - base, i)).collect::<Vec<_>>().join(";")
}

fn parse_nests(s: &str) -> Vec<(usize, usize, usize)> {
    s.split(';')
        .filter_map(|t| {
            let mut it = t.split('-').map(|x| x.parse::<usize>().ok());
            Some((it.next()??, it.next()??, it.next()??))
        })
        .collect()
}

fn count_budget(bytes: &[u8], l: &Limits) -> Result<(), Error> {
    let n = bytes.iter().filter(|b| matches!(**b, b'<' | b'{')).count();
    if n > l.max_units.saturating_mul(40) {
        return Err(lex::limit_err(LimitKind::XmlElements, format!("This XSLT file has far more markup than {} units allow. Split it into smaller files.", l.max_units)));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Span {
    Unit(usize),
    /// A start tag that carries a literal `lang` to restamp.
    Html,
}

impl Xslt {
    /// Like [`Filter::extract`] with explicit limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        guard::check_input(bytes, limits)?;
        count_budget(bytes, limits)?;
        let declared = declared_encoding(bytes)?;
        let dec = enc::decode_text(bytes, &enc::DecodeOptions::with_limits(*limits).with_source_lang(opts.source_lang.as_deref()).declared())?;
        if enc::is_legacy(&dec.encoding) {
            if declared.is_none() || declared == Some(UTF_8) {
                return Err(unreadable("This XSLT file is not valid UTF-8 and declares no other encoding; re-save the file as UTF-8."));
            }
            if textenc::encode(&dec.text, &dec.encoding, &bilingua_model::MergeOptions::default())? != bytes {
                // double-mapped bytes of the CJK code pages read to the same character and write back as the other
                // spelling; an untouched merge must be the original bytes, so the file is refused, never altered
                return Err(unreadable("This XSLT file uses byte sequences of its legacy encoding that cannot be written back unchanged. Re-save the file as UTF-8."));
            }
        }
        let text: &str = &dec.text;
        // the generic XML guard refuses every entity declaration; the vetted simple DOCTYPE is taken out first
        match lex::doctype_span(text)? {
            Some((s, e)) => {
                let mut blank = String::with_capacity(text.len());
                blank.push_str(&text[..s]);
                blank.push_str(&text[e..]);
                guard::check_xml(blank.as_bytes(), limits)?;
            }
            None => guard::check_xml(text.as_bytes(), limits)?,
        }
        let lexed: Lexed = lex::lex(text, limits)?;
        let plans = plan::plan(text, &lexed, limits)?;
        let mut built: Vec<Built> = plans.iter().enumerate().map(|(i, p)| build_unit(text, i, p)).collect();
        for b in &built {
            let chars = b.unit.segments[0].source.iter().map(|i| match i {
                Inline::Text(t) => t.chars().count(),
                Inline::Tag { display, .. } => display.chars().count(),
            });
            let total: usize = chars.sum();
            if total > limits.max_unit_chars {
                guard::check_unit_chars(total, limits)?;
            }
        }

        // attribute units that sit inside a text unit's markup are written by that unit
        let mut text_spans: Vec<(usize, usize, usize)> = plans.iter().enumerate().filter(|(_, p)| p.attr.is_none()).map(|(i, p)| (p.s, p.e, i)).collect();
        text_spans.sort_unstable();
        let find_text = |pos: usize| -> Option<usize> {
            let k = text_spans.partition_point(|t| t.0 <= pos);
            (k > 0).then(|| text_spans[k - 1]).filter(|t| pos < t.1).map(|t| t.2)
        };
        let mut nested = vec![false; plans.len()];
        let mut unit_nests: BTreeMap<usize, Nests> = BTreeMap::new();
        let mut tok_nests: BTreeMap<(usize, String), Nests> = BTreeMap::new();
        for (j, p) in plans.iter().enumerate() {
            if p.attr.is_none() {
                continue;
            }
            let Some(t) = find_text(p.s) else { continue };
            if p.e > plans[t].e {
                continue;
            }
            let Some((ts, _, id)) = built[t].tokens.iter().find(|(s, e, _)| *s <= p.s && p.e <= *e).cloned() else { continue };
            nested[j] = true;
            unit_nests.entry(t).or_default().push((p.s, p.e, j));
            tok_nests.entry((t, id)).or_default().push((p.s - ts, p.e - ts, j));
        }
        for (t, v) in &unit_nests {
            let v: Vec<_> = v.iter().map(|(s, e, i)| (*s, *e, *i)).collect();
            built[*t].unit.set_hidden(NEST, nest_list(&v, plans[*t].s));
        }
        for ((t, id), v) in &tok_nests {
            built[*t].unit.set_hidden(format!("{TNEST}{id}"), nest_list(v, 0));
        }

        // spans of the skeleton, in file order
        let mut spans: Vec<(usize, usize, Span)> = Vec::new();
        for (i, p) in plans.iter().enumerate() {
            if !nested[i] {
                spans.push((p.s, p.e, Span::Unit(i)));
            }
        }
        for el in &lexed.els {
            if el.is_xsl || !el.local.eq_ignore_ascii_case("html") || find_text(el.s).is_some() {
                continue;
            }
            let lang = el.attrs.iter().any(|a| (a.name == "lang" || a.name == "xml:lang") && !text[a.vs..a.ve].contains('{'));
            if lang {
                spans.push((el.s, el.oe, Span::Html));
            }
        }
        spans.sort_unstable_by_key(|s| (s.0, s.1));
        for w in spans.windows(2) {
            if w[1].0 < w[0].1 {
                return Err(lex::perr("This XSLT file could not be read (overlapping text)."));
            }
        }
        let mut parts = Vec::with_capacity(spans.len() * 2 + 1);
        let (mut pos, mut line, mut counted) = (0usize, 1usize, 0usize);
        for (start, end, what) in spans {
            if let Span::Unit(i) = what {
                line += text.as_bytes()[counted..start].iter().filter(|b| **b == b'\n').count();
                counted = start;
                built[i].unit.context.location = Some(format!("line {line}"));
            }
            if start > pos {
                parts.push(SkelPart::Raw(text[pos..start].to_string()));
            }
            pos = end;
            parts.push(match what {
                Span::Unit(i) => SkelPart::Unit(i),
                Span::Html => SkelPart::Raw(text[start..end].to_string()),
            });
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let doc = Document::new("xslt", built.into_iter().map(|b| b.unit).collect(), Skeleton::text(parts, dec.encoding, bilingua_model::parser_info!())).with_options(opts);
        guard::finish(doc, dec.warnings, limits)
    }
}

fn head_text(bytes: &[u8]) -> String {
    let head = &bytes[..bytes.len().min(4096)];
    if head.starts_with(&[0xFF, 0xFE]) {
        return bilingua_model::encoding_rs::UTF_16LE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return bilingua_model::encoding_rs::UTF_16BE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    String::from_utf8_lossy(head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head)).into_owned()
}

impl Filter for Xslt {
    fn id(&self) -> &'static str {
        "xslt"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["xsl", "xslt"]
    }

    /// Tight: XML whose first 4 KB names the XSLT namespace.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let h = head_text(bytes);
        h.trim_start().starts_with('<') && h.contains(lex::XSL_NS)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        self.extract_with(bytes, opts, &opts.limits())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("xslt needs a text skeleton".into()));
        };
        let refs = char_refs(parts, encoding);
        let any_translated = doc.units.iter().any(|u| u.translate && u.segments.iter().any(|s| s.has_target()));
        let lang = opts.stamp_lang(doc, any_translated).and_then(html_lang);
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => match lang.as_deref().and_then(|l| restamp(s, l)) {
                    Some(r) => out.push_str(&r),
                    None => out.push_str(s),
                },
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    out.push_str(&write_unit(doc, u, refs));
                }
            }
        }
        textenc::encode(&out, encoding, opts)
    }
}

/// The BCP 47 tag to declare for a locale: `es_ES` gives `es-ES`, `EN-us` gives `en-US`; `None` when it is not
/// a well-formed tag.
fn html_lang(code: &str) -> Option<String> {
    let t = code.trim().replace('_', "-");
    let parts: Vec<&str> = t.split('-').collect();
    let first = parts[0];
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let mut out = vec![first.to_ascii_lowercase()];
    for p in &parts[1..] {
        if p.is_empty() || p.len() > 8 || !p.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        out.push(if p.len() == 4 && p.bytes().all(|b| b.is_ascii_alphabetic()) {
            let (a, b) = p.split_at(1);
            format!("{}{}", a.to_ascii_uppercase(), b.to_ascii_lowercase())
        } else if p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic()) {
            p.to_ascii_uppercase()
        } else {
            p.to_ascii_lowercase()
        });
    }
    Some(out.join("-"))
}

/// A skeleton part that is exactly an `<html>` start tag gets its literal `lang` / `xml:lang` set to `tag`.
fn restamp(part: &str, tag: &str) -> Option<String> {
    let (name, attrs) = single_start_tag(part)?;
    let (prefix, local) = name.rsplit_once(':').unwrap_or(("", &name));
    if !local.eq_ignore_ascii_case("html") || prefix == "xsl" {
        return None;
    }
    let mut out = String::with_capacity(part.len() + 8);
    let mut last = 0;
    let mut changed = false;
    for (n, vs, ve, q) in attrs {
        if (n == "lang" || n == "xml:lang") && !part[vs..ve].contains('{') {
            out.push_str(&part[last..vs]);
            out.push_str(&escape_lit(tag, false, true, q, false, false));
            last = ve;
            changed = true;
        }
    }
    if !changed {
        return None;
    }
    out.push_str(&part[last..]);
    Some(out)
}

/// Characters a translated span writes as character references whatever the engine's writer would do.
#[derive(Clone, Copy, PartialEq)]
enum CharRefs {
    None,
    /// UTF-8 bytes under an `encoding="us-ascii"` declaration: everything outside ASCII.
    NonAscii,
    /// GBK / GB2312: the euro sign (the engine writes GBK through GB18030, whose A2 E3 GBK readers cannot read).
    Euro,
}

fn char_refs(parts: &[SkelPart], encoding: &str) -> CharRefs {
    let base = enc::base(encoding);
    if base == "utf-8" {
        if let Some(SkelPart::Raw(head)) = parts.first() {
            if let Some(decl) = head.strip_prefix("<?xml").and_then(|r| r.split("?>").next()) {
                let d = decl.to_ascii_lowercase().replace(' ', "");
                if ["us-ascii", "ascii"].iter().any(|l| d.contains(&format!("encoding=\"{l}\"")) || d.contains(&format!("encoding='{l}'"))) {
                    return CharRefs::NonAscii;
                }
            }
        }
    } else if Encoding::for_label(base.as_bytes()) == Some(bilingua_model::encoding_rs::GBK) {
        return CharRefs::Euro;
    }
    CharRefs::None
}

fn with_char_refs(s: String, refs: CharRefs) -> String {
    let hit = |c: char| match refs {
        CharRefs::None => false,
        CharRefs::NonAscii => !c.is_ascii(),
        CharRefs::Euro => c == '\u{20AC}',
    };
    if !s.chars().any(hit) {
        return s;
    }
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        if hit(c) {
            out.push_str(&format!("&#{};", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------- writing

fn meta_of(u: &Unit) -> (bool, u8, bool, bool) {
    let m = code(u, META).unwrap_or_default();
    let b = m.as_bytes();
    let get = |i: usize| b.get(i).copied().unwrap_or(b'0');
    (get(0) == b'a', if get(1) == b'"' || get(1) == b'\'' { get(1) } else { b'"' }, get(2) == b'1', get(3) == b'1')
}

/// `raw` with the attribute units that sit inside it written in. Only attribute units are written in, and an
/// attribute unit never writes nests of its own, so a tampered nest list cannot recurse.
fn apply_nests(doc: &Document, raw: &str, nests: &str, refs: CharRefs) -> String {
    let mut list = parse_nests(nests);
    if list.is_empty() {
        return raw.to_string();
    }
    list.sort_unstable();
    let mut out = String::with_capacity(raw.len());
    let mut last = 0;
    for (s, e, idx) in list {
        if s < last || e > raw.len() || !raw.is_char_boundary(s) || !raw.is_char_boundary(e) {
            continue;
        }
        let Some(u) = doc.units.get(idx).filter(|u| meta_of(u).0) else { continue };
        out.push_str(&raw[last..s]);
        out.push_str(&write_unit(doc, u, refs));
        last = e;
    }
    out.push_str(&raw[last..]);
    out
}

fn write_unit(doc: &Document, u: &Unit, refs: CharRefs) -> String {
    let raw = code(u, RAW).unwrap_or_default();
    let (attr, ..) = meta_of(u);
    let nests = code(u, NEST).unwrap_or_default();
    let plain = |raw: &str| if attr { raw.to_string() } else { apply_nests(doc, raw, &nests, refs) };
    if !u.translate || u.segments.iter().all(|s| !s.has_target()) {
        return plain(&raw);
    }
    let new = u.output();
    if new == u.source() {
        return plain(&raw);
    }
    rebuild(doc, u, &new, refs)
}

/// Writes a translated unit: literal text escaped for its place, tokens put back as the source wrote them.
/// A damaged token stream is repaired: an unknown or repeated token is dropped, an unclosed pair is closed, a
/// dropped point is appended; nothing protected is lost.
fn rebuild(doc: &Document, u: &Unit, target: &[Inline], refs: CharRefs) -> String {
    let (attr, quote, crlf, top_tvt) = meta_of(u);
    let keep_cr = u.source().iter().any(|i| matches!(i, Inline::Text(t) if t.contains('\r')));
    let mut out = String::new();
    let mut stack: Vec<String> = Vec::new();
    let mut tvts = vec![top_tvt];
    let mut used: Vec<String> = Vec::new();
    let tvt_of = |id: &str| code(u, &format!("{TVT}{id}")).is_some_and(|v| v == "1");
    let side = |id: &str, close: bool| -> Option<String> {
        let c = u.codes.get(id)?;
        if close {
            return c.close.clone();
        }
        let raw = c.open.clone().or_else(|| c.standalone.clone())?;
        if attr {
            return Some(raw);
        }
        let nests = code(u, &format!("{TNEST}{id}")).unwrap_or_default();
        Some(apply_nests(doc, &raw, &nests, refs))
    };
    let kind_of = |id: &str| -> Option<bool> {
        // true for a paired token
        let c = u.codes.get(id)?;
        Some(c.open.is_some())
    };
    // Text written right against a whitespace-only node the stylesheet strips would join it, so the whitespace
    // would be kept in the output and the units would merge on the next read: such edge text goes in `xsl:text`.
    let edge = code(u, EDGE).unwrap_or_default();
    let (wrap_first, wrap_last, wrap_prefix) = if attr || edge.len() < 2 { (false, false, "") } else { (edge.starts_with('1'), edge[1..].starts_with('1'), &edge[2..]) };
    let last = target.len().saturating_sub(1);
    for (n, i) in target.iter().enumerate() {
        match i {
            Inline::Text(t) => {
                let tvt = *tvts.last().unwrap_or(&top_tvt);
                let lit = with_char_refs(escape_lit(&xml_target_text(t), tvt, attr, quote, crlf, keep_cr), refs);
                if ((n == 0 && wrap_first) || (n == last && wrap_last)) && !lit.trim().is_empty() && stack.is_empty() {
                    out.push_str(&format!("<{wrap_prefix}text>{lit}</{wrap_prefix}text>"));
                } else {
                    out.push_str(&lit);
                }
            }
            Inline::Tag { id, kind, .. } => match kind {
                TagKind::Close => {
                    if let Some(at) = stack.iter().position(|s| s == id) {
                        while stack.len() > at {
                            let top = stack.pop().unwrap_or_default();
                            tvts.pop();
                            out.push_str(&side(&top, true).unwrap_or_default());
                        }
                    }
                }
                k => {
                    let paired = *k == TagKind::Open;
                    if used.contains(id) || kind_of(id) != Some(paired) {
                        continue;
                    }
                    let Some(raw) = side(id, false) else { continue };
                    used.push(id.clone());
                    out.push_str(&raw);
                    if paired {
                        stack.push(id.clone());
                        tvts.push(tvt_of(id));
                    }
                }
            },
        }
    }
    while let Some(top) = stack.pop() {
        out.push_str(&side(&top, true).unwrap_or_default());
    }
    // a dropped protected point is appended, never lost
    for i in u.source() {
        if let Inline::Tag { id, kind: TagKind::Standalone, .. } = i {
            if !used.contains(&id) {
                out.push_str(&side(&id, false).unwrap_or_default());
                used.push(id);
            }
        }
    }
    out
}
