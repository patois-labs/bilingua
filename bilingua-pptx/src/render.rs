//! Writing a document back. A paragraph whose output equals its source is written from the stored original
//! XML, so an unchanged presentation keeps every XML part byte for byte.

use crate::rich::*;
use crate::xml::{self, escape_text, Node};
use crate::*;
use std::collections::HashMap;

/// What the writer wants the caller to know.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub warnings: Vec<String>,
}

pub struct Renderer<'a> {
    doc: &'a Document,
    ids: HashMap<&'a str, usize>,
    target_lang: Option<String>,
    pub warnings: Vec<String>,
}

fn code_str(u: &Unit, key: &str) -> String {
    u.codes.get(key).and_then(|c| c.standalone.clone()).unwrap_or_default()
}

fn primary(lang: &str) -> String {
    lang.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase()
}

pub fn is_rtl_lang(lang: &str) -> bool {
    matches!(primary(lang).as_str(), "ar" | "he" | "fa" | "ur")
}

/// Run properties for a rebuilt run: spell-check marks dropped, `lang` set to the target language when the
/// run had one.
fn clean_rpr(rpr: &str, lang: Option<&str>) -> String {
    if rpr.is_empty() {
        return String::new();
    }
    let Some(end) = rpr.find('>') else {
        return rpr.to_string();
    };
    let (tag, rest) = rpr.split_at(end + 1);
    let mut tag = crate::extract::remove_attrs(tag, &["err", "dirty"]);
    if let Some(l) = lang {
        let pat = " lang=";
        if let Some(p) = tag.find(pat) {
            let vs = p + pat.len();
            if let Some(q) = tag[vs..].chars().next().filter(|c| *c == '"' || *c == '\'') {
                if let Some(e) = tag[vs + 1..].find(q) {
                    tag.replace_range(vs + 1..vs + 1 + e, &escape_text(l).replace('"', "&quot;"));
                }
            }
        }
    }
    format!("{tag}{rest}")
}

/// Runs for a text; a line feed becomes `a:br`.
fn runs_xml(a: &str, rpr: &str, text: &str, lang: Option<&str>, s: &mut String) {
    let rpr = clean_rpr(rpr, lang);
    let mut piece = String::new();
    let flush = |s: &mut String, piece: &mut String| {
        if !piece.is_empty() {
            s.push_str(&format!("<{a}r>{rpr}<{a}t>{}</{a}t></{a}r>", escape_text(piece)));
            piece.clear();
        }
    };
    for c in text.chars() {
        match c {
            '\n' => {
                flush(s, &mut piece);
                if rpr.is_empty() {
                    s.push_str(&format!("<{a}br/>"));
                } else {
                    s.push_str(&format!("<{a}br>{rpr}</{a}br>"));
                }
            }
            '\r' => {}
            c => piece.push(c),
        }
    }
    flush(s, &mut piece);
}

fn with_rtl(lead: &str, a: &str) -> String {
    if lead.is_empty() {
        return format!("<{a}pPr rtl=\"1\"/>");
    }
    if lead.contains(" rtl=") {
        return lead.to_string();
    }
    let p = lead[1..].find([' ', '/', '>']).map(|k| k + 1).unwrap_or(lead.len());
    format!("{} rtl=\"1\"{}", &lead[..p], &lead[p..])
}

impl<'a> Renderer<'a> {
    pub fn new(doc: &'a Document) -> Renderer<'a> {
        let ids = doc.units.iter().enumerate().map(|(i, u)| (u.id.as_str(), i)).collect();
        Renderer { doc, ids, target_lang: doc.target_lang.clone(), warnings: vec![] }
    }

    pub fn unit(&mut self, idx: usize) -> Result<String, Error> {
        let doc = self.doc;
        let u = doc.units.get(idx).ok_or_else(|| Error::Write(format!("unit {idx} does not exist")))?;
        let out = u.output();
        if out == u.source() {
            return Ok(code_str(u, C_ORIG));
        }
        let a = code_str(u, C_PFX);
        let lang = self.target_lang.clone();
        let mut lead = code_str(u, C_LEAD);
        if lang.as_deref().is_some_and(is_rtl_lang) {
            lead = with_rtl(&lead, &a);
        }
        let body = render(
            u,
            &out,
            |fmt, t, s| runs_xml(&a, fmt, t, lang.as_deref(), s),
            |x, s| s.push_str(x),
        );
        Ok(format!("{lead}{body}"))
    }

    /// Replaces markers with their text.
    pub fn resolve(&mut self, s: &str) -> Result<String, Error> {
        if !s.contains(M_MIRROR) {
            return Ok(s.to_string());
        }
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(p) = rest.find(M_MIRROR) {
            out.push_str(&rest[..p]);
            let after = &rest[p + M_MIRROR.len_utf8()..];
            let end = after.find(M_END).ok_or_else(|| Error::Write("a marker is not closed".into()))?;
            let id = &after[..end];
            let tail = &after[end + M_END.len_utf8()..];
            let oe = tail.find(M_ORIG).ok_or_else(|| Error::Write("a marker has no original text".into()))?;
            let orig = &tail[..oe];
            rest = &tail[oe + M_ORIG.len_utf8()..];
            let i = *self.ids.get(id).ok_or_else(|| Error::Write(format!("unknown unit {id}")))?;
            let u = &self.doc.units[i];
            let o = u.output();
            if o == u.source() {
                out.push_str(orig);
                continue;
            }
            // Keep the paragraph properties and the first run's properties of the drawing copy.
            let a = code_str(u, C_PFX);
            let mut lead = String::new();
            let mut rpr = String::new();
            if let Ok(nodes) = xml::parse(&format!("<x>{orig}</x>")) {
                if let Some(Node::El(root)) = nodes.first() {
                    for c in root.elements() {
                        if xml::local_name(&c.name) == "pPr" && lead.is_empty() {
                            lead = c.raw();
                        }
                        if xml::local_name(&c.name) == "r" && rpr.is_empty() {
                            if let Some(r) = c.elements().find(|x| xml::local_name(&x.name) == "rPr") {
                                rpr = r.raw();
                            }
                        }
                    }
                }
            }
            if self.target_lang.as_deref().is_some_and(is_rtl_lang) {
                lead = with_rtl(&lead, &a);
            }
            let text = plain_text(&o);
            out.push_str(&lead);
            runs_xml(&a, &rpr, &text, self.target_lang.as_deref(), &mut out);
        }
        out.push_str(rest);
        Ok(out)
    }
}
