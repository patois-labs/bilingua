//! The line-level patterns of the LocOS filter (`_BULLET`, `_ENUM`, `_FIELD`, ...), each a `re.match` at the start
//! of a string where `$` means the end of the string. Written by hand to the same backtracking order.

use crate::pyu::*;

/// `[ \t]+|$` at `p`: the end of the match, or None when neither holds.
fn trailer(s: &str, p: usize) -> Option<usize> {
    let rest = &s[p..];
    if rest.is_empty() {
        return Some(p);
    }
    let n = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    (n > 0).then_some(p + n)
}

pub fn bullet(s: &str) -> Option<usize> {
    let c = s.chars().next()?;
    if matches!(c, '-' | '*' | '+' | '\u{2022}' | '\u{2023}' | '\u{2043}') {
        trailer(s, c.len_utf8())
    } else {
        None
    }
}

pub fn line_block(s: &str) -> Option<usize> {
    s.starts_with('|').then(|| trailer(s, 1)).flatten()
}

pub struct EnumM<'a> {
    pub paren: bool,
    pub g2: &'a str,
    pub g3: char,
    pub end: usize,
}

pub fn enum_match(s: &str) -> Option<EnumM<'_>> {
    let (paren, off) = if s.starts_with('(') { (true, 1) } else { (false, 0) };
    let b = &s[off..];
    let mut cands: Vec<usize> = Vec::with_capacity(4);
    let digits = b.find(|c: char| !is_digit(c)).unwrap_or(b.len());
    let nd = b[..digits].chars().count();
    if (1..=9).contains(&nd) {
        cands.push(digits);
    }
    if b.starts_with('#') {
        cands.push(1);
    }
    if b.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        cands.push(1);
    }
    let roman = b.find(|c: char| !"ivxlcdmIVXLCDM".contains(c)).unwrap_or(b.len());
    if (1..=10).contains(&roman) {
        cands.push(roman);
    }
    for e in cands {
        let Some(g3) = b[e..].chars().next().filter(|c| matches!(c, '.' | ')')) else { continue };
        let p = off + e + 1;
        if let Some(end) = trailer(s, p) {
            return Some(EnumM { paren, g2: &b[..e], g3, end });
        }
    }
    None
}

/// `_FIELD.match(s)`.
pub fn field(s: &str) -> bool {
    if !s.starts_with(':') {
        return false;
    }
    let after = &s[1..];
    if matches!(after.chars().next(), Some(':' | ' ')) {
        return false;
    }
    let (mut p, mut reps) = (1usize, 0usize);
    let mut last = ':';
    while p < s.len() && reps < 200 {
        let c = s[p..].chars().next().unwrap_or(':');
        if c == ':' || c == '\r' || c == '\n' {
            break;
        }
        if c == '\\' {
            match s[p + 1..].chars().next() {
                Some(n) if n != '\n' => {
                    p += 1 + n.len_utf8();
                    last = n;
                }
                _ => return false,
            }
        } else {
            p += c.len_utf8();
            last = c;
        }
        reps += 1;
    }
    if reps == 0 || !s[p..].starts_with(':') || last == ' ' {
        return false;
    }
    trailer(s, p + 1).is_some()
}

fn option_group1(s: &str, p: usize) -> Option<bool> {
    let rest = &s[p..];
    let sp = rest.len() - rest.trim_start_matches(' ').len();
    if sp >= 2 && rest[sp..].chars().next().is_some_and(|c| !is_space(c)) {
        return Some(true);
    }
    let t = rest.trim_start_matches([' ', '\t']);
    t.is_empty().then_some(false)
}

fn opt_tail(s: &str, e: usize) -> Option<bool> {
    if matches!(s[e..].chars().next(), Some(' ' | '=')) {
        let q = e + 1;
        let mut lts = vec![q];
        if s[q..].starts_with('<') {
            lts.insert(0, q + 1);
        }
        for q2 in lts {
            let run = s[q2..].find(|c: char| !(is_word(c) || c == '-')).unwrap_or(s.len() - q2);
            let m2 = q2 + run;
            let mut e2 = m2;
            while e2 > q2 {
                let mut gts = vec![e2];
                if s[e2..].starts_with('>') {
                    gts.insert(0, e2 + 1);
                }
                for e3 in gts {
                    if let Some(r) = option_group1(s, e3) {
                        return Some(r);
                    }
                }
                e2 -= prev_char(s, e2).map_or(1, char::len_utf8);
            }
        }
    }
    option_group1(s, e)
}

/// `_OPTION_LIST.match(s)`: `Some(group 1 is not blank)` when it matches.
pub fn option_list(s: &str) -> Option<bool> {
    let try_from = |p1: usize, class: fn(char) -> bool| -> Option<bool> {
        let run = s[p1..].find(|c: char| !class(c)).unwrap_or(s.len() - p1);
        let mut e = p1 + run;
        loop {
            if let Some(r) = opt_tail(s, e) {
                return Some(r);
            }
            if e == p1 {
                return None;
            }
            e -= prev_char(s, e).map_or(1, char::len_utf8);
        }
    };
    let wd = |c: char| is_word(c) || c == '-';
    if s.starts_with('-') {
        let mut starts = vec![1usize];
        if s.starts_with("--") {
            starts.insert(0, 2);
        }
        for p0 in starts {
            if s[p0..].chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
                if let Some(r) = try_from(p0 + 1, wd) {
                    return Some(r);
                }
            }
        }
        None
    } else if s.starts_with('/') && s[1..].chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
        try_from(2, is_word)
    } else {
        None
    }
}

/// `_GRID_TOP.fullmatch(s)`.
pub fn grid_top_full(s: &str) -> bool {
    grid_top(s, true)
}

/// `_GRID_TOP.match(s)`.
pub fn grid_top_prefix(s: &str) -> bool {
    grid_top(s, false)
}

fn grid_top(s: &str, full: bool) -> bool {
    let b = s.as_bytes();
    if b.first() != Some(&b'+') {
        return false;
    }
    let mut p = 1usize;
    let mut groups = 0usize;
    loop {
        let q = p;
        while p < b.len() && (b[p] == b'-' || b[p] == b'=') {
            p += 1;
        }
        if p > q && b.get(p) == Some(&b'+') {
            p += 1;
            groups += 1;
        } else {
            p = q;
            break;
        }
    }
    if groups == 0 {
        return false;
    }
    !full || s[p..].chars().all(|c| c == ' ' || c == '\t')
}

fn ws_tail(s: &str) -> bool {
    s.chars().all(|c| c == ' ' || c == '\t')
}

/// `_SIMPLE_BORDER.fullmatch(s)`: `=+(?:[ \t]+=+)+[ \t]*`.
pub fn simple_border(s: &str) -> bool {
    runs_full(s, b'=', true)
}

/// `_SIMPLE_SPAN.fullmatch(s)`: `-+(?:[ \t]+-+)*[ \t]*`.
pub fn simple_span(s: &str) -> bool {
    runs_full(s, b'-', false)
}

fn runs_full(s: &str, ch: u8, need_two: bool) -> bool {
    let b = s.as_bytes();
    let mut p = 0usize;
    let mut groups = 0usize;
    while p < b.len() && b[p] == ch {
        p += 1;
    }
    if p == 0 {
        return false;
    }
    groups += 1;
    loop {
        let q = p;
        while p < b.len() && (b[p] == b' ' || b[p] == b'\t') {
            p += 1;
        }
        let r = p;
        while p < b.len() && b[p] == ch {
            p += 1;
        }
        if r > q && p > r {
            groups += 1;
        } else {
            p = q;
            break;
        }
    }
    (!need_two || groups >= 2) && ws_tail(&s[p..])
}

/// `_DIRECTIVE.match(rest)`: (name, end).
pub fn directive(rest: &str) -> Option<(&str, usize)> {
    let first = rest.chars().next()?;
    if !first.is_ascii_alphanumeric() {
        return None;
    }
    // class chars after the first: at most 100 plus the final \w
    let mut idx: Vec<(usize, char)> = Vec::new();
    for (i, c) in rest.char_indices().skip(1) {
        if idx.len() > 101 || !(is_word(c) || matches!(c, ':' | '+' | '.' | '-')) {
            break;
        }
        idx.push((i, c));
    }
    // candidate name ends e (byte index just after the name): after the first char, or after any run char
    // that is \w, at most 101 characters past the first
    let mut ends: Vec<usize> = vec![first.len_utf8()];
    for (n, &(i, c)) in idx.iter().enumerate() {
        if n < 101 && is_word(c) {
            ends.push(i + c.len_utf8());
        }
    }
    for &e in ends.iter().rev() {
        if rest[e..].starts_with("::") {
            if let Some(end) = trailer(rest, e + 2) {
                return Some((&rest[..e], end));
            }
        }
    }
    None
}

/// `_FOOTNOTE.match(rest)`: the end.
pub fn footnote(rest: &str) -> Option<usize> {
    if !rest.starts_with('[') {
        return None;
    }
    let close = rest.find(']')?;
    let inner = &rest[1..close];
    let fnc = |c: char| is_word(c) || c == '.' || c == '-';
    let ok = (!inner.is_empty() && inner.chars().all(is_digit))
        || (inner.starts_with('#') && inner[1..].chars().all(fnc))
        || inner == "*"
        || (!inner.is_empty() && inner.chars().all(fnc));
    if !ok {
        return None;
    }
    trailer(rest, close + 1)
}

/// `_OPTION_LINE.match(s)`: `:[\w][\w .-]{0,100}:(?:[ \t]|$)`.
pub fn option_line(s: &str) -> bool {
    if !s.starts_with(':') {
        return false;
    }
    let mut it = s[1..].char_indices();
    let Some((_, c0)) = it.next() else { return false };
    if !is_word(c0) {
        return false;
    }
    let mut p = 1 + c0.len_utf8();
    let mut n = 0usize;
    for c in s[p..].chars() {
        if !(is_word(c) || matches!(c, ' ' | '.' | '-')) {
            break;
        }
        n += 1;
        if n > 100 {
            return false;
        }
        p += c.len_utf8();
    }
    if !s[p..].starts_with(':') {
        return false;
    }
    let t = &s[p + 1..];
    t.is_empty() || t.starts_with([' ', '\t'])
}

/// `_META_FIELD.match(s)` (ignore case): (lower-cased name, end).
pub fn meta_field(s: &str) -> Option<(&'static str, usize)> {
    if !s.starts_with(':') {
        return None;
    }
    let fold = |c: char| match c {
        '\u{17F}' => 's',
        '\u{212A}' => 'k',
        c => c.to_ascii_lowercase(),
    };
    for name in ["description", "keywords"] {
        let mut p = 1usize;
        let mut ok = true;
        let mut it = s[1..].chars();
        for want in name.chars() {
            match it.next() {
                Some(c) if fold(c) == want => p += c.len_utf8(),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if ok && s[p..].starts_with(':') {
            if let Some(end) = trailer(s, p + 1) {
                return Some((name, end));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_patterns() {
        assert_eq!(bullet("- a"), Some(2));
        assert_eq!(bullet("-a"), None);
        assert_eq!(bullet("-"), Some(1));
        let e = enum_match("(iv) x").unwrap();
        assert!(e.paren && e.g2 == "iv" && e.g3 == ')');
        assert!(enum_match("1234567890. x").is_none());
        assert!(field(":Author: Jane") && field(":a:") && !field(":a b :") && !field(":: x"));
        assert_eq!(option_list("-v  verbose"), Some(true));
        assert_eq!(option_list("--long"), Some(false));
        assert_eq!(option_list("--opt=<v>  d"), Some(true));
        assert!(grid_top_full("+---+===+") && !grid_top_full("+---") && grid_top_prefix("+--+ x"));
        assert!(simple_border("===  ===") && !simple_border("===") && simple_span("--- ---") && simple_span("---"));
        assert_eq!(directive("code-block:: python"), Some(("code-block", 13)));
        assert_eq!(directive("py:function:: f()").map(|d| d.0), Some("py:function"));
        assert_eq!(footnote("[1] text"), Some(4));
        assert!(option_line(":class: big") && !option_line(": x:"));
        assert_eq!(meta_field(":Keywords:").map(|m| m.0), Some("keywords"));
    }
}
