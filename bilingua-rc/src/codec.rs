//! Reading and writing the file's bytes the way LocOS does: UTF-16 with a BOM, BOM-less UTF-16 LE, UTF-8 with a
//! BOM, then the code page a `#pragma code_page(N)` declares, then UTF-8, then Windows-1252.

use crate::sbcs::{SBCS, UNDEF};
use bilingua_model::encoding_rs::{self, Encoding};
use bilingua_model::Error;
use regex::bytes::Regex as BRegex;
use regex::Regex;
use std::sync::OnceLock;

/// How a declared code page is read and written.
pub enum Kind {
    /// A single-byte table (bytes 0x80..=0xFF).
    Table(&'static [u32; 128]),
    /// A multi-byte encoding.
    Multi(&'static Encoding),
}

pub struct Page {
    /// The skeleton tag for a file in this code page.
    pub label: &'static str,
    pub kind: Kind,
    /// The matching `encoding_rs` decoder, for the garbled-letters check.
    pub rs: Option<&'static Encoding>,
}

fn table(label: &str) -> &'static [u32; 128] {
    SBCS.iter().find(|(l, _)| *l == label).map(|(_, t)| t).expect("generated table")
}

const NUMS: &[(&str, &str)] = &[
    ("1250", "windows-1250"),
    ("1251", "windows-1251"),
    ("1252", "windows-1252"),
    ("1253", "windows-1253"),
    ("1254", "windows-1254"),
    ("1255", "windows-1255"),
    ("1256", "windows-1256"),
    ("1257", "windows-1257"),
    ("1258", "windows-1258"),
    ("932", "shift_jis"),
    ("936", "gbk"),
    ("949", "euc-kr"),
    ("950", "big5"),
    ("874", "windows-874"),
    ("437", "ibm437"),
    ("850", "ibm850"),
    ("866", "ibm866"),
    ("20866", "koi8-r"),
    ("21866", "koi8-u"),
    ("28591", "iso-8859-1"),
    ("28592", "iso-8859-2"),
    ("28595", "iso-8859-5"),
    ("28597", "iso-8859-7"),
    ("28599", "iso-8859-9"),
    ("28605", "iso-8859-15"),
    ("54936", "gb18030"),
    ("51949", "euc-kr"),
    ("20932", "euc-jp"),
];

/// The page for a tag label.
pub fn page_for_label(label: &str) -> Option<Page> {
    let multi = |l: &'static str, e: &'static Encoding| Page { label: l, kind: Kind::Multi(e), rs: Some(e) };
    Some(match label {
        "shift_jis" => multi("shift_jis", encoding_rs::SHIFT_JIS),
        "gbk" => multi("gbk", encoding_rs::GBK),
        "euc-kr" => multi("euc-kr", encoding_rs::EUC_KR),
        "big5" => multi("big5", encoding_rs::BIG5),
        "gb18030" => multi("gb18030", encoding_rs::GB18030),
        "euc-jp" => multi("euc-jp", encoding_rs::EUC_JP),
        _ => {
            let (l, _) = SBCS.iter().find(|(l, _)| *l == label)?;
            let rs = match *l {
                "ibm437" | "ibm850" | "iso-8859-1" | "iso-8859-9" => None,
                other => Encoding::for_label(other.as_bytes()),
            };
            Page { label: l, kind: Kind::Table(table(l)), rs }
        }
    })
}

/// The page a `#pragma code_page(N)` names (`65001` is UTF-8 and handled apart).
pub fn page_for_number(num: &str) -> Option<Page> {
    NUMS.iter().find(|(n, _)| *n == num).and_then(|(_, l)| page_for_label(l))
}

fn refuse(msg: impl Into<String>) -> Error {
    Error::Encoding(format!("{} Re-save the resource script as UTF-16 (Unicode) or UTF-8.", msg.into()))
}

fn pragma_re() -> &'static BRegex {
    static RE: OnceLock<BRegex> = OnceLock::new();
    RE.get_or_init(|| BRegex::new(r"(?im-u)^[ \t]*#[ \t]*pragma[ \t]+code_page[ \t]*\(([^)\r\n]{0,20})\)").expect("pragma pattern"))
}

fn pragma_text_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?im)^([ \t]*#[ \t]*pragma[ \t]+code_page[ \t]*\()([^)\r\n]{0,20})(\))").expect("pragma pattern"))
}

fn looks_binary(data: &[u8]) -> bool {
    if data.contains(&0) {
        return true;
    }
    let bad = data.iter().filter(|b| **b < 0x20 && !matches!(**b, 0x09..=0x0D | 0x1A | 0x1B)).count();
    bad > 2 + data.len() / 100
}

/// Decodes `data` strictly in a page; the error is the position of the first byte the page rejects.
fn decode_page(p: &Page, data: &[u8]) -> Result<String, usize> {
    match &p.kind {
        Kind::Table(t) => {
            let mut s = String::with_capacity(data.len());
            for (i, &b) in data.iter().enumerate() {
                if b < 0x80 {
                    s.push(b as char);
                } else {
                    let u = t[(b - 0x80) as usize];
                    if u == UNDEF {
                        return Err(i);
                    }
                    s.push(char::from_u32(u).ok_or(i)?);
                }
            }
            Ok(s)
        }
        Kind::Multi(e) if *e == encoding_rs::SHIFT_JIS => decode_cp932(data),
        Kind::Multi(e) => bilingua_model::enc::decode_strict(e, data),
    }
}

/// Python's `cp932` reads the single bytes 0xA0 and 0xFD..=0xFF as U+F8F0..=U+F8F3 (private use); the
/// WHATWG Shift_JIS decoder rejects them. LocOS reads them, so these four bytes are handled here.
fn decode_cp932(data: &[u8]) -> Result<String, usize> {
    let mut out = String::with_capacity(data.len());
    let (mut i, mut run) = (0usize, 0usize);
    let flush = |out: &mut String, from: usize, to: usize| -> Result<(), usize> {
        if to > from {
            out.push_str(&bilingua_model::enc::decode_strict(encoding_rs::SHIFT_JIS, &data[from..to]).map_err(|p| p + from)?);
        }
        Ok(())
    };
    while i < data.len() {
        let b = data[i];
        if (0x81..=0x9F).contains(&b) || (0xE0..=0xFC).contains(&b) {
            i += 2;
        } else if matches!(b, 0xA0 | 0xFD..=0xFF) {
            flush(&mut out, run, i)?;
            out.push(char::from_u32(0xF8F0 + if b == 0xA0 { 0 } else { (b - 0xFC) as u32 }).unwrap_or('?'));
            i += 1;
            run = i;
        } else {
            i += 1;
        }
    }
    flush(&mut out, run, data.len())?;
    Ok(out)
}

/// A decoded file.
pub struct Decoded {
    pub text: String,
    /// The skeleton tag: `utf-8`, `utf-8-bom`, `utf-16le` / `utf-16be` (with a BOM), `utf-16le-nobom`, or a page label.
    pub tag: String,
}

fn utf16(data: &[u8], be: bool, tag: &str, what: &str) -> Result<Decoded, Error> {
    let e = if be { encoding_rs::UTF_16BE } else { encoding_rs::UTF_16LE };
    match e.decode_without_bom_handling_and_without_replacement(data) {
        Some(s) => Ok(Decoded { text: s.into_owned(), tag: tag.into() }),
        None => Err(refuse(what)),
    }
}

pub fn decode(data: &[u8]) -> Result<Decoded, Error> {
    const BAD16: &str = "This .rc file starts with a UTF-16 byte-order mark but is not valid UTF-16.";
    if data.starts_with(&[0xFF, 0xFE]) {
        return utf16(&data[2..], false, "utf-16le", BAD16);
    }
    if data.starts_with(&[0xFE, 0xFF]) {
        return utf16(&data[2..], true, "utf-16be", BAD16);
    }
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return match std::str::from_utf8(&data[3..]) {
            Ok(s) => Ok(Decoded { text: s.to_string(), tag: "utf-8-bom".into() }),
            Err(_) => Err(refuse("This .rc file starts with a UTF-8 byte-order mark but is not valid UTF-8.")),
        };
    }
    let head = &data[..data.len().min(4096)];
    if head.len() >= 4 && data.len().is_multiple_of(2) {
        let n_odd = head.len() / 2;
        let odd_zero = head.iter().skip(1).step_by(2).filter(|&&b| b == 0).count();
        let even_zero = head.iter().step_by(2).filter(|&&b| b == 0).count();
        if odd_zero as f64 >= 0.9 * n_odd as f64 && (even_zero as f64) < 0.1 * n_odd as f64 {
            if let Ok(d) = utf16(data, false, "utf-16le-nobom", "") {
                return Ok(d);
            }
        }
    }
    let mut pages: Vec<Vec<u8>> = Vec::new();
    for c in pragma_re().captures_iter(data) {
        // Python's bytes.strip(): ASCII white space including VT (0x0B), which `trim_ascii` keeps
        let ws = |b: &u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C);
        let raw = &c[1];
        let (a, z) = (raw.iter().position(|b| !ws(b)).unwrap_or(raw.len()), raw.iter().rposition(|b| !ws(b)).map_or(0, |p| p + 1));
        let v = raw[a..z.max(a)].to_ascii_uppercase();
        if v != b"DEFAULT" && !pages.contains(&v) {
            pages.push(v);
        }
    }
    if pages.len() > 1 {
        return Err(refuse(
            "This .rc file switches between several code pages (#pragma code_page); one file can only be read in one encoding.",
        ));
    }
    if let Some(p) = pages.pop() {
        let num = String::from_utf8_lossy(&p).into_owned();
        if num == "65001" {
            return match std::str::from_utf8(data) {
                Ok(s) => Ok(Decoded { text: s.to_string(), tag: "utf-8".into() }),
                Err(e) => Err(refuse(format!(
                    "This .rc file declares code page 65001 but byte 0x{:02x} at position {} is not valid in it.",
                    data[e.valid_up_to()],
                    e.valid_up_to()
                ))),
            };
        }
        let Some(page) = page_for_number(&num) else {
            return Err(refuse(format!("This .rc file declares code page {num}, which is not supported.")));
        };
        return match decode_page(&page, data) {
            Ok(text) => Ok(Decoded { text, tag: page.label.into() }),
            Err(pos) => Err(refuse(format!(
                "This .rc file declares code page {num} but byte 0x{:02x} at position {pos} is not valid in it.",
                data.get(pos).copied().unwrap_or(0)
            ))),
        };
    }
    if let Ok(s) = std::str::from_utf8(data) {
        return Ok(Decoded { text: s.to_string(), tag: "utf-8".into() });
    }
    if looks_binary(data) {
        return Err(refuse("This .rc file is not valid UTF-8 or UTF-16 and looks like a binary file."));
    }
    let page = page_for_label("windows-1252").expect("1252");
    match decode_page(&page, data) {
        Ok(text) => Ok(Decoded { text, tag: "windows-1252".into() }),
        Err(pos) => Err(refuse(format!(
            "This .rc file is not valid UTF-8 or UTF-16 and could not be read as Windows-1252 either (byte 0x{:02x} at position {pos}).",
            data.get(pos).copied().unwrap_or(0)
        ))),
    }
}

/// The text in a page, or `None` when a character does not fit.
pub fn encode_page(p: &Page, text: &str) -> Option<Vec<u8>> {
    match &p.kind {
        Kind::Table(t) => {
            let mut rev: Vec<(u32, u8)> =
                t.iter().enumerate().filter(|(_, u)| **u != UNDEF).map(|(i, u)| (*u, 0x80 + i as u8)).collect();
            rev.sort_by_key(|(u, b)| (*u, *b));
            let mut out = Vec::with_capacity(text.len());
            for c in text.chars() {
                let u = c as u32;
                if u < 0x80 {
                    out.push(u as u8);
                } else {
                    let k = rev.partition_point(|(x, _)| *x < u);
                    match rev.get(k) {
                        Some((x, b)) if *x == u => out.push(*b),
                        _ => return None,
                    }
                }
            }
            Some(out)
        }
        Kind::Multi(e) if *e == encoding_rs::SHIFT_JIS => {
            let mut out = Vec::with_capacity(text.len());
            let mut rest = text;
            loop {
                let cut = rest.find(|c: char| ('\u{F8F0}'..='\u{F8F3}').contains(&c));
                let (head, tail) = match cut {
                    Some(k) => (&rest[..k], Some(&rest[k..])),
                    None => (rest, None),
                };
                let (bytes, _, bad) = e.encode(head);
                if bad {
                    return None;
                }
                out.extend_from_slice(&bytes);
                let Some(t) = tail else { break };
                let c = t.chars().next()?;
                out.push(if c == '\u{F8F0}' { 0xA0 } else { 0xFC + (c as u32 - 0xF8F0) as u8 });
                rest = &t[c.len_utf8()..];
            }
            Some(out)
        }
        Kind::Multi(e) => {
            let (bytes, _, bad) = e.encode(text);
            (!bad).then(|| bytes.into_owned())
        }
    }
}

fn nl_of(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn utf8_with_pragma(text: &str) -> Vec<u8> {
    let t = if pragma_text_re().is_match(text) {
        pragma_text_re().replace_all(text, |m: &regex::Captures| format!("{}65001{}", &m[1], &m[3])).into_owned()
    } else {
        format!("#pragma code_page(65001){}{}", nl_of(text), text)
    };
    t.into_bytes()
}

fn utf16_bytes(text: &str, be: bool, bom: bool) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::with_capacity(text.len() * 2 + 2);
    if bom {
        v.extend_from_slice(if be { &[0xFE, 0xFF] } else { &[0xFF, 0xFE] });
    }
    for u in text.encode_utf16() {
        v.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
    }
    v
}

/// Writes `text` back in the encoding it was read in. `changed` is false for text that is exactly the original,
/// which never gets a code page pragma added.
pub fn encode(text: &str, tag: &str, changed: bool) -> Vec<u8> {
    match tag {
        "utf-16le" => utf16_bytes(text, false, true),
        "utf-16be" => utf16_bytes(text, true, true),
        "utf-16le-nobom" => utf16_bytes(text, false, false),
        "utf-8-bom" => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(text.as_bytes());
            v
        }
        "utf-8" => {
            if changed && !text.is_ascii() && !pragma_text_re().is_match(text) {
                // rc.exe reads a BOM-less file as ANSI unless told otherwise
                format!("#pragma code_page(65001){}{}", nl_of(text), text).into_bytes()
            } else {
                text.as_bytes().to_vec()
            }
        }
        other => match page_for_label(other).and_then(|p| encode_page(&p, text)) {
            Some(b) => b,
            // the translation does not fit the ANSI code page: write UTF-8 and say so to rc.exe
            None => utf8_with_pragma(text),
        },
    }
}

/// True when re-encoding the decoded text gives back different bytes (a multi-byte page with duplicate mappings).
pub fn round_trip_differs(tag: &str, text: &str, data: &[u8]) -> bool {
    match page_for_label(tag) {
        Some(p) if matches!(p.kind, Kind::Multi(_)) => encode_page(&p, text).is_none_or(|b| b != data),
        _ => false,
    }
}

/// The `encoding_rs` decoder of a tag, for the garbled-letters check (`None` when there is none).
pub fn rs_encoding(tag: &str) -> Option<&'static Encoding> {
    page_for_label(tag).and_then(|p| p.rs)
}
