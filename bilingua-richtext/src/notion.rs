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

/// Where the blocks are: a list at the root or under `results`, `children` or `blocks` (a list response, an
/// append-children body, a page with its blocks), or the root is one block.
enum Shape {
    List(Vec<Step>),
    Single,
}

const LIST_KEYS: [&str; 3] = ["results", "children", "blocks"];

fn shape(root: &Value) -> Option<Shape> {
    if root.as_arr().is_some() {
        return Some(Shape::List(Vec::new()));
    }
    if !root.is_obj() {
        return None;
    }
    if let Some(key) = LIST_KEYS.iter().find(|key| root.get(key).and_then(Value::as_arr).is_some()) {
        return Some(Shape::List(vec![k(key)]));
    }
    is_block(root).then_some(Shape::Single)
}

fn is_block(v: &Value) -> bool {
    v.is_obj() && v.get("type").and_then(Value::as_str).is_some_and(|t| v.get(t).is_some_and(Value::is_obj))
}

pub(crate) fn is_notion(root: &Value) -> bool {
    if is_block(root) && root.has_str("object", "block") {
        return true;
    }
    // a sniff reads a bare list or a `results` list only; the other shapes open by extension
    let arr = if root.is_obj() { root.get("results").and_then(Value::as_arr) } else { root.as_arr() };
    arr.is_some_and(|a| !a.is_empty() && a.iter().all(|b| is_block(b) || !b.is_obj()) && a.iter().any(|b| is_block(b) && (b.has_str("object", "block") || b.get("id").is_some())))
}

/// What extract takes: a block array (also an empty one, or one with non-block entries, which LocOS skips), a
/// response or page object with the blocks under `results`, `children` or `blocks`, or a single block.
pub(crate) fn accepts(root: &Value) -> bool {
    shape(root).is_some()
}

fn is_run(el: &Value) -> bool {
    el.has_str("type", "text") && el.get("text").is_some_and(|t| t.get("content").and_then(Value::as_str).is_some())
}

fn push_runs(base: &[Step], sub: &[Step], kind: &str, a: &[Value], block_id: Option<&str>, out: &mut Vec<Plan>) {
    for (j, el) in a.iter().enumerate() {
        if is_run(el) {
            let mut path = base.to_vec();
            path.extend(sub.iter().cloned());
            path.push(Step::I(j));
            let mut b = Builder::default();
            b.text(el.get("text").and_then(|t| t.get("content")).and_then(Value::as_str).unwrap_or(""));
            let mut extra = Vec::new();
            if let Some(id) = block_id {
                extra.push(("block_id".to_string(), id.to_string()));
            }
            out.push(b.finish(path, kind, extra));
        }
    }
}

fn walk(blocks: &Value, base: &mut Vec<Step>, in_code: bool, out: &mut Vec<Plan>) {
    let Some(arr) = blocks.as_arr() else { return };
    for (bi, block) in arr.iter().enumerate() {
        base.push(Step::I(bi));
        walk_block(block, base, in_code, out);
        base.pop();
    }
}

/// One block; `base` is the path to it.
fn walk_block(block: &Value, base: &mut Vec<Step>, in_code: bool, out: &mut Vec<Plan>) {
    let Some(ntype) = block.get("type").and_then(Value::as_str) else { return };
    let Some(container) = block.get(ntype).filter(|c| c.is_obj()) else { return };
    let this_code = in_code || ntype == "code";
    let n0 = base.len();
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
        push_runs(base, &sub, kind, a, block.get("id").and_then(Value::as_str), out);
    }
    if let Some(children) = container.get("children") {
        base.push(k("children"));
        walk(children, base, this_code, out);
        base.pop();
    }
    base.truncate(n0);
}

/// The title runs of a page object (`properties.<name>` of type `title`), before its blocks.
fn page_titles(root: &Value, out: &mut Vec<Plan>) {
    let Some(props) = root.get("properties").filter(|p| p.is_obj()) else { return };
    let Value::Obj(entries) = props else { return };
    for (name, prop) in entries {
        if prop.has_str("type", "title") {
            if let Some(a) = prop.get("title").and_then(Value::as_arr) {
                push_runs(&[k("properties"), k(name)], &[k("title")], "title", a, None, out);
            }
        }
    }
}

pub(crate) fn plan(root: &Value, _leaf: bool) -> Vec<Plan> {
    let mut out = Vec::new();
    match shape(root) {
        Some(Shape::Single) => walk_block(root, &mut Vec::new(), false, &mut out),
        Some(Shape::List(mut base)) => {
            if !base.is_empty() {
                page_titles(root, &mut out);
            }
            let list = match base.first() {
                None => Some(root),
                Some(Step::K(key)) => root.get(key),
                Some(Step::I(_)) => None,
            };
            if let Some(l) = list {
                walk(l, &mut base, false, &mut out);
            }
        }
        None => {}
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
