//! Reading the messages of a bundle, extraction into a `Document`, and the two writers (XTB from XMB, XTB splice).

use crate::render::{escape_attr, render};
use crate::scan::{self, perr, Child, Elem, Tree};
use crate::{lang, textenc, LEAD, RAW, TRAIL};
use bilingua_model::*;
use std::collections::{BTreeMap, HashSet};

const NOTE_MAX: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Xmb,
    Xtb,
}

impl Kind {
    fn root(self) -> &'static str {
        match self {
            Kind::Xmb => "messagebundle",
            Kind::Xtb => "translationbundle",
        }
    }
    fn msg(self) -> &'static str {
        match self {
            Kind::Xmb => "msg",
            Kind::Xtb => "translation",
        }
    }
    fn id(self) -> &'static str {
        match self {
            Kind::Xmb => "xmb",
            Kind::Xtb => "xtb",
        }
    }
}

enum Piece {
    Text(String),
    Raw(String),
    Ph(usize),
}

/// One `<msg>` or `<translation>`.
pub struct Msg {
    pub id: Option<String>,
    /// Content span: after the start tag to the end tag.
    pub open_end: usize,
    pub close_start: usize,
    pub inl: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
    pub lead: String,
    pub trail: String,
    pub note: String,
    pub desc: Option<String>,
    pub meaning: Option<String>,
}

impl Msg {
    /// Text without tags is blank: the message is copied as it is and never a unit.
    fn placeholder_only(&self) -> bool {
        plain_text(&self.inl).trim().is_empty()
    }
}

/// `START_X` ... `CLOSE_X` pairs that nest properly: `(close of each start, start of each close)`.
fn pair_names(names: &[String]) -> (Vec<Option<usize>>, Vec<Option<usize>>) {
    let mut close_of = vec![None; names.len()];
    let mut start_of = vec![None; names.len()];
    let mut stack: Vec<(&str, usize)> = Vec::new();
    for (i, nm) in names.iter().enumerate() {
        if let Some(x) = nm.strip_prefix("START_") {
            stack.push((x, i));
        } else if let Some(x) = nm.strip_prefix("CLOSE_") {
            if stack.last().is_some_and(|(s, _)| *s == x) {
                let (_, s) = stack.pop().expect("checked");
                close_of[s] = Some(i);
                start_of[i] = Some(s);
            }
        }
    }
    (close_of, start_of)
}

/// All character data under an element, entities resolved.
fn itertext(text: &str, tree: &Tree, e: &Elem, out: &mut String) {
    // iterative walk
    let mut stack: Vec<std::slice::Iter<Child>> = vec![e.children.iter()];
    while let Some(it) = stack.last_mut() {
        match it.next() {
            None => {
                stack.pop();
            }
            Some(Child::Text { start, end, cdata }) => {
                if *cdata {
                    out.push_str(&text[*start..*end]);
                } else {
                    out.push_str(&scan::unescape(&text[*start..*end]));
                }
            }
            Some(Child::Other { .. }) => {}
            Some(Child::Elem(i)) => stack.push(tree.elems[*i].children.iter()),
        }
    }
}

fn direct_text(text: &str, e: &Elem) -> String {
    let mut s = String::new();
    for c in &e.children {
        if let Child::Text { start, end, cdata } = c {
            if *cdata {
                s.push_str(&text[*start..*end]);
            } else {
                s.push_str(&scan::unescape(&text[*start..*end]));
            }
        }
    }
    s
}

fn shorten(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn analyse(text: &str, tree: &Tree, el: &Elem) -> Msg {
    let mut pieces: Vec<Piece> = Vec::new();
    for c in &el.children {
        match c {
            Child::Text { start, end, cdata } => {
                pieces.push(Piece::Text(if *cdata { text[*start..*end].to_string() } else { scan::unescape(&text[*start..*end]) }))
            }
            Child::Other { start, end } => pieces.push(Piece::Raw(text[*start..*end].to_string())),
            Child::Elem(i) => {
                let ce = &tree.elems[*i];
                match ce.name.as_str() {
                    "source" => {}
                    "ph" => pieces.push(Piece::Ph(*i)),
                    _ => pieces.push(Piece::Raw(text[ce.start..ce.end].to_string())),
                }
            }
        }
    }
    let ph_elems: Vec<&Elem> = pieces
        .iter()
        .filter_map(|p| if let Piece::Ph(i) = p { Some(&tree.elems[*i]) } else { None })
        .collect();
    let names: Vec<String> = ph_elems.iter().map(|e| e.attr("name").unwrap_or("").to_string()).collect();
    let (close_of, start_of) = pair_names(&names);

    let mut inl: Vec<Inline> = Vec::new();
    let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
    let mut examples: Vec<String> = Vec::new();
    let mut n = 0usize;
    let mut ph_i = 0usize;
    let mut token_of: BTreeMap<usize, String> = BTreeMap::new();
    for p in &pieces {
        match p {
            Piece::Text(t) => inl.push(Inline::Text(t.clone())),
            Piece::Raw(r) => {
                n += 1;
                let id = n.to_string();
                let disp: String = shorten(r, 120);
                codes.insert(id.clone(), CodeData::new().with_standalone(r.clone()));
                inl.push(Inline::tag(id, TagKind::Standalone, disp));
            }
            Piece::Ph(ei) => {
                let e = &tree.elems[*ei];
                let name = names[ph_i].clone();
                let raw = text[e.start..e.end].to_string();
                let ex = e.children.iter().find_map(|c| match c {
                    Child::Elem(i) if tree.elems[*i].name == "ex" => Some(*i),
                    _ => None,
                });
                let ex_text = ex.map(|i| {
                    let mut s = String::new();
                    itertext(text, tree, &tree.elems[i], &mut s);
                    s
                });
                let body = direct_text(text, e);
                let body = body.trim();
                let display = if !body.is_empty() {
                    body.to_string()
                } else if let Some(x) = ex_text.as_deref().filter(|x| !x.is_empty()) {
                    x.to_string()
                } else {
                    format!("{{{name}}}")
                };
                if let Some(s) = start_of[ph_i] {
                    let id = token_of[&s].clone();
                    if let Some(c) = codes.get_mut(&id) {
                        c.close = Some(raw);
                    }
                    inl.push(Inline::tag(id, TagKind::Close, display));
                } else {
                    n += 1;
                    let id = n.to_string();
                    if close_of[ph_i].is_some() {
                        token_of.insert(ph_i, id.clone());
                        codes.insert(id.clone(), CodeData::new().with_open(raw).with_label(name.clone()));
                        inl.push(Inline::tag(id, TagKind::Open, display));
                    } else {
                        codes.insert(id.clone(), CodeData::new().with_standalone(raw).with_label(name.clone()));
                        inl.push(Inline::tag(id, TagKind::Standalone, display));
                    }
                    if let Some(x) = ex_text.filter(|x| !x.is_empty()) {
                        examples.push(format!("{name} = {x}"));
                    }
                }
                ph_i += 1;
            }
        }
    }
    let mut inl = merge_text(inl);
    // edge white space stays outside the unit
    let mut lead = String::new();
    let mut trail = String::new();
    if let Some(Inline::Text(t)) = inl.first_mut() {
        let rest = t.trim_start();
        lead = t[..t.len() - rest.len()].to_string();
        *t = rest.to_string();
    }
    let whole_blank = inl.is_empty() || (inl.len() == 1 && matches!(&inl[0], Inline::Text(t) if t.is_empty()));
    if !whole_blank {
        if let Some(Inline::Text(t)) = inl.last_mut() {
            let rest = t.trim_end();
            trail = t[rest.len()..].to_string();
            *t = rest.to_string();
        }
    }
    inl.retain(|i| !matches!(i, Inline::Text(t) if t.is_empty()));

    let desc = el.attr("desc").filter(|s| !s.is_empty()).map(str::to_string);
    let meaning = el.attr("meaning").filter(|s| !s.is_empty()).map(str::to_string);
    let mut notes = Vec::new();
    if let Some(d) = &desc {
        notes.push(format!("Description: {d}"));
    }
    if let Some(m) = &meaning {
        notes.push(format!("Meaning: {m}"));
    }
    if !examples.is_empty() {
        notes.push(format!("Placeholders: {}", examples.join("; ")));
    }
    Msg {
        id: el.attr("id").map(str::to_string),
        open_end: el.open_end,
        close_start: el.close_start,
        inl,
        codes,
        lead,
        trail,
        note: shorten(&notes.join(" | "), NOTE_MAX),
        desc,
        meaning,
    }
}

pub struct Parsed {
    pub text: String,
    pub kind: Kind,
    pub msgs: Vec<Msg>,
}

pub fn read(text: String, limits: &Limits) -> Result<Parsed, Error> {
    // Forty markup characters per allowed unit is far above any real bundle: bound the tree before building it.
    if text.bytes().filter(|b| *b == b'<').count() > limits.max_units.saturating_mul(40) {
        return Err(LimitError::new(
            LimitKind::XmlElements,
            format!("This XMB/XTB file has far more markup than {} segments allow. Split it into smaller files.", limits.max_units),
        )
        .into());
    }
    let tree = scan::parse(&text)?;
    let root = &tree.elems[tree.root];
    let kind = match root.name.as_str() {
        "messagebundle" => Kind::Xmb,
        "translationbundle" => Kind::Xtb,
        _ => {
            return Err(perr("not an Angular message bundle: the root element must be <messagebundle> (XMB) or <translationbundle> (XTB)."))
        }
    };
    debug_assert_eq!(kind.root(), root.name);
    let mut msgs = Vec::new();
    for c in &root.children {
        if let Child::Elem(i) = c {
            let el = &tree.elems[*i];
            if el.name == kind.msg() {
                msgs.push(analyse(&text, &tree, el));
            }
        }
    }
    Ok(Parsed { text, kind, msgs })
}

/// Indices (into `msgs`) of the messages that are units: first occurrence of an id, and not placeholder-only.
fn unit_indices(msgs: &[Msg]) -> Vec<usize> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for (i, m) in msgs.iter().enumerate() {
        let Some(id) = m.id.as_deref() else { continue };
        if !seen.insert(id) || m.placeholder_only() {
            continue;
        }
        out.push(i);
    }
    out
}

pub fn extract(bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
    guard::check_input(bytes, limits)?;
    guard::check_xml(bytes, limits)?;
    let dec = enc::decode_text(bytes, &enc::DecodeOptions::with_limits(*limits).with_source_lang(opts.source_lang.as_deref()).declared())?;
    if enc::is_legacy(&dec.encoding) && enc::declared_encoding(bytes).is_none() {
        // XML without an encoding declaration is UTF-8 (or UTF-16 with a BOM): never guess a code page
        return Err(Error::Encoding("This file is not valid UTF-8 and declares no other encoding. Re-save it as UTF-8 and upload it again.".into()));
    }
    if !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") && textenc::encode(&dec.text, &dec.encoding, &bilingua_model::MergeOptions::default())? != bytes {
        // double-mapped bytes of the CJK code pages read to the same character and write back as the other spelling;
        // an untouched merge must be the original bytes, so the file is refused, never altered
        return Err(Error::Encoding("This file uses byte sequences of its legacy encoding that cannot be written back unchanged. Re-save the file as UTF-8.".into()));
    }
    let parsed = read(dec.text, limits)?;
    let text = &parsed.text;
    let idxs = unit_indices(&parsed.msgs);
    let mut units: Vec<Unit> = Vec::with_capacity(idxs.len());
    let mut parts: Vec<SkelPart> = Vec::with_capacity(idxs.len() * 2 + 1);
    let mut pos = 0usize;
    for &mi in &idxs {
        let m = &parsed.msgs[mi];
        guard::check_unit_count(units.len() + 1, limits)?;
        let mut u = Unit::new(format!("u{}", units.len() + 1), m.inl.clone());
        u.codes = m.codes.clone();
        u.set_hidden(RAW, &text[m.open_end..m.close_start]);
        u.set_hidden(LEAD, &m.lead);
        u.set_hidden(TRAIL, &m.trail);
        u.context.kind = Some("message".into());
        u.context.key = m.id.clone();
        u.context.location = Some(format!("message {}", mi + 1));
        if !m.note.is_empty() {
            u.context.notes.push(m.note.clone());
        }
        u.context.extra.insert("index".into(), mi.to_string());
        if opts.is_profile(profile::LOCOS) {
            // LocOS `_parse`: p_index is the message's position among every message of the bundle (a duplicate id
            // or a placeholder-only message is skipped but counted), so a skipped message never renumbers the rest
            u.context.extra.insert(profile::P_INDEX.into(), mi.to_string());
        }
        if let Some(d) = &m.desc {
            u.context.extra.insert("description".into(), d.clone());
        }
        if let Some(d) = &m.meaning {
            u.context.extra.insert("meaning".into(), d.clone());
        }
        parts.push(SkelPart::Raw(text[pos..m.open_end].to_string()));
        parts.push(SkelPart::Unit(units.len()));
        pos = m.close_start;
        units.push(u);
    }
    parts.push(SkelPart::Raw(text[pos..].to_string()));
    let doc = Document::new(parsed.kind.id(), units, Skeleton::text(parts, dec.encoding, bilingua_model::parser_info!())).with_options(opts);
    guard::finish(doc, dec.warnings, limits)
}

fn has_target(u: &Unit) -> bool {
    u.translate && u.segments.iter().any(|s| s.has_target())
}

fn raw_of(u: &Unit, key: &str) -> String {
    u.hidden(key).map(str::to_string).unwrap_or_default()
}

/// One unit of an XTB splice: the original content unless the translation differs from the source.
pub fn write_xtb_unit(u: &Unit, out: &mut String) {
    let raw = raw_of(u, RAW);
    if !has_target(u) {
        out.push_str(&raw);
        return;
    }
    let new = u.output();
    if new == u.source() {
        out.push_str(&raw);
        return;
    }
    out.push_str(&raw_of(u, LEAD));
    out.push_str(&render(&u.codes, &new, true));
    out.push_str(&raw_of(u, TRAIL));
}

/// Writes `lang` on the root start tag of the first skeleton part: replaces its value, or adds the attribute.
pub fn set_lang(head: &str, lang: &str) -> Result<String, Error> {
    let (_, _, st) = scan::root_tag(head).map_err(|e| Error::Write(format!("the skeleton is not an XTB any more: {e}")))?;
    let mut out = String::with_capacity(head.len() + lang.len() + 8);
    match st.attrs.iter().find(|a| a.name == "lang") {
        Some(a) => {
            out.push_str(&head[..a.vstart]);
            out.push_str(&escape_attr(lang, a.quote));
            out.push_str(&head[a.vend..]);
        }
        None => {
            out.push_str(&head[..st.name_end]);
            out.push_str(&format!(" lang=\"{}\"", escape_attr(lang, b'"')));
            out.push_str(&head[st.name_end..]);
        }
    }
    Ok(out)
}

/// A fresh XTB from an XMB: one `<translation>` per distinct message id, in XMB order, ids copied byte for byte.
pub fn xtb_from_xmb(original: &str, doc: &Document) -> Result<String, Error> {
    let parsed = read(original.to_string(), &Limits::default()).map_err(|e| Error::Write(format!("the skeleton is not an XMB any more: {e}")))?;
    if parsed.kind != Kind::Xmb {
        return Err(Error::Write("the skeleton is not an XMB".into()));
    }
    let idxs = unit_indices(&parsed.msgs);
    if idxs.len() != doc.units.len() {
        return Err(Error::Write("the units no longer match the skeleton".into()));
    }
    let unit_of: BTreeMap<usize, &Unit> = idxs.iter().copied().zip(doc.units.iter()).collect();
    let lang = doc.target_lang.as_deref().and_then(lang::html_lang);
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE translationbundle>\n");
    match &lang {
        Some(l) => out.push_str(&format!("<translationbundle lang=\"{}\">\n", escape_attr(l, b'"'))),
        None => out.push_str("<translationbundle>\n"),
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for (i, m) in parsed.msgs.iter().enumerate() {
        let Some(id) = m.id.as_deref() else { continue };
        if !seen.insert(id) {
            continue;
        }
        let body = match unit_of.get(&i) {
            Some(u) => render(&u.codes, &u.output(), false),
            None => render(&m.codes, &m.inl, false),
        };
        out.push_str(&format!("  <translation id=\"{}\">{}{}{}</translation>\n", escape_attr(id, b'"'), m.lead, body, m.trail));
    }
    out.push_str("</translationbundle>\n");
    Ok(out)
}
