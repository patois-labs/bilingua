//! Bilingua ICML filter: Adobe InCopy ICML story files and WoodWing WCML articles. See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod dom;
mod lang;
mod parse;
mod rebuild;
mod walk;

use bilingua_model::guard::{LimitError, LimitKind};
use bilingua_model::*;
use dom::{Dom, Id, Kind, NIL};
use rebuild::TP;
use std::collections::{BTreeSet, HashMap};
use walk::*;

pub use bilingua_model::Limits;

/// The ICML / WCML filter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Icml;

const FORMAT: &str = "icml";
/// Hidden code holding the paragraph number of a unit.
const PIDX: &str = "p";

fn parse_err(msg: impl Into<String>) -> Error {
    Error::Parse { format: FORMAT.into(), msg: msg.into() }
}

/// A parsed file: the tree, the stories in document order (each with the document that declares its
/// conditions) and the hidden conditions of every such document.
struct Prep {
    dom: Dom,
    stories: Vec<(Id, Id)>,
    hidden: HashMap<Id, BTreeSet<String>>,
    /// The root is a bare `<Story>` (read natively; LocOS refuses it, so the LocOS profile does too).
    bare_story: bool,
}

fn find_doctype(b: &[u8]) -> bool {
    let ci = |r: &[u8], w: &[u8]| r.len() >= w.len() && r[..w.len()].eq_ignore_ascii_case(w);
    let mut i = 0;
    while i + 2 < b.len() {
        if b[i] == b'<' && b[i + 1] == b'!' {
            let mut j = i + 2;
            while j < b.len() && matches!(b[j], b' ' | b'\t' | b'\n' | b'\r') {
                j += 1;
            }
            if ci(&b[j..], b"DOCTYPE") || ci(&b[j..], b"ENTITY") {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn utf16_like(b: &[u8]) -> bool {
    b.starts_with(&[0xFF, 0xFE]) || b.starts_with(&[0xFE, 0xFF]) || b.starts_with(&[b'<', 0]) || b.starts_with(&[0, b'<'])
}

/// Bytes to text: UTF-8 only (one BOM allowed). Returns the text without the BOM and whether there was one.
fn decode(bytes: &[u8]) -> Result<(String, bool), Error> {
    if utf16_like(bytes) {
        return Err(Error::Encoding("This InCopy file is UTF-16; ICML is UTF-8. Re-save it from InCopy.".into()));
    }
    let (body, bom) = match bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(r) => (r, true),
        None => (bytes, false),
    };
    match std::str::from_utf8(body) {
        Ok(s) => Ok((s.to_string(), bom)),
        Err(_) => Err(Error::Encoding("This InCopy file is not valid UTF-8 and declares no other encoding. Re-save it from InCopy.".into())),
    }
}

fn prepare_text(text: String, limits: &Limits) -> Result<Prep, Error> {
    let b = text.as_bytes();
    guard::check_xml(b, limits)?;
    if find_doctype(b) {
        return Err(parse_err("This InCopy file declares a DOCTYPE or entity; ICML never needs one."));
    }
    let budget = limits.max_units.saturating_mul(40);
    if b.iter().filter(|c| **c == b'<').count() > budget {
        return Err(LimitError::new(
            LimitKind::XmlElements,
            format!("This InCopy file has far more markup than the {} unit limit allows. Split the document.", limits.max_units),
        )
        .into());
    }
    let depth = limits.max_xml_depth.min(1000);
    let parsed = match parse::parse(&text, depth) {
        Ok(p) => p,
        Err(parse::PErr::Syntax(m)) => return Err(parse_err(format!("This file is not well-formed InCopy XML: {m}"))),
        Err(parse::PErr::Depth(d)) => {
            return Err(LimitError::new(LimitKind::XmlDepth, format!("This file nests XML elements more than {d} deep. It was not processed.")).into())
        }
    };
    if let Some(e) = &parsed.decl_encoding {
        let up = e.to_uppercase().replace('_', "-");
        if up != "UTF-8" && up != "UTF8" {
            return Err(Error::Encoding(format!("This InCopy file is encoded as {e}; ICML is UTF-8. Re-save it from InCopy.")));
        }
    }
    let dom = Dom { text, nodes: parsed.nodes, uris: parsed.uris, root: parsed.root };
    let root = dom.root;
    let is_doc = dom.local(root) == "Document";
    let woodwing = dom.local(root) == "Stories" && dom.ns_uri(root).starts_with("urn:SmartConnection");
    // a bare `<Story>` root: an InCopy story exported on its own (LocOS refuses it; Okapi reads it)
    let bare_story = dom.is_plain(root, "Story");
    if !is_doc && !woodwing && !bare_story {
        return Err(parse_err("This XML file is not an InCopy or WoodWing document (its root is neither <Document>, <Story> nor WoodWing's <ea:Stories>)."));
    }
    let stories = find_stories(&dom);
    if stories.is_empty() {
        return Err(parse_err("This InCopy document holds no <Story>."));
    }
    let mut hidden: HashMap<Id, BTreeSet<String>> = HashMap::new();
    for (_, d) in &stories {
        if !hidden.contains_key(d) {
            hidden.insert(*d, hidden_conditions(&dom, *d));
        }
    }
    Ok(Prep { dom, stories, hidden, bare_story })
}

/// Every plain `<Story>` not inside another one, in document order; in a wrapped (WoodWing) file only those
/// inside a snippet `<Document>`.
fn find_stories(dom: &Dom) -> Vec<(Id, Id)> {
    let wrapped = !dom.is_plain(dom.root, "Document") && !dom.is_plain(dom.root, "Story");
    let mut out = Vec::new();
    let mut stack: Vec<(Id, bool, Id)> = vec![(dom.root, false, NIL)];
    while let Some((el, in_story, doc)) = stack.pop() {
        let is_story = dom.is_plain(el, "Story");
        let is_doc = dom.is_plain(el, "Document");
        if is_story && !in_story && (!wrapped || doc != NIL) {
            out.push((el, if doc != NIL { doc } else { dom.root }));
        }
        let doc_here = if is_doc { el } else { doc };
        for c in dom.n(el).children.iter().rev() {
            if dom.is_elem(*c) {
                stack.push((*c, in_story || is_story, doc_here));
            }
        }
    }
    out
}

fn hidden_conditions(dom: &Dom, doc: Id) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![doc];
    while let Some(el) = stack.pop() {
        if dom.local(el) == "Condition" && dom.attr(el, "Visible").is_some_and(|v| v.to_lowercase() == "false") {
            if let Some(s) = dom.attr(el, "Self").filter(|s| !s.is_empty()) {
                out.insert(s.to_string());
            }
        }
        stack.extend(dom.n(el).children.iter().copied().filter(|c| dom.is_elem(*c)));
    }
    out
}

fn declared_languages(dom: &Dom) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![dom.root];
    while let Some(el) = stack.pop() {
        if dom.local(el) == "Language" {
            if let Some(n) = dom.attr(el, "Name").filter(|n| !n.is_empty()) {
                out.insert(n.to_string());
            }
        }
        stack.extend(dom.n(el).children.iter().copied().filter(|c| dom.is_elem(*c)));
    }
    out
}

fn clip(s: &str) -> String {
    s.chars().take(160).collect()
}

/// The start tag of an element as written (or `<name>` for an element made in memory).
fn open_text(dom: &Dom, id: Id) -> String {
    let n = dom.n(id);
    match (n.kind, n.src) {
        (Kind::Elem, Some((s, _))) if n.start_end > 0 => clip(&dom.text[s as usize..n.start_end as usize]),
        (Kind::Elem, _) => format!("<{}>", n.name),
        (_, Some((s, e))) => clip(&dom.text[s as usize..e as usize]),
        _ => String::new(),
    }
}

fn point_label(dom: &Dom, obj: &Pt) -> (&'static str, String) {
    match obj {
        Pt::Ch(c) => (
            match c.as_str() {
                "\t" => "tab",
                "\u{2028}" => "line-break",
                _ => "paragraph-separator",
            },
            c.clone(),
        ),
        Pt::Node(n) => {
            let label = if dom.is_pi(*n) {
                "special-char"
            } else {
                match dom.local(*n) {
                    "Br" => "break",
                    "PageReference" => "index-marker",
                    "Footnote" => "footnote",
                    "Table" => "table",
                    "Note" => "note",
                    "HiddenText" => "hidden-text",
                    "TextVariableInstance" => "text-variable",
                    "Rectangle" | "Oval" | "Polygon" | "GraphicLine" | "TextFrame" | "Group" => "anchored-frame",
                    _ => "point",
                }
            };
            (label, open_text(dom, *n))
        }
    }
}

fn wrap_label(dom: &Dom, el: Id) -> &'static str {
    match dom.local(el) {
        "HyperlinkTextSource" => "link",
        "XMLElement" => "xml-tag",
        "Change" => "tracked-change",
        _ => "range",
    }
}

fn unit_for(dom: &Dom, tok: &Tok, para: &Para, p_index: usize, story: Id) -> Unit {
    let (_lead, core, _trail) = split_ws(&tok.parts);
    let mut inl: Vec<Inline> = Vec::new();
    let mut codes: std::collections::BTreeMap<String, CodeData> = Default::default();
    for part in &core {
        match part {
            Part::Text(t) => inl.push(Inline::text(t.clone())),
            Part::ROpen(n) | Part::WOpen(n) => {
                let (label, el, run) = match &tok.reg[n - 1] {
                    Entry::Run(r) => ("run", para.runs[*r].csr, true),
                    Entry::Wrap { el, .. } => (wrap_label(dom, *el), *el, false),
                    _ => continue,
                };
                let _ = run;
                let open = open_text(dom, el);
                let close = format!("</{}>", dom.n(el).name);
                inl.push(Inline::tag(n.to_string(), TagKind::Open, open.clone()));
                codes.insert(n.to_string(), CodeData::new().with_open(open).with_close(close).with_label(label.to_string()));
            }
            Part::RClose(n) | Part::WClose(n) => {
                let el = match &tok.reg[n - 1] {
                    Entry::Run(r) => para.runs[*r].csr,
                    Entry::Wrap { el, .. } => *el,
                    _ => continue,
                };
                inl.push(Inline::tag(n.to_string(), TagKind::Close, format!("</{}>", dom.n(el).name)));
            }
            Part::Point(n) => {
                let Entry::Point { obj, .. } = &tok.reg[n - 1] else { continue };
                let (label, display) = point_label(dom, obj);
                inl.push(Inline::tag(n.to_string(), TagKind::Standalone, display.clone()));
                codes.insert(n.to_string(), CodeData::new().with_standalone(display).with_label(label.to_string()));
            }
        }
    }
    let mut u = Unit::new(format!("u{p_index}"), merge_text(inl));
    u.codes = codes;
    u.set_hidden(PIDX, p_index.to_string());
    u.context.kind = Some("paragraph".into());
    let style = style_of(dom, para);
    if !style.is_empty() {
        u.context.style = Some(style);
    }
    u.context.key = dom.attr(story, "Self").map(str::to_string);
    u.context.location = Some(format!("story {} paragraph {}", dom.attr(story, "Self").unwrap_or(""), p_index));
    u
}

impl Icml {
    /// Like [`Filter::extract`] with explicit limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        guard::check_input(bytes, limits)?;
        let (text, bom) = decode(bytes)?;
        let mut prep = prepare_text(text, limits)?;
        // the LocOS profile walks as LocOS does: no table cells straight under a story or paragraph range
        let block_tables = !opts.is_profile(bilingua_model::profile::LOCOS);
        if prep.bare_story && !block_tables {
            // LocOS `icml_filter._parse` reads only a `<Document>` or WoodWing `<ea:Stories>` root
            return Err(parse_err("This XML file is not an InCopy or WoodWing document (its root is neither <Document> nor WoodWing's <ea:Stories>)."));
        }
        let mut units: Vec<Unit> = Vec::new();
        let mut idx = 0usize;
        let stories = prep.stories.clone();
        for (story, d) in stories {
            let hidden = prep.hidden.get(&d).cloned().unwrap_or_default();
            let paras = walk(&mut prep.dom, &hidden, story, block_tables);
            for para in &paras {
                let tok = tokenize(&prep.dom, para);
                if has_text(&tok.parts) {
                    guard::check_unit_count(units.len() + 1, limits)?;
                    units.push(unit_for(&prep.dom, &tok, para, idx, story));
                }
                idx += 1;
            }
        }
        let doc = Document::new(FORMAT, units, Skeleton::text(
                vec![SkelPart::Raw(std::mem::take(&mut prep.dom.text))],
                if bom { "utf-8-bom" } else { "utf-8" },
                bilingua_model::parser_info!(),
            )).with_options(opts);
        guard::finish(doc, Vec::new(), limits)
    }
}

fn p_index_of(u: &Unit) -> Option<usize> {
    u.hidden(PIDX).and_then(|s| s.parse().ok())
}

/// A target as translated pieces; XML-illegal characters dropped, line ends turned into spaces (a newline is
/// never a paragraph break here). `keep_nl`: the paragraph's source text has literal line feeds in its `Content`
/// (a code block written by pandoc, for instance), so a line feed in the target stays one instead of being
/// flattened to a space. `locos`: every line end becomes InDesign's forced line break (U+2028), as LocOS writes it.
fn to_tp(v: &[Inline], keep_nl: bool, locos: bool) -> Vec<TP> {
    fn clean(t: &str, keep_nl: bool, locos: bool) -> String {
        let eol = if locos { '\u{2028}' } else if keep_nl { '\n' } else { ' ' };
        let mut s = String::with_capacity(t.len());
        let mut it = t.chars().peekable();
        while let Some(c) = it.next() {
            let c = if matches!(c, '\u{0B}' | '\u{0C}') { '\n' } else { c };
            if matches!(c as u32, 0..=8 | 0x0E..=0x1F | 0xFFFE | 0xFFFF) {
                continue;
            }
            if c == '\r' {
                if it.peek() == Some(&'\n') {
                    it.next();
                }
                s.push(eol);
            } else if c == '\n' {
                s.push(eol);
            } else {
                s.push(c);
            }
        }
        s
    }
    let mut out = Vec::new();
    for i in v {
        match i {
            Inline::Text(t) => out.push(TP::Text(clean(t, keep_nl, locos))),
            Inline::Tag { id, kind: TagKind::Open, .. } => out.push(TP::Open(id.clone())),
            Inline::Tag { id, kind: TagKind::Close, .. } => out.push(TP::Close(id.clone())),
            Inline::Tag { id, kind: TagKind::Standalone, .. } => out.push(TP::Point(id.clone())),
        }
    }
    out
}

/// Keeps the paragraph's own leading and trailing white space around the translation.
fn fit_ws(tp: &mut Vec<TP>, lead: &str, trail: &str) {
    if !lead.is_empty() {
        let starts = matches!(tp.first(), Some(TP::Text(t)) if t.starts_with(lead));
        if !starts {
            if let Some(TP::Text(t)) = tp.first_mut() {
                *t = t.trim_start_matches(py_space).to_string();
            }
            tp.insert(0, TP::Text(lead.to_string()));
        }
    }
    if !trail.is_empty() {
        let ends = matches!(tp.last(), Some(TP::Text(t)) if t.ends_with(trail));
        if !ends {
            if let Some(TP::Text(t)) = tp.last_mut() {
                *t = t.trim_end_matches(py_space).to_string();
            }
            tp.push(TP::Text(trail.to_string()));
        }
    }
}

/// LocOS `base._newline_style`: `\r\n` when every line end of `text` is CRLF, `\r` when there are only bare CRs,
/// else `None` (LF, or mixed: the piece is left as it is).
fn newline_style(text: &str) -> Option<&'static str> {
    let b = text.as_bytes();
    let crlf = b.windows(2).filter(|w| *w == b"\r\n").count();
    let lf = b.iter().filter(|&&c| c == b'\n').count();
    let cr = b.iter().filter(|&&c| c == b'\r').count();
    if crlf > 0 && crlf == lf && lf == cr {
        Some("\r\n")
    } else if cr > 0 && lf == 0 {
        Some("\r")
    } else {
        None
    }
}

/// Every line end of `piece` (`\r\n`, `\r` or `\n`) written as `nl` (LocOS `newline_like`).
fn with_newlines(piece: &str, nl: &str) -> String {
    let mut out = String::with_capacity(piece.len() + piece.len() / 32);
    let mut it = piece.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\r' => {
                if it.peek() == Some(&'\n') {
                    it.next();
                }
                out.push_str(nl);
            }
            '\n' => out.push_str(nl),
            c => out.push(c),
        }
    }
    out
}

fn skeleton_text(doc: &Document) -> Result<(String, bool), Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write("icml needs a text skeleton".into()));
    };
    let mut text = String::new();
    for p in parts {
        if let SkelPart::Raw(s) = p {
            text.push_str(s);
        }
    }
    Ok((text, encoding == "utf-8-bom"))
}

impl Filter for Icml {
    fn id(&self) -> &'static str {
        FORMAT
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["icml", "wcml"]
    }

    /// Tight: an XML file whose first 8 KB names the InCopy interchange snippet type or WoodWing's Smart
    /// Connection namespace.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(8192)];
        let head = head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head);
        let h = String::from_utf8_lossy(head);
        h.trim_start().starts_with('<') && (h.contains("InCopyInterchange") || h.contains("urn:SmartConnection"))
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        self.extract_with(bytes, opts, &opts.limits())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let (text, bom) = skeleton_text(doc)?;
        let finish = |s: &str| -> Vec<u8> {
            let mut v = Vec::with_capacity(s.len() + 3);
            if bom {
                v.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            }
            v.extend_from_slice(s.as_bytes());
            v
        };
        let mut wanted: HashMap<usize, Vec<TP>> = HashMap::new();
        for u in &doc.units {
            if !u.translate || u.segments.iter().all(|s| !s.has_target()) {
                continue;
            }
            let Some(p) = p_index_of(u) else { continue };
            let keep_nl = u.source().iter().any(|i| matches!(i, Inline::Text(t) if t.contains('\n')));
            wanted.insert(p, to_tp(&u.output(), keep_nl, opts.is_profile(bilingua_model::profile::LOCOS)));
        }
        if wanted.is_empty() {
            return Ok(finish(&text));
        }
        let limits = Limits::default();
        let mut prep = prepare_text(text, &limits)?;
        // the walk the document was extracted with (the profile's has no block-level tables)
        let block_tables = !opts.is_profile(bilingua_model::profile::LOCOS);
        // the language is set per rewritten (translated) paragraph, so `stamp_target_always` adds nothing; ICML is
        // UTF-8 XML, so `output_encoding` has nothing to change either
        let tag = opts.stamp_lang(doc, true).and_then(lang::html_lang);
        let language = tag.as_deref().and_then(|t| lang::indesign_language(&declared_languages(&prep.dom), t));
        let rtl = tag.as_deref().is_some_and(lang::is_rtl);
        let mut changes: Vec<(usize, usize, String)> = Vec::new();
        let mut idx = 0usize;
        let stories = prep.stories.clone();
        for (story, d) in stories {
            let hidden = prep.hidden.get(&d).cloned().unwrap_or_default();
            let dom = &mut prep.dom;
            let paras = walk(dom, &hidden, story, block_tables);
            let n = paras.len();
            if (idx..idx + n).any(|k| wanted.contains_key(&k)) {
                normalize(dom, &paras);
                let paras2 = walk(dom, &hidden, story, block_tables);
                if paras2.len() != n {
                    return Err(Error::Write("story paragraph structure changed on rebuild".into()));
                }
                let mut plans = Vec::new();
                for (k, para) in paras2.iter().enumerate() {
                    let Some(new) = wanted.get(&(idx + k)) else { continue };
                    let tok = tokenize(dom, para);
                    if !has_text(&tok.parts) {
                        continue;
                    }
                    let (lead, _core, trail) = split_ws(&tok.parts);
                    let mut tp = new.clone();
                    fit_ws(&mut tp, &lead, &trail);
                    plans.push((para, tok, tp));
                }
                let mut touched = false;
                for (para, tok, tp) in &plans {
                    let ranges = rebuild::rebuild(dom, para, tok, tp);
                    if let Some(l) = &language {
                        rebuild::stamp(dom, &ranges, l);
                    }
                    if rtl {
                        rebuild::stamp_rtl(dom, &ranges);
                    }
                    touched = true;
                }
                if touched {
                    if let Some((s, e)) = dom.n(story).src {
                        let mut out = String::new();
                        if opts.is_profile(bilingua_model::profile::LOCOS) {
                            // LocOS re-serialises the story with lxml: start tags and empty elements are regenerated
                            dom.ser_lxml(story, &mut out);
                        } else {
                            dom.ser(story, &mut out, false);
                        }
                        changes.push((s as usize, e as usize, out));
                    }
                }
            }
            idx += n;
        }
        if let Some(bad) = wanted.keys().find(|k| **k >= idx) {
            return Err(Error::Write(format!("unit points at paragraph {bad}, but the file has {idx}")));
        }
        if changes.is_empty() {
            return Ok(finish(&prep.dom.text));
        }
        changes.sort_by_key(|c| c.0);
        let src = &prep.dom.text;
        // LocOS `newline_like`: a rebuilt story takes the file's line endings when it uses one style throughout
        // (CRLF from InCopy on Windows), so no bare LF lands in a CRLF file
        if let Some(nl) = newline_style(src) {
            for c in &mut changes {
                c.2 = with_newlines(&c.2, nl);
            }
        }
        let mut out = String::with_capacity(src.len() + 1024);
        let mut pos = 0usize;
        for (s, e, new) in &changes {
            out.push_str(&src[pos..*s]);
            out.push_str(new);
            pos = *e;
        }
        out.push_str(&src[pos..]);
        Ok(finish(&out))
    }
}
