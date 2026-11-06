//! Messages to units and back: ICU MessageFormat (ARB, FormatJS, Structured JSON), Chrome `$name$` and go-i18n
//! `{{ ... }}`. A message with plural, selectordinal or select arguments gives one unit for the text around each
//! argument and one per branch; the argument itself and `#` are standalone tags that carry no code of their own
//! (LocOS keeps them as `⟦xN⟧` tokens), simple arguments (`{name}`) are free tags labelled `placeholder` (LocOS keeps
//! them as plain text).

use crate::dialect::Syntax;
use crate::icu::{self, Complex, Kind, Part};
use bilingua_model::*;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// What a tag id in a unit stands for.
#[derive(Debug, Clone)]
pub enum Role {
    /// A simple argument, written back as its raw text.
    Simple(String),
    /// `#` or a complex argument: index into the level's pieces; true for a complex argument.
    Token(usize, bool),
    /// A Chrome `$name$` or go-i18n `{{ ... }}` placeholder, written back as its raw text.
    Raw(String),
}

pub struct UnitPlan {
    pub inlines: Vec<Inline>,
    pub roles: Vec<Role>,
    pub codes: BTreeMap<String, CodeData>,
    /// Where the unit sits in the message ("count: plural one"), empty for the message's own text.
    pub path: String,
}

pub struct Plan {
    pub units: Vec<UnitPlan>,
    /// The ICU parse failed: the whole string is one opaque unit.
    pub fallback: bool,
}

const TOK_OPEN: &str = "\u{27E6}x";
const TOK_CLOSE: char = '\u{27E7}';

/// `⟦x<digits>⟧` somewhere in `t`: a LocOS argument token.
pub fn has_locos_token(t: &str) -> bool {
    t.match_indices(TOK_OPEN).any(|(at, m)| {
        let rest = &t[at + m.len()..];
        let d = rest.chars().take_while(|c| py_isdecimal_char(*c)).map(char::len_utf8).sum::<usize>();
        d > 0 && rest[d..].starts_with(TOK_CLOSE)
    })
}

/// LocOS `_translatable`: a letter or digit once the `⟦xN⟧` tokens are removed.
fn translatable_text(s: &str) -> bool {
    let mut rest = s;
    while !rest.is_empty() {
        match rest.find(TOK_OPEN) {
            None => return py_has_alnum(rest),
            Some(at) => {
                if py_has_alnum(&rest[..at]) {
                    return true;
                }
                let after = &rest[at + TOK_OPEN.len()..];
                let d = after.chars().take_while(|c| py_isdecimal_char(*c)).map(char::len_utf8).sum::<usize>();
                if d > 0 && after[d..].starts_with(TOK_CLOSE) {
                    rest = &after[d + TOK_CLOSE.len_utf8()..];
                } else {
                    // not a token: its `x` is a letter
                    return true;
                }
            }
        }
    }
    false
}

fn chrome_re() -> &'static regex::Regex {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"\$[A-Za-z0-9_@]+\$|\$[1-9]").expect("static regex"))
}

fn go_re() -> &'static regex::Regex {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"(?s)\{\{.*?\}\}").expect("static regex"))
}

pub fn plan_message(value: &str, syntax: Syntax) -> Plan {
    match syntax {
        Syntax::Icu => match icu::parse(value) {
            Ok(parts) => {
                let mut units = Vec::new();
                collect(&parts, String::new(), &mut units);
                Plan { units, fallback: false }
            }
            Err(_) => {
                let units = if translatable_text(value) { vec![fallback_unit(value)] } else { vec![] };
                Plan { units, fallback: true }
            }
        },
        Syntax::Chrome | Syntax::Go => {
            let rx = if syntax == Syntax::Chrome { chrome_re() } else { go_re() };
            let (mut inlines, mut roles) = (Vec::new(), Vec::new());
            let mut probe = String::new();
            let mut at = 0usize;
            for m in rx.find_iter(value) {
                if m.start() > at {
                    inlines.push(Inline::Text(value[at..m.start()].to_string()));
                    probe.push_str(&value[at..m.start()]);
                }
                roles.push(Role::Raw(m.as_str().to_string()));
                inlines.push(Inline::tag(roles.len().to_string(), TagKind::Standalone, m.as_str()));
                probe.push_str(&format!("{TOK_OPEN}{}{TOK_CLOSE}", roles.len()));
                at = m.end();
            }
            if at < value.len() {
                inlines.push(Inline::Text(value[at..].to_string()));
                probe.push_str(&value[at..]);
            }
            let units = if translatable_text(&probe) { vec![UnitPlan { inlines, roles, codes: BTreeMap::new(), path: String::new() }] } else { vec![] };
            Plan { units, fallback: false }
        }
    }
}

/// LocOS `_translatable` of one ICU level: its text with each `#` and complex argument as a `⟦xN⟧` token (a
/// literal token in the source does not count either).
fn level_translatable(parts: &[Part]) -> bool {
    if !icu::translatable(parts) {
        return false;
    }
    let mut text = String::new();
    for p in parts {
        match p {
            Part::Text(t) | Part::Simple(t) => text.push_str(t),
            Part::Pound | Part::Complex(_) => {
                text.push_str(TOK_OPEN);
                text.push('1');
                text.push(TOK_CLOSE);
            }
        }
    }
    translatable_text(&text)
}

/// Pre-order: this level's text if it has words, then each complex argument's branches.
fn collect(parts: &[Part], path: String, out: &mut Vec<UnitPlan>) {
    if level_translatable(parts) {
        out.push(level_unit(parts, &path));
    }
    for p in parts {
        if let Part::Complex(cx) = p {
            let arg = arg_name(&cx.head);
            let kind = match cx.kind {
                Kind::Plural => "plural",
                Kind::SelectOrdinal => "selectordinal",
                Kind::Select => "select",
            };
            for b in &cx.branches {
                let here = if path.is_empty() { String::new() } else { format!("{path} / ") };
                collect(&b.msg, format!("{here}{arg}: {kind} {}", b.selector), out);
            }
        }
    }
}

fn arg_name(head: &str) -> String {
    let inner = head.trim_start_matches('{').trim_start();
    inner.split([',', '}']).next().unwrap_or("").trim().to_string()
}

fn level_unit(parts: &[Part], path: &str) -> UnitPlan {
    let mut inlines = Vec::new();
    let mut roles = Vec::new();
    let mut codes = BTreeMap::new();
    let mut tokens = 0usize;
    for p in parts {
        let id = (roles.len() + 1).to_string();
        match p {
            Part::Text(t) => inlines.push(Inline::Text(t.clone())),
            Part::Simple(raw) => {
                inlines.push(Inline::free_tag(id.clone(), raw.clone()));
                codes.insert(id, CodeData::new().with_standalone(raw.clone()).with_label("placeholder".to_string()));
                roles.push(Role::Simple(raw.clone()));
            }
            Part::Pound => {
                inlines.push(Inline::tag(id, TagKind::Standalone, "#"));
                roles.push(Role::Token(tokens, false));
                tokens += 1;
            }
            Part::Complex(cx) => {
                inlines.push(Inline::tag(id, TagKind::Standalone, format!("{} ...}}", cx.head.trim_end())));
                roles.push(Role::Token(tokens, true));
                tokens += 1;
            }
        }
    }
    UnitPlan { inlines, roles, codes, path: path.to_string() }
}

/// An unparsable message: text plus `{name}` placeholders as free tags.
fn fallback_unit(value: &str) -> UnitPlan {
    let b = value.as_bytes();
    let mut inlines = Vec::new();
    let mut roles = Vec::new();
    let mut codes = BTreeMap::new();
    let mut run = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'{' && (i == 0 || b[i - 1] != b'\'') {
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || matches!(b[j], b'_' | b'.' | b'-')) {
                j += 1;
            }
            if j > i + 1 && b.get(j) == Some(&b'}') {
                if i > run {
                    inlines.push(Inline::Text(value[run..i].to_string()));
                }
                let raw = value[i..=j].to_string();
                let id = (roles.len() + 1).to_string();
                inlines.push(Inline::free_tag(id.clone(), raw.clone()));
                codes.insert(id, CodeData::new().with_standalone(raw.clone()).with_label("placeholder".to_string()));
                roles.push(Role::Simple(raw));
                i = j + 1;
                run = i;
                continue;
            }
        }
        i += 1;
    }
    if run < value.len() {
        inlines.push(Inline::Text(value[run..].to_string()));
    }
    UnitPlan { inlines, roles, codes, path: String::new() }
}

/// Translations of one message's units, in unit order; `None` for a unit nobody translated.
pub struct Trans {
    pub list: Vec<Option<Vec<Inline>>>,
    pub at: usize,
}

impl Trans {
    fn next(&mut self) -> Option<Vec<Inline>> {
        let t = self.list.get(self.at).cloned().flatten();
        self.at += 1;
        t
    }
}

pub fn rebuild_message(value: &str, plan: &Plan, syntax: Syntax, tr: &mut Trans, target: Option<&str>, locos: bool) -> String {
    if syntax != Syntax::Icu {
        let Some(unit) = plan.units.first() else { return value.to_string() };
        let Some(t) = tr.next() else { return value.to_string() };
        return render_raw(&t, &unit.roles, locos).unwrap_or_else(|| value.to_string());
    }
    if plan.fallback {
        let Some(t) = tr.next() else { return value.to_string() };
        let roles = &plan.units[0].roles;
        let mut s = String::new();
        for i in &t {
            match i {
                Inline::Text(x) => s.push_str(x),
                Inline::Tag { id, .. } => {
                    if let Some(Role::Simple(raw)) = id.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|n| roles.get(n)) {
                        s.push_str(raw);
                    }
                }
            }
        }
        return s;
    }
    match icu::parse(value) {
        Ok(parts) => rebuild_level(&parts, tr, target, locos),
        Err(_) => value.to_string(),
    }
}

/// Chrome and go-i18n: the translation with its placeholders written back; `None` (keep the source) for a tag
/// that is not one of the message's, or, under the LocOS profile, a `⟦xN⟧` token left in the text.
fn render_raw(inl: &[Inline], roles: &[Role], locos: bool) -> Option<String> {
    let mut s = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => {
                if locos && has_locos_token(t) {
                    return None;
                }
                s.push_str(t);
            }
            Inline::Tag { id, kind, .. } => {
                if *kind != TagKind::Standalone {
                    return None;
                }
                match roles.get(id.parse::<usize>().ok()?.checked_sub(1)?)? {
                    Role::Raw(raw) => s.push_str(raw),
                    _ => return None,
                }
            }
        }
    }
    Some(s)
}

fn rebuild_level(parts: &[Part], tr: &mut Trans, target: Option<&str>, locos: bool) -> String {
    let translatable = level_translatable(parts);
    let mine = if translatable { tr.next() } else { None };
    let mut pieces: Vec<String> = Vec::new();
    for p in parts {
        match p {
            Part::Pound => pieces.push("#".into()),
            Part::Complex(cx) => {
                let rendered: Vec<String> = cx.branches.iter().map(|b| rebuild_level(&b.msg, tr, target, locos)).collect();
                pieces.push(render_branches(cx, rendered, target));
            }
            _ => {}
        }
    }
    if let Some(inl) = mine {
        let roles = level_unit(parts, "").roles;
        if let Some(text) = render_translation(&inl, &roles, &pieces, locos) {
            return text;
        }
    }
    let mut out = String::new();
    let mut k = 0;
    for p in parts {
        match p {
            Part::Text(t) | Part::Simple(t) => out.push_str(t),
            Part::Pound | Part::Complex(_) => {
                out.push_str(&pieces[k]);
                k += 1;
            }
        }
    }
    out
}

/// The text of a translated level, or `None` when it must fall back to the source: an unknown tag, a complex
/// argument missing or repeated, or text that would break the ICU structure it is written into.
fn render_translation(inl: &[Inline], roles: &[Role], pieces: &[String], locos: bool) -> Option<String> {
    let mut text = String::new();
    let mut safe = String::new();
    let mut uses = vec![0usize; pieces.len()];
    for i in inl {
        match i {
            Inline::Text(t) => {
                if locos && has_locos_token(t) {
                    return None;
                }
                text.push_str(t);
                safe.push_str(t);
            }
            Inline::Tag { id, kind, .. } => {
                if *kind != TagKind::Standalone {
                    return None;
                }
                match roles.get(id.parse::<usize>().ok()?.checked_sub(1)?)? {
                    Role::Simple(raw) => {
                        text.push_str(raw);
                        safe.push_str(raw);
                    }
                    Role::Token(k, _) => {
                        uses[*k] += 1;
                        text.push_str(&pieces[*k]);
                        safe.push('0');
                    }
                    Role::Raw(_) => return None,
                }
            }
        }
    }
    for r in roles {
        if let Role::Token(k, true) = r {
            if uses[*k] != 1 {
                return None;
            }
        }
    }
    icu::parse(&safe).ok()?;
    Some(text)
}

/// A complex argument rebuilt from its rendered branches. Plural and selectordinal follow the target's
/// categories: unused ones go, missing ones copy the `other` text, `=N` branches stay.
fn render_branches(cx: &Complex, rendered: Vec<String>, target: Option<&str>) -> String {
    struct B {
        selector: String,
        mid: String,
        text: String,
        trail: String,
    }
    let mut list: Vec<B> = cx
        .branches
        .iter()
        .zip(rendered)
        .map(|(b, text)| B { selector: b.selector.clone(), mid: b.mid.clone(), text, trail: b.trail.clone() })
        .collect();
    let cats = if cx.kind == Kind::Select { None } else { target.and_then(|t| crate::cldr::categories(t, cx.kind == Kind::SelectOrdinal)) };
    if let Some(cats) = cats {
        if let Some(o) = list.iter().find(|b| b.selector == "other") {
            let (other_mid, other_text) = (o.mid.clone(), o.text.clone());
            let mut kept: Vec<B> = list.into_iter().filter(|b| b.selector.starts_with('=') || cats.contains(&b.selector.as_str())).collect();
            let missing: Vec<&str> = cats.iter().copied().filter(|c| !kept.iter().any(|b| b.selector == *c)).collect();
            if !missing.is_empty() {
                let idx = kept.iter().position(|b| b.selector == "other").unwrap_or(kept.len());
                let sep = if idx > 0 && !kept[idx - 1].trail.is_empty() { kept[idx - 1].trail.clone() } else { " ".to_string() };
                let new = missing.iter().map(|c| B { selector: (*c).to_string(), mid: other_mid.clone(), text: other_text.clone(), trail: sep.clone() });
                let tail = kept.split_off(idx);
                kept.extend(new);
                kept.extend(tail);
            }
            list = kept;
        }
    }
    let mut out = cx.head.clone();
    for b in &list {
        out.push_str(&b.selector);
        out.push_str(&b.mid);
        out.push('{');
        out.push_str(&b.text);
        out.push('}');
        out.push_str(&b.trail);
    }
    out.push('}');
    out
}
