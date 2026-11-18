//! FormatJS (ICU MessageFormat) strings as one unit per branch (bilingua#23), outside the LocOS profile: the same
//! cut the json ICU dialects and arb make (`bilingua_model::icu::levels`), with a unit for each plural category the
//! target needs and a message lacks (a copy of `other`, marked [`icu::ADDED_HIDDEN`]). A unit holds the text of
//! one level; its `#` and nested plural or select arguments are standalone tags (tokens), its simple arguments and
//! the placeholders the crate protects elsewhere (`{{x}}`, printf, tags) are tags written back as they are.
//!
//! The merge rebuilds the message from the units and [`icu::render_branches`], so the target's categories are
//! written, and the crate writes the new string in the literal's own quote style. A message nobody translated
//! keeps its literal byte for byte.

use crate::ph;
use bilingua_model::icu::{self, Kind, Part};
use bilingua_model::*;
use std::collections::BTreeMap;

/// What a tag id in a unit stands for.
#[derive(Debug, Clone)]
enum Role {
    /// Written back as this text (a simple argument or a protected placeholder).
    Raw(String),
    /// `#` or a complex argument: index into the level's pieces; true for a complex argument.
    Token(usize, bool),
}

/// One planned unit.
pub struct Planned {
    pub inlines: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
    /// Where the unit sits in the message (`count: plural one`), empty for the message's own text.
    pub path: String,
    pub slot: Option<icu::BranchSlot>,
}

/// The plural or ordinal categories of `lang` (CLDR order, with `other`); `None` for a select or no language.
pub fn categories(lang: Option<&str>, kind: Kind) -> Option<Vec<&'static str>> {
    if kind == Kind::Select {
        return None;
    }
    let l = cldr::language(lang?);
    cldr::bits_babel(&l, kind == Kind::SelectOrdinal).map(cldr::names)
}

/// A level's inlines, codes and tag roles.
fn level(parts: &[Part]) -> (Vec<Inline>, BTreeMap<String, CodeData>, Vec<Role>) {
    let mut inl = Vec::new();
    let mut codes = BTreeMap::new();
    let mut roles: Vec<Role> = Vec::new();
    let mut tokens = 0usize;
    let mut raw = |text: &str, free: bool, inl: &mut Vec<Inline>, roles: &mut Vec<Role>| {
        let id = (roles.len() + 1).to_string();
        codes.insert(id.clone(), CodeData::new().with_standalone(text.to_string()).with_label("placeholder".to_string()));
        inl.push(if free { Inline::free_tag(id, text) } else { Inline::tag(id, TagKind::Standalone, text) });
        roles.push(Role::Raw(text.to_string()));
    };
    for p in parts {
        match p {
            Part::Text(t) => {
                for (is_ph, free, run) in ph::text_runs(t) {
                    if is_ph {
                        raw(run, free, &mut inl, &mut roles);
                    } else {
                        inl.push(Inline::Text(run.to_string()));
                    }
                }
            }
            Part::Simple(s) => raw(s, true, &mut inl, &mut roles),
            Part::Pound => {
                inl.push(Inline::tag((roles.len() + 1).to_string(), TagKind::Standalone, "#"));
                roles.push(Role::Token(tokens, false));
                tokens += 1;
            }
            Part::Complex(cx) => {
                inl.push(Inline::tag((roles.len() + 1).to_string(), TagKind::Standalone, format!("{} ...}}", cx.head.trim_end())));
                roles.push(Role::Token(tokens, true));
                tokens += 1;
            }
        }
    }
    (bilingua_model::merge_text(inl), codes, roles)
}

fn translatable(parts: &[Part]) -> bool {
    ph::translatable(&level(parts).0)
}

/// The units of an ICU message with a plural, selectordinal or select argument, or `None` for any other string
/// (not ICU, no such argument, or nothing to translate in it). `added`: the target whose missing plural categories
/// get units of their own.
pub fn plan(value: &str, added: Option<&str>) -> Option<Vec<Planned>> {
    if !(value.contains("plural") || value.contains("select")) {
        return None;
    }
    let parts = icu::parse(value).ok()?;
    if !parts.iter().any(|p| matches!(p, Part::Complex(_))) {
        return None;
    }
    let units: Vec<Planned> = icu::levels(&parts, &|k| categories(added, k))
        .into_iter()
        .filter(|l| translatable(l.parts))
        .map(|l| {
            let (inlines, codes, _) = level(l.parts);
            Planned { inlines, codes, path: l.path, slot: l.slot }
        })
        .collect();
    (!units.is_empty()).then_some(units)
}

/// True when `units` are what [`plan`] gives for `value` with `added`: as many, and each added unit where the plan
/// adds the same path.
pub fn aligned(value: &str, units: &[&Unit], added: Option<&str>) -> bool {
    let Some(p) = plan(value, added) else { return false };
    p.len() == units.len() && units.iter().zip(&p).all(|(u, up)| icu::added_matches(u, up.slot.as_ref().filter(|s| s.added).map(|_| up.path.as_str())))
}

/// Translations of one message's units, in unit order; `None` for a unit nobody translated.
struct Trans {
    list: Vec<Option<Vec<Inline>>>,
    at: usize,
}

impl Trans {
    fn next(&mut self) -> Option<Vec<Inline>> {
        let t = self.list.get(self.at).cloned().flatten();
        self.at += 1;
        t
    }
}

/// The message rebuilt from its units (planned with `added`), the plural arguments following `target`'s
/// categories. `None` when `value` is not ICU any more.
pub fn rebuild(value: &str, units: &[&Unit], target: Option<&str>, added: Option<&str>) -> Option<String> {
    let parts = icu::parse(value).ok()?;
    let mut tr = Trans { list: units.iter().map(|u| if u.segments.iter().all(|s| !s.has_target()) { None } else { Some(u.output()) }).collect(), at: 0 };
    Some(rebuild_level(&parts, &mut tr, target, added))
}

fn rebuild_level(parts: &[Part], tr: &mut Trans, target: Option<&str>, added: Option<&str>) -> String {
    let mine = if translatable(parts) { tr.next() } else { None };
    let mut pieces: Vec<String> = Vec::new();
    for p in parts {
        match p {
            Part::Pound => pieces.push("#".into()),
            Part::Complex(cx) => {
                let mut rendered: Vec<String> = Vec::with_capacity(cx.branches.len());
                let mut other_units = None;
                for b in &cx.branches {
                    let from = tr.at;
                    rendered.push(rebuild_level(&b.msg, tr, target, added));
                    if other_units.is_none() && b.selector == "other" {
                        other_units = Some(from..tr.at);
                    }
                }
                // the units added for the categories the source lacks, each a copy of `other`'s units: an
                // untranslated one takes `other`'s translation
                let mut extra: Vec<(&str, String)> = Vec::new();
                if let (Some(cats), Some(span), Some(other)) = (categories(added, cx.kind), other_units, cx.other()) {
                    for c in icu::missing(cx, &cats) {
                        let list = span.clone().map(|i| tr.next().or_else(|| tr.list.get(i).cloned().flatten())).collect();
                        extra.push((c, rebuild_level(&other.msg, &mut Trans { list, at: 0 }, target, added)));
                    }
                }
                let cats = categories(target, cx.kind);
                pieces.push(icu::render_branches(cx, rendered, &extra, cats.as_deref()));
            }
            _ => {}
        }
    }
    if let Some(inl) = mine {
        let roles = level(parts).2;
        if let Some(text) = render_translation(&inl, &roles, &pieces) {
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
fn render_translation(inl: &[Inline], roles: &[Role], pieces: &[String]) -> Option<String> {
    let mut text = String::new();
    let mut safe = String::new();
    let mut uses = vec![0usize; pieces.len()];
    for i in inl {
        match i {
            Inline::Text(t) => {
                text.push_str(t);
                safe.push_str(t);
            }
            Inline::Tag { id, kind, .. } => {
                if *kind != TagKind::Standalone {
                    return None;
                }
                match roles.get(id.parse::<usize>().ok()?.checked_sub(1)?)? {
                    Role::Raw(raw) => {
                        text.push_str(raw);
                        safe.push_str(raw);
                    }
                    Role::Token(k, _) => {
                        uses[*k] += 1;
                        text.push_str(&pieces[*k]);
                        safe.push('0');
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polish_gets_few_and_many_after_the_source_branches() {
        let p = plan("{count, plural, one {# file} other {# files}}", Some("pl")).unwrap();
        let paths: Vec<&str> = p.iter().map(|u| u.path.as_str()).collect();
        assert_eq!(paths, ["count: plural one", "count: plural other", "count: plural few", "count: plural many"]);
        assert!(p[2].slot.as_ref().unwrap().added);
        assert!(plan("Hello {name}", Some("pl")).is_none());
        assert!(plan("{n, plural, other {#}}", Some("pl")).is_none(), "nothing to translate");
        assert!(plan("{count, plural, one {x}", Some("pl")).is_none(), "not ICU");
    }

    #[test]
    fn placeholders_in_branches_are_tags() {
        let p = plan("{n, plural, one {<b>#</b> file {name}} other {<b>#</b> files}}", None).unwrap();
        let tags = p[0].inlines.iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
        assert_eq!(tags, 4, "{:?}", p[0].inlines);
    }
}
