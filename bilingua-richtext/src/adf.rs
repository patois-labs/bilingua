//! Jira ADF (Atlassian Document Format). Default (`adf.units` = `inline`): one unit per inline container (a paragraph,
//! heading, table cell paragraph), a marked text leaf is a tag pair that keeps its `marks`, a mention, emoji, card,
//! date, status, media or `hardBreak` a standalone tag. `adf.units` = `leaf`: one unit per text leaf, no tags (the
//! older Localization OS walker). Code block text is never a unit in either mode.

use crate::common::*;
use crate::json::Value;
use bilingua_model::*;

const OPAQUE: &[&str] = &["mention", "emoji", "inlineCard", "blockCard", "embedCard", "date", "status", "media", "mediaSingle", "mediaGroup", "mediaInline", "hardBreak", "rule", "placeholder"];

pub(crate) fn is_adf(v: &Value) -> bool {
    v.has_str("type", "doc") && v.get("content").and_then(Value::as_arr).is_some()
}

/// What extract takes: an object typed `doc` (LocOS walks any object; an empty doc may lack `content`). The sniff
/// also needs the `content` array.
pub(crate) fn accepts(v: &Value) -> bool {
    v.has_str("type", "doc")
}

fn ty(n: &Value) -> Option<&str> {
    n.get("type").and_then(Value::as_str)
}

fn is_leaf(n: &Value) -> bool {
    n.is_obj() && ty(n) == Some("text") && n.get("text").and_then(Value::as_str).is_some()
}

fn opaque(n: &Value) -> bool {
    n.is_obj() && ty(n).is_some_and(|t| OPAQUE.contains(&t))
}

fn is_inline_child(n: &Value) -> bool {
    is_leaf(n) || opaque(n)
}

fn content_of(n: &Value) -> Option<&Vec<Value>> {
    n.get("content").and_then(Value::as_arr)
}

fn marks_label(marks: &Value) -> String {
    let parts = marks.as_arr().map(|a| a.iter().filter_map(|m| m.get("type").and_then(Value::as_str).map(str::to_string)).collect()).unwrap_or_default();
    label_of(parts, "text")
}

fn walk_leaf(node: &Value, path: &mut Vec<Step>, in_code: bool, out: &mut Vec<Plan>) {
    if !node.is_obj() || opaque(node) {
        return;
    }
    if is_leaf(node) {
        if !in_code {
            let mut b = Builder::default();
            b.text(node.get("text").and_then(Value::as_str).unwrap_or(""));
            let mut p = b.finish(path.clone(), "text", Vec::new());
            p.leaf = true;
            out.push(p);
        }
        return;
    }
    let code = in_code || ty(node) == Some("codeBlock");
    if let Some(c) = content_of(node) {
        for (i, child) in c.iter().enumerate() {
            path.push(k("content"));
            path.push(Step::I(i));
            walk_leaf(child, path, code, out);
            path.truncate(path.len() - 2);
        }
    }
}

fn walk_inline(node: &Value, path: &mut Vec<Step>, in_code: bool, out: &mut Vec<Plan>) {
    if !node.is_obj() || opaque(node) {
        return;
    }
    let Some(content) = content_of(node) else { return };
    let code = in_code || ty(node) == Some("codeBlock");
    if content.iter().any(is_leaf) && content.iter().all(is_inline_child) {
        if !code {
            let mut b = Builder::default();
            for c in content {
                if is_leaf(c) {
                    let t = c.get("text").and_then(Value::as_str).unwrap_or("");
                    match c.get("marks") {
                        Some(m) if m.truthy() => {
                            let label = marks_label(m);
                            let n = b.open(&label, Token::M(m.clone()));
                            b.text(t);
                            b.close(n, &label);
                        }
                        _ => b.text(t),
                    }
                } else {
                    let label = ty(c).unwrap_or("node").to_string();
                    b.point(&label, Token::X(c.clone()));
                }
            }
            out.push(b.finish(path.clone(), ty(node).unwrap_or("paragraph"), Vec::new()));
        }
        return;
    }
    for (i, child) in content.iter().enumerate() {
        path.push(k("content"));
        path.push(Step::I(i));
        walk_inline(child, path, code, out);
        path.truncate(path.len() - 2);
    }
}

pub(crate) fn plan(root: &Value, leaf: bool) -> Vec<Plan> {
    let mut out = Vec::new();
    if !root.is_obj() {
        return out;
    }
    if leaf {
        if let Some(c) = content_of(root) {
            for (i, child) in c.iter().enumerate() {
                let mut path = vec![k("content"), Step::I(i)];
                walk_leaf(child, &mut path, false, &mut out);
            }
        }
    } else {
        walk_inline(root, &mut Vec::new(), false, &mut out);
    }
    out
}

fn emit(out: &mut Vec<Value>, text: &str, marks: Option<&Value>) {
    if text.is_empty() {
        return;
    }
    let mut n = Value::Obj(vec![("type".into(), Value::Str("text".into())), ("text".into(), Value::Str(text.to_string()))]);
    if let Some(m) = marks.filter(|m| m.truthy()) {
        n.set("marks", m.clone());
    }
    out.push(n);
}

fn rebuild(evs: &[Ev], p: &Plan) -> Option<Vec<Value>> {
    let mut out = Vec::new();
    let mut open: Option<(u32, &Value)> = None;
    for ev in evs {
        match ev {
            Ev::T(s) => emit(&mut out, s, open.map(|(_, m)| m)),
            Ev::P(n) => {
                let Token::X(v) = p.tokens.get(n)? else { return None };
                out.push(v.clone());
            }
            Ev::O(n) => {
                if open.is_some() {
                    return None;
                }
                let Token::M(m) = p.tokens.get(n)? else { return None };
                open = Some((*n, m));
            }
            Ev::C(n) => match open {
                Some((m, _)) if m == *n => open = None,
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
        let node = node_mut(root, &p.path).ok_or_else(|| Error::Write("the original text no longer matches the document".into()))?;
        // the leaf walk's plans only: an inline container may itself be typed `text` (a malformed leaf with content)
        if p.leaf {
            if !is_leaf(node) {
                return Err(Error::Write("the original text no longer matches the document".into()));
            }
            node.set("text", Value::Str(plain_text(o)));
            continue;
        }
        let evs = events(o, &p.tokens).ok_or_else(|| unwritable(i, "the translation does not keep every placeholder of the source exactly once"))?;
        let rebuilt = rebuild(&evs, p).ok_or_else(|| unwritable(i, "a marked span may not contain another, and its placeholders must be in pairs"))?;
        // an empty translation keeps the source content (LocOS)
        if !rebuilt.is_empty() {
            node.set("content", Value::Arr(rebuilt));
        }
    }
    Ok(())
}

/// The type of every non-text node, depth first through `content` (LocOS `_skeleton`): the Jira connector refuses an
/// inline rebuild whose skeleton differs from the source's (`structure_ok_inline`), so a moved mention or emoji fails
/// there. Used under the LocOS profile.
pub(crate) fn skeleton(n: &Value, out: &mut Vec<Option<Value>>) {
    if !n.is_obj() {
        return;
    }
    if ty(n) != Some("text") {
        out.push(n.get("type").cloned());
    }
    if let Some(c) = content_of(n) {
        for x in c {
            skeleton(x, out);
        }
    }
}
