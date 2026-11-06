//! Character classes the way Python's `re` and `str` see them, and the line-level patterns of the AsciiDoc grammar.
//!
//! The LocOS filter is a set of Python regular expressions (with look-behind, look-ahead and back references). The
//! `regex` crate has none of those, so every pattern is a small hand-written scanner here; the comments say which
//! pattern each one stands for.

use regex::Regex;
use std::sync::OnceLock;

/// Python `str.isspace` (and `\s`): Unicode white space plus the four information separators `\x1c`..`\x1f`.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

pub fn is_nl(c: char) -> bool {
    c == '\r' || c == '\n'
}

/// Python `str.strip()`.
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// Python `str.rstrip()`.
pub fn rstrip(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

fn cat_re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).expect("category pattern"))
}

fn matches_class(cell: &'static OnceLock<Regex>, pat: &str, c: char) -> bool {
    let mut b = [0u8; 4];
    cat_re(cell, pat).is_match(c.encode_utf8(&mut b))
}

/// Python `str.isalpha` for one character (Unicode category L*).
pub fn is_letter(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphabetic();
    }
    static R: OnceLock<Regex> = OnceLock::new();
    c.is_alphabetic() && matches_class(&R, r"^\p{L}$", c)
}

/// Python `str.isalnum` for one character (letters and numbers).
pub fn is_alnum(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric();
    }
    static R: OnceLock<Regex> = OnceLock::new();
    c.is_alphanumeric() && matches_class(&R, r"^[\p{L}\p{N}]$", c)
}

/// Python `\w` (alphanumeric or underscore).
pub fn is_word(c: char) -> bool {
    c == '_' || is_alnum(c)
}

/// Ruby's `\p{Word}` as the filter's `_wordish` uses it: letters, marks, digits and connector punctuation.
pub fn is_wordish(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '_';
    }
    static R: OnceLock<Regex> = OnceLock::new();
    matches_class(&R, r"^[\p{L}\p{N}\p{M}\p{Pc}]$", c)
}

pub fn any_letter(s: &str) -> bool {
    s.chars().any(is_letter)
}

fn is_ht(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

/// Number of bytes of `[ \t]` at the start of `s`.
pub fn ht_run(s: &str) -> usize {
    s.bytes().take_while(|b| is_ht(*b)).count()
}

/// A Python `str.splitlines`-free line scan: yields `(start, end)` of the content of every line; the terminator is
/// `\r\n`, `\r` or `\n`. The last line has none; an empty text has no lines and a final terminator adds no line.
pub fn lines(text: &str) -> Vec<(usize, usize)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < b.len() {
        let mut e = pos;
        while e < b.len() && b[e] != b'\r' && b[e] != b'\n' {
            e += 1;
        }
        bilingua_model::work::tick(1 + (e - pos) as u64);
        out.push((pos, e));
        if e >= b.len() {
            break;
        }
        pos = e + if b[e] == b'\r' && b.get(e + 1) == Some(&b'\n') { 2 } else { 1 };
    }
    out
}

/// `\r\n`, `\r` or `\n`: the first line ending in the text, else `\n`.
pub fn first_newline(text: &str) -> &'static str {
    let b = text.as_bytes();
    match b.iter().position(|c| *c == b'\r' || *c == b'\n') {
        Some(i) if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') => "\r\n",
        Some(i) if b[i] == b'\r' => "\r",
        _ => "\n",
    }
}

// --------------------------------------------------------------------------- line patterns

/// `_PREPROC.match`: `\\?(include|ifdef|ifndef|ifeval|endif)::` at the start of the line.
pub fn is_preproc(body: &str) -> bool {
    let b = body.strip_prefix('\\').unwrap_or(body);
    ["include::", "ifdef::", "ifndef::", "ifeval::", "endif::"].iter().any(|p| b.starts_with(p))
}

/// `_VERBATIM_DELIM.fullmatch`: four or more of one of `-` `.` `+` `/`.
pub fn is_verbatim_delim(rs: &str) -> bool {
    let b = rs.as_bytes();
    b.len() >= 4 && matches!(b[0], b'-' | b'.' | b'+' | b'/') && b.iter().all(|c| *c == b[0])
}

/// `_FENCE_OPEN.fullmatch`: three backticks and an optional language.
pub fn is_fence_open(rs: &str) -> bool {
    match rs.strip_prefix("```") {
        Some(rest) => rest.chars().all(|c| is_word(c) || matches!(c, '+' | '#' | '.' | '-')),
        None => false,
    }
}

/// `_COMPOUND_DELIM.fullmatch`: four or more of `=` `*` `_`, or exactly `--`.
pub fn is_compound_delim(rs: &str) -> bool {
    let b = rs.as_bytes();
    rs == "--" || (b.len() >= 4 && matches!(b[0], b'=' | b'*' | b'_') && b.iter().all(|c| *c == b[0]))
}

/// `_TABLE_DELIM.fullmatch`: the separator character of a table fence (`|===`, `!===`, `,===`, `:===`).
pub fn table_delim(rs: &str) -> Option<char> {
    let b = rs.as_bytes();
    if b.len() >= 4 && matches!(b[0], b'|' | b'!' | b',' | b':') && b[1..].iter().all(|c| *c == b'=') {
        Some(b[0] as char)
    } else {
        None
    }
}

/// `_ATTR_ENTRY.match`: `:name:` or `:!name:` or `:name!:`, then a space, a tab or the end.
pub fn is_attr_entry(body: &str) -> bool {
    let b = body.as_bytes();
    if b.first() != Some(&b':') {
        return false;
    }
    let mut i = 1;
    if b.get(i) == Some(&b'!') {
        i += 1;
    }
    match b.get(i) {
        Some(c) if c.is_ascii_alphanumeric() || *c == b'_' => i += 1,
        _ => return false,
    }
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') {
        i += 1;
    }
    if b.get(i) == Some(&b'!') {
        i += 1;
    }
    if b.get(i) != Some(&b':') {
        return false;
    }
    i += 1;
    i >= b.len() || is_ht(b[i])
}

/// `_BLOCK_ATTR.fullmatch`: `[...]`.
pub fn is_block_attr(rs: &str) -> bool {
    rs.len() >= 2 && rs.starts_with('[') && rs.ends_with(']')
}

/// `_BLOCK_MACRO.fullmatch`: `name::target[...]`.
pub fn is_block_macro(rs: &str) -> bool {
    let b = rs.as_bytes();
    if b.is_empty() || !b[0].is_ascii_lowercase() {
        return false;
    }
    let mut chars = rs.char_indices().skip(1);
    let mut i = rs.len();
    for (k, c) in chars.by_ref() {
        if !(is_word(c) || c == '-') {
            i = k;
            break;
        }
    }
    if !rs[i..].starts_with("::") {
        return false;
    }
    let rest = &rs[i + 2..];
    let tgt = rest.find(|c: char| is_space(c) || c == '[').unwrap_or(rest.len());
    match rest[tgt..].strip_prefix('[') {
        Some(r) => r.ends_with(']'),
        None => false,
    }
}

/// `_BREAK.fullmatch`: `'''`, `<<<` or three dashes, asterisks or underscores (with spaces between).
pub fn is_break(rs: &str) -> bool {
    let b = rs.as_bytes();
    if b.len() >= 3 && (b.iter().all(|c| *c == b'\'') || b.iter().all(|c| *c == b'<')) {
        return true;
    }
    for m in [b'-', b'*', b'_'] {
        let mut i = 0;
        let mut ok = true;
        for _ in 0..3 {
            if b.get(i) != Some(&m) {
                ok = false;
                break;
            }
            i += 1;
            while i < b.len() && is_ht(b[i]) {
                i += 1;
            }
        }
        if ok && i == b.len() {
            return true;
        }
    }
    false
}

/// `_LINE_COMMENT.match`: `//` not followed by a third slash.
pub fn is_line_comment(body: &str) -> bool {
    body.starts_with("//") && !body[2..].starts_with('/')
}

/// `_SETEXT.fullmatch`: a run of one of `=` `-` `~` `^` `+`.
pub fn is_setext_line(rs: &str) -> bool {
    let b = rs.as_bytes();
    !b.is_empty() && matches!(b[0], b'=' | b'-' | b'~' | b'^' | b'+') && b.iter().all(|c| *c == b[0])
}

/// `_SECTION.match`: one to six `=` or `#`, blanks, then a non-space. Returns (marker char, marker length, end).
pub fn section_match(body: &str) -> Option<(char, usize, usize)> {
    let b = body.as_bytes();
    let m = *b.first()?;
    if m != b'=' && m != b'#' {
        return None;
    }
    let n = b.iter().take_while(|c| **c == m).count();
    if n > 6 {
        return None;
    }
    let sp = ht_run(&body[n..]);
    if sp == 0 {
        return None;
    }
    let end = n + sp;
    let next = body[end..].chars().next()?;
    if is_space(next) {
        return None;
    }
    Some((m as char, n, end))
}

/// `_BLOCK_TITLE.match`: a dot and then a character that is neither a dot nor a space.
pub fn is_block_title(body: &str) -> bool {
    match body.strip_prefix('.') {
        Some(r) => r.chars().next().is_some_and(|c| c != '.' && !is_space(c)),
        None => false,
    }
}

/// `_ADMONITION.match`: end of `NOTE:` (and the like) plus blanks, when a non-space follows.
pub fn admonition_end(body: &str) -> Option<usize> {
    for w in ["NOTE:", "TIP:", "IMPORTANT:", "CAUTION:", "WARNING:"] {
        if let Some(rest) = body.strip_prefix(w) {
            let sp = ht_run(rest);
            if sp == 0 {
                return None;
            }
            let end = w.len() + sp;
            return match body[end..].chars().next() {
                Some(c) if !is_space(c) => Some(end),
                _ => None,
            };
        }
    }
    None
}

fn list_marker_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^[ \t]*(?:\*{1,5}|-|\.{1,5}|\d{1,9}\.|[a-zA-Z]\.|[ivxlcdmIVXLCDM]{1,10}\)|<\d{1,3}>|<\.>)[ \t]+")
            .expect("list marker pattern")
    })
}

/// `_LIST_ITEM.match`: the end of the marker (and a checklist box) with the blanks after it.
pub fn list_item_end(body: &str) -> Option<usize> {
    let m = list_marker_re().find(body)?;
    let m1 = m.end();
    let rest = &body[m1..];
    // a checklist box `[x] ` is part of the marker when text follows it
    let rb = rest.as_bytes();
    if rb.len() >= 3 && rb[0] == b'[' && matches!(rb[1], b' ' | b'x' | b'X' | b'*') && rb[2] == b']' {
        let sp = ht_run(&rest[3..]);
        if sp > 0 {
            let after = m1 + 3 + sp;
            if body[after..].chars().next().is_some_and(|c| !is_space(c)) {
                return Some(after);
            }
        }
    }
    match rest.chars().next() {
        Some(c) if !is_space(c) => Some(m1),
        _ => None,
    }
}

/// `_DLIST.match(rs)`: a description list term. Returns (term start, term end, match end) as byte offsets in `rs`.
pub fn dlist_match(rs: &str) -> Option<(usize, usize, usize)> {
    let b = rs.as_bytes();
    let p = ht_run(rs);
    let first = rs[p..].chars().next()?;
    if is_space(first) {
        return None;
    }
    let tail_ok = |after: usize| -> Option<usize> {
        if after >= rs.len() {
            return Some(rs.len());
        }
        let sp = ht_run(&rs[after..]);
        if sp == 0 {
            return None;
        }
        let end = after + sp;
        match rs[end..].chars().next() {
            Some(c) if !is_space(c) => Some(end),
            _ => None,
        }
    };
    let mut k = p + first.len_utf8();
    while k <= rs.len() {
        if k < rs.len() {
            if b[k] == b':' {
                let run = b[k..].iter().take_while(|c| **c == b':').count();
                if run >= 2 {
                    let mut c = run.min(4);
                    while c >= 2 {
                        if let Some(end) = tail_ok(k + c) {
                            return Some((p, k, end));
                        }
                        c -= 1;
                    }
                }
            } else if b[k] == b';' && b.get(k + 1) == Some(&b';') {
                if let Some(end) = tail_ok(k + 2) {
                    return Some((p, k, end));
                }
            }
        }
        match rs[k..].chars().next() {
            Some(c) => k += c.len_utf8(),
            None => break,
        }
    }
    None
}

const VERBATIM_STYLES: &[&str] = &[
    "source", "listing", "literal", "pass", "comment", "stem", "latexmath", "asciimath", "plantuml", "ditaa", "graphviz", "mermaid",
    "svgbob", "a2s", "blockdiag",
];

/// `_block_style`: the first positional of a block attribute line, lower case: `[source%linenums,python]` is `source`.
pub fn block_style(attr_line: &str) -> String {
    let t = strip(attr_line);
    let inner = t.get(1..t.len().saturating_sub(1)).unwrap_or("");
    let first = strip(inner.split(',').next().unwrap_or(""));
    let cut = first.find(['#', '.', '%']).unwrap_or(first.len());
    strip(&first[..cut]).to_lowercase()
}

pub fn is_verbatim_style(style: &str) -> bool {
    VERBATIM_STYLES.contains(&style)
}

/// `_TABLE_FMT_SKIP.search`: `format=csv` (or dsv, tsv) among the block attributes.
pub fn is_table_format_skip(pending: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"(?i)\bformat\s*=\s*"?(?:csv|dsv|tsv)"#).expect("format pattern")).is_match(pending)
}

/// `_BLOCK_START.match`: line-start syntax that would change a translated paragraph's block type.
pub fn is_block_start(s: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"^(?:(?:={1,6}|\#{1,6})(?:[ \t]|\n?\z)|\.[^.\s]|\[|//|:!?[A-Za-z0-9_][A-Za-z0-9_-]*!?:|(?:NOTE|TIP|IMPORTANT|CAUTION|WARNING):[ \t]|(?:\*{1,5}|-|\.{1,5}|\d{1,9}\.|[a-zA-Z]\.|[ivxlcdmIVXLCDM]{1,10}\)|<\d{1,3}>|<\.>)(?:[ \t]|\n?\z)|-{2,}|\.{4,}|\+|_{4,}|\*{4,}|'{3,}|<{3,}|[|!,:]={3,}|```|\\?(?:include|ifdef|ifndef|ifeval|endif)::)",
        )
        .expect("block start pattern")
    })
    .is_match(s)
}

/// `_SOFT_BREAK.sub(" ", s)`: runs of blanks and line breaks inside a paragraph become one space.
pub fn plain(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    let mut copied = 0usize;
    while i < b.len() {
        if b[i] == b' ' || b[i] == b'\t' || b[i] == b'\r' || b[i] == b'\n' {
            // `(?:[ \t]*(?:\r\n|\r|\n))+[ \t]*` starting at i
            let mut j = i;
            let mut found = false;
            loop {
                let mut k = j;
                while k < b.len() && is_ht(b[k]) {
                    k += 1;
                }
                if k < b.len() && b[k] == b'\r' {
                    j = k + if b.get(k + 1) == Some(&b'\n') { 2 } else { 1 };
                    found = true;
                } else if k < b.len() && b[k] == b'\n' {
                    j = k + 1;
                    found = true;
                } else {
                    // no further line break; a blank run after the last one is part of the match
                    if found {
                        j = k;
                    } else {
                        // the blank run alone: no match here, and none from inside it either
                        i = k.max(i + 1);
                    }
                    break;
                }
            }
            if found {
                out.push_str(&s[copied..i]);
                out.push(' ');
                i = j;
                copied = j;
            }
        } else {
            i += 1;
        }
    }
    out.push_str(&s[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_collapses_breaks() {
        assert_eq!(plain("a \n  b"), "a b");
        assert_eq!(plain("a\r\n\r\nb"), "a b");
        assert_eq!(plain("a  b"), "a  b");
        assert_eq!(plain("a \t\n"), "a ");
        assert_eq!(plain("  \n x"), " x");
        assert_eq!(plain("x   "), "x   ");
    }

    #[test]
    fn dlist_terms() {
        assert_eq!(dlist_match("CPU:: The brain"), Some((0, 3, 6)));
        assert_eq!(dlist_match("Hard drive::"), Some((0, 10, 12)));
        assert_eq!(dlist_match("a::::: b").map(|m| (m.0, m.1)), Some((0, 2)));
        assert!(dlist_match("no delimiter").is_none());
        assert!(dlist_match("a::b").is_none());
    }

    #[test]
    fn list_items() {
        assert_eq!(list_item_end("* item"), Some(2));
        assert_eq!(list_item_end("  ** nested"), Some(5));
        assert_eq!(list_item_end("* [x] done"), Some(6));
        assert_eq!(list_item_end("* [x]  "), Some(2));
        assert_eq!(list_item_end("******* too many"), None);
        assert_eq!(list_item_end("ii) roman"), Some(4));
        assert_eq!(list_item_end("<1> call"), Some(4));
        assert_eq!(list_item_end("-text"), None);
    }

    #[test]
    fn section_and_titles() {
        assert_eq!(section_match("== Title =="), Some(('=', 2, 3)));
        assert_eq!(section_match("======= x"), None);
        assert!(section_match("==x").is_none());
        assert!(is_block_title(".Title"));
        assert!(!is_block_title("..x"));
    }
}
