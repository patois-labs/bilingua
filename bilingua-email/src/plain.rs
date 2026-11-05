//! text/plain bodies: paragraphs as LocOS cuts them (RFC 3676 format=flowed unwrapped, fixed text joined at hard
//! wraps and re-wrapped at the paragraph's own width), with quotes, the signature separator and blank lines kept.

use crate::pyish::{is_space, lstrip, rstrip, rstrip_eol, splitlines_keep, strip, wrap};
use regex::Regex;
use std::sync::OnceLock;

pub const MAX_LINE: usize = 998;

/// One paragraph of a plain part.
#[derive(Debug, Clone)]
pub struct Para {
    pub text: String,
    pub raw: String,
    pub lead: String,
    pub trail: String,
    pub width: usize,
    pub multi: bool,
    pub eol: String,
}

#[derive(Debug, Clone)]
pub enum Item {
    /// Kept verbatim.
    Raw(String),
    Para(Para),
}

fn list_mark() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\s*(?:[-*+\u{2022}]|\d{1,3}[.)])\s").unwrap())
}

fn clen(s: &str) -> usize {
    s.chars().count()
}

fn eol_of(raw: &str) -> String {
    raw[rstrip_eol(raw).len()..].to_string()
}

pub fn items(text: &str, flowed: bool, delsp: bool) -> Vec<Item> {
    let lines = splitlines_keep(text);
    let mut out: Vec<Item> = Vec::new();
    if flowed {
        let mut i = 0;
        while i < lines.len() {
            let mut j = i;
            let mut parts: Vec<&str> = Vec::new();
            while j < lines.len() {
                let body = rstrip_eol(lines[j]);
                if j > i && (body.starts_with('>') != lines[i].starts_with('>') || body == "-- " || body == "--") {
                    break;
                }
                parts.push(body.strip_prefix(' ').unwrap_or(body));
                j += 1;
                if !(body.ends_with(' ') && body != "-- ") || body.starts_with('>') {
                    break;
                }
            }
            let raw: String = lines[i..j].concat();
            let first = lines[i];
            if first.starts_with('>') || matches!(rstrip_eol(first), "-- " | "--") {
                out.push(Item::Raw(raw));
            } else {
                let last = parts.len() - 1;
                let joined: String = parts
                    .iter()
                    .enumerate()
                    .map(|(k, p)| {
                        if delsp && k < last {
                            let mut c = p.chars();
                            c.next_back();
                            c.as_str()
                        } else {
                            p
                        }
                    })
                    .collect();
                if strip(&joined).is_empty() {
                    out.push(Item::Raw(raw));
                } else {
                    let width = lines[i..j].iter().map(|l| clen(rstrip_eol(l))).max().unwrap_or(0);
                    out.push(Item::Para(Para {
                        text: strip(&joined).to_string(),
                        lead: joined[..joined.len() - lstrip(&joined).len()].to_string(),
                        trail: String::new(),
                        width,
                        multi: true,
                        eol: eol_of(&raw),
                        raw,
                    }));
                }
            }
            i = j;
        }
        return out;
    }
    let mut group: Vec<&str> = Vec::new();
    fn flush(group: &mut Vec<&str>, out: &mut Vec<Item>) {
        if group.is_empty() {
            return;
        }
        let bodies: Vec<&str> = group.iter().map(|g| rstrip_eol(g)).collect();
        let raw: String = group.concat();
        let first = bodies[0];
        let last = bodies[bodies.len() - 1];
        out.push(Item::Para(Para {
            text: bodies.iter().map(|b| strip(b)).collect::<Vec<_>>().join(" "),
            lead: first[..first.len() - lstrip(first).len()].to_string(),
            trail: if group.len() == 1 { last[rstrip(last).len()..].to_string() } else { String::new() },
            width: bodies.iter().map(|b| clen(rstrip(b))).max().unwrap_or(0),
            multi: group.len() > 1,
            eol: eol_of(&raw),
            raw,
        }));
        group.clear();
    }
    for line in lines {
        let body = rstrip_eol(line);
        if strip(body).is_empty() || lstrip(body).starts_with('>') || body == "-- " || body == "--" {
            flush(&mut group, &mut out);
            out.push(Item::Raw(line.to_string()));
            continue;
        }
        if let Some(prev) = group.last() {
            let prev = rstrip(rstrip_eol(prev));
            let joins = clen(prev) >= 60 && !body.chars().next().is_some_and(is_space) && !list_mark().is_match(body);
            if !joins {
                flush(&mut group, &mut out);
            }
        }
        group.push(line);
    }
    flush(&mut group, &mut out);
    out
}

/// The text of a translated paragraph, laid out like the original one.
pub fn render(p: &Para, new: &str, flowed: bool, delsp: bool) -> String {
    let sep = if p.raw.contains("\r\n") { "\r\n" } else { "\n" };
    if flowed {
        let width = p.width.clamp(72, MAX_LINE - 2);
        let mut chunks = wrap(&format!("{}{}", p.lead, new), width - 1);
        if chunks.is_empty() {
            chunks.push(String::new());
        }
        let last = chunks.len() - 1;
        let mut out: Vec<String> = Vec::with_capacity(chunks.len());
        for (k, mut chunk) in chunks.into_iter().enumerate() {
            if k < last {
                chunk.push(' ');
                if delsp {
                    chunk.push(' ');
                }
            } else {
                chunk = chunk.trim_end_matches(' ').to_string();
            }
            if chunk.starts_with([' ', '>']) || chunk.starts_with("From ") {
                chunk.insert(0, ' ');
            }
            out.push(chunk);
        }
        return out.join(sep) + &p.eol;
    }
    if !p.multi {
        return format!("{}{}{}{}", p.lead, new, p.trail, p.eol);
    }
    let width = p.width.saturating_sub(clen(&p.lead)).max(20);
    let mut chunks = wrap(new, width);
    if chunks.is_empty() {
        chunks.push(String::new());
    }
    chunks.iter().map(|c| format!("{}{}", p.lead, c)).collect::<Vec<_>>().join(sep) + &p.eol
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(t: &str, flowed: bool) -> Vec<String> {
        items(t, flowed, false).into_iter().filter_map(|i| if let Item::Para(p) = i { Some(p.text) } else { None }).collect()
    }

    #[test]
    fn fixed_text_paragraphs() {
        let t = "Hi team,\n\nYour order %1$s ships on {date}.\n  Indented line stays indented.\n- first item\n- second item\n";
        assert_eq!(texts(t, false), vec!["Hi team,", "Your order %1$s ships on {date}.", "Indented line stays indented.", "- first item", "- second item"]);
    }

    #[test]
    fn flowed_unwraps_and_stops_at_quote_and_signature() {
        let t = "Mine flows \n>Theirs\nSign off \n-- \nAnn\n";
        assert_eq!(texts(t, true), vec!["Mine flows", "Sign off", "Ann"]);
    }

    #[test]
    fn rewrap_keeps_width() {
        let t = "This paragraph was hard wrapped by an old mail client at sixty-eight\ncolumns, so its lines belong together as one sentence for translation.\n";
        let it = items(t, false, false);
        let Item::Para(p) = &it[0] else { panic!() };
        assert!(p.multi);
        let out = render(p, &"Ein Absatz ".repeat(20), false, false);
        assert!(out.lines().all(|l| l.chars().count() <= 71));
    }
}
