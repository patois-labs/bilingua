//! The JSON resource dialects LocOS recognises by shape: Chrome/WebExtension `messages.json`, FormatJS, Transifex
//! Structured JSON, go-i18n (object and v1 array forms) and ARB-shaped files. Detection is strict: only a file whose
//! every entry fits the dialect leaves the generic walk.

use crate::tree::{Node, Obj};

pub const CLDR: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];
const CHROME_KEYS: [&str; 3] = ["message", "description", "placeholders"];
const FORMATJS_KEYS: [&str; 8] = ["defaultMessage", "description", "id", "file", "start", "end", "line", "col"];
const STRUCTURED_KEYS: [&str; 4] = ["string", "context", "developer_comment", "character_limit"];
const GO_RESERVED: [&str; 12] = ["id", "description", "hash", "leftdelim", "rightdelim", "translation", "zero", "one", "two", "few", "many", "other"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Chrome,
    FormatJs,
    Structured,
    Go,
    Arb,
}

impl Dialect {
    pub fn name(self) -> &'static str {
        match self {
            Dialect::Chrome => "chrome",
            Dialect::FormatJs => "formatjs",
            Dialect::Structured => "structured",
            Dialect::Go => "go",
            Dialect::Arb => "arb",
        }
    }

    pub fn parse(s: &str) -> Option<Dialect> {
        Some(match s {
            "chrome" => Dialect::Chrome,
            "formatjs" => Dialect::FormatJs,
            "structured" => Dialect::Structured,
            "go" => Dialect::Go,
            "arb" => Dialect::Arb,
            _ => return None,
        })
    }
}

fn keys_within(o: &Obj, allowed: &[&str]) -> bool {
    o.pairs.iter().all(|p| allowed.contains(&p.key.as_str()))
}

fn is_chrome(root: &Node) -> bool {
    let Some(r) = root.as_obj() else { return false };
    if r.pairs.is_empty() {
        return false;
    }
    r.pairs.iter().all(|p| {
        let Some(v) = p.value.as_obj() else { return false };
        if !v.get("message").is_some_and(Node::is_str) || !keys_within(v, &CHROME_KEYS) {
            return false;
        }
        match v.get("placeholders") {
            None => true,
            Some(Node::Obj(ph)) => ph.pairs.iter().all(|x| x.value.as_obj().is_some()),
            Some(_) => false,
        }
    })
}

fn is_formatjs(root: &Node) -> bool {
    let Some(r) = root.as_obj() else { return false };
    !r.pairs.is_empty() && r.pairs.iter().all(|p| p.value.as_obj().is_some_and(|v| v.get("defaultMessage").is_some_and(Node::is_str) && keys_within(v, &FORMATJS_KEYS)))
}

fn structured_leaf(v: &Node) -> bool {
    v.as_obj().is_some_and(|o| o.get("string").is_some_and(Node::is_str) && keys_within(o, &STRUCTURED_KEYS))
}

fn is_structured(root: &Node) -> bool {
    let Some(r) = root.as_obj() else { return false };
    if r.pairs.is_empty() {
        return false;
    }
    let mut leaves = 0usize;
    let mut stack = vec![r];
    while let Some(node) = stack.pop() {
        for p in &node.pairs {
            if structured_leaf(&p.value) {
                leaves += 1;
            } else if let Some(o) = p.value.as_obj().filter(|o| !o.pairs.is_empty() && !o.has("string")) {
                stack.push(o);
            } else {
                return false;
            }
        }
    }
    leaves > 0
}

fn lower_keys(o: &Obj) -> Vec<String> {
    o.pairs.iter().map(|p| p.key.to_lowercase()).collect()
}

fn go_message(v: &Node) -> bool {
    let Some(o) = v.as_obj() else { return false };
    if o.pairs.is_empty() {
        return false;
    }
    let keys = lower_keys(o);
    if !keys.iter().all(|k| GO_RESERVED.contains(&k.as_str())) {
        return false;
    }
    if !keys.iter().any(|k| CLDR.contains(&k.as_str()) || k == "translation") {
        return false;
    }
    for p in &o.pairs {
        let lk = p.key.to_lowercase();
        if CLDR.contains(&lk.as_str()) && !p.value.is_str() {
            return false;
        }
        if lk == "translation" && !(p.value.is_str() || go_cats_obj(Some(&p.value))) {
            return false;
        }
    }
    true
}

fn go_cats_obj(v: Option<&Node>) -> bool {
    v.and_then(Node::as_obj).is_some_and(|o| !o.pairs.is_empty() && o.pairs.iter().all(|p| CLDR.contains(&p.key.to_lowercase().as_str()) && p.value.is_str()))
}

fn go_plural(o: &Obj) -> bool {
    let keys = lower_keys(o);
    keys.iter().any(|k| k == "other") && keys.iter().any(|k| k != "other" && CLDR.contains(&k.as_str()))
}

fn is_go(root: &Node) -> bool {
    if let Node::Arr { items, .. } = root {
        return !items.is_empty()
            && items.iter().all(|v| v.as_obj().is_some_and(|o| o.get("id").is_some_and(Node::is_str) && o.has("translation")) && go_message(v));
    }
    let Some(r) = root.as_obj() else { return false };
    if r.pairs.is_empty() {
        return false;
    }
    let mut plural_seen = false;
    let mut stack = vec![r];
    while let Some(node) = stack.pop() {
        for p in &node.pairs {
            if p.value.is_str() {
                continue;
            }
            if go_message(&p.value) {
                let o = p.value.as_obj().expect("a go message is an object");
                plural_seen = plural_seen || go_plural(o) || go_cats_obj(o.get("translation"));
            } else if let Some(o) = p.value.as_obj().filter(|o| !o.pairs.is_empty()) {
                stack.push(o);
            } else {
                return false;
            }
        }
    }
    plural_seen
}

fn is_arb_shape(root: &Node) -> bool {
    root.as_obj().is_some_and(|o| o.get("@@locale").is_some_and(Node::is_str))
}

/// The dialect of a file, `None` for the generic walk.
pub fn detect(root: &Node) -> Option<Dialect> {
    if is_chrome(root) {
        Some(Dialect::Chrome)
    } else if is_formatjs(root) {
        Some(Dialect::FormatJs)
    } else if is_structured(root) {
        Some(Dialect::Structured)
    } else if is_go(root) {
        Some(Dialect::Go)
    } else if is_arb_shape(root) {
        Some(Dialect::Arb)
    } else {
        None
    }
}

/// How a message string is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    Icu,
    Chrome,
    Go,
}

/// A message string: its node, syntax, JSON pointer and the object that holds its description, when it has one.
pub struct Msg<'a> {
    pub start: usize,
    pub end: usize,
    pub value: &'a str,
    pub syntax: Syntax,
    pub pointer: String,
    pub notes: Vec<String>,
}

fn esc(k: &str) -> String {
    k.replace('~', "~0").replace('/', "~1")
}

fn notes_of(o: &Obj, keys: &[&str]) -> Vec<String> {
    let mut v = Vec::new();
    for k in keys {
        if let Some(Node::Str { val: Some(s), .. }) = o.get(k) {
            if !s.trim().is_empty() {
                v.push(s.clone());
            }
        }
    }
    v
}

/// The message strings of the file in document order (a string with a lone surrogate is left out).
pub fn messages<'a>(root: &'a Node, d: Dialect) -> Vec<Msg<'a>> {
    let mut out = Vec::new();
    let mut path: Vec<String> = Vec::new();
    match d {
        Dialect::Arb => {
            let Some(r) = root.as_obj() else { return out };
            let meta: std::collections::HashMap<&str, &Node> =
                r.pairs.iter().filter_map(|p| p.key.strip_prefix('@').filter(|k| !k.starts_with('@')).map(|k| (k, &p.value))).collect();
            for p in &r.pairs {
                if p.key.starts_with('@') {
                    continue;
                }
                let notes = meta.get(p.key.as_str()).and_then(|n| n.as_obj()).map(|o| notes_of(o, &["description", "context"])).unwrap_or_default();
                push(&mut out, &p.value, Syntax::Icu, format!("/{}", esc(&p.key)), notes);
            }
        }
        Dialect::Chrome => {
            if let Some(r) = root.as_obj() {
                for p in &r.pairs {
                    if let Some(v) = p.value.as_obj() {
                        if let Some(m) = v.get("message") {
                            push(&mut out, m, Syntax::Chrome, format!("/{}/message", esc(&p.key)), notes_of(v, &["description"]));
                        }
                    }
                }
            }
        }
        Dialect::FormatJs => {
            if let Some(r) = root.as_obj() {
                for p in &r.pairs {
                    if let Some(v) = p.value.as_obj() {
                        if let Some(m) = v.get("defaultMessage") {
                            push(&mut out, m, Syntax::Icu, format!("/{}/defaultMessage", esc(&p.key)), notes_of(v, &["description"]));
                        }
                    }
                }
            }
        }
        Dialect::Structured => {
            if let Some(r) = root.as_obj() {
                walk_structured(r, &mut path, &mut out);
            }
        }
        Dialect::Go => match root {
            Node::Arr { items, .. } => {
                for (i, v) in items.iter().enumerate() {
                    if let Some(o) = v.as_obj() {
                        go_strings(o, format!("/{i}"), &mut out);
                    }
                }
            }
            Node::Obj(r) => walk_go(r, &mut path, &mut out),
            _ => {}
        },
    }
    out
}

fn push<'a>(out: &mut Vec<Msg<'a>>, n: &'a Node, syntax: Syntax, pointer: String, notes: Vec<String>) {
    if let Node::Str { val: Some(v), start, end } = n {
        out.push(Msg { start: *start, end: *end, value: v, syntax, pointer, notes });
    }
}

fn ptr(path: &[String]) -> String {
    path.iter().map(|p| format!("/{p}")).collect()
}

fn walk_structured<'a>(o: &'a Obj, path: &mut Vec<String>, out: &mut Vec<Msg<'a>>) {
    for p in &o.pairs {
        path.push(esc(&p.key));
        if structured_leaf(&p.value) {
            let v = p.value.as_obj().expect("a leaf is an object");
            let mut pointer = ptr(path);
            pointer.push_str("/string");
            push(out, v.get("string").expect("a leaf has a string"), Syntax::Icu, pointer, notes_of(v, &["context", "developer_comment"]));
        } else if let Some(c) = p.value.as_obj() {
            walk_structured(c, path, out);
        }
        path.pop();
    }
}

fn walk_go<'a>(o: &'a Obj, path: &mut Vec<String>, out: &mut Vec<Msg<'a>>) {
    for p in &o.pairs {
        path.push(esc(&p.key));
        if p.value.is_str() {
            push(out, &p.value, Syntax::Go, ptr(path), Vec::new());
        } else if go_message(&p.value) {
            go_strings(p.value.as_obj().expect("object"), ptr(path), out);
        } else if let Some(c) = p.value.as_obj() {
            walk_go(c, path, out);
        }
        path.pop();
    }
}

fn go_strings<'a>(v: &'a Obj, base: String, out: &mut Vec<Msg<'a>>) {
    let notes = notes_of(v, &["description"]);
    for p in &v.pairs {
        let lk = p.key.to_lowercase();
        let here = format!("{base}/{}", esc(&p.key));
        if CLDR.contains(&lk.as_str()) {
            push(out, &p.value, Syntax::Go, here, notes.clone());
        } else if lk == "translation" {
            match &p.value {
                Node::Obj(c) => {
                    for q in &c.pairs {
                        push(out, &q.value, Syntax::Go, format!("{here}/{}", esc(&q.key)), notes.clone());
                    }
                }
                n => push(out, n, Syntax::Go, here, notes.clone()),
            }
        }
    }
}

/// Every go-i18n object whose keys are plural categories, in document order.
pub fn go_plural_objects(root: &Node) -> Vec<&Obj> {
    fn walk<'a>(n: &'a Node, found: &mut Vec<&'a Obj>) {
        match n {
            Node::Arr { items, .. } => items.iter().for_each(|v| walk(v, found)),
            Node::Obj(o) => {
                if go_message(n) {
                    let tr = o.get("translation");
                    if go_cats_obj(tr) {
                        found.push(tr.and_then(Node::as_obj).expect("object"));
                    } else if go_plural(o) {
                        found.push(o);
                    }
                    return;
                }
                o.pairs.iter().for_each(|p| walk(&p.value, found));
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(root, &mut found);
    found
}
