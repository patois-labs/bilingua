//! The Xcode localization bundle (`.xcloc`) filter. Behaviour follows Localization OS (`xcloc_filter.py`): the
//! bundle arrives zipped, every `Localized Contents/*.xliff` goes through the XLIFF reader, and a merge rewrites
//! only the XLIFF files that received a translation (plus the target language, when one is asked for).

use crate::json;
use bilingua_model::guard;
use bilingua_model::{Document, Error, ExtractOptions, Filter, Granularity, LimitError, LimitKind, Limits, SegState, Skeleton, ZipData, ZipEntry};
use bilingua_xliff::Doc;
use bilingua_zip::{write_zip_parts, Body, RawEntry, ZipLimits};
use regex::Regex;
use std::sync::OnceLock;

pub(crate) const FORMAT: &str = "xcloc";
const CONTENTS: &str = "contents.json";
const LOCALIZED: &str = "Localized Contents/";
/// `contents.json` is a few hundred bytes in every Xcode export (LocOS cap).
const MAX_CONTENTS_BYTES: usize = 1024 * 1024;
/// The skeleton keeps the whole archive under this name.
const ARCHIVE: &str = "\0bilingua-xcloc-archive";

/// The Xcode localization bundle filter.
#[derive(Debug, Default, Clone, Copy)]
pub struct Xcloc;

fn parse_err(msg: impl Into<String>) -> Error {
    Error::Parse { format: FORMAT.into(), msg: msg.into() }
}

fn not_a_bundle() -> Error {
    parse_err("This .xcloc file is not a zipped Xcode localization catalog. Compress the .xcloc folder (Finder: Compress), rename the .zip to .xcloc and upload that.")
}

/// A reader error from the zip layer in the words of this format.
fn zip_err(e: Error) -> Error {
    match e {
        Error::Parse { msg, .. } if msg.contains("not a zip file") || msg.contains("unreadable zip") => not_a_bundle(),
        Error::Parse { msg, .. } if msg.contains("appears twice") => parse_err("This .xcloc archive lists the same file twice. It was not processed."),
        e => e,
    }
}

struct Layout {
    contents: usize,
    xliffs: Vec<usize>,
}

fn layout(entries: &[RawEntry]) -> Result<Layout, Error> {
    let live: Vec<(usize, &RawEntry)> = entries.iter().enumerate().filter(|(_, e)| !e.name.starts_with("__MACOSX/")).collect();
    let mut roots: Vec<Layout> = Vec::new();
    for &(ci, e) in &live {
        let n = e.name.as_str();
        if e.meta.dir || !(n == CONTENTS || n.ends_with(&format!("/{CONTENTS}"))) {
            continue;
        }
        let prefix = &n[..n.len() - CONTENTS.len()];
        let loc = format!("{prefix}{LOCALIZED}");
        let xliffs: Vec<usize> = live
            .iter()
            .filter(|(_, m)| !m.meta.dir && m.name.starts_with(&loc) && !m.name[loc.len()..].contains('/') && m.name.to_ascii_lowercase().ends_with(".xliff"))
            .map(|(i, _)| *i)
            .collect();
        if !xliffs.is_empty() {
            roots.push(Layout { contents: ci, xliffs });
        }
    }
    match roots.len() {
        0 => Err(parse_err("This .xcloc archive has no contents.json with a 'Localized Contents/<language>.xliff' beside it, so it is not an Xcode localization catalog.")),
        1 => Ok(roots.remove(0)),
        n => Err(parse_err(format!("This .xcloc archive holds {n} localization catalogs. Upload one .xcloc per file."))),
    }
}

struct Contents {
    development_region: String,
}

fn read_contents(e: &RawEntry, limits: &Limits) -> Result<Contents, Error> {
    if e.data.len() > MAX_CONTENTS_BYTES {
        return Err(LimitError::new(LimitKind::InputBytes, format!("This .xcloc has a contents.json over {} KB. It was not processed.", MAX_CONTENTS_BYTES / 1024)).into());
    }
    let body = e.data.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&e.data);
    let text = std::str::from_utf8(body).map_err(|_| parse_err("This .xcloc has a contents.json that is not valid JSON."))?;
    let root = json::parse(text, limits.max_xml_depth).map_err(|er| match er {
        json::JsonError::Deep => LimitError::new(LimitKind::XmlDepth, "This .xcloc has a contents.json that is nested too deeply.").into(),
        json::JsonError::Syntax(_) => parse_err("This .xcloc has a contents.json that is not valid JSON."),
    })?;
    let Some(o) = root.obj() else { return Err(parse_err("This .xcloc has a contents.json that is not a JSON object.")) };
    let s = |k: &str| o.get(k).and_then(|n| n.str()).unwrap_or("").to_string();
    Ok(Contents { development_region: s("developmentRegion") })
}

fn xliff_doc(e: &RawEntry, limits: &Limits) -> Result<Doc, Error> {
    guard::check_xml(&e.data, limits)?;
    Doc::from_bytes(e.data.clone()).map_err(|er| Error::Parse { format: FORMAT.into(), msg: format!("{}: {er}", e.name) })
}

/// The names in the zip's central directory, read straight from the bytes: the zip reader merges two entries
/// of one name, and LocOS refuses an archive that lists the same file twice.
fn directory_has_duplicates(z: &[u8]) -> bool {
    let from = z.len().saturating_sub(22 + 65535);
    let Some(mut i) = z.len().checked_sub(22) else { return false };
    while !z[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06]) {
        if i <= from {
            return false;
        }
        i -= 1;
    }
    let count = u16::from_le_bytes([z[i + 10], z[i + 11]]) as usize;
    let mut at = u32::from_le_bytes([z[i + 16], z[i + 17], z[i + 18], z[i + 19]]) as usize;
    let mut seen: std::collections::HashSet<&[u8]> = std::collections::HashSet::new();
    for _ in 0..count {
        // checked: on a 32-bit target (wasm) a hostile offset near u32::MAX would overflow
        let Some(names_at) = at.checked_add(46) else { return false };
        let Some(h) = z.get(at..names_at) else { return false };
        if !h.starts_with(&[0x50, 0x4b, 0x01, 0x02]) {
            return false;
        }
        let n = u16::from_le_bytes([h[28], h[29]]) as usize;
        let skip = n + u16::from_le_bytes([h[30], h[31]]) as usize + u16::from_le_bytes([h[32], h[33]]) as usize;
        let Some(name) = names_at.checked_add(n).and_then(|e| z.get(names_at..e)) else { return false };
        if !seen.insert(name) {
            return true;
        }
        let Some(next) = names_at.checked_add(skip) else { return false };
        at = next;
    }
    false
}

fn read_bundle(bytes: &[u8], limits: &Limits) -> Result<Vec<RawEntry>, Error> {
    if directory_has_duplicates(bytes) {
        return Err(parse_err("This .xcloc archive lists the same file twice. It was not processed."));
    }
    bilingua_zip::read_zip(FORMAT, bytes, &ZipLimits::from(limits)).map_err(zip_err)
}

impl Filter for Xcloc {
    fn id(&self) -> &'static str {
        FORMAT
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["xcloc"]
    }

    /// A zip that names a `contents.json` and a `Localized Contents/` folder.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let has = |w: &[u8]| bytes.windows(w.len()).any(|x| x == w);
        bytes.starts_with(b"PK") && has(CONTENTS.as_bytes()) && has(LOCALIZED.as_bytes())
    }

    fn bilingual(&self) -> bool {
        true
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let limits = opts.limits();
        guard::check_input(bytes, &limits)?;
        let entries = read_bundle(bytes, &limits)?;
        let lay = layout(&entries)?;
        let contents = read_contents(&entries[lay.contents], &limits)?;
        let mut units = Vec::new();
        let mut warnings = Vec::new();
        for (k, &ei) in lay.xliffs.iter().enumerate() {
            let e = &entries[ei];
            let doc = xliff_doc(e, &limits)?;
            let mut d = bilingua_xliff::to_document(&doc, "xliff", opts)?;
            warnings.append(&mut d.warnings);
            let prefix = format!("x{}/", k + 1);
            for mut u in d.units {
                // LocOS: an empty <source> is not a unit
                if u.segments.iter().all(|s| s.source.is_empty()) {
                    continue;
                }
                u.id = format!("{prefix}{}", u.id);
                for s in &mut u.segments {
                    s.id = format!("{prefix}{}", s.id);
                }
                u.context.location = Some(format!("{} {}", e.name, u.context.key.as_deref().unwrap_or("")).trim().to_string());
                units.push(u);
                guard::check_unit_count(units.len(), &limits)?;
            }
        }
        let d = Document::new(FORMAT, units, Skeleton::zip(vec![ZipEntry::raw(ARCHIVE, bytes.to_vec())], bilingua_model::parser_info!())).with_source_lang(opts.source_lang.clone().or_else(|| (!contents.development_region.is_empty()).then(|| contents.development_region.clone()))).with_target_lang(opts.target_lang.clone()).with_granularity(Granularity::Paragraph);
        guard::finish(d, warnings, &limits)
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Zip { entries: skel, .. } = &doc.skeleton else {
            return Err(Error::Write("xcloc needs a zip skeleton".into()));
        };
        let original = skel
            .iter()
            .find_map(|e| match (&e.data, e.name == ARCHIVE) {
                (ZipData::Raw(b), true) => Some(b.as_slice()),
                _ => None,
            })
            .ok_or_else(|| Error::Write("xcloc skeleton has no archive".into()))?;
        let limits = Limits::default();
        let entries = read_bundle(original, &limits)?;
        let lay = layout(&entries)?;
        let tag = doc.target_lang.as_deref().and_then(canonical_tag);
        let mut replaced: Vec<Option<Vec<u8>>> = vec![None; entries.len()];
        let mut any_touched = false;
        for (k, &ei) in lay.xliffs.iter().enumerate() {
            let e = &entries[ei];
            let mut x = xliff_doc(e, &limits)?;
            let prefix = format!("x{}/", k + 1);
            let mut touched = false;
            for u in doc.units.iter().filter(|u| u.translate) {
                for s in &u.segments {
                    let Some(local) = s.id.strip_prefix(&prefix) else { continue };
                    let Some(i) = x.index_of(local) else { continue };
                    if x.seg(i).locked || s.target == x.seg(i).target || !s.has_target() {
                        continue;
                    }
                    let (orig_state, native) = (x.orig_state(i), x.seg(i).native_state.map(str::to_string));
                    x.put_target_keep_state(i, s.target.clone());
                    // Xcode exports untranslated strings as state="new" and reads `state` as the review status:
                    // a target we wrote becomes "translated". A target without a state is left as Xcode wrote it.
                    let wanted = if s.state != orig_state && s.state != SegState::Draft {
                        s.state
                    } else if native.as_deref().is_some_and(|n| n != "translated") {
                        SegState::Translated
                    } else if native.is_none() && s.state == SegState::Draft {
                        // The XLIFF writer always gives a new target a state; LocOS writes none. "translated" is
                        // what Xcode reads as a finished string (documented difference).
                        SegState::Translated
                    } else {
                        orig_state
                    };
                    if wanted != orig_state {
                        x.put_state(i, wanted);
                    }
                    touched = true;
                }
            }
            if touched {
                any_touched = true;
                replaced[ei] = Some(x.to_bytes());
            }
        }
        // The language is declared only on a bundle that received a translation: an export with nothing
        // translated stays the file it was (LocOS declares it anyway; documented difference).
        if let (Some(t), true) = (&tag, opts.stamp_lang(doc, any_touched).is_some()) {
            for &ei in &lay.xliffs {
                let cur = replaced[ei].as_deref().unwrap_or(&entries[ei].data);
                if let Some(b) = stamp_target_language(cur, t) {
                    replaced[ei] = Some(b);
                }
            }
            if let Some(b) = set_target_locale(&entries[lay.contents].data, t) {
                replaced[lay.contents] = Some(b);
            }
        }
        if replaced.iter().all(Option::is_none) {
            return Ok(original.to_vec());
        }
        let parts: Vec<(bilingua_zip::EntryMeta, Body<'_>)> = entries
            .iter()
            .zip(replaced)
            .map(|(e, r)| {
                let body = match (r, &e.packed) {
                    (Some(b), _) => Body::Owned(b),
                    (None, Some((p, bytes))) => Body::Packed(p, bytes),
                    (None, None) => Body::Plain(&e.data),
                };
                (e.meta.clone(), body)
            })
            .collect();
        let mut out = write_zip_parts(&parts)?;
        if let Some(c) = archive_comment(original) {
            add_comment(&mut out, c);
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------------------------------------
// The BCP 47 tag LocOS declares for a locale (`html_lang`)

pub(crate) fn canonical_tag(locale: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8})*$").expect("static pattern"));
    let tag = locale.trim().replace('_', "-");
    if tag.len() > 64 || !re.is_match(&tag) {
        return None;
    }
    let mut parts = tag.split('-');
    let mut out = vec![parts.next()?.to_ascii_lowercase()];
    let mut extension = false;
    for sub in parts {
        let alpha = sub.chars().all(|c| c.is_ascii_alphabetic());
        let digit = sub.chars().all(|c| c.is_ascii_digit());
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let mut c = sub.to_ascii_lowercase();
            c[..1].make_ascii_uppercase();
            out.push(c);
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && digit) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

// ---------------------------------------------------------------------------------------------------------
// Text splices

/// Replaces the `targetLocale` string value in `contents.json`, leaving every other byte. `None` = no change.
fn set_target_locale(raw: &[u8], tag: &str) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(raw).ok()?;
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    let root = json::parse(body, 256).ok()?;
    let current = root.obj()?.get("targetLocale")?.str()?;
    if current == tag {
        return None;
    }
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"("targetLocale"\s*:\s*")((?:[^"\\]|\\.)*)(")"#).expect("static pattern"));
    let m = re.captures(text)?;
    let g = m.get(2)?;
    let mut out = String::with_capacity(text.len() + tag.len());
    out.push_str(&text[..g.start()]);
    out.push_str(tag);
    out.push_str(&text[g.end()..]);
    Some(out.into_bytes())
}

struct Attr {
    name: (usize, usize),
    val: (usize, usize),
}

/// The start tag at `at` (a `<`): its name end, its attributes, the index just past the last attribute (or the
/// name), and whether it is a complete tag.
fn start_tag(b: &[u8], at: usize) -> Option<(usize, Vec<Attr>, usize, usize)> {
    let mut i = at + 1;
    while i < b.len() && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/') {
        i += 1;
    }
    let name_end = i;
    let mut last = i;
    let mut attrs = Vec::new();
    loop {
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        match b.get(i)? {
            b'>' => return Some((name_end, attrs, last, i + 1)),
            b'/' => {
                if b.get(i + 1) == Some(&b'>') {
                    return Some((name_end, attrs, last, i + 2));
                }
                return None;
            }
            _ => {}
        }
        let ns = i;
        while i < b.len() && !matches!(b[i], b'=' | b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/') {
            i += 1;
        }
        let ne = i;
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if b.get(i) != Some(&b'=') {
            return None;
        }
        i += 1;
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        let q = *b.get(i)?;
        if q != b'"' && q != b'\'' {
            return None;
        }
        let vs = i + 1;
        let ve = vs + b[vs..].iter().position(|c| *c == q)?;
        attrs.push(Attr { name: (ns, ne), val: (vs, ve) });
        i = ve + 1;
        last = i;
    }
}

/// Sets `target-language` on every `<file>` of an XLIFF 1.2 (or `trgLang` on the root of an XLIFF 2). `None` =
/// nothing to change.
fn stamp_target_language(b: &[u8], tag: &str) -> Option<Vec<u8>> {
    struct Ed {
        at: usize,
        end: usize,
        text: String,
    }
    let mut edits: Vec<Ed> = Vec::new();
    let mut v2 = None;
    let mut i = 0;
    while i < b.len() {
        let Some(p) = b[i..].iter().position(|c| *c == b'<') else { break };
        i += p;
        let rest = &b[i..];
        if rest.starts_with(b"<!--") {
            i = find(b, i + 4, b"-->").map_or(b.len(), |e| e + 3);
            continue;
        }
        if rest.starts_with(b"<![CDATA[") {
            i = find(b, i + 9, b"]]>").map_or(b.len(), |e| e + 3);
            continue;
        }
        if rest.starts_with(b"<?") {
            i = find(b, i + 2, b"?>").map_or(b.len(), |e| e + 2);
            continue;
        }
        if rest.starts_with(b"<!") {
            // DOCTYPE and the like: skip to the closing '>'
            i = rest.iter().position(|c| *c == b'>').map_or(b.len(), |e| i + e + 1);
            continue;
        }
        let Some((name_end, attrs, last, end)) = start_tag(b, i) else {
            i += 1;
            continue;
        };
        let name = &b[i + 1..name_end];
        let attr_named = |n: &str| attrs.iter().find(|a| &b[a.name.0..a.name.1] == n.as_bytes());
        let local = name.rsplit(|c| *c == b':').next().unwrap_or(name);
        let want_attr = if local == b"xliff" && v2.is_none() {
            let version = attr_named("version").map(|a| &b[a.val.0..a.val.1]).unwrap_or_default();
            v2 = Some(version.starts_with(b"2"));
            if v2 == Some(true) {
                Some("trgLang")
            } else {
                None
            }
        } else if local == b"file" && v2 == Some(false) {
            Some("target-language")
        } else {
            None
        };
        if let Some(an) = want_attr {
            match attr_named(an) {
                Some(a) if &b[a.val.0..a.val.1] != tag.as_bytes() => edits.push(Ed { at: a.val.0, end: a.val.1, text: tag.to_string() }),
                Some(_) => {}
                None => edits.push(Ed { at: last, end: last, text: format!(" {an}=\"{tag}\"") }),
            }
        }
        i = end;
    }
    if edits.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(b.len() + edits.len() * 32);
    let mut pos = 0;
    for e in &edits {
        out.extend_from_slice(&b[pos..e.at]);
        out.extend_from_slice(e.text.as_bytes());
        pos = e.end;
    }
    out.extend_from_slice(&b[pos..]);
    Some(out)
}

fn find(b: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= b.len() {
        return None;
    }
    b[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

// ---------------------------------------------------------------------------------------------------------
// The archive comment (the zip writer drops it; LocOS keeps it)

fn archive_comment(z: &[u8]) -> Option<&[u8]> {
    let from = z.len().saturating_sub(22 + 65535);
    let mut i = z.len().checked_sub(22)?;
    loop {
        if z[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06]) {
            let n = u16::from_le_bytes([z[i + 20], z[i + 21]]) as usize;
            let c = z.get(i + 22..i + 22 + n)?;
            return (!c.is_empty()).then_some(c);
        }
        if i <= from {
            return None;
        }
        i -= 1;
    }
}

fn add_comment(z: &mut Vec<u8>, comment: &[u8]) {
    let Some(i) = z.len().checked_sub(22) else { return };
    if z[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06]) && z[i + 20] == 0 && z[i + 21] == 0 {
        z[i + 20..i + 22].copy_from_slice(&(comment.len() as u16).to_le_bytes());
        z.extend_from_slice(comment);
    }
}
