//! W3C ITS 2.0 rules: reading `<its:rules>` and working out, for every element, whether it is translatable, inline
//! (within text), preformatted and what localization note applies, plus which attributes are translatable.
//!
//! Translate: element content is translatable by default and inherited; attributes are not unless a rule says so.
//! A local `its:translate` beats global rules; among global rules the last one wins. Rule sources, later wins: the
//! dialect, rules given to the filter, then `<its:rules>` inside the document. Linked rules (`xlink:href`) are never
//! fetched.

use crate::dialect::{Dialect, ITS_NS};
use crate::tree::{decode_refs, Tree, NONE, XML_NS};
use crate::xpath::{check_cost, Budget, Node, XPath};
use bilingua_model::{Error, LimitError, LimitKind, Limits};
use std::collections::HashMap;

pub(crate) const MAX_RULES: usize = 100;
pub(crate) const MAX_SELECTOR: usize = 512;

pub(crate) fn bad(msg: impl Into<String>) -> Error {
    Error::Parse { format: "xml".into(), msg: msg.into() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuleKind {
    Translate,
    WithinText,
    LocNote,
    PreserveSpace,
}

#[derive(Debug, Clone)]
pub(crate) struct Rule {
    pub kind: RuleKind,
    pub xpath: XPath,
    pub value: String,
    pub note: String,
    pub pointer: Option<XPath>,
}

/// A validated set of ITS rules, for example read from a user's `.its` rules file.
#[derive(Debug, Clone, Default)]
pub struct ItsRules {
    pub(crate) rules: Vec<Rule>,
}

impl ItsRules {
    pub fn new() -> ItsRules {
        ItsRules::default()
    }

    /// Reads an `<its:rules version="2.0">` document (the root must be `its:rules`). Unsafe or malformed rules
    /// (a selector that would be quadratic, more than 100 rules, a bad value) are refused with a plain message.
    pub fn parse(bytes: &[u8]) -> Result<ItsRules, Error> {
        let o = bilingua_model::ExtractOptions::default();
        let dec = bilingua_model::enc::decode_text(bytes, &bilingua_model::enc::DecodeOptions::new(&o).declared())?;
        if dec.text.len() > 1_000_000 {
            return Err(bad("ITS rules file is larger than 1 MB; refused."));
        }
        bilingua_model::guard::check_xml(bytes, &Limits::default())?;
        let t = Tree::parse(&dec.text)?;
        if t.local(t.root) != "rules" || t.el_ns(t.root) != ITS_NS {
            return Err(bad("An ITS rules file must have <its:rules> as its root element."));
        }
        Ok(ItsRules { rules: rules_from_element(&t, t.root)? })
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

fn compile(expr: &str, ns: &[(String, String)], what: &str, pointer: bool) -> Result<XPath, Error> {
    check_cost(expr, what, pointer, MAX_SELECTOR).map_err(bad)?;
    XPath::compile(expr, ns).map_err(|e| bad(format!("{what}: invalid XPath {:?}: {e}", expr.chars().take(80).collect::<String>())))
}

fn norm_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn rules_from_element(t: &Tree, rules_el: u32) -> Result<Vec<Rule>, Error> {
    let mut out = Vec::new();
    for it in &t.els[rules_el as usize].items {
        if it.kind != crate::tree::Kind::Elem {
            continue;
        }
        let c = it.el;
        if t.el_ns(c) != ITS_NS {
            continue;
        }
        let name = t.local(c);
        if !matches!(name, "translateRule" | "withinTextRule" | "locNoteRule" | "preserveSpaceRule") {
            continue;
        }
        if out.len() >= MAX_RULES {
            return Err(bad(format!("ITS rules: more than {MAX_RULES} rules; refused.")));
        }
        let ns = t.nsmap(c);
        let sel = t.attr_value(c, "", "selector").unwrap_or_default();
        let what = format!("ITS {name}");
        let xp = compile(&sel, &ns, &what, false)?;
        let attr = |n: &str| t.attr_value(c, "", n).unwrap_or_default().trim().to_string();
        match name {
            "translateRule" => {
                let v = attr("translate");
                if v != "yes" && v != "no" {
                    return Err(bad(format!("{what}: translate must be yes or no.")));
                }
                out.push(Rule { kind: RuleKind::Translate, xpath: xp, value: v, note: String::new(), pointer: None });
            }
            "withinTextRule" => {
                let v = attr("withinText");
                if !matches!(v.as_str(), "yes" | "no" | "nested") {
                    return Err(bad(format!("{what}: withinText must be yes, no or nested.")));
                }
                out.push(Rule { kind: RuleKind::WithinText, xpath: xp, value: v, note: String::new(), pointer: None });
            }
            "preserveSpaceRule" => {
                let v = attr("space");
                if v != "preserve" && v != "default" {
                    return Err(bad(format!("{what}: space must be preserve or default.")));
                }
                out.push(Rule { kind: RuleKind::PreserveSpace, xpath: xp, value: v, note: String::new(), pointer: None });
            }
            _ => {
                let ptr = t.attr_value(c, "", "locNotePointer");
                let note_el = t.els[c as usize].items.iter().find(|i| i.kind == crate::tree::Kind::Elem && t.local(i.el) == "locNote").map(|i| i.el);
                if let Some(p) = ptr.filter(|p| !p.is_empty()) {
                    let pointer = Some(compile(&p, &ns, &format!("{what} pointer"), true)?);
                    out.push(Rule { kind: RuleKind::LocNote, xpath: xp, value: String::new(), note: String::new(), pointer });
                } else if let Some(n) = note_el {
                    let mut s = String::new();
                    t.string_value(n, &mut s, 1 << 20);
                    out.push(Rule { kind: RuleKind::LocNote, xpath: xp, value: String::new(), note: norm_ws(&s), pointer: None });
                } else {
                    return Err(bad(format!("{what}: needs a locNote child or a locNotePointer.")));
                }
            }
        }
    }
    Ok(out)
}

/// The ITS properties of every element, indexed by element number.
pub(crate) struct Props {
    pub tr: Vec<bool>,
    /// 0 no, 1 yes, 2 nested
    pub wt: Vec<u8>,
    pub pre: Vec<bool>,
    /// index into `notes` (0 = no note)
    pub note: Vec<u32>,
    pub notes: Vec<String>,
    /// translatable attribute (index into `Tree::attrs`) -> note index
    pub attr_tr: HashMap<u32, u32>,
}

impl Props {
    /// Plain properties: everything translatable and not inline.
    pub(crate) fn plain(n: usize) -> Props {
        Props { tr: vec![true; n], wt: vec![0; n], pre: vec![false; n], note: vec![0; n], notes: vec![String::new()], attr_tr: HashMap::new() }
    }
}

fn limit(msg: String) -> Error {
    Error::Limit(LimitError::new(LimitKind::XmlElements, msg))
}

fn note_for(t: &Tree, rule: &Rule, el: u32, budget: &mut Budget) -> String {
    let Some(p) = &rule.pointer else { return rule.note.clone() };
    match p.eval(t, Node::El(el), budget) {
        Ok(crate::xpath::Val::Nodes(nodes)) => {
            let mut parts: Vec<String> = Vec::new();
            let mut size = 0usize;
            for n in nodes.iter().take(20) {
                let s = match n {
                    Node::El(e) => {
                        let mut s = String::new();
                        t.string_value(*e, &mut s, 2000);
                        s
                    }
                    Node::At(_, a) => decode_refs(t.attr_raw(&t.attrs[*a as usize]), |_| None),
                    _ => String::new(),
                };
                let s: String = s.chars().take(2000usize.saturating_sub(size)).collect();
                size += s.chars().count();
                parts.push(s);
                if size >= 2000 {
                    break;
                }
            }
            norm_ws(&parts.join(" ")).chars().take(2000).collect()
        }
        Ok(crate::xpath::Val::Str(s)) => norm_ws(&s).chars().take(2000).collect(),
        Ok(crate::xpath::Val::Num(n)) => norm_ws(&n.to_string()),
        Ok(crate::xpath::Val::Bool(b)) => b.to_string(),
        Err(_) => String::new(),
    }
}

/// Works out the properties. `extra` are rules given to the filter; rules inside the document come after them.
pub(crate) fn eval_props(t: &Tree, dialect: Option<&Dialect>, extra: &[Rule], limits: &Limits) -> Result<Props, Error> {
    let n = t.els.len();
    let mut tr_x = vec![0u8; n]; // 0 unset, 1 no, 2 yes
    let mut wt_x = vec![255u8; n];
    let mut pre_x = vec![0u8; n];
    let mut note_x = vec![0u32; n];
    let mut notes: Vec<String> = vec![String::new()];
    let mut attr_yes: HashMap<u32, u32> = HashMap::new(); // translatable attr idx -> owner element
    let mut attr_note: HashMap<u32, u32> = HashMap::new(); // attr idx -> note idx
    let intern = |s: String, notes: &mut Vec<String>| -> u32 {
        if s.is_empty() {
            return 0;
        }
        notes.push(s);
        (notes.len() - 1) as u32
    };

    if let Some(d) = dialect {
        for i in 0..n as u32 {
            let ln = t.local(i);
            if d.no_translate.contains(ln) {
                tr_x[i as usize] = 1;
            }
            if d.inline.contains(ln) {
                wt_x[i as usize] = 1;
            }
            if d.nested.contains(ln) {
                wt_x[i as usize] = 2;
            }
            if d.preserve.contains(ln) {
                pre_x[i as usize] = 2;
            }
            for r in &d.attrs {
                if r.elements.is_empty() || r.elements.iter().any(|e| e == "*" || e == ln) {
                    if let Some(a) = t.find_attr(i, "", &r.attr) {
                        let idx = (a as *const _ as usize - t.attrs.as_ptr() as usize) / std::mem::size_of::<crate::tree::Attr>();
                        attr_yes.insert(idx as u32, i);
                    }
                }
            }
            if let Some(ta) = &d.translate_attr {
                match t.attr_value(i, "", ta).as_deref() {
                    Some("yes") => tr_x[i as usize] = 2,
                    Some("no") => tr_x[i as usize] = 1,
                    _ => {}
                }
            }
            if d.no_translate_if_attr.iter().any(|a| t.find_attr(i, "", a).is_some()) {
                tr_x[i as usize] = 1;
            }
        }
    }

    // rules: extra first, then those in the document
    let mut rules: Vec<Rule> = dialect.map(|d| d.its_rules.rules.clone()).unwrap_or_default();
    rules.extend(extra.iter().cloned());
    for i in 0..n as u32 {
        if t.local(i) == "rules" && t.el_ns(i) == ITS_NS {
            rules.extend(rules_from_element(t, i)?);
            if rules.len() > 2 * MAX_RULES {
                return Err(bad(format!("ITS rules: more than {} rules in all; refused.", 2 * MAX_RULES)));
            }
        }
    }
    let cap = 200usize.saturating_mul(limits.max_units);
    if !rules.is_empty() && rules.len().saturating_mul(n) > cap {
        return Err(limit(format!(
            "This XML file is too large for the number of ITS rules it carries ({} rules, {} elements). Split it into smaller files.",
            rules.len(),
            n
        )));
    }
    let budget_per_rule = 400u64 * (n as u64 + 1000);
    // string bytes are shared by every rule and note pointer of the document
    let mut bytes = Budget::new(t, 0).bytes;
    for rule in &rules {
        let mut budget = Budget { steps: budget_per_rule, bytes };
        let hits = rule.xpath.select(t, Node::Doc, &mut budget).map_err(|e| bad(format!("ITS rule could not be evaluated: {e}")))?;
        if rule.pointer.is_some() && hits.len().saturating_mul(n) > cap {
            return Err(limit(format!(
                "This XML file is too large for its ITS locNotePointer rule ({} selected elements, {} elements). Split it into smaller files or use a locNote text instead of a pointer.",
                hits.len(),
                n
            )));
        }
        bytes = budget.bytes;
        let mut note_budget = Budget { steps: 4u64 * budget_per_rule, bytes };
        for h in hits {
            match h {
                Node::El(e) => {
                    let i = e as usize;
                    match rule.kind {
                        RuleKind::Translate => tr_x[i] = if rule.value == "yes" { 2 } else { 1 },
                        RuleKind::WithinText => {
                            wt_x[i] = match rule.value.as_str() {
                                "yes" => 1,
                                "nested" => 2,
                                _ => 0,
                            }
                        }
                        RuleKind::PreserveSpace => pre_x[i] = if rule.value == "preserve" { 2 } else { 1 },
                        RuleKind::LocNote => {
                            let s = note_for(t, rule, e, &mut note_budget);
                            note_x[i] = intern(s, &mut notes);
                            if note_x[i] == 0 {
                                note_x[i] = u32::MAX; // explicitly empty
                            }
                        }
                    }
                }
                Node::At(e, a) => match rule.kind {
                    RuleKind::Translate => {
                        if rule.value == "yes" {
                            attr_yes.insert(a, e);
                        } else {
                            attr_yes.remove(&a);
                        }
                    }
                    RuleKind::LocNote => {
                        let s = note_for(t, rule, e, &mut note_budget);
                        let id = intern(s, &mut notes);
                        attr_note.insert(a, if id == 0 { u32::MAX } else { id });
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        bytes = note_budget.bytes;
    }

    // local ITS markup beats global rules; ITS elements are never content
    let has_its = t.uris.iter().any(|u| u == ITS_NS);
    let has_xml = t.uris.iter().any(|u| u == XML_NS);
    for i in 0..n as u32 {
        let iu = i as usize;
        let local = |name: &str| if has_its { t.attr_value(i, ITS_NS, name) } else { None };
        if t.els[iu].attr_lo != t.els[iu].attr_hi && (has_its || has_xml) {
            match local("translate").as_deref() {
                Some("yes") => tr_x[iu] = 2,
                Some("no") => tr_x[iu] = 1,
                _ => {}
            }
            match local("withinText").as_deref() {
                Some("yes") => wt_x[iu] = 1,
                Some("no") => wt_x[iu] = 0,
                Some("nested") => wt_x[iu] = 2,
                _ => {}
            }
            if let Some(v) = local("locNote") {
                note_x[iu] = intern(norm_ws(&v), &mut notes);
                if note_x[iu] == 0 {
                    note_x[iu] = u32::MAX;
                }
            }
            match if has_xml { t.attr_value(i, XML_NS, "space") } else { None }.as_deref() {
                Some("preserve") => pre_x[iu] = 2,
                Some("default") => pre_x[iu] = 1,
                _ => {}
            }
        }
        if t.el_ns(i) == ITS_NS {
            tr_x[iu] = 1;
        }
    }

    let mut p = Props { tr: vec![true; n], wt: vec![0; n], pre: vec![false; n], note: vec![0; n], notes, attr_tr: HashMap::new() };
    for i in 0..n {
        let parent = t.els[i].parent;
        let (ptr, ppre, pnote) = if parent == NONE { (true, false, 0) } else { (p.tr[parent as usize], p.pre[parent as usize], p.note[parent as usize]) };
        p.tr[i] = match tr_x[i] {
            0 => ptr,
            1 => false,
            _ => true,
        };
        p.wt[i] = if wt_x[i] == 255 { 0 } else { wt_x[i] };
        p.pre[i] = match pre_x[i] {
            0 => ppre,
            1 => false,
            _ => true,
        };
        p.note[i] = match note_x[i] {
            0 => pnote,
            u32::MAX => 0,
            v => v,
        };
    }
    for (a, owner) in attr_yes {
        let note = match attr_note.get(&a) {
            Some(&u32::MAX) => 0,
            Some(&v) => v,
            None => p.note[owner as usize],
        };
        p.attr_tr.insert(a, note);
    }
    Ok(p)
}
