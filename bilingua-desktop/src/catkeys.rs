//! The Haiku `.catkeys` filter (plain-text catalog of the Haiku Locale Kit).
//!
//! Layout: UTF-8, one record per line, fields separated by TAB. The header is
//! `1<TAB>language<TAB>app signature<TAB>fingerprint` (the language is Haiku's lower-case English name, for example
//! `german` or `portuguese (brazil)`); an entry is `source<TAB>context<TAB>comment<TAB>translation`. Source,
//! comment and translation are backslash-escaped.
//!
//! Units: the unescaped SOURCE of every entry that has a letter. The translation column is what a merge writes
//! (escaped with `\\`, `\n`, `\t`, `\"` and `\r`); the source, context, comment and every untouched line stay byte
//! for byte. A merge with a target language and at least one translation also sets the header language. The
//! fingerprint is kept as it is: Haiku derives it from the source strings only, and translating never changes
//! those ([`catkeys_fingerprint`] recomputes it).
//!
//! Skeleton: `parts[0]` is the header up to its language field, `parts[1]` the language, then the text up to each
//! translation field with the unit in its place. A unit's hidden code `$catkeys` holds the raw translation.

use crate::text::py_strip;
use crate::{locale::language_name, Record};
use bilingua_model::guard;
use bilingua_model::*;

const ID: &str = "catkeys";
const HIDDEN: &str = "$catkeys";

/// The `.catkeys` filter.
pub struct CatkeysFilter;

struct Entry<'a> {
    source: String,
    context: &'a str,
    comment: String,
    raw_translation: &'a str,
    tr_start: usize,
    tr_end: usize,
}

struct Walk<'a> {
    header: [&'a str; 4],
    lang_start: usize,
    lang_end: usize,
    entries: Vec<Entry<'a>>,
}

/// Haiku's `parseQuotedChars`: `\a \b \f \n \r \t \v \"` and `\xNN`; any other escaped character stands for itself.
pub fn unescape(raw: &str) -> String {
    if !raw.contains('\\') {
        return raw.to_string();
    }
    let b = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if c == b'\\' && i + 1 < b.len() {
            let nxt = b[i + 1];
            if nxt == b'x' {
                if i + 3 < b.len() && b[i + 2].is_ascii_hexdigit() && b[i + 3].is_ascii_hexdigit() {
                    out.push(u8::from_str_radix(&raw[i + 2..i + 4], 16).unwrap_or(0));
                    i += 4;
                    continue;
                }
                out.push(b'x');
                i += 2;
                continue;
            }
            match nxt {
                b'a' => out.push(7),
                b'b' => out.push(8),
                b'f' => out.push(12),
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'v' => out.push(11),
                b'"' => out.push(b'"'),
                _ => {
                    // any other escaped character stands for itself (it may be multi-byte)
                    let ch_len = raw[i + 1..].chars().next().map_or(1, char::len_utf8);
                    out.extend_from_slice(&b[i + 1..i + 1 + ch_len]);
                    i += 1 + ch_len;
                    continue;
                }
            }
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Haiku's `escapeQuotedChars` (`\\`, `\n`, `\t`, `\"`) plus `\r`, so a translation can never break the line.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '"' => out.push_str("\\\""),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

fn hash(s: &str, mut h: u32) -> u32 {
    for b in s.bytes() {
        h = h.wrapping_mul(5).wrapping_add(b as i8 as i32 as u32);
    }
    h.wrapping_mul(5).wrapping_add(1)
}

fn key_hash(source: &str, context: &str, comment: &str) -> u32 {
    hash(comment, hash(context, hash(source, 0)))
}

fn parse_err(msg: impl Into<String>) -> Error {
    Error::Parse { format: ID.into(), msg: msg.into() }
}

fn walk<'a>(text: &'a str, limits: &Limits) -> Result<(Walk<'a>, Vec<usize>), Error> {
    let n = text.len();
    let mut pos = 0usize;
    let mut header: Option<([&str; 4], usize, usize)> = None;
    let mut entries: Vec<Entry<'a>> = Vec::new();
    let mut units: Vec<usize> = Vec::new();
    let mut line_no = 0usize;
    while pos < n {
        let end = text[pos..].find('\n').map_or(n, |i| pos + i);
        let content_end = if end > pos && text.as_bytes()[end - 1] == b'\r' { end - 1 } else { end };
        let line = &text[pos..content_end];
        line_no += 1;
        if header.is_none() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() != 4 || py_strip(f[0]) != "1" {
                return Err(parse_err("the first line must be '1<TAB>language<TAB>signature<TAB>fingerprint'"));
            }
            let fp = py_strip(f[3]);
            if fp.is_empty() || !fp.chars().all(|c| c.is_ascii_digit()) {
                return Err(parse_err("the header fingerprint is not a number"));
            }
            let ls = pos + f[0].len() + 1;
            header = Some(([f[0], f[1], f[2], f[3]], ls, ls + f[1].len()));
        } else if !line.is_empty() {
            let f: Vec<&str> = line.splitn(4, '\t').collect();
            if f.len() != 4 {
                return Err(parse_err(format!("line {line_no} does not have four TAB-separated fields (source, context, comment, translation)")));
            }
            let tr_start = pos + line.len() - f[3].len();
            let e = Entry { source: unescape(f[0]), context: f[1], comment: unescape(f[2]), raw_translation: f[3], tr_start, tr_end: content_end };
            if bilingua_model::py_has_alpha(&e.source) {
                units.push(entries.len());
                guard::check_unit_count(units.len(), limits)?;
            }
            entries.push(e);
        }
        pos = end + 1;
    }
    let Some((h, lang_start, lang_end)) = header else {
        return Err(parse_err("the file is empty"));
    };
    Ok((Walk { header: h, lang_start, lang_end, entries }, units))
}

/// `(fingerprint declared in the header, fingerprint recomputed from the entries the way Haiku's
/// `HashMapCatalog::ComputeFingerprint` does)`.
pub fn catkeys_fingerprint(bytes: &[u8]) -> Result<(u32, u32), Error> {
    let opts = ExtractOptions::default();
    let rd = crate::text::read(bytes, &opts, ".catkeys")?;
    let (w, _) = walk(&rd.text, &opts.limits())?;
    let total = w.entries.iter().fold(0u32, |t, e| t.wrapping_add(key_hash(&e.source, e.context, &e.comment)));
    let declared = py_strip(w.header[3]).parse::<u64>().map_or(u32::MAX, |v| v as u32);
    Ok((declared, total))
}

impl CatkeysFilter {
    fn extract_inner(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        let rd = crate::text::read(bytes, opts, ".catkeys")?;
        let text = rd.text.as_str();
        let (w, alpha_units) = walk(text, &limits)?;
        let mut parts: Vec<SkelPart> = vec![SkelPart::Raw(text[..w.lang_start].to_string()), SkelPart::Raw(text[w.lang_start..w.lang_end].to_string())];
        let mut units: Vec<Unit> = Vec::with_capacity(alpha_units.len());
        let mut last = w.lang_end;
        let mut next_alpha = 0usize;
        for (i, e) in w.entries.iter().enumerate() {
            let translatable = alpha_units.get(next_alpha) == Some(&i);
            if translatable {
                next_alpha += 1;
            }
            if !translatable && !opts.include_locked {
                continue;
            }
            parts.push(SkelPart::Raw(text[last..e.tr_start].to_string()));
            last = e.tr_end;
            let mut unit = Unit::new(format!("u{}", units.len() + 1), vec![Inline::text(e.source.clone())]);
            unit.codes.insert(HIDDEN.into(), CodeData { standalone: Some(e.raw_translation.to_string()), open: None, close: None, label: None });
            unit.translate = translatable;
            if !translatable {
                unit.lock_reason = Some("no letter in the source".into());
            }
            unit.context.kind = Some("string".into());
            unit.context.key = Some(e.source.clone());
            if !e.comment.is_empty() {
                unit.context.notes = vec![e.comment.clone()];
            }
            if !e.context.is_empty() {
                unit.context.extra.insert("context".into(), e.context.to_string());
            }
            let cur = unescape(e.raw_translation);
            if !cur.is_empty() {
                unit.context.extra.insert("translation".into(), cur);
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
        }
        parts.push(SkelPart::Raw(text[last..].to_string()));
        let doc = Document {
            format: ID.into(),
            name: None,
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(parts, rd.encoding, bilingua_model::parser_info!()),
            ..Default::default()
        };
        guard::finish(doc, rd.warnings, &limits)
    }
}

impl Filter for CatkeysFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["catkeys"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(4096)];
        let head = head.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(head);
        let line = head.split(|&b| b == b'\n').next().unwrap_or(&[]);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let f: Vec<&[u8]> = line.split(|&b| b == b'\t').collect();
        f.len() == 4
            && f[0] == b"1"
            && !f[1].is_empty()
            && f[1].iter().all(|b| b.is_ascii_alphabetic() || matches!(b, b' ' | b'(' | b')' | b'-' | b',' | b'.'))
            && !f[3].is_empty()
            && f[3].iter().all(u8::is_ascii_digit)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_inner(bytes, opts)
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        if doc.format != ID {
            return Err(Error::Write(format!("a {} document cannot be merged by the catkeys filter", doc.format)));
        }
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("catkeys needs a text skeleton".into()));
        };
        let translated = |u: &Unit| u.translate && u.segments.iter().any(|s| !s.target.is_empty());
        let any = doc.units.iter().any(translated);
        let lang = opts.stamp_lang(doc, any).map(language_name).filter(|l| !l.is_empty());
        let mut out = String::new();
        for (idx, p) in parts.iter().enumerate() {
            match p {
                SkelPart::Raw(s) if idx == 1 => match &lang {
                    Some(l) if l != s => out.push_str(l),
                    _ => out.push_str(s),
                },
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    let raw = u.codes.get(HIDDEN).and_then(|c| c.standalone.as_deref()).unwrap_or("");
                    if translated(u) {
                        let new = plain_text(&u.output());
                        if new != unescape(raw) {
                            out.push_str(&escape(&new));
                            continue;
                        }
                    }
                    out.push_str(raw);
                }
            }
        }
        bilingua_model::enc::encode_text_with(&out, encoding, opts)
    }
}

/// Every entry of the catalog (units or not) as a key-based project sees it.
pub fn records(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    let opts = ExtractOptions::default();
    let rd = crate::text::read(bytes, &opts, ".catkeys")?;
    let (w, alpha) = walk(&rd.text, &opts.limits())?;
    let mut unit_of = std::collections::HashMap::with_capacity(alpha.len());
    for (k, e) in alpha.iter().enumerate() {
        unit_of.insert(*e, k);
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(w.entries.len());
    for (i, e) in w.entries.iter().enumerate() {
        let duplicate = !seen.insert((e.source.as_str(), e.context, e.comment.as_str()));
        out.push(Record {
            key: e.source.clone(),
            value: unescape(e.raw_translation),
            comment: e.comment.clone(),
            context: e.context.to_string(),
            unit_index: unit_of.get(&i).copied(),
            duplicate,
        });
    }
    Ok(out)
}
