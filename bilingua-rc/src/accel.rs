//! The `&` keyboard accelerator of menu items and dialog controls: when the source has exactly one, the target
//! keeps exactly one.

/// One character of a unit's text, or one of its codes (by id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Ch(char),
    Code(String),
}

fn positions(v: &[Item]) -> Vec<usize> {
    let mut res = Vec::new();
    let mut i = 0;
    while i < v.len() {
        if v[i] == Item::Ch('&') {
            if v.get(i + 1) == Some(&Item::Ch('&')) {
                i += 2;
                continue;
            }
            if let Some(Item::Ch(c)) = v.get(i + 1) {
                if *c != ' ' && *c != '\t' {
                    res.push(i + 1);
                }
            }
        }
        i += 1;
    }
    res
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11FF | 0x3040..=0x30FF | 0x3130..=0x318F | 0x3400..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF)
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Keeps exactly one accelerator in `target` when `source` has exactly one. `raw_of` gives the written form of a
/// code (to find the tab that separates the shortcut).
pub fn fix(source: &[Item], mut target: Vec<Item>, raw_of: &dyn Fn(&str) -> Option<String>) -> Vec<Item> {
    let src = positions(source);
    if src.len() != 1 {
        return target;
    }
    let tgt = positions(&target);
    if tgt.len() == 1 {
        return target;
    }
    if tgt.len() > 1 {
        for &p in tgt[1..].iter().rev() {
            target.insert(p - 1, Item::Ch('&'));
        }
        return target;
    }
    let Item::Ch(letter) = source[src[0]] else { return target };
    // the text before a tab code (the shortcut after the tab is never marked)
    let mut cut = target.len();
    for (k, it) in target.iter().enumerate() {
        if let Item::Code(id) = it {
            if let Some(r) = raw_of(id) {
                if matches!(r.as_str(), "\\t" | "\\011" | "\\x09") {
                    cut = k;
                    break;
                }
            }
        }
    }
    let plain: Vec<Option<char>> = target[..cut].iter().map(|it| if let Item::Ch(c) = it { Some(*c) } else { None }).collect();
    let idx = if letter.is_alphanumeric() {
        let l = lower(letter);
        plain.iter().position(|c| c.is_some_and(|c| lower(c) == l))
    } else {
        None
    };
    if let Some(idx) = idx {
        if letter.is_ascii() && plain[idx].is_some_and(|c| c.is_ascii()) {
            target.insert(idx, Item::Ch('&'));
            return target;
        }
    }
    if !plain.iter().any(|c| c.is_some_and(is_cjk)) {
        // alphabetic scripts mark a letter of the word itself (Windows UI convention);
        // only CJK targets append the source letter as (&X)
        if let Some(idx) = plain.iter().position(|c| c.is_some_and(char::is_alphabetic)) {
            target.insert(idx, Item::Ch('&'));
            return target;
        }
    }
    // CJK style: append (&X) before the tab and before trailing "..." / ":"
    let mut end = cut;
    while end > 0 && matches!(&target[end - 1], Item::Ch(c) if c.is_whitespace()) {
        end -= 1;
    }
    for tail in ["...", "\u{2026}", ":", "\u{ff1a}"] {
        let t: Vec<char> = tail.chars().collect();
        if end >= t.len() && t.iter().enumerate().all(|(k, c)| target[end - t.len() + k] == Item::Ch(*c)) {
            end -= t.len();
            break;
        }
    }
    let mut ins: Vec<Item> = vec![Item::Ch('('), Item::Ch('&')];
    ins.extend(letter.to_uppercase().map(Item::Ch));
    ins.push(Item::Ch(')'));
    let mut out: Vec<Item> = target[..end].to_vec();
    out.extend(ins);
    out.extend_from_slice(&target[end..]);
    out
}
