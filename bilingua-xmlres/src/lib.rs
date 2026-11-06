//! Bilingua XML software resources that arrive as plain `.xml`: Android `strings.xml` and Java XML properties
//! (Localization OS `app/filters/xml_resource.py`, routed by root element). See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod cldr;
mod cldr_table;
mod scan;

pub use bilingua_model::Limits;

use bilingua_model::enc::{self, DecodeOptions};
use bilingua_model::guard;
use bilingua_model::*;
use scan::{Dialect, Kind, Piece};
use std::collections::BTreeMap;

/// Android string resources (`res/values*/strings.xml`: `<string>`, `<plurals>`, `<string-array>`).
pub struct AndroidStrings;

/// Java XML properties (`java.util.Properties.storeToXML`: `<properties>` with `<entry key>`).
pub struct JavaXmlProperties;

// Reserved hidden keys (never a numeric tag id).
const RAW: &str = "raw"; // the original inner XML
const LEAD: &str = "lead"; // white space the shown text was cut of, before
const TRAIL: &str = "trail"; // and after
const CDATA: &str = "cdata"; // the value held a CDATA section
const QUOTED: &str = "quoted"; // Android: the whole value was in double quotes
const ESC_NL: &str = "escnl"; // Android: the source writes line breaks as \n escapes and has no raw one (#706)
const MSGFMT: &str = "msgfmt"; // Java: the value has MessageFormat arguments ({0}, {1,date})

/// Android: the text a translator sees for a raw text piece (`\'` `\"` `\@` `\?` unescaped; `\n`, `\t`, `\uXXXX`
/// and `\\` kept as typed).
fn android_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(&n) = it.peek() {
                it.next();
                if matches!(n, '\'' | '"' | '@' | '?') {
                    out.push(n);
                } else {
                    out.push('\\');
                    out.push(n);
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Android: a translated text piece escaped for aapt2: `'` and `"` get a backslash, an existing backslash pair
/// passes through, a lone trailing backslash is doubled, and a leading `@` or `?` of the whole value is escaped so
/// it is not read as a reference.
fn android_escape(s: &str, first: bool) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.next() {
                Some(n) => {
                    out.push('\\');
                    out.push(n);
                }
                None => out.push_str("\\\\"),
            },
            '\'' | '"' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    if first && (out.starts_with('@') || out.starts_with('?')) {
        out.insert(0, '\\');
    }
    out
}

/// #706: the engine often answers a source `\n` escape with a real line break, which aapt2 reads as one space. When
/// the source writes its breaks as `\n` and has no raw break, every raw break (with the spaces around it, and an
/// escape the engine kept right before it) is written as `\n`.
fn escape_raw_breaks(s: &str) -> String {
    if !s.contains('\n') {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let mut start = 0;
    while i < b.len() {
        // a candidate: optional `\n` escape, spaces or tabs, optional CR, LF, spaces or tabs
        let mut j = i;
        if b[j] == b'\\' && b.get(j + 1) == Some(&b'n') {
            j += 2;
        }
        while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
            j += 1;
        }
        if j < b.len() && b[j] == b'\r' && b.get(j + 1) == Some(&b'\n') {
            j += 1;
        }
        if j < b.len() && b[j] == b'\n' {
            j += 1;
            while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
                j += 1;
            }
            out.push_str(&s[start..i]);
            out.push_str("\\n");
            i = j;
            start = j;
            continue;
        }
        // No break starts anywhere in what the scan passed over (an escape and spaces not followed by a break), so
        // go on after it: one pass, also over a long run of spaces. `j` sits on an ASCII byte's end, a char boundary.
        i = if j > i { j } else { i + s[i..].chars().next().map_or(1, char::len_utf8) };
    }
    out.push_str(&s[start..]);
    out
}

/// Java MessageFormat argument: `{` digits optional spaces, then `,` or `}`.
fn has_msgfmt_arg(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while let Some(p) = b[i..].iter().position(|c| *c == b'{') {
        let mut j = i + p + 1;
        while j < b.len() && py_isspace_char(b[j] as char) && b[j].is_ascii() {
            j += 1;
        }
        let d0 = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > d0 {
            while j < b.len() && b[j].is_ascii() && py_isspace_char(b[j] as char) {
                j += 1;
            }
            if j < b.len() && (b[j] == b',' || b[j] == b'}') {
                return true;
            }
        }
        i = i + p + 1;
    }
    false
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

fn text_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

struct Parsed {
    text: String,
    enc: String,
    warnings: Vec<Warning>,
    scan: scan::Scan,
}

fn parse(d: Dialect, bytes: &[u8], opts: &ExtractOptions) -> Result<Parsed, Error> {
    let l = opts.limits();
    guard::check_input(bytes, &l)?;
    let dec = enc::decode_text(bytes, &DecodeOptions::new(opts).declared())?;
    guard::check_xml(dec.text.as_bytes(), &l)?;
    let scan = scan::scan(&dec.text, d)?;
    Ok(Parsed { text: dec.text, enc: dec.encoding, warnings: dec.warnings, scan })
}

/// What a slot shows the translator: inline pieces with the dialect's unescaping, the quoted flag (Android).
fn shown(d: Dialect, slot: &scan::Slot) -> (Vec<Piece>, bool) {
    let mut pieces = slot.pieces.clone();
    if d == Dialect::JavaProps {
        if has_msgfmt_arg(&first_text(&slot.pieces)) {
            for p in pieces.iter_mut() {
                if let Piece::Text(t) = p {
                    *t = t.replace("''", "'");
                }
            }
        }
        return (pieces, false);
    }
    // Android: a value wholly in double quotes (the quotes are dropped and spaces kept)
    let mut quoted = false;
    let n = pieces.len();
    if let (Some(Piece::Text(a)), Some(Piece::Text(z))) = (pieces.first(), pieces.last()) {
        let (a_st, z_st) = (a.trim_start_matches(py_isspace_char), z.trim_end_matches(py_isspace_char));
        let whole: usize = if n == 1 { a_st.trim_end_matches(py_isspace_char).chars().count() } else { 2 };
        if whole >= 2 && a_st.starts_with('"') && z_st.ends_with('"') && !z_st.ends_with("\\\"") {
            quoted = true;
        }
    }
    if quoted {
        if n == 1 {
            if let Piece::Text(t) = &mut pieces[0] {
                let st = t.trim_matches(py_isspace_char);
                *t = st[1..st.len() - 1].to_string();
            }
        } else {
            if let Piece::Text(t) = &mut pieces[0] {
                let st = t.trim_start_matches(py_isspace_char);
                *t = st[1..].to_string();
            }
            if let Piece::Text(t) = &mut pieces[n - 1] {
                let st = t.trim_end_matches(py_isspace_char);
                *t = st[..st.len() - 1].to_string();
            }
        }
    }
    for p in pieces.iter_mut() {
        if let Piece::Text(t) = p {
            *t = android_unescape(t);
        }
    }
    (pieces, quoted)
}

fn first_text(p: &[Piece]) -> String {
    match p.first() {
        Some(Piece::Text(t)) => t.clone(),
        _ => String::new(),
    }
}

fn raw_text(p: &[Piece]) -> String {
    p.iter().filter_map(|x| if let Piece::Text(t) = x { Some(t.as_str()) } else { None }).collect()
}

/// Pieces to inline content and codes; the lead and trail white space are cut off the outer text pieces.
fn to_inline(mut pieces: Vec<Piece>) -> (Vec<Inline>, BTreeMap<String, CodeData>, String, String) {
    let mut lead = String::new();
    let mut trail = String::new();
    if let Some(Piece::Text(t)) = pieces.first_mut() {
        let rest = t.trim_start_matches(py_isspace_char).to_string();
        lead = t[..t.len() - rest.len()].to_string();
        *t = rest;
    }
    if let Some(Piece::Text(t)) = pieces.last_mut() {
        let keep = t.trim_end_matches(py_isspace_char).len();
        trail = t[keep..].to_string();
        t.truncate(keep);
    }
    let mut inl = Vec::new();
    let mut codes = BTreeMap::new();
    let mut n = 0usize;
    let mut open: Vec<String> = Vec::new();
    for p in pieces {
        match p {
            Piece::Text(t) if t.is_empty() => {}
            Piece::Text(t) => inl.push(Inline::text(t)),
            Piece::Open(raw) => {
                n += 1;
                let id = n.to_string();
                codes.insert(id.clone(), CodeData::new().with_open(raw.clone()));
                inl.push(Inline::tag(id.clone(), TagKind::Open, raw));
                open.push(id);
            }
            Piece::Close(raw) => {
                let id = open.pop().unwrap_or_default();
                if let Some(c) = codes.get_mut(&id) {
                    c.close = Some(raw.clone());
                }
                inl.push(Inline::tag(id, TagKind::Close, if raw.is_empty() { "</>".to_string() } else { raw }));
            }
            Piece::Point(raw) => {
                n += 1;
                let id = n.to_string();
                codes.insert(id.clone(), CodeData::new().with_standalone(raw.clone()));
                inl.push(Inline::tag(id, TagKind::Standalone, raw));
            }
        }
    }
    (inl, codes, lead, trail)
}

fn extract(d: Dialect, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let limits = &opts.limits();
    let p = parse(d, bytes, opts)?;
    let mut units: Vec<Unit> = Vec::new();
    let mut parts: Vec<SkelPart> = Vec::new();
    let mut pos = 0usize;
    let (mut n_tr, mut n_lock) = (0usize, 0usize);
    let (mut line, mut line_at) = (1usize, 0usize);
    for s in &p.scan.slots {
        let Some((a, b)) = s.inner else { continue };
        // Android: a value that is a resource or theme reference (@string/x, ?attr/x) is not text
        let reference = d == Dialect::Android
            && !s.pieces.iter().any(|x| !matches!(x, Piece::Text(_)))
            && first_text(&s.pieces).trim_start_matches(py_isspace_char).starts_with(['@', '?']);
        let (pieces, quoted) = shown(d, s);
        let has_tag = pieces.iter().any(|x| !matches!(x, Piece::Text(_)));
        let (inl, codes, lead, trail) = to_inline(pieces);
        let core_text: String = inl.iter().filter_map(|i| if let Inline::Text(t) = i { Some(t.as_str()) } else { None }).collect();
        // LocOS counts its token text (`⟦g1⟧`) as letters, so a value with markup is always translatable
        let has_text = py_has_alnum(&core_text) || has_tag;
        let translatable = !s.skip && !reference && has_text;
        if !translatable && (!opts.include_locked || inl.is_empty()) {
            continue;
        }
        guard::check_unit_count(units.len() + 1, limits)?;
        let mut unit = if translatable {
            n_tr += 1;
            Unit::new(format!("u{n_tr}"), inl).with_codes(codes)
        } else {
            n_lock += 1;
            let mut u = Unit::new(format!("k{n_lock}"), inl).with_codes(codes);
            u.translate = false;
            u.lock_reason = Some(if s.skip { "translatable=\"false\"".into() } else if reference { "resource reference".into() } else { "no translatable text".into() });
            u
        };
        unit.set_hidden(RAW, &p.text[a..b]);
        if !lead.is_empty() {
            unit.set_hidden(LEAD, lead);
        }
        if !trail.is_empty() {
            unit.set_hidden(TRAIL, trail);
        }
        if s.cdata {
            unit.set_hidden(CDATA, "1");
        }
        if quoted {
            unit.set_hidden(QUOTED, "1");
        }
        let raw = raw_text(&s.pieces);
        if d == Dialect::Android && raw.contains("\\n") && !raw.contains('\n') && !raw.contains('\r') {
            unit.set_hidden(ESC_NL, "1");
        }
        if d == Dialect::JavaProps && has_msgfmt_arg(&first_text(&s.pieces)) {
            unit.set_hidden(MSGFMT, "1");
        }
        line += p.text.as_bytes()[line_at..s.start].iter().filter(|c| **c == b'\n').count();
        line_at = s.start;
        let (kind, extra) = match &s.kind {
            Kind::String => ("string", None),
            Kind::Plural(q) => ("plurals", Some(("plural", q.clone()))),
            Kind::Array(i) => ("string-array", Some(("index", i.to_string()))),
            Kind::Entry => ("entry", None),
        };
        unit.context.kind = Some(kind.into());
        unit.context.key = Some(s.name.clone());
        unit.context.location = Some(format!("line {line}"));
        if let Some((k, v)) = extra {
            unit.context.extra.insert(k.into(), v);
        }
        if !s.comment.is_empty() {
            unit.context.notes = vec![s.comment.clone()];
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
    let doc = Document::new(d.id(), units, Skeleton::text(parts, p.enc, bilingua_model::parser_info!())).with_options(opts);
    guard::finish(doc, p.warnings, limits)
}

/// One resource entry as a key-based project sees it (Localization OS `ResourceRecord`): every entry of the file in
/// order, translatable or not.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Record {
    /// The resource name (Android `name`, Java `key`).
    pub key: String,
    /// The text a translator sees, in LocOS's token text for inline markup, with its outer white space and without
    /// the escapes a build puts back (Android `\'`, a value wholly in double quotes).
    pub value: String,
    /// The comment right before the resource (Android).
    pub comment: String,
    /// The CLDR quantity of an Android `<plurals>` item, else empty.
    pub plural: String,
    /// The position of an Android `<string-array>` item.
    pub index: Option<usize>,
    /// A length limit; neither dialect has one, so always `None` (kept for LocOS parity).
    pub max_length: Option<usize>,
    /// Neither dialect has a context; always empty (kept for LocOS parity).
    pub context: String,
    /// Not `translatable="false"`, not a resource reference, and the value has a letter, a digit or markup.
    pub translatable: bool,
}

fn records_of(d: Dialect, bytes: &[u8]) -> Result<Vec<Record>, Error> {
    let p = parse(d, bytes, &ExtractOptions::default())?;
    let mut out = Vec::with_capacity(p.scan.slots.len());
    for s in &p.scan.slots {
        let reference = d == Dialect::Android
            && !s.pieces.iter().any(|x| !matches!(x, Piece::Text(_)))
            && first_text(&s.pieces).trim_start_matches(py_isspace_char).starts_with(['@', '?']);
        let (pieces, _quoted) = shown(d, s);
        let has_tag = pieces.iter().any(|x| !matches!(x, Piece::Text(_)));
        let (inl, _codes, lead, trail) = to_inline(pieces);
        let core_text: String = inl.iter().filter_map(|i| if let Inline::Text(t) = i { Some(t.as_str()) } else { None }).collect();
        let translatable = !s.skip && !reference && (py_has_alnum(&core_text) || has_tag);
        let value = format!("{lead}{}{trail}", profile::tokens::to_locos_text(&inl).0);
        let (plural, index) = match &s.kind {
            Kind::Plural(q) => (q.clone(), None),
            Kind::Array(i) => (String::new(), Some(*i)),
            _ => (String::new(), None),
        };
        out.push(Record { key: s.name.clone(), value, comment: s.comment.clone(), plural, index, max_length: None, context: String::new(), translatable });
    }
    Ok(out)
}

/// Every entry of an Android `strings.xml` or a Java XML properties file, chosen by the root element as LocOS's
/// `.xml` dispatcher does (`<resources>`, `<properties>`); any other root is `Error::Unsupported`.
pub fn records(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    if AndroidStrings.sniff(bytes) {
        records_of(Dialect::Android, bytes)
    } else if JavaXmlProperties.sniff(bytes) {
        records_of(Dialect::JavaProps, bytes)
    } else {
        Err(Error::Unsupported("This XML file is not a supported resource format. Supported .xml files: Android strings.xml (<resources>) and Java XML properties (<properties>).".into()))
    }
}

impl AndroidStrings {
    /// Every entry of the file in order (the key-based view); see [`Record`].
    pub fn records(&self, bytes: &[u8]) -> Result<Vec<Record>, Error> {
        records_of(Dialect::Android, bytes)
    }
}

impl JavaXmlProperties {
    /// Every entry of the file in order (the key-based view); see [`Record`].
    pub fn records(&self, bytes: &[u8]) -> Result<Vec<Record>, Error> {
        records_of(Dialect::JavaProps, bytes)
    }
}

/// The new inner XML of a translated unit, or `None` when the original is kept.
fn write_unit(d: Dialect, u: &Unit) -> Option<String> {
    if !u.translate || u.segments.iter().all(|s| !s.has_target()) {
        return None;
    }
    let out = u.output();
    if out == u.source() {
        return None;
    }
    // text and markup pieces of the translation
    enum P {
        T(String),
        M(String),
    }
    // The markup is repaired as LocOS's `_rebuild_target` does, so a translation never writes broken XML: an
    // unknown tag or a second use of one is dropped, a close with no open on top of the stack is dropped, and a
    // pair the translation left open is closed at the end.
    let mut ps: Vec<P> = Vec::new();
    let mut used: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut stack: Vec<&str> = Vec::new();
    for i in &out {
        match i {
            Inline::Text(t) => match ps.last_mut() {
                Some(P::T(s)) => s.push_str(t),
                _ => ps.push(P::T(t.clone())),
            },
            Inline::Tag { id, kind, .. } => {
                let Some((key, c)) = u.codes.get_key_value(id) else { continue };
                match kind {
                    TagKind::Open => {
                        if let Some(open) = c.open.as_ref().filter(|_| used.insert(key.as_str())) {
                            stack.push(key.as_str());
                            ps.push(P::M(open.clone()));
                        }
                    }
                    TagKind::Close => {
                        if stack.last() == Some(&key.as_str()) {
                            stack.pop();
                            ps.push(P::M(c.close.clone().unwrap_or_default()));
                        }
                    }
                    TagKind::Standalone => {
                        if let Some(s) = c.standalone.as_ref().filter(|_| used.insert(key.as_str())) {
                            ps.push(P::M(s.clone()));
                        }
                    }
                }
            }
        }
    }
    while let Some(key) = stack.pop() {
        ps.push(P::M(u.codes.get(key).and_then(|c| c.close.clone()).unwrap_or_default()));
    }
    if u.has_hidden(ESC_NL) {
        for p in ps.iter_mut() {
            if let P::T(t) = p {
                *t = escape_raw_breaks(t);
            }
        }
    }
    let lead = u.hidden(LEAD).unwrap_or("");
    let trail = u.hidden(TRAIL).unwrap_or("");
    match ps.first_mut() {
        Some(P::T(t)) => t.insert_str(0, lead),
        _ if !lead.is_empty() => ps.insert(0, P::T(lead.to_string())),
        _ => {}
    }
    match ps.last_mut() {
        Some(P::T(t)) => t.push_str(trail),
        _ if !trail.is_empty() => ps.push(P::T(trail.to_string())),
        _ => {}
    }
    let markup = ps.iter().any(|p| matches!(p, P::M(_)));
    let mut inner = String::new();
    for (i, p) in ps.iter().enumerate() {
        match p {
            P::M(m) => inner.push_str(m),
            P::T(t) => {
                let t = clean(t);
                let esc = match d {
                    Dialect::Android => android_escape(&t, i == 0),
                    Dialect::JavaProps if u.has_hidden(MSGFMT) => t.replace("''", "'").replace('\'', "''"),
                    Dialect::JavaProps => t,
                };
                if !markup && u.has_hidden(CDATA) && !esc.contains("]]>") {
                    inner.push_str("<![CDATA[");
                    inner.push_str(&esc);
                    inner.push_str("]]>");
                } else {
                    inner.push_str(&text_escape(&esc));
                }
            }
        }
    }
    if d == Dialect::Android && u.has_hidden(QUOTED) {
        inner = match inner.strip_prefix("<![CDATA[").and_then(|x| x.strip_suffix("]]>")) {
            Some(body) => format!("<![CDATA[\"{body}\"]]>"),
            None => format!("\"{inner}\""),
        };
    }
    Some(inner)
}

/// Android plural fill on a target-language merge: every `<plurals>` (not translatable="false") gets the CLDR
/// categories the language uses, filled with the `other` item's (translated) text, and loses the ones it never
/// uses (`other` is always kept). Works on the merged text, so the `other` item already carries its translation.
fn fill_plurals(text: &str, locale: &str) -> Result<String, Error> {
    let cats = cldr::categories(locale);
    let scan = scan::scan(text, Dialect::Android)?;
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for g in scan.groups.iter().filter(|g| !g.off && !g.items.is_empty()) {
        let Some(other) = g.items.iter().find(|i| i.quantity == "other") else { continue };
        for it in &g.items {
            if it.quantity != "other" && !cats.contains(&it.quantity.as_str()) {
                let mut s = it.start;
                while s > 0 && matches!(text.as_bytes()[s - 1], b' ' | b'\t' | b'\r' | b'\n') {
                    s -= 1;
                }
                edits.push((s, it.end, String::new()));
            }
        }
        let missing: Vec<&str> = cats.iter().copied().filter(|c| !g.items.iter().any(|i| i.quantity == *c)).collect();
        if missing.is_empty() {
            continue;
        }
        let inner = other.inner.map_or("", |(a, b)| &text[a..b]);
        let line_start = text[..other.start].rfind('\n').map_or(0, |p| p + 1);
        let mut indent = &text[line_start..other.start];
        let mut nl = if line_start >= 2 && &text[line_start - 2..line_start] == "\r\n" { "\r\n" } else { "\n" };
        if !indent.trim().is_empty() {
            indent = "";
            nl = "";
        }
        let last = g.items.last().expect("items");
        let add: String = missing.iter().map(|q| format!("{nl}{indent}<item quantity=\"{q}\">{inner}</item>")).collect();
        edits.push((last.end, last.end, add));
    }
    // one pass over the text (replacing ranges in place would move the tail once per edit)
    edits.sort_by_key(|a| (a.0, a.1));
    let mut out = String::with_capacity(text.len() + edits.iter().map(|e| e.2.len()).sum::<usize>());
    let mut pos = 0;
    for (s, e, r) in edits {
        let s = s.max(pos);
        out.push_str(&text[pos..s]);
        out.push_str(&r);
        pos = e.max(s);
    }
    out.push_str(&text[pos..]);
    Ok(out)
}

fn merge(d: Dialect, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write(format!("{} needs a text skeleton", d.id())));
    };
    let mut out = String::new();
    let mut changed = false;
    for p in parts {
        match p {
            SkelPart::Raw(s) => out.push_str(s),
            SkelPart::Unit(i) => {
                let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                match write_unit(d, u) {
                    Some(inner) => {
                        out.push_str(&inner);
                        changed = true;
                    }
                    None => out.push_str(u.hidden(RAW).unwrap_or("")),
                }
            }
        }
    }
    if d == Dialect::Android {
        if let Some(locale) = opts.stamp_lang(doc, changed) {
            out = fill_plurals(&out, locale)?;
            changed = true;
        }
    }
    finish_bytes(out, encoding, changed, opts)
}

/// The encoding named in the XML declaration: byte span of the name and the name.
fn declared(text: &str) -> Option<(usize, usize, String)> {
    let head_end = text.char_indices().nth(300).map_or(text.len(), |(i, _)| i);
    let head = &text[..head_end];
    let head = head.strip_prefix('\u{FEFF}').unwrap_or(head);
    let off = text.len() - text.strip_prefix('\u{FEFF}').unwrap_or(text).len();
    if !head.starts_with("<?xml") {
        return None;
    }
    let decl = &head[..head.find("?>")?];
    let k = decl.find("encoding")?;
    let rest = &decl[k + 8..];
    let after = &rest[rest.find('=')? + 1..];
    let q = after.find(['"', '\''])?;
    let quote = after.as_bytes()[q] as char;
    let vstart = decl.len() - after.len() + q + 1;
    let vlen = decl[vstart..].find(quote)?;
    Some((off + vstart, off + vstart + vlen, decl[vstart..vstart + vlen].to_string()))
}

fn finish_bytes(mut out: String, encoding: &str, changed: bool, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    // a UTF-8 file whose declaration names another charset says UTF-8 once non-ASCII text is written into it
    if changed && matches!(encoding, "utf-8" | "utf-8-bom") && !out.is_ascii() {
        if let Some((a, b, name)) = declared(&out) {
            if !matches!(name.trim().to_ascii_lowercase().replace('_', "-").as_str(), "utf-8" | "utf8") {
                out = format!("{}UTF-8{}", &out[..a], &out[b..]);
            }
        }
    }
    enc::encode_text_with(&out, encoding, opts)
}

/// The root element's local name from the first 8 KB (any encoding the decoder knows).
fn root_of(bytes: &[u8]) -> Option<String> {
    let head = &bytes[..bytes.len().min(8192)];
    let o = ExtractOptions::default();
    let d = DecodeOptions::new(&o);
    let t = enc::decode_text(head, &d).or_else(|_| enc::decode_text(&head[..head.len().saturating_sub(3)], &d)).ok()?;
    // cut at the first element's end so a file longer than the head still gives its root
    let text = t.text.trim_start_matches('\u{FEFF}');
    let mut s = text;
    loop {
        let t2 = s.trim_start();
        if let Some(r) = t2.strip_prefix("<?") {
            s = &r[r.find("?>")? + 2..];
        } else if let Some(r) = t2.strip_prefix("<!--") {
            s = &r[r.find("-->")? + 3..];
        } else if t2.starts_with("<!DOCTYPE") || t2.starts_with("<!doctype") {
            // skip a DOCTYPE, with an internal subset if any
            let r = &t2[9..];
            let end = match (r.find('['), r.find('>')) {
                (Some(o), Some(g)) if o < g => {
                    let c = o + r[o..].find(']')?;
                    c + r[c..].find('>')?
                }
                (_, Some(g)) => g,
                _ => return None,
            };
            s = &r[end + 1..];
        } else if let Some(r) = t2.strip_prefix('<') {
            let name: String = r.chars().take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/').collect();
            return Some(name.rsplit(':').next().unwrap_or("").to_string());
        } else {
            return None;
        }
    }
}

macro_rules! filter_impl {
    ($t:ty, $d:expr) => {
        impl $t {
            /// Like [`Filter::extract`] with explicit limits.
            pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
                extract($d, bytes, &opts.clone().with_limits(*limits))
            }
        }

        impl Filter for $t {
            fn id(&self) -> &'static str {
                $d.id()
            }

            /// No extension of its own: `.xml` belongs to the generic XML filter, and the facade hands a `.xml` file
            /// to this filter when its root element says so (as LocOS's `.xml` dispatcher).
            fn extensions(&self) -> &'static [&'static str] {
                &[]
            }

            fn sniff(&self, bytes: &[u8]) -> bool {
                let want = match $d {
                    Dialect::Android => "resources",
                    Dialect::JavaProps => "properties",
                };
                root_of(bytes).is_some_and(|r| r == want)
            }

            fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
                bilingua_model::check_own_options(self, &opts.format_options)?;
                extract($d, bytes, opts)
            }

            fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
                bilingua_model::check_own_options(self, &opts.format_options)?;
                merge($d, doc, opts)
            }
        }
    };
}

filter_impl!(AndroidStrings, Dialect::Android);
filter_impl!(JavaXmlProperties, Dialect::JavaProps);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_match_locos() {
        assert_eq!(android_unescape(r#"Can\'t open \"%s\" \n A \\ \@x"#), r#"Can't open "%s" \n A \\ @x"#);
        assert_eq!(android_escape("l'été \"x\"", false), r#"l\'été \"x\""#);
        assert_eq!(android_escape("@home", true), r"\@home");
        assert_eq!(android_escape("@home", false), "@home");
        assert_eq!(android_escape(r"a\nb\", true), r"a\nb\\");
        assert_eq!(escape_raw_breaks("Eins\\n\nZwei"), "Eins\\nZwei");
        assert_eq!(escape_raw_breaks("Noch %d Datei.\r\n  Weiter so."), "Noch %d Datei.\\nWeiter so.");
        assert_eq!(escape_raw_breaks("ohne"), "ohne");
        assert!(has_msgfmt_arg("Hello {0}, it''s {1,date,long}.") && has_msgfmt_arg("{ 1 }") && !has_msgfmt_arg("{name} {}"));
    }

    #[test]
    fn root_names() {
        assert_eq!(root_of(b"<?xml version=\"1.0\"?>\n<!-- c -->\n<resources>").as_deref(), Some("resources"));
        assert_eq!(root_of(b"<!DOCTYPE properties SYSTEM \"x.dtd\">\n<properties>").as_deref(), Some("properties"));
        assert_eq!(root_of(b"<!DOCTYPE r [<!ENTITY a \"b\">]><r:resources/>").as_deref(), Some("resources"));
        assert_eq!(root_of(b"\xef\xbb\xbf<catalog/>").as_deref(), Some("catalog"));
        assert_eq!(root_of(b"plain text"), None);
    }
}
