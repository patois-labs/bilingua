//! Code spans, found two ways:
//!
//! * `Spans` / `find_at`: the way the LocOS regex ``(?P<tick>`+)[\s\S]*?(?<!`)(?P=tick)(?!`)``
//!   (`md_emphasis._PROTECT`, any characters) finds them, without back-references. A closing mark is a maximal run
//!   of backticks of exactly the opener's length that starts after a non-backtick. The opener may be any suffix of
//!   a run of backticks; the regex tries the longest first and backs off.
//! * `commonmark` / `text_spans`: the CommonMark way, for the spans a translation's escape leaves as code (LocOS
//!   `_code_spans`, tracker #790).

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

/// Code spans in a translation, the CommonMark way (LocOS tracker #790; the LocOS `_escape_text` regex read
/// ``\`{x}` `` and ``w ``<z` `` as spans and left the `{` / `<` after them live):
///
/// * an opener is a whole backtick run, never a suffix of a longer run;
/// * a backtick after an odd run of backslashes is a literal, so that run opens one shorter;
/// * the closer is the next whole run of the same length on the same line;
/// * `row`: in a table row an unescaped `|` ends the cell first, so no span crosses one.
pub(crate) fn commonmark(s: &str, row: bool) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'`' {
            let st = i;
            while i < b.len() && b[i] == b'`' {
                i += 1;
            }
            runs.push((st, i));
        } else {
            i += 1;
        }
    }
    let mut spans = Vec::new();
    let mut j = 0;
    while j < runs.len() {
        let (mut a, e) = runs[j];
        let bs = b[..a].iter().rev().take_while(|&&c| c == b'\\').count();
        if bs % 2 == 1 {
            a += 1;
        }
        let n = e - a;
        let mut close = None;
        if n > 0 {
            for (c, &(ca, cb)) in runs.iter().enumerate().skip(j + 1) {
                if gap_ends_span(&b[e..ca], row) {
                    break;
                }
                if cb - ca == n {
                    close = Some(c);
                    break;
                }
            }
        }
        match close {
            Some(c) => {
                spans.push((a, runs[c].1));
                j = c + 1;
            }
            None => j += 1,
        }
    }
    spans
}

/// A line break between an opener and a closer ends the span; in a table row so does an unescaped `|`.
fn gap_ends_span(gap: &[u8], row: bool) -> bool {
    gap.iter().enumerate().any(|(k, &c)| c == b'\n' || c == b'\r' || (row && c == b'|' && (k == 0 || gap[k - 1] != b'\\')))
}

/// The spans `escape_text` keeps as code: in a table row only those both a cell-first reader and a whole-line reader
/// see (fail closed).
pub(crate) fn text_spans(s: &str, row: bool) -> Vec<(usize, usize)> {
    let all = commonmark(s, false);
    if !row {
        return all;
    }
    let cell: std::collections::HashSet<(usize, usize)> = commonmark(s, true).into_iter().collect();
    all.into_iter().filter(|sp| cell.contains(sp)).collect()
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
        assert_eq!(commonmark("a `b` c", false), vec![(2, 5)]);
        assert_eq!(commonmark("``a`b``", false), vec![(0, 7)]);
        // a suffix of a longer run never opens a span (#790)
        assert!(commonmark("``a`", false).is_empty());
        assert!(commonmark("w ``<z`", false).is_empty());
        assert!(commonmark("`a\nb`", false).is_empty());
        assert!(commonmark("`a\rb`", false).is_empty());
        assert!(commonmark("``", false).is_empty());
        // a backslash-escaped backtick is a literal: the run opens one shorter
        assert!(commonmark("\\`{x}`", false).is_empty());
        assert_eq!(commonmark("\\``{x}`", false), vec![(2, 7)]);
        assert_eq!(commonmark("\\\\`{x}`", false), vec![(2, 7)]);
        // the closer is a whole run of the same length
        assert_eq!(commonmark("`a``b`", false), vec![(0, 6)]);
    }

    #[test]
    fn row_spans_do_not_cross_a_pipe() {
        assert_eq!(commonmark("| `a | b` |", false), vec![(2, 9)]);
        assert!(commonmark("| `a | b` |", true).is_empty());
        assert!(text_spans("| `a | b` |", true).is_empty());
        assert_eq!(commonmark("| `a \\| b` |", true), vec![(2, 10)]);
        assert_eq!(text_spans("| `a` | `b` |", true), vec![(2, 5), (8, 11)]);
    }
}
