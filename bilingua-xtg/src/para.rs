//! One paragraph line: its tags, the character attributes switched on and reset again (paired codes), and the
//! unit's inline codes. A port of LocOS `_pair_runs` and `_Para` (#680).

use crate::attr::{attr_ops, bit, is_toggle, label, reset_tags, Key, OpKind, TOGGLE_MASK};
use crate::lex::Item;
use bilingua_model::{CodeData, Inline, TagKind};
use std::collections::{BTreeMap, HashMap};

/// Keys of the unit's hidden values (`Unit::hidden`).
pub const RAW: &str = "raw";
pub const GROUPS: &str = "groups";
pub const RESET_ALL: &str = "all";
pub fn resets_id(id: &str) -> String {
    format!("r{id}")
}

#[derive(Default)]
struct Run {
    tags: Vec<usize>,
    closer_tags: Vec<usize>,
    /// Openers this run ends, innermost first.
    closes: Vec<usize>,
    opener: Option<usize>,
}

struct Opener {
    run: usize,
    tags: Vec<usize>,
    keys: Vec<Key>,
    mask: u32,
    close_run: Option<usize>,
}

/// An ordered set of attribute keys.
#[derive(Default)]
struct OSet {
    v: Vec<Key>,
    m: u32,
}

impl OSet {
    fn contains(&self, k: Key) -> bool {
        self.m & bit(k) != 0
    }
    fn insert(&mut self, k: Key) {
        if !self.contains(k) {
            self.v.push(k);
            self.m |= bit(k);
        }
    }
    fn remove(&mut self, k: Key) {
        if self.contains(k) {
            self.v.retain(|&x| x != k);
            self.m &= !bit(k);
        }
    }
    fn clear(&mut self) {
        self.v.clear();
        self.m = 0;
    }
    fn retain_non_toggle(&mut self) {
        self.v.retain(|&k| !is_toggle(k));
        self.m &= !TOGGLE_MASK;
    }
    fn is_empty(&self) -> bool {
        self.v.is_empty()
    }
}

fn raw_of<'a>(text: &'a str, items: &[Item], t: usize) -> &'a str {
    &text[items[t].a..items[t].b]
}

/// Matches each character attribute change with the later run that resets it.
#[allow(clippy::needless_range_loop)]
fn pair_runs(text: &str, items: &[Item], runs: &mut [Run], openers: &mut Vec<Opener>) -> Vec<usize> {
    let mut stack: Vec<usize> = Vec::new();
    let mut pairs = Vec::new();
    for r in 0..runs.len() {
        let mask_of = |stack: &[usize], openers: &[Opener]| stack.iter().fold(0u32, |m, &o| m | openers[o].mask);
        let mut open_keys = mask_of(&stack, openers);
        let mut run_open = OSet::default();
        let mut opener_tags: Vec<usize> = Vec::new();
        let (mut close_keys, mut close_all) = (0u32, false);
        for ti in 0..runs[r].tags.len() {
            let t = runs[r].tags[ti];
            let Some(ops) = attr_ops(raw_of(text, items, t)) else { continue };
            let mut t_close = 0u32;
            let mut t_all = false;
            let mut t_open = OSet::default();
            let mut clash = false;
            for op in ops {
                let k = op.key;
                match op.kind {
                    OpKind::ResetAll => {
                        t_all = true;
                        t_open.clear();
                    }
                    OpKind::Plain => {
                        t_close |= open_keys & TOGGLE_MASK;
                        t_open.retain_non_toggle();
                    }
                    OpKind::ResetOne => {
                        if open_keys & bit(k) != 0 {
                            t_close |= bit(k);
                        }
                        t_open.remove(k);
                    }
                    OpKind::Toggle | OpKind::Set => {
                        if run_open.contains(k) || t_open.contains(k) {
                            clash = true;
                        } else if open_keys & bit(k) != 0 && close_keys & bit(k) == 0 {
                            t_close |= bit(k);
                            if op.kind == OpKind::Set {
                                t_open.insert(k);
                            }
                        } else {
                            t_open.insert(k);
                        }
                    }
                }
            }
            let closes = t_close != 0 || (t_all && !stack.is_empty());
            if clash || (closes && !t_open.is_empty()) {
                let hit = if t_all { open_keys } else { t_close };
                stack.retain(|&o| openers[o].mask & hit == 0);
                open_keys = mask_of(&stack, openers);
                continue;
            }
            if closes {
                runs[r].closer_tags.push(t);
                close_keys |= t_close;
                close_all = close_all || t_all;
            } else if !t_open.is_empty() {
                opener_tags.push(t);
                for &k in &t_open.v {
                    run_open.insert(k);
                }
            }
        }
        while !stack.is_empty() && (close_all || stack.iter().any(|&o| openers[o].mask & close_keys != 0)) {
            let top = stack.pop().unwrap_or_default();
            if close_all || openers[top].mask & !close_keys == 0 {
                openers[top].close_run = Some(r);
                runs[r].closes.push(top);
                pairs.push(top);
            }
        }
        if runs[r].closes.is_empty() {
            runs[r].closer_tags.clear();
        }
        if !run_open.is_empty() {
            openers.push(Opener { run: r, tags: opener_tags, mask: run_open.m, keys: run_open.v, close_run: None });
            runs[r].opener = Some(openers.len() - 1);
            stack.push(openers.len() - 1);
        }
    }
    for run in runs.iter_mut() {
        if let Some(o) = run.opener {
            if openers[o].close_run.is_none() {
                run.opener = None;
            }
        }
    }
    pairs
}

/// A paragraph analysed: the byte span that becomes the unit and the unit's inline codes.
pub struct Built {
    pub start: usize,
    pub end: usize,
    pub inlines: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
    /// The unit's hidden values: reset lists and attribute groups.
    pub hidden: BTreeMap<String, String>,
}

struct Out {
    inl: Vec<Inline>,
    buf: String,
}

impl Out {
    fn flush_text(&mut self) {
        if !self.buf.is_empty() {
            self.inl.push(Inline::Text(std::mem::take(&mut self.buf)));
        }
    }
    fn tag(&mut self, id: &str, kind: TagKind, display: String) {
        self.flush_text();
        self.inl.push(Inline::tag(id, kind, display));
    }
}

/// `items` holds at least one text item.
#[allow(clippy::needless_range_loop)]
pub fn build(text: &str, items: &[Item]) -> Built {
    let first = items.iter().position(|i| !i.is_tag).unwrap_or(0);
    let last = items.iter().rposition(|i| !i.is_tag).unwrap_or(0);
    let mut runs: Vec<Run> = Vec::new();
    let mut run_of = vec![usize::MAX; items.len()];
    for n in 0..items.len() {
        if !items[n].is_tag {
            continue;
        }
        if n == 0 || !items[n - 1].is_tag {
            runs.push(Run::default());
        }
        let ri = runs.len() - 1;
        runs[ri].tags.push(n);
        run_of[n] = ri;
    }
    let mut openers: Vec<Opener> = Vec::new();
    let pairs = pair_runs(text, items, &mut runs, &mut openers);
    let lead = items[0].is_tag.then(|| run_of[0]);
    let trail = items[items.len() - 1].is_tag.then(|| run_of[items.len() - 1]);
    // A pair with one end inside the text pulls its other end into the unit; a pair around the whole text
    // (both ends outside it) stays in the skeleton.
    let (mut inc_lead, mut inc_trail) = (false, false);
    loop {
        let mut changed = false;
        for &o in &pairs {
            let o_out = Some(openers[o].run) == lead && !inc_lead;
            let c_out = openers[o].close_run.is_some() && openers[o].close_run == trail && !inc_trail;
            if o_out && !c_out {
                inc_lead = true;
                changed = true;
            } else if c_out && !o_out {
                inc_trail = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let lo = match (inc_lead, lead) {
        (true, Some(l)) => runs[l].opener.and_then(|o| openers[o].tags.iter().copied().min()).unwrap_or(first),
        _ => first,
    };
    let hi = match (inc_trail, trail) {
        (true, Some(t)) => runs[t].closer_tags.iter().copied().max().unwrap_or(last),
        _ => last,
    };
    let live: Vec<bool> = openers.iter().map(|o| Some(o.run) != lead || inc_lead).collect();
    let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
    let mut out = Out { inl: Vec::new(), buf: String::new() };
    let mut ids: HashMap<usize, String> = HashMap::new();
    let mut resets: HashMap<String, String> = HashMap::new();
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut reset_all: Vec<String> = Vec::new();
    let mut pending: Vec<usize> = Vec::new();
    let mut count = 0usize;

    let flush = |pending: &mut Vec<usize>, out: &mut Out, codes: &mut BTreeMap<String, CodeData>, count: &mut usize| {
        if pending.is_empty() {
            return;
        }
        *count += 1;
        let id = count.to_string();
        let raw: String = pending.iter().map(|&t| raw_of(text, items, t)).collect();
        let lab = label(raw_of(text, items, pending[0]));
        out.tag(&id, TagKind::Standalone, raw.clone());
        codes.insert(id, CodeData { standalone: Some(raw), label: Some(lab.into()), ..Default::default() });
        pending.clear();
    };

    let mut n = lo;
    while n <= hi {
        let it = items[n];
        if !it.is_tag {
            flush(&mut pending, &mut out, &mut codes, &mut count);
            match it.lit {
                Some(c) => out.buf.push(c),
                None => out.buf.push_str(&text[it.a..it.b]),
            }
            n += 1;
            continue;
        }
        let run = &runs[run_of[n]];
        let closes: Vec<usize> = run.closes.iter().copied().filter(|&o| live[o]).collect();
        let opener = run.opener.filter(|&o| live[o]);
        let closer_tags: &[usize] = if closes.is_empty() { &[] } else { &run.closer_tags };
        let opener_tags: &[usize] = opener.map_or(&[], |o| &openers[o].tags);
        let (mut done_close, mut done_open) = (closes.is_empty(), opener.is_none());
        while n <= hi && items[n].is_tag {
            if !done_close && (closer_tags.contains(&n) || opener_tags.contains(&n)) {
                flush(&mut pending, &mut out, &mut codes, &mut count);
                let joined: String = run.closer_tags.iter().map(|&t| raw_of(text, items, t)).collect();
                let mut group = Vec::new();
                for (k, o) in closes.iter().enumerate() {
                    let Some(tid) = ids.get(o).cloned() else { continue };
                    let disp = if k + 1 == closes.len() { joined.clone() } else { resets.get(&tid).cloned().unwrap_or_default() };
                    if let Some(c) = codes.get_mut(&tid) {
                        c.close = Some(disp.clone());
                    }
                    out.tag(&tid, TagKind::Close, disp);
                    group.push(tid);
                }
                if let Some(l) = group.last() {
                    let has_all = run
                        .closer_tags
                        .iter()
                        .any(|&t| attr_ops(raw_of(text, items, t)).is_some_and(|v| v.iter().any(|o| o.kind == OpKind::ResetAll)));
                    if has_all {
                        reset_all.push(l.clone());
                    }
                    groups.push(group);
                }
                done_close = true;
            }
            if closer_tags.contains(&n) {
                n += 1;
                continue;
            }
            if opener_tags.contains(&n) {
                if !done_open {
                    flush(&mut pending, &mut out, &mut codes, &mut count);
                    count += 1;
                    let tid = count.to_string();
                    if let Some(o) = opener {
                        ids.insert(o, tid.clone());
                        let raw: String = openers[o].tags.iter().map(|&t| raw_of(text, items, t)).collect();
                        let lab = label(raw_of(text, items, openers[o].tags[0]));
                        resets.insert(tid.clone(), reset_tags(&openers[o].keys));
                        out.tag(&tid, TagKind::Open, raw.clone());
                        codes.insert(tid, CodeData { open: Some(raw), label: Some(lab.into()), ..Default::default() });
                    }
                    done_open = true;
                }
                n += 1;
                continue;
            }
            pending.push(n);
            n += 1;
        }
    }
    flush(&mut pending, &mut out, &mut codes, &mut count);
    out.flush_text();
    let mut hidden = BTreeMap::new();
    for (tid, r) in &resets {
        hidden.insert(resets_id(tid), r.clone());
    }
    if !groups.is_empty() {
        let s: Vec<String> = groups.iter().map(|g| g.join(",")).collect();
        hidden.insert(GROUPS.into(), s.join(";"));
    }
    if !reset_all.is_empty() {
        hidden.insert(RESET_ALL.into(), reset_all.join(","));
    }
    Built { start: items[lo].a, end: items[hi.max(lo)].b, inlines: out.inl, codes, hidden }
}
