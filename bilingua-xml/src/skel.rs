//! From a plan to a Bilingua document, and back to bytes.
//!
//! The skeleton is the file text split into `Raw` parts and `Unit` slots. Two kinds of marker, written with a NUL
//! character (which XML 1.0 forbids, so no file text can contain one), stand for content that is not fixed:
//!
//! * `\0U<n>\0`  the content of unit `n` (a nested unit inside a protected element), rendered where it stands;
//! * `\0A<n>\0`  the value of translatable attribute unit `n`, inside a start tag;
//! * `\0L<attr>|<0 or 1>\0 ... \0l\0`  around the root start tag: the language attribute to update (or add, `1`) on a
//!   target-language merge;
//! * `\0N<attr>\0 ... \0l\0`  around the start tag of a nested element that holds translated units and declares a
//!   language itself: on a target-language merge the attribute is changed only if it still names the source language.
//!
//! Markers appear in `Raw` parts and in the markup strings of a unit's codes (`open`, `close`, `standalone`).
//! Hidden values (`Unit::hidden`): `raw` the original markup of the run (or the raw value of an attribute),
//! `lead` and `trail` the white space around it, `cdata` the run is one CDATA section, `q` the quote of an
//! attribute value.

use crate::dialect::LangStamp;
use crate::plan::{Entry, Planned, Piece, Ref, ROLE_INLINE};
use crate::tree::{Item, Kind, Tree};
use bilingua_model::{
    parser_info, plain_text, CodeData, Document, Error, ExtractOptions, Inline, Note, SkelPart, Skeleton, TagKind, Unit,
};
use std::collections::HashMap;

pub(crate) const RAW: &str = "raw";
pub(crate) const LEAD: &str = "lead";
pub(crate) const TRAIL: &str = "trail";
pub(crate) const CDATA: &str = "cdata";
pub(crate) const QUOTE: &str = "q";

/// One step of [`Emit::emit`].
enum Job {
    /// File text `s..e` as written.
    Text(usize, usize),
    /// The marker of unit n.
    Unit(usize),
    Item(Item),
    Node(u32),
}

pub(crate) struct Emit<'a, 'b> {
    pub t: &'b Tree<'a>,
    pub p: &'b Planned,
    /// attribute index -> unit index
    pub attr_unit: HashMap<u32, usize>,
    pub lang: Option<(String, bool)>,
    /// Nested elements whose own language attribute is restamped (see the `N` marker).
    pub restamp: std::collections::HashSet<u32>,
}

impl Emit<'_, '_> {
    fn stag(&self, el: u32, out: &mut String) {
        let t = self.t;
        let e = &t.els[el as usize];
        let is_root = el == t.root;
        let stamp = if is_root { self.lang.as_ref() } else { None };
        let nested = !is_root && self.restamp.contains(&el);
        if let Some((a, add)) = stamp {
            out.push_str(&format!("\0L{a}|{}\0", u8::from(*add)));
        } else if nested {
            if let Some((a, _)) = &self.lang {
                out.push_str(&format!("\0N{a}\0"));
            }
        }
        let mut pos = e.s0;
        for ai in e.attr_lo..e.attr_hi {
            if let Some(&u) = self.attr_unit.get(&ai) {
                let a = &t.attrs[ai as usize];
                out.push_str(&t.text[pos..a.val_s]);
                out.push_str(&format!("\0A{u}\0"));
                pos = a.val_e;
            }
        }
        out.push_str(&t.text[pos..e.s1]);
        if stamp.is_some() || (nested && self.lang.is_some()) {
            out.push_str("\0l\0");
        }
    }

    /// An item as the skeleton writes it: inline elements in full, planned elements with their unit markers.
    pub(crate) fn raw_item(&self, it: &Item, out: &mut String) {
        self.emit(Job::Item(*it), out);
    }

    /// A planned element: its start tag, its plan (raw items and unit markers) and its end tag.
    pub(crate) fn node(&self, el: u32, out: &mut String) {
        self.emit(Job::Node(el), out);
    }

    /// The writer behind `raw_item` and `node`: an explicit stack of jobs, so deep nesting costs heap, not call
    /// stack.
    fn emit(&self, first: Job, out: &mut String) {
        let t = self.t;
        let mut jobs: Vec<Job> = vec![first];
        while let Some(job) = jobs.pop() {
            match job {
                Job::Text(s, e) => out.push_str(&t.text[s..e]),
                Job::Unit(u) => {
                    out.push_str("\0U");
                    out.push_str(&u.to_string());
                    out.push('\0');
                }
                Job::Item(it) => {
                    if it.kind != Kind::Elem {
                        out.push_str(&t.text[it.s..it.e]);
                        continue;
                    }
                    let el = it.el;
                    if self.p.role[el as usize] != ROLE_INLINE {
                        jobs.push(Job::Node(el));
                        continue;
                    }
                    self.stag(el, out);
                    let e = &t.els[el as usize];
                    if !e.empty {
                        jobs.push(Job::Text(e.e0, e.e1));
                        jobs.extend(e.items.iter().rev().map(|c| Job::Item(*c)));
                    }
                }
                Job::Node(el) => {
                    self.stag(el, out);
                    let e = &t.els[el as usize];
                    if e.empty {
                        continue;
                    }
                    jobs.push(Job::Text(e.e0, e.e1));
                    for entry in self.p.plans[el as usize].iter().rev() {
                        match entry {
                            Entry::Raw(it) => jobs.push(Job::Item(*it)),
                            Entry::Run { unit: Some(u), .. } => jobs.push(Job::Unit(*u)),
                            Entry::Run { lo, hi, unit: None } => jobs.extend(e.items[*lo..*hi].iter().rev().map(|c| Job::Item(*c))),
                        }
                    }
                }
            }
        }
    }

    fn range(&self, owner: u32, lo: usize, hi: usize) -> String {
        let mut s = String::new();
        for it in &self.t.els[owner as usize].items[lo..hi] {
            self.raw_item(it, &mut s);
        }
        s
    }
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

/// Where the root language attribute goes (decided from the root start tag), as the skeleton marker needs.
pub(crate) fn lang_plan(t: &Tree, stamp: &LangStamp) -> Option<(String, bool)> {
    let names: Vec<&str> = t.attrs_of(t.root).iter().map(|a| t.attr_qname(a)).collect();
    for u in &stamp.update {
        if names.contains(&u.as_str()) {
            return Some((u.clone(), false));
        }
    }
    if let Some((ns, alt)) = &stamp.add_if_root_outside {
        if t.el_ns(t.root) != ns {
            return Some((alt.clone(), true));
        }
    }
    if !stamp.add.is_empty() {
        return Some((stamp.add.clone(), true));
    }
    None
}

/// Builds the Bilingua units and skeleton from a plan.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_document(t: &Tree, p: &Planned, notes: &[String], alt: &std::collections::HashSet<String>, opts: &ExtractOptions, encoding: String, lang: Option<(String, bool)>, format: &str) -> Document {
    let mut attr_unit = HashMap::new();
    for (i, u) in p.units.iter().enumerate() {
        if let Some(a) = &u.attr {
            attr_unit.insert(a.attr, i);
        }
    }
    // nested elements that hold translated text and carry their own language attribute
    let mut restamp = std::collections::HashSet::new();
    if let Some((a, _)) = &lang {
        let mut mark = |mut el: u32| {
            while el != crate::tree::NONE && el != t.root {
                if !restamp.insert(el) {
                    break;
                }
                el = t.els[el as usize].parent;
            }
        };
        for u in &p.units {
            mark(u.attr.as_ref().map_or(u.owner, |x| x.el));
        }
        restamp.retain(|&el| t.attrs_of(el).iter().any(|x| t.attr_qname(x) == a.as_str()));
    }
    let em = Emit { t, p, attr_unit, lang, restamp };
    // skeleton parts
    let mut whole = String::with_capacity(t.text.len() + 64);
    for it in &t.prolog {
        em.raw_item(it, &mut whole);
    }
    em.node(t.root, &mut whole);
    for it in &t.epilog {
        em.raw_item(it, &mut whole);
    }
    let parts = split_parts(&whole);

    // units
    let mut units: Vec<Unit> = Vec::with_capacity(p.units.len());
    let mut line = 1usize;
    let mut line_at = 0usize;
    for (i, pu) in p.units.iter().enumerate() {
        let mut inl: Vec<Inline> = Vec::with_capacity(pu.pieces.len());
        let mut codes: std::collections::BTreeMap<String, CodeData> = Default::default();
        for pc in &pu.pieces {
            match pc {
                Piece::Text(s) => inl.push(Inline::text(s.clone())),
                Piece::Open(n) | Piece::Close(n) | Piece::Point(n) => {
                    let id = n.to_string();
                    let r = &pu.refs[n - 1];
                    match pc {
                        Piece::Open(_) => {
                            let Ref::Pair(el) = r else { continue };
                            let e = &t.els[*el as usize];
                            let mut open = String::new();
                            em.stag_for_code(*el, &mut open);
                            let disp = short(&t.text[e.s0..e.s1], 120);
                            let close = t.text[e.e0..e.e1].to_string();
                            codes.insert(id.clone(), CodeData::new().with_open(open).with_close(close));
                            inl.push(Inline::tag(id, TagKind::Open, disp));
                        }
                        Piece::Close(_) => {
                            let Ref::Pair(el) = r else { continue };
                            let e = &t.els[*el as usize];
                            inl.push(Inline::tag(id, TagKind::Close, t.text[e.e0..e.e1].to_string()));
                        }
                        _ => {
                            let (markup, disp) = match r {
                                Ref::PointEl(el) => {
                                    let e = &t.els[*el as usize];
                                    let mut s = String::new();
                                    em.raw_item(&Item { kind: Kind::Elem, s: e.s0, e: e.e1, el: *el }, &mut s);
                                    let plain = &t.text[e.s0..e.e1];
                                    let disp = if e.empty || plain.chars().count() <= 60 { short(plain, 120) } else { format!("<{}>…</{}>", t.qname(*el), t.qname(*el)) };
                                    (s, disp)
                                }
                                Ref::PointItem(s, e) => (t.text[*s..*e].to_string(), short(&t.text[*s..*e], 80)),
                                Ref::Entity(src) => (src.clone(), src.clone()),
                                Ref::Pair(_) => continue,
                            };
                            codes.insert(id.clone(), CodeData::new().with_standalone(markup));
                            inl.push(Inline::tag(id, TagKind::Standalone, disp));
                        }
                    }
                }
            }
        }
        let mut u = Unit::new(format!("u{}", i + 1), inl);
        u.codes = codes;
        match &pu.attr {
            Some(a) => {
                let at = &t.attrs[a.attr as usize];
                u.set_hidden(RAW, t.attr_raw(at));
                u.set_hidden(QUOTE, (at.quote as char).to_string());
                u.context.kind = Some("attribute".into());
                u.context.style = Some(if alt.contains(&a.local) { "a11y:img-alt".into() } else { format!("attr:{}", a.local) });
                u.context.key = Some(format!("{}/@{}", path_of(t, a.el), a.local));
            }
            None => {
                u.set_hidden(RAW, em.range(pu.owner, pu.lo, pu.hi));
                if !pu.lead.is_empty() {
                    u.set_hidden(LEAD, pu.lead.clone());
                }
                if !pu.trail.is_empty() {
                    u.set_hidden(TRAIL, pu.trail.clone());
                }
                if pu.cdata {
                    u.set_hidden(CDATA, "1");
                }
                u.context.kind = Some("element".into());
                u.context.key = Some(path_of(t, pu.owner));
            }
        }
        let off = pu.key.0;
        if off > line_at {
            line += t.text.as_bytes()[line_at..off].iter().filter(|b| **b == b'\n').count();
            line_at = off;
        }
        u.context.location = Some(format!("line {line}"));
        if pu.note != 0 {
            let text = notes[pu.note as usize].clone();
            u.context.notes.push(text.clone());
            u.notes.push(Note::new(text).with_kind("locNote".to_string()));
        }
        units.push(u);
    }
    Document::new(format, units, Skeleton::text(parts, encoding, parser_info!())).with_options(opts)
}

impl Emit<'_, '_> {
    fn stag_for_code(&self, el: u32, out: &mut String) {
        // an inline element's start tag, with attribute-unit markers (never the root, so no language markers)
        self.stag(el, out);
    }
}

/// Longest element path a unit key holds. Every unit carries its path, so without a cap a deep chain of long element
/// names above many units multiplies (256 levels of 1 000-character names over 50 000 units is 12 GB from a 1 MB
/// file); a longer path keeps its innermost elements after `/…`.
pub(crate) const MAX_KEY_PATH: usize = 512;

fn path_of(t: &Tree, mut el: u32) -> String {
    let mut names: Vec<&str> = Vec::new();
    let mut len = 0usize;
    let mut cut = false;
    while el != crate::tree::NONE {
        let n = t.local(el);
        if len + n.len() + 1 > MAX_KEY_PATH {
            cut = true;
            break;
        }
        len += n.len() + 1;
        names.push(n);
        el = t.els[el as usize].parent;
    }
    let mut s = String::with_capacity(len + 4);
    if cut {
        s.push_str("/\u{2026}");
    }
    for n in names.iter().rev() {
        s.push('/');
        s.push_str(n);
    }
    s
}

/// Splits the marked-up text into `Raw` parts and `Unit` slots at the top-level `\0U<n>\0` markers.
fn split_parts(whole: &str) -> Vec<SkelPart> {
    let mut parts = Vec::new();
    let mut rest = whole;
    let mut raw = String::new();
    while let Some(p) = rest.find('\0') {
        raw.push_str(&rest[..p]);
        let after = &rest[p + 1..];
        // the parser refuses NUL in file text, so every NUL here opens a marker this module wrote; never panic
        // all the same (an unclosed marker is kept as text and merge reports it)
        let Some(end) = after.find('\0') else {
            raw.push('\0');
            raw.push_str(after);
            rest = "";
            break;
        };
        let body = &after[..end];
        if let Some(n) = body.strip_prefix('U').and_then(|n| n.parse::<usize>().ok()) {
            if !raw.is_empty() {
                parts.push(SkelPart::Raw(std::mem::take(&mut raw)));
            }
            parts.push(SkelPart::Unit(n));
        } else {
            raw.push('\0');
            raw.push_str(body);
            raw.push('\0');
        }
        rest = &after[end + 1..];
    }
    raw.push_str(rest);
    if !raw.is_empty() {
        parts.push(SkelPart::Raw(raw));
    }
    parts
}

// ------------------------------------------------------------------------------------------- merge

pub(crate) fn xml_target_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{0B}' | '\u{0C}' => out.push('\n'),
            '\u{0}'..='\u{8}' | '\u{0E}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

pub(crate) fn esc_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

pub(crate) fn esc_attr(s: &str, quote: char) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '"' if quote == '"' => out.push_str("&quot;"),
            '\'' if quote == '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn code_str<'u>(u: &'u Unit, key: &str) -> Option<&'u str> {
    u.hidden(key)
}

pub(crate) fn translated(u: &Unit) -> bool {
    u.translate && u.segments.iter().any(|s| s.has_target())
}

pub(crate) struct Merger<'d> {
    pub doc: &'d Document,
    pub out: String,
    pub lang_tag: Option<String>,
    lang_start: Option<(usize, String, bool, bool)>,
    /// The language the root declared in the source (the stand-in for the source language when the document has none).
    root_lang: Option<String>,
    /// Units already written: a well-formed skeleton names each unit once, so a second use (a hand-edited or
    /// hostile document, possibly a cycle or an exponential fan-out) is refused.
    seen: Vec<bool>,
}

/// One step of the merge (an explicit stack, so nested units cost heap, not call stack).
enum Work<'d> {
    /// Marked-up text whose markers are still to resolve.
    Marked(&'d str),
    /// Text written as is.
    Plain(&'d str),
    /// Escaped translation text.
    Owned(String),
    Unit(usize),
}

impl<'d> Merger<'d> {
    pub(crate) fn new(doc: &'d Document, lang_tag: Option<String>) -> Merger<'d> {
        Merger { doc, out: String::new(), lang_tag, lang_start: None, root_lang: None, seen: vec![false; doc.units.len()] }
    }

    pub(crate) fn unit(&self, i: usize) -> Result<&'d Unit, Error> {
        self.doc.units.get(i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))
    }

    /// Writes marked-up text, resolving its markers.
    pub(crate) fn resolve(&mut self, s: &'d str) -> Result<(), Error> {
        self.run(vec![Work::Marked(s)])
    }

    pub(crate) fn render_unit(&mut self, i: usize) -> Result<(), Error> {
        self.run(vec![Work::Unit(i)])
    }

    /// The unit's translation (leading and trailing white space included), whatever the source says.
    pub(crate) fn render_translated(&mut self, i: usize) -> Result<(), Error> {
        let mut work = Vec::new();
        self.translated_work(i, &mut work)?;
        self.run(work)
    }

    fn run(&mut self, mut work: Vec<Work<'d>>) -> Result<(), Error> {
        while let Some(w) = work.pop() {
            match w {
                Work::Plain(s) => self.out.push_str(s),
                Work::Owned(s) => self.out.push_str(&s),
                Work::Unit(i) => {
                    let u = self.unit(i)?;
                    if std::mem::replace(&mut self.seen[i], true) {
                        return Err(Error::Write(format!("skeleton uses unit {i} more than once")));
                    }
                    if translated(u) && u.output() != u.source() {
                        self.translated_work(i, &mut work)?;
                    } else {
                        work.push(Work::Marked(code_str(u, RAW).unwrap_or("")));
                    }
                }
                Work::Marked(s) => self.marked(s, &mut work)?,
            }
        }
        Ok(())
    }

    /// Writes `s` up to its first unit marker; the marker and the rest go back on the work list.
    fn marked(&mut self, s: &'d str, work: &mut Vec<Work<'d>>) -> Result<(), Error> {
        let mut rest = s;
        while let Some(p) = rest.find('\0') {
            self.out.push_str(&rest[..p]);
            let after = &rest[p + 1..];
            let end = after.find('\0').ok_or_else(|| Error::Write("unterminated skeleton marker".into()))?;
            let body = &after[..end];
            rest = &after[end + 1..];
            let (kind, arg) = body.split_at(body.chars().next().map_or(0, char::len_utf8));
            match kind {
                "U" => {
                    let i: usize = arg.parse().map_err(|_| Error::Write("bad unit marker".into()))?;
                    work.push(Work::Marked(rest));
                    work.push(Work::Unit(i));
                    return Ok(());
                }
                "A" => {
                    let i: usize = arg.parse().map_err(|_| Error::Write("bad attribute marker".into()))?;
                    self.render_attr(i)?;
                }
                "L" => {
                    if let (Some(_), Some((a, add))) = (&self.lang_tag, arg.split_once('|')) {
                        self.lang_start = Some((self.out.len(), a.to_string(), add == "1", false));
                    }
                }
                "N" => {
                    if self.lang_tag.is_some() {
                        self.lang_start = Some((self.out.len(), arg.to_string(), false, true));
                    }
                }
                "l" => {
                    if let (Some((at, attr, add, nested)), Some(tag)) = (self.lang_start.take(), self.lang_tag.clone()) {
                        if at <= self.out.len() && self.out.is_char_boundary(at) {
                            let stag = self.out.split_off(at);
                            let old = lang_value(&stag, &attr);
                            let edited = if nested {
                                // only a language that is still the source language: a foreign passage stays as it is
                                // (no source language anywhere: the first language the file declares is its source)
                                if self.doc.source_lang.is_none() && self.root_lang.is_none() {
                                    self.root_lang = old.clone();
                                }
                                let src = self.doc.source_lang.clone().or_else(|| self.root_lang.clone());
                                match (old, src) {
                                    (Some(o), Some(s)) if same_lang(&o, &s) => set_lang(&stag, &attr, &tag, false),
                                    _ => stag,
                                }
                            } else {
                                self.root_lang = old;
                                set_lang(&stag, &attr, &tag, add)
                            };
                            self.out.push_str(&edited);
                        }
                    }
                }
                _ => return Err(Error::Write("unknown skeleton marker".into())),
            }
        }
        self.out.push_str(rest);
        Ok(())
    }

    pub(crate) fn render_attr(&mut self, i: usize) -> Result<(), Error> {
        let u = self.unit(i)?;
        let raw = code_str(u, RAW).unwrap_or("");
        if !translated(u) {
            self.out.push_str(raw);
            return Ok(());
        }
        let new = xml_target_text(&plain_text(&u.output()));
        if new == plain_text(&u.source()) {
            self.out.push_str(raw);
            return Ok(());
        }
        let q = code_str(u, QUOTE).and_then(|s| s.chars().next()).unwrap_or('"');
        self.out.push_str(&esc_attr(&new, q));
        Ok(())
    }

    /// Queues the translation of unit `i`: text and tags, with the tags the translation dropped repaired (a stray
    /// close is dropped, an unclosed pair is closed, a protected element the translation lost is kept at the end).
    fn translated_work(&mut self, i: usize, work: &mut Vec<Work<'d>>) -> Result<(), Error> {
        let u = self.unit(i)?;
        self.out.push_str(code_str(u, LEAD).unwrap_or(""));
        let new = u.output();
        let trail = code_str(u, TRAIL).unwrap_or("");
        if code_str(u, CDATA).is_some() {
            let body = xml_target_text(&plain_text(&new)).replace("]]>", "]]]]><![CDATA[>");
            self.out.push_str("<![CDATA[");
            self.out.push_str(&body);
            self.out.push_str("]]>");
            self.out.push_str(trail);
            return Ok(());
        }
        let mut seq: Vec<Work<'d>> = Vec::new();
        let mut used: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut stack: Vec<&'d str> = Vec::new();
        for inl in &new {
            match inl {
                Inline::Text(t) => {
                    let mut s = String::new();
                    esc_text(&xml_target_text(t), &mut s);
                    seq.push(Work::Owned(s));
                }
                Inline::Tag { id, kind, .. } => {
                    let Some((key, cd)) = u.codes.get_key_value(id) else { continue };
                    match kind {
                        TagKind::Open => {
                            if let (Some(open), false) = (&cd.open, used.contains(key.as_str())) {
                                used.insert(key);
                                stack.push(key);
                                seq.push(Work::Marked(open));
                            }
                        }
                        TagKind::Close => {
                            if stack.last() == Some(&key.as_str()) {
                                stack.pop();
                                if let Some(c) = &cd.close {
                                    seq.push(Work::Plain(c));
                                }
                            }
                        }
                        TagKind::Standalone => {
                            if let (Some(s), false) = (&cd.standalone, used.contains(key.as_str())) {
                                used.insert(key);
                                seq.push(Work::Marked(s));
                            }
                        }
                    }
                }
            }
        }
        while let Some(id) = stack.pop() {
            if let Some(c) = u.codes.get(id).and_then(|c| c.close.as_deref()) {
                seq.push(Work::Plain(c));
            }
        }
        let mut rest: Vec<(&'d String, &'d CodeData)> = u
            .codes
            .iter()
            .filter(|(k, c)| k.bytes().all(|b| b.is_ascii_digit()) && c.standalone.is_some() && !used.contains(k.as_str()))
            .collect();
        rest.sort_by_key(|(k, _)| k.parse::<usize>().unwrap_or(usize::MAX));
        for (_, c) in rest {
            if let Some(s) = &c.standalone {
                seq.push(Work::Marked(s));
            }
        }
        seq.push(Work::Plain(trail));
        work.extend(seq.into_iter().rev());
        Ok(())
    }
}

/// Whether two language tags name the same language for a restamp: the primary subtag must match, and the script and
/// the region too when both tags have one (`en` and `en-US` match, `en-GB` and `en-US` do not, `zh-Hans` and `zh-Hant`
/// do not). Case and `_` against `-` are ignored.
pub(crate) fn same_lang(a: &str, b: &str) -> bool {
    fn parts(s: &str) -> (String, Option<String>, Option<String>) {
        let s = s.trim().replace('_', "-").to_ascii_lowercase();
        let mut it = s.split('-');
        let primary = it.next().unwrap_or("").to_string();
        let (mut script, mut region) = (None, None);
        for p in it {
            let alpha = p.bytes().all(|c| c.is_ascii_alphabetic());
            if script.is_none() && region.is_none() && p.len() == 4 && alpha {
                script = Some(p.to_string());
            } else if region.is_none() && ((p.len() == 2 && alpha) || (p.len() == 3 && p.bytes().all(|c| c.is_ascii_digit()))) {
                region = Some(p.to_string());
            } else {
                break;
            }
        }
        (primary, script, region)
    }
    let (pa, sa, ra) = parts(a);
    let (pb, sb, rb) = parts(b);
    let both = |x: Option<String>, y: Option<String>| match (x, y) {
        (Some(x), Some(y)) => x == y,
        _ => true,
    };
    !pa.is_empty() && pa == pb && both(sa, sb) && both(ra, rb)
}

#[cfg(test)]
mod lang_tests {
    use super::same_lang;

    #[test]
    fn same_language_rule() {
        for (a, b) in [("en", "en"), ("en", "EN-us"), ("en-US", "en"), ("en_US", "en-us"), ("zh-Hant-TW", "zh-TW"), ("zh", "zh-Hans")] {
            assert!(same_lang(a, b), "{a} {b}");
        }
        for (a, b) in [("en-GB", "en-US"), ("fr", "en"), ("zh-Hans", "zh-Hant"), ("", ""), ("en", "eng")] {
            assert!(!same_lang(a, b), "{a} {b}");
        }
    }
}

/// The raw (still escaped) value of attribute `attr` in a start tag, if it has one.
fn lang_value(stag: &str, attr: &str) -> Option<String> {
    let probe = set_lang_scan(stag, attr)?;
    Some(stag[probe.0..probe.1].to_string())
}

/// Sets (or adds) the language attribute in a start tag, keeping everything else.
pub(crate) fn set_lang(stag: &str, attr: &str, value: &str, add: bool) -> String {
    if let Some((s, e, q)) = set_lang_scan_q(stag, attr) {
        return format!("{}{}{}", &stag[..s], esc_attr(value, q), &stag[e..]);
    }
    if add {
        let at = stag.len() - if stag.ends_with("/>") { 2 } else { 1 };
        return format!("{} {attr}=\"{}\"{}", &stag[..at], esc_attr(value, '"'), &stag[at..]);
    }
    stag.to_string()
}

fn set_lang_scan(stag: &str, attr: &str) -> Option<(usize, usize)> {
    set_lang_scan_q(stag, attr).map(|(s, e, _)| (s, e))
}

/// The value range and quote of the last `attr` attribute of a start tag.
fn set_lang_scan_q(stag: &str, attr: &str) -> Option<(usize, usize, char)> {
    let b = stag.as_bytes();
    let mut i = 1;
    while i < b.len() && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>') {
        i += 1;
    }
    let mut hit: Option<(usize, usize, char)> = None;
    loop {
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i >= b.len() || b[i] == b'/' || b[i] == b'>' {
            break;
        }
        let ns = i;
        while i < b.len() && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'=' | b'/' | b'>') {
            i += 1;
        }
        let name = &stag[ns..i];
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' {
            break;
        }
        i += 1;
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i >= b.len() || (b[i] != b'"' && b[i] != b'\'') {
            break;
        }
        let q = b[i];
        let vs = i + 1;
        let Some(len) = b[vs..].iter().position(|c| *c == q) else { break };
        if name == attr {
            hit = Some((vs, vs + len, q as char));
        }
        i = vs + len + 1;
    }
    hit
}

pub(crate) fn merge_parts<'d>(doc: &'d Document, parts: &'d [SkelPart], lang_tag: Option<String>) -> Result<String, Error> {
    let mut m = Merger::new(doc, lang_tag);
    for p in parts {
        match p {
            SkelPart::Raw(s) => m.resolve(s)?,
            SkelPart::Unit(i) => m.render_unit(*i)?,
        }
    }
    Ok(m.out)
}

