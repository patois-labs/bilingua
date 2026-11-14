//! Rich text: runs with formatting become text plus paired tags. The most common formatting of a unit needs no
//! tag (it is the "base"). Other runs are wrapped in a tag pair whose code keeps the raw run properties.
//! Identical logic lives in `bilingua-xlsx`; the two crates do not share code on purpose.

use bilingua_model::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Reserved code names in `Unit::codes` (never used by an inline).
pub const C_ORIG: &str = "_o";
pub const C_BASE: &str = "_base";

#[derive(Clone)]
pub enum Atom {
    Text { fmt: String, key: String, text: String },
    Code { xml: String, display: String, label: String },
}

pub struct Built {
    pub inlines: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
}

/// Which of b / i / u / sup / sub a run property string switches on.
pub type Props = fn(&str) -> BTreeSet<&'static str>;

fn label_of(p: &str) -> &'static str {
    match p {
        "b" => "bold",
        "i" => "italic",
        "u" => "underline",
        "sup" => "superscript",
        "sub" => "subscript",
        "link" => "link",
        _ => "formatting",
    }
}

/// What a unit built from these atoms holds at least, counted as `guard::check_document` counts a unit: one per
/// character of text and one per code. A paragraph already over `max_unit_chars` by this count is refused before
/// `build` does any work on it (the finished document would be refused for the same unit anyway).
pub fn check_size(atoms: &[Atom], l: &Limits) -> Result<(), Error> {
    let mut chars = 0usize;
    for a in atoms {
        chars = chars.saturating_add(match a {
            Atom::Text { text, .. } => text.chars().count(),
            Atom::Code { .. } => 1,
        });
    }
    guard::check_unit_chars(chars, l)
}

pub fn build(atoms: Vec<Atom>, props: Props) -> Built {
    // Whitespace-only runs take the formatting of their neighbour, so they do not open a tag of their own.
    // One pass: the nearest text run before is carried along (`prev`), and the look ahead for the next run with
    // text happens only while there is no run before, which is true for at most one run. A paragraph of many
    // thousands of empty runs is then linear work, not a scan per run.
    // A run records which run's formatting it uses (`src`) instead of a copy of it: one long run property followed
    // by many empty runs then costs its own size once, not once per empty run.
    let n = atoms.len();
    let mut src: Vec<usize> = (0..n).collect();
    let mut prev: Option<usize> = None;
    for i in 0..n {
        let ws = match &atoms[i] {
            Atom::Text { text, .. } => text.trim().is_empty(),
            Atom::Code { .. } => continue,
        };
        if ws {
            let from = prev.map(|p| src[p]).or_else(|| (i + 1..n).find(|&j| matches!(&atoms[j], Atom::Text { text, .. } if !text.trim().is_empty())));
            if let Some(j) = from {
                src[i] = j;
            }
        }
        prev = Some(i);
    }
    // The formatting and key of run `s` (a run whose `src` is itself).
    let fmt_of = |s: usize| match &atoms[s] {
        Atom::Text { fmt, .. } => fmt.as_str(),
        Atom::Code { .. } => "",
    };
    let key_of = |s: usize| match &atoms[s] {
        Atom::Text { key, .. } => key.as_str(),
        Atom::Code { .. } => "",
    };
    let same_key = |a: usize, b: usize| a == b || key_of(a) == key_of(b);
    // Merge neighbouring runs with the same formatting.
    let mut merged: Vec<M> = Vec::new();
    for (i, a) in atoms.iter().enumerate() {
        match (merged.last_mut(), a) {
            (Some(M::Text { src: s0, text: t0 }), Atom::Text { text, .. }) if same_key(*s0, src[i]) => t0.push_str(text),
            (_, Atom::Text { text, .. }) => merged.push(M::Text { src: src[i], text: text.clone() }),
            (_, Atom::Code { .. }) => merged.push(M::Code(i)),
        }
    }
    // Base = formatting with the most text.
    // The tally is kept in first-seen order (a tie goes to the formatting seen first) and found through a map, so
    // many different formattings do not cost a search each. A run whose formatting was already looked up is found
    // by its index, so the key is not hashed again.
    let mut weight: Vec<(usize, usize)> = Vec::new();
    let mut weight_at: HashMap<&str, usize> = HashMap::new();
    let mut weight_of_src: HashMap<usize, usize> = HashMap::new();
    for a in &merged {
        if let M::Text { src: s, text } = a {
            let w = match weight_of_src.get(s) {
                Some(&w) => w,
                None => {
                    let w = match weight_at.get(key_of(*s)) {
                        Some(&w) => w,
                        None => {
                            weight_at.insert(key_of(*s), weight.len());
                            weight.push((*s, 0));
                            weight.len() - 1
                        }
                    };
                    weight_of_src.insert(*s, w);
                    w
                }
            };
            weight[w].1 += text.chars().count();
        }
    }
    let mut base: Option<usize> = None;
    let mut best = 0;
    for (s, w) in &weight {
        if *w > best {
            best = *w;
            base = Some(*s);
        }
    }
    let base_fmt = base.map_or("", fmt_of);
    let is_base = |s: usize| base.is_some_and(|b| same_key(b, s)) || (base.is_none() && key_of(s).is_empty());
    let base_props = props(base_fmt);

    let mut inlines = Vec::new();
    let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
    codes.insert(
        C_BASE.into(),
        CodeData::new().with_standalone(base_fmt.to_string()),
    );
    let mut counter = 0usize;
    let mut open: Option<(usize, String)> = None; // (run whose key is open, tag id)
    let mut close_display = String::new();
    let close_open = |inl: &mut Vec<Inline>, open: &mut Option<(usize, String)>, cd: &str| {
        if let Some((_, id)) = open.take() {
            inl.push(Inline::tag(id, TagKind::Close, cd));
        }
    };
    let n_merged = merged.len();
    // For each atom, where the next text atom is: filled from the end, so a code does not search forward for it.
    let mut next_text: Vec<Option<usize>> = vec![None; n_merged];
    let mut seen: Option<usize> = None;
    for i in (0..n_merged).rev() {
        next_text[i] = seen;
        if let M::Text { src: s, .. } = &merged[i] {
            seen = Some(*s);
        }
    }
    for (ai, a) in merged.into_iter().enumerate() {
        match a {
            M::Text { src: s, text } => {
                if is_base(s) {
                    close_open(&mut inlines, &mut open, &close_display);
                } else if open.as_ref().map(|(k, _)| !same_key(*k, s)).unwrap_or(true) {
                    close_open(&mut inlines, &mut open, &close_display);
                    counter += 1;
                    let id = counter.to_string();
                    let fmt = fmt_of(s);
                    let added: Vec<&str> = props(fmt).difference(&base_props).copied().collect();
                    let (name, label) = if added.len() == 1 {
                        (added[0].to_string(), label_of(added[0]).to_string())
                    } else if added.len() > 1 {
                        ("fmt".to_string(), added.iter().map(|p| label_of(p)).collect::<Vec<_>>().join(" "))
                    } else {
                        ("fmt".to_string(), "formatting".to_string())
                    };
                    codes.insert(
                        id.clone(),
                        CodeData::new().with_open(fmt.to_string()).with_label(label),
                    );
                    inlines.push(Inline::tag(id.clone(), TagKind::Open, format!("<{name}>")));
                    close_display = format!("</{name}>");
                    open = Some((s, id));
                }
                inlines.push(Inline::Text(text));
            }
            M::Code(i) => {
                let Atom::Code { xml, display, label } = &atoms[i] else { continue };
                // The tag pair stays open across the code only when the next text has the same formatting.
                if open.as_ref().map(|(k, _)| next_text[ai].is_none_or(|nx| !same_key(*k, nx))).unwrap_or(false) {
                    close_open(&mut inlines, &mut open, &close_display);
                }
                counter += 1;
                let id = counter.to_string();
                codes.insert(
                    id.clone(),
                    CodeData::new().with_standalone(xml.clone()).with_label(label.clone()),
                );
                inlines.push(Inline::tag(id, TagKind::Standalone, display.clone()));
            }
        }
    }
    close_open(&mut inlines, &mut open, &close_display);
    Built { inlines: merge_text(inlines), codes }
}

/// A run of a paragraph after merging: text in the formatting of atom `src`, or the code at that atom index.
enum M {
    Text { src: usize, text: String },
    Code(usize),
}

/// Writes `out` using the unit's codes. `write_text(fmt, text, s)` writes one piece of text in a formatting;
/// standalone codes are written as they were.
pub fn render(
    u: &Unit,
    out: &[Inline],
    mut write_text: impl FnMut(&str, &str, &mut String),
    mut write_code: impl FnMut(&str, &mut String),
) -> String {
    let base = u
        .codes
        .get(C_BASE)
        .and_then(|c| c.standalone.clone())
        .unwrap_or_default();
    let mut stack: Vec<String> = Vec::new();
    let mut s = String::new();
    for i in out {
        match i {
            Inline::Text(t) => {
                let f = stack.last().unwrap_or(&base).clone();
                write_text(&f, t, &mut s);
            }
            Inline::Tag { id, kind, .. } => {
                let Some(code) = u.codes.get(id) else { continue };
                match kind {
                    TagKind::Open => stack.push(code.open.clone().unwrap_or_default()),
                    TagKind::Close => {
                        stack.pop();
                    }
                    TagKind::Standalone => {
                        if let Some(x) = &code.standalone {
                            write_code(x, &mut s);
                        }
                    }
                }
            }
        }
    }
    s
}
