//! One `Para` statement as a unit: text, point tags and font-change tags, plus what the writer needs to rebuild it.

use crate::font::{font_props, pair_fonts, pgf_font, Props};
use crate::scan::Tree;
use bilingua_model::{merge_text, CodeData, Inline, TagKind};
use std::collections::{BTreeMap, HashMap};

/// `<Char name>` statements that are text.
pub fn char_text(name: &str) -> Option<char> {
    Some(match name {
        "Tab" => '\t',
        "HardSpace" => '\u{a0}',
        "HardHyphen" => '\u{2011}',
        "NumberSpace" => '\u{2007}',
        "ThinSpace" => '\u{2009}',
        "EnSpace" => '\u{2002}',
        "EmSpace" => '\u{2003}',
        "Cent" => '\u{a2}',
        "Pound" => '\u{a3}',
        "Yen" => '\u{a5}',
        "EnDash" => '\u{2013}',
        "EmDash" => '\u{2014}',
        "Dagger" => '\u{2020}',
        "DoubleDagger" => '\u{2021}',
        "Bullet" => '\u{2022}',
        "HardReturn" => '\n',
        _ => return None,
    })
}

enum Item {
    Text(String),
    Raw(u32),
    Rect(u32),
}

/// The key of the hidden code that holds the rebuild data of a unit.
pub const HIDDEN: &str = "$mif";

/// Concatenates length-prefixed pieces (`3:abc0:2:xy`), so any text round trips.
pub fn pack(parts: &[&str]) -> String {
    let mut s = String::new();
    for p in parts {
        s.push_str(&p.len().to_string());
        s.push(':');
        s.push_str(p);
    }
    s
}

/// The pieces `pack` joined; `None` for a damaged string.
pub fn unpack(s: &str) -> Option<Vec<&str>> {
    let mut out = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        let (len, tail) = rest.split_once(':')?;
        let n: usize = len.parse().ok()?;
        let piece = tail.get(..n)?;
        out.push(piece);
        rest = &tail[n..];
    }
    Some(out)
}

pub struct Analysed {
    pub inlines: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
    pub style: String,
    /// Byte range from the first ParaLine to the end of the last one.
    pub span: (usize, usize),
}

pub enum Outcome {
    /// Not a paragraph with lines.
    NoLines,
    /// Text that already holds the marker characters, or no letter or digit: not a unit.
    Skip { marker_chars: bool },
    Unit(Box<Analysed>),
}

fn label_of(name: &str) -> &'static str {
    match name {
        "Variable" => "variable",
        "XRef" | "XRefEnd" => "cross-reference",
        "Marker" => "marker",
        "FNote" => "footnote",
        "ATbl" => "table",
        "AFrame" => "anchored frame",
        "Font" => "font",
        "Char" => "character",
        "Conditional" | "InCondition" => "conditional text",
        _ => "placeholder",
    }
}

pub fn analyse(
    text: &str,
    tree: &Tree,
    para: u32,
    pgf_fonts: &HashMap<String, Props>,
    nl: &str,
) -> Outcome {
    let lines: Vec<u32> = tree
        .children(para)
        .filter(|&c| tree.name(text, c) == "ParaLine")
        .collect();
    if lines.is_empty() {
        return Outcome::NoLines;
    }
    let style = tree
        .children(para)
        .find(|&c| tree.name(text, c) == "PgfTag")
        .and_then(|c| tree.string_arg(text, c))
        .unwrap_or_default();
    let mut items: Vec<Item> = Vec::new();
    let mut fonts: Vec<(usize, u32)> = Vec::new();
    let mut in_xref = false;
    for &pl in &lines {
        for c in tree.children(pl) {
            let name = tree.name(text, c);
            if name == "Font" && !in_xref {
                fonts.push((items.len(), c));
            }
            if in_xref {
                items.push(Item::Raw(c));
                in_xref = name != "XRefEnd";
                continue;
            }
            match name {
                "String" => match tree.string_arg(text, c) {
                    Some(s) => items.push(Item::Text(s)),
                    None => items.push(Item::Raw(c)),
                },
                "Char" => match tree.atom_arg(text, c).and_then(char_text) {
                    Some(ch) => items.push(Item::Text(ch.to_string())),
                    None => items.push(Item::Raw(c)),
                },
                "TextRectID" => items.push(Item::Rect(c)),
                "XRef" => {
                    items.push(Item::Raw(c));
                    in_xref = true;
                }
                _ => items.push(Item::Raw(c)),
            }
        }
    }
    let idx: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, it)| matches!(it, Item::Text(_)))
        .map(|(i, _)| i)
        .collect();
    let mut marker_chars = false;
    let mut alnum = false;
    for it in &items {
        if let Item::Text(s) = it {
            marker_chars |= s.contains('\u{27e6}') || s.contains('\u{27e7}');
            alnum = alnum || s.chars().any(char::is_alphanumeric);
        }
    }
    if idx.is_empty() || marker_chars || !alnum {
        return Outcome::Skip { marker_chars };
    }
    let mut lo = idx[0];
    let mut hi = *idx.last().expect("non-empty");
    let mut base = pgf_fonts.get(&style).cloned().unwrap_or_default();
    for c in tree.children(para) {
        if tree.name(text, c) == "Pgf" {
            if let Some(f) = pgf_font(text, tree, c).1 {
                base.extend(f);
            }
        }
    }
    let font_list: Vec<(usize, Option<Props>)> = fonts
        .iter()
        .map(|&(i, c)| (i, font_props(text, tree, c)))
        .collect();
    let pairs = pair_fonts(&font_list, &base);
    loop {
        let mut grew = false;
        for (&o, &c) in &pairs {
            if (lo <= o && o <= hi) != (lo <= c && c <= hi) {
                lo = lo.min(o);
                hi = hi.max(c);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    let opener: HashMap<usize, usize> = pairs
        .iter()
        .filter(|(o, c)| lo <= **o && **c <= hi)
        .map(|(&o, &c)| (o, c))
        .collect();
    let closer: HashMap<usize, usize> = opener.iter().map(|(&o, &c)| (c, o)).collect();
    let raw_of = |it: &Item| match it {
        Item::Raw(n) | Item::Rect(n) => tree.raw(text, *n),
        Item::Text(_) => "",
    };

    let mut inlines: Vec<Inline> = Vec::new();
    let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
    let mut ids: HashMap<usize, String> = HashMap::new();
    let mut run: Vec<u32> = Vec::new();
    let mut n = 0usize;
    let flush = |run: &mut Vec<u32>,
                 n: &mut usize,
                 inlines: &mut Vec<Inline>,
                 codes: &mut BTreeMap<String, CodeData>| {
        if run.is_empty() {
            return;
        }
        *n += 1;
        let id = n.to_string();
        let display: String = run.iter().map(|&x| tree.raw(text, x)).collect();
        let stmts: Vec<&str> = run
            .iter()
            .filter(|&&x| tree.name(text, x) != "TextRectID")
            .map(|&x| tree.raw(text, x))
            .collect();
        let label = label_of(tree.name(text, run[0]));
        codes.insert(
            id.clone(),
            CodeData {
                open: Some(pack(&stmts)),
                close: None,
                standalone: Some(display.clone()),
                label: Some(label.into()),
            },
        );
        inlines.push(Inline::tag(id, TagKind::Standalone, display));
        run.clear();
    };
    for j in lo..=hi {
        match &items[j] {
            Item::Rect(_) => continue,
            Item::Raw(nd) if !opener.contains_key(&j) && !closer.contains_key(&j) => {
                run.push(*nd);
                continue;
            }
            _ => {}
        }
        flush(&mut run, &mut n, &mut inlines, &mut codes);
        if let Some(&c) = opener.get(&j) {
            n += 1;
            let id = n.to_string();
            let (o_raw, c_raw) = (raw_of(&items[j]), raw_of(&items[c]));
            codes.insert(
                id.clone(),
                CodeData {
                    open: Some(o_raw.into()),
                    close: Some(c_raw.into()),
                    standalone: None,
                    label: Some("font".into()),
                },
            );
            inlines.push(Inline::tag(id.clone(), TagKind::Open, o_raw));
            ids.insert(j, id);
        } else if let Some(&o) = closer.get(&j) {
            let id = ids.get(&o).cloned().unwrap_or_default();
            inlines.push(Inline::tag(id, TagKind::Close, raw_of(&items[j])));
        } else if let Item::Text(s) = &items[j] {
            inlines.push(Inline::Text(s.clone()));
        }
    }
    let inlines = merge_text(inlines);

    // rebuild data
    let first_line = lines[0];
    let last_line = *lines.last().expect("non-empty");
    let fl_start = tree.nodes[first_line as usize].start;
    let line_start = text[..fl_start].rfind('\n').map_or(0, |p| p + 1);
    let ind0 = &text[line_start..fl_start];
    let ind = if ind0.chars().all(crate::scan::is_ws) {
        ind0
    } else {
        ""
    };
    let mut body: Vec<&str> = Vec::new();
    let mut rect_done = false;
    for it in &items[..lo] {
        match it {
            Item::Rect(_) => {
                if !rect_done {
                    body.push(raw_of(it));
                    rect_done = true;
                }
            }
            _ => body.push(raw_of(it)),
        }
    }
    let tail: Vec<&str> = items[hi + 1..]
        .iter()
        .filter(|it| matches!(it, Item::Raw(_)))
        .map(raw_of)
        .collect();
    let lstart = tree.nodes[last_line as usize].start;
    let extras: Vec<&str> = tree
        .children(para)
        .filter(|&c| {
            let s = tree.nodes[c as usize].start;
            tree.name(text, c) != "ParaLine" && fl_start < s && s < lstart
        })
        .map(|c| tree.raw(text, c))
        .collect();
    let (b, t, e) = (pack(&body), pack(&tail), pack(&extras));
    let span = (fl_start, tree.nodes[last_line as usize].end);
    codes.insert(
        HIDDEN.into(),
        CodeData {
            open: Some(pack(&[nl, ind, &b, &t, &e])),
            close: None,
            standalone: Some(text[span.0..span.1].to_string()),
            label: None,
        },
    );
    Outcome::Unit(Box::new(Analysed {
        inlines,
        codes,
        style,
        span,
    }))
}
