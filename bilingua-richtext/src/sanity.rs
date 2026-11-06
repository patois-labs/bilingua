//! Sanity Portable Text: an array of blocks. One unit per text block. A span with marks (a decorator or a link
//! annotation key) is a tag pair that keeps the span's `_key` and `marks`; a code span and an inline object are
//! standalone tags; plain spans are bare text. Every other block (image, code, custom objects), `markDefs`, `style`
//! and `listItem` stay as they are.

use crate::common::*;
use crate::json::Value;
use bilingua_model::*;
use std::collections::{BTreeMap, BTreeSet};

const PLAIN_KEYS: &[&str] = &["_key", "_type", "text", "marks"];

pub(crate) fn is_portable_text(v: &Value) -> bool {
    v.as_arr().is_some_and(|a| a.iter().any(|b| b.has_str("_type", "block") && b.get("children").and_then(Value::as_arr).is_some()))
}

fn is_span(n: &Value) -> bool {
    n.has_str("_type", "span") && n.get("text").and_then(Value::as_str).is_some()
}

fn has_mark(n: &Value, mark: &str) -> bool {
    match n.get("marks") {
        Some(Value::Arr(a)) => a.iter().any(|m| m.as_str() == Some(mark)),
        Some(Value::Str(s)) => s.contains(mark),
        _ => false,
    }
}

fn is_plain_span(n: &Value) -> bool {
    is_span(n) && !n.get("marks").is_some_and(Value::truthy) && matches!(n, Value::Obj(m) if m.iter().all(|(k, _)| PLAIN_KEYS.contains(&k.as_str())))
}

fn mark_label(block: &Value, span: &Value) -> String {
    let defs = block.get("markDefs").and_then(Value::as_arr).map(|a| a.as_slice()).unwrap_or(&[]);
    let parts: Vec<String> = match span.get("marks") {
        Some(Value::Arr(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .map(|m| defs.iter().find(|d| d.has_str("_key", m)).and_then(|d| d.get("_type").and_then(Value::as_str)).unwrap_or(m).to_string())
            .collect(),
        _ => Vec::new(),
    };
    label_of(parts, "text")
}

fn serialize(block: &Value, children: &[Value], b: &mut Builder) -> bool {
    for c in children {
        if is_span(c) {
            let t = c.get("text").and_then(Value::as_str).unwrap_or("");
            if t.contains(['\u{27E6}', '\u{27E7}']) {
                return false;
            }
            if is_plain_span(c) {
                b.text(t);
            } else if has_mark(c, "code") {
                b.point("code", Token::X(c.clone()));
            } else {
                let label = mark_label(block, c);
                let n = b.open(&label, Token::M(c.without("text")));
                b.text(t);
                b.close(n, &label);
            }
        } else {
            let label = c.get("_type").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| "object".into());
            b.point(&label, Token::X(c.clone()));
        }
    }
    true
}

pub(crate) fn plan(root: &Value, _leaf: bool) -> Vec<Plan> {
    let mut out = Vec::new();
    let Some(blocks) = root.as_arr() else { return out };
    for (i, b) in blocks.iter().enumerate() {
        if !b.has_str("_type", "block") {
            continue;
        }
        let Some(children) = b.get("children").and_then(Value::as_arr) else { continue };
        if children.is_empty() {
            continue;
        }
        let mut bl = Builder::default();
        if serialize(b, children, &mut bl) && bl.alpha {
            let kind = b.get("listItem").and_then(Value::as_str).map(|_| "list-item".to_string()).or_else(|| b.get("style").and_then(Value::as_str).map(str::to_string)).unwrap_or_else(|| "block".into());
            let mut extra = Vec::new();
            if let Some(k) = b.get("_key").and_then(Value::as_str) {
                extra.push(("_key".to_string(), k.to_string()));
            }
            out.push(bl.finish(vec![Step::I(i)], &kind, extra));
        }
    }
    out
}

fn seed_of(block: &Value) -> String {
    match block.get("_key") {
        Some(Value::Str(s)) if !s.is_empty() => s.clone(),
        Some(Value::Num(n)) if n != "0" => n.clone(),
        _ => "b".into(),
    }
}

fn rebuild(evs: &[Ev], tokens: &BTreeMap<u32, Token>, block: &Value) -> Option<Vec<Value>> {
    let mut taken: BTreeSet<String> = block.get("children").and_then(Value::as_arr).map(|a| a.iter().filter_map(|c| c.get("_key").and_then(Value::as_str).map(str::to_string)).collect()).unwrap_or_default();
    let seed = seed_of(block);
    let mut counter = 0u32;
    let mut out: Vec<Value> = Vec::new();
    let mut open: Option<(u32, String)> = None;
    for ev in evs {
        match ev {
            Ev::T(s) => match open.as_mut() {
                Some((_, buf)) => buf.push_str(s),
                None => {
                    if !s.is_empty() {
                        loop {
                            counter += 1;
                            let key = format!("{seed}t{counter}");
                            if taken.insert(key.clone()) {
                                out.push(Value::Obj(vec![
                                    ("_type".into(), Value::Str("span".into())),
                                    ("_key".into(), Value::Str(key)),
                                    ("text".into(), Value::Str(s.clone())),
                                    ("marks".into(), Value::Arr(vec![])),
                                ]));
                                break;
                            }
                        }
                    }
                }
            },
            Ev::P(n) => {
                if open.is_some() {
                    return None;
                }
                let Token::X(v) = tokens.get(n)? else { return None };
                out.push(v.clone());
            }
            Ev::O(n) => {
                if open.is_some() {
                    return None;
                }
                open = Some((*n, String::new()));
            }
            Ev::C(n) => match open.take() {
                Some((m, buf)) if m == *n => {
                    if !buf.is_empty() {
                        let Token::M(shell) = tokens.get(n)? else { return None };
                        let mut span = shell.clone();
                        span.set("text", Value::Str(buf));
                        out.push(span);
                    }
                }
                _ => return None,
            },
        }
    }
    if open.is_some() {
        return None;
    }
    Some(out)
}

pub(crate) fn apply(root: &mut Value, plans: &[Plan], outs: &[Option<Vec<Inline>>]) -> Result<(), Error> {
    for (i, (p, o)) in plans.iter().zip(outs).enumerate() {
        let Some(o) = o else { continue };
        if blank(o) {
            return Err(unwritable(i, "the translation is empty"));
        }
        let evs = events(o, &p.tokens).ok_or_else(|| unwritable(i, "the translation does not keep every placeholder of the source exactly once"))?;
        let node = node_mut(root, &p.path).ok_or_else(|| Error::Write("the original text no longer matches the document".into()))?;
        let rebuilt = rebuild(&evs, &p.tokens, node).filter(|r| !r.is_empty()).ok_or_else(|| unwritable(i, "a marked span may hold text only, and its placeholders must be in pairs"))?;
        node.set("children", Value::Arr(rebuilt));
    }
    Ok(())
}
