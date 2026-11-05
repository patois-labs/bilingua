//! Bilingua TOML filter. See the README.
//!
//! Every string value is a unit (`context.key` is its key path); keys, numbers, dates, comments and layout are
//! never touched. A translated value is spliced into the exact span of its original token, in the quoting
//! style the original used when the translation fits it.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod render;
mod scalar;
mod scan;

use bilingua_model::*;
use scan::{Kind, ScanError};

pub struct TomlFilter;

const ID: &str = "toml";
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
/// Hidden codes (never tag ids): the original token, the original value, and "kind lead newline".
const RAW: &str = "$raw";
const OV: &str = "$ov";
const META: &str = "$m";

/// Files larger than this are refused.
pub const MAX_BYTES: usize = 256 * 1024 * 1024;
/// More units than this are refused.
pub const MAX_UNITS: usize = 200_000;
/// A unit longer than this many characters is refused.
pub const MAX_UNIT_CHARS: usize = 100_000;

const PLURAL_KEYS: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];
const META_KEYS: [&str; 4] = ["id", "hash", "leftdelim", "rightdelim"];
const ACTION_MAX: usize = 500;

fn parse_err(msg: impl Into<String>) -> Error {
    Error::Parse { format: ID.into(), msg: msg.into() }
}

fn scan_err(e: ScanError) -> Error {
    match e {
        ScanError::Syntax(m) => parse_err(format!("this TOML file could not be read ({m})")),
        ScanError::Limit(m) => Error::Unsupported(m),
    }
}

/// UTF-8 text of a TOML file and whether it had a BOM.
fn decode(bytes: &[u8]) -> Result<(&str, bool), Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Unsupported("TOML files larger than 256 MB".into()));
    }
    let (body, bom) = match bytes.strip_prefix(BOM) {
        Some(r) => (r, true),
        None => (bytes, false),
    };
    if body.starts_with(&[0xFF, 0xFE]) || body.starts_with(&[0xFE, 0xFF]) || looks_binary(body) {
        return Err(Error::Unsupported("this TOML file is not UTF-8 text (TOML files must be UTF-8); save it as UTF-8".into()));
    }
    let text = std::str::from_utf8(body)
        .map_err(|e| Error::Unsupported(format!("this TOML file is not valid UTF-8 (byte {}); save it as UTF-8", e.valid_up_to())))?;
    // A line budget before any per-line work.
    let lines = body.iter().filter(|&&c| c == b'\n' || c == b'\r').count();
    if lines > 20 * MAX_UNITS {
        return Err(Error::Unsupported("this TOML file has far too many lines; split it".into()));
    }
    Ok((text, bom))
}

fn looks_binary(d: &[u8]) -> bool {
    if d.contains(&0) {
        return true;
    }
    let bad = d.iter().filter(|&&c| c < 0x20 && !matches!(c, 9..=13 | 0x1a | 0x1b)).count();
    bad > 2 + d.len() / 100
}

fn is_message(scan: &scan::Scan, t: usize) -> bool {
    let tb = &scan.tables[t];
    let mut any = false;
    for (k, s) in tb.members() {
        any = true;
        let l = k.to_lowercase();
        let reserved = META_KEYS.contains(&l.as_str()) || l == "description" || PLURAL_KEYS.contains(&l.as_str());
        if !(k == "translation" || (reserved && s.is_some())) {
            return false;
        }
    }
    any
}

/// Template actions `left ... right` (at most ACTION_MAX characters inside), as byte spans.
fn actions(v: &str, left: &str, right: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut pos = 0;
    while let Some(off) = v[pos..].find(left) {
        let a = pos + off;
        let inner = a + left.len();
        let window = ACTION_MAX + right.chars().count();
        let end = v[inner..].char_indices().nth(window).map_or(v.len(), |(i, _)| inner + i);
        match v[inner..end].find(right) {
            Some(b) => {
                let close = inner + b + right.len();
                spans.push((a, close));
                pos = close;
            }
            None => pos = a + v[a..].chars().next().map_or(1, char::len_utf8),
        }
    }
    spans
}

struct Class {
    unit: bool,
    style: String,
    message: bool,
}

fn classify(scan: &scan::Scan, s: &scan::Str) -> (Class, String, String) {
    // returns (class, left delim, right delim)
    let last = last_key(&s.key);
    let lkey = last.to_lowercase();
    let message = s.parent.is_some_and(|p| is_message(scan, p));
    let mut unit = true;
    if META_KEYS.contains(&lkey.as_str()) || (message && lkey == "description") {
        unit = false;
    }
    let style = if message && PLURAL_KEYS.contains(&lkey.as_str()) { format!("toml:{lkey}") } else { "toml".to_string() };
    let (mut ld, mut rd) = ("{{".to_string(), "}}".to_string());
    if let Some(p) = s.parent {
        let t = &scan.tables[p];
        if let (Some(l), Some(r)) = (t.string("leftdelim"), t.string("rightdelim")) {
            let (l, r) = (&scan.strings[l].value, &scan.strings[r].value);
            if !l.is_empty() && !r.is_empty() {
                ld = l.to_string();
                rd = r.to_string();
            }
        }
    }
    (Class { unit, style, message }, ld, rd)
}

/// The last key of a readable key path, "" when the path ends in an array index.
fn last_key(path: &str) -> String {
    if path.ends_with(']') {
        return String::new();
    }
    if path.ends_with('"') {
        // quoted: find the opening quote (an escaped quote is preceded by a backslash)
        let b = path.as_bytes();
        let mut i = b.len() - 1;
        while i > 0 {
            i -= 1;
            if b[i] == b'"' {
                let mut bs = 0;
                let mut j = i;
                while j > 0 && b[j - 1] == b'\\' {
                    bs += 1;
                    j -= 1;
                }
                if bs % 2 == 0 {
                    return unescape_basic(&path[i + 1..path.len() - 1]);
                }
            }
        }
        return String::new();
    }
    path.rsplit('.').next().unwrap_or("").to_string()
}

fn unescape_basic(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('b') => out.push('\u{8}'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('f') => out.push('\u{c}'),
            Some('r') => out.push('\r'),
            Some('u') => {
                let h: String = it.by_ref().take(4).collect();
                out.push(u32::from_str_radix(&h, 16).ok().and_then(char::from_u32).unwrap_or('\u{fffd}'));
            }
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

fn kind_char(k: Kind) -> char {
    match k {
        Kind::Basic => 'b',
        Kind::Literal => 'l',
        Kind::MlBasic => 'B',
        Kind::MlLiteral => 'L',
    }
}

fn kind_of(c: char) -> Kind {
    match c {
        'l' => Kind::Literal,
        'B' => Kind::MlBasic,
        'L' => Kind::MlLiteral,
        _ => Kind::Basic,
    }
}

impl Filter for TomlFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["toml"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let Ok((text, _)) = decode(bytes) else { return false };
        scan::scan(text).is_ok_and(|s| s.has_content)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let (text, bom) = decode(bytes)?;
        let scan = scan::scan(text).map_err(scan_err)?;
        let nl = match text.find('\n') {
            Some(i) if i > 0 && text.as_bytes()[i - 1] == b'\r' => 'r',
            _ => 'n',
        };

        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        for s in &scan.strings {
            let (cls, ld, rd) = classify(&scan, s);
            let (mut inl, mut codes) = (Vec::new(), std::collections::BTreeMap::new());
            let (mut bare_alnum, mut p) = (false, 0usize);
            let v: &str = &s.value;
            for (n, (a, b)) in actions(v, &ld, &rd).into_iter().enumerate() {
                inl.push(Inline::text(&v[p..a]));
                bare_alnum |= v[p..a].chars().any(char::is_alphanumeric);
                let id = (n + 1).to_string();
                codes.insert(id.clone(), CodeData { standalone: Some(v[a..b].to_string()), label: Some("placeholder".into()), ..Default::default() });
                inl.push(Inline::tag(id, TagKind::Standalone, &v[a..b]));
                p = b;
            }
            inl.push(Inline::text(&v[p..]));
            bare_alnum |= v[p..].chars().any(char::is_alphanumeric);
            let inl = merge_text(inl);
            let translatable = cls.unit && bare_alnum;
            if !translatable && (!opts.include_locked || v.trim().is_empty()) {
                continue;
            }
            if units.len() >= MAX_UNITS {
                return Err(Error::Unsupported(format!("this TOML file has more than {MAX_UNITS} translatable strings; split it")));
            }
            if v.chars().count() > MAX_UNIT_CHARS {
                return Err(Error::Unsupported(format!("a string in this TOML file is longer than {MAX_UNIT_CHARS} characters")));
            }
            if s.start > pos {
                parts.push(SkelPart::Raw(text[pos..s.start].to_string()));
            }
            pos = s.end;
            let mut u = Unit::new(format!("u{}", units.len() + 1), inl);
            u.codes = codes;
            u.codes.insert(RAW.into(), CodeData { standalone: Some(text[s.start..s.end].to_string()), ..Default::default() });
            u.codes.insert(OV.into(), CodeData { standalone: Some(v.to_string()), ..Default::default() });
            let lead = match s.lead {
                "\r\n" => 'r',
                "\n" => 'n',
                _ => '-',
            };
            u.codes.insert(META.into(), CodeData { standalone: Some(format!("{}{}{}", kind_char(s.kind), lead, nl)), ..Default::default() });
            u.context.kind = Some("string".into());
            u.context.style = Some(cls.style);
            u.context.key = Some(s.key.clone());
            if cls.message {
                if let Some(d) = s.parent.and_then(|p| scan.tables[p].string("description")) {
                    u.context.notes.push(scan.strings[d].value.to_string());
                }
            }
            u.translate = translatable;
            parts.push(SkelPart::Unit(units.len()));
            units.push(u);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        Ok(Document {
            format: ID.into(),
            name: None,
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(parts, if bom { "utf-8-bom" } else { "utf-8" }, bilingua_model::parser_info!()),
        })
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("toml needs a text skeleton".into()));
        };
        let mut out = String::new();
        let mut edited = false;
        let mut placed: Vec<(usize, usize)> = Vec::new(); // (output offset, unit index)
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    placed.push((out.len(), *i));
                    let (token, changed) = render_unit(u);
                    edited |= changed;
                    out.push_str(&token);
                }
            }
        }
        if edited {
            verify(&out, &placed, doc)?;
        }
        let mut bytes = Vec::with_capacity(out.len() + 3);
        if encoding == "utf-8-bom" {
            bytes.extend_from_slice(BOM);
        }
        bytes.extend_from_slice(out.as_bytes());
        Ok(bytes)
    }
}

fn code(u: &Unit, key: &str) -> Option<String> {
    u.codes.get(key).and_then(|c| c.standalone.clone())
}

/// The token to write for a unit, and whether it differs from the original.
fn render_unit(u: &Unit) -> (String, bool) {
    let raw = code(u, RAW).unwrap_or_default();
    if u.segments.iter().all(|s| s.target.is_empty()) {
        return (raw, false);
    }
    let mut value = String::new();
    for i in u.output() {
        match i {
            Inline::Text(t) => value.push_str(&t),
            Inline::Tag { id, .. } => {
                if let Some(c) = u.codes.get(&id).and_then(|c| c.standalone.as_deref()) {
                    value.push_str(c);
                }
            }
        }
    }
    if Some(&value) == code(u, OV).as_ref() {
        return (raw, false);
    }
    let meta: Vec<char> = code(u, META).unwrap_or_default().chars().collect();
    let kind = kind_of(*meta.first().unwrap_or(&'b'));
    let lead = match meta.get(1) {
        Some('r') => "\r\n",
        Some('n') => "\n",
        _ => "",
    };
    let nl = if meta.get(2) == Some(&'r') { "\r\n" } else { "\n" };
    (render::render(kind, lead, &value, nl), true)
}

/// The rebuilt file must still be valid TOML and hold, at every unit's place, the same key and the value the
/// unit wrote. Never ship a broken file.
fn verify(out: &str, placed: &[(usize, usize)], doc: &Document) -> Result<(), Error> {
    let bad = |m: &str| Error::Write(format!("the translated TOML file could not be rebuilt safely ({m})"));
    let sc = scan::scan(out).map_err(|e| bad(&match e {
        ScanError::Syntax(m) | ScanError::Limit(m) => m,
    }))?;
    for (at, ui) in placed {
        let u = &doc.units[*ui];
        let Ok(ix) = sc.strings.binary_search_by_key(at, |s| s.start) else { return Err(bad("value position")) };
        let s = &sc.strings[ix];
        if Some(&s.key) != u.context.key.as_ref() {
            return Err(bad("key path"));
        }
        let want = if u.segments.iter().all(|s| s.target.is_empty()) {
            code(u, OV).unwrap_or_default()
        } else {
            let mut v = String::new();
            for i in u.output() {
                match i {
                    Inline::Text(t) => v.push_str(&t),
                    Inline::Tag { id, .. } => {
                        if let Some(c) = u.codes.get(&id).and_then(|c| c.standalone.as_deref()) {
                            v.push_str(c);
                        }
                    }
                }
            }
            v.replace("\r\n", "\n")
        };
        if s.value != want {
            return Err(bad("value"));
        }
    }
    Ok(())
}

/// Key paths of all units, in order.
pub fn keys(doc: &Document) -> Vec<String> {
    doc.units.iter().filter_map(|u| u.context.key.clone()).collect()
}
