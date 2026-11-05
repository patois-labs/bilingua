//! The document header: `:description:` / `:keywords:` values (#733) and the `:lang:` entry.

use crate::lex::*;
use bilingua_model::{CodeData, Inline, TagKind, Unit};
use regex::Regex;
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub struct Meta {
    pub start: usize,
    pub end: usize,
    pub style: &'static str,
    pub value: String,
    pub line: usize,
}

fn attr_ref() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\\?\{[A-Za-z0-9_][A-Za-z0-9_-]{0,100}\}").expect("attribute reference pattern"))
}

fn continued(s: &str) -> bool {
    s.ends_with(" \\") || s.ends_with("\t\\")
}

/// `_META_ENTRY.match`: end of `:description:` / `:keywords:` and the blanks after it, with its style.
fn meta_entry(body: &str) -> Option<(usize, &'static str)> {
    for (name, style) in [(":description:", "meta:description"), (":keywords:", "meta:keywords")] {
        if let Some(rest) = body.strip_prefix(name) {
            let sp = ht_run(rest);
            if sp > 0 || rest.is_empty() {
                return Some((name.len() + sp, style));
            }
        }
    }
    None
}

/// The description / keywords values of the document header (leading comments, then a title or attribute entries,
/// until a blank line).
pub fn header_meta(text: &str) -> Vec<Meta> {
    let lines = lines(text);
    let mut out = Vec::new();
    let mut started = false;
    let mut i = 0usize;
    while i < lines.len() {
        let (s, e) = lines[i];
        let line = i + 1;
        i += 1;
        let mut body = &text[s..e];
        if strip(body).is_empty() {
            if started {
                break;
            }
            continue;
        }
        if !started {
            if is_line_comment(body) {
                continue;
            }
            let title = section_match(body).is_some_and(|m| m.0 == '=' && m.1 == 1);
            if !(title || is_attr_entry(body)) {
                break;
            }
            started = true;
        }
        let Some((mend, style)) = meta_entry(body) else {
            while continued(body) && i < lines.len() {
                let (s2, e2) = lines[i];
                body = &text[s2..e2];
                i += 1;
            }
            continue;
        };
        let mut parts: Vec<String> = vec![body[mend..].to_string()];
        let mut end = e;
        while continued(parts.last().map_or("", |p| p.as_str())) && i < lines.len() && !strip(&text[lines[i].0..lines[i].1]).is_empty() {
            let last = parts.pop().unwrap_or_default();
            let cut = &last[..last.len() - 1];
            parts.push(rstrip(cut).to_string());
            let (_, e2) = lines[i];
            end = e2;
            parts.push(strip(&text[lines[i].0..e2]).to_string());
            i += 1;
        }
        if parts[..parts.len() - 1].iter().any(|p| p.ends_with(" +") || p == "+") {
            continue;
        }
        let value = parts.iter().map(|p| strip(p)).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ");
        if any_letter(&attr_ref().replace_all(&value, "")) {
            out.push(Meta { start: s + mend, end, style, value, line });
        }
    }
    out
}

/// The unit of a header value: attribute references are standalone tags.
pub fn meta_unit(value: &str) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    let mut inl = Vec::new();
    let mut codes = BTreeMap::new();
    let mut pos = 0usize;
    for m in attr_ref().find_iter(value) {
        if m.start() > pos {
            inl.push(Inline::Text(value[pos..m.start()].to_string()));
        }
        let id = (codes.len() + 1).to_string();
        codes.insert(id.clone(), CodeData { standalone: Some(m.as_str().to_string()), label: Some("attribute".into()), ..Default::default() });
        inl.push(Inline::tag(id, TagKind::Standalone, m.as_str()));
        pos = m.end();
    }
    if pos < value.len() {
        inl.push(Inline::Text(value[pos..].to_string()));
    }
    (inl, codes)
}

/// A translated header value on one line, attribute references restored, a trailing backslash as `{backslash}`.
pub fn meta_render(unit: &Unit, target: &[Inline]) -> String {
    let mut s = String::new();
    for i in target {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind, .. } if *kind != TagKind::Close => {
                if let Some(raw) = unit.codes.get(id).and_then(|c| c.standalone.as_deref()) {
                    s.push_str(raw);
                }
            }
            _ => {}
        }
    }
    let mut t = s.split(is_space).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    let n = t.len() - t.trim_end_matches('\\').len();
    if n > 0 {
        t.truncate(t.len() - n);
        t.push_str(&"{backslash}".repeat(n));
    }
    t
}

fn safe_lang(lang: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^[A-Za-z]{2,8}(?:[-_][A-Za-z0-9]{1,8}){0,4}$").expect("lang pattern")).is_match(lang)
}

/// Declares the target language in the document header: an existing `:lang:` entry is rewritten, otherwise one is
/// added at the end of the header, or a header holding only that entry at the top. A malformed locale changes nothing.
pub fn set_lang(text: &str, lang: &str) -> String {
    if !safe_lang(lang) {
        return text.to_string();
    }
    let nl = first_newline(text);
    let mut pos = 0usize;
    let mut header_start: Option<usize> = None;
    for (s, e) in lines(text) {
        let body = &text[s..e];
        if strip(body).is_empty() {
            if header_start.is_none() {
                pos = e;
                continue;
            }
            break;
        }
        if header_start.is_none() {
            if is_line_comment(body) {
                continue;
            }
            if section_match(body).is_some_and(|m| m.0 == '=' && m.1 == 1) || is_attr_entry(body) {
                header_start = Some(s);
            } else {
                break;
            }
        }
        pos = e;
    }
    let Some(hs) = header_start else {
        return format!(":lang: {lang}{nl}{nl}{text}");
    };
    let head = &text[hs..pos];
    // `^:lang:[ \t]*([^\r\n]*)$` with re.MULTILINE: a line ended by "\n" (a "\r" before it stops the match)
    let mut off = 0usize;
    for seg in head.split('\n') {
        if let Some(rest) = seg.strip_prefix(":lang:") {
            if !seg.contains('\r') {
                let a = hs + off + ":lang:".len() + ht_run(rest);
                let b = hs + off + seg.len();
                return format!("{}{}{}", &text[..a], lang, &text[b..]);
            }
        }
        off += seg.len() + 1;
    }
    format!("{}{}:lang: {}{}", &text[..pos], nl, lang, &text[pos..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_entry() {
        assert_eq!(set_lang("= Title\n:lang: en\n:toc:\n\nBody text.\n", "de"), "= Title\n:lang: de\n:toc:\n\nBody text.\n");
        assert_eq!(set_lang("Just a paragraph.\n", "ja"), ":lang: ja\n\nJust a paragraph.\n");
        assert_eq!(set_lang("Just.\n", "bad value\n:x:"), "Just.\n");
    }
}
