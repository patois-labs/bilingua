//! i18next plural fill (LocOS #729): i18next v4 plural keys (`item_one`, `item_other`; ordinals `place_ordinal_one`)
//! get every CLDR category the target uses that the object lacks, each holding the translated `_other` value and
//! placed before it. A group counts only when it has `_other` and at least one other category key, all string
//! values; nothing is removed.
//!
//! Two ways of writing the result. By default the new keys are spliced into the text, so the file keeps its layout.
//! Under the LocOS profile the file is written the way LocOS writes it: parsed and dumped again with two-space
//! indentation (`json.dumps(.., ensure_ascii=False, indent=2)`), in the line endings of the original.

use crate::tree::{Node, Obj};
use crate::{cldr, scan};
use std::collections::HashMap;

const SUFFIXES: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];
const MAX_DEPTH: usize = 200;

/// For one object's members (key, value is a string) in order: for each `..._other` key the new keys to put
/// before it.
fn adds(members: &[(&str, bool)], target: &str) -> Vec<(String, Vec<String>)> {
    let mut order: Vec<(String, bool)> = Vec::new();
    let mut groups: HashMap<(String, bool), Vec<&str>> = HashMap::new();
    let have: std::collections::HashSet<&str> = members.iter().map(|m| m.0).collect();
    for (key, is_str) in members {
        if !is_str || !key.contains('_') {
            continue;
        }
        let (stem, cat) = key.rsplit_once('_').expect("contains an underscore");
        if !SUFFIXES.contains(&cat) || stem.is_empty() {
            continue;
        }
        let (stem, ordinal) = match stem.strip_suffix("_ordinal") {
            Some(s) => (s, true),
            None => (stem, false),
        };
        let k = (stem.to_string(), ordinal);
        if !groups.contains_key(&k) {
            order.push(k.clone());
        }
        let g = groups.entry(k).or_default();
        if !g.contains(&cat) {
            g.push(cat);
        }
    }
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for (stem, ordinal) in order {
        let cats = &groups[&(stem.clone(), ordinal)];
        if cats.len() < 2 || !cats.contains(&"other") {
            continue;
        }
        let mid = if ordinal { "_ordinal" } else { "" };
        let other_key = format!("{stem}{mid}_other");
        let Some(want) = cldr::categories(target, ordinal) else { continue };
        for c in want {
            if cats.contains(&c) {
                continue;
            }
            let new_key = format!("{stem}{mid}_{c}");
            if have.contains(new_key.as_str()) {
                continue;
            }
            // one entry per `_other` key (each group has its own), so the last entry is this group's
            match out.last_mut().filter(|(k, _)| *k == other_key) {
                Some((_, v)) => v.push(new_key),
                None => out.push((other_key.clone(), vec![new_key])),
            }
        }
    }
    out
}

/// The text with the missing plural keys added, `None` when there is nothing to add (or the file is not an object).
pub fn fill(text: &str, target: &str, locos: bool, original: &str) -> Option<String> {
    let root = crate::tree::scan(text).ok()?;
    if !matches!(root, Node::Obj(_)) {
        return None;
    }
    if locos {
        let mut changed = false;
        let v = to_value(&root, text, 0);
        let filled = fill_value(v, target, 0, &mut changed);
        if !changed {
            return None;
        }
        let mut out = String::new();
        dump(&filled, 0, &mut out);
        out.push('\n');
        let nl = bilingua_model::enc::newline_style(original);
        if nl.is_empty() || nl == "\n" {
            return Some(out);
        }
        return Some(out.replace('\n', nl));
    }
    let mut edits: Vec<(usize, String)> = Vec::new();
    splice_fill(&root, text, target, 0, &mut edits);
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|e| e.0);
    let mut out = String::with_capacity(text.len() + 64);
    let mut pos = 0;
    for (at, ins) in edits {
        out.push_str(&text[pos..at]);
        out.push_str(&ins);
        pos = at;
    }
    out.push_str(&text[pos..]);
    Some(out)
}

fn splice_fill(n: &Node, text: &str, target: &str, depth: usize, edits: &mut Vec<(usize, String)>) {
    if depth > MAX_DEPTH {
        return;
    }
    match n {
        Node::Arr { items, .. } => items.iter().for_each(|v| splice_fill(v, text, target, depth + 1, edits)),
        Node::Obj(o) => {
            // dict view: distinct keys at their first place, the last value (first and last member index per key)
            let mut seen: Vec<(&str, bool)> = Vec::new();
            let mut at: HashMap<&str, (usize, usize, usize)> = HashMap::new();
            for (i, p) in o.pairs.iter().enumerate() {
                match at.get_mut(p.key.as_str()) {
                    Some(e) => {
                        e.1 = i;
                        seen[e.2].1 = p.value.is_str();
                    }
                    None => {
                        at.insert(&p.key, (i, i, seen.len()));
                        seen.push((&p.key, p.value.is_str()));
                    }
                }
            }
            for (other_key, new_keys) in adds(&seen, target) {
                let Some(&(fi, li, _)) = at.get(other_key.as_str()) else { continue };
                let (first, last) = (&o.pairs[fi], &o.pairs[li]);
                let value = &text[last.value.start()..last.value.end()];
                let sep = match text[o.start..first.key_start].rfind('\n') {
                    Some(rel) => {
                        let nl = o.start + rel;
                        let indent = &text[nl + 1..first.key_start];
                        if indent.trim().is_empty() {
                            let crlf = nl > 0 && text.as_bytes()[nl - 1] == b'\r';
                            format!("{}{indent}", if crlf { "\r\n" } else { "\n" })
                        } else {
                            " ".to_string()
                        }
                    }
                    None => " ".to_string(),
                };
                let ins: String = new_keys.iter().map(|k| format!("{}: {value},{sep}", scan::quote(k))).collect();
                edits.push((first.key_start, ins));
            }
            for p in &o.pairs {
                splice_fill(&p.value, text, target, depth + 1, edits);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------------------
// The LocOS way: a value tree dumped like Python's json.dumps
// ---------------------------------------------------------------------------------------------------------

enum Value {
    Str(String),
    /// Already in Python's spelling.
    Lit(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

fn to_value(n: &Node, text: &str, depth: usize) -> Value {
    match n {
        Node::Str { val, start, end } => Value::Str(val.clone().unwrap_or_else(|| scan::unescape_lossy(&text[start + 1..end - 1]))),
        Node::Other { start, end } => Value::Lit(py_literal(&text[*start..*end])),
        Node::Arr { items, .. } => Value::Arr(items.iter().map(|v| to_value(v, text, depth + 1)).collect()),
        Node::Obj(o) => Value::Obj(dedupe(o, text, depth)),
    }
}

/// An `OrderedDict` from the pairs: a repeated key keeps its first place and its last value.
fn dedupe(o: &Obj, text: &str, depth: usize) -> Vec<(String, Value)> {
    let mut out: Vec<(String, Value)> = Vec::with_capacity(o.pairs.len());
    let mut at: HashMap<&str, usize> = HashMap::new();
    for p in &o.pairs {
        let v = to_value(&p.value, text, depth + 1);
        match at.get(p.key.as_str()) {
            Some(&i) => out[i].1 = v,
            None => {
                at.insert(&p.key, out.len());
                out.push((p.key.clone(), v));
            }
        }
    }
    out
}

fn fill_value(v: Value, target: &str, depth: usize, changed: &mut bool) -> Value {
    if depth > MAX_DEPTH {
        return v;
    }
    match v {
        Value::Arr(items) => Value::Arr(items.into_iter().map(|x| fill_value(x, target, depth + 1, changed)).collect()),
        Value::Obj(members) => {
            let flags: Vec<(&str, bool)> = members.iter().map(|(k, v)| (k.as_str(), matches!(v, Value::Str(_)))).collect();
            let mut add: HashMap<String, Vec<String>> = adds(&flags, target).into_iter().collect();
            let mut out: Vec<(String, Value)> = Vec::with_capacity(members.len());
            for (k, val) in members {
                if let Some(new_keys) = add.remove(&k) {
                    if let Value::Str(s) = &val {
                        for nk in new_keys {
                            *changed = true;
                            out.push((nk, Value::Str(s.clone())));
                        }
                    }
                }
                out.push((k, fill_value(val, target, depth + 1, changed)));
            }
            Value::Obj(out)
        }
        other => other,
    }
}

fn dump(v: &Value, level: usize, out: &mut String) {
    match v {
        Value::Str(s) => out.push_str(&scan::quote(s)),
        Value::Lit(s) => out.push_str(s),
        Value::Arr(items) if items.is_empty() => out.push_str("[]"),
        Value::Obj(m) if m.is_empty() => out.push_str("{}"),
        Value::Arr(items) => {
            out.push('[');
            for (i, x) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                out.push_str(&"  ".repeat(level + 1));
                dump(x, level + 1, out);
            }
            out.push('\n');
            out.push_str(&"  ".repeat(level));
            out.push(']');
        }
        Value::Obj(m) => {
            out.push('{');
            for (i, (k, x)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                out.push_str(&"  ".repeat(level + 1));
                out.push_str(&scan::quote(k));
                out.push_str(": ");
                dump(x, level + 1, out);
            }
            out.push('\n');
            out.push_str(&"  ".repeat(level));
            out.push('}');
        }
    }
}

/// A JSON number or literal in the spelling Python's `json.dumps` gives it after `json.loads`.
fn py_literal(s: &str) -> String {
    match s {
        "true" | "false" | "null" | "NaN" | "Infinity" | "-Infinity" => return s.to_string(),
        _ => {}
    }
    if !s.contains(['.', 'e', 'E']) {
        return if s == "-0" { "0".to_string() } else { s.to_string() };
    }
    let Ok(x) = s.parse::<f64>() else { return s.to_string() };
    py_float_repr(x)
}

/// Python's `repr(float)`.
fn py_float_repr(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    // shortest digits and exponent from Rust's `{:e}`: d.ddde<exp>
    let e = format!("{:e}", x.abs());
    let (mant, exp) = e.split_once('e').expect("exponent form");
    let exp: i32 = exp.parse().expect("exponent");
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let n = digits.len() as i32;
    let decpt = exp + 1;
    let body = if -4 < decpt && decpt <= 16 {
        if decpt <= 0 {
            format!("0.{}{digits}", "0".repeat((-decpt) as usize))
        } else if decpt >= n {
            format!("{digits}{}.0", "0".repeat((decpt - n) as usize))
        } else {
            format!("{}.{}", &digits[..decpt as usize], &digits[decpt as usize..])
        }
    } else {
        let m = if n > 1 { format!("{}.{}", &digits[..1], &digits[1..]) } else { digits.clone() };
        format!("{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    };
    if x < 0.0 {
        format!("-{body}")
    } else {
        body
    }
}
