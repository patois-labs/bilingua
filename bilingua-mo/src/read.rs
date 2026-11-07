//! Reading a compiled catalog. Every count, offset and length in the file is untrusted: each one is checked
//! against the file size before it is used, and no allocation is sized from a number in the file.
//!
//! Layout (32-bit integers in the byte order the magic announces): magic, revision, N (strings), O (original
//! table), T (translation table), S (hash table size), H (hash table offset); revision minor 1 adds five words of
//! system-dependent segments. Each table holds N (length, offset) pairs; a string is `length` bytes at `offset`,
//! NUL terminated. A context is `msgctxt EOT msgid`; plural forms are NUL separated.

use bilingua_model::encoding_rs::{self, Encoding};
use bilingua_model::{enc, Error, LimitError, LimitKind, Limits};

pub const LE_MAGIC: u32 = 0x9504_12de;
pub const BE_MAGIC: u32 = 0xde12_0495;
pub const HEADER_BYTES: usize = 28;
const SYSDEP_END: usize = 48;

/// How the strings of a catalog are encoded (the charset its header declares).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Utf8,
    Ascii,
    Latin1,
    Legacy(&'static Encoding),
}

/// One message as the file holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Position in the file's table, from 1 (the header entry is usually 1).
    pub index: usize,
    pub ctxt: Option<String>,
    pub id: String,
    pub id_plural: Option<String>,
    /// `[msgstr]`, or the plural slots `msgstr[0]`, `msgstr[1]`, ...
    pub strs: Vec<String>,
}

impl Entry {
    pub fn is_header(&self) -> bool {
        self.ctxt.is_none() && self.id.is_empty()
    }
}

pub struct Catalog {
    pub entries: Vec<Entry>,
    pub codec: Codec,
}

pub fn damaged(msg: impl Into<String>) -> Error {
    Error::Parse { format: "mo".into(), msg: msg.into() }
}

fn limit(kind: LimitKind, msg: impl Into<String>) -> Error {
    Error::Limit(LimitError::new(kind, msg))
}

fn word(buf: &[u8], off: u64, big: bool) -> Option<u32> {
    let off = usize::try_from(off).ok()?;
    let b: [u8; 4] = buf.get(off..off.checked_add(4)?)?.try_into().ok()?;
    Some(if big { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) })
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_py_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// The charset names declared in a header text: the byte ranges of the name after `charset=` on each
/// `Content-Type:` line (at most one per line; a name is 1 to 40 of `A-Za-z0-9_.:-`).
pub fn charset_spans(h: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut ls = 0;
    loop {
        let le = h[ls..].iter().position(|&b| b == b'\n').map_or(h.len(), |p| ls + p);
        let line = &h[ls..le];
        if line.len() >= 13 && line[..13].eq_ignore_ascii_case(b"Content-Type:") {
            let mut i = 13;
            while i + 7 <= line.len() {
                if line[i..i + 7].eq_ignore_ascii_case(b"charset") && (i == 0 || !is_word_byte(line[i - 1])) {
                    let mut j = i + 7;
                    while line.get(j).is_some_and(|&b| is_py_space(b)) {
                        j += 1;
                    }
                    if line.get(j) == Some(&b'=') {
                        j += 1;
                        while line.get(j).is_some_and(|&b| is_py_space(b)) {
                            j += 1;
                        }
                        let mut k = j;
                        while k < line.len() && k - j < 40 && (line[k].is_ascii_alphanumeric() || matches!(line[k], b'_' | b'.' | b':' | b'-')) {
                            k += 1;
                        }
                        if k > j {
                            out.push((ls + j, ls + k));
                            break;
                        }
                    }
                }
                i += 1;
            }
        }
        if le >= h.len() {
            break;
        }
        ls = le + 1;
    }
    out
}

/// The codec a header charset label names. Anything unknown (the template word `CHARSET`) is UTF-8, as msgfmt
/// writes it.
pub fn codec_for_label(label: &str) -> Codec {
    let l = label.to_ascii_lowercase().replace('_', "-");
    match l.as_str() {
        "utf-8" | "utf8" | "u8" | "utf" | "utf8-ucs2" => return Codec::Utf8,
        "ascii" | "us-ascii" | "646" | "ansi-x3.4-1968" | "iso646-us" | "us" => return Codec::Ascii,
        "latin1" | "latin-1" | "l1" | "iso-8859-1" | "iso8859-1" | "8859" | "cp819" | "iso8859" | "iso-8859" | "latin" | "ibm819"
        | "csisolatin1" | "iso-ir-100" | "iso-8859-1:1987" | "iso-8859-1-1987" => return Codec::Latin1,
        // Windows code page names Python knows and WHATWG does not (encoding_rs's Shift_JIS, EUC-KR and GBK are
        // the Windows variants of those code pages).
        "cp932" | "ms932" => return Codec::Legacy(encoding_rs::SHIFT_JIS),
        "cp949" | "ms949" | "uhc" => return Codec::Legacy(encoding_rs::EUC_KR),
        "cp936" | "ms936" => return Codec::Legacy(encoding_rs::GBK),
        _ => {}
    }
    match Encoding::for_label(l.as_bytes()) {
        // a catalog may declare any other code page too (iso-8859-4, -13, -16, mac, ibm866 ...)
        Some(e) if enc::is_declared_legacy(e) || e == encoding_rs::WINDOWS_1252 => Codec::Legacy(e),
        Some(e) if !matches!(e.name(), "UTF-8" | "UTF-16LE" | "UTF-16BE" | "replacement" | "x-user-defined") => Codec::Legacy(e),
        _ => Codec::Utf8,
    }
}

pub fn codec_name(c: Codec) -> &'static str {
    match c {
        Codec::Utf8 => "UTF-8",
        Codec::Ascii => "ASCII",
        Codec::Latin1 => "ISO-8859-1",
        Codec::Legacy(e) => e.name(),
    }
}

fn charset_of(header_msgstr: &[u8]) -> Codec {
    match charset_spans(header_msgstr).first() {
        Some(&(a, b)) => codec_for_label(&String::from_utf8_lossy(&header_msgstr[a..b])),
        None => Codec::Utf8,
    }
}

pub fn decode(c: Codec, bytes: &[u8]) -> Option<String> {
    match c {
        Codec::Utf8 => std::str::from_utf8(bytes).ok().map(str::to_string),
        Codec::Ascii => bytes.is_ascii().then(|| String::from_utf8_lossy(bytes).into_owned()),
        Codec::Latin1 => Some(bytes.iter().map(|&b| b as char).collect()),
        Codec::Legacy(e) => enc::decode_strict(e, bytes).ok(),
    }
}

/// The string in `c`, or `None` when a character does not fit.
pub fn encode(c: Codec, s: &str) -> Option<Vec<u8>> {
    match c {
        Codec::Utf8 | Codec::Ascii => Some(s.as_bytes().to_vec()),
        Codec::Latin1 => s.chars().map(|ch| u8::try_from(u32::from(ch)).ok()).collect(),
        Codec::Legacy(e) => {
            let (bytes, _, unmappable) = e.encode(s);
            (!unmappable).then(|| bytes.into_owned())
        }
    }
}

/// Reads and validates a catalog.
pub fn read(buf: &[u8], limits: &Limits) -> Result<Catalog, Error> {
    if buf.len() < HEADER_BYTES {
        return Err(damaged("this is not a gettext MO file: it is shorter than the MO header"));
    }
    let big = match u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) {
        LE_MAGIC => false,
        BE_MAGIC => true,
        _ => return Err(damaged("this is not a gettext MO file (bad magic number)")),
    };
    let w = |i: u64| word(buf, 4 * i, big).unwrap_or(0);
    let (revision, n, o, t, s, h) = (w(1), u64::from(w(2)), u64::from(w(3)), u64::from(w(4)), u64::from(w(5)), u64::from(w(6)));
    let (major, minor) = (revision >> 16, revision & 0xFFFF);
    if major > 1 {
        return Err(Error::Unsupported(format!("unsupported MO file format revision {major}.{minor}")));
    }
    if n > limits.max_units as u64 {
        return Err(limit(
            LimitKind::Units,
            format!("This MO file has {n} strings, more than the {} segments allowed. Split it into smaller catalogs.", limits.max_units),
        ));
    }
    let size = buf.len() as u64;
    for (name, table) in [("original", o), ("translation", t)] {
        if table < HEADER_BYTES as u64 || table + 8 * n > size {
            return Err(damaged(format!("this MO file is damaged: its {name} table lies outside the file")));
        }
    }
    if s != 0 && (h < HEADER_BYTES as u64 || h + 4 * s > size) {
        return Err(damaged("this MO file is damaged: its hash table lies outside the file"));
    }
    // System-dependent strings (<PRIu64> and friends) are expanded per platform by libintl; there is no text to
    // translate or write back, so refuse rather than drop them from a rebuilt file.
    if minor >= 1 && buf.len() >= SYSDEP_END && w(9) != 0 {
        return Err(Error::Unsupported(
            "this MO file has system-dependent strings (<inttypes.h> macros); use the PO file it was compiled from".into(),
        ));
    }
    let per_string = 8 * limits.max_unit_chars as u64;
    let mut total = 0u64;
    let mut ranges: [Vec<(usize, usize)>; 2] = [Vec::new(), Vec::new()];
    for (slot, table) in [o, t].into_iter().enumerate() {
        ranges[slot].reserve(n as usize);
        for i in 0..n {
            let length = u64::from(word(buf, table + 8 * i, big).unwrap_or(0));
            let off = u64::from(word(buf, table + 8 * i + 4, big).unwrap_or(0));
            // Room for the string and its NUL terminator: a string ending exactly at the end of the file is damaged.
            if off + length >= size {
                return Err(damaged("this MO file is damaged: a string lies outside the file"));
            }
            if length > per_string {
                return Err(limit(
                    LimitKind::UnitChars,
                    format!("This MO file has a string longer than {per_string} bytes (8 x the segment length limit). Split it into smaller catalogs."),
                ));
            }
            total += length;
            ranges[slot].push((off as usize, length as usize));
        }
    }
    // Overlapping offsets could make a small file expand into gigabytes of decoded text.
    if total > 2 * size {
        return Err(limit(
            LimitKind::InputBytes,
            "This MO file references more string bytes than it holds (overlapping strings). Recompile it with msgfmt.",
        ));
    }
    let slice = |(off, len): (usize, usize)| &buf[off..off + len];
    let mut codec = Codec::Utf8;
    for (a, b) in ranges[0].iter().zip(&ranges[1]) {
        if a.1 == 0 {
            codec = charset_of(slice(*b));
            break;
        }
    }
    let mut entries = Vec::with_capacity(ranges[0].len());
    let bad_text = || damaged(format!("this MO file's strings are not valid {} (the charset its header declares)", codec_name(codec)));
    for (i, (a, b)) in ranges[0].iter().zip(&ranges[1]).enumerate() {
        let (mut raw_id, raw_str) = (slice(*a), slice(*b));
        let mut ctxt = None;
        if let Some(p) = raw_id.iter().position(|&c| c == 4) {
            ctxt = Some(decode(codec, &raw_id[..p]).ok_or_else(bad_text)?);
            raw_id = &raw_id[p + 1..];
        }
        let id_text = decode(codec, raw_id).ok_or_else(bad_text)?;
        let str_text = decode(codec, raw_str).ok_or_else(bad_text)?;
        let ids: Vec<&str> = id_text.split('\0').collect();
        let strs: Vec<&str> = str_text.split('\0').collect();
        if ids.len() > 2 || (ids.len() == 1 && strs.len() > 1) {
            return Err(damaged("this MO file is damaged: a message has stray NUL bytes"));
        }
        entries.push(Entry {
            index: i + 1,
            ctxt,
            id: ids[0].to_string(),
            id_plural: ids.get(1).map(|s| s.to_string()),
            strs: strs.iter().map(|s| s.to_string()).collect(),
        });
    }
    Ok(Catalog { entries, codec })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Labels Python's codecs read and WHATWG does not (or reads as Windows-1252): decoded as LocOS decodes them.
    #[test]
    fn python_only_charset_labels() {
        for l in ["latin", "csisolatin1", "IBM819", "iso-ir-100", "ISO_8859-1:1987"] {
            assert_eq!(decode(codec_for_label(l), b"\x80\xe9").as_deref(), Some("\u{80}\u{e9}"), "{l}");
        }
        assert_eq!(decode(codec_for_label("CP949"), b"\xb0\xa1\xc7\xd1").as_deref(), Some("\u{ac00}\u{d55c}"));
        assert_eq!(decode(codec_for_label("cp932"), b"\x93\xfa\x96{").as_deref(), Some("\u{65e5}\u{672c}"));
        assert_eq!(decode(codec_for_label("cp936"), b"\xd6\xd0\xce\xc4").as_deref(), Some("\u{4e2d}\u{6587}"));
    }
}
