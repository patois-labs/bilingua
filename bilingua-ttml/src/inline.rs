//! The content of one caption `<p>`: cutting it into text and inline codes, and writing a translation back.
//!
//! * `<br/>` is a standalone code;
//! * `<span ...>` (styled or timed text) is a paired code;
//! * any other child element (`<set>`, `<metadata>`, animation, a foreign element), a comment or a processing
//!   instruction is carried whole as one standalone code.
//!
//! The content has already passed the scanner, so it is well-formed: the tokenizer here only finds boundaries.

use crate::scan::unescape;
use bilingua_model::{py_lstrip, py_rstrip, py_strip, CodeData, Inline, TagKind};
use std::collections::BTreeMap;

/// Markup tokens of a well-formed fragment: comment, CDATA, processing instruction, end tag, start or empty tag.
pub fn tokens(f: &str) -> Vec<(usize, usize)> {
    let b = f.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = b[i..].iter().position(|c| *c == b'<') {
        let s = i + p;
        let rest = &f[s..];
        let end = if let Some(r) = rest.strip_prefix("<!--") {
            r.find("-->").map(|k| s + 4 + k + 3)
        } else if let Some(r) = rest.strip_prefix("<![CDATA[") {
            r.find("]]>").map(|k| s + 9 + k + 3)
        } else if let Some(r) = rest.strip_prefix("<?") {
            r.find("?>").map(|k| s + 2 + k + 2)
        } else if rest.starts_with("</") {
            rest.find('>').map(|k| s + k + 1)
        } else {
            let mut q = 0u8;
            let mut e = None;
            for (k, c) in b[s + 1..].iter().enumerate() {
                if q != 0 {
                    if *c == q {
                        q = 0;
                    }
                } else if *c == b'"' || *c == b'\'' {
                    q = *c;
                } else if *c == b'>' {
                    e = Some(s + 1 + k + 1);
                    break;
                }
            }
            e
        };
        let end = end.unwrap_or(f.len());
        out.push((s, end));
        i = end;
        if i >= b.len() {
            break;
        }
    }
    out
}

fn name_of(tok: &str) -> &str {
    let t = tok.strip_prefix('<').unwrap_or(tok);
    let t = t.strip_prefix('/').unwrap_or(t);
    let t = t.trim_start_matches(|c: char| c.is_whitespace());
    let end = t.find(|c: char| c.is_whitespace() || c == '/' || c == '>').unwrap_or(t.len());
    &t[..end]
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

#[derive(Clone, Copy, PartialEq)]
enum K {
    Text,
    Point,
    Br,
    Open,
    Close,
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.contains(['\r', '\n']) {
            out.push(' ');
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in s.chars() {
        if matches!(c, ' ' | '\t' | '\r' | '\n') {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// Source inlines and their codes for one `<p>` content. `preserve` is the effective `xml:space`.
pub fn tokenize(fragment: &str, preserve: bool) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
    let mut parts: Vec<(K, String)> = Vec::new();
    let mut n = 0usize;
    let mut pos = 0usize;
    let toks = tokens(fragment);
    let mut i = 0;
    let mut spans: Vec<usize> = Vec::new();
    let std_code = |s: &str, label: &str| CodeData { standalone: Some(s.to_string()), label: Some(label.into()), ..Default::default() };
    while i < toks.len() {
        let (ts, te) = toks[i];
        parts.push((K::Text, unescape(&fragment[pos..ts])));
        let tok = &fragment[ts..te];
        pos = te;
        i += 1;
        if tok.starts_with("<![CDATA[") {
            parts.push((K::Text, tok[9..tok.len() - 3].to_string()));
            continue;
        }
        if tok.starts_with("<!--") || tok.starts_with("<?") {
            n += 1;
            codes.insert(n.to_string(), std_code(tok, if tok.starts_with("<?") { "processing instruction" } else { "comment" }));
            parts.push((K::Point, n.to_string()));
            continue;
        }
        let name = name_of(tok);
        if tok.starts_with("</") {
            if let Some(k) = spans.pop() {
                if let Some(c) = codes.get_mut(&k.to_string()) {
                    c.close = Some(tok.to_string());
                }
                parts.push((K::Close, k.to_string()));
            }
            continue;
        }
        n += 1;
        let self_closing = tok.ends_with("/>");
        let loc = local(name);
        if loc == "br" {
            let mut raw_end = te;
            if !self_closing {
                // consume the balanced subtree up to the matching end tag
                let mut depth = 1;
                while i < toks.len() && depth > 0 {
                    let t2 = &fragment[toks[i].0..toks[i].1];
                    if !t2.starts_with("<!") && !t2.starts_with("<?") {
                        if t2.starts_with("</") {
                            depth -= 1;
                        } else if !t2.ends_with("/>") {
                            depth += 1;
                        }
                    }
                    raw_end = toks[i].1;
                    i += 1;
                }
                pos = raw_end;
            }
            codes.insert(n.to_string(), std_code(&fragment[ts..raw_end], "line break"));
            parts.push((K::Br, n.to_string()));
        } else if loc == "span" && !self_closing {
            codes.insert(n.to_string(), CodeData { open: Some(tok.to_string()), close: None, label: Some("span".into()), ..Default::default() });
            spans.push(n);
            parts.push((K::Open, n.to_string()));
        } else {
            let mut end = te;
            if !self_closing {
                let mut depth = 1;
                while i < toks.len() && depth > 0 {
                    let t2 = &fragment[toks[i].0..toks[i].1];
                    if !t2.starts_with("<!") && !t2.starts_with("<?") {
                        if t2.starts_with("</") {
                            depth -= 1;
                        } else if !t2.ends_with("/>") {
                            depth += 1;
                        }
                    }
                    end = toks[i].1;
                    i += 1;
                }
                pos = end;
            }
            codes.insert(n.to_string(), std_code(&fragment[ts..end], loc));
            parts.push((K::Point, n.to_string()));
        }
    }
    parts.push((K::Text, unescape(&fragment[pos..])));
    if !preserve {
        // xml:space="default": a run of whitespace holding a line break is layout, not content. It vanishes next to
        // a <br/> and collapses to one space elsewhere.
        for idx in 0..parts.len() {
            if parts[idx].0 != K::Text {
                continue;
            }
            let prev_br = idx > 0 && parts[idx - 1].0 == K::Br;
            let next_br = idx + 1 < parts.len() && parts[idx + 1].0 == K::Br;
            let mut v = collapse_ws(&parts[idx].1);
            if prev_br {
                v = v.trim_start_matches(' ').to_string();
            }
            if next_br {
                v = v.trim_end_matches(' ').to_string();
            }
            parts[idx].1 = v;
        }
    }
    let mut inl: Vec<Inline> = Vec::new();
    for (k, v) in parts {
        match k {
            K::Text => {
                if v.is_empty() {
                    continue;
                }
                match inl.last_mut() {
                    Some(Inline::Text(t)) => t.push_str(&v),
                    _ => inl.push(Inline::Text(v)),
                }
            }
            K::Point | K::Br => inl.push(Inline::tag(v.clone(), TagKind::Standalone, code_display(&codes, &v))),
            K::Open => inl.push(Inline::tag(v.clone(), TagKind::Open, code_display(&codes, &v))),
            K::Close => {
                let d = codes.get(&v).and_then(|c| c.close.clone()).unwrap_or_default();
                inl.push(Inline::tag(v, TagKind::Close, d));
            }
        }
    }
    // Python str.strip() on the joined text: only text at the very ends can be stripped
    if let Some(Inline::Text(t)) = inl.first_mut() {
        *t = py_lstrip(t).to_string();
    }
    if matches!(inl.first(), Some(Inline::Text(t)) if t.is_empty()) {
        inl.remove(0);
    }
    if let Some(Inline::Text(t)) = inl.last_mut() {
        *t = py_rstrip(t).to_string();
    }
    if matches!(inl.last(), Some(Inline::Text(t)) if t.is_empty()) {
        inl.pop();
    }
    (inl, codes)
}

fn code_display(codes: &BTreeMap<String, CodeData>, id: &str) -> String {
    codes.get(id).and_then(|c| c.standalone.clone().or_else(|| c.open.clone())).unwrap_or_default()
}

/// A unit has text when some text piece holds a non-whitespace character (codes alone do not make a caption).
pub fn has_text(inl: &[Inline]) -> bool {
    inl.iter().any(|i| matches!(i, Inline::Text(t) if !py_strip(t).is_empty()))
}

/// XML 1.0 characters only; pasted Word breaks become newlines.
pub fn xml_target_text(s: &str) -> String {
    s.chars()
        .map(|c| if matches!(c, '\u{0B}' | '\u{0C}') { '\n' } else { c })
        .filter(|c| !matches!(*c as u32, 0..=8 | 0x0E..=0x1F | 0xFFFE | 0xFFFF))
        .collect()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// A translation to `<p>` content. Tolerant: an unknown or repeated code and a stray close are dropped, an unclosed
/// span is closed at the end, a typed newline becomes a `<br/>`.
pub fn render(out: &[Inline], codes: &BTreeMap<String, CodeData>, br_name: &str) -> String {
    let mut res = String::new();
    let mut used: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut stack: Vec<(&str, Option<&str>)> = Vec::new();
    for i in out {
        match i {
            Inline::Text(t) => {
                let t = xml_target_text(t);
                let mut first = true;
                for line in escape(&t).split('\n') {
                    if !first {
                        res.push('<');
                        res.push_str(br_name);
                        res.push_str("/>");
                    }
                    first = false;
                    res.push_str(line);
                }
            }
            // hidden splice data (`~raw`, `~meta`) is never a code a translation can place: `~raw` would copy the
            // whole source caption into the target
            Inline::Tag { id, .. } if id.starts_with('~') => {}
            Inline::Tag { id, kind, .. } => {
                let entry = codes.get(id.as_str());
                let is_span = entry.is_some_and(|c| c.open.is_some());
                match kind {
                    TagKind::Close => {
                        if let Some((k, close)) = stack.last().copied() {
                            if k == id {
                                stack.pop();
                                if let Some(c) = close {
                                    res.push_str(c);
                                }
                            }
                        }
                    }
                    TagKind::Open => {
                        if entry.is_none() || used.contains(id.as_str()) || !is_span {
                            stack.push((id, None));
                            continue;
                        }
                        used.insert(id);
                        let c = entry.expect("checked");
                        res.push_str(c.open.as_deref().unwrap_or(""));
                        stack.push((id, c.close.as_deref()));
                    }
                    TagKind::Standalone => {
                        let Some(c) = entry else { continue };
                        if is_span || used.contains(id.as_str()) {
                            continue;
                        }
                        used.insert(id);
                        res.push_str(c.standalone.as_deref().unwrap_or(""));
                    }
                }
            }
        }
    }
    for (_, close) in stack.into_iter().rev() {
        if let Some(c) = close {
            res.push_str(c);
        }
    }
    res
}
