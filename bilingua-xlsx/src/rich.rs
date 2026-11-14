//! Rich text: runs with formatting become text plus paired tags. The most common formatting of a unit needs no
//! tag (it is the "base"). Other runs are wrapped in a tag pair whose code keeps the raw run properties.
//! Identical logic lives in `bilingua-pptx`; the two crates do not share code on purpose.

use bilingua_model::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Reserved code names in `Unit::codes` (never used by an inline).
pub const C_ORIG: &str = "_o";
pub const C_BASE: &str = "_base";
pub const C_MODE: &str = "_mode";

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

pub fn build(mut atoms: Vec<Atom>, props: Props) -> Built {
    // Whitespace-only runs take the formatting of their neighbour, so they do not open a tag of their own.
    // One pass: the nearest text run before is carried along (`prev`), and the look ahead for the next run with
    // text happens only while there is no run before, which is true for at most one run. A paragraph of many
    // thousands of empty runs is then linear work, not a scan per run.
    let n = atoms.len();
    let mut prev: Option<usize> = None;
    for i in 0..n {
        let ws = match &atoms[i] {
            Atom::Text { text, .. } => text.trim().is_empty(),
            Atom::Code { .. } => continue,
        };
        if ws {
            let from = prev.or_else(|| (i + 1..n).find(|&j| matches!(&atoms[j], Atom::Text { text, .. } if !text.trim().is_empty())));
            let taken = from.and_then(|j| match &atoms[j] {
                Atom::Text { fmt, key, .. } => Some((fmt.clone(), key.clone())),
                Atom::Code { .. } => None,
            });
            if let Some((f, k)) = taken {
                if let Atom::Text { fmt, key, .. } = &mut atoms[i] {
                    *fmt = f;
                    *key = k;
                }
            }
        }
        prev = Some(i);
    }
    // Merge neighbouring runs with the same formatting.
    let mut merged: Vec<Atom> = Vec::new();
    for a in atoms {
        match (merged.last_mut(), a) {
            (
                Some(Atom::Text { key: k0, text: t0, .. }),
                Atom::Text { key, text, .. },
            ) if *k0 == key => t0.push_str(&text),
            (_, a) => merged.push(a),
        }
    }
    // Base = formatting with the most text.
    // The tally is kept in first-seen order (a tie goes to the formatting seen first) and found through a map, so
    // many different formattings do not cost a search each.
    let mut weight: Vec<(String, String, usize)> = Vec::new();
    let mut weight_at: HashMap<&str, usize> = HashMap::new();
    for a in &merged {
        if let Atom::Text { fmt, key, text } = a {
            match weight_at.get(key.as_str()) {
                Some(&w) => weight[w].2 += text.chars().count(),
                None => {
                    weight_at.insert(key.as_str(), weight.len());
                    weight.push((key.clone(), fmt.clone(), text.chars().count()));
                }
            }
        }
    }
    let mut base: (String, String) = (String::new(), String::new());
    let mut best = 0;
    for (k, f, w) in &weight {
        if *w > best {
            best = *w;
            base = (k.clone(), f.clone());
        }
    }
    let base_props = props(&base.1);

    let mut inlines = Vec::new();
    let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
    codes.insert(
        C_BASE.into(),
        CodeData::new().with_standalone(base.1.clone()),
    );
    let mut counter = 0usize;
    let mut open: Option<(String, String)> = None; // (key, tag id)
    let mut close_display = String::new();
    let close_open = |inl: &mut Vec<Inline>, open: &mut Option<(String, String)>, cd: &str| {
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
        if matches!(&merged[i], Atom::Text { .. }) {
            seen = Some(i);
        }
    }
    for (ai, a) in merged.iter().cloned().enumerate().take(n_merged) {
        match a {
            Atom::Text { fmt, key, text } => {
                if key == base.0 {
                    close_open(&mut inlines, &mut open, &close_display);
                } else if open.as_ref().map(|(k, _)| k != &key).unwrap_or(true) {
                    close_open(&mut inlines, &mut open, &close_display);
                    counter += 1;
                    let id = counter.to_string();
                    let added: Vec<&str> = props(&fmt).difference(&base_props).copied().collect();
                    let (name, label) = if added.len() == 1 {
                        (added[0].to_string(), label_of(added[0]).to_string())
                    } else if added.len() > 1 {
                        ("fmt".to_string(), added.iter().map(|p| label_of(p)).collect::<Vec<_>>().join(" "))
                    } else {
                        ("fmt".to_string(), "formatting".to_string())
                    };
                    codes.insert(
                        id.clone(),
                        CodeData::new().with_open(fmt).with_label(label),
                    );
                    inlines.push(Inline::tag(id.clone(), TagKind::Open, format!("<{name}>")));
                    close_display = format!("</{name}>");
                    open = Some((key, id));
                }
                inlines.push(Inline::Text(text));
            }
            Atom::Code { xml, display, label } => {
                // The tag pair stays open across the code only when the next text has the same formatting.
                let next_key = next_text[ai].and_then(|j| match &merged[j] {
                    Atom::Text { key, .. } => Some(key),
                    Atom::Code { .. } => None,
                });
                if open.as_ref().map(|(k, _)| Some(k) != next_key).unwrap_or(false) {
                    close_open(&mut inlines, &mut open, &close_display);
                }
                counter += 1;
                let id = counter.to_string();
                codes.insert(
                    id.clone(),
                    CodeData::new().with_standalone(xml).with_label(label),
                );
                inlines.push(Inline::tag(id, TagKind::Standalone, display));
            }
        }
    }
    close_open(&mut inlines, &mut open, &close_display);
    Built { inlines: merge_text(inlines), codes }
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
