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
        // every accelerator after the first is doubled (`&&`, a literal ampersand), in one pass: an insert per
        // accelerator shifts the rest of the target each time, quadratic for a long target (run 2, the rc pass)
        let mut out = Vec::with_capacity(target.len() + tgt.len());
        let mut next = tgt[1..].iter().map(|p| p - 1).peekable();
        for (k, it) in target.into_iter().enumerate() {
            if next.peek() == Some(&k) {
                next.next();
                out.push(Item::Ch('&'));
            }
            out.push(it);
        }
        return out;
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
    let idx = if bilingua_model::py_isalnum_char(letter) {
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
        if let Some(idx) = plain.iter().position(|c| c.is_some_and(bilingua_model::py_isalpha_char)) {
            target.insert(idx, Item::Ch('&'));
            return target;
        }
    }
    // CJK style: append (&X) before the tab and before trailing "..." / ":"
    let mut end = cut;
    while end > 0 && matches!(&target[end - 1], Item::Ch(c) if bilingua_model::py_isspace_char(*c)) {
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

#[cfg(test)]
mod run2 {
    use super::*;

    fn items(s: &str) -> Vec<Item> {
        s.chars().map(Item::Ch).collect()
    }

    fn text(v: &[Item]) -> String {
        v.iter().map(|i| if let Item::Ch(c) = i { *c } else { '?' }).collect()
    }

    /// A target with many accelerators keeps the first and doubles the rest: one insert per accelerator used to
    /// shift the rest of the target each time.
    #[test]
    fn many_accelerators_are_doubled_in_one_pass() {
        let n = 100_000;
        let src = items("&File");
        let t = std::time::Instant::now();
        let out = fix(&src, items(&"&ab".repeat(n)), &|_| None);
        let a = t.elapsed().as_secs_f64();
        let t = std::time::Instant::now();
        let _ = fix(&src, items(&"&&b".repeat(n)), &|_| None);
        let c = t.elapsed().as_secs_f64();
        assert!(a < c * 5.0 + 3.0, "accelerators {a:.2} s, control {c:.2} s");
        assert_eq!(out.len(), 4 * n - 1);
        assert_eq!(text(&fix(&src, items("&a &b &c"), &|_| None)), "&a &&b &&c");
    }
}
