//! The LocOS profile's PO filter (Track D, X3 and X5): a port of Localization OS's `po_filter.py`, so a document
//! read and written under the profile has LocOS's units and LocOS's bytes.
//!
//! Units: each message's `msgid` and, for a plural message, its `msgid_plural` (two units whatever the target's
//! plural count), as plain text (placeholders stay text). The header and every message without a msgid are not
//! units. Build writes a translation into `msgstr` (or `msgstr[0]` / `msgstr[1]`), drops `fuzzy` and the `#|`
//! previous-string lines of a message that got one, and writes an entry nothing changed as its own lines.
//! A localized build (a target language is set) also stamps `Language`, the target's `Plural-Forms` and a UTF-8
//! charset in the header (a header is added when the file has none), and fills every plural slot of the target's
//! rule. The skeleton is the decoded file itself (`Skeleton::Binary`, UTF-8), which marks a profile document.

use bilingua_model::plural_table::RULES;
use bilingua_model::*;
use std::collections::BTreeMap;

/// One entry as LocOS's `_parse_entries` reads it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Fields {
    comments: Vec<String>,
    msgctxt: Option<String>,
    msgid: String,
    msgid_plural: Option<String>,
    msgstr: Option<String>,
    slots: BTreeMap<usize, String>,
    has_kw: bool,
}

#[derive(Debug, Clone)]
struct PEntry {
    f: Fields,
    msgid_ok: bool,
    span: Option<(usize, usize)>,
    was: Option<Fields>,
}

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Ctxt,
    Plural,
    Id,
    Str,
    Slot(usize),
}

fn py_lstrip(s: &str) -> &str {
    bilingua_model::py_lstrip(s)
}

/// LocOS `_unescape`: a backslash and the character after it, left to right; a trailing lone backslash stays.
fn unescape(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(o) => out.push(o),
            None => out.push('\\'),
        }
    }
    out
}

/// LocOS `_escape`.
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\t', "\\t").replace('\r', "\\r")
}

/// LocOS `_qval`: the text between the first and the last quote, unescaped.
fn qval(line: &str) -> String {
    match (line.find('"'), line.rfind('"')) {
        (Some(a), Some(b)) if b > a => unescape(&line[a + 1..b]),
        _ => String::new(),
    }
}

/// LocOS `_kw`.
fn kw(line: &str) -> Option<&'static str> {
    let s = py_lstrip(line);
    ["msgctxt", "msgid_plural", "msgid", "msgstr"].into_iter().find(|k| s.starts_with(k) && matches!(s[k.len()..].chars().next(), Some(' ' | '[' | '"')))
}

/// `\s*msgstr\[(\d+)\]` at the start of the line: the slot number.
fn slot_of(line: &str) -> Option<usize> {
    let rest = line.trim_start_matches(bilingua_model::py_isspace_char).strip_prefix("msgstr[")?;
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    (end > 0 && rest[end..].starts_with(']')).then(|| rest[..end].parse().ok()).flatten()
}

fn msgstr_done(f: &Fields) -> bool {
    f.msgstr.is_some() || !f.slots.is_empty()
}

fn field_mut(f: &mut Fields, k: Field) -> &mut String {
    match k {
        Field::Ctxt => f.msgctxt.get_or_insert_with(String::new),
        Field::Plural => f.msgid_plural.get_or_insert_with(String::new),
        Field::Id => &mut f.msgid,
        Field::Str => f.msgstr.get_or_insert_with(String::new),
        Field::Slot(i) => f.slots.entry(i).or_default(),
    }
}

/// LocOS `_parse_entries` over text whose line breaks are already `\n`.
fn parse_entries(text: &str) -> Vec<PEntry> {
    let mut entries = Vec::new();
    let mut cur = PEntry { f: Fields::default(), msgid_ok: false, span: None, was: None };
    let mut field: Option<Field> = None;
    let mut span: Vec<usize> = Vec::new();
    let flush = |cur: &mut PEntry, span: &mut Vec<usize>, entries: &mut Vec<PEntry>| {
        let mut e = std::mem::replace(cur, PEntry { f: Fields::default(), msgid_ok: false, span: None, was: None });
        if !e.f.comments.is_empty() || e.f.has_kw {
            if let (Some(a), Some(b)) = (span.first(), span.last()) {
                e.span = Some((*a, *b));
                e.was = Some(e.f.clone());
            }
            entries.push(e);
        }
        span.clear();
    };
    for (n, line) in text.split('\n').enumerate() {
        if bilingua_model::py_strip(line).is_empty() {
            flush(&mut cur, &mut span, &mut entries);
            field = None;
            continue;
        }
        let boundary = msgstr_done(&cur.f);
        if py_lstrip(line).starts_with('#') {
            if boundary {
                flush(&mut cur, &mut span, &mut entries);
                field = None;
            }
            cur.f.comments.push(line.to_string());
            span.push(n);
            continue;
        }
        let k = kw(line);
        if matches!(k, Some("msgctxt" | "msgid")) && boundary {
            flush(&mut cur, &mut span, &mut entries);
            field = None;
        }
        span.push(n);
        if k.is_some() {
            cur.f.has_kw = true;
        }
        match k {
            Some("msgctxt") => {
                cur.f.msgctxt = Some(qval(line));
                field = Some(Field::Ctxt);
            }
            Some("msgid_plural") => {
                cur.f.msgid_plural = Some(qval(line));
                field = Some(Field::Plural);
            }
            Some("msgid") => {
                cur.f.msgid = qval(line);
                cur.msgid_ok = matches!((line.find('"'), line.rfind('"')), (Some(a), Some(b)) if a < b);
                field = Some(Field::Id);
            }
            Some(_) => match slot_of(line) {
                Some(i) => {
                    cur.f.slots.insert(i, qval(line));
                    field = Some(Field::Slot(i));
                }
                None => {
                    cur.f.msgstr = Some(qval(line));
                    field = Some(Field::Str);
                }
            },
            None => {
                if let (true, Some(f)) = (py_lstrip(line).starts_with('"'), field) {
                    let v = qval(line);
                    field_mut(&mut cur.f, f).push_str(&v);
                }
            }
        }
    }
    flush(&mut cur, &mut span, &mut entries);
    entries
}

/// LocOS `_emit`.
fn emit(f: &Fields) -> Vec<String> {
    let mut lines = f.comments.clone();
    if !f.has_kw {
        return lines;
    }
    if let Some(c) = &f.msgctxt {
        lines.push(format!("msgctxt \"{}\"", escape(c)));
    }
    lines.push(format!("msgid \"{}\"", escape(&f.msgid)));
    if let Some(p) = &f.msgid_plural {
        lines.push(format!("msgid_plural \"{}\"", escape(p)));
        let idx: Vec<usize> = if f.slots.is_empty() { vec![0, 1] } else { f.slots.keys().copied().collect() };
        for i in idx {
            lines.push(format!("msgstr[{i}] \"{}\"", escape(f.slots.get(&i).map(String::as_str).unwrap_or(""))));
        }
    } else {
        lines.push(format!("msgstr \"{}\"", escape(f.msgstr.as_deref().unwrap_or(""))));
    }
    lines
}

/// `^(msgctxt|msgid_plural|msgid|msgstr(?:\[\d+\])?) "(.*)"$`: the keyword of an emitted line.
fn kw_line(line: &str) -> Option<&str> {
    let (k, rest) = line.split_once(' ')?;
    let ok = matches!(k, "msgctxt" | "msgid_plural" | "msgid" | "msgstr")
        || k.strip_prefix("msgstr[").and_then(|r| r.strip_suffix(']')).is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
    (ok && rest.len() >= 2 && rest.starts_with('"') && rest.ends_with('"')).then_some(k)
}

/// LocOS `_multiline`: a value with a line break inside it is written `kw ""` and one quoted line per piece.
fn multiline(emitted: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for line in emitted {
        let Some(k) = kw_line(&line) else {
            out.push(line);
            continue;
        };
        let value = qval(&line);
        let inner = &value[..value.len() - value.chars().next_back().map_or(0, char::len_utf8)];
        if !inner.contains('\n') {
            out.push(line);
            continue;
        }
        out.push(format!("{k} \"\""));
        let mut rest = value.as_str();
        while !rest.is_empty() {
            let cut = rest.find('\n').map_or(rest.len(), |p| p + 1);
            out.push(format!("\"{}\"", escape(&rest[..cut])));
            rest = &rest[cut..];
        }
    }
    out
}

fn is_header(e: &PEntry) -> bool {
    e.f.has_kw && e.f.msgid.is_empty() && e.f.msgctxt.is_none()
}

/// `(?m)^[A-Za-z][A-Za-z-]*: ` somewhere in the text.
fn has_header_field(h: &str) -> bool {
    h.split('\n').any(|l| {
        let b = l.as_bytes();
        let name = b.iter().take_while(|c| c.is_ascii_alphabetic() || **c == b'-').count();
        name > 0 && b[0].is_ascii_alphabetic() && b[name..].starts_with(b": ")
    })
}

/// The decoded text with `\n` breaks and no BOM (the file's BOM, line breaks and encoding are in its skeleton tag,
/// and `enc::encode_text` writes them back as LocOS's `encode_like` does).
fn normalized(text: &str) -> String {
    let body = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    body.replace("\r\n", "\n").replace('\r', "\n")
}

/// LocOS `_render` + `_render_over`: `entries` (parsed from `source`) written over the source's lines.
fn render(entries: &mut [PEntry], source: &str) -> String {
    let heads: Vec<usize> = (0..entries.len()).filter(|&i| is_header(&entries[i])).collect();
    let good: Vec<usize> = heads.iter().copied().filter(|&i| entries[i].msgid_ok).collect();
    let keep = good.iter().copied().find(|&i| has_header_field(entries[i].f.msgstr.as_deref().unwrap_or(""))).or(good.first().copied()).or(heads.first().copied());
    for i in heads {
        if Some(i) != keep {
            entries[i].f.has_kw = false;
        }
    }
    let text = normalized(source);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::new();
    let mut pos = 0usize;
    for e in entries.iter() {
        let span = match e.span {
            Some(s) if s.0 >= pos => s,
            _ => {
                out.extend(multiline(emit(&e.f)));
                out.push(String::new());
                continue;
            }
        };
        let raw = &lines[span.0..=span.1];
        let block: Vec<String> = if e.was.as_ref() == Some(&e.f) {
            raw.iter().map(|s| s.to_string()).collect()
        } else if raw.iter().any(|r| py_lstrip(r).starts_with('"')) {
            multiline(emit(&e.f))
        } else {
            emit(&e.f)
        };
        if !block.is_empty() {
            out.extend(lines[pos..span.0].iter().map(|s| s.to_string()));
            out.extend(block);
        }
        pos = span.1 + 1;
    }
    out.extend(lines[pos.min(lines.len())..].iter().map(|s| s.to_string()));
    out.join("\n")
}

/// LocOS `_edge_newlines`.
fn edge_newlines(msgid: &str, text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut t = text.to_string();
    if msgid.starts_with('\n') && !t.starts_with('\n') {
        t.insert(0, '\n');
    } else if !msgid.starts_with('\n') && t.starts_with('\n') {
        t = t.trim_start_matches('\n').to_string();
    }
    if msgid.ends_with('\n') && !t.ends_with('\n') {
        t.push('\n');
    } else if !msgid.ends_with('\n') && t.ends_with('\n') {
        t = t.trim_end_matches('\n').to_string();
    }
    t
}

/// The units LocOS makes: (unit text) in order.
fn unit_texts(entries: &[PEntry]) -> Vec<String> {
    let mut out = Vec::new();
    for e in entries {
        if is_header(e) || e.f.msgid.is_empty() {
            continue;
        }
        out.push(e.f.msgid.clone());
        if let Some(p) = e.f.msgid_plural.as_ref().filter(|p| !p.is_empty()) {
            out.push(p.clone());
        }
    }
    out
}

/// LocOS `build`.
fn build(source: &str, texts: &BTreeMap<usize, String>) -> String {
    let norm = normalized(source);
    let mut entries = parse_entries(&norm);
    let mut k = 0usize;
    for e in entries.iter_mut() {
        if is_header(e) || e.f.msgid.is_empty() {
            continue;
        }
        let plural = e.f.msgid_plural.as_ref().is_some_and(|p| !p.is_empty());
        let mut supplied = texts.contains_key(&k);
        if let Some(t) = texts.get(&k) {
            let v = edge_newlines(&e.f.msgid, t);
            if plural {
                e.f.slots.insert(0, v);
            } else {
                e.f.msgstr = Some(v);
            }
        }
        k += 1;
        if plural {
            supplied |= texts.contains_key(&k);
            if let Some(t) = texts.get(&k) {
                e.f.slots.insert(1, edge_newlines(&e.f.msgid, t));
            }
            k += 1;
        }
        if supplied {
            let mut comments = Vec::new();
            for line in &e.f.comments {
                let s = py_lstrip(line);
                if s.starts_with("#|") {
                    continue;
                }
                if let Some(body) = s.strip_prefix("#,") {
                    let flags: Vec<&str> = body.split(',').map(bilingua_model::py_strip).filter(|f| !f.is_empty() && *f != "fuzzy").collect();
                    if flags.is_empty() {
                        continue;
                    }
                    comments.push(format!("#, {}", flags.join(", ")));
                    continue;
                }
                comments.push(line.clone());
            }
            e.f.comments = comments;
        }
    }
    render(&mut entries, source)
}

/// A gettext plural rule from the LocOS table: (nplurals, expression, slot for n = 1, catch-all slot).
#[derive(Clone, Copy)]
struct Rule {
    n: usize,
    expr: &'static str,
    one: usize,
    catch_all: usize,
}

impl Rule {
    fn lookup(value: &str) -> Option<Rule> {
        let find = |key: &str| {
            let i = RULES.binary_search_by(|r| r.0.cmp(key)).ok()?;
            let (_, n, expr, one, catch_all) = RULES[i];
            Some(Rule { n: n as usize, expr, one: one as usize, catch_all: catch_all as usize })
        };
        find(value).or_else(|| find(value.split('_').next().unwrap_or("")))
    }
    fn declaration(&self) -> String {
        format!("nplurals={}; plural={};", self.n, self.expr)
    }
    /// The slot whose CLDR form is `one`.
    fn takes_singular(&self, i: usize) -> bool {
        i == self.one && i != self.catch_all
    }
    fn plural_slot(&self) -> usize {
        if self.n <= 1 || self.one != 1 {
            return if self.n <= 1 { 0 } else { 1 };
        }
        if self.catch_all < self.n {
            self.catch_all
        } else {
            1
        }
    }
    fn owned(&self, i: usize) -> bool {
        i == self.plural_slot() || self.takes_singular(i)
    }
}

/// LocOS `_rule_key`: (nplurals, plural expression) of a `Plural-Forms` line, spacing and outer parens ignored.
fn rule_key(field: &str) -> (String, String) {
    let body: String = field.split_once(':').map_or(field, |(_, b)| b).chars().filter(|c| !bilingua_model::py_isspace_char(*c)).collect();
    let mut parts: BTreeMap<&str, &str> = BTreeMap::new();
    for p in body.split(';') {
        if let Some((a, b)) = p.split_once('=') {
            parts.insert(a, b);
        }
    }
    let expr = strip_outer_parens(parts.get("plural").copied().unwrap_or(""));
    (parts.get("nplurals").copied().unwrap_or("").to_string(), expr.to_string())
}

/// LocOS's loop `while expr.startswith("(") and expr.endswith(")")`: strip one pair of outer parentheses unless
/// the depth, counted from the first one, comes back to zero before the last character. LocOS rescans the rest at
/// every layer; this does it in one pass: layer `t` (the `t` outer `(` already gone, so the depth before it is `t`)
/// is stripped when the depth over the bytes `t..=len-2-t` never falls to `t`, that is when their minimum stays
/// above `t`, found from a suffix minimum on the left of the middle and a prefix minimum on its right.
fn strip_outer_parens(expr: &str) -> &str {
    let b = expr.as_bytes();
    let l = b.len();
    if l < 2 {
        return expr;
    }
    let mut depth = Vec::with_capacity(l);
    let mut d = 0i64;
    for c in b {
        d += match c {
            b'(' => 1,
            b')' => -1,
            _ => 0,
        };
        depth.push(d);
    }
    let mid = (l - 2) / 2;
    let mut left = vec![i64::MAX; mid + 1];
    let mut m = i64::MAX;
    for t in (0..=mid).rev() {
        m = m.min(depth[t]);
        left[t] = m;
    }
    let mut right = vec![i64::MAX; l];
    let mut m = i64::MAX;
    for j in mid..l {
        m = m.min(depth[j]);
        right[j] = m;
    }
    let mut t = 0usize;
    // layer t is the text t..l-t, two characters at least; its window is t..=l-2-t (t <= mid <= l-2-t)
    while 2 * t + 2 <= l && b[t] == b'(' && b[l - 1 - t] == b')' {
        if left[t].min(right[l - 2 - t]) <= t as i64 {
            break;
        }
        t += 1;
    }
    &expr[t..l - t]
}

/// The first line of `h` that starts with `prefix`: its byte range without the line break.
fn find_line(h: &str, prefix: &str) -> Option<(usize, usize)> {
    let mut start = 0;
    loop {
        let end = h[start..].find('\n').map_or(h.len(), |p| start + p);
        if h[start..end].starts_with(prefix) {
            return Some((start, end));
        }
        if end >= h.len() {
            return None;
        }
        start = end + 1;
    }
}

/// LocOS `_utf8_charset`.
fn utf8_charset(h: &str) -> String {
    let Some((s, e)) = find_line(h, "Content-Type:") else {
        return format!("Content-Type: text/plain; charset=UTF-8\n{h}");
    };
    let line = &h[s..e];
    let lower = line.to_ascii_lowercase();
    let lb = lower.as_bytes();
    let space = |c: u8| c.is_ascii_whitespace() || c == 0x0b;
    let mut i = 0;
    while let Some(p) = lower[i..].find("charset") {
        let at = i + p;
        let boundary = at == 0 || !(lb[at - 1].is_ascii_alphanumeric() || lb[at - 1] == b'_');
        let mut j = at + 7;
        while lb.get(j).is_some_and(|c| space(*c)) {
            j += 1;
        }
        if boundary && lb.get(j) == Some(&b'=') {
            j += 1;
            while lb.get(j).is_some_and(|c| space(*c)) {
                j += 1;
            }
            let mut k = j;
            while k < line.len() && lb[k] != b';' && !space(lb[k]) {
                k += 1;
            }
            return format!("{}{}UTF-8{}{}", &h[..s], &line[..j], &line[k..], &h[e..]);
        }
        i = at + 7;
    }
    format!("{}{}; charset=UTF-8{}", &h[..s], line.trim_end().trim_end_matches(';'), &h[e..])
}

/// A canonical BCP 47 tag (LocOS `a11y.target_tag`), or `None` when unusable.
fn canonical_tag(lang: &str) -> Option<String> {
    let t = lang.trim().replace('_', "-");
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    let mut parts = t.split('-');
    let first = parts.next()?;
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let mut out = vec![first.to_ascii_lowercase()];
    let mut extension = false;
    for sub in parts {
        if sub.is_empty() || sub.len() > 8 || !sub.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        let alpha = sub.bytes().all(|b| b.is_ascii_alphabetic());
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let mut c = sub.to_ascii_lowercase();
            c[..1].make_ascii_uppercase();
            out.push(c);
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && sub.bytes().all(|b| b.is_ascii_digit())) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

fn new_header() -> PEntry {
    PEntry {
        f: Fields {
            msgstr: Some("MIME-Version: 1.0\nContent-Type: text/plain; charset=UTF-8\nContent-Transfer-Encoding: 8bit\n".into()),
            has_kw: true,
            ..Fields::default()
        },
        msgid_ok: true,
        span: None,
        was: None,
    }
}

/// LocOS `_fill_plural_slots`.
fn fill_plural_slots(entries: &mut [PEntry], original: &[PEntry], texts: &BTreeMap<usize, String>, rule: Rule, keep_extra: bool) {
    let msgs: Vec<usize> = (0..entries.len()).filter(|&i| !is_header(&entries[i])).collect();
    let orig: Vec<&PEntry> = original.iter().filter(|e| !is_header(e)).collect();
    let aligned = orig.len() == msgs.len() && orig.iter().zip(&msgs).all(|(o, &i)| o.f.msgid == entries[i].f.msgid);
    let orig_fields: Vec<Fields> = if aligned { orig.iter().map(|e| e.f.clone()).collect() } else { msgs.iter().map(|&i| entries[i].f.clone()).collect() };
    let mut k = 0usize;
    for (mi, &ei) in msgs.iter().enumerate() {
        if entries[ei].f.msgid.is_empty() {
            continue;
        }
        let singular = texts.get(&k).filter(|t| !t.is_empty());
        k += 1;
        if entries[ei].f.msgid_plural.as_deref().is_none_or(str::is_empty) {
            continue;
        }
        let plural = texts.get(&k).filter(|t| !t.is_empty());
        k += 1;
        let e = &mut entries[ei].f;
        e.slots.retain(|i, _| *i < rule.n);
        for i in 0..rule.n {
            let mut own = orig_fields[mi].slots.get(&i).cloned().unwrap_or_default();
            if i >= 2 && !keep_extra {
                own.clear();
            }
            let want = if rule.n == 1 { plural.or(singular) } else if rule.takes_singular(i) { singular } else { plural };
            let v = match want {
                Some(w) if !(keep_extra && !rule.owned(i) && !own.is_empty()) => edge_newlines(&e.msgid, w),
                _ => own,
            };
            e.slots.insert(i, v);
        }
    }
}

/// LocOS `build_localized`: the text, and whether it is written as UTF-8 (LocOS's localized export, `_render(..,
/// utf8=True)`) rather than in the file's own encoding (the plain build it falls back to).
fn build_localized(source: &str, texts: &BTreeMap<usize, String>, locale: &str) -> (String, bool) {
    let out = build(source, texts);
    let Some(tag) = canonical_tag(locale) else { return (out, false) };
    let parts: Vec<&str> = tag.split('-').collect();
    let region = parts[1..].iter().find(|p| p.len() == 2 && p.chars().all(bilingua_model::py_isalpha_char));
    let value = match region {
        Some(r) => format!("{}_{r}", parts[0]),
        None => parts[0].to_string(),
    };
    let mut entries = parse_entries(&out);
    let created = !entries.iter().any(is_header);
    if created {
        let at = entries.iter().position(|e| e.f.has_kw).unwrap_or(entries.len());
        entries.insert(at, new_header());
    }
    let hi = entries.iter().position(is_header).unwrap_or(0);
    let Some(rule) = Rule::lookup(&value) else {
        if created {
            entries[hi].f.msgstr.get_or_insert_with(String::new).push_str(&format!("Language: {value}\n"));
            return (render(&mut entries, &out), true);
        }
        return (out, false);
    };
    let h = entries[hi].f.msgstr.get_or_insert_with(String::new);
    *h = utf8_charset(h);
    let declaration = format!("Plural-Forms: {}", rule.declaration());
    let source_language = match find_line(h, "Language:") {
        Some((s, e)) => {
            let old = bilingua_model::py_strip(&h[s + 9..e]).to_string();
            h.replace_range(s..e, &format!("Language: {value}"));
            old
        }
        None => {
            let same_rule = find_line(h, "Plural-Forms:").is_some_and(|(s, e)| rule_key(&h[s..e]) == rule_key(&declaration));
            if !h.is_empty() && !h.ends_with('\n') {
                h.push('\n');
            }
            h.push_str(&format!("Language: {value}\n"));
            if same_rule {
                value.clone()
            } else {
                String::new()
            }
        }
    };
    if find_line(h, "Plural-Forms:").is_some() {
        // every Plural-Forms line, as LocOS's `re.sub`
        let mut rebuilt = Vec::new();
        for l in h.split('\n') {
            rebuilt.push(if l.starts_with("Plural-Forms:") { declaration.clone() } else { l.to_string() });
        }
        *h = rebuilt.join("\n");
    } else {
        *h = format!("{}\n{declaration}\n", h.trim_end_matches('\n'));
    }
    let keep_extra = source_language.replace('-', "_").split('_').next() == Some(parts[0]);
    let original = parse_entries(&normalized(source));
    fill_plural_slots(&mut entries, &original, texts, rule, keep_extra);
    (render(&mut entries, &out), true)
}

/// Extracts a PO file the LocOS way (see the module notes).
pub fn extract(text: &str, encoding: &str, opts: &ExtractOptions, warnings: Vec<Warning>) -> Result<Document, Error> {
    let entries = parse_entries(&normalized(text));
    let mut units = Vec::new();
    for (n, t) in unit_texts(&entries).into_iter().enumerate() {
        let mut u = Unit::new(format!("u{}", n + 1), vec![Inline::text(t)]);
        u.context.kind = Some("string".into());
        units.push(u);
    }
    let data = enc::locos_skeleton(encoding, text);
    let doc = Document::new("po", units, Skeleton::binary(data, bilingua_model::parser_info!())).with_options(opts);
    bilingua_model::guard::finish(doc, warnings, &opts.limits())
}

/// Writes a profile document: LocOS's `build`, or `build_localized` when the document has a target language and
/// `stamp_target_always` is set or something is translated (LocOS's localized export).
pub fn merge(doc: &Document, data: &[u8], opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    let (tag, source) = enc::locos_skeleton_parts(data).ok_or_else(|| Error::Write("po: not a LocOS profile skeleton".into()))?;
    let mut texts = BTreeMap::new();
    for (k, u) in doc.units.iter().enumerate() {
        if u.segments.iter().any(Segment::has_target) {
            texts.insert(k, plain_text(&u.output()));
        }
    }
    let (out, utf8) = match opts.stamp_lang(doc, !texts.is_empty()) {
        Some(lang) => build_localized(source, &texts, lang),
        None => (build(source, &texts), false),
    };
    // the plain build keeps the file's encoding; the localized export is UTF-8 (a UTF-8 BOM and the line breaks
    // kept), as LocOS writes them
    enc::encode_text(&out, &if utf8 { enc::locos_utf8_tag(tag) } else { tag.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_without_a_blank_line_and_continuations() {
        let e = parse_entries("msgid \"a\"\nmsgstr \"\"\nmsgid \"\"\n\"b\\n\"\n\"c\"\nmsgstr \"x\"\n");
        assert_eq!(e.len(), 2);
        assert_eq!(e[1].f.msgid, "b\nc");
        assert_eq!(unit_texts(&e), ["a", "b\nc"]);
    }

    #[test]
    fn rule_keys_ignore_spacing_and_outer_parens() {
        assert_eq!(rule_key("Plural-Forms: nplurals=2; plural=(n != 1);"), rule_key("Plural-Forms: nplurals=2;plural=n!=1;"));
        assert_ne!(rule_key("Plural-Forms: nplurals=2; plural=(n > 1);"), rule_key("Plural-Forms: nplurals=2; plural=(n != 1);"));
    }

    #[test]
    fn multiline_splits_after_line_breaks() {
        assert_eq!(multiline(vec!["msgstr \"a\\nb\"".into()]), ["msgstr \"\"", "\"a\\n\"", "\"b\""]);
        assert_eq!(multiline(vec!["msgstr \"a\\n\"".into()]), ["msgstr \"a\\n\""]);
    }
}
