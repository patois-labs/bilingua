//! Port of LocOS `md_emphasis.py`: repairs a translated Markdown line so CommonMark reads it the way its source
//! was read (CJK emphasis #695, bare URLs #700, block starts #704, table pipes #710, line breaks #714).
//! Offsets are byte offsets; every structural character is ASCII.

use crate::codespan;
use crate::pyx::{char_at, char_before, is_punct_symbol, is_space, trim_end};
use regex::Regex;
use std::collections::BTreeMap;
use std::sync::OnceLock;

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).expect("static pattern"))
}

fn ws(c: Option<char>) -> bool {
    c.is_none_or(is_space)
}

fn punct(c: Option<char>) -> bool {
    match c {
        None => false,
        Some(c) if c.is_ascii() => !c.is_ascii_alphanumeric() && !is_space(c),
        Some(c) => is_punct_symbol(c),
    }
}

fn ascii_alnum(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphanumeric())
}

/// Regions whose characters are never delimiters (the `_PROTECT` regex).
fn protected(s: &str) -> Vec<(usize, usize)> {
    static TOK: OnceLock<Regex> = OnceLock::new();
    let tok = re(&TOK, r"^\u{27E6}/?[a-z]?\d+\u{27E7}");
    let sp = codespan::spans_any(s);
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let mut end = None;
        match b[i] {
            b'`' => end = codespan::find_at(s, &sp, i),
            b'<' => {
                // <[^<>\s][^<>]*>
                if let Some(c) = char_at(s, i + 1) {
                    if c != '<' && c != '>' && !is_space(c) {
                        let rest = &s[i + 1 + c.len_utf8()..];
                        if let Some(p) = rest.find(['<', '>']) {
                            if rest.as_bytes()[p] == b'>' {
                                end = Some(i + 1 + c.len_utf8() + p + 1);
                            }
                        }
                    }
                }
            }
            b']' if b.get(i + 1) == Some(&b'(') => end = dest_end(s, i + 2),
            0xE2 => {
                if let Some(m) = tok.find(&s[i..]) {
                    end = Some(i + m.end());
                }
            }
            _ => {}
        }
        match end {
            Some(e) if e > i => {
                out.push((i, e));
                i = e;
            }
            _ => i += 1,
        }
    }
    out
}

/// `\]\(` already consumed: `[^()\s]*(?:\s+"[^"]*")?\)`.
fn dest_end(s: &str, mut j: usize) -> Option<usize> {
    let mut it = s[j..].char_indices();
    let mut stop = s.len() - j;
    for (k, c) in it.by_ref() {
        if c == '(' || c == ')' || is_space(c) {
            stop = k;
            break;
        }
    }
    j += stop;
    match char_at(s, j) {
        Some(')') => Some(j + 1),
        Some(c) if is_space(c) => {
            let after = s[j..].trim_start_matches(is_space);
            let q = s.len() - after.len();
            let rest = after.strip_prefix('"')?;
            let close = rest.find('"')?;
            let e = q + 1 + close + 1;
            (s.as_bytes().get(e) == Some(&b')')).then_some(e + 1)
        }
        _ => None,
    }
}

fn flanks(s: &str, a: usize, b: usize) -> (bool, bool) {
    let before = char_before(s, a);
    let after = char_at(s, b);
    let left = !ws(after) && (!punct(after) || ws(before) || punct(before));
    let right = !ws(before) && (!punct(before) || ws(after) || punct(after));
    (left, right)
}

fn can_open(s: &str, a: usize, b: usize) -> bool {
    let (left, right) = flanks(s, a, b);
    if s.as_bytes()[a] == b'*' {
        return left;
    }
    left && (!right || punct(char_before(s, a)))
}

fn can_close(s: &str, a: usize, b: usize) -> bool {
    let (left, right) = flanks(s, a, b);
    if s.as_bytes()[a] == b'*' {
        return right;
    }
    right && (!left || punct(char_at(s, b)))
}

/// Delimiter runs of one or two marks outside protected regions, unescaped, not an identifier underscore.
fn runs(s: &str) -> Vec<(usize, usize)> {
    let prot = protected(s);
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'*' && b[i] != b'_' {
            i += 1;
            continue;
        }
        let a = i;
        while i < b.len() && b[i] == b[a] {
            i += 1;
        }
        let e = i;
        if e - a > 2 || prot.iter().any(|&(x, y)| x < e && a < y) {
            continue;
        }
        let mut k = a;
        while k > 0 && b[k - 1] == b'\\' {
            k -= 1;
        }
        if (a - k) % 2 == 1 {
            continue;
        }
        let before = char_before(s, a);
        let after = char_at(s, e);
        if b[a] == b'_' && ascii_alnum(before) && ascii_alnum(after) {
            continue;
        }
        if ws(before) && ws(after) {
            continue;
        }
        out.push((a, e));
    }
    out
}

type Pair = ((usize, usize), (usize, usize));

fn pairs(s: &str) -> BTreeMap<String, Vec<Pair>> {
    let mut kinds: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();
    for (a, b) in runs(s) {
        kinds.entry(s[a..b].to_string()).or_default().push((a, b));
    }
    kinds
        .into_iter()
        .filter(|(_, v)| v.len() % 2 == 0)
        .map(|(k, v)| {
            let p = (0..v.len()).step_by(2).map(|i| (v[i], v[i + 1])).collect();
            (k, p)
        })
        .collect()
}

fn source_kinds(src: &str) -> Vec<String> {
    pairs(src)
        .into_iter()
        .filter(|(_, ps)| ps.iter().any(|&(o, c)| can_open(src, o.0, o.1) && can_close(src, c.0, c.1)))
        .map(|(k, _)| k)
        .collect()
}

fn try_pair(s: &str, o: (usize, usize), c: (usize, usize), mark: &str, mv: bool) -> Option<String> {
    let inner = &s[o.1..c.0];
    let (mut lead, mut trail, mut core) = ("", "", inner);
    if mv {
        let movable = |ch: char| is_punct_symbol_or_ascii(ch) && !"*_\\`".contains(ch);
        let mut i = 0;
        for (k, ch) in inner.char_indices() {
            if movable(ch) {
                i = k + ch.len_utf8();
            } else {
                break;
            }
            if i == inner.len() {
                break;
            }
        }
        let mut j = inner.len();
        for (k, ch) in inner.char_indices().rev() {
            if j > i && movable(ch) {
                j = k;
            } else {
                break;
            }
        }
        lead = &inner[..i];
        trail = &inner[j..];
        core = &inner[i..j];
        if core.is_empty() || core.chars().next().is_some_and(is_space) || core.chars().next_back().is_some_and(is_space) {
            return None;
        }
    }
    let new = format!("{}{}{}{}{}{}{}", &s[..o.0], lead, mark, core, mark, trail, &s[c.1..]);
    let oa = o.0 + lead.len();
    let ca = oa + mark.len() + core.len();
    (can_open(&new, oa, oa + mark.len()) && can_close(&new, ca, ca + mark.len())).then_some(new)
}

fn is_punct_symbol_or_ascii(ch: char) -> bool {
    punct(Some(ch))
}

/// Rewrites emphasis pairs of a translation that CommonMark would print as literal marks.
pub(crate) fn fix_cjk_emphasis(src: &str, new: &str) -> String {
    if new.is_empty() || (!new.contains('*') && !new.contains('_')) {
        return new.to_string();
    }
    let wanted = source_kinds(src);
    if wanted.is_empty() {
        return new.to_string();
    }
    type Todo = ((usize, usize), (usize, usize), String);
    let mut todo: Vec<Todo> = Vec::new();
    for (kind, lst) in pairs(new) {
        if !wanted.contains(&kind) {
            continue;
        }
        for (o, c) in lst {
            if !(can_open(new, o.0, o.1) && can_close(new, c.0, c.1)) {
                todo.push((o, c, kind.clone()));
            }
        }
    }
    todo.sort();
    let mut s = new.to_string();
    for (o, c, kind) in todo.into_iter().rev() {
        let star = "*".repeat(kind.len());
        let fixed = if kind.starts_with('_') { try_pair(&s, o, c, &star, false) } else { None }.or_else(|| try_pair(&s, o, c, &star, true));
        if let Some(f) = fixed {
            s = f;
        }
    }
    s
}

const URL_TRAIL: &str = "?!.,:*_~";

fn masked_spans(s: &str) -> Vec<(usize, usize)> {
    static LINKS: OnceLock<Regex> = OnceLock::new();
    let l = re(&LINKS, r#"!?\[[^\[\]]*\](?:\([^()\s\x1c-\x1f]*(?:[\s\x1c-\x1f]+"[^"]*")?\)|\[[^\[\]]*\])"#);
    let mut v = protected(s);
    v.extend(l.find_iter(s).map(|m| (m.start(), m.end())));
    v
}

fn inside(spans: &[(usize, usize)], a: usize) -> bool {
    spans.iter().any(|&(x, y)| x <= a && a < y)
}

fn bare_url_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"(?:https?://|www\.)[^\s\x1c-\x1f<>]+")
}

fn source_urls(src: &str) -> Vec<String> {
    let spans = masked_spans(src);
    let mut out: Vec<String> = Vec::new();
    for m in bare_url_re().find_iter(src) {
        if inside(&spans, m.start()) {
            continue;
        }
        let mut url = m.as_str().trim_end_matches(|c| URL_TRAIL.contains(c));
        if url.ends_with(')') && url.matches('(').count() < url.matches(')').count() {
            url = &url[..url.len() - 1];
        }
        if !out.iter().any(|u| u == url) {
            out.push(url.to_string());
        }
    }
    out
}

/// Wraps a source URL the translation glued to CJK text in an explicit link.
pub(crate) fn fix_cjk_autolinks(src: &str, new: &str, mdx: bool) -> String {
    if new.is_empty() || (!new.contains("://") && !new.contains("www.")) {
        return new.to_string();
    }
    let mut urls = source_urls(src);
    if urls.is_empty() {
        return new.to_string();
    }
    urls.sort_by_key(|u| std::cmp::Reverse(u.chars().count()));
    let spans = masked_spans(new);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for url in &urls {
        let mut start = 0;
        while let Some(p) = new[start..].find(url.as_str()) {
            let a = start + p;
            let b = a + url.len();
            start = b;
            if inside(&spans, a) || edits.iter().any(|&(x, y, _)| x < b && a < y) {
                continue;
            }
            let before = char_before(new, a);
            let tail = &new[b..];
            let run_len = tail.find(|c: char| is_space(c) || c == '<').unwrap_or(tail.len());
            let run = &tail[..run_len];
            if let Some(f) = run.chars().next() {
                if f.is_ascii() && !URL_TRAIL.contains(f) {
                    continue;
                }
            }
            let glued_before = before.is_some_and(|c| !is_space(c) && !"*_~(".contains(c));
            let glued_after = !run.trim_end_matches(|c| URL_TRAIL.contains(c)).is_empty();
            if glued_before || glued_after {
                let rep = if url.starts_with("www.") {
                    format!("[{url}](http://{url})")
                } else if mdx {
                    format!("[{url}]({url})")
                } else {
                    format!("<{url}>")
                };
                edits.push((a, b, rep));
            }
        }
    }
    edits.sort();
    let mut out = new.to_string();
    for (a, b, rep) in edits.into_iter().rev() {
        out.replace_range(a..b, &rep);
    }
    out
}

/// A block marker (ordered number, ATX hash, bullet, quote) at the start of `s`: (marker start, escape point).
fn block_start(s: &str) -> Option<usize> {
    let lead = s.len() - s.trim_start_matches([' ', '\t']).len();
    if lead > 3 {
        return None;
    }
    let b = s.as_bytes();
    let ws_or_end = |i: usize| match b.get(i) {
        None => true,
        Some(b' ') | Some(b'\t') => true,
        Some(b'\n') => i + 1 == b.len(),
        _ => false,
    };
    let mut i = lead;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let digits = i - lead;
    if (1..=9).contains(&digits) && matches!(b.get(i), Some(b'.') | Some(b')')) && ws_or_end(i + 1) {
        return Some(i);
    }
    let mut h = lead;
    while h < b.len() && b[h] == b'#' {
        h += 1;
    }
    if (1..=6).contains(&(h - lead)) && ws_or_end(h) {
        return Some(lead);
    }
    if matches!(b.get(lead), Some(b'-') | Some(b'+') | Some(b'*')) && ws_or_end(lead + 1) {
        return Some(lead);
    }
    if b.get(lead) == Some(&b'>') {
        return Some(lead);
    }
    None
}

/// A translated line-start unit must not open a block its source did not.
pub(crate) fn escape_block_start(src: &str, new: &str) -> String {
    match block_start(new) {
        Some(at) if block_start(src).is_none() => format!("{}\\{}", &new[..at], &new[at..]),
        _ => new.to_string(),
    }
}

fn cell_split(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut st = 0;
    for i in 0..b.len() {
        if b[i] == b'|' && (i == 0 || b[i - 1] != b'\\') {
            out.push(&s[st..i]);
            st = i + 1;
        }
    }
    out.push(&s[st..]);
    out
}

fn lone_backslashes(s: &str) -> Vec<usize> {
    let b = s.as_bytes();
    (0..b.len()).filter(|&i| b[i] == b'\\' && matches!(b.get(i + 1), Some(b' ') | Some(b'\t'))).collect()
}

fn restore_dropped_pipes(src: &str, new: &str) -> String {
    let missing = src.matches("\\|").count() as i64 - new.matches("\\|").count() as i64;
    let extra = lone_backslashes(new).len() as i64 - lone_backslashes(src).len() as i64;
    let n = missing.min(extra);
    if n <= 0 {
        return new.to_string();
    }
    let at = lone_backslashes(new);
    let mut out = String::new();
    let mut pos = 0;
    for &i in at.iter().take(n as usize) {
        out.push_str(&new[pos..i]);
        out.push_str("\\|");
        pos = i + 1;
    }
    out.push_str(&new[pos..]);
    out
}

/// A translated table row keeps the source's `\|` escapes.
pub(crate) fn restore_cell_escapes(src: &str, new: &str) -> String {
    if new.is_empty() || !src.contains("\\|") {
        return new.to_string();
    }
    let new = restore_dropped_pipes(src, new);
    let s_cells = cell_split(src);
    let t_cells = cell_split(&new);
    let mut excess = t_cells.len() as i64 - s_cells.len() as i64;
    if excess <= 0 {
        return new;
    }
    let mut out: Vec<String> = Vec::new();
    let mut ti = 0;
    for sc in &s_cells {
        if ti >= t_cells.len() {
            break;
        }
        let mut cell = t_cells[ti].to_string();
        ti += 1;
        let mut lost = sc.matches("\\|").count() as i64 - cell.matches("\\|").count() as i64;
        while lost > 0 && excess > 0 && ti < t_cells.len() {
            cell.push_str("\\|");
            cell.push_str(t_cells[ti]);
            ti += 1;
            lost -= 1;
            excess -= 1;
        }
        out.push(cell);
    }
    out.extend(t_cells[ti..].iter().map(|c| c.to_string()));
    out.join("|")
}

/// A one-line unit stays one line (#714).
pub(crate) fn fold_line_breaks(src: &str, new: &str) -> String {
    if new.is_empty() || src.contains('\n') || src.contains('\r') {
        return new.to_string();
    }
    static BREAK: OnceLock<Regex> = OnceLock::new();
    let r = re(&BREAK, r"[ \t]*(?:\r\n|\r|\n)[ \t]*");
    r.replace_all(new.trim_matches(['\r', '\n']), " ").into_owned()
}

#[allow(unused)]
pub(crate) fn rstrip_py(s: &str) -> &str {
    trim_end(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emphasis_left_alone() {
        // a source without emphasis, spaced pairs, code spans, link targets, an identifier underscore
        assert_eq!(fix_cjk_emphasis("Plain.", "平文_です_"), "平文_です_");
        assert_eq!(fix_cjk_emphasis("a _b_ c", "あ _b_ い"), "あ _b_ い");
        let kept = "`a_b_`と[_文_](http://x/a_b_c)。";
        assert_eq!(fix_cjk_emphasis("`a_b_` and [_d_](http://x/a_b_c).", kept), kept);
    }

    #[test]
    fn emphasis_rewritten() {
        assert_eq!(fix_cjk_emphasis("Choose __Advanced__ options.", "__詳細__オプションを選択。"), "**詳細**オプションを選択。");
        assert_eq!(fix_cjk_emphasis("Click **\"Save\"** now.", "**「保存」**をクリック。"), "「**保存**」をクリック。");
    }

    #[test]
    fn block_starts() {
        assert_eq!(escape_block_start("Plain", "1984. Das"), "1984\\. Das");
        assert_eq!(escape_block_start("Plain", "# Titel"), "\\# Titel");
        assert_eq!(escape_block_start("1984. x", "1984. y"), "1984. y");
        assert_eq!(escape_block_start("Plain", "-5 Grad"), "-5 Grad");
        assert_eq!(escape_block_start("Plain", "      deep"), "      deep");
        assert_eq!(escape_block_start("Plain", "1234567890. x"), "1234567890. x");
    }

    #[test]
    fn line_breaks_fold() {
        assert_eq!(fold_line_breaks("one", "a \r\n b\n"), "a b");
        assert_eq!(fold_line_breaks("two\nlines", "a\nb"), "a\nb");
    }
}
