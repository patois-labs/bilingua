//! Decoding one caption line: hex words into events (characters and control codes), events into rows.

use crate::tables::*;

/// One 4-hex-digit word exactly as the file spells it.
pub type Word = [u8; 4];

fn hexv(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => b - b'A' + 10,
    }
}

pub fn is_word(w: &[u8]) -> bool {
    w.len() == 4 && w.iter().all(u8::is_ascii_hexdigit)
}

/// Bytes of a word (parity bit still set).
pub fn word_bytes(w: &Word) -> (u8, u8) {
    ((hexv(w[0]) << 4) | hexv(w[1]), (hexv(w[2]) << 4) | hexv(w[3]))
}

fn parity(b: u8) -> u8 {
    if b.count_ones() & 1 == 0 {
        b | 0x80
    } else {
        b
    }
}

/// A new word for two 7-bit bytes, with odd parity, in lower case.
pub fn make_word(b1: u8, b2: u8) -> Word {
    const H: &[u8; 16] = b"0123456789abcdef";
    let (x, y) = (parity(b1), parity(b2));
    [H[(x >> 4) as usize], H[(x & 15) as usize], H[(y >> 4) as usize], H[(y & 15) as usize]]
}

pub fn words_text(ws: &[Word]) -> String {
    let mut s = String::with_capacity(ws.len() * 5);
    for (i, w) in ws.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.extend(w.iter().map(|b| *b as char));
    }
    s
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Name {
    /// A basic character (no control code).
    None,
    Pac,
    Mid,
    Special,
    Ext,
    To,
    Rcl,
    Bs,
    Aon,
    Aof,
    Der,
    Ru2,
    Ru3,
    Ru4,
    Fon,
    Rdc,
    Tr,
    Rtd,
    Edm,
    Cr,
    Enm,
    Eoc,
    Other,
    Xds,
}

const MISC: [Name; 16] = [
    Name::Rcl,
    Name::Bs,
    Name::Aon,
    Name::Aof,
    Name::Der,
    Name::Ru2,
    Name::Ru3,
    Name::Ru4,
    Name::Fon,
    Name::Rdc,
    Name::Tr,
    Name::Rtd,
    Name::Edm,
    Name::Cr,
    Name::Enm,
    Name::Eoc,
];

impl Name {
    /// Codes that make a line too complex to re-encode when they appear after its text started.
    fn complex(self) -> bool {
        matches!(
            self,
            Name::Bs
                | Name::Der
                | Name::Eoc
                | Name::Edm
                | Name::Enm
                | Name::Rcl
                | Name::Rdc
                | Name::Ru2
                | Name::Ru3
                | Name::Ru4
                | Name::Tr
                | Name::Rtd
        )
    }
    /// Codes that may sit between two rows of one caption (row breaks).
    fn row_break(self) -> bool {
        matches!(self, Name::Pac | Name::Cr)
    }
    pub fn label(self) -> &'static str {
        match self {
            Name::Mid => "mid-row code",
            Name::To => "tab offset",
            Name::Xds => "XDS code",
            _ => "control code",
        }
    }
}

/// One decoded event: a character cell or a (possibly doubled) control code.
#[derive(Clone, Debug)]
pub struct Ev {
    pub is_char: bool,
    pub name: Name,
    /// The raw words (control codes and special or extended characters; empty for a basic character).
    pub words: Vec<Word>,
    pub ch: char,
    pub basic: bool,
    /// PAC: row; MID: second byte; TO: columns.
    pub a: u8,
    /// PAC: attribute bits.
    pub b: u8,
    /// Data channel, 1 or 2.
    pub chan: u8,
}

impl Ev {
    fn ctrl(name: Name, words: Vec<Word>, chan: u8) -> Ev {
        Ev { is_char: false, name, words, ch: ' ', basic: false, a: 0, b: 0, chan }
    }
    fn chr(name: Name, words: Vec<Word>, ch: char, basic: bool, chan: u8) -> Ev {
        Ev { is_char: true, name, words, ch, basic, a: 0, b: 0, chan }
    }
    /// Short text for the tag chip of an inline control code.
    pub fn display(&self) -> String {
        match self.name {
            Name::Mid => {
                const COLORS: [&str; 8] = ["white", "green", "blue", "cyan", "red", "yellow", "magenta", "italics"];
                let c = COLORS[((self.a >> 1) & 7) as usize];
                if self.a & 1 == 1 {
                    format!("[{c} underline]")
                } else {
                    format!("[{c}]")
                }
            }
            Name::To => format!("[tab {}]", self.a),
            Name::Xds => "[xds]".to_string(),
            _ => "[code]".to_string(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    PopOn,
    RollUp,
    PaintOn,
}

impl Mode {
    pub fn word(self) -> &'static str {
        match self {
            Mode::PopOn => "popon",
            Mode::RollUp => "rollup",
            Mode::PaintOn => "painton",
        }
    }
    pub fn from_word(s: &str) -> Option<Mode> {
        Some(match s {
            "popon" => Mode::PopOn,
            "rollup" => Mode::RollUp,
            "painton" => Mode::PaintOn,
            _ => return None,
        })
    }
}

/// What carries from one line to the next: the data channel and the caption mode.
#[derive(Clone, Copy, Debug)]
pub struct State {
    pub chan: u8,
    pub mode: Mode,
    pub depth: u8,
}

impl Default for State {
    fn default() -> State {
        State { chan: 1, mode: Mode::PopOn, depth: 2 }
    }
}

impl State {
    pub fn to_text(self) -> String {
        format!("{} {} {}", self.chan, self.mode.word(), self.depth)
    }
    pub fn from_text(s: &str) -> Option<State> {
        let mut it = s.split(' ');
        let chan: u8 = it.next()?.parse().ok()?;
        let mode = Mode::from_word(it.next()?)?;
        let depth: u8 = it.next()?.parse().ok()?;
        if it.next().is_some() || !(1..=2).contains(&chan) || !(2..=4).contains(&depth) {
            return None;
        }
        Some(State { chan, mode, depth })
    }
}

fn classify(b1: u8, b2: u8) -> (Name, u8, u8, Option<char>) {
    let c = b1 & 0x17;
    if b2 >= 0x40 {
        return match pac_row(c, b2 & 0x60) {
            Some(row) => (Name::Pac, row, b2 & 0x1F, None),
            None => (Name::Other, 0, 0, None),
        };
    }
    if c == 0x11 && (0x20..=0x2F).contains(&b2) {
        return (Name::Mid, b2, 0, None);
    }
    if c == 0x11 && (0x30..=0x3F).contains(&b2) {
        return (Name::Special, 0, 0, Some(SPECIAL[(b2 - 0x30) as usize]));
    }
    if c == 0x12 && (0x20..=0x3F).contains(&b2) {
        return (Name::Ext, 0, 0, Some(EXT12[(b2 - 0x20) as usize]));
    }
    if c == 0x13 && (0x20..=0x3F).contains(&b2) {
        return (Name::Ext, 0, 0, Some(EXT13[(b2 - 0x20) as usize]));
    }
    if (c == 0x14 || c == 0x15) && (0x20..=0x2F).contains(&b2) {
        return (MISC[(b2 - 0x20) as usize], 0, 0, None);
    }
    if c == 0x17 && (0x21..=0x23).contains(&b2) {
        return (Name::To, b2 - 0x20, 0, None);
    }
    (Name::Other, 0, 0, None)
}

/// Decodes a line's words. `state` carries the data channel across lines.
fn events(words: &[Word], state: &mut State) -> Vec<Ev> {
    let mut out: Vec<Ev> = Vec::with_capacity(words.len() * 2);
    let mut prev: Option<[u8; 4]> = None; // the previous control word, lower case, for the redundancy rule
    for w in words {
        let (r1, r2) = word_bytes(w);
        let (b1, b2) = (r1 & 0x7F, r2 & 0x7F);
        if (0x10..=0x1F).contains(&b1) {
            let lw = w.map(|c| c.to_ascii_lowercase());
            if prev == Some(lw) {
                if let Some(last) = out.last_mut() {
                    last.words.push(*w); // the redundant second copy
                }
                prev = None;
                continue;
            }
            prev = Some(lw);
            let chan = if b1 & 0x08 != 0 { 2 } else { 1 };
            state.chan = chan;
            let (name, a, b, ch) = classify(b1, b2);
            if let Some(ch) = ch {
                out.push(Ev::chr(name, vec![*w], ch, false, chan));
            } else {
                let mut e = Ev::ctrl(name, vec![*w], chan);
                e.a = a;
                e.b = b;
                out.push(e);
            }
            continue;
        }
        prev = None;
        if b1 < 0x10 && b1 != 0 {
            out.push(Ev::ctrl(Name::Xds, vec![*w], state.chan)); // XDS / unknown
            continue;
        }
        for b in [b1, b2] {
            if b >= 0x20 {
                out.push(Ev::chr(Name::None, Vec::new(), basic_char(b), true, state.chan));
            }
        }
    }
    out
}

pub struct Row {
    /// Controls before the row's text (mode, PAC, tab offset, leading mid-row).
    pub prefix: Vec<Ev>,
    /// Characters, and inline control codes.
    pub cells: Vec<Ev>,
}

/// One caption line, decoded.
pub struct Line {
    pub mode: Mode,
    pub depth: u8,
    pub rows: Vec<Row>,
    /// Codes after the last text (EOC, EDM, ...).
    pub tail: Vec<Ev>,
    /// False when the line is kept verbatim (channel 2, backspace, several captions in one line).
    pub unit: bool,
}

impl Line {
    pub fn new(words: &[Word], state: &mut State) -> Line {
        let (mode0, depth0) = (state.mode, state.depth);
        let evs = events(words, state);
        let mut rows: Vec<Row> = Vec::new();
        let mut unit = true;
        let mut pending: Vec<Ev> = Vec::new();
        for ev in evs {
            if !ev.is_char {
                match ev.name {
                    Name::Ru2 | Name::Ru3 | Name::Ru4 => {
                        state.mode = Mode::RollUp;
                        state.depth = match ev.name {
                            Name::Ru2 => 2,
                            Name::Ru3 => 3,
                            _ => 4,
                        };
                    }
                    Name::Rcl => state.mode = Mode::PopOn,
                    Name::Rdc => state.mode = Mode::PaintOn,
                    _ => {}
                }
                pending.push(ev);
                continue;
            }
            if ev.chan != 1 {
                unit = false;
            }
            if rows.is_empty() {
                rows.push(Row { prefix: std::mem::take(&mut pending), cells: Vec::new() });
            } else if !pending.is_empty() {
                if pending.iter().any(|p| p.name.complex()) {
                    unit = false;
                }
                if pending.iter().any(|p| p.name.row_break()) {
                    rows.push(Row { prefix: std::mem::take(&mut pending), cells: Vec::new() });
                } else if let Some(cur) = rows.last_mut() {
                    cur.cells.append(&mut pending);
                }
            }
            pending.clear();
            if let Some(cur) = rows.last_mut() {
                if ev.name == Name::Ext && cur.cells.last().is_some_and(|l| l.is_char && l.basic) {
                    // an extended character replaces its fallback
                    if let Some(last) = cur.cells.last_mut() {
                        *last = ev;
                    }
                } else {
                    cur.cells.push(ev);
                }
            }
        }
        let (mode, depth) = if rows.is_empty() { (mode0, depth0) } else { (state.mode, state.depth) };
        if rows.iter().any(|r| r.cells.iter().any(|p| matches!(p.name, Name::Bs | Name::Der))) {
            unit = false;
        }
        Line { mode, depth, rows, tail: pending, unit }
    }

    /// The inline control codes of the line in order: token number n is `tokens()[n - 1]`.
    pub fn tokens(&self) -> Vec<&Ev> {
        self.rows.iter().flat_map(|r| r.cells.iter()).filter(|c| !c.is_char).collect()
    }

    /// The caption text: rows joined with a line break, inline codes as tokens.
    pub fn text(&self) -> Vec<Atom> {
        let mut n = 0u32;
        let mut out: Vec<Atom> = Vec::new();
        for (i, r) in self.rows.iter().enumerate() {
            let mut parts: Vec<Atom> = Vec::with_capacity(r.cells.len());
            for c in &r.cells {
                if c.is_char {
                    parts.push(Atom::C(c.ch));
                } else {
                    n += 1;
                    if matches!(c.name, Name::Mid | Name::Other) && !parts.is_empty() && parts.last() != Some(&Atom::C(' ')) {
                        parts.push(Atom::C(' ')); // the code occupies a cell shown as a space
                    }
                    parts.push(Atom::T(n));
                }
            }
            if i > 0 {
                out.push(Atom::C('\n'));
            }
            out.extend_from_slice(strip_atoms(&parts));
        }
        let s = out.iter().position(|a| *a != Atom::C('\n')).unwrap_or(out.len());
        let e = out.iter().rposition(|a| *a != Atom::C('\n')).map_or(s, |p| p + 1);
        out[s..e].to_vec()
    }
}

/// A character or an inline control code (token number, 1-based).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Atom {
    C(char),
    T(u32),
}

pub fn atom_is_space(a: &Atom) -> bool {
    matches!(a, Atom::C(c) if bilingua_model::py_isspace_char(*c))
}

/// Python's `str.strip()` on atoms.
pub fn strip_atoms(a: &[Atom]) -> &[Atom] {
    let s = a.iter().position(|x| !atom_is_space(x)).unwrap_or(a.len());
    let e = a.iter().rposition(|x| !atom_is_space(x)).map_or(s, |p| p + 1);
    &a[s..e]
}
