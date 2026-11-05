//! Bilingua WiX localization (`.wxl`) filter.
//!
//! A `.wxl` file holds the strings of a Windows Installer package for one culture. Two spellings exist and both
//! are read and written in place, as Localization OS does (`xml_resource.py`, `WxlFilter`):
//!
//! - WiX 4 and later: `<String Id=".." Value=".."/>`, the text is the `Value` attribute;
//! - WiX 3: `<String Id="..">text</String>`, the text is the element content.
//!
//! Also translated: the `Text` attribute of `<UI Dialog=".." Control=".." Text=".."/>` (dialog control text).
//! `Localizable="no"` (or `false`) marks a value that is not text (a GUID, a path): skipped. Only direct
//! children of the root are read; a `<String>` with child nodes (comment, element) is skipped.
//!
//! Behaviour that goes beyond the LocOS filter or differs from it:
//!
//! - `[ProductName]`, `[#FileKey]`, `[%ENV]`, `[1]` and the other Windows Installer formatted references are
//!   standalone free tags (label `placeholder`, display = the exact source text). The LocOS filter keeps them as
//!   plain text, so the adapter flattens them back;
//! - `Culture` on the root element is rewritten (lower case, `-` for `_`, as LocOS) when the document has a
//!   `target_lang` and at least one unit is translated. An untouched merge is byte for byte the original, even
//!   with `target_lang` set (LocOS stamps the culture on every localized build). `Codepage` and the decimal
//!   `Language` are never changed, as in LocOS: set them for the target language before building the MSI;
//! - the file is never re-serialised: declaration, comments, processing instructions, CDATA, attribute order and
//!   quoting, white space and line endings stay as they are. A value that was not changed is written back as
//!   its original spelling;
//! - UTF-8 (BOM kept), UTF-16 with a BOM and the legacy charsets an XML declaration may name are read and
//!   written back in the same encoding (`bilingua_model::enc`);
//! - unlike LocOS, text with a character XML cannot hold is cleaned in attribute values too (LocOS cleans element
//!   content only), so the output is always well-formed XML;
//! - a DOCTYPE without entity declarations is accepted and kept (LocOS refuses any internal subset); a file that
//!   declares entities is refused by the model's XML guard.
//!
//! Unit ids are `u<n>` where `n` is the 1-based position among the LocOS slots (every `<String>`, and each `<UI>`
//! with a `Text` attribute; LocOS `p_index` + 1), so they do not
//! change with `include_locked`. `context.key` is the `Id` (or `Dialog.Control` for `<UI>`).

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod scan;

use bilingua_model::enc::{self, DecodeOptions};
use bilingua_model::guard;
use bilingua_model::*;
use scan::{Entry, Kind, Place};

const ID: &str = "wxl";
/// Hidden codes on a unit: the original spelling of the value, the white space around the text, and the form.
const RAW_KEY: &str = "$raw";
const LEAD_KEY: &str = "$lead";
const TRAIL_KEY: &str = "$trail";
const FORM_KEY: &str = "$form";

/// The WiX localization filter.
#[derive(Debug, Default, Clone, Copy)]
pub struct Wxl;

/// Python `str.isspace` (what LocOS strips): White_Space plus U+001C to U+001F.
fn is_space(c: char) -> bool {
    bilingua_model::py_isspace_char(c)
}

fn split_ws(s: &str) -> (&str, &str, &str) {
    let core = s.trim_matches(is_space);
    if core.is_empty() {
        return (s, "", "");
    }
    let lead_len = s.len() - s.trim_start_matches(is_space).len();
    let trail_start = s.trim_end_matches(is_space).len();
    (&s[..lead_len], core, &s[trail_start..])
}

/// LocOS `is_translatable`: Python's `str.isalnum`, exactly (`bilingua_model::py_has_alnum`); a value of
/// combining marks and symbols only is skipped.
fn is_translatable(v: &str) -> bool {
    bilingua_model::py_has_alnum(v)
}

fn text_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            c => o.push(c),
        }
    }
    o
}

fn attr_escape(s: &str, quote: u8) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '\n' => o.push_str("&#10;"),
            '\r' => o.push_str("&#13;"),
            '\t' => o.push_str("&#9;"),
            '"' if quote == b'"' => o.push_str("&quot;"),
            '\'' if quote == b'\'' => o.push_str("&apos;"),
            c => o.push(c),
        }
    }
    o
}

/// Keeps XML 1.0 characters, turns pasted Word breaks (VT, FF) into newlines (LocOS `xml_target_text`).
fn clean_target(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\u{b}' | '\u{c}' => Some('\n'),
            '\0'..='\u{8}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}' => None,
            c => Some(c),
        })
        .collect()
}

fn is_no(v: &Option<String>) -> bool {
    matches!(v.as_deref().map(|s| s.trim_matches(is_space).to_lowercase()).as_deref(), Some("no" | "false"))
}

/// The end of a Windows Installer formatted reference that starts at `s[0] == '['`, if it is one.
fn reference_len(s: &str) -> Option<usize> {
    let close = s.find(']')?;
    let inner = &s[1..close];
    if inner.is_empty() || inner.len() > 128 || inner.contains('[') {
        return None;
    }
    let ok = if inner == "~" {
        true
    } else if let Some(rest) = inner.strip_prefix('\\') {
        rest.chars().count() == 1
    } else {
        let body = inner.strip_prefix(['#', '!', '$', '%']).unwrap_or(inner);
        !body.is_empty() && body.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '@' | '-'))
    };
    ok.then_some(close + 1)
}

/// Splits text into text pieces and reference tags.
fn inlines_of(core: &str, unit: &mut Unit) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut last = 0;
    let mut n = 0;
    let mut i = 0;
    while let Some(p) = core[i..].find('[') {
        let at = i + p;
        match reference_len(&core[at..]) {
            Some(len) => {
                if at > last {
                    out.push(Inline::text(&core[last..at]));
                }
                n += 1;
                let shown = &core[at..at + len];
                let id = n.to_string();
                out.push(Inline::free_tag(id.clone(), shown));
                unit.codes.insert(id, CodeData { standalone: Some(shown.to_string()), label: Some("placeholder".into()), ..Default::default() });
                last = at + len;
                i = last;
            }
            None => i = at + 1,
        }
    }
    if last < core.len() {
        out.push(Inline::text(&core[last..]));
    }
    out
}

fn code(s: &str) -> CodeData {
    CodeData { standalone: Some(s.to_string()), ..Default::default() }
}

fn flatten(v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { display, .. } => s.push_str(display),
        }
    }
    s
}

/// What goes into the file for a full value (lead + text + trail) in a given form.
fn encode_value(form: &str, value: &str, original_cdata_ok: bool) -> String {
    let v = clean_target(value);
    match form {
        "a\"" => attr_escape(&v, b'"'),
        "a'" => attr_escape(&v, b'\''),
        "c" if original_cdata_ok && !v.contains("]]>") => format!("<![CDATA[{v}]]>"),
        _ => text_escape(&v),
    }
}

fn parse_err(msg: String) -> Error {
    Error::Parse { format: ID.into(), msg }
}

impl Filter for Wxl {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["wxl"]
    }

    /// Tight: the root element must be `WixLocalization`. It can go anywhere in `filters()`, but before any
    /// generic XML filter that would otherwise claim the file.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(8192)];
        let text = if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
            let le = head[0] == 0xFF;
            let units = head[2..].chunks_exact(2).map(|p| if le { u16::from_le_bytes([p[0], p[1]]) } else { u16::from_be_bytes([p[0], p[1]]) });
            char::decode_utf16(units).map(|r| r.unwrap_or('?')).collect::<String>()
        } else {
            String::from_utf8_lossy(head).into_owned()
        };
        scan::root_name(text.trim_start_matches('\u{feff}')).as_deref() == Some("WixLocalization")
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        guard::check_input(bytes, &limits)?;
        let dec = enc::decode_text(bytes, &DecodeOptions::new(opts).declared())?;
        let text = dec.text;
        if !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be")
            && enc::encode_text(&text, &dec.encoding)? != bytes
        {
            // double-mapped or lone bytes of a legacy code page read to a character that writes back as another
            // spelling; an untouched merge must be the original bytes, so the file is refused, never altered
            return Err(Error::Encoding(
                "This WiX localization file uses byte sequences of its legacy encoding that cannot be written back unchanged. Re-save the file as UTF-8.".into(),
            ));
        }
        guard::check_xml(text.as_bytes(), &limits)?;
        let scanned = scan::scan(&text).map_err(|m| parse_err(format!("this file is not well-formed XML: {m}")))?;

        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        let mut counted = 0usize;
        // LocOS slots: every `<String>`, and a `<UI>` only when it has a `Text` attribute (a `<UI>` without one
        // is not a slot and does not move the `p_index` of the entries after it).
        let slots = scanned.entries.iter().filter(|e| e.kind == Kind::String || matches!(e.place, Place::Attr { .. }));
        for (idx, e) in slots.enumerate() {
            // The LocOS slots: which entries are read, and which are skipped but counted. `Localizable="no"` is
            // kept as a locked unit when asked for; an entry with nothing to write into is left out.
            let no_place = match e.kind {
                Kind::String => !matches!(e.place, Place::Attr { .. }) && e.has_children,
                Kind::Ui => !matches!(e.place, Place::Attr { .. }),
            };
            let marked_no = e.kind == Kind::String && is_no(&e.localizable);
            let (lead, core, trail) = split_ws(&e.text);
            if no_place || core.is_empty() {
                continue;
            }
            let translatable = !marked_no && is_translatable(core);
            if !translatable && !opts.include_locked {
                continue;
            }
            if translatable {
                counted += 1;
                guard::check_unit_count(counted, &limits)?;
            }
            let (start, end, form) = match &e.place {
                Place::Attr { start, end, quote } => (*start, *end, if *quote == b'"' { "a\"" } else { "a'" }),
                Place::Content { start, end, cdata } => (*start, *end, if *cdata { "c" } else { "t" }),
                Place::None => continue,
            };
            if start > pos {
                parts.push(SkelPart::Raw(text[pos..start].to_string()));
            }
            pos = end;
            let mut unit = Unit::new(format!("u{}", idx + 1), vec![]);
            let source = inlines_of(core, &mut unit);
            unit.segments[0].source = source;
            if !lead.is_empty() {
                unit.codes.insert(LEAD_KEY.into(), code(lead));
            }
            if !trail.is_empty() {
                unit.codes.insert(TRAIL_KEY.into(), code(trail));
            }
            unit.codes.insert(FORM_KEY.into(), code(form));
            let raw = &text[start..end];
            if encode_value(form, &e.text, form == "c") != raw {
                unit.codes.insert(RAW_KEY.into(), code(raw));
            }
            fill_context(&mut unit, e, idx, form);
            unit.translate = translatable;
            if !translatable {
                unit.lock_reason = Some(if marked_no { "Localizable=\"no\": not text" } else { "no translatable text" }.into());
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let doc = Document::new(ID, units, Skeleton::text(parts, dec.encoding, bilingua_model::parser_info!())).with_options(opts);
        guard::finish(doc, dec.warnings, &limits)
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("wxl needs a text skeleton".into()));
        };
        let changed = doc.units.iter().any(|u| u.translate && flatten(&u.output()) != flatten(&u.source()));
        let culture = opts.stamp_lang(doc, changed).and_then(culture_of);
        let mut out = String::new();
        let mut first = true;
        for p in parts {
            match p {
                SkelPart::Raw(s) => {
                    match (&culture, first) {
                        (Some(c), true) => out.push_str(&set_culture(s, c)),
                        _ => out.push_str(s),
                    }
                    first = false;
                }
                SkelPart::Unit(i) => {
                    let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    out.push_str(&render(unit));
                    first = false;
                }
            }
        }
        enc::encode_text_with(&out, encoding, opts)
    }
}

fn fill_context(unit: &mut Unit, e: &Entry, idx: usize, form: &str) {
    let ctx = &mut unit.context;
    ctx.extra.insert("p_index".into(), idx.to_string());
    match e.kind {
        Kind::String => {
            ctx.kind = Some("string".into());
            ctx.style = Some("String".into());
            ctx.key = Some(e.id.clone().unwrap_or_default());
            ctx.extra.insert("form".into(), if form.starts_with('a') { "value" } else { "text" }.into());
        }
        Kind::Ui => {
            ctx.kind = Some("ui".into());
            ctx.style = Some("UI".into());
            let joined = [e.dialog.as_deref(), e.control.as_deref()].into_iter().flatten().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(".");
            ctx.key = Some(if joined.is_empty() { e.id.clone().unwrap_or_default() } else { joined });
            if let Some(d) = &e.dialog {
                ctx.extra.insert("dialog".into(), d.clone());
            }
            if let Some(c) = &e.control {
                ctx.extra.insert("control".into(), c.clone());
            }
        }
    }
    if let Some(v) = &e.overridable {
        ctx.extra.insert("overridable".into(), v.clone());
    }
    if let Some(v) = &e.localizable {
        ctx.extra.insert("localizable".into(), v.clone());
    }
}

fn hidden<'a>(u: &'a Unit, key: &str) -> &'a str {
    u.codes.get(key).and_then(|c| c.standalone.as_deref()).unwrap_or("")
}

/// What goes between the quotes (or between the tags) for a unit.
fn render(unit: &Unit) -> String {
    let out = flatten(&unit.output());
    let form = hidden(unit, FORM_KEY);
    if out == flatten(&unit.source()) {
        if let Some(raw) = unit.codes.get(RAW_KEY).and_then(|c| c.standalone.as_deref()) {
            return raw.to_string();
        }
    }
    let full = format!("{}{}{}", hidden(unit, LEAD_KEY), out, hidden(unit, TRAIL_KEY));
    encode_value(form, &full, form == "c")
}

/// The culture to write for a target locale: LocOS `html_lang(...).lower()`, or `None` when the tag is unusable.
fn culture_of(lang: &str) -> Option<String> {
    let t = lang.trim_matches(is_space).replace('_', "-");
    let mut parts = t.split('-');
    let first = parts.next()?;
    let ok_first = (2..=8).contains(&first.len()) && first.bytes().all(|b| b.is_ascii_alphabetic());
    let ok_rest = parts.all(|p| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()));
    (t.len() <= 64 && ok_first && ok_rest).then(|| t.to_lowercase())
}

/// Sets `Culture` on the root start tag inside the first raw part.
fn set_culture(raw: &str, culture: &str) -> String {
    let b = raw.as_bytes();
    // find the root start tag: skip declaration, comments, PIs and the DOCTYPE
    let mut i = 0;
    let tag_start = loop {
        let Some(p) = raw[i..].find('<') else { return raw.to_string() };
        let at = i + p;
        let rest = &raw[at..];
        let skip = if rest.starts_with("<!--") {
            rest.find("-->").map(|e| e + 3)
        } else if rest.starts_with("<?") {
            rest.find("?>").map(|e| e + 2)
        } else if rest.starts_with("<!") {
            doctype_len(rest)
        } else {
            break at;
        };
        match skip {
            Some(n) => i = at + n,
            None => return raw.to_string(),
        }
    };
    // the end of the tag: the first '>' outside quotes
    let mut q = 0u8;
    let mut tag_end = None;
    for (k, &c) in b.iter().enumerate().skip(tag_start) {
        if q != 0 {
            if c == q {
                q = 0;
            }
        } else if c == b'"' || c == b'\'' {
            q = c;
        } else if c == b'>' {
            tag_end = Some(k + 1);
            break;
        }
    }
    let Some(tag_end) = tag_end else { return raw.to_string() };
    let Ok((_, attrs)) = scan::parse_attrs(b, tag_start, tag_end) else { return raw.to_string() };
    match attrs.iter().find(|a| a.name == "Culture") {
        Some(a) => format!("{}{}{}", &raw[..a.start], attr_escape(culture, a.quote), &raw[a.end..]),
        None => {
            let mut close = tag_end - 1;
            if close > tag_start && b[close - 1] == b'/' {
                close -= 1;
            }
            while close > tag_start && b[close - 1].is_ascii_whitespace() {
                close -= 1;
            }
            format!("{} Culture=\"{}\"{}", &raw[..close], attr_escape(culture, b'"'), &raw[close..])
        }
    }
}

/// Length of a `<!DOCTYPE ...>` declaration, with an internal subset in brackets.
fn doctype_len(rest: &str) -> Option<usize> {
    let (mut q, mut sub) = (0u8, 0usize);
    for (k, &c) in rest.as_bytes().iter().enumerate() {
        if q != 0 {
            if c == q {
                q = 0;
            }
        } else if c == b'"' || c == b'\'' {
            q = c;
        } else if c == b'[' {
            sub += 1;
        } else if c == b']' {
            sub = sub.saturating_sub(1);
        } else if c == b'>' && sub == 0 {
            return Some(k + 1);
        }
    }
    None
}
