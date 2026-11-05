//! The Text field of an ASS event: override blocks and escapes become inline codes (Localization OS `_tokenize`).
//!
//! Protected tokens are `{...}` (up to the first `}`) and the escapes `\N`, `\n`, `\h`. Leading and trailing
//! tokens and white space stay out of the unit. A style switch turned on and later off in the same line is a
//! paired code (`Open` ... `Close`), anything else is a standalone code.

use bilingua_model::{py_isspace_char, py_lstrip, py_rstrip, py_strip, CodeData, Inline, TagKind};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy)]
struct Piece {
    tok: bool,
    a: usize,
    b: usize,
}

/// A tokenized Text field: the unit's inline codes and how much of the field is skeleton on each side.
pub(crate) struct Tokenized {
    pub prefix_len: usize,
    pub suffix_len: usize,
    pub inlines: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
}

/// Splits the text into text and token pieces. `{` without a later `}` is plain text.
fn split_pieces(text: &str) -> Vec<Piece> {
    let b = text.as_bytes();
    let mut pieces = Vec::new();
    let mut text_start = 0usize;
    let mut i = 0usize;
    let mut no_more_close = false;
    while i < b.len() {
        let c = b[i];
        let tok_end = if c == b'{' && !no_more_close {
            match b[i + 1..].iter().position(|x| *x == b'}') {
                Some(p) => Some(i + 1 + p + 1),
                None => {
                    no_more_close = true;
                    None
                }
            }
        } else if c == b'\\' && matches!(b.get(i + 1), Some(b'N' | b'n' | b'h')) {
            Some(i + 2)
        } else {
            None
        };
        match tok_end {
            Some(e) => {
                if text_start < i {
                    pieces.push(Piece { tok: false, a: text_start, b: i });
                }
                pieces.push(Piece { tok: true, a: i, b: e });
                i = e;
                text_start = e;
            }
            None => i += 1,
        }
    }
    if text_start < b.len() {
        pieces.push(Piece { tok: false, a: text_start, b: b.len() });
    }
    pieces
}

/// True when an override block switches drawing mode on (`\p1` and up): such a line holds vector commands.
pub(crate) fn in_drawing_mode(text: &str) -> bool {
    for p in split_pieces(text) {
        if !p.tok || !text[p.a..p.b].starts_with('{') {
            continue;
        }
        let blk = &text[p.a..p.b];
        let mut rest = blk;
        while let Some(at) = rest.find("\\p") {
            let after = &rest[at + 2..];
            let digits_from = after.trim_start_matches(py_isspace_char);
            let n = digits_from.bytes().take_while(u8::is_ascii_digit).count();
            if n > 0 && digits_from.as_bytes()[..n].iter().any(|d| *d != b'0') {
                return true;
            }
            rest = after;
        }
    }
    false
}

const KEYS: [char; 4] = ['i', 'u', 's', 'b'];

fn key_bit(c: char) -> u8 {
    1 << KEYS.iter().position(|k| *k == c).unwrap_or(0)
}

/// `{\i1}`, `{\b0}`, `{\b700\u1}`: a block of only bold, italic, underline and strikeout switches. Returns the
/// switches in order with whether each is on.
fn toggle_block(s: &str) -> Option<Vec<(char, bool)>> {
    let inner = s.strip_prefix('{')?.strip_suffix('}')?;
    let b = inner.as_bytes();
    if b.is_empty() {
        return None;
    }
    let mut ops = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' {
            return None;
        }
        i += 1;
        let k = *b.get(i)?;
        i += 1;
        match k {
            b'i' | b'u' | b's' => {
                let d = *b.get(i)?;
                if d != b'0' && d != b'1' {
                    return None;
                }
                i += 1;
                ops.push((k as char, d == b'1'));
            }
            b'b' => {
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                let n = i - start;
                if !(1..=4).contains(&n) {
                    return None;
                }
                ops.push(('b', b[start..i].iter().any(|d| *d != b'0')));
            }
            _ => return None,
        }
    }
    Some(ops)
}

fn toggle_reset(open_block: &str) -> String {
    let keys: Vec<char> = toggle_block(open_block).unwrap_or_default().iter().map(|(k, _)| *k).collect();
    let mut s = String::from("{");
    for k in keys.iter().rev() {
        s.push('\\');
        s.push(*k);
        s.push('0');
    }
    s.push('}');
    s
}

/// (opener, closer) piece indexes for style switches turned on and later off in the same line.
fn pair_blocks(text: &str, pieces: &[Piece]) -> Vec<(usize, usize)> {
    let mut stack: Vec<(usize, u8)> = Vec::new();
    let mut pairs = Vec::new();
    for (k, p) in pieces.iter().enumerate() {
        if !p.tok {
            continue;
        }
        let s = &text[p.a..p.b];
        let (on, off): (u8, Option<u8>);
        if s == "{\\r}" {
            on = 0;
            off = None;
        } else if let Some(ops) = toggle_block(s) {
            let mut o = 0u8;
            let mut f = 0u8;
            for (key, v) in ops {
                if v {
                    o |= key_bit(key);
                } else {
                    f |= key_bit(key);
                }
            }
            if o != 0 && f != 0 {
                stack.retain(|e| e.1 & (o | f) == 0);
                continue;
            }
            on = o;
            off = Some(f);
        } else {
            continue;
        }
        if on != 0 {
            stack.push((k, on));
            continue;
        }
        while !stack.is_empty() && (off.is_none() || stack.iter().any(|e| e.1 & off.unwrap_or(0) != 0)) {
            let (o, keys) = stack.pop().unwrap_or((0, 0));
            if off.is_none() || keys & !off.unwrap_or(0) == 0 {
                pairs.push((o, k));
            }
        }
    }
    pairs
}

/// [lo, hi) of the pieces that stay in the unit.
fn core_bounds(text: &str, pieces: &[Piece], pinned: &[bool]) -> (usize, usize) {
    let hoisted = |k: usize| {
        let p = pieces[k];
        (p.tok && !pinned[k]) || (!p.tok && py_strip(&text[p.a..p.b]).is_empty())
    };
    let mut lo = 0;
    while lo < pieces.len() && hoisted(lo) {
        lo += 1;
    }
    let mut hi = pieces.len();
    while hi > lo && hoisted(hi - 1) {
        hi -= 1;
    }
    (lo, hi)
}

fn label_of(s: &str) -> &'static str {
    match s {
        "\\N" => "line break",
        "\\n" => "soft break",
        "\\h" => "hard space",
        _ => match toggle_block(s) {
            Some(ops) if ops.len() == 1 => match ops[0].0 {
                'i' => "italic",
                'b' => "bold",
                'u' => "underline",
                _ => "strikeout",
            },
            Some(_) => "style",
            None if s == "{\\r}" => "style reset",
            None => "override",
        },
    }
}

/// Tokenizes one Text field. `None` when nothing translatable is left (blank, or only codes).
pub(crate) fn tokenize(text: &str) -> Option<Tokenized> {
    let pieces = split_pieces(text);
    if !pieces.iter().any(|p| !p.tok && !py_strip(&text[p.a..p.b]).is_empty()) {
        return None;
    }
    let pairs = pair_blocks(text, &pieces);
    let mut pinned = vec![false; pieces.len()];
    let (lo, hi) = loop {
        let (lo, hi) = core_bounds(text, &pieces, &pinned);
        let mut changed = false;
        for &(o, c) in &pairs {
            if (lo <= o && o < hi) != (lo <= c && c < hi) {
                for e in [o, c] {
                    if !pinned[e] {
                        pinned[e] = true;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break (lo, hi);
        }
    };
    // trim white space off the first and last text piece of the core
    let mut core_a = pieces[lo].a;
    let mut core_b = pieces[hi - 1].b;
    if !pieces[lo].tok {
        let s = &text[pieces[lo].a..pieces[lo].b];
        core_a += s.len() - py_lstrip(s).len();
    }
    if !pieces[hi - 1].tok {
        let s = &text[core_a.max(pieces[hi - 1].a)..pieces[hi - 1].b];
        core_b -= s.len() - py_rstrip(s).len();
    }
    let opener_of: HashMap<usize, usize> = pairs.iter().map(|&(o, c)| (o, c)).collect();
    let mut closes_at: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(o, c) in &pairs {
        if lo <= o && o < hi {
            closes_at.entry(c).or_default().push(o);
        }
    }
    let mut inlines: Vec<Inline> = Vec::new();
    let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
    let mut ids: HashMap<usize, usize> = HashMap::new();
    let mut n = 0usize;
    for k in lo..hi {
        let p = pieces[k];
        if !p.tok {
            let a = if k == lo { core_a } else { p.a };
            let b = if k == hi - 1 { core_b } else { p.b };
            if a < b {
                inlines.push(Inline::Text(text[a..b].to_string()));
            }
            continue;
        }
        let s = &text[p.a..p.b];
        if let Some(openers) = closes_at.get(&k) {
            let mut openers = openers.clone();
            openers.sort_unstable_by(|x, y| y.cmp(x)); // innermost first
            let outermost = *openers.last().unwrap_or(&0);
            let many = openers.len() > 1;
            for o in openers {
                let Some(tid) = ids.get(&o).map(|t| t.to_string()) else { continue };
                // one pair ends here: the source block; several: the inner ones their own reset, the outermost
                // the source block (a `{\r}` also resets colours and fonts set in between)
                let close = if !many || o == outermost { s.to_string() } else { toggle_reset(&text[pieces[o].a..pieces[o].b]) };
                if let Some(c) = codes.get_mut(&tid) {
                    c.close = Some(close.clone());
                }
                inlines.push(Inline::tag(tid, TagKind::Close, close));
            }
            continue;
        }
        n += 1;
        let id = n.to_string();
        match opener_of.get(&k) {
            Some(&c) if c < hi => {
                ids.insert(k, n);
                codes.insert(id.clone(), CodeData { open: Some(s.to_string()), close: None, standalone: None, label: Some(label_of(s).into()) });
                inlines.push(Inline::tag(id, TagKind::Open, s));
            }
            _ => {
                codes.insert(id.clone(), CodeData { open: None, close: None, standalone: Some(s.to_string()), label: Some(label_of(s).into()) });
                inlines.push(Inline::tag(id, TagKind::Standalone, s));
            }
        }
    }
    Some(Tokenized { prefix_len: core_a, suffix_len: text.len() - core_b, inlines, codes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tl(t: &str) -> Option<String> {
        let tk = tokenize(t)?;
        Some(bilingua_model::profile::tokens::to_locos_text(&tk.inlines).0)
    }

    #[test]
    fn matches_the_locos_shapes() {
        assert_eq!(tl("{\\an8}I said {\\i1}no{\\i0}.\\NNever again.").unwrap(), "I said \u{27E6}g1\u{27E7}no\u{27E6}/g1\u{27E7}.\u{27E6}x2\u{27E7}Never again.");
        assert_eq!(tl("{\\pos(400,570)}"), None);
        assert_eq!(tl("{\\b1}a {\\i1}b{\\r} c").unwrap(), "\u{27E6}g1\u{27E7}a \u{27E6}g2\u{27E7}b\u{27E6}/g2\u{27E7}\u{27E6}/g1\u{27E7} c");
        assert_eq!(tl("x {\\b1}a{\\i1}b{\\b0}c{\\i0} d").unwrap(), "x \u{27E6}g1\u{27E7}a\u{27E6}x2\u{27E7}b\u{27E6}/g1\u{27E7}c\u{27E6}x3\u{27E7} d");
    }

    #[test]
    fn drawing_mode() {
        assert!(in_drawing_mode("{\\p1}m 0 0 l 1 1{\\p0}"));
        assert!(in_drawing_mode("{\\p 2}m 0 0"));
        assert!(!in_drawing_mode("{\\pos(1,2)}Hi{\\p0}"));
        assert!(!in_drawing_mode("Hi \\p1 there"));
    }
}
