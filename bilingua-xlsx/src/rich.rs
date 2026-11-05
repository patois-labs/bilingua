//! Rich text: runs with formatting become text plus paired tags. The most common formatting of a unit needs no
//! tag (it is the "base"). Other runs are wrapped in a tag pair whose code keeps the raw run properties.
//! Identical logic lives in `bilingua-pptx`; the two crates do not share code on purpose.

use bilingua_model::*;
use std::collections::{BTreeMap, BTreeSet};

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

pub fn build(mut atoms: Vec<Atom>, props: Props) -> Built {
    // Whitespace-only runs take the formatting of their neighbour, so they do not open a tag of their own.
    let n = atoms.len();
    for i in 0..n {
        let ws = matches!(&atoms[i], Atom::Text { text, .. } if text.trim().is_empty());
        if !ws {
            continue;
        }
        let prev = (0..i).rev().find_map(|j| match &atoms[j] {
            Atom::Text { fmt, key, .. } => Some((fmt.clone(), key.clone())),
            _ => None,
        });
        let next = (i + 1..n).find_map(|j| match &atoms[j] {
            Atom::Text { fmt, key, text } if !text.trim().is_empty() => Some((fmt.clone(), key.clone())),
            _ => None,
        });
        if let Some((f, k)) = prev.or(next) {
            if let Atom::Text { fmt, key, .. } = &mut atoms[i] {
                *fmt = f;
                *key = k;
            }
        }
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
    let mut weight: Vec<(String, String, usize)> = Vec::new();
    for a in &merged {
        if let Atom::Text { fmt, key, text } = a {
            match weight.iter_mut().find(|(k, _, _)| k == key) {
                Some(w) => w.2 += text.chars().count(),
                None => weight.push((key.clone(), fmt.clone(), text.chars().count())),
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
                let next_key = merged[ai + 1..].iter().find_map(|x| match x {
                    Atom::Text { key, .. } => Some(key.clone()),
                    _ => None,
                });
                if open.as_ref().map(|(k, _)| Some(k) != next_key.as_ref()).unwrap_or(false) {
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
