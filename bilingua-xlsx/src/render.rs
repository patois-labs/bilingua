//! Writing a document back. A unit whose output equals its source is written from the stored original XML, so
//! an unchanged workbook keeps every XML part byte for byte.

use crate::refs::quote_sheet;
use crate::rich::*;
use crate::xml::escape_text;
use crate::*;
use std::collections::{HashMap, HashSet};

/// What the writer wants the caller to know.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub warnings: Vec<String>,
}

pub struct Renderer<'a> {
    doc: &'a Document,
    ids: HashMap<&'a str, usize>,
    /// Final sheet names by unit id.
    names: HashMap<String, String>,
    pub warnings: Vec<String>,
    warned: HashSet<String>,
}

fn attr_escape(s: &str) -> String {
    escape_text(s).replace('"', "&quot;")
}

fn code_str(u: &Unit, key: &str) -> String {
    u.codes.get(key).and_then(|c| c.standalone.clone()).unwrap_or_default()
}

/// Excel refuses sheet names with : \ / ? * [ ], longer than 31 characters, or with a leading or trailing '.
fn clean_sheet_name(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']') || c.is_control() { '-' } else { c })
        .collect();
    let t = t.trim().trim_matches('\'').to_string();
    t.chars().take(31).collect()
}

impl<'a> Renderer<'a> {
    pub fn new(doc: &'a Document) -> Renderer<'a> {
        let ids = doc.units.iter().enumerate().map(|(i, u)| (u.id.as_str(), i)).collect();
        let mut r = Renderer { doc, ids, names: HashMap::new(), warnings: vec![], warned: HashSet::new() };
        r.sheet_names();
        r
    }

    fn sheet_names(&mut self) {
        let mut taken: HashSet<String> = HashSet::new();
        let mut finals: Vec<(String, String)> = vec![];
        let units: Vec<&Unit> = self.doc.units.iter().filter(|u| u.id.starts_with("sheetname-")).collect();
        // Names that stay as they are are taken first, so a new name cannot collide with them.
        for u in &units {
            let src = plain_text(&u.source());
            if plain_text(&u.output()) == src {
                taken.insert(src.to_lowercase());
                finals.push((u.id.clone(), src));
            }
        }
        for u in &units {
            let src = plain_text(&u.source());
            let out = plain_text(&u.output());
            if out == src {
                continue;
            }
            let cleaned = clean_sheet_name(&out);
            let ok = !cleaned.is_empty() && !taken.contains(&cleaned.to_lowercase());
            if ok {
                if cleaned != out {
                    self.warnings.push(format!("{}: the sheet name was changed to \"{cleaned}\" because Excel does not allow the original", u.id));
                }
                taken.insert(cleaned.to_lowercase());
                finals.push((u.id.clone(), cleaned));
            } else {
                self.warnings.push(format!("{}: the sheet name \"{out}\" is empty or already used, so the source name was kept", u.id));
                taken.insert(src.to_lowercase());
                finals.push((u.id.clone(), src));
            }
        }
        self.names = finals.into_iter().collect();
    }

    pub fn unit(&mut self, idx: usize) -> Result<String, Error> {
        let doc = self.doc;
        let u = doc.units.get(idx).ok_or_else(|| Error::Write(format!("unit {idx} does not exist")))?;
        let out = u.output();
        if out == u.source() {
            return Ok(code_str(u, C_ORIG));
        }
        let pfx = code_str(u, crate::C_PFX);
        match code_str(u, C_MODE).as_str() {
            "si" => {
                if u.codes.contains_key("_ph") && self.warned.insert(u.id.clone()) {
                    self.warnings.push(format!("{}: phonetic guides were removed because the text was translated", u.id));
                }
                let runs = out.iter().any(|i| matches!(i, Inline::Tag { .. })) || !code_str(u, C_BASE).is_empty();
                let p = pfx.clone();
                Ok(render(
                    u,
                    &out,
                    |fmt, t, s| {
                        let t = t.replace('\r', "");
                        let preserve = t.starts_with(char::is_whitespace) || t.ends_with(char::is_whitespace) || t.contains("  ") || t.contains('\n');
                        let attr = if preserve { " xml:space=\"preserve\"" } else { "" };
                        if runs {
                            s.push_str(&format!("<{p}r>{fmt}<{p}t{attr}>{}</{p}t></{p}r>", escape_text(&t)));
                        } else {
                            s.push_str(&format!("<{p}t{attr}>{}</{p}t>", escape_text(&t)));
                        }
                    },
                    |x, s| s.push_str(x),
                ))
            }
            "plain" => Ok(escape_text(&plain_text(&out).replace('\r', ""))),
            "hf" => Ok(render(
                u,
                &out,
                |_, t, s| s.push_str(&escape_text(&t.replace('&', "&&"))),
                |x, s| s.push_str(&escape_text(x)),
            )),
            m => Err(Error::Write(format!("unit {} has the unknown mode {m:?}", u.id))),
        }
    }

    fn unit_by_id(&self, id: &str) -> Result<&'a Unit, Error> {
        let i = *self.ids.get(id).ok_or_else(|| Error::Write(format!("unknown unit {id}")))?;
        Ok(&self.doc.units[i])
    }

    /// Replaces markers with their text.
    pub fn resolve(&mut self, s: &str) -> Result<String, Error> {
        if !s.contains([M_NAME, M_SREF, M_COL, M_UNIT]) {
            return Ok(s.to_string());
        }
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(p) = rest.find([M_NAME, M_SREF, M_COL, M_UNIT]) {
            out.push_str(&rest[..p]);
            let after = &rest[p..];
            let kind = after.chars().next().unwrap_or(M_UNIT);
            let k = kind.len_utf8();
            let end = after[k..].find(M_END).ok_or_else(|| Error::Write("a marker is not closed".into()))?;
            let id = &after[k..k + end];
            let tail = &after[k + end + M_END.len_utf8()..];
            if kind == M_UNIT {
                let i = *self.ids.get(id).ok_or_else(|| Error::Write(format!("unknown unit {id}")))?;
                out.push_str(&self.unit(i)?);
                rest = tail;
                continue;
            }
            let oe = tail.find(M_ORIG).ok_or_else(|| Error::Write("a marker has no original text".into()))?;
            let orig = &tail[..oe];
            rest = &tail[oe + M_ORIG.len_utf8()..];
            let u = self.unit_by_id(id)?;
            let src = plain_text(&u.source());
            match kind {
                M_NAME | M_SREF => {
                    let fin = self.names.get(id).cloned().unwrap_or_else(|| src.clone());
                    if fin == src {
                        out.push_str(orig);
                    } else if kind == M_NAME {
                        out.push_str(&attr_escape(&fin));
                    } else {
                        out.push_str(&escape_text(&quote_sheet(&fin)));
                    }
                }
                _ => {
                    let new = plain_text(&u.output());
                    if new == src {
                        out.push_str(orig);
                    } else {
                        if self.warned.insert(format!("col-{id}")) {
                            self.warnings.push(format!(
                                "{id}: a table column name was changed to match its header cell; formulas that use [{src}] must be updated"
                            ));
                        }
                        out.push_str(&attr_escape(&new.replace('\r', "")));
                    }
                }
            }
        }
        out.push_str(rest);
        Ok(out)
    }
}
