//! Placeholders inside a decoded message: i18next `{{x}}` and `$t(key)`, vue-i18n and ICU `{x}`, `{'@'}` and `@:key`
//! links, ICU plural / select structure, printf, `${x}`, HTML and Trans tags. Same rules as the LocOS filter.

use bilingua_model::{CodeData, Inline, TagKind};
use regex::Regex;
use std::collections::BTreeMap;
use std::sync::OnceLock;

fn ph_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?x)
              \u{27E6}[^\u{27E6}\u{27E7}]{0,20}\u{27E7}
            | \{\{[^{}]{0,200}\}\}
            | \$t\([^()]{0,200}\)
            | \$\{[^{}]{0,200}\}
            | @(?:\.[A-Za-z]+)?:(?:\{'[^'{}]*'\}|\([\w.\-]+\)|[\w.\-]*\w)
            | \{'[^'{}]*'\}
            | \{\ ?[A-Za-z0-9_$.\-]{1,100}\ ?(?:,\ ?[A-Za-z]{1,30}\ ?(?:,[^{}]{0,200})?)?\}
            | </?[A-Za-z0-9][^<>]{0,500}>
            | %(?:\d{1,3}\$)?[-+0\#']{0,5}\d{0,10}(?:\.\d{1,10})?[bcdieEfFgGosuxX@%]
            | %\([A-Za-z_]\w*\)[sdif]
            ",
        )
        .expect("placeholder pattern")
    })
}

fn icu_hint_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\{\s*[^\s,{}#']+\s*,\s*(?:plural|select|selectordinal)\s*,").expect("icu hint")
    })
}

/// Placeholders that name their argument may move in the translation (LocOS `FREE_POINT_KIND`).
fn is_free(p: &str) -> bool {
    let b = p.as_bytes();
    if p.starts_with("{{")
        || p.starts_with("${")
        || p.starts_with("$t(")
        || p.starts_with('@')
        || p.starts_with("%(")
    {
        return true;
    }
    if b[0] == b'{' {
        let rest = &p[1..];
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        return rest
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.' | '-'));
    }
    if b[0] == b'%' {
        let digits = b[1..].iter().take_while(|c| c.is_ascii_digit()).count();
        return (1..=3).contains(&digits) && b.get(1 + digits) == Some(&b'$');
    }
    false
}

#[derive(PartialEq, Clone, Copy)]
enum K {
    Text,
    Tok,
    Hash,
}

fn is_sp(c: char) -> bool {
    bilingua_model::py_isspace_char(c)
}

struct Icu<'a> {
    s: &'a str,
    out: Vec<(K, String)>,
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    for c in s[i..].chars() {
        if !is_sp(c) {
            break;
        }
        i += c.len_utf8();
    }
    i
}

impl<'a> Icu<'a> {
    fn emit(&mut self, k: K, t: &str) {
        if t.is_empty() {
            return;
        }
        match self.out.last_mut() {
            Some((lk, lt)) if *lk == k && k != K::Hash => lt.push_str(t),
            _ => self.out.push((k, t.to_string())),
        }
    }

    fn message(&mut self, mut i: usize, depth: usize, in_plural: bool) -> Option<usize> {
        let s = self.s;
        let b = s.as_bytes();
        let n = b.len();
        let mut start = i;
        while i < n {
            match b[i] {
                b'{' => {
                    self.emit(K::Text, &s[start..i]);
                    i = self.arg(i, depth)?;
                    start = i;
                }
                b'}' => break,
                b'#' if in_plural => {
                    self.emit(K::Text, &s[start..i]);
                    self.emit(K::Hash, "#");
                    i += 1;
                    start = i;
                }
                b'\'' if i + 1 < n && b[i + 1] == b'\'' => i += 2,
                b'\'' if i + 1 < n && matches!(b[i + 1], b'{' | b'}' | b'#' | b'|') => {
                    i = match s[i + 1..].find('\'') {
                        Some(k) => i + 1 + k + 1,
                        None => n,
                    };
                }
                _ => i += 1,
            }
        }
        self.emit(K::Text, &s[start..i]);
        Some(i)
    }

    /// `\{\s*([^\s,{}#']+)\s*(?:,\s*([A-Za-z]+)\s*)?` at `i`: (end, type).
    fn head(&self, i: usize) -> Option<(usize, Option<&'a str>)> {
        let s: &'a str = self.s;
        let mut j = skip_ws(s, i + 1);
        let name_start = j;
        for c in s[j..].chars() {
            if is_sp(c) || matches!(c, ',' | '{' | '}' | '#' | '\'') {
                break;
            }
            j += c.len_utf8();
        }
        if j == name_start {
            return None;
        }
        j = skip_ws(s, j);
        let after_name = j;
        if s.as_bytes().get(j) == Some(&b',') {
            let k = skip_ws(s, j + 1);
            let mut e = k;
            while e < s.len() && s.as_bytes()[e].is_ascii_alphabetic() {
                e += 1;
            }
            if e > k {
                return Some((skip_ws(s, e), Some(&s[k..e])));
            }
        }
        Some((after_name, None))
    }

    fn variant(&self, j: usize) -> Option<usize> {
        let s = self.s;
        let b = s.as_bytes();
        let k = skip_ws(s, j);
        // alternative 1: =-?\d+(\.\d+)?
        if b.get(k) == Some(&b'=') {
            let mut e = k + 1;
            if b.get(e) == Some(&b'-') {
                e += 1;
            }
            let d = e;
            while e < b.len() && b[e].is_ascii_digit() {
                e += 1;
            }
            if e > d {
                if b.get(e) == Some(&b'.') {
                    let mut f = e + 1;
                    while f < b.len() && b[f].is_ascii_digit() {
                        f += 1;
                    }
                    if f > e + 1 {
                        e = f;
                    }
                }
                let m = skip_ws(s, e);
                if b.get(m) == Some(&b'{') {
                    return Some(m + 1);
                }
            }
        }
        // alternative 2: [^\s{}]+
        let mut e = k;
        for c in s[k..].chars() {
            if is_sp(c) || c == '{' || c == '}' {
                break;
            }
            e += c.len_utf8();
        }
        if e == k {
            return None;
        }
        let m = skip_ws(s, e);
        (b.get(m) == Some(&b'{')).then_some(m + 1)
    }

    fn arg(&mut self, i: usize, depth: usize) -> Option<usize> {
        if depth > 20 {
            return None;
        }
        let s = self.s;
        let b = s.as_bytes();
        let (mut j, typ) = self.head(i)?;
        let Some(typ) = typ else {
            if b.get(j) == Some(&b'}') {
                self.emit(K::Tok, &s[i..j + 1]);
                return Some(j + 1);
            }
            return None;
        };
        if matches!(typ, "plural" | "select" | "selectordinal") {
            if b.get(j) != Some(&b',') {
                return None;
            }
            j += 1;
            // \s*offset\s*:\s*\d+
            let o = skip_ws(s, j);
            if s[o..].starts_with("offset") {
                let c = skip_ws(s, o + 6);
                if b.get(c) == Some(&b':') {
                    let mut e = skip_ws(s, c + 1);
                    let d = e;
                    while e < b.len() && b[e].is_ascii_digit() {
                        e += 1;
                    }
                    if e > d {
                        j = e;
                    }
                }
            }
            let mut tok_start = i;
            let mut variants = 0;
            loop {
                match self.variant(j) {
                    None => {
                        let m = skip_ws(s, j);
                        if b.get(m) == Some(&b'}') && variants > 0 {
                            self.emit(K::Tok, &s[tok_start..m + 1]);
                            return Some(m + 1);
                        }
                        return None;
                    }
                    Some(vend) => {
                        self.emit(K::Tok, &s[tok_start..vend]);
                        j = self.message(vend, depth + 1, typ != "select" || depth > 0)?;
                        if j >= b.len() || b[j] != b'}' {
                            return None;
                        }
                        tok_start = j;
                        j += 1;
                        variants += 1;
                    }
                }
            }
        }
        let k = j + s[j..].find('}')?;
        if s[j..k].contains('{') {
            return None;
        }
        self.emit(K::Tok, &s[i..k + 1]);
        Some(k + 1)
    }
}

/// ICU MessageFormat split into (is-structure, text) pieces, or `None` when the string is not well-formed ICU.
fn icu_split(s: &str) -> Option<Vec<(K, String)>> {
    let mut icu = Icu { s, out: Vec::new() };
    let end = icu.message(0, 0, false)?;
    (end == s.len()).then_some(icu.out)
}

/// The unit's inline content and its hidden codes (tag id to the original placeholder text).
pub fn unit_inlines(value: &str) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    let mut inl: Vec<Inline> = Vec::new();
    let mut codes = BTreeMap::new();
    let triggers = value
        .bytes()
        .any(|b| matches!(b, b'{' | b'$' | b'@' | b'<' | b'%'))
        || value.contains('\u{27E6}');
    if !triggers {
        if !value.is_empty() {
            inl.push(Inline::Text(value.to_string()));
        }
        return (inl, codes);
    }
    let pieces = if icu_hint_re().is_match(value) {
        icu_split(value)
    } else {
        None
    };
    let pieces = pieces.unwrap_or_else(|| vec![(K::Text, value.to_string())]);
    let mut n = 0usize;
    let mut point = |raw: &str, free: bool, inl: &mut Vec<Inline>| {
        n += 1;
        let id = n.to_string();
        codes.insert(
            id.clone(),
            CodeData {
                standalone: Some(raw.to_string()),
                label: Some("placeholder".into()),
                ..Default::default()
            },
        );
        inl.push(if free {
            Inline::free_tag(id, raw)
        } else {
            Inline::tag(id, TagKind::Standalone, raw)
        });
    };
    for (k, chunk) in &pieces {
        if *k != K::Text {
            point(chunk, false, &mut inl);
            continue;
        }
        let mut pos = 0;
        for m in ph_re().find_iter(chunk) {
            if m.start() > pos {
                inl.push(Inline::Text(chunk[pos..m.start()].to_string()));
            }
            point(m.as_str(), is_free(m.as_str()), &mut inl);
            pos = m.end();
        }
        if pos < chunk.len() {
            inl.push(Inline::Text(chunk[pos..].to_string()));
        }
    }
    (bilingua_model::merge_text(inl), codes)
}

/// True when some text outside the placeholders has a letter or digit.
pub fn translatable(inl: &[Inline]) -> bool {
    inl.iter()
        .any(|i| matches!(i, Inline::Text(t) if bilingua_model::py_has_alnum(t)))
}
