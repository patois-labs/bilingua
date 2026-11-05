//! Delimiter detection as Localization OS does it: the candidate that splits the first lines most consistently.
//! Python's `max(set(widths), key=widths.count)` depends on the order a CPython set iterates small integers, so
//! the tie-break below reproduces that order exactly.

use crate::rec::{blank, FIELD_LIMIT};

const CANDIDATES: [u8; 4] = [b',', b';', b'\t', b'|'];

/// Python `str.splitlines()`.
pub fn splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let brk = matches!(c, '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}');
        if !brk {
            continue;
        }
        let mut end = i + c.len_utf8();
        if c == '\r' && it.peek().is_some_and(|&(_, n)| n == '\n') {
            it.next();
            end += 1;
        }
        out.push(&s[start..i]);
        start = end;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// Row widths of `lines` read as CSV with `delim` (each line is fed without its line ending). `None` when a field
/// is over the field limit (Python raises `csv.Error` and the candidate is skipped).
fn widths(lines: &[&str], delim: u8) -> Option<Vec<usize>> {
    #[derive(Clone, Copy, PartialEq)]
    enum S {
        Start,
        InField,
        InQuoted,
        QuoteIn,
    }
    let mut out = Vec::new();
    let mut s = S::Start;
    let mut fields = 0usize;
    let mut flen = 0usize; // characters of the current field
    let add = |flen: &mut usize| -> bool {
        *flen += 1;
        *flen <= FIELD_LIMIT
    };
    for line in lines {
        for ch in line.chars() {
            let c = if ch.is_ascii() { ch as u8 } else { 0x80 };
            match s {
                S::Start => {
                    if c == b'"' {
                        s = S::InQuoted;
                    } else if c == delim {
                        fields += 1;
                        flen = 0;
                    } else {
                        if !add(&mut flen) {
                            return None;
                        }
                        s = S::InField;
                    }
                }
                S::InField => {
                    if c == delim {
                        fields += 1;
                        flen = 0;
                        s = S::Start;
                    } else if !add(&mut flen) {
                        return None;
                    }
                }
                S::InQuoted => {
                    if c == b'"' {
                        s = S::QuoteIn;
                    } else if !add(&mut flen) {
                        return None;
                    }
                }
                S::QuoteIn => {
                    if c == b'"' {
                        if !add(&mut flen) {
                            return None;
                        }
                        s = S::InQuoted;
                    } else if c == delim {
                        fields += 1;
                        flen = 0;
                        s = S::Start;
                    } else {
                        if !add(&mut flen) {
                            return None;
                        }
                        s = S::InField;
                    }
                }
            }
        }
        // end of the line
        match s {
            S::InQuoted => {}
            S::Start if fields == 0 && flen == 0 => out.push(0), // an empty line
            _ => {
                out.push(fields + 1);
                fields = 0;
                flen = 0;
                s = S::Start;
            }
        }
    }
    if s == S::InQuoted {
        out.push(fields + 1);
    }
    Some(out)
}

/// A CPython `set` of small non-negative integers, enough to know the order it iterates in.
struct PySet {
    table: Vec<Option<usize>>,
    fill: usize,
}

impl PySet {
    fn new() -> PySet {
        PySet { table: vec![None; 8], fill: 0 }
    }

    fn slot(table: &[Option<usize>], key: usize) -> Option<usize> {
        // the free slot (or the slot holding the key) for `key`
        let mask = table.len() - 1;
        let mut perturb = key;
        let mut i = key & mask;
        loop {
            let probes = if i + 9 <= mask { 9 } else { 0 };
            for j in 0..=probes {
                let k = i + j;
                match table[k] {
                    None => return Some(k),
                    Some(v) if v == key => return None,
                    _ => {}
                }
            }
            perturb >>= 5;
            i = (i.wrapping_mul(5).wrapping_add(1).wrapping_add(perturb)) & mask;
        }
    }

    fn add(&mut self, key: usize) {
        let Some(k) = Self::slot(&self.table, key) else { return };
        self.table[k] = Some(key);
        self.fill += 1;
        let mask = self.table.len() - 1;
        if self.fill * 5 < mask * 3 {
            return;
        }
        let minused = if self.fill > 50_000 { self.fill * 2 } else { self.fill * 4 };
        let mut size = 8;
        while size <= minused {
            size <<= 1;
        }
        let mut next = vec![None; size];
        for v in self.table.iter().flatten() {
            if let Some(k) = Self::slot(&next, *v) {
                next[k] = Some(*v);
            }
        }
        self.table = next;
    }

    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.table.iter().flatten().copied()
    }
}

/// `max(set(widths), key=widths.count)`.
fn most_common(widths: &[usize]) -> usize {
    let mut set = PySet::new();
    for &w in widths {
        set.add(w);
    }
    let mut best = (0usize, 0usize);
    let mut first = true;
    for w in set.iter() {
        let c = widths.iter().filter(|&&x| x == w).count();
        if first || c > best.1 {
            best = (w, c);
            first = false;
        }
    }
    best.0
}

/// The delimiter that splits the first lines most consistently (outside quotes), else `default`.
pub fn sniff_delimiter(text: &str, default: u8) -> u8 {
    let end = text.char_indices().nth(65536).map_or(text.len(), |(i, _)| i);
    let lines: Vec<&str> = splitlines(&text[..end]).into_iter().take(20).filter(|l| !blank(l)).collect();
    if lines.is_empty() {
        return default;
    }
    let (mut best, mut best_score) = (default, -1i64);
    for d in CANDIDATES {
        let Some(w) = widths(&lines, d) else { continue };
        if w.is_empty() || w.iter().copied().max().unwrap_or(0) < 2 {
            continue;
        }
        let common = most_common(&w);
        let score = if common > 1 { w.iter().filter(|&&x| x == common).count() as i64 * 1000 + common.min(999) as i64 } else { 0 };
        if score > best_score {
            best = d;
            best_score = score;
        }
    }
    best
}

/// A strict test for format detection of a file with no extension: at least three lines, one delimiter that gives
/// the same width of two or more on nearly every line.
pub fn looks_tabular(text: &str, only: Option<u8>) -> bool {
    let end = text.char_indices().nth(8192).map_or(text.len(), |(i, _)| i);
    let mut lines: Vec<&str> = splitlines(&text[..end]).into_iter().filter(|l| !blank(l)).collect();
    if end < text.len() {
        lines.pop(); // the last line may be cut
    }
    let lines: Vec<&str> = lines.into_iter().take(40).collect();
    if lines.len() < 3 || text.starts_with(['<', '{']) {
        return false;
    }
    let cands: Vec<u8> = only.map_or(CANDIDATES.to_vec(), |d| vec![d]);
    cands.into_iter().any(|d| {
        let Some(w) = widths(&lines, d) else { return false };
        let common = most_common(&w);
        common >= 2 && w.iter().filter(|&&x| x == common).count() * 10 >= w.len() * 9
    })
}
