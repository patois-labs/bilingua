//! Bilingua filter for JavaScript and TypeScript locale modules (`.js .cjs .mjs .ts .mts .cts`): files whose only
//! content is a static message object (vue-i18n, i18next resources, hand-rolled translation tables). See the README.
//!
//! The file is tokenised, never executed. Every string value with a letter or digit outside its placeholders is a
//! unit, in source order; keys never. Code, types, comments, numbers and untranslatable strings stay in the
//! skeleton byte for byte, and only the literal of a changed unit is rewritten, in its own quote style.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod enc;
mod lex;
mod parse;
mod ph;

use bilingua_model::*;

const ID: &str = "js";
/// The one hidden code of a unit: `standalone` = the original literal (quotes included, a concatenation whole),
/// `open` = the quote character, `close` = "1" when the literal is a multi-line template.
const HIDDEN: &str = "$js";

/// Size limits (the Localization OS defaults). `bilingua-model` has no limits in this tree, so the crate caps itself.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// String values in one file (and the base of the token, quote and line budgets).
    pub max_units: usize,
    /// Characters in one value.
    pub max_value_chars: usize,
    /// Bytes in one file.
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_units: 50_000,
            max_value_chars: 100_000,
            max_bytes: 128 * 1024 * 1024,
        }
    }
}

/// Why a file was not read.
#[derive(Debug)]
pub(crate) enum ModuleError {
    /// Not a static locale module (code, a variable, a spread...), with the line.
    Refused(String),
    /// Over a size limit.
    Limit(String),
}

impl From<ModuleError> for Error {
    fn from(e: ModuleError) -> Error {
        match e {
            ModuleError::Refused(msg) => Error::Parse {
                format: ID.into(),
                msg,
            },
            ModuleError::Limit(msg) => Error::Unsupported(msg),
        }
    }
}

/// One translatable string as a key-based project sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Dotted key path (the default export's own name is dropped).
    pub key: String,
    /// The decoded value.
    pub value: String,
    /// Index of the unit.
    pub unit_index: usize,
}

/// The JavaScript / TypeScript locale module filter.
pub struct JsModule;

/// A file that is XML (a Qt Linguist `.ts` file) is not a JavaScript module.
fn is_xml(data: &[u8]) -> bool {
    let mut head: Vec<u8> = data[..data.len().min(512)].to_vec();
    for bom in [&[0xEF, 0xBB, 0xBF][..], &[0xFF, 0xFE], &[0xFE, 0xFF]] {
        if head.starts_with(bom) {
            head = head[bom.len()..]
                .iter()
                .copied()
                .filter(|&b| b != 0)
                .collect();
            break;
        }
    }
    let first = head
        .iter()
        .find(|&&b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'));
    first == Some(&b'<')
}

struct Loaded {
    text: String,
    encoding: &'static str,
}

fn load(bytes: &[u8], limits: &Limits) -> Result<Loaded, Error> {
    if bytes.len() > limits.max_bytes {
        return Err(Error::Unsupported(format!(
            "this file is larger than {} bytes; split it into smaller files",
            limits.max_bytes
        )));
    }
    if is_xml(bytes) {
        return Err(Error::Parse {
            format: ID.into(),
            msg: "This file is XML (a Qt Linguist .ts file?), not a JavaScript/TypeScript locale module.".into(),
        });
    }
    let (text, encoding) = enc::decode(bytes)?;
    let lim = limits.max_units;
    let (mut lines, mut quotes, mut values) = (0usize, 0usize, 0usize);
    for &b in text.as_bytes() {
        match b {
            b'\n' | b'\r' => lines += 1,
            b'\'' | b'"' | b'`' => quotes += 1,
            b',' | b'{' | b'[' | b'(' => values += 1,
            _ => {}
        }
    }
    if lines > 20 * lim {
        return Err(Error::Unsupported(format!(
            "this file has more than {} lines; split it into smaller files",
            20 * lim
        )));
    }
    if quotes > 40 * lim {
        return Err(Error::Unsupported(format!("this file has far more strings than the limit allows ({lim}); split it into smaller files")));
    }
    if values > 20 * lim {
        return Err(Error::Unsupported(format!("this file has far more values than the limit allows ({lim}); split it into smaller files")));
    }
    Ok(Loaded { text, encoding })
}

fn key_of(path: &str, root: &str, default_ref: Option<&str>) -> String {
    if let Some(d) = default_ref {
        if !root.is_empty() && root == d {
            if path == root {
                return String::new();
            }
            if let Some(rest) = path.strip_prefix(root).and_then(|r| r.strip_prefix('.')) {
                return rest.to_string();
            }
        }
    }
    path.to_string()
}

/// Every translatable string with its dotted key, in order (the LocOS `records()` view).
pub fn records(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    let limits = Limits::default();
    let l = load(bytes, &limits)?;
    let parsed = parse::parse(&l.text, &limits)?;
    let mut out = Vec::new();
    for ru in &parsed.units {
        let (inl, _) = ph::unit_inlines(&ru.value);
        if ph::translatable(&inl) {
            out.push(Record {
                key: key_of(&ru.path, &ru.root, parsed.default_ref.as_deref()),
                value: ru.value.clone(),
                unit_index: out.len(),
            });
        }
    }
    Ok(out)
}

/// The text a list of inline pieces writes (tags give back their original placeholder).
fn out_text(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, .. } => {
                if let Some(c) = u.codes.get(id).and_then(|c| c.standalone.as_deref()) {
                    s.push_str(c);
                }
            }
        }
    }
    s
}

/// The literal for `s` in the given quote style: escaped for that quote (LocOS rules).
fn escape(s: &str, quote: char, multiline: bool) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    let mut it = s.chars().peekable();
    while let Some(ch) = it.next() {
        match ch {
            '\\' => out.push_str("\\\\"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            '$' if quote == '`' && it.peek() == Some(&'{') => out.push_str("\\$"),
            '\n' => out.push_str(if quote == '`' && multiline {
                "\n"
            } else {
                "\\n"
            }),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' | '\u{2029}' => out.push_str(&format!("\\u{:04x}", ch as u32)),
            c if (c as u32) < 0x20 || c as u32 == 0x7F => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

fn hidden(u: &Unit) -> Option<&CodeData> {
    u.codes.get(HIDDEN)
}

/// The literal to write for a unit: `None` when it is unchanged (the original literal stays).
fn new_literal(u: &Unit) -> Option<String> {
    if u.segments.iter().all(|s| !s.has_target()) {
        return None;
    }
    let out = out_text(u, &u.output());
    if out == out_text(u, &u.source()) {
        return None;
    }
    let h = hidden(u)?;
    let quote = h
        .open
        .as_deref()
        .and_then(|q| q.chars().next())
        .unwrap_or('\'');
    Some(escape(&out, quote, h.close.as_deref() == Some("1")))
}

impl JsModule {
    /// `extract` with explicit size limits.
    pub fn extract_with(
        &self,
        bytes: &[u8],
        opts: &ExtractOptions,
        limits: &Limits,
    ) -> Result<Document, Error> {
        let l = load(bytes, limits)?;
        let text = l.text.as_str();
        let parsed = parse::parse(text, limits)?;
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        let (mut line_pos, mut line) = (0usize, 1usize);
        for ru in &parsed.units {
            let (inl, codes) = ph::unit_inlines(&ru.value);
            let translatable = ph::translatable(&inl);
            if !(translatable || opts.include_locked && !ru.value.trim().is_empty()) {
                continue;
            }
            if ru.value.len() > limits.max_value_chars
                && ru.value.chars().count() > limits.max_value_chars
            {
                return Err(Error::Unsupported(format!(
                    "this file has a string longer than {} characters; break it into shorter strings",
                    limits.max_value_chars
                )));
            }
            if ru.start > pos {
                parts.push(SkelPart::Raw(text[pos..ru.start].to_string()));
            }
            pos = ru.end;
            line += text.as_bytes()[line_pos..ru.start]
                .iter()
                .filter(|&&b| b == b'\n')
                .count();
            line_pos = ru.start;
            let mut u = Unit::new(format!("u{}", units.len() + 1), inl);
            u.codes = codes;
            u.codes.insert(
                HIDDEN.into(),
                CodeData {
                    standalone: Some(text[ru.start..ru.end].to_string()),
                    open: Some((ru.quote as char).to_string()),
                    close: Some(if ru.multiline { "1" } else { "0" }.into()),
                    ..Default::default()
                },
            );
            u.context.kind = Some("string".into());
            u.context.style = Some("js".into());
            u.context.key = Some(key_of(&ru.path, &ru.root, parsed.default_ref.as_deref()));
            u.context.location = Some(format!("line {line}"));
            u.translate = translatable;
            parts.push(SkelPart::Unit(units.len()));
            units.push(u);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        Ok(Document::new(ID, units, Skeleton::text(parts, l.encoding, bilingua_model::parser_info!())).with_options(opts))
    }
}

impl Filter for JsModule {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["js", "cjs", "mjs", "ts", "mts", "cts"]
    }

    /// True when the whole file has the locale-module shape. XML (a Qt Linguist `.ts`) is refused, and so is a file
    /// with no export or declaration at all (an empty or prose file).
    fn sniff(&self, bytes: &[u8]) -> bool {
        let limits = Limits::default();
        let Ok(l) = load(bytes, &limits) else {
            return false;
        };
        parse::parse(&l.text, &limits).is_ok_and(|p| p.statements > 0)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &Limits::default())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text {
            parts, encoding, ..
        } = &doc.skeleton
        else {
            return Err(Error::Write("js needs a text skeleton".into()));
        };
        let mut out = String::new();
        let mut changed = false;
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc
                        .units
                        .get(*i)
                        .ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    match new_literal(u) {
                        Some(lit) => {
                            changed = true;
                            out.push_str(&lit);
                        }
                        None => {
                            out.push_str(
                                hidden(u).and_then(|h| h.standalone.as_deref()).ok_or_else(
                                    || Error::Write(format!("unit {} lost its literal", u.id)),
                                )?,
                            )
                        }
                    }
                }
            }
        }
        // Like LocOS: a file with nothing changed comes back exactly as it was read; a changed file is written as
        // UTF-8 (the byte order mark is kept when the source had one).
        if let Some(b) = bilingua_model::enc::utf8_output(&out, encoding, opts) {
            return Ok(b);
        }
        if !changed {
            return enc::encode(&out, encoding);
        }
        enc::encode(
            &out,
            if encoding == enc::UTF8_BOM {
                enc::UTF8_BOM
            } else {
                enc::UTF8
            },
        )
    }
}
