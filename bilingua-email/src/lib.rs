//! Bilingua email filter (`.eml`, RFC 5322 plus MIME). Spec and baseline: the Localization OS filter
//! `app/filters/email_filter.py` and its suite `tests/verify_email_filter.py`. See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod codec;
mod cte;
mod header;
mod lang;
mod mime;
mod plain;
mod pyish;

use bilingua_html::Html;
use bilingua_model::*;
use codec::{Codec, CP1252, UTF8};
use header::Edits;
use mime::{Cap, Part, WalkError};
use std::borrow::Cow;
use std::collections::HashMap;

pub use mime::{MAX_DEPTH, MAX_HEADER_BLOCK, MAX_PARTS};

/// The email filter.
pub struct Email;

/// Name of the one skeleton member that holds the message as it came in.
const MEMBER: &str = "message.eml";
const FORMAT: &str = "eml";

const PGP_INLINE: [&str; 2] = ["-----BEGIN PGP SIGNED MESSAGE-----", "-----BEGIN PGP MESSAGE-----"];

fn walk_error(e: WalkError) -> Error {
    match e {
        WalkError::Refused(msg) if msg.starts_with("This file is not an email") || msg.contains("MIME headers") => {
            Error::Parse { format: FORMAT.into(), msg }
        }
        WalkError::Refused(msg) => Error::Unsupported(msg),
        WalkError::Limit(cap, msg) => {
            let kind = match cap {
                Cap::Parts => LimitKind::Units,
                Cap::Depth => LimitKind::XmlDepth,
                Cap::Header => LimitKind::InputBytes,
            };
            Error::Limit(LimitError::new(kind, msg))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Subject,
    Plain,
    Html,
}

struct Slot {
    kind: Kind,
    part: usize,
    text: String,
    codec: Codec,
    declared: bool,
    flowed: bool,
    delsp: bool,
    subject: Option<header::Subject>,
}

struct Parsed {
    parts: Vec<Part>,
    slots: Vec<Slot>,
    eol: &'static str,
}

fn translatable(p: &Part) -> bool {
    p.children.is_empty()
        && matches!(p.ctype.as_str(), "text/plain" | "text/html")
        && p.disposition.as_deref() != Some("attachment")
        && matches!(p.cte.as_str(), "7bit" | "8bit" | "binary" | "quoted-printable" | "base64")
}

fn decode_body(data: &[u8], p: &Part) -> Result<(String, Codec, bool), Error> {
    let raw = &data[p.body_start..p.end];
    let bytes: Cow<[u8]> = match p.cte.as_str() {
        "base64" => Cow::Owned(cte::b64_decode(raw).map_err(|_| Error::Encoding("A base64 text part of this email is damaged.".into()))?),
        "quoted-printable" => Cow::Owned(cte::qp_decode(raw)),
        _ => Cow::Borrowed(raw),
    };
    if let Some(label) = p.param("charset").filter(|l| !l.is_empty()) {
        let label = label.trim();
        let Some(codec) = Codec::lookup(label) else {
            let short: String = label.chars().take(40).collect();
            return Err(Error::Encoding(format!("This email has a text part in an unknown charset ({short}).")));
        };
        return match codec.decode(&bytes) {
            Ok(s) => Ok((s, codec, true)),
            Err(()) => {
                if codec == Codec::Ascii {
                    // a common mislabel: 8-bit text marked us-ascii
                    for alt in [UTF8, CP1252] {
                        if let Ok(s) = alt.decode(&bytes) {
                            return Ok((s, alt, false));
                        }
                    }
                }
                Err(Error::Encoding(format!(
                    "A text part of this email declares charset {label} but its bytes do not match it. Re-save the message and upload it again."
                )))
            }
        };
    }
    for codec in [Codec::Ascii, UTF8, CP1252] {
        if let Ok(s) = codec.decode(&bytes) {
            return Ok((s, codec, false));
        }
    }
    Err(Error::Encoding("A text part of this email is not readable text.".into()))
}

fn parse_message(data: &[u8]) -> Result<Parsed, Error> {
    let parts = mime::walk(data).map_err(walk_error)?;
    let eol = if data[..parts[0].body_start].windows(2).any(|w| w == b"\r\n") { "\r\n" } else { "\n" };
    let mut slots = Vec::new();
    if let Some(s) = header::read_subject(data, parts[0].start, parts[0].hdr_end) {
        slots.push(Slot { kind: Kind::Subject, part: 0, text: s.text.clone(), codec: Codec::Ascii, declared: false, flowed: false, delsp: false, subject: Some(s) });
    }
    for (idx, p) in parts.iter().enumerate() {
        if !translatable(p) {
            continue;
        }
        let (text, codec, declared) = decode_body(data, p)?;
        if PGP_INLINE.iter().any(|m| text.contains(m)) {
            return Err(Error::Unsupported(mime::signed_message("signed or encrypted")));
        }
        let kind = if p.ctype == "text/html" { Kind::Html } else { Kind::Plain };
        let flowed = kind == Kind::Plain && p.param("format").is_some_and(|f| f.eq_ignore_ascii_case("flowed"));
        let delsp = kind == Kind::Plain && p.param("delsp").is_some_and(|f| f.eq_ignore_ascii_case("yes"));
        slots.push(Slot { kind, part: idx, text, codec, declared, flowed, delsp, subject: None });
    }
    Ok(Parsed { parts, slots, eol })
}

/// Limits that never refuse: the file was already accepted when it was extracted.
fn unlimited() -> Limits {
    Limits::new().with_max_input_bytes(u64::MAX).with_max_units(usize::MAX).with_max_unit_chars(usize::MAX).with_max_lines(usize::MAX).with_max_xml_depth(usize::MAX).with_max_xml_elements(usize::MAX)
}

/// The HTML of a part read by `bilingua-html` as UTF-8 (a byte-order mark makes it skip the charset declaration,
/// which describes the transfer bytes, not this text).
fn html_doc(text: &str, source_lang: Option<&str>, limits: Limits) -> Result<Document, Error> {
    let mut bytes = Vec::with_capacity(text.len() + 3);
    bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    bytes.extend_from_slice(text.as_bytes());
    let opts = ExtractOptions::new().with_source_lang(source_lang.map(str::to_string)).with_limits(limits);
    shielded(|| Html.extract(&bytes, &opts))
}

/// Runs a call into `bilingua-html` so that a panic inside it (its entity scan slices text at a byte offset
/// that can fall inside a character) comes back as an error instead of taking the caller down. Where panics
/// abort (WASM) the guard cannot help, and the bug has to be fixed in `bilingua-html`.
fn shielded<T>(f: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
            Ok(r) => r,
            Err(_) => Err(Error::Parse { format: "html".into(), msg: "the HTML part could not be read".into() }),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        f()
    }
}

fn id_of(slot: &Slot) -> String {
    match slot.kind {
        Kind::Subject => "subject".into(),
        Kind::Plain => format!("p{}", slot.part),
        Kind::Html => format!("h{}", slot.part),
    }
}

/// The units of one slot, and for an HTML slot also its sub-document.
fn slot_units(slot: &Slot, source_lang: Option<&str>, limits: Limits) -> Result<(Vec<Unit>, Option<Document>), Error> {
    let base = id_of(slot);
    match slot.kind {
        Kind::Subject => {
            let t = pyish::strip(&slot.text);
            if t.is_empty() {
                return Ok((vec![], None));
            }
            let mut u = Unit::new(base, vec![Inline::text(t)]);
            u.context.kind = Some("subject".into());
            u.context.style = Some("Subject".into());
            u.context.key = Some("Subject".into());
            u.context.location = Some("header Subject".into());
            Ok((vec![u], None))
        }
        Kind::Plain => {
            guard::check_lines(&slot.text, &limits)?;
            let mut out = Vec::new();
            let mut k = 0;
            for it in plain::items(&slot.text, slot.flowed, slot.delsp) {
                if let plain::Item::Para(p) = it {
                    let mut u = Unit::new(format!("{base}.{k}"), vec![Inline::text(p.text)]);
                    u.context.kind = Some("paragraph".into());
                    u.context.style = Some("text/plain".into());
                    u.context.location = Some(format!("part {} text/plain paragraph {}", slot.part, k + 1));
                    out.push(u);
                    k += 1;
                    guard::check_unit_count(out.len(), &limits)?;
                }
            }
            Ok((out, None))
        }
        Kind::Html => {
            let sub = html_doc(&slot.text, source_lang, limits)?;
            let mut out = Vec::new();
            for u in &sub.units {
                // page metadata is not offered: parts are concatenated into one document
                if u.context.kind.as_deref() == Some("meta") {
                    continue;
                }
                let mut nu = u.clone();
                nu.id = format!("{base}.{}", u.id);
                for s in &mut nu.segments {
                    s.id = format!("{base}.{}", s.id);
                }
                let loc = nu.context.location.take().unwrap_or_default();
                nu.context.location = Some(format!("part {} text/html {}", slot.part, loc).trim_end().to_string());
                out.push(nu);
            }
            Ok((out, Some(sub)))
        }
    }
}

fn original_of(doc: &Document) -> Result<&[u8], Error> {
    match &doc.skeleton {
        Skeleton::Zip { entries, .. } => entries
            .iter()
            .find_map(|e| match (&e.data, e.name == MEMBER) {
                (ZipData::Raw(b), true) => Some(b.as_slice()),
                _ => None,
            })
            .ok_or_else(|| Error::Write("the skeleton has no message".into())),
        _ => Err(Error::Write("an email needs a zip-style skeleton".into())),
    }
}

/// The text a unit writes: its target (source for untranslated segments), tags flattened to their display text.
fn flat(inl: &[Inline]) -> String {
    inl.iter()
        .map(|i| match i {
            Inline::Text(t) => t.as_str(),
            Inline::Tag { display, .. } => display.as_str(),
        })
        .collect()
}

/// Some(new inlines) when the unit holds a translation different from its source.
fn changed(u: &Unit) -> Option<Vec<Inline>> {
    if !u.translate || !u.segments.iter().any(|s| s.has_target()) {
        return None;
    }
    let out = u.output();
    (out != u.source()).then_some(out)
}

fn delimiters(parsed: &Parsed) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for p in &parsed.parts {
        if p.children.is_empty() {
            continue;
        }
        if let Some(b) = p.param("boundary") {
            if b.is_ascii() {
                let mut d = b"--".to_vec();
                d.extend_from_slice(b.as_bytes());
                out.push(d);
            }
        }
    }
    out
}

/// True when a line of an encoded body could be read as a MIME delimiter.
fn hits_delimiter(body: &[u8], delims: &[Vec<u8>]) -> bool {
    if delims.is_empty() {
        return false;
    }
    body.split(|c| *c == b'\n').any(|ln| {
        let mut l = ln;
        while l.first() == Some(&b'\r') {
            l = &l[1..];
        }
        l.starts_with(b"--") && delims.iter().any(|d| l.starts_with(d))
    })
}

fn decl_re() -> &'static regex::Regex {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| {
        regex::Regex::new(
            r#"(?i)(<\?xml[^>]{0,200}?\bencoding\s*=\s*["']\s*|<meta\b[^>]{0,400}?\bcharset\s*=\s*["']?\s*|Content-Type:[^"\n]{0,100}?\bcharset\s*=\s*)([A-Za-z0-9_.:-]{1,40})"#,
        )
        .unwrap()
    })
}

/// Encodes a rebuilt body in its original charset and transfer encoding, falling back to UTF-8 and/or
/// quoted-printable when the translation does not fit, and to base64 when a line would read as a delimiter.
fn encode_part(slot: &Slot, part: &Part, new_text: &str, data: &[u8], eol: &str, delims: &[Vec<u8>]) -> Result<(Vec<u8>, Edits), Error> {
    let mut edits = Edits::default();
    let payload = match slot.codec.encode(new_text) {
        Some(p) => {
            if !slot.declared && !p.is_ascii() {
                edits.charset = Some(codec::mime_name(slot.codec).to_string());
            }
            p
        }
        None => {
            edits.charset = Some("utf-8".into());
            if slot.kind == Kind::Html {
                // keep a <meta charset> truthful
                decl_re().replace_all(new_text, |c: &regex::Captures| format!("{}utf-8", &c[1])).into_owned().into_bytes()
            } else {
                new_text.as_bytes().to_vec()
            }
        }
    };
    let raw_old = &data[part.body_start..part.end];
    let mut cte = part.cte.clone();
    if matches!(cte.as_str(), "7bit" | "8bit" | "binary") {
        let too_long = pyish::byte_lines_keep(&payload).iter().any(|l| {
            let mut n = l.len();
            if l.ends_with(b"\n") {
                n -= 1;
            }
            if l.ends_with(b"\r\n") || (l.ends_with(b"\r") && !l.ends_with(b"\n")) {
                n -= 1;
            }
            n > plain::MAX_LINE
        });
        if (cte == "7bit" && !payload.is_ascii()) || too_long {
            cte = "quoted-printable".into();
            edits.cte = Some(cte.clone());
        } else if !hits_delimiter(&payload, delims) {
            return Ok((payload, edits));
        } else {
            cte = "quoted-printable".into();
        }
    }
    if cte == "quoted-printable" {
        let norm = replace_bytes(&payload, b"\r\n", b"\n");
        let body = replace_bytes(&cte::qp_encode(&norm), b"\n", eol.as_bytes());
        if !hits_delimiter(&body, delims) {
            if cte != part.cte {
                edits.cte = Some(cte);
            }
            return Ok((body, edits));
        }
        edits.cte = Some("base64".into());
    }
    let mut body = replace_bytes(&cte::b64_encode_lines(&payload), b"\n", eol.as_bytes());
    if !raw_old.ends_with(b"\n") {
        while matches!(body.last(), Some(b'\r' | b'\n')) {
            body.pop();
        }
    }
    Ok((body, edits))
}

fn replace_bytes(src: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len() + src.len() / 16);
    let mut i = 0;
    while i < src.len() {
        if src[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(src[i]);
            i += 1;
        }
    }
    out
}

impl Filter for Email {
    fn id(&self) -> &'static str {
        FORMAT
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["eml"]
    }

    /// An email starts with header fields, and the first block has at least one that mail carries.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(4096)];
        if head.contains(&0) {
            return false;
        }
        let end = head.windows(2).position(|w| w == b"\n\n").or_else(|| head.windows(4).position(|w| w == b"\r\n\r\n")).unwrap_or(head.len());
        let mut known = 0;
        let mut fields = 0;
        let mut first_ok = false;
        for line in head[..end].split(|c| *c == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.is_empty() || matches!(line[0], b' ' | b'\t') {
                continue;
            }
            let Some(c) = line.iter().position(|c| *c == b':') else { return false };
            let name = line[..c].to_ascii_lowercase();
            if name.is_empty() || !name.iter().all(|b| (b'!'..=b'~').contains(b) && *b != b':') {
                return false;
            }
            fields += 1;
            if fields == 1 {
                // mail header names are written Capitalised-Like-This; YAML and INI keys rarely are
                first_ok = line[0].is_ascii_uppercase();
            }
            if matches!(
                name.as_slice(),
                b"from" | b"to" | b"subject" | b"date" | b"message-id" | b"received" | b"return-path" | b"mime-version" | b"delivered-to" | b"cc" | b"reply-to"
            ) {
                known += 1;
            }
        }
        first_ok && fields >= 2 && known >= 1
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        guard::check_input(bytes, &limits)?;
        let parsed = parse_message(bytes)?;
        let mut units: Vec<Unit> = Vec::new();
        for slot in &parsed.slots {
            let (local, _) = slot_units(slot, opts.source_lang.as_deref(), limits)?;
            for u in local {
                units.push(u);
                guard::check_unit_count(units.len(), &limits)?;
            }
        }
        let blank = |u: &Unit| flat(&u.source()).trim().is_empty() && !u.source().iter().any(|i| matches!(i, Inline::Tag { .. }));
        if units.iter().all(blank) {
            return Err(Error::Unsupported("This email has no Subject or text body to translate (attachments are never translated).".into()));
        }
        let doc = Document::new(FORMAT, units, Skeleton::zip(vec![ZipEntry { name: MEMBER.into(), data: ZipData::Raw(bytes.to_vec()) }], bilingua_model::parser_info!())).with_options(opts);
        guard::finish(doc, vec![], &limits)
    }

    /// `stamp_target_always` sets the Content-Language even when nothing is translated. `output_encoding` does not
    /// re-encode the message: each MIME part keeps the charset its header declares (a mail client reads it from
    /// there), so `Utf8` changes nothing here.
    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let data = original_of(doc)?;
        let parsed = parse_message(data)?;
        let limits = unlimited();
        let by_id: HashMap<&str, &Unit> = doc.units.iter().map(|u| (u.id.as_str(), u)).collect();
        let any_translated = doc.units.iter().any(|u| changed(u).is_some());
        let tag = opts.stamp_lang(doc, any_translated).and_then(lang::canonical_tag);
        let delims = delimiters(&parsed);
        let beol = parsed.eol.as_bytes();
        let mut header_edits: HashMap<usize, Edits> = HashMap::new();
        let mut bodies: HashMap<usize, Vec<u8>> = HashMap::new();
        if let Some(t) = &tag {
            for (i, p) in parsed.parts.iter().enumerate() {
                if header::has_field(&data[p.start..p.hdr_end], "content-language") {
                    header_edits.entry(i).or_default().lang = Some(t.clone());
                }
            }
        }
        for slot in &parsed.slots {
            let base = id_of(slot);
            let new_text: String = match slot.kind {
                Kind::Subject => {
                    let t = pyish::strip(&slot.text);
                    let Some(new) = by_id.get(base.as_str()).and_then(|u| changed(u)).map(|o| flat(&o)) else { continue };
                    if t.is_empty() || new == t {
                        continue;
                    }
                    let sub = slot.subject.as_ref().expect("subject slot");
                    let field = header::encode_subject(sub, &new, parsed.eol).ok_or_else(|| Error::Write("the Subject cannot be encoded".into()))?;
                    header_edits.entry(0).or_default().subject = Some(field);
                    continue;
                }
                Kind::Plain => {
                    let mut pieces = String::with_capacity(slot.text.len());
                    let mut k = 0;
                    for it in plain::items(&slot.text, slot.flowed, slot.delsp) {
                        match it {
                            plain::Item::Raw(r) => pieces.push_str(&r),
                            plain::Item::Para(p) => {
                                let id = format!("{base}.{k}");
                                k += 1;
                                match by_id.get(id.as_str()).and_then(|u| changed(u)).map(|o| flat(&o)) {
                                    Some(new) if new != p.text => pieces.push_str(&plain::render(&p, &new, slot.flowed, slot.delsp)),
                                    _ => pieces.push_str(&p.raw),
                                }
                            }
                        }
                    }
                    pieces
                }
                Kind::Html => {
                    let mut sub = html_doc(&slot.text, doc.source_lang.as_deref(), limits)?;
                    let mut touched = false;
                    let mut sets = Vec::new();
                    for u in &sub.units {
                        let id = format!("{base}.{}", u.id);
                        if let Some(out) = by_id.get(id.as_str()).and_then(|o| changed(o)) {
                            sets.push((u.segments[0].id.clone(), out));
                        }
                    }
                    for (seg, out) in sets {
                        sub.set_target(&seg, out)?;
                        touched = true;
                    }
                    let mut text = slot.text.clone();
                    if touched {
                        let bytes = shielded(|| Html.merge(&sub))?;
                        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&bytes);
                        text = String::from_utf8(bytes.to_vec()).map_err(|_| Error::Write("the HTML part is not valid UTF-8".into()))?;
                    }
                    if let Some(t) = &tag {
                        text = lang::declare_language(&text, t);
                    }
                    text
                }
            };
            if new_text == slot.text {
                continue;
            }
            let part = &parsed.parts[slot.part];
            let (body, edits) = encode_part(slot, part, &new_text, data, parsed.eol, &delims)?;
            bodies.insert(slot.part, body);
            let e = header_edits.entry(slot.part).or_default();
            if edits.cte.is_some() {
                e.cte = edits.cte;
            }
            if edits.charset.is_some() {
                e.charset = edits.charset;
            }
        }
        // splice: header blocks and bodies are disjoint ranges of the original
        let mut cuts: Vec<(usize, usize, Vec<u8>)> = Vec::new();
        for (i, e) in &header_edits {
            let p = &parsed.parts[*i];
            cuts.push((p.start, p.hdr_end, header::edit_block(&data[p.start..p.hdr_end], beol, e)));
        }
        for (i, b) in bodies {
            let p = &parsed.parts[i];
            cuts.push((p.body_start, p.end, b));
        }
        cuts.sort_by_key(|c| (c.0, c.1));
        let mut out = Vec::with_capacity(data.len() + 1024);
        let mut pos = 0;
        for (s, e, new) in cuts {
            if s < pos {
                return Err(Error::Write("overlapping edits".into()));
            }
            out.extend_from_slice(&data[pos..s]);
            out.extend_from_slice(&new);
            pos = e;
        }
        out.extend_from_slice(&data[pos..]);
        Ok(out)
    }
}
