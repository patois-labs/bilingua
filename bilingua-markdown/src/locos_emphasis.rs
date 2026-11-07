//! Localization OS's Markdown emphasis and autolink repairs (`md_emphasis.py`: `fix_cjk_emphasis`, #695, and
//! `fix_cjk_autolinks`, #700), for the LocOS profile's build. Both work on code points, as Python does, and match
//! LocOS's regular expressions by hand (Rust's `regex` has no backreferences or lookbehind):
//!
//! - protected regions (`_PROTECT`): code spans (a backtick run closed by the first later run of the same length,
//!   the opening run shortened when no later run is that long), `<...>` autolinks and tags, `](dest "title")`
//!   destinations, LocOS tokens;
//! - whole links (`_LINKS`) and bare URLs (`_BARE_URL`) for the autolink repair.
//!
//! An emphasis repair keeps the text's length and only rewrites the span of its own pair, so the pairs are fixed
//! in place, right to left, in linear time (LocOS rebuilds the string for each). The autolink repair looks for
//! every source URL in the translation; when that would take more than `AUTOLINK_BUDGET` character steps (a line
//! with thousands of URLs, where LocOS itself runs for minutes) the translation is left as it is.

use bilingua_model::pytext::py_is_punct_or_symbol_char;
use bilingua_model::{py_isdecimal_char, py_isspace_char};
use std::collections::{BTreeSet, HashMap};

const URL_TRAIL: &[char] = &['?', '!', '.', ',', ':', '*', '_', '~'];
const AUTOLINK_BUDGET: usize = 1 << 26;

fn ws(c: Option<char>) -> bool {
    c.is_none_or(py_isspace_char)
}

/// `_punct`: an ASCII character that is neither alphanumeric nor white space, or a Unicode P* / S* character.
fn punct(c: Option<char>) -> bool {
    match c {
        None => false,
        Some(c) if c.is_ascii() => !c.is_ascii_alphanumeric() && !py_isspace_char(c),
        Some(c) => py_is_punct_or_symbol_char(c),
    }
}

fn ascii_alnum(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphanumeric())
}

fn at(s: &[char], i: usize) -> Option<char> {
    s.get(i).copied()
}

fn before(s: &[char], a: usize) -> Option<char> {
    a.checked_sub(1).and_then(|i| at(s, i))
}

/// (left-flanking, right-flanking) of the delimiter run `s[a..b]` (CommonMark 6.2).
fn flanks(s: &[char], a: usize, b: usize) -> (bool, bool) {
    let (bf, af) = (before(s, a), at(s, b));
    let left = !ws(af) && (!punct(af) || ws(bf) || punct(bf));
    let right = !ws(bf) && (!punct(bf) || ws(af) || punct(af));
    (left, right)
}

fn can_open(s: &[char], a: usize, b: usize) -> bool {
    let (left, right) = flanks(s, a, b);
    if s[a] == '*' {
        return left;
    }
    left && (!right || punct(before(s, a)))
}

fn can_close(s: &[char], a: usize, b: usize) -> bool {
    let (left, right) = flanks(s, a, b);
    if s[a] == '*' {
        return right;
    }
    right && (!left || punct(at(s, b)))
}

// ---- protected regions --------------------------------------------------------------------------------

/// `<[^<>\s][^<>]*>` at `p`: its end.
fn angle(s: &[char], p: usize) -> Option<usize> {
    let c = at(s, p + 1)?;
    if c == '<' || c == '>' || py_isspace_char(c) {
        return None;
    }
    let mut j = p + 2;
    while j < s.len() && s[j] != '<' && s[j] != '>' {
        j += 1;
    }
    (at(s, j) == Some('>')).then_some(j + 1)
}

/// `\(` `[^()\s]*(?:\s+"[^"]*")?\)` at `p` (the `(`): its end.
fn destination(s: &[char], p: usize) -> Option<usize> {
    if at(s, p) != Some('(') {
        return None;
    }
    let mut j = p + 1;
    while j < s.len() && s[j] != '(' && s[j] != ')' && !py_isspace_char(s[j]) {
        j += 1;
    }
    match at(s, j)? {
        ')' => Some(j + 1),
        c if py_isspace_char(c) => {
            let mut k = j;
            while k < s.len() && py_isspace_char(s[k]) {
                k += 1;
            }
            if at(s, k) != Some('"') {
                return None;
            }
            let mut m = k + 1;
            while m < s.len() && s[m] != '"' {
                m += 1;
            }
            (m < s.len() && at(s, m + 1) == Some(')')).then_some(m + 2)
        }
        _ => None,
    }
}

/// `⟦/?[a-z]?\d+⟧` at `p`: its end.
fn token(s: &[char], p: usize) -> Option<usize> {
    let mut j = p + 1;
    if at(s, j) == Some('/') {
        j += 1;
    }
    if at(s, j).is_some_and(|c| c.is_ascii_lowercase()) {
        j += 1;
    }
    let d = s[j.min(s.len())..].iter().take_while(|c| py_isdecimal_char(**c)).count();
    (d > 0 && at(s, j + d) == Some('\u{27E7}')).then_some(j + d + 1)
}

/// For every maximal backtick run: the code span it opens when the regex starts at the run (the closing run's
/// end), or `None`. `(`+)[\s\S]*?(?<!`)(?P=tick)(?!`)`: the opening tick takes the longest length k <= the run's
/// length for which a later run has exactly k backticks; the span ends with the first such run.
fn code_spans(s: &[char]) -> HashMap<usize, usize> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i] == '`' {
            let n = s[i..].iter().take_while(|c| **c == '`').count();
            runs.push((i, n));
            i += n;
        } else {
            i += 1;
        }
    }
    let mut out = HashMap::new();
    let mut lens: BTreeSet<usize> = BTreeSet::new();
    let mut first_with: HashMap<usize, usize> = HashMap::new();
    for r in (0..runs.len()).rev() {
        let (start, n) = runs[r];
        if let Some(&k) = lens.range(..=n).next_back() {
            let (cs, cl) = runs[first_with[&k]];
            out.insert(start, cs + cl);
        }
        lens.insert(n);
        first_with.insert(n, r);
    }
    out
}

/// The spans `_PROTECT.finditer` finds.
fn protected(s: &[char]) -> Vec<(usize, usize)> {
    let codes = code_spans(s);
    let mut out = Vec::new();
    let mut p = 0;
    while p < s.len() {
        let end = match s[p] {
            // a run whose start fails fails at every later position inside it too
            '`' => match codes.get(&p) {
                Some(&e) => Some(e),
                None => {
                    p += s[p..].iter().take_while(|c| **c == '`').count();
                    continue;
                }
            },
            '<' => angle(s, p),
            ']' => destination(s, p + 1),
            '\u{27E6}' => token(s, p),
            _ => None,
        };
        match end {
            Some(e) => {
                out.push((p, e));
                p = e;
            }
            None => p += 1,
        }
    }
    out
}

/// The spans `_LINKS.finditer` finds: `!?\[[^\[\]]*\](?:\(...\)|\[[^\[\]]*\])`.
fn links(s: &[char]) -> Vec<(usize, usize)> {
    let close_bracket = |from: usize| -> Option<usize> {
        let mut j = from;
        while j < s.len() && s[j] != '[' && s[j] != ']' {
            j += 1;
        }
        (at(s, j) == Some(']')).then_some(j)
    };
    let mut out = Vec::new();
    let mut p = 0;
    while p < s.len() {
        let open = match s[p] {
            '!' if at(s, p + 1) == Some('[') => Some(p + 1),
            '[' => Some(p),
            _ => None,
        };
        let end = open.and_then(|o| close_bracket(o + 1)).and_then(|j| match at(s, j + 1) {
            Some('(') => destination(s, j + 1),
            Some('[') => close_bracket(j + 2).map(|k| k + 1),
            _ => None,
        });
        match end {
            Some(e) => {
                out.push((p, e));
                p = e;
            }
            None => p += 1,
        }
    }
    out
}

fn mask(n: usize, spans: &[(usize, usize)]) -> Vec<bool> {
    let mut m = vec![false; n];
    for &(a, b) in spans {
        for x in &mut m[a..b.min(n)] {
            *x = true;
        }
    }
    m
}

// ---- emphasis -------------------------------------------------------------------------------------------

/// `_runs`: delimiter runs of one or two marks outside protected regions, unescaped, not an identifier
/// underscore and not a lone mark between spaces.
fn delimiter_runs(s: &[char]) -> Vec<(usize, usize)> {
    let masked = mask(s.len(), &protected(s));
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c != '*' && c != '_' {
            i += 1;
            continue;
        }
        let a = i;
        let b = a + s[a..].iter().take_while(|x| **x == c).count();
        i = b;
        if b - a > 2 || masked[a..b].iter().any(|m| *m) {
            continue;
        }
        let slashes = s[..a].iter().rev().take_while(|x| **x == '\\').count();
        if slashes % 2 == 1 {
            continue;
        }
        let (bf, af) = (before(s, a), at(s, b));
        if c == '_' && ascii_alnum(bf) && ascii_alnum(af) {
            continue;
        }
        if ws(bf) && ws(af) {
            continue;
        }
        out.push((a, b));
    }
    out
}

type Run = (usize, usize);

/// `_pairs`: the runs of each kind (`*`, `**`, `_`, `__`), paired in order, for the kinds with an even count.
fn pairs(s: &[char]) -> Vec<(String, Vec<(Run, Run)>)> {
    let mut kinds: Vec<(String, Vec<Run>)> = Vec::new();
    for (a, b) in delimiter_runs(s) {
        let k: String = s[a..b].iter().collect();
        match kinds.iter_mut().find(|(n, _)| *n == k) {
            Some((_, v)) => v.push((a, b)),
            None => kinds.push((k, vec![(a, b)])),
        }
    }
    kinds
        .into_iter()
        .filter(|(_, v)| v.len() % 2 == 0)
        .map(|(k, v)| {
            let p = v.chunks(2).map(|w| (w[0], w[1])).collect();
            (k, p)
        })
        .collect()
}

/// `_try`: the region `o.0..c.1` rewritten with `mark` (moving the punctuation at the inner edges outside when
/// `mv`), when CommonMark then reads the pair; written into `s` in place (the length does not change).
fn try_pair(s: &mut [char], o: Run, c: Run, mark: char, mv: bool) -> bool {
    let len = o.1 - o.0;
    let inner: Vec<char> = s[o.1..c.0].to_vec();
    let (mut i, mut j) = (0, inner.len());
    if mv {
        let movable = |x: char| punct(Some(x)) && !matches!(x, '*' | '_' | '\\' | '`');
        while i < j && movable(inner[i]) {
            i += 1;
        }
        while j > i && movable(inner[j - 1]) {
            j -= 1;
        }
        let core = &inner[i..j];
        if core.is_empty() || py_isspace_char(core[0]) || py_isspace_char(core[core.len() - 1]) {
            return false;
        }
    }
    let mut region: Vec<char> = Vec::with_capacity(c.1 - o.0);
    region.extend_from_slice(&inner[..i]);
    region.extend(std::iter::repeat_n(mark, len));
    region.extend_from_slice(&inner[i..j]);
    region.extend(std::iter::repeat_n(mark, len));
    region.extend_from_slice(&inner[j..]);
    let old: Vec<char> = s[o.0..c.1].to_vec();
    s[o.0..c.1].copy_from_slice(&region);
    let oa = o.0 + i;
    let ca = oa + len + (j - i);
    if can_open(s, oa, oa + len) && can_close(s, ca, ca + len) {
        return true;
    }
    s[o.0..c.1].copy_from_slice(&old);
    false
}

/// LocOS `fix_cjk_emphasis`: rewrites the emphasis pairs of a translation that CommonMark would print as literal
/// marks, for the kinds of emphasis the source has; unchanged when nothing needs it.
pub fn fix_cjk_emphasis(src: &str, new: &str) -> String {
    if new.is_empty() || (!new.contains('*') && !new.contains('_')) {
        return new.to_string();
    }
    let sv: Vec<char> = src.chars().collect();
    let wanted: Vec<String> = pairs(&sv)
        .into_iter()
        .filter(|(_, ps)| ps.iter().any(|(o, c)| can_open(&sv, o.0, o.1) && can_close(&sv, c.0, c.1)))
        .map(|(k, _)| k)
        .collect();
    if wanted.is_empty() {
        return new.to_string();
    }
    let mut s: Vec<char> = new.chars().collect();
    let mut todo: Vec<(Run, Run, String)> = Vec::new();
    for (kind, ps) in pairs(&s) {
        if !wanted.contains(&kind) {
            continue;
        }
        for (o, c) in ps {
            if !(can_open(&s, o.0, o.1) && can_close(&s, c.0, c.1)) {
                todo.push((o, c, kind.clone()));
            }
        }
    }
    todo.sort();
    for (o, c, kind) in todo.into_iter().rev() {
        let underscore = kind.starts_with('_');
        if !(underscore && try_pair(&mut s, o, c, '*', false)) {
            try_pair(&mut s, o, c, '*', true);
        }
    }
    s.into_iter().collect()
}

// ---- autolinks ------------------------------------------------------------------------------------------

/// `(?:https?://|www\.)[^\s<>]+` matches: (start, end).
fn bare_urls(s: &[char]) -> Vec<(usize, usize)> {
    let starts = |p: usize, w: &str| w.chars().enumerate().all(|(k, c)| at(s, p + k) == Some(c));
    let mut out = Vec::new();
    let mut p = 0;
    while p < s.len() {
        let head = if starts(p, "https://") {
            8
        } else if starts(p, "http://") {
            7
        } else if starts(p, "www.") {
            4
        } else {
            0
        };
        if head > 0 {
            let mut j = p + head;
            while j < s.len() && !py_isspace_char(s[j]) && s[j] != '<' && s[j] != '>' {
                j += 1;
            }
            if j > p + head {
                out.push((p, j));
                p = j;
                continue;
            }
        }
        p += 1;
    }
    out
}

fn masked_spans(s: &[char]) -> Vec<bool> {
    let mut spans = protected(s);
    spans.extend(links(s));
    mask(s.len(), &spans)
}

/// `_source_urls`: the bare URLs of the source outside protected regions and links, trailing punctuation off.
fn source_urls(src: &[char]) -> BTreeSet<String> {
    let masked = masked_spans(src);
    let mut out = BTreeSet::new();
    let mut glued_in_source = BTreeSet::new();
    for (a, b) in bare_urls(src) {
        if masked[a] {
            continue;
        }
        let mut url: Vec<char> = src[a..b].to_vec();
        while url.last().is_some_and(|c| URL_TRAIL.contains(c)) {
            url.pop();
        }
        let opens = url.iter().filter(|c| **c == '(').count();
        let closes = url.iter().filter(|c| **c == ')').count();
        if url.last() == Some(&')') && opens < closes {
            url.pop();
        }
        // a URL the source itself glues to its neighbours (a link text, a CJK sentence) is not repaired: the glue is
        // the author's, and wrapping it would rewrite link text
        let end = a + url.len();
        let key: String = url.into_iter().collect();
        if glued_before(src, a) || src[end..b.max(end)].iter().any(|c| !URL_TRAIL.contains(c) && *c != ')') {
            glued_in_source.insert(key.clone());
        }
        out.insert(key);
    }
    out.retain(|u| !glued_in_source.contains(u));
    out
}

/// True when the character before a URL at `a` is neither a blank nor a mark that may open a URL.
fn glued_before(s: &[char], a: usize) -> bool {
    before(s, a).is_some_and(|c| !py_isspace_char(c) && !matches!(c, '*' | '_' | '~' | '('))
}

/// LocOS `fix_cjk_autolinks` (Markdown, not MDX): a bare URL of the source that the translation glued to the text
/// around it is written `<url>` (`[www.x](http://www.x)` for a `www.` one); unchanged otherwise.
pub fn fix_cjk_autolinks(src: &str, new: &str) -> String {
    if new.is_empty() || (!new.contains("://") && !new.contains("www.")) {
        return new.to_string();
    }
    let sv: Vec<char> = src.chars().collect();
    let urls = source_urls(&sv);
    if urls.is_empty() {
        return new.to_string();
    }
    let s: Vec<char> = new.chars().collect();
    if urls.len().saturating_mul(s.len()) > AUTOLINK_BUDGET {
        return new.to_string();
    }
    let masked = masked_spans(&s);
    // byte offset of every char, to search with `str::find` and map back
    let mut boff: Vec<usize> = Vec::with_capacity(s.len() + 1);
    let mut o = 0;
    for c in &s {
        boff.push(o);
        o += c.len_utf8();
    }
    boff.push(o);
    let char_at = |byte: usize| boff.partition_point(|&x| x < byte);
    let mut edited = vec![false; s.len()];
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut ordered: Vec<&String> = urls.iter().collect();
    ordered.sort_by_key(|u| std::cmp::Reverse(u.chars().count()));
    for url in ordered {
        let ulen = url.chars().count();
        if ulen == 0 {
            continue;
        }
        let mut start = 0usize;
        while let Some(rel) = new[boff[start]..].find(url.as_str()) {
            let a = char_at(boff[start] + rel);
            let b = a + ulen;
            start = b;
            if masked[a] || edited[a..b].iter().any(|e| *e) {
                continue;
            }
            let run_len = s[b..].iter().take_while(|c| !py_isspace_char(**c) && **c != '<').count();
            let run = &s[b..b + run_len];
            if run.first().is_some_and(|c| c.is_ascii() && !URL_TRAIL.contains(c)) {
                continue;
            }
            let glued_before = glued_before(&s, a);
            let glued_after = run.iter().any(|c| !URL_TRAIL.contains(c));
            if glued_before || glued_after {
                let rep = if url.starts_with("www.") { format!("[{url}](http://{url})") } else { format!("<{url}>") };
                for e in &mut edited[a..b] {
                    *e = true;
                }
                edits.push((a, b, rep));
            }
        }
    }
    if edits.is_empty() {
        return new.to_string();
    }
    edits.sort();
    let mut out = String::with_capacity(new.len() + 2 * edits.len());
    let mut pos = 0;
    for (a, b, rep) in edits {
        out.extend(&s[pos..a]);
        out.push_str(&rep);
        pos = b;
    }
    out.extend(&s[pos..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underscores_glued_to_cjk_become_stars() {
        assert_eq!(fix_cjk_emphasis("Click __Save__ now", "__保存__をクリック"), "**保存**をクリック");
        // punctuation at the inner edges moves outside
        assert_eq!(fix_cjk_emphasis("Click **\"Save\"** now", "**「変更を保存」**をクリック"), "「**変更を保存**」をクリック");
        assert_eq!(fix_cjk_emphasis("no marks", "**x**"), "**x**");
    }

    #[test]
    fn code_spans_shorten_their_opening_run() {
        let s: Vec<char> = "```a`` b".chars().collect();
        assert_eq!(protected(&s), vec![(0, 6)]);
        let s: Vec<char> = "``x` y".chars().collect();
        assert_eq!(protected(&s), vec![(0, 4)]);
    }

    #[test]
    fn glued_urls_are_wrapped() {
        assert_eq!(fix_cjk_autolinks("See https://e.com/a now", "参见https://e.com/a。"), "参见<https://e.com/a>。");
        assert_eq!(fix_cjk_autolinks("See https://e.com/a now", "See https://e.com/a now"), "See https://e.com/a now");
        assert_eq!(fix_cjk_autolinks("see www.e.com", "见www.e.com"), "见[www.e.com](http://www.e.com)");
    }

    #[test]
    fn a_url_the_source_already_glued_is_left_alone() {
        // a link text that holds a URL (the destination is not a plain one, so the link is not masked)
        let src = "[https://example.com/a #hash](<a b.md>)";
        assert_eq!(fix_cjk_autolinks(src, src), src);
        let tr = "[https://example.com/a #hash](<a b.md>)\u{3002}";
        assert_eq!(fix_cjk_autolinks(src, tr), tr);
        // glued after in the source: a CJK sentence
        let tr = "\u{53c2}\u{898b}https://e.com/a\u{3002}";
        assert_eq!(fix_cjk_autolinks("see\u{898b}https://e.com/a\u{3002}", tr), tr);
    }

    #[test]
    fn many_pairs_stay_linear() {
        let src = "a __b__ c";
        let new = "あ__い__う".repeat(50_000);
        let out = fix_cjk_emphasis(src, &new);
        assert_eq!(out.chars().count(), new.chars().count());
        let ticks = "`".repeat(200_000);
        let new = format!("{ticks}*x*y");
        assert_eq!(fix_cjk_emphasis(&format!("{ticks} *a*"), &new), new);
        let urls: String = (0..1_000).map(|k| format!("https://e.com/{k} ")).collect();
        assert!(fix_cjk_autolinks(&urls, &urls.replace(' ', "。")).starts_with("<https://e.com/0>"));
    }
}
