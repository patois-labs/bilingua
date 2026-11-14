//! Small text helpers that behave as the Python string methods the Localization OS filter uses
//! (`str.strip`, `str.splitlines`, `str.split`, `str.expandtabs`, `textwrap.wrap`), so unit text and
//! re-wrapped output match it character for character.

/// Python `str.isspace` for one character: Unicode white space plus the four information separators.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

pub fn lstrip(s: &str) -> &str {
    s.trim_start_matches(is_space)
}

pub fn rstrip(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// `s.rstrip("\r\n")`.
pub fn rstrip_eol(s: &str) -> &str {
    s.trim_end_matches(['\r', '\n'])
}

/// `" ".join(s.split())`.
pub fn collapse_ws(s: &str) -> String {
    s.split(is_space).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Python `str.splitlines(keepends=True)`: breaks at `\n`, `\r`, `\r\n`, VT, FF, FS, GS, RS, NEL, LS and PS.
pub fn splitlines_keep(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let b = s.as_bytes();
    let mut iter = s.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        let brk = matches!(c, '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}');
        if !brk {
            continue;
        }
        let mut end = i + c.len_utf8();
        if c == '\r' && b.get(end) == Some(&b'\n') {
            end += 1;
            iter.next();
        }
        out.push(&s[start..end]);
        start = end;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// Python `bytes.splitlines(keepends=True)`: breaks at `\n`, `\r` and `\r\n` only.
pub fn byte_lines_keep(b: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\n' => {
                out.push(&b[start..=i]);
                start = i + 1;
            }
            b'\r' => {
                let end = if b.get(i + 1) == Some(&b'\n') { i + 2 } else { i + 1 };
                out.push(&b[start..end]);
                start = end;
                i = end - 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < b.len() {
        out.push(&b[start..]);
    }
    out
}

/// `str.expandtabs(8)`.
fn expand_tabs(s: &str) -> String {
    if !s.contains('\t') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    let mut col = 0usize;
    for c in s.chars() {
        match c {
            '\t' => {
                let n = 8 - col % 8;
                out.extend(std::iter::repeat_n(' ', n));
                col += n;
            }
            '\n' | '\r' => {
                out.push(c);
                col = 0;
            }
            _ => {
                out.push(c);
                col += 1;
            }
        }
    }
    out
}

/// `textwrap.wrap(text, width, break_long_words=False, break_on_hyphens=False)` with the other defaults (tabs
/// expanded, white space replaced by spaces, white space dropped at the edges of wrapped lines).
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let t: String = expand_tabs(text)
        .chars()
        .map(|c| if matches!(c, '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r') { ' ' } else { c })
        .collect();
    // chunks: runs of spaces and runs of anything else
    let mut chunks: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut prev_space: Option<bool> = None;
    for (i, c) in t.char_indices() {
        let sp = c == ' ';
        if prev_space.is_some_and(|p| p != sp) {
            chunks.push(&t[start..i]);
            start = i;
        }
        prev_space = Some(sp);
    }
    if start < t.len() {
        chunks.push(&t[start..]);
    }
    let len = |s: &str| s.chars().count();
    let all_space = |s: &str| s.chars().all(is_space);
    let mut lines: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chunks.len() {
        let mut cur: Vec<&str> = Vec::new();
        let mut cur_len = 0usize;
        if !lines.is_empty() && all_space(chunks[i]) {
            i += 1;
        }
        while i < chunks.len() {
            let l = len(chunks[i]);
            if cur_len + l <= width {
                cur.push(chunks[i]);
                cur_len += l;
                i += 1;
            } else {
                break;
            }
        }
        if i < chunks.len() && len(chunks[i]) > width && cur.is_empty() {
            cur.push(chunks[i]);
            i += 1;
        }
        if cur.last().is_some_and(|c| all_space(c)) {
            cur.pop();
        }
        if !cur.is_empty() {
            lines.push(cur.concat());
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_like_python() {
        assert_eq!(splitlines_keep("a\r\nb\rc\nd"), vec!["a\r\n", "b\r", "c\n", "d"]);
        assert_eq!(splitlines_keep("a\u{2028}b"), vec!["a\u{2028}", "b"]);
        assert!(splitlines_keep("").is_empty());
        assert_eq!(byte_lines_keep(b"a\r\nb\rc\nd\x0b"), vec![&b"a\r\n"[..], b"b\r", b"c\n", b"d\x0b"]);
    }

    #[test]
    fn wrap_like_textwrap() {
        assert_eq!(wrap("aaa bbb ccc", 7), vec!["aaa bbb", "ccc"]);
        assert_eq!(wrap("  lead and text here", 10), vec!["  lead and", "text here"]);
        assert_eq!(wrap("averyveryverylongword x", 5), vec!["averyveryverylongword", "x"]);
        assert!(wrap("", 10).is_empty());
        assert_eq!(wrap("a\tb", 80), vec!["a       b"]);
    }

    #[test]
    fn strip_knows_separators() {
        assert_eq!(strip("\u{1c} x \u{a0}"), "x");
        assert_eq!(collapse_ws(" a \t b\n"), "a b");
    }
}
