//! CEA-608 character tables (47 CFR 79.101; CTA-608-E) and the encoder's fallback rules.

/// Special characters: first byte 0x11 (channel 1), second byte 0x30-0x3F. Entry 9 (0x39) is the transparent
/// space, which the LocOS table spells as U+00A0.
pub const SPECIAL: [char; 16] = [
    '\u{ae}', '\u{b0}', '\u{bd}', '\u{bf}', '\u{2122}', '\u{a2}', '\u{a3}', '\u{266a}', '\u{e0}', '\u{a0}', '\u{e8}', '\u{e2}',
    '\u{ea}', '\u{ee}', '\u{f4}', '\u{fb}',
];

/// Extended characters, first byte 0x12, second byte 0x20-0x3F.
pub const EXT12: [char; 32] = [
    '\u{c1}', '\u{c9}', '\u{d3}', '\u{da}', '\u{dc}', '\u{fc}', '\u{2018}', '\u{a1}', '\u{2a}', '\u{27}', '\u{2014}', '\u{a9}',
    '\u{2120}', '\u{2022}', '\u{201c}', '\u{201d}', '\u{c0}', '\u{c2}', '\u{c7}', '\u{c8}', '\u{ca}', '\u{cb}', '\u{eb}', '\u{ce}',
    '\u{cf}', '\u{ef}', '\u{d4}', '\u{d9}', '\u{f9}', '\u{db}', '\u{ab}', '\u{bb}',
];

/// Extended characters, first byte 0x13, second byte 0x20-0x3F.
pub const EXT13: [char; 32] = [
    '\u{c3}', '\u{e3}', '\u{cd}', '\u{cc}', '\u{ec}', '\u{d2}', '\u{f2}', '\u{d5}', '\u{f5}', '\u{7b}', '\u{7d}', '\u{5c}',
    '\u{5e}', '\u{5f}', '\u{a6}', '\u{7e}', '\u{c4}', '\u{e4}', '\u{d6}', '\u{f6}', '\u{df}', '\u{a5}', '\u{a4}', '\u{7c}',
    '\u{c5}', '\u{e5}', '\u{d8}', '\u{f8}', '\u{250c}', '\u{2510}', '\u{2514}', '\u{2518}',
];

/// The basic set: ASCII except these positions.
fn basic_diff(b: u8) -> Option<char> {
    Some(match b {
        0x2A => '\u{e1}',
        0x5C => '\u{e9}',
        0x5E => '\u{ed}',
        0x5F => '\u{f3}',
        0x60 => '\u{fa}',
        0x7B => '\u{e7}',
        0x7C => '\u{f7}',
        0x7D => '\u{d1}',
        0x7E => '\u{f1}',
        0x7F => '\u{2588}',
        _ => return None,
    })
}

/// The character of a basic byte 0x20..=0x7F.
pub fn basic_char(b: u8) -> char {
    basic_diff(b).unwrap_or(b as char)
}

/// The basic byte that shows `c`, if any.
pub fn basic_byte(c: char) -> Option<u8> {
    let u = c as u32;
    if (0x20..0x80).contains(&u) {
        let b = u as u8;
        return if basic_diff(b).is_some() { None } else { Some(b) };
    }
    (0x20u8..0x80).find(|b| basic_diff(*b) == Some(c))
}

/// How a character is written in CEA-608.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Enc {
    Basic(u8),
    Special(u8),
    Ext(u8, u8),
}

/// Basic wins over special, special over extended (the LocOS table is filled in the opposite order).
pub fn encode_char(c: char) -> Option<Enc> {
    if let Some(b) = basic_byte(c) {
        return Some(Enc::Basic(b));
    }
    if let Some(p) = SPECIAL.iter().position(|x| *x == c) {
        return Some(Enc::Special(0x30 + p as u8));
    }
    if let Some(p) = EXT12.iter().position(|x| *x == c) {
        return Some(Enc::Ext(0x12, 0x20 + p as u8));
    }
    EXT13.iter().position(|x| *x == c).map(|p| Enc::Ext(0x13, 0x20 + p as u8))
}

/// Typographic characters with a plain 608 equivalent (applied before encoding).
pub fn normalize_char(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{2019}' | '\u{2bc}' => "'",
        '\u{2013}' | '\u{2012}' | '\u{2212}' | '\u{2010}' | '\u{2011}' => "-",
        '\u{2026}' => "...",
        '\u{2009}' | '\u{202f}' | '\u{2007}' => " ",
        '\u{200b}' | '\u{ad}' => "",
        _ => return None,
    })
}

/// The basic character shown in front of an extended one (older decoders show it; a newer one replaces it).
pub fn fallback(c: char) -> Option<char> {
    Some(match c {
        '\u{2018}' => '\'',
        '\u{2014}' => '-',
        '\u{a9}' => 'c',
        '\u{2120}' => 's',
        '\u{2022}' => '.',
        '\u{201c}' | '\u{201d}' | '\u{ab}' | '\u{bb}' => '"',
        '\u{7b}' => '(',
        '\u{7d}' => ')',
        '\u{5c}' => '/',
        '\u{5e}' => '\'',
        '\u{5f}' => '-',
        '\u{a6}' => '!',
        '\u{7e}' => '-',
        '\u{df}' => 's',
        '\u{a5}' => 'Y',
        '\u{a4}' => 'o',
        '\u{7c}' => '!',
        '\u{2a}' => '.',
        '\u{250c}' | '\u{2510}' | '\u{2514}' | '\u{2518}' => '+',
        _ => return None,
    })
}

/// PAC rows 1..=15 (index row-1): (first byte, second-byte base), channel 1.
pub const PAC_ROWS: [(u8, u8); 15] = [
    (0x11, 0x40),
    (0x11, 0x60),
    (0x12, 0x40),
    (0x12, 0x60),
    (0x15, 0x40),
    (0x15, 0x60),
    (0x16, 0x40),
    (0x16, 0x60),
    (0x17, 0x40),
    (0x17, 0x60),
    (0x10, 0x40),
    (0x13, 0x40),
    (0x13, 0x60),
    (0x14, 0x40),
    (0x14, 0x60),
];

/// The row of a (first byte, base) pair.
pub fn pac_row(c: u8, base: u8) -> Option<u8> {
    PAC_ROWS.iter().position(|p| *p == (c, base)).map(|i| i as u8 + 1)
}
