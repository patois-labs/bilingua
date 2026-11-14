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

/// The most font changes one paragraph can have open (changed and not yet restored) at a time. Each Font statement
/// is compared with every open change, so without a bound a paragraph of changes that are never restored costs
/// quadratic time. A change that arrives while this many are open stays a point, as a change never restored does.
const MAX_OPEN: usize = 64;

/// An open font change: the statement, and what it took to make it.
struct Open<'a> {
    /// The item index of the Font statement.
    item: usize,
    f: &'a Props,
    /// How many properties the font in effect before the statement had.
    before_len: usize,
    /// The properties the statement changed, each with its earlier value (`None`: not set before). Only these are
    /// kept, not a copy of the whole earlier font, so an entry is never larger than its own statement.
    undo: BTreeMap<&'a str, Option<&'a str>>,
}

/// Font `g` resets exactly the properties opener `o` changed, each to its earlier value (where that is the
/// paragraph format's, to the format's own value or, when the file does not give it, to a plain value).
fn resets(g: &Props, o: &Open, base: &Props) -> bool {
    if o.undo.is_empty() || g.len() != o.undo.len() {
        return false;
    }
    for (k, v) in g {
        let Some(earlier) = o.undo.get(k.as_str()) else { return false };
        if o.f.get(k) == Some(v) {
            return false;
        }
        let ok = match earlier.or_else(|| base.get(k).map(String::as_str)) {
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
///
/// A later statement restores an opener when it is the font in effect before the opener exactly, or resets exactly
/// the properties the opener changed (see `resets`).
pub fn pair_fonts(fonts: &[(usize, Option<Props>)], base: &Props) -> HashMap<usize, usize> {
    let mut stack: Vec<Open> = Vec::new();
    // the font in effect: every open change applied in order
    let mut state: BTreeMap<&str, &str> = BTreeMap::new();
    let mut pairs = HashMap::new();
    for (i, g) in fonts {
        let Some(g) = g else { continue };
        // `seen[n]`: the value of g's n-th property in the font before the open change under examination. It starts
        // as the font in effect and steps back one open change at a time, so no earlier font is ever copied.
        let mut seen: Vec<Option<&str>> = g.keys().map(|k| state.get(k.as_str()).copied()).collect();
        let mut hit = None;
        for depth in (0..stack.len()).rev() {
            let o = &stack[depth];
            for (s, k) in seen.iter_mut().zip(g.keys()) {
                if let Some(earlier) = o.undo.get(k.as_str()) {
                    *s = *earlier;
                }
            }
            let same = o.before_len == g.len() && seen.iter().zip(g.values()).all(|(s, v)| *s == Some(v.as_str()));
            if same || resets(g, o, base) {
                pairs.insert(o.item, *i);
                hit = Some(depth);
                break;
            }
        }
        match hit {
            Some(depth) => {
                // back to the font before the restored opener
                for o in stack.drain(depth..).rev() {
                    for (k, earlier) in o.undo {
                        match earlier {
                            Some(v) => state.insert(k, v),
                            None => state.remove(k),
                        };
                    }
                }
            }
            None => {
                let undo: BTreeMap<&str, Option<&str>> = g
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str(), state.get(k.as_str()).copied()))
                    .filter(|(_, v, earlier)| *earlier != Some(*v))
                    .map(|(k, _, earlier)| (k, earlier))
                    .collect();
                let differs = !undo.is_empty() || state.len() != g.len();
                if differs && stack.len() < MAX_OPEN {
                    let before_len = state.len();
                    for k in undo.keys() {
                        state.insert(k, g[*k].as_str());
                    }
                    stack.push(Open { item: *i, f: g, before_len, undo });
                }
            }
        }
    }
    pairs
}
