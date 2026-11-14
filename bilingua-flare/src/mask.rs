//! The MadCap phrasing elements of a topic, hidden from `bilingua-html` and put back after it has written.
//!
//! `bilingua-html` knows the HTML inline elements only. A Flare topic has inline elements of its own
//! (`MadCap:xref`, `MadCap:conditionalText`, `MadCap:variable`, ...) that Localization OS keeps inside the
//! sentence as codes. Before the topic goes to the HTML filter each of them is swapped for an HTML inline element
//! that carries its number:
//!
//! * a paired start tag becomes `<span KEY="n">`, its end tag `</span KEY="n">`;
//! * a point (self-closing tag, `MadCap:variable` with its cached content, `snippetText`, `keyword`, `concept`,
//!   and the end tag of those) becomes `<wbr KEY="n"/>`.
//!
//! `table[n]` holds the original markup. [`unmask`] swaps them back in the finished text, wherever they ended up
//! (inside a unit, in a stored code or in the skeleton).

use crate::scan::{next_tag, Tag};
use bilingua_model::Error;

/// MadCap elements that sit inside a sentence as a pair (lowercase).
const PAIRED: &[&str] = &[
    "madcap:conditionaltext",
    "madcap:xref",
    "madcap:glossaryterm",
    "madcap:annotation",
    "madcap:change",
    "madcap:toggler",
    "madcap:textpopup",
    "madcap:textpopuphead",
    "madcap:textpopupbody",
    "madcap:expanding",
    "madcap:expandinghead",
    "madcap:expandingbody",
    "madcap:popup",
    "madcap:popuphead",
    "madcap:popupbody",
];
/// MadCap elements that are always a point (lowercase).
const POINTS: &[&str] = &["madcap:variable", "madcap:snippettext", "madcap:keyword", "madcap:concept"];

/// One `term` attribute of a `MadCap:keyword` tag: where its value is inside `table[entry]`.
#[derive(Debug, Clone)]
pub struct KwAttr {
    pub entry: usize,
    pub vs: usize,
    pub ve: usize,
}

pub struct Masked {
    pub text: String,
    pub table: Vec<String>,
    pub key: String,
    pub keywords: Vec<KwAttr>,
}

/// An attribute name that does not occur in `text`: `mcf` and one `x` more than the longest run of `x` after any
/// `mcf` in the text. One linear pass; a topic full of `mcfxxx...` only makes the name longer, it is never refused.
fn pick_key(text: &str) -> String {
    match text.match_indices("mcf").map(|(p, _)| text[p + 3..].bytes().take_while(|b| *b == b'x').count() + 1).max() {
        None => "mcf".into(),
        Some(n) => format!("mcf{}", "x".repeat(n)),
    }
}

pub fn mask(text: &str) -> Result<Masked, Error> {
    let key = pick_key(text);
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut table: Vec<String> = Vec::new();
    let mut keywords = Vec::new();
    let mut copied = 0usize;
    let mut i = 0usize;
    // once a search for the end of a MadCap:variable failed, no later one can succeed
    let mut no_var_close = false;
    while let Some(tag) = next_tag(text, i) {
        i = tag.resume;
        let paired = PAIRED.contains(&tag.name.as_str());
        let point = POINTS.contains(&tag.name.as_str());
        if !paired && !point {
            continue;
        }
        let mut end = tag.end;
        if tag.name == "madcap:variable" && !tag.closing && !tag.self_closing && !no_var_close {
            let mut j = tag.end;
            let mut found = None;
            while let Some(t) = next_tag(text, j) {
                j = t.resume;
                if t.closing && t.name == "madcap:variable" {
                    found = Some(t.end);
                    break;
                }
            }
            match found {
                Some(e) => end = e,
                None => no_var_close = true,
            }
        }
        out.push_str(&text[copied..tag.start]);
        let n = table.len();
        table.push(text[tag.start..end].to_string());
        if tag.name == "madcap:keyword" && !tag.closing {
            for a in &tag.attrs {
                if let (true, true, Some((vs, ve))) = (a.name == "term", a.quoted, a.value) {
                    keywords.push(KwAttr { entry: n, vs: vs - tag.start, ve: ve - tag.start });
                }
            }
        }
        out.push_str(&replacement(&key, n, &tag, paired));
        copied = end;
        i = i.max(end);
    }
    out.push_str(&text[copied..]);
    Ok(Masked { text: out, table, key, keywords })
}

fn replacement(key: &str, n: usize, tag: &Tag, paired: bool) -> String {
    if paired && !tag.self_closing {
        if tag.closing {
            format!("</span {key}=\"{n}\">")
        } else {
            format!("<span {key}=\"{n}\">")
        }
    } else {
        format!("<wbr {key}=\"{n}\"/>")
    }
}

/// Which table entry a mask in `s` names (the first one), and whether it is an opening, closing or point mask.
pub fn find_mask(s: &str, key: &str) -> Option<(usize, MaskKind)> {
    let pat = format!("{key}=\"");
    let p = s.find(&pat)?;
    let kind = if s[..p].ends_with("</span ") {
        MaskKind::Close
    } else if s[..p].ends_with("<span ") {
        MaskKind::Open
    } else if s[..p].ends_with("<wbr ") {
        MaskKind::Point
    } else {
        return None;
    };
    let digits: String = s[p + pat.len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
    Some((digits.parse().ok()?, kind))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskKind {
    Open,
    Close,
    Point,
}

/// Puts the original markup back for every mask in `s`.
pub fn unmask(s: &str, key: &str, table: &[String]) -> String {
    let pat = format!("{key}=\"");
    let mut out = String::with_capacity(s.len());
    let mut last = 0usize;
    for (p, _) in s.match_indices(&pat) {
        if p < last {
            continue;
        }
        let before = &s[last..p];
        let (prefix, tail) = if before.ends_with("</span ") {
            ("</span ", "\">")
        } else if before.ends_with("<span ") {
            ("<span ", "\">")
        } else if before.ends_with("<wbr ") {
            ("<wbr ", "\"/>")
        } else {
            continue;
        };
        let after = &s[p + pat.len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || !after[digits..].starts_with(tail) {
            continue;
        }
        let Some(entry) = after[..digits].parse::<usize>().ok().and_then(|n| table.get(n)) else { continue };
        out.push_str(&s[last..p - prefix.len()]);
        out.push_str(entry);
        last = p + pat.len() + digits + tail.len();
    }
    out.push_str(&s[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_round_trip() {
        let t = "<p>a <MadCap:xref href=\"x\">b <MadCap:variable name=\"v\">cached</MadCap:variable></MadCap:xref> <MadCap:snippetText src=\"s\"/>\u{65e5}<!-- <MadCap:xref> --></p>";
        let m = mask(t).unwrap();
        assert!(!m.text.contains("MadCap:xref href"), "{}", m.text);
        assert!(m.text.contains("<!-- <MadCap:xref> -->"));
        assert_eq!(m.table.len(), 4);
        assert_eq!(m.table[1], "<MadCap:variable name=\"v\">cached</MadCap:variable>");
        assert_eq!(unmask(&m.text, &m.key, &m.table), t);
    }

    #[test]
    fn keyword_values_are_found() {
        let t = "<p><MadCap:keyword term=\"A;B:c\" /></p>";
        let m = mask(t).unwrap();
        assert_eq!(m.keywords.len(), 1);
        let k = &m.keywords[0];
        assert_eq!(&m.table[k.entry][k.vs..k.ve], "A;B:c");
    }

    #[test]
    fn unclosed_variables_stay_linear_and_the_key_avoids_the_text() {
        let t = "<p><MadCap:variable name=\"x\">".repeat(20_000);
        let m = mask(&t).unwrap();
        assert_eq!(unmask(&m.text, &m.key, &m.table), t);
        let m = mask("<p>mcf <MadCap:xref>a</MadCap:xref></p>").unwrap();
        assert_eq!(m.key, "mcfx");
        let t = "<p>mcfxxxxxxxxx mcfx <MadCap:xref>a</MadCap:xref></p>";
        let m = mask(t).unwrap();
        assert!(!t.contains(&m.key), "{}", m.key);
        assert_eq!(unmask(&m.text, &m.key, &m.table), t);
    }
}
