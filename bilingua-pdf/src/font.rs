//! Fonts as the text layer needs them: char codes from string bytes, the Unicode of each code, advance widths
//! and an approximate glyph box. The rules are PDFium's (`CPDF_Font` and its subclasses): ToUnicode first, then
//! the encoding's glyph names through the Adobe glyph list, and for a code with no Unicode at all the code itself
//! (PDFium's "not unicode" characters).
//!
//! What a font program alone knows (glyph boxes, the built-in encoding of a CFF or TrueType program, glyph names
//! in a TrueType `post` table) is not read; the box is approximated from the descriptor and the advance, the
//! encoding of a Type 1 program is read from its clear-text part. The report lists the known differences.

use crate::obj::{Dict, Lexer, Obj, Tok};
use bilingua_model::encoding_rs;
use crate::fontprog::Program;
use crate::tables;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const INVALID_CODE: u32 = 0xFFFF_FFFF;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BaseEnc {
    Builtin,
    Standard,
    WinAnsi,
    MacRoman,
    MacExpert,
    PdfDoc,
    Symbol,
    Zapf,
    MsSymbol,
}

fn base_name(enc: BaseEnc, code: u8) -> Option<&'static str> {
    let (table, first): (&[Option<&'static str>], usize) = match enc {
        BaseEnc::Standard => (&tables::STANDARD_NAMES, tables::STANDARD_NAMES_FIRST),
        BaseEnc::WinAnsi => (&tables::WINANSI_NAMES, tables::WINANSI_NAMES_FIRST),
        BaseEnc::MacRoman => (&tables::MACROMAN_NAMES, tables::MACROMAN_NAMES_FIRST),
        BaseEnc::MacExpert => (&tables::MACEXPERT_NAMES, tables::MACEXPERT_NAMES_FIRST),
        BaseEnc::PdfDoc => (&tables::PDFDOC_NAMES, tables::PDFDOC_NAMES_FIRST),
        BaseEnc::Symbol => (&tables::SYMBOL_NAMES, tables::SYMBOL_NAMES_FIRST),
        BaseEnc::Zapf => (&tables::ZAPF_NAMES, tables::ZAPF_NAMES_FIRST),
        _ => return None,
    };
    let c = code as usize;
    if c < first {
        return None;
    }
    table.get(c - first).copied().flatten()
}

/// PDFium's `UnicodeFromAdobeName` (FreeType's rules): `uniXXXX`, `uXXXX[XX]`, then the glyph list, a suffix
/// after the first non-initial dot ignored. 0 when unknown.
pub fn unicode_from_name(name: &str) -> u32 {
    let b = name.as_bytes();
    let hexup = |c: u8| -> Option<u32> {
        match c {
            b'0'..=b'9' => Some((c - b'0') as u32),
            b'A'..=b'F' => Some((c - b'A' + 10) as u32),
            _ => None,
        }
    };
    if b.starts_with(b"uni") {
        let mut v = 0u32;
        let mut n = 0;
        while n < 4 {
            match b.get(3 + n).and_then(|c| hexup(*c)) {
                Some(d) => v = (v << 4) + d,
                None => break,
            }
            n += 1;
        }
        if n == 4 && matches!(b.get(7), None | Some(b'.')) {
            return v;
        }
    }
    if b.first() == Some(&b'u') {
        let mut v = 0u32;
        let mut n = 0;
        while n < 6 {
            match b.get(1 + n).and_then(|c| hexup(*c)) {
                Some(d) => v = (v << 4) + d,
                None => break,
            }
            n += 1;
        }
        if n >= 4 && matches!(b.get(1 + n), None | Some(b'.')) {
            return v;
        }
    }
    let base = match b.iter().skip(1).position(|c| *c == b'.') {
        Some(p) => &name[..p + 1],
        None => name,
    };
    match tables::AGL.binary_search_by(|(n, _)| (*n).cmp(base)) {
        Ok(i) => tables::AGL[i].1 as u32,
        Err(_) => 0,
    }
}

// ---------------------------------------------------------------- ToUnicode

#[derive(Default, Debug)]
pub struct ToUnicode {
    map: HashMap<u32, Vec<u16>>,
    reverse: HashMap<u16, u32>,
}

impl ToUnicode {
    /// PDFium's `CPDF_ToUnicodeMap`: bfchar and bfrange sections whose declared count matches (an over-long or
    /// malformed section is dropped whole), codes up to 0xFFFF, later mappings replace earlier ones.
    pub fn parse(data: &[u8]) -> ToUnicode {
        let mut t = ToUnicode::default();
        let words = cmap_words(data);
        let mut i = 0;
        while i < words.len() {
            let w = &words[i];
            if w == b"beginbfchar" {
                let count = if i > 0 { atoi(&words[i - 1]) } else { -1 };
                let mut j = i + 1;
                let mut items = Vec::new();
                let mut valid = (0..=160_000).contains(&count);
                while j < words.len() && words[j] != b"endbfchar" {
                    if valid {
                        match code_of(&words[j]) {
                            Some(c) if c <= 0xFFFF && j + 1 < words.len() => {
                                items.push((c, words[j + 1].clone()));
                                j += 1;
                                if items.len() as i64 > count {
                                    valid = false;
                                }
                            }
                            _ => valid = false,
                        }
                    }
                    j += 1;
                }
                if valid && items.len() as i64 == count {
                    for (c, dst) in items {
                        t.set(c, wide_of(&dst));
                    }
                }
                i = j;
            } else if w == b"beginbfrange" {
                let count = if i > 0 { atoi(&words[i - 1]) } else { -1 };
                let mut valid = (0..=160_000).contains(&count);
                let mut j = i + 1;
                enum R {
                    Words(u32, Vec<Vec<u8>>),
                    Single(u32, u32, u32),
                    Multi(u32, Vec<Vec<u16>>),
                }
                let mut ranges: Vec<R> = Vec::new();
                while j < words.len() && words[j] != b"endbfrange" {
                    if !valid {
                        j += 1;
                        continue;
                    }
                    let (Some(lo), Some(hi)) = (code_of(&words[j]), words.get(j + 1).and_then(|w| code_of(w))) else {
                        valid = false;
                        j += 1;
                        continue;
                    };
                    let hi = (lo & 0xffff_ff00) | (hi & 0xff);
                    if lo > 0xFFFF || hi > 0xFFFF || lo > hi {
                        valid = false;
                        j += 2;
                        continue;
                    }
                    let Some(start) = words.get(j + 2) else {
                        valid = false;
                        break;
                    };
                    if start == b"[" {
                        let mut list = Vec::new();
                        let mut k = j + 3;
                        for _ in lo..=hi {
                            if let Some(w) = words.get(k) {
                                list.push(w.clone());
                            }
                            k += 1;
                        }
                        if words.get(k).map(|w| w.as_slice()) != Some(b"]") {
                            valid = false;
                        }
                        ranges.push(R::Words(lo, list));
                        j = k + 1;
                    } else {
                        let dst = wide_of(start);
                        if dst.len() == 1 {
                            ranges.push(R::Single(lo, hi, dst[0] as u32));
                        } else {
                            let mut list = vec![dst];
                            for _ in lo + 1..=hi {
                                let next = string_add(list.last().unwrap());
                                list.push(next);
                            }
                            ranges.push(R::Multi(lo, list));
                        }
                        j += 3;
                    }
                    if ranges.len() as i64 > count {
                        valid = false;
                    }
                }
                if valid && ranges.len() as i64 == count {
                    for r in ranges {
                        match r {
                            R::Words(lo, list) => {
                                for (k, w) in list.iter().enumerate() {
                                    t.set(lo + k as u32, wide_of(w));
                                }
                            }
                            R::Single(lo, hi, start) => {
                                for (k, c) in (lo..=hi).enumerate() {
                                    t.insert(c, vec![(start + k as u32) as u16]);
                                }
                            }
                            R::Multi(lo, list) => {
                                for (k, w) in list.into_iter().enumerate() {
                                    t.insert(lo + k as u32, w);
                                }
                            }
                        }
                    }
                }
                i = j;
            }
            i += 1;
        }
        t
    }

    fn set(&mut self, code: u32, dst: Vec<u16>) {
        if !dst.is_empty() {
            self.insert(code, dst);
        }
    }

    fn insert(&mut self, code: u32, dst: Vec<u16>) {
        if dst.len() == 1 {
            self.reverse.insert(dst[0], code);
        }
        self.map.insert(code, dst);
    }

    pub fn lookup(&self, code: u32) -> Option<&Vec<u16>> {
        self.map.get(&code)
    }

    pub fn reverse(&self, u: u16) -> u32 {
        self.reverse.get(&u).copied().unwrap_or(0)
    }
}

fn string_add(s: &[u16]) -> Vec<u16> {
    let mut out = s.to_vec();
    let mut carry = 1u16;
    for c in out.iter_mut().rev() {
        let (v, o) = c.overflowing_add(carry);
        *c = v;
        carry = if o { 1 } else { 0 };
        if carry == 0 {
            break;
        }
    }
    if carry != 0 {
        out.insert(0, carry);
    }
    out
}

/// PDFium's `CPDF_SimpleParser` words: `<...>` hex strings kept whole, `(...)` strings, names, `[`, `]`.
fn cmap_words(data: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    let n = data.len();
    while i < n && out.len() < 4_000_000 {
        let c = data[i];
        if crate::obj::is_ws(c) {
            i += 1;
            continue;
        }
        if c == b'%' {
            while i < n && data[i] != b'\n' && data[i] != b'\r' {
                i += 1;
            }
            continue;
        }
        let s = i;
        match c {
            b'<' => {
                if data.get(i + 1) == Some(&b'<') {
                    i += 2;
                } else {
                    while i < n && data[i] != b'>' {
                        i += 1;
                    }
                    i = (i + 1).min(n);
                }
            }
            b'>' => {
                i += if data.get(i + 1) == Some(&b'>') { 2 } else { 1 };
            }
            b'(' => {
                let mut depth = 0;
                while i < n {
                    match data[i] {
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        b'\\' => i += 1,
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'[' | b']' | b'{' | b'}' => i += 1,
            _ => {
                i += 1;
                while i < n && crate::obj::is_regular(data[i]) {
                    i += 1;
                }
            }
        }
        out.push(data[s..i.min(n)].to_vec());
    }
    out
}

fn atoi(w: &[u8]) -> i64 {
    let mut v: i64 = 0;
    let mut neg = false;
    let mut i = 0;
    if w.first() == Some(&b'-') {
        neg = true;
        i = 1;
    } else if w.first() == Some(&b'+') {
        i = 1;
    }
    if i >= w.len() || !w[i].is_ascii_digit() {
        return 0;
    }
    while i < w.len() && w[i].is_ascii_digit() {
        v = v.saturating_mul(10).saturating_add((w[i] - b'0') as i64);
        i += 1;
    }
    if neg {
        -v
    } else {
        v
    }
}

fn hexd(c: u8) -> Option<u32> {
    (c as char).to_digit(16)
}

/// `<hex>` to a code; whitespace inside ignored; None for anything else or overflow.
fn code_of(w: &[u8]) -> Option<u32> {
    if w.len() <= 2 || w[0] != b'<' || w[w.len() - 1] != b'>' {
        return None;
    }
    let mut v: u64 = 0;
    for &c in &w[1..w.len() - 1] {
        if crate::obj::is_ws(c) {
            continue;
        }
        v = v * 16 + hexd(c)? as u64;
        if v > u32::MAX as u64 {
            return None;
        }
    }
    Some(v as u32)
}

/// `<hex>` to UTF-16 units, four digits per unit, stopping at the first non-hex character.
fn wide_of(w: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    if w.len() <= 2 || w[0] != b'<' || w[w.len() - 1] != b'>' {
        return out;
    }
    let (mut ch, mut n) = (0u32, 0);
    for &c in &w[1..w.len() - 1] {
        if crate::obj::is_ws(c) {
            continue;
        }
        let Some(d) = hexd(c) else { break };
        ch = ch.wrapping_mul(16).wrapping_add(d);
        n += 1;
        if n == 4 {
            out.push(ch as u16);
            ch = 0;
            n = 0;
        }
    }
    out
}

// ---------------------------------------------------------------- CMaps of Type 0 fonts

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Coding {
    Cid,
    Ucs2,
    Utf16,
    /// A legacy multi-byte charset (the CMap's bytes are that charset's text).
    Legacy(&'static encoding_rs::Encoding),
    Unknown,
}

#[derive(Clone, Debug)]
enum Scheme {
    OneByte,
    TwoBytes,
    MixedTwo(Box<[bool; 256]>),
    /// Codespace ranges: (byte count, low bytes, high bytes).
    MixedFour(Vec<(usize, [u8; 4], [u8; 4])>),
}

#[derive(Clone, Debug)]
pub struct CMap {
    scheme: Scheme,
    pub coding: Coding,
    pub vertical: bool,
    /// Embedded CMap code to CID ranges (start code, end code, start cid); empty means code == CID.
    cid_ranges: Vec<(u32, u32, u32)>,
}

impl CMap {
    fn identity(vertical: bool) -> CMap {
        CMap { scheme: Scheme::TwoBytes, coding: Coding::Cid, vertical, cid_ranges: vec![] }
    }

    fn predefined(name: &[u8]) -> Option<CMap> {
        if name == b"Identity-H" || name == b"Identity-V" {
            return Some(CMap::identity(name == b"Identity-V"));
        }
        let vertical = name.last() == Some(&b'V');
        let stem = if name.len() > 2 { &name[..name.len() - 2] } else { name };
        let stem = std::str::from_utf8(stem).ok()?;
        type Ranges = &'static [(u8, u8)];
        let (coding, ranges): (Coding, Option<Ranges>) = match stem {
            "GB-EUC" => (Coding::Legacy(encoding_rs::GBK), Some(&[(0xa1, 0xfe)])),
            "GBpc-EUC" => (Coding::Legacy(encoding_rs::GBK), Some(&[(0xa1, 0xfc)])),
            "GBK-EUC" | "GBKp-EUC" | "GBK2K-EUC" | "GBK2K" => (Coding::Legacy(encoding_rs::GB18030), Some(&[(0x81, 0xfe)])),
            "UniGB-UCS2" | "UniCNS-UCS2" | "UniJIS-UCS2" | "UniJIS-UCS2-HW" | "UniKS-UCS2" => (Coding::Ucs2, None),
            "UniGB-UTF16" | "UniCNS-UTF16" | "UniJIS-UTF16" | "UniKS-UTF16" => (Coding::Utf16, None),
            "B5pc" => (Coding::Legacy(encoding_rs::BIG5), Some(&[(0xa1, 0xfc)])),
            "HKscs-B5" => (Coding::Legacy(encoding_rs::BIG5), Some(&[(0x88, 0xfe)])),
            "ETen-B5" | "ETenms-B5" => (Coding::Legacy(encoding_rs::BIG5), Some(&[(0xa1, 0xfe)])),
            "83pv-RKSJ" | "90ms-RKSJ" | "90msp-RKSJ" | "90pv-RKSJ" | "Add-RKSJ" | "Ext-RKSJ" => {
                (Coding::Legacy(encoding_rs::SHIFT_JIS), Some(&[(0x81, 0x9f), (0xe0, 0xfc)]))
            }
            "EUC" => (Coding::Legacy(encoding_rs::EUC_JP), Some(&[(0x8e, 0x8e), (0xa1, 0xfe)])),
            "KSC-EUC" | "KSCpc-EUC" => (Coding::Legacy(encoding_rs::EUC_KR), Some(&[(0xa1, 0xfe)])),
            "KSCms-UHC" | "KSCms-UHC-HW" => (Coding::Legacy(encoding_rs::EUC_KR), Some(&[(0x81, 0xfe)])),
            _ => return None,
        };
        let scheme = match ranges {
            None => Scheme::TwoBytes,
            Some(r) => {
                let mut lead = [false; 256];
                for (a, b) in r {
                    for x in *a..=*b {
                        lead[x as usize] = true;
                    }
                }
                Scheme::MixedTwo(Box::new(lead))
            }
        };
        Some(CMap { scheme, coding, vertical, cid_ranges: vec![] })
    }

    /// An embedded CMap stream: codespace ranges, cidrange/cidchar, `usecmap` of a predefined CMap.
    fn embedded(data: &[u8]) -> CMap {
        let words = cmap_words(data);
        let mut spaces: Vec<(usize, [u8; 4], [u8; 4])> = Vec::new();
        let mut ranges: Vec<(u32, u32, u32)> = Vec::new();
        let mut base: Option<CMap> = None;
        let mut vertical = false;
        let mut i = 0;
        while i < words.len() {
            let w = words[i].as_slice();
            match w {
                b"begincodespacerange" => {
                    let mut j = i + 1;
                    while j + 1 < words.len() && words[j] != b"endcodespacerange" {
                        let (a, b) = (hex_bytes(&words[j]), hex_bytes(&words[j + 1]));
                        if !a.is_empty() && a.len() == b.len() && a.len() <= 4 {
                            let mut lo = [0u8; 4];
                            let mut hi = [0u8; 4];
                            lo[..a.len()].copy_from_slice(&a);
                            hi[..b.len()].copy_from_slice(&b);
                            spaces.push((a.len(), lo, hi));
                        }
                        j += 2;
                    }
                    i = j;
                }
                b"begincidrange" => {
                    let mut j = i + 1;
                    while j + 2 < words.len() && words[j] != b"endcidrange" {
                        if let (Some(a), Some(b)) = (code_of(&words[j]), code_of(&words[j + 1])) {
                            ranges.push((a, b, atoi(&words[j + 2]).max(0) as u32));
                        }
                        j += 3;
                    }
                    i = j;
                }
                b"begincidchar" => {
                    let mut j = i + 1;
                    while j + 1 < words.len() && words[j] != b"endcidchar" {
                        if let Some(a) = code_of(&words[j]) {
                            ranges.push((a, a, atoi(&words[j + 1]).max(0) as u32));
                        }
                        j += 2;
                    }
                    i = j;
                }
                b"usecmap" if i > 0 => {
                    if let Some(n) = words[i - 1].strip_prefix(b"/") {
                        base = CMap::predefined(n);
                    }
                }
                b"/WMode" => {
                    if words.get(i + 1).map(|w| w.as_slice()) == Some(b"1") {
                        vertical = true;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        let mut m = base.unwrap_or_else(|| CMap::identity(false));
        if !spaces.is_empty() {
            m.scheme = if spaces.iter().all(|s| s.0 == 1) {
                Scheme::OneByte
            } else if spaces.iter().all(|s| s.0 == 2) {
                Scheme::TwoBytes
            } else {
                Scheme::MixedFour(spaces)
            };
        }
        if !ranges.is_empty() {
            ranges.sort_by_key(|r| r.0);
            m.cid_ranges = ranges;
        }
        m.vertical |= vertical;
        m
    }

    fn next_char(&self, s: &[u8], off: &mut usize) -> u32 {
        let get = |o: &mut usize| -> u8 {
            let b = s.get(*o).copied().unwrap_or(0);
            *o += 1;
            b
        };
        match &self.scheme {
            Scheme::OneByte => get(off) as u32,
            Scheme::TwoBytes => {
                let a = get(off) as u32;
                let b = get(off) as u32;
                a * 256 + b
            }
            Scheme::MixedTwo(lead) => {
                let a = get(off);
                if !lead[a as usize] {
                    return a as u32;
                }
                let b = get(off) as u32;
                a as u32 * 256 + b
            }
            Scheme::MixedFour(spaces) => {
                let mut codes = [0u8; 4];
                let mut size = 1;
                codes[0] = get(off);
                loop {
                    let r = check_range(&codes[..size], spaces);
                    if r == 0 {
                        return 0;
                    }
                    if r == 2 {
                        let mut v = 0u32;
                        for c in &codes[..size] {
                            v = (v << 8) | *c as u32;
                        }
                        return v;
                    }
                    if size == 4 || *off >= s.len() {
                        return 0;
                    }
                    codes[size] = get(off);
                    size += 1;
                }
            }
        }
    }

    fn cid(&self, code: u32) -> u32 {
        if self.cid_ranges.is_empty() {
            return code & 0xFFFF;
        }
        let i = self.cid_ranges.partition_point(|r| r.0 <= code);
        if i == 0 {
            return 0;
        }
        let r = self.cid_ranges[i - 1];
        if code <= r.1 {
            (r.2 + code - r.0) & 0xFFFF
        } else {
            0
        }
    }
}

fn check_range(codes: &[u8], spaces: &[(usize, [u8; 4], [u8; 4])]) -> u8 {
    for (size, lo, hi) in spaces.iter().rev() {
        if *size < codes.len() {
            continue;
        }
        let mut i = 0;
        while i < codes.len() && codes[i] >= lo[i] && codes[i] <= hi[i] {
            i += 1;
        }
        if i == *size {
            return 2;
        }
        if i > 0 {
            return if codes.len() == *size { 2 } else { 1 };
        }
    }
    0
}

fn hex_bytes(w: &[u8]) -> Vec<u8> {
    if w.len() < 2 || w[0] != b'<' {
        return vec![];
    }
    let digits: Vec<u32> = w[1..w.len() - 1].iter().filter_map(|c| hexd(*c)).collect();
    digits.chunks(2).map(|c| (c[0] * 16 + c.get(1).copied().unwrap_or(0)) as u8).collect()
}

// ---------------------------------------------------------------- the font

#[allow(clippy::large_enum_variant)]
pub enum Kind {
    Simple {
        /// Unicode per code from the encoding (0 = none).
        unicode: [u16; 256],
        widths: [i32; 256],
        bbox: Vec<[i32; 4]>,
    },
    Cid {
        cmap: CMap,
        default_width: i32,
        widths: Vec<(u32, u32, i32)>,
        vert: Vec<(u32, u32, [i32; 3])>,
        dw2: [i32; 2],
        /// The embedded program, when it parses (glyph boxes).
        prog: Option<Rc<Program>>,
        cid_to_gid: Option<Vec<u16>>,
        boxes: RefCell<HashMap<u32, [i32; 4]>>,
    },
}

pub struct Font {
    pub kind: Kind,
    pub to_unicode: Option<ToUnicode>,
    pub ascent: i32,
    pub descent: i32,
    /// Identity for display; the same dictionary object gives the same font.
    pub id: usize,
}

pub type FontRef = Rc<Font>;

fn flags_symbolic(f: i64) -> bool {
    f & 4 != 0
}
fn flags_nonsymbolic(f: i64) -> bool {
    f & 32 != 0
}

pub fn standard_font_index(name: &[u8]) -> Option<usize> {
    const ALT: &[(&str, usize)] = &[
        ("Arial", 4), ("Arial,Bold", 5), ("Arial,BoldItalic", 6), ("Arial,Italic", 7), ("Arial-Bold", 5), ("Arial-BoldItalic", 6),
        ("Arial-BoldItalicMT", 6), ("Arial-BoldMT", 5), ("Arial-Italic", 7), ("Arial-ItalicMT", 7), ("ArialBold", 5),
        ("ArialBoldItalic", 6), ("ArialItalic", 7), ("ArialMT", 4), ("ArialMT,Bold", 5), ("ArialMT,BoldItalic", 6),
        ("ArialMT,Italic", 7), ("ArialRoundedMTBold", 5), ("Courier", 0), ("Courier,Bold", 1), ("Courier,BoldItalic", 2),
        ("Courier,Italic", 3), ("Courier-Bold", 1), ("Courier-BoldOblique", 2), ("Courier-Oblique", 3), ("CourierBold", 1),
        ("CourierBoldItalic", 2), ("CourierItalic", 3), ("CourierNew", 0), ("CourierNew,Bold", 1), ("CourierNew,BoldItalic", 2),
        ("CourierNew,Italic", 3), ("CourierNew-Bold", 1), ("CourierNew-BoldItalic", 2), ("CourierNew-Italic", 3),
        ("CourierNewBold", 1), ("CourierNewBoldItalic", 2), ("CourierNewItalic", 3), ("CourierNewPS-BoldItalicMT", 2),
        ("CourierNewPS-BoldMT", 1), ("CourierNewPS-ItalicMT", 3), ("CourierNewPSMT", 0), ("CourierStd", 0), ("CourierStd-Bold", 1),
        ("CourierStd-BoldOblique", 2), ("CourierStd-Oblique", 3), ("Helvetica", 4), ("Helvetica,Bold", 5),
        ("Helvetica,BoldItalic", 6), ("Helvetica,Italic", 7), ("Helvetica-Bold", 5), ("Helvetica-BoldItalic", 6),
        ("Helvetica-BoldOblique", 6), ("Helvetica-Italic", 7), ("Helvetica-Oblique", 7), ("HelveticaBold", 5),
        ("HelveticaBoldItalic", 6), ("HelveticaItalic", 7), ("Symbol", 12), ("SymbolMT", 12), ("Times-Bold", 9),
        ("Times-BoldItalic", 10), ("Times-Italic", 11), ("Times-Roman", 8), ("TimesBold", 9), ("TimesBoldItalic", 10),
        ("TimesItalic", 11), ("TimesNewRoman", 8), ("TimesNewRoman,Bold", 9), ("TimesNewRoman,BoldItalic", 10),
        ("TimesNewRoman,Italic", 11), ("TimesNewRoman-Bold", 9), ("TimesNewRoman-BoldItalic", 10), ("TimesNewRoman-Italic", 11),
        ("TimesNewRomanBold", 9), ("TimesNewRomanBoldItalic", 10), ("TimesNewRomanItalic", 11), ("TimesNewRomanPS", 8),
        ("TimesNewRomanPS-Bold", 9), ("TimesNewRomanPS-BoldItalic", 10), ("TimesNewRomanPS-BoldItalicMT", 10),
        ("TimesNewRomanPS-BoldMT", 9), ("TimesNewRomanPS-Italic", 11), ("TimesNewRomanPS-ItalicMT", 11), ("TimesNewRomanPSMT", 8),
        ("TimesNewRomanPSMT,Bold", 9), ("TimesNewRomanPSMT,BoldItalic", 10), ("TimesNewRomanPSMT,Italic", 11), ("ZapfDingbats", 13),
    ];
    let n = std::str::from_utf8(name).ok()?;
    ALT.iter().find(|(a, _)| a.eq_ignore_ascii_case(n)).map(|(_, i)| *i)
}

fn afm_glyph(font: usize, name: &str) -> Option<(u16, [i16; 4])> {
    let afm = tables::CORE14.get(font)?;
    afm.glyphs.binary_search_by(|g| g.0.cmp(name)).ok().map(|i| (afm.glyphs[i].1, afm.glyphs[i].2))
}

pub struct Loader<'r> {
    pub resolve: &'r dyn Fn(&Obj) -> Obj,
    pub stream: &'r dyn Fn(&Obj) -> Option<Vec<u8>>,
    /// The bounding box of the text a Type 3 glyph procedure draws (glyph space), for a `d0` glyph.
    #[allow(clippy::type_complexity)]
    pub form_box: &'r dyn Fn(&[u8], Option<Rc<Dict>>) -> Option<[f64; 4]>,
}

impl Loader<'_> {
    fn get(&self, d: &Dict, k: &[u8]) -> Obj {
        d.get(k).map(|o| (self.resolve)(o)).unwrap_or(Obj::Null)
    }
    fn int(&self, d: &Dict, k: &[u8], def: i64) -> i64 {
        self.get(d, k).int().unwrap_or(def)
    }
    fn name(&self, d: &Dict, k: &[u8]) -> Vec<u8> {
        match self.get(d, k) {
            Obj::Name(n) => n.to_vec(),
            Obj::Str(s) => s.to_vec(),
            _ => vec![],
        }
    }

    /// PDFium's `CPDF_Font::Create` and `Load`. `None` when the font does not load (the previous font stays).
    pub fn load(&self, fd: &Dict, id: usize) -> Option<Font> {
        let subtype = self.name(fd, b"Subtype");
        let to_unicode = match fd.get(b"ToUnicode") {
            Some(o) => match (self.resolve)(o) {
                s @ Obj::Stream(_) => (self.stream)(&s).map(|d| ToUnicode::parse(&d)),
                _ => None,
            },
            None => None,
        };
        if subtype == b"Type0" {
            return self.load_cid(fd, to_unicode, id);
        }
        Some(self.load_simple(fd, &subtype, to_unicode, id))
    }

    #[allow(clippy::type_complexity)]
    fn descriptor(&self, d: &Dict) -> (Option<Rc<Dict>>, i64, i32, i32, [i32; 4], Option<Obj>) {
        let desc = match self.get(d, b"FontDescriptor") {
            Obj::Dict(x) => Some(x),
            _ => None,
        };
        let mut flags = 32;
        let (mut asc, mut desc_v, mut bbox) = (0i32, 0i32, [0i32; 4]);
        let mut file = None;
        if let Some(dd) = &desc {
            flags = self.int(dd, b"Flags", 32);
            asc = self.int(dd, b"Ascent", 0) as i32;
            desc_v = self.int(dd, b"Descent", 0) as i32;
            if desc_v > 10 {
                desc_v = -desc_v;
            }
            if let Obj::Array(a) = self.get(dd, b"FontBBox") {
                for (i, v) in a.iter().take(4).enumerate() {
                    bbox[i] = (self.resolve)(v).int().unwrap_or(0) as i32;
                }
            }
            for k in [&b"FontFile"[..], b"FontFile2", b"FontFile3"] {
                if let s @ Obj::Stream(_) = self.get(dd, k) {
                    file = Some(s);
                    break;
                }
            }
        }
        (desc, flags, asc, desc_v, bbox, file)
    }

    fn load_simple(&self, fd: &Dict, subtype: &[u8], to_unicode: Option<ToUnicode>, id: usize) -> Font {
        let type3 = subtype == b"Type3";
        let truetype = subtype == b"TrueType";
        let base_font = self.name(fd, b"BaseFont");
        let (desc, mut flags, mut ascent, mut descent, mut fbbox, file) = self.descriptor(fd);
        let embedded = file.is_some();
        let file_data = file.as_ref().and_then(|f| (self.stream)(f));
        let prog = if truetype || subtype == b"Type1" || subtype == b"MMType1" { file_data.as_deref().and_then(Program::parse) } else { None };
        let base14 = if !type3 && !truetype { standard_font_index(&base_font) } else { None };
        let mut base = BaseEnc::Builtin;
        let mut widths = [0xFFFF_i32; 256];
        let mut type3_scale = (1.0f64, 1.0f64);
        let mut type3_m = [0.001f64, 0.0, 0.0, 0.001, 0.0, 0.0];
        if let Some(b14) = base14 {
            flags = if desc.as_ref().is_some_and(|d| d.has(b"Flags")) {
                flags
            } else if b14 >= 12 {
                4
            } else {
                32
            };
            if b14 <= 3 {
                widths = [600; 256];
            }
            base = if b14 == 12 {
                BaseEnc::Symbol
            } else if b14 == 13 {
                BaseEnc::Zapf
            } else if flags_nonsymbolic(flags) {
                BaseEnc::Standard
            } else {
                BaseEnc::Builtin
            };
            let afm = tables::CORE14[b14];
            if ascent == 0 && descent == 0 {
                ascent = afm.ascent as i32;
                descent = afm.descent as i32;
            }
            if fbbox == [0; 4] {
                fbbox = [afm.bbox[0] as i32, afm.bbox[1] as i32, afm.bbox[2] as i32, afm.bbox[3] as i32];
            }
        }
        // Widths (Type 3: scaled by the font matrix).
        let widths_arr = self.get(fd, b"Widths");
        if type3 {
            if let Obj::Array(m) = self.get(fd, b"FontMatrix") {
                let v: Vec<f64> = m.iter().map(|x| (self.resolve)(x).num().unwrap_or(0.0)).collect();
                if v.len() >= 4 {
                    type3_scale = (v[0], v[3]);
                }
                if v.len() >= 6 {
                    type3_m = [v[0], v[1], v[2], v[3], v[4], v[5]];
                }
            } else {
                type3_scale = (0.001, 0.001);
            }
            if let Obj::Array(b) = self.get(fd, b"FontBBox") {
                let v: Vec<f64> = b.iter().take(4).map(|x| (self.resolve)(x).num().unwrap_or(0.0)).collect();
                if v.len() == 4 {
                    fbbox = [
                        (v[0] * type3_scale.0 * 1000.0) as i32,
                        (v[1] * type3_scale.1 * 1000.0) as i32,
                        (v[2] * type3_scale.0 * 1000.0) as i32,
                        (v[3] * type3_scale.1 * 1000.0) as i32,
                    ];
                }
            }
            widths = [0; 256];
            let first = self.int(fd, b"FirstChar", 0);
            if let Obj::Array(w) = &widths_arr {
                if (0..256).contains(&first) {
                    for (i, x) in w.iter().enumerate().take(256 - first as usize) {
                        widths[first as usize + i] = ((self.resolve)(x).num().unwrap_or(0.0) * type3_scale.0 * 1000.0).round() as i32;
                    }
                }
            }
        } else if let Obj::Array(w) = &widths_arr {
            if let Some(dd) = &desc {
                if dd.has(b"MissingWidth") {
                    widths = [self.int(dd, b"MissingWidth", 0) as i32; 256];
                }
            }
            let start = self.int(fd, b"FirstChar", 0);
            let mut end = self.int(fd, b"LastChar", 0);
            if (0..=255).contains(&start) && !w.is_empty() {
                if end == 0 || end >= start + w.len() as i64 {
                    end = start + w.len() as i64 - 1;
                }
                let end = end.min(255);
                let mut i = start;
                while i <= end {
                    widths[i as usize] = (self.resolve)(&w[(i - start) as usize]).int().unwrap_or(0) as i32;
                    i += 1;
                }
            }
        }
        let use_font_width = !type3 && !matches!(widths_arr, Obj::Array(_));
        // The encoding.
        if !type3 && !flags_symbolic(flags) {
            base = BaseEnc::Standard;
        }
        let mut names: Vec<Option<String>> = Vec::new();
        let enc = self.get(fd, b"Encoding");
        match &enc {
            Obj::Null => {
                if base_font == b"Symbol" {
                    base = if truetype { BaseEnc::MsSymbol } else { BaseEnc::Symbol };
                } else if !embedded && base == BaseEnc::Builtin && !type3 {
                    base = BaseEnc::WinAnsi;
                }
            }
            Obj::Name(n) => {
                if !matches!(base, BaseEnc::Symbol | BaseEnc::Zapf) {
                    if flags_symbolic(flags) && base_font == b"Symbol" {
                        if !truetype {
                            base = BaseEnc::Symbol;
                        }
                    } else {
                        let n: &[u8] = if n.as_slice() == b"MacExpertEncoding" { b"WinAnsiEncoding" } else { n };
                        if let Some(b) = predefined(n) {
                            base = b;
                        }
                    }
                }
            }
            Obj::Dict(d) => {
                if !matches!(base, BaseEnc::Symbol | BaseEnc::Zapf) {
                    let mut be = self.name(d, b"BaseEncoding");
                    if truetype && be == b"MacExpertEncoding" {
                        be = b"WinAnsiEncoding".to_vec();
                    }
                    if let Some(b) = predefined(&be) {
                        base = b;
                    }
                }
                if (!embedded || truetype || type3) && base == BaseEnc::Builtin {
                    base = BaseEnc::Standard;
                }
                if let Obj::Array(diffs) = self.get(d, b"Differences") {
                    names = vec![None; 256];
                    let mut code: i64 = 0;
                    for x in diffs.iter() {
                        match (self.resolve)(x) {
                            Obj::Name(n) => {
                                if (0..256).contains(&code) {
                                    names[code as usize] = Some(String::from_utf8_lossy(&n).into_owned());
                                }
                                code += 1;
                            }
                            o => code = o.int().unwrap_or(0),
                        }
                    }
                }
            }
            _ => {}
        }
        // A Type 1 program's own encoding, for a built-in base.
        let mut builtin: Vec<Option<String>> = Vec::new();
        if base == BaseEnc::Builtin && !truetype && !type3 {
            if let Some(names) = prog.as_ref().and_then(|p| p.builtin_names()) {
                builtin = names;
            } else if let Some(data) = &file_data {
                builtin = type1_encoding(data);
            }
        }
        let name_of = |code: usize| -> Option<String> {
            if let Some(Some(n)) = names.get(code) {
                if !n.is_empty() {
                    return Some(n.clone());
                }
            }
            if base != BaseEnc::Builtin {
                return base_name(base, code as u8).map(|s| s.to_string());
            }
            builtin.get(code).cloned().flatten()
        };
        let mut unicode = [0u16; 256];
        let mut bbox = vec![[0i32; 4]; 256];
        if !type3 {
            for (code, slot) in unicode.iter_mut().enumerate() {
                let Some(n) = name_of(code) else {
                    if truetype && embedded && base == BaseEnc::Builtin {
                        *slot = code as u16;
                    }
                    continue;
                };
                *slot = if n == ".notdef" { 0x20 } else { (unicode_from_name(&n) & 0xFFFF) as u16 };
            }
        }
        let symbolic = flags_symbolic(flags);
        // Type 3 glyphs: the width and box of each glyph procedure (PDFium's `CPDF_Type3Char`).
        let mut t3: Vec<Option<(i32, [i32; 4])>> = vec![None; 256];
        if type3 {
            let res = match self.get(fd, b"Resources") {
                Obj::Dict(r) => Some(r),
                _ => None,
            };
            if let Obj::Dict(procs) = self.get(fd, b"CharProcs") {
                for (code, slot) in t3.iter_mut().enumerate() {
                    let Some(n) = name_of(code) else { continue };
                    let Some(o) = procs.get(n.as_bytes()) else { continue };
                    let Some(data) = (self.stream)(&(self.resolve)(o)) else { continue };
                    let text_box = || (self.form_box)(&data, res.clone());
                    *slot = Some(type3_glyph(&data, &type3_m, &text_box));
                }
            }
        }
        let gid_of = |code: usize, name: Option<&str>, u: u16| -> Option<u32> {
            let p = prog.as_ref()?;
            if let Some(n) = name {
                if let Some(g) = p.gid_for_name(n) {
                    return Some(g);
                }
            }
            if symbolic && p.has_cmap(3, 0) {
                for prefix in [0u32, 0xF000, 0xF100, 0xF200] {
                    if let Some(g) = p.cmap(3, 0, prefix + code as u32) {
                        return Some(g);
                    }
                }
            }
            if u != 0 {
                if let Some(g) = p.cmap(3, 1, u as u32) {
                    return Some(g);
                }
            }
            p.cmap(1, 0, code as u32)
        };
        // Widths and boxes: the embedded program, else the core 14 metrics (Helvetica's for a font with no
        // Widths and no metrics of its own, as PDFium measures its substitute face).
        for code in 0..256usize {
            let name = name_of(code);
            let gid = gid_of(code, name.as_deref(), unicode[code]);
            let prog_box = gid.and_then(|g| prog.as_ref().and_then(|p| p.glyph_bbox(g, false)));
            if use_font_width && widths[code] == 0xFFFF {
                if let Some(a) = gid.and_then(|g| prog.as_ref().and_then(|p| p.advance(g))) {
                    widths[code] = a;
                }
            }
            // A font with no Widths and no core 14 metrics takes Helvetica's (PDFium measures its substitute face).
            let metrics_font = base14.or(if !embedded || use_font_width { Some(4) } else { None });
            let afm = metrics_font.and_then(|b| name.as_deref().and_then(|n| afm_glyph(b, n)));
            if use_font_width && widths[code] == 0xFFFF {
                widths[code] = afm.map(|a| a.0 as i32).unwrap_or(0);
            }
            let afm_scale = use_font_width && base14.is_some_and(|b| b <= 3);
            if widths[code] == 0xFFFF {
                widths[code] = 0;
            }
            if type3 && widths[code] == 0 {
                if let Some((w, _)) = t3[code] {
                    widths[code] = w;
                }
            }
            let w = widths[code];
            let space = name.as_deref().is_some_and(|n| n == "space" || n == "nbspace" || n == "nonbreakingspace" || n == ".notdef")
                || (name.is_none() && code == 32)
                || unicode[code] == 0x20
                || unicode[code] == 0xA0;
            bbox[code] = if let Some(b) = prog_box {
                b
            } else if let Some((aw, b)) = afm {
                let (mut l, mut r) = (b[0] as i32, b[2] as i32);
                // PDFium scales the substitute glyph box only for a fixed-pitch core font, whose widths it presets.
                if afm_scale && aw > 0 && !embedded {
                    l = l * w / aw as i32;
                    r = r * w / aw as i32;
                }
                [l, b[1] as i32, r, b[3] as i32]
            } else if space {
                [0, 0, 0, 0]
            } else if type3 {
                t3[code].map(|g| g.1).unwrap_or([0, 0, 0, 0])
            } else {
                [0, if descent != 0 { descent } else { fbbox[1].min(0) }, w.max(0), if ascent != 0 { ascent } else { fbbox[3].max(700) }]
            };
        }
        if ascent == 0 && descent == 0 {
            ascent = if fbbox[3] != 0 { fbbox[3] } else { 750 };
            descent = if fbbox[1] != 0 { fbbox[1] } else { -250 };
        }
        Font { kind: Kind::Simple { unicode, widths, bbox }, to_unicode, ascent, descent, id }
    }

    fn load_cid(&self, fd: &Dict, to_unicode: Option<ToUnicode>, id: usize) -> Option<Font> {
        let Obj::Array(desc_fonts) = self.get(fd, b"DescendantFonts") else { return None };
        if desc_fonts.len() != 1 {
            return None;
        }
        let Obj::Dict(cf) = (self.resolve)(&desc_fonts[0]) else { return None };
        let enc = self.get(fd, b"Encoding");
        let cmap = match &enc {
            Obj::Name(n) => CMap::predefined(n).unwrap_or(CMap { scheme: Scheme::TwoBytes, coding: Coding::Unknown, vertical: n.last() == Some(&b'V'), cid_ranges: vec![] }),
            s @ Obj::Stream(_) => CMap::embedded(&(self.stream)(s).unwrap_or_default()),
            _ => return None,
        };
        let (_desc, _flags, mut ascent, mut descent, fbbox, file) = self.descriptor(&cf);
        let prog = file.as_ref().and_then(|f| (self.stream)(f)).as_deref().and_then(Program::parse).map(Rc::new);
        let cid_to_gid = match self.get(&cf, b"CIDToGIDMap") {
            s @ Obj::Stream(_) => (self.stream)(&s).map(|d| d.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect()),
            _ => None,
        };
        let default_width = self.int(&cf, b"DW", 1000) as i32;
        let mut widths = Vec::new();
        if let Obj::Array(w) = self.get(&cf, b"W") {
            metrics(&w, self.resolve, |a, b, v| widths.push((a, b, v[0])), 1);
        }
        let mut vert = Vec::new();
        let mut dw2 = [880, -1000];
        if cmap.vertical {
            if let Obj::Array(w) = self.get(&cf, b"W2") {
                metrics(&w, self.resolve, |a, b, v| vert.push((a, b, [v[0], v[1], v[2]])), 3);
            }
            if let Obj::Array(d) = self.get(&cf, b"DW2") {
                if d.len() >= 2 {
                    dw2 = [(self.resolve)(&d[0]).int().unwrap_or(880) as i32, (self.resolve)(&d[1]).int().unwrap_or(-1000) as i32];
                }
            }
        }
        if ascent == 0 && descent == 0 {
            ascent = if fbbox[3] != 0 { fbbox[3] } else { 880 };
            descent = if fbbox[1] != 0 { fbbox[1] } else { -120 };
        }
        Some(Font {
            kind: Kind::Cid { cmap, default_width, widths, vert, dw2, prog, cid_to_gid, boxes: RefCell::new(HashMap::new()) },
            to_unicode,
            ascent,
            descent,
            id,
        })
    }
}

fn predefined(n: &[u8]) -> Option<BaseEnc> {
    match n {
        b"WinAnsiEncoding" => Some(BaseEnc::WinAnsi),
        b"MacRomanEncoding" => Some(BaseEnc::MacRoman),
        b"MacExpertEncoding" => Some(BaseEnc::MacExpert),
        b"PDFDocEncoding" => Some(BaseEnc::PdfDoc),
        _ => None,
    }
}

/// A Type 3 glyph procedure's width and box in glyph units: the `d1` operands, or for `d0` the extent of its path
/// points, transformed by the font matrix and rounded as PDFium does.
fn type3_glyph(data: &[u8], m: &[f64; 6], text_box: &dyn Fn() -> Option<[f64; 4]>) -> (i32, [i32; 4]) {
    let mut lx = Lexer::new(data);
    let mut nums: Vec<f64> = Vec::new();
    let mut d = [0f64; 6];
    let mut have_box = false;
    let mut seen_d = false;
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut ops = 0;
    loop {
        ops += 1;
        if ops > 200_000 {
            break;
        }
        match lx.next() {
            Tok::Eof => break,
            Tok::Word(w, true) => {
                if let Some(v) = crate::obj::parse_number(w).num() {
                    nums.push(v);
                }
                if nums.len() > 64 {
                    nums.remove(0);
                }
            }
            Tok::Word(op, _) => {
                match op {
                    b"d0" if !seen_d && nums.len() >= 2 => {
                        d[0] = nums[nums.len() - 2];
                        d[1] = nums[nums.len() - 1];
                        seen_d = true;
                    }
                    b"d1" if !seen_d && nums.len() >= 6 => {
                        d.copy_from_slice(&nums[nums.len() - 6..]);
                        seen_d = true;
                        have_box = true;
                    }
                    b"m" | b"l" | b"c" | b"v" | b"y" => {
                        for p in nums.chunks(2) {
                            if p.len() == 2 {
                                x0 = x0.min(p[0]);
                                x1 = x1.max(p[0]);
                                y0 = y0.min(p[1]);
                                y1 = y1.max(p[1]);
                            }
                        }
                    }
                    b"re" if nums.len() >= 4 => {
                        let n = nums.len();
                        let (x, y, w, h) = (nums[n - 4], nums[n - 3], nums[n - 2], nums[n - 1]);
                        x0 = x0.min(x).min(x + w);
                        x1 = x1.max(x).max(x + w);
                        y0 = y0.min(y).min(y + h);
                        y1 = y1.max(y).max(y + h);
                    }
                    _ => {}
                }
                nums.clear();
            }
            _ => nums.clear(),
        }
        if have_box {
            break;
        }
    }
    let width = (d[0] * 1000.0).round();
    let xunit = if m[1] == 0.0 { m[0].abs() } else if m[0] == 0.0 { m[1].abs() } else { (m[0] * m[0] + m[1] * m[1]).sqrt() };
    let w = (width * xunit + 0.5) as i32;
    let mut r = [(d[2] * 1000.0).round(), (d[3] * 1000.0).round(), (d[4] * 1000.0).round(), (d[5] * 1000.0).round()];
    if r[2] <= r[0] || r[1] >= r[3] {
        // PDFium's `CalcBoundingBox` of the glyph's objects: its path points and the text it draws.
        if let Some(t) = text_box() {
            x0 = x0.min(t[0]);
            y0 = y0.min(t[1]);
            x1 = x1.max(t[2]);
            y1 = y1.max(t[3]);
        }
        r = if x0 <= x1 { [x0 * 1000.0, y0 * 1000.0, x1 * 1000.0, y1 * 1000.0] } else { [0.0; 4] };
    }
    let pts = [(r[0], r[1]), (r[0], r[3]), (r[2], r[1]), (r[2], r[3])];
    let tx = |x: f64, y: f64| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for (x, y) in pts {
        let (a, c) = tx(x, y);
        b[0] = b[0].min(a);
        b[1] = b[1].min(c);
        b[2] = b[2].max(a);
        b[3] = b[3].max(c);
    }
    let rd = |v: f64| v.round().clamp(-1e9, 1e9) as i32;
    (w, [rd(b[0]), rd(b[1]), rd(b[2]), rd(b[3])])
}

/// `/W` (n = 1) and `/W2` (n = 3) arrays: `c [v ...]` and `cfirst clast v...`.
fn metrics(a: &[Obj], resolve: &dyn Fn(&Obj) -> Obj, mut push: impl FnMut(u32, u32, &[i32]), n: usize) {
    let mut i = 0;
    let mut pending: Vec<i64> = Vec::new();
    while i < a.len() {
        let o = resolve(&a[i]);
        match o {
            Obj::Array(list) => {
                if let Some(&first) = pending.first() {
                    let vals: Vec<i32> = list.iter().map(|x| resolve(x).int().unwrap_or(0) as i32).collect();
                    for (k, chunk) in vals.chunks(n).enumerate() {
                        if chunk.len() == n {
                            let c = (first.max(0) as u32).saturating_add(k as u32);
                            push(c, c, chunk);
                        }
                    }
                }
                pending.clear();
            }
            _ => {
                pending.push(o.int().unwrap_or(0));
                if pending.len() == 2 + n {
                    let vals: Vec<i32> = pending[2..].iter().map(|v| *v as i32).collect();
                    push(pending[0].max(0) as u32, pending[1].max(0) as u32, &vals);
                    pending.clear();
                }
            }
        }
        i += 1;
        if i > 2_000_000 {
            break;
        }
    }
}

/// The clear-text `/Encoding` of a Type 1 font program: `StandardEncoding`, or the `dup <code> /<name> put` lines.
fn type1_encoding(data: &[u8]) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = vec![None; 256];
    // PFB segments start with 0x80 0x01; the clear text ends at eexec.
    let body = if data.first() == Some(&0x80) && data.len() > 6 { &data[6..] } else { data };
    let end = crate::doc::find(body, b"eexec", 0).unwrap_or(body.len());
    let text = &body[..end];
    let Some(p) = crate::doc::find(text, b"/Encoding", 0) else { return out };
    let rest = &text[p + 9..];
    let mut lx = Lexer::new(rest);
    if let Tok::Word(b"StandardEncoding", _) = lx.next() {
        for (c, slot) in out.iter_mut().enumerate() {
            *slot = base_name(BaseEnc::Standard, c as u8).map(|s| s.to_string());
        }
        return out;
    }
    lx.pos = 0;
    let mut last: Vec<Vec<u8>> = Vec::new();
    loop {
        match lx.next() {
            Tok::Eof => break,
            Tok::Word(b"put", _) => {
                if last.len() >= 3 && last[last.len() - 3] == b"dup" {
                    let code = crate::obj::atoui(&last[last.len() - 2]) as usize;
                    let name = &last[last.len() - 1];
                    if code < 256 && name.first() == Some(&b'/') {
                        out[code] = Some(String::from_utf8_lossy(&name[1..]).into_owned());
                    }
                }
                last.clear();
            }
            Tok::Word(b"def", _) | Tok::Word(b"readonly", _) => break,
            Tok::Word(w, _) => last.push(w.to_vec()),
            Tok::Name(n) => {
                let mut v = b"/".to_vec();
                v.extend_from_slice(n);
                last.push(v);
            }
            _ => {}
        }
        if last.len() > 8 {
            last.remove(0);
        }
    }
    out
}

impl Font {
    pub fn is_cid(&self) -> bool {
        matches!(self.kind, Kind::Cid { .. })
    }


    pub fn is_vertical(&self) -> bool {
        matches!(&self.kind, Kind::Cid { cmap, .. } if cmap.vertical)
    }

    /// Splits a string operand into char codes.
    pub fn codes(&self, s: &[u8]) -> Vec<u32> {
        match &self.kind {
            Kind::Simple { .. } => s.iter().map(|b| *b as u32).collect(),
            Kind::Cid { cmap, .. } => {
                let mut out = Vec::new();
                let mut off = 0;
                while off < s.len() {
                    out.push(cmap.next_char(s, &mut off));
                }
                out
            }
        }
    }

    /// `CPDF_Font::UnicodeFromCharCode`: empty when the code has no Unicode.
    pub fn unicode(&self, code: u32) -> Vec<u16> {
        if let Some(t) = &self.to_unicode {
            if let Some(v) = t.lookup(code) {
                return v.clone();
            }
        }
        match &self.kind {
            Kind::Simple { unicode, .. } => {
                let u = if code < 256 { unicode[code as usize] } else { 0 };
                if u == 0 {
                    vec![]
                } else {
                    vec![u]
                }
            }
            Kind::Cid { cmap, .. } => match cmap.coding {
                Coding::Ucs2 | Coding::Utf16 => {
                    if code & 0xFFFF == 0 {
                        vec![]
                    } else {
                        vec![code as u16]
                    }
                }
                Coding::Legacy(enc) => {
                    let bytes: Vec<u8> = if code < 256 { vec![code as u8] } else { vec![(code >> 8) as u8, code as u8] };
                    let (s, _, bad) = enc.decode(&bytes);
                    let u: Vec<u16> = s.encode_utf16().collect();
                    if bad || u.len() != 1 || u[0] == 0 {
                        vec![]
                    } else {
                        u
                    }
                }
                _ => vec![],
            },
        }
    }

    /// `CharCodeFromUnicode`: the ToUnicode reverse map, then the encoding (simple), the coding (CID).
    pub fn code_from_unicode(&self, u: u16) -> u32 {
        if let Some(t) = &self.to_unicode {
            let c = t.reverse(u);
            if c != 0 {
                return c;
            }
        }
        match &self.kind {
            Kind::Simple { unicode, .. } => unicode.iter().position(|x| *x == u).map(|p| p as u32).unwrap_or(INVALID_CODE),
            Kind::Cid { cmap, .. } => match cmap.coding {
                Coding::Unknown => 0,
                Coding::Ucs2 | Coding::Utf16 => u as u32,
                Coding::Cid => 0,
                Coding::Legacy(_) => {
                    if u < 0x80 {
                        u as u32
                    } else {
                        0
                    }
                }
            },
        }
    }


    /// Advance width in 1/1000 of the font size (PDFium's `GetCharWidth`).
    pub fn width(&self, code: u32) -> i32 {
        if code == INVALID_CODE {
            return 0;
        }
        match &self.kind {
            Kind::Simple { widths, .. } => widths[if code > 255 { 0 } else { code as usize }],
            Kind::Cid { cmap, default_width, widths, .. } => {
                let cid = cmap.cid(code);
                for (a, b, w) in widths.iter() {
                    if cid >= *a && cid <= *b {
                        return *w;
                    }
                }
                *default_width
            }
        }
    }

    /// Vertical metrics of a CID: (w1y, vx, vy).
    pub fn vert(&self, code: u32) -> (i32, i32, i32) {
        match &self.kind {
            Kind::Cid { cmap, vert, dw2, .. } => {
                let cid = cmap.cid(code);
                for (a, b, v) in vert.iter() {
                    if cid >= *a && cid <= *b {
                        return (v[0], v[1], v[2]);
                    }
                }
                (dw2[1], self.width(code) / 2, dw2[0])
            }
            _ => (-1000, 0, 880),
        }
    }

    /// PDFium's `GetCharWidth` helper in the text page: the width, else the box width.
    pub fn char_width_or_box(&self, code: u32) -> i32 {
        if code == INVALID_CODE {
            return 0;
        }
        let w = self.width(code);
        if w > 0 {
            return w;
        }
        let b = self.char_bbox(code);
        if b[2] >= b[0] && b[3] >= b[1] {
            (b[2] - b[0]).max(0)
        } else {
            0
        }
    }

    /// The glyph box in 1/1000 units (approximate, see the module notes).
    pub fn char_bbox(&self, code: u32) -> [i32; 4] {
        match &self.kind {
            Kind::Simple { bbox, .. } => bbox[if code > 255 { 0 } else { code as usize }],
            Kind::Cid { cmap, prog: Some(p), cid_to_gid, boxes, .. } => {
                if let Some(b) = boxes.borrow().get(&code) {
                    return *b;
                }
                let cid = cmap.cid(code);
                let gid = match cid_to_gid {
                    Some(m) => m.get(cid as usize).copied().unwrap_or(0) as u32,
                    None => p.gid_for_cid(cid),
                };
                let b = p.glyph_bbox(gid, true).unwrap_or([0, 0, 0, 0]);
                if boxes.borrow().len() < 100_000 {
                    boxes.borrow_mut().insert(code, b);
                }
                b
            }
            Kind::Cid { .. } => {
                let u = self.unicode(code);
                if u.len() == 1 && matches!(u[0], 0x20 | 0xA0 | 0x3000 | 0x2000..=0x200B) {
                    return [0, 0, 0, 0];
                }
                [0, self.descent, self.width(code).max(0), self.ascent]
            }
        }
    }
}

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Font#{}", self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(unicode_from_name("A"), 0x41);
        assert_eq!(unicode_from_name("uni0141"), 0x141);
        assert_eq!(unicode_from_name("uni0141.sc"), 0x141);
        assert_eq!(unicode_from_name("u1F600"), 0x1F600);
        assert_eq!(unicode_from_name("a.sc"), 0x61);
        assert_eq!(unicode_from_name("fi"), 0xFB01);
        assert_eq!(unicode_from_name("g123"), 0);
        assert_eq!(unicode_from_name("uni00e9"), 0);
    }

    #[test]
    fn tounicode() {
        let cm = b"2 beginbfchar <0001> <0048> <0002> <d83dde00> endbfchar 1 beginbfrange <0010> <0012> <0061> endbfrange \
                   1 beginbfrange <0020> <0021> [<0041> <00420043>] endbfrange";
        let t = ToUnicode::parse(cm);
        assert_eq!(t.lookup(1).unwrap(), &vec![0x48]);
        assert_eq!(t.lookup(2).unwrap(), &vec![0xd83d, 0xde00]);
        assert_eq!(t.lookup(0x11).unwrap(), &vec![0x62]);
        assert_eq!(t.lookup(0x21).unwrap(), &vec![0x42, 0x43]);
        assert_eq!(t.reverse(0x62), 0x11);
        // A section whose count does not match is dropped whole.
        let t = ToUnicode::parse(b"3 beginbfchar <01> <0041> endbfchar");
        assert!(t.lookup(1).is_none());
    }

    #[test]
    fn cmaps() {
        let m = CMap::predefined(b"90ms-RKSJ-H").unwrap();
        let s = [0x93u8, 0xfa, 0x41];
        let mut off = 0;
        assert_eq!(m.next_char(&s, &mut off), 0x93fa);
        assert_eq!(m.next_char(&s, &mut off), 0x41);
    }
}
