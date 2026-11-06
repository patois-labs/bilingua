//! Notion blocks: a list of block objects (or a list response with `results`), each with its content under the key of
//! its `type`. One unit per `text` run of a `rich_text` array, a `caption` or a table row cell, as Localization OS
//! reads them: the run's annotations and link are kept as they are, so a bold word is its own unit. Mentions and
//! equations are kept. The `rich_text` of a code block (and of anything inside one) is never a unit; its caption is.
//! A translation longer than Notion's 2000 character cap is written as several runs with the same annotations.

use crate::common::*;
use crate::json::Value;
use bilingua_model::*;

/// Notion's cap on the content of one run.
const RUN_MAX: usize = 2000;

pub(crate) fn blocks_path(root: &Value) -> Option<Vec<Step>> {
    match root {
        Value::Arr(_) => Some(Vec::new()),
        v if v.is_obj() && v.get("results").and_then(Value::as_arr).is_some() => Some(vec![k("results")]),
        _ => None,
    }
}

fn is_block(v: &Value) -> bool {
    v.is_obj() && v.get("type").and_then(Value::as_str).is_some_and(|t| v.get(t).is_some_and(Value::is_obj))
}

pub(crate) fn is_notion(root: &Value) -> bool {
    let Some(path) = blocks_path(root) else { return false };
    let arr = if path.is_empty() { root.as_arr() } else { root.get("results").and_then(Value::as_arr) };
    arr.is_some_and(|a| !a.is_empty() && a.iter().all(|b| is_block(b) || !b.is_obj()) && a.iter().any(|b| is_block(b) && (b.has_str("object", "block") || b.get("id").is_some())))
}

/// What extract takes: a block array (also an empty one, or one with non-block entries, which LocOS skips) or a list
/// response with `results`. The sniff needs real blocks.
pub(crate) fn accepts(root: &Value) -> bool {
    blocks_path(root).is_some()
}

fn is_run(el: &Value) -> bool {
    el.has_str("type", "text") && el.get("text").is_some_and(|t| t.get("content").and_then(Value::as_str).is_some())
}

fn walk(blocks: &Value, base: &mut Vec<Step>, in_code: bool, out: &mut Vec<Plan>) {
    let Some(arr) = blocks.as_arr() else { return };
    for (bi, block) in arr.iter().enumerate() {
        let Some(ntype) = block.get("type").and_then(Value::as_str) else { continue };
        let Some(container) = block.get(ntype).filter(|c| c.is_obj()) else { continue };
        let this_code = in_code || ntype == "code";
        let n0 = base.len();
        base.push(Step::I(bi));
        base.push(k(ntype));
        let mut arrays: Vec<(Vec<Step>, &str, &Vec<Value>)> = Vec::new();
        if let Some(cells) = container.get("cells").and_then(Value::as_arr) {
            for (ci, cell) in cells.iter().enumerate() {
                if let Some(a) = cell.as_arr() {
                    arrays.push((vec![k("cells"), Step::I(ci)], "cell", a));
                }
            }
        }
        if !this_code {
            if let Some(a) = container.get("rich_text").and_then(Value::as_arr) {
                arrays.push((vec![k("rich_text")], ntype, a));
            }
        }
        if let Some(a) = container.get("caption").and_then(Value::as_arr) {
            arrays.push((vec![k("caption")], "caption", a));
        }
        for (sub, kind, a) in arrays {
            for (j, el) in a.iter().enumerate() {
                if is_run(el) {
                    let mut path = base.clone();
                    path.extend(sub.iter().cloned());
                    path.push(Step::I(j));
                    let mut b = Builder::default();
                    b.text(el.get("text").and_then(|t| t.get("content")).and_then(Value::as_str).unwrap_or(""));
                    let mut extra = Vec::new();
                    if let Some(id) = block.get("id").and_then(Value::as_str) {
                        extra.push(("block_id".to_string(), id.to_string()));
                    }
                    out.push(b.finish(path, kind, extra));
                }
            }
        }
        if let Some(children) = container.get("children") {
            base.push(k("children"));
            walk(children, base, this_code, out);
            base.pop();
        }
        base.truncate(n0);
    }
}

pub(crate) fn plan(root: &Value, _leaf: bool) -> Vec<Plan> {
    let mut out = Vec::new();
    let Some(mut base) = blocks_path(root) else { return out };
    let list = if base.is_empty() { Some(root) } else { root.get("results") };
    if let Some(l) = list {
        walk(l, &mut base, false, &mut out);
    }
    out
}

pub(crate) fn apply(root: &mut Value, plans: &[Plan], outs: &[Option<Vec<Inline>>]) -> Result<(), Error> {
    // from the last run to the first: a split adds runs after its own place, so earlier paths stay valid
    for (p, o) in plans.iter().zip(outs).rev() {
        let Some(o) = o else { continue };
        let text = plain_text(o);
        let Some((Step::I(at), parent)) = p.path.split_last().map(|(l, r)| (l.clone(), r)) else { continue };
        let arr = node_mut(root, parent).and_then(Value::as_arr_mut).ok_or_else(|| Error::Write("the original text no longer matches the document".into()))?;
        let run = arr.get(at).cloned().ok_or_else(|| Error::Write("the original text no longer matches the document".into()))?;
        let chars: Vec<char> = text.chars().collect();
        let pieces: Vec<String> = if chars.is_empty() { vec![String::new()] } else { chars.chunks(RUN_MAX).map(|c| c.iter().collect()).collect() };
        let new: Vec<Value> = pieces
            .into_iter()
            .map(|piece| {
                let mut r = run.clone();
                if let Some(t) = r.get_mut("text") {
                    t.set("content", Value::Str(piece.clone()));
                }
                r.set("plain_text", Value::Str(piece));
                r
            })
            .collect();
        arr.splice(at..=at, new);
    }
    Ok(())
}
