//! The XLIFF family under the Localization OS profile (Track D group B): a port of LocOS's `xliff_filter.py` and
//! `cat_xliff.py`, so the adapter gets LocOS's units, keys and files.
//!
//! * Units: one per `<source>` (plain XLIFF 1.2 and 2.0), or one per `<mrk mtype="seg">` of a `<seg-source>`
//!   (SDLXLIFF, and any XLIFF 1.2 file that has one). Protected units (translate="no", approved, a final or
//!   signed-off target, vendor locks and confirmations) are left out, as are blank sources and, for plain XLIFF,
//!   sources that hold only inline codes. `locos.p_index` is LocOS's key: the `<source>` position for a
//!   whole-source unit, `SEG_BASE` plus the position among every seg `mrk` for a segment unit; skipped units keep
//!   their number.
//! * Inline codes are numbered as LocOS numbers them (every element child of the source, in document
//!   order; point elements are not entered): `x`, `ph`, `bpt`, `ept`, `it`, `sc`, `ec`, `sm`, `em` are standalone
//!   codes (an `<x id>` / `<ph id>` that stands for no markup side and no unnumbered argument is free), anything
//!   else is a pair. A tag's id is its LocOS token number. Phrase MXLIFF text marks (`{1}`, `{2>..<2}`) are codes
//!   too, numbered after the elements.
//! * Writing: a plain XLIFF without `<seg-source>` is re-serialised the way lxml writes it (namespace declarations
//!   first, double quotes, `<x/>` for empty elements), with the original's declaration, BOM, line ends and trailing
//!   white space (LocOS `xml_like`); a new `<target>` follows its `<source>`; `state` new / needs-translation
//!   becomes translated; `target-language` (1.2, every `<file>`) or `trgLang` (2.0) is set to the target. The
//!   vendor formats and seg-source files are a byte splice: only target contents and the vendor's status
//!   attributes change (SDL `conf="Translated"`, memoQ `ManuallyConfirmed`, WorldServer `finished`), and a file
//!   that has a `target-language` keeps it.
//!
//! Documents made under the profile carry the original as a `Skeleton::Binary`; `merge` picks this writer by that.

use bilingua_xmltree as dom;

use bilingua_model::pytext::py_strip;
use bilingua_model::{profile, CodeData, Document, Error, ExtractOptions, Granularity, Inline, MergeOptions, Skeleton, TagKind, Unit, UnitContext};
use dom::{Dom, Kind};
use std::collections::{BTreeMap, HashMap};

/// The first key of a seg-source segment unit (LocOS `SEG_BASE`).
pub const SEG_BASE: i64 = 1_000_000_000;
const SDL_NS: &str = "http://sdl.com/FileTypes/SdlXliff/1.0";
const MQ_NS: &str = "MQXliff";
const M_NS: &str = "http://www.memsource.com/mxlf/2.0";
const POINT: [&str; 9] = ["x", "ph", "bpt", "ept", "it", "sc", "ec", "sm", "em"];

fn perr(format: &str, msg: impl std::fmt::Display) -> Error {
    Error::Parse { format: format.to_string(), msg: msg.to_string() }
}

fn werr(msg: impl std::fmt::Display) -> Error {
    Error::Write(msg.to_string())
}

// ------------------------------------------------------------------------------------------- tokens

/// One piece of LocOS token text: plain text, or a token with its number.
#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Text(String),
    Open(usize),
    Close(usize),
    /// A point token; `true` when it is free (`p`).
    Point(usize, bool),
}

/// What a token number stands for: an element of the source (its node), or an MXLIFF text mark.
#[derive(Debug, Clone)]
enum Code {
    Node(usize),
    /// A point mark (`{1}`) or a pair (`{2>`, `<2}`).
    Mark(String, Option<String>),
}

fn tok_string(toks: &[Tok]) -> String {
    let mut s = String::new();
    for t in toks {
        match t {
            Tok::Text(x) => s.push_str(x),
            Tok::Open(n) => s.push_str(&format!("\u{27e6}g{n}\u{27e7}")),
            Tok::Close(n) => s.push_str(&format!("\u{27e6}/g{n}\u{27e7}")),
            Tok::Point(n, true) => s.push_str(&format!("\u{27e6}p{n}\u{27e7}")),
            Tok::Point(n, false) => s.push_str(&format!("\u{27e6}x{n}\u{27e7}")),
        }
    }
    s
}

/// A LocOS placeholder token at the start of `s` (`⟦/?[a-z]?\d+⟧`): (closing, letter, number, byte length).
fn token_at(s: &str) -> Option<(bool, Option<char>, &str, usize)> {
    let rest = s.strip_prefix('\u{27e6}')?;
    let (close, rest) = match rest.strip_prefix('/') {
        Some(r) => (true, r),
        None => (false, rest),
    };
    let mut chars = rest.char_indices();
    let (letter, digits_at) = match chars.next() {
        Some((_, c)) if c.is_ascii_lowercase() => (Some(c), 1),
        Some((_, c)) if c.is_ascii_digit() => (None, 0),
        _ => return None,
    };
    let digits = &rest[digits_at..];
    let n = digits.bytes().take_while(u8::is_ascii_digit).count();
    if n == 0 || !digits[n..].starts_with('\u{27e7}') {
        return None;
    }
    let len = s.len() - rest.len() + digits_at + n + '\u{27e7}'.len_utf8();
    Some((close, letter, &digits[..n], len))
}

/// A run of text, or a token as (closing, letter, number).
type Piece = Result<String, (bool, Option<char>, String)>;

/// Splits LocOS text into text runs and tokens (`_PLACEHOLDER_RE.finditer`).
fn split_tokens(s: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < s.len() {
        if let Some((close, letter, num, len)) = token_at(&s[i..]) {
            if !text.is_empty() {
                out.push(Ok(std::mem::take(&mut text)));
            }
            out.push(Err((close, letter, num.to_string())));
            i += len;
            continue;
        }
        let c = s[i..].chars().next().unwrap();
        text.push(c);
        i += c.len_utf8();
    }
    if !text.is_empty() {
        out.push(Ok(text));
    }
    out
}

/// `strip_placeholders(text).strip()` is empty.
fn only_tokens(s: &str) -> bool {
    let rest: String = split_tokens(s).into_iter().filter_map(Result::ok).collect();
    py_strip(&rest).is_empty()
}

/// An unnumbered printf argument or `{}` (LocOS `_ORDER_BOUND_CODE`, matched whole).
fn order_bound(code: &str) -> bool {
    if code == "{}" {
        return true;
    }
    let Some(rest) = code.strip_prefix('%') else { return false };
    let d = rest.bytes().take_while(u8::is_ascii_digit).count();
    if d > 0 && rest[d..].starts_with('$') {
        return false;
    }
    let mut r = rest.trim_start_matches(['-', '+', ' ', '#', '0', '\'']);
    if let Some(x) = r.strip_prefix('*') {
        r = x;
    } else {
        r = r.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    if let Some(x) = r.strip_prefix('.') {
        r = match x.strip_prefix('*') {
            Some(y) => y,
            None => {
                let n = x.bytes().take_while(u8::is_ascii_digit).count();
                if n == 0 {
                    return false;
                }
                &x[n..]
            }
        };
    }
    let conv = |s: &str| s.len() == 1 && "@diouxXcCsSpaAeEfFgG".contains(s);
    if conv(r) {
        return true;
    }
    ["hh", "h", "ll", "l", "q", "L", "z", "t", "j", "I64", "I32", "I"].iter().any(|l| r.strip_prefix(l).is_some_and(conv))
}

fn pair_side_id(s: &str) -> bool {
    s.starts_with("START_") || s.starts_with("CLOSE_")
}

/// What an `<x>` / `<ph>` stands for (`_native_codes`).
fn native_codes(d: &Dom, el: usize) -> Vec<String> {
    let mut codes: Vec<String> = ["equiv-text", "equiv", "disp"].iter().filter_map(|a| d.get(el, a).map(str::to_string)).collect();
    let lead = d.lead_text(el);
    if !lead.is_empty() {
        codes.push(lead);
    }
    if let Some(r) = d.get(el, "dataRef") {
        if let Some(unit) = d.ancestor(el, "unit") {
            let hit = d.iter(unit).into_iter().find(|&x| d.local(x) == "data" && d.get(x, "id") == Some(r));
            if let Some(x) = hit {
                codes.push(d.itertext(x));
            }
        }
    }
    codes.into_iter().map(|c| py_strip(&c).to_string()).filter(|c| !c.is_empty()).collect()
}

/// `_free_point`: an `<x id>` / `<ph id>` placeholder that may move in the translation.
fn free_point(d: &Dom, el: usize, local: &str) -> bool {
    if !(local == "x" || local == "ph") {
        return false;
    }
    let Some(id) = d.get(el, "id") else { return false };
    if pair_side_id(id) || pair_side_id(d.get(el, "equiv").unwrap_or("")) {
        return false;
    }
    if ["bold", "italic", "underline", "link"].contains(&d.get(el, "ctype").unwrap_or("").to_lowercase().as_str()) {
        return false;
    }
    !native_codes(d, el).iter().any(|c| (c.starts_with('<') && !c.ends_with("/>")) || order_bound(c))
}

/// `_tokenize_source`: the LocOS token stream of a source element and what each number stands for.
fn tokenize_source(d: &Dom, src: usize) -> (Vec<Tok>, Vec<Code>) {
    let mut toks = Vec::new();
    let mut codes = Vec::new();
    fn walk(d: &Dom, el: usize, toks: &mut Vec<Tok>, codes: &mut Vec<Code>) {
        for &c in &d.nodes[el].children {
            let n = &d.nodes[c];
            if n.kind == Kind::Text {
                toks.push(Tok::Text(n.value.clone()));
                continue;
            }
            // a comment or processing instruction is dropped (its tail text stays): never translatable text, no number
            if n.kind != Kind::Element {
                continue;
            }
            codes.push(Code::Node(c));
            let num = codes.len();
            let local = d.local(c);
            if n.kind == Kind::Element && POINT.contains(&local.as_str()) {
                toks.push(Tok::Point(num, free_point(d, c, &local)));
                continue;
            }
            toks.push(Tok::Open(num));
            walk(d, c, toks, codes);
            toks.push(Tok::Close(num));
        }
    }
    walk(d, src, &mut toks, &mut codes);
    (join_text(toks), codes)
}

fn join_text(toks: Vec<Tok>) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::with_capacity(toks.len());
    for t in toks {
        match (out.last_mut(), t) {
            (_, Tok::Text(s)) if s.is_empty() => {}
            (Some(Tok::Text(a)), Tok::Text(b)) => a.push_str(&b),
            (_, t) => out.push(t),
        }
    }
    out
}

/// A Phrase text mark at the start of `s`: `{id}`, `{id>` or `<id}` (`_TEXT_TAG`), as (kind 0/1/2, id, length).
fn text_mark(s: &str) -> Option<(u8, &str, usize)> {
    let b = s.as_bytes();
    let alnum = |x: &[u8]| x.iter().take_while(|c| c.is_ascii_alphanumeric()).count();
    match b.first()? {
        b'{' => {
            let n = alnum(&b[1..]);
            if !(1..=12).contains(&n) {
                return None;
            }
            match b.get(1 + n)? {
                b'}' => Some((0, &s[1..1 + n], n + 2)),
                b'>' => Some((1, &s[1..1 + n], n + 2)),
                _ => None,
            }
        }
        b'<' => {
            let n = alnum(&b[1..]);
            if !(1..=12).contains(&n) || b.get(1 + n) != Some(&b'}') {
                return None;
            }
            Some((2, &s[1..1 + n], n + 2))
        }
        _ => None,
    }
}

/// `_text_tags`: MXLIFF text marks in the text runs become tokens numbered after the element codes.
fn text_tags(toks: Vec<Tok>, codes: &mut Vec<Code>) -> Vec<Tok> {
    // every match, in order: (tok index, byte start, kind, id, len)
    let mut matches: Vec<(usize, usize, u8, String, usize)> = Vec::new();
    for (ti, t) in toks.iter().enumerate() {
        let Tok::Text(s) = t else { continue };
        let mut i = 0;
        while i < s.len() {
            if let Some((k, id, len)) = text_mark(&s[i..]) {
                matches.push((ti, i, k, id.to_string(), len));
                i += len;
            } else {
                i += s[i..].chars().next().unwrap().len_utf8();
            }
        }
    }
    if matches.is_empty() {
        return toks;
    }
    let mut pair_of: HashMap<usize, usize> = HashMap::new();
    let mut stack: Vec<(String, usize)> = Vec::new();
    for (i, m) in matches.iter().enumerate() {
        if m.2 == 1 {
            stack.push((m.3.clone(), i));
        } else if m.2 == 2 {
            if let Some(j) = stack.iter().rposition(|(id, _)| *id == m.3) {
                let o = stack[j].1;
                pair_of.insert(o, i);
                pair_of.insert(i, o);
                stack.truncate(j);
            }
        }
    }
    let raw = |m: &(usize, usize, u8, String, usize)| -> String {
        let Tok::Text(s) = &toks[m.0] else { unreachable!() };
        s[m.1..m.1 + m.4].to_string()
    };
    let mut num_of: HashMap<usize, usize> = HashMap::new();
    let mut repl: Vec<Tok> = Vec::with_capacity(matches.len());
    for (i, m) in matches.iter().enumerate() {
        if let (Some(&j), 1) = (pair_of.get(&i), m.2) {
            codes.push(Code::Mark(raw(m), Some(raw(&matches[j]))));
            num_of.insert(i, codes.len());
            repl.push(Tok::Open(codes.len()));
        } else if let Some(&o) = pair_of.get(&i) {
            repl.push(Tok::Close(num_of[&o]));
        } else {
            codes.push(Code::Mark(raw(m), None));
            repl.push(Tok::Point(codes.len(), false));
        }
    }
    let mut out = Vec::new();
    let mut mi = 0;
    for (ti, t) in toks.into_iter().enumerate() {
        let Tok::Text(s) = t else {
            out.push(t);
            continue;
        };
        let mut pos = 0;
        while mi < matches.len() && matches[mi].0 == ti {
            let m = &matches[mi];
            out.push(Tok::Text(s[pos..m.1].to_string()));
            out.push(repl[mi].clone());
            pos = m.1 + m.4;
            mi += 1;
        }
        out.push(Tok::Text(s[pos..].to_string()));
    }
    join_text(out)
}

fn tokenize(d: &Dom, el: usize, marks: bool) -> (Vec<Tok>, Vec<Code>) {
    let (toks, mut codes) = tokenize_source(d, el);
    if marks {
        let toks = text_tags(toks, &mut codes);
        return (toks, codes);
    }
    (toks, codes)
}

/// `text.strip()` on a token stream (tokens are never white space).
fn strip_toks(mut toks: Vec<Tok>) -> Vec<Tok> {
    if let Some(Tok::Text(s)) = toks.first_mut() {
        *s = bilingua_model::pytext::py_lstrip(s).to_string();
    }
    if let Some(Tok::Text(s)) = toks.last_mut() {
        *s = bilingua_model::pytext::py_rstrip(s).to_string();
    }
    toks.retain(|t| !matches!(t, Tok::Text(s) if s.is_empty()));
    toks
}

// --------------------------------------------------------------------------------------- protection

fn has_content(d: &Dom, el: Option<usize>) -> bool {
    el.is_some_and(|e| !py_strip(&d.itertext(e)).is_empty() || d.nodes[e].children.iter().any(|&c| d.nodes[c].kind != Kind::Text))
}

fn translate_no(d: &Dom, tu: usize) -> bool {
    let mut el = Some(tu);
    while let Some(e) = el {
        if ["trans-unit", "group", "unit", "file"].contains(&d.local(e).as_str()) {
            if let Some(v) = d.get(e, "translate") {
                return v.trim().eq_ignore_ascii_case("no");
            }
        }
        el = d.nodes[e].parent;
    }
    false
}

fn low(v: Option<&str>) -> String {
    v.unwrap_or("").trim().to_lowercase()
}

fn is_gs4tr(d: &Dom, el: usize) -> bool {
    d.nsmap(el).iter().any(|(_, u)| u.contains("gs4tr"))
}

fn gs4tr_attr(d: &Dom, el: Option<usize>, local: &str) -> String {
    let Some(el) = el else { return String::new() };
    d.attrs_ns(el).into_iter().find(|(ns, l, _)| l == local && ns.as_deref().is_some_and(|n| n.contains("gs4tr"))).map(|(_, _, v)| v.trim().to_lowercase()).unwrap_or_default()
}

fn is_iws(d: &Dom, el: usize) -> bool {
    if !d.is_el(el) {
        return false;
    }
    let n = &d.nodes[el];
    n.name.split_once(':').is_some_and(|(p, _)| p == "iws") || n.ns.as_deref().unwrap_or("").contains("idiominc.com")
}

fn iws_status(d: &Dom, tu: usize) -> Option<usize> {
    let meta = d.element_children(tu).find(|&c| is_iws(d, c) && d.local(c) == "segment-metadata")?;
    d.element_children(meta).find(|&c| is_iws(d, c) && d.local(c) == "status")
}

/// `_unit_skip`: why a whole trans-unit is protected, or "".
fn unit_skip(d: &Dom, tu: usize, target: Option<usize>) -> &'static str {
    if translate_no(d, tu) {
        return "translate-no";
    }
    if low(d.get(tu, "approved")) == "yes" {
        return "approved";
    }
    if target.is_some_and(|t| ["final", "signed-off"].contains(&low(d.get(t, "state")).as_str())) {
        return "final";
    }
    if d.local(tu) == "segment" && low(d.get(tu, "state")) == "final" {
        return "final";
    }
    if ["locked", "true"].contains(&low(d.get_ns(tu, MQ_NS, "locked")).as_str()) {
        return "locked";
    }
    if low(d.get_ns(tu, M_NS, "locked")) == "true" {
        return "locked";
    }
    if is_gs4tr(d, tu) {
        if gs4tr_attr(d, Some(tu), "locked") == "true" || gs4tr_attr(d, target, "locked") == "true" {
            return "locked";
        }
        let state = target.map(|t| low(d.get(t, "state"))).unwrap_or_default();
        if has_content(d, target) && state != "new" && !state.starts_with("needs-") {
            return "confirmed";
        }
    }
    let mq = low(d.get_ns(tu, MQ_NS, "status"));
    if ["proofread", "reviewer1confirmed", "reviewer2confirmed"].contains(&mq.as_str()) {
        return "approved";
    }
    if mq == "manuallyconfirmed" && has_content(d, target) {
        return "confirmed";
    }
    let confirmed = d.get_ns(tu, M_NS, "confirmed").unwrap_or("").trim().to_string();
    if !confirmed.is_empty() && confirmed != "0" && has_content(d, target) {
        return "confirmed";
    }
    if let Some(st) = iws_status(d, tu) {
        if low(d.get(st, "lock_status")) == "locked" {
            return "locked";
        }
        if low(d.get(st, "translation_status")) == "finished" {
            return "final";
        }
    }
    ""
}

fn is_ns(d: &Dom, el: usize, ns: &str, local: &str) -> bool {
    d.nodes[el].ns.as_deref() == Some(ns) && d.nodes[el].name.rsplit(':').next() == Some(local)
}

fn sdl_seg(d: &Dom, tu: usize, mid: &str) -> (Option<usize>, Option<usize>) {
    let Some(defs) = d.element_children(tu).find(|&c| is_ns(d, c, SDL_NS, "seg-defs")) else { return (None, None) };
    let seg = d.element_children(defs).find(|&s| is_ns(d, s, SDL_NS, "seg") && d.get(s, "id") == Some(mid));
    (Some(defs), seg)
}

fn seg_skip(d: &Dom, tu: usize, mid: &str, tgt_mrk: Option<usize>) -> &'static str {
    let (_, Some(seg)) = sdl_seg(d, tu, mid) else { return "" };
    if low(d.get(seg, "locked")) == "true" {
        return "locked";
    }
    let conf = low(d.get(seg, "conf"));
    if conf == "approvedtranslation" || conf == "approvedsignoff" {
        return "approved";
    }
    if conf == "translated" && has_content(d, tgt_mrk) {
        return "confirmed";
    }
    ""
}

fn seg_mrks(d: &Dom, container: Option<usize>) -> Vec<usize> {
    let Some(c) = container else { return Vec::new() };
    d.iter(c).into_iter().filter(|&m| d.local(m) == "mrk" && d.get(m, "mtype") == Some("seg")).collect()
}

fn has_seg_source(d: &Dom) -> bool {
    d.iter(d.root).into_iter().any(|e| d.local(e) == "seg-source" && !seg_mrks(d, Some(e)).is_empty())
}

fn source_elements(d: &Dom) -> Vec<usize> {
    d.iter(d.root)
        .into_iter()
        .filter(|&e| d.local(e) == "source" && d.nodes[e].parent.is_some_and(|p| ["trans-unit", "segment"].contains(&d.local(p).as_str())))
        .collect()
}

#[derive(Debug, Clone)]
struct Slot {
    key: i64,
    tu: usize,
    src: usize,
    mrk: Option<usize>,
    skip: &'static str,
}

/// `cat_xliff.walk`: every unit slot in document order, protected ones included.
fn walk(d: &Dom) -> Vec<Slot> {
    let mut slots = Vec::new();
    let mut seg_n: i64 = 0;
    for (idx, src) in source_elements(d).into_iter().enumerate() {
        let tu = d.nodes[src].parent.unwrap();
        let target = d.child(tu, "target");
        let us = unit_skip(d, tu, target);
        let mrks = if d.local(tu) == "trans-unit" { seg_mrks(d, d.child(tu, "seg-source")) } else { Vec::new() };
        if mrks.is_empty() {
            slots.push(Slot { key: idx as i64, tu, src, mrk: None, skip: us });
            continue;
        }
        let tgt: HashMap<String, usize> = seg_mrks(d, target).into_iter().map(|m| (d.get(m, "mid").unwrap_or("").to_string(), m)).collect();
        for m in mrks {
            let mid = d.get(m, "mid").unwrap_or("").to_string();
            let skip = if us.is_empty() { seg_skip(d, tu, &mid, tgt.get(&mid).copied()) } else { us };
            slots.push(Slot { key: SEG_BASE + seg_n, tu, src: m, mrk: Some(m), skip });
            seg_n += 1;
        }
    }
    slots
}

// --------------------------------------------------------------------------------------- reading

/// The decoded text of a vendor file, its encoding and BOM (`cat_xliff._decode`).
#[derive(Clone, Copy, PartialEq)]
enum Codec {
    Utf8,
    Utf16Le,
    Utf16Be,
}

fn decode_cat(raw: &[u8]) -> Result<(String, Codec, Vec<u8>), String> {
    if let Some(r) = raw.strip_prefix(&[0xEF, 0xBB, 0xBF][..]) {
        return String::from_utf8(r.to_vec()).map(|t| (t, Codec::Utf8, raw[..3].to_vec())).map_err(|_| "This XLIFF file is not valid UTF-8. Re-save it as UTF-8.".into());
    }
    if raw.starts_with(&[0xFF, 0xFE]) || raw.starts_with(&[0xFE, 0xFF]) {
        let le = raw[0] == 0xFF;
        let units: Vec<u16> = raw[2..].chunks(2).map(|c| if c.len() < 2 { 0xFFFD } else if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        if !raw.len().is_multiple_of(2) {
            return Err("This XLIFF file is not valid UTF-16.".into());
        }
        let t = String::from_utf16(&units).map_err(|_| "This XLIFF file is not valid UTF-16.".to_string())?;
        return Ok((t, if le { Codec::Utf16Le } else { Codec::Utf16Be }, raw[..2].to_vec()));
    }
    let head: String = raw[..raw.len().min(200)].iter().map(|&b| if b.is_ascii() { b as char } else { '\u{fffd}' }).collect();
    if let Some(enc) = declared_encoding(&head) {
        let e = enc.to_lowercase().replace('_', "-");
        if e != "utf-8" && e != "utf8" {
            return Err(format!("This XLIFF file is encoded as {enc}; only UTF-8 and UTF-16 are supported. Re-save it as UTF-8 in the CAT tool."));
        }
    }
    String::from_utf8(raw.to_vec()).map(|t| (t, Codec::Utf8, Vec::new())).map_err(|_| "This XLIFF file is not valid UTF-8. Re-save it as UTF-8.".into())
}

/// `encoding\s*=\s*["']([A-Za-z0-9_.-]+)` anywhere in `head`.
fn declared_encoding(head: &str) -> Option<String> {
    let mut at = 0;
    while let Some(i) = head[at..].find("encoding") {
        let rest = head[at + i + 8..].trim_start();
        if let Some(r) = rest.strip_prefix('=') {
            let r = r.trim_start();
            if let Some(r) = r.strip_prefix(['"', '\'']) {
                let v: String = r.chars().take_while(|c| c.is_ascii_alphanumeric() || "_.-".contains(*c)).collect();
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
        at += i + 8;
    }
    None
}

fn encode(text: &str, codec: Codec, bom: &[u8]) -> Vec<u8> {
    let mut out = bom.to_vec();
    match codec {
        Codec::Utf8 => out.extend_from_slice(text.as_bytes()),
        Codec::Utf16Le => text.encode_utf16().for_each(|u| out.extend_from_slice(&u.to_le_bytes())),
        Codec::Utf16Be => text.encode_utf16().for_each(|u| out.extend_from_slice(&u.to_be_bytes())),
    }
    out
}

/// The text a plain XLIFF is read as (lxml: the declared encoding, a BOM, else UTF-8).
fn decode_plain(raw: &[u8], format: &str) -> Result<String, Error> {
    if let Ok((t, _, _)) = decode_cat(raw) {
        return Ok(t);
    }
    let opts = ExtractOptions::new();
    bilingua_model::enc::decode_text(raw, &bilingua_model::enc::DecodeOptions::new(&opts).declared()).map(|d| d.text).map_err(|e| perr(format, e))
}

fn check_version(d: &Dom, format: &str, require_ns: Option<&str>) -> Result<(), Error> {
    let v = d.get(d.root, "version").unwrap_or("1.2");
    if d.local(d.root) != "xliff" || v.starts_with('2') {
        return Err(perr(format, format!("This is not an XLIFF 1.2 file, so it cannot be read as .{format}.")));
    }
    if let Some(ns) = require_ns {
        if !d.nsmap(d.root).iter().any(|(_, u)| u == ns) {
            return Err(perr(format, format!("This file does not declare the {} namespace ({ns}); upload a plain XLIFF as .xlf.", format.to_uppercase())));
        }
    }
    Ok(())
}

fn require_ns(format: &str) -> Option<&'static str> {
    match format {
        "sdlxliff" => Some(SDL_NS),
        "mqxliff" => Some(MQ_NS),
        "mxliff" => Some(M_NS),
        _ => None,
    }
}

fn marks_on(d: &Dom) -> bool {
    d.nsmap(d.root).iter().any(|(_, u)| u == M_NS)
}

/// One LocOS unit of an XLIFF file.
#[derive(Debug, Clone)]
pub struct LocosUnit {
    /// LocOS's key (`p_index` of a loose file).
    pub key: i64,
    /// Position of the unit's slot in LocOS's walk (packages number by it).
    pub pos: usize,
    /// The trans-unit / unit id.
    pub unit_id: String,
    /// The source with its codes; tag ids are LocOS token numbers.
    pub source: Vec<Inline>,
    /// The markup of each code, by tag id.
    pub codes: BTreeMap<String, CodeData>,
}

fn raw_markup(d: &Dom, text: &str, code: &Code) -> (Option<String>, Option<String>) {
    match code {
        Code::Mark(o, c) => (Some(o.clone()), c.clone()),
        Code::Node(n) => match d.nodes[*n].span {
            Some(sp) if sp.empty || POINT.contains(&d.local(*n).as_str()) => (Some(text[sp.start..sp.end].to_string()), None),
            Some(sp) => (Some(text[sp.start..sp.start_end].to_string()), Some(text[sp.end_start..sp.end].to_string())),
            None => (Some(String::new()), None),
        },
    }
}

fn to_unit(d: &Dom, text: &str, key: i64, pos: usize, tu: usize, toks: &[Tok], codes: &[Code]) -> LocosUnit {
    let mut source = Vec::new();
    let mut cmap = BTreeMap::new();
    for t in toks {
        let (n, kind, free) = match t {
            Tok::Text(s) => {
                source.push(Inline::text(s.clone()));
                continue;
            }
            Tok::Open(n) => (*n, TagKind::Open, false),
            Tok::Close(n) => (*n, TagKind::Close, false),
            Tok::Point(n, f) => (*n, TagKind::Standalone, *f),
        };
        let (open, close) = raw_markup(d, text, &codes[n - 1]);
        let c: &mut CodeData = cmap.entry(n.to_string()).or_default();
        match kind {
            TagKind::Open => c.open = open.clone(),
            TagKind::Close => c.close = close.clone(),
            _ => c.standalone = open.clone(),
        }
        let display = match kind {
            TagKind::Close => close.unwrap_or_default(),
            _ => open.unwrap_or_default(),
        };
        source.push(Inline::tag(n.to_string(), kind, display).with_free(free));
    }
    LocosUnit { key, pos, unit_id: d.get(tu, "id").unwrap_or("").to_string(), source, codes: cmap }
}

/// `cat_xliff.parse_units`: the units of a vendor or seg-source file, and the number of walk slots.
pub fn cat_units(raw: &[u8], format: &str, need_ns: Option<&str>) -> Result<(Vec<LocosUnit>, usize), Error> {
    let (text, _, _) = decode_cat(raw).map_err(|m| perr(format, m))?;
    let d = dom::parse(&text).map_err(|e| perr(format, format!("XLIFF is not well-formed XML: {e}")))?;
    check_version(&d, format, need_ns)?;
    let marks = marks_on(&d);
    let slots = walk(&d);
    let mut units = Vec::new();
    for (pos, s) in slots.iter().enumerate() {
        if !s.skip.is_empty() {
            continue;
        }
        let (toks, codes) = tokenize(&d, s.src, marks);
        let toks = strip_toks(toks);
        if toks.is_empty() {
            continue;
        }
        units.push(to_unit(&d, &text, s.key, pos, s.tu, &toks, &codes));
    }
    Ok((units, slots.len()))
}

/// The units of a plain XLIFF without seg-source (`XliffFilter.parse`).
fn plain_units(d: &Dom, text: &str) -> Vec<LocosUnit> {
    let mut units = Vec::new();
    for (idx, src) in source_elements(d).into_iter().enumerate() {
        let tu = d.nodes[src].parent.unwrap();
        if !unit_skip(d, tu, d.child(tu, "target")).is_empty() {
            continue;
        }
        let (toks, codes) = tokenize_source(d, src);
        let toks = strip_toks(toks);
        if toks.is_empty() || only_tokens(&tok_string(&toks)) {
            continue;
        }
        let holder = if d.local(tu) == "trans-unit" { Some(tu) } else { d.nodes[tu].parent };
        if holder.is_some_and(|h| low(d.get(h, "translate")) == "no") {
            continue;
        }
        units.push(to_unit(d, text, idx as i64, idx, tu, &toks, &codes));
    }
    units
}

/// Extracts an XLIFF family file the way Localization OS does (`format`: xliff, sdlxliff, mqxliff or mxliff).
pub fn extract(bytes: &[u8], format: &str, opts: &ExtractOptions) -> Result<Document, Error> {
    let units = if format == "xliff" {
        let text = decode_plain(bytes, format)?;
        let d = dom::parse(&text).map_err(|e| perr(format, format!("XLIFF is not well-formed XML: {e}")))?;
        if has_seg_source(&d) {
            cat_units(bytes, format, None)?.0
        } else {
            plain_units(&d, &text)
        }
    } else {
        cat_units(bytes, format, require_ns(format))?.0
    };
    let (src_lang, _) = file_langs(bytes);
    let units = units.into_iter().map(|u| document_unit(u, format, None)).collect();
    let doc = Document::new(format.to_string(), units, Skeleton::binary(bytes.to_vec(), bilingua_model::parser_info!()))
        .with_source_lang(opts.source_lang.clone().or(src_lang))
        .with_target_lang(opts.target_lang.clone())
        .with_granularity(Granularity::Paragraph);
    Ok(doc)
}

/// The source and target language the file declares (first `<file>` of 1.2, the root of 2.0).
fn file_langs(bytes: &[u8]) -> (Option<String>, Option<String>) {
    let Ok(text) = decode_plain(bytes, "xliff") else { return (None, None) };
    let Ok(d) = dom::parse(&text) else { return (None, None) };
    let nonblank = |v: Option<&str>| v.map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
    if d.get(d.root, "version").unwrap_or("").starts_with('2') {
        return (nonblank(d.get(d.root, "srcLang")), nonblank(d.get(d.root, "trgLang")));
    }
    match d.iter(d.root).into_iter().find(|&e| d.local(e) == "file") {
        Some(f) => (nonblank(d.get(f, "source-language")), nonblank(d.get(f, "target-language"))),
        None => (None, None),
    }
}

/// A Bilingua unit for a LocOS unit; `location` (a package member's path and the package-wide number) makes
/// package ids unique (`<path>|<key>`).
pub fn document_unit(u: LocosUnit, format: &str, location: Option<(&str, i64)>) -> Unit {
    let p = location.map(|(_, p)| p).unwrap_or(u.key);
    let id = match location {
        Some((path, _)) => format!("{}|{}", path.replace('%', "%25").replace('|', "%7C"), u.key),
        None => u.key.to_string(),
    };
    let mut unit = Unit::new(id, u.source);
    unit.codes = u.codes;
    let mut ctx = UnitContext::new().with_kind("segment").with_key((!u.unit_id.is_empty()).then_some(u.unit_id));
    if let Some((path, _)) = location {
        ctx = ctx.with_location(path.to_string());
    }
    ctx.extra.insert(profile::P_INDEX.to_string(), p.to_string());
    ctx.extra.insert(KEY.to_string(), u.key.to_string());
    ctx.extra.insert("xliff.dialect".to_string(), format.to_string());
    unit.context = ctx;
    unit
}

// --------------------------------------------------------------------------------------- writing

// Hides token look-alikes in literal text from the writers, and gives them back where the text is cut at its
// tokens: only a unit's real codes are tokens when a translation is written (issue 814).
use bilingua_model::profile::tokens::{escape_literal as esc_lit, unescape_literal as unesc_lit};

/// The LocOS text of a target: tag ids are token numbers. Literal text is copied as it is, so text that looks like
/// a token reads as one ([`build_cat`] takes such text).
pub fn target_text(target: &[Inline]) -> String {
    tagged_text(target, false)
}

/// [`target_text`] with literal text escaped (`esc_lit`): what the writers take, so a token look-alike in the text
/// stays text. Not part of the semver API.
#[doc(hidden)]
pub fn target_text_escaped(target: &[Inline]) -> String {
    tagged_text(target, true)
}

fn tagged_text(target: &[Inline], esc: bool) -> String {
    let mut s = String::new();
    for i in target {
        match i {
            Inline::Text(t) if esc => s.push_str(&esc_lit(t)),
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind, free, .. } => {
                let tok = match kind {
                    TagKind::Open => format!("\u{27e6}g{id}\u{27e7}"),
                    TagKind::Close => format!("\u{27e6}/g{id}\u{27e7}"),
                    TagKind::Standalone if *free => format!("\u{27e6}p{id}\u{27e7}"),
                    TagKind::Standalone => format!("\u{27e6}x{id}\u{27e7}"),
                };
                s.push_str(&tok);
            }
        }
    }
    s
}

/// `xml_target_text`: XML 1.0 characters only, Word's pasted breaks as newlines.
fn xml_target_text(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\u{b}' || c == '\u{c}' { '\n' } else { c })
        .filter(|&c| !matches!(c, '\u{0}'..='\u{8}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}'))
        .collect()
}

/// `_rebuild_target`: fills `tgt` from translated LocOS text (literal text escaped, `esc_lit`) and the source's
/// inline elements.
fn rebuild_target(d: &mut Dom, tgt: usize, text: &str, codes: &[Code]) {
    d.clear(tgt);
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    // (element or None for a phantom, key)
    let mut stack: Vec<(Option<usize>, String)> = Vec::new();
    let container = |stack: &[(Option<usize>, String)]| stack.iter().rev().find_map(|(e, _)| *e).unwrap_or(tgt);
    for piece in split_tokens(text) {
        match piece {
            Ok(t) => {
                let c = container(&stack);
                d.append_text(c, &unesc_lit(&t));
            }
            Err((true, _, key)) => {
                if stack.last().is_some_and(|(_, k)| *k == key) {
                    stack.pop();
                }
            }
            Err((false, letter, key)) => {
                let is_point = matches!(letter, Some('x') | Some('p'));
                let el = key.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|i| codes.get(i)).and_then(|c| match c {
                    Code::Node(n) => Some(*n),
                    Code::Mark(..) => None,
                });
                let Some(el) = el.filter(|_| !used.contains(&key)) else {
                    if !is_point {
                        stack.push((None, key));
                    }
                    continue;
                };
                used.insert(key.clone());
                let clone = d.deep_copy(el);
                let c = container(&stack);
                d.append(c, clone);
                if !is_point {
                    d.clear(clone);
                    stack.push((Some(clone), key));
                }
            }
        }
    }
}

/// LocOS `xml_like`: an lxml serialisation put back in the original's envelope.
fn xml_like(original: &[u8], built: &str) -> Vec<u8> {
    let lxml_decl = "<?xml version='1.0' encoding='UTF-8'?>\n";
    let (bom, src) = match original.strip_prefix(&[0xEF, 0xBB, 0xBF][..]) {
        Some(r) => (&original[..3], r),
        None => (&original[..0], original),
    };
    let whole = format!("{lxml_decl}{built}");
    if src.starts_with(&[0xFF, 0xFE]) || src.starts_with(&[0xFE, 0xFF]) {
        return whole.into_bytes();
    }
    let decl_end = |b: &[u8]| -> Option<usize> {
        if !b.starts_with(b"<?xml") {
            return None;
        }
        b.iter().position(|&c| c == b'>').filter(|&i| i > 0 && b[i - 1] == b'?').map(|i| i + 1)
    };
    let m_src = decl_end(src);
    if let Some(e) = m_src {
        let decl = String::from_utf8_lossy(&src[..e]).to_string();
        if let Some(enc) = declared_encoding(&decl) {
            let e = enc.to_lowercase().replace('_', "-");
            if !(e == "utf-8" || e == "utf8" || e == "u8") {
                return whole.into_bytes();
            }
        }
    }
    let mut out: Vec<u8> = match m_src {
        Some(e) => [&src[..e], b"\n", built.as_bytes()].concat(),
        None => built.as_bytes().to_vec(),
    };
    let trail_len = src.len() - src.iter().rposition(|c| !b" \t\r\n".contains(c)).map(|i| i + 1).unwrap_or(0);
    let trail = &src[src.len() - trail_len..];
    if !trail.is_empty() && !out.ends_with(trail) {
        let keep = out.iter().rposition(|c| !b" \t\r\n".contains(c)).map(|i| i + 1).unwrap_or(0);
        out.truncate(keep);
        out.extend_from_slice(trail);
    }
    let count = |n: &[u8]| src.windows(n.len()).filter(|w| *w == n).count();
    let (crlf, lf, cr) = (count(b"\r\n"), count(b"\n"), count(b"\r"));
    let newline: Option<&[u8]> = if crlf > 0 && crlf == lf && lf == cr {
        Some(b"\r\n")
    } else if cr > 0 && lf == 0 {
        Some(b"\r")
    } else {
        None
    };
    if let Some(nl) = newline {
        let mut o = Vec::with_capacity(out.len() + out.len() / 16);
        let mut i = 0;
        while i < out.len() {
            match out[i] {
                b'\r' if out.get(i + 1) == Some(&b'\n') => {
                    o.extend_from_slice(nl);
                    i += 2;
                }
                b'\r' | b'\n' => {
                    o.extend_from_slice(nl);
                    i += 1;
                }
                c => {
                    o.push(c);
                    i += 1;
                }
            }
        }
        out = o;
    }
    [bom, &out].concat()
}

/// Sets `target-language` on every 1.2 `<file>`, or `trgLang` on a 2.0 root (`XliffFilter.build_localized`).
fn stamp_dom(d: &mut Dom, tag: &str) {
    if d.get(d.root, "version").unwrap_or("").starts_with('2') {
        let r = d.root;
        d.set(r, "trgLang", tag);
        return;
    }
    for f in d.iter(d.root) {
        if d.local(f) == "file" {
            d.set(f, "target-language", tag);
        }
    }
}

/// `XliffFilter.build` (plain XLIFF without seg-source) plus the target language; `texts` are escaped
/// ([`written_texts_escaped`]).
fn build_plain(original: &[u8], text: &str, texts: &HashMap<i64, String>, tag: Option<&str>) -> Result<Vec<u8>, Error> {
    let mut d = dom::parse(text).map_err(|e| werr(format!("XLIFF is not well-formed XML: {e}")))?;
    for (k, src) in source_elements(&d).into_iter().enumerate() {
        let Some(new) = texts.get(&(k as i64)) else { continue };
        let tu = d.nodes[src].parent.unwrap();
        if !unit_skip(&d, tu, d.child(tu, "target")).is_empty() {
            continue;
        }
        let tgt = match d.child(tu, "target") {
            Some(t) => t,
            None => {
                let name = match d.nodes[src].name.split_once(':') {
                    Some((p, _)) => format!("{p}:target"),
                    None => "target".to_string(),
                };
                let (scope, ns) = (d.nodes[src].scope, d.nodes[src].ns.clone());
                let t = d.new_element(&name, scope, ns);
                d.add_next(src, t);
                t
            }
        };
        let (_, codes) = tokenize_source(&d, src);
        rebuild_target(&mut d, tgt, &xml_target_text(new), &codes);
        if ["new", "needs-translation"].contains(&low(d.get(tgt, "state")).as_str()) {
            d.set(tgt, "state", "translated");
        }
    }
    if let Some(t) = tag {
        stamp_dom(&mut d, t);
    }
    Ok(xml_like(original, &d.serialize()))
}

// ------------------------------------------------------------------------------ the byte splice

struct Edits {
    items: Vec<(usize, usize, usize, String)>,
}

impl Edits {
    fn add(&mut self, start: usize, end: usize, new: String) {
        let n = self.items.len();
        self.items.push((start, end, n, new));
    }
    fn apply(mut self, text: &str) -> Result<String, Error> {
        self.items.sort();
        let mut out = String::with_capacity(text.len() + 256);
        let mut pos = 0;
        for (start, end, _, new) in self.items {
            if start < pos {
                return Err(werr("overlapping XLIFF edits"));
            }
            out.push_str(&text[pos..start]);
            out.push_str(&new);
            pos = end;
        }
        out.push_str(&text[pos..]);
        Ok(out)
    }
}

fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace("]]>", "]]&gt;")
}

fn esc_attr(s: &str) -> String {
    esc_text(s).replace('"', "&quot;")
}

/// `_with_attr`: sets one attribute in a start tag's text, keeping its quoting and every other byte.
fn with_attr(tag: &str, qname: &str, value: &str) -> String {
    let b = tag.as_bytes();
    let mut i = 0;
    while let Some(off) = tag[i..].find(qname) {
        let at = i + off;
        i = at + 1;
        if at == 0 || !(b[at - 1] as char).is_whitespace() {
            continue;
        }
        let mut j = at + qname.len();
        while j < b.len() && (b[j] as char).is_whitespace() {
            j += 1;
        }
        if b.get(j) != Some(&b'=') {
            continue;
        }
        j += 1;
        while j < b.len() && (b[j] as char).is_whitespace() {
            j += 1;
        }
        let Some(&q) = b.get(j) else { continue };
        if q != b'"' && q != b'\'' {
            continue;
        }
        let Some(close) = tag[j + 1..].find(q as char) else { continue };
        let val = if q == b'"' { esc_attr(value) } else { esc_attr(value).replace('\'', "&apos;") };
        return format!("{}{}{val}{}{}", &tag[..j], q as char, q as char, &tag[j + 1 + close + 1..]);
    }
    let mut close = tag.len() - if tag.ends_with("/>") { 2 } else { 1 };
    while close > 0 && (b[close - 1] as char).is_whitespace() {
        close -= 1;
    }
    format!("{} {qname}=\"{}\"{}", &tag[..close], esc_attr(value), &tag[close..])
}

/// A literal for each token number: a point's markup, or a pair's (open, close) (`_literals`).
enum Lit {
    Point(String),
    Pair(String, String),
}

fn literals(d: &Dom, text: &str, codes: &[Code]) -> Vec<Lit> {
    codes
        .iter()
        .map(|c| match c {
            Code::Mark(o, None) => Lit::Point(esc_text(o)),
            Code::Mark(o, Some(cl)) => Lit::Pair(esc_text(o), esc_text(cl)),
            Code::Node(n) => {
                let node = &d.nodes[*n];
                match (node.kind == Kind::Element, node.span) {
                    (true, Some(sp)) if POINT.contains(&d.local(*n).as_str()) => Lit::Point(text[sp.start..sp.end].to_string()),
                    (true, Some(sp)) if sp.empty => {
                        let tag = &text[sp.start..sp.start_end];
                        Lit::Pair(format!("{}>", tag[..tag.len() - 2].trim_end()), format!("</{}>", node.name))
                    }
                    (true, Some(sp)) => Lit::Pair(text[sp.start..sp.start_end].to_string(), text[sp.end_start..sp.end].to_string()),
                    _ => Lit::Point(String::new()),
                }
            }
        })
        .collect()
}

/// `_render`: translated LocOS text (literal text escaped, `esc_lit`) as XML content, using the source's own tag
/// bytes.
fn render(text: &str, lits: &[Lit]) -> String {
    let mut out = String::new();
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stack: Vec<(String, Option<String>)> = Vec::new();
    for piece in split_tokens(text) {
        match piece {
            Ok(t) => out.push_str(&esc_text(&unesc_lit(&t))),
            Err((true, _, key)) => {
                if stack.last().is_some_and(|(k, _)| *k == key) {
                    if let Some((_, Some(close))) = stack.pop() {
                        out.push_str(&close);
                    }
                }
            }
            Err((false, letter, key)) => {
                let is_point = matches!(letter, Some('x') | Some('p'));
                let lit = key.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|i| lits.get(i));
                let Some(lit) = lit.filter(|_| !used.contains(&key)) else {
                    if !is_point {
                        stack.push((key, None));
                    }
                    continue;
                };
                used.insert(key.clone());
                match lit {
                    Lit::Pair(o, c) if is_point => {
                        out.push_str(o);
                        out.push_str(c);
                    }
                    Lit::Pair(o, c) => {
                        out.push_str(o);
                        stack.push((key, Some(c.clone())));
                    }
                    Lit::Point(p) => out.push_str(p),
                }
            }
        }
    }
    for (_, close) in stack.into_iter().rev() {
        if let Some(c) = close {
            out.push_str(&c);
        }
    }
    out
}

/// `_edges`: the white space before and after the stripped body.
fn edges(s: &str) -> (String, String) {
    let body = py_strip(s);
    if body.is_empty() {
        return (String::new(), String::new());
    }
    let i = s.find(body).unwrap_or(0);
    (s[..i].to_string(), s[i + body.len()..].to_string())
}

fn qname_for(d: &Dom, el: usize, ns: &str, local: &str) -> Option<String> {
    let p = d.prefix_for(el, ns)?;
    Some(if p.is_empty() { local.to_string() } else { format!("{p}:{local}") })
}

/// `cat_xliff.build_units`: the byte splice of a vendor or seg-source file. `tag`: the target language to put on
/// a `<file>` that has none. Every well-formed token in `texts` is read as LocOS reads it.
pub fn build_cat(raw: &[u8], texts: &HashMap<i64, String>, tag: Option<&str>) -> Result<Vec<u8>, Error> {
    build_cat_escaped(raw, &plain_to_escaped(texts), tag)
}

/// Plain LocOS text holds no escaped look-alike: doubling its own U+E000 is all the escape it needs.
fn plain_to_escaped(texts: &HashMap<i64, String>) -> HashMap<i64, String> {
    texts.iter().map(|(k, t)| (*k, t.replace('\u{e000}', "\u{e000}\u{e000}"))).collect()
}

/// [`build_cat`] for texts whose literal text is escaped ([`written_texts_escaped`]), so only real codes are
/// tokens. Not part of the semver API.
#[doc(hidden)]
pub fn build_cat_escaped(raw: &[u8], texts: &HashMap<i64, String>, tag: Option<&str>) -> Result<Vec<u8>, Error> {
    let (text, codec, bom) = decode_cat(raw).map_err(werr)?;
    let d = dom::parse(&text).map_err(|e| werr(format!("XLIFF is not well-formed XML: {e}")))?;
    let marks = marks_on(&d);
    let span = |id: usize| d.nodes[id].span.expect("parsed elements have spans");
    let mut edits = Edits { items: Vec::new() };
    let content_for = |slot: &Slot, new: &str| -> String {
        let (toks, codes) = tokenize(&d, slot.src, marks);
        let (lead, trail) = edges(&tok_string(&toks));
        let body = xml_target_text(new);
        let body = py_strip(&body);
        render(&format!("{lead}{body}{trail}"), &literals(&d, &text, &codes))
    };
    let set_inner = |edits: &mut Edits, el: usize, inner: String, tag_text: Option<String>| {
        let sp = span(el);
        let start_tag = tag_text.clone().unwrap_or_else(|| text[sp.start..sp.start_end].to_string());
        if sp.empty {
            let open = format!("{}>", start_tag[..start_tag.len() - 2].trim_end());
            edits.add(sp.start, sp.end, format!("{open}{inner}</{}>", d.nodes[el].name));
        } else {
            if let Some(t) = tag_text {
                edits.add(sp.start, sp.start_end, t);
            }
            edits.add(sp.start_end, sp.end_start, inner);
        }
    };
    // writable slots per trans-unit, in walk order
    let mut by_tu: Vec<(usize, Vec<(Slot, String)>)> = Vec::new();
    for slot in walk(&d) {
        let Some(new) = texts.get(&slot.key) else { continue };
        if !slot.skip.is_empty() {
            continue;
        }
        match by_tu.iter_mut().find(|(tu, _)| *tu == slot.tu) {
            Some((_, v)) => v.push((slot, new.clone())),
            None => by_tu.push((slot.tu, vec![(slot, new.clone())])),
        }
    }
    for (tu, items) in &by_tu {
        let tu = *tu;
        let target = d.child(tu, "target");
        let tu_sp = span(tu);
        if items[0].0.mrk.is_none() {
            let (slot, new) = &items[0];
            let inner = content_for(slot, new);
            match target {
                Some(t) => {
                    let st = low(d.get(t, "state"));
                    let resets = st == "new" || st == "needs-translation" || (is_gs4tr(&d, tu) && st.starts_with("needs-"));
                    let tsp = span(t);
                    let tag_text = resets.then(|| with_attr(&text[tsp.start..tsp.start_end], "state", "translated"));
                    set_inner(&mut edits, t, inner, tag_text);
                }
                None => {
                    let after = d.child(tu, "seg-source").unwrap_or(slot.src);
                    let name = &d.nodes[slot.src].name;
                    let tname = format!("{}target", &name[..name.len() - "source".len()]);
                    edits.add(span(after).end, span(after).end, format!("<{tname}>{inner}</{tname}>"));
                }
            }
            if d.get_ns(tu, MQ_NS, "status").is_some() {
                if let Some(q) = qname_for(&d, tu, MQ_NS, "status") {
                    edits.add(tu_sp.start, tu_sp.start_end, with_attr(&text[tu_sp.start..tu_sp.start_end], &q, "ManuallyConfirmed"));
                }
            }
            mark_iws(&d, &text, tu, &mut edits);
            continue;
        }
        let written: Vec<(String, String)> = items.iter().map(|(s, new)| (d.get(s.mrk.unwrap(), "mid").unwrap_or("").to_string(), content_for(s, new))).collect();
        match target {
            None => {
                let ss = d.child(tu, "seg-source").expect("a seg unit has a seg-source");
                let ssp = span(ss);
                let mut pieces = String::new();
                let mut pos = ssp.start_end;
                for m in seg_mrks(&d, Some(ss)) {
                    let msp = span(m);
                    let mid = d.get(m, "mid").unwrap_or("");
                    let inner = written.iter().rev().find(|(k, _)| k == mid).map(|(_, v)| v.as_str()).unwrap_or("");
                    if msp.empty {
                        let tag = &text[msp.start..msp.start_end];
                        pieces.push_str(&text[pos..msp.start]);
                        pieces.push_str(&format!("{}>{inner}</{}>", tag[..tag.len() - 2].trim_end(), d.nodes[m].name));
                        pos = msp.end;
                    } else {
                        pieces.push_str(&text[pos..msp.start_end]);
                        pieces.push_str(inner);
                        pos = msp.end_start;
                    }
                }
                pieces.push_str(&text[pos..ssp.end_start]);
                let name = &d.nodes[ss].name;
                let tname = format!("{}target", &name[..name.len() - "seg-source".len()]);
                edits.add(ssp.end, ssp.end, format!("<{tname}>{pieces}</{tname}>"));
            }
            Some(t) => {
                let tgt: HashMap<String, usize> = seg_mrks(&d, Some(t)).into_iter().map(|m| (d.get(m, "mid").unwrap_or("").to_string(), m)).collect();
                let mut seen = std::collections::HashSet::new();
                for (mid, inner) in written.iter().rev() {
                    if !seen.insert(mid.clone()) {
                        continue;
                    }
                    let Some(&tm) = tgt.get(mid) else {
                        return Err(werr(format!("The target of trans-unit {:?} has no segment {mid:?}; its segmentation does not match <seg-source>.", d.get(tu, "id").unwrap_or(""))));
                    };
                    set_inner(&mut edits, tm, inner.clone(), None);
                }
            }
        }
        let mut mids: Vec<String> = Vec::new();
        for (m, _) in &written {
            if !mids.contains(m) {
                mids.push(m.clone());
            }
        }
        mark_sdl(&d, &text, tu, &mids, &mut edits);
        mark_iws(&d, &text, tu, &mut edits);
    }
    if let Some(tag) = tag {
        for el in d.iter(d.root) {
            if d.local(el) == "file" && d.get(el, "target-language").is_none_or(|v| v.trim().is_empty()) {
                let sp = span(el);
                let start_tag = with_attr(&text[sp.start..sp.start_end], "target-language", tag);
                edits.add(sp.start, if sp.empty { sp.end } else { sp.start_end }, start_tag);
            }
        }
    }
    if edits.items.is_empty() {
        return Ok(raw.to_vec());
    }
    let out = edits.apply(&text)?;
    dom::parse(&out).map_err(|e| werr(format!("the spliced XLIFF is not well-formed: {e}")))?;
    Ok(encode(&out, codec, &bom))
}

fn mark_iws(d: &Dom, text: &str, tu: usize, edits: &mut Edits) {
    let Some(st) = iws_status(d, tu) else { return };
    if d.get(st, "translation_status").is_none() {
        return;
    }
    let sp = d.nodes[st].span.unwrap();
    edits.add(sp.start, sp.start_end, with_attr(&text[sp.start..sp.start_end], "translation_status", "finished"));
}

fn mark_sdl(d: &Dom, text: &str, tu: usize, mids: &[String], edits: &mut Edits) {
    let Some(prefix) = d.prefix_for(tu, SDL_NS) else { return };
    let (defs, _) = sdl_seg(d, tu, "");
    let mut new_segs = String::new();
    for mid in mids {
        match sdl_seg(d, tu, mid).1 {
            Some(seg) => {
                let sp = d.nodes[seg].span.unwrap();
                edits.add(sp.start, sp.start_end, with_attr(&text[sp.start..sp.start_end], "conf", "Translated"));
            }
            None => new_segs.push_str(&format!("<{prefix}:seg id=\"{}\" conf=\"Translated\"/>", esc_attr(mid))),
        }
    }
    if new_segs.is_empty() {
        return;
    }
    match defs {
        Some(defs) => {
            let sp = d.nodes[defs].span.unwrap();
            if sp.empty {
                let tag = &text[sp.start..sp.start_end];
                edits.add(sp.start, sp.end, format!("{}>{new_segs}</{}>", tag[..tag.len() - 2].trim_end(), d.nodes[defs].name));
            } else {
                edits.add(sp.end_start, sp.end_start, new_segs);
            }
        }
        None => {
            let sp = d.nodes[tu].span.unwrap();
            edits.add(sp.end_start, sp.end_start, format!("<{prefix}:seg-defs>{new_segs}</{prefix}:seg-defs>"));
        }
    }
}

/// `UnitContext::extra` key with the unit's key inside its XLIFF file (for a package member, `locos.p_index` is the
/// package-wide number).
pub const KEY: &str = "xliff.locos_key";

/// The LocOS texts some units write, by their key inside their file ([`KEY`]): every segment with a target.
pub fn written_texts<'a>(units: impl Iterator<Item = &'a Unit>) -> HashMap<i64, String> {
    texts_of(units, false)
}

/// [`written_texts`] with literal text escaped, for [`build_cat_escaped`]. Not part of the semver API.
#[doc(hidden)]
pub fn written_texts_escaped<'a>(units: impl Iterator<Item = &'a Unit>) -> HashMap<i64, String> {
    texts_of(units, true)
}

fn texts_of<'a>(units: impl Iterator<Item = &'a Unit>, esc: bool) -> HashMap<i64, String> {
    let mut out = HashMap::new();
    for u in units.filter(|u| u.translate) {
        let Some(p) = u.context.extra.get(KEY).and_then(|p| p.parse::<i64>().ok()) else { continue };
        if let Some(s) = u.segments.first().filter(|s| s.has_target()) {
            out.insert(p, tagged_text(&s.target, esc));
        }
    }
    out
}

/// Writes a document extracted under the profile (its skeleton holds the original file).
pub fn merge(doc: &Document, format: &str, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    let Skeleton::Binary { data, .. } = &doc.skeleton else {
        return Err(werr(format!("{format}: a LocOS-profile document needs its original file")));
    };
    let texts = written_texts_escaped(doc.units.iter());
    let tag = opts.stamp_lang(doc, !texts.is_empty()).and_then(profile::lang_tag);
    if format != "xliff" {
        return build_cat_escaped(data, &texts, tag.as_deref());
    }
    build_xliff_escaped(data, &texts, tag.as_deref())
}

/// The number of `<source>` elements of a plain XLIFF file (LocOS `xcloc_filter._source_count`, which offsets the
/// `p_index` of each bundle member by the sources of the members before it). Not part of the semver API.
#[doc(hidden)]
pub fn source_count(raw: &[u8]) -> Result<usize, Error> {
    let text = decode_plain(raw, "xliff")?;
    let d = dom::parse(&text).map_err(|e| perr("xliff", format!("XLIFF is not well-formed XML: {e}")))?;
    Ok(source_elements(&d).len())
}

/// LocOS `xcloc_filter._mark_translated`: a target written for one of the `written` sources that carries a
/// `state` other than `translated` gets `state="translated"`; the file is then serialised as lxml's
/// `etree.tostring(root, xml_declaration=True, encoding="UTF-8")`. Unchanged when no target needed it. Not part of
/// the semver API.
#[doc(hidden)]
pub fn mark_translated(raw: &[u8], written: &std::collections::HashSet<i64>) -> Result<Vec<u8>, Error> {
    let text = decode_plain(raw, "xliff").map_err(werr)?;
    let mut d = dom::parse(&text).map_err(|e| werr(format!("XLIFF is not well-formed XML: {e}")))?;
    let mut changed = false;
    for (k, src) in source_elements(&d).into_iter().enumerate() {
        if !written.contains(&(k as i64)) {
            continue;
        }
        let Some(tgt) = d.nodes[src].parent.and_then(|tu| d.child(tu, "target")) else { continue };
        if d.get(tgt, "state").is_some_and(|s| s != "translated") {
            d.set(tgt, "state", "translated");
            changed = true;
        }
    }
    if !changed {
        return Ok(raw.to_vec());
    }
    Ok(format!("<?xml version='1.0' encoding='UTF-8'?>\n{}", d.serialize()).into_bytes())
}

/// `XliffFilter.build` / `build_localized` of a plain `.xliff` (with or without seg-source) for texts keyed by their
/// unit key and escaped ([`written_texts_escaped`]). Not part of the semver API.
#[doc(hidden)]
pub fn build_xliff_escaped(data: &[u8], texts: &HashMap<i64, String>, tag: Option<&str>) -> Result<Vec<u8>, Error> {
    let format = "xliff";
    let text = decode_plain(data, format).map_err(werr)?;
    let d = dom::parse(&text).map_err(|e| werr(format!("XLIFF is not well-formed XML: {e}")))?;
    if !has_seg_source(&d) {
        return build_plain(data, &text, texts, tag);
    }
    // XLIFF 1.2 with seg-source: LocOS splices, then sets `target-language` on every <file> through lxml
    let out = build_cat_escaped(data, texts, None)?;
    let Some(tag) = tag else { return Ok(out) };
    let text = decode_plain(&out, format).map_err(werr)?;
    let mut d = dom::parse(&text).map_err(|e| werr(format!("XLIFF is not well-formed XML: {e}")))?;
    stamp_dom(&mut d, tag);
    Ok(xml_like(data, &d.serialize()))
}

#[cfg(test)]
mod tests;
