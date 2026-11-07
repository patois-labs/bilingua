//! Multilingual XML: one file holds the same content in several languages as sibling nodes.
//!
//! Three layouts: `attr` (siblings with a language attribute, `xml:lang` or `lang` or a configured one),
//! `element` (siblings named by language: `<en>`, `<fr>`) and `xpath` (`source_xpath` / `target_xpath` with a
//! `{lang}` placeholder, the target rule evaluated from each source node). Units are the source-language nodes. A
//! target-language merge writes each translation into the target sibling (an empty one is opened, a missing one is
//! created after the group, with the source's indentation and start tag, `id` / `xml:id` dropped) and leaves every
//! other byte, including every other language and the root `xml:lang`, alone. Without a target language the
//! translation replaces the source node's content in place.

use crate::its::{bad, Props};
use crate::langs::{is_known_primary, lang_code_ok, norm};
use crate::plan::Planner;
use crate::tree::{Item, Kind, Tree, NONE, XML_NS};
use crate::xpath::{check_cost, Budget, Node, XPath};
use bilingua_model::{Document, Error, LimitError, LimitKind, Limits};
use std::collections::{HashMap, HashSet};

pub(crate) const US: char = '\u{1F}';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MlMode {
    Attr,
    Element,
    Xpath,
}

impl MlMode {
    fn name(self) -> &'static str {
        match self {
            MlMode::Attr => "attr",
            MlMode::Element => "element",
            MlMode::Xpath => "xpath",
        }
    }
}

/// How a multilingual file is read. `MlConfig::default()` detects the layout on its own (at least two groups, each
/// holding two or more languages); anything set explicitly drops that threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MlConfig {
    /// `None` = detect.
    pub mode: Option<MlMode>,
    /// Source language code; empty = the language present in the most groups.
    pub source: String,
    /// Language attribute (`xml:lang`, `lang`, `loc`); empty = try `xml:lang` then `lang`.
    pub lang_attr: String,
    pub source_xpath: String,
    pub target_xpath: String,
    /// Create a missing target node (default yes).
    pub create: bool,
    pub enabled: bool,
    /// Set by `from_json` and by the in-file `<?loc-multilingual?>` processing instruction.
    pub explicit: bool,
}

impl Default for MlConfig {
    fn default() -> Self {
        MlConfig { mode: None, source: String::new(), lang_attr: String::new(), source_xpath: String::new(), target_xpath: String::new(), create: true, enabled: true, explicit: false }
    }
}

fn cfg_err(msg: &str) -> Error {
    bad(format!("Multilingual XML config: {msg}"))
}

#[derive(Debug, Clone)]
enum V {
    S(String),
    B(bool),
    Other,
}

fn attr_name_ok(s: &str) -> bool {
    let part_ok = |p: &str| {
        let mut c = p.chars();
        p.len() <= 32 && c.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_') && c.all(|x| x.is_ascii_alphanumeric() || matches!(x, '_' | '.' | '-'))
    };
    match s.split_once(':') {
        Some((a, b)) => part_ok(a) && part_ok(b),
        None => part_ok(s),
    }
}

fn flag(v: &V, key: &str) -> Result<bool, Error> {
    match v {
        V::B(b) => Ok(*b),
        V::S(s) => match s.trim().to_lowercase().as_str() {
            "yes" | "true" | "1" | "on" => Ok(true),
            "no" | "false" | "0" | "off" => Ok(false),
            _ => Err(cfg_err(&format!("{key} must be yes or no."))),
        },
        V::Other => Err(cfg_err(&format!("{key} must be yes or no."))),
    }
}

impl MlConfig {
    /// Detect the layout (the same as `MlConfig::default()`).
    pub fn new() -> MlConfig {
        MlConfig::default()
    }
    /// The layout; `None` detects it. Setting anything explicitly drops the detection threshold.
    pub fn with_mode(mut self, mode: Option<MlMode>) -> MlConfig {
        self.mode = mode;
        self.explicit = true;
        self
    }
    pub fn with_source(mut self, source: impl Into<String>) -> MlConfig {
        self.source = source.into();
        self.explicit = true;
        self
    }
    pub fn with_lang_attr(mut self, attr: impl Into<String>) -> MlConfig {
        self.lang_attr = attr.into();
        self.explicit = true;
        self
    }
    pub fn with_source_xpath(mut self, xpath: impl Into<String>) -> MlConfig {
        self.source_xpath = xpath.into();
        self.explicit = true;
        self
    }
    pub fn with_target_xpath(mut self, xpath: impl Into<String>) -> MlConfig {
        self.target_xpath = xpath.into();
        self.explicit = true;
        self
    }
    /// Create a missing target node (default yes).
    pub fn with_create(mut self, create: bool) -> MlConfig {
        self.create = create;
        self
    }
    pub fn with_enabled(mut self, enabled: bool) -> MlConfig {
        self.enabled = enabled;
        self
    }
    /// Reads a JSON object: `mode`, `source`, `lang_attr`, `source_xpath`, `target_xpath` (strings), `create`,
    /// `enabled` (true / false or "yes" / "no"). Hyphens in keys are read as underscores.
    pub fn from_json(json: &str) -> Result<MlConfig, Error> {
        if json.len() > 20_000 {
            return Err(cfg_err("larger than 20 KB; refused."));
        }
        let pairs = parse_flat_json(json)?;
        MlConfig::from_pairs(pairs)
    }

    fn from_pairs(pairs: Vec<(String, V)>) -> Result<MlConfig, Error> {
        const KEYS: [&str; 7] = ["mode", "source", "lang_attr", "source_xpath", "target_xpath", "create", "enabled"];
        let mut norm_pairs: Vec<(String, V)> = Vec::new();
        for (k, v) in pairs {
            let k = k.replace('-', "_");
            norm_pairs.retain(|(x, _)| *x != k);
            norm_pairs.push((k, v));
        }
        let mut unknown: Vec<&str> = norm_pairs.iter().map(|(k, _)| k.as_str()).filter(|k| !KEYS.contains(k)).collect();
        unknown.sort_unstable();
        if !unknown.is_empty() {
            return Err(cfg_err(&format!("unknown key(s) {}.", unknown.join(", "))));
        }
        for (k, v) in &norm_pairs {
            if k != "create" && k != "enabled" && !matches!(v, V::S(_)) {
                return Err(cfg_err(&format!("{k} must be text.")));
            }
        }
        let get = |k: &str| -> String {
            match norm_pairs.iter().find(|(x, _)| x == k) {
                Some((_, V::S(s))) => s.trim().to_string(),
                _ => String::new(),
            }
        };
        let mut mode = get("mode");
        if !mode.is_empty() && !matches!(mode.as_str(), "attr" | "element" | "xpath") {
            return Err(cfg_err("mode must be attr, element or xpath."));
        }
        let source = get("source");
        if !source.is_empty() && !lang_code_ok(&source) {
            return Err(cfg_err("source is not a language code."));
        }
        let lang_attr = get("lang_attr");
        if !lang_attr.is_empty() && !attr_name_ok(&lang_attr) {
            return Err(cfg_err("lang_attr is not an attribute name."));
        }
        let (sx, tx) = (get("source_xpath"), get("target_xpath"));
        for s in [&mode, &source, &lang_attr, &sx, &tx] {
            if s.chars().any(|c| c.is_control()) {
                return Err(cfg_err("values cannot hold control characters."));
            }
        }
        if mode == "xpath" || !sx.is_empty() || !tx.is_empty() {
            if !mode.is_empty() && mode != "xpath" {
                return Err(cfg_err("XPath rules need mode xpath."));
            }
            mode = "xpath".into();
            if sx.is_empty() || tx.is_empty() || !sx.contains("{lang}") || !tx.contains("{lang}") {
                return Err(cfg_err("mode xpath needs source_xpath and target_xpath, each with a {lang} placeholder."));
            }
            if source.is_empty() {
                return Err(cfg_err("mode xpath needs a source language."));
            }
            check_cost(&sx.replace("{lang}", "xx"), "Multilingual source_xpath", false, crate::its::MAX_SELECTOR).map_err(bad)?;
            check_cost(&tx.replace("{lang}", "xx"), "Multilingual target_xpath", true, crate::its::MAX_SELECTOR).map_err(bad)?;
        }
        let fl = |k: &str, dflt: bool| -> Result<bool, Error> {
            match norm_pairs.iter().find(|(x, _)| x == k) {
                Some((_, v)) => flag(v, k),
                None => Ok(dflt),
            }
        };
        Ok(MlConfig {
            mode: match mode.as_str() {
                "attr" => Some(MlMode::Attr),
                "element" => Some(MlMode::Element),
                "xpath" => Some(MlMode::Xpath),
                _ => None,
            },
            source,
            lang_attr,
            source_xpath: sx,
            target_xpath: tx,
            create: fl("create", true)?,
            enabled: fl("enabled", true)?,
            explicit: true,
        })
    }

    pub(crate) fn encode(&self) -> String {
        [
            self.mode.map_or("", MlMode::name).to_string(),
            self.source.clone(),
            self.lang_attr.clone(),
            self.source_xpath.clone(),
            self.target_xpath.clone(),
            u8::from(self.create).to_string(),
            u8::from(self.explicit).to_string(),
        ]
        .join(&US.to_string())
    }

    pub(crate) fn decode(s: &str) -> Option<MlConfig> {
        let f: Vec<&str> = s.split(US).collect();
        if f.len() != 7 {
            return None;
        }
        Some(MlConfig {
            mode: match f[0] {
                "attr" => Some(MlMode::Attr),
                "element" => Some(MlMode::Element),
                "xpath" => Some(MlMode::Xpath),
                _ => None,
            },
            source: f[1].into(),
            lang_attr: f[2].into(),
            source_xpath: f[3].into(),
            target_xpath: f[4].into(),
            create: f[5] == "1",
            enabled: true,
            explicit: f[6] == "1",
        })
    }
}

/// Parses a flat JSON object of string / bool / other values.
fn parse_flat_json(s: &str) -> Result<Vec<(String, V)>, Error> {
    let not_json = |e: &str| bad(format!("Multilingual XML config: not valid JSON: {e}"));
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let ws = |i: &mut usize| {
        while *i < c.len() && c[*i].is_whitespace() {
            *i += 1;
        }
    };
    ws(&mut i);
    if i >= c.len() {
        return Err(not_json("empty"));
    }
    if c[i] != '{' {
        if c[i] == '[' || c[i] == '"' || c[i].is_ascii_digit() || matches!(c[i], 't' | 'f' | 'n' | '-') {
            // valid JSON but not an object, or invalid: tell them apart cheaply
            if c[i] == '[' && s.trim_end().ends_with(']') {
                return Err(bad("Multilingual XML config: must be an object."));
            }
            if c[i] == '"' || c[i].is_ascii_digit() {
                return Err(bad("Multilingual XML config: must be an object."));
            }
        }
        return Err(not_json("expected an object"));
    }
    i += 1;
    let mut out = Vec::new();
    ws(&mut i);
    if i < c.len() && c[i] == '}' {
        return Ok(out);
    }
    loop {
        ws(&mut i);
        let key = json_string(&c, &mut i).map_err(|e| not_json(&e))?;
        ws(&mut i);
        if i >= c.len() || c[i] != ':' {
            return Err(not_json("expected ':'"));
        }
        i += 1;
        ws(&mut i);
        let v = match c.get(i) {
            Some('"') => V::S(json_string(&c, &mut i).map_err(|e| not_json(&e))?),
            Some(_) => {
                let st = i;
                while i < c.len() && !matches!(c[i], ',' | '}') {
                    i += 1;
                }
                let tok: String = c[st..i].iter().collect();
                match tok.trim() {
                    "true" => V::B(true),
                    "false" => V::B(false),
                    "null" => V::Other,
                    t if t.parse::<f64>().is_ok() => V::Other,
                    _ => return Err(not_json("bad value")),
                }
            }
            None => return Err(not_json("unexpected end")),
        };
        out.push((key, v));
        ws(&mut i);
        match c.get(i) {
            Some(',') => i += 1,
            Some('}') => {
                i += 1;
                break;
            }
            _ => return Err(not_json("expected ',' or '}'")),
        }
    }
    ws(&mut i);
    if i < c.len() {
        return Err(not_json("trailing characters"));
    }
    Ok(out)
}

fn json_string(c: &[char], i: &mut usize) -> Result<String, String> {
    if c.get(*i) != Some(&'"') {
        return Err("expected a string".into());
    }
    *i += 1;
    let mut out = String::new();
    while *i < c.len() {
        let ch = c[*i];
        *i += 1;
        match ch {
            '"' => return Ok(out),
            '\\' => {
                let e = *c.get(*i).ok_or("bad escape")?;
                *i += 1;
                match e {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    '/' | '\\' | '"' => out.push(e),
                    'u' => {
                        let h: String = c.get(*i..*i + 4).ok_or("bad escape")?.iter().collect();
                        *i += 4;
                        out.push(char::from_u32(u32::from_str_radix(&h, 16).map_err(|_| "bad escape")?).unwrap_or('\u{FFFD}'));
                    }
                    _ => return Err("bad escape".into()),
                }
            }
            c => out.push(c),
        }
    }
    Err("unterminated string".into())
}

/// `<?loc-multilingual key="value" ...?>` before the root element.
fn pi_config(t: &Tree) -> Result<Option<MlConfig>, Error> {
    for it in &t.prolog {
        if it.kind != Kind::Pi {
            continue;
        }
        let raw = &t.text[it.s..it.e];
        let Some(body) = raw.strip_prefix("<?loc-multilingual") else { continue };
        if !body.starts_with(|c: char| c.is_whitespace() || c == '?') {
            continue;
        }
        let body = body.strip_suffix("?>").unwrap_or(body);
        if body.len() > 2000 {
            continue;
        }
        let mut pairs: Vec<(String, V)> = Vec::new();
        let b = body.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if !b[i].is_ascii_alphabetic() {
                i += 1;
                continue;
            }
            let ks = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') {
                i += 1;
            }
            let key = body[ks..i].to_string();
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= b.len() || b[i] != b'=' {
                continue;
            }
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= b.len() || (b[i] != b'"' && b[i] != b'\'') {
                continue;
            }
            let q = b[i];
            let vs = i + 1;
            let Some(len) = b[vs..].iter().position(|c| *c == q) else { break };
            pairs.push((key, V::S(body[vs..vs + len].to_string())));
            i = vs + len + 1;
        }
        return MlConfig::from_pairs(pairs).map(Some);
    }
    Ok(None)
}

// ---------------------------------------------------------------- layout

pub(crate) struct Layout {
    pub mode: MlMode,
    /// source code as written in the file
    pub source: String,
    pub lang_attr: String,
    pub attr_key: Option<(String, String)>,
    pub sources: Vec<u32>,
    /// source element -> members (element, language) of its group
    pub groups: HashMap<u32, Vec<(u32, String)>>,
    pub languages: Vec<String>,
}

fn attr_key(raw: &str, t: &Tree) -> Option<(String, String)> {
    if let Some(l) = raw.strip_prefix("xml:") {
        return Some((XML_NS.into(), l.into()));
    }
    if let Some((p, l)) = raw.split_once(':') {
        let ns = t.nsmap(t.root);
        return ns.iter().find(|(q, _)| q == p).map(|(_, u)| (u.clone(), l.to_string()));
    }
    Some((String::new(), raw.to_string()))
}

fn leaves(t: &Tree, cands: &[(u32, String)], include_root: bool) -> Vec<(u32, String)> {
    let is_cand: HashSet<u32> = cands.iter().map(|(e, _)| *e).collect();
    let mut inner: HashSet<u32> = HashSet::new();
    let mut seen: HashSet<u32> = HashSet::new();
    for (e, _) in cands {
        let mut cur = t.els[*e as usize].parent;
        while cur != NONE {
            if !seen.insert(cur) {
                break;
            }
            if is_cand.contains(&cur) {
                inner.insert(cur);
            }
            cur = t.els[cur as usize].parent;
        }
    }
    cands.iter().filter(|(e, _)| !inner.contains(e) && (include_root || *e != t.root)).cloned().collect()
}

struct RawLayout {
    mode: MlMode,
    lang_attr: String,
    attr_key: Option<(String, String)>,
    groups: Vec<Vec<(u32, String)>>,
}

fn layout_attr(t: &Tree, raw: &str) -> Option<RawLayout> {
    let key = attr_key(raw, t)?;
    let mut cands: Vec<(u32, String)> = Vec::new();
    for e in 0..t.els.len() as u32 {
        if let Some(v) = t.attr_value(e, &key.0, &key.1) {
            let v = v.trim().to_string();
            if lang_code_ok(&v) {
                cands.push((e, v));
            }
        }
    }
    let leaf = leaves(t, &cands, false);
    let mut order: Vec<(u32, String)> = Vec::new();
    let mut idx: HashMap<(u32, u32, String), usize> = HashMap::new();
    let mut groups: Vec<Vec<(u32, String)>> = Vec::new();
    for (e, lang) in leaf {
        let el = &t.els[e as usize];
        let k = (el.parent, el.ns, t.local(e).to_string());
        let gi = *idx.entry(k).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[gi].push((e, lang));
    }
    let _ = &mut order;
    Some(RawLayout { mode: MlMode::Attr, lang_attr: raw.to_string(), attr_key: Some(key), groups })
}

fn layout_element(t: &Tree, strict: bool) -> RawLayout {
    let mut cands: Vec<(u32, String)> = Vec::new();
    for e in 0..t.els.len() as u32 {
        let local = t.local(e);
        if lang_code_ok(local) && is_known_primary(norm(local).split('-').next().unwrap_or("")) {
            cands.push((e, local.to_string()));
        }
    }
    let leaf = leaves(t, &cands, false);
    let mut idx: HashMap<u32, usize> = HashMap::new();
    let mut parents: Vec<u32> = Vec::new();
    let mut groups: Vec<Vec<(u32, String)>> = Vec::new();
    for (e, lang) in leaf {
        let p = t.els[e as usize].parent;
        let gi = *idx.entry(p).or_insert_with(|| {
            groups.push(Vec::new());
            parents.push(p);
            groups.len() - 1
        });
        groups[gi].push((e, lang));
    }
    if strict {
        let keep: Vec<bool> = groups
            .iter()
            .zip(&parents)
            .map(|(g, &p)| {
                if p == NONE {
                    return false;
                }
                let items = &t.els[p as usize].items;
                let n_el = items.iter().filter(|i| i.kind == Kind::Elem).count();
                g.len() == n_el && !items.iter().any(|i| matches!(i.kind, Kind::Text | Kind::Cdata) && !t.text[i.s..i.e].chars().all(bilingua_model::py_isspace_char))
            })
            .collect();
        groups = groups.into_iter().zip(keep).filter(|(_, k)| *k).map(|(g, _)| g).collect();
    }
    RawLayout { mode: MlMode::Element, lang_attr: String::new(), attr_key: None, groups }
}

fn finish(lay: RawLayout, source: &str, threshold: usize) -> Option<Layout> {
    let multi = lay.groups.iter().filter(|g| g.iter().map(|(_, l)| norm(l)).collect::<HashSet<_>>().len() >= 2).count();
    if multi < threshold {
        return None;
    }
    let mut source = source.to_string();
    if source.is_empty() {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut first: HashMap<String, usize> = HashMap::new();
        let mut seen = 0usize;
        for g in &lay.groups {
            let mut done: Vec<String> = Vec::new();
            for (_, l) in g {
                let n = norm(l);
                if done.contains(&n) {
                    continue;
                }
                done.push(n.clone());
                *counts.entry(n.clone()).or_insert(0) += 1;
                first.entry(n).or_insert(seen);
                seen += 1;
            }
        }
        source = counts.keys().min_by_key(|c| (std::cmp::Reverse(counts[*c]), first[*c]))?.clone();
    }
    let src = norm(&source);
    let mut sources: Vec<u32> = Vec::new();
    let mut groups: HashMap<u32, Vec<(u32, String)>> = HashMap::new();
    let mut languages: Vec<(String, String)> = Vec::new();
    let mut written: Option<String> = None;
    for g in &lay.groups {
        for (_, l) in g {
            if !languages.iter().any(|(n, _)| *n == norm(l)) {
                languages.push((norm(l), l.clone()));
            }
        }
        for (e, l) in g {
            if norm(l) == src {
                sources.push(*e);
                groups.insert(*e, g.clone());
                if written.is_none() {
                    written = Some(l.clone());
                }
            }
        }
    }
    let written = written?;
    Some(Layout {
        mode: lay.mode,
        source: written,
        lang_attr: lay.lang_attr,
        attr_key: lay.attr_key,
        sources,
        groups,
        languages: languages.into_iter().map(|(_, l)| l).collect(),
    })
}

fn layout_xpath(t: &Tree, cfg: &MlConfig, budget: &mut Budget) -> Result<Layout, Error> {
    let ns = t.nsmap(t.root);
    let expr = cfg.source_xpath.replace("{lang}", &cfg.source);
    let xp = XPath::compile(&expr, &ns).map_err(|e| bad(format!("Multilingual source_xpath is not valid XPath: {e}")))?;
    // the target rule must compile now, not first at merge time
    XPath::compile(&cfg.target_xpath.replace("{lang}", &cfg.source), &ns).map_err(|e| bad(format!("Multilingual target_xpath is not valid XPath: {e}")))?;
    let hits = xp.select(t, Node::Doc, budget).map_err(|e| bad(format!("Multilingual source_xpath failed: {e}")))?;
    let cands: Vec<(u32, String)> = hits
        .into_iter()
        .filter_map(|h| if let Node::El(e) = h { (e != t.root).then(|| (e, cfg.source.clone())) } else { None })
        .collect();
    let mut seen = HashSet::new();
    let cands: Vec<(u32, String)> = cands.into_iter().filter(|(e, _)| seen.insert(*e)).collect();
    let mut sources: Vec<u32> = leaves(t, &cands, true).into_iter().map(|(e, _)| e).collect();
    sources.sort_unstable_by_key(|e| t.els[*e as usize].s0);
    let key = if cfg.lang_attr.is_empty() { None } else { attr_key(&cfg.lang_attr, t) };
    Ok(Layout {
        mode: MlMode::Xpath,
        source: cfg.source.clone(),
        lang_attr: cfg.lang_attr.clone(),
        attr_key: key,
        groups: sources.iter().map(|e| (*e, vec![(*e, cfg.source.clone())])).collect(),
        sources,
        languages: vec![cfg.source.clone()],
    })
}

fn detect(t: &Tree, cfg: &MlConfig, budget: &mut Budget) -> Result<Option<Layout>, Error> {
    if !cfg.enabled {
        return Ok(None);
    }
    let threshold = if cfg.explicit { 0 } else { 2 };
    if cfg.mode == Some(MlMode::Xpath) {
        let lay = layout_xpath(t, cfg, budget)?;
        return Ok((!lay.sources.is_empty()).then_some(lay));
    }
    if matches!(cfg.mode, None | Some(MlMode::Attr)) {
        let raws: Vec<&str> = if cfg.lang_attr.is_empty() { vec!["xml:lang", "lang"] } else { vec![cfg.lang_attr.as_str()] };
        for raw in raws {
            if let Some(l) = layout_attr(t, raw) {
                if let Some(done) = finish(l, &cfg.source, threshold) {
                    return Ok(Some(done));
                }
            }
        }
    }
    if matches!(cfg.mode, None | Some(MlMode::Element)) {
        let l = layout_element(t, !cfg.explicit);
        if let Some(done) = finish(l, &cfg.source, threshold) {
            return Ok(Some(done));
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------- plan

pub(crate) struct MlPlan {
    pub layout: Layout,
    pub planned: crate::plan::Planned,
    pub cfg: MlConfig,
    /// source element of each unit
    pub unit_src: Vec<u32>,
}

fn check_target_budget(t: &Tree, lay: &Layout, limits: &Limits) -> Result<(), Error> {
    if lay.mode != MlMode::Xpath {
        return Ok(());
    }
    let mut cost = 0usize;
    for &e in &lay.sources {
        let p = t.els[e as usize].parent;
        if p != NONE {
            cost += t.els[p as usize].items.iter().filter(|i| matches!(i.kind, Kind::Elem | Kind::Comment | Kind::Pi)).count();
        }
    }
    if cost > 200usize.saturating_mul(limits.max_units) {
        return Err(Error::Limit(LimitError::new(LimitKind::XmlElements, "Multilingual XML: the target rule would visit too many nodes; split the file.")));
    }
    Ok(())
}

/// The multilingual plan, or `None` when the file is not (configured as) multilingual. `config` is the filter's
/// own; an in-file processing instruction applies when it has none.
pub(crate) fn plan(t: &Tree, config: Option<&MlConfig>, limits: &Limits, check_budget: bool, use_pi: bool) -> Result<Option<MlPlan>, Error> {
    let mut cfg = config.cloned().unwrap_or_default();
    if use_pi && !cfg.explicit {
        if let Some(pi) = pi_config(t)? {
            cfg = pi;
        }
    }
    if !cfg.enabled {
        return Ok(None);
    }
    let mut budget = Budget::new(t, 400u64 * (t.els.len() as u64 + 1000));
    let Some(layout) = detect(t, &cfg, &mut budget)? else {
        if cfg.explicit {
            return Err(bad("Multilingual XML: no source-language nodes found with the configured rule."));
        }
        return Ok(None);
    };
    if check_budget {
        check_target_budget(t, &layout, limits)?;
    }
    // Refused here, not first at merge: a file that extracts must merge. A missing target is written as a copy of
    // the source's start tag, which needs the language attribute or a language element name to change.
    if cfg.create && layout.sources.iter().any(|&e| !can_host_target(t, &layout, e)) {
        return Err(bad(NO_TARGET_NODE));
    }
    let mut props = Props::plain(t.els.len());
    for &e in &layout.sources {
        let el = &t.els[e as usize];
        // every child element is inline markup
        let mut stack: Vec<u32> = t.els[e as usize].items.iter().filter(|i| i.kind == Kind::Elem).map(|i| i.el).collect();
        while let Some(x) = stack.pop() {
            props.wt[x as usize] = 1;
            stack.extend(t.els[x as usize].items.iter().filter(|i| i.kind == Kind::Elem).map(|i| i.el));
        }
        let _ = el;
        // nearest xml:space on the element or an ancestor
        let mut cur = e;
        while cur != NONE {
            if let Some(v) = t.attr_value(cur, XML_NS, "space") {
                props.pre[e as usize] = v == "preserve";
                break;
            }
            cur = t.els[cur as usize].parent;
        }
    }
    let mut pl = Planner::new(t, &props, limits);
    pl.plan_attrs = false;
    let mut unit_src = Vec::new();
    for &e in &layout.sources {
        let before = pl.units.len();
        let n = t.els[e as usize].items.len();
        if !t.els[e as usize].empty {
            pl.flush(e, 0, n, 0)?;
            pl.drain()?;
        }
        if pl.units.len() > before {
            unit_src.push(e);
        }
    }
    let resolved = MlConfig { mode: Some(layout.mode), source: layout.source.clone(), lang_attr: layout.lang_attr.clone(), ..cfg };
    Ok(Some(MlPlan { layout, planned: pl.finish(), cfg: resolved, unit_src }))
}

// ---------------------------------------------------------------- merge

fn styled(target: &str, source_written: &str) -> String {
    let mut out = if source_written.contains('_') { target.replace('-', "_") } else { target.to_string() };
    if source_written.chars().count() > 3 {
        let has_up = source_written.chars().any(char::is_uppercase);
        let has_lo = source_written.chars().any(char::is_lowercase);
        if has_up && !has_lo {
            out = out.to_uppercase();
        } else if has_lo && !has_up {
            out = out.to_lowercase();
        }
    }
    out
}

fn target_code(lay: &Layout, target: &str) -> String {
    let want = norm(target);
    let primary = want.split('-').next().unwrap_or("").to_string();
    let src = norm(&lay.source);
    let langs: Vec<(String, &String)> = lay.languages.iter().map(|l| (norm(l), l)).collect();
    if let Some((_, l)) = langs.iter().find(|(n, _)| *n == want) {
        return (*l).clone();
    }
    if primary != src {
        if let Some((_, l)) = langs.iter().find(|(n, _)| *n == primary) {
            return (*l).clone();
        }
    }
    styled(target, &lay.source)
}

fn find_target(t: &Tree, lay: &Layout, cfg: &MlConfig, el: u32, code: &str, budget: &mut Budget) -> Result<Vec<u32>, Error> {
    let want = norm(code);
    if lay.mode == MlMode::Xpath {
        let ns = t.nsmap(t.root);
        let primary = want.split('-').next().unwrap_or("").to_string();
        let mut tries = vec![code.to_string()];
        if primary != norm(&lay.source) && primary != code {
            tries.push(primary);
        }
        for c in tries {
            if !lang_code_ok(&c) {
                continue;
            }
            let expr = cfg.target_xpath.replace("{lang}", &c);
            let xp = XPath::compile(&expr, &ns).map_err(|e| bad(format!("Multilingual target_xpath failed: {e}")))?;
            let hits = xp.select(t, Node::El(el), budget).map_err(|e| bad(format!("Multilingual target_xpath failed: {e}")))?;
            let found: Vec<u32> = hits
                .into_iter()
                .filter_map(|h| if let Node::El(x) = h { Some(x) } else { None })
                .filter(|&x| x != el && t.els[x as usize].parent == t.els[el as usize].parent)
                .collect();
            if !found.is_empty() {
                return Ok(found);
            }
        }
        return Ok(Vec::new());
    }
    Ok(lay.groups.get(&el).map(|g| g.iter().filter(|(m, l)| *m != el && norm(l) == want).map(|(m, _)| *m).collect()).unwrap_or_default())
}

fn indent_before(t: &Tree, el: u32) -> String {
    let p = t.els[el as usize].parent;
    if p == NONE {
        return String::new();
    }
    let items = &t.els[p as usize].items;
    let Some(pos) = items.iter().position(|i| i.kind == Kind::Elem && i.el == el) else { return String::new() };
    if pos == 0 {
        return String::new();
    }
    let prev: &Item = &items[pos - 1];
    if prev.kind != Kind::Text {
        return String::new();
    }
    let raw = &t.text[prev.s..prev.e];
    if !raw.chars().all(bilingua_model::py_isspace_char) {
        return String::new();
    }
    match raw.rfind('\n') {
        Some(mut k) => {
            if k > 0 && raw.as_bytes()[k - 1] == b'\r' {
                k -= 1;
            }
            raw[k..].to_string()
        }
        None => raw.to_string(),
    }
}

const NO_TARGET_NODE: &str = "Multilingual XML: the target node is missing and cannot be created (the source node carries neither the language attribute nor a language element name); set lang_attr, or create to no.";

/// Whether a target node can be made from the source element `el`: the same test `new_node` applies.
fn can_host_target(t: &Tree, lay: &Layout, el: u32) -> bool {
    let has_attr = lay.attr_key.as_ref().and_then(|(ns, l)| t.find_attr(el, ns, l)).is_some();
    (has_attr && !lay.lang_attr.is_empty()) || norm(t.local(el)) == norm(&lay.source)
}

fn new_node(t: &Tree, lay: &Layout, el: u32, code: &str, body: &str) -> Result<String, Error> {
    let e = &t.els[el as usize];
    let mut stag = t.text[e.s0..e.s1].to_string();
    let mut etag = t.text[e.e0..e.e1].to_string();
    let has_attr = lay.attr_key.as_ref().and_then(|(ns, l)| t.find_attr(el, ns, l)).copied();
    if let Some(a) = has_attr.filter(|_| !lay.lang_attr.is_empty()) {
        let (vs, ve) = (a.val_s - e.s0, a.val_e - e.s0);
        stag = format!("{}{}{}", &stag[..vs], crate::skel::esc_attr(code, a.quote as char), &stag[ve..]);
    } else if norm(t.local(el)) == norm(&lay.source) {
        let q = t.qname(el);
        let local = t.local(el);
        let prefix = &q[..q.len() - local.len()];
        stag = format!("<{prefix}{code}{}", &stag[1 + q.len()..]);
        etag = format!("</{prefix}{code}>");
    } else {
        return Err(bad(NO_TARGET_NODE));
    }
    let stag = strip_ids(&stag);
    Ok(format!("{stag}{body}{etag}"))
}

/// Removes ` id="..."` and ` xml:id="..."` attributes from a start tag.
fn strip_ids(stag: &str) -> String {
    let b = stag.as_bytes();
    let mut i = 1;
    while i < b.len() && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>') {
        i += 1;
    }
    let mut out = stag[..i].to_string();
    loop {
        let ws_start = i;
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i >= b.len() || b[i] == b'/' || b[i] == b'>' {
            out.push_str(&stag[ws_start..]);
            return out;
        }
        let ns = i;
        while i < b.len() && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'=') {
            i += 1;
        }
        let name = &stag[ns..i];
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'=') {
            i += 1;
        }
        let Some(&q) = b.get(i) else { out.push_str(&stag[ws_start..]); return out };
        let vs = i + 1;
        let len = b[vs..].iter().position(|c| *c == q).unwrap_or(0);
        i = vs + len + 1;
        if name == "id" || name == "xml:id" {
            // drop the white space before it too: keep only what preceded the last white-space char
            let keep = stag[ws_start..ns].len().saturating_sub(1);
            out.push_str(&stag[ws_start..ws_start + keep]);
        } else {
            out.push_str(&stag[ws_start..i]);
        }
    }
}

/// Merges translations into a multilingual file. `texts[k]` is unit k's rendered body (None = untranslated).
pub(crate) fn merge(doc: &Document, text: &str, cfg_blob: &str, target: Option<&str>, limits: &Limits) -> Result<String, Error> {
    let cfg = MlConfig::decode(cfg_blob).ok_or_else(|| Error::Write("bad multilingual skeleton".into()))?;
    let t = Tree::parse(text, limits)?;
    let mp = plan(&t, Some(&cfg), limits, false, false)?.ok_or_else(|| Error::Write("the multilingual layout no longer matches the file".into()))?;
    let lay = &mp.layout;
    if mp.unit_src.len() != doc.units.len() {
        return Err(Error::Write("the multilingual skeleton does not match this document's units".into()));
    }
    let code = match target {
        Some(tg) => {
            let c = target_code(lay, tg);
            if !lang_code_ok(&c) {
                return Err(bad(format!("Multilingual XML: {tg:?} is not a language code.")));
            }
            Some(c)
        }
        None => None,
    };
    let mut edits: Vec<(usize, usize, usize, String)> = Vec::new();
    let mut used: HashSet<u32> = HashSet::new();
    let sources: HashSet<u32> = lay.sources.iter().copied().collect();
    let mut budget = Budget::new(&t, 400u64 * (t.els.len() as u64 + 1000));
    for (k, &el) in mp.unit_src.iter().enumerate() {
        let u = &doc.units[k];
        if !crate::skel::translated(u) {
            continue;
        }
        let nd = &t.els[el as usize];
        let mut m = crate::skel::Merger::new(doc, None);
        m.render_translated(k)?;
        let body = m.out;
        let Some(code) = &code else {
            if u.output() == u.source() {
                continue;
            }
            edits.push((nd.s1, nd.e0, k, body));
            continue;
        };
        let hits: Vec<u32> = find_target(&t, lay, &mp.cfg, el, code, &mut budget)?.into_iter().filter(|h| !used.contains(h) && !sources.contains(h)).collect();
        if let Some(&tel) = hits.first() {
            used.insert(tel);
            let tn = &t.els[tel as usize];
            if tn.empty {
                let stag = &t.text[tn.s0..tn.s1];
                let stag = format!("{}>", stag[..stag.len() - 2].trim_end());
                edits.push((tn.s0, tn.e1, k, format!("{stag}{body}</{}>", t.qname(tel))));
            } else {
                edits.push((tn.s1, tn.e0, k, body));
            }
            continue;
        }
        if !mp.cfg.create {
            continue;
        }
        let members: Vec<u32> = lay.groups.get(&el).map(|g| g.iter().map(|(m, _)| *m).collect()).unwrap_or_else(|| vec![el]);
        let after = members.iter().map(|m| t.els[*m as usize].e1).max().unwrap_or(nd.e1);
        let sep = indent_before(&t, el);
        edits.push((after, after, k, format!("{sep}{}", new_node(&t, lay, el, code, &body)?)));
    }
    edits.sort_by_key(|e| (e.0, e.2));
    let mut out = String::with_capacity(text.len() + 1024);
    let mut pos = 0usize;
    for (a, b, _, rep) in edits {
        if a < pos {
            return Err(bad("Multilingual XML: overlapping edits (internal error)."));
        }
        out.push_str(&text[pos..a]);
        out.push_str(&rep);
        pos = b;
    }
    out.push_str(&text[pos..]);
    Ok(out)
}

