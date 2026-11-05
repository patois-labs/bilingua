//! The column mapping: the spec string, header detection, locale matching and the plan that says which column is the
//! source, which ones are targets and which ones are notes. The rules are Localization OS's (`csv_filter_common`).

use crate::rec::{blank, py_space, MAX_COLUMNS};
use bilingua_model::Error;

const SPEC_MAX_CHARS: usize = 2000;

fn bad(msg: impl Into<String>) -> Error {
    Error::Unsupported(msg.into())
}

/// Whether the first row is a header: decided from the file, or as the mapping says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum HeaderMode {
    #[default]
    Auto,
    Yes,
    No,
}

/// A parsed column-mapping spec such as `source=English, target.de=German, key=id`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Spec {
    pub header: HeaderMode,
    pub source: Option<String>,
    pub target: Option<String>,
    /// `target.<locale>=COL`, locale normalised (lower case, `-`), in the order given.
    pub targets: Vec<(String, String)>,
    pub key: Option<String>,
    pub context: Vec<String>,
    pub maxlen: Option<String>,
    pub delimiter: Option<u8>,
}

/// Lower case, white space collapsed, BOM removed (header and locale comparison).
pub fn norm(name: &str) -> String {
    let s: String = name.replace('\u{feff}', "").to_lowercase();
    s.split(py_space_ws).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
}

fn py_space_ws(c: char) -> bool {
    py_space(c)
}

pub fn norm_locale(code: &str) -> String {
    norm(code).replace('_', "-")
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn strip(s: &str) -> &str {
    s.trim_matches(py_space)
}

pub fn parse_spec(spec: Option<&str>) -> Result<Spec, Error> {
    let mut out = Spec::default();
    let spec = strip(spec.unwrap_or(""));
    if spec.is_empty() {
        return Ok(out);
    }
    if spec.chars().count() > SPEC_MAX_CHARS {
        return Err(bad(format!("The column mapping is too long (limit {SPEC_MAX_CHARS}).")));
    }
    for part in spec.split(',') {
        let part = strip(part);
        if part.is_empty() {
            continue;
        }
        let Some((role, col)) = part.split_once('=') else {
            let shown: String = part.chars().take(60).collect();
            return Err(bad(format!("Column mapping entry '{shown}' needs the form role=column (for example source=English).")));
        };
        let (role, col) = (strip(role), strip(col));
        let role_l = role.to_lowercase();
        if col.is_empty() {
            let shown: String = role.chars().take(40).collect();
            return Err(bad(format!("Column mapping entry '{shown}' names no column.")));
        }
        match role_l.as_str() {
            "header" => {
                out.header = match col.to_lowercase().as_str() {
                    "yes" | "true" => HeaderMode::Yes,
                    "no" | "false" => HeaderMode::No,
                    "auto" => HeaderMode::Auto,
                    _ => return Err(bad("header= takes yes, no or auto.")),
                }
            }
            "delimiter" => {
                out.delimiter = Some(match col.to_lowercase().as_str() {
                    "comma" | "," => b',',
                    "semicolon" | ";" => b';',
                    "tab" => b'\t',
                    "pipe" | "|" => b'|',
                    _ => return Err(bad("delimiter= takes comma, semicolon, tab or pipe.")),
                })
            }
            "source" => out.source = Some(col.to_string()),
            "target" => out.target = Some(col.to_string()),
            "key" => out.key = Some(col.to_string()),
            "maxlen" => out.maxlen = Some(col.to_string()),
            "context" => out.context.push(col.to_string()),
            r if r.starts_with("target.") || r.starts_with("target:") => {
                let loc = norm_locale(&role.chars().skip(7).collect::<String>());
                if loc.is_empty() {
                    return Err(bad("target.<locale>= needs a locale, e.g. target.de=German."));
                }
                match out.targets.iter_mut().find(|(l, _)| *l == loc) {
                    Some(e) => e.1 = col.to_string(),
                    None => out.targets.push((loc, col.to_string())),
                }
            }
            _ => {
                let shown: String = role.chars().take(40).collect();
                return Err(bad(format!(
                    "Unknown column role '{shown}'. Use source, target, target.<locale>, key, context, maxlen, header or delimiter."
                )));
            }
        }
    }
    Ok(out)
}

fn has_mapping(s: &Spec) -> bool {
    s.source.is_some() || s.target.is_some() || !s.targets.is_empty()
}

const SOURCE: &[&str] = &["source", "source text", "source_text", "source string", "text", "string", "original", "default content", "source language"];
const TARGET: &[&str] = &["target", "target text", "target_text", "translation", "translated", "translated content", "target language"];
const KEY: &[&str] = &["key", "id", "string id", "string_id", "identifier", "msgid", "identification"];
const CONTEXT: &[&str] = &["context", "notes", "note", "comment", "comments", "description", "field"];
const MAXLEN: &[&str] = &["max length", "max_length", "maxlength", "maxlen", "char limit", "length limit"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Source,
    Target,
    Key,
    Context,
    Maxlen,
}

fn role_of(name: &str) -> Option<Role> {
    let n = norm(name);
    let n = n.as_str();
    [(Role::Source, SOURCE), (Role::Target, TARGET), (Role::Key, KEY), (Role::Context, CONTEXT), (Role::Maxlen, MAXLEN)]
        .into_iter()
        .find(|(_, names)| names.contains(&n))
        .map(|(r, _)| r)
}

/// The project locale a header names (`de`, `de-DE`, `de_de`), else `None`. An exact match wins; a bare language
/// matches a regional locale only when it is the only one with that language.
pub fn locale_match<'a>(name: &str, locales: &[&'a str]) -> Option<&'a str> {
    let n = norm_locale(name);
    if n.is_empty() || n.chars().count() > 20 {
        return None;
    }
    // a dict keyed by the normalised locale: the first key position, the last value
    let mut by_norm: Vec<(String, &'a str)> = Vec::new();
    for x in locales.iter().filter(|x| !x.is_empty()) {
        let k = norm_locale(x);
        match by_norm.iter_mut().find(|(kk, _)| *kk == k) {
            Some(e) => e.1 = x,
            None => by_norm.push((k, x)),
        }
    }
    if let Some((_, v)) = by_norm.iter().find(|(k, _)| *k == n) {
        return Some(v);
    }
    let lang = n.split('-').next().unwrap_or("");
    let plain = !n.contains('-');
    let hits: Vec<&'a str> = by_norm.iter().filter(|(k, _)| k.split('-').next().unwrap_or("") == lang && plain).map(|(_, v)| *v).collect();
    if hits.len() == 1 {
        Some(hits[0])
    } else {
        None
    }
}

fn looks_like_header(first: &[String], source_locale: &str, targets: &[String]) -> bool {
    let locs: Vec<&str> = std::iter::once(source_locale).chain(targets.iter().map(String::as_str)).filter(|x| !x.is_empty()).collect();
    first.iter().any(|cell| role_of(cell).is_some() || locale_match(cell, &locs).is_some())
}

fn col_index(r: &str, header: Option<&[String]>, width: usize, new_ok: bool) -> Result<usize, Error> {
    let r = strip(r);
    if is_digits(r) {
        let i = r.parse::<usize>().ok().and_then(|v| v.checked_sub(1));
        return match i {
            Some(i) if i < MAX_COLUMNS && (i < width.max(1) || new_ok) => Ok(i),
            _ => Err(bad(format!("Column {r} does not exist (the file has {width})."))),
        };
    }
    let shown: String = r.chars().take(60).collect();
    let Some(header) = header else {
        return Err(bad(format!("Column '{shown}' is named but the file has no header row; use a column number.")));
    };
    let want = norm(r);
    if let Some(i) = header.iter().position(|h| norm(h) == want) {
        return Ok(i);
    }
    let found: String = header.iter().filter(|h| !h.is_empty()).map(String::as_str).collect::<Vec<_>>().join(", ").chars().take(200).collect();
    let found = if found.is_empty() { "(blank header)".to_string() } else { found };
    Err(bad(format!("No column named '{shown}'. Found: {found}.")))
}

/// Which column is what.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub header: bool,
    pub source: Option<usize>,
    /// Target columns by project locale, or `*` for the one every target writes into.
    pub targets: Vec<(String, usize)>,
    /// Note columns: label and column.
    pub notes: Vec<(String, usize)>,
    /// No source column found or named: every text cell is translated where it stands.
    pub inplace: bool,
}

impl Plan {
    fn target_named(&self, k: &str) -> bool {
        self.targets.iter().any(|(l, _)| l == k)
    }
}

pub fn resolve(spec: &Spec, first: &[String], width: usize, source_locale: &str, targets: &[String]) -> Result<Plan, Error> {
    let tl: Vec<&str> = targets.iter().map(String::as_str).collect();
    let has_header = match spec.header {
        HeaderMode::Yes => true,
        HeaderMode::No => false,
        HeaderMode::Auto => {
            let mut refs: Vec<&str> = [&spec.source, &spec.target, &spec.key, &spec.maxlen].into_iter().flatten().map(String::as_str).collect();
            refs.extend(spec.targets.iter().map(|(_, c)| c.as_str()));
            refs.extend(spec.context.iter().map(String::as_str));
            refs.iter().any(|r| !r.is_empty() && !is_digits(strip(r))) || looks_like_header(first, source_locale, targets)
        }
    };
    let header: Option<&[String]> = has_header.then_some(first);
    let mut plan = Plan { header: has_header, ..Default::default() };
    let named = has_mapping(spec) || spec.key.is_some() || !spec.context.is_empty() || spec.maxlen.is_some();
    if named {
        if let Some(s) = &spec.source {
            plan.source = Some(col_index(s, header, width, false)?);
        }
        if let Some(t) = &spec.target {
            plan.targets.push(("*".into(), col_index(t, header, width, true)?));
        }
        for (loc, r) in &spec.targets {
            let m = locale_match(loc, &tl).unwrap_or(loc.as_str()).to_string();
            let c = col_index(r, header, width, true)?;
            match plan.targets.iter_mut().find(|(l, _)| *l == m) {
                Some(e) => e.1 = c,
                None => plan.targets.push((m, c)),
            }
        }
        if let Some(k) = &spec.key {
            plan.notes.push(("Key".into(), col_index(k, header, width, false)?));
        }
        for r in &spec.context {
            let i = col_index(r, header, width, false)?;
            let label = header.and_then(|h| h.get(i).cloned()).unwrap_or_default();
            plan.notes.push((if label.is_empty() { "Context".into() } else if header.is_some() { label } else { "Context".into() }, i));
        }
        if let Some(m) = &spec.maxlen {
            plan.notes.push(("Max length".into(), col_index(m, header, width, false)?));
        }
        if plan.source.is_none() {
            if !plan.targets.is_empty() {
                return Err(bad("The column mapping names targets but no source=."));
            }
            plan.inplace = true;
        }
    } else if has_header {
        for (i, h) in first.iter().take(MAX_COLUMNS).enumerate() {
            let role = role_of(h);
            if role == Some(Role::Source) && plan.source.is_none() {
                plan.source = Some(i);
            } else if role == Some(Role::Target) && !plan.target_named("*") {
                plan.targets.push(("*".into(), i));
            } else if role == Some(Role::Key) {
                plan.notes.push(("Key".into(), i));
            } else if role == Some(Role::Context) {
                let t = strip(h);
                plan.notes.push((if t.is_empty() { "Context".into() } else { t.to_string() }, i));
            } else if role == Some(Role::Maxlen) {
                plan.notes.push(("Max length".into(), i));
            } else if !source_locale.is_empty() && plan.source.is_none() && locale_match(h, &[source_locale]).is_some() {
                plan.source = Some(i);
            } else if let Some(loc) = locale_match(h, &tl) {
                if !plan.target_named(loc) {
                    plan.targets.push((loc.to_string(), i));
                }
            }
        }
        if plan.source.is_none() {
            plan.targets.clear();
            plan.inplace = true;
        }
    } else {
        plan.inplace = true;
    }
    if let Some(s) = plan.source {
        if plan.targets.iter().any(|(_, c)| *c == s) {
            return Err(bad("The source column cannot also be a target column."));
        }
    }
    Ok(plan)
}

/// The column a build for `locale` writes into, or `None` (in place / no column).
pub fn target_col(plan: &Plan, locale: Option<&str>) -> Option<usize> {
    let star = || plan.targets.iter().find(|(l, _)| l == "*").map(|(_, c)| *c);
    if let Some(loc) = locale.filter(|l| !l.is_empty()) {
        let want = norm_locale(loc);
        if let Some((_, c)) = plan.targets.iter().find(|(k, _)| k != "*" && norm_locale(k) == want) {
            return Some(*c);
        }
        let keys: Vec<&str> = plan.targets.iter().map(|(k, _)| k.as_str()).filter(|k| *k != "*").collect();
        if let Some(hit) = locale_match(loc, &keys) {
            return plan.targets.iter().find(|(k, _)| k == hit).map(|(_, c)| *c);
        }
    }
    star()
}

/// The note shown with a unit: `Key: hello | notes: greeting`, one part per non-blank note column.
pub fn note_parts(plan: &Plan, row: &[String]) -> Vec<String> {
    let mut parts = Vec::new();
    for (label, i) in &plan.notes {
        let v = row.get(*i).map(|v| strip(v)).unwrap_or("");
        if !v.is_empty() {
            let v: String = v.chars().take(500).collect();
            parts.push(format!("{label}: {v}"));
        }
    }
    parts
}

/// A source cell worth a segment: has a letter and is not a formula (`=SUM(...)`, Excel's `="0012"`).
/// Python `str.isalpha` is letters only (not digits); Rust's `is_alphabetic` also counts letter numbers and a few
/// marks, which this removes as far as the standard library allows (a known small difference).
pub fn is_translatable(v: &str) -> bool {
    if v.trim_start_matches(py_space).starts_with('=') {
        return false;
    }
    v.chars().any(py_alpha)
}

pub fn py_alpha(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphabetic();
    }
    bilingua_model::py_isalpha_char(c)
}

/// Why a non-blank cell is not a unit.
pub fn skip_reason(v: &str) -> Option<&'static str> {
    if blank(v) {
        None
    } else if v.trim_start_matches(py_space).starts_with('=') {
        Some("formula")
    } else if !v.chars().any(py_alpha) {
        Some("no letters")
    } else {
        None
    }
}
