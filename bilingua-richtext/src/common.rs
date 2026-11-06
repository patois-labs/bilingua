//! What the four filters share: the plan of a unit, inline tags built from a walk, the check that a translation keeps
//! every tag of its source exactly once, and the document and merge scaffolding.

use crate::json::{self, Value};
use bilingua_model::*;
use std::collections::BTreeMap;

/// One step of a path from the root of the JSON tree.
#[derive(Debug, Clone)]
pub(crate) enum Step {
    K(String),
    I(usize),
}

pub(crate) fn k(s: &str) -> Step {
    Step::K(s.to_string())
}

/// What a tag stands for in the source tree (copied back on merge, never rebuilt from text).
#[derive(Debug, Clone)]
pub(crate) enum Token {
    /// A marked text node or span: the node without its text; the pair holds the text.
    M(Value),
    /// A wrapper node (a link): the node without its content; the pair holds the children.
    W(Value),
    /// A node kept as it is.
    X(Value),
}

impl Token {
    fn paired(&self) -> bool {
        !matches!(self, Token::X(_))
    }
}

/// One translatable unit of a tree.
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub path: Vec<Step>,
    /// A text leaf of the ADF leaf walk (`adf.units=leaf`): its translation is written to the leaf's `text`.
    pub leaf: bool,
    pub kind: String,
    pub extra: Vec<(String, String)>,
    pub src: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
    pub tokens: BTreeMap<u32, Token>,
}

impl Plan {
    pub fn pointer(&self) -> String {
        let mut s = String::new();
        for st in &self.path {
            s.push('/');
            match st {
                Step::K(k) => s.push_str(&k.replace('~', "~0").replace('/', "~1")),
                Step::I(i) => s.push_str(&i.to_string()),
            }
        }
        s
    }
}

/// Builds the inlines of one unit while a walk goes over its nodes.
#[derive(Default)]
pub(crate) struct Builder {
    pub inl: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
    pub tokens: BTreeMap<u32, Token>,
    n: u32,
    /// Some text outside the tags holds a letter (LocOS `_translatable`).
    pub alpha: bool,
}

impl Builder {
    pub fn text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        if py_has_alpha(s) {
            self.alpha = true;
        }
        if let Some(Inline::Text(t)) = self.inl.last_mut() {
            t.push_str(s);
        } else {
            self.inl.push(Inline::Text(s.to_string()));
        }
    }

    pub fn open(&mut self, label: &str, tok: Token) -> u32 {
        self.n += 1;
        let id = self.n.to_string();
        self.inl.push(Inline::tag(id.clone(), TagKind::Open, format!("<{label}>")));
        self.codes.insert(id, CodeData::paired(format!("<{label}>"), format!("</{label}>")).with_label(label));
        self.tokens.insert(self.n, tok);
        self.n
    }

    pub fn close(&mut self, n: u32, label: &str) {
        self.inl.push(Inline::tag(n.to_string(), TagKind::Close, format!("</{label}>")));
    }

    pub fn point(&mut self, label: &str, tok: Token) -> u32 {
        self.n += 1;
        let id = self.n.to_string();
        self.inl.push(Inline::tag(id.clone(), TagKind::Standalone, format!("<{label}/>")));
        self.codes.insert(id, CodeData::standalone(format!("<{label}/>")).with_label(label));
        self.tokens.insert(self.n, tok);
        self.n
    }

    pub fn finish(self, path: Vec<Step>, kind: &str, extra: Vec<(String, String)>) -> Plan {
        Plan { path, leaf: false, kind: kind.to_string(), extra, src: self.inl, codes: self.codes, tokens: self.tokens }
    }
}

/// A translation as events.
pub(crate) enum Ev {
    T(String),
    O(u32),
    C(u32),
    P(u32),
}

/// The translation's inlines as events, or `None` when its tags are not exactly the source's: every tag of the
/// source once (a pair as an open and a close), nothing else. Order and nesting are checked by the rebuild.
pub(crate) fn events(inl: &[Inline], tokens: &BTreeMap<u32, Token>) -> Option<Vec<Ev>> {
    let mut evs: Vec<Ev> = Vec::new();
    let mut open: BTreeMap<u32, u32> = BTreeMap::new();
    let mut close: BTreeMap<u32, u32> = BTreeMap::new();
    let mut point: BTreeMap<u32, u32> = BTreeMap::new();
    for i in inl {
        match i {
            Inline::Text(t) => match evs.last_mut() {
                Some(Ev::T(s)) => s.push_str(t),
                _ => evs.push(Ev::T(t.clone())),
            },
            Inline::Tag { id, kind, .. } => {
                let n: u32 = id.parse().ok()?;
                let tok = tokens.get(&n)?;
                match kind {
                    TagKind::Open if tok.paired() => {
                        *open.entry(n).or_default() += 1;
                        evs.push(Ev::O(n));
                    }
                    TagKind::Close if tok.paired() => {
                        *close.entry(n).or_default() += 1;
                        evs.push(Ev::C(n));
                    }
                    TagKind::Standalone if !tok.paired() => {
                        *point.entry(n).or_default() += 1;
                        evs.push(Ev::P(n));
                    }
                    _ => return None,
                }
            }
            #[allow(unreachable_patterns)]
            _ => return None,
        }
    }
    for (n, tok) in tokens {
        let ok = if tok.paired() { open.get(n) == Some(&1) && close.get(n) == Some(&1) } else { point.get(n) == Some(&1) };
        if !ok {
            return None;
        }
    }
    Some(evs)
}

/// True when the translation is only blank text (no tags): LocOS holds such a field.
pub(crate) fn blank(inl: &[Inline]) -> bool {
    inl.iter().all(|i| matches!(i, Inline::Text(t) if py_strip(t).is_empty()))
}

pub(crate) fn node_mut<'a>(root: &'a mut Value, path: &[Step]) -> Option<&'a mut Value> {
    let mut cur = root;
    for st in path {
        cur = match (st, cur) {
            (Step::K(key), v @ Value::Obj(_)) => v.get_mut(key)?,
            (Step::I(i), Value::Arr(a)) => a.get_mut(*i)?,
            _ => return None,
        };
    }
    Some(cur)
}

pub(crate) fn unwritable(index: usize, reason: impl Into<String>) -> Error {
    Error::Unwritable { unit: Some(format!("u{}", index + 1)), reason: reason.into() }
}

/// A label for a list of marks: their types, or the keys of a mark object.
pub(crate) fn label_of(parts: Vec<String>, fallback: &str) -> String {
    if parts.is_empty() {
        fallback.to_string()
    } else {
        parts.join(" ")
    }
}

pub(crate) type PlanFn = fn(&Value, bool) -> Vec<Plan>;
pub(crate) type ApplyFn = fn(&mut Value, &[Plan], &[Option<Vec<Inline>>]) -> Result<(), Error>;

fn reparse(id: &str, e: Error) -> Error {
    match e {
        Error::Parse { msg, .. } => Error::Parse { format: id.into(), msg },
        other => other,
    }
}

pub(crate) fn extract_doc(id: &'static str, what: &str, bytes: &[u8], opts: &ExtractOptions, mode: &str, shape: fn(&Value) -> bool, plan: PlanFn) -> Result<Document, Error> {
    guard::check_input(bytes, &opts.limits())?;
    let mut dec = enc::decode_text(bytes, &enc::DecodeOptions::new(opts))?;
    let text = std::mem::take(&mut dec.text);
    let parsed = json::parse(&text).map_err(|e| reparse(id, e))?;
    if !shape(&parsed.value) {
        return Err(Error::Parse { format: id.into(), msg: format!("this JSON is not {what}") });
    }
    let leaf = mode == "leaf";
    let plans = plan(&parsed.value, leaf);
    guard::check_unit_count(plans.len(), &opts.limits())?;
    let locos = opts.is_profile(profile::LOCOS);
    let mut units = Vec::with_capacity(plans.len());
    for (n, p) in plans.into_iter().enumerate() {
        let mut u = Unit::new(format!("u{}", n + 1), p.src.clone());
        u.codes = p.codes.clone();
        u.context.kind = Some(p.kind.clone());
        u.context.key = Some(p.pointer());
        for (a, b) in &p.extra {
            u.context.extra.insert(a.clone(), b.clone());
        }
        if locos {
            u.context.extra.insert(profile::P_INDEX.into(), n.to_string());
        }
        units.push(u);
    }
    let skeleton = Skeleton::text(vec![SkelPart::Raw(mode.to_string()), SkelPart::Raw(text)], dec.encoding.clone(), parser_info!());
    guard::finish(Document::new(id, units, skeleton).with_options(opts), std::mem::take(&mut dec.warnings), &opts.limits())
}

pub(crate) fn merge_doc(id: &'static str, doc: &Document, opts: &MergeOptions, plan: PlanFn, apply: ApplyFn) -> Result<Vec<u8>, Error> {
    check_parser(doc, &parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write(format!("{id} needs a text skeleton")));
    };
    let (Some(SkelPart::Raw(mode)), Some(SkelPart::Raw(text))) = (parts.first(), parts.get(1)) else {
        return Err(Error::Write(format!("{id} lost its original text")));
    };
    let touched = doc.units.iter().any(Unit::is_translated);
    if !touched {
        return enc::encode_text_with(text, encoding, opts);
    }
    let parsed = json::parse(text).map_err(|e| reparse(id, e))?;
    let mut root = parsed.value.clone();
    let plans = plan(&root, mode == "leaf");
    if plans.len() != doc.units.len() {
        return Err(Error::Write(format!("{id}: the document has {} units but the original text has {}", doc.units.len(), plans.len())));
    }
    let outs: Vec<Option<Vec<Inline>>> = plans
        .iter()
        .zip(&doc.units)
        .map(|(p, u)| {
            if !u.is_translated() {
                return None;
            }
            let o = u.output();
            (o != p.src).then_some(o)
        })
        .collect();
    if outs.iter().all(Option::is_none) {
        return enc::encode_text_with(text, encoding, opts);
    }
    // LocOS's walkers refuse a translation with a stray placeholder bracket (a token its adapter could not read
    // back); Bilingua's tags are structural, so outside the profile the brackets are plain characters.
    if opts.is_profile(profile::LOCOS) && id != "notion" && mode != "leaf" {
        for (i, o) in outs.iter().enumerate() {
            if o.as_ref().is_some_and(|o| o.iter().any(|x| matches!(x, Inline::Text(t) if t.contains(['\u{27E6}', '\u{27E7}'])))) {
                return Err(unwritable(i, "the translation holds a placeholder bracket that is not a token"));
            }
        }
    }
    apply(&mut root, &plans, &outs)?;
    let body = json::splice(text, &parsed.value, &root, &parsed.span, &parsed.style);
    enc::encode_text_with(&body, encoding, opts)
}

pub(crate) fn mode_option(opts: &std::collections::BTreeMap<String, String>) -> &'static str {
    match opts.get("adf.units").map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("leaf") => "leaf",
        _ => "inline",
    }
}
