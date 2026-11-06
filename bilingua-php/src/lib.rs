//! Bilingua filter for PHP array resource files: Laravel `lang/*.php` (`return [...]`), CodeIgniter 4 and CodeIgniter 3
//! (`$lang['key'] = '...'`) language files. See the README.
//!
//! The file is tokenised, never executed. Every string value with a letter or digit is a unit, in source order; keys
//! never. Code, comments, numbers and symbol strings stay in the skeleton byte for byte, and only the literal of a
//! changed unit is rewritten, in the same quote style.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod lex;
mod parse;
mod ph;

use bilingua_model::*;

const ID: &str = "php";
/// The one hidden code of a unit: `standalone` = the original literal (quotes included, a concatenation whole),
/// `open` = the quote style (`'`, `"`, `h` heredoc, `n` nowdoc).
const HIDDEN: &str = "$php";

/// One translatable string as a key-based project sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Record {
    /// Dotted key path (`nested.deep.title`; a `$lang` variable is dropped).
    pub key: String,
    /// The decoded value.
    pub value: String,
    /// Index of the unit.
    pub unit_index: usize,
}

/// The PHP array resource filter.
pub struct Php;

struct Loaded {
    text: String,
    encoding: String,
    warnings: Vec<Warning>,
}

fn load(bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Loaded, Error> {
    guard::check_input(bytes, limits)?;
    let dec = enc::decode_text(
        bytes,
        &enc::DecodeOptions::with_limits(*limits).with_source_lang(opts.source_lang.as_deref()),
    )?;
    if !matches!(
        enc::base(&dec.encoding),
        "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be"
    ) && enc::encode_text(&dec.text, &dec.encoding)? != bytes
    {
        // double-mapped bytes of the CJK code pages read to the same character and write back as the other
        // spelling; an untouched merge must be the original bytes, so the file is refused, never altered
        return Err(Error::Encoding("This PHP file uses byte sequences of its legacy encoding that cannot be written back unchanged. Re-save the file as UTF-8.".into()));
    }
    budget(&dec.text, limits)?;
    Ok(Loaded {
        text: dec.text,
        encoding: dec.encoding,
        warnings: dec.warnings,
    })
}

/// LocOS `_budget`: refuse a file with far more strings or values than the unit limit could ever allow, before lexing.
fn budget(text: &str, limits: &Limits) -> Result<(), Error> {
    let lim = limits.max_units;
    let (mut quotes, mut values) = (0usize, 0usize);
    for &b in text.as_bytes() {
        match b {
            b'\'' | b'"' => quotes += 1,
            b',' | b'[' | b'(' => values += 1,
            _ => {}
        }
    }
    let over = |what: &str| -> Error {
        LimitError::new(
            LimitKind::Units,
            format!("This PHP file has far more {what} than {lim} segments allow. Split it into smaller files."),
        )
        .into()
    };
    if quotes > lim.saturating_mul(40) {
        return Err(over("strings"));
    }
    if values > lim.saturating_mul(20) {
        return Err(over("values"));
    }
    Ok(())
}

/// Every translatable string with its dotted key, in order (the LocOS `records()` view).
pub fn records(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    let opts = ExtractOptions::default();
    let limits = opts.limits();
    let l = load(bytes, &opts, &limits)?;
    let parsed = parse::parse(&l.text, &limits)?;
    Ok(parsed
        .units
        .iter()
        .filter(|u| u.translatable)
        .enumerate()
        .map(|(i, u)| Record {
            key: u.key.clone(),
            value: u.value.clone(),
            unit_index: i,
        })
        .collect())
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

/// The literal for `s`: double-quoted for a double-quoted original, single-quoted for everything else (LocOS).
fn escape(s: &str, double: bool) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    if double {
        out.push('"');
        for ch in s.chars() {
            match ch {
                '\\' | '"' | '$' => {
                    out.push('\\');
                    out.push(ch);
                }
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 || c as u32 == 0x7F => {
                    out.push_str(&format!("\\x{:02x}", c as u32))
                }
                c => out.push(c),
            }
        }
        out.push('"');
    } else {
        out.push('\'');
        for ch in s.chars() {
            if ch == '\\' || ch == '\'' {
                out.push('\\');
            }
            out.push(ch);
        }
        out.push('\'');
    }
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
    let double = hidden(u)?.open.as_deref() == Some("\"");
    Some(escape(&out, double))
}

impl Php {
    /// `extract` with explicit size limits.
    pub fn extract_with(
        &self,
        bytes: &[u8],
        opts: &ExtractOptions,
        limits: &Limits,
    ) -> Result<Document, Error> {
        let l = load(bytes, opts, limits)?;
        let text = l.text.as_str();
        let parsed = parse::parse(text, limits)?;
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        let (mut line_pos, mut line) = (0usize, 1usize);
        for ru in &parsed.units {
            if !(ru.translatable || opts.include_locked) {
                continue;
            }
            guard::check_unit_count(units.len() + 1, limits)?;
            if ru.value.len() > limits.max_unit_chars {
                guard::check_unit_chars(ru.value.chars().count(), limits)?;
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
            let (inl, codes) = ph::unit_inlines(&ru.value);
            let mut u = Unit::new(format!("u{}", units.len() + 1), inl);
            u.codes = codes;
            u.codes.insert(
                HIDDEN.into(),
                CodeData::new().with_open(ru.quote.as_char().to_string()).with_standalone(text[ru.start..ru.end].to_string()),
            );
            u.context.kind = Some("string".into());
            u.context.style = Some("php".into());
            u.context.key = Some(ru.key.clone());
            u.context.location = Some(format!("line {line}"));
            u.translate = ru.translatable;
            if !ru.translatable {
                u.lock_reason = Some("no letter or digit".into());
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(u);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let doc = Document::new(ID, units, Skeleton::text(parts, l.encoding, bilingua_model::parser_info!())).with_options(opts);
        guard::finish(doc, l.warnings, limits)
    }
}

impl Filter for Php {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["php"]
    }

    /// True when the whole file is a static PHP language file: it starts with `<?php`, parses, and holds at least one
    /// `return` or assignment statement. Tight enough to be put anywhere in the filter list.
    fn sniff(&self, bytes: &[u8]) -> bool {
        // cheap exit for every other format: UTF-16 files are decoded, anything else must open with `<?php`
        if !(bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF])) {
            let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
            let skip = b.iter().take_while(|c| c.is_ascii_whitespace()).count();
            if !b[skip..].get(..5).is_some_and(|h| h.eq_ignore_ascii_case(b"<?php")) {
                return false;
            }
        }
        let opts = ExtractOptions::default();
        let limits = opts.limits();
        let Ok(l) = load(bytes, &opts, &limits) else {
            return false;
        };
        parse::parse(&l.text, &limits).is_ok_and(|p| p.statements > 0)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        self.extract_with(bytes, opts, &opts.limits())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text {
            parts, encoding, ..
        } = &doc.skeleton
        else {
            return Err(Error::Write("php needs a text skeleton".into()));
        };
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc
                        .units
                        .get(*i)
                        .ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    match new_literal(u) {
                        Some(lit) => out.push_str(&lit),
                        None => out.push_str(
                            hidden(u).and_then(|h| h.standalone.as_deref()).ok_or_else(
                                || Error::Write(format!("unit {} lost its literal", u.id)),
                            )?,
                        ),
                    }
                }
            }
        }
        // The decoder takes the byte order mark off and the encoder takes one leading U+FEFF off the text before it
        // writes the mark back, so a U+FEFF that is file content (a file that starts with two marks) needs one more
        // in front of it to survive an untouched merge.
        if out.starts_with('\u{feff}') {
            out.insert(0, '\u{feff}');
        }
        enc::encode_text_with(&out, encoding, opts)
    }
}
