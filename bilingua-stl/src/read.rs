//! Reading an EBU STL file: the GSI block, the TTI blocks grouped into subtitles, and the text field of one
//! subtitle split into rows, control codes and characters.

use crate::codec::{self, Cct};
use bilingua_model::{py_isspace_char, Error, LimitError, LimitKind, Limits};

pub const GSI_SIZE: usize = 1024;
pub const TTI_SIZE: usize = 128;
pub const TF_SIZE: usize = 112;
pub const MAX_TTI_BLOCKS: usize = 99_999;
pub const DEFAULT_MNC: usize = 40;

pub fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "stl".into(), msg: msg.into() }
}

/// The text blocks of one subtitle (indices into the block list) and their joined text field.
pub struct Group {
    pub sn: [u8; 2],
    pub comment: bool,
    pub blocks: Vec<usize>,
    pub tf: Vec<u8>,
}

pub struct Parsed {
    pub cct: Cct,
    pub nblocks: usize,
    pub groups: Vec<Group>,
}

pub fn block(raw: &[u8], i: usize) -> &[u8] {
    &raw[GSI_SIZE + i * TTI_SIZE..GSI_SIZE + (i + 1) * TTI_SIZE]
}

pub fn has_gsi(raw: &[u8]) -> bool {
    raw.len() >= GSI_SIZE && matches!(&raw[3..11], b"STL25.01" | b"STL30.01")
}

/// `limits` is `None` on merge: the file passed them when it was extracted.
pub fn parse(raw: &[u8], limits: Option<&Limits>) -> Result<Parsed, Error> {
    if !has_gsi(raw) {
        return Err(perr(
            "Not an EBU STL subtitle file (the GSI block must start with a code page and STL25.01 or STL30.01). 3D-model .stl files are not translatable.",
        ));
    }
    if !(raw.len() - GSI_SIZE).is_multiple_of(TTI_SIZE) {
        return Err(perr("EBU STL file is truncated: the TTI blocks are not a whole number of 128 bytes."));
    }
    let n = (raw.len() - GSI_SIZE) / TTI_SIZE;
    if n > MAX_TTI_BLOCKS {
        return Err(LimitError::new(LimitKind::Units, format!("EBU STL file has more than {MAX_TTI_BLOCKS} TTI blocks; refused.")).into());
    }
    let (c0, c1) = (raw[12], raw[13]);
    let cct = if py_isspace_char(c0 as char) && py_isspace_char(c1 as char) {
        Cct(0)
    } else {
        Cct::from_digits(c0, c1).ok_or_else(|| perr(format!("EBU STL character code table '{}{}' is not one of 00-04; refused.", c0 as char, c1 as char)))?
    };
    let mut groups: Vec<Group> = Vec::new();
    let mut cur: Option<usize> = None;
    for i in 0..n {
        let blk = block(raw, i);
        let (sn, ebn) = ([blk[1], blk[2]], blk[3]);
        if ebn == 0xFE || (0xF0..=0xFD).contains(&ebn) {
            continue; // user data / reserved: never text
        }
        let gi = match cur {
            Some(g) if groups[g].sn == sn => g,
            _ => {
                groups.push(Group { sn, comment: blk[15] == 0x01, blocks: Vec::new(), tf: Vec::new() });
                if let Some(l) = limits {
                    bilingua_model::guard::check_unit_count(groups.len(), l)?;
                }
                groups.len() - 1
            }
        };
        let g = &mut groups[gi];
        g.blocks.push(i);
        let tf = &blk[16..];
        let end = tf.iter().rposition(|b| *b != 0x8F).map_or(0, |p| p + 1);
        g.tf.extend_from_slice(&tf[..end]);
        cur = if ebn == 0xFF { None } else { Some(gi) };
    }
    Ok(Parsed { cct, nblocks: n, groups })
}

/// Maximum characters per row (GSI MNC); 40 when absent or not a positive number. Python `int()` rules for two
/// ASCII bytes: surrounding whitespace and one sign are allowed.
pub fn mnc(raw: &[u8]) -> usize {
    let b = &raw[251..253];
    if !b.is_ascii() {
        return DEFAULT_MNC;
    }
    let ws = |c: u8| matches!(c, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r' | 0x1C..=0x1F);
    let mut s: &[u8] = b;
    while let [f, rest @ ..] = s {
        if ws(*f) {
            s = rest
        } else {
            break;
        }
    }
    while let [rest @ .., l] = s {
        if ws(*l) {
            s = rest
        } else {
            break;
        }
    }
    let (neg, digits) = match s {
        [b'+', d @ ..] => (false, d),
        [b'-', d @ ..] => (true, d),
        d => (false, d),
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return DEFAULT_MNC;
    }
    let v = digits.iter().fold(0usize, |a, d| a * 10 + (d - b'0') as usize);
    if neg || v == 0 {
        DEFAULT_MNC
    } else {
        v
    }
}

/// The text of a subtitle as the unit shows it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Atom {
    /// A character (a row break is `'\n'`).
    C(char),
    /// A control code byte inside a row.
    T(u8),
    /// A code the original does not have (a tag with an unknown id): occupies nothing, is dropped on output.
    U,
}

#[derive(Clone)]
pub enum Cell {
    Ch { s: [char; 2], n: usize, raw: [u8; 2], rawn: usize },
    Ctl(u8),
}

impl Cell {
    pub fn raw(&self) -> &[u8] {
        match self {
            Cell::Ch { raw, rawn, .. } => &raw[..*rawn],
            Cell::Ctl(b) => std::slice::from_ref(b),
        }
    }
    fn edge(&self) -> bool {
        match self {
            Cell::Ctl(_) => true,
            Cell::Ch { s, n, .. } => *n == 1 && s[0] == ' ',
        }
    }
}

fn decode_row(raw: &[u8], cct: Cct) -> Vec<Cell> {
    let t = codec::table(cct);
    let mut cells = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        let mark = if cct.is_latin() { codec::diacritic(b) } else { None };
        if let Some(m) = mark {
            if let Some(&nb) = raw.get(i + 1) {
                if nb < 0x80 {
                    if let Some(base) = t.char_of(nb) {
                        let (s, n) = codec::compose(base, m);
                        cells.push(Cell::Ch { s, n, raw: [b, nb], rawn: 2 });
                        i += 2;
                        continue;
                    }
                }
            }
            cells.push(Cell::Ctl(b));
        } else if let Some(c) = t.char_of(b) {
            cells.push(Cell::Ch { s: [c, '\0'], n: 1, raw: [b, 0], rawn: 1 });
        } else {
            cells.push(Cell::Ctl(b));
        }
        i += 1;
    }
    cells
}

pub struct Row {
    pub prefix: Vec<u8>,
    pub cells: Vec<Cell>,
    pub suffix: Vec<u8>,
}

/// The text field of one subtitle: head bytes, text rows with the bytes between them, tail bytes.
pub struct Sub {
    pub rows: Vec<Row>,
    pub seps: Vec<Vec<u8>>,
    pub head: Vec<u8>,
    pub tail: Vec<u8>,
}

impl Sub {
    pub fn new(tf: &[u8], cct: Cct) -> Sub {
        let mut sub = Sub { rows: Vec::new(), seps: Vec::new(), head: Vec::new(), tail: Vec::new() };
        let mut between: Vec<u8> = Vec::new();
        let mut i = 0;
        loop {
            let start = i;
            while i < tf.len() && tf[i] != 0x8A {
                i += 1;
            }
            let piece = &tf[start..i];
            let cells = decode_row(piece, cct);
            let mut lo = 0;
            while lo < cells.len() && cells[lo].edge() {
                lo += 1;
            }
            let mut hi = cells.len();
            while hi > lo && cells[hi - 1].edge() {
                hi -= 1;
            }
            if lo == hi {
                between.extend_from_slice(piece);
            } else {
                let prefix: Vec<u8> = cells[..lo].iter().flat_map(|c| c.raw().iter().copied()).collect();
                let suffix: Vec<u8> = cells[hi..].iter().flat_map(|c| c.raw().iter().copied()).collect();
                if sub.rows.is_empty() {
                    sub.head = std::mem::take(&mut between);
                } else {
                    sub.seps.push(std::mem::take(&mut between));
                }
                sub.rows.push(Row { prefix, cells: cells[lo..hi].to_vec(), suffix });
            }
            if i >= tf.len() {
                break;
            }
            let s = i;
            while i < tf.len() && tf[i] == 0x8A {
                i += 1;
            }
            between.extend_from_slice(&tf[s..i]);
        }
        sub.tail = between;
        sub
    }

    /// The text atoms (rows joined by `'\n'`; a control code shown as a space gets one) and the control code
    /// bytes in order: code number n is `tokens[n - 1]`.
    pub fn atoms(&self) -> (Vec<Atom>, Vec<u8>) {
        let mut out = Vec::new();
        let mut toks = Vec::new();
        for (ri, row) in self.rows.iter().enumerate() {
            if ri > 0 {
                out.push(Atom::C('\n'));
            }
            let (mut started, mut last_space) = (false, false);
            for cell in &row.cells {
                match cell {
                    Cell::Ch { s, n, .. } => {
                        out.extend(s[..*n].iter().map(|c| Atom::C(*c)));
                        last_space = *n == 1 && s[0] == ' ';
                    }
                    Cell::Ctl(b) => {
                        toks.push(*b);
                        if *b < 0x20 && started && !last_space {
                            out.push(Atom::C(' '));
                        }
                        out.push(Atom::T(*b));
                        last_space = false;
                    }
                }
                started = true;
            }
        }
        (out, toks)
    }

    pub fn double_height(&self, row: usize) -> bool {
        self.rows.get(row).is_some_and(|r| r.prefix.contains(&0x0D) || r.prefix.contains(&0x0F))
    }
}
