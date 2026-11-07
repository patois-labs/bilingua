//! The five EBU STL character code tables (GSI bytes 12-13, CCT) and the single-character encode and decode.
//!
//! Table 00 is the Latin set of Tech 3264 Appendix 2 (ISO 6937 as printed there): ASCII below 0x80, symbols and
//! ligatures from 0xA0, and the floating diacritics 0xC1..0xCF that come before their base letter. Tables 01 to 04
//! are ISO 8859-5, -6, -7 and -8 above 0xA0. The tables themselves are generated from Localization OS
//! (`tables.rs`).

use crate::tables::*;
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

/// A character code table, 0 to 4 (`00` to `04` in the GSI).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cct(pub u8);

impl Cct {
    /// All five tables in the order LocOS tries them.
    pub const ALL: [Cct; 5] = [Cct(0), Cct(1), Cct(2), Cct(3), Cct(4)];

    /// The two ASCII digits written to the GSI.
    pub fn digits(self) -> [u8; 2] {
        [b'0', b'0' + self.0]
    }

    /// The table named by two GSI bytes (`"00"` to `"04"`), if any.
    pub fn from_digits(a: u8, b: u8) -> Option<Cct> {
        (a == b'0' && (b'0'..=b'4').contains(&b)).then(|| Cct(b - b'0'))
    }

    pub fn is_latin(self) -> bool {
        self.0 == 0
    }
}

/// byte -> character for one table; NUL = the byte has no character.
pub struct Table {
    dec: [char; 256],
}

impl Table {
    fn build(cct: Cct) -> Table {
        let mut dec = ['\0'; 256];
        for b in 0x20u32..0x7F {
            dec[b as usize] = char::from_u32(b).unwrap_or('\0');
        }
        match cct.0 {
            0 => {
                for &(b, c) in T00_HIGH {
                    dec[b as usize] = c;
                }
            }
            n => {
                let high = match n {
                    1 => &HIGH_01,
                    2 => &HIGH_02,
                    3 => &HIGH_03,
                    _ => &HIGH_04,
                };
                for (i, &c) in high.iter().enumerate() {
                    dec[0xA0 + i] = c;
                }
            }
        }
        Table { dec }
    }

    /// The character of a printable byte (0x20..=0x7E and the table's upper half).
    pub fn char_of(&self, b: u8) -> Option<char> {
        match self.dec[b as usize] {
            '\0' => None,
            c => Some(c),
        }
    }

    /// The lowest byte that is this character (`$` is 0x24 not 0xA4, as LocOS).
    fn byte_of(&self, c: char) -> Option<u8> {
        let u = c as u32;
        if (0x20..0x7F).contains(&u) {
            return Some(u as u8);
        }
        if u < 0xA0 {
            return None;
        }
        (0xA0..=0xFFu8).find(|&b| self.dec[b as usize] == c)
    }
}

pub fn table(cct: Cct) -> &'static Table {
    static TABLES: OnceLock<Vec<Table>> = OnceLock::new();
    let all = TABLES.get_or_init(|| Cct::ALL.iter().map(|&c| Table::build(c)).collect());
    &all[cct.0.min(4) as usize]
}

/// The combining character of a floating diacritic byte (table 00 only).
pub fn diacritic(b: u8) -> Option<char> {
    DIACRITICS.iter().find(|(x, _)| *x == b).map(|(_, c)| *c)
}

fn diacritic_byte(c: char) -> Option<u8> {
    DIACRITICS.iter().find(|(_, x)| *x == c).map(|(b, _)| *b)
}

/// The fold of a character the table may lack (thin space, ellipsis, typographic quotes).
pub fn fold(c: char) -> Option<&'static str> {
    NORMALIZE.iter().find(|(x, _)| *x == c).map(|(_, s)| *s)
}

/// One character as table bytes: one byte, or (table 00) a floating diacritic and its base letter.
pub fn encode_char(c: char, cct: Cct) -> Option<([u8; 2], usize)> {
    let t = table(cct);
    if let Some(b) = t.byte_of(c) {
        return Some(([b, 0], 1));
    }
    if cct.is_latin() && !c.is_ascii() {
        let mut it = std::iter::once(c).nfd();
        let (a, m, rest) = (it.next(), it.next(), it.next());
        if let (Some(a), Some(m), None) = (a, m, rest) {
            if let (Some(mb), Some(ab)) = (diacritic_byte(m), t.byte_of(a)) {
                if ab < 0x80 {
                    return Some(([mb, ab], 2));
                }
            }
        }
    }
    None
}

/// A base letter and a floating diacritic that have no single code (`D` + U+0300): the two bytes of the pair.
pub fn encode_pair(base: char, mark: char, cct: Cct) -> Option<[u8; 2]> {
    if !cct.is_latin() {
        return None;
    }
    let (mb, ab) = (diacritic_byte(mark)?, table(cct).byte_of(base)?);
    (ab < 0x80).then_some([mb, ab])
}

/// NFC of a text run (ASCII is returned as it is).
pub fn nfc(s: &str) -> String {
    if s.is_ascii() {
        s.to_string()
    } else {
        s.nfc().collect()
    }
}

/// A base letter and a floating diacritic composed (NFC), one or two characters.
pub fn compose(base: char, mark: char) -> ([char; 2], usize) {
    let mut out = ['\0'; 2];
    let mut n = 0;
    for c in [base, mark].into_iter().nfc() {
        if n < 2 {
            out[n] = c;
            n += 1;
        }
    }
    (out, n)
}
