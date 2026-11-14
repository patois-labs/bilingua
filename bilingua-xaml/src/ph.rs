//! Placeholders inside a resource string: `{0}`, `{0:N2}`, `{name}` and printf (`%s`, `%1$d`). LocOS keeps them as
//! plain text; Bilingua makes them tags whose display is the exact source text (label `placeholder`), so a
//! translator cannot break them and the LocOS adapter can flatten them back to the same text.

use regex::Regex;
use std::sync::OnceLock;

pub enum Piece<'a> {
    Text(&'a str),
    Ph(&'a str),
}

fn re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\{[^{}]*\}|%(?:\d+\$)?[-#+0-9.]*(?:hh|ll|[hlqLzjt])?[a-zA-Z@]").expect("placeholder pattern"))
}

pub fn split(s: &str) -> Vec<Piece<'_>> {
    if !s.contains(['{', '%']) {
        return vec![Piece::Text(s)];
    }
    let mut out = Vec::new();
    let mut last = 0;
    for m in re().find_iter(s) {
        if m.start() > last {
            out.push(Piece::Text(&s[last..m.start()]));
        }
        out.push(Piece::Ph(m.as_str()));
        last = m.end();
    }
    if last < s.len() {
        out.push(Piece::Text(&s[last..]));
    }
    out
}
