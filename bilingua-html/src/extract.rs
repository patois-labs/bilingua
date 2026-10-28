//! HTML to [`Document`]. The text of each unit is cut out of the input by byte range; everything else stays in
//! the skeleton exactly as it was.

use crate::ent::*;
use crate::tok::*;
use bilingua_model::*;
use std::collections::{BTreeMap, HashMap};

/// Inline elements: they stay inside a unit as tags.
pub const INLINE: &[&str] = &[
    "b", "strong", "i", "em", "a", "span", "code", "sup", "sub", "small", "br", "img", "abbr", "mark", "u", "s",
    "cite", "q", "del", "ins", "time", "wbr", "font", "big", "bdi", "bdo", "dfn", "strike", "var", "kbd", "samp",
    "input", "tt", "data",
];
const PROTECTED: &[&str] = &["code", "kbd", "samp"];
const SKIP_SUBTREE: &[&str] = &["pre", "template", "svg", "math", "textarea", "script", "style"];

/// Reserved code ids (they start with `~`, which never collides with the numeric tag ids).
pub const RAW: &str = "~raw";
pub const VALUE: &str = "~v";
pub const OWNER: &str = "~attr";

struct AttrCand {
    attr: String,
    elem: String,
    kind: &'static str,
    full: (usize, usize),
    value: String,
    path: String,
}

#[derive(PartialEq)]
enum IK {
    Text,
    Open,
    Close,
    Standalone,
}

struct Item {
    kind: IK,
    a: usize,
    b: usize,
    name: String,
    attrs: Vec<AttrCand>,
}

struct Owner {
    run: usize,
    code: String,
    part: &'static str,
    rs: usize,
    re: usize,
}

struct Pending {
    start: usize,
    end: usize,
    unit: Unit,
    skel: bool,
    owner: Option<Owner>,
}

struct Frame {
    name: String,
    nth: usize,
    counts: HashMap<String, usize>,
    locked: bool,
}

struct Ex<'a> {
    s: &'a str,
    toks: Vec<Tok>,
    include_locked: bool,
    frames: Vec<Frame>,
    run: Vec<Item>,
    pend: Vec<Pending>,
    last_heading: Option<String>,
    html_tag: Option<(usize, usize)>,
    html_lang: Option<String>,
}

pub fn extract_html(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let (text, enc) = decode_input(bytes)?;
    let toks = tokenize(&text);
    let mut ex = Ex {
        s: &text,
        toks,
        include_locked: opts.include_locked,
        frames: vec![Frame { name: String::new(), nth: 0, counts: HashMap::new(), locked: false }],
        run: Vec::new(),
        pend: Vec::new(),
        last_heading: None,
        html_tag: None,
        html_lang: None,
    };
    ex.walk();
    let (units, skeleton_parts) = ex.finish();
    let source_lang = opts.source_lang.clone().or(ex.html_lang.clone());
    Ok(Document {
        format: "html".into(),
        name: None,
        source_lang,
        target_lang: opts.target_lang.clone(),
        granularity: opts.granularity,
        units,
        skeleton: Skeleton::Text { parts: skeleton_parts, encoding: enc.name().into() },
    })
}

fn is_ws_str(s: &str) -> bool {
    s.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c))
}

fn label_for(name: &str) -> &str {
    match name {
        "b" | "strong" => "bold",
        "i" | "em" | "cite" => "italic",
        "a" => "link",
        "u" | "ins" => "underline",
        "s" | "strike" | "del" => "strikethrough",
        "code" | "kbd" | "samp" | "tt" => "code",
        "br" | "wbr" => "line break",
        "img" => "image",
        "sup" => "superscript",
        "sub" => "subscript",
        "abbr" => "abbreviation",
        "mark" => "highlight",
        "input" => "input",
        other => other,
    }
}

fn kind_for(frame: &str) -> &'static str {
    match frame {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
        "li" | "dt" | "dd" => "list-item",
        "td" | "th" => "cell",
        "button" => "button",
        "label" => "label",
        "option" => "option",
        "title" => "title",
        "figcaption" | "caption" => "caption",
        "summary" => "summary",
        _ => "paragraph",
    }
}

fn is_notranslate(s: &str, attrs: &[Attr]) -> bool {
    if attr_value(s, attrs, "translate").is_some_and(|v| v.trim().eq_ignore_ascii_case("no")) {
        return true;
    }
    attr_value(s, attrs, "class").is_some_and(|v| v.split_ascii_whitespace().any(|c| c == "notranslate"))
}

impl<'a> Ex<'a> {
    fn walk(&mut self) {
        let mut ti = 0;
        while ti < self.toks.len() {
            let tok = self.toks[ti].clone();
            match tok {
                Tok::Text(a, b) => {
                    if !(self.run.is_empty() && is_ws_str(&self.s[a..b])) {
                        self.run.push(Item { kind: IK::Text, a, b, name: String::new(), attrs: vec![] });
                    }
                }
                Tok::Comment(..) | Tok::Decl(..) | Tok::Raw(..) => self.flush(),
                Tok::Start { start, end, name, attrs, self_close } => {
                    ti = self.start(ti, start, end, name, attrs, self_close);
                }
                Tok::End { start, end, name } => self.end(start, end, &name),
            }
            ti += 1;
        }
        self.flush();
    }

    fn matching_end(&self, ti: usize, name: &str) -> Option<usize> {
        let mut depth = 0;
        for j in ti + 1..self.toks.len() {
            match &self.toks[j] {
                Tok::Start { name: n, self_close, .. } if n == name && !self_close && !is_void(n) => depth += 1,
                Tok::End { name: n, .. } if n == name => {
                    if depth == 0 {
                        return Some(j);
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        None
    }

    fn bump(&mut self, name: &str) -> usize {
        let f = self.frames.last_mut().expect("root frame");
        let c = f.counts.entry(name.to_string()).or_insert(0);
        *c += 1;
        *c
    }

    fn path(&self, own: Option<(&str, usize)>) -> String {
        let mut parts: Vec<String> = self
            .frames
            .iter()
            .skip(1)
            .filter(|f| f.name != "html")
            .map(|f| format!("{}:nth-of-type({})", f.name, f.nth))
            .collect();
        if let Some((n, i)) = own {
            parts.push(format!("{n}:nth-of-type({i})"));
        }
        parts.join(" > ")
    }

    fn locked_now(&self) -> bool {
        self.frames.last().is_some_and(|f| f.locked)
    }

    fn cands(&self, name: &str, attrs: &[Attr], path: &str) -> Vec<AttrCand> {
        if matches!(name, "html" | "script" | "style" | "link" | "base" | "template") {
            return vec![];
        }
        let s = self.s;
        let low = |n: &str| attr_value(s, attrs, n).map(|v| v.trim().to_ascii_lowercase());
        let meta_ok = name == "meta"
            && (low("name").is_some_and(|v| matches!(v.as_str(), "description" | "keywords" | "twitter:title" | "twitter:description"))
                || low("property").is_some_and(|v| matches!(v.as_str(), "og:title" | "og:description" | "og:site_name" | "og:image:alt")));
        let btn = name == "input" && low("type").is_some_and(|v| matches!(v.as_str(), "button" | "submit" | "reset"));
        let mut out = Vec::new();
        for a in attrs {
            let (Some(full), Some(inner)) = (a.full, a.val) else { continue };
            let kind = match a.name.as_str() {
                "alt" if matches!(name, "img" | "area" | "input") => "alt",
                "title" => "title",
                "aria-label" | "aria-description" => "label",
                "placeholder" if matches!(name, "input" | "textarea") => "label",
                "value" if btn => "button",
                "content" if meta_ok => "meta",
                _ => continue,
            };
            let value = decode_entities(&s[inner.0..inner.1]);
            if !value.chars().any(|c| c.is_alphanumeric()) {
                continue;
            }
            out.push(AttrCand { attr: a.name.clone(), elem: name.to_string(), kind, full, value, path: path.to_string() });
        }
        out
    }

    fn attr_unit(&self, c: &AttrCand) -> Unit {
        let mut u = Unit::new("", vec![Inline::text(c.value.clone())]);
        u.context.kind = Some(c.kind.into());
        u.context.style = Some(c.elem.clone());
        u.context.key = Some(c.attr.clone());
        u.context.location = Some(format!("{}[{}]", c.path, c.attr));
        u.context.heading = self.last_heading.clone();
        u.translate = !self.locked_now();
        u.codes.insert(RAW.into(), CodeData { standalone: Some(self.s[c.full.0..c.full.1].to_string()), ..Default::default() });
        u.codes.insert(VALUE.into(), CodeData { label: Some("attr".into()), ..Default::default() });
        u
    }

    fn push_top_attrs(&mut self, cands: Vec<AttrCand>) {
        for c in cands {
            let unit = self.attr_unit(&c);
            self.pend.push(Pending { start: c.full.0, end: c.full.1, unit, skel: true, owner: None });
        }
    }

    fn start(&mut self, ti: usize, start: usize, end: usize, name: String, attrs: Vec<Attr>, self_close: bool) -> usize {
        let s = self.s;
        let notrans = is_notranslate(s, &attrs);
        if name == "html" {
            self.flush();
            if self.html_tag.is_none() {
                self.html_tag = Some((start, end));
                self.html_lang = attr_value(s, &attrs, "lang").map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
            }
            let nth = self.bump("html");
            self.frames.push(Frame { name, nth, counts: HashMap::new(), locked: false });
            return ti;
        }
        if INLINE.contains(&name.as_str()) {
            if PROTECTED.contains(&name.as_str()) || notrans {
                let (idx, bend) = if is_void(&name) || self_close {
                    (ti, end)
                } else {
                    match self.matching_end(ti, &name) {
                        Some(j) => (j, self.toks[j].range().1),
                        None => (ti, end),
                    }
                };
                self.run.push(Item { kind: IK::Standalone, a: start, b: bend, name, attrs: vec![] });
                return idx;
            }
            let nth = self.bump(&name);
            let path = self.path(Some((&name, nth)));
            let cands = self.cands(&name, &attrs, &path);
            let kind = if is_void(&name) || self_close { IK::Standalone } else { IK::Open };
            self.run.push(Item { kind, a: start, b: end, name, attrs: cands });
            return ti;
        }
        // Block-level element, or anything unknown: ends the current run.
        self.flush();
        let nth = self.bump(&name);
        let path = self.path(Some((&name, nth)));
        let locked_here = notrans || self.locked_now();
        if (notrans && !self.include_locked) || SKIP_SUBTREE.contains(&name.as_str()) {
            if name == "textarea" {
                let cands = self.cands(&name, &attrs, &path);
                self.push_top_attrs(cands);
            }
            if is_void(&name) || self_close {
                return ti;
            }
            return match self.matching_end(ti, &name) {
                Some(j) => j,
                None => self.toks.len().saturating_sub(1),
            };
        }
        self.implied_close(&name);
        let cands = self.cands(&name, &attrs, &path);
        let was_locked = self.locked_now();
        // Attributes of a locked element are locked too.
        if locked_here && !was_locked {
            self.frames.push(Frame { name: "~".into(), nth: 0, counts: HashMap::new(), locked: true });
            self.push_top_attrs(cands);
            self.frames.pop();
        } else {
            self.push_top_attrs(cands);
        }
        if is_void(&name) || self_close {
            return ti;
        }
        self.frames.push(Frame { name, nth, counts: HashMap::new(), locked: locked_here });
        ti
    }

    fn implied_close(&mut self, name: &str) {
        let pop_if = |this: &mut Self, names: &[&str]| {
            if this.frames.len() > 1 && this.frames.last().is_some_and(|f| names.contains(&f.name.as_str())) {
                this.frames.pop();
                true
            } else {
                false
            }
        };
        match name {
            "li" => {
                pop_if(self, &["p"]);
                pop_if(self, &["li"]);
            }
            "dt" | "dd" => {
                pop_if(self, &["p"]);
                pop_if(self, &["dt", "dd"]);
            }
            "td" | "th" => {
                pop_if(self, &["p"]);
                pop_if(self, &["td", "th"]);
            }
            "tr" => {
                pop_if(self, &["p"]);
                pop_if(self, &["td", "th"]);
                pop_if(self, &["tr"]);
            }
            "thead" | "tbody" | "tfoot" => {
                pop_if(self, &["td", "th"]);
                pop_if(self, &["tr"]);
                pop_if(self, &["thead", "tbody", "tfoot"]);
            }
            "option" => {
                pop_if(self, &["option"]);
            }
            "optgroup" => {
                pop_if(self, &["option"]);
                pop_if(self, &["optgroup"]);
            }
            "address" | "article" | "aside" | "blockquote" | "div" | "dl" | "fieldset" | "footer" | "form" | "h1"
            | "h2" | "h3" | "h4" | "h5" | "h6" | "header" | "hr" | "main" | "nav" | "ol" | "p" | "pre" | "section"
            | "table" | "ul" | "figure" | "details" => {
                pop_if(self, &["p"]);
            }
            _ => {}
        }
    }

    fn end(&mut self, start: usize, end: usize, name: &str) {
        if INLINE.contains(&name) {
            if !self.run.is_empty() {
                self.run.push(Item { kind: IK::Close, a: start, b: end, name: name.to_string(), attrs: vec![] });
            }
            return;
        }
        self.flush();
        if let Some(pos) = self.frames.iter().rposition(|f| f.name == name) {
            if pos > 0 {
                self.frames.truncate(pos);
            }
        }
    }

    fn flush(&mut self) {
        if self.run.is_empty() {
            return;
        }
        let items = std::mem::take(&mut self.run);
        self.make_run(items);
    }

    fn make_run(&mut self, mut items: Vec<Item>) {
        let s = self.s;
        while items.first().is_some_and(|i| i.kind == IK::Text && is_ws_str(&s[i.a..i.b])) {
            items.remove(0);
        }
        while items.last().is_some_and(|i| i.kind == IK::Text && is_ws_str(&s[i.a..i.b])) {
            items.pop();
        }
        if items.is_empty() {
            return;
        }
        if let Some(f) = items.first_mut() {
            if f.kind == IK::Text {
                let t = &s[f.a..f.b];
                f.a += t.len() - t.trim_start_matches([' ', '\t', '\n', '\r', '\u{0c}']).len();
            }
        }
        if let Some(l) = items.last_mut() {
            if l.kind == IK::Text {
                let t = &s[l.a..l.b];
                l.b -= t.len() - t.trim_end_matches([' ', '\t', '\n', '\r', '\u{0c}']).len();
            }
        }
        let has_text = items
            .iter()
            .any(|i| i.kind == IK::Text && decode_entities(&s[i.a..i.b]).chars().any(|c| c.is_alphanumeric()));
        if !has_text {
            let all: Vec<AttrCand> = items.into_iter().flat_map(|i| i.attrs).collect();
            self.push_top_attrs(all);
            return;
        }
        let start = items[0].a;
        let end = items[items.len() - 1].b;
        let frame_name = self.frames.last().map(|f| f.name.clone()).unwrap_or_default();

        // Plain-text elements (title, option with no markup): one text, no codes.
        let text_only = items.iter().all(|i| i.kind == IK::Text);
        let plain_elem = matches!(frame_name.as_str(), "title" | "option") && text_only;

        // Pair inline tags.
        let n = items.len();
        let mut pair: Vec<Option<usize>> = vec![None; n];
        let mut stack: Vec<usize> = Vec::new();
        for idx in 0..n {
            match items[idx].kind {
                IK::Open => stack.push(idx),
                IK::Close => {
                    if let Some(k) = stack.iter().rposition(|&o| items[o].name == items[idx].name) {
                        let o = stack[k];
                        pair[o] = Some(idx);
                        pair[idx] = Some(o);
                        stack.truncate(k);
                    }
                }
                _ => {}
            }
        }

        let mut inlines: Vec<Inline> = Vec::new();
        let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
        let mut code_of: Vec<Option<(String, &'static str)>> = vec![None; n];
        let mut next_id = 1;
        for idx in 0..n {
            let it = &items[idx];
            let raw = s[it.a..it.b].to_string();
            match it.kind {
                IK::Text => inlines.push(Inline::Text(decode_collapse(&raw))),
                IK::Open if pair[idx].is_some() => {
                    let id = next_id.to_string();
                    next_id += 1;
                    let close = pair[idx].map(|c| s[items[c].a..items[c].b].to_string());
                    codes.insert(id.clone(), CodeData { open: Some(raw), close, standalone: None, label: Some(label_for(&it.name).into()) });
                    inlines.push(Inline::tag(id.clone(), TagKind::Open, format!("<{}>", it.name)));
                    code_of[idx] = Some((id, "open"));
                }
                IK::Close if pair[idx].is_some() => {
                    let o = pair[idx].unwrap_or(0);
                    if let Some((id, _)) = code_of[o].clone() {
                        inlines.push(Inline::tag(id, TagKind::Close, format!("</{}>", it.name)));
                    }
                }
                _ => {
                    let id = next_id.to_string();
                    next_id += 1;
                    let display = if it.kind == IK::Close { format!("</{}>", it.name) } else { format!("<{}>", it.name) };
                    codes.insert(id.clone(), CodeData { open: None, close: None, standalone: Some(raw), label: Some(label_for(&it.name).into()) });
                    inlines.push(Inline::tag(id.clone(), TagKind::Standalone, display));
                    code_of[idx] = Some((id, "standalone"));
                }
            }
        }
        let inlines = merge_text(inlines);

        let mut unit = Unit::new("", inlines);
        unit.codes = codes;
        unit.codes.insert(RAW.into(), CodeData { standalone: Some(s[start..end].to_string()), ..Default::default() });
        if plain_elem {
            unit.codes.insert(VALUE.into(), CodeData { label: Some("text".into()), ..Default::default() });
        }
        let mut kind = kind_for(&frame_name);
        let mut style = if frame_name.is_empty() { None } else { Some(frame_name.clone()) };
        if kind == "paragraph"
            && n > 0
            && items[0].kind == IK::Open
            && items[0].name == "a"
            && pair[0] == Some(n - 1)
        {
            kind = "link";
            style = Some("a".into());
        }
        unit.context.kind = Some(kind.into());
        unit.context.style = style;
        unit.context.location = Some(self.path(None)).filter(|p| !p.is_empty());
        unit.translate = !self.locked_now();
        if kind == "heading" {
            self.last_heading = Some(plain_text(&unit.segments[0].source));
        } else {
            unit.context.heading = self.last_heading.clone();
        }
        let r = self.pend.len();
        self.pend.push(Pending { start, end, unit, skel: true, owner: None });
        for (idx, it) in items.into_iter().enumerate() {
            let Some((code, part)) = code_of[idx].clone() else {
                // Attributes on tags that were dropped cannot happen; keep them safe anyway.
                continue;
            };
            for c in it.attrs {
                let unit = self.attr_unit(&c);
                let owner = Owner { run: r, code: code.clone(), part, rs: c.full.0 - it.a, re: c.full.1 - it.a };
                self.pend.push(Pending { start: c.full.0, end: c.full.1, unit, skel: false, owner: Some(owner) });
            }
        }
    }

    fn finish(&mut self) -> (Vec<Unit>, Vec<SkelPart>) {
        let n = self.pend.len();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| (self.pend[i].start, !self.pend[i].skel));
        let mut rank = vec![0usize; n];
        for (new, &old) in order.iter().enumerate() {
            rank[old] = new;
        }
        let mut pend: Vec<Option<Pending>> = std::mem::take(&mut self.pend).into_iter().map(Some).collect();
        let mut units = Vec::with_capacity(n);
        let mut parts = Vec::new();
        let mut cursor = 0;
        let s = self.s;
        let html = self.html_tag;
        let push_raw = |parts: &mut Vec<SkelPart>, a: usize, b: usize| {
            if a >= b {
                return;
            }
            match html {
                Some((hs, he)) if a <= hs && he <= b => {
                    if a < hs {
                        parts.push(SkelPart::Raw(s[a..hs].to_string()));
                    }
                    parts.push(SkelPart::Raw(s[hs..he].to_string()));
                    if he < b {
                        parts.push(SkelPart::Raw(s[he..b].to_string()));
                    }
                }
                _ => parts.push(SkelPart::Raw(s[a..b].to_string())),
            }
        };
        for (new, &old) in order.iter().enumerate() {
            let Some(mut p) = pend[old].take() else { continue };
            let id = format!("u{}", new + 1);
            p.unit.id = id.clone();
            if let Some(seg) = p.unit.segments.first_mut() {
                seg.id = id.clone();
            }
            if let Some(o) = &p.owner {
                let owner_id = format!("u{}", rank[o.run] + 1);
                p.unit.codes.insert(
                    OWNER.into(),
                    CodeData { standalone: Some(format!("{}|{}|{}|{}|{}", owner_id, o.code, o.part, o.rs, o.re)), ..Default::default() },
                );
            }
            if p.skel && p.start >= cursor {
                push_raw(&mut parts, cursor, p.start);
                parts.push(SkelPart::Unit(new));
                cursor = p.end;
            }
            units.push(p.unit);
        }
        push_raw(&mut parts, cursor, s.len());
        (units, parts)
    }
}
