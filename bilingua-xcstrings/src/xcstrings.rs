//! The String Catalog (`.xcstrings`) filter. Behaviour follows Localization OS (`apple_filter.py`, section
//! xcstrings); see the crate docs.

use crate::json::{self, Kind, Node, Obj, Style, J};
use crate::{ph, plural};
use bilingua_model::guard;
use bilingua_model::{Document, Error, ExtractOptions, Filter, LimitError, LimitKind, Limits, SkelPart, Skeleton, Unit};

pub(crate) const FORMAT: &str = "xcstrings";
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
/// Deepest nesting of variations and substitutions below a localization (LocOS `_loc_leaves`).
const MAX_LEAF_DEPTH: usize = 16;

/// The Xcode String Catalog filter.
#[derive(Debug, Default, Clone, Copy)]
pub struct Xcstrings;

fn parse_err(msg: impl Into<String>) -> Error {
    Error::Parse { format: FORMAT.into(), msg: msg.into() }
}

fn nested() -> Error {
    LimitError::new(LimitKind::XmlDepth, "This String Catalog is nested too deeply.").into()
}

/// A catalog read into a tree that remembers where everything is in the text.
pub(crate) struct Catalog {
    pub text: String,
    pub bom: bool,
    pub root: Node,
}

impl Catalog {
    /// `budget`: the pre-parse limits of the extract call (merge reads a file already accepted).
    pub fn load(bytes: &[u8], limits: &Limits, budget: bool) -> Result<Catalog, Error> {
        let (bom, body) = match bytes.strip_prefix(BOM) {
            Some(r) => (true, r),
            None => (false, bytes),
        };
        let text = std::str::from_utf8(body).map_err(|e| {
            let at = e.valid_up_to() + (bytes.len() - body.len());
            Error::Encoding(format!("String Catalog is not valid UTF-8 (byte 0x{:02x} at position {at}); re-save it from Xcode.", bytes[at]))
        })?;
        Catalog::from_text(text.to_string(), bom, limits, budget)
    }

    pub fn from_text(text: String, bom: bool, limits: &Limits, budget: bool) -> Result<Catalog, Error> {
        if budget {
            // LocOS `check_json_budget`: quotes and structure characters bound the work before a tree is built.
            let (mut quotes, mut structure) = (0usize, 0usize);
            for &c in text.as_bytes() {
                match c {
                    b'"' => quotes += 1,
                    b',' | b'[' | b'{' => structure += 1,
                    _ => {}
                }
            }
            if quotes > limits.max_units.saturating_mul(40) {
                return Err(LimitError::new(
                    LimitKind::Units,
                    format!("This String Catalog has far more strings than the limit of {} segments allows. Split it into smaller files.", limits.max_units),
                )
                .into());
            }
            if structure > limits.max_units.saturating_mul(20) {
                return Err(LimitError::new(
                    LimitKind::Units,
                    format!("This String Catalog has far more values than the limit of {} segments allows. Split it into smaller files.", limits.max_units),
                )
                .into());
            }
        }
        let root = json::parse(&text, limits.max_xml_depth).map_err(|e| match e {
            json::JsonError::Deep => nested(),
            json::JsonError::Syntax(m) => parse_err(format!("String Catalog is not valid JSON ({m})")),
        })?;
        let ok = root.obj().is_some_and(|o| o.get("strings").is_some_and(|s| s.obj().is_some()) && o.get("sourceLanguage").is_some_and(|s| s.str().is_some()));
        if !ok {
            return Err(parse_err("not a String Catalog: expected an object with sourceLanguage and strings"));
        }
        Ok(Catalog { text, bom, root })
    }

    pub fn source(&self) -> &str {
        self.root.obj().and_then(|o| o.get("sourceLanguage")).and_then(Node::str).unwrap_or("")
    }

    fn strings(&self) -> &Obj {
        self.root.obj().and_then(|o| o.get("strings")).and_then(Node::obj).expect("checked in load")
    }
}

pub(crate) struct Leaf {
    pub path: Vec<String>,
    pub value: String,
    /// The byte range of the value's string literal.
    pub span: (usize, usize),
    /// Index among the translatable units.
    pub idx: Option<usize>,
}

pub(crate) struct Entry<'a> {
    pub key: &'a str,
    pub node: &'a Node,
    pub obj: &'a Obj,
    /// The source-language localization when the catalog has one.
    pub src: Option<&'a Node>,
    pub wanted: bool,
    pub leaves: Vec<Leaf>,
}

fn leaves(loc: &Node, path: &mut Vec<String>, depth: usize, out: &mut Vec<(Vec<String>, String, (usize, usize))>) -> Result<(), Error> {
    if depth > MAX_LEAF_DEPTH {
        return Err(nested());
    }
    let Some(o) = loc.obj() else { return Ok(()) };
    for (key, val) in o.items() {
        match key {
            "stringUnit" => {
                if let Some(v) = val.obj().and_then(|su| su.get("value")) {
                    if let Some(s) = v.str() {
                        let mut p = path.clone();
                        p.push("stringUnit".into());
                        out.push((p, s.to_string(), (v.start, v.end)));
                    }
                }
            }
            "stringSet" => {
                // App Shortcuts phrases: one unit per string of `values`, numbered by position
                if let Some(Kind::Arr(items)) = val.obj().and_then(|so| so.get("values")).map(|v| &v.kind) {
                    for (i, v) in items.iter().enumerate() {
                        if let Some(s) = v.str() {
                            let mut p = path.clone();
                            p.extend(["stringSet".to_string(), i.to_string()]);
                            out.push((p, s.to_string(), (v.start, v.end)));
                        }
                    }
                }
            }
            "variations" => {
                if let Some(vo) = val.obj() {
                    for (vtype, vmap) in vo.items() {
                        if let Some(vm) = vmap.obj() {
                            for (cat, sub) in vm.items() {
                                let n = path.len();
                                path.extend(["variations".to_string(), vtype.to_string(), cat.to_string()]);
                                leaves(sub, path, depth + 1, out)?;
                                path.truncate(n);
                            }
                        }
                    }
                }
            }
            "substitutions" => {
                if let Some(so) = val.obj() {
                    for (name, sub) in so.items() {
                        let n = path.len();
                        path.extend(["substitutions".to_string(), name.to_string()]);
                        leaves(sub, path, depth + 1, out)?;
                        path.truncate(n);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn entry_wanted(text: &str, o: &Obj) -> bool {
    let should_not = o.get("shouldTranslate").is_some_and(|n| matches!(n.kind, Kind::Other) && &text[n.start..n.end] == "false");
    let stale = o.get("extractionState").and_then(Node::str) == Some("stale");
    !should_not && !stale
}

impl Catalog {
    /// Every entry with its leaves in file order; translatable leaves are numbered.
    pub fn walk(&self, limits: &Limits) -> Result<Vec<Entry<'_>>, Error> {
        let source = self.source();
        let mut n = 0usize;
        let mut out = Vec::new();
        for (key, node) in self.strings().items() {
            let Some(obj) = node.obj() else { continue };
            let src = obj.get("localizations").and_then(Node::obj).and_then(|l| l.get(source)).filter(|s| s.obj().is_some());
            let wanted = entry_wanted(&self.text, obj);
            let mut raw = Vec::new();
            match src {
                Some(s) => leaves(s, &mut Vec::new(), 0, &mut raw)?,
                None => raw.push((vec!["stringUnit".to_string()], key.to_string(), (0, 0))),
            }
            let mut ls = Vec::with_capacity(raw.len());
            for (path, value, span) in raw {
                let mut idx = None;
                if wanted && ph::is_translatable(&value) {
                    idx = Some(n);
                    n += 1;
                    guard::check_unit_count(n, limits)?;
                }
                ls.push(Leaf { path, value, span, idx });
            }
            out.push(Entry { key, node, obj, src, wanted, leaves: ls });
        }
        Ok(out)
    }
}

/// Source inlines and codes for one leaf value.
fn unit_for(id: String, e: &Entry<'_>, leaf: &Leaf, source: &str, limits: &Limits) -> Result<Unit, Error> {
    let (_, core, _) = ph::split_ws(&leaf.value);
    if core.len() > limits.max_unit_chars {
        guard::check_unit_chars(core.chars().count(), limits)?;
    }
    let mut codes = std::collections::BTreeMap::new();
    let inl = ph::inlines(core, &mut codes);
    let mut u = Unit::new(id, inl);
    u.codes = codes;
    u.context.kind = Some("string".into());
    u.context.key = Some(e.key.to_string());
    let p = &leaf.path;
    let mut loc = String::new();
    if e.src.is_some() {
        loc.push_str("localizations.");
        loc.push_str(source);
        let in_set = p.len() >= 2 && p[p.len() - 2] == "stringSet";
        for part in &p[..p.len() - 1] {
            loc.push('.');
            loc.push_str(part);
        }
        if in_set {
            loc.push('[');
            loc.push_str(&p[p.len() - 1]);
            loc.push(']');
            u.context.extra.insert("string_set_index".into(), p[p.len() - 1].clone());
            u.context.kind = Some("stringSet".into());
        }
    } else {
        loc.push_str("key");
    }
    u.context.location = Some(loc);
    let mut i = 0;
    while i + 1 < p.len() {
        match p[i].as_str() {
            "variations" if i + 2 < p.len() => {
                let v = format!("{}:{}", p[i + 1], p[i + 2]);
                let slot = u.context.extra.entry("variation".into()).or_default();
                if !slot.is_empty() {
                    slot.push(' ');
                }
                slot.push_str(&v);
                i += 3;
            }
            "substitutions" if i + 1 < p.len() => {
                u.context.extra.insert("substitution".into(), p[i + 1].clone());
                i += 2;
            }
            _ => i += 1,
        }
    }
    if let Some(c) = e.obj.get("comment").and_then(Node::str) {
        if !c.is_empty() {
            u.context.notes.push(c.to_string());
        }
    }
    Ok(u)
}

impl Filter for Xcstrings {
    fn id(&self) -> &'static str {
        FORMAT
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["xcstrings"]
    }

    /// A JSON object with a string `sourceLanguage` and an object `strings`. Fairly strict (the file is parsed),
    /// but it is still JSON: put it before the generic JSON filter.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(64 * 1024)];
        let has = |w: &[u8]| head.windows(w.len()).any(|x| x == w);
        if !has(b"\"sourceLanguage\"") || !has(b"\"strings\"") {
            return false;
        }
        Catalog::load(bytes, &Limits::default(), true).is_ok()
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let limits = opts.limits();
        guard::check_input(bytes, &limits)?;
        let cat = Catalog::load(bytes, &limits, true)?;
        let source = cat.source().to_string();
        let entries = cat.walk(&limits)?;
        let mut units = Vec::new();
        let mut locked = 0usize;
        for e in &entries {
            for leaf in &e.leaves {
                match leaf.idx {
                    Some(i) => units.push(unit_for(format!("u{}", i + 1), e, leaf, &source, &limits)?),
                    None if opts.include_locked && !ph::split_ws(&leaf.value).1.is_empty() => {
                        locked += 1;
                        let mut u = unit_for(format!("l{locked}"), e, leaf, &source, &limits)?;
                        u.translate = false;
                        u.lock_reason = Some(
                            if !e.wanted {
                                if e.obj.get("extractionState").and_then(Node::str) == Some("stale") {
                                    "stale string (no longer in the code)"
                                } else {
                                    "marked shouldTranslate false"
                                }
                            } else {
                                "no translatable text"
                            }
                            .into(),
                        );
                        units.push(u);
                    }
                    None => {}
                }
            }
        }
        let doc = Document::new(FORMAT, units, Skeleton::text(
                vec![SkelPart::Raw(cat.text.clone())],
                if cat.bom { "utf-8-bom" } else { "utf-8" },
                bilingua_model::parser_info!(),
            )).with_source_lang(opts.source_lang.clone().or(Some(source))).with_target_lang(opts.target_lang.clone()).with_granularity(opts.granularity);
        guard::finish(doc, vec![], &limits)
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        // JSON is UTF-8 in and out (BOM kept); the target language names where translations go, it is not a
        // stamp, so with nothing translated there is nothing to write
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("xcstrings needs a text skeleton".into()));
        };
        let bom = match encoding.as_str() {
            "utf-8" => false,
            "utf-8-bom" => true,
            other => return Err(Error::Write(format!("xcstrings cannot write the encoding {other}"))),
        };
        let mut text = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => text.push_str(s),
                SkelPart::Unit(_) => return Err(Error::Write("xcstrings skeleton holds a unit slot".into())),
            }
        }
        let limits = Limits::default();
        let cat = Catalog::from_text(text, bom, &limits, false)?;
        // The file was accepted when it was extracted: no unit cap here.
        let unlimited = limits.with_max_units(usize::MAX);
        let entries = cat.walk(&unlimited)?;
        let out = apply(&cat, &entries, doc)?;
        let mut bytes = Vec::with_capacity(out.len() + 3);
        if bom {
            bytes.extend_from_slice(BOM);
        }
        bytes.extend_from_slice(out.as_bytes());
        Ok(bytes)
    }
}

// ---------------------------------------------------------------------------------------------------------
// Merge

type Trans = Vec<(Vec<String>, String)>;

fn lookup<'a>(t: &'a Trans, path: &[String]) -> Option<&'a str> {
    t.iter().find(|(p, _)| p.as_slice() == path).map(|(_, v)| v.as_str())
}

/// What the writer may use of the source localization.
struct Shape {
    items: Vec<ShapeItem>,
}
/// A plural or device category, with its shape when its value is an object.
type Category = (String, Option<Shape>);
/// A substitution: name, shape, copied `argNum` / `formatSpecifier`.
type Substitution = (String, Shape, Vec<(String, J)>);

enum ShapeItem {
    StringUnit,
    /// The source strings of a `stringSet`.
    StringSet(Vec<String>),
    /// variation type -> (category, shape or None for a value that is not an object)
    Variations(Vec<(String, Vec<Category>)>),
    /// name, shape, copied `argNum` / `formatSpecifier`
    Substitutions(Vec<Substitution>),
}

fn shape_of(text: &str, loc: &Node) -> Shape {
    let mut items = Vec::new();
    if let Some(o) = loc.obj() {
        for (key, val) in o.items() {
            match key {
                "stringUnit" if val.obj().is_some() => items.push(ShapeItem::StringUnit),
                "stringSet" => {
                    if let Some(Kind::Arr(vals)) = val.obj().and_then(|so| so.get("values")).map(|v| &v.kind) {
                        items.push(ShapeItem::StringSet(vals.iter().filter_map(|v| v.str().map(str::to_string)).collect()));
                    }
                }
                "variations" => {
                    if let Some(vo) = val.obj() {
                        let mut v = Vec::new();
                        for (vtype, vmap) in vo.items() {
                            if let Some(vm) = vmap.obj() {
                                v.push((vtype.to_string(), vm.items().into_iter().map(|(c, s)| (c.to_string(), s.obj().map(|_| shape_of(text, s)))).collect()));
                            }
                        }
                        items.push(ShapeItem::Variations(v));
                    }
                }
                "substitutions" => {
                    if let Some(so) = val.obj() {
                        let mut v = Vec::new();
                        for (name, sub) in so.items() {
                            if let Some(sobj) = sub.obj() {
                                let meta = sobj
                                    .items()
                                    .into_iter()
                                    .filter(|(k, _)| matches!(*k, "argNum" | "formatSpecifier"))
                                    .map(|(k, x)| (k.to_string(), J::from_node(text, x)))
                                    .collect();
                                v.push((name.to_string(), shape_of(text, sub), meta));
                            }
                        }
                        items.push(ShapeItem::Substitutions(v));
                    }
                }
                _ => {}
            }
        }
    }
    Shape { items }
}

fn translated_unit(value: &str) -> J {
    J::Obj(vec![("state".into(), J::Str("translated".into())), ("value".into(), J::Str(value.into()))])
}

/// The target localization for one string: the source shape, translated leaves (or the existing target leaf
/// when that unit has no translation). LocOS `_mirror`.
fn mirror(shape: &Shape, old: Option<&J>, trans: &Trans, path: &mut Vec<String>, cats: Option<&[&'static str]>) -> Vec<(String, J)> {
    let old = old.filter(|o| matches!(o, J::Obj(_)));
    let mut out: Vec<(String, J)> = Vec::new();
    for item in &shape.items {
        match item {
            ShapeItem::StringUnit => {
                path.push("stringUnit".into());
                let t = lookup(trans, path);
                path.pop();
                if let Some(v) = t {
                    out.push(("stringUnit".into(), translated_unit(v)));
                } else if let Some(o) = old.and_then(|o| o.get("stringUnit")).filter(|o| matches!(o, J::Obj(_))) {
                    out.push(("stringUnit".into(), o.clone()));
                }
            }
            ShapeItem::StringSet(src) => {
                let oset = old.and_then(|o| o.get("stringSet")).filter(|o| matches!(o, J::Obj(_)));
                let oldv: Vec<&str> = match oset.and_then(|o| o.get("values")) {
                    Some(J::Arr(a)) => a.iter().filter_map(|x| if let J::Str(s) = x { Some(s.as_str()) } else { None }).collect(),
                    _ => Vec::new(),
                };
                let n = path.len();
                path.push("stringSet".into());
                let mut any = false;
                let mut vals: Vec<J> = Vec::with_capacity(src.len());
                for (i, sv) in src.iter().enumerate() {
                    path.push(i.to_string());
                    let t = lookup(trans, path);
                    path.pop();
                    any |= t.is_some();
                    // values are positional: a string without a translation keeps the target's old string, else the source
                    vals.push(J::Str(t.or_else(|| oldv.get(i).copied()).unwrap_or(sv).to_string()));
                }
                path.truncate(n);
                if any {
                    out.push(("stringSet".into(), J::Obj(vec![("state".into(), J::Str("translated".into())), ("values".into(), J::Arr(vals))])));
                } else if let Some(o) = oset {
                    out.push(("stringSet".into(), o.clone()));
                }
            }
            ShapeItem::Variations(types) => {
                let ovar = old.and_then(|o| o.get("variations")).filter(|o| matches!(o, J::Obj(_)));
                let mut vout: Vec<(String, J)> = Vec::new();
                for (vtype, vm) in types {
                    let has = |c: &str| vm.iter().any(|(k, _)| k == c);
                    let order: Vec<String> = match cats {
                        Some(cs) if vtype == "plural" && !cs.is_empty() => {
                            let keep_zero = has("zero") && !cs.contains(&"zero");
                            plural::ORDER.iter().filter(|c| cs.contains(c) || (keep_zero && **c == "zero")).map(|c| c.to_string()).collect()
                        }
                        _ => vm.iter().map(|(k, _)| k.clone()).collect(),
                    };
                    let omap = ovar.and_then(|o| o.get(vtype)).filter(|o| matches!(o, J::Obj(_)));
                    let mut cout: Vec<(String, J)> = Vec::new();
                    for cat in order {
                        let scat = if has(&cat) {
                            cat.as_str()
                        } else if has("other") {
                            "other"
                        } else {
                            continue;
                        };
                        let Some(Some(sshape)) = vm.iter().find(|(k, _)| k == scat).map(|(_, s)| s) else { continue };
                        let n = path.len();
                        path.extend(["variations".to_string(), vtype.clone(), scat.to_string()]);
                        let sub = mirror(sshape, omap.and_then(|o| o.get(&cat)), trans, path, cats);
                        path.truncate(n);
                        if !sub.is_empty() {
                            cout.push((cat.clone(), J::Obj(sub)));
                        }
                    }
                    if !cout.is_empty() {
                        vout.push((vtype.clone(), J::Obj(cout)));
                    }
                }
                if !vout.is_empty() {
                    out.push(("variations".into(), J::Obj(vout)));
                }
            }
            ShapeItem::Substitutions(subs) => {
                let osub = old.and_then(|o| o.get("substitutions")).filter(|o| matches!(o, J::Obj(_)));
                let mut sout: Vec<(String, J)> = Vec::new();
                for (name, sshape, meta) in subs {
                    let n = path.len();
                    path.extend(["substitutions".to_string(), name.clone()]);
                    let m = mirror(sshape, osub.and_then(|o| o.get(name)), trans, path, cats);
                    path.truncate(n);
                    if !m.is_empty() {
                        let mut all = meta.clone();
                        all.extend(m);
                        sout.push((name.clone(), J::Obj(all)));
                    }
                }
                if !sout.is_empty() {
                    out.push(("substitutions".into(), J::Obj(sout)));
                }
            }
        }
    }
    out
}

fn norm(s: &str) -> String {
    s.replace('_', "-").to_lowercase()
}

struct Edit {
    start: usize,
    end: usize,
    text: String,
}

/// An edit that adds `key: value` to the object `o` (at nesting `depth`) at the end, or before `before` (the
/// index of a member).
fn insert_member(text: &str, st: &Style, o: &Obj, depth: usize, key: &str, v: &J, before: Option<usize>) -> Edit {
    let member = st.member(key, v, depth);
    let sep = if o.members.len() >= 2 { text[o.members[0].val.end..o.members[1].key_start].to_string() } else { st.separator(depth) };
    match before {
        Some(j) => Edit { start: o.members[j].key_start, end: o.members[j].key_start, text: format!("{member}{sep}") },
        None => {
            let at = o.members.last().map_or(0, |m| m.val.end);
            Edit { start: at, end: at, text: format!("{sep}{member}") }
        }
    }
}

fn replace_node(st: &Style, n: &Node, v: &J, depth: usize) -> Edit {
    let mut s = String::new();
    st.emit(v, depth, &mut s);
    Edit { start: n.start, end: n.end, text: s }
}

/// Sets `entry["localizations"][lang] = newj` where the localizations object may be missing or empty.
/// The language being written: its key, the member it replaces, and whether the catalog keeps languages sorted.
#[derive(Clone, Copy)]
struct Slot<'a> {
    lang: &'a str,
    existing: Option<&'a Node>,
    was_sorted: bool,
}

fn set_locale(text: &str, st: &Style, e: &Entry<'_>, slot: &Slot<'_>, newj: J, edits: &mut Vec<Edit>) {
    let Slot { lang, existing, was_sorted } = *slot;
    let locs_member = e.obj.members.iter().rev().find(|m| m.key == "localizations");
    let locs_obj = locs_member.and_then(|m| m.val.obj());
    match (locs_member, locs_obj) {
        (_, Some(lo)) if !lo.members.is_empty() => match existing {
            Some(n) => edits.push(replace_node(st, n, &newj, 4)),
            None => {
                let before = if was_sorted { lo.members.iter().position(|m| m.key.as_str() > lang) } else { None };
                edits.push(insert_member(text, st, lo, 3, lang, &newj, before));
            }
        },
        (Some(m), _) => {
            // missing members, or a value that is not an object: the whole value is replaced
            edits.push(replace_node(st, &m.val, &J::Obj(vec![(lang.to_string(), newj)]), 3));
        }
        (None, _) => {
            let v = J::Obj(vec![(lang.to_string(), newj)]);
            if e.obj.members.is_empty() {
                edits.push(replace_node(st, e.node, &J::Obj(vec![("localizations".into(), v)]), 2));
            } else {
                edits.push(insert_member(text, st, e.obj, 2, "localizations", &v, None));
            }
        }
    }
}

fn apply(cat: &Catalog, entries: &[Entry<'_>], doc: &Document) -> Result<String, Error> {
    let text = &cat.text;
    // translations by unit number
    let mut by_idx: std::collections::HashMap<usize, &Unit> = std::collections::HashMap::new();
    for u in &doc.units {
        if !u.translate {
            continue;
        }
        if let Some(n) = u.id.strip_prefix('u').and_then(|s| s.parse::<usize>().ok()) {
            if n >= 1 && u.segments.iter().any(|s| s.has_target()) {
                by_idx.insert(n - 1, u);
            }
        }
    }
    if by_idx.is_empty() {
        return Ok(text.clone());
    }
    let source = cat.source();
    let target = doc.target_lang.as_deref().map(str::trim).unwrap_or("");
    let in_source = target.is_empty() || norm(target) == norm(source);
    let cats = if in_source { None } else { plural::categories(target) };
    let st = Style::detect(text);
    let mut edits: Vec<Edit> = Vec::new();
    for e in entries {
        let mut trans: Trans = Vec::new();
        for leaf in &e.leaves {
            let Some(idx) = leaf.idx else { continue };
            let Some(u) = by_idx.get(&idx) else { continue };
            let (lead, core, trail) = ph::split_ws(&leaf.value);
            let new_core = ph::render(u, &u.output());
            let full = if new_core == core { leaf.value.clone() } else { format!("{lead}{new_core}{trail}") };
            trans.push((leaf.path.clone(), full));
        }
        if trans.is_empty() {
            continue;
        }
        let locs_member = e.obj.members.iter().rev().find(|m| m.key == "localizations");
        let locs_obj = locs_member.and_then(|m| m.val.obj());
        if in_source {
            if e.src.is_some() {
                for (path, value) in &trans {
                    if let Some(leaf) = e.leaves.iter().find(|l| &l.path == path) {
                        if leaf.value != *value {
                            edits.push(Edit { start: leaf.span.0, end: leaf.span.1, text: st.string(value) });
                        }
                    }
                }
            } else if let Some((_, value)) = trans.iter().find(|(p, _)| p.len() == 1 && p[0] == "stringUnit") {
                if value != e.key {
                    let newj = J::Obj(vec![("stringUnit".into(), translated_unit(value))]);
                    let existing = locs_obj.and_then(|o| o.get(source));
                    set_locale(text, &st, e, &Slot { lang: source, existing, was_sorted: false }, newj, &mut edits);
                }
            }
            continue;
        }
        let items: Vec<(&str, &Node)> = locs_obj.map(Obj::items).unwrap_or_default();
        let want = norm(target);
        let found = items.iter().find(|(k, _)| norm(k) == want).map(|(k, n)| (*k, *n));
        let lang: String = found.map_or_else(|| target.replace('_', "-"), |(k, _)| k.to_string());
        let old_j = found.map(|(_, n)| J::from_node(text, n));
        let shape = match e.src {
            Some(s) => shape_of(text, s),
            None => Shape { items: vec![ShapeItem::StringUnit] },
        };
        let new_loc = mirror(&shape, old_j.as_ref(), &trans, &mut Vec::new(), cats.as_deref());
        if new_loc.is_empty() {
            continue;
        }
        let keys: Vec<&str> = items.iter().map(|(k, _)| *k).collect();
        let was_sorted = keys.windows(2).all(|w| w[0] <= w[1]);
        let mut newj = J::Obj(new_loc);
        if was_sorted {
            newj = newj.sorted_deep();
        }
        if let Some(o) = &old_j {
            if json::same(o, &newj) {
                continue;
            }
        }
        set_locale(text, &st, e, &Slot { lang: &lang, existing: found.map(|(_, n)| n), was_sorted }, newj, &mut edits);
    }
    if edits.is_empty() {
        return Ok(text.clone());
    }
    edits.sort_by_key(|e| (e.start, e.end));
    let mut out = String::with_capacity(text.len() + edits.iter().map(|e| e.text.len()).sum::<usize>());
    let mut pos = 0;
    for e in &edits {
        if e.start < pos {
            return Err(Error::Write("xcstrings: two edits overlap".into()));
        }
        out.push_str(&text[pos..e.start]);
        out.push_str(&e.text);
        pos = e.end;
    }
    out.push_str(&text[pos..]);
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------
// Key-based view (LocOS `records`)

/// One catalog key as a key-based project sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Record {
    pub key: String,
    pub value: String,
    pub comment: String,
    /// Plural forms of the source (or the device variants when there is no plural), category to text.
    pub plural_forms: Option<Vec<(String, String)>>,
    /// Top-level plural category to the unit number (from 0) that carries it.
    pub plural_units: Option<Vec<(String, usize)>>,
    /// The first unit number of the key; `None` when the string is not translated (shouldTranslate false, stale,
    /// or only placeholders).
    pub unit_index: Option<usize>,
}

fn nav<'a>(n: &'a Node, path: &[&str]) -> Option<&'a Node> {
    let mut cur = n;
    for p in path {
        cur = cur.obj()?.get(p)?;
    }
    Some(cur)
}

/// Every key of the catalog in file order, translatable or not.
pub fn records(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    let limits = Limits::default();
    guard::check_input(bytes, &limits)?;
    let cat = Catalog::load(bytes, &limits, true)?;
    let mut out = Vec::new();
    for e in cat.walk(&limits)? {
        let mut value: Option<String> = match e.src {
            Some(s) => nav(s, &["stringUnit", "value"]).and_then(Node::str).map(str::to_string),
            None => Some(e.key.to_string()),
        };
        let variants = e.src.and_then(|s| {
            let non_empty = |n: &Node| n.obj().is_some_and(|o| !o.members.is_empty());
            nav(s, &["variations", "plural"]).filter(|n| non_empty(n)).or_else(|| nav(s, &["variations", "device"]).filter(|n| non_empty(n)))
        });
        let mut forms = None;
        if let Some(vo) = variants.and_then(Node::obj) {
            let f: Vec<(String, String)> = vo
                .items()
                .into_iter()
                .filter_map(|(c, v)| nav(v, &["stringUnit", "value"]).and_then(Node::str).map(|s| (c.to_string(), s.to_string())))
                .collect();
            if value.is_none() {
                value = Some(f.iter().find(|(c, _)| c == "other").map(|(_, v)| v.clone()).filter(|v| !v.is_empty()).or_else(|| f.first().map(|(_, v)| v.clone())).unwrap_or_default());
            }
            forms = Some(f);
        }
        let units: Vec<(String, usize)> =
            e.leaves.iter().filter_map(|l| l.idx.filter(|_| l.path.len() == 4 && l.path[0] == "variations" && l.path[1] == "plural").map(|i| (l.path[2].clone(), i))).collect();
        out.push(Record {
            key: e.key.to_string(),
            value: value.unwrap_or_default(),
            comment: e.obj.get("comment").and_then(Node::str).unwrap_or("").to_string(),
            plural_forms: forms,
            plural_units: (!units.is_empty()).then_some(units),
            unit_index: e.leaves.iter().find_map(|l| l.idx),
        });
    }
    Ok(out)
}
