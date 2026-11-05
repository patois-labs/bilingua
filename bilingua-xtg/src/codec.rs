//! Reading and writing the file's bytes the way LocOS does: a BOM first, then UTF-16 without one, then the
//! `<eN>` header tag, then the shared text decode (UTF-8, else Windows-1252).

use crate::lex;
use crate::tables::{Mb, BIG5, CP1252, CP932, CP949, DUP_BIG5, DUP_CP932, DUP_CP949, DUP_GBK, GBK, MAC_ROMAN, UNDEF};
use bilingua_model::encoding_rs::{self, Encoding};
use bilingua_model::enc::{self, DecodeOptions};
use bilingua_model::{guard, Error, ExtractOptions, Warning};

/// A legacy (8-bit or multi-byte) code page, named after the Python codec LocOS uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Legacy {
    MacRoman,
    Cp1252,
    Latin1,
    Cp932,
    Big5,
    Gbk,
    Cp949,
    Other(&'static Encoding),
}

impl Legacy {
    pub fn tag(self) -> String {
        match self {
            Legacy::MacRoman => "mac_roman".into(),
            Legacy::Cp1252 => "windows-1252".into(),
            Legacy::Latin1 => "iso-8859-1".into(),
            Legacy::Cp932 => "shift_jis".into(),
            Legacy::Big5 => "big5".into(),
            Legacy::Gbk => "gbk".into(),
            Legacy::Cp949 => "euc-kr".into(),
            Legacy::Other(e) => e.name().to_ascii_lowercase(),
        }
    }

    pub fn from_tag(t: &str) -> Option<Legacy> {
        Some(match t {
            "mac_roman" | "macintosh" => Legacy::MacRoman,
            "windows-1252" => Legacy::Cp1252,
            "iso-8859-1" => Legacy::Latin1,
            "shift_jis" => Legacy::Cp932,
            "big5" => Legacy::Big5,
            "gbk" => Legacy::Gbk,
            "euc-kr" => Legacy::Cp949,
            other => Legacy::Other(Encoding::for_label(other.as_bytes())?),
        })
    }

    /// The Python codec name, for messages.
    pub fn py(self) -> String {
        match self {
            Legacy::MacRoman => "mac_roman".into(),
            Legacy::Cp1252 => "cp1252".into(),
            Legacy::Latin1 => "latin-1".into(),
            Legacy::Cp932 => "cp932".into(),
            Legacy::Big5 => "big5".into(),
            Legacy::Gbk => "gbk".into(),
            Legacy::Cp949 => "cp949".into(),
            Legacy::Other(e) => e.name().to_ascii_lowercase(),
        }
    }

    /// The `encoding_rs` decoder for the garbled-letters check.
    pub fn rs(self) -> Option<&'static Encoding> {
        Some(match self {
            Legacy::MacRoman => encoding_rs::MACINTOSH,
            Legacy::Cp1252 => encoding_rs::WINDOWS_1252,
            Legacy::Latin1 => return None,
            Legacy::Cp932 => encoding_rs::SHIFT_JIS,
            Legacy::Big5 => encoding_rs::BIG5,
            Legacy::Gbk => encoding_rs::GBK,
            Legacy::Cp949 => encoding_rs::EUC_KR,
            Legacy::Other(e) => e,
        })
    }

    fn multibyte(self) -> bool {
        matches!(self, Legacy::Cp932 | Legacy::Big5 | Legacy::Gbk | Legacy::Cp949 | Legacy::Other(_))
    }

    /// Strict decode; the error is the position of the first byte the code page rejects.
    pub fn decode(self, data: &[u8]) -> Result<String, usize> {
        match self {
            Legacy::MacRoman => table_decode(&MAC_ROMAN, data),
            Legacy::Cp1252 => table_decode(&CP1252, data),
            Legacy::Latin1 => Ok(data.iter().map(|&b| b as char).collect()),
            Legacy::Cp932 => validate(&CP932, data).and_then(|()| decode_cp932(data)),
            Legacy::Gbk => validate(&GBK, data).and_then(|()| enc::decode_strict(encoding_rs::GBK, data)),
            Legacy::Big5 => validate(&BIG5, data).map(|()| decode_big5(data)),
            Legacy::Cp949 => validate(&CP949, data).and_then(|()| enc::decode_strict(encoding_rs::EUC_KR, data)),
            Legacy::Other(e) => enc::decode_strict(e, data),
        }
    }

    /// The text in this code page, or `None` when a character does not fit.
    pub fn encode(self, text: &str) -> Option<Vec<u8>> {
        match self {
            Legacy::MacRoman => table_encode(&MAC_ROMAN, text),
            Legacy::Cp1252 => table_encode(&CP1252, text),
            Legacy::Latin1 => text.chars().map(|c| u8::try_from(c as u32).ok()).collect(),
            Legacy::Cp932 => encode_mb(encoding_rs::SHIFT_JIS, text, DUP_CP932, true).filter(|b| validate(&CP932, b).is_ok()),
            Legacy::Gbk => encode_mb(encoding_rs::GBK, text, DUP_GBK, false).filter(|b| validate(&GBK, b).is_ok()),
            Legacy::Big5 => encode_big5(text),
            Legacy::Cp949 => encode_mb(encoding_rs::EUC_KR, text, DUP_CP949, false).filter(|b| validate(&CP949, b).is_ok()),
            Legacy::Other(e) => rs_encode(e, text),
        }
    }
}

fn has(set: &[u32; 8], b: u8) -> bool {
    set[(b >> 5) as usize] >> (b & 31) & 1 == 1
}

/// Python's rule for which byte sequences a multi-byte codec accepts: the error is the position of the first
/// byte it rejects.
fn validate(t: &Mb, data: &[u8]) -> Result<(), usize> {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if has(&t.lead, b) {
            let ok = data.get(i + 1).is_some_and(|&tr| {
                tr >= 0x40 && {
                    let k = (b as usize - 0x81) * 191 + (tr as usize - 0x40);
                    t.pairs[k >> 5] >> (k & 31) & 1 == 1
                }
            });
            if !ok {
                return Err(i);
            }
            i += 2;
        } else if has(&t.single, b) {
            i += 1;
        } else {
            return Err(i);
        }
    }
    Ok(())
}

fn rs_encode(e: &'static Encoding, text: &str) -> Option<Vec<u8>> {
    let (b, _, bad) = e.encode(text);
    (!bad).then(|| b.into_owned())
}

fn table_decode(t: &[u32; 128], data: &[u8]) -> Result<String, usize> {
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

fn table_encode(t: &[u32; 128], text: &str) -> Option<Vec<u8>> {
    let mut rev: Vec<(u32, u8)> = t.iter().enumerate().filter(|(_, u)| **u != UNDEF).map(|(i, u)| (*u, 0x80 + i as u8)).collect();
    rev.sort_unstable();
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

/// Python's `cp932` reads the single bytes 0xA0 and 0xFD..=0xFF as U+F8F0..=U+F8F3; the WHATWG decoder rejects them.
fn decode_cp932(data: &[u8]) -> Result<String, usize> {
    let mut out = String::with_capacity(data.len());
    let (mut i, mut run) = (0usize, 0usize);
    let flush = |out: &mut String, from: usize, to: usize| -> Result<(), usize> {
        if to > from {
            out.push_str(&enc::decode_strict(encoding_rs::SHIFT_JIS, &data[from..to]).map_err(|p| p + from)?);
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

static BIG5_TABLE: &[u8] = include_bytes!("big5.bin");

fn big5_char(lead: u8, trail: u8) -> u16 {
    let k = (lead as usize - 0x81) * 191 + (trail as usize - 0x40);
    u16::from_le_bytes([BIG5_TABLE[2 * k], BIG5_TABLE[2 * k + 1]])
}

/// Python's Big5 (validated first, so every pair is in the table).
fn decode_big5(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 || i + 1 >= data.len() {
            s.push(b as char);
            i += 1;
        } else {
            s.push(char::from_u32(big5_char(b, data[i + 1]) as u32).unwrap_or('\u{FFFD}'));
            i += 2;
        }
    }
    s
}

fn encode_big5(text: &str) -> Option<Vec<u8>> {
    static REV: std::sync::OnceLock<Vec<(u16, u16)>> = std::sync::OnceLock::new();
    let rev = REV.get_or_init(|| {
        let mut v: Vec<(u16, u16)> = Vec::with_capacity(14_000);
        for lead in 0x81..=0xFEu8 {
            for t in 0x40..=0xFEu8 {
                let c = big5_char(lead, t);
                if c != 0 {
                    v.push((c, u16::from_be_bytes([lead, t])));
                }
            }
        }
        for &(c, code) in DUP_BIG5 {
            v.retain(|x| u32::from(x.0) != c);
            v.push((c as u16, code));
        }
        v.sort_unstable();
        v
    });
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let u = ch as u32;
        if u < 0x80 {
            out.push(u as u8);
        } else {
            let u = u16::try_from(u).ok()?;
            let k = rev.binary_search_by_key(&u, |x| x.0).ok()?;
            out.extend_from_slice(&rev[k].1.to_be_bytes());
        }
    }
    Some(out)
}

/// Writes `text` in a multi-byte code page the way Python does: characters with several spellings get the one
/// Python writes, and (cp932) the single bytes 0xA0 and 0xFD..=0xFF (U+F8F0..=U+F8F3) and the user-defined area
/// 0xF040..=0xF9FC (U+E000..=U+E757), which the WHATWG encoders lack.
fn encode_mb(e: &'static Encoding, text: &str, dups: &[(u32, u16)], cp932: bool) -> Option<Vec<u8>> {
    let dup = |c: char| dups.binary_search_by_key(&(c as u32), |d| d.0).ok();
    let special = |c: char| dup(c).is_some() || (cp932 && (('\u{F8F0}'..='\u{F8F3}').contains(&c) || ('\u{E000}'..='\u{E757}').contains(&c)));
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text;
    loop {
        let (head, tail) = match rest.find(special) {
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
        let u = c as u32;
        if let Some(i) = dup(c) {
            out.extend_from_slice(&dups[i].1.to_be_bytes());
        } else if u >= 0xF8F0 {
            out.push(if u == 0xF8F0 { 0xA0 } else { 0xFC + (u - 0xF8F0) as u8 });
        } else {
            let k = u - 0xE000;
            let (lead, tr) = (0xF0 + (k / 188) as u8, (k % 188) as u8);
            out.extend_from_slice(&[lead, 0x40 + tr + u8::from(tr >= 0x3F)]);
        }
        rest = &t[c.len_utf8()..];
    }
    Some(out)
}

/// How the bytes of a file are written back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Enc {
    Utf8 { bom: bool },
    Utf16 { be: bool, bom: bool },
    Legacy(Legacy),
}

impl Enc {
    pub fn tag(self) -> String {
        match self {
            Enc::Utf8 { bom: false } => "utf-8".into(),
            Enc::Utf8 { bom: true } => "utf-8-bom".into(),
            Enc::Utf16 { be, bom } => format!("utf-16{}{}", if be { "be" } else { "le" }, if bom { "" } else { "-nobom" }),
            Enc::Legacy(l) => l.tag(),
        }
    }

    pub fn from_tag(t: &str) -> Option<Enc> {
        Some(match t {
            "utf-8" => Enc::Utf8 { bom: false },
            "utf-8-bom" => Enc::Utf8 { bom: true },
            "utf-16le" => Enc::Utf16 { be: false, bom: true },
            "utf-16be" => Enc::Utf16 { be: true, bom: true },
            "utf-16le-nobom" => Enc::Utf16 { be: false, bom: false },
            "utf-16be-nobom" => Enc::Utf16 { be: true, bom: false },
            other => Enc::Legacy(Legacy::from_tag(other)?),
        })
    }
}

pub struct Decoded {
    pub text: String,
    pub enc: Enc,
    pub warnings: Vec<Warning>,
}

fn enc_err(msg: String) -> Error {
    Error::Encoding(msg)
}

/// The `<eN>` number to a code page: `Ok(None)` for Unicode (read by the bytes).
fn codec_for(num: u32) -> Result<Option<Legacy>, Error> {
    Ok(match num {
        0 => Some(Legacy::MacRoman),
        1 => Some(Legacy::Cp1252),
        2 => Some(Legacy::Latin1),
        3 => Some(Legacy::Cp932),
        6 => Some(Legacy::Big5),
        7 => Some(Legacy::Gbk),
        8 | 9 => None,
        19 => Some(Legacy::Cp949),
        20 | 21 => {
            let name = if num == 20 { "Korean Mac" } else { "Japanese Mac" };
            return Err(enc_err(format!(
                "This XPress Tags file is in the {name} encoding (<e{num}>), which is not supported. Re-save it from QuarkXPress as Unicode (UTF-8 or UTF-16)."
            )));
        }
        _ => {
            return Err(enc_err(format!(
                "This XPress Tags file declares an unknown encoding <e{num}>. Re-save it from QuarkXPress as Unicode (UTF-8 or UTF-16)."
            )))
        }
    })
}

fn utf16_bad(body: &[u8], be: bool) -> usize {
    let unit = |i: usize| if be { u16::from_be_bytes([body[i], body[i + 1]]) } else { u16::from_le_bytes([body[i], body[i + 1]]) };
    let mut i = 0;
    while i + 1 < body.len() {
        let u = unit(i);
        if (0xD800..0xDC00).contains(&u) {
            if i + 3 < body.len() && (0xDC00..0xE000).contains(&unit(i + 2)) {
                i += 4;
                continue;
            }
            return i;
        }
        if (0xDC00..0xE000).contains(&u) {
            return i;
        }
        i += 2;
    }
    i
}

const RESAVE: &str = "Re-save it from QuarkXPress as Unicode (UTF-8 or UTF-16).";

fn read_failed(what: &str, data: &[u8], pos: usize) -> Error {
    enc_err(format!(
        "This XPress Tags file could not be read in its encoding ({what}; byte 0x{:02x} at position {pos}). {RESAVE}",
        data.get(pos).copied().unwrap_or(0)
    ))
}

pub fn decode(data: &[u8], opts: &ExtractOptions) -> Result<Decoded, Error> {
    let limits = opts.limits();
    guard::check_input(data, &limits)?;
    let mut warnings = Vec::new();
    let (bom_len, enc): (usize, Enc) = if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (3, Enc::Utf8 { bom: true })
    } else if data.starts_with(&[0xFF, 0xFE]) {
        (2, Enc::Utf16 { be: false, bom: true })
    } else if data.starts_with(&[0xFE, 0xFF]) {
        (2, Enc::Utf16 { be: true, bom: true })
    } else if data.starts_with(b"<\0") {
        (0, Enc::Utf16 { be: false, bom: false })
    } else if data.starts_with(b"\0<") {
        (0, Enc::Utf16 { be: true, bom: false })
    } else {
        match lex::e_tag(&data[..data.len().min(64)]) {
            None => {
                // no <e> tag: the shared text decode (UTF-8, else the Windows code page)
                let d = enc::decode_text(data, &DecodeOptions::new(opts))?;
                let base = enc::base(&d.encoding);
                let e = if base == "utf-8" {
                    Enc::Utf8 { bom: false }
                } else {
                    Enc::Legacy(Legacy::from_tag(base).ok_or_else(|| enc_err(format!("unknown encoding {base}")))?)
                };
                return finish(d.text, e, d.warnings, &limits);
            }
            Some((_, _, num)) => match codec_for(num)? {
                None => (0, Enc::Utf8 { bom: false }),
                Some(l) => (0, Enc::Legacy(l)),
            },
        }
    };
    let body = &data[bom_len..];
    let text = match enc {
        Enc::Utf8 { .. } => match std::str::from_utf8(body) {
            Ok(s) => s.to_string(),
            Err(e) => return Err(read_failed("utf-8", data, bom_len + e.valid_up_to())),
        },
        Enc::Utf16 { be, .. } => {
            if body.len() % 2 == 1 {
                return Err(enc_err(
                    "This XPress Tags file is UTF-16 but has an odd number of bytes. Re-save it from QuarkXPress.".into(),
                ));
            }
            let e = if be { encoding_rs::UTF_16BE } else { encoding_rs::UTF_16LE };
            match e.decode_without_bom_handling_and_without_replacement(body) {
                Some(s) => s.into_owned(),
                None => {
                    let what = if be { "utf-16-be" } else { "utf-16-le" };
                    return Err(read_failed(what, data, bom_len + utf16_bad(body, be)));
                }
            }
        }
        Enc::Legacy(l) => match l.decode(body) {
            Ok(s) => {
                if let (Some(e), Some(lang)) = (l.rs(), opts.source_lang.as_deref()) {
                    warnings.extend(enc::warn_if_garbled(&s, e, lang, None));
                }
                if l.multibyte() && l.encode(&s).is_none_or(|b| b != body) {
                    warnings.push(Warning::new(
                        "text_encoding",
                        "text encoding",
                        "Some bytes of this file have more than one spelling in its code page, so saving it may normalise them.",
                    ));
                }
                s
            }
            Err(p) => return Err(read_failed(&l.py(), data, bom_len + p)),
        },
    };
    finish(text, enc, warnings, &limits)
}

fn finish(text: String, enc: Enc, warnings: Vec<Warning>, limits: &bilingua_model::Limits) -> Result<Decoded, Error> {
    if text.contains('\0') {
        return Err(enc_err("This file is not XPress Tags text (it contains NUL characters).".into()));
    }
    guard::check_lines(&text, limits)?;
    Ok(Decoded { text, enc, warnings })
}

/// The text back as bytes. A translation the legacy code page cannot hold switches the whole file to UTF-8
/// (and the header to `<e9>`).
pub fn encode(text: &str, enc: Enc) -> Vec<u8> {
    match enc {
        Enc::Utf8 { bom } => {
            let mut v = Vec::with_capacity(text.len() + 3);
            if bom {
                v.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            }
            v.extend_from_slice(text.as_bytes());
            v
        }
        Enc::Utf16 { be, bom } => {
            let mut v = Vec::with_capacity(text.len() * 2 + 2);
            if bom {
                v.extend_from_slice(if be { &[0xFE, 0xFF] } else { &[0xFF, 0xFE] });
            }
            for u in text.encode_utf16() {
                v.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
            }
            v
        }
        Enc::Legacy(l) => match l.encode(text) {
            Some(b) => b,
            None => match lex::e_tag(&text.as_bytes()[..text.len().min(64)]) {
                Some((a, b, _)) => {
                    let mut s = String::with_capacity(text.len());
                    s.push_str(&text[..a]);
                    s.push_str("<e9>");
                    s.push_str(&text[b..]);
                    s.into_bytes()
                }
                None => {
                    let mut v = vec![0xEF, 0xBB, 0xBF];
                    v.extend_from_slice(text.as_bytes());
                    v
                }
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fnv(b: &[u8]) -> String {
        let mut h: u64 = 0xCBF29CE484222325;
        for &x in b {
            h = (h ^ x as u64).wrapping_mul(0x100000001B3);
        }
        format!("{h:016x}")
    }

    /// Every sequence the code page accepts, in the order the golden generator uses.
    fn all_valid(l: Legacy, multi: bool) -> Vec<u8> {
        let mut out = Vec::new();
        for b in 0..=255u8 {
            if l.decode(&[b]).is_ok() {
                out.push(b);
            }
        }
        if multi {
            for lead in 0x81..=0xFEu8 {
                if l.decode(&[lead]).is_ok() {
                    continue;
                }
                for t in 0x40..=0xFEu8 {
                    if l.decode(&[lead, t]).is_ok() {
                        out.extend_from_slice(&[lead, t]);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn code_pages_decode_and_encode_like_python() {
        let g: serde_json::Value = serde_json::from_str(include_str!("../tests/data/codec_golden.json")).unwrap();
        let pages = [
            ("mac_roman", Legacy::MacRoman, false),
            ("cp1252", Legacy::Cp1252, false),
            ("latin-1", Legacy::Latin1, false),
            ("cp932", Legacy::Cp932, true),
            ("big5", Legacy::Big5, true),
            ("gbk", Legacy::Gbk, true),
            ("cp949", Legacy::Cp949, true),
        ];
        for (name, l, multi) in pages {
            let want = &g[name];
            let data = all_valid(l, multi);
            assert_eq!(data.len() as u64, want["len"].as_u64().unwrap(), "{name}: accepted bytes");
            let text = l.decode(&data).unwrap_or_else(|p| panic!("{name}: rejected at {p}"));
            assert_eq!(text.chars().count() as u64, want["chars"].as_u64().unwrap(), "{name}: chars");
            assert_eq!(fnv(text.as_bytes()), want["text"].as_str().unwrap(), "{name}: decoded text");
            let re = l.encode(&text).unwrap_or_else(|| panic!("{name}: encode failed"));
            assert_eq!(fnv(&re), want["reenc"].as_str().unwrap(), "{name}: re-encoded bytes");
        }
    }
}
