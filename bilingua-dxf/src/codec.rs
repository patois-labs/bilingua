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

#[cfg(test)]
mod tests {
    use super::Codec;

    /// FNV-1a 64 fingerprints of Python 3.13.9's codecs (the LocOS runtime), made by `tools/codec_fingerprints.py` (LocOS venv):
    /// (codec, strict encode of every code point, strict decode of every one- and two-byte input).
    const PY_FINGERPRINTS: &[(&str, u64, u64)] = &[
        ("cp874", 0xE0D7478D181F92FE, 0x076604FCBF167449),
        ("cp1250", 0x2F0D10CF9F2061D9, 0xF96BDE145EDCBE6E),
        ("cp1251", 0xE27B238B5DC1D576, 0x7FB943DB7BEA1336),
        ("cp1252", 0x771EC181B8238641, 0x9BB0DC520C5D1E19),
        ("cp1253", 0x48CB340856791783, 0x5A6A769F6D53FB51),
        ("cp1254", 0x6237EDE083BA37CB, 0x3F9ABE892957CA7F),
        ("cp1255", 0x8313701B7DAA2B7B, 0x5DDF72461C67E533),
        ("cp1256", 0xF7E7BE416632B52C, 0x4734B05AB0629197),
        ("cp1257", 0xF1C8F1E254EC288F, 0xCE35C0AC43C17165),
        ("cp1258", 0xDE8804FCDA1A44C7, 0xCC22328E5423F23C),
        ("cp437", 0x5DB5BA1A735A50A9, 0x1F93EFB7EA15FBE9),
        ("cp850", 0x425CEAEFF41A3B41, 0xBF70FDEAF17F958F),
        ("cp852", 0xDF0DE303090F2D12, 0xAAC5871145ECE252),
        ("cp855", 0xB328CC816D7D85AA, 0x681AA45CC65D71CD),
        ("cp857", 0x10EF7888B4716769, 0xEE04A5AAB29B56C5),
        ("cp860", 0xD592C7A2594E9404, 0xE922238A38615345),
        ("cp861", 0xFB779E1D8A160A52, 0x211851A9B7BFBDEE),
        ("cp863", 0x17DC6CA434253CEE, 0x8C62DAB52448D149),
        ("cp864", 0xE6ECDFA85F1F22FF, 0x1D1936852A557C39),
        ("cp865", 0x7BF5A657D870AA01, 0x4D404C058011B881),
        ("cp866", 0x45649029592E2EE1, 0xF53A1DF13D6D0E86),
        ("cp869", 0xB86EBEF469CA8572, 0xAC7556E36986E957),
        ("cp932", 0xA2B20B63CE862F82, 0x4903AE740A37A795),
        ("gbk", 0x13FFD912D6511F3D, 0x673E2577FF2E316A),
        ("cp949", 0x670F116063C1DD7B, 0x8F7A0317D35AD44D),
        ("cp950", 0x236806DF5B5A19E5, 0x5847D992226F1358),
    ];

    struct Fnv(u64);
    impl Fnv {
        fn feed(&mut self, b: &[u8]) {
            for &x in b {
                self.0 ^= u64::from(x);
                self.0 = self.0.wrapping_mul(0x100_0000_01B3);
            }
        }
    }

    fn dec_one(f: &mut Fnv, c: Codec, raw: &[u8]) {
        f.feed(raw);
        match c.decode(raw) {
            Ok(s) => {
                f.feed(&[1]);
                f.feed(s.as_bytes());
                f.feed(&[0xFF]);
            }
            Err(p) => {
                f.feed(&[0]);
                f.feed(&u32::try_from(p).unwrap().to_le_bytes());
            }
        }
    }

    #[test]
    fn every_code_page_reads_and_writes_exactly_as_python() {
        let mut bad = Vec::new();
        for &(name, want_enc, want_dec) in PY_FINGERPRINTS {
            let c = Codec::from_name(name).unwrap();
            let mut e = Fnv(0xCBF2_9CE4_8422_2325);
            let mut buf = Vec::new();
            for ch in (0..0x11_0000u32).filter_map(char::from_u32) {
                buf.clear();
                if c.encode_char(ch, &mut buf) {
                    e.feed(&(ch as u32).to_le_bytes());
                    e.feed(&[u8::try_from(buf.len()).unwrap()]);
                    e.feed(&buf);
                }
            }
            let mut d = Fnv(0xCBF2_9CE4_8422_2325);
            for a in 0..=255u8 {
                dec_one(&mut d, c, &[a]);
            }
            for a in 0..=255u8 {
                for b in 0..=255u8 {
                    dec_one(&mut d, c, &[a, b]);
                }
            }
            if e.0 != want_enc {
                bad.push(format!("{name}: encode differs from Python"));
            }
            if d.0 != want_dec {
                bad.push(format!("{name}: decode differs from Python"));
            }
        }
        assert!(bad.is_empty(), "{bad:?}");
    }
}
