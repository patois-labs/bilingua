//! The code pages a DXF file is read and written in, exactly as Python's codecs do it (Localization OS reads and
//! writes DXF with them): `$DWGCODEPAGE` names a Windows or DOS page, and AutoCAD 2007 and later are UTF-8.
//!
//! Single-byte pages come from `sbcs.rs`, the four double-byte pages (cp932, gbk, cp949, cp950) from `cjk.bin`;
//! both are generated from Python (see `tools/`). Decoding is strict: a byte the codec rejects is an error with its
//! position. Encoding says "cannot write" for a character the codec lacks, and the caller then writes `\U+XXXX`.

use crate::sbcs::{PAGES, UNDEF};
use std::sync::OnceLock;

static CJK_BIN: &[u8] = include_bytes!("cjk.bin");
const CJK_NAMES: [&str; 4] = ["cp932", "gbk", "cp949", "cp950"];
const LEADS: usize = 126; // 0x81..=0xFE
const TRAILS: usize = 191; // 0x40..=0xFE

/// `$DWGCODEPAGE` value to Python codec name.
const DWG: &[(&str, &str)] = &[
    ("ANSI_874", "cp874"),
    ("ANSI_932", "cp932"),
    ("ANSI_936", "gbk"),
    ("ANSI_949", "cp949"),
    ("ANSI_950", "cp950"),
    ("ANSI_1250", "cp1250"),
    ("ANSI_1251", "cp1251"),
    ("ANSI_1252", "cp1252"),
    ("ANSI_1253", "cp1253"),
    ("ANSI_1254", "cp1254"),
    ("ANSI_1255", "cp1255"),
    ("ANSI_1256", "cp1256"),
    ("ANSI_1257", "cp1257"),
    ("ANSI_1258", "cp1258"),
    ("DOS437", "cp437"),
    ("DOS850", "cp850"),
    ("DOS852", "cp852"),
    ("DOS855", "cp855"),
    ("DOS857", "cp857"),
    ("DOS860", "cp860"),
    ("DOS861", "cp861"),
    ("DOS863", "cp863"),
    ("DOS864", "cp864"),
    ("DOS865", "cp865"),
    ("DOS866", "cp866"),
    ("DOS869", "cp869"),
    ("DOS932", "cp932"),
    ("UTF8", "utf-8"),
];

struct Cjk {
    singles: [u16; 128],
    pairs: &'static [u8],
    /// character to code, sorted by character
    enc: Vec<(u32, u16)>,
}

fn cjk() -> &'static [Cjk; 4] {
    static T: OnceLock<[Cjk; 4]> = OnceLock::new();
    T.get_or_init(|| {
        let mut off = 0usize;
        let mut v: Vec<Cjk> = Vec::with_capacity(4);
        let u16_at = |o: usize| u16::from_le_bytes([CJK_BIN[o], CJK_BIN[o + 1]]);
        for _ in 0..4 {
            let mut singles = [0u16; 128];
            for (i, s) in singles.iter_mut().enumerate() {
                *s = u16_at(off + 2 * i);
            }
            off += 256;
            let pairs = &CJK_BIN[off..off + 2 * LEADS * TRAILS];
            off += 2 * LEADS * TRAILS;
            let n = u32::from_le_bytes([CJK_BIN[off], CJK_BIN[off + 1], CJK_BIN[off + 2], CJK_BIN[off + 3]]) as usize;
            off += 4;
            let mut map: std::collections::BTreeMap<u32, u16> = std::collections::BTreeMap::new();
            for (i, s) in singles.iter().enumerate() {
                if *s != 0 {
                    map.entry(u32::from(*s)).or_insert(0x80 + i as u16);
                }
            }
            for li in 0..LEADS {
                for ti in 0..TRAILS {
                    let c = u16::from_le_bytes([pairs[2 * (li * TRAILS + ti)], pairs[2 * (li * TRAILS + ti) + 1]]);
                    if c != 0 {
                        map.entry(u32::from(c)).or_insert(((0x81 + li as u16) << 8) | (0x40 + ti as u16));
                    }
                }
            }
            for _ in 0..n {
                let cp = u32::from_le_bytes([CJK_BIN[off], CJK_BIN[off + 1], CJK_BIN[off + 2], CJK_BIN[off + 3]]);
                let code = u16_at(off + 4);
                off += 6;
                if code == 0 {
                    map.remove(&cp);
                } else {
                    map.insert(cp, code);
                }
            }
            v.push(Cjk { singles, pairs, enc: map.into_iter().collect() });
        }
        v.try_into().unwrap_or_else(|_| unreachable!("four tables"))
    })
}

type Reverse = Vec<(u32, u8)>;

fn reverse(i: usize) -> &'static Reverse {
    static R: OnceLock<Vec<Reverse>> = OnceLock::new();
    &R.get_or_init(|| {
        PAGES
            .iter()
            .map(|p| {
                let mut r: Reverse = p.table.iter().enumerate().filter(|(_, u)| **u != UNDEF).map(|(b, u)| (*u, b as u8)).collect();
                for (c, b) in p.extras {
                    r.push((*c, *b));
                }
                // the lowest byte wins for a character several bytes decode to
                r.sort();
                r.dedup_by_key(|e| e.0);
                r
            })
            .collect()
    })[i]
}

/// A code page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Utf8,
    /// Index into `sbcs::PAGES`.
    Sb(usize),
    /// Index into `CJK_NAMES`.
    Cjk(usize),
}

impl Codec {
    /// The codec a `$DWGCODEPAGE` value (upper case) names.
    pub fn from_dwg(value: &str) -> Option<Codec> {
        let (_, py) = DWG.iter().find(|(d, _)| *d == value)?;
        Codec::from_name(py)
    }

    /// The codec with this Python name.
    pub fn from_name(name: &str) -> Option<Codec> {
        if name == "utf-8" {
            return Some(Codec::Utf8);
        }
        if let Some(i) = PAGES.iter().position(|p| p.name == name) {
            return Some(Codec::Sb(i));
        }
        CJK_NAMES.iter().position(|n| *n == name).map(Codec::Cjk)
    }

    /// Python's name for the codec.
    pub fn name(self) -> &'static str {
        match self {
            Codec::Utf8 => "utf-8",
            Codec::Sb(i) => PAGES[i].name,
            Codec::Cjk(i) => CJK_NAMES[i],
        }
    }

    /// Strict decode; on failure the position of the first byte the codec cannot read.
    pub fn decode(self, raw: &[u8]) -> Result<String, usize> {
        match self {
            Codec::Utf8 => std::str::from_utf8(raw).map(str::to_string).map_err(|e| e.valid_up_to()),
            Codec::Sb(p) => {
                let t = &PAGES[p].table;
                let mut s = String::with_capacity(raw.len());
                for (i, &b) in raw.iter().enumerate() {
                    if b < 0x80 && t[usize::from(b)] == u32::from(b) {
                        s.push(b as char);
                        continue;
                    }
                    let u = t[usize::from(b)];
                    if u == UNDEF {
                        return Err(i);
                    }
                    s.push(char::from_u32(u).ok_or(i)?);
                }
                Ok(s)
            }
            Codec::Cjk(k) => {
                let t = &cjk()[k];
                let mut s = String::with_capacity(raw.len());
                let mut i = 0;
                while i < raw.len() {
                    let b = raw[i];
                    if b < 0x80 {
                        s.push(b as char);
                        i += 1;
                        continue;
                    }
                    let single = t.singles[usize::from(b - 0x80)];
                    if single != 0 {
                        s.push(char::from_u32(u32::from(single)).ok_or(i)?);
                        i += 1;
                        continue;
                    }
                    if !(0x81..=0xFE).contains(&b) || i + 1 >= raw.len() {
                        return Err(i);
                    }
                    let tr = raw[i + 1];
                    if !(0x40..=0xFE).contains(&tr) {
                        return Err(i);
                    }
                    let o = 2 * ((usize::from(b) - 0x81) * TRAILS + (usize::from(tr) - 0x40));
                    let c = u16::from_le_bytes([t.pairs[o], t.pairs[o + 1]]);
                    if c == 0 {
                        return Err(i);
                    }
                    s.push(char::from_u32(u32::from(c)).ok_or(i)?);
                    i += 2;
                }
                Ok(s)
            }
        }
    }

    /// Appends the character's bytes; false (nothing appended) when the codec cannot write it.
    pub fn encode_char(self, c: char, out: &mut Vec<u8>) -> bool {
        match self {
            Codec::Utf8 => {
                let mut b = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                true
            }
            Codec::Sb(p) => {
                let u = c as u32;
                if u < 0x80 && PAGES[p].table[u as usize] == u {
                    out.push(u as u8);
                    return true;
                }
                let r = reverse(p);
                match r.binary_search_by_key(&u, |e| e.0) {
                    Ok(i) => {
                        out.push(r[i].1);
                        true
                    }
                    Err(_) => false,
                }
            }
            Codec::Cjk(k) => {
                let u = c as u32;
                if u < 0x80 {
                    out.push(u as u8);
                    return true;
                }
                let enc = &cjk()[k].enc;
                match enc.binary_search_by_key(&u, |e| e.0) {
                    Ok(i) => {
                        let code = enc[i].1;
                        if code >= 0x100 {
                            out.push((code >> 8) as u8);
                        }
                        out.push((code & 0xFF) as u8);
                        true
                    }
                    Err(_) => false,
                }
            }
        }
    }

    /// The whole text, or `None` when a character does not fit.
    pub fn encode(self, s: &str) -> Option<Vec<u8>> {
        if self == Codec::Utf8 {
            return Some(s.as_bytes().to_vec());
        }
        let mut out = Vec::with_capacity(s.len());
        for c in s.chars() {
            if !self.encode_char(c, &mut out) {
                return None;
            }
        }
        Some(out)
    }

    /// True when every character fits.
    pub fn can_write(self, s: &str) -> bool {
        self == Codec::Utf8 || self.encode(s).is_some()
    }
}
