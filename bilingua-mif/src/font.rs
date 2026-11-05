//! Font changes: a `<Font ...>` statement as a property map, and the pairing of a change with the later statement
//! that restores the font (LocOS #680).

use crate::scan::{is_ws, Tree};
use std::collections::{BTreeMap, HashMap};

pub type Props = BTreeMap<String, String>;

/// Font sub-statements that are not font properties.
const IGNORE: &[&str] = &["FLocked"];

/// Property values that are the plain setting.
const PLAIN: &[&str] = &[
    "`Regular'",
    "FNoUnderlining",
    "No",
    "FAsTyped",
    "FNormal",
    "`Black'",
];

/// Whitespace-normalised: runs of white space become one space, the ends are trimmed.
fn squash(s: &str) -> String {
    s.split(is_ws)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A Font (or PgfFont) statement as `{property: value}`; an empty `FTag` (the paragraph default) is left out. `None`
/// when the statement has loose arguments instead of a property list.
pub fn font_props(text: &str, tree: &Tree, node: u32) -> Option<Props> {
    if !tree.nodes[node as usize].args.is_empty() {
        return None;
    }
    let mut props = Props::new();
    for c in tree.children(node) {
        let name = tree.name(text, c);
        if IGNORE.contains(&name) {
            continue;
        }
        let raw = tree.raw(text, c);
        // from the end of the (first) name inside the statement to just before its closing `>`
        let at = raw.find(name).unwrap_or(0) + name.len();
        let inner = raw.get(at..raw.len().saturating_sub(1)).unwrap_or("");
        let val = squash(inner);
        if name == "FTag" && (val == "`'" || val.is_empty()) {
            continue;
        }
        props.insert(name.to_string(), val);
    }
    Some(props)
}

/// `{paragraph tag: PgfFont properties}` from the file's PgfCatalog.
pub fn pgf_fonts(text: &str, tree: &Tree) -> HashMap<String, Props> {
    let mut out = HashMap::new();
    for cat in tree.children(0) {
        if tree.name(text, cat) != "PgfCatalog" {
            continue;
        }
        for pgf in tree.children(cat) {
            if tree.name(text, pgf) == "Pgf" {
                let (tag, font) = pgf_font(text, tree, pgf);
                if let (Some(t), Some(f)) = (tag, font) {
                    out.insert(t, f);
                }
            }
        }
    }
    out
}

/// A Pgf statement's tag and its PgfFont properties.
pub fn pgf_font(text: &str, tree: &Tree, pgf: u32) -> (Option<String>, Option<Props>) {
    let (mut tag, mut font) = (None, None);
    for c in tree.children(pgf) {
        match tree.name(text, c) {
            "PgfTag" => tag = tree.string_arg(text, c),
            "PgfFont" => font = font_props(text, tree, c),
            _ => {}
        }
    }
    (tag, font)
}

/// Font `g` restores the font in effect before opener `f`: it is that font exactly, or it resets exactly the
/// properties `f` changed, each to its earlier value (where that is the paragraph format's, to the format's own
/// value or, when the file does not give it, to a plain value).
fn restores(g: &Props, before: &Props, f: &Props, base: &Props) -> bool {
    if g == before {
        return true;
    }
    let changed: Vec<&String> = f
        .iter()
        .filter(|(k, v)| before.get(*k) != Some(*v))
        .map(|(k, _)| k)
        .collect();
    if changed.is_empty() || g.len() != changed.len() || !changed.iter().all(|k| g.contains_key(*k)) {
        return false;
    }
    for (k, v) in g {
        if f.get(k) == Some(v) {
            return false;
        }
        let want = before.get(k).or_else(|| base.get(k));
        let ok = match want {
            Some(w) => v == w,
            None => PLAIN.contains(&v.as_str()),
        };
        if !ok {
            return false;
        }
    }
    true
}

/// `{opener item index: closer item index}` for the font changes (`fonts`: item indexes with their parsed
/// properties) that a later Font statement restores. A change never restored, a no-op Font or one with loose
/// arguments stays a point.
pub fn pair_fonts(fonts: &[(usize, Option<Props>)], base: &Props) -> HashMap<usize, usize> {
    let mut stack: Vec<(usize, Props, Props)> = Vec::new();
    let mut state = Props::new();
    let mut pairs = HashMap::new();
    for (i, g) in fonts {
        let Some(g) = g else { continue };
        let mut hit = None;
        for depth in (0..stack.len()).rev() {
            let (o, before, f) = &stack[depth];
            if restores(g, before, f, base) {
                pairs.insert(*o, *i);
                hit = Some((depth, before.clone()));
                break;
            }
        }
        match hit {
            Some((depth, before)) => {
                stack.truncate(depth);
                state = before;
            }
            None => {
                if *g != state {
                    stack.push((*i, state.clone(), g.clone()));
                    for (k, v) in g {
                        state.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }
    pairs
}
