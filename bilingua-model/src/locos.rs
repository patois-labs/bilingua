//! Conversion between Bilingua inline codes and the text tokens Localization OS uses.
//!
//! LocOS puts inline codes into the text as `\u{27E6}..\u{27E7}` tokens, numbered from 1 in source order, and
//! keeps the real markup in a side map (`TextUnit.inline_map`). Every Bilingua tag becomes exactly one token:
//!
//! | Bilingua tag                  | token       | letter |
//! |-------------------------------|-------------|--------|
//! | `Open`                        | `\u{27E6}g1\u{27E7}`   | `g` (or the pair letter you pass, `r` for DOCX runs) |
//! | `Close` (matches an `Open`)   | `\u{27E6}/g1\u{27E7}`  | same number and letter as its `Open` |
//! | `Standalone`                  | `\u{27E6}x1\u{27E7}`   | `x`, its order matters |
//! | `Standalone` with `free`      | `\u{27E6}p1\u{27E7}`   | `p`, may move in the target |
//!
//! The map ([`LocosMap`]) is keyed by the token number as text, like `inline_map`, and holds what is needed to
//! rebuild the exact tag: its id, its displays and its free flag. The conversion back reads the kind from the
//! token and the rest from the map, so a translator or model may reorder tokens and the tags still come back.
//!
//! Damage is never silent and never loses text: a token that is malformed, or whose number is not in the map,
//! stays in the text as it is and is reported by [`parse_locos_text`].

use crate::{merge_text, Inline, TagKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const OPEN: char = '\u{27E6}';
const CLOSE: char = '\u{27E7}';

/// The kind letter for paired codes unless the caller picks another.
pub const PAIR_LETTER: char = 'g';
/// The kind letter of a standalone code whose order matters.
pub const POINT_LETTER: char = 'x';
/// The kind letter of a standalone code that may move freely (LocOS `FREE_POINT_KIND`).
pub const FREE_LETTER: char = 'p';

/// What the side map holds for one token number.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocosCode {
    /// Bilingua tag id.
    pub id: String,
    /// Kind letter used in the token: `g`/`r`/... for a pair, `x` or `p` for a standalone code.
    pub letter: char,
    /// Display of the opening (or standalone) tag.
    #[serde(default)]
    pub open: Option<String>,
    /// Display of the closing tag, when the pair has one in the text.
    #[serde(default)]
    pub close: Option<String>,
    /// Standalone code that may move freely.
    #[serde(default)]
    pub free: bool,
}

/// Token number (as text, from "1") to the tag it stands for.
pub type LocosMap = BTreeMap<String, LocosCode>;

/// Inline codes as LocOS text with `g` for pairs.
pub fn to_locos_text(inl: &[Inline]) -> (String, LocosMap) {
    to_locos_text_as(inl, PAIR_LETTER)
}

/// Like [`to_locos_text`] with another kind letter for pairs (`r` for DOCX runs). `pair_letter` must be a
/// lower-case ASCII letter other than `x` and `p`; anything else falls back to `g`.
pub fn to_locos_text_as(inl: &[Inline], pair_letter: char) -> (String, LocosMap) {
    let pair = if pair_letter.is_ascii_lowercase() && pair_letter != POINT_LETTER && pair_letter != FREE_LETTER {
        pair_letter
    } else {
        PAIR_LETTER
    };
    let mut text = String::new();
    let mut map = LocosMap::new();
    // (tag id, token number, letter) of the pairs still open
    let mut open: Vec<(String, usize, char)> = Vec::new();
    let mut n = 0usize;
    for i in inl {
        match i {
            Inline::Text(t) => text.push_str(t),
            Inline::Tag { id, kind, display, free } => match kind {
                TagKind::Open => {
                    n += 1;
                    open.push((id.clone(), n, pair));
                    map.insert(
                        n.to_string(),
                        LocosCode { id: id.clone(), letter: pair, open: Some(display.clone()), close: None, free: false },
                    );
                    text.push_str(&format!("{OPEN}{pair}{n}{CLOSE}"));
                }
                TagKind::Close => {
                    if let Some(pos) = open.iter().rposition(|(oid, _, _)| oid == id) {
                        let (_, num, letter) = open.remove(pos);
                        if let Some(c) = map.get_mut(&num.to_string()) {
                            c.close = Some(display.clone());
                        }
                        text.push_str(&format!("{OPEN}/{letter}{num}{CLOSE}"));
                    } else {
                        // A close with no open before it gets a number of its own.
                        n += 1;
                        map.insert(
                            n.to_string(),
                            LocosCode { id: id.clone(), letter: pair, open: None, close: Some(display.clone()), free: false },
                        );
                        text.push_str(&format!("{OPEN}/{pair}{n}{CLOSE}"));
                    }
                }
                TagKind::Standalone => {
                    n += 1;
                    let letter = if *free { FREE_LETTER } else { POINT_LETTER };
                    map.insert(
                        n.to_string(),
                        LocosCode { id: id.clone(), letter, open: Some(display.clone()), close: None, free: *free },
                    );
                    text.push_str(&format!("{OPEN}{letter}{n}{CLOSE}"));
                }
            },
        }
    }
    (text, map)
}

/// What was wrong with a token in LocOS text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenProblem {
    /// A `\u{27E6}` or `\u{27E7}` that is not part of a well-formed token. Left in the text.
    Stray { at: usize },
    /// A well-formed token whose number is not in the map. Left in the text.
    Unknown { token: String },
    /// A token whose kind does not fit the map (a closing token for a standalone code, an `x` token for a pair).
    /// Left in the text.
    KindMismatch { token: String },
}

/// LocOS text back to inline codes. Tokens that cannot be resolved stay in the text; see [`parse_locos_text`]
/// to find out which.
pub fn from_locos_text(text: &str, map: &LocosMap) -> Vec<Inline> {
    parse_locos_text(text, map).0
}

/// Like [`from_locos_text`], and lists every token that could not be resolved.
pub fn parse_locos_text(text: &str, map: &LocosMap) -> (Vec<Inline>, Vec<TokenProblem>) {
    let mut out: Vec<Inline> = Vec::new();
    let mut problems = Vec::new();
    let mut lit = String::new();
    let mut rest = text;
    let mut pos = 0usize;
    while let Some(ch) = rest.chars().next() {
        let len = ch.len_utf8();
        if ch == OPEN {
            if let Some((tok_len, closing, letter, num)) = scan_token(rest) {
                let token = &rest[..tok_len];
                match resolve(map, closing, letter, &num) {
                    Resolved::Tag(t) => {
                        if !lit.is_empty() {
                            out.push(Inline::Text(std::mem::take(&mut lit)));
                        }
                        out.push(t);
                    }
                    Resolved::Unknown => {
                        problems.push(TokenProblem::Unknown { token: token.to_string() });
                        lit.push_str(token);
                    }
                    Resolved::Mismatch => {
                        problems.push(TokenProblem::KindMismatch { token: token.to_string() });
                        lit.push_str(token);
                    }
                }
                rest = &rest[tok_len..];
                pos += tok_len;
                continue;
            }
            problems.push(TokenProblem::Stray { at: pos });
        } else if ch == CLOSE {
            problems.push(TokenProblem::Stray { at: pos });
        }
        lit.push(ch);
        rest = &rest[len..];
        pos += len;
    }
    if !lit.is_empty() {
        out.push(Inline::Text(lit));
    }
    (merge_text(out), problems)
}

/// Reads `OPEN [/] [a-z] digits CLOSE` at the start of `s`: (byte length, is closing, letter, number text).
fn scan_token(s: &str) -> Option<(usize, bool, Option<char>, String)> {
    let mut it = s.char_indices();
    let (_, c) = it.next()?;
    if c != OPEN {
        return None;
    }
    let mut closing = false;
    let mut letter = None;
    let mut num = String::new();
    for (idx, c) in it {
        match c {
            '/' if !closing && letter.is_none() && num.is_empty() => closing = true,
            c if c.is_ascii_lowercase() && letter.is_none() && num.is_empty() => letter = Some(c),
            c if c.is_ascii_digit() => num.push(c),
            CLOSE if !num.is_empty() => return Some((idx + CLOSE.len_utf8(), closing, letter, num)),
            _ => return None,
        }
    }
    None
}

enum Resolved {
    Tag(Inline),
    Unknown,
    Mismatch,
}

fn resolve(map: &LocosMap, closing: bool, letter: Option<char>, num: &str) -> Resolved {
    let Some(code) = map.get(num) else { return Resolved::Unknown };
    let is_point = code.letter == POINT_LETTER || code.letter == FREE_LETTER;
    // The letter in the text may differ from the map's only between x and p (a model may normalise it); a
    // pair letter must agree, and a closing token never belongs to a standalone code.
    if is_point {
        if closing || letter.is_some_and(|l| l != POINT_LETTER && l != FREE_LETTER) {
            return Resolved::Mismatch;
        }
        return Resolved::Tag(Inline::Tag {
            id: code.id.clone(),
            kind: TagKind::Standalone,
            display: code.open.clone().unwrap_or_default(),
            free: code.free,
        });
    }
    if letter.is_some_and(|l| l == POINT_LETTER || l == FREE_LETTER) {
        return Resolved::Mismatch;
    }
    if closing {
        Resolved::Tag(Inline::Tag {
            id: code.id.clone(),
            kind: TagKind::Close,
            display: code.close.clone().unwrap_or_default(),
            free: false,
        })
    } else {
        Resolved::Tag(Inline::Tag {
            id: code.id.clone(),
            kind: TagKind::Open,
            display: code.open.clone().unwrap_or_default(),
            free: false,
        })
    }
}

/// True when a placeholder names its argument, so its position in a translation is free: `%1$s`, `%(name)s`,
/// `{name}`, `{0}`, `{{name}}`, `${name}`. False for markup and for unnumbered placeholders (`%s`, `%d`, `{}`),
/// whose order is the argument order.
pub fn is_named_placeholder(s: &str) -> bool {
    let b = s.as_bytes();
    match b {
        [b'{', b'{', ..] => s.ends_with("}}") && s.len() > 4,
        [b'$', b'{', ..] => s.ends_with('}') && s.len() > 3,
        [b'{', ..] => {
            let inner = &s[1..s.len().saturating_sub(1)];
            s.ends_with('}') && !inner.is_empty() && !inner.starts_with(' ')
        }
        [b'%', b'(', ..] => s[2..].contains(')'),
        [b'%', d, ..] if d.is_ascii_digit() => {
            let digits = s[1..].bytes().take_while(u8::is_ascii_digit).count();
            s.as_bytes().get(1 + digits) == Some(&b'$')
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Inline> {
        vec![
            Inline::text("Hola "),
            Inline::tag("1", TagKind::Open, "<b>"),
            Inline::text("mundo"),
            Inline::tag("1", TagKind::Close, "</b>"),
            Inline::text(", "),
            Inline::free_tag("2", "{name}"),
            Inline::text(" "),
            Inline::tag("3", TagKind::Standalone, "%s"),
            Inline::text("."),
        ]
    }

    #[test]
    fn tokens_follow_the_table() {
        let (t, map) = to_locos_text(&sample());
        assert_eq!(t, "Hola \u{27E6}g1\u{27E7}mundo\u{27E6}/g1\u{27E7}, \u{27E6}p2\u{27E7} \u{27E6}x3\u{27E7}.");
        assert_eq!(map.len(), 3);
        assert_eq!(map["1"].close.as_deref(), Some("</b>"));
        assert!(map["2"].free && map["2"].letter == 'p');
        assert!(!map["3"].free && map["3"].letter == 'x');
    }

    #[test]
    fn round_trips() {
        let s = sample();
        let (t, map) = to_locos_text(&s);
        assert_eq!(from_locos_text(&t, &map), s);
        let (t, map) = to_locos_text_as(&s, 'r');
        assert!(t.contains("\u{27E6}r1\u{27E7}") && t.contains("\u{27E6}/r1\u{27E7}"));
        assert_eq!(from_locos_text(&t, &map), s);
    }

    #[test]
    fn bad_pair_letter_falls_back() {
        let (t, _) = to_locos_text_as(&sample(), 'x');
        assert!(t.contains("\u{27E6}g1\u{27E7}"));
        let (t, _) = to_locos_text_as(&sample(), 'G');
        assert!(t.contains("\u{27E6}g1\u{27E7}"));
    }

    #[test]
    fn reordered_free_tokens_come_back_by_number() {
        let (_, map) = to_locos_text(&[Inline::free_tag("a", "%1$s"), Inline::text(" y "), Inline::free_tag("b", "%2$s")]);
        let back = from_locos_text("\u{27E6}p2\u{27E7} \u{27E6}p1\u{27E7}", &map);
        assert_eq!(back[0], Inline::free_tag("b", "%2$s"));
        assert_eq!(back[2], Inline::free_tag("a", "%1$s"));
    }

    #[test]
    fn unmatched_close_and_open_survive() {
        let s = vec![
            Inline::tag("9", TagKind::Close, "</i>"),
            Inline::text("x"),
            Inline::tag("8", TagKind::Open, "<u>"),
            Inline::text("y"),
        ];
        let (t, map) = to_locos_text(&s);
        assert_eq!(from_locos_text(&t, &map), s);
    }

    #[test]
    fn nested_and_repeated_ids() {
        let s = vec![
            Inline::tag("a", TagKind::Open, "<b>"),
            Inline::tag("b", TagKind::Open, "<i>"),
            Inline::text("t"),
            Inline::tag("b", TagKind::Close, "</i>"),
            Inline::tag("a", TagKind::Close, "</b>"),
            Inline::tag("a", TagKind::Open, "<b>"),
            Inline::text("u"),
            Inline::tag("a", TagKind::Close, "</b>"),
        ];
        let (t, map) = to_locos_text(&s);
        assert_eq!(map.len(), 3);
        assert_eq!(from_locos_text(&t, &map), s);
    }

    #[test]
    fn malformed_tokens_stay_as_text_and_are_reported() {
        let (_, map) = to_locos_text(&sample());
        for bad in [
            "a \u{27E6} b",
            "a \u{27E7} b",
            "\u{27E6}g\u{27E7}",
            "\u{27E6}g1",
            "\u{27E6}/\u{27E6}g1\u{27E7}",
            "\u{27E6}G1\u{27E7}",
            "\u{27E6}x1{limit}\u{27E7}",
            "\u{27E6}g99\u{27E7}",
            "\u{27E6}/x3\u{27E7}",
            "\u{27E6}x1\u{27E7}",
        ] {
            let (inl, problems) = parse_locos_text(bad, &map);
            let rebuilt: String = inl
                .iter()
                .map(|i| match i {
                    Inline::Text(t) => t.clone(),
                    Inline::Tag { .. } => String::from("#"),
                })
                .collect();
            if bad == "\u{27E6}x1\u{27E7}" {
                // x1 is a pair in the map: a point token for a pair is a mismatch.
                assert!(problems.iter().any(|p| matches!(p, TokenProblem::KindMismatch { .. })), "{bad}");
            } else {
                assert!(!problems.is_empty(), "{bad:?} gave no problem");
            }
            assert!(rebuilt.contains('\u{27E6}') || rebuilt.contains('\u{27E7}') || bad.contains("g1\u{27E7}"), "{bad:?} lost text");
        }
    }

    #[test]
    fn text_in_every_language_survives() {
        for t in [
            "Hello", "Hola", "Bonjour", "Ciao", "Hallo", "Hallo", "Ola", "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}",
            "\u{4f60}\u{597d}", "\u{c548}\u{b155}", "\u{645}\u{631}\u{62d}\u{628}\u{627}",
        ] {
            let s = vec![Inline::tag("1", TagKind::Open, "<b>"), Inline::text(t), Inline::tag("1", TagKind::Close, "</b>"), Inline::free_tag("2", "{0}")];
            let (x, map) = to_locos_text(&s);
            assert_eq!(from_locos_text(&x, &map), s);
        }
    }

    #[test]
    fn empty_and_tagless() {
        let (t, m) = to_locos_text(&[]);
        assert!(t.is_empty() && m.is_empty());
        assert!(from_locos_text("", &m).is_empty());
        assert_eq!(from_locos_text("plain", &m), vec![Inline::text("plain")]);
    }

    #[test]
    fn named_placeholders() {
        for yes in ["%1$s", "%12$d", "%(name)s", "{name}", "{0}", "{{count}}", "${user}"] {
            assert!(is_named_placeholder(yes), "{yes}");
        }
        for no in ["%s", "%d", "%.2f", "{}", "{ x }", "<b>", "%%", "%5d", "{{}}", "${}", "%(x"] {
            assert!(!is_named_placeholder(no), "{no}");
        }
    }
}
