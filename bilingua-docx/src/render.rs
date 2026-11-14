//! Writing a document back: the skeleton parts, with each unit's text rebuilt as Word runs.
//!
//! Rules:
//! - A unit whose output equals its source is written from the stored original XML, so an unchanged
//!   document keeps every XML part byte for byte (spell-check marks, rsids, tracked changes and all).
//! - A changed unit is rebuilt: the invisible marks that led the paragraph come first, then the runs made from
//!   the target (text in the formatting its tag stands for, base formatting outside tags, standalone tags
//!   written as they were), then the marks that trailed. Tracked changes in a changed paragraph are written as
//!   accepted text and a warning is added to the report.

use crate::extract::*;
use crate::xml::escape_text;
use bilingua_model::*;
use std::collections::{HashMap, HashSet};

/// What the writer wants the caller to know.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct MergeReport {
    pub warnings: Vec<String>,
}

pub struct Renderer<'a> {
    doc: &'a Document,
    ids: HashMap<&'a str, usize>,
    w: String,
    target_lang: Option<String>,
    pub warnings: Vec<String>,
    warned: HashSet<usize>,
}

/// How the runs of a changed unit are written.
#[derive(Clone, Copy)]
enum Runs<'k> {
    /// Word runs (`w:r`)
    Word,
    /// DrawingML runs (`a:r`)
    Dml,
    /// A DrawingML unit written into its Word fallback copy number `k` (`w:r` with the fallback's `w:rPr`)
    Mirror(&'k str),
}

struct Scope {
    base: String,
    fmts: Vec<String>,
    close: String,
}

impl<'a> Renderer<'a> {
    pub fn new(doc: &'a Document, w: String) -> Renderer<'a> {
        let ids = doc
            .units
            .iter()
            .enumerate()
            .map(|(i, u)| (u.id.as_str(), i))
            .collect();
        Renderer {
            doc,
            ids,
            w,
            target_lang: doc.target_lang.clone(),
            warnings: vec![],
            warned: HashSet::new(),
        }
    }

    pub fn unit(&mut self, idx: usize) -> Result<String, Error> {
        self.unit_body(idx, C_ORIG, 0)
    }

    fn unit_body(&mut self, idx: usize, orig_key: &str, depth: usize) -> Result<String, Error> {
        if depth > 16 {
            return Err(Error::Write("text boxes are nested too deeply".into()));
        }
        let doc = self.doc;
        let u = doc
            .units
            .get(idx)
            .ok_or_else(|| Error::Write(format!("unit {idx} does not exist")))?;
        let out = u.output();
        let changed = out != u.source();
        if !changed {
            let orig = u
                .codes
                .get(orig_key)
                .and_then(|c| c.standalone.as_deref())
                .ok_or_else(|| Error::Write(format!("unit {} lost its original XML", u.id)))?;
            return self.resolve(orig, depth);
        }
        if u.codes.contains_key(C_TRACKED) && self.warned.insert(idx) {
            self.warnings.push(format!(
                "{}: tracked changes were accepted because the text was translated",
                u.id
            ));
        }
        if u.codes.contains_key(C_SIMPLE) {
            return Ok(escape_text(&plain_text(&out)));
        }
        if u.codes.contains_key(C_DML) {
            let runs = match orig_key.strip_prefix("_m") {
                Some(k) => Runs::Mirror(k),
                None => Runs::Dml,
            };
            return self.rebuild(u, &out, depth, runs);
        }
        self.rebuild(u, &out, depth, Runs::Word)
    }

    fn code_str(u: &Unit, key: &str) -> String {
        u.codes
            .get(key)
            .and_then(|c| c.standalone.clone())
            .unwrap_or_default()
    }

    fn rebuild(&mut self, u: &Unit, out: &[Inline], depth: usize, runs: Runs) -> Result<String, Error> {
        let mut s = String::new();
        s.push_str(&self.resolve(&Self::code_str(u, C_LEAD), depth)?);
        let base = match runs {
            Runs::Mirror(k) => Self::code_str(u, &format!("_fb{k}")),
            _ => Self::code_str(u, C_BASE),
        };
        let mut scopes = vec![Scope {
            base,
            fmts: vec![],
            close: String::new(),
        }];
        for i in out {
            match i {
                Inline::Text(t) => {
                    let top = scopes.last().expect("scope stack is never empty");
                    let rpr = top.fmts.last().unwrap_or(&top.base).clone();
                    match runs {
                        Runs::Dml => self.push_dml(&mut s, t, &rpr),
                        _ => self.push_text(&mut s, t, &rpr),
                    }
                }
                Inline::Tag { id, kind, .. } => {
                    let Some(code) = u.codes.get(id) else {
                        continue;
                    };
                    let wrapper = code.standalone.is_some() && *kind != TagKind::Standalone;
                    match kind {
                        TagKind::Open if wrapper => {
                            s.push_str(&self.resolve(code.open.as_deref().unwrap_or(""), depth)?);
                            scopes.push(Scope {
                                base: code.standalone.clone().unwrap_or_default(),
                                fmts: vec![],
                                close: code.close.clone().unwrap_or_default(),
                            });
                        }
                        TagKind::Open => {
                            let top = scopes.last_mut().expect("scope stack is never empty");
                            let fmt = match runs {
                                // the fallback copy uses its own w:rPr for this tag, or its base
                                Runs::Mirror(k) => u
                                    .codes
                                    .get(&format!("_fb{k}.{id}"))
                                    .and_then(|c| c.standalone.clone())
                                    .unwrap_or_else(|| top.base.clone()),
                                _ => code.open.clone().unwrap_or_default(),
                            };
                            top.fmts.push(fmt);
                        }
                        TagKind::Close if wrapper => {
                            if scopes.len() > 1 {
                                let sc = scopes.pop().expect("checked above");
                                s.push_str(&sc.close);
                            }
                        }
                        TagKind::Close => {
                            scopes
                                .last_mut()
                                .expect("scope stack is never empty")
                                .fmts
                                .pop();
                        }
                        TagKind::Standalone => {
                            if let Some(x) = &code.standalone {
                                s.push_str(&self.resolve(x, depth)?);
                            }
                        }
                    }
                }
            }
        }
        while scopes.len() > 1 {
            let sc = scopes.pop().expect("checked above");
            s.push_str(&sc.close);
        }
        s.push_str(&self.resolve(&Self::code_str(u, C_TAIL), depth)?);
        Ok(s)
    }

    /// DrawingML runs: `a:t` keeps its spaces and tabs as they are; a line break is an `a:br`.
    fn push_dml(&self, s: &mut String, t: &str, rpr: &str) {
        let rpr = match &self.target_lang {
            Some(l) => set_dml_lang(rpr, l),
            None => rpr.to_string(),
        };
        for (i, line) in t.split('\n').enumerate() {
            if i > 0 {
                if rpr.is_empty() {
                    s.push_str("<a:br/>");
                } else {
                    s.push_str(&format!("<a:br>{rpr}</a:br>"));
                }
            }
            let line = line.replace('\r', "");
            if !line.is_empty() {
                s.push_str(&format!("<a:r>{rpr}<a:t>{}</a:t></a:r>", escape_text(&line)));
            }
        }
    }

    fn push_text(&self, s: &mut String, t: &str, rpr: &str) {
        let rpr = match &self.target_lang {
            Some(l) => set_lang(rpr, &self.w, l),
            None => rpr.to_string(),
        };
        let w = &self.w;
        let mut piece = String::new();
        let flush = |s: &mut String, piece: &mut String| {
            if piece.is_empty() {
                return;
            }
            let preserve = piece.starts_with(char::is_whitespace)
                || piece.ends_with(char::is_whitespace)
                || piece.contains("  ");
            let attr = if preserve {
                " xml:space=\"preserve\""
            } else {
                ""
            };
            s.push_str(&format!(
                "<{w}r>{rpr}<{w}t{attr}>{}</{w}t></{w}r>",
                escape_text(piece)
            ));
            piece.clear();
        };
        for c in t.chars() {
            match c {
                '\t' => {
                    flush(s, &mut piece);
                    s.push_str(&format!("<{w}r>{rpr}<{w}tab/></{w}r>"));
                }
                '\n' => {
                    flush(s, &mut piece);
                    s.push_str(&format!("<{w}r>{rpr}<{w}br/></{w}r>"));
                }
                '\r' => {}
                c => piece.push(c),
            }
        }
        flush(s, &mut piece);
    }

    /// Replaces unit markers with the rendered units.
    pub fn resolve(&mut self, s: &str, depth: usize) -> Result<String, Error> {
        if !s.contains(M_UNIT) && !s.contains(M_MIRROR) {
            return Ok(s.to_string());
        }
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(p) = rest.find([M_UNIT, M_MIRROR]) {
            out.push_str(&rest[..p]);
            let after = &rest[p..];
            let kind = after.chars().next().unwrap_or(M_UNIT);
            let body_start = kind.len_utf8();
            let end = after[body_start..]
                .find(M_END)
                .ok_or_else(|| Error::Write("a unit marker is not closed".into()))?;
            let body = &after[body_start..body_start + end];
            let piece = if kind == M_UNIT {
                let idx = *self
                    .ids
                    .get(body)
                    .ok_or_else(|| Error::Write(format!("unknown unit {body}")))?;
                self.unit_body(idx, C_ORIG, depth + 1)?
            } else {
                let (id, k) = body
                    .rsplit_once(':')
                    .ok_or_else(|| Error::Write("a mirror marker is malformed".into()))?;
                let idx = *self
                    .ids
                    .get(id)
                    .ok_or_else(|| Error::Write(format!("unknown unit {id}")))?;
                self.unit_body(idx, &format!("_m{k}"), depth + 1)?
            };
            out.push_str(&piece);
            rest = &after[body_start + end + M_END.len_utf8()..];
        }
        out.push_str(rest);
        Ok(out)
    }
}

fn primary(lang: &str) -> String {
    lang.split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Sets the language of a run's `w:lang` to the target language, when the run has one.
pub fn set_lang(rpr: &str, w: &str, lang: &str) -> String {
    let pat = format!("<{w}lang");
    let Some(p) = rpr.find(&pat) else {
        return rpr.to_string();
    };
    let Some(len) = rpr[p..].find('>') else {
        return rpr.to_string();
    };
    let tag = &rpr[p..p + len + 1];
    let prim = primary(lang);
    let mut new_tag = replace_attr(tag, &format!("{w}val"), lang);
    if matches!(prim.as_str(), "ja" | "zh" | "ko") {
        new_tag = replace_attr(&new_tag, &format!("{w}eastAsia"), lang);
    }
    if matches!(prim.as_str(), "ar" | "he" | "fa" | "ur") {
        new_tag = replace_attr(&new_tag, &format!("{w}bidi"), lang);
    }
    format!("{}{}{}", &rpr[..p], new_tag, &rpr[p + len + 1..])
}

/// Sets the `lang` attribute of a DrawingML `a:rPr` to the target language, when it has one.
pub fn set_dml_lang(rpr: &str, lang: &str) -> String {
    let Some(end) = rpr.find('>') else {
        return rpr.to_string();
    };
    format!("{}{}", replace_attr(&rpr[..=end], "lang", lang), &rpr[end + 1..])
}

fn replace_attr(tag: &str, name: &str, value: &str) -> String {
    for q in ['"', '\''] {
        let pat = format!(" {name}={q}");
        if let Some(p) = tag.find(&pat) {
            let vs = p + pat.len();
            if let Some(e) = tag[vs..].find(q) {
                return format!(
                    "{}{}{}",
                    &tag[..vs],
                    crate::xml::escape_text(value).replace('"', "&quot;"),
                    &tag[vs + e..]
                );
            }
        }
    }
    tag.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_is_replaced_where_it_exists() {
        let r = "<w:rPr><w:lang w:val=\"en-US\" w:eastAsia=\"ja-JP\"/></w:rPr>";
        assert_eq!(
            set_lang(r, "w:", "es-ES"),
            "<w:rPr><w:lang w:val=\"es-ES\" w:eastAsia=\"ja-JP\"/></w:rPr>"
        );
        assert_eq!(set_lang(r, "w:", "ja-JP"), r.replace("en-US", "ja-JP"));
        assert_eq!(
            set_lang("<w:rPr><w:b/></w:rPr>", "w:", "es"),
            "<w:rPr><w:b/></w:rPr>"
        );
    }
}
