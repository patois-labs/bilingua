//! The planner: decides, element by element, which runs of content are units and which parts of the file are
//! left alone. Extraction and the multilingual merge run the same planner, so unit numbers always line up.

use crate::its::Props;
use crate::tree::{decode_refs, is_xml_ws, Item, Kind, Tree};
use bilingua_model::{guard, Error, Limits};

/// Deepest element nesting the engine accepts, whatever `Limits::max_xml_depth` says. The planner and the
/// skeleton emitter and the merge walk the tree with explicit stacks (no recursion per level, so any depth is safe on a
/// 1 MB debug stack, wasm's default); the cap stays as a bound on the work a caller who raises the limit can ask for.
pub(crate) const HARD_DEPTH: usize = 1024;

fn too_deep() -> Error {
    Error::Limit(bilingua_model::LimitError::new(bilingua_model::LimitKind::XmlDepth, "This file nests XML elements too deeply. It was not processed."))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Piece {
    Text(String),
    Open(usize),
    Close(usize),
    Point(usize),
}

#[derive(Clone, Debug)]
pub(crate) enum Ref {
    /// An inline element shown as an open and a close tag.
    Pair(u32),
    /// A protected element (not inline, not translatable, nested): one tag, its content planned separately.
    PointEl(u32),
    /// A comment or processing instruction inside running text.
    PointItem(usize, usize),
    /// An entity reference the document does not define (`&product;`), kept as written.
    Entity(String),
}

#[derive(Clone, Debug)]
pub(crate) struct PUnit {
    pub key: (usize, i32),
    /// An attribute value (otherwise a run of element content).
    pub attr: Option<AttrUnit>,
    pub pieces: Vec<Piece>,
    /// ref number n is `refs[n - 1]`
    pub refs: Vec<Ref>,
    pub note: u32,
    pub owner: u32,
    pub lo: usize,
    pub hi: usize,
    pub lead: String,
    pub trail: String,
    pub cdata: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct AttrUnit {
    pub el: u32,
    pub attr: u32,
    pub local: String,
}

#[derive(Clone, Debug)]
pub(crate) enum Entry {
    Raw(Item),
    /// items `lo..hi` of the owner; the unit (final index) if the run is one
    Run { lo: usize, hi: usize, unit: Option<usize> },
}

pub(crate) const ROLE_CONTAINER: u8 = 1;
pub(crate) const ROLE_INLINE: u8 = 2;

pub(crate) struct Planned {
    pub units: Vec<PUnit>,
    pub plans: Vec<Vec<Entry>>,
    pub role: Vec<u8>,
}

pub(crate) struct Planner<'a, 'b> {
    pub t: &'b Tree<'a>,
    pub props: &'b Props,
    pub limits: &'b Limits,
    pub units: Vec<PUnit>,
    pub plans: Vec<Vec<Entry>>,
    pub role: Vec<u8>,
    /// Plans entries refer to units by creation number until `finish` renumbers them.
    pub plan_attrs: bool,
    /// Elements still to plan as containers, with their depth (a work list instead of recursion).
    pending: Vec<(u32, usize)>,
}

struct Flags {
    cdata: usize,
    other: usize,
}

/// Python `str.isspace` (what LocOS `strip()` removes).
pub(crate) fn py_ws(c: char) -> bool {
    bilingua_model::py_isspace_char(c)
}

impl<'a, 'b> Planner<'a, 'b> {
    pub(crate) fn new(t: &'b Tree<'a>, props: &'b Props, limits: &'b Limits) -> Self {
        let n = t.els.len();
        Planner { t, props, limits, units: Vec::new(), plans: vec![Vec::new(); n], role: vec![0; n], plan_attrs: true, pending: Vec::new() }
    }

    fn add(&mut self, u: PUnit) -> Result<usize, Error> {
        self.units.push(u);
        guard::check_unit_count(self.units.len(), self.limits)?;
        Ok(self.units.len() - 1)
    }

    fn attrs_of(&mut self, el: u32) -> Result<(), Error> {
        if !self.plan_attrs {
            return Ok(());
        }
        let t = self.t;
        let e = &t.els[el as usize];
        for (k, ai) in (e.attr_lo..e.attr_hi).enumerate() {
            let Some(&note) = self.props.attr_tr.get(&ai) else { continue };
            let a = &t.attrs[ai as usize];
            let raw = t.attr_raw(a);
            // an undefined entity in the value: leave it alone
            if raw.contains('&') {
                let mut other = false;
                decode_refs(raw, |_| {
                    other = true;
                    None
                });
                if other {
                    continue;
                }
            }
            let shown = decode_refs(raw, |_| None).split_whitespace().collect::<Vec<_>>().join(" ");
            if shown.is_empty() {
                continue;
            }
            let local = t.attr_local(a).to_string();
            self.add(PUnit {
                key: (e.s0, k as i32),
                attr: Some(AttrUnit { el, attr: ai, local }),
                pieces: vec![Piece::Text(shown)],
                refs: Vec::new(),
                note,
                owner: el,
                lo: 0,
                hi: 0,
                lead: String::new(),
                trail: String::new(),
                cdata: false,
            })?;
        }
        Ok(())
    }

    /// Plans `el` and everything under it. Units are created in any order (`finish` sorts them by position), and
    /// each element's own plan only depends on its own items, so a work list replaces recursion.
    pub(crate) fn container(&mut self, el: u32, depth: usize) -> Result<(), Error> {
        self.pending.push((el, depth));
        self.drain()
    }

    /// Plans every element left on the work list.
    pub(crate) fn drain(&mut self) -> Result<(), Error> {
        while let Some((el, depth)) = self.pending.pop() {
            self.container_one(el, depth)?;
        }
        Ok(())
    }

    fn container_one(&mut self, el: u32, depth: usize) -> Result<(), Error> {
        if depth > HARD_DEPTH {
            return Err(too_deep());
        }
        self.role[el as usize] = ROLE_CONTAINER;
        self.attrs_of(el)?;
        let e = &self.t.els[el as usize];
        if e.empty {
            return Ok(());
        }
        let items: &'b [Item] = &self.t.els[el as usize].items;
        if !self.props.tr[el as usize] {
            for it in items {
                if it.kind == Kind::Elem {
                    self.pending.push((it.el, depth + 1));
                }
                self.plans[el as usize].push(Entry::Raw(*it));
            }
            return Ok(());
        }
        let mut lo = 0usize;
        for (i, it) in items.iter().enumerate() {
            if it.kind == Kind::Elem && self.props.wt[it.el as usize] == 0 {
                self.flush(el, lo, i, depth)?;
                lo = i + 1;
                self.pending.push((it.el, depth + 1));
                self.plans[el as usize].push(Entry::Raw(*it));
            }
        }
        self.flush(el, lo, items.len(), depth)
    }

    /// Items `lo..hi` of `owner` as one run: a unit when it has text of its own. Protected and nested elements in
    /// the run go on the work list (call [`Planner::drain`] after, as [`Planner::container`] does).
    pub(crate) fn flush(&mut self, owner: u32, lo: usize, hi: usize, depth: usize) -> Result<(), Error> {
        if lo >= hi {
            return Ok(());
        }
        let t = self.t;
        let items: &'b [Item] = &t.els[owner as usize].items[lo..hi];
        let mut pieces: Vec<Piece> = Vec::new();
        let mut refs: Vec<Ref> = Vec::new();
        let mut flags = Flags { cdata: 0, other: 0 };
        self.tokenize(items, &mut pieces, &mut refs, &mut flags, depth + 1)?;
        // merge adjacent text, collapse white space unless preserved, strip
        let mut merged: Vec<Piece> = Vec::with_capacity(pieces.len());
        for p in pieces {
            match (merged.last_mut(), p) {
                (Some(Piece::Text(a)), Piece::Text(b)) => a.push_str(&b),
                (_, p) => merged.push(p),
            }
        }
        if !self.props.pre[owner as usize] {
            for p in &mut merged {
                if let Piece::Text(s) = p {
                    *s = collapse_ws(s);
                }
            }
        }
        if let Some(Piece::Text(s)) = merged.first_mut() {
            *s = s.trim_start_matches([' ', '\t', '\r', '\n']).to_string();
        }
        if let Some(Piece::Text(s)) = merged.last_mut() {
            *s = s.trim_end_matches([' ', '\t', '\r', '\n']).to_string();
        }
        merged.retain(|p| !matches!(p, Piece::Text(s) if s.is_empty()));
        let has_text = merged.iter().any(|p| matches!(p, Piece::Text(s) if s.chars().any(|c| !py_ws(c))));
        let mut unit = None;
        if has_text {
            let first = &items[0];
            let last = &items[items.len() - 1];
            let (mut lead, mut trail) = (String::new(), String::new());
            if first.kind == Kind::Text {
                let raw = &t.text[first.s..first.e];
                lead = raw[..raw.len() - raw.trim_start_matches([' ', '\t', '\r', '\n']).len()].to_string();
            }
            if last.kind == Kind::Text {
                let raw = &t.text[last.s..last.e];
                trail = raw[xml_trim_end_len(raw)..].to_string();
                if std::ptr::eq(first, last) && lead.len() == raw.len() {
                    trail.clear();
                }
            }
            let start = items
                .iter()
                .find(|it| it.kind == Kind::Elem || !t.text[it.s..it.e].chars().all(py_ws))
                .map_or(items[0].s, |it| it.s);
            let note = self.props.note[owner as usize];
            let idx = self.add(PUnit {
                key: (start, -1),
                attr: None,
                pieces: merged,
                refs,
                note,
                owner,
                lo,
                hi,
                lead,
                trail,
                cdata: flags.cdata == 1 && flags.other == 0,
            })?;
            unit = Some(idx);
        }
        self.plans[owner as usize].push(Entry::Run { lo, hi, unit });
        Ok(())
    }

    /// The run's content as pieces, inline elements as open and close tags. Iterative: an explicit stack of the
    /// open inline elements, so deep inline nesting costs heap, not call stack.
    fn tokenize(&mut self, items: &'b [Item], pieces: &mut Vec<Piece>, refs: &mut Vec<Ref>, flags: &mut Flags, depth: usize) -> Result<(), Error> {
        if depth > HARD_DEPTH {
            return Err(too_deep());
        }
        let t = self.t;
        // (items, next index, ref number of the open tag to close when done; 0 for the run itself)
        let mut stack: Vec<(&'b [Item], usize, usize)> = vec![(items, 0, 0)];
        while let Some(top) = stack.last_mut() {
            let (list, i) = (top.0, top.1);
            if i >= list.len() {
                let (_, _, n) = stack.pop().expect("stack is not empty");
                if n != 0 {
                    pieces.push(Piece::Close(n));
                }
                continue;
            }
            top.1 += 1;
            let it = &list[i];
            let depth = depth + stack.len() - 1;
            match it.kind {
                Kind::Elem => {
                    flags.other += 1;
                    let el = it.el;
                    if !self.props.tr[el as usize] || self.props.wt[el as usize] != 1 {
                        refs.push(Ref::PointEl(el));
                        pieces.push(Piece::Point(refs.len()));
                        self.pending.push((el, depth));
                        continue;
                    }
                    self.role[el as usize] = ROLE_INLINE;
                    self.attrs_of(el)?;
                    if t.els[el as usize].empty {
                        refs.push(Ref::PointEl(el));
                        pieces.push(Piece::Point(refs.len()));
                        continue;
                    }
                    if depth + 1 > HARD_DEPTH {
                        return Err(too_deep());
                    }
                    refs.push(Ref::Pair(el));
                    let n = refs.len();
                    pieces.push(Piece::Open(n));
                    let inner: &'b [Item] = &t.els[el as usize].items;
                    stack.push((inner, 0, n));
                }
                Kind::Text => {
                    let raw = &t.text[it.s..it.e];
                    if !raw.chars().all(py_ws) {
                        flags.other += 1;
                    }
                    if !raw.contains('&') {
                        pieces.push(Piece::Text(raw.to_string()));
                        continue;
                    }
                    // undefined entities become point tags; the rest is decoded
                    let mut found: Vec<String> = Vec::new();
                    let s = decode_refs(raw, |e| {
                        found.push(e.to_string());
                        Some("\u{0}".into())
                    });
                    let mut it2 = found.into_iter();
                    for (k, part) in s.split('\u{0}').enumerate() {
                        if k > 0 {
                            if let Some(src) = it2.next() {
                                refs.push(Ref::Entity(src));
                                pieces.push(Piece::Point(refs.len()));
                            }
                        }
                        if !part.is_empty() {
                            pieces.push(Piece::Text(part.to_string()));
                        }
                    }
                }
                Kind::Cdata => {
                    flags.cdata += 1;
                    pieces.push(Piece::Text(t.text[it.s + 9..it.e - 3].to_string()));
                }
                Kind::Comment | Kind::Pi => {
                    flags.other += 1;
                    refs.push(Ref::PointItem(it.s, it.e));
                    pieces.push(Piece::Point(refs.len()));
                }
                Kind::Doctype => {}
            }
        }
        Ok(())
    }

    /// Sorts the units into document order and renumbers the plan entries.
    pub(crate) fn finish(self) -> Planned {
        let n = self.units.len();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| self.units[i].key);
        let mut rank = vec![0usize; n];
        for (r, &i) in order.iter().enumerate() {
            rank[i] = r;
        }
        let mut slots: Vec<Option<PUnit>> = self.units.into_iter().map(Some).collect();
        let units: Vec<PUnit> = order.iter().map(|&i| slots[i].take().expect("unit moved once")).collect();
        let mut plans = self.plans;
        for p in &mut plans {
            for e in p.iter_mut() {
                if let Entry::Run { unit: Some(u), .. } = e {
                    *u = rank[*u];
                }
            }
        }
        Planned { units, plans, role: self.role }
    }
}

fn xml_trim_end_len(raw: &str) -> usize {
    raw.trim_end_matches([' ', '\t', '\r', '\n']).len()
}

/// Runs of XML white space become one space.
pub(crate) fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if c.is_ascii() && is_xml_ws(c as u8) {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            in_ws = false;
            out.push(c);
        }
    }
    out
}

