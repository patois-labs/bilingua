//! Bilingua subtitle filters for YouTube SBV (`.sbv`) and SubViewer 2.0 / MicroDVD (`.sub`). See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod mdvd;
mod scan;

use bilingua_model::*;
use scan::*;
use std::collections::{BTreeMap, HashSet};

/// Default characters per line, stored in `context.extra["max_cpl"]` like the SRT filter.
pub const DEFAULT_MAX_CPL: &str = "42";

const RAW: &str = "raw"; // the cue text as written
const KIND: &str = "kind"; // sbv, sv (SubViewer 2.0) or mdvd (MicroDVD)
const JOINER: &str = "joiner"; // what a line break is written as
const HEADS: &str = "heads"; // MicroDVD: pairs whose code opens its line, comma separated
const FPS: &str = "fps"; // MicroDVD: the text may not read as a frame-rate declaration
const VOBSUB_MAGIC: &[u8] = &[0, 0, 1, 0xBA];
const SNIFF_LINES: usize = 400;

/// The YouTube SBV filter.
pub struct Sbv;
/// The SubViewer 2.0 / MicroDVD filter (told apart by content).
pub struct Sub;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dialect {
    Sbv,
    SubViewer,
    MicroDvd,
}

impl Dialect {
    fn kind(self) -> &'static str {
        match self {
            Dialect::Sbv => "sbv",
            Dialect::SubViewer => "sv",
            Dialect::MicroDvd => "mdvd",
        }
    }
}

fn perr(fmt: &str, msg: impl Into<String>) -> Error {
    Error::Parse { format: fmt.into(), msg: msg.into() }
}

// ------------------------------------------------------------------------------------------ sniffing

/// Tells SubViewer 2.0 from MicroDVD in decoded `.sub` text, or refuses with a plain message.
fn sniff_text(text: &str) -> Result<Dialect, Error> {
    let (mut micro, mut sv, mut sv1, mut seen) = (0usize, 0usize, 0usize, 0usize);
    for l in lines(text) {
        if py_strip(l.text).is_empty() {
            continue;
        }
        seen += 1;
        if mdvd_cue(l.text).is_some() {
            micro += 1;
        } else if time_pair(l.text).is_some() {
            sv += 1;
        } else if sv1_time(l.text) {
            sv1 += 1;
        }
        if seen >= SNIFF_LINES {
            break;
        }
    }
    if micro > 0 && sv > 0 {
        return Err(perr(
            "sub",
            "This .sub file mixes MicroDVD ({frame}{frame}text) and SubViewer (hh:mm:ss.xx,hh:mm:ss.xx) lines, so its format is ambiguous. Save it as one format (or as .srt) and upload it again.",
        ));
    }
    if micro > 0 {
        return Ok(Dialect::MicroDvd);
    }
    if sv > 0 {
        return Ok(Dialect::SubViewer);
    }
    if sv1 > 0 {
        return Err(perr(
            "sub",
            "This .sub file is SubViewer 1.0 ([hh:mm:ss] time lines), which is not supported. Save it as SubViewer 2.0 or .srt and upload it again.",
        ));
    }
    Err(perr(
        "sub",
        "This .sub file is not MicroDVD ({frame}{frame}text) or SubViewer 2.0 (hh:mm:ss.xx,hh:mm:ss.xx) subtitles. Save it as .srt and upload it again.",
    ))
}

fn head_text(bytes: &[u8]) -> String {
    let mut n = bytes.len().min(64 * 1024);
    while n > 0 && n < bytes.len() && (bytes[n] & 0xC0) == 0x80 {
        n -= 1;
    }
    let s = String::from_utf8_lossy(&bytes[..n]).into_owned();
    s.trim_start_matches('\u{feff}').to_string()
}

// ------------------------------------------------------------------------------------------ slots

struct Slot {
    start: usize,
    end: usize,
    inl: Vec<Inline>,
    codes: BTreeMap<String, CodeData>,
    joiner: String,
    heads: Vec<String>,
    fps: bool,
    line: usize,
    t0: String,
    t1: String,
}

struct Cur {
    first: Option<usize>,
    last_end: usize,
    line: usize,
    t0: String,
    t1: String,
}

fn br_find(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    (0..b.len().saturating_sub(3)).find(|&i| b[i] == b'[' && b[i + 1].eq_ignore_ascii_case(&b'b') && b[i + 2].eq_ignore_ascii_case(&b'r') && b[i + 3] == b']')
}

fn replace_br(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = br_find(rest) {
        out.push_str(&rest[..p]);
        out.push('\n');
        rest = &rest[p + 4..];
    }
    out.push_str(rest);
    out
}

fn block_slots(text: &str, sv: bool, limits: &Limits) -> Result<Vec<Slot>, Error> {
    let nl = first_newline(text);
    let mut slots: Vec<Slot> = Vec::new();
    let mut cur: Option<Cur> = None;
    let close = |cur: &mut Option<Cur>, slots: &mut Vec<Slot>| -> Result<(), Error> {
        let Some(c) = cur.take() else { return Ok(()) };
        let Some(first) = c.first else { return Ok(()) };
        let (start, end) = core(text, first, c.last_end);
        if start >= end {
            return Ok(());
        }
        let raw = &text[start..end];
        let mut unit = split_newlines(raw).join("\n");
        let has_nl = unit.contains('\n');
        let mut joiner = nl.to_string();
        if sv {
            unit = replace_br(&unit);
            joiner = match br_find(raw) {
                Some(p) => raw[p..p + 4].to_string(),
                None if unit.contains('\n') => first_newline(raw).to_string(),
                None => "[br]".to_string(),
            };
        } else if has_nl {
            joiner = first_newline(raw).to_string();
        }
        let unit = unit.split('\n').map(py_strip).collect::<Vec<_>>().join("\n");
        if py_strip(&unit).is_empty() {
            return Ok(());
        }
        slots.push(Slot { start, end, inl: vec![Inline::Text(unit)], codes: BTreeMap::new(), joiner, heads: vec![], fps: false, line: c.line, t0: c.t0, t1: c.t1 });
        guard::check_unit_count(slots.len(), limits)
    };
    for l in lines(text) {
        if let Some((a, b)) = time_pair(l.text) {
            close(&mut cur, &mut slots)?;
            cur = Some(Cur { first: None, last_end: 0, line: 0, t0: a.into(), t1: b.into() });
        } else if py_strip(l.text).is_empty() {
            close(&mut cur, &mut slots)?;
        } else if let Some(c) = cur.as_mut() {
            if c.first.is_none() {
                c.first = Some(l.start);
                c.line = l.no;
            }
            c.last_end = l.end;
        }
    }
    close(&mut cur, &mut slots)?;
    Ok(slots)
}

fn mdvd_slots(text: &str, limits: &Limits) -> Result<Vec<Slot>, Error> {
    let mut slots: Vec<Slot> = Vec::new();
    for l in lines(text) {
        let Some((plen, first, last)) = mdvd_cue(l.text) else { continue };
        let (mut start, mut end) = core(text, l.start + plen, l.end);
        if start >= end {
            continue;
        }
        let mut body = &text[start..end];
        if matches!(first, "0" | "1") && matches!(last, "0" | "1") && fps_text(body) {
            continue;
        }
        let mut lead = false;
        let mut cut = lead_end(body);
        if cut > 0 {
            lead = true;
            let rest = &body[cut..];
            let mut bare = String::new();
            let mut p = 0;
            for (a, b) in find_codes(rest) {
                bare.push_str(&rest[p..a]);
                p = b;
            }
            bare.push_str(&rest[p..]);
            if bare.contains('|') {
                if let Some((a, _)) = find_codes(&body[..cut]).into_iter().find(|&(a, _)| body.as_bytes()[a + 1].is_ascii_lowercase()) {
                    cut = a;
                }
            }
            (start, end) = core(text, start + cut, end);
            if start >= end {
                continue;
            }
            body = &text[start..end];
        }
        let built = mdvd::build(body);
        if !mdvd::has_words(&built.inl) {
            continue;
        }
        slots.push(Slot {
            start,
            end,
            inl: built.inl,
            codes: built.codes,
            joiner: "|".into(),
            heads: built.heads,
            fps: matches!(first, "0" | "1") && matches!(last, "0" | "1") && (!lead || cut == 0),
            line: l.no,
            t0: first.into(),
            t1: last.into(),
        });
        guard::check_unit_count(slots.len(), limits)?;
    }
    Ok(slots)
}

fn extract(fmt: &'static str, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    if fmt == "sub" && bytes.starts_with(VOBSUB_MAGIC) {
        return Err(perr(
            "sub",
            "This .sub file is a VobSub image subtitle (binary, paired with a .idx file), not text. Convert it to SRT with an OCR tool, then upload the .srt.",
        ));
    }
    let limits = opts.limits();
    let mut dec = enc::decode_text(bytes, &enc::DecodeOptions::new(opts).mark_before_legacy())?;
    let text_owned = std::mem::take(&mut dec.text);
    let text = text_owned.as_str();
    let dialect = if fmt == "sbv" { Dialect::Sbv } else { sniff_text(text)? };
    let slots = match dialect {
        Dialect::MicroDvd => mdvd_slots(text, &limits)?,
        d => block_slots(text, d == Dialect::SubViewer, &limits)?,
    };
    let mut units = Vec::with_capacity(slots.len());
    let mut parts = Vec::with_capacity(slots.len() * 2 + 1);
    let mut cursor = 0;
    for (i, s) in slots.into_iter().enumerate() {
        let mut u = Unit::new(format!("c{}", i + 1), s.inl);
        u.codes = s.codes;
        u.set_hidden(RAW, &text[s.start..s.end]);
        u.set_hidden(KIND, dialect.kind());
        u.set_hidden(JOINER, &s.joiner);
        if !s.heads.is_empty() {
            u.set_hidden(HEADS, s.heads.join(","));
        }
        if s.fps {
            u.set_hidden(FPS, "1");
        }
        u.context.kind = Some("subtitle".into());
        u.context.key = Some((i + 1).to_string());
        u.context.location = Some(format!("line {}", s.line));
        u.context.extra.insert("start".into(), s.t0);
        u.context.extra.insert("end".into(), s.t1);
        u.context.extra.insert("max_cpl".into(), DEFAULT_MAX_CPL.into());
        units.push(u);
        if s.start > cursor {
            parts.push(SkelPart::Raw(text[cursor..s.start].to_string()));
        }
        parts.push(SkelPart::Unit(i));
        cursor = s.end;
    }
    if cursor < text.len() {
        parts.push(SkelPart::Raw(text[cursor..].to_string()));
    }
    guard::finish(
        Document::new(fmt, units, Skeleton::text(parts, dec.encoding.as_str(), parser_info!())).with_options(opts),
        std::mem::take(&mut dec.warnings),
        &limits,
    )
}

// ------------------------------------------------------------------------------------------ merge

fn get<'a>(u: &'a Unit, id: &str) -> Option<&'a str> {
    u.hidden(id)
}

fn render_block(target: &[Inline], joiner: &str, sv: bool) -> Result<String, String> {
    let text: String = target.iter().filter_map(|i| if let Inline::Text(t) = i { Some(t.as_str()) } else { None }).collect();
    let t = py_strip(&text);
    let out = if joiner.contains(['\r', '\n']) {
        // a line break is a real line break in the file: a line of only Unicode spaces would read as a blank line
        // and end the cue (LocOS writes it), so such lines are dropped
        let mut out = String::with_capacity(text.len());
        let mut first = true;
        for piece in split_newlines(t) {
            let p = piece.trim_matches([' ', '\t']);
            if py_strip(p).is_empty() {
                continue;
            }
            if !first {
                out.push_str(joiner);
            }
            out.push_str(p);
            first = false;
        }
        out
    } else {
        collapse_newlines(t, joiner)
    };
    let probe = if sv { replace_br(&out) } else { out.clone() };
    if py_strip(&probe.replace(joiner, "")).is_empty() {
        return Err("the translation is empty, so the cue would drop out of the file".into());
    }
    for line in split_newlines(&out) {
        if time_pair(line).is_some() {
            return Err("a line of the translation reads as a timecode".into());
        }
        if sv && mdvd_cue(line).is_some() {
            return Err("a line of the translation reads as a MicroDVD cue".into());
        }
    }
    Ok(out)
}

/// Writes one unit (cue number `cue`, 1-based); true when the translation (not the original text) was written. A
/// translation that cannot be written is refused with LocOS's sentence, which names the cue (B-SIDE W6 st-4).
fn write_unit(u: &Unit, cue: usize, out: &mut String) -> Result<bool, Error> {
    let raw = get(u, RAW).unwrap_or("");
    if !u.translate || u.segments.iter().all(|s| !s.has_target()) {
        out.push_str(raw);
        return Ok(false);
    }
    let new = u.output();
    if new == u.source() {
        out.push_str(raw);
        return Ok(false);
    }
    let kind = get(u, KIND).unwrap_or("sbv");
    let joiner = get(u, JOINER).unwrap_or("\n");
    let r = match kind {
        "mdvd" => {
            let heads: HashSet<&str> = get(u, HEADS).map(|h| h.split(',').collect()).unwrap_or_default();
            mdvd::render(&new, &u.codes, &heads, get(u, FPS).is_some())
        }
        k => render_block(&new, joiner, k == "sv"),
    };
    match r {
        Ok(s) => {
            out.push_str(&s);
            Ok(true)
        }
        Err(m) => Err(Error::Unwritable {
            unit: Some(u.context.key.clone().unwrap_or_else(|| u.id.clone())),
            reason: format!("Cue {cue} was not exported: {m}. Edit that translation and export again."),
        }),
    }
}

fn merge(fmt: &str, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    check_parser(doc, &parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write(format!("{fmt} needs a text skeleton")));
    };
    let mut out = String::new();
    let mut written = false;
    for p in parts {
        match p {
            SkelPart::Raw(s) => out.push_str(s),
            SkelPart::Unit(i) => {
                let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                written |= write_unit(u, *i + 1, &mut out)?;
            }
        }
    }
    // LocOS (B-bytes): a build that writes nothing returns the upload exactly, in its own encoding.
    // the encoder drops one leading U+FEFF (the byte-order mark it writes itself): a second one is text
    if out.starts_with('\u{feff}') {
        out.insert(0, '\u{feff}');
    }
    // (`output_encoding: Utf8` alone writes UTF-8 literally; the LocOS profile keeps LocOS's behaviour)
    if !written && (opts.output_encoding == bilingua_model::OutputEncoding::Keep || opts.is_profile(bilingua_model::profile::LOCOS)) {
        return enc::encode_text(&out, encoding);
    }
    enc::encode_text_with(&out, encoding, opts)
}

// ------------------------------------------------------------------------------------------ filters

impl Filter for Sbv {
    fn id(&self) -> &'static str {
        "sbv"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["sbv"]
    }
    /// Loose: the first non-blank line is an `H:MM:SS.fff,H:MM:SS.fff` timecode pair and the file has no
    /// SubViewer header or MicroDVD cue.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = head_text(bytes);
        let first = lines(&head).find(|l| !py_strip(l.text).is_empty());
        first.is_some_and(|l| time_pair(l.text).is_some()) && !lines(&head).take(SNIFF_LINES).any(|l| mdvd_cue(l.text).is_some())
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        extract("sbv", bytes, opts)
    }
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        merge("sbv", doc, opts)
    }
}

impl Filter for Sub {
    fn id(&self) -> &'static str {
        "sub"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["sub"]
    }
    /// MicroDVD cues, or SubViewer 2.0 with its `[INFORMATION]` or `[SUBTITLE]` header. A bare timecode file is SBV.
    fn sniff(&self, bytes: &[u8]) -> bool {
        if bytes.starts_with(VOBSUB_MAGIC) {
            return false;
        }
        let head = head_text(bytes);
        match sniff_text(&head) {
            Ok(Dialect::MicroDvd) => true,
            Ok(Dialect::SubViewer) => {
                let up = head.to_ascii_uppercase();
                up.contains("[INFORMATION]") || up.contains("[SUBTITLE]")
            }
            _ => false,
        }
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        extract("sub", bytes, opts)
    }
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        merge("sub", doc, opts)
    }
}
