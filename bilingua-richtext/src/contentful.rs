//! Contentful Rich Text: a `document` node tree. One unit per inline container (a paragraph or heading; list items and
//! table cells hold paragraphs). A marked text node or a link wrapper is a tag pair, a code span or an embedded
//! inline entry a standalone tag; everything else is kept as it is.

use crate::common::*;
use crate::json::Value;
use bilingua_model::*;
use std::collections::BTreeMap;

const WRAPPERS: &[&str] = &["hyperlink", "entry-hyperlink", "asset-hyperlink", "resource-hyperlink"];
const VOIDS: &[&str] = &["embedded-entry-inline", "embedded-resource-inline"];
const TEXT_BLOCKS: &[&str] = &["paragraph", "heading-1", "heading-2", "heading-3", "heading-4", "heading-5", "heading-6"];
const STRUCT_BLOCKS: &[&str] = &["document", "ordered-list", "unordered-list", "list-item", "blockquote", "table", "table-row", "table-cell", "table-header-cell"];

pub(crate) fn is_document(v: &Value) -> bool {
    v.has_str("nodeType", "document")
}

fn node_type(n: &Value) -> Option<&str> {
    n.get("nodeType").and_then(Value::as_str)
}

fn is_text(n: &Value) -> bool {
    n.is_obj() && node_type(n) == Some("text") && n.get("value").and_then(Value::as_str).is_some()
}

fn is_inline_child(n: &Value) -> bool {
    is_text(n) || node_type(n).is_some_and(|t| WRAPPERS.contains(&t) || VOIDS.contains(&t))
}

fn is_wrapper(n: &Value) -> bool {
    n.is_obj() && node_type(n).is_some_and(|t| WRAPPERS.contains(&t))
}

fn marks_of(n: &Value) -> &[Value] {
    n.get("marks").and_then(Value::as_arr).map(|a| a.as_slice()).unwrap_or(&[])
}

fn is_code(n: &Value) -> bool {
    marks_of(n).iter().any(|m| m.has_str("type", "code"))
}

fn plain_text_node(n: &Value) -> bool {
    !n.get("marks").is_some_and(Value::truthy) && !n.get("data").is_some_and(Value::truthy)
}

fn content_of(n: &Value) -> &[Value] {
    n.get("content").and_then(Value::as_arr).map(|a| a.as_slice()).unwrap_or(&[])
}

fn dashed(t: &str) -> String {
    t.replace('-', " ")
}

fn serialize(children: &[Value], b: &mut Builder) -> bool {
    for c in children {
        if is_text(c) {
            let v = c.get("value").and_then(Value::as_str).unwrap_or("");
            if v.contains(['\u{27E6}', '\u{27E7}']) {
                return false;
            }
            if is_code(c) {
                b.point("code", Token::X(c.clone()));
            } else if plain_text_node(c) {
                b.text(v);
            } else {
                let label = label_of(marks_of(c).iter().filter_map(|m| m.get("type").and_then(Value::as_str).map(str::to_string)).collect(), "text");
                let n = b.open(&label, Token::M(c.without("value")));
                b.text(v);
                b.close(n, &label);
            }
        } else if is_wrapper(c) && !content_of(c).is_empty() && content_of(c).iter().all(is_inline_child) {
            let label = dashed(node_type(c).unwrap_or("link"));
            let n = b.open(&label, Token::W(c.without("content")));
            if !serialize(content_of(c), b) {
                return false;
            }
            b.close(n, &label);
        } else {
            let label = node_type(c).map(dashed).unwrap_or_else(|| "node".into());
            b.point(&label, Token::X(c.clone()));
        }
    }
    true
}

fn walk(node: &Value, path: &mut Vec<Step>, out: &mut Vec<Plan>) {
    if !node.is_obj() {
        return;
    }
    let Some(kind) = node_type(node) else { return };
    let is_text_block = TEXT_BLOCKS.contains(&kind);
    if !is_text_block && !STRUCT_BLOCKS.contains(&kind) {
        return;
    }
    let content = content_of(node);
    if content.is_empty() {
        return;
    }
    let has_block_child = content.iter().any(|c| node_type(c).is_some_and(|t| TEXT_BLOCKS.contains(&t) || STRUCT_BLOCKS.contains(&t)));
    let inline_container = content.iter().all(is_inline_child) && content.iter().any(|c| is_text(c) || is_wrapper(c));
    if (is_text_block && !has_block_child) || inline_container {
        let mut b = Builder::default();
        if serialize(content, &mut b) && b.alpha {
            out.push(b.finish(path.clone(), kind, Vec::new()));
        }
        return;
    }
    for (i, child) in content.iter().enumerate() {
        path.push(k("content"));
        path.push(Step::I(i));
        walk(child, path, out);
        path.truncate(path.len() - 2);
    }
}

pub(crate) fn plan(root: &Value, _leaf: bool) -> Vec<Plan> {
    let mut out = Vec::new();
    if is_document(root) {
        walk(root, &mut Vec::new(), &mut out);
    }
    out
}

fn text_node(value: &str, shell: Option<&Value>) -> Value {
    let mut n = match shell {
        Some(s) => s.clone(),
        None => Value::Obj(vec![("nodeType".into(), Value::Str("text".into())), ("marks".into(), Value::Arr(vec![])), ("data".into(), Value::Obj(vec![]))]),
    };
    n.set("value", Value::Str(value.to_string()));
    n
}

struct Frame {
    num: u32,
    tok: Token,
    children: Vec<Value>,
    text: String,
}

fn rebuild(evs: &[Ev], tokens: &BTreeMap<u32, Token>) -> Option<Vec<Value>> {
    let mut root: Vec<Value> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    fn cur<'a>(root: &'a mut Vec<Value>, stack: &'a mut [Frame]) -> &'a mut Vec<Value> {
        match stack.last_mut() {
            Some(f) => &mut f.children,
            None => root,
        }
    }
    for ev in evs {
        match ev {
            Ev::T(s) => {
                if s.is_empty() {
                    continue;
                }
                match stack.last_mut() {
                    Some(f) if matches!(f.tok, Token::M(_)) => f.text.push_str(s),
                    _ => cur(&mut root, &mut stack).push(text_node(s, None)),
                }
            }
            Ev::P(n) => {
                if matches!(stack.last(), Some(Frame { tok: Token::M(_), .. })) {
                    return None;
                }
                let Token::X(v) = tokens.get(n)? else { return None };
                cur(&mut root, &mut stack).push(v.clone());
            }
            Ev::O(n) => {
                if matches!(stack.last(), Some(Frame { tok: Token::M(_), .. })) {
                    return None;
                }
                stack.push(Frame { num: *n, tok: tokens.get(n)?.clone(), children: Vec::new(), text: String::new() });
            }
            Ev::C(n) => {
                if stack.last().map(|f| f.num) != Some(*n) {
                    return None;
                }
                let f = stack.pop()?;
                match f.tok {
                    Token::M(shell) => {
                        if !f.text.is_empty() {
                            cur(&mut root, &mut stack).push(text_node(&f.text, Some(&shell)));
                        }
                    }
                    Token::W(mut shell) => {
                        // a link with no text (an empty hyperlink around an image) keeps its one empty text node
                        let mut children = f.children;
                        if children.is_empty() {
                            children.push(text_node("", None));
                        }
                        shell.set("content", Value::Arr(children));
                        cur(&mut root, &mut stack).push(shell);
                    }
                    Token::X(_) => return None,
                }
            }
        }
    }
    if !stack.is_empty() {
        return None;
    }
    Some(root)
}

fn skeleton(n: &Value, out: &mut BTreeMap<String, usize>) {
    if !n.is_obj() {
        return;
    }
    if node_type(n) != Some("text") {
        *out.entry(format!("{:?}", n.get("nodeType"))).or_default() += 1;
    }
    for c in content_of(n) {
        skeleton(c, out);
    }
}

pub(crate) fn apply(root: &mut Value, plans: &[Plan], outs: &[Option<Vec<Inline>>]) -> Result<(), Error> {
    let mut before = BTreeMap::new();
    skeleton(root, &mut before);
    for (i, (p, o)) in plans.iter().zip(outs).enumerate() {
        let Some(o) = o else { continue };
        if blank(o) {
            return Err(unwritable(i, "the translation is empty"));
        }
        let evs = events(o, &p.tokens).ok_or_else(|| unwritable(i, "the translation does not keep every placeholder of the source exactly once"))?;
        let rebuilt = rebuild(&evs, &p.tokens).filter(|r| !r.is_empty()).ok_or_else(|| unwritable(i, "the translation's placeholders are crossed, nested or unopened"))?;
        let node = node_mut(root, &p.path).ok_or_else(|| Error::Write("the original text no longer matches the document".into()))?;
        node.set("content", Value::Arr(rebuilt));
    }
    let mut after = BTreeMap::new();
    skeleton(root, &mut after);
    if before != after {
        return Err(Error::Unwritable { unit: None, reason: "the translated tree has a different node structure from the source".into() });
    }
    Ok(())
}
