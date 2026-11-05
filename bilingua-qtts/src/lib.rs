//! Bilingua Qt Linguist filter: `.ts` translation source files (`<TS>`, `<context>`, `<message>`, numerus forms).
//! See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod cldr;
mod lang;
mod textenc;
mod xml;

use bilingua_model::*;
use std::collections::HashSet;
use xml::{content, leading_text, perr, scan, Node, Scan};

pub use bilingua_model::Limits;
pub use cldr::BABEL_VERSION;

/// The Qt Linguist filter.
#[derive(Debug, Clone, Copy, Default)]
pub struct QtTs;

// Keys of the unit's hidden values (`Unit::hidden`).
const RAW: &str = "raw"; // the `<translation>` element as written
const LEAD: &str = "lead"; // white space before the text in the source
const TRAIL: &str = "trail"; // white space after it
const NUM: &str = "num"; // present for a numerus message

fn code(u: &Unit, key: &str) -> Option<String> {
    u.hidden(key).map(str::to_string)
}

fn first_child(sc: &Scan, text: &str, i: usize, name: &str) -> Option<usize> {
    sc.children(i).find(|c| sc.local(text, *c) == name)
}

/// Python's `str.isalnum` for the purpose of "has a letter or digit". Rust's test is a little wider (combining
/// marks of some scripts count as letters), a known difference noted in the README.
fn has_letter_or_digit(s: &str) -> bool {
    bilingua_model::py_has_alnum(s)
}

fn xml_target_text(s: &str) -> String {
    s.chars()
        .map(|c| if matches!(c, '\u{0B}' | '\u{0C}') { '\n' } else { c })
        .filter(|c| !matches!(*c as u32, 0..=8 | 0x0E..=0x1F | 0xFFFE | 0xFFFF | 0xD800..=0xDFFF))
        .collect()
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_attr(s: &str, quote: u8) -> String {
    let s = s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let s = if quote == b'"' { s.replace('"', "&quot;") } else { s.replace('\'', "&apos;") };
    s.replace('\t', "&#9;").replace('\n', "&#10;").replace('\r', "&#13;")
}

/// Splits the leading and trailing white space off a run of inlines. Returns `(lead, trail)`.
fn trim_inlines(v: &mut Vec<Inline>) -> (String, String) {
    let (mut lead, mut trail) = (String::new(), String::new());
    if let Some(Inline::Text(t)) = v.first_mut() {
        let core = t.trim_start();
        lead = t[..t.len() - core.len()].to_string();
        *t = core.to_string();
    }
    if v.first().is_some_and(|i| matches!(i, Inline::Text(t) if t.is_empty())) {
        v.remove(0);
    }
    if let Some(Inline::Text(t)) = v.last_mut() {
        let core = t.trim_end();
        trail = t[core.len()..].to_string();
        *t = core.to_string();
    }
    if v.last().is_some_and(|i| matches!(i, Inline::Text(t) if t.is_empty())) {
        v.pop();
    }
    (lead, trail)
}

struct Found {
    /// Span of the `<translation>` element, when the message has one.
    span: Option<(usize, usize)>,
    unit: Unit,
}

impl QtTs {
    /// Like [`Filter::extract`] with explicit limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        guard::check_input(bytes, limits)?;
        guard::check_xml(bytes, limits)?;
        // like LocOS and the XML rules: undeclared input that is not UTF-8 or UTF-16 is refused, not guessed
        if !bytes.starts_with(&[0xFF, 0xFE]) && !bytes.starts_with(&[0xFE, 0xFF]) && !has_declared_encoding(bytes) && std::str::from_utf8(bytes).is_err() {
            return Err(Error::Encoding("This Qt Linguist .ts file is not UTF-8 and does not declare its encoding (encoding=\"..\" in the XML declaration). Declare it or re-save the file as UTF-8.".into()));
        }
        let dec = enc::decode_text(bytes, &enc::DecodeOptions { declared: true, source_lang: opts.source_lang.as_deref(), limits: *limits })?;
        let text = dec.text.as_str();
        if !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") && textenc::encode(text, &dec.encoding, &bilingua_model::MergeOptions::default())? != bytes {
            // bytes of a legacy code page that read to the same character as another spelling cannot be written
            // back unchanged: refuse rather than alter the file
            return Err(Error::Encoding("This Qt Linguist .ts file uses byte sequences of its legacy encoding that cannot be written back unchanged. Re-save the file as UTF-8.".into()));
        }
        // the pre-parse budget LocOS applies: far more markup than allowed units means a hostile file
        if text.bytes().filter(|b| *b == b'<').count() > 40 * limits.max_units {
            return Err(LimitError::new(
                LimitKind::Units,
                format!("This Qt Linguist .ts file has far more entries than the limit of {} allows. Split it into smaller files.", limits.max_units),
            )
            .into());
        }
        let sc = scan(text, false, limits)?;
        if sc.local(text, 0) != "TS" {
            return Err(perr("This Qt Linguist .ts file has no <TS> root; it is not a Qt Linguist file."));
        }
        let mut units: Vec<Unit> = Vec::new();
        let mut spans: Vec<(usize, usize, usize)> = Vec::new();
        let mut slot = 0usize;
        for ctx in sc.children(0).filter(|c| sc.local(text, *c) == "context") {
            let cname = first_child(&sc, text, ctx, "name").map(|n| leading_text(&content(&sc, text, n))).unwrap_or_default();
            for msg in sc.children(ctx).filter(|c| sc.local(text, *c) == "message") {
                let Some(src) = first_child(&sc, text, msg, "source") else { continue };
                let this = slot;
                slot += 1;
                let Some(found) = self.message(&sc, text, msg, src, this, &cname, opts)? else { continue };
                guard::check_unit_count(units.len() + 1, limits)?;
                let chars: usize = found.unit.source().iter().map(|i| match i {
                    Inline::Text(t) => t.chars().count(),
                    Inline::Tag { .. } => 1,
                }).sum();
                if chars > limits.max_unit_chars {
                    guard::check_unit_chars(chars, limits)?;
                }
                if let Some((s, e)) = found.span {
                    spans.push((s, e, units.len()));
                }
                units.push(found.unit);
            }
        }
        spans.sort_unstable();
        let mut parts = Vec::with_capacity(spans.len() * 2 + 1);
        let mut pos = 0usize;
        for (start, end, idx) in spans {
            if start > pos {
                parts.push(SkelPart::Raw(text[pos..start].to_string()));
            }
            pos = end;
            parts.push(SkelPart::Unit(idx));
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let root_attr = |n: &str| sc.attr(text, 0, n).map(|v| v.trim().replace('_', "-")).filter(|v| !v.is_empty());
        let doc = Document::new("ts", units, Skeleton::text(parts, dec.encoding, bilingua_model::parser_info!())).with_source_lang(opts.source_lang.clone().or_else(|| root_attr("sourcelanguage"))).with_target_lang(opts.target_lang.clone()).with_granularity(opts.granularity);
        guard::finish(doc, dec.warnings, limits)
    }

    #[allow(clippy::too_many_arguments)]
    fn message(&self, sc: &Scan, text: &str, msg: usize, src: usize, slot: usize, cname: &str, opts: &ExtractOptions) -> Result<Option<Found>, Error> {
        let tr = first_child(sc, text, msg, "translation");
        let ttype = tr.and_then(|t| sc.attr(text, t, "type")).unwrap_or_default();
        let skip = tr.is_none() || ttype == "vanished" || ttype == "obsolete";
        if skip && !opts.include_locked {
            return Ok(None);
        }
        // the source, with every child element, comment and instruction as one placeholder
        let nodes = content(sc, text, src);
        let mut inl: Vec<Inline> = Vec::new();
        let mut codes = std::collections::BTreeMap::new();
        let mut n = 0usize;
        for node in &nodes {
            match node {
                Node::Text(t) => inl.push(Inline::text(t.clone())),
                Node::Raw(s, e, label) => {
                    n += 1;
                    let id = n.to_string();
                    let raw = text[*s..*e].to_string();
                    codes.insert(id.clone(), CodeData { standalone: Some(raw.clone()), label: Some(label.clone()), ..Default::default() });
                    inl.push(Inline::tag(id, TagKind::Standalone, raw));
                }
            }
        }
        let (lead, trail) = trim_inlines(&mut inl);
        let translatable = !inl.is_empty() && (n > 0 || inl.iter().any(|i| matches!(i, Inline::Text(t) if has_letter_or_digit(t))));
        if !translatable {
            return Ok(None);
        }
        let lock = skip;
        let mut unit = Unit::new(if lock { format!("k{}", slot + 1) } else { format!("u{}", slot + 1) }, inl);
        unit.codes = codes;
        if lock {
            unit.translate = false;
            unit.lock_reason = Some(if tr.is_none() { "no translation element".to_string() } else { format!("{ttype} message") });
        }
        let numerus = sc.attr(text, msg, "numerus").is_some_and(|v| v.eq_ignore_ascii_case("yes"));
        if let Some(t) = tr {
            let e = &sc.els[t];
            unit.set_hidden(RAW, &text[e.start..e.end]);
        }
        if !lead.is_empty() {
            unit.set_hidden(LEAD, lead);
        }
        if !trail.is_empty() {
            unit.set_hidden(TRAIL, trail);
        }
        if numerus {
            unit.set_hidden(NUM, "yes");
        }
        // context: key, location, notes
        let src_lead = leading_text(&nodes);
        unit.context.kind = Some("string".into());
        unit.context.key = Some(sc.attr(text, msg, "id").filter(|v| !v.is_empty()).unwrap_or(src_lead));
        if let Some(loc) = first_child(sc, text, msg, "location") {
            let file = sc.attr(text, loc, "filename").unwrap_or_default();
            unit.context.location = Some(match sc.attr(text, loc, "line") {
                Some(l) if !file.is_empty() => format!("{file}:{l}"),
                Some(l) => format!("line {l}"),
                None => file,
            });
        }
        for name in ["comment", "extracomment"] {
            if let Some(c) = first_child(sc, text, msg, name) {
                let t = leading_text(&content(sc, text, c));
                let t = t.trim();
                if !t.is_empty() {
                    unit.context.notes.push(t.to_string());
                }
            }
        }
        unit.context.extra.insert("context".into(), cname.to_string());
        unit.context.extra.insert("p_index".into(), slot.to_string());
        if numerus {
            unit.context.extra.insert("plural".into(), "numerus".into());
        }
        let span = tr.map(|t| (sc.els[t].start, sc.els[t].end));
        Ok(Some(Found { span, unit }))
    }
}

// ---------------------------------------------------------------------------------------------------------
// Sniffing
// ---------------------------------------------------------------------------------------------------------

/// True when the XML declaration at the start names an encoding.
fn has_declared_encoding(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(200)];
    let head = head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head);
    if !head.starts_with(b"<?xml") {
        return false;
    }
    let end = head.windows(2).position(|w| w == b"?>").unwrap_or(head.len());
    let decl = String::from_utf8_lossy(&head[..end]).to_ascii_lowercase();
    decl.find("encoding").is_some_and(|p| decl[p + 8..].trim_start().starts_with('='))
}

fn head_text(bytes: &[u8]) -> String {
    let head = &bytes[..bytes.len().min(8192)];
    if head.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs_decode(true, &head[2..]);
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs_decode(false, &head[2..]);
    }
    String::from_utf8_lossy(head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head)).into_owned()
}

fn encoding_rs_decode(le: bool, b: &[u8]) -> String {
    let e = if le { bilingua_model::encoding_rs::UTF_16LE } else { bilingua_model::encoding_rs::UTF_16BE };
    e.decode_without_bom_handling(b).0.into_owned()
}

/// True when the first element of the XML (after the declaration, comments and DOCTYPE) is `<TS`.
fn root_is_ts(head: &str) -> bool {
    let mut s = head.trim_start();
    loop {
        if let Some(r) = s.strip_prefix("<?") {
            match r.find("?>") {
                Some(e) => s = r[e + 2..].trim_start(),
                None => return false,
            }
        } else if let Some(r) = s.strip_prefix("<!--") {
            match r.find("-->") {
                Some(e) => s = r[e + 3..].trim_start(),
                None => return false,
            }
        } else if s.len() >= 9 && s.is_char_boundary(9) && s[..9].eq_ignore_ascii_case("<!DOCTYPE") {
            let b = s.as_bytes();
            let (mut j, mut q) = (9, 0u8);
            while j < b.len() {
                let c = b[j];
                if q != 0 {
                    if c == q {
                        q = 0;
                    }
                } else if c == b'"' || c == b'\'' {
                    q = c;
                } else if c == b'>' {
                    break;
                }
                j += 1;
            }
            if j >= b.len() {
                return false;
            }
            s = s[j + 1..].trim_start();
        } else {
            break;
        }
    }
    s.strip_prefix("<TS").is_some_and(|r| r.chars().next().is_some_and(|c| c.is_whitespace() || c == '>' || c == '/'))
}

impl Filter for QtTs {
    fn id(&self) -> &'static str {
        "ts"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ts"]
    }

    /// Strict: XML whose root element is `<TS>` (after a declaration, comments and a DOCTYPE). TypeScript, which
    /// shares the `.ts` extension, never starts that way. Safe anywhere in the filter list.
    fn sniff(&self, bytes: &[u8]) -> bool {
        root_is_ts(&head_text(bytes))
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &opts.limits())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("a Qt Linguist file needs a text skeleton".into()));
        };
        let wr = |m: String| Error::Write(m);
        // newline style of the file: new text uses it when the whole file does
        let (mut lf, mut crlf_n) = (0usize, 0usize);
        for p in parts {
            if let SkelPart::Raw(s) = p {
                let b = s.as_bytes();
                for (i, c) in b.iter().enumerate() {
                    if *c == b'\n' {
                        lf += 1;
                        if i > 0 && b[i - 1] == b'\r' {
                            crlf_n += 1;
                        }
                    }
                }
            }
        }
        let crlf = lf > 0 && lf == crlf_n;
        let touched = doc.units.iter().any(|u| u.translate && u.segments.iter().any(|s| !s.target.is_empty()));
        let target = opts.stamp_lang(doc, touched).map(str::trim);
        let first_raw = parts.iter().position(|p| matches!(p, SkelPart::Raw(_)));
        let file_lang = first_raw.and_then(|i| match &parts[i] {
            SkelPart::Raw(s) => root_language(s),
            _ => None,
        });
        // LocOS: the target's tag stamps the file and picks the plural forms; an unusable tag leaves both to the file
        // (`stamp_target_always`: even when nothing is translated)
        let tag = target.and_then(lang::qt_language);
        let plural_lang = tag.clone().or(file_lang.clone()).unwrap_or_default();
        let fill = target.is_some();
        // LocOS writes the Qt spelling whenever the target is usable, so `PL-pl` becomes `pl_PL`; an equal value is left
    // as written (same bytes either way)
    let stamp = tag.filter(|t| file_lang.as_deref() != Some(t.as_str()));
        let refs = char_refs(parts, encoding);
        let mut out = String::with_capacity(parts.len() * 64);
        let mut stamped = stamp.is_none();
        for (k, p) in parts.iter().enumerate() {
            match p {
                SkelPart::Raw(s) => {
                    if !stamped && Some(k) == first_raw {
                        stamped = true;
                        match set_language(s, stamp.as_deref().unwrap_or_default()) {
                            Some(n) => out.push_str(&n),
                            None => out.push_str(s),
                        }
                    } else {
                        out.push_str(s);
                    }
                }
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| wr(format!("skeleton points at missing unit {i}")))?;
                    write_unit(u, crlf, fill, &plural_lang, refs, &mut out)?;
                }
            }
        }
        textenc::encode(&out, encoding, opts)
    }
}

// ---------------------------------------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------------------------------------

/// The span of the root start tag in a text that begins with the prolog.
fn root_stag(s: &str) -> Option<(usize, usize)> {
    let mut i = 0usize;
    let b = s.as_bytes();
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let r = &s[i..];
        if r.starts_with("<?") {
            i += r.find("?>")? + 2;
        } else if r.starts_with("<!--") {
            i += r.find("-->")? + 3;
        } else if r.len() >= 9 && r.is_char_boundary(9) && r[..9].eq_ignore_ascii_case("<!DOCTYPE") {
            let (mut j, mut q) = (i + 9, 0u8);
            while j < b.len() {
                let c = b[j];
                if q != 0 {
                    if c == q {
                        q = 0;
                    }
                } else if c == b'"' || c == b'\'' {
                    q = c;
                } else if c == b'>' {
                    break;
                }
                j += 1;
            }
            i = j + 1;
        } else {
            break;
        }
    }
    if !s[i.min(s.len())..].starts_with('<') {
        return None;
    }
    let (mut j, mut q) = (i + 1, 0u8);
    while j < b.len() {
        let c = b[j];
        if q != 0 {
            if c == q {
                q = 0;
            }
        } else if c == b'"' || c == b'\'' {
            q = c;
        } else if c == b'>' {
            return Some((i, j + 1));
        }
        j += 1;
    }
    None
}

/// Scans a start tag on its own (closed with a made-up end tag when it is not self-closing).
fn stag_scan(stag: &str) -> Option<Scan> {
    let lim = Limits::default();
    if stag.ends_with("/>") {
        return scan(stag, true, &lim).ok();
    }
    let name_end = stag[1..].find(|c: char| c.is_whitespace() || c == '>' || c == '/').map_or(stag.len(), |p| p + 1);
    scan(&format!("{stag}</{}>", &stag[1..name_end]), true, &lim).ok()
}

fn root_language(raw: &str) -> Option<String> {
    let (s, e) = root_stag(raw)?;
    let stag = &raw[s..e];
    let sc = stag_scan(stag)?;
    sc.attr(stag, 0, "language")
}

/// Sets `language` on the root start tag of `raw` (the first raw part), keeping the quote style and every other
/// byte of the tag.
fn set_language(raw: &str, value: &str) -> Option<String> {
    let (s, e) = root_stag(raw)?;
    let stag = &raw[s..e];
    let sc = stag_scan(stag)?;
    let new = match sc.attrs_of(0).iter().find(|a| &stag[a.name.0..a.name.1] == "language") {
        Some(a) => format!("{}{}{}", &stag[..a.val.0], escape_attr(value, a.quote), &stag[a.val.1..]),
        None => {
            let body = stag.strip_suffix('>')?;
            let body = body.strip_suffix('/').unwrap_or(body);
            let cut = body.trim_end().len();
            format!("{} language=\"{}\"{}", &stag[..cut], escape_attr(value, b'"'), &stag[cut..])
        }
    };
    Some(format!("{}{}{}", &raw[..s], new, &raw[e..]))
}

/// Characters a translated span writes as character references whatever the engine's writer would do.
#[derive(Clone, Copy, PartialEq)]
enum CharRefs {
    None,
    /// UTF-8 bytes under an `encoding="us-ascii"` declaration.
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
    } else if bilingua_model::encoding_rs::Encoding::for_label(base.as_bytes()) == Some(bilingua_model::encoding_rs::GBK) {
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

/// The translated text of a unit as inner XML: tags back as their original markup (each at most once, unknown
/// ones dropped), text escaped, the source's surrounding white space kept.
fn body_of(u: &Unit, crlf: bool) -> String {
    let mut used: HashSet<String> = HashSet::new();
    let mut s = code(u, LEAD).unwrap_or_default();
    for i in u.output() {
        match i {
            Inline::Text(t) => s.push_str(&escape_text(&xml_target_text(&t))),
            Inline::Tag { id, .. } => {
                if let Some(raw) = u.codes.get(&id).and_then(|c| c.standalone.clone()) {
                    if used.insert(id) {
                        s.push_str(&raw);
                    }
                }
            }
        }
    }
    s.push_str(&code(u, TRAIL).unwrap_or_default());
    // new text follows the file's line ends
    let s = s.replace("\r\n", "\n").replace('\r', "\n");
    if crlf {
        s.replace('\n', "\r\n")
    } else {
        s
    }
}

fn write_unit(u: &Unit, crlf: bool, fill: bool, plural_lang: &str, refs: CharRefs, out: &mut String) -> Result<(), Error> {
    let raw = code(u, RAW).ok_or_else(|| Error::Write("a unit lost its translation element".into()))?;
    if !u.translate || u.segments.iter().all(|s| s.target.is_empty()) {
        out.push_str(&raw);
        return Ok(());
    }
    let body = with_char_refs(body_of(u, crlf), refs);
    let numerus = u.has_hidden(NUM);
    let sc = scan(&raw, true, &Limits::default()).map_err(|_| Error::Write("the skeleton's translation element is not XML any more".into()))?;
    let el = sc.els[0];
    let qname = sc.qname(&raw, 0).to_string();
    // the start tag, without type="unfinished" (the message now counts as translated)
    let mut stag = raw[..el.stag_end].to_string();
    if let Some(a) = sc.attrs_of(0).iter().find(|a| &raw[a.name.0..a.name.1] == "type" && &raw[a.val.0..a.val.1] == "unfinished") {
        stag = format!("{}{}", &raw[..a.whole.0], &raw[a.whole.1..el.stag_end]);
    }
    if el.empty {
        stag = close_empty(&stag);
    }
    let inner = if numerus {
        let forms: Vec<usize> = sc.children(0).filter(|c| sc.local(&raw, *c) == "numerusform").collect();
        let want = cldr::forms(plural_lang).max(1);
        if forms.is_empty() {
            (0..want).map(|_| format!("<numerusform>{body}</numerusform>")).collect::<String>()
        } else {
            let mut inner = String::new();
            let mut cur = el.stag_end;
            for f in &forms {
                let fe = sc.els[*f];
                if fe.empty {
                    inner.push_str(&raw[cur..fe.start]);
                    inner.push_str(&close_empty(&raw[fe.start..fe.stag_end]));
                    inner.push_str(&body);
                    inner.push_str(&format!("</{}>", sc.qname(&raw, *f)));
                    cur = fe.end;
                } else {
                    inner.push_str(&raw[cur..fe.stag_end]);
                    inner.push_str(&body);
                    cur = fe.etag_start;
                }
            }
            inner.push_str(&raw[cur..el.etag_start]);
            if fill && want > forms.len() {
                // the target language has more plural forms than the file: one more per missing form, laid out
                // like the first, each with the translation
                let last = sc.els[*forms.last().expect("not empty")];
                let first = sc.els[forms[0]];
                let tail_len = el.etag_start - last.end;
                let sep = &raw[el.stag_end..first.start];
                let sep = if sep.trim().is_empty() { sep } else { "" };
                let q = sc.qname(&raw, *forms.last().expect("not empty"));
                let extra: String = (0..want - forms.len()).map(|_| format!("{sep}<{q}>{body}</{q}>")).collect();
                let cut = inner.len() - tail_len;
                inner.insert_str(cut, &extra);
            }
            inner
        }
    } else {
        body
    };
    out.push_str(&stag);
    out.push_str(&inner);
    out.push_str(&format!("</{qname}>"));
    Ok(())
}

/// `<a x="1" />` to `<a x="1">`.
fn close_empty(stag: &str) -> String {
    match stag.strip_suffix("/>") {
        Some(r) => format!("{}>", r.trim_end()),
        None => stag.to_string(),
    }
}
