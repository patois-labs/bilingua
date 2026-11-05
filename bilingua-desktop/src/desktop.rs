//! The freedesktop Desktop Entry filter (Desktop Entry Specification 1.5).
//!
//! Units: the unlocalized `Name`, `GenericName`, `Comment` and `Keywords` values of `[Desktop Entry]` and every
//! `[Desktop Action <id>]` group (a `Keywords` list gives one unit per item). Localized lines (`Name[de]=`), every
//! other key, comments and every other group stay in the skeleton byte for byte.
//!
//! Values follow the spec's escapes: `\s`, `\n`, `\t`, `\r`, `\\` and, inside a list, `\;`. A unit holds the
//! unescaped text. Merge:
//! * with a target language (`Document::target_lang`) and at least one translated unit, a `Key[ll_CC]=value` line
//!   goes after each source key that has a translation, or replaces the file's own line for that locale;
//! * without a target language the changed values are spliced into the source lines (a leading or trailing
//!   space is written as `\s`);
//! * with nothing translated the original bytes come back.
//!
//! The skeleton keeps the text around each value; every unit has one hidden code `$desktop` holding the raw
//! (escaped) item of the source.

use crate::text::{py_strip, py_ws};
use crate::{locale::posix_locale, Record};
use bilingua_model::guard;
use bilingua_model::*;
use std::collections::HashMap;

const ID: &str = "desktop";
const HIDDEN: &str = "$desktop";
const TRANSLATABLE_KEYS: [&str; 4] = ["Name", "GenericName", "Comment", "Keywords"];

/// The `.desktop` filter.
pub struct DesktopFilter;

pub(crate) struct Entry<'a> {
    pub group: &'a str,
    pub key: &'a str,
    /// Byte offset just past the line content (before the line break).
    pub line_end: usize,
    pub eol: &'a str,
    pub v_start: usize,
    pub v_end: usize,
    pub raw_items: Vec<&'a str>,
    pub trailing: bool,
    pub comments: Vec<&'a str>,
}

impl Entry<'_> {
    fn in_list(&self) -> bool {
        self.key == "Keywords"
    }
}

pub(crate) struct Scan<'a> {
    pub entries: Vec<Entry<'a>>,
    /// `(entry index, item index)` per unit, in file order.
    pub units: Vec<(usize, usize)>,
    /// `(group, key, locale)` to the first such line's span.
    pub localized: HashMap<(&'a str, &'a str, &'a str), (usize, usize)>,
}

fn translatable_group(g: &str) -> bool {
    g == "Desktop Entry" || g.starts_with("Desktop Action ")
}

/// The spec's unescape; an unknown escape stays as written.
pub(crate) fn unescape(raw: &str) -> String {
    if !raw.contains('\\') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut it = raw.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(&n) = it.peek() {
                it.next();
                match n {
                    's' => out.push(' '),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '\\' => out.push('\\'),
                    ';' => out.push(';'),
                    other => {
                        out.push('\\');
                        out.push(other);
                    }
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Splits a list value on unescaped `;`. Returns the raw items and whether the value ended with `;`.
fn split_list(raw: &str) -> (Vec<&str>, bool) {
    let b = raw.as_bytes();
    let (mut items, mut start, mut i) = (Vec::new(), 0usize, 0usize);
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            // skip the escaped character (it may be multi-byte: only the first byte is looked at, and no
            // continuation byte equals a backslash or semicolon)
            i += 2;
            continue;
        }
        if b[i] == b';' {
            items.push(&raw[start..i]);
            start = i + 1;
        }
        i += 1;
    }
    let tail = &raw[start..];
    let trailing = tail.is_empty() && !items.is_empty();
    if !trailing {
        items.push(tail);
    }
    (items, trailing)
}

/// The escaped form of a translation as a value (`in_list`: inside a `Keywords` list).
pub(crate) fn escape(s: &str, in_list: bool) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            ';' if in_list => out.push_str("\\;"),
            c => out.push(c),
        }
    }
    // readers strip white space around a value or list item: keep edge spaces as \s
    let lead = out.len() - out.trim_start_matches(' ').len();
    let mut res = "\\s".repeat(lead);
    res.push_str(&out[lead..]);
    let trail = res.len() - res.trim_end_matches(' ').len();
    if trail > 0 {
        res.truncate(res.len() - trail);
        res.push_str(&"\\s".repeat(trail));
    }
    res
}

/// `Key` or `Key[locale]` as the spec's key grammar, `(key, locale)`.
fn parse_key(s: &str) -> Option<(&str, Option<&str>)> {
    let kend = s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).unwrap_or(s.len());
    if kend == 0 {
        return None;
    }
    let (key, rest) = s.split_at(kend);
    if rest.is_empty() {
        return Some((key, None));
    }
    let inner = rest.strip_prefix('[')?.strip_suffix(']')?;
    if inner.is_empty() || inner.contains(|c: char| c == ']' || py_ws(c)) {
        return None;
    }
    Some((key, Some(inner)))
}

fn first_eol(text: &str) -> &'static str {
    match text.find('\n') {
        Some(i) if i > 0 && text.as_bytes()[i - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

pub(crate) fn scan<'a>(text: &'a str, limits: &Limits) -> Result<Scan<'a>, Error> {
    scan_inner(text, limits)
}

fn scan_inner<'a>(text: &'a str, limits: &Limits) -> Result<Scan<'a>, Error> {
    let mut entries: Vec<Entry<'a>> = Vec::new();
    let mut units: Vec<(usize, usize)> = Vec::new();
    let mut localized = HashMap::new();
    let mut group: Option<&str> = None;
    let mut seen_desktop_entry = false;
    let mut comments: Vec<&str> = Vec::new();
    let mut pos = 0usize;
    while pos < text.len() {
        let end = text[pos..].find('\n').map_or(text.len(), |i| pos + i + 1);
        let line = &text[pos..end];
        let eol = if line.ends_with("\r\n") {
            "\r\n"
        } else if line.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        let content = &line[..line.len() - eol.len()];
        let start = pos;
        pos = end;
        let stripped = py_strip(content);
        if stripped.is_empty() {
            comments.clear();
            continue;
        }
        if let Some(c) = stripped.strip_prefix('#') {
            comments.push(py_strip(c));
            continue;
        }
        if stripped.starts_with('[') && stripped.ends_with(']') {
            let g = &stripped[1..stripped.len() - 1];
            group = Some(g);
            seen_desktop_entry |= g == "Desktop Entry";
            comments.clear();
            continue;
        }
        let note = std::mem::take(&mut comments);
        let Some(eq) = content.find('=') else { continue };
        let Some(g) = group.filter(|g| translatable_group(g)) else { continue };
        let Some((key, loc)) = parse_key(py_strip(&content[..eq])) else { continue };
        if let Some(loc) = loc {
            localized.entry((g, key, loc)).or_insert((start, start + content.len()));
            continue;
        }
        if !TRANSLATABLE_KEYS.contains(&key) {
            continue;
        }
        let rest = &content[eq + 1..];
        let core = rest.trim_matches([' ', '\t']);
        let v_start = start + eq + 1 + (rest.len() - rest.trim_start_matches([' ', '\t']).len());
        let v_end = v_start + core.len();
        let (raw_items, trailing) = if key == "Keywords" { split_list(core) } else { (vec![core], false) };
        let e_idx = entries.len();
        for (i, raw) in raw_items.iter().enumerate() {
            if !unescape(raw).trim_matches(py_ws).is_empty() {
                units.push((e_idx, i));
                guard::check_unit_count(units.len(), limits)?;
            }
        }
        entries.push(Entry { group: g, key, line_end: start + content.len(), eol, v_start, v_end, raw_items, trailing, comments: note });
    }
    if !seen_desktop_entry {
        return Err(Error::Parse { format: ID.into(), msg: "it has no [Desktop Entry] group".into() });
    }
    Ok(Scan { entries, units, localized })
}

fn unit_key(e: &Entry, item: usize) -> String {
    let idx = if e.in_list() { format!("[{item}]") } else { String::new() };
    format!("{}/{}{}", e.group, e.key, idx)
}

impl DesktopFilter {
    /// Extracts with the size limits of `opts`.
    fn extract_inner(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        let rd = crate::text::read(bytes, opts, ".desktop")?;
        let text = rd.text.as_str();
        let sc = scan(text, &limits)?;
        let unit_at: HashMap<(usize, usize), usize> = sc.units.iter().enumerate().map(|(n, p)| (*p, n)).collect();
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::with_capacity(sc.units.len());
        let mut last = 0usize;
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (e_idx, e) in sc.entries.iter().enumerate() {
            if !(0..e.raw_items.len()).any(|i| unit_at.contains_key(&(e_idx, i))) {
                continue;
            }
            parts.push(SkelPart::Raw(text[last..e.v_start].to_string()));
            for (i, raw) in e.raw_items.iter().enumerate() {
                if i > 0 {
                    parts.push(SkelPart::Raw(";".into()));
                }
                match unit_at.get(&(e_idx, i)) {
                    Some(&n) => {
                        let mut unit = Unit::new(format!("u{}", n + 1), vec![Inline::text(unescape(raw))]);
                        unit.codes.insert(HIDDEN.into(), CodeData { standalone: Some((*raw).to_string()), open: None, close: None, label: e.in_list().then(|| "list".to_string()) });
                        unit.context.kind = Some("key".into());
                        let k = unit_key(e, i);
                        unit.context.notes = e.comments.iter().filter(|c| !c.is_empty()).map(|c| c.to_string()).collect();
                        unit.context.extra.insert("group".into(), e.group.to_string());
                        if !seen.insert(k.clone()) {
                            unit.context.extra.insert("duplicate".into(), "true".into());
                        }
                        unit.context.key = Some(k);
                        parts.push(SkelPart::Unit(units.len()));
                        units.push(unit);
                    }
                    None => parts.push(SkelPart::Raw((*raw).to_string())),
                }
            }
            if e.trailing {
                parts.push(SkelPart::Raw(";".into()));
            }
            last = e.v_end;
        }
        parts.push(SkelPart::Raw(text[last..].to_string()));
        let doc = Document::new(ID, units, Skeleton::text(parts, rd.encoding, bilingua_model::parser_info!())).with_options(opts);
        guard::finish(doc, rd.warnings, &limits)
    }
}

/// `true` when the sniffed head has a `[Desktop Entry]` line.
fn head_has_group(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    let body = head.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(head);
    let text = match std::str::from_utf8(body) {
        Ok(t) => t,
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&body[..e.valid_up_to()]).unwrap_or(""),
        Err(_) => return false,
    };
    text.lines().any(|l| py_strip(l) == "[Desktop Entry]")
}

fn translated_text(u: &Unit) -> Option<String> {
    if !u.translate || u.segments.iter().all(|s| !s.has_target()) {
        return None;
    }
    Some(plain_text(&u.output()))
}

impl Filter for DesktopFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["desktop"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        head_has_group(bytes)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_inner(bytes, opts)
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        if doc.format != ID {
            return Err(Error::Write(format!("a {} document cannot be merged by the desktop filter", doc.format)));
        }
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("desktop needs a text skeleton".into()));
        };
        let tr: Vec<Option<String>> = doc.units.iter().map(translated_text).collect();
        let any = tr.iter().any(Option::is_some);
        let loc = opts.stamp_lang(doc, any).map(posix_locale).filter(|l| !l.is_empty());
        let get = |i: usize| doc.units.get(i).ok_or_else(|| Error::Write(format!("no unit {i}")));
        let raw_of = |u: &Unit| u.codes.get(HIDDEN).and_then(|c| c.standalone.clone()).unwrap_or_default();
        let in_list = |u: &Unit| u.codes.get(HIDDEN).and_then(|c| c.label.as_deref()) == Some("list");

        // The original text, and the text with changed values spliced in.
        let mut orig = String::new();
        let mut spliced = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => {
                    orig.push_str(s);
                    spliced.push_str(s);
                }
                SkelPart::Unit(i) => {
                    let u = get(*i)?;
                    let raw = raw_of(u);
                    orig.push_str(&raw);
                    match &tr[*i] {
                        Some(t) if *t != unescape(&raw) => spliced.push_str(&escape(t, in_list(u))),
                        _ => spliced.push_str(&raw),
                    }
                }
            }
        }
        let out = match &loc {
            Some(loc) => localize(&orig, doc, &tr, loc)?,
            _ => spliced,
        };
        bilingua_model::enc::encode_text_with(&out, encoding, opts)
    }
}

/// Adds or replaces `Key[loc]=value` for every entry that has a translated unit.
fn localize(orig: &str, doc: &Document, tr: &[Option<String>], loc: &str) -> Result<String, Error> {
    let sc = scan(orig, &Limits::new().with_max_units(usize::MAX).with_max_lines(usize::MAX))?;
    if sc.units.len() != doc.units.len() {
        return Err(Error::Write("the skeleton does not match the units".into()));
    }
    let mut item_unit: HashMap<(usize, usize), usize> = HashMap::with_capacity(sc.units.len());
    for (n, p) in sc.units.iter().enumerate() {
        item_unit.insert(*p, n);
    }
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for (e_idx, e) in sc.entries.iter().enumerate() {
        let mut touched = false;
        let mut parts: Vec<String> = Vec::with_capacity(e.raw_items.len());
        for (i, raw) in e.raw_items.iter().enumerate() {
            match item_unit.get(&(e_idx, i)).and_then(|&u| tr[u].as_ref()) {
                Some(t) => {
                    touched = true;
                    parts.push(escape(t, e.in_list()));
                }
                None => parts.push((*raw).to_string()),
            }
        }
        if !touched {
            continue;
        }
        let value = if e.in_list() { parts.join(";") + if e.trailing { ";" } else { "" } } else { parts.swap_remove(0) };
        let line = format!("{}[{}]={}", e.key, loc, value);
        if let Some(&(s, en)) = sc.localized.get(&(e.group, e.key, loc)) {
            edits.push((s, en, line));
        } else {
            let eol = if e.eol.is_empty() { first_eol(orig) } else { e.eol };
            if e.eol.is_empty() {
                edits.push((e.line_end, e.line_end, format!("{eol}{line}")));
            } else {
                let at = e.line_end + e.eol.len();
                edits.push((at, at, format!("{line}{eol}")));
            }
        }
    }
    edits.sort_by_key(|(s, e, _)| (*s, *e));
    let mut out = String::with_capacity(orig.len() + edits.len() * 40);
    let mut last = 0usize;
    for (s, e, rep) in edits {
        if s > last {
            out.push_str(&orig[last..s]);
        }
        out.push_str(&rep);
        last = last.max(e);
    }
    out.push_str(&orig[last..]);
    Ok(out)
}

/// Every translatable value as a key-based project sees it.
pub fn records(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    let opts = ExtractOptions::default();
    let rd = crate::text::read(bytes, &opts, ".desktop")?;
    let sc = scan(&rd.text, &opts.limits())?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(sc.units.len());
    for (n, &(e_idx, i)) in sc.units.iter().enumerate() {
        let e = &sc.entries[e_idx];
        let key = unit_key(e, i);
        let duplicate = !seen.insert(key.clone());
        out.push(Record {
            key,
            value: unescape(e.raw_items[i]),
            comment: e.comments.join("\n"),
            context: e.group.to_string(),
            unit_index: Some(n),
            duplicate,
        });
    }
    Ok(out)
}
