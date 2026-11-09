//! Bilingua HTML Help filters: contents (`.hhc`), index (`.hhk`) and project (`.hhp`) files of Microsoft HTML Help 1.x.
//!
//! * `.hhc` / `.hhk` (sitemaps): the `value` of every `<param name="Name">` and `<param name="Keyword">` inside an
//!   `<OBJECT type="text/sitemap">` is a unit. Local, URL, ImageNumber, Merge, comments and the site-properties
//!   object are never touched. A `See Also` value must name an index keyword, so on merge it follows the translation
//!   of the keyword whose source text it names.
//! * `.hhp` (project): `Title=` in `[OPTIONS]` and the caption of each `[WINDOWS]` definition. When the file is
//!   merged for a target language, `Language=` becomes that language's LCID (`0x40c French (France)`).
//!
//! Units are plain text (no inline codes): like Localization OS, this filter makes no tags. Topic pages (`.htm`) are
//! not part of these formats; they go through `bilingua-html`.
//!
//! Encodings: HTML Help 1.x reads these files in the Windows ANSI code page of the help language. A file is read as
//! UTF-8 or UTF-16 with a byte-order mark, valid non-ASCII UTF-8, a charset it declares in a meta tag, the code page
//! of the project's `Language=` (see [`hhp_code_page`] and [`HhcFilter::extract_with`]), else Windows-1252. A merge
//! with a target language writes the target language's ANSI code page (Russian Windows-1251, Japanese Shift_JIS, ...),
//! rewrites a declared meta charset and the charset of the site-properties `Font` to match, and refuses a character
//! the code page cannot hold (or writes UTF-8 with the format option [`UTF8_FALLBACK`], which HTML Help Workshop shows
//! wrongly).

/// Skeleton schema number: bump only when this crate's skeleton layout changes.
pub const SKELETON_SCHEMA: u32 = 1;

mod browser;
mod data;
mod decdiff;
mod enc;
mod encdiff;
mod hhp;
mod lcid;
mod sitemap;
mod text;

use bilingua_model::codepage::{encoding_for_code_page, target_code_page};
use bilingua_model::guard::{self, LimitError, LimitKind};
use bilingua_model::*;
use sitemap::Span;
use std::collections::HashMap;

pub use lcid::target_lcid;

const HIDDEN: &str = "$hh";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Hhc,
    Hhk,
    Hhp,
}

impl Kind {
    fn id(self) -> &'static str {
        match self {
            Kind::Hhc => "hhc",
            Kind::Hhk => "hhk",
            Kind::Hhp => "hhp",
        }
    }
    fn what(self) -> &'static str {
        match self {
            Kind::Hhc => "Contents",
            Kind::Hhk => "Index",
            Kind::Hhp => "project",
        }
    }
}

/// The format option (`ExtractOptions::format_options`) for Localization OS's `HTMLHELP_UTF8_FALLBACK`: with `"true"`,
/// a merged text the ANSI code page cannot hold is written as UTF-8 (and a declared meta charset says so) instead of
/// the merge being refused. HTML Help Workshop does not show those characters correctly. Off by default. The extract
/// records it in the skeleton, so the merge of that document honours it.
pub const UTF8_FALLBACK: &str = "htmlhelp.utf8_fallback";

/// The format option (`ExtractOptions::format_options`) for the ANSI code page of the help project's `Language=`
/// (decision 21, LocOS's `hhp_code_page` parse keyword), as a Windows code page number (`"1251"`): used for a file
/// that declares no charset, as [`HhcFilter::extract_with`] does. See [`hhp_code_page`].
pub const CODE_PAGE: &str = "htmlhelp.code_page";

/// The `format_options` keys of the HTML Help filters.
pub const FORMAT_OPTIONS: &[FormatOptionKey] = &[
    FormatOptionKey::bool(UTF8_FALLBACK, "Write text the file's code page cannot hold as UTF-8 (LocOS HTMLHELP_UTF8_FALLBACK)."),
    FormatOptionKey::int(CODE_PAGE, "The Windows code page of the help project's Language= (LocOS hhp_code_page), for a file that declares no charset."),
];
const FALLBACK_MARK: &str = "utf8_fallback";

/// The ANSI code page of an HTML Help project's language: `Language=0x419 Russian` in `[OPTIONS]` gives 1251. `None`
/// when the file names no language, names one with no ANSI code page, or cannot be read. Pass it to
/// [`HhcFilter::extract_with`] for the project's contents and index files.
pub fn hhp_code_page(bytes: &[u8]) -> Option<u16> {
    let limits = Limits::default();
    // A project in a code page the default decode cannot read (a localized Shift_JIS project) still has an ASCII
    // `Language=` line: read it (Localization OS returns None here and then cannot read its own export; see the gate
    // report). The code page is a hint the caller passes on, never a decode of its own.
    let t = match load(bytes, &limits, None) {
        Ok((t, _)) => t,
        // a size, `<` count or line limit gives None, as LocOS does; only a code page the decode cannot read falls back
        Err(Error::Limit(_)) => return None,
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    };
    let (_, lang) = hhp::walk(&t, &limits).ok()?;
    lcid::code_page_of_language(&lang?.text)
}

fn limit(kind: LimitKind, msg: String) -> Error {
    LimitError::new(kind, msg).into()
}

fn load(bytes: &[u8], limits: &Limits, hint: Option<u16>) -> Result<(String, String), Error> {
    guard::check_input(bytes, limits)?;
    // Every sitemap entry needs several '<'; forty per allowed unit is far above any real file.
    if bytes.iter().filter(|&&b| b == b'<').count() > 40 * limits.max_units {
        return Err(limit(
            LimitKind::Units,
            format!("This HTML Help file has far more entries than the limit of {} segments allows. Split it into smaller files.", limits.max_units),
        ));
    }
    let (text, tag) = enc::decode(bytes, hint)?;
    guard::check_lines(&text, limits)?;
    Ok((text, tag))
}

fn owned(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

fn extract_impl(kind: Kind, bytes: &[u8], opts: &ExtractOptions, hint: Option<u16>) -> Result<Document, Error> {
    let limits = opts.limits();
    // The project's Language= code page decides first; else the code page of the project's source language (a
    // Japanese project's undeclared file is Shift_JIS, not Windows-1252). Localization OS has no second rule.
    // The caller's code page (the `htmlhelp.code_page` option) when no explicit hint is given; a project file is
    // read in the code page of its own Language= (LocOS #797, decode version 2).
    let hint = hint
        .or_else(|| opts.format_option(CODE_PAGE).and_then(|v| v.trim().parse::<u16>().ok()))
        .or_else(|| if kind == Kind::Hhp { hhp_code_page(bytes) } else { None });
    // Bilingua's own rule, not LocOS's: the source language's code page; the LocOS profile has only the project's
    let hint = if opts.is_profile(bilingua_model::profile::LOCOS) { hint } else { hint.or_else(|| opts.source_lang.as_deref().and_then(target_code_page)) };
    let (text, tag) = load(bytes, &limits, hint)?;
    let fallback = opts.format_flag(UTF8_FALLBACK) == Some(true);
    // A code page whose decode is not one to one (Shift_JIS NEC and IBM duplicates) cannot give the same bytes back
    // from the text: such a file keeps its bytes for an untouched merge.
    let keep_raw = enc::is_ansi(&tag) && original_bytes(&text, &tag) != bytes;
    let (spans, structure) = if kind == Kind::Hhp {
        (hhp::walk(&text, &limits)?.0, hhp::has_section(&text))
    } else {
        let w = sitemap::walk(&text, &limits)?;
        (w.units, w.structure)
    };
    // A valid file with nothing to translate opens with no units and a warning (Localization OS refuses it; see
    // MIGRATION.md). A merge of it gives the original bytes back. A file with no HTML Help structure at all (empty,
    // whitespace, plain text, junk), or whose entry names a browser reads but the walker cannot, is still refused.
    let mut warnings = Vec::new();
    if spans.is_empty() {
        let parse = |msg: String| Error::Parse { format: kind.id().into(), msg };
        if !structure {
            return Err(parse(if kind == Kind::Hhp {
                "This is not an HTML Help project file: it has no [OPTIONS], [WINDOWS] or other [section] line.".to_string()
            } else {
                format!("This is not an HTML Help {} file: it has no <UL> list and no <OBJECT> entry.", kind.what())
            }));
        }
        if kind != Kind::Hhp && browser::has_entry_name(&text) {
            return Err(parse(format!(
                "This HTML Help {} file has entry names (param Name / Keyword) that a browser reads but that sit inside markup the filter reads differently (for example an abrupt comment <!-->). Correct that markup in the file and open it again.",
                kind.what()
            )));
        }
        let msg = if kind == Kind::Hhp {
            "This HTML Help project file has no Title= or window caption to translate.".to_string()
        } else {
            format!("This HTML Help {} file has no entry names (param Name / Keyword inside OBJECT type=\"text/sitemap\") to translate.", kind.what())
        };
        warnings.push(bilingua_model::Warning::new("no_translatable_text", kind.id(), msg));
    }
    let mut parts: Vec<SkelPart> = Vec::with_capacity(spans.len() * 2 + 1);
    let mut units: Vec<Unit> = Vec::with_capacity(spans.len());
    let mut pos = 0usize;
    for sp in &spans {
        parts.push(SkelPart::Raw(text[pos..sp.start].to_string()));
        pos = sp.end;
        let mut unit = Unit::new(format!("u{}", units.len() + 1), vec![Inline::text(sp.text.clone())]);
        unit.codes.insert(
            HIDDEN.into(),
            CodeData::new().with_open(owned(sp.quote)).with_standalone(text[sp.start..sp.end].to_string()).with_label(fallback.then(|| FALLBACK_MARK.to_string())),
        );
        unit.context.kind = Some(match sp.kind {
            "w" => "window".to_string(),
            k => k.to_string(),
        });
        if keep_raw && units.is_empty() {
            if let Some(c) = unit.codes.get_mut(HIDDEN) {
                c.close = Some(bilingua_model::b64::encode(bytes));
            }
        }
        parts.push(SkelPart::Unit(units.len()));
        units.push(unit);
    }
    parts.push(SkelPart::Raw(text[pos..].to_string()));
    // No unit to carry the original bytes of a file that does not round-trip through its code page: keep them whole.
    let skeleton = if units.is_empty() && keep_raw {
        Skeleton::binary(bytes.to_vec(), bilingua_model::parser_info!())
    } else {
        Skeleton::text(parts, tag, bilingua_model::parser_info!())
    };
    guard::finish(
        Document::new(kind.id(), units, skeleton).with_options(opts),
        warnings,
        &limits,
    )
}

fn changed(unit: &Unit, old: &str) -> Option<String> {
    if !unit.translate || unit.segments.iter().all(|s| !s.has_target()) {
        return None;
    }
    let new = plain_text(&unit.output());
    (new != old).then_some(new)
}

fn apply(text: &str, edits: &[(usize, usize, String)]) -> String {
    let mut sorted: Vec<&(usize, usize, String)> = edits.iter().collect();
    sorted.sort_by_key(|e| e.0);
    let mut out = String::with_capacity(text.len() + 64);
    let mut last = 0;
    for (s, e, new) in sorted {
        if *s < last {
            continue;
        }
        out.push_str(&text[last..*s]);
        out.push_str(new);
        last = *e;
    }
    out.push_str(&text[last..]);
    out
}

/// (envelope tag, code page, label) of the rebuilt file.
fn out_encoding(tag: &str, target: Option<&str>, localized: bool) -> (String, Option<u16>, String) {
    if !enc::is_ansi(tag) {
        return (tag.to_string(), None, tag.to_string());
    }
    if localized {
        if let Some(cp) = target.and_then(target_code_page) {
            if let Some(e) = encoding_for_code_page(cp) {
                return (enc::Codec::Enc(e, cp).tag(), Some(cp), enc::cp_label(cp));
            }
        }
    }
    match enc::Codec::from_tag(tag).and_then(enc::Codec::cp) {
        Some(n) => (tag.to_string(), Some(n), format!("source (Windows {n})")),
        None => (tag.to_string(), None, format!("source ({tag})")),
    }
}

fn font_edit(sp: &Span, cp: Option<u16>) -> Option<(usize, usize, String)> {
    let (_, gdi) = enc::cp_info(cp?)?;
    let mut parts: Vec<&str> = sp.text.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    let cs = text::py_trim(parts[2]);
    if cs.is_empty() || !cs.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let want = gdi.to_string();
    if cs == want {
        return None;
    }
    parts[2] = &want;
    Some((sp.start, sp.end, sitemap::escape_attr(&parts.join(","), sp.quote)))
}

/// True when no unit has a translation that differs from its source.
fn edits_none(spans: &[Span], order: &[usize], doc: &Document) -> bool {
    spans.iter().zip(order).all(|(sp, &i)| changed(&doc.units[i], &sp.text).is_none())
}

/// The original file: the decoded text written back in its envelope (byte-exact for every encoding this crate
/// reads, since a decode is only accepted when the code page writes each character back).
fn original_bytes(orig: &str, tag: &str) -> Vec<u8> {
    if !enc::is_ansi(tag) {
        return enc::encode_unicode(orig, tag);
    }
    match enc::Codec::from_tag(tag) {
        Some(c) => enc::encode_ansi_raw(orig, c),
        None => orig.as_bytes().to_vec(),
    }
}

fn merge_impl(kind: Kind, doc: &Document, mo: &MergeOptions) -> Result<Vec<u8>, Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    if let Skeleton::Binary { data, .. } = &doc.skeleton {
        // Only an empty file in a code page that does not round-trip has a binary skeleton (see extract_impl).
        if !doc.units.is_empty() {
            return Err(Error::Write("htmlhelp needs a text skeleton".into()));
        }
        if mo.output_encoding == OutputEncoding::Keep {
            return Ok(data.clone());
        }
        return Err(Error::Unsupported(
            "This HTML Help file has nothing to translate and is in a code page whose text does not convert back to the same bytes, so it cannot be rewritten as UTF-8; merge it with the file's own encoding (output encoding keep) to get the original file back.".into(),
        ));
    }
    let Skeleton::Text { parts, encoding: tag, .. } = &doc.skeleton else {
        return Err(Error::Write("htmlhelp needs a text skeleton".into()));
    };
    let tag = tag.as_str();
    // the original text and the unit order in it
    let mut orig = String::new();
    let mut order: Vec<usize> = Vec::with_capacity(doc.units.len());
    let mut at: Vec<(usize, usize)> = Vec::with_capacity(doc.units.len());
    for p in parts {
        match p {
            SkelPart::Raw(s) => orig.push_str(s),
            SkelPart::Unit(i) => {
                let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                let raw = u.codes.get(HIDDEN).and_then(|c| c.standalone.as_deref()).unwrap_or("");
                let s = orig.len();
                orig.push_str(raw);
                order.push(*i);
                at.push((s, orig.len()));
            }
        }
    }
    let limits = Limits::new().with_max_units(usize::MAX).with_max_lines(usize::MAX);
    let (spans, extra_see, extra_fonts, lang) = if kind == Kind::Hhp {
        let (u, l) = hhp::walk(&orig, &limits)?;
        (u, Vec::new(), Vec::new(), l)
    } else {
        let w = sitemap::walk(&orig, &limits)?;
        (w.units, w.see_also, w.fonts, None)
    };
    if spans.len() != order.len() || spans.iter().zip(&at).any(|(s, a)| (s.start, s.end) != *a) {
        return Err(Error::Write("the skeleton does not match its file; open the file again with the current parser".into()));
    }
    if order.is_empty() && mo.output_encoding == OutputEncoding::Keep {
        return Ok(original_bytes(&orig, tag));
    }
    let translated = order.iter().any(|&i| doc.units[i].translate && doc.units[i].segments.iter().any(|s| s.has_target()));
    let target = doc.target_lang.as_deref().filter(|t| !t.trim().is_empty());
    // Localization OS's build_localized always stamps; Bilingua only when something is translated, unless asked.
    let localized = target.is_some() && (translated || mo.stamp_target_always);
    let fallback = doc.units.iter().any(|u| u.codes.get(HIDDEN).and_then(|c| c.label.as_deref()) == Some(FALLBACK_MARK));
    if !localized && edits_none(&spans, &order, doc) && mo.output_encoding == OutputEncoding::Keep {
        let raw = order.first().and_then(|&i| doc.units[i].codes.get(HIDDEN)).and_then(|c| c.close.as_deref());
        return match raw {
            Some(b) => bilingua_model::b64::decode(b).map_err(|e| Error::Write(format!("the skeleton's original bytes are damaged: {e}"))),
            None => Ok(original_bytes(&orig, tag)),
        };
    }

    // #676: a value a browser does not read at this same span is written opaque to every HTML5 tag state
    let mut agreed: Option<std::collections::HashSet<browser::ValueSpan>> = None;
    let mut value_out = |sp: &Span, new: &str| -> String {
        let e = sitemap::escape_attr(new, sp.quote);
        let a = agreed.get_or_insert_with(|| browser::agreed_value_spans(&orig));
        if a.contains(&(sp.start, sp.end, sp.quote)) {
            e
        } else {
            sitemap::strict_escape(&e)
        }
    };
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut renamed: HashMap<&str, String> = HashMap::new();
    for (sp, &i) in spans.iter().zip(&order) {
        let unit = &doc.units[i];
        let Some(new) = changed(unit, &sp.text) else { continue };
        if new.contains('\0') {
            return Err(Error::Unwritable { unit: Some(unit.id.clone()), reason: format!("the translation of {} holds a NUL character, which would make the file unreadable as text", unit.id) });
        }
        let rendered = if kind == Kind::Hhp {
            hhp::value(&new, sp.kind)
        } else {
            value_out(sp, &new)
        };
        if kind != Kind::Hhp {
            renamed.entry(sp.text.as_str()).or_insert(new);
        }
        edits.push((sp.start, sp.end, rendered));
    }
    for sp in &extra_see {
        if let Some(new) = renamed.get(sp.text.as_str()) {
            let v = value_out(sp, new);
            edits.push((sp.start, sp.end, v));
        }
    }
    if kind != Kind::Hhp && !edits.is_empty() {
        // #676: the translated sitemap must parse, in a browser, into the source's elements
        // a translation that a browser would read back as other elements cannot be written (the file itself is fine)
        browser::check_build(&orig, &apply(&orig, &edits)).map_err(|reason| Error::Unwritable { unit: None, reason })?;
    }
    let (out_tag, cp, label) = out_encoding(tag, target, localized);
    let ansi = enc::is_ansi(tag);
    // Native mode (issue 12): a character the code page cannot hold, in a sitemap value, is written as a numeric
    // character reference, which HTML Help reads in a `param` value. The LocOS profile keeps LocOS's refusal
    // (`HtmlHelpEncodingError`), and so does a project file or any other place.
    if kind != Kind::Hhp && ansi && !fallback && mo.output_encoding == OutputEncoding::Keep && !mo.is_profile(bilingua_model::profile::LOCOS) {
        if let Some(codec) = enc::Codec::from_tag(&out_tag) {
            for edit in &mut edits {
                edit.2 = numeric_references(&edit.2, codec);
            }
            // a value that was not translated is written as it was, unless the target code page cannot hold it
            for sp in spans.iter().chain(&extra_see) {
                if !edits.iter().any(|e| (e.0, e.1) == (sp.start, sp.end)) {
                    let raw = &orig[sp.start..sp.end];
                    let fixed = numeric_references(raw, codec);
                    if fixed != raw {
                        edits.push((sp.start, sp.end, fixed));
                    }
                }
            }
        }
    }
    let mut all = edits.clone();
    if localized && ansi {
        all.extend(extra_fonts.iter().filter_map(|f| font_edit(f, cp)));
    }
    if localized && kind == Kind::Hhp {
        if let (Some(l), Some(loc)) = (&lang, target) {
            if let Some((lcid, name)) = lcid::target_lcid(loc) {
                let v = format!("0x{lcid:x} {name}");
                let v = v.trim_end().to_string();
                if v != l.text {
                    all.push((l.start, l.end, v));
                }
            }
        }
    }
    let text1 = apply(&orig, &all);
    if mo.output_encoding == OutputEncoding::Utf8 {
        // Always UTF-8: a declared charset says so; a UTF-8 BOM stays, a UTF-16 BOM goes.
        let mut with = all.clone();
        if let Some((s, en)) = enc::declared_span(&orig) {
            let m = (s, en, "UTF-8".to_string());
            if !enc::label_is_utf8(&orig[s..en]) && !all.iter().any(|(a, b, _)| m.0 < *b && *a < m.1) {
                with.push(m);
            }
        }
        let t = apply(&orig, &with);
        return Ok(if tag == "utf-8-bom" { enc::encode_unicode(&t, tag) } else { t.into_bytes() });
    }
    if !ansi {
        return Ok(enc::encode_unicode(&text1, tag));
    }
    let e = enc::Codec::from_tag(&out_tag).ok_or_else(|| Error::Write(format!("unknown encoding {out_tag}")))?;
    let meta = |t: &str, to_utf8: bool| -> Option<(usize, usize, String)> {
        let (s, en) = enc::declared_span(t)?;
        if to_utf8 {
            return Some((s, en, "UTF-8".into()));
        }
        let (lab, _) = enc::cp_info(cp?)?;
        (!enc::label_is_code_page(&t[s..en], cp?)).then(|| (s, en, lab.to_string()))
    };
    let overlaps = |m: &(usize, usize, String), t: &str| -> bool {
        // positions are in `orig`; a meta match inside an edited value is left alone
        let _ = t;
        all.iter().any(|(s, en, _)| m.0 < *en && *s < m.1)
    };
    match enc::encode_ansi(&text1, e) {
        Ok(bytes) => {
            if localized {
                if let Some(m) = meta(&orig, false) {
                    if !overlaps(&m, &orig) {
                        let mut with = all.clone();
                        with.push(m);
                        let text2 = apply(&orig, &with);
                        if let Ok(b2) = enc::encode_ansi(&text2, e) {
                            return Ok(b2);
                        }
                    }
                }
            }
            Ok(bytes)
        }
        Err(enc::Unwritable(ch)) => {
            if fallback {
                let mut with = all.clone();
                if let Some(m) = meta(&orig, true) {
                    if !overlaps(&m, &orig) {
                        with.push(m);
                    }
                }
                return Ok(apply(&orig, &with).into_bytes());
            }
            Err(Error::Unwritable {
                unit: None,
                reason: format!(
                "The translated HTML Help file cannot be written in the {label} code page: the character {ch:?} (U+{:04X}) has no representation there, and HTML Help 1.x reads Contents, Index and project files in the help language's Windows ANSI code page. Fix the text (a translation or an untranslated value), or extract with the format option htmlhelp.utf8_fallback to write UTF-8 instead (HTML Help Workshop will not show those characters correctly).",
                ch as u32
            ),
            })
        }
    }
}

/// `text` with every character `codec` cannot write as a numeric character reference (`&#272;`).
fn numeric_references(text: &str, codec: enc::Codec) -> String {
    if text.is_ascii() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 8);
    let mut buf = [0u8; 4];
    for c in text.chars() {
        if c.is_ascii() || enc::encode_ansi(c.encode_utf8(&mut buf), codec).is_ok() {
            out.push(c);
        } else {
            out.push_str(&format!("&#{};", c as u32));
        }
    }
    out
}

fn head_text(bytes: &[u8]) -> String {
    let head = &bytes[..bytes.len().min(4096)];
    if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
        let be = head[0] == 0xFE;
        let units: Vec<u16> = head[2..].chunks_exact(2).map(|p| if be { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) }).collect();
        return String::from_utf16_lossy(&units).to_lowercase();
    }
    String::from_utf8_lossy(head).to_lowercase()
}

fn is_sitemap(h: &str) -> bool {
    h.contains("text/sitemap")
}

fn is_index(h: &str) -> bool {
    h.contains("name=\"keyword\"") || h.contains("name=keyword") || h.contains("\"see also\"")
}

macro_rules! filter {
    ($ty:ident, $kind:expr, $id:expr, $ext:expr, $sniff:expr) => {
        #[doc = concat!("The `", $id, "` filter.")]
        pub struct $ty;

        impl Filter for $ty {
            fn id(&self) -> &'static str {
                $id
            }
            fn extensions(&self) -> &'static [&'static str] {
                &[$ext]
            }
            fn sniff(&self, bytes: &[u8]) -> bool {
                let h = head_text(bytes);
                $sniff(&h)
            }
            fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
                bilingua_model::check_own_options(self, &opts.format_options)?;
                extract_impl($kind, bytes, opts, None)
            }
            fn format_option_keys(&self) -> &'static [FormatOptionKey] {
                FORMAT_OPTIONS
            }
            fn merge_with(&self, doc: &Document, mo: &MergeOptions) -> Result<Vec<u8>, Error> {
                bilingua_model::check_own_options(self, &mo.format_options)?;
                merge_impl($kind, doc, mo)
            }
        }
    };
}

filter!(HhcFilter, Kind::Hhc, "hhc", "hhc", |h: &str| is_sitemap(h) && !is_index(h));
filter!(HhkFilter, Kind::Hhk, "hhk", "hhk", |h: &str| is_sitemap(h) && is_index(h));
filter!(HhpFilter, Kind::Hhp, "hhp", "hhp", |h: &str| {
    h.lines().any(|l| l.trim() == "[options]")
        && ["contents file=", "compiled file=", "index file=", "default topic=", "language=0x"].iter().any(|k| h.contains(k))
});

impl HhcFilter {
    /// `extract` for a contents file of a project whose `Language=` has the ANSI code page `hhp_code_page` (see
    /// [`hhp_code_page`]); used only for a file that declares no charset.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, hhp_code_page: Option<u16>) -> Result<Document, Error> {
        extract_impl(Kind::Hhc, bytes, opts, hhp_code_page)
    }
}

impl HhkFilter {
    /// Same as [`HhcFilter::extract_with`], for an index file.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, hhp_code_page: Option<u16>) -> Result<Document, Error> {
        extract_impl(Kind::Hhk, bytes, opts, hhp_code_page)
    }
}

impl HhpFilter {
    /// `extract` for a project file that declares no charset but is known to be in the ANSI code page `code_page`
    /// (for example from [`hhp_code_page`] on the file itself, or from the build system). Localization OS reads a
    /// project file as Windows-1252 unless it is UTF-8 or UTF-16.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, code_page: Option<u16>) -> Result<Document, Error> {
        extract_impl(Kind::Hhp, bytes, opts, code_page)
    }
}
