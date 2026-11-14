//! The `.hhp` project file walk: INI-like lines; `Title=` in `[OPTIONS]` and the caption of each `[WINDOWS]`
//! definition are units, `Language=` is stamped on a localized merge.

use crate::sitemap::Span;
use crate::text::{has_alnum, py_space, py_trim, py_trim_start, split_lines};
use bilingua_model::guard::check_unit_count;
use bilingua_model::{Error, Limits};

pub fn walk(text: &str, limits: &Limits) -> Result<(Vec<Span>, Option<Span>), Error> {
    let mut units: Vec<Span> = Vec::new();
    let mut lang: Option<Span> = None;
    let mut section = String::new();
    let mut pos = 0usize;
    for line in split_lines(text) {
        let body = line.trim_end_matches(['\r', '\n']);
        let start = pos;
        pos += line.len();
        // [Section]
        let t = py_trim_start(body);
        if let Some(name) = section_name(t) {
            section = name;
            continue;
        }
        if t.starts_with(';') {
            continue;
        }
        let Some(eq) = body.find('=') else { continue };
        if eq == 0 {
            continue;
        }
        if section == "OPTIONS" {
            let key = py_trim(&body[..eq]).to_lowercase();
            let after = &body[eq + 1..];
            let skipped = after.len() - after.trim_start_matches([' ', '\t']).len();
            let value = after.trim_start_matches([' ', '\t']).trim_end_matches([' ', '\t']);
            let vs = start + eq + 1 + skipped;
            let sp = Span { start: vs, end: vs + value.len(), text: value.to_string(), quote: "raw", kind: "" };
            if key == "title" && has_alnum(value) {
                units.push(Span { kind: "title", ..sp });
                check_unit_count(units.len(), limits)?;
            } else if key == "language" {
                lang = Some(Span { kind: "language", ..sp });
            }
        } else if section == "WINDOWS" {
            let after = &body[eq + 1..];
            let ws = after.len() - py_trim_start(after).len();
            let Some(q) = after[ws..].strip_prefix('"') else { continue };
            let Some(close) = q.find('"') else { continue };
            let caption = &q[..close];
            if has_alnum(caption) {
                let cs = start + eq + 1 + ws + 1;
                units.push(Span { start: cs, end: cs + caption.len(), text: caption.to_string(), quote: "\"", kind: "w" });
                check_unit_count(units.len(), limits)?;
            }
        }
    }
    Ok((units, lang))
}

/// `[NAME]` on a line (leading whitespace already trimmed): the section name in upper case.
fn section_name(t: &str) -> Option<String> {
    let inner = t.strip_prefix('[')?;
    let close = inner.find(']')?;
    inner[close + 1..].chars().all(py_space).then(|| py_trim(&inner[..close]).to_uppercase())
}

/// True when some line is a `[Section]` header: the file is an INI-like project file, not plain text.
pub fn has_section(text: &str) -> bool {
    split_lines(text).iter().any(|line| section_name(py_trim_start(line.trim_end_matches(['\r', '\n']))).is_some())
}

/// What a translation is written as: one line, trimmed, and no double quote in a window caption.
pub fn value(new: &str, kind: &str) -> String {
    let one = new.replace(['\r', '\n'], " ");
    let one = py_trim(&one);
    if kind == "w" {
        one.replace('"', "'")
    } else {
        one.to_string()
    }
}
