//! The LocOS profile's Markdown filter (Track D, X1 and X2): a port of Localization OS's `markdown_filter.py`
//! (walk 3), so units and built files match LocOS.
//!
//! Units, in this order: the one-line string values of the allowed front matter keys (YAML `---` or TOML `+++`
//! block at the start), then one unit per prose LINE: the line without its markup prefix (heading `#`, bullet,
//! number, `>`, a footnote label) and trailing white space, inline Markdown kept as text. Fenced and indented
//! code, horizontal rules and table separators, HTML comment blocks and link reference definitions are skipped
//! (a definition's one-line title is a unit). A line with an image has the style `a11y:md-image`.
//! Build replaces only the unit's span of its line and applies LocOS's repairs: a line break in a one-line unit
//! becomes a space, emphasis CommonMark would not read is rewritten and a URL glued to the text around it becomes
//! an explicit autolink (`locos_emphasis`), a table row gets back the `\|` escapes it lost, a line never opens a
//! block its source did not.
//! The skeleton is the decoded file (`Skeleton::Binary`, UTF-8), which marks a profile document.

use bilingua_model::pytext::{py_split_line_end, py_splitlines_keepends};
use bilingua_model::*;
use std::collections::HashMap;

/// LocOS `DEFAULT_FRONT_MATTER_KEYS`, lower case.
const FM_KEYS: &[&str] = &["title", "description", "summary", "excerpt", "subtitle", "sidebar_label", "pagination_label", "linktitle"];
const FM_MAX_CHARS: usize = 256 * 1024;

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn cs(v: &[char], a: usize, b: usize) -> String {
    v[a.min(v.len())..b.min(v.len())].iter().collect()
}

fn sp(c: char) -> bool {
    py_isspace_char(c)
}

/// `[ \t]*(?:\r\n|[\r\n\x0b\x0c\x1c-\x1e\x85  ])[ \t]*`: a line break with the blanks around it.
fn line_break_char(c: char) -> bool {
    matches!(c, '\r' | '\n' | '\u{0b}' | '\u{0c}' | '\u{1c}'..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

fn fold_breaks(s: &str, crlf_only: bool) -> String {
    let v = chars(s);
    let mut out = String::new();
    let mut i = 0;
    while i < v.len() {
        let brk = |c: char| if crlf_only { c == '\r' || c == '\n' } else { line_break_char(c) };
        // find a run of blanks followed by a break
        let mut j = i;
        while j < v.len() && (v[j] == ' ' || v[j] == '\t') {
            j += 1;
        }
        if j < v.len() && brk(v[j]) {
            let mut k = if v[j] == '\r' && v.get(j + 1) == Some(&'\n') { j + 2 } else { j + 1 };
            while k < v.len() && (v[k] == ' ' || v[k] == '\t') {
                k += 1;
            }
            out.push(' ');
            i = k;
            continue;
        }
        if j > i {
            out.extend(&v[i..j]);
            i = j;
            continue;
        }
        out.push(v[i]);
        i += 1;
    }
    out
}

// ---- front matter -------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct FmField {
    line: usize,
    start: usize,
    end: usize,
    style: &'static str,
    text: String,
}

struct FrontMatter {
    toml: bool,
    end: usize,
    fields: Vec<FmField>,
}

/// PyYAML's printable characters.
fn yaml_printable(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{7e}' | '\u{85}' | '\u{a0}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

/// True when PyYAML's implicit resolvers read this plain scalar as something other than a string.
fn yaml_plain_not_str(s: &str) -> bool {
    const WORDS: &[&str] =
        &["yes", "Yes", "YES", "no", "No", "NO", "true", "True", "TRUE", "false", "False", "FALSE", "on", "On", "ON", "off", "Off", "OFF", "~", "null", "Null", "NULL", "", "<<", "="];
    if WORDS.contains(&s) {
        return true;
    }
    let b = s.as_bytes();
    let digit = |c: u8| c.is_ascii_digit();
    let unsigned = b.strip_prefix(b"-").or_else(|| b.strip_prefix(b"+")).unwrap_or(b);
    // int: binary, octal, decimal, hex, sexagesimal
    let all = |v: &[u8], f: &dyn Fn(u8) -> bool| !v.is_empty() && v.iter().all(|c| f(*c));
    if let Some(r) = unsigned.strip_prefix(b"0b") {
        if all(r, &|c| c == b'0' || c == b'1' || c == b'_') {
            return true;
        }
    }
    if let Some(r) = unsigned.strip_prefix(b"0x") {
        if all(r, &|c| c.is_ascii_hexdigit() || c == b'_') {
            return true;
        }
    }
    if unsigned.first() == Some(&b'0') && all(&unsigned[1..], &|c| (b'0'..=b'7').contains(&c) || c == b'_') {
        return true;
    }
    if unsigned == b"0" || (unsigned.first().is_some_and(|c| (b'1'..=b'9').contains(c)) && all(unsigned, &|c| digit(c) || c == b'_')) {
        return true;
    }
    // sexagesimal int / float: [1-9][0-9_]*(:[0-5]?[0-9])+ (then \.[0-9_]* for a float)
    let sexa = |v: &[u8]| -> Option<usize> {
        if !v.first().is_some_and(|c| digit(*c)) {
            return None;
        }
        let mut i = v.iter().take_while(|c| digit(**c) || **c == b'_').count();
        let mut groups = 0;
        while v.get(i) == Some(&b':') {
            let mut j = i + 1;
            let d = v[j..].iter().take_while(|c| digit(**c)).count();
            if d == 0 || d > 2 || (d == 2 && v[j] > b'5') {
                break;
            }
            j += d;
            i = j;
            groups += 1;
        }
        (groups > 0).then_some(i)
    };
    if unsigned.first().is_some_and(|c| (b'1'..=b'9').contains(c)) && sexa(unsigned) == Some(unsigned.len()) {
        return true;
    }
    if let Some(i) = sexa(unsigned) {
        if unsigned.get(i) == Some(&b'.') && unsigned[i + 1..].iter().all(|c| digit(*c) || *c == b'_') {
            return true;
        }
    }
    // float: [-+]?([0-9][0-9_]*)?\.[0-9_]*([eE][-+][0-9]+)?, .inf, .nan
    if matches!(unsigned, b".inf" | b".Inf" | b".INF") || matches!(b, b".nan" | b".NaN" | b".NAN") {
        return true;
    }
    let lead = unsigned.iter().take_while(|c| digit(**c) || **c == b'_').count();
    let lead_ok = lead == 0 || digit(unsigned[0]);
    if lead_ok && unsigned.get(lead) == Some(&b'.') {
        let rest = &unsigned[lead + 1..];
        let frac = rest.iter().take_while(|c| digit(**c) || **c == b'_').count();
        let tail = &rest[frac..];
        let exp_ok = tail.is_empty() || (matches!(tail[0], b'e' | b'E') && tail.len() > 2 && matches!(tail[1], b'-' | b'+') && all(&tail[2..], &|c| digit(c)));
        let has_digit = lead > 0 || rest.first().is_some_and(|c| digit(*c));
        if exp_ok && has_digit && (lead > 0 || b.len() == unsigned.len()) {
            return true;
        }
    }
    // timestamp: yyyy-m?m-d?d ...
    if b.len() >= 8 && b[..4].iter().all(|c| digit(*c)) && b[4] == b'-' {
        let r = &b[5..];
        let m = r.iter().take_while(|c| digit(**c)).count();
        if (1..=2).contains(&m) && r.get(m) == Some(&b'-') {
            let d = r[m + 1..].iter().take_while(|c| digit(**c)).count();
            if (1..=2).contains(&d) {
                return true;
            }
        }
    }
    false
}

/// What `yaml.safe_load("k: " + s)` reads for a one-line plain scalar `s`, when it is a string.
fn yaml_plain(s: &str) -> Option<String> {
    let v = chars(s);
    let first = *v.first()?;
    if sp(first) || v.last().is_some_and(|c| sp(*c)) || !v.iter().all(|c| yaml_printable(*c)) || v.iter().any(|c| line_break_char(*c)) {
        return None;
    }
    if matches!(first, ',' | '[' | ']' | '{' | '}' | '#' | '&' | '*' | '!' | '|' | '>' | '\'' | '"' | '%' | '@' | '`') {
        return None;
    }
    if matches!(first, '-' | '?' | ':') && v.get(1).is_none_or(|c| sp(*c)) {
        return None;
    }
    for i in 0..v.len() {
        if v[i] == ':' && v.get(i + 1).is_none_or(|c| sp(*c)) {
            return None;
        }
        if v[i] == '#' && i > 0 && sp(v[i - 1]) {
            return None;
        }
    }
    if v.contains(&'\t') || yaml_plain_not_str(s) {
        return None;
    }
    Some(s.to_string())
}

/// `yaml.safe_load` of a single-quoted scalar's inside (`''` is a quote).
fn yaml_single(inner: &str) -> Option<String> {
    if !inner.chars().all(yaml_printable) || inner.chars().any(line_break_char) {
        return None;
    }
    Some(inner.replace("''", "'"))
}

/// `yaml.safe_load` of a double-quoted scalar's inside.
fn yaml_double(inner: &str) -> Option<String> {
    let v = chars(inner);
    if !v.iter().all(|c| yaml_printable(*c)) || v.iter().any(|c| line_break_char(*c)) {
        return None;
    }
    let mut out = String::new();
    let mut i = 0;
    while i < v.len() {
        let c = v[i];
        if c == '"' {
            return None;
        }
        if c != '\\' {
            out.push(c);
            i += 1;
            continue;
        }
        let e = *v.get(i + 1)?;
        let hex = |n: usize| -> Option<char> {
            let h: String = v.get(i + 2..i + 2 + n)?.iter().collect();
            if h.len() != n || !h.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            char::from_u32(u32::from_str_radix(&h, 16).ok()?)
        };
        let (ch, len) = match e {
            '0' => ('\0', 2),
            'a' => ('\u{7}', 2),
            'b' => ('\u{8}', 2),
            't' | '\t' => ('\t', 2),
            'n' => ('\n', 2),
            'v' => ('\u{b}', 2),
            'f' => ('\u{c}', 2),
            'r' => ('\r', 2),
            'e' => ('\u{1b}', 2),
            ' ' => (' ', 2),
            '"' => ('"', 2),
            '\\' => ('\\', 2),
            '/' => ('/', 2),
            'N' => ('\u{85}', 2),
            '_' => ('\u{a0}', 2),
            'L' => ('\u{2028}', 2),
            'P' => ('\u{2029}', 2),
            'x' => (hex(2)?, 4),
            'u' => (hex(4)?, 6),
            'U' => (hex(8)?, 10),
            _ => return None,
        };
        out.push(ch);
        i += len;
    }
    Some(out)
}

/// What `tomllib.loads("k = " + scalar)` reads, for a basic (`"`) or literal (`'`) string.
fn toml_string(inner: &str, basic: bool) -> Option<String> {
    let v = chars(inner);
    if v.iter().any(|c| (*c < ' ' && *c != '\t') || *c == '\u{7f}') {
        return None;
    }
    if !basic {
        return (!inner.contains('\'')).then(|| inner.to_string());
    }
    let mut out = String::new();
    let mut i = 0;
    while i < v.len() {
        let c = v[i];
        if c == '"' {
            return None;
        }
        if c != '\\' {
            out.push(c);
            i += 1;
            continue;
        }
        let hex = |n: usize| -> Option<char> {
            let h: String = v.get(i + 2..i + 2 + n)?.iter().collect();
            if h.len() != n || !h.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            char::from_u32(u32::from_str_radix(&h, 16).ok()?)
        };
        let (ch, len) = match *v.get(i + 1)? {
            'b' => ('\u{8}', 2),
            't' => ('\t', 2),
            'n' => ('\n', 2),
            'f' => ('\u{c}', 2),
            'r' => ('\r', 2),
            '"' => ('"', 2),
            '\\' => ('\\', 2),
            'u' => (hex(4)?, 6),
            'U' => (hex(8)?, 10),
            _ => return None,
        };
        out.push(ch);
        i += len;
    }
    Some(out)
}

/// The value a one-line scalar (`style`, the text of its span) reads as.
fn scalar_value(toml: bool, style: &str, span: &str) -> Option<String> {
    let inner = || span.get(1..span.len().saturating_sub(1)).unwrap_or("");
    match (toml, style) {
        (false, "plain") => yaml_plain(span),
        (false, "single") => yaml_single(inner()),
        (false, _) => yaml_double(inner()),
        (true, "literal") => toml_string(inner(), false),
        (true, _) => toml_string(inner(), true),
    }
}

/// `_YAML_KEY` / `_TOML_KEY` at the start of a line: (key, char offset of the value).
fn fm_key(v: &[char], toml: bool) -> Option<(String, usize)> {
    let mut i = 0;
    if toml {
        while i < v.len() && (v[i].is_ascii_alphanumeric() || v[i] == '_' || v[i] == '-') {
            i += 1;
        }
    } else {
        if !v.first().is_some_and(|c| c.is_ascii_alphabetic() || *c == '_') {
            return None;
        }
        while i < v.len() && (v[i].is_ascii_alphanumeric() || v[i] == '_' || v[i] == '-') {
            i += 1;
        }
    }
    if i == 0 {
        return None;
    }
    let key: String = v[..i].iter().collect();
    let mut j = i;
    while j < v.len() && (v[j] == ' ' || v[j] == '\t') {
        j += 1;
    }
    if v.get(j) != Some(&if toml { '=' } else { ':' }) {
        return None;
    }
    j += 1;
    let blanks = j;
    while j < v.len() && (v[j] == ' ' || v[j] == '\t') {
        j += 1;
    }
    if toml {
        matches!(v.get(j), Some('"' | '\'')).then_some((key, j))
    } else {
        (j > blanks && v.get(j).is_some_and(|c| !sp(*c))).then_some((key, j))
    }
}

/// `rest.strip() and not _TRAILING_COMMENT.fullmatch(rest)`: the text after a scalar is not just a comment.
fn bad_tail(rest: &[char]) -> bool {
    let s: String = rest.iter().collect();
    if py_strip(&s).is_empty() {
        return false;
    }
    let blanks = rest.iter().take_while(|c| **c == ' ' || **c == '\t').count();
    !(blanks > 0 && rest.get(blanks) == Some(&'#') && !rest.iter().any(|c| *c == '\n' || *c == '\r'))
}

fn yaml_span(v: &[char], pos: usize) -> Option<(usize, &'static str)> {
    let (end, style) = match v[pos] {
        '"' => {
            let mut j = pos + 1;
            loop {
                if j >= v.len() {
                    return None;
                }
                if v[j] == '\\' {
                    j += 2;
                    continue;
                }
                if v[j] == '"' {
                    break;
                }
                j += 1;
            }
            (j + 1, "double")
        }
        '\'' => {
            let mut j = pos + 1;
            let k = loop {
                let k = (j..v.len()).find(|&k| v[k] == '\'')?;
                if v.get(k + 1) == Some(&'\'') {
                    j = k + 2;
                    continue;
                }
                break k;
            };
            (k + 1, "single")
        }
        _ => {
            let stop = (pos..v.len()).find(|&k| (v[k] == ' ' || v[k] == '\t') && v.get(k + 1) == Some(&'#')).unwrap_or(v.len());
            let mut end = stop;
            while end > pos && sp(v[end - 1]) {
                end -= 1;
            }
            (end, "plain")
        }
    };
    if bad_tail(&v[end.min(v.len())..]) {
        return None;
    }
    Some((end, style))
}

fn toml_span(v: &[char], pos: usize) -> Option<(usize, &'static str)> {
    let q = v[pos];
    if v.get(pos + 1) == Some(&q) && v.get(pos + 2) == Some(&q) {
        return None;
    }
    let (end, style) = if q == '"' {
        let mut j = pos + 1;
        loop {
            if j >= v.len() {
                return None;
            }
            if v[j] == '\\' {
                j += 2;
                continue;
            }
            if v[j] == '"' {
                break;
            }
            j += 1;
        }
        (j + 1, "basic")
    } else {
        let k = (pos + 1..v.len()).find(|&k| v[k] == '\'')?;
        (k + 1, "literal")
    };
    if bad_tail(&v[end.min(v.len())..]) {
        return None;
    }
    Some((end, style))
}

/// `_YAML_KEYISH` / `_TOML_KEYISH` at the start of a line.
fn keyish(line: &str, toml: bool) -> bool {
    let v = chars(line);
    let word = |c: char| py_is_word_char(c) || c == '-';
    if toml {
        if v.first() == Some(&'[') {
            return true;
        }
        let n = v.iter().take_while(|c| word(**c) || matches!(c, '.' | '"' | '\'')).count();
        let mut j = n;
        while j < v.len() && (v[j] == ' ' || v[j] == '\t') {
            j += 1;
        }
        return n > 0 && v.get(j) == Some(&'=');
    }
    let mut i = usize::from(matches!(v.first(), Some('"' | '\'')));
    let n = v[i..].iter().take_while(|c| word(**c)).count();
    if n == 0 {
        return false;
    }
    i += n;
    if matches!(v.get(i), Some('"' | '\'')) {
        i += 1;
    }
    while i < v.len() && (v[i] == ' ' || v[i] == '\t') {
        i += 1;
    }
    v.get(i) == Some(&':') && v.get(i + 1).is_none_or(|c| *c == ' ' || *c == '\t')
}

fn offerable(v: &str) -> bool {
    py_has_alnum(v) && !v.chars().any(line_break_char)
}

// ---- whole-block readability (LocOS parses the block with yaml.safe_load / tomllib first) ----------------

/// What a YAML line's value leaves open for the lines after it.
#[derive(Clone, Copy, PartialEq)]
enum YamlAfter {
    /// An empty value: a nested block (more indented lines, or `- ` items at the key's own column) may follow.
    Nested(usize),
    /// A plain scalar, which more indented lines continue.
    Plain(usize),
    /// A block scalar (`|`, `>`): more indented lines are its text.
    Block(usize),
    /// A quoted or flow value closed on its line, or a comment after a scalar: nothing may continue it.
    Closed,
    /// `- ` items of a sequence at the column of the key above it.
    Items(usize),
}

/// The `(#.*)` comment of a value: where `[ \t]#` starts one in `v`, else `v.len()`.
fn comment_at(v: &[char]) -> usize {
    (0..v.len()).find(|&k| v[k] == '#' && k > 0 && (v[k - 1] == ' ' || v[k - 1] == '\t')).unwrap_or(v.len())
}

/// True when `rest` (after a closed scalar) is empty, blanks, or blanks and a comment.
fn only_comment(rest: &[char]) -> bool {
    let b = rest.iter().take_while(|c| **c == ' ' || **c == '\t').count();
    b == rest.len() || (b > 0 && rest[b] == '#')
}

/// A flow collection (`[..]`, `{..}`) at the start of `v`: `Some(end)` when it closes on the line (quotes
/// skipped), `None` when it goes on.
fn flow_end(v: &[char]) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = 0;
    while i < v.len() {
        match v[i] {
            '"' => {
                i += 1;
                while i < v.len() && v[i] != '"' {
                    i += if v[i] == '\\' { 2 } else { 1 };
                }
            }
            '\'' => {
                i += 1;
                while i < v.len() && v[i] != '\'' {
                    i += 1;
                }
            }
            '[' | '{' => depth += 1,
            ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The state after a YAML value `v` (the text after `key:` and its blanks) on a line at column `ind`; `Err(())`
/// when PyYAML refuses it, `Ok(None)` when the value goes on in a way this check does not follow (the block is
/// then taken as readable).
fn yaml_value(v: &[char], ind: usize, anchors: &mut std::collections::HashSet<String>) -> Result<Option<YamlAfter>, ()> {
    let name_end = |v: &[char], from: usize| (from..v.len()).find(|&k| sp(v[k]) || matches!(v[k], ',' | '[' | ']' | '{' | '}')).unwrap_or(v.len());
    // the node's properties first: one anchor `&name` and one tag at most, in either order
    let mut v = v;
    let (mut anchored, mut tagged) = (false, false);
    loop {
        let e = match v.first() {
            Some('&') if !anchored => {
                anchored = true;
                let e = name_end(v, 1);
                anchors.insert(cs(v, 1, e));
                e
            }
            Some('!') if !tagged => {
                tagged = true;
                let e = name_end(v, 1);
                const KNOWN: &[&str] = &["!", "!!str", "!!int", "!!float", "!!bool", "!!null", "!!binary", "!!timestamp", "!!map", "!!seq", "!!set", "!!omap", "!!pairs"];
                if !KNOWN.contains(&cs(v, 0, e).as_str()) {
                    return Err(());
                }
                e
            }
            Some('&' | '!') => return Err(()),
            _ => break,
        };
        let rest = &v[e..];
        v = &rest[rest.iter().take_while(|c| **c == ' ' || **c == '\t').count()..];
    }
    let Some(&c) = v.first() else { return Ok(Some(YamlAfter::Nested(ind))) };
    let name_end = |from: usize| name_end(v, from);
    match c {
        '#' => Ok(Some(YamlAfter::Nested(ind))),
        '"' => {
            let mut j = 1;
            while j < v.len() && v[j] != '"' {
                j += if v[j] == '\\' { 2 } else { 1 };
            }
            if j >= v.len() {
                return Ok(None);
            }
            if yaml_double(&cs(v, 1, j)).is_none() || !only_comment(&v[j + 1..]) {
                return Err(());
            }
            Ok(Some(YamlAfter::Closed))
        }
        '\'' => {
            let mut j = 1;
            loop {
                match (j..v.len()).find(|&k| v[k] == '\'') {
                    None => return Ok(None),
                    Some(k) if v.get(k + 1) == Some(&'\'') => j = k + 2,
                    Some(k) => {
                        j = k;
                        break;
                    }
                }
            }
            if !only_comment(&v[j + 1..]) {
                return Err(());
            }
            Ok(Some(YamlAfter::Closed))
        }
        '|' | '>' => {
            // [+-]?[1-9]? or [1-9][+-]?, then blanks and a comment at most
            let mut j = 1;
            let sign = |c: Option<&char>| matches!(c, Some('+' | '-'));
            let digit = |c: Option<&char>| c.is_some_and(|c| ('1'..='9').contains(c));
            if sign(v.get(j)) {
                j += 1;
                if digit(v.get(j)) {
                    j += 1;
                }
            } else if digit(v.get(j)) {
                j += 1;
                if sign(v.get(j)) {
                    j += 1;
                }
            }
            if !only_comment(&v[j..]) {
                return Err(());
            }
            Ok(Some(YamlAfter::Block(ind)))
        }
        '[' | '{' => match flow_end(v) {
            Some(e) if only_comment(&v[e..]) => Ok(Some(YamlAfter::Closed)),
            Some(_) => Err(()),
            None => Ok(None),
        },
        '*' => {
            let e = name_end(1);
            if !anchors.contains(&cs(v, 1, e)) || !only_comment(&v[e..]) {
                return Err(());
            }
            Ok(Some(YamlAfter::Closed))
        }
        '@' | '`' | '%' | ']' | '}' | ',' => Err(()),
        '-' | '?' | ':' if v.get(1).is_none_or(|c| sp(*c)) => Err(()),
        _ => {
            let end = comment_at(v);
            // PyYAML refuses a comment that a tab separates from a plain scalar
            if end < v.len() && v[end - 1] == '\t' {
                return Err(());
            }
            let body = &v[..end];
            let mut stop = body.len();
            while stop > 0 && sp(body[stop - 1]) {
                stop -= 1;
            }
            let body = &body[..stop];
            if (0..body.len()).any(|k| body[k] == ':' && body.get(k + 1).is_none_or(|c| sp(*c))) {
                return Err(());
            }
            Ok(Some(if end < v.len() { YamlAfter::Closed } else { YamlAfter::Plain(ind) }))
        }
    }
}

/// `key:` at the start of `v` (after `ind` blanks): the char offset of the value (after the blanks that follow the
/// colon). `None` when the line is not a key line; `Some(None)` for a key this check does not follow.
fn yaml_key_line(v: &[char], ind: usize) -> Option<Option<usize>> {
    let first = *v.get(ind)?;
    if matches!(first, '[' | '{' | '*' | '&' | '!' | '|' | '>' | '%' | '@' | '`' | ',' | '?') {
        return Some(None);
    }
    let mut k = ind;
    if first == '"' || first == '\'' {
        k += 1;
        while k < v.len() && v[k] != first {
            k += if first == '"' && v[k] == '\\' { 2 } else { 1 };
        }
        k += 1;
        while k < v.len() && (v[k] == ' ' || v[k] == '\t') {
            k += 1;
        }
        if v.get(k) != Some(&':') {
            return None;
        }
    } else {
        loop {
            let c = *v.get(k)?;
            if c == ':' && v.get(k + 1).is_none_or(|c| sp(*c)) {
                break;
            }
            if c == '#' && k > ind && sp(v[k - 1]) {
                return None;
            }
            k += 1;
        }
    }
    let mut j = k + 1;
    while j < v.len() && (v[j] == ' ' || v[j] == '\t') {
        j += 1;
    }
    Some(Some(j))
}

/// Whether `yaml.safe_load` reads the front matter block: a check of the mistakes PyYAML refuses (a `: ` or a
/// bad start in a plain value, text after a quoted one, a block scalar header with text, an unknown alias or local
/// tag, a line that is not a key, more indented text after a closed value, tab indentation). Constructs it does
/// not follow (multi-line quoted and flow values, nested blocks) are taken as readable.
fn yaml_block_ok(body: &[&str]) -> bool {
    let mut anchors = std::collections::HashSet::new();
    let mut after = YamlAfter::Closed;
    // a block scalar's text column: its first line's
    let mut block_col: Option<usize> = None;
    for line in body {
        let v = chars(line);
        if py_strip(line).is_empty() {
            continue;
        }
        let ind = v.iter().take_while(|c| **c == ' ').count();
        if let YamlAfter::Block(p) = after {
            if ind > p {
                match block_col {
                    None => block_col = Some(ind),
                    Some(c) if ind < c => return false,
                    Some(_) => {}
                }
                continue;
            }
        }
        block_col = None;
        if v.get(ind) == Some(&'\t') {
            return false;
        }
        if v.get(ind) == Some(&'#') {
            if let YamlAfter::Plain(_) = after {
                after = YamlAfter::Closed;
            }
            continue;
        }
        if ind > 0 {
            match after {
                YamlAfter::Plain(p) if ind > p => {
                    let end = comment_at(&v);
                    let body = &v[ind..end];
                    if (0..body.len()).any(|k| body[k] == ':' && body.get(k + 1).is_none_or(|c| sp(*c))) {
                        return false;
                    }
                    if end < v.len() {
                        after = YamlAfter::Closed;
                    }
                    continue;
                }
                // a nested block or sequence: not followed further
                YamlAfter::Nested(p) | YamlAfter::Items(p) if ind > p => return true,
                _ => return false,
            }
        }
        let is_item = v[0] == '-' && v.get(1).is_none_or(|c| sp(*c));
        if is_item {
            match after {
                YamlAfter::Nested(0) | YamlAfter::Items(0) => {
                    after = YamlAfter::Items(0);
                    continue;
                }
                _ => return false,
            }
        }
        match yaml_key_line(&v, 0) {
            None => return false,
            Some(None) => return true,
            Some(Some(at)) => match yaml_value(&v[at..], 0, &mut anchors) {
                Err(()) => return false,
                Ok(None) => return true,
                Ok(Some(a)) => after = a,
            },
        }
    }
    true
}

/// A TOML value that is not a string, array or table: a boolean, a number or a date/time (one token, or a date
/// and a time with one space between them).
fn toml_scalar(t: &str) -> bool {
    if matches!(t, "true" | "false") {
        return true;
    }
    let digits = |s: &str, ok: fn(char) -> bool| -> bool {
        // digits with single underscores between them
        !s.is_empty() && !s.starts_with('_') && !s.ends_with('_') && !s.contains("__") && s.chars().all(|c| c == '_' || ok(c))
    };
    let unsigned = t.strip_prefix(['+', '-']).unwrap_or(t);
    if matches!(unsigned, "inf" | "nan") {
        return true;
    }
    if t == unsigned {
        if let Some(h) = t.strip_prefix("0x") {
            return digits(h, |c| c.is_ascii_hexdigit());
        }
        if let Some(o) = t.strip_prefix("0o") {
            return digits(o, |c| ('0'..='7').contains(&c));
        }
        if let Some(b) = t.strip_prefix("0b") {
            return digits(b, |c| c == '0' || c == '1');
        }
    }
    let dec = |s: &str| digits(s, |c| c.is_ascii_digit()) && (s == "0" || !s.starts_with('0'));
    // integer or float: int part, optional .frac, optional exponent
    let (mant, exp) = match unsigned.find(['e', 'E']) {
        Some(i) => (&unsigned[..i], Some(&unsigned[i + 1..])),
        None => (unsigned, None),
    };
    let (int, frac) = match mant.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (mant, None),
    };
    let exp_ok = exp.is_none_or(|e| digits(e.strip_prefix(['+', '-']).unwrap_or(e), |c| c.is_ascii_digit()));
    if dec(int) && frac.is_none_or(|f| digits(f, |c| c.is_ascii_digit())) && exp_ok {
        return true;
    }
    // date, time or date-time
    let b = t.as_bytes();
    let n = |s: &[u8]| s.iter().all(|c| c.is_ascii_digit());
    let date = |s: &[u8]| s.len() == 10 && n(&s[..4]) && s[4] == b'-' && n(&s[5..7]) && s[7] == b'-' && n(&s[8..10]);
    let time = |s: &[u8]| -> bool {
        if s.len() < 8 || !(n(&s[..2]) && s[2] == b':' && n(&s[3..5]) && s[5] == b':' && n(&s[6..8])) {
            return false;
        }
        let mut r = &s[8..];
        if let Some(f) = r.strip_prefix(b".") {
            let d = f.iter().take_while(|c| c.is_ascii_digit()).count();
            if d == 0 {
                return false;
            }
            r = &f[d..];
        }
        r.is_empty() || r == b"Z" || r == b"z" || (r.len() == 6 && matches!(r[0], b'+' | b'-') && n(&r[1..3]) && r[3] == b':' && n(&r[4..6]))
    };
    if date(b) || time(b) {
        return true;
    }
    b.len() > 11 && date(&b[..10]) && matches!(b[10], b'T' | b't' | b' ') && time(&b[11..])
}

/// Whether `tomllib.loads` reads the front matter block: every line a comment, a table header declared once, or
/// `key = value` with a key its table has not set yet and a value TOML reads (with nothing but a comment after
/// it). A multi-line string or array is taken as readable.
fn toml_block_ok(body: &[&str]) -> bool {
    use std::collections::HashSet;
    let mut tables: HashSet<String> = HashSet::new();
    let mut scope = String::new();
    let mut keys: HashSet<(String, String)> = HashSet::new();
    let mut arrays = 0usize;
    for line in body {
        let t = py_strip(line);
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let v = chars(t);
        if t.starts_with("[[") {
            let Some(e) = t.find("]]") else { return false };
            if !only_comment(&chars(&t[e + 2..])) {
                return false;
            }
            arrays += 1;
            scope = format!("[[{}]]#{arrays}", t[2..e].trim());
            continue;
        }
        if t.starts_with('[') {
            let Some(e) = t.find(']') else { return false };
            let name = t[1..e].trim().to_string();
            if !only_comment(&chars(&t[e + 1..])) || !tables.insert(name.clone()) {
                return false;
            }
            scope = name;
            continue;
        }
        // the key: up to the first `=` outside quotes
        let mut k = 0;
        while k < v.len() && v[k] != '=' {
            if v[k] == '"' || v[k] == '\'' {
                let q = v[k];
                k += 1;
                while k < v.len() && v[k] != q {
                    k += if q == '"' && v[k] == '\\' { 2 } else { 1 };
                }
            }
            k += 1;
        }
        if k >= v.len() {
            return false;
        }
        let key: String = cs(&v, 0, k).trim().to_string();
        let bare = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        let key_ok = key.split('.').all(|p| {
            let p = p.trim();
            bare(p) || (p.len() >= 2 && ((p.starts_with('"') && p.ends_with('"')) || (p.starts_with('\'') && p.ends_with('\''))))
        });
        if !key_ok || !keys.insert((scope.clone(), key)) {
            return false;
        }
        let mut j = k + 1;
        while j < v.len() && (v[j] == ' ' || v[j] == '\t') {
            j += 1;
        }
        let val = &v[j..];
        let Some(&q) = val.first() else { return false };
        let tail_from = match q {
            '"' | '\'' if val.get(1) == Some(&q) && val.get(2) == Some(&q) => {
                match (3..val.len().saturating_sub(2)).find(|&e| val[e] == q && val[e + 1] == q && val[e + 2] == q) {
                    // the closing run may be one or two quotes longer (they belong to the string)
                    Some(e) => {
                        let mut end = e + 3;
                        while end < e + 5 && val.get(end) == Some(&q) {
                            end += 1;
                        }
                        end
                    }
                    None => return true,
                }
            }
            '"' => {
                let mut e = 1;
                while e < val.len() && val[e] != '"' {
                    e += if val[e] == '\\' { 2 } else { 1 };
                }
                if e >= val.len() || toml_string(&cs(val, 1, e), true).is_none() {
                    return false;
                }
                e + 1
            }
            '\'' => match (1..val.len()).find(|&e| val[e] == '\'') {
                Some(e) => e + 1,
                None => return false,
            },
            '[' | '{' => match flow_end(val) {
                Some(e) => e,
                None => return true,
            },
            _ => {
                let end = (0..val.len()).find(|&e| val[e] == '#').unwrap_or(val.len());
                let token: String = cs(val, 0, end).trim_end().to_string();
                if !toml_scalar(&token) {
                    return false;
                }
                end
            }
        };
        if !only_comment(&val[tail_from.min(val.len())..]) {
            return false;
        }
    }
    true
}

/// A plain YAML value that goes on over the next line(s): nothing but blanks after it on its line, and the next
/// non-blank line is indented and not a comment. The whole block then reads the key as the folded text, not
/// the first line, so the key is left alone (LocOS: `data.get(key) != val`).
fn plain_continues(next: &[&str], tail: &[char]) -> bool {
    if tail.iter().any(|c| !sp(*c)) {
        return false;
    }
    next.iter()
        .find(|l| !py_strip(l).is_empty())
        .is_some_and(|l| l.starts_with([' ', '\t']) && !py_lstrip(l).starts_with('#'))
}

/// LocOS `find_front_matter` with the allowlist `keys` (lower case: `markdown.fm_keys`, else LocOS's defaults). The whole block is checked for what PyYAML / tomllib refuse
/// (`yaml_block_ok`, `toml_block_ok`), not parsed: a key's last top-level one-line value stands for what the block
/// gives it (YAML keeps the last of a repeated key), and TOML keys after
/// the first table header are not top-level. A repeated TOML key makes the block unreadable (no fields).
fn find_front_matter(lines: &[&str], keys: &[String]) -> Option<FrontMatter> {
    let first = lines.first()?.trim_start_matches('\u{FEFF}').trim_end_matches([' ', '\t']);
    let toml = match first {
        "---" => false,
        "+++" => true,
        _ => return None,
    };
    let closers: &[&str] = if toml { &["+++"] } else { &["---", "..."] };
    let close = (1..lines.len()).find(|&i| closers.contains(&lines[i].trim_end_matches([' ', '\t'])))?;
    let body = &lines[1..close];
    let content: Vec<&&str> = body.iter().filter(|l| !py_strip(l).is_empty() && !py_lstrip(l).starts_with('#')).collect();
    let looks_like = content.first().is_none_or(|l| keyish(l, toml));
    let mut fm = FrontMatter { toml, end: close + 1, fields: Vec::new() };
    let size: usize = body.iter().map(|l| l.chars().count() + 1).sum::<usize>().saturating_sub(1);
    if size > FM_MAX_CHARS {
        return looks_like.then_some(fm);
    }
    if content.is_empty() {
        return Some(fm);
    }
    if !looks_like {
        return None;
    }
    // a block yaml.safe_load / tomllib refuses is front matter left untouched
    if !(if toml { toml_block_ok(body) } else { yaml_block_ok(body) }) {
        return Some(fm);
    }
    // the last top-level value of every key (TOML: before the first table)
    let mut last: HashMap<String, Option<String>> = HashMap::new();
    let mut toml_seen: HashMap<String, usize> = HashMap::new();
    let mut top_end = close;
    for (i, line) in lines.iter().enumerate().take(close).skip(1) {
        let v = chars(line);
        if toml && py_lstrip(line).starts_with('[') {
            top_end = i;
            break;
        }
        let Some((key, at)) = fm_key(&v, toml) else {
            continue;
        };
        if toml {
            *toml_seen.entry(key.clone()).or_default() += 1;
        }
        let val = (if toml { toml_span(&v, at) } else { yaml_span(&v, at) })
            .filter(|(_, style)| !(*style == "plain" && "&!*|>".contains(v[at])))
            .filter(|(end, style)| !(!toml && *style == "plain" && plain_continues(&lines[i + 1..close], &v[*end..])))
            .and_then(|(end, style)| scalar_value(toml, style, &cs(&v, at, end)));
        last.insert(key, val);
    }
    if toml && toml_seen.values().any(|n| *n > 1) {
        return Some(fm);
    }
    for (i, line) in lines.iter().enumerate().take(top_end).skip(1) {
        let v = chars(line);
        let Some((key, at)) = fm_key(&v, toml) else {
            continue;
        };
        let lower = key.to_lowercase();
        if !keys.contains(&lower) {
            continue;
        }
        let Some((end, style)) = (if toml { toml_span(&v, at) } else { yaml_span(&v, at) }) else {
            continue;
        };
        if style == "plain" && "&!*|>".contains(v[at]) {
            continue;
        }
        let Some(val) = scalar_value(toml, style, &cs(&v, at, end)) else {
            continue;
        };
        if !offerable(&val) || last.get(&key).cloned().flatten().as_deref() != Some(val.as_str()) {
            continue;
        }
        fm.fields.push(FmField { line: i, start: at, end, style, text: val });
    }
    Some(fm)
}

fn dq_escape(s: &str, toml: bool) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        let o = ch as u32;
        if ch == '\\' {
            out.push_str("\\\\");
        } else if ch == '"' {
            out.push_str("\\\"");
        } else if o < 0x20 || o == 0x7f || (!toml && (matches!(o, 0x85 | 0xFEFF | 0x2028 | 0x2029) || (0x80..0xa0).contains(&o))) {
            if o <= 0xffff {
                out.push_str(&format!("\\u{o:04x}"));
            } else {
                out.push_str(&format!("\\U{o:08x}"));
            }
        } else {
            out.push(ch);
        }
    }
    out.push('"');
    out
}

/// LocOS `encode_fm_value`.
fn encode_fm_value(toml: bool, style: &str, new: &str) -> Option<String> {
    let new = fold_breaks(new, false);
    let reads = |c: &str, st: &str| scalar_value(toml, st, c).as_deref() == Some(new.as_str());
    if toml {
        let mut cands: Vec<(String, &str)> = Vec::new();
        if style == "literal" {
            cands.push((format!("'{new}'"), "literal"));
        }
        cands.push((dq_escape(&new, true), "basic"));
        return cands.into_iter().find(|(c, st)| reads(c, st)).map(|c| c.0);
    }
    let mut cands: Vec<(String, &str)> = Vec::new();
    if style == "plain" {
        cands.push((new.clone(), "plain"));
    }
    if style == "plain" || style == "single" {
        cands.push((format!("'{}'", new.replace('\'', "''")), "single"));
    }
    cands.push((dq_escape(&new, false), "double"));
    cands.into_iter().find(|(c, st)| reads(c, st)).map(|c| c.0)
}

// ---- block structure ------------------------------------------------------------------------------

/// (column, char offset) of the first character after the leading blanks; a BOM takes no column.
fn indent(v: &[char]) -> (usize, usize) {
    let mut i = usize::from(v.first() == Some(&'\u{FEFF}'));
    let mut col = 0;
    while i < v.len() && (v[i] == ' ' || v[i] == '\t') {
        col = if v[i] == '\t' { col + 4 - col % 4 } else { col + 1 };
        i += 1;
    }
    (col, i)
}

/// `[-*_=]{3,}|[\s:|.-]+` over the stripped line.
fn is_skip_line(line: &str) -> bool {
    let s = py_strip(line);
    if s.is_empty() {
        return false;
    }
    (s.chars().count() >= 3 && s.chars().all(|c| matches!(c, '-' | '*' | '_' | '='))) || s.chars().all(|c| sp(c) || matches!(c, ':' | '|' | '.' | '-'))
}

/// `^(\s*(?:#{1,6}\s+|[-*+]\s+|\d+[.)]\s+|>\s+)*)`: the markup prefix's length in chars.
fn prefix_len(v: &[char]) -> usize {
    let mut i = 0;
    while i < v.len() && sp(v[i]) {
        i += 1;
    }
    loop {
        let blanks = |j: usize| v[j.min(v.len())..].iter().take_while(|c| sp(**c)).count();
        let start = i;
        let n = v[i..].iter().take_while(|c| **c == '#').count();
        if (1..=6).contains(&n) && blanks(i + n) > 0 {
            i += n + blanks(i + n);
            continue;
        }
        if v.get(i).is_some_and(|c| matches!(c, '-' | '*' | '+')) && blanks(i + 1) > 0 {
            i += 1 + blanks(i + 1);
            continue;
        }
        let d = v[i..].iter().take_while(|c| py_isdecimal_char(**c)).count();
        if d > 0 && v.get(i + d).is_some_and(|c| matches!(c, '.' | ')')) && blanks(i + d + 1) > 0 {
            i += d + 1 + blanks(i + d + 1);
            continue;
        }
        if v.get(i) == Some(&'>') && blanks(i + 1) > 0 {
            i += 1 + blanks(i + 1);
            continue;
        }
        if i == start {
            return i;
        }
    }
}

/// `\[\^[^\]\s]+\]:[ \t]*` at `p`: its end.
fn footnote_def(v: &[char], p: usize) -> Option<usize> {
    if v.get(p) != Some(&'[') || v.get(p + 1) != Some(&'^') {
        return None;
    }
    let n = v[p + 2..].iter().take_while(|c| **c != ']' && !sp(**c)).count();
    if n == 0 || v.get(p + 2 + n) != Some(&']') || v.get(p + 3 + n) != Some(&':') {
        return None;
    }
    let mut e = p + 4 + n;
    while e < v.len() && (v[e] == ' ' || v[e] == '\t') {
        e += 1;
    }
    Some(e)
}

/// The content column of a list item opening at `p` (column `col`).
fn list_content_col(line: &str, v: &[char], col: usize, p: usize) -> Option<usize> {
    let marker = if v.get(p).is_some_and(|c| matches!(c, '-' | '*' | '+')) {
        1
    } else {
        let d = v[p.min(v.len())..].iter().take_while(|c| py_isdecimal_char(**c)).count();
        if !(1..=9).contains(&d) || !v.get(p + d).is_some_and(|c| matches!(c, '.' | ')')) {
            return None;
        }
        d + 1
    };
    let g = v[p + marker..].iter().take_while(|c| **c == ' ' || **c == '\t').count();
    if g == 0 && p + marker < v.len() {
        return None;
    }
    if is_skip_line(line) {
        return None;
    }
    let mut gap = 0;
    for c in &v[p + marker..p + marker + g] {
        gap = if *c == '\t' { gap + 4 - gap % 4 } else { gap + 1 };
    }
    Some(col + marker + if (1..=4).contains(&gap) { gap } else { 1 })
}

/// `(`{3,})[^`]*$|(~{3,})` at `p`: the fence (char, length).
fn fence_open(v: &[char], p: usize) -> Option<(char, usize)> {
    let b = v[p.min(v.len())..].iter().take_while(|c| **c == '`').count();
    if b >= 3 && !v[p + b..].contains(&'`') {
        return Some(('`', b));
    }
    let t = v[p.min(v.len())..].iter().take_while(|c| **c == '~').count();
    (t >= 3).then_some(('~', t))
}

/// `(`{3,}|~{3,})[ \t]*$` at `p`.
fn fence_close(v: &[char], p: usize) -> Option<(char, usize)> {
    let c = *v.get(p)?;
    if c != '`' && c != '~' {
        return None;
    }
    let n = v[p..].iter().take_while(|x| **x == c).count();
    (n >= 3 && v[p + n..].iter().all(|x| *x == ' ' || *x == '\t')).then_some((c, n))
}

/// HTML comment blocks (CommonMark 4.6, type 2) that hold nothing but comments.
struct CommentBlocks<'a> {
    lines: &'a [&'a str],
    next: Option<Vec<Option<usize>>>,
    tail_ok: HashMap<usize, bool>,
}

fn only_comments(s: &str, mut pos: usize) -> bool {
    let b = s.as_bytes();
    loop {
        while pos < b.len() && matches!(b[pos], b' ' | b'\t' | b'\n') {
            pos += 1;
        }
        if pos >= b.len() {
            return true;
        }
        if !s[pos..].starts_with("<!--") {
            return false;
        }
        match s[pos + 2..].find("-->") {
            Some(c) => pos = pos + 2 + c + 3,
            None => return false,
        }
    }
}

impl CommentBlocks<'_> {
    fn end(&mut self, i: usize) -> Option<usize> {
        let line = self.lines[i];
        let v = chars(line);
        let (col, p) = indent(&v);
        let pb: usize = v[..p].iter().map(|c| c.len_utf8()).sum();
        if col > 3 || !line[pb..].starts_with("<!--") {
            return None;
        }
        if line[pb + 2..].contains("-->") {
            return only_comments(line, pb).then_some(i);
        }
        if self.next.is_none() {
            let mut nxt = vec![None; self.lines.len() + 1];
            let mut acc = None;
            for k in (0..self.lines.len()).rev() {
                if self.lines[k].contains("-->") {
                    acc = Some(k);
                }
                nxt[k] = acc;
            }
            self.next = Some(nxt);
        }
        let j = self.next.as_ref().and_then(|n| n.get(i + 1).copied().flatten())?;
        let ok = *self.tail_ok.entry(j).or_insert_with(|| {
            let t = self.lines[j];
            only_comments(t, t.find("-->").map_or(0, |x| x + 3))
        });
        ok.then_some(j)
    }
}

/// A link title's span: (line index, char start, char end, delimiter).
type Title = (usize, usize, usize, char);

fn title_end(s: &[char], k: usize) -> Option<usize> {
    let close = match s.get(k)? {
        '"' => '"',
        '\'' => '\'',
        '(' => ')',
        _ => return None,
    };
    let mut e = k + 1;
    while e < s.len() {
        let c = s[e];
        if c == '\\' {
            e += 2;
            continue;
        }
        if c == close {
            return Some(e + 1);
        }
        if close == ')' && c == '(' {
            return None;
        }
        e += 1;
    }
    None
}

fn ws_once(s: &[char], mut k: usize) -> usize {
    while k < s.len() && (s[k] == ' ' || s[k] == '\t') {
        k += 1;
    }
    if k < s.len() && s[k] == '\n' {
        k += 1;
        while k < s.len() && (s[k] == ' ' || s[k] == '\t') {
            k += 1;
        }
    }
    k
}

fn blank_st(s: &[char]) -> bool {
    s.iter().all(|c| *c == ' ' || *c == '\t')
}

/// LocOS `link_ref_def`: (last line of the definition, its one-line title).
fn link_ref_def(lines: &[&str], i: usize) -> Option<(usize, Option<Title>)> {
    let first = chars(lines[i]);
    let (_, p) = indent(&first);
    if first.get(p) != Some(&'[') {
        return None;
    }
    let mut chunk: Vec<&str> = Vec::new();
    for (j, l) in lines.iter().enumerate().skip(i).take(3) {
        if j > i && py_strip(l).is_empty() {
            break;
        }
        chunk.push(l);
    }
    let s = chars(&chunk.join("\n"));
    let mut starts = vec![0usize];
    for l in &chunk[..chunk.len() - 1] {
        starts.push(starts.last().copied().unwrap_or(0) + l.chars().count() + 1);
    }
    let n = s.len();
    let mut k = p + 1;
    while k < n {
        let c = s[k];
        if c == '\\' {
            k += 2;
            continue;
        }
        if c == '[' {
            return None;
        }
        if c == ']' {
            break;
        }
        k += 1;
    }
    if k >= n {
        return None;
    }
    let label = cs(&s, p + 1, k);
    if py_strip(&label).is_empty() || label.chars().count() > 999 || label.starts_with('^') {
        return None;
    }
    if s.get(k + 1) != Some(&':') {
        return None;
    }
    k = ws_once(&s, k + 2);
    if k >= n || s[k] == '\n' {
        return None;
    }
    let mut e;
    if s[k] == '<' {
        e = k + 1;
        while e < n && !matches!(s[e], '<' | '>' | '\n') {
            e += if s[e] == '\\' { 2 } else { 1 };
        }
        if e >= n || s[e] != '>' {
            return None;
        }
        e += 1;
    } else {
        e = k;
        let mut depth = 0i32;
        while e < n {
            let c = s[e];
            if c == '\\' && e + 1 < n && !matches!(s[e + 1], ' ' | '\t' | '\n') {
                e += 2;
                continue;
            }
            if matches!(c, ' ' | '\t' | '\n') || (c as u32) < 0x20 || c == '\u{7f}' {
                break;
            }
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            e += 1;
        }
        if e == k || depth != 0 {
            return None;
        }
    }
    let line_of = |off: usize| starts.partition_point(|&x| x <= off) - 1;
    let done = |t: Option<(usize, usize)>| {
        let last = line_of(e.max(t.map_or(0, |x| x.1)) - 1);
        let title = t.and_then(|(t0, t1)| {
            (line_of(t0) == line_of(t1 - 1)).then(|| {
                let li = line_of(t0);
                (i + li, t0 + 1 - starts[li], t1 - 1 - starts[li], s[t0])
            })
        });
        Some((i + last, title))
    };
    let eol = (e..n).find(|&x| s[x] == '\n').unwrap_or(n);
    let after = &s[e.min(n)..eol];
    if blank_st(after) {
        if eol < n {
            let mut t0 = eol + 1;
            while t0 < n && (s[t0] == ' ' || s[t0] == '\t') {
                t0 += 1;
            }
            if let Some(t1) = title_end(&s, t0) {
                let rest_end = (t1..n).find(|&x| s[x] == '\n').unwrap_or(n);
                if blank_st(&s[t1.min(n)..rest_end]) {
                    return done(Some((t0, t1)));
                }
            }
        }
        return done(None);
    }
    if !matches!(after[0], ' ' | '\t') {
        return None;
    }
    let t0 = e + after.iter().take_while(|c| **c == ' ' || **c == '\t').count();
    let t1 = title_end(&s, t0)?;
    let rest_end = (t1..n).find(|&x| s[x] == '\n').unwrap_or(n);
    if !blank_st(&s[t1.min(n)..rest_end]) {
        return None;
    }
    done(Some((t0, t1)))
}

/// `\\([!-/:-@\[-`{-~])` -> the character.
fn decode_link_title(raw: &str) -> String {
    let v = chars(raw);
    let mut out = String::new();
    let mut i = 0;
    while i < v.len() {
        if v[i] == '\\' && v.get(i + 1).is_some_and(|c| c.is_ascii_punctuation()) {
            out.push(v[i + 1]);
            i += 2;
        } else {
            out.push(v[i]);
            i += 1;
        }
    }
    out
}

fn encode_link_title(new: &str, delim: char) -> String {
    let new = fold_breaks(new, false);
    let special: &[char] = if delim == '(' { &['\\', '(', ')'] } else { &['\\', delim] };
    let mut out = String::new();
    for c in new.chars() {
        if special.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// One unit of the walk: (line, char start, char end, text, prefix, title delimiter or none).
struct Item {
    line: usize,
    start: usize,
    end: usize,
    text: String,
    prefix: String,
    delim: Option<char>,
}

/// LocOS `_iter_md`, walk 3.
fn iter_md(lines: &[&str], start: usize) -> Vec<Item> {
    let mut out = Vec::new();
    let mut comments = CommentBlocks { lines, next: None, tail_ok: HashMap::new() };
    let (mut in_code, mut para) = (false, false);
    let mut fence: Option<(char, usize, usize)> = None;
    let mut stack: Vec<usize> = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let line = lines[i];
        let v = chars(line);
        i += 1;
        let (col, p) = indent(&v);
        if let Some((fch, flen, fbase)) = fence {
            if !py_strip(line).is_empty() && col < fbase {
                fence = None;
            } else {
                if let Some((c, n)) = fence_close(&v, p) {
                    if c == fch && n >= flen && col - fbase < 4 {
                        fence = None;
                    }
                }
                continue;
            }
        }
        if py_strip(line).is_empty() {
            para = false;
            continue;
        }
        if in_code && col.saturating_sub(stack.last().copied().unwrap_or(0)) >= 4 && col >= stack.last().copied().unwrap_or(0) {
            continue;
        }
        in_code = false;
        let item = list_content_col(line, &v, col, p);
        let opens = fence_open(&v, p);
        if !para || item.is_some() || opens.is_some() {
            while stack.last().is_some_and(|&s| col < s) {
                stack.pop();
            }
        }
        let base = stack.last().copied().unwrap_or(0);
        if col >= base && col - base >= 4 {
            if !para {
                in_code = true;
                continue;
            }
        } else if let Some((c, n)) = opens {
            fence = Some((c, n, base));
            para = false;
            continue;
        }
        if is_skip_line(line) {
            para = false;
            continue;
        }
        if let Some(end) = comments.end(i - 1) {
            para = false;
            i = end + 1;
            continue;
        }
        if !para {
            if let Some((end, title)) = link_ref_def(lines, i - 1) {
                if let Some((li, ts, te, delim)) = title {
                    let lv = chars(lines[li]);
                    let text = decode_link_title(&cs(&lv, ts, te));
                    if py_has_alnum(&text) {
                        out.push(Item { line: li, start: ts, end: te, text, prefix: cs(&lv, 0, ts), delim: Some(delim) });
                    }
                }
                i = end + 1;
                continue;
            }
        }
        let mut plen = prefix_len(&v);
        let heading = v[..plen].contains(&'#');
        if let Some(it) = item {
            stack.push(it);
        }
        if let Some(fe) = footnote_def(&v, plen) {
            plen = fe;
            stack.push(col + 4);
        }
        let mut cend = v.len();
        while cend > plen && sp(v[cend - 1]) {
            cend -= 1;
        }
        let core = cs(&v, plen, cend);
        let has_text = !py_strip(&core).is_empty();
        para = !heading && has_text;
        if has_text && py_has_alnum(&core) {
            out.push(Item { line: i - 1, start: plen, end: cend, text: core, prefix: cs(&v, 0, plen), delim: None });
        }
    }
    out
}

/// `!\[([^\[\]]*)\]\s*[(\[]([^)\]]*)[)\]]` somewhere in the text.
fn has_image(s: &str) -> bool {
    let v = chars(s);
    for a in 0..v.len() {
        if v[a] != '!' || v.get(a + 1) != Some(&'[') {
            continue;
        }
        let mut j = a + 2;
        while j < v.len() && v[j] != '[' && v[j] != ']' {
            j += 1;
        }
        if v.get(j) != Some(&']') {
            continue;
        }
        j += 1;
        while j < v.len() && sp(v[j]) {
            j += 1;
        }
        if !v.get(j).is_some_and(|c| *c == '(' || *c == '[') {
            continue;
        }
        j += 1;
        while j < v.len() && v[j] != ')' && v[j] != ']' {
            j += 1;
        }
        if j < v.len() {
            return true;
        }
    }
    false
}

// ---- build repairs (md_emphasis) -----------------------------------------------------------------

/// LocOS `fold_line_breaks`.
fn fold_line_breaks(src: &str, new: &str) -> String {
    if new.is_empty() || src.contains('\n') || src.contains('\r') {
        return new.to_string();
    }
    fold_breaks(new.trim_matches(['\r', '\n']), true)
}

/// `_block_start`: where a block marker begins (the delimiter for a number), if the text starts with one.
fn block_start(s: &str) -> Option<usize> {
    let v = chars(s);
    let lead = v.iter().take_while(|c| **c == ' ' || **c == '\t').count();
    if lead > 3 {
        return None;
    }
    let ends = |j: usize| v.get(j).is_none_or(|c| *c == ' ' || *c == '\t');
    let d = v[lead..].iter().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&d) && v.get(lead + d).is_some_and(|c| *c == '.' || *c == ')') && ends(lead + d + 1) {
        return Some(lead + d);
    }
    let h = v[lead..].iter().take_while(|c| **c == '#').count();
    if (1..=6).contains(&h) && ends(lead + h) {
        return Some(lead);
    }
    if v.get(lead).is_some_and(|c| matches!(c, '-' | '+' | '*')) && ends(lead + 1) {
        return Some(lead);
    }
    if v.get(lead) == Some(&'>') {
        return Some(lead);
    }
    // a code fence would swallow the rest of the file (LocOS tracker #790)
    let fence = |c: char| v[lead..].iter().take(3).filter(|x| **x == c).count() == 3;
    (fence('`') || fence('~')).then_some(lead)
}

/// LocOS `escape_block_start`.
fn escape_block_start(src: &str, new: &str) -> String {
    // four columns of indent would make the line an indented code block (structure fuzz): the blanks go
    let indent = |s: &str| s.chars().take_while(|c| matches!(c, ' ' | '\t')).fold(0, |w, c| if c == '\t' { w + 4 - w % 4 } else { w + 1 });
    let new = if indent(new) >= 4 && indent(src) < 4 { new.trim_start_matches([' ', '\t']) } else { new };
    match block_start(new) {
        Some(at) if block_start(src).is_none() => {
            let v = chars(new);
            format!("{}\\{}", cs(&v, 0, at), cs(&v, at, v.len()))
        }
        _ => new.to_string(),
    }
}

/// Splits at every `|` not preceded by a backslash.
fn cell_split(s: &str) -> Vec<String> {
    let v = chars(s);
    let mut out = vec![String::new()];
    for (i, c) in v.iter().enumerate() {
        if *c == '|' && (i == 0 || v[i - 1] != '\\') {
            out.push(String::new());
        } else {
            out.last_mut().expect("one cell").push(*c);
        }
    }
    out
}

fn lone_backslashes(s: &str) -> usize {
    let v = chars(s);
    (0..v.len()).filter(|&i| v[i] == '\\' && v.get(i + 1).is_some_and(|c| *c == ' ' || *c == '\t')).count()
}

/// LocOS `restore_cell_escapes` (with `_restore_dropped_pipes`).
fn restore_cell_escapes(src: &str, new: &str) -> String {
    if new.is_empty() || !src.contains("\\|") {
        return new.to_string();
    }
    let missing = src.matches("\\|").count() as i64 - new.matches("\\|").count() as i64;
    let extra = lone_backslashes(new) as i64 - lone_backslashes(src) as i64;
    let mut n = missing.min(extra);
    let mut new = new.to_string();
    if n > 0 {
        let v = chars(&new);
        let mut out = String::new();
        for i in 0..v.len() {
            out.push(v[i]);
            if n > 0 && v[i] == '\\' && v.get(i + 1).is_some_and(|c| *c == ' ' || *c == '\t') {
                out.push('|');
                n -= 1;
            }
        }
        new = out;
    }
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
        let mut cell = t_cells[ti].clone();
        ti += 1;
        let mut lost = sc.matches("\\|").count() as i64 - cell.matches("\\|").count() as i64;
        while lost > 0 && excess > 0 && ti < t_cells.len() {
            cell.push_str("\\|");
            cell.push_str(&t_cells[ti]);
            ti += 1;
            lost -= 1;
            excess -= 1;
        }
        out.push(cell);
    }
    out.extend(t_cells[ti..].iter().cloned());
    out.join("|")
}

// ---- extract and merge ----------------------------------------------------------------------------

/// The one leading U+FEFF (as text) and the rest.
fn split_bom(text: &str) -> (&str, &str) {
    text.split_at(if text.starts_with('\u{FEFF}') { '\u{FEFF}'.len_utf8() } else { 0 })
}

/// The lines (`splitlines()`) of the text.
fn split(text: &str) -> (Vec<&str>, Vec<&str>) {
    let raw = py_splitlines_keepends(text);
    let bare = raw.iter().map(|l| py_split_line_end(l).0).collect();
    (bare, raw)
}

/// The skeleton tag text: the decoder's tag, or `utf-8` when it has none a skeleton line can carry.
fn enc_tag(encoding: &str) -> &str {
    if encoding.is_empty() || encoding.contains('\n') {
        "utf-8"
    } else {
        encoding
    }
}

/// The front matter allowlist a skeleton tag names (`;fm=a:b`, written when `markdown.fm_keys` was set), else
/// LocOS's default keys. A key LocOS can match is ASCII letters, digits, `_` and `-` (its `_YAML_KEY` and
/// `_TOML_KEY`), so the tag keeps only those: a name with any other character never matches a key either way.
fn tag_fm_keys(tag: &str) -> Vec<String> {
    match tag.split(';').skip(1).find_map(|p| p.strip_prefix("fm=")) {
        Some(list) => list.split(':').filter(|k| !k.is_empty()).map(str::to_string).collect(),
        None => FM_KEYS.iter().map(|k| k.to_string()).collect(),
    }
}

fn fm_tag_part(keys: &[String]) -> String {
    let usable: Vec<&str> = keys
        .iter()
        .map(String::as_str)
        .filter(|k| k.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'))
        .collect();
    format!(";fm={}", usable.join(":"))
}

/// `_FENCE` (`^\s*(```|~~~)`), the fence toggle of LocOS's image scan.
fn image_fence(line: &str) -> bool {
    let t = line.trim_start_matches(sp);
    t.starts_with("```") || t.starts_with("~~~")
}

/// `_IMAGE.finditer`: (alt, target) of every `![alt](target)` / `![alt][ref]` in a line.
fn images(s: &str) -> Vec<(String, String)> {
    let v = chars(s);
    let mut out = Vec::new();
    let mut a = 0;
    while a < v.len() {
        match image_at(&v, a) {
            Some((alt, target, end)) => {
                out.push((alt, target));
                a = end;
            }
            None => a += 1,
        }
    }
    out
}

/// One `_IMAGE` match starting at `a`: (alt, target, char offset after the match).
fn image_at(v: &[char], a: usize) -> Option<(String, String, usize)> {
    if v[a] != '!' || v.get(a + 1) != Some(&'[') {
        return None;
    }
    let mut j = a + 2;
    while j < v.len() && v[j] != '[' && v[j] != ']' {
        j += 1;
    }
    if v.get(j) != Some(&']') {
        return None;
    }
    let alt = cs(v, a + 2, j);
    j += 1;
    while j < v.len() && sp(v[j]) {
        j += 1;
    }
    if !v.get(j).is_some_and(|c| *c == '(' || *c == '[') {
        return None;
    }
    let t0 = j + 1;
    let mut k = t0;
    while k < v.len() && v[k] != ')' && v[k] != ']' {
        k += 1;
    }
    (k < v.len()).then(|| (alt, cs(v, t0, k), k + 1))
}

/// LocOS's image inventory (`skeleton["a11y_images"]`): every image outside code fences after the front matter,
/// on the unit `a11y.anchor_for` gives it (the unit on its line, else the nearest one before, else the first),
/// as the unit extras [`profile::A11Y_IMAGES`] (how many) and [`profile::A11Y_EMPTY_ALT`] (the file names of those
/// with an empty alt, one per line). `spans` is (line, unit index) in unit order.
fn note_images(lines: &[&str], start: usize, spans: &[(usize, usize)], units: &mut [Unit]) {
    let mut in_fence = false;
    let mut found: Vec<(usize, String, String)> = Vec::new();
    for (li, line) in lines.iter().enumerate().skip(start) {
        if image_fence(line) {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            found.extend(images(line).into_iter().map(|(alt, target)| (li, alt, target)));
        }
    }
    let mut count: HashMap<usize, usize> = HashMap::new();
    let mut empty: HashMap<usize, Vec<String>> = HashMap::new();
    for (li, alt, target) in found {
        let anchor = spans.iter().find(|(l, _)| *l == li).or_else(|| spans.iter().rev().find(|(l, _)| *l < li)).or(spans.first());
        let Some(&(_, u)) = anchor else { continue };
        *count.entry(u).or_default() += 1;
        if py_strip(&alt).is_empty() {
            let first = py_strip(&target).split(' ').next().unwrap_or("");
            empty.entry(u).or_default().push(first.rsplit('/').next().unwrap_or("").to_string());
        }
    }
    for (u, n) in count {
        units[u].context.extra.insert(profile::A11Y_IMAGES.into(), n.to_string());
    }
    for (u, names) in empty {
        units[u].context.extra.insert(profile::A11Y_EMPTY_ALT.into(), names.join("\n"));
    }
}

/// Extracts a Markdown file the LocOS way (see the module notes).
pub fn extract(text: &str, encoding: &str, opts: &ExtractOptions, warnings: Vec<Warning>) -> Result<Document, Error> {
    let mut full = String::with_capacity(text.len() + 3);
    if encoding.starts_with("utf-8-bom") && !text.starts_with('\u{FEFF}') {
        full.push('\u{FEFF}');
    }
    full.push_str(text);
    // the byte order mark is not part of the first line (LocOS reads and builds with sig=True): the walk starts after it
    let (lines, _) = split(split_bom(&full).1);
    let keys_opt = crate::fm_keys(&opts.format_options);
    let keys = keys_opt.clone().unwrap_or_else(|| FM_KEYS.iter().map(|k| k.to_string()).collect());
    let fm = find_front_matter(&lines, &keys);
    let mut units = Vec::new();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut add = |t: String, image: bool| {
        let mut u = Unit::new(format!("u{}", units.len() + 1), vec![Inline::text(t)]);
        u.context.kind = Some("line".into());
        if image {
            u.context.extra.insert(profile::STYLE.into(), "a11y:md-image".into());
        }
        units.push(u);
    };
    for f in fm.as_ref().map(|f| f.fields.as_slice()).unwrap_or(&[]) {
        spans.push((f.line, spans.len()));
        add(f.text.clone(), false);
    }
    let start = fm.as_ref().map_or(0, |f| f.end);
    for it in iter_md(&lines, start) {
        let image = it.delim.is_none() && has_image(&it.text);
        spans.push((it.line, spans.len()));
        add(it.text, image);
    }
    note_images(&lines, start, &spans, &mut units);
    // the file's skeleton tag, a line break, then the decoded text (a UTF-8 BOM kept in it): the tag is what
    // `source_encoding` reports; the file is written as UTF-8, as LocOS writes it
    // a set allowlist travels in the tag, so the merge walks the file with the keys it was extracted with
    let tag = match &keys_opt {
        Some(k) => format!("{}{}", enc_tag(encoding), fm_tag_part(k)),
        None => enc_tag(encoding).to_string(),
    };
    let mut data = Vec::with_capacity(tag.len() + 1 + full.len());
    data.extend_from_slice(tag.as_bytes());
    data.push(b'\n');
    data.extend_from_slice(full.as_bytes());
    let doc = Document::new("markdown", units, Skeleton::binary(data, bilingua_model::parser_info!())).with_options(opts);
    bilingua_model::guard::finish(doc, warnings, &opts.limits())
}

/// LocOS `build` (Markdown has no language declaration, so `build_localized` is the same).
pub fn merge(doc: &Document, data: &[u8]) -> Result<Vec<u8>, Error> {
    // schema 1 (before 0.9.1) kept the decoded file alone, with no tag line: read it as it was written, so a stored
    // Document still merges and its first line is never taken for a tag
    let (tag, text) = if doc.skeleton.parser().map_or(1, |p| p.schema) < 2 {
        ("utf-8", std::str::from_utf8(data).map_err(|_| Error::Write("markdown: the LocOS skeleton is not UTF-8".into()))?)
    } else {
        bilingua_model::enc::locos_skeleton_parts(data).ok_or_else(|| Error::Write("markdown: not a LocOS profile skeleton".into()))?
    };
    let mut texts: HashMap<usize, String> = HashMap::new();
    for (k, u) in doc.units.iter().enumerate() {
        if u.segments.iter().any(Segment::has_target) {
            texts.insert(k, plain_text(&u.output()));
        }
    }
    let (bom, body) = split_bom(text);
    let (lines, raw) = split(body);
    let mut out: Vec<String> = raw.iter().map(|s| s.to_string()).collect();
    let fm = find_front_matter(&lines, &tag_fm_keys(tag));
    let fields = fm.as_ref().map(|f| f.fields.clone()).unwrap_or_default();
    let toml = fm.as_ref().is_some_and(|f| f.toml);
    for (k, f) in fields.iter().enumerate() {
        if let Some(new) = texts.get(&k).filter(|n| **n != f.text) {
            if let Some(val) = encode_fm_value(toml, f.style, new) {
                let v = chars(lines[f.line]);
                out[f.line] = format!("{}{val}{}{}", cs(&v, 0, f.start), cs(&v, f.end, v.len()), &raw[f.line][lines[f.line].len()..]);
            }
        }
    }
    let off = fields.len();
    for (k, it) in iter_md(&lines, fm.as_ref().map_or(0, |f| f.end)).into_iter().enumerate() {
        let Some(new) = texts.get(&(k + off)) else { continue };
        let v = chars(lines[it.line]);
        let ending = &raw[it.line][lines[it.line].len()..];
        if let Some(delim) = it.delim {
            if *new != it.text {
                out[it.line] = format!("{}{}{}{ending}", cs(&v, 0, it.start), encode_link_title(new, delim), cs(&v, it.end, v.len()));
            }
            continue;
        }
        let tail = cs(&v, it.end, v.len());
        let mut new = new.clone();
        if new != it.text {
            new = fold_line_breaks(&it.text, &new);
            new = crate::locos_emphasis::fix_cjk_emphasis(&it.text, &new);
            new = crate::locos_emphasis::fix_cjk_autolinks(&it.text, &new);
            new = restore_cell_escapes(&it.text, &new);
            // heading text too (beyond LocOS, structure fuzz): `# # w` reads back as the heading `w`, the prefix
            // having taken the translated `# `; escaped, it prints the same and reads back as written
            new = escape_block_start(&it.text, &new);
        }
        out[it.line] = format!("{}{new}{tail}{ending}", it.prefix);
    }
    Ok(format!("{bom}{}", out.concat()).into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_and_skip_lines() {
        assert_eq!(prefix_len(&chars("> - 1. Item")), 7);
        assert_eq!(prefix_len(&chars("####### x")), 0);
        assert!(is_skip_line("| --- | :-: |"));
        assert!(is_skip_line("***"));
        assert!(!is_skip_line("a"));
    }

    #[test]
    fn yaml_scalars() {
        assert_eq!(yaml_plain("Hello world").as_deref(), Some("Hello world"));
        assert!(yaml_plain("yes").is_none() && yaml_plain("2024-01-02").is_none() && yaml_plain("1.5").is_none());
        assert!(yaml_plain("a: b").is_none());
        assert_eq!(yaml_double("a\\tb \\u00e9").as_deref(), Some("a\tb \u{e9}"));
        assert_eq!(yaml_single("it''s").as_deref(), Some("it's"));
        assert_eq!(encode_fm_value(false, "plain", "Hello: world").as_deref(), Some("'Hello: world'"));
    }

    #[test]
    fn front_matter_blocks_read_like_pyyaml_and_tomllib() {
        // each case checked with PyYAML 6 / tomllib of the LocOS venv (refused: false)
        let yaml = |s: &str| yaml_block_ok(&s.split('\n').collect::<Vec<_>>());
        for bad in [
            "a: b: c",
            "a: x\nb",
            "a: x\n  y: z",
            "a: \"q\" x",
            "a: > folded",
            "a: *x",
            "a: !foo x",
            "a: @x",
            "a: - x",
            "a: \"\\q\"",
            "a: x\n- y",
            "a: b\n\tc: d",
            "a: 'x' \n  y",
            "a: |\n  y\n x",
            "a: c\n# x\n  d",
            "a: [b]c",
            "k: *star* (parens): colon",
            "a: x\t#c",
        ] {
            assert!(!yaml(bad), "{bad:?}");
        }
        for good in [
            "a: x\n  y",
            "a:\n  y: z",
            "a: \"q\" # c",
            "a: |",
            "a: |-",
            "a: | # c",
            "a: &x v\nb: *x",
            "a: !!str x",
            "a: [a, b]",
            "a: x #c: d",
            "a:\n- y",
            "a: x\n\n  y",
            "a: |\n  t: z\nb: c",
            "a: 1\na: 2",
            "a: x:y",
            "a: 12:30",
            "a: x\n  - y",
            "a: ! x",
            "a: &x\n  b: c",
            "a: x\n   y\n  z",
        ] {
            assert!(yaml(good), "{good:?}");
        }
        let toml = |s: &str| toml_block_ok(&s.split('\n').collect::<Vec<_>>());
        for bad in ["a = 1\na = 2", "a = x", "a = \"x\" y", "[t]\na = 1\n[t]\nb = 2", "[t]\na = 1\na = 2", "a = 07", "a = 1.", "a = .5", "a = \"\\q\"", "a = 'x' y", "a = 1\nb"] {
            assert!(!toml(bad), "{bad:?}");
        }
        for good in [
            "a = \"x\"",
            "a = 'x'",
            "a = \"\"\"m\"\"\"",
            "a = [1, 2]",
            "a = 1979-05-27",
            "a = true",
            "a = 1_000",
            "a = 0x1F",
            "a = inf",
            "a = +1.5e3",
            "a = \"x\" # c",
            "a = 1\n[t]\na = 2",
            "a.b = 1",
            "\"a b\" = 1",
            "a = {x = 1}",
            "a = 12:30:00",
            "a = 10",
        ] {
            assert!(toml(good), "{good:?}");
        }
    }

    #[test]
    fn link_reference_titles() {
        let lines = ["[install]: https://example.com \"Installation guide\""];
        let (end, title) = link_ref_def(&lines, 0).unwrap();
        assert_eq!(end, 0);
        let (li, a, b, d) = title.unwrap();
        assert_eq!((li, d), (0, '"'));
        assert_eq!(cs(&chars(lines[0]), a, b), "Installation guide");
    }

    #[test]
    fn block_start_is_escaped() {
        assert_eq!(escape_block_start("In 1984. we", "1984. haben"), "1984\\. haben");
        assert_eq!(escape_block_start("- a", "- b"), "- b");
        // a code fence would swallow the rest of the file (#790)
        assert_eq!(escape_block_start("Plain", "```x"), "\\```x");
        assert_eq!(escape_block_start("Plain", " ~~~ x"), " \\~~~ x");
        assert_eq!(escape_block_start("Plain", "``x"), "``x");
        // four columns of indent would be an indented code block
        assert_eq!(escape_block_start("Plain", "    x"), "x");
    }
}
