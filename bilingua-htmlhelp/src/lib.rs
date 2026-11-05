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
//! the code page cannot hold (or writes UTF-8 with [`MergeOptions::utf8_fallback`], which HTML Help Workshop shows
//! wrongly).

/// Skeleton schema number: bump only when this crate's skeleton layout changes.
pub const SKELETON_SCHEMA: u32 = 1;

mod data;
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

/// How a merge behaves where the Localization OS default can be switched.
#[derive(Debug, Clone, Copy, Default)]
pub struct MergeOptions {
    /// A text the ANSI code page cannot hold is written as UTF-8 (and a declared meta charset says so) instead of
    /// refusing the merge. HTML Help Workshop does not show those characters correctly. Off by default.
    pub utf8_fallback: bool,
}

/// The ANSI code page of an HTML Help project's language: `Language=0x419 Russian` in `[OPTIONS]` gives 1251. `None`
/// when the file names no language, names one with no ANSI code page, or cannot be read. Pass it to
/// [`HhcFilter::extract_with`] for the project's contents and index files.
pub fn hhp_code_page(bytes: &[u8]) -> Option<u16> {
    let limits = Limits::default();
    // A project in a code page the default decode cannot read (Shift_JIS) still has an ASCII `Language=` line.
    let t = match load(bytes, &limits, None) {
        Ok((t, _)) => t,
        Err(_) if bytes.len() as u64 <= limits.max_input_bytes => bytes.iter().map(|&b| b as char).collect(),
        Err(_) => return None,
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
    let hint = hint.or_else(|| opts.source_lang.as_deref().and_then(target_code_page));
    let (text, tag) = load(bytes, &limits, hint)?;
    let spans = if kind == Kind::Hhp { hhp::walk(&text, &limits)?.0 } else { sitemap::walk(&text, &limits)?.units };
    if spans.is_empty() {
        let msg = if kind == Kind::Hhp {
            "This HTML Help project file has no Title= or window caption to translate.".to_string()
        } else {
            format!("This HTML Help {} file has no entry names (param Name / Keyword inside OBJECT type=\"text/sitemap\") to translate.", kind.what())
        };
        return Err(Error::Parse { format: kind.id().into(), msg });
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
            CodeData {
                standalone: Some(text[sp.start..sp.end].to_string()),
                open: owned(sp.quote),
                close: None,
                label: None,
            },
        );
        unit.context.kind = Some(match sp.kind {
            "w" => "window".to_string(),
            k => k.to_string(),
        });
        parts.push(SkelPart::Unit(units.len()));
        units.push(unit);
    }
    parts.push(SkelPart::Raw(text[pos..].to_string()));
    guard::finish(
        Document {
            format: kind.id().into(),
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(parts, tag, bilingua_model::parser_info!()),
            ..Default::default()
        },
        Vec::new(),
        &limits,
    )
}

fn changed(unit: &Unit, old: &str) -> Option<String> {
    if !unit.translate || unit.segments.iter().all(|s| s.target.is_empty()) {
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

fn same_charset(declared: &str, cp: u16) -> bool {
    let n: String = declared.chars().filter(|c| !matches!(c, '-' | '_')).collect::<String>().to_lowercase();
    n == format!("windows{cp}") || n == format!("cp{cp}")
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

fn merge_impl(kind: Kind, doc: &Document, mo: &MergeOptions) -> Result<Vec<u8>, Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
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
    let limits = Limits { max_units: usize::MAX, max_lines: usize::MAX, ..Limits::default() };
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
    let translated = order.iter().any(|&i| doc.units[i].translate && doc.units[i].segments.iter().any(|s| !s.target.is_empty()));
    let target = doc.target_lang.as_deref().filter(|t| !t.trim().is_empty());
    let localized = translated && target.is_some();

    let regions = if kind == Kind::Hhp { Vec::new() } else { sitemap::risky_regions(&orig) };
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut renamed: HashMap<&str, String> = HashMap::new();
    for (sp, &i) in spans.iter().zip(&order) {
        let unit = &doc.units[i];
        let Some(new) = changed(unit, &sp.text) else { continue };
        if new.contains('\0') {
            return Err(Error::Write(format!("the translation of {} holds a NUL character, which would make the file unreadable as text", unit.id)));
        }
        let rendered = if kind == Kind::Hhp {
            hhp::value(&new, sp.kind)
        } else {
            let e = sitemap::escape_attr(&new, sp.quote);
            if sitemap::in_regions(&regions, sp.start) {
                sitemap::strict_escape(&e)
            } else {
                e
            }
        };
        if kind != Kind::Hhp {
            renamed.entry(sp.text.as_str()).or_insert(new);
        }
        edits.push((sp.start, sp.end, rendered));
    }
    for sp in &extra_see {
        if let Some(new) = renamed.get(sp.text.as_str()) {
            let e = sitemap::escape_attr(new, sp.quote);
            edits.push((sp.start, sp.end, if sitemap::in_regions(&regions, sp.start) { sitemap::strict_escape(&e) } else { e }));
        }
    }
    let (out_tag, cp, label) = out_encoding(tag, target, localized);
    let ansi = enc::is_ansi(tag);
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
        (!same_charset(&t[s..en], cp?)).then(|| (s, en, lab.to_string()))
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
            if mo.utf8_fallback {
                let mut with = all.clone();
                if let Some(m) = meta(&orig, true) {
                    if !overlaps(&m, &orig) {
                        with.push(m);
                    }
                }
                return Ok(apply(&orig, &with).into_bytes());
            }
            Err(Error::Write(format!(
                "The translated HTML Help file cannot be written in the {label} code page: the character {ch:?} (U+{:04X}) has no representation there, and HTML Help 1.x reads Contents, Index and project files in the help language's Windows ANSI code page. Fix the text (a translation or an untranslated value), or merge with MergeOptions::utf8_fallback to write UTF-8 instead (HTML Help Workshop will not show those characters correctly).",
                ch as u32
            )))
        }
    }
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

        impl $ty {
            /// `merge` with explicit options.
            pub fn merge_with(&self, doc: &Document, mo: &MergeOptions) -> Result<Vec<u8>, Error> {
                merge_impl($kind, doc, mo)
            }
        }

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
                extract_impl($kind, bytes, opts, None)
            }
            fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
                merge_impl($kind, doc, &MergeOptions::default())
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
