//! Reading and writing HTML Help files: UTF-8 or UTF-16 with a byte-order mark, or the Windows ANSI code page of the
//! help language (the usual case). The envelope travels in the skeleton's `encoding` string: `utf-8`, `utf-8-bom`,
//! `utf-16le`, `utf-16be` (both with a BOM), `iso-8859-1` (true Latin-1, declared by a file), or the `encoding_rs` name
//! of an ANSI code page (`windows-1252`, ...).

use crate::data::{ALIASES, CP1258};
use crate::decdiff::{DEC_DIFF, EUCKR_CHO_BYTE, EUCKR_CHO_OF, EUCKR_JONG_BYTE, EUCKR_JONG_OF};
use crate::encdiff::{ENC_DIFF, GB2312_OK};
use crate::text::is_zs;
use bilingua_model::codepage::{code_page_for_encoding, encoding_for_code_page};
use bilingua_model::encoding_rs::{self, Encoding};
use bilingua_model::Error;
use regex::Regex;
use std::sync::OnceLock;

pub const BOM8: &[u8] = &[0xEF, 0xBB, 0xBF];

/// (code page, meta charset label, GDI font charset) as HTML Help Workshop writes them.
pub const CP_INFO: &[(u16, &str, u32)] = &[
    (874, "windows-874", 222),
    (932, "shift_jis", 128),
    (936, "gb2312", 134),
    (949, "ks_c_5601-1987", 129),
    (950, "big5", 136),
    (1250, "windows-1250", 238),
    (1251, "windows-1251", 204),
    (1252, "windows-1252", 0),
    (1253, "windows-1253", 161),
    (1254, "windows-1254", 162),
    (1255, "windows-1255", 177),
    (1256, "windows-1256", 178),
    (1257, "windows-1257", 186),
    (1258, "windows-1258", 163),
];

pub fn cp_info(cp: u16) -> Option<(&'static str, u32)> {
    CP_INFO.iter().find(|(c, _, _)| *c == cp).map(|(_, l, g)| (*l, *g))
}

fn enc_err(msg: impl Into<String>) -> Error {
    Error::Encoding(msg.into())
}

fn decl_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r#"(?i)(<\?xml[^>]{0,200}?\bencoding\s*=\s*["']\s*|<meta\b[^>]{0,400}?\bcharset\s*=\s*["']?\s*|Content-Type:[^"\n]{0,100}?\bcharset\s*=\s*)([A-Za-z0-9_.:-]{1,40})"#,
        )
        .expect("charset declaration pattern")
    })
}

/// A codec an ANSI file is read and written with: true ISO-8859-1 (Python's `iso8859-1`: every character up to U+00FF
/// is one byte) or an `encoding_rs` decoder plus an id that says which Python codec it stands for (the Windows code
/// page number, or 3001.. for the Python codecs that are not a code page).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Latin1,
    Enc(&'static Encoding, u16),
}

/// (Python codec name, id, encoding_rs label) of every codec a file may be read and written with.
const PY_CODECS: &[(&str, u16, &str)] = &[
    ("cp874", 874, "windows-874"),
    ("cp932", 932, "shift_jis"),
    ("gbk", 936, "gbk"),
    ("cp949", 949, "euc-kr"),
    ("cp950", 950, "big5"),
    ("cp1250", 1250, "windows-1250"),
    ("cp1251", 1251, "windows-1251"),
    ("cp1252", 1252, "windows-1252"),
    ("cp1253", 1253, "windows-1253"),
    ("cp1254", 1254, "windows-1254"),
    ("cp1255", 1255, "windows-1255"),
    ("cp1256", 1256, "windows-1256"),
    ("cp1257", 1257, "windows-1257"),
    ("cp1258", 1258, "windows-1258"),
    ("shift_jis", 3001, "shift_jis"),
    ("gb2312", 3002, "gbk"),
    ("big5", 3003, "big5"),
    ("euc_kr", 3004, "euc-kr"),
    ("iso8859-2", 3005, "iso-8859-2"),
    ("iso8859-5", 3006, "iso-8859-5"),
    ("iso8859-7", 3007, "iso-8859-7"),
    ("iso8859-9", 3008, "windows-1254"),
    ("iso8859-15", 3009, "iso-8859-15"),
    ("koi8-r", 3010, "koi8-r"),
    ("koi8-u", 3011, "koi8-u"),
    ("euc_jp", 3012, "euc-jp"),
    ("gb18030", 3013, "gb18030"),
];

fn py_codec(name: &str) -> Option<Codec> {
    if name == "iso8859-1" {
        return Some(Codec::Latin1);
    }
    let (_, id, label) = PY_CODECS.iter().find(|(n, _, _)| *n == name)?;
    Some(Codec::Enc(Encoding::for_label(label.as_bytes())?, *id))
}

/// Python's `codecs.lookup` normalisation of a charset label.
fn py_normalize(label: &str) -> String {
    let lower = label.to_lowercase().replace(' ', "-");
    let mut out = String::with_capacity(lower.len());
    let mut punct = false;
    for c in lower.chars() {
        if c.is_alphanumeric() || c == '.' {
            if punct && !out.is_empty() {
                out.push('_');
            }
            out.push(c);
            punct = false;
        } else {
            punct = true;
        }
    }
    out
}

/// The codec LocOS reads a declared charset with, when it accepts it (`_SOURCE_OK`, never UTF-8 or UTF-16).
fn source_ok(label: &str) -> Option<Codec> {
    let n = py_normalize(label);
    let key = |k: &str| ALIASES.binary_search_by(|(a, _)| a.as_bytes().cmp(k.as_bytes())).ok();
    let i = key(&n).or_else(|| key(&n.replace('.', "_")))?;
    py_codec(ALIASES[i].1)
}

impl Codec {
    pub fn from_tag(tag: &str) -> Option<Codec> {
        if tag == "iso-8859-1" {
            return Some(Codec::Latin1);
        }
        if let Some(name) = tag.strip_prefix("py:") {
            return py_codec(name);
        }
        let e = Encoding::for_label(tag.as_bytes())?;
        Some(Codec::Enc(e, code_page_for_encoding(e).unwrap_or(0)))
    }

    pub fn tag(self) -> String {
        match self {
            Codec::Latin1 => "iso-8859-1".into(),
            Codec::Enc(e, id) if id < 3000 => e.name().to_ascii_lowercase(),
            Codec::Enc(_, id) => format!("py:{}", PY_CODECS.iter().find(|(_, i, _)| *i == id).map_or("", |c| c.0)),
        }
    }

    /// The Windows code page this codec is (Python's codec of that number), if it is one.
    pub fn cp(self) -> Option<u16> {
        match self {
            Codec::Enc(_, id) if CP_INFO.iter().any(|(c, _, _)| *c == id) => Some(id),
            _ => None,
        }
    }

    fn decode(self, raw: &[u8]) -> Result<String, usize> {
        match self {
            Codec::Latin1 => Ok(raw.iter().map(|&b| b as char).collect()),
            Codec::Enc(e, id) => py_decode(e, id, raw),
        }
    }
}

/// The one character Python's codec reads from exactly `seq` (see `tests/tools/gen_decdiff.py`).
fn py_one(e: &'static Encoding, id: u16, seq: &[u8]) -> Option<char> {
    let bytes = seq.iter().fold(0u64, |a, &b| (a << 8) | u64::from(b));
    let key = (u64::from(id) << 40) | ((seq.len() as u64) << 32) | bytes;
    if let Ok(i) = DEC_DIFF.binary_search_by_key(&key, |(k, _)| *k) {
        return char::from_u32(DEC_DIFF[i].1);
    }
    if matches!(id, 3002 | 3004) && seq.len() == 2 && seq.iter().any(|&b| b < 0xA1) {
        return None; // GB2312 and KS X 1001 in EUC form: both bytes A1..FE (GBK's and UHC's extensions are not)
    }
    let s = e.decode_without_bom_handling_and_without_replacement(seq)?;
    let mut it = s.chars();
    let c = it.next()?;
    it.next().is_none().then_some(c)
}

/// A KS X 1001 Annex 3 make-up sequence `A4 D4 A4 cho A4 jung A4 jong` (Python's euc_kr reads one at every `A4 D4`).
fn euckr_make_up(b: &[u8]) -> Option<char> {
    if b.len() < 8 || b[2] != 0xA4 || b[4] != 0xA4 || b[6] != 0xA4 {
        return None;
    }
    let idx = |t: &[u8; 30], c: u8| (0xA1..=0xBE).contains(&c).then(|| t[usize::from(c - 0xA1)]).filter(|&v| v != 0xFF);
    let cho = idx(&EUCKR_CHO_OF, b[3])?;
    let jung = (0xBF..=0xD3).contains(&b[5]).then(|| b[5] - 0xBF)?;
    let jong = if b[7] == 0xD4 { 0 } else { idx(&EUCKR_JONG_OF, b[7])? };
    char::from_u32(0xAC00 + u32::from(cho) * 588 + u32::from(jung) * 28 + u32::from(jong))
}

/// Python's strict decode with this codec: `Err(position)` of the first byte it cannot read.
fn py_decode(e: &'static Encoding, id: u16, raw: &[u8]) -> Result<String, usize> {
    const UNSET: u32 = 0xFFFF_FFFE;
    let mut single = [UNSET; 128];
    let mut pairs: std::collections::HashMap<u16, Option<char>> = std::collections::HashMap::new();
    let mut out = String::with_capacity(raw.len() + raw.len() / 2);
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        if b < 0x80 {
            out.push(b as char);
            i += 1;
            continue;
        }
        if id == 3004 && b == 0xA4 && raw.get(i + 1) == Some(&0xD4) {
            out.push(euckr_make_up(&raw[i..]).ok_or(i)?);
            i += 8;
            continue;
        }
        let slot = &mut single[usize::from(b - 0x80)];
        if *slot == UNSET {
            *slot = py_one(e, id, &raw[i..=i]).map_or(0xFFFF_FFFF, |c| c as u32);
        }
        if let Some(c) = char::from_u32(*slot) {
            out.push(c);
            i += 1;
            continue;
        }
        if i + 1 < raw.len() {
            let k = u16::from_be_bytes([b, raw[i + 1]]);
            if let Some(c) = *pairs.entry(k).or_insert_with(|| py_one(e, id, &raw[i..i + 2])) {
                out.push(c);
                i += 2;
                continue;
            }
        }
        if id == 3012 && b == 0x8F && i + 2 < raw.len() {
            if let Some(c) = py_one(e, id, &raw[i..i + 3]) {
                out.push(c);
                i += 3;
                continue;
            }
        }
        if id == 3013 && i + 3 < raw.len() {
            if let Some(c) = py_one(e, id, &raw[i..i + 4]) {
                out.push(c);
                i += 4;
                continue;
            }
        }
        return Err(i);
    }
    Ok(out)
}

/// The byte range of the declared charset name in the first 4096 characters of `text`.
pub fn declared_span(text: &str) -> Option<(usize, usize)> {
    let mut end = text.len().min(4096);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let m = decl_re().captures(&text[..end])?.get(2)?;
    Some((m.start(), m.end()))
}

fn declared_in_bytes(raw: &[u8]) -> Option<Codec> {
    let head: String = raw.iter().take(4096).map(|&b| if b.is_ascii() { b as char } else { '\u{fffd}' }).collect();
    let name = decl_re().captures(&head)?.get(2)?.as_str().to_string();
    source_ok(&name)
}

fn is_binary_control(b: u8) -> bool {
    b < 0x20 && !matches!(b, 0x09..=0x0D | 0x1A | 0x1B)
}

fn looks_binary(data: &[u8]) -> bool {
    if data.contains(&0) {
        return true;
    }
    data.iter().filter(|&&b| is_binary_control(b)).count() > 2 + data.len() / 100
}

fn utf16(body: &[u8], big: bool, name: &str) -> Result<String, Error> {
    let fail = |at: usize| {
        enc_err(format!(
            "This HTML Help file starts with a {name} byte-order mark but is not valid {name} (byte at position {at}); re-save the file as UTF-8 or in its Windows ANSI code page."
        ))
    };
    if !body.len().is_multiple_of(2) {
        return Err(fail(body.len() + 1));
    }
    let units = body.chunks_exact(2).map(|p| if big { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) });
    let mut s = String::with_capacity(body.len() / 2);
    for (n, r) in char::decode_utf16(units).enumerate() {
        s.push(r.map_err(|_| fail(2 + 2 * n))?);
    }
    Ok(s)
}

/// Bytes to text and the envelope tag. `hint_cp`: the code page of the help project's `Language=`; it replaces the
/// Windows-1252 default of a file that declares no charset (a BOM, valid non-ASCII UTF-8 and a declared charset still
/// win).
pub fn decode(raw: &[u8], hint_cp: Option<u16>) -> Result<(String, String), Error> {
    let hint = hint_cp.and_then(encoding_for_code_page).map(|e| Codec::Enc(e, code_page_for_encoding(e).unwrap_or(0)));
    if let Some(body) = raw.strip_prefix(BOM8) {
        let s = std::str::from_utf8(body).map_err(|e| {
            enc_err(format!(
                "This HTML Help file starts with a utf-8 byte-order mark but is not valid utf-8 (byte at position {}); re-save the file as UTF-8 or in its Windows ANSI code page.",
                e.valid_up_to() + 3
            ))
        })?;
        return Ok((s.to_string(), "utf-8-bom".into()));
    }
    if let Some(body) = raw.strip_prefix(&[0xFF, 0xFE]) {
        return Ok((utf16(body, false, "utf-16-le")?, "utf-16le".into()));
    }
    if let Some(body) = raw.strip_prefix(&[0xFE, 0xFF]) {
        return Ok((utf16(body, true, "utf-16-be")?, "utf-16be".into()));
    }
    if looks_binary(raw) {
        return Err(enc_err("file looks like a binary file, not an HTML Help file; re-save the file as UTF-8 or in its Windows ANSI code page."));
    }
    if let Ok(s) = std::str::from_utf8(raw) {
        if s.is_ascii() {
            let e = declared_in_bytes(raw).or(hint).unwrap_or(Codec::Enc(encoding_rs::WINDOWS_1252, 1252));
            return Ok((s.to_string(), e.tag()));
        }
        return Ok((s.to_string(), "utf-8".into()));
    }
    let declared = declared_in_bytes(raw);
    let mut last = 0usize;
    for e in [declared, hint, Some(Codec::Enc(encoding_rs::WINDOWS_1252, 1252))].into_iter().flatten() {
        match e.decode(raw) {
            Ok(t) => return Ok((t, e.tag())),
            Err(pos) => last = pos,
        }
    }
    Err(enc_err(format!(
        "file is not valid UTF-8 and could not be read as Windows-1252 or its declared charset either (byte 0x{:02x} at position {last}); re-save the file as UTF-8 or in its Windows ANSI code page.",
        raw.get(last).copied().unwrap_or(0)
    )))
}

pub fn is_ansi(tag: &str) -> bool {
    !matches!(tag, "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be")
}

/// Plain name of a code page for messages: `Windows-1253`, `Windows 932`.
pub fn cp_label(cp: u16) -> String {
    if cp >= 1250 {
        format!("Windows-{cp}")
    } else {
        format!("Windows {cp}")
    }
}

const NOBREAK_SPACES: [char; 2] = ['\u{2007}', '\u{202f}'];
const HYPHENS: [char; 4] = ['\u{2010}', '\u{2011}', '\u{2043}', '\u{2212}'];
const ZERO_WIDTH: [char; 3] = ['\u{200b}', '\u{2060}', '\u{feff}'];

/// Where Python's codec (what Localization OS writes) disagrees with encoding_rs about a character of this code page.
fn diff(id: u16, c: char) -> Option<&'static [u8]> {
    ENC_DIFF.binary_search_by(|(k, ch, _)| (*k, *ch).cmp(&(id, c as u32))).ok().map(|i| ENC_DIFF[i].2)
}

impl Codec {
    /// True when the code page writes `c` as one or more bytes (Python's codec rules).
    fn can_write(self, c: char) -> bool {
        if c.is_ascii() {
            return true;
        }
        match self {
            Codec::Latin1 => (c as u32) <= 0xFF,
            Codec::Enc(_, 3002) => c.len_utf8() < 4 && GB2312_OK.binary_search(&(c as u32 as u16)).is_ok() && (c as u32) <= 0xFFFF,
            Codec::Enc(e, id) => match diff(id, c) {
                Some(b) => !b.is_empty(),
                None => {
                    let mut b = [0u8; 4];
                    !e.encode(c.encode_utf8(&mut b)).2
                }
            },
        }
    }

    fn all_writable(self, text: &str) -> bool {
        if text.is_ascii() {
            return true;
        }
        match self {
            Codec::Latin1 => text.chars().all(|c| (c as u32) <= 0xFF),
            Codec::Enc(_, 3002) => text.chars().all(|c| self.can_write(c)),
            Codec::Enc(e, id) => {
                if text.chars().any(|c| !c.is_ascii() && diff(id, c).is_some()) {
                    return text.chars().all(|c| self.can_write(c));
                }
                !e.encode(text).2
            }
        }
    }

    fn encode_str(self, text: &str) -> Vec<u8> {
        match self {
            Codec::Latin1 => text.chars().map(|c| c as u32 as u8).collect(),
            Codec::Enc(e, 3004) => {
                // Python's euc_kr: KS X 1001 as encoding_rs writes it; any other Hangul syllable as a make-up sequence
                let mut out = Vec::with_capacity(text.len());
                let mut b = [0u8; 4];
                for c in text.chars() {
                    let bytes = e.encode(c.encode_utf8(&mut b)).0;
                    let k = c as u32;
                    if (0xAC00..=0xD7A3).contains(&k) && bytes.iter().any(|&x| x < 0xA1) {
                        let k = (k - 0xAC00) as usize;
                        let jung = u8::try_from((k % 588) / 28).unwrap_or(0);
                        out.extend_from_slice(&[0xA4, 0xD4, 0xA4, EUCKR_CHO_BYTE[k / 588], 0xA4, 0xBF + jung, 0xA4, EUCKR_JONG_BYTE[k % 28]]);
                    } else {
                        out.extend_from_slice(&bytes);
                    }
                }
                out
            }
            Codec::Enc(e, id) => {
                if text.is_ascii() || !text.chars().any(|c| diff(id, c).is_some()) {
                    return e.encode(text).0.into_owned();
                }
                let mut out = Vec::with_capacity(text.len());
                let mut b = [0u8; 4];
                for c in text.chars() {
                    match diff(id, c) {
                        Some(bytes) => out.extend_from_slice(bytes),
                        None => out.extend_from_slice(&e.encode(c.encode_utf8(&mut b)).0),
                    }
                }
                out
            }
        }
    }
}

/// Windows-1258 writes most toned Vietnamese letters as base + combining tone mark (#681).
fn cp1258_text(text: &str, codec: Codec) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if codec.can_write(c) {
            out.push(c);
        } else if let Ok(i) = CP1258.binary_search_by(|(k, _)| k.cmp(&c)) {
            out.push_str(CP1258[i].1);
        } else {
            out.push(c);
        }
    }
    out
}

/// #715: typographic characters no ANSI code page needs to refuse a file over become the nearest code-page one.
fn typographic(text: &str, codec: Codec) -> String {
    if codec.all_writable(text) {
        return text.to_string();
    }
    let nbsp_ok = codec.can_write('\u{a0}');
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if codec.can_write(c) {
            out.push(c);
        } else if ZERO_WIDTH.contains(&c) {
        } else if HYPHENS.contains(&c) {
            out.push('-');
        } else if NOBREAK_SPACES.contains(&c) || is_zs(c) {
            out.push(if NOBREAK_SPACES.contains(&c) && nbsp_ok { '\u{a0}' } else { ' ' });
        } else {
            out.push(c);
        }
    }
    out
}

/// The text as the code page will write it: Windows-1258 composition, then the typographic fallbacks.
fn for_codepage(text: &str, codec: Codec) -> String {
    let t;
    let text = if codec.cp() == Some(1258) {
        t = cp1258_text(text, codec);
        &t
    } else {
        text
    };
    typographic(text, codec)
}

/// Why a text cannot be written in an ANSI code page: the first character with no byte there.
pub struct Unwritable(pub char);

pub fn encode_ansi(text: &str, codec: Codec) -> Result<Vec<u8>, Unwritable> {
    let t = for_codepage(text, codec);
    if !codec.all_writable(&t) {
        let bad = t.chars().find(|&c| !codec.can_write(c)).unwrap_or('?');
        return Err(Unwritable(bad));
    }
    Ok(codec.encode_str(&t))
}

/// Text to bytes in a non-ANSI envelope.
pub fn encode_unicode(text: &str, tag: &str) -> Vec<u8> {
    match tag {
        "utf-8-bom" => {
            let mut v = BOM8.to_vec();
            v.extend_from_slice(text.as_bytes());
            v
        }
        "utf-16le" | "utf-16be" => {
            let big = tag == "utf-16be";
            let mut v: Vec<u8> = if big { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
            for u in text.encode_utf16() {
                v.extend_from_slice(&if big { u.to_be_bytes() } else { u.to_le_bytes() });
            }
            v
        }
        _ => text.as_bytes().to_vec(),
    }
}

/// The text in the code page with no fallbacks (for text that was decoded from it).
pub fn encode_ansi_raw(text: &str, codec: Codec) -> Vec<u8> {
    codec.encode_str(text)
}

/// True when a declared charset label names the Python codec of Windows code page `cp` (Localization OS compares
/// `codecs.lookup` names: `GBK` is code page 936, `gb2312` is not).
pub fn label_is_code_page(label: &str, cp: u16) -> bool {
    let Some(name) = PY_CODECS.iter().find(|(_, id, _)| *id == cp).map(|c| c.0) else { return false };
    source_ok(label).is_some() && source_ok(label) == py_codec(name)
}

/// True when a declared charset label is Python's UTF-8 codec.
pub fn label_is_utf8(label: &str) -> bool {
    matches!(py_normalize(label).as_str(), "utf_8" | "utf8" | "u8" | "utf" | "utf8_ucs2" | "utf8_ucs4" | "cp65001")
}
