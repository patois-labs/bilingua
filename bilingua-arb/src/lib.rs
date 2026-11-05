//! Bilingua filter for Flutter ARB (Application Resource Bundle) files.
//!
//! An ARB file is a JSON object. Plain keys hold ICU MessageFormat messages; `@key` objects hold the metadata of
//! that message (description, placeholders, type, context) and `@@` keys are file-wide attributes (`@@locale`).
//! Only the messages are translated. A message with `plural`, `selectordinal` or `select` arguments is split
//! into one unit per branch plus one for the text around the argument, as Localization OS splits it. The file is
//! never re-serialised: the skeleton holds every byte between messages, and only the string token of a changed
//! message is rewritten. When the document has a target language and something was translated, `@@locale` names
//! the target (added as the first entry when the source has none) and every plural follows the target language's
//! CLDR categories, including suffix-style plurals (`itemCountOne`, `itemCountOther`).

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod cldr;
mod icu;
mod scan;

pub use cldr::{categories as plural_categories, BABEL_VERSION};

use bilingua_model::*;
use icu::{Complex, Kind, Part};
use scan::Node;
use std::collections::{BTreeMap, HashMap};

const ID: &str = "arb";
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
/// Key in `Unit::codes` (first unit of each message) that holds the original string token.
const RAW_KEY: &str = "$raw";

/// Size limits. The defaults are far above any real resource file; a caller that takes uploads can lower them.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Largest file in bytes.
    pub max_bytes: usize,
    /// Most units (messages and plural branches) in one file.
    pub max_units: usize,
    /// Deepest JSON nesting.
    pub max_depth: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_bytes: 256 * 1024 * 1024, max_units: 1_000_000, max_depth: 200 }
    }
}

/// The Flutter ARB filter.
pub struct Arb;

impl Arb {
    /// Like [`Filter::extract`] with explicit limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        if bytes.len() > limits.max_bytes {
            return Err(parse_err(format!("the file is larger than {} bytes", limits.max_bytes)));
        }
        if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
            return Err(Error::Unsupported("UTF-16 ARB files; save the file as UTF-8".into()));
        }
        let (body, encoding) = match bytes.strip_prefix(BOM) {
            Some(rest) => (rest, "utf-8-bom"),
            None => (bytes, "utf-8"),
        };
        let text = std::str::from_utf8(body)
            .map_err(|e| parse_err(format!("the file is not valid UTF-8 (at byte {}); re-save it as UTF-8", e.valid_up_to())))?;
        let root = scan_root(text, limits)?;
        let msgs = messages(text, &root);

        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        for m in msgs.into_iter().filter(|m| !m.plan.units.is_empty()) {
            if units.len() + m.plan.units.len() > limits.max_units {
                return Err(parse_err(format!("the file has more than {} units", limits.max_units)));
            }
            if m.start > pos {
                parts.push(SkelPart::Raw(text[pos..m.start].to_string()));
            }
            pos = m.end;
            parts.push(SkelPart::Unit(units.len()));
            let meta = m.meta.map(|n| Meta::read(text, n));
            for (k, up) in m.plan.units.into_iter().enumerate() {
                let mut unit = Unit::new(format!("u{}", units.len() + 1), up.inlines);
                unit.codes = up.codes;
                if let Some(meta) = &meta {
                    unit.context.notes = meta.notes.clone();
                }
                if k == 0 {
                    unit.codes.insert(RAW_KEY.into(), CodeData { standalone: Some(text[m.start..m.end].to_string()), ..Default::default() });
                }
                unit.context.kind = Some("message".into());
                unit.context.key = Some(m.key.clone());
                if !up.path.is_empty() {
                    unit.context.extra.insert("icu".into(), up.path);
                }
                units.push(unit);
            }
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        Ok(Document::new(ID, units, Skeleton::text(parts, encoding, bilingua_model::parser_info!())).with_options(opts))
    }
}

fn parse_err(msg: String) -> Error {
    Error::Parse { format: ID.into(), msg }
}

fn scan_root(text: &str, limits: &Limits) -> Result<scan::Node, Error> {
    let root = scan::scan(text, limits.max_depth).map_err(|e| {
        let (l, c) = scan::line_col(text, e.pos);
        parse_err(format!("{} (line {l}, column {c})", e.msg))
    })?;
    if !matches!(root, Node::Obj { .. }) {
        return Err(parse_err("an ARB file must be a JSON object of messages".into()));
    }
    Ok(root)
}

impl Filter for Arb {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["arb"]
    }

    /// An object with a string `@@locale`: the shape that marks a JSON file as ARB.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
        let Ok(text) = std::str::from_utf8(body) else { return false };
        if !text.trim_start().starts_with('{') {
            return false;
        }
        let Ok(Node::Obj { pairs, .. }) = scan::scan(text, 200) else { return false };
        // the last `@@locale` counts, as in `json.loads` (LocOS's sniff)
        pairs.iter().rev().find(|p| p.key == "@@locale").is_some_and(|p| matches!(p.value, Node::Str { .. }))
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &Limits::default())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("arb needs a text skeleton".into()));
        };
        // The original text: skeleton pieces with each message's original token put back.
        let mut orig = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => orig.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    let raw = u.codes.get(RAW_KEY).and_then(|c| c.standalone.as_deref());
                    orig.push_str(raw.ok_or_else(|| Error::Write(format!("unit {} lost the original message", u.id)))?);
                }
            }
        }
        let touched = |u: &Unit| u.segments.iter().any(|s| s.has_target());
        let stamp = opts.stamp_lang(doc, false).is_some();
        let out = if stamp || doc.units.iter().any(touched) { splice(&orig, doc)? } else { orig };
        let mut bytes = Vec::with_capacity(out.len() + 3);
        if encoding == "utf-8-bom" {
            bytes.extend_from_slice(BOM);
        }
        bytes.extend_from_slice(out.as_bytes());
        Ok(bytes)
    }
}

/// The JSON pointer-free keys of all units' messages, in order (for tools and tests).
pub fn keys(doc: &Document) -> Vec<String> {
    doc.units.iter().filter_map(|u| u.context.key.clone()).collect()
}

// ---------------------------------------------------------------------------------------------------------
// Messages and units
// ---------------------------------------------------------------------------------------------------------

/// What a tag id in a unit stands for.
#[derive(Debug, Clone)]
enum Role {
    /// A simple argument, written back as its raw text.
    Simple(String),
    /// `#` or a complex argument: index into the level's pieces; true for a complex argument.
    Token(usize, bool),
}

struct UnitPlan {
    inlines: Vec<Inline>,
    roles: Vec<Role>,
    codes: BTreeMap<String, CodeData>,
    /// Where the unit sits in the message: "count: plural one", empty for the message's own text.
    path: String,
}

struct Plan {
    units: Vec<UnitPlan>,
    /// The ICU parse failed: the whole string is one opaque unit.
    fallback: bool,
}

struct Msg<'a> {
    key: String,
    start: usize,
    end: usize,
    value: String,
    plan: Plan,
    meta: Option<&'a Node>,
}

/// The messages of the root object (plain keys with a string value), in file order. A message may have no unit
/// (nothing to translate) and still be rewritten on a target-language merge: its plurals follow the target.
fn messages<'a>(text: &str, root: &'a Node) -> Vec<Msg<'a>> {
    let Node::Obj { pairs, .. } = root else { return vec![] };
    let mut meta: HashMap<&str, &Node> = HashMap::new();
    for p in pairs {
        if let Some(k) = p.key.strip_prefix('@') {
            if !k.starts_with('@') {
                meta.insert(k, &p.value);
            }
        }
    }
    let mut out = Vec::new();
    for p in pairs {
        if p.key.starts_with('@') {
            continue;
        }
        let Node::Str { start, end } = p.value else { continue };
        let Some(value) = scan::unescape(&text[start + 1..end - 1]) else { continue };
        let plan = plan_message(&value);
        out.push(Msg { key: p.key.clone(), start, end, value, plan, meta: meta.get(p.key.as_str()).copied() });
    }
    out
}

fn plan_message(value: &str) -> Plan {
    match icu::parse(value) {
        Ok(parts) => {
            let mut units = Vec::new();
            collect(&parts, String::new(), &mut units);
            Plan { units, fallback: false }
        }
        Err(_) => {
            let units = if bilingua_model::py_has_alnum(value) { vec![fallback_unit(value)] } else { vec![] };
            Plan { units, fallback: true }
        }
    }
}

/// Pre-order: this level's text if it has words, then each complex argument's branches.
fn collect(parts: &[Part], path: String, out: &mut Vec<UnitPlan>) {
    if icu::translatable(parts) {
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

/// The argument name inside `{name, plural, ...`.
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
                codes.insert(id, CodeData { standalone: Some(raw.clone()), label: Some("placeholder".into()), ..Default::default() });
                roles.push(Role::Simple(raw.clone()));
            }
            Part::Pound => {
                inlines.push(Inline::tag(id.clone(), TagKind::Standalone, "#"));
                codes.insert(id, CodeData { standalone: Some("#".into()), label: Some("number".into()), ..Default::default() });
                roles.push(Role::Token(tokens, false));
                tokens += 1;
            }
            Part::Complex(cx) => {
                let display = format!("{} ...}}", cx.head.trim_end());
                inlines.push(Inline::tag(id.clone(), TagKind::Standalone, display.clone()));
                codes.insert(id, CodeData { standalone: Some(display), label: Some("plural or select".into()), ..Default::default() });
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
                codes.insert(id, CodeData { standalone: Some(raw.clone()), label: Some("placeholder".into()), ..Default::default() });
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

// ---------------------------------------------------------------------------------------------------------
// @key metadata: notes
// ---------------------------------------------------------------------------------------------------------

/// `description`, `context` and the placeholders' `type` and `example` as notes. Placeholder tags keep the label
/// `placeholder` and their exact source text as display: LocOS keeps `{name}` as plain text in the unit, so the
/// LocOS adapter flattens a `placeholder` tag back to its display.
struct Meta {
    notes: Vec<String>,
}

impl Meta {
    fn read(text: &str, node: &Node) -> Meta {
        let mut m = Meta { notes: vec![] };
        let Node::Obj { pairs, .. } = node else { return m };
        let mut ph_notes = Vec::new();
        for p in pairs {
            match (p.key.as_str(), &p.value) {
                ("description" | "context", Node::Str { start, end }) => {
                    if let Some(s) = scan::unescape(&text[start + 1..end - 1]) {
                        if !s.trim().is_empty() {
                            m.notes.push(s);
                        }
                    }
                }
                ("placeholders", Node::Obj { pairs: phs, .. }) => {
                    for ph in phs {
                        let Node::Obj { pairs: attrs, .. } = &ph.value else { continue };
                        let get = |name: &str| {
                            attrs.iter().rev().find(|a| a.key == name).and_then(|a| match a.value {
                                Node::Str { start, end } => scan::unescape(&text[start + 1..end - 1]),
                                _ => None,
                            })
                        };
                        let (ty, ex) = (get("type"), get("example"));
                        let detail = match (ty, ex) {
                            (Some(t), Some(e)) => format!("{t}, e.g. {e}"),
                            (Some(t), None) => t,
                            (None, Some(e)) => format!("e.g. {e}"),
                            (None, None) => continue,
                        };
                        ph_notes.push(format!("{{{}}}: {detail}", ph.key));
                    }
                }
                _ => {}
            }
        }
        m.notes.extend(ph_notes);
        m
    }
}

// ---------------------------------------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------------------------------------

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

fn splice(orig: &str, doc: &Document) -> Result<String, Error> {
    let limits = Limits::default();
    let root = scan_root(orig, &limits).map_err(|e| Error::Write(format!("the skeleton is not an ARB file any more: {e}")))?;
    let Node::Obj { pairs, start: root_start, end: root_end } = &root else { return Err(Error::Write("not an object".into())) };
    let target = doc.target_lang.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let msgs = messages(orig, &root);

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    // Added first so that, at the same position, @@locale comes before added plural keys.
    if let Some(t) = target {
        edits.push(locale_edit(orig, &root, t));
    }
    let mut changed: HashMap<usize, String> = HashMap::new(); // value start -> new token
    let mut head = 0usize;
    for m in &msgs {
        let k = m.plan.units.len();
        let units = doc.units.get(head..head + k).ok_or_else(|| Error::Write("the units no longer match the skeleton".into()))?;
        if units.first().is_some_and(|u| u.codes.get(RAW_KEY).and_then(|c| c.standalone.as_deref()) != Some(&orig[m.start..m.end])) {
            return Err(Error::Write("the units no longer match the skeleton".into()));
        }
        head += k;
        let touched = units.iter().any(|u| u.segments.iter().any(|s| s.has_target()));
        if !touched && target.is_none() {
            continue;
        }
        let mut tr = Trans {
            list: units.iter().map(|u| if u.segments.iter().all(|s| !s.has_target()) { None } else { Some(u.output()) }).collect(),
            at: 0,
        };
        let new = rebuild_message(m, &mut tr, target);
        if new != m.value {
            let token = scan::quote(&new);
            changed.insert(m.start, token.clone());
            edits.push((m.start, m.end, token));
        }
    }
    if let Some(t) = target {
        suffix_plural_edits(orig, pairs, *root_start, t, &changed, &mut edits);
    }
    let _ = root_end;
    edits.sort_by_key(|e| e.0);
    let mut out = String::with_capacity(orig.len() + 64);
    let mut pos = 0usize;
    for (s, e, rep) in edits {
        if s < pos {
            continue;
        }
        out.push_str(&orig[pos..s]);
        out.push_str(&rep);
        pos = e;
    }
    out.push_str(&orig[pos..]);
    Ok(out)
}

fn rebuild_message(m: &Msg, tr: &mut Trans, target: Option<&str>) -> String {
    if m.plan.fallback {
        let Some(t) = tr.next() else { return m.value.clone() };
        let roles = &m.plan.units[0].roles;
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
    match icu::parse(&m.value) {
        Ok(parts) => rebuild_level(&parts, tr, target),
        Err(_) => m.value.clone(),
    }
}

fn rebuild_level(parts: &[Part], tr: &mut Trans, target: Option<&str>) -> String {
    let translatable = icu::translatable(parts);
    let mine = if translatable { tr.next() } else { None };
    let mut pieces: Vec<String> = Vec::new();
    for p in parts {
        match p {
            Part::Pound => pieces.push("#".into()),
            Part::Complex(cx) => {
                let rendered: Vec<String> = cx.branches.iter().map(|b| rebuild_level(&b.msg, tr, target)).collect();
                pieces.push(render_branches(cx, rendered, target));
            }
            _ => {}
        }
    }
    if let Some(inl) = mine {
        let roles = level_unit(parts, "").roles;
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
                    Role::Simple(raw) => {
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
    let cats = if cx.kind == Kind::Select { None } else { target.and_then(|t| cldr::categories(t, cx.kind == Kind::SelectOrdinal)) };
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

/// `\n` or `\r\n`, whichever line ending the newline at `nl` ends.
fn newline_at(text: &str, nl: usize) -> &'static str {
    if nl > 0 && text.as_bytes()[nl - 1] == b'\r' {
        "\r\n"
    } else {
        "\n"
    }
}

/// Sets `@@locale` to the target, or adds it as the first entry, indented like the first key.
fn locale_edit(text: &str, root: &Node, target: &str) -> (usize, usize, String) {
    let loc = scan::quote(&target.trim().replace('-', "_"));
    let Node::Obj { pairs, start, end } = root else { return (0, 0, String::new()) };
    if let Some(p) = pairs.iter().rev().find(|p| p.key == "@@locale") {
        return (p.value.start(), p.value.end(), loc);
    }
    if let Some(first) = pairs.first() {
        let key_start = first.key_start;
        let sep = match text[*start..key_start].rfind('\n') {
            Some(rel) => {
                let nl = start + rel;
                format!("{}{}", newline_at(text, nl), &text[nl + 1..key_start])
            }
            None => " ".to_string(),
        };
        return (key_start, key_start, format!("\"@@locale\": {loc},{sep}"));
    }
    (*start, *end, format!("{{\"@@locale\": {loc}}}"))
}

/// Suffix-style plurals (`itemCountOne`, `itemCountOther`): a message for each category the target uses that the
/// group lacks, holding the `Other` message, placed before it.
fn suffix_plural_edits(text: &str, pairs: &[scan::Pair], root_start: usize, target: &str, changed: &HashMap<usize, String>, edits: &mut Vec<(usize, usize, String)>) {
    const WORDS: [&str; 6] = ["Zero", "One", "Two", "Few", "Many", "Other"];
    let all_keys: std::collections::HashSet<&str> = pairs.iter().map(|p| p.key.as_str()).collect();
    let mut order: Vec<&str> = Vec::new();
    let mut groups: HashMap<&str, HashMap<&str, (usize, &Node)>> = HashMap::new();
    for p in pairs {
        if !matches!(p.value, Node::Str { .. }) || !p.key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            continue;
        }
        for w in WORDS {
            if let Some(stem) = p.key.strip_suffix(w) {
                if !stem.is_empty() {
                    if !groups.contains_key(stem) {
                        order.push(stem);
                    }
                    groups.entry(stem).or_default().insert(w, (p.key_start, &p.value));
                    break;
                }
            }
        }
    }
    let Some(cats) = cldr::categories(target, false) else { return };
    for stem in order {
        let nodes = &groups[stem];
        let Some(&(key_at, other)) = nodes.get("Other") else { continue };
        if nodes.len() < 2 {
            continue;
        }
        let missing: Vec<String> = cats
            .iter()
            .map(|c| {
                let mut w = c.to_string();
                w[..1].make_ascii_uppercase();
                w
            })
            .filter(|w| !nodes.contains_key(w.as_str()) && !all_keys.contains(format!("{stem}{w}").as_str()))
            .collect();
        if missing.is_empty() {
            continue;
        }
        let value = changed.get(&other.start()).cloned().unwrap_or_else(|| text[other.start()..other.end()].to_string());
        let sep = match text[root_start..key_at].rfind('\n') {
            Some(rel) => {
                let nl = root_start + rel;
                let indent = &text[nl + 1..key_at];
                if indent.trim().is_empty() {
                    format!("{}{indent}", newline_at(text, nl))
                } else {
                    " ".to_string()
                }
            }
            None => " ".to_string(),
        };
        let rep: String = missing.iter().map(|w| format!("{}: {value},{sep}", scan::quote(&format!("{stem}{w}")))).collect();
        edits.push((key_at, key_at, rep));
    }
}

#[cfg(test)]
mod tests;
