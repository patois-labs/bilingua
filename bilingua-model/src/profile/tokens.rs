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
//! | `Open` or `Close` with no partner, or crossing another pair | `\u{27E6}x1\u{27E7}` | `x`, like LocOS's own begin and end codes (`<bpt>`, `<ept>`, `<it>`) |
//!
//! Pairs are matched by id with proper nesting, so the text is always well formed for LocOS's tag checks
//! (`placeholders_well_formed`): an unclosed open, a close with no open and the inner side of crossed pairs are
//! point codes whose order matters. One counter numbers every token, so numbers never repeat in a unit.
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
    /// For an `x` token that stands for an `Open` or `Close` with no partner: which side it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unpaired: Option<TagKind>,
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
    convert(inl, pair_letter, pairs(inl), false)
}

/// The pair-token letter a format's LocOS filter uses (the adapter's `_PAIR_LETTER`): `r` for docx and pptx runs,
/// else [`PAIR_LETTER`].
pub fn locos_pair_letter(format: &str) -> char {
    if matches!(format, "docx" | "pptx") {
        'r'
    } else {
        PAIR_LETTER
    }
}

/// Formats whose LocOS filter writes its own tag tokens into the unit text: every open tag a pair token, every
/// close the pair token of the nearest open of its id even when pairs cross, and an open with no close a pair
/// token too (LocOS `html_filter._iter_units`). Their LocOS text is not always well formed for
/// `placeholders_well_formed`; it is what LocOS stores.
const LOCOS_PAIRS_BY_ID: &[&str] = &["html"];

/// Formats whose tag ids are LocOS's own token numbers, which do not always follow the order of the text (DOCX:
/// runs `r1`..`rN` first, then the object groups from `x<N+1>`): their tokens carry the id as the number.
const LOCOS_NUMBERED_BY_ID: &[&str] = &["docx"];

/// A unit's inline codes as the LocOS filter of `format` writes them: the format's pair letter
/// ([`locos_pair_letter`]), and for the formats whose filter keeps its own tag tokens (`html`) every `Open` a
/// pair token and every `Close` the closing token of the latest open of its id, crossing or not, an `Open` with
/// no close a pair token with no closing one; a `Close` with no open stays an `x` token. For the formats whose tag
/// ids are LocOS's token numbers (`docx`) each token is numbered by its tag id when every id is a distinct
/// positive number, else by position. Other formats: [`to_locos_text_as`]. [`from_locos_text`] reads both back.
pub fn to_locos_text_for(format: &str, inl: &[Inline]) -> (String, LocosMap) {
    let letter = locos_pair_letter(format);
    if LOCOS_PAIRS_BY_ID.contains(&format) {
        convert(inl, letter, pairs_by_id(inl), false)
    } else if LOCOS_NUMBERED_BY_ID.contains(&format) {
        let partner = pairs(inl);
        let by_id = numbered_by_id(inl, &partner);
        convert(inl, letter, partner, by_id)
    } else {
        to_locos_text_as(inl, letter)
    }
}

/// True when every token of `inl` can carry its tag id as its number: each id that starts a token (a paired
/// `Open`, an unpaired side, a standalone code) is a positive number written without leading zeros, and no two
/// tokens share one.
fn numbered_by_id(inl: &[Inline], partner: &[Option<usize>]) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    for (pos, i) in inl.iter().enumerate() {
        let Inline::Tag { id, kind, .. } = i else { continue };
        if *kind == TagKind::Close && partner[pos].is_some() {
            continue;
        }
        let ok = !id.is_empty() && !id.starts_with('0') && id.len() <= 18 && id.bytes().all(|b| b.is_ascii_digit());
        if !ok || !seen.insert(id.as_str()) {
            return false;
        }
    }
    true
}

fn convert(inl: &[Inline], pair_letter: char, partner: Vec<Option<usize>>, by_id: bool) -> (String, LocosMap) {
    let pair = if pair_letter.is_ascii_lowercase() && pair_letter != POINT_LETTER && pair_letter != FREE_LETTER {
        pair_letter
    } else {
        PAIR_LETTER
    };
    let mut text = String::new();
    let mut map = LocosMap::new();
    // token number of each paired Open, by its position in `inl`
    let mut open_num: BTreeMap<usize, usize> = BTreeMap::new();
    let mut n = 0usize;
    for (pos, i) in inl.iter().enumerate() {
        let (id, kind, display, free) = match i {
            Inline::Text(t) => {
                text.push_str(t);
                continue;
            }
            Inline::Tag { id, kind, display, free } => (id, kind, display, free),
        };
        let code = |letter, open: Option<&String>, close: Option<&String>, free, unpaired| LocosCode {
            id: id.clone(),
            letter,
            open: open.cloned(),
            close: close.cloned(),
            free,
            unpaired,
        };
        match (kind, partner[pos]) {
            (TagKind::Open, Some(_)) => {
                n = if by_id { id.parse().unwrap_or(n + 1) } else { n + 1 };
                open_num.insert(pos, n);
                map.insert(n.to_string(), code(pair, Some(display), None, false, None));
                text.push_str(&format!("{OPEN}{pair}{n}{CLOSE}"));
            }
            (TagKind::Close, Some(o)) => {
                let num = open_num.get(&o).copied().unwrap_or_default();
                if let Some(c) = map.get_mut(&num.to_string()) {
                    c.close = Some(display.clone());
                }
                text.push_str(&format!("{OPEN}/{pair}{num}{CLOSE}"));
            }
            (TagKind::Open, None) => {
                n = if by_id { id.parse().unwrap_or(n + 1) } else { n + 1 };
                map.insert(n.to_string(), code(POINT_LETTER, Some(display), None, false, Some(TagKind::Open)));
                text.push_str(&format!("{OPEN}{POINT_LETTER}{n}{CLOSE}"));
            }
            (TagKind::Close, None) => {
                n = if by_id { id.parse().unwrap_or(n + 1) } else { n + 1 };
                map.insert(n.to_string(), code(POINT_LETTER, None, Some(display), false, Some(TagKind::Close)));
                text.push_str(&format!("{OPEN}{POINT_LETTER}{n}{CLOSE}"));
            }
            (TagKind::Standalone, _) => {
                n = if by_id { id.parse().unwrap_or(n + 1) } else { n + 1 };
                let letter = if *free { FREE_LETTER } else { POINT_LETTER };
                map.insert(n.to_string(), code(letter, Some(display), None, *free, None));
                text.push_str(&format!("{OPEN}{letter}{n}{CLOSE}"));
            }
        }
    }
    (text, map)
}

/// For each position in `inl`, the position of its partner when it is an `Open` or `Close` in a properly nested
/// pair with the same id. A close whose open is not on top of the stack closes it and leaves the opens above it
/// unpaired; a close whose id is not open at all is unpaired.
fn pairs(inl: &[Inline]) -> Vec<Option<usize>> {
    let mut partner = vec![None; inl.len()];
    let mut stack: Vec<(&str, usize)> = Vec::new();
    for (pos, i) in inl.iter().enumerate() {
        match i {
            Inline::Tag { id, kind: TagKind::Open, .. } => stack.push((id.as_str(), pos)),
            Inline::Tag { id, kind: TagKind::Close, .. } => {
                if let Some(at) = stack.iter().rposition(|(oid, _)| oid == id) {
                    let (_, o) = stack[at];
                    stack.truncate(at);
                    partner[o] = Some(pos);
                    partner[pos] = Some(o);
                }
            }
            _ => {}
        }
    }
    partner
}

/// Like [`pairs`], the way LocOS's HTML filter pairs its tags: a close pairs with the latest open of its id still
/// open, wherever it is on the stack (pairs may cross); an open never closed is its own partner (a pair token with
/// no closing one). A close with no open is unpaired.
fn pairs_by_id(inl: &[Inline]) -> Vec<Option<usize>> {
    let mut partner = vec![None; inl.len()];
    // the opens still open, per id: one pass whatever the nesting
    let mut open: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (pos, i) in inl.iter().enumerate() {
        match i {
            Inline::Tag { id, kind: TagKind::Open, .. } => {
                partner[pos] = Some(pos);
                open.entry(id.as_str()).or_default().push(pos);
            }
            Inline::Tag { id, kind: TagKind::Close, .. } => {
                if let Some(o) = open.get_mut(id.as_str()).and_then(Vec::pop) {
                    partner[o] = Some(pos);
                    partner[pos] = Some(o);
                }
            }
            _ => {}
        }
    }
    partner
}

/// What was wrong with a token in LocOS text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenProblem {
    /// A `\u{27E6}` or `\u{27E7}` that is not part of a well-formed token. Left in the text.
    Stray {
        /// Its byte position.
        at: usize,
    },
    /// A well-formed token whose number is not in the map. Left in the text.
    Unknown {
        /// The token.
        token: String,
    },
    /// A token whose kind does not fit the map (a closing token for a standalone code, an `x` token for a pair).
    /// Left in the text.
    KindMismatch {
        /// The token.
        token: String,
    },
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
        return Resolved::Tag(match code.unpaired {
            Some(TagKind::Close) => Inline::Tag {
                id: code.id.clone(),
                kind: TagKind::Close,
                display: code.close.clone().unwrap_or_default(),
                free: false,
            },
            Some(TagKind::Open) => Inline::Tag {
                id: code.id.clone(),
                kind: TagKind::Open,
                display: code.open.clone().unwrap_or_default(),
                free: false,
            },
            _ => Inline::Tag {
                id: code.id.clone(),
                kind: TagKind::Standalone,
                display: code.open.clone().unwrap_or_default(),
                free: code.free,
            },
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

    #[test]
    fn docx_tokens_carry_locos_numbers() {
        let inl = vec![
            Inline::tag("1", TagKind::Open, "<w:r>"),
            Inline::text("a"),
            Inline::tag("1", TagKind::Close, "</w:r>"),
            Inline::tag("3", TagKind::Standalone, "object"),
            Inline::tag("2", TagKind::Open, "<w:r>"),
            Inline::text("b"),
            Inline::tag("2", TagKind::Close, "</w:r>"),
        ];
        let (t, map) = to_locos_text_for("docx", &inl);
        assert_eq!(t, "\u{27e6}r1\u{27e7}a\u{27e6}/r1\u{27e7}\u{27e6}x3\u{27e7}\u{27e6}r2\u{27e7}b\u{27e6}/r2\u{27e7}");
        assert_eq!(from_locos_text(&t, &map), inl);
        // other formats, and ids that cannot be numbers, keep the positional numbering
        assert_eq!(to_locos_text_for("pptx", &inl).0, "\u{27e6}r1\u{27e7}a\u{27e6}/r1\u{27e7}\u{27e6}x2\u{27e7}\u{27e6}r3\u{27e7}b\u{27e6}/r3\u{27e7}");
        let dup = vec![Inline::tag("1", TagKind::Standalone, "o"), Inline::tag("1", TagKind::Standalone, "o")];
        assert_eq!(to_locos_text_for("docx", &dup).0, "\u{27e6}x1\u{27e7}\u{27e6}x2\u{27e7}");
        let odd = vec![Inline::tag("x", TagKind::Standalone, "o"), Inline::tag("07", TagKind::Standalone, "o")];
        assert_eq!(to_locos_text_for("docx", &odd).0, "\u{27e6}x1\u{27e7}\u{27e6}x2\u{27e7}");
    }

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
        assert_eq!(t, "\u{27E6}x1\u{27E7}x\u{27E6}x2\u{27E7}y");
        assert!(well_formed(&t));
        assert_eq!(map["1"].unpaired, Some(TagKind::Close));
        assert_eq!(from_locos_text(&t, &map), s);
        // The map keeps the side through JSON.
        let back: LocosMap = serde_json::from_str(&serde_json::to_string(&map).unwrap()).unwrap();
        assert_eq!(from_locos_text(&t, &back), s);
    }

    /// LocOS `placeholders_well_formed`: pairs nest by number, point letters (x, p) never pair.
    fn well_formed(t: &str) -> bool {
        let mut stack = Vec::new();
        for tok in t.split('\u{27E6}').skip(1) {
            let tok = tok.split('\u{27E7}').next().unwrap_or_default();
            if let Some(rest) = tok.strip_prefix('/') {
                let key = rest.trim_start_matches(|c: char| c.is_ascii_lowercase());
                if stack.pop() != Some(key.to_string()) {
                    return false;
                }
            } else if !tok.starts_with(['x', 'p']) {
                stack.push(tok.trim_start_matches(|c: char| c.is_ascii_lowercase()).to_string());
            }
        }
        stack.is_empty()
    }

    #[test]
    fn crossed_pairs_become_well_formed() {
        let s = vec![
            Inline::tag("a", TagKind::Open, "<b>"),
            Inline::text("1"),
            Inline::tag("b", TagKind::Open, "<i>"),
            Inline::text("2"),
            Inline::tag("a", TagKind::Close, "</b>"),
            Inline::text("3"),
            Inline::tag("b", TagKind::Close, "</i>"),
        ];
        let (t, map) = to_locos_text(&s);
        assert_eq!(t, "\u{27E6}g1\u{27E7}1\u{27E6}x2\u{27E7}2\u{27E6}/g1\u{27E7}3\u{27E6}x3\u{27E7}");
        assert!(well_formed(&t));
        assert_eq!(from_locos_text(&t, &map), s);
    }

    #[test]
    fn html_keeps_its_own_tokens() {
        // LocOS html_filter: an open with no close is a pair token, crossed pairs keep their numbers
        let s = vec![
            Inline::tag("1", TagKind::Open, "<a>"),
            Inline::text("Hi "),
            Inline::tag("2", TagKind::Open, "<b>"),
            Inline::text("1"),
            Inline::tag("3", TagKind::Open, "<i>"),
            Inline::text("2"),
            Inline::tag("2", TagKind::Close, "</b>"),
            Inline::text("3"),
            Inline::tag("3", TagKind::Close, "</i>"),
            Inline::tag("4", TagKind::Standalone, "<br>"),
            Inline::tag("9", TagKind::Close, "</u>"),
        ];
        let (t, map) = to_locos_text_for("html", &s);
        assert_eq!(
            t,
            "\u{27E6}g1\u{27E7}Hi \u{27E6}g2\u{27E7}1\u{27E6}g3\u{27E7}2\u{27E6}/g2\u{27E7}3\u{27E6}/g3\u{27E7}\u{27E6}x4\u{27E7}\u{27E6}x5\u{27E7}"
        );
        assert_eq!(map["1"].letter, 'g');
        assert_eq!(map["1"].close, None);
        assert_eq!(from_locos_text(&t, &map), s);
        // the other formats keep the well-formed conversion, with their pair letter
        assert_eq!(to_locos_text_for("md", &s), to_locos_text_as(&s, 'g'));
        assert_eq!(to_locos_text_for("docx", &s), to_locos_text_as(&s, 'r'));
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
        assert!(well_formed(&t));
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
