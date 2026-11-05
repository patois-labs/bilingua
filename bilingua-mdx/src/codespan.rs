//! Code spans the way two LocOS regexes find them, without back-references:
//!
//! * `(`+)(?:.+?)(?<!`)\1(?!`)` (`_escape_text`, a span stays on one line), and
//! * ``(?P<tick>`+)[\s\S]*?(?<!`)(?P=tick)(?!`)`` (`md_emphasis._PROTECT`, any characters).
//!
//! A closing mark is a maximal run of backticks of exactly the opener's length that starts after a non-backtick.
//! The opener may be any suffix of a run of backticks; the regex tries the longest first and backs off.

use std::collections::BTreeMap;

pub(crate) struct Spans {
    by_len: BTreeMap<usize, Vec<usize>>,
    newlines: Vec<usize>,
}

impl Spans {
    pub fn new(s: &str, single_line: bool) -> Spans {
        let b = s.as_bytes();
        let mut by_len: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut newlines = Vec::new();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'`' => {
                    let st = i;
                    while i < b.len() && b[i] == b'`' {
                        i += 1;
                    }
                    by_len.entry(i - st).or_default().push(st);
                }
                b'\n' => {
                    if single_line {
                        newlines.push(i);
                    }
                    i += 1;
                }
                _ => i += 1,
            }
        }
        Spans { by_len, newlines }
    }

    /// End of the first maximal run of exactly `run` backticks that starts at or after `from` and ends by `end`.
    pub fn close_run(&self, run: usize, from: usize, end: usize) -> Option<usize> {
        let starts = self.by_len.get(&run)?;
        let k = starts.partition_point(|&st| st < from);
        let st = *starts.get(k)?;
        (st + run <= end).then_some(st + run)
    }

    /// End of the span opened at byte `i` (where `s[i]` is a backtick), or None.
    fn open_at(&self, b: &[u8], i: usize, single_line: bool) -> Option<usize> {
        let mut m = 0;
        while b.get(i + m) == Some(&b'`') {
            m += 1;
        }
        for (&n, starts) in self.by_len.range(..=m).rev() {
            let from = i + n;
            let k = starts.partition_point(|&st| st < from);
            let Some(&q) = starts.get(k) else { continue };
            if single_line {
                let k = self.newlines.partition_point(|&p| p < from);
                if self.newlines.get(k).is_some_and(|&p| p < q) {
                    continue;
                }
            }
            return Some(q + n);
        }
        None
    }
}

/// All non-overlapping spans, left to right.
pub(crate) fn find_all(s: &str, single_line: bool) -> Vec<(usize, usize)> {
    let sp = Spans::new(s, single_line);
    if sp.by_len.is_empty() {
        return Vec::new();
    }
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'`' {
            if let Some(e) = sp.open_at(b, i, single_line) {
                out.push((i, e));
                i = e;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The span that starts exactly at byte `i`, if any (for the protect scanner).
pub(crate) fn find_at(s: &str, sp: &Spans, i: usize) -> Option<usize> {
    sp.open_at(s.as_bytes(), i, false)
}

pub(crate) fn spans_any(s: &str) -> Spans {
    Spans::new(s, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_spans() {
        assert_eq!(find_all("a `b` c", true), vec![(2, 5)]);
        assert_eq!(find_all("``a`b``", true), vec![(0, 7)]);
        assert_eq!(find_all("``a`", true), vec![(0, 4)]);
        assert!(find_all("`a\nb`", true).is_empty());
        assert_eq!(find_all("`a\nb`", false), vec![(0, 5)]);
        assert!(find_all("``", true).is_empty());
    }
}
