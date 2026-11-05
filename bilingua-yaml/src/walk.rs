//! The one walk over the event stream that extract and merge share: which scalars are translatable values, their
//! key paths, plural maps and the Rails locale root.

use crate::resolve::resolves_str;
use crate::scan::{Ev, ScalarEv, ScanError, Style};

const PLURAL_CATEGORIES: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

pub struct WUnit {
    pub start: usize,
    pub end: usize,
    pub value: String,
    pub style: Style,
    pub flow: bool,
    pub key_line: Option<usize>,
    pub path: String,
    pub plural: Option<String>,
}

pub struct PluralMap {
    pub cats: Vec<String>,
    pub other_unit: Option<usize>,
}

pub struct RootKey {
    pub key: String,
    pub start: usize,
    pub end: usize,
    pub style: Style,
}

pub struct Walk {
    pub units: Vec<WUnit>,
    pub plural_maps: Vec<PluralMap>,
    pub rails_root: Option<RootKey>,
}

#[derive(Clone)]
enum Val {
    Other,
    Scalar(Option<usize>),
}

struct Frame {
    map: bool,
    prefix: String,
    in_key: bool,
    flow: bool,
    expecting_key: bool,
    cur_key: String,
    idx: usize,
    key_line: Option<usize>,
    keys: Vec<(String, Val)>,
    is_key: bool,
}

fn is_str(s: &ScalarEv) -> bool {
    match s.tag.as_deref() {
        None => s.style != Style::Plain || resolves_str(&s.value),
        Some("!") => resolves_str(&s.value),
        Some(t) => matches!(t, "!!str" | "!<tag:yaml.org,2002:str>" | "tag:yaml.org,2002:str"),
    }
}

fn custom_tag(t: &Option<String>) -> bool {
    !matches!(t.as_deref(), None | Some("!") | Some("!!map") | Some("!!seq") | Some("!!omap") | Some("!!pairs") | Some("!!set"))
}

pub fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

fn translatable(s: &str) -> bool {
    s.chars().any(|c| c.is_alphanumeric())
}

pub struct Limits {
    pub max_units: usize,
    pub max_unit_chars: usize,
    pub max_depth: usize,
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_units: 50_000, max_unit_chars: 100_000, max_depth: 200, max_bytes: 64 << 20 }
    }
}

impl Limits {
    pub fn unlimited() -> Limits {
        Limits { max_units: usize::MAX / 64, max_unit_chars: usize::MAX / 64, max_depth: 200, max_bytes: usize::MAX / 64 }
    }
}

fn dotted(prefix: &str, seg: &str) -> String {
    if prefix.is_empty() {
        seg.to_string()
    } else {
        format!("{prefix}.{seg}")
    }
}

pub fn walk(events: &[Ev], text: &str, limits: &Limits) -> Result<Walk, ScanError> {
    let nl: Vec<usize> = text.bytes().enumerate().filter(|(_, c)| *c == b'\n').map(|(i, _)| i).collect();
    let line_of = |pos: usize| nl.partition_point(|&p| p < pos);
    let mut units: Vec<WUnit> = Vec::new();
    let mut plural_maps = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut docs = 0usize;
    let mut root_keys: Vec<(RootKey, Option<&'static str>)> = Vec::new();
    let mut root_is_map = false;

    // (prefix for the node starting now, in_key, is_key, flow)
    fn begin(stack: &[Frame]) -> (String, bool, bool, bool) {
        match stack.last() {
            None => (String::new(), false, false, false),
            Some(top) if top.map => {
                if top.expecting_key {
                    (top.prefix.clone(), true, true, top.flow)
                } else {
                    (dotted(&top.prefix, &top.cur_key), top.in_key, false, top.flow)
                }
            }
            Some(top) => (format!("{}[{}]", top.prefix, top.idx), top.in_key, false, top.flow),
        }
    }

    fn finish(stack: &mut [Frame], docs: usize, root_keys: &mut [(RootKey, Option<&'static str>)], key: Option<String>, kind: Option<&'static str>, val: Val) {
        let depth = stack.len();
        let Some(top) = stack.last_mut() else { return };
        if top.map {
            if top.expecting_key {
                top.cur_key = key.unwrap_or_else(|| "?".into());
                top.expecting_key = false;
            } else {
                top.keys.push((top.cur_key.clone(), val));
                if docs == 1 && depth == 1 {
                    if let Some(last) = root_keys.last_mut() {
                        last.1 = kind;
                    }
                }
                top.expecting_key = true;
            }
        } else {
            top.idx += 1;
        }
    }

    for ev in events {
        match ev {
            Ev::DocStart => docs += 1,
            Ev::Scalar(sc) => {
                let (path, in_key, is_key, flow) = begin(&stack);
                if is_key {
                    if let Some(top) = stack.last_mut() {
                        top.key_line = Some(line_of(sc.end));
                    }
                    if docs == 1 && stack.len() == 1 && sc.token {
                        root_keys.push((RootKey { key: sc.value.clone(), start: sc.start, end: sc.end, style: sc.style }, None));
                    }
                    finish(&mut stack, docs, &mut root_keys, Some(sc.value.clone()), None, Val::Other);
                    continue;
                }
                let ok = sc.token && !in_key && is_str(sc) && translatable(&sc.value);
                let mut unit = None;
                if ok {
                    if units.len() + 1 > limits.max_units {
                        return Err(ScanError { msg: format!("this file has more than {} segments", limits.max_units), pos: sc.start, limit: true });
                    }
                    if sc.value.chars().count() > limits.max_unit_chars {
                        return Err(ScanError { msg: format!("a value is longer than {} characters", limits.max_unit_chars), pos: sc.start, limit: true });
                    }
                    let key_line = stack.last().filter(|t| t.map).and_then(|t| t.key_line);
                    unit = Some(units.len());
                    units.push(WUnit { start: sc.start, end: sc.end, value: sc.value.clone(), style: sc.style, flow, key_line, path, plural: None });
                }
                finish(&mut stack, docs, &mut root_keys, None, Some("scalar"), Val::Scalar(unit));
            }
            Ev::Alias(name) => {
                let (_p, _ik, is_key, _f) = begin(&stack);
                finish(&mut stack, docs, &mut root_keys, if is_key { Some(format!("*{name}")) } else { None }, Some("alias"), Val::Other);
            }
            Ev::MapStart { tag, flow: f } | Ev::SeqStart { tag, flow: f } => {
                let map = matches!(ev, Ev::MapStart { .. });
                let (path, in_key, is_key, flow) = begin(&stack);
                if stack.is_empty() && docs == 1 && map {
                    root_is_map = true;
                }
                stack.push(Frame {
                    map,
                    prefix: path,
                    in_key: in_key || is_key || custom_tag(tag),
                    flow: flow || *f,
                    expecting_key: true,
                    cur_key: String::new(),
                    idx: 0,
                    key_line: None,
                    keys: Vec::new(),
                    is_key,
                });
            }
            Ev::MapEnd | Ev::SeqEnd => {
                let Some(frame) = stack.pop() else { continue };
                if frame.map && !frame.keys.is_empty() && !frame.in_key {
                    let mut names: Vec<&str> = frame.keys.iter().map(|k| k.0.as_str()).collect();
                    names.sort_unstable();
                    names.dedup();
                    let all_scalar = frame.keys.iter().all(|(_, v)| matches!(v, Val::Scalar(_)));
                    if names.len() == frame.keys.len() && names.contains(&"other") && names.iter().all(|n| PLURAL_CATEGORIES.contains(n)) && all_scalar {
                        let mut other_unit = None;
                        for (k, v) in &frame.keys {
                            if let Val::Scalar(Some(u)) = v {
                                units[*u].plural = Some(k.clone());
                                if k == "other" {
                                    other_unit = Some(*u);
                                }
                            }
                        }
                        plural_maps.push(PluralMap { cats: frame.keys.iter().map(|k| k.0.clone()).collect(), other_unit });
                    }
                }
                if frame.is_key {
                    finish(&mut stack, docs, &mut root_keys, Some("?".into()), None, Val::Other);
                } else {
                    finish(&mut stack, docs, &mut root_keys, None, Some(if frame.map { "map" } else { "seq" }), Val::Other);
                }
            }
        }
    }

    let mut rails_root = None;
    if docs == 1 && root_is_map && root_keys.len() == 1 && root_keys[0].1 == Some("map") && crate::locale::looks_locale(&root_keys[0].0.key) {
        rails_root = Some(root_keys.pop().unwrap().0);
    }
    Ok(Walk { units, plural_maps, rails_root })
}
