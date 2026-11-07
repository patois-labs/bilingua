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

use crate::{merge_text, CodeData, Inline, TagKind};
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
    convert(inl, pair_letter, pairs(inl), None, None)
}

/// The pair-token letter a format's LocOS filter uses (the adapter's `_PAIR_LETTER`): `r` for docx and pptx runs
/// and for the runs of an xlsx text box (LocOS tokenizes those with its PowerPoint filter), else [`PAIR_LETTER`].
pub fn locos_pair_letter(format: &str) -> char {
    if matches!(format, "docx" | "pptx" | "xlsx") {
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

/// Formats whose LocOS filter does not number its tokens in reading order, so the tag ids carry LocOS's numbers
/// ([`to_locos_text_numbered`], the one id-numbering path): DOCX numbers the runs `r1..rN` first and the object
/// groups from `x<N+1>`; PPTX, and the text boxes of an XLSX workbook (LocOS tokenizes those with its PowerPoint
/// filter), number the runs first and the breaks and fields after them. XLSX cells carry no tags, so the rule
/// changes nothing for them. Every other format is numbered by position.
const LOCOS_NUMBERED_BY_ID: &[&str] = &["docx", "pptx", "xlsx"];

/// A unit's inline codes as the LocOS filter of `format` writes them: the format's pair letter
/// ([`locos_pair_letter`]), and for the formats whose filter keeps its own tag tokens (`html`) every `Open` a
/// pair token and every `Close` the closing token of the latest open of its id, crossing or not, an `Open` with
/// no close a pair token with no closing one; a `Close` with no open stays an `x` token. For the formats whose
/// tag ids are LocOS's token numbers (`docx`, `pptx`, `xlsx`): [`to_locos_text_numbered`]. Other formats:
/// [`to_locos_text_as`].
/// [`from_locos_text`] reads them all back.
pub fn to_locos_text_for(format: &str, inl: &[Inline]) -> (String, LocosMap) {
    let letter = locos_pair_letter(format);
    if LOCOS_PAIRS_BY_ID.contains(&format) {
        convert(inl, letter, pairs_by_id(inl), None, None)
    } else if LOCOS_NUMBERED_BY_ID.contains(&format) {
        to_locos_text_numbered(inl, letter)
    } else {
        to_locos_text_as(inl, letter)
    }
}

/// Formats whose LocOS filter writes two pair letters (IDML and ICML, `idml_filter._tokenize`): a
/// CharacterStyleRange is an `r` pair, every other wrap (hyperlink, XML tag, tracked change) a `g` pair. The
/// Bilingua code of a range is labelled `run`.
const LOCOS_RUN_PAIRS: &[&str] = &["idml", "icml"];

/// A segment's inline codes as the LocOS filter of `format` writes them, knowing the unit's codes: what
/// [`to_locos_text_for`] gives, except for IDML and ICML, where a pair whose code is labelled `run` (a
/// CharacterStyleRange) is an `r` pair (`⟦r1⟧bold⟦/r1⟧`) and every other pair a `g` pair, as LocOS writes them.
/// [`crate::profile::locos_units`] numbers every segment this way.
pub fn to_locos_text_coded(format: &str, inl: &[Inline], codes: &BTreeMap<String, CodeData>) -> (String, LocosMap) {
    if !LOCOS_RUN_PAIRS.contains(&format) {
        return to_locos_text_for(format, inl);
    }
    let run = |id: &str| codes.get(id).filter(|c| c.label.as_deref() == Some("run")).map(|_| 'r');
    convert(inl, PAIR_LETTER, pairs(inl), None, Some(&run))
}

/// Like [`to_locos_text_as`], but each token takes its number from its tag's id when the ids are LocOS's own token
/// numbers: the formats whose LocOS filter does not number its tokens in reading order (DOCX, PPTX and XLSX text
/// boxes under the profile, where LocOS numbers the runs `r1..rN` first and the breaks, fields and objects after
/// them, so a paragraph reads `⟦r1⟧a⟦/r1⟧⟦x3⟧⟦r2⟧b⟦/r2⟧`). The ids are used only when every tag id is a positive decimal number (no sign, no
/// leading zero) and no number would stand for two tokens; otherwise this is exactly [`to_locos_text_as`]. The
/// map is keyed by those numbers, so [`from_locos_text`] reads the text back as usual.
pub fn to_locos_text_numbered(inl: &[Inline], pair_letter: char) -> (String, LocosMap) {
    let partner = pairs(inl);
    let mut nums = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (pos, i) in inl.iter().enumerate() {
        let Inline::Tag { id, kind, .. } = i else { continue };
        // a paired close takes its open's number; every other tag is a token of its own
        if *kind == TagKind::Close && partner[pos].is_some() {
            continue;
        }
        let ok = !id.is_empty() && id.len() <= 18 && id.bytes().all(|b| b.is_ascii_digit()) && !id.starts_with('0');
        match id.parse::<usize>() {
            Ok(n) if ok && seen.insert(n) => nums.push(n),
            _ => return to_locos_text_as(inl, pair_letter),
        }
    }
    // source text that already looks like a token with one of these numbers: the numbers give way (see `convert`)
    let reserved = reserved_numbers(inl);
    if nums.iter().any(|n| reserved.contains(n)) {
        return to_locos_text_as(inl, pair_letter);
    }
    convert(inl, pair_letter, partner, Some(&nums), None)
}

/// The numbers of token-like text (`⟦x2⟧`, `⟦/g1⟧`, `⟦3⟧`) that the plain text of `inl` already holds. LocOS reads
/// such text in a source as one of its tokens and drops or moves it on export; here a real code never takes the
/// number of a look-alike, so [`from_locos_text`] can only read a look-alike as the text it is (it names no code
/// in the map).
fn reserved_numbers(inl: &[Inline]) -> std::collections::BTreeSet<usize> {
    let mut out = std::collections::BTreeSet::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut std::collections::BTreeSet<usize>| {
        let mut rest = run.as_str();
        while let Some(at) = rest.find(OPEN) {
            rest = &rest[at..];
            match scan_token(rest) {
                Some((len, _, _, num)) => {
                    if let Ok(n) = num.parse() {
                        out.insert(n);
                    }
                    rest = &rest[len..];
                }
                None => rest = &rest[OPEN.len_utf8()..],
            }
        }
        run.clear();
    };
    for i in inl {
        match i {
            Inline::Text(t) => run.push_str(t),
            Inline::Tag { .. } => flush(&mut run, &mut out),
        }
    }
    flush(&mut run, &mut out);
    out
}

/// The shared body; `nums[k]` is the number of the k-th token (`None`: 1, 2, 3 ... in reading order).
/// A pair's letter by its tag id, when it is not the format's one ([`to_locos_text_coded`]).
type PairLetterOf<'a> = &'a dyn Fn(&str) -> Option<char>;

fn convert(inl: &[Inline], pair_letter: char, partner: Vec<Option<usize>>, nums: Option<&[usize]>, letter_of: Option<PairLetterOf>) -> (String, LocosMap) {
    let pair = if pair_letter.is_ascii_lowercase() && pair_letter != POINT_LETTER && pair_letter != FREE_LETTER {
        pair_letter
    } else {
        PAIR_LETTER
    };
    // positional numbers skip the numbers that look-alike text in the source uses
    let reserved = if nums.is_some() { Default::default() } else { reserved_numbers(inl) };
    let mut last = 0usize;
    let mut number = |k: usize| match nums {
        Some(v) => v.get(k - 1).copied().unwrap_or(k),
        None => {
            last += 1;
            while reserved.contains(&last) {
                last += 1;
            }
            last
        }
    };
    let mut text = String::new();
    let mut map = LocosMap::new();
    // token number of each paired Open, by its position in `inl`
    let mut open_num: BTreeMap<usize, usize> = BTreeMap::new();
    let mut k = 0usize;
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
                k += 1;
                let n = number(k);
                open_num.insert(pos, n);
                let pair = letter_of.and_then(|f| f(id)).unwrap_or(pair);
                map.insert(n.to_string(), code(pair, Some(display), None, false, None));
                text.push_str(&format!("{OPEN}{pair}{n}{CLOSE}"));
            }
            (TagKind::Close, Some(o)) => {
                let num = open_num.get(&o).copied().unwrap_or_default();
                // the close takes its open's letter
                let mut letter = pair;
                if let Some(c) = map.get_mut(&num.to_string()) {
                    c.close = Some(display.clone());
                    letter = c.letter;
                }
                text.push_str(&format!("{OPEN}/{letter}{num}{CLOSE}"));
            }
            (TagKind::Open, None) => {
                k += 1;
                let n = number(k);
                map.insert(n.to_string(), code(POINT_LETTER, Some(display), None, false, Some(TagKind::Open)));
                text.push_str(&format!("{OPEN}{POINT_LETTER}{n}{CLOSE}"));
            }
            (TagKind::Close, None) => {
                k += 1;
                let n = number(k);
                map.insert(n.to_string(), code(POINT_LETTER, None, Some(display), false, Some(TagKind::Close)));
                text.push_str(&format!("{OPEN}{POINT_LETTER}{n}{CLOSE}"));
            }
            (TagKind::Standalone, _) => {
                k += 1;
                let n = number(k);
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
    parse_tokens(text, map, false)
}

/// Formats whose LocOS filter drops a token of the wrong kind that names one of the unit's codes (a pair token
/// `\u{27E6}g1\u{27E7}`, `\u{27E6}/g1\u{27E7}` for a point code), where other formats keep it as text: LocOS
/// `toml_filter._detokenize` and `js_module_filter._render` (#814.1, g12). A token whose number names no code stays
/// text in every format.
const LOCOS_DROPS_MISMATCH: &[&str] = &["toml", "js"];

/// [`parse_locos_text`] with the token rules of `format`'s LocOS filter: for TOML and JavaScript modules a token of
/// the wrong kind for a code of the unit is dropped (LocOS writes nothing for it) instead of kept as text; it is
/// still listed as [`TokenProblem::KindMismatch`]. Every other format reads as [`parse_locos_text`].
pub fn parse_locos_text_for(format: &str, text: &str, map: &LocosMap) -> (Vec<Inline>, Vec<TokenProblem>) {
    let (inl, problems, _) = repair_locos_text_for(format, text, map);
    (inl, problems)
}

/// Formats whose LocOS writer repairs the tags of a translation instead of refusing it (every CAT XML writer:
/// `cat_xliff._render` for sdlxliff, mqxliff, mxliff, txlf, txml, ttx and the xlz, mqxlz and Trados package
/// members, `xliff_filter._rebuild_target` for plain XLIFF). See [`repair_locos_text_for`].
const LOCOS_REPAIRS_TAGS: &[&str] = &["xliff", "sdlxliff", "mqxliff", "mxliff", "txlf", "txml", "ttx", "xlz", "mqxlz", "sdlppx"];

/// True when the LocOS writer of `format` repairs a translation's tags ([`repair_locos_text_for`]) rather than
/// refusing them: the XLIFF family, TXML, TTX and the XLIFF packages.
pub fn locos_repairs_tags(format: &str) -> bool {
    LOCOS_REPAIRS_TAGS.contains(&format)
}

/// One repair [`repair_locos_text_for`] made to a translation's tokens. The text around a token always stays; only
/// the token's tag is dropped, or a close added.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TagRepair {
    /// A token whose code was already written: dropped (a repeated pair keeps its inner text, without the tags).
    Repeated {
        /// The token.
        token: String,
    },
    /// A closing token with no open pair of its number on top (or a closing token for a standalone code): dropped.
    StrayClose {
        /// The token.
        token: String,
    },
    /// A pair opened and never closed: closed at the end of the segment.
    Unclosed {
        /// The opening token.
        token: String,
    },
    /// A pair code written as a point token (`\u{27E6}x1\u{27E7}` for `\u{27E6}g1\u{27E7}`): written empty, its open
    /// and close tags side by side.
    PairAsPoint {
        /// The token.
        token: String,
    },
}

/// [`parse_locos_text_for`] that also lists the tag repairs. For the formats whose LocOS writer repairs tags
/// ([`locos_repairs_tags`]: the XLIFF family, TXML, TTX, the XLIFF packages) every token whose number is in the map
/// becomes its tag whatever its letter (a pair named by a point token becomes its open and close side by side, a
/// closing token for a standalone code a close of its id), repeated or unbalanced as typed: their Bilingua writers
/// apply LocOS's rule (`cat_xliff._render`) while writing, after the target's edges are stripped, so the white space
/// next to a dropped tag comes out as LocOS writes it. The rule: a code is written at most once (a repeated pair
/// loses its tags, its inner text stays in place; a repeated point is dropped); a closing token that does not close
/// the pair on top is dropped; a pair still open at the end is closed there; a pair named by a point token is
/// written empty. Every repair the writer will make is listed, so a caller can show it (LocOS's QA flags the same
/// target through its placeholder check). A token whose number is not in the map stays text, as everywhere. Other
/// formats: the plain reading, no repairs.
pub fn repair_locos_text_for(format: &str, text: &str, map: &LocosMap) -> (Vec<Inline>, Vec<TokenProblem>, Vec<TagRepair>) {
    if locos_repairs_tags(format) {
        return parse_tokens_repairing(text, map);
    }
    let (inl, problems) = parse_tokens(text, map, LOCOS_DROPS_MISMATCH.contains(&format));
    (inl, problems, Vec::new())
}

/// The tag a point token stands for (a standalone code, or an `Open` or `Close` with no partner).
fn point_tag(code: &LocosCode) -> Inline {
    match code.unpaired {
        Some(TagKind::Close) => Inline::Tag { id: code.id.clone(), kind: TagKind::Close, display: code.close.clone().unwrap_or_default(), free: false },
        Some(TagKind::Open) => Inline::Tag { id: code.id.clone(), kind: TagKind::Open, display: code.open.clone().unwrap_or_default(), free: false },
        _ => Inline::Tag { id: code.id.clone(), kind: TagKind::Standalone, display: code.open.clone().unwrap_or_default(), free: code.free },
    }
}

fn pair_tag(code: &LocosCode, kind: TagKind) -> Inline {
    let display = if kind == TagKind::Close { code.close.clone() } else { code.open.clone() };
    Inline::Tag { id: code.id.clone(), kind, display: display.unwrap_or_default(), free: false }
}

/// The CAT writers' token rule (`cat_xliff._render`), see [`repair_locos_text_for`]: every token as its tag (nothing
/// dropped, the writer does that), the problems, and the repairs the writer will make.
fn parse_tokens_repairing(text: &str, map: &LocosMap) -> (Vec<Inline>, Vec<TokenProblem>, Vec<TagRepair>) {
    let mut out: Vec<Inline> = Vec::new();
    let mut problems = Vec::new();
    let mut repairs = Vec::new();
    let mut lit = String::new();
    let mut used: std::collections::BTreeSet<String> = Default::default();
    // the pairs open now: (number, opening token, written); a pair not written (a repeat) only eats its close
    let mut stack: Vec<(String, String, bool)> = Vec::new();
    let mut rest = text;
    let mut pos = 0usize;
    while let Some(ch) = rest.chars().next() {
        let len = ch.len_utf8();
        if ch == OPEN {
            if let Some((tok_len, closing, letter, num)) = scan_token(rest) {
                let token = rest[..tok_len].to_string();
                rest = &rest[tok_len..];
                pos += tok_len;
                let Some(code) = map.get(&num) else {
                    problems.push(TokenProblem::Unknown { token: token.clone() });
                    lit.push_str(&token);
                    continue;
                };
                if !lit.is_empty() {
                    out.push(Inline::Text(std::mem::take(&mut lit)));
                }
                let code_point = code.letter == POINT_LETTER || code.letter == FREE_LETTER;
                let typed_point = letter.is_some_and(|l| l == POINT_LETTER || l == FREE_LETTER);
                if closing {
                    // a closing token for a standalone code is a close of its id: the writer drops it
                    out.push(if code_point {
                        Inline::Tag { id: code.id.clone(), kind: TagKind::Close, display: code.close.clone().unwrap_or_default(), free: false }
                    } else {
                        pair_tag(code, TagKind::Close)
                    });
                    if !code_point && stack.last().is_some_and(|f| f.0 == num) {
                        stack.pop();
                    } else {
                        repairs.push(TagRepair::StrayClose { token });
                    }
                } else if code_point {
                    out.push(point_tag(code));
                    if !used.insert(num) {
                        repairs.push(TagRepair::Repeated { token });
                    }
                } else if typed_point {
                    // written empty (`cat_xliff._render`): an open and a close side by side read the same way
                    out.push(pair_tag(code, TagKind::Open));
                    out.push(pair_tag(code, TagKind::Close));
                    repairs.push(if used.insert(num) { TagRepair::PairAsPoint { token } } else { TagRepair::Repeated { token } });
                } else {
                    out.push(pair_tag(code, TagKind::Open));
                    let first = used.insert(num.clone());
                    if !first {
                        repairs.push(TagRepair::Repeated { token: token.clone() });
                    }
                    stack.push((num, token, first));
                }
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
    for (_, token, written) in stack.into_iter().rev() {
        if written {
            repairs.push(TagRepair::Unclosed { token });
        }
    }
    (merge_text(out), problems, repairs)
}

/// The CAT writers' rule ([`repair_locos_text_for`]) on an inline list, for a target given as tags rather than
/// LocOS text: each source code is written at most as often as the source has it (a repeated pair loses its tags,
/// its text stays), a close that does not close the pair on top is dropped, a pair left open is closed at the end.
/// A tag the source lacks is kept, so the model's check still refuses it.
pub(crate) fn repair_tags(source: &[Inline], target: Vec<Inline>) -> Vec<Inline> {
    use std::collections::HashMap;
    let partner = pairs(source);
    let mut pair_allowed: HashMap<&str, usize> = HashMap::new();
    let mut point_allowed: HashMap<(&str, TagKind), usize> = HashMap::new();
    let mut close_display: HashMap<&str, &str> = HashMap::new();
    for (pos, i) in source.iter().enumerate() {
        let Inline::Tag { id, kind, display, .. } = i else { continue };
        match (kind, partner[pos]) {
            (TagKind::Open, Some(_)) => *pair_allowed.entry(id.as_str()).or_default() += 1,
            (TagKind::Close, Some(_)) => {
                close_display.entry(id.as_str()).or_insert(display.as_str());
            }
            _ => *point_allowed.entry((id.as_str(), *kind)).or_default() += 1,
        }
    }
    let known: std::collections::HashSet<&str> = source.iter().filter_map(|i| if let Inline::Tag { id, .. } = i { Some(id.as_str()) } else { None }).collect();
    let mut pair_used: HashMap<String, usize> = HashMap::new();
    let mut point_used: HashMap<(String, TagKind), usize> = HashMap::new();
    let mut stack: Vec<(String, bool)> = Vec::new();
    let mut out = Vec::with_capacity(target.len());
    for i in target {
        let Inline::Tag { id, kind, .. } = &i else {
            out.push(i);
            continue;
        };
        let paired = pair_allowed.get(id.as_str()).copied().unwrap_or(0);
        match kind {
            TagKind::Open if paired > 0 => {
                let n = pair_used.entry(id.clone()).or_default();
                let write = *n < paired;
                if write {
                    *n += 1;
                }
                stack.push((id.clone(), write));
                if write {
                    out.push(i);
                }
            }
            TagKind::Close if paired > 0 && stack.last().is_some_and(|(top, _)| top == id) => {
                if let Some((_, true)) = stack.pop() {
                    out.push(i);
                }
            }
            TagKind::Close if paired > 0 && !point_allowed.contains_key(&(id.as_str(), TagKind::Close)) => {}
            _ => match point_allowed.get(&(id.as_str(), *kind)) {
                // a code of the source written as another kind (a close for a standalone code): dropped
                None if known.contains(id.as_str()) => {}
                // not a code of the source: kept, the model's check names it
                None => out.push(i),
                Some(max) => {
                    let n = point_used.entry((id.clone(), *kind)).or_default();
                    if *n < *max {
                        *n += 1;
                        out.push(i);
                    }
                }
            },
        }
    }
    for (id, written) in stack.into_iter().rev() {
        if written {
            let display = close_display.get(id.as_str()).map(|d| d.to_string()).unwrap_or_default();
            out.push(Inline::Tag { id, kind: TagKind::Close, display, free: false });
        }
    }
    merge_text(out)
}

fn parse_tokens(text: &str, map: &LocosMap, drop_mismatch: bool) -> (Vec<Inline>, Vec<TokenProblem>) {
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
                        if !drop_mismatch {
                            lit.push_str(token);
                        }
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

/// Hides the token brackets of literal text from a writer that cuts LocOS text at its tokens (the Office writers
/// of the LocOS profile). `\u{27E6}` becomes U+E000 U+E001 and U+E000 becomes U+E000 U+E000, so the result holds
/// no token and [`unescape_literal`] gives back the text byte for byte. Shared by the format crates; not part of
/// the semver API.
#[doc(hidden)]
pub fn escape_literal(s: &str) -> String {
    if !s.contains([OPEN, LIT_MARK]) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            OPEN => out.push_str("\u{e000}\u{e001}"),
            LIT_MARK => out.push_str("\u{e000}\u{e000}"),
            c => out.push(c),
        }
    }
    out
}

/// The inverse of [`escape_literal`]. A U+E000 that starts no pair (only text that was not escaped holds one)
/// stays as it is. Not part of the semver API.
#[doc(hidden)]
pub fn unescape_literal(s: &str) -> String {
    if !s.contains(LIT_MARK) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == LIT_MARK {
            match it.peek() {
                Some('\u{e001}') => {
                    it.next();
                    out.push(OPEN);
                    continue;
                }
                Some(&LIT_MARK) => {
                    it.next();
                }
                _ => {}
            }
        }
        out.push(c);
    }
    out
}

const LIT_MARK: char = '\u{e000}';

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_escape_round_trips_every_short_string() {
        // every string of up to six characters over the characters that matter: the escape never holds a token
        // bracket and always comes back byte for byte
        let alpha = ['\u{27e6}', '\u{27e7}', '\u{e000}', '\u{e001}', 'x', '1'];
        let mut cur: Vec<String> = vec![String::new()];
        for _ in 0..6 {
            let mut next = Vec::new();
            for s in &cur {
                for c in alpha {
                    let mut t = s.clone();
                    t.push(c);
                    let e = escape_literal(&t);
                    assert!(!e.contains('\u{27e6}'), "{t:?}");
                    assert_eq!(unescape_literal(&e), t, "{t:?}");
                    next.push(t);
                }
            }
            cur = next;
        }
        // the escape of a concatenation is the concatenation of the escapes, so text cut at a token unescapes
        // piece by piece
        assert_eq!(escape_literal("a\u{e000}\u{27e6}x1\u{27e7}\u{e001}"), "a\u{e000}\u{e000}\u{e000}\u{e001}x1\u{27e7}\u{e001}");
        assert_eq!(unescape_literal("lone \u{e000} mark"), "lone \u{e000} mark");
    }

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
        // one id-numbering path for docx, pptx and xlsx text boxes; every other format keeps the positional numbering
        for f in ["docx", "pptx", "xlsx"] {
            assert_eq!(to_locos_text_for(f, &inl), to_locos_text_numbered(&inl, 'r'), "{f}");
        }
        for f in ["xliff", "html", "po", "markdown", "json", "odt", ""] {
            assert_eq!(to_locos_text_for(f, &inl).0.matches("\u{27e6}x2\u{27e7}").count(), 1, "{f}");
        }
        assert_eq!(to_locos_text_for("xliff", &inl).0, "\u{27e6}g1\u{27e7}a\u{27e6}/g1\u{27e7}\u{27e6}x2\u{27e7}\u{27e6}g3\u{27e7}b\u{27e6}/g3\u{27e7}");
        // a bare xlsx cell has no tags: the rule changes nothing
        let cell = vec![Inline::text("Total")];
        assert_eq!(to_locos_text_for("xlsx", &cell), to_locos_text_as(&cell, 'r'));
        let dup = vec![Inline::tag("1", TagKind::Standalone, "o"), Inline::tag("1", TagKind::Standalone, "o")];
        assert_eq!(to_locos_text_for("docx", &dup).0, "\u{27e6}x1\u{27e7}\u{27e6}x2\u{27e7}");
        let odd = vec![Inline::tag("x", TagKind::Standalone, "o"), Inline::tag("07", TagKind::Standalone, "o")];
        assert_eq!(to_locos_text_for("docx", &odd).0, "\u{27e6}x1\u{27e7}\u{27e6}x2\u{27e7}");
    }

    /// W4 idml-1 / icml-1: LocOS writes a CharacterStyleRange as an `r` pair and every other wrap as a `g` pair.
    #[test]
    fn idml_and_icml_runs_are_r_pairs() {
        let inl = vec![
            Inline::tag("1", TagKind::Open, "<CharacterStyleRange>"),
            Inline::text("bold "),
            Inline::tag("2", TagKind::Open, "<HyperlinkTextSource>"),
            Inline::text("link"),
            Inline::tag("2", TagKind::Close, "</HyperlinkTextSource>"),
            Inline::tag("3", TagKind::Standalone, "<Br/>"),
            Inline::tag("1", TagKind::Close, "</CharacterStyleRange>"),
        ];
        let mut codes = BTreeMap::new();
        codes.insert("1".to_string(), CodeData::paired("<CharacterStyleRange>", "</CharacterStyleRange>").with_label("run"));
        codes.insert("2".to_string(), CodeData::paired("<HyperlinkTextSource>", "</HyperlinkTextSource>").with_label("link"));
        codes.insert("3".to_string(), CodeData::standalone("<Br/>").with_label("br"));
        let want = "\u{27e6}r1\u{27e7}bold \u{27e6}g2\u{27e7}link\u{27e6}/g2\u{27e7}\u{27e6}x3\u{27e7}\u{27e6}/r1\u{27e7}";
        for f in ["idml", "icml"] {
            let (t, map) = to_locos_text_coded(f, &inl, &codes);
            assert_eq!(t, want, "{f}");
            assert_eq!(map["1"].letter, 'r');
            assert_eq!(from_locos_text(&t, &map), inl, "{f}");
            // the reader is lenient about a pair's letter (as before): a run written as `g` is still the run
            let (back, problems) = parse_locos_text(&t.replace("r1", "g1"), &map);
            assert!(problems.is_empty() && back == inl);
        }
        // other formats ignore the labels; without codes every pair is `g`, as before
        assert_eq!(to_locos_text_coded("odt", &inl, &codes), to_locos_text_for("odt", &inl));
        assert_eq!(to_locos_text_coded("idml", &inl, &BTreeMap::new()).0.matches("\u{27e6}g").count(), 2);
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
    fn token_look_alikes_in_the_text_are_text() {
        // the text says "use ⟦x2⟧ and ⟦g1⟧": no real code may take number 1 or 2
        let inl = vec![
            Inline::text("Use \u{27E6}x2\u{27E7} and \u{27E6}g1\u{27E7} "),
            Inline::tag("a", TagKind::Open, "<b>"),
            Inline::text("here"),
            Inline::tag("a", TagKind::Close, "</b>"),
            Inline::tag("b", TagKind::Standalone, "<br/>"),
        ];
        for f in ["xliff", "html", "docx"] {
            let (t, map) = to_locos_text_for(f, &inl);
            assert!(!map.contains_key("1") && !map.contains_key("2"), "{f}: {map:?}");
            assert_eq!(from_locos_text(&t, &map), inl, "{f}: {t}");
            // a translator keeps the text and moves the code
            let moved = t.replace("here", "HERE");
            let back = from_locos_text(&moved, &map);
            assert!(matches!(&back[0], Inline::Text(x) if x.starts_with("Use \u{27E6}x2\u{27E7} and \u{27E6}g1\u{27E7}")), "{f}");
        }
        // numbers with gaps: a translator who swaps the codes gets each tag back by its own number
        let inl = vec![
            Inline::text("\u{27E6}x2\u{27E7} "),
            Inline::tag("1", TagKind::Open, "<b>"),
            Inline::text("one"),
            Inline::tag("1", TagKind::Close, "</b>"),
            Inline::tag("3", TagKind::Standalone, "<br/>"),
            Inline::text("two"),
        ];
        for f in ["html", "docx"] {
            let (t, map) = to_locos_text_for(f, &inl);
            let p = locos_pair_letter(f);
            let (n_pair, n_point) = (map.iter().find(|(_, c)| c.id == "1").unwrap().0, map.iter().find(|(_, c)| c.id == "3").unwrap().0);
            assert!(!map.contains_key("2"), "{f}: {map:?}");
            let swapped = format!("\u{27E6}x2\u{27E7} two\u{27E6}x{n_point}\u{27E7}\u{27E6}{p}{n_pair}\u{27E7}one\u{27E6}/{p}{n_pair}\u{27E7}");
            let (back, problems) = parse_locos_text(&swapped, &map);
            assert_eq!(problems.len(), 1, "{f}: only the look-alike is unknown: {problems:?} ({t})");
            assert_eq!(
                back,
                vec![
                    Inline::text("\u{27E6}x2\u{27E7} two"),
                    Inline::tag("3", TagKind::Standalone, "<br/>"),
                    Inline::tag("1", TagKind::Open, "<b>"),
                    Inline::text("one"),
                    Inline::tag("1", TagKind::Close, "</b>"),
                ],
                "{f}"
            );
        }
        // a letterless look-alike ("⟦1⟧") also keeps its number free
        let inl = vec![Inline::text("\u{27E6}1\u{27E7} "), Inline::tag("a", TagKind::Standalone, "<br/>")];
        let (t, map) = to_locos_text(&inl);
        assert_eq!(t, "\u{27E6}1\u{27E7} \u{27E6}x2\u{27E7}");
        assert_eq!(from_locos_text(&t, &map), inl);
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
    fn numbered_by_id_like_locos_pptx() {
        // LocOS's PPTX numbering: runs r1, r2 first, the break and the field after them
        let s = vec![
            Inline::tag("1", TagKind::Open, "<a:r>"),
            Inline::text("a"),
            Inline::tag("1", TagKind::Close, "</a:r>"),
            Inline::tag("3", TagKind::Standalone, "<a:br/>"),
            Inline::tag("2", TagKind::Open, "<a:r>"),
            Inline::text("b"),
            Inline::tag("2", TagKind::Close, "</a:r>"),
            Inline::tag("4", TagKind::Standalone, "<a:fld/>"),
        ];
        let (t, map) = to_locos_text_numbered(&s, 'r');
        assert_eq!(t, "\u{27E6}r1\u{27E7}a\u{27E6}/r1\u{27E7}\u{27E6}x3\u{27E7}\u{27E6}r2\u{27E7}b\u{27E6}/r2\u{27E7}\u{27E6}x4\u{27E7}");
        assert!(well_formed(&t));
        assert_eq!(from_locos_text(&t, &map), s);
        // ids that are not usable token numbers: reading order, exactly as to_locos_text_as
        for bad in [
            vec![Inline::tag("a", TagKind::Standalone, "x")],
            vec![Inline::tag("0", TagKind::Standalone, "x")],
            vec![Inline::tag("01", TagKind::Standalone, "x")],
            vec![Inline::tag("+2", TagKind::Standalone, "x")],
            vec![Inline::tag("2", TagKind::Standalone, "x"), Inline::tag("2", TagKind::Standalone, "y")],
            vec![Inline::tag("9", TagKind::Open, "x"), Inline::tag("9", TagKind::Open, "y")],
            vec![Inline::tag("99999999999999999999999", TagKind::Standalone, "x")],
        ] {
            assert_eq!(to_locos_text_numbered(&bad, 'r'), to_locos_text_as(&bad, 'r'), "{bad:?}");
        }
        assert_eq!(to_locos_text_numbered(&sample(), 'g'), to_locos_text(&sample()));
        assert_eq!(to_locos_text_numbered(&[], 'r'), (String::new(), LocosMap::new()));
        // the per-format entry point picks it for pptx
        assert_eq!(to_locos_text_for("pptx", &s), (t, map));
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

    /// #814.1 (g12): TOML and JS drop a pair token naming a point code; an unknown number stays text; other
    /// formats keep the wrong-kind token as text.
    #[test]
    fn wrong_kind_tokens_are_dropped_for_toml_and_js() {
        let src = vec![Inline::text("Reading time "), Inline::tag("1", TagKind::Standalone, "{{ .Count }}"), Inline::text(" min")];
        let (_, map) = to_locos_text(&src);
        let t = "Lesezeit \u{27E6}x9\u{27E7} \u{27E6}g1\u{27E7}Min\u{27E6}/g1\u{27E7} \u{27E6}x1\u{27E7}";
        for f in ["toml", "js"] {
            let (inl, problems) = parse_locos_text_for(f, t, &map);
            assert_eq!(
                inl,
                vec![Inline::text("Lesezeit \u{27E6}x9\u{27E7} Min "), Inline::tag("1", TagKind::Standalone, "{{ .Count }}")],
                "{f}"
            );
            assert_eq!(problems.iter().filter(|p| matches!(p, TokenProblem::KindMismatch { .. })).count(), 2, "{f}");
            assert!(problems.contains(&TokenProblem::Unknown { token: "\u{27E6}x9\u{27E7}".into() }), "{f}");
        }
        let (kept, _) = parse_locos_text_for("json", t, &map);
        assert_eq!(kept, parse_locos_text(t, &map).0);
        assert!(matches!(&kept[0], Inline::Text(x) if x.contains("\u{27E6}g1\u{27E7}Min")));
    }

    /// B-SIDE 8: the CAT writers' rule (LocOS `cat_xliff._render`): a repeated pair loses its tags and keeps its
    /// text, a stray close is dropped, an unclosed pair is closed at the end, a pair named by a point token is
    /// written empty, a repeated point is dropped; each repair is listed. Other formats read as before.
    #[test]
    fn cat_formats_repair_tags_like_locos() {
        let open = Inline::tag("1", TagKind::Open, "<g id=\"1\">");
        let close = Inline::tag("1", TagKind::Close, "</g>");
        let x = Inline::tag("2", TagKind::Standalone, "<x id=\"2\"/>");
        let src = vec![Inline::text("Click "), open.clone(), Inline::text("here"), close.clone(), x.clone()];
        let (text, map) = to_locos_text(&src);
        assert_eq!(text, "Click \u{27E6}g1\u{27E7}here\u{27E6}/g1\u{27E7}\u{27E6}x2\u{27E7}");
        let cases: Vec<(&str, Vec<Inline>, Vec<&str>)> = vec![
            // verify_filter_roundtrip_509: the second frame loses its tags, its text stays in place
            (
                "bitte \u{27E6}g1\u{27E7}Klick\u{27E6}/g1\u{27E7} hier \u{27E6}g1\u{27E7}doppelt\u{27E6}/g1\u{27E7} jetzt",
                vec![Inline::text("bitte "), open.clone(), Inline::text("Klick"), close.clone(), Inline::text(" hier doppelt jetzt")],
                vec!["repeated"],
            ),
            // verify_cat_xliff_filter 165: an open used again with no close of its own is dropped, the pair closed at the end
            (
                "\u{27E6}g1\u{27E7}A \u{27E6}x2\u{27E7}B \u{27E6}g1\u{27E7}C",
                vec![open.clone(), Inline::text("A "), x.clone(), Inline::text("B C"), close.clone()],
                vec!["repeated", "unclosed"],
            ),
            ("\u{27E6}/g1\u{27E7}Hi \u{27E6}/x2\u{27E7}", vec![Inline::text("Hi ")], vec!["stray_close", "stray_close"]),
            ("Hi \u{27E6}x1\u{27E7} you", vec![Inline::text("Hi "), open.clone(), close.clone(), Inline::text(" you")], vec!["pair_as_point"]),
            ("\u{27E6}x2\u{27E7}a\u{27E6}x2\u{27E7}b\u{27E6}g2\u{27E7}", vec![x.clone(), Inline::text("ab")], vec!["repeated", "repeated"]),
            // the letter does not matter, only the number: a close typed with a point letter closes the pair
            ("\u{27E6}g1\u{27E7}a\u{27E6}/x1\u{27E7}", vec![open.clone(), Inline::text("a"), close.clone()], vec![]),
            // a number the map lacks stays text
            ("a \u{27E6}g9\u{27E7}", vec![Inline::text("a \u{27E6}g9\u{27E7}")], vec![]),
        ];
        for (t, want, kinds) in cases {
            for f in ["sdlxliff", "xliff", "mqxliff", "mxliff", "txlf", "txml", "ttx", "xlz", "mqxlz", "sdlppx"] {
                let (inl, _, repairs) = repair_locos_text_for(f, t, &map);
                // every token stays a tag (the writer applies the rule); the rule on the inlines gives LocOS's result
                assert_eq!(repair_tags(&src, inl.clone()), want, "{f}: {t}");
                let got: Vec<&str> = repairs
                    .iter()
                    .map(|r| match r {
                        TagRepair::Repeated { .. } => "repeated",
                        TagRepair::StrayClose { .. } => "stray_close",
                        TagRepair::Unclosed { .. } => "unclosed",
                        TagRepair::PairAsPoint { .. } => "pair_as_point",
                    })
                    .collect();
                assert_eq!(got, kinds, "{f}: {t}");
                // the inline repair leaves a repaired target as it is
                assert_eq!(repair_tags(&src, want.clone()), want, "{f}: {t}");
                assert_eq!(parse_locos_text_for(f, t, &map).0, inl, "{f}: {t}");
            }
            let (plain, _, none) = repair_locos_text_for("json", t, &map);
            assert!(none.is_empty());
            assert_eq!(plain, parse_locos_text(t, &map).0);
        }
    }

    /// The inline repair (a target given as tags): the same rule by id; a source that repeats an id may repeat it
    /// as often; an invented tag is kept for the model's check.
    #[test]
    fn inline_targets_are_repaired_by_id() {
        let open = Inline::tag("1", TagKind::Open, "<b>");
        let close = Inline::tag("1", TagKind::Close, "</b>");
        let src = vec![open.clone(), Inline::text("a"), close.clone()];
        let t = vec![open.clone(), Inline::text("x"), close.clone(), open.clone(), Inline::text("y"), close.clone(), close.clone(), open.clone()];
        assert_eq!(repair_tags(&src, t), vec![open.clone(), Inline::text("x"), close.clone(), Inline::text("y")]);
        assert_eq!(repair_tags(&src, vec![open.clone(), Inline::text("z")]), vec![open.clone(), Inline::text("z"), close.clone()]);
        let twice = vec![open.clone(), Inline::text("a"), close.clone(), open.clone(), Inline::text("b"), close.clone()];
        assert_eq!(repair_tags(&twice, twice.clone()), twice);
        let invented = vec![Inline::tag("9", TagKind::Standalone, "<hr/>")];
        assert_eq!(repair_tags(&src, invented.clone()), invented);
    }
}
