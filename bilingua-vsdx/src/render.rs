//! Writing a document back. A unit whose output equals its source is written from its stored original markup, so
//! an unchanged drawing keeps every part byte for byte; a changed unit is rebuilt from the target. For a target
//! language, a translated shape also gets the language and (right to left) the paragraph direction.

use crate::lang;
use crate::xml::*;
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

fn code_str(u: &Unit, key: &str) -> String {
    u.codes.get(key).and_then(|c| c.standalone.clone()).unwrap_or_default()
}

pub fn merge(doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
    check_parser(doc, &parser_info!())?;
    let Skeleton::Zip { entries, .. } = &doc.skeleton else {
        return Err(Error::Write("vsdx needs a zip skeleton".into()));
    };
    let mut meta: Option<Meta> = None;
    let mut real = Vec::new();
    for e in entries {
        if e.name == META_NAME {
            if let ZipData::Raw(b) = &e.data {
                meta = Some(serde_json::from_slice::<Meta>(b).map_err(|e| Error::Write(format!("the vsdx skeleton's zip record is unreadable: {e}")))?);
            }
        } else {
            real.push(e);
        }
    }
    let mut r = Renderer::new(doc, opts);
    let mut out: Vec<(EntryMeta, bilingua_zip::Body)> = Vec::with_capacity(real.len());
    for e in real {
        let body = match &e.data {
            ZipData::Raw(b) => bilingua_zip::raw_body(meta.as_ref().map(|m| &m.packed), &e.name, b),
            ZipData::Parts(parts) => {
                let s = r.render_parts(parts)?;
                match meta.as_ref().and_then(|m| m.enc.get(&e.name)) {
                    Some(tag) => bilingua_zip::Body::Owned(bilingua_model::enc::encode_text_with(&s, tag, opts)?),
                    None => bilingua_zip::Body::Owned(s.into_bytes()),
                }
            }
        };
        let em = meta.as_ref().and_then(|m| m.entries.iter().find(|x| x.name == e.name).cloned()).unwrap_or(EntryMeta {
            name: e.name.clone(),
            stored: false,
            dir: e.name.ends_with('/'),
            modified: None,
        });
        out.push((em, body));
    }
    bilingua_zip::write_zip_parts(&out)
}

pub struct Renderer<'a> {
    doc: &'a Document,
    lcid: Option<u32>,
    rtl: bool,
    changed: Vec<Option<bool>>,
}

impl<'a> Renderer<'a> {
    /// The language is set on the shapes whose text changed, so `stamp_target_always` adds nothing (an unchanged
    /// shape is never rewritten).
    pub fn new(doc: &'a Document, opts: &bilingua_model::MergeOptions) -> Renderer<'a> {
        let tl = opts.stamp_lang(doc, true);
        Renderer { doc, lcid: tl.and_then(lang::lcid), rtl: tl.is_some_and(lang::is_rtl), changed: vec![None; doc.units.len()] }
    }

    fn unit_ref(&self, idx: usize) -> Result<&'a Unit, Error> {
        self.doc.units.get(idx).ok_or_else(|| Error::Write(format!("unit {idx} does not exist")))
    }

    fn is_changed(&mut self, idx: usize) -> Result<bool, Error> {
        let u = self.unit_ref(idx)?;
        if let Some(c) = self.changed[idx] {
            return Ok(c);
        }
        let c = u.output() != u.source();
        self.changed[idx] = Some(c);
        Ok(c)
    }

    /// Renders the skeleton parts of one entry: units in, region markers resolved.
    pub fn render_parts(&mut self, parts: &[SkelPart]) -> Result<String, Error> {
        let mut stack: Vec<(String, usize)> = Vec::new();
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Unit(i) => {
                    let s = self.unit(*i)?;
                    stack.last_mut().map(|t| &mut t.0).unwrap_or(&mut out).push_str(&s);
                }
                SkelPart::Raw(t) => {
                    let mut rest = t.as_str();
                    while let Some(p) = rest.find(M_OPEN) {
                        stack.last_mut().map(|t| &mut t.0).unwrap_or(&mut out).push_str(&rest[..p]);
                        let after = &rest[p + 1..];
                        let end = after.find(M_END).ok_or_else(|| Error::Write("a skeleton marker is not closed".into()))?;
                        let body = &after[..end];
                        rest = &after[end + 1..];
                        let (kind, arg) = body.split_at(body.char_indices().nth(1).map(|x| x.0).unwrap_or(body.len()));
                        match kind {
                            "N" => {
                                let idx: usize = arg.parse().map_err(|_| Error::Write("a skeleton marker is damaged".into()))?;
                                let s = self.name_u(idx)?;
                                stack.last_mut().map(|t| &mut t.0).unwrap_or(&mut out).push_str(&s);
                            }
                            "B" => {
                                let idx: usize = arg.parse().map_err(|_| Error::Write("a skeleton marker is damaged".into()))?;
                                self.unit_ref(idx)?;
                                stack.push((String::new(), idx));
                            }
                            "E" => {
                                let (buf, idx) = stack.pop().ok_or_else(|| Error::Write("a skeleton region ends without a start".into()))?;
                                let s = self.region(buf, idx)?;
                                stack.last_mut().map(|t| &mut t.0).unwrap_or(&mut out).push_str(&s);
                            }
                            _ => return Err(Error::Write("a skeleton marker is damaged".into())),
                        }
                    }
                    stack.last_mut().map(|t| &mut t.0).unwrap_or(&mut out).push_str(rest);
                }
            }
        }
        if !stack.is_empty() {
            return Err(Error::Write("a skeleton region is not closed".into()));
        }
        Ok(out)
    }

    /// ` NameU='...'` for a renamed page that had none (the universal name formulas refer to stays).
    fn name_u(&mut self, idx: usize) -> Result<String, Error> {
        if !self.is_changed(idx)? {
            return Ok(String::new());
        }
        let u = self.unit_ref(idx)?;
        let q = code_str(u, C_QUOTE).chars().next().unwrap_or('\'');
        Ok(format!(" NameU={q}{}{q}", escape_attr(&plain_text(&u.source()), q)))
    }

    pub fn unit(&mut self, idx: usize) -> Result<String, Error> {
        let u = self.unit_ref(idx)?;
        if !self.is_changed(idx)? {
            return Ok(code_str(u, C_ORIG));
        }
        let out = u.output();
        if code_str(u, C_KIND) == "attr" {
            let q = code_str(u, C_QUOTE).chars().next().unwrap_or('"');
            return Ok(escape_attr(&clean_target(&plain_text(&out)), q));
        }
        // run content: prefix, body, suffix
        let markers: BTreeMap<usize, &str> = u
            .codes
            .iter()
            .filter_map(|(k, c)| k.parse::<usize>().ok().and_then(|n| c.standalone.as_deref().map(|s| (n, s))))
            .collect();
        let mut used: BTreeSet<usize> = BTreeSet::new();
        let mut body = String::new();
        // CRLF files: only the translated text gets CRLF; markers are original markup and keep their own line ends
        let crlf = u.codes.contains_key(C_CRLF);
        for i in &out {
            match i {
                Inline::Text(t) => {
                    let t = escape_text(&clean_target(t));
                    body.push_str(&if crlf { t.replace('\n', "\r\n") } else { t });
                }
                Inline::Tag { id, .. } => {
                    if let Some(n) = id.parse::<usize>().ok().filter(|n| markers.contains_key(n) && !used.contains(n)) {
                        used.insert(n);
                        body.push_str(markers[&n]);
                    }
                }
            }
        }
        // a marker the translation dropped is appended, so no run property or field is lost
        for (n, m) in &markers {
            if !used.contains(n) {
                body.push_str(m);
            }
        }
        Ok(format!("{}{}{}", code_str(u, C_PRE), body, code_str(u, C_SUF)))
    }

    /// The part of a shape before its text, after the shape's text was translated.
    fn region(&mut self, buf: String, idx: usize) -> Result<String, Error> {
        if (self.lcid.is_none() && !self.rtl) || !self.is_changed(idx)? {
            return Ok(buf);
        }
        let u = self.unit_ref(idx)?;
        let mut p = parse_fragment(&buf, DEFAULT_MAX_DEPTH_CAP).map_err(|m| Error::Write(format!("a shape could not be read back: {m}")))?;
        if let Some(l) = self.lcid {
            stamp_langid(&mut p.nodes, l);
        }
        if self.rtl {
            let shape = code_str(u, C_SHAPE);
            let name = shape.get(1..).unwrap_or("").split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or("").to_string();
            let q = shape.bytes().find(|b| *b == b'"' || *b == b'\'').map(|b| b as char).unwrap_or('"');
            let mut used: BTreeSet<u32> = BTreeSet::new();
            used.insert(0);
            let mut raws: Vec<String> = vec![code_str(u, C_PRE), code_str(u, C_SUF)];
            raws.extend(u.codes.iter().filter(|(k, _)| k.parse::<usize>().is_ok()).filter_map(|(_, c)| c.standalone.clone()));
            for r in raws {
                if let Ok(f) = parse_fragment(&r, DEFAULT_MAX_DEPTH_CAP) {
                    collect_pp(&f.nodes, &mut used);
                }
            }
            stamp_rtl(&mut p.nodes, prefix_of(&name), q, &used);
        }
        Ok(raw_of(&p.nodes))
    }
}

fn collect_pp(nodes: &[Node], used: &mut BTreeSet<u32>) {
    for n in nodes {
        if let Node::El(e) = n {
            if e.local() == "pp" {
                if let Some(ix) = e.attr("IX").filter(|i| !i.is_empty() && i.bytes().all(|b| b.is_ascii_digit())).and_then(|i| i.parse().ok()) {
                    used.insert(ix);
                }
            }
            collect_pp(&e.children, used);
        }
    }
}

/// The translated shape's literal Character `LangID` cells declare the target language. A cell with a formula, or
/// one the shape does not have (it inherits), is left alone.
fn stamp_langid(nodes: &mut [Node], lcid: u32) {
    for n in nodes {
        let Node::El(sec) = n else { continue };
        if sec.local() != "Section" || sec.attr("N").as_deref() != Some("Character") {
            continue;
        }
        for r in sec.children.iter_mut() {
            let Node::El(row) = r else { continue };
            for c in row.children.iter_mut() {
                let Node::El(cell) = c else { continue };
                if cell.local() == "Cell" && cell.attr("N").as_deref() == Some("LangID") && !cell.has_attr("F") {
                    cell.set_attr("V", &lcid.to_string());
                }
            }
        }
    }
}

fn new_el(pfx: &str, local: &str, attrs: &[(&str, String)], q: char, selfclose: bool) -> El {
    let name = if pfx.is_empty() { local.to_string() } else { format!("{pfx}:{local}") };
    let mut start = format!("<{name}");
    for (k, v) in attrs {
        start.push_str(&format!(" {k}={q}{}{q}", escape_attr(v, q)));
    }
    if selfclose {
        start.push_str("/>");
        El { name, start, ..Default::default() }
    } else {
        start.push('>');
        El { end: format!("</{name}>"), name, start, ..Default::default() }
    }
}

/// Right to left target: the Paragraph section `Flags` cell is the paragraph direction (1 = right to left). In
/// every Paragraph row the translated text uses (row 0 and every `<pp IX>`) a literal Flags cell becomes 1 and a
/// missing one is added; a cell with a real formula stays, `F="Inh"` (only inherited) becomes a local 1. A used
/// row the shape lacks is added as a sparse row holding only Flags, and a Paragraph section is added when the
/// shape has none. Alignment is not mirrored.
fn stamp_rtl(nodes: &mut Vec<Node>, pfx: &str, q: char, used: &BTreeSet<u32>) {
    let pos_of = |nodes: &[Node], f: &dyn Fn(&El) -> bool| nodes.iter().position(|n| matches!(n, Node::El(e) if f(e)));
    let is_para = |e: &El| e.local() == "Section" && e.attr("N").as_deref() == Some("Paragraph");
    let sec_pos = match pos_of(nodes, &is_para) {
        Some(p) => {
            if matches!(&nodes[p], Node::El(e) if e.attr("Del").as_deref() == Some("1")) {
                return;
            }
            p
        }
        None => {
            let sec = new_el(pfx, "Section", &[("N", "Paragraph".into())], q, false);
            let anchor = nodes.iter().rposition(|n| matches!(n, Node::El(e) if e.local() == "Section" && e.attr("N").as_deref() == Some("Character")));
            let at = match anchor {
                Some(a) => a + 1,
                None => nodes.iter().rposition(|n| matches!(n, Node::El(e) if matches!(e.local(), "Cell" | "Trigger" | "Section"))).map(|p| p + 1).unwrap_or(0),
            };
            nodes.insert(at, Node::El(sec));
            at
        }
    };
    let Node::El(sec) = &mut nodes[sec_pos] else { return };
    // existing rows by IX (a missing or non-numeric IX is the row's position), sorted by IX
    let mut have: BTreeMap<u32, usize> = BTreeMap::new();
    let mut row_no = 0u32;
    for (pos, c) in sec.children.iter().enumerate() {
        if let Node::El(row) = c {
            if row.local() == "Row" {
                let ix = row.attr("IX").filter(|i| !i.is_empty() && i.bytes().all(|b| b.is_ascii_digit())).and_then(|i| i.parse().ok()).unwrap_or(row_no);
                have.insert(ix, pos);
                row_no += 1;
            }
        }
    }
    for (ix, pos) in have.clone() {
        if !used.contains(&ix) {
            continue;
        }
        let Node::El(row) = &mut sec.children[pos] else { continue };
        if row.attr("Del").as_deref() == Some("1") {
            continue;
        }
        let flags = row.children.iter_mut().find_map(|c| match c {
            Node::El(c) if c.local() == "Cell" && c.attr("N").as_deref() == Some("Flags") => Some(c),
            _ => None,
        });
        match flags {
            None => row.children.push(Node::El(new_el(pfx, "Cell", &[("N", "Flags".into()), ("V", "1".into())], q, true))),
            Some(c) => {
                let f = c.attr("F");
                if f.is_none() || f.as_deref() == Some("Inh") {
                    c.remove_attr("F");
                    c.set_attr("V", "1");
                }
            }
        }
    }
    let missing: Vec<u32> = used.iter().filter(|i| !have.contains_key(i)).copied().collect();
    for ix in &missing {
        let mut row = new_el(pfx, "Row", &[("IX", ix.to_string())], q, false);
        row.children.push(Node::El(new_el(pfx, "Cell", &[("N", "Flags".into()), ("V", "1".into())], q, true)));
        let at = have.range(..*ix).next_back().map(|(_, p)| p + 1).unwrap_or(0);
        sec.children.insert(at, Node::El(row));
        for p in have.values_mut() {
            if *p >= at {
                *p += 1;
            }
        }
        have.insert(*ix, at);
    }
}
