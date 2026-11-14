//! Apple format specifiers and App Intents `${name}` parameters as inline codes, and the LocOS rules for what
//! counts as translatable text.

use bilingua_model::{CodeData, Inline, TagKind, Unit};
use regex::Regex;
use std::sync::OnceLock;

struct Res {
    var: Regex,
    spec: Regex,
    param: Regex,
}

fn res() -> &'static Res {
    static R: OnceLock<Res> = OnceLock::new();
    R.get_or_init(|| Res {
        // `%#@name@` and `%1$#@name@`
        var: Regex::new(r"\A%(?:\d+\$)?#@[A-Za-z_][A-Za-z0-9_]*@").expect("static pattern"),
        // printf and Foundation: positional, flags, width, precision, length, conversion (including `@`)
        spec: Regex::new(r"\A%(?:\d+\$)?[-+ #0']*(?:\d+|\*)?(?:\.(?:\d+|\*))?(?:hh|h|ll|l|q|L|z|t|j)?[@diouxXcCsSpaAeEfFgGD]")
            .expect("static pattern"),
        // App Intents and App Shortcuts parameters: `${applicationName}`, `${country}`
        param: Regex::new(r"\A\$\{[A-Za-z_][A-Za-z0-9_]*\}").expect("static pattern"),
    })
}

/// End of a specifier starting at `at` (a `%`), by the LocOS pattern `APPLE_PH`.
fn spec_end(s: &str, at: usize) -> Option<usize> {
    let rest = &s[at..];
    let r = res();
    if let Some(m) = r.var.find(rest) {
        return Some(at + m.end());
    }
    // `%arg` (String Catalog substitution argument) unless a letter, digit or underscore follows
    if let Some(after) = rest.strip_prefix("%arg") {
        if !after.chars().next().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Some(at + 4);
        }
    }
    r.spec.find(rest).map(|m| at + m.end())
}

/// End of an App Intents parameter `${name}` starting at `at` (a `$`). Not in LocOS, which leaves it as text.
fn param_end(s: &str, at: usize) -> Option<usize> {
    res().param.find(&s[at..]).map(|m| at + m.end())
}

/// Splits a value into text and specifiers. `%%` is a literal percent and never starts a specifier.
pub fn split(s: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let (mut i, mut text_start) = (0, 0);
    while i < b.len() {
        if b[i] == b'%' || (b[i] == b'$' && b.get(i + 1) == Some(&b'{')) {
            if b[i] == b'%' && b.get(i + 1) == Some(&b'%') {
                i += 2;
                continue;
            }
            let end = if b[i] == b'%' { spec_end(s, i) } else { param_end(s, i) };
            if let Some(end) = end {
                if text_start < i {
                    out.push(Piece::Text(&s[text_start..i]));
                }
                out.push(Piece::Ph(&s[i..end]));
                i = end;
                text_start = end;
                continue;
            }
        }
        i += 1;
    }
    if text_start < b.len() {
        out.push(Piece::Text(&s[text_start..]));
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    Text(&'a str),
    Ph(&'a str),
}

/// A positional specifier (`%1$@`) names its argument, so a translation may move it (LocOS token `p`). A
/// `${name}` parameter is named too.
pub fn is_positional(spec: &str) -> bool {
    if spec.starts_with("${") {
        return true;
    }
    let b = spec.as_bytes();
    let mut i = 1;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i > 1 && b.get(i) == Some(&b'$')
}

/// Python `str.isspace` (what LocOS strips).
pub fn is_space(c: char) -> bool {
    bilingua_model::py_isspace_char(c)
}

pub fn split_ws(s: &str) -> (&str, &str, &str) {
    let core = s.trim_matches(is_space);
    if core.is_empty() {
        return (s, "", "");
    }
    let lead = s.len() - s.trim_start_matches(is_space).len();
    let trail = s.trim_end_matches(is_space).len();
    (&s[..lead], core, &s[trail..])
}

/// A letter or digit outside the specifiers (LocOS `is_translatable`). Uses Python's `str.isalnum`
/// exactly (`bilingua_model::py_has_alnum`).
pub fn is_translatable(v: &str) -> bool {
    if !bilingua_model::py_has_alnum(v) {
        return false;
    }
    for p in split(&v.replace("%%", "")) {
        if let Piece::Text(t) = p {
            if bilingua_model::py_has_alnum(t) {
                return true;
            }
        }
    }
    false
}

/// Source inlines for the core text of a value, with the code table (`1`, `2`, ...).
pub fn inlines(core: &str, codes: &mut std::collections::BTreeMap<String, CodeData>) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut n = 0;
    // `%%` pieces stay text: split on specifiers only, `split` already skips `%%`
    for p in split(core) {
        match p {
            Piece::Text(t) => out.push(Inline::text(t)),
            Piece::Ph(s) => {
                n += 1;
                let id = n.to_string();
                codes.insert(id.clone(), CodeData::new().with_standalone(s.to_string()).with_label("placeholder".to_string()));
                out.push(if is_positional(s) { Inline::free_tag(id, s) } else { Inline::tag(id, TagKind::Standalone, s) });
            }
        }
    }
    bilingua_model::merge_text(out)
}

/// The text of inlines with each tag as its original specifier.
pub fn render(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, display, .. } => match u.codes.get(id).and_then(|c| c.standalone.as_deref()) {
                Some(c) => s.push_str(c),
                None => s.push_str(display),
            },
        }
    }
    s
}
