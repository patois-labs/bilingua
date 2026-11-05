//! Bilingua DTD filter: Mozilla localization `.dtd` entity files.
//!
//! The value of each general internal entity (`<!ENTITY name "value">`) with a letter or digit outside its
//! references and markup is a unit. Parameter entities, external entities, `%name;` references, processing
//! instructions, comments and every other declaration stay in the skeleton byte for byte, and so does a value that
//! was not changed. Nothing is expanded, resolved or fetched, and the file is never given to an XML parser.
//!
//! The translator sees the value with `&amp; &quot; &apos;` and character references resolved. Every other
//! reference (`&brandShortName;`, `&lt;`) and markup (`<html:b>`, `&lt;i>`) is an inline tag whose display is the
//! text as written and whose label is `placeholder` (Localization OS keeps these as plain text). Build escapes `&`,
//! the value's own quote and `%` (as `&#037;`), keeps the quote character, and leaves other bytes alone.
//!
//! UTF-8 only (BOM kept), as Mozilla ships. The comments directly above an entity are its notes.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod scan;

use bilingua_model::*;
use scan::{encode_value, is_translatable, split_ws, token_re, unit_text};
use std::collections::BTreeMap;

/// The DTD filter.
pub struct DtdFilter;

const ID: &str = "dtd";
const BOM8: &[u8] = &[0xEF, 0xBB, 0xBF];
/// The one hidden code of a unit: `standalone` = the original raw value, `open` = white space before the text,
/// `close` = white space after it, `label` = the quote character.
const HIDDEN: &str = "$dtd";

/// The size limits (`bilingua_model::Limits`, Localization OS's numbers). This filter reads `max_input_bytes`,
/// `max_lines`, `max_units` (also the declaration budget: at most four `<!` per allowed unit) and `max_unit_chars`.
pub use bilingua_model::Limits;

fn parse_err(msg: String) -> Error {
    Error::Parse { format: ID.into(), msg }
}

fn read<'a>(bytes: &'a [u8], l: &Limits) -> Result<(&'a str, &'static str, Vec<scan::Entity>), Error> {
    guard::check_input(bytes, l)?;
    let (body, enc) = match bytes.strip_prefix(BOM8) {
        Some(b) => (b, "utf-8-bom"),
        None => (bytes, "utf-8"),
    };
    let text = std::str::from_utf8(body).map_err(|e| {
        parse_err(format!("Mozilla .dtd files are UTF-8; byte {} is not valid. Re-save the file as UTF-8.", e.valid_up_to() + bytes.len() - body.len()))
    })?;
    if text.contains('\0') {
        return Err(parse_err("the file holds NUL bytes, so it is not UTF-8 text; re-save the file as UTF-8".into()));
    }
    guard::check_lines(text, l)?;
    let decls = text.matches("<!").count();
    if decls > l.max_units.saturating_mul(4) {
        return Err(LimitError::new(
            LimitKind::Units,
            format!("This DTD file has far more declarations than {} segments allow. Split it into smaller files.", l.max_units),
        )
        .into());
    }
    Ok((text, enc, scan::scan(text)?))
}

fn owned(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

#[derive(PartialEq)]
enum Shape {
    Open,
    Close,
    Whole,
}

/// Tag shape and element name of a markup token (`<b>`, `</b>`, `<br/>`, `&lt;i>`, `&lt;/i>`).
fn shape(tok: &str) -> (Shape, &str) {
    let inner = tok.strip_prefix("&lt;").or_else(|| tok.strip_prefix('<')).unwrap_or(tok);
    let (closing, inner) = match inner.strip_prefix('/') {
        Some(r) => (true, r),
        None => (false, inner),
    };
    let name_len = inner.find(|c: char| bilingua_model::py_isspace_char(c) || c == '/' || c == '>').unwrap_or(inner.len());
    let shape = if closing {
        Shape::Close
    } else if tok.ends_with("/>") {
        Shape::Whole
    } else {
        Shape::Open
    };
    (shape, &inner[..name_len])
}

/// Splits a translator value into text and tags. Markup that has its partner becomes an open/close pair; other
/// markup and every reference is a standalone code (references can move freely).
fn tokenize(core: &str) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    struct Tok<'a> {
        raw: &'a str,
        shape: Option<(Shape, &'a str)>,
        partner: Option<usize>,
    }
    let mut pieces: Vec<Result<&str, Tok>> = Vec::new();
    let mut pos = 0;
    for m in token_re().find_iter(core) {
        if m.start() > pos {
            pieces.push(Ok(&core[pos..m.start()]));
        }
        let raw = m.as_str();
        let sh = (!raw.starts_with('&') || raw.starts_with("&lt;")).then(|| shape(raw));
        pieces.push(Err(Tok { raw, shape: sh, partner: None }));
        pos = m.end();
    }
    if pos < core.len() {
        pieces.push(Ok(&core[pos..]));
    }
    // pair markup: a closing tag takes the nearest open one of the same name (per-name stacks keep this linear)
    let mut open: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for i in 0..pieces.len() {
        let (is_open, is_close, name) = match &pieces[i] {
            Err(Tok { shape: Some((s, n)), .. }) => (*s == Shape::Open, *s == Shape::Close, n.to_string()),
            _ => continue,
        };
        if is_open {
            open.entry(name).or_default().push(i);
        } else if is_close {
            if let Some(j) = open.get_mut(&name).and_then(Vec::pop) {
                if let Err(t) = &mut pieces[j] {
                    t.partner = Some(i);
                }
                if let Err(t) = &mut pieces[i] {
                    t.partner = Some(j);
                }
            }
        }
    }
    let mut inl = Vec::new();
    let mut codes = BTreeMap::new();
    let mut ids: BTreeMap<usize, String> = BTreeMap::new();
    let mut n = 0usize;
    for (i, p) in pieces.iter().enumerate() {
        match p {
            Ok(t) => inl.push(Inline::text(*t)),
            Err(t) => {
                let label = Some("placeholder".to_string());
                if let Some(j) = t.partner {
                    let (id, is_open) = match ids.get(&j) {
                        Some(id) => (id.clone(), false),
                        None => {
                            n += 1;
                            (n.to_string(), true)
                        }
                    };
                    if is_open {
                        ids.insert(i, id.clone());
                        let close = match &pieces[j] {
                            Err(c) => c.raw,
                            Ok(_) => "",
                        };
                        codes.insert(id.clone(), CodeData { open: Some(t.raw.to_string()), close: Some(close.to_string()), standalone: None, label });
                        inl.push(Inline::tag(id, TagKind::Open, t.raw));
                    } else {
                        inl.push(Inline::tag(id, TagKind::Close, t.raw));
                    }
                } else {
                    n += 1;
                    let id = n.to_string();
                    codes.insert(id.clone(), CodeData { standalone: Some(t.raw.to_string()), label, ..Default::default() });
                    inl.push(if t.shape.is_none() { Inline::free_tag(id, t.raw) } else { Inline::tag(id, TagKind::Standalone, t.raw) });
                }
            }
        }
    }
    (inl, codes)
}

/// Text of a segment with every tag written as its source spelling.
fn flat(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind, display, .. } => {
                let c = u.codes.get(id);
                let raw = match kind {
                    TagKind::Open => c.and_then(|c| c.open.as_deref()),
                    TagKind::Close => c.and_then(|c| c.close.as_deref()),
                    _ => c.and_then(|c| c.standalone.as_deref()),
                };
                s.push_str(raw.unwrap_or(display));
            }
        }
    }
    s
}

impl DtdFilter {
    /// `extract` with explicit size limits (`extract` uses `opts.limits()`).
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        let (text, encoding, entities) = read(bytes, limits)?;
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        for e in entities {
            let raw = &text[e.start..e.end];
            let value = unit_text(raw);
            let translatable = is_translatable(&value);
            if !translatable && (!opts.include_locked || value.trim_matches(scan::is_ws).is_empty()) {
                continue;
            }
            let (lead, core, trail) = split_ws(&value);
            guard::check_unit_chars(core.chars().take(limits.max_unit_chars.saturating_add(1)).count(), limits)?;
            if translatable {
                guard::check_unit_count(units.len() + 1, limits)?;
            }
            if e.start > pos {
                parts.push(SkelPart::Raw(text[pos..e.start].to_string()));
            }
            pos = e.end;
            let (inl, mut codes) = tokenize(core);
            let mut unit = Unit::new(format!("u{}", units.len() + 1), inl);
            codes.insert(
                HIDDEN.into(),
                CodeData { standalone: Some(raw.to_string()), open: owned(lead), close: owned(trail), label: Some(e.quote.to_string()) },
            );
            unit.codes = codes;
            unit.translate = translatable;
            unit.context.kind = Some("key".into());
            unit.context.key = Some(e.name);
            unit.context.notes = e.notes;
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        Ok(Document::new(ID, units, Skeleton::text(parts, encoding, bilingua_model::parser_info!())).with_options(opts))
    }
}

impl Filter for DtdFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["dtd"]
    }

    /// Strict: after the BOM, white space, comments and processing instructions the file must start with an
    /// `<!ENTITY`, `<!ELEMENT` or `<!ATTLIST` declaration (or the head ends inside a leading comment that is
    /// followed by `<!ENTITY` somewhere in it).
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(16384)];
        let body = head.strip_prefix(BOM8).unwrap_or(head);
        let text = match std::str::from_utf8(body) {
            Ok(t) => t,
            Err(e) if e.error_len().is_none() => std::str::from_utf8(&body[..e.valid_up_to()]).unwrap_or(""),
            Err(_) => return false,
        };
        if text.contains('\0') {
            return false;
        }
        let mut rest = text;
        loop {
            rest = rest.trim_start_matches(scan::is_ws);
            if let Some(r) = rest.strip_prefix("<!--") {
                match r.find("-->") {
                    Some(i) => rest = &r[i + 3..],
                    None => return text.contains("<!ENTITY"),
                }
            } else if let Some(r) = rest.strip_prefix("<?") {
                match r.find("?>") {
                    Some(i) => rest = &r[i + 2..],
                    None => return false,
                }
            } else {
                break;
            }
        }
        ["<!ENTITY", "<!ELEMENT", "<!ATTLIST"].iter().any(|p| rest.starts_with(p))
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &opts.limits())
    }

    fn merge_with(&self, doc: &Document, _opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        // UTF-8 in, UTF-8 out (BOM kept), so `output_encoding` has nothing to change; no language stamp in this format
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("dtd needs a text skeleton".into()));
        };
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    out.push_str(&render(unit));
                }
            }
        }
        match encoding.as_str() {
            "utf-8" => Ok(out.into_bytes()),
            "utf-8-bom" => {
                let mut v = BOM8.to_vec();
                v.extend_from_slice(out.as_bytes());
                Ok(v)
            }
            other => Err(Error::Write(format!("unknown encoding {other:?} in the dtd skeleton"))),
        }
    }
}

/// The raw value to write for a unit: the original spelling when nothing changed, else the translation escaped.
fn render(unit: &Unit) -> String {
    let Some(h) = unit.codes.get(HIDDEN) else { return String::new() };
    let raw = h.standalone.as_deref().unwrap_or("");
    if !unit.translate || unit.segments.iter().all(|s| !s.has_target()) {
        return raw.to_string();
    }
    let new = flat(unit, &unit.output());
    if new == flat(unit, &unit.source()) {
        return raw.to_string();
    }
    let quote = h.label.as_deref().and_then(|q| q.chars().next()).unwrap_or('"');
    encode_value(&format!("{}{}{}", h.open.as_deref().unwrap_or(""), new, h.close.as_deref().unwrap_or("")), quote)
}
