//! Re-encoding a translated caption: 608 character check, row layout, position codes, words.

use crate::line::*;
use crate::tables::*;
use unicode_normalization::UnicodeNormalization;

pub const ROW_CELLS: usize = 32;
pub const POPON_MAX_ROWS: usize = 4;

/// NFC, then the typographic characters with a plain 608 equivalent.
pub fn normalize_608(atoms: &[Atom]) -> Vec<Atom> {
    let mut out = Vec::with_capacity(atoms.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut Vec<Atom>| {
        if run.is_empty() {
            return;
        }
        for c in run.nfc() {
            match normalize_char(c) {
                Some(r) => out.extend(r.chars().map(Atom::C)),
                None => out.push(Atom::C(c)),
            }
        }
        run.clear();
    };
    for a in atoms {
        match a {
            Atom::C(c) => run.push(*c),
            t => {
                flush(&mut run, &mut out);
                out.push(*t);
            }
        }
    }
    flush(&mut run, &mut out);
    out
}

/// The first `cap` characters (not tokens, not line breaks) that CEA-608 cannot represent, each once, in order.
/// Only the first eight are ever shown, so the scan stops there (LocOS collects them all in a list, which is
/// quadratic in the number of distinct characters).
pub fn unsupported_chars(atoms: &[Atom], cap: usize) -> Vec<char> {
    let mut seen: Vec<char> = Vec::new();
    for a in atoms {
        if let Atom::C(c) = a {
            if *c != '\n' && encode_char(*c).is_none() && !seen.contains(c) {
                seen.push(*c);
                if seen.len() >= cap {
                    break;
                }
            }
        }
    }
    seen
}

/// Python's `str.isprintable` for one character, as far as std can tell.
fn printable(c: char) -> bool {
    if c == ' ' {
        return true;
    }
    if c.is_control() || c.is_whitespace() {
        return false;
    }
    let u = c as u32;
    let format = matches!(
        u,
        0xAD | 0x600..=0x605 | 0x61C | 0x6DD | 0x70F | 0x890..=0x891 | 0x8E2 | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E
            | 0x2060..=0x2064 | 0x2066..=0x206F | 0xFEFF | 0xFFF9..=0xFFFB | 0x110BD | 0x110CD | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0xE0001 | 0xE0020..=0xE007F
    );
    let private = matches!(u, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD);
    !(format || private)
}

/// Python's `repr(c)` for one character.
pub fn py_repr(c: char) -> String {
    match c {
        '\'' => "\"'\"".to_string(),
        '\\' => "'\\\\'".to_string(),
        '\n' => "'\\n'".to_string(),
        '\r' => "'\\r'".to_string(),
        '\t' => "'\\t'".to_string(),
        c if printable(c) => format!("'{c}'"),
        c if (c as u32) < 0x100 => format!("'\\x{:02x}'", c as u32),
        c if (c as u32) < 0x10000 => format!("'\\u{:04x}'", c as u32),
        c => format!("'\\U{:08x}'", c as u32),
    }
}

#[derive(Clone, Copy)]
struct Meter {
    n: usize,
    prev_space: bool,
}

impl Meter {
    fn new() -> Meter {
        Meter { n: 0, prev_space: false }
    }
    fn push(&mut self, a: &Atom, toks: &[&Ev]) {
        match a {
            Atom::C(c) => {
                self.n += 1;
                self.prev_space = *c == ' ';
            }
            Atom::T(i) => {
                let ev = toks.get((*i as usize).wrapping_sub(1));
                match ev {
                    Some(e) if e.name == Name::To => {
                        self.n += e.a as usize;
                        self.prev_space = false;
                    }
                    _ => {
                        // MID / OTHER / XDS: one cell, consuming a preceding space
                        if !self.prev_space {
                            self.n += 1;
                        }
                        self.prev_space = false;
                    }
                }
            }
        }
    }
}

fn cells(atoms: &[Atom], toks: &[&Ev]) -> usize {
    let mut m = Meter::new();
    for a in atoms {
        m.push(a, toks);
    }
    m.n
}

fn wrap(words: &[Vec<Atom>], width: usize, toks: &[&Ev]) -> Vec<Vec<Atom>> {
    let mut lines: Vec<Vec<Atom>> = Vec::new();
    let mut cur: Vec<Atom> = Vec::new();
    let mut meter = Meter::new();
    for w in words {
        if cur.is_empty() {
            cur.extend_from_slice(w);
            meter = Meter::new();
            for a in w {
                meter.push(a, toks);
            }
            continue;
        }
        let mut cand = meter;
        cand.push(&Atom::C(' '), toks);
        for a in w {
            cand.push(a, toks);
        }
        if cand.n <= width {
            cur.push(Atom::C(' '));
            cur.extend_from_slice(w);
            meter = cand;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur.extend_from_slice(w);
            meter = Meter::new();
            for a in w {
                meter.push(a, toks);
            }
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Rows for a translation, or None when it cannot fit.
fn layout(text: &[Atom], n_rows: usize, max_rows: usize, toks: &[&Ev]) -> Option<Vec<Vec<Atom>>> {
    let own: Vec<Vec<Atom>> = text
        .split(|a| *a == Atom::C('\n'))
        .map(|r| strip_atoms(r).to_vec())
        .filter(|r| !r.is_empty())
        .collect();
    if !own.is_empty() && own.len() <= max_rows && own.iter().all(|r| cells(r, toks) <= ROW_CELLS) {
        return Some(own);
    }
    let mut joined: Vec<Atom> = Vec::new();
    for (i, r) in own.iter().enumerate() {
        if i > 0 {
            joined.push(Atom::C(' '));
        }
        joined.extend_from_slice(r);
    }
    let words: Vec<Vec<Atom>> = joined.split(atom_is_space).filter(|w| !w.is_empty()).map(|w| w.to_vec()).collect();
    let widths: Vec<usize> = words.iter().map(|w| cells(w, toks)).collect();
    if words.is_empty() || widths.iter().any(|w| *w > ROW_CELLS) {
        return None;
    }
    // cannot fit whatever the breaks: refuse before re-wrapping a huge text once per width
    if widths.iter().sum::<usize>() > ROW_CELLS.saturating_mul(max_rows) {
        return None;
    }
    let target = n_rows.min(max_rows).max(1);
    for width in 1..=ROW_CELLS {
        let rows = wrap(&words, width, toks);
        if rows.len() <= target && rows.iter().all(|r| cells(r, toks) <= ROW_CELLS) {
            return Some(rows);
        }
    }
    let rows = wrap(&words, ROW_CELLS, toks);
    (rows.len() <= max_rows).then_some(rows)
}

struct RowPos<'a> {
    head: &'a [Ev],
    pos: &'a [Ev],
    rnum: Option<u8>,
    attr: u8,
    col: usize,
    lead: usize,
}

fn row_pos(row: &Row) -> RowPos<'_> {
    let lead = row.cells.iter().take_while(|c| c.is_char && c.ch == ' ').count();
    match row.prefix.iter().rposition(|p| p.name == Name::Pac) {
        None => {
            let cr = row.prefix.iter().rposition(|p| p.name == Name::Cr).map_or(0, |i| i + 1);
            RowPos { head: &row.prefix[..cr], pos: &row.prefix[cr..], rnum: None, attr: 0, col: 0, lead }
        }
        Some(idx) => {
            let (head, pos) = row.prefix.split_at(idx);
            let (rnum, attr) = (pos[0].a, pos[0].b);
            let mut col = if attr & 0x10 != 0 { (((attr >> 1) & 0x07) as usize) * 4 } else { 0 };
            col += pos.iter().filter(|p| p.name == Name::To).map(|p| p.a as usize).sum::<usize>();
            RowPos { head, pos, rnum: Some(rnum), attr, col, lead }
        }
    }
}

/// New position words: PAC (+ tab offset) for row `rnum` at column `col`, and the number of leading spaces still
/// needed (a colour PAC can only start at column 0).
fn pac_words(rnum: u8, mut attr: u8, col: usize, rest: &[Ev]) -> (Vec<Word>, usize) {
    let (to, pad);
    if col == 0 {
        to = 0;
        pad = 0;
    } else if attr & 0x10 != 0 || (attr & 0x0E) == 0 {
        attr = 0x10 | (((col / 4) as u8) << 1) | (attr & 0x01);
        to = col % 4;
        pad = 0;
    } else {
        to = col.min(3);
        pad = col - to;
    }
    let (b1, base) = PAC_ROWS[(rnum.clamp(1, 15) - 1) as usize];
    let w = make_word(b1, base | attr);
    let mut words = vec![w, w];
    if to > 0 {
        let t = make_word(0x17, 0x20 + to as u8);
        words.push(t);
        words.push(t);
    }
    for p in rest {
        if p.name != Name::Pac && p.name != Name::To {
            words.extend_from_slice(&p.words);
        }
    }
    (words, pad)
}

/// Words for a row's atoms (characters packed two per word, codes doubled).
fn encode_atoms(atoms: &[Atom], toks: &[&Ev]) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    let mut half: Vec<u8> = Vec::new();

    fn flush(words: &mut Vec<Word>, half: &mut Vec<u8>) {
        if !half.is_empty() {
            let b2 = if half.len() == 1 { 0 } else { half[1] };
            words.push(make_word(half[0], b2));
            half.clear();
        }
    }
    fn basic(words: &mut Vec<Word>, half: &mut Vec<u8>, b: u8) {
        half.push(b);
        if half.len() == 2 {
            flush(words, half);
        }
    }

    for (i, a) in atoms.iter().enumerate() {
        match a {
            Atom::T(n) => {
                let Some(ev) = toks.get((*n as usize).wrapping_sub(1)) else { continue };
                if matches!(ev.name, Name::Mid | Name::Other) && i > 0 && atoms[i - 1] == Atom::C(' ') {
                    // the code takes the space's cell: drop that space byte
                    if !half.is_empty() {
                        half.pop();
                    } else if let Some(w) = words.last_mut() {
                        if w[2] == b'2' && w[3] == b'0' {
                            w[2] = b'8';
                            w[3] = b'0';
                        }
                    }
                }
                flush(&mut words, &mut half);
                words.extend_from_slice(&ev.words);
            }
            Atom::C(c) => match encode_char(*c) {
                Some(Enc::Basic(b)) => basic(&mut words, &mut half, b),
                Some(Enc::Special(b2)) => {
                    flush(&mut words, &mut half);
                    let w = make_word(0x11, b2);
                    words.push(w);
                    words.push(w);
                }
                Some(Enc::Ext(b1, b2)) => {
                    let fb = fallback(*c).or_else(|| c.to_string().nfd().next());
                    let byte = fb.and_then(basic_byte).unwrap_or(0x20);
                    basic(&mut words, &mut half, byte);
                    flush(&mut words, &mut half);
                    let w = make_word(b1, b2);
                    words.push(w);
                    words.push(w);
                }
                None => {}
            },
        }
    }
    flush(&mut words, &mut half);
    words
}

/// The words of a translated caption line, or the reason it cannot be written.
pub fn encode_line(line: &Line, text: &[Atom], locos_glue: bool) -> Result<Vec<Word>, String> {
    let toks = line.tokens();
    // a token the line does not have is dropped, as LocOS does
    let kept: Vec<Atom> = text
        .iter()
        .copied()
        .filter(|a| !matches!(a, Atom::T(n) if *n == 0 || *n as usize > toks.len()))
        .collect();
    let text = normalize_608(&kept);
    let bad = unsupported_chars(&text, 8);
    if !bad.is_empty() {
        let shown: Vec<String> = bad.iter().map(|c| py_repr(*c)).collect();
        return Err(format!("characters CEA-608 cannot show: {}", shown.join(" ")));
    }
    let n = line.rows.len();
    let max_rows = n.max(if line.mode == Mode::RollUp { line.depth as usize } else { POPON_MAX_ROWS });
    let Some(rows) = layout(&text, n, max_rows, &toks) else {
        return Err(format!("does not fit in {max_rows} rows of {ROW_CELLS} characters"));
    };
    let m = rows.len();
    let src: Vec<RowPos> = line.rows.iter().map(row_pos).collect();
    if src.is_empty() {
        return Err("has no rows".to_string());
    }
    let bottom = src[src.len() - 1].rnum;
    let cr_words: Vec<Word> = src
        .iter()
        .flat_map(|s| s.head.iter())
        .find(|e| e.name == Name::Cr)
        .map(|e| e.words.clone())
        .unwrap_or_else(|| {
            let w = make_word(0x14, 0x2D);
            vec![w, w]
        });
    let mut out: Vec<Word> = Vec::new();
    for (i, row_text) in rows.iter().enumerate() {
        let mut atoms = row_text.clone();
        let width = cells(&atoms, &toks);
        let j = i as isize - (m as isize - n as isize);
        let s = if j >= 0 { &src[j as usize] } else { &src[0] };
        let mut words: Vec<Word> = Vec::new();
        if i == 0 {
            words.extend(src[0].head.iter().flat_map(|e| e.words.iter().copied()));
        } else if j < 1 {
            if line.mode == Mode::RollUp {
                words.extend_from_slice(&cr_words);
            }
        } else {
            words.extend(s.head.iter().flat_map(|e| e.words.iter().copied()));
        }
        match s.rnum {
            None => {
                // (the locos profile keeps LocOS's glued rows, byte for byte)
                // a row without a PAC has nothing that starts a new row: add a carriage return, or the
                // rows of a re-wrapped caption would run together ("orWindows")
                let has_break = if j >= 1 { s.head.iter().any(|e| e.name == Name::Cr) } else { line.mode == Mode::RollUp };
                if i > 0 && !locos_glue && !has_break {
                    words.extend_from_slice(&cr_words);
                }
                // the first row's mode and style codes are not repeated on an added row (a second mode code
                // makes the row read back as another caption)
                if i == 0 || j >= 1 || locos_glue {
                    words.extend(s.pos.iter().flat_map(|e| e.words.iter().copied()));
                }
            }
            Some(rnum) => {
                let mut new_r = rnum as usize;
                if line.mode != Mode::RollUp && m != n {
                    if let Some(b) = bottom {
                        new_r = (1usize.max((b as usize + 1).saturating_sub(m)) + i).min(15);
                    }
                }
                let new_c = (s.col + s.lead).min(ROW_CELLS - width.min(ROW_CELLS));
                if new_r == rnum as usize && new_c == s.col && s.lead == 0 && j >= 0 {
                    words.extend(s.pos.iter().flat_map(|e| e.words.iter().copied()));
                } else {
                    let (pw, pad) = pac_words(new_r as u8, s.attr, new_c, &s.pos[1..]);
                    words.extend(pw);
                    if pad > 0 {
                        let mut padded = vec![Atom::C(' '); pad];
                        padded.append(&mut atoms);
                        atoms = padded;
                    }
                }
            }
        }
        out.extend(words);
        out.extend(encode_atoms(&atoms, &toks));
    }
    for e in &line.tail {
        out.extend_from_slice(&e.words);
    }
    Ok(out)
}
