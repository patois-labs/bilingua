//! BILI-54: string statements outside paragraphs as units, with `mif.extract_variables` (Okapi's okf_mif reads
//! them; LocOS does not, so it is off by default). A user variable's definition (`<VariableDef `...'>` in
//! `VariableFormats`) and a paragraph's number string (`<PgfNumString `...'>`) are units; the building blocks of
//! a definition (`<$chapnum>`, `<Emphasis>`) are point tags. A unit spans the string argument only, so an untouched
//! merge is the original bytes and a translated one rewrites the string and nothing else.

use crate::esc::encode_string;
use crate::scan::{Tree, NONE};
use bilingua_model::{CodeData, Inline, TagKind, Unit};
use std::collections::{BTreeMap, HashSet};

/// The format option that turns these units on.
pub const OPTION: &str = "mif.extract_variables";
/// The key of the hidden code of a string unit: its standalone text is the original string argument.
pub const STR_HIDDEN: &str = "$mifstr";

pub struct StrUnit {
    pub inlines: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
    pub kind: &'static str,
    pub style: Option<String>,
    /// Byte range of the `` `...' `` argument.
    pub span: (usize, usize),
}

/// The string units of the file in file order, and how many strings were left out for holding the marker
/// characters (as paragraphs are). Iterative, like the scanner.
pub fn string_units(text: &str, tree: &Tree) -> (Vec<StrUnit>, usize) {
    let (mut out, mut skipped) = (Vec::new(), 0usize);
    let mut stack: Vec<u32> = vec![0];
    while let Some(node) = stack.pop() {
        let kids: Vec<u32> = tree.children(node).collect();
        // the variable's name, or the paragraph format of the Para or Pgf the number string belongs to
        let label = |name: &str| kids.iter().find(|&&c| tree.name(text, c) == name).and_then(|&c| tree.string_arg(text, c));
        for &c in &kids {
            let kind = match tree.name(text, c) {
                "VariableDef" => "variable",
                "PgfNumString" => "paragraph number",
                _ => continue,
            };
            let style = if kind == "variable" { label("VariableName") } else { label("PgfTag") };
            match one(text, tree, c, kind, style) {
                Some(Ok(u)) => out.push(u),
                Some(Err(())) => skipped += 1,
                None => {}
            }
        }
        for &c in kids.iter().rev() {
            if tree.nodes[c as usize].first_child != NONE {
                stack.push(c);
            }
        }
    }
    out.sort_by_key(|u| u.span.0);
    (out, skipped)
}

/// The first building block of `s`: `<...>` with no `<` inside (`<$chapnum>`, `<Emphasis>`), as a byte range.
fn next_block(s: &str) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let mut from = 0;
    while let Some(p) = s[from..].find('<') {
        let i = from + p;
        let j = i + 1 + s[i + 1..].find(['<', '>'])?;
        if b[j] == b'>' {
            return Some((i, j + 1));
        }
        from = j;
    }
    None
}

/// `None` when the statement is not one string or has no letter outside its building blocks (`2.`, `<$year>`);
/// `Some(Err)` when its text holds the marker characters.
fn one(text: &str, tree: &Tree, node: u32, kind: &'static str, style: Option<String>) -> Option<Result<StrUnit, ()>> {
    let s = tree.string_arg(text, node)?;
    let a = tree.nodes[node as usize].args[0];
    let mut inlines: Vec<Inline> = Vec::new();
    let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
    let mut words = String::new();
    let mut rest = s.as_str();
    let mut n = 0usize;
    loop {
        let (lit, block) = match next_block(rest) {
            Some((i, e)) => (&rest[..i], Some(&rest[i..e])),
            None => (rest, None),
        };
        if !lit.is_empty() {
            inlines.push(Inline::text(lit));
            words.push_str(lit);
        }
        let Some(b) = block else { break };
        n += 1;
        codes.insert(n.to_string(), CodeData::new().with_standalone(b.to_string()).with_label("variable".to_string()));
        inlines.push(Inline::tag(n.to_string(), TagKind::Standalone, b));
        rest = &rest[lit.len() + b.len()..];
    }
    if words.contains('\u{27e6}') || words.contains('\u{27e7}') {
        return Some(Err(()));
    }
    if !bilingua_model::py_has_alpha(&words) {
        return None;
    }
    codes.insert(STR_HIDDEN.into(), CodeData::new().with_standalone(text[a.start..a.end].to_string()));
    Some(Ok(StrUnit { inlines, codes, kind, style, span: (a.start, a.end) }))
}

/// The string argument a translated string unit is written as: its text and building blocks, then every building
/// block of the source the translation left out (so a variable keeps what it computes), quoted and escaped.
pub fn render(unit: &Unit, output: &[Inline]) -> String {
    let block = |id: &str| if id == STR_HIDDEN { None } else { unit.codes.get(id).and_then(|c| c.standalone.as_deref()) };
    let mut s = String::new();
    let mut used: HashSet<String> = HashSet::new();
    for i in output {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, .. } => {
                if let Some(b) = block(id) {
                    s.push_str(b);
                    used.insert(id.clone());
                }
            }
        }
    }
    for i in unit.source() {
        if let Inline::Tag { id, .. } = &i {
            if !used.contains(id) {
                if let Some(b) = block(id) {
                    s.push_str(b);
                }
            }
        }
    }
    format!("`{}'", encode_string(&s))
}
