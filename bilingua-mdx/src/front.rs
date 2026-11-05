//! Front matter as the LocOS Markdown filter reads it (#731): only the one-line string values of allowlisted
//! keys are units. YAML is checked with a small reader of our own (no YAML library is portable and small); TOML is
//! parsed with the `toml` crate. Where the YAML reader and PyYAML could disagree the block is treated as valid.

use crate::mdline::{has_line_break, line_breaks_to_space};
use crate::pyx::{has_alnum, is_space, trim};
use regex::Regex;
use std::sync::OnceLock;

/// The default allowlist (lower case).
pub const DEFAULT_KEYS: &[&str] = &[
    "title",
    "description",
    "summary",
    "excerpt",
    "subtitle",
    "sidebar_label",
    "pagination_label",
    "linktitle",
];

const FM_MAX_CHARS: usize = 256 * 1024;

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).expect("static pattern"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Yaml,
    Toml,
}

#[derive(Debug, Clone)]
pub(crate) struct FmField {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub style: &'static str,
    pub text: String,
}

#[derive(Debug)]
pub(crate) struct FrontMatter {
    pub kind: Kind,
    /// First line index after the closing fence.
    pub end: usize,
    pub fields: Vec<FmField>,
}

// ---- YAML scalars ---------------------------------------------------------------------------------------------

fn printable(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | ' '..='~' | '\u{85}' | '\u{a0}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn implicit_non_string(t: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    let r = re(
        &R,
        r"^(?:~|null|Null|NULL|<<|=|yes|Yes|YES|no|No|NO|true|True|TRUE|false|False|FALSE|on|On|ON|off|Off|OFF|[-+]?0b[0-1_]+|[-+]?0[0-7_]+|[-+]?(?:0|[1-9][0-9_]*)|[-+]?0x[0-9a-fA-F_]+|[-+]?[1-9][0-9_]*(?::[0-5]?[0-9])+|[-+]?(?:[0-9][0-9_]*)\.[0-9_]*(?:[eE][-+][0-9]+)?|\.[0-9][0-9_]*(?:[eE][-+][0-9]+)?|[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN)|[0-9]{4}-[0-9]{2}-[0-9]{2}|[0-9]{4}-[0-9]{1,2}-[0-9]{1,2}(?:[Tt]|[ \t]+)[0-9]{1,2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]*)?(?:[ \t]*(?:Z|[-+][0-9]{1,2}(?::[0-9]{2})?))?)$",
    );
    r.is_match(t)
}

fn dq_decode(inner: &str) -> Option<String> {
    let mut out = String::new();
    let mut it = inner.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let e = it.next()?;
        let hex = |it: &mut std::str::Chars, n: usize| -> Option<char> {
            let s: String = it.by_ref().take(n).collect();
            if s.chars().count() != n || !s.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            char::from_u32(u32::from_str_radix(&s, 16).ok()?)
        };
        out.push(match e {
            '0' => '\0',
            'a' => '\x07',
            'b' => '\x08',
            't' | '\t' => '\t',
            'n' => '\n',
            'v' => '\x0b',
            'f' => '\x0c',
            'r' => '\r',
            'e' => '\x1b',
            ' ' => ' ',
            '"' => '"',
            '/' => '/',
            '\\' => '\\',
            'N' => '\u{85}',
            '_' => '\u{a0}',
            'L' => '\u{2028}',
            'P' => '\u{2029}',
            'x' => hex(&mut it, 2)?,
            'u' => hex(&mut it, 4)?,
            'U' => hex(&mut it, 8)?,
            _ => return None,
        });
    }
    Some(out)
}

/// What a one-line YAML scalar reads as.
pub(crate) enum Scalar {
    Str(String),
    /// Valid, but a number, boolean, null or date.
    NonStr,
    /// Not valid YAML there (PyYAML raises).
    Err,
}

/// What a one-line YAML scalar (the text after `key: `) reads as, when it is a string.
pub(crate) fn yaml_scalar_value(text: &str) -> Option<String> {
    match scalar(text) {
        Scalar::Str(s) => Some(s),
        _ => None,
    }
}

pub(crate) fn scalar(text: &str) -> Scalar {
    if !text.chars().all(printable) {
        return Scalar::Err;
    }
    let t = trim(text);
    let Some(first) = t.chars().next() else { return Scalar::NonStr };
    let some = |o: Option<String>| o.map_or(Scalar::Err, Scalar::Str);
    match first {
        '"' => {
            let b = t.as_bytes();
            let mut j = 1;
            while j < b.len() {
                match b[j] {
                    b'\\' => j += 2,
                    b'"' => break,
                    _ => j += 1,
                }
            }
            if j >= b.len() {
                return Scalar::Err;
            }
            let rest = &t[j + 1..];
            if !rest.is_empty() && !is_comment(rest) {
                return Scalar::Err;
            }
            some(dq_decode(&t[1..j]))
        }
        '\'' => {
            let mut out = String::new();
            let mut it = t[1..].char_indices().peekable();
            while let Some((k, c)) = it.next() {
                if c == '\'' {
                    if it.peek().is_some_and(|&(_, n)| n == '\'') {
                        out.push('\'');
                        it.next();
                        continue;
                    }
                    let rest = &t[1 + k + 1..];
                    return some((rest.is_empty() || is_comment(rest)).then_some(out));
                }
                out.push(c);
            }
            Scalar::Err
        }
        '[' | '{' | '#' | '&' | '*' | '!' | '|' | '>' => Scalar::NonStr,
        ']' | '}' | ',' | '%' | '@' | '`' => Scalar::Err,
        '-' | '?' | ':' if t.len() == 1 || t[1..].starts_with([' ', '\t']) => Scalar::Err,
        _ => {
            let cut = t.match_indices('#').find(|&(i, _)| i > 0 && matches!(t.as_bytes()[i - 1], b' ' | b'\t')).map_or(t.len(), |(i, _)| i);
            let plain = t[..cut].trim_end_matches([' ', '\t']);
            if plain.contains(": ") || plain.contains(":\t") || plain.ends_with(':') {
                return Scalar::Err;
            }
            if implicit_non_string(plain) {
                Scalar::NonStr
            } else {
                Scalar::Str(plain.to_string())
            }
        }
    }
}

fn is_comment(rest: &str) -> bool {
    // `[ \t]+#.*`
    let r = rest.trim_start_matches([' ', '\t']);
    r.len() < rest.len() && r.starts_with('#')
}

// ---- YAML block (does it read as a mapping, and what are its top-level values) ---------------------------------

pub(crate) struct YamlEntry {
    pub key: String,
    /// The string value when the entry is a one-line string scalar.
    pub value: Option<String>,
}

pub(crate) enum YamlTop {
    Dict(Vec<YamlEntry>),
    NotDict,
}

fn indent_of(l: &str) -> usize {
    l.len() - l.trim_start_matches(' ').len()
}

fn yaml_top(block: &str) -> YamlTop {
    if !block.chars().all(printable) {
        return YamlTop::NotDict;
    }
    let lines: Vec<&str> = block.split('\n').collect();
    let mut entries: Vec<YamlEntry> = Vec::new();
    let mut anchors: Vec<String> = Vec::new();
    #[derive(Clone, Copy)]
    enum Last {
        Plain,
        Quoted,
        Other,
    }
    let mut last = Last::Other;
    let mut nest: Vec<(usize, bool)> = Vec::new();
    let mut nested_ok = false;
    let mut empty_value = false;
    let mut base: Option<usize> = None;
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i].trim_end_matches('\r');
        i += 1;
        let t = trim(l);
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if l.starts_with('\t') {
            return YamlTop::NotDict;
        }
        let ind = indent_of(l);
        let b = *base.get_or_insert(ind);
        if ind < b {
            return YamlTop::NotDict;
        }
        if ind > b {
            // nested content of the previous entry; after a one-line scalar it is a continuation line
            match last {
                Last::Quoted => return YamlTop::NotDict,
                Last::Plain => {
                    if t.contains(": ") || t.ends_with(':') {
                        return YamlTop::NotDict;
                    }
                    if let Some(e) = entries.last_mut() {
                        e.value = None; // the value is the folded multi-line text
                    }
                }
                Last::Other => {
                    if nested_ok && !nested_line(&mut nest, &l[ind..], ind) {
                        return YamlTop::NotDict;
                    }
                    if !nested_ok || l.contains(NESTED_COMPLEX) {
                        nested_ok = false;
                    }
                }
            }
            continue;
        }
        let body = &l[ind..];
        if body == "-" || body.starts_with("- ") {
            // a sequence at the key's own indent is its value; anywhere else it breaks the mapping
            if empty_value && nested_ok {
                if !nested_line(&mut nest, body, ind) {
                    return YamlTop::NotDict;
                }
                continue;
            }
            return YamlTop::NotDict;
        }
        nest = vec![(b, false)];
        nested_ok = true;
        empty_value = false;
        if body.starts_with("? ") {
            continue;
        }
        // key
        let (key, rest) = if body.starts_with('"') || body.starts_with('\'') {
            let q = body.chars().next().unwrap();
            let mut end = None;
            let bb = body.as_bytes();
            let mut j = 1;
            while j < bb.len() {
                if q == '"' && bb[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bb[j] == q as u8 {
                    if q == '\'' && bb.get(j + 1) == Some(&b'\'') {
                        j += 2;
                        continue;
                    }
                    end = Some(j);
                    break;
                }
                j += 1;
            }
            let Some(e) = end else { return YamlTop::NotDict };
            let after = body[e + 1..].trim_start_matches([' ', '\t']);
            let Some(rest) = after.strip_prefix(':') else { return YamlTop::NotDict };
            if !(rest.is_empty() || rest.starts_with(' ')) {
                return YamlTop::NotDict;
            }
            (body[1..e].to_string(), rest)
        } else {
            let mut found = None;
            for (k, _) in body.match_indices(':') {
                let nx = body[k + 1..].chars().next();
                if nx.is_none() || nx == Some(' ') {
                    found = Some(k);
                    break;
                }
            }
            let Some(k) = found else { return YamlTop::NotDict };
            (body[..k].trim_end().to_string(), &body[k + 1..])
        };
        let v = trim(rest);
        let value = if v.is_empty() || v.starts_with('#') {
            None
        } else if v.starts_with('&') || v.starts_with('*') {
            // an anchor may be defined once; an alias must name a defined anchor (PyYAML composer errors)
            let name: String = v[1..].chars().take_while(|c| !is_space(*c) && !matches!(c, ',' | '[' | ']' | '{' | '}')).collect();
            if v.starts_with('&') {
                if anchors.contains(&name) {
                    return YamlTop::NotDict;
                }
                anchors.push(name);
            } else if !anchors.contains(&name) {
                return YamlTop::NotDict;
            }
            None
        } else if v.starts_with('|') || v.starts_with('>') || v.starts_with('!') {
            None
        } else if v.starts_with('[') || v.starts_with('{') {
            // flow collection: skip until balanced (quotes ignored)
            let mut depth: i64 = 0;
            let mut cur = v.to_string();
            loop {
                for c in cur.chars() {
                    match c {
                        '[' | '{' => depth += 1,
                        ']' | '}' => depth -= 1,
                        _ => {}
                    }
                }
                if depth <= 0 || i >= lines.len() {
                    break;
                }
                cur = lines[i].to_string();
                i += 1;
            }
            None
        } else if v.starts_with('"') || v.starts_with('\'') {
            let q = v.chars().next().unwrap();
            if closes(v, q) {
                match scalar(v) {
                    Scalar::Str(s) => Some(s),
                    Scalar::NonStr => None,
                    Scalar::Err => return YamlTop::NotDict,
                }
            } else {
                // a multi-line quoted scalar: skip to the closing quote
                let mut found = false;
                while i < lines.len() {
                    let nl = lines[i];
                    i += 1;
                    let probe = format!("{q}{nl}");
                    if closes(&probe, q) {
                        // the text after the closing quote must be empty or a comment
                        let b = probe.as_bytes();
                        let mut j = 1;
                        loop {
                            if q == '"' && b[j] == b'\\' {
                                j += 2;
                            } else if b[j] == q as u8 {
                                if q == '\'' && b.get(j + 1) == Some(&b'\'') {
                                    j += 2;
                                } else {
                                    break;
                                }
                            } else {
                                j += 1;
                            }
                        }
                        let rest = &probe[j + 1..];
                        if !(trim(rest).is_empty() || is_comment(rest)) {
                            return YamlTop::NotDict;
                        }
                        found = true;
                        break;
                    }
                }
                if !found {
                    return YamlTop::NotDict;
                }
                None
            }
        } else {
            match scalar(v) {
                Scalar::Str(s) => Some(s),
                Scalar::NonStr => None,
                Scalar::Err => return YamlTop::NotDict,
            }
        };
        last = if v.is_empty() || v.starts_with('#') || v.starts_with(['|', '>', '[', '{']) {
            Last::Other
        } else if v.starts_with(['"', '\'']) {
            Last::Quoted
        } else {
            Last::Plain
        };
        empty_value = v.is_empty() || v.starts_with('#');
        nested_ok = empty_value;
        entries.push(YamlEntry { key, value });
    }
    YamlTop::Dict(entries)
}

/// Lines with these characters are not checked for nesting (flow collections, block scalars, quotes, anchors).
const NESTED_COMPLEX: &[char] = &['|', '>', '[', ']', '{', '}', '"', '\'', '&', '*', '!', '?', '\t'];

/// Checks one nested line against the open levels `(indent, is_sequence)`; false when YAML would refuse it.
fn nested_line(nest: &mut Vec<(usize, bool)>, t: &str, ind: usize) -> bool {
    let mut popped = false;
    while nest.last().is_some_and(|&(i, _)| i > ind) {
        nest.pop();
        popped = true;
    }
    let is_seq = |s: &str| s == "-" || s.starts_with("- ");
    match nest.last().copied() {
        Some((i, _)) if i < ind => {
            if popped {
                return false; // dedent to an indent no open level has
            }
            nest.push((ind, is_seq(t)));
        }
        Some((_, true)) if !is_seq(t) => return false,
        Some(_) => {}
        None => nest.push((ind, is_seq(t))),
    }
    // `- key: v` opens a mapping (or a nested sequence) at the column of the text after the dash
    let (mut col, mut rest) = (ind, t);
    while is_seq(rest) {
        let after = rest[1..].trim_start_matches(' ');
        col += rest.len() - after.len();
        rest = after;
        if rest.is_empty() {
            break;
        }
        if is_seq(rest) {
            nest.push((col, true));
        } else if rest.contains(": ") || rest.ends_with(':') {
            nest.push((col, false));
        }
    }
    true
}

fn closes(v: &str, q: char) -> bool {
    let b = v.as_bytes();
    let mut j = 1;
    while j < b.len() {
        if q == '"' && b[j] == b'\\' {
            j += 2;
            continue;
        }
        if b[j] == q as u8 {
            if q == '\'' && b.get(j + 1) == Some(&b'\'') {
                j += 2;
                continue;
            }
            return true;
        }
        j += 1;
    }
    false
}

// ---- line scanners (the `_yaml_span` and `_toml_span` of LocOS) -----------------------------------------------

fn yaml_span(line: &str, pos: usize) -> Option<(usize, &'static str)> {
    let b = line.as_bytes();
    let (end, style) = match b[pos] {
        b'"' => {
            let mut j = pos + 1;
            loop {
                if j >= b.len() {
                    return None;
                }
                match b[j] {
                    b'\\' => j += 2,
                    b'"' => break,
                    _ => j += 1,
                }
            }
            (j + 1, "double")
        }
        b'\'' => {
            let mut j = pos + 1;
            loop {
                let k = line[j.min(line.len())..].find('\'')? + j;
                if b.get(k + 1) == Some(&b'\'') {
                    j = k + 2;
                    continue;
                }
                break (k + 1, "single");
            }
        }
        _ => {
            let r = &line[pos..];
            let stop = r.match_indices('#').find(|&(i, _)| i > 0 && matches!(r.as_bytes()[i - 1], b' ' | b'\t')).map_or(r.len(), |(i, _)| i - 1);
            (pos + r[..stop].trim_end_matches(is_space).len(), "plain")
        }
    };
    let rest = &line[end..];
    if !trim(rest).is_empty() && !is_comment_full(rest) {
        return None;
    }
    Some((end, style))
}

/// `_TRAILING_COMMENT.fullmatch`.
fn is_comment_full(rest: &str) -> bool {
    is_comment(rest)
}

fn toml_span(line: &str, pos: usize) -> Option<(usize, &'static str)> {
    let b = line.as_bytes();
    let q = b[pos];
    if line[pos..].starts_with(&line[pos..pos + 1].repeat(3)) {
        return None;
    }
    let (end, style) = if q == b'"' {
        let mut j = pos + 1;
        loop {
            if j >= b.len() {
                return None;
            }
            match b[j] {
                b'\\' => j += 2,
                b'"' => break,
                _ => j += 1,
            }
        }
        (j + 1, "basic")
    } else {
        (line[pos + 1..].find('\'')? + pos + 2, "literal")
    };
    let rest = &line[end..];
    if !trim(rest).is_empty() && !is_comment(rest) {
        return None;
    }
    Some((end, style))
}

fn offerable(v: &str) -> bool {
    has_alnum(v) && !has_line_break(v)
}

fn key_match(line: &str, kind: Kind) -> Option<(String, usize)> {
    let b = line.as_bytes();
    let mut i = 0;
    match kind {
        Kind::Yaml => {
            if !(b.first().is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')) {
                return None;
            }
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') {
                i += 1;
            }
        }
        Kind::Toml => {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') {
                i += 1;
            }
            if i == 0 {
                return None;
            }
        }
    }
    let key = line[..i].to_string();
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    match kind {
        Kind::Yaml => {
            if b.get(i) != Some(&b':') {
                return None;
            }
            i += 1;
            let s = i;
            while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
                i += 1;
            }
            if i == s {
                return None;
            }
            let nx = line[i..].chars().next()?;
            if is_space(nx) {
                return None;
            }
        }
        Kind::Toml => {
            if b.get(i) != Some(&b'=') {
                return None;
            }
            i += 1;
            while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
                i += 1;
            }
            if !matches!(b.get(i), Some(b'"') | Some(b'\'')) {
                return None;
            }
        }
    }
    Some((key, i))
}

fn keyish(first: &str, kind: Kind) -> bool {
    static Y: OnceLock<Regex> = OnceLock::new();
    static T: OnceLock<Regex> = OnceLock::new();
    match kind {
        Kind::Yaml => re(&Y, r#"^["']?[\w-]+["']?[ \t]*:(?:[ \t]|$)"#).is_match(first),
        Kind::Toml => re(&T, r#"^(?:[\w.\"'-]+[ \t]*=|\[)"#).is_match(first),
    }
}

/// The front matter at the start of `lines` (no line endings), or None. A block that is front matter but cannot be
/// read safely comes back with no fields.
pub(crate) fn find_front_matter(lines: &[&str], keys: &std::collections::HashSet<String>) -> Option<FrontMatter> {
    let first = lines.first()?.trim_start_matches('\u{feff}').trim_end_matches([' ', '\t']);
    let (kind, closers): (Kind, &[&str]) = match first {
        "---" => (Kind::Yaml, &["---", "..."]),
        "+++" => (Kind::Toml, &["+++"]),
        _ => return None,
    };
    let close = (1..lines.len()).find(|&i| closers.contains(&lines[i].trim_end_matches([' ', '\t'])))?;
    let body = &lines[1..close];
    let content: Vec<&&str> = body.iter().filter(|l| !trim(l).is_empty() && !trim_start_hash(l)).collect();
    let looks_like = content.is_empty() || content.first().is_some_and(|l| keyish(l, kind));
    let block = body.join("\n");
    let mut fm = FrontMatter { kind, end: close + 1, fields: Vec::new() };
    if block.chars().count() > FM_MAX_CHARS {
        return if looks_like { Some(fm) } else { None };
    }
    let mut yaml_entries: Vec<YamlEntry> = Vec::new();
    let mut toml_table: Option<toml::Table> = None;
    match kind {
        Kind::Yaml => match yaml_top(&block) {
            YamlTop::Dict(e) => yaml_entries = e,
            YamlTop::NotDict => {
                return if content.is_empty() || looks_like { Some(fm) } else { None };
            }
        },
        Kind::Toml => match toml::from_str::<toml::Table>(&block) {
            Ok(t) => toml_table = Some(t),
            Err(_) => return if looks_like { Some(fm) } else { None },
        },
    }
    for (i, line) in lines.iter().enumerate().take(close).skip(1) {
        let line = *line;
        let Some((key, vs)) = key_match(line, kind) else { continue };
        if !keys.contains(&key.to_lowercase()) {
            continue;
        }
        let span = if kind == Kind::Yaml { yaml_span(line, vs) } else { toml_span(line, vs) };
        let Some((end, style)) = span else { continue };
        if style == "plain" && "&!*|>".contains(line[vs..].chars().next().unwrap_or(' ')) {
            continue;
        }
        let scalar = &line[vs..end];
        let val = if kind == Kind::Yaml {
            let Some(v) = yaml_scalar_value(scalar) else { continue };
            v
        } else {
            let Ok(t) = toml::from_str::<toml::Table>(&line[..end]) else { continue };
            if t.len() != 1 {
                continue;
            }
            match t.get(&key) {
                Some(toml::Value::String(s)) => s.clone(),
                _ => continue,
            }
        };
        if !offerable(&val) {
            continue;
        }
        let effective = if kind == Kind::Yaml {
            yaml_entries.iter().rev().find(|e| e.key == key).and_then(|e| e.value.clone())
        } else {
            match toml_table.as_ref().and_then(|t| t.get(&key)) {
                Some(toml::Value::String(s)) => Some(s.clone()),
                _ => None,
            }
        };
        if effective.as_deref() != Some(val.as_str()) {
            continue;
        }
        fm.fields.push(FmField { line: i, start: vs, end, style, text: val });
    }
    Some(fm)
}

fn trim_start_hash(l: &str) -> bool {
    l.trim_start_matches(is_space).starts_with('#')
}

fn dq_escape(s: &str, toml: bool) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        let o = ch as u32;
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ if o < 0x20 || o == 0x7f || (!toml && (matches!(o, 0x85 | 0xFEFF | 0x2028 | 0x2029) || (0x80..0xA0).contains(&o))) => {
                if o <= 0xFFFF {
                    out.push_str(&format!("\\u{o:04x}"));
                } else {
                    out.push_str(&format!("\\U{o:08x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn yaml_reads(scalar: &str, value: &str) -> bool {
    yaml_scalar_value(scalar).as_deref() == Some(value)
}

fn toml_reads(scalar: &str, value: &str) -> bool {
    toml::from_str::<toml::Table>(&format!("k = {scalar}")).ok().is_some_and(|t| t.len() == 1 && t.get("k").and_then(|v| v.as_str()) == Some(value))
}

/// `new` written as a scalar that reads back as exactly `new` (line breaks folded to a space), or None.
pub(crate) fn encode_value(kind: Kind, style: &str, new: &str) -> Option<String> {
    let new = line_breaks_to_space(new);
    if kind == Kind::Toml {
        let mut c: Vec<String> = Vec::new();
        if style == "literal" {
            c.push(format!("'{new}'"));
        }
        c.push(dq_escape(&new, true));
        return c.into_iter().find(|c| toml_reads(c, &new));
    }
    let mut c: Vec<String> = Vec::new();
    if style == "plain" {
        c.push(new.clone());
    }
    if style == "plain" || style == "single" {
        c.push(format!("'{}'", new.replace('\'', "''")));
    }
    c.push(dq_escape(&new, false));
    c.into_iter().find(|c| yaml_reads(c, &new))
}
