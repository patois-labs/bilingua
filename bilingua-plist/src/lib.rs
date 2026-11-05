//! Bilingua Apple property list filters: `.plist` (Info.plist, Settings.bundle, generic) and `.stringsdict`, XML or
//! binary. See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod bin;
mod cldr;
mod cldr_table;
mod ph;
mod policy;
mod tree;
mod xml;

use bilingua_model::*;
use policy::{Leaf, Mode};
use std::collections::{BTreeMap, HashMap};
use tree::*;

pub use bilingua_model::Limits;

/// The `.plist` filter: Info.plist keys, Settings.bundle titles and the prose strings of any other plist.
#[derive(Debug, Clone, Copy, Default)]
pub struct Plist;

/// The `.stringsdict` filter: plural, device and width rules; every string except the type tags.
#[derive(Debug, Clone, Copy, Default)]
pub struct Stringsdict;

/// Both filters, `.stringsdict` first (a stringsdict is also a plist).
pub fn filters() -> Vec<Box<dyn Filter>> {
    vec![Box::new(Stringsdict), Box::new(Plist)]
}

// Keys of the unit's hidden values (`Unit::hidden`).
const OV: &str = "ov"; // the value's core text as written (decoded)
const RAW: &str = "raw"; // XML: the content as written in the file
const LEAD: &str = "lead";
const TRAIL: &str = "trail";
const ORD: &str = "ord"; // binary: position of the value among the file's strings

/// Skeleton encoding tag of a binary plist (the skeleton keeps the original file as base64).
const BINARY: &str = "bplist";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fmt {
    Plist,
    Stringsdict,
}

impl Fmt {
    fn id(self) -> &'static str {
        match self {
            Fmt::Plist => "plist",
            Fmt::Stringsdict => "stringsdict",
        }
    }
}

fn code(u: &Unit, key: &str) -> Option<String> {
    u.hidden(key).map(str::to_string)
}

fn split_ws(s: &str) -> (&str, &str, &str) {
    let t = s.trim_start_matches(xml::is_space);
    if t.is_empty() {
        return (s, "", "");
    }
    let core = t.trim_end_matches(xml::is_space);
    (&s[..s.len() - t.len()], core, &t[core.len()..])
}

type Source = (Vec<Inline>, BTreeMap<String, CodeData>, usize);

/// The unit text of a value core: specifiers become tags. The last number is the length LocOS would count
/// (a token `⟦x1⟧` is three characters and the number's digits).
fn source_inlines(core: &str) -> Source {
    let mut inl = Vec::new();
    let mut codes = BTreeMap::new();
    let mut n = 0usize;
    let mut chars = 0usize;
    for piece in ph::split(core) {
        match piece {
            ph::Piece::Text(t) => {
                chars += t.chars().count();
                inl.push(Inline::text(t));
            }
            ph::Piece::Ph(p) => {
                n += 1;
                let id = n.to_string();
                chars += 3 + id.len();
                codes.insert(id.clone(), CodeData::new().with_standalone(p.to_string()).with_label("format specifier".to_string()));
                inl.push(if ph::is_positional(p) { Inline::free_tag(id, p) } else { Inline::tag(id, TagKind::Standalone, p) });
            }
        }
    }
    (merge_text(inl), codes, chars)
}

fn out_text(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, display, .. } => match u.codes.get(id).and_then(|c| c.standalone.as_deref()) {
                Some(c) => s.push_str(c),
                None => s.push_str(display),
            },
        }
    }
    s
}

fn has_target(u: &Unit) -> bool {
    u.segments.iter().any(|s| s.has_target())
}

/// The new full value of a translated unit, or `None` when the value stays as it is.
fn new_value(u: &Unit) -> Option<String> {
    if !u.translate || !has_target(u) {
        return None;
    }
    let text = out_text(u, &u.output());
    if text == code(u, OV).unwrap_or_default() {
        return None;
    }
    Some(format!("{}{}{}", code(u, LEAD).unwrap_or_default(), text, code(u, TRAIL).unwrap_or_default()))
}

/// How the file was read.
enum Origin {
    Xml { text: String, encoding: String, warnings: Vec<Warning> },
    Binary { raw: Vec<u8> },
}

fn check_declared_encoding(head: &str, fmt: &str) -> Result<(), Error> {
    let head: String = head.chars().take(200).collect();
    if !head.starts_with("<?xml") {
        return Ok(());
    }
    let decl = &head[..head.find('>').unwrap_or(head.len())];
    let Some(p) = decl.find("encoding") else { return Ok(()) };
    let rest = decl[p + 8..].trim_start_matches(xml::is_space);
    let Some(rest) = rest.strip_prefix('=') else { return Ok(()) };
    let rest = rest.trim_start_matches(xml::is_space);
    let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else { return Ok(()) };
    let val: String = rest[1..].chars().take_while(|c| *c != q).collect();
    if val.is_empty() {
        return Ok(());
    }
    let norm = val.trim().to_ascii_lowercase().replace('_', "-");
    if matches!(norm.as_str(), "utf-8" | "utf8" | "utf-16" | "utf-16le" | "utf-16be") {
        Ok(())
    } else {
        Err(xml::perr(fmt, "property list declares an encoding other than UTF-8/UTF-16"))
    }
}

fn read_any(fmt: Fmt, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<(Tree, Origin), Error> {
    guard::check_input(bytes, limits)?;
    if bytes.starts_with(b"bplist") {
        let tree = bin::read(bytes, fmt.id(), limits)?;
        return Ok((tree, Origin::Binary { raw: bytes.to_vec() }));
    }
    guard::check_xml(bytes, limits)?;
    let dec = enc::decode_text(bytes, &enc::DecodeOptions::with_limits(*limits).with_source_lang(opts.source_lang.as_deref()))?;
    if enc::is_legacy(&dec.encoding) {
        return Err(Error::Encoding("This property list is not valid UTF-8 or UTF-16 text. Re-save it as UTF-8 XML.".into()));
    }
    let head = dec.text.trim_start_matches(xml::is_space);
    if !head.starts_with('<') {
        return Err(xml::perr(
            fmt.id(),
            "not an XML or binary property list (old-style OpenStep plists are not supported; convert with `plutil -convert xml1`)",
        ));
    }
    check_declared_encoding(head, fmt.id())?;
    if dec.text.bytes().filter(|c| *c == b'<').count() > limits.max_units.saturating_mul(40) {
        return Err(xml::too_many(limits.max_units));
    }
    let tree = xml::scan(&dec.text, fmt.id(), limits)?;
    Ok((tree, Origin::Xml { text: dec.text, encoding: dec.encoding, warnings: dec.warnings }))
}

fn extract_impl(fmt: Fmt, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
    let (tree, origin) = read_any(fmt, bytes, opts, limits)?;
    let mode = if fmt == Fmt::Plist { policy::mode(&tree) } else { Mode::Generic };
    let leaves = policy::walk(&tree, limits.max_units)?;
    let mut units: Vec<Unit> = Vec::new();
    let mut parts: Vec<SkelPart> = Vec::new();
    let binary = matches!(origin, Origin::Binary { .. });
    if let Origin::Binary { raw } = &origin {
        parts.push(SkelPart::Raw(b64::encode(raw)));
    }
    let (mut cursor, mut line, mut counted_to, mut translatable_n) = (0usize, 1usize, 0usize, 0usize);
    for (ord0, leaf) in leaves.iter().enumerate() {
        let node = &tree.nodes[leaf.node];
        let (lead, core, trail) = split_ws(&node.text);
        if core.is_empty() || (!binary && node.cstart == NO_SPAN) {
            continue;
        }
        let translatable = policy::wanted(fmt == Fmt::Stringsdict, mode, leaf, &node.text);
        if !translatable && !opts.include_locked {
            continue;
        }
        let (inl, codes, chars) = source_inlines(core);
        if translatable {
            translatable_n += 1;
            guard::check_unit_count(translatable_n, limits)?;
            guard::check_unit_chars(chars, limits)?;
        }
        let mut u = Unit::new(format!("s{}", ord0 + 1), inl);
        u.codes = codes;
        u.context.kind = Some("string".into());
        u.context.key = Some(leaf.path.clone());
        if !leaf.near.is_empty() {
            u.context.style = Some(leaf.near.clone());
        }
        u.translate = translatable;
        if !translatable {
            u.lock_reason = Some(if ph::is_translatable(&node.text) { "not a translatable key".into() } else { "no translatable text".into() });
        }
        u.set_hidden(OV, core);
        if !lead.is_empty() {
            u.set_hidden(LEAD, lead);
        }
        if !trail.is_empty() {
            u.set_hidden(TRAIL, trail);
        }
        if let Origin::Xml { text, .. } = &origin {
            u.set_hidden(RAW, &text[node.cstart..node.cend]);
            line += text.as_bytes()[counted_to..node.cstart].iter().filter(|c| **c == b'\n').count();
            counted_to = node.cstart;
            u.context.location = Some(format!("line {line}"));
            if node.cstart > cursor {
                parts.push(SkelPart::Raw(text[cursor..node.cstart].to_string()));
            }
            cursor = node.cend;
        } else {
            u.set_hidden(ORD, (ord0 + 1).to_string());
        }
        parts.push(SkelPart::Unit(units.len()));
        units.push(u);
    }
    let (encoding, warnings) = match origin {
        Origin::Xml { text, encoding, warnings } => {
            if cursor < text.len() {
                parts.push(SkelPart::Raw(text[cursor..].to_string()));
            }
            (encoding, warnings)
        }
        Origin::Binary { .. } => (BINARY.to_string(), Vec::new()),
    };
    let doc = Document::new(fmt.id(), units, Skeleton::text(parts, encoding, bilingua_model::parser_info!())).with_options(opts);
    guard::finish(doc, warnings, limits)
}

/// Plural rule dictionaries (`NSStringPluralRuleType`): the dict nodes, found two levels below the root.
fn plural_vars(tree: &Tree) -> Vec<usize> {
    let mut out = Vec::new();
    let root = &tree.nodes[tree.root];
    if root.kind != Kind::Dict {
        return out;
    }
    for entry in &root.kids {
        let e = &tree.nodes[entry.node];
        if e.kind != Kind::Dict {
            continue;
        }
        for var in &e.kids {
            let v = &tree.nodes[var.node];
            if v.kind != Kind::Dict {
                continue;
            }
            if let Some(p) = tree.find_key(var.node, "NSStringFormatSpecTypeKey") {
                let spec = &tree.nodes[v.kids[p].node];
                if spec.kind == Kind::Str && spec.text == "NSStringPluralRuleType" {
                    out.push(var.node);
                }
            }
        }
    }
    out
}

/// Adds the target language's plural categories that the XML text lacks, before `other`, filled with the `other`
/// text (LocOS `build_localized`).
fn fill_xml(text: &str, locale: &str) -> Result<String, Error> {
    let Some(cats) = cldr::categories(locale) else { return Ok(text.to_string()) };
    let tree = xml::scan(text, "stringsdict", &Limits::default()).map_err(|e| Error::Write(format!("the merged file could not be read again: {e}")))?;
    let mut splices: Vec<(usize, String)> = Vec::new();
    for var in plural_vars(&tree) {
        let v = &tree.nodes[var];
        let Some(oi) = tree.find_key(var, "other") else { continue };
        let other = &tree.nodes[v.kids[oi].node];
        if other.kind != Kind::Str {
            continue;
        }
        let fill = xml::escape(&xml::target_text(&other.text));
        let okid = &v.kids[oi];
        let prev_end = if oi > 0 {
            tree.nodes[v.kids[oi - 1].node].end
        } else {
            text[v.start..].find('>').map_or(v.start, |p| v.start + p + 1)
        };
        let gap = &text[prev_end..okid.kstart];
        let gap = if !gap.is_empty() && gap.chars().all(xml::is_space) { gap } else { "\n" };
        let kv = &text[okid.kend..tree.nodes[okid.node].start];
        let kv = if kv.chars().all(xml::is_space) { kv } else { "" };
        for cat in &cats {
            if tree.find_key(var, cat).is_none() {
                splices.push((okid.kstart, format!("<key>{cat}</key>{kv}<string>{fill}</string>{gap}")));
            }
        }
    }
    splices.sort_by_key(|s| s.0);
    let mut out = String::with_capacity(text.len() + splices.iter().map(|s| s.1.len()).sum::<usize>());
    let mut pos = 0;
    for (at, piece) in splices {
        out.push_str(&text[pos..at]);
        out.push_str(&piece);
        pos = at;
    }
    out.push_str(&text[pos..]);
    Ok(out)
}

/// The same fill on a binary plist's tree.
fn fill_tree(tree: &mut Tree, locale: &str) {
    let Some(cats) = cldr::categories(locale) else { return };
    for var in plural_vars(tree) {
        let Some(oi) = tree.find_key(var, "other") else { continue };
        let other = &tree.nodes[tree.nodes[var].kids[oi].node];
        if other.kind != Kind::Str {
            continue;
        }
        let fill = other.text.clone();
        for cat in &cats {
            if tree.find_key(var, cat).is_none() {
                let at = tree.find_key(var, "other").unwrap_or(0);
                let idx = tree.nodes.len();
                tree.nodes.push(Node::string(fill.clone()));
                tree.nodes[var].kids.insert(at, Kid { key: Some((*cat).to_string()), kstart: NO_SPAN, kend: NO_SPAN, node: idx });
            }
        }
    }
}

/// Plural forms are filled for the target language when something was translated, or always with
/// `MergeOptions::stamp_target_always`.
fn wants_fill<'d>(fmt: Fmt, doc: &'d Document, opts: &MergeOptions) -> Option<&'d str> {
    opts.stamp_lang(doc, doc.units.iter().any(|u| u.translate && has_target(u))).filter(|_| fmt == Fmt::Stringsdict)
}

fn merge_impl(fmt: Fmt, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write(format!("{} needs a text skeleton", fmt.id())));
    };
    let unit = |i: usize| doc.units.get(i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")));
    if encoding == BINARY {
        let Some(SkelPart::Raw(b)) = parts.first() else { return Err(Error::Write("binary plist skeleton lost the original file".into())) };
        let original = b64::decode(b).map_err(|e| Error::Write(format!("binary plist skeleton is damaged: {e}")))?;
        let mut tree = bin::read(&original, fmt.id(), &Limits::default())?;
        let leaves: Vec<Leaf> = policy::walk(&tree, Limits::default().max_units)?;
        let mut changed = false;
        for p in parts {
            let SkelPart::Unit(i) = p else { continue };
            let u = unit(*i)?;
            let Some(v) = new_value(u) else { continue };
            let ord: usize = code(u, ORD).and_then(|s| s.parse().ok()).unwrap_or(0);
            let leaf = ord.checked_sub(1).and_then(|k| leaves.get(k)).ok_or_else(|| Error::Write(format!("unit {} points at no string of the file", u.id)))?;
            tree.nodes[leaf.node].text = xml::target_text(&v);
            changed = true;
        }
        if let Some(lang) = wants_fill(fmt, doc, opts) {
            let before = tree.nodes.len();
            fill_tree(&mut tree, lang);
            changed |= tree.nodes.len() != before;
        }
        return Ok(if changed { bin::write(&tree) } else { original });
    }
    let mut out = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => out.push_str(s),
            SkelPart::Unit(i) => {
                let u = unit(*i)?;
                match new_value(u) {
                    Some(v) => out.push_str(&xml::escape(&xml::target_text(&v))),
                    None => out.push_str(&code(u, RAW).unwrap_or_default()),
                }
            }
        }
    }
    if let Some(lang) = wants_fill(fmt, doc, opts) {
        out = fill_xml(&out, lang)?;
    }
    enc::encode_text_with(&out, encoding, opts)
}

fn head_text(bytes: &[u8], n: usize) -> String {
    let head = &bytes[..bytes.len().min(n)];
    if head.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    String::from_utf8_lossy(head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head)).into_owned()
}

fn is_plist(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"bplist00") {
        return true;
    }
    let h = head_text(bytes, 4096);
    h.trim_start_matches(xml::is_space).starts_with('<') && h.contains("<plist")
}

const STRINGSDICT_MARKS: [&str; 4] = ["NSStringPluralRuleType", "NSStringLocalizedFormatKey", "NSStringDeviceSpecificRuleType", "NSStringVariableWidthRuleType"];

fn is_stringsdict(bytes: &[u8]) -> bool {
    if !is_plist(bytes) {
        return false;
    }
    if bytes.starts_with(b"bplist00") {
        return STRINGSDICT_MARKS.iter().any(|m| bytes.windows(m.len()).any(|w| w == m.as_bytes()));
    }
    let h = head_text(bytes, 64 * 1024);
    STRINGSDICT_MARKS.iter().any(|m| h.contains(m))
}

macro_rules! filter_impl {
    ($ty:ident, $fmt:expr, $ext:literal, $sniff:ident) => {
        impl $ty {
            /// Like [`Filter::extract`] with explicit limits.
            pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
                extract_impl($fmt, bytes, opts, limits)
            }
        }

        impl Filter for $ty {
            fn id(&self) -> &'static str {
                $fmt.id()
            }
            fn extensions(&self) -> &'static [&'static str] {
                &[$ext]
            }
            fn sniff(&self, bytes: &[u8]) -> bool {
                $sniff(bytes)
            }
            fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
                bilingua_model::check_own_options(self, &opts.format_options)?;
                extract_impl($fmt, bytes, opts, &opts.limits())
            }
            fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
                bilingua_model::check_own_options(self, &opts.format_options)?;
                merge_impl($fmt, doc, opts)
            }
        }
    };
}

filter_impl!(Plist, Fmt::Plist, "plist", is_plist);
filter_impl!(Stringsdict, Fmt::Stringsdict, "stringsdict", is_stringsdict);

/// One entry of a file as a key-based project sees it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Record {
    /// A plist: the path of the value. A stringsdict: the entry key.
    pub key: String,
    pub value: String,
    /// A stringsdict entry with several variables: the variable's name for the second and later records.
    pub context: String,
    /// A stringsdict variable: its string forms (`zero one other`, device names, ...) in file order.
    pub plural_forms: Vec<(String, String)>,
    /// For each of those forms that is a unit, the unit's index among the file's translatable units.
    pub plural_units: Vec<(String, usize)>,
    /// Index of the entry's unit among the translatable units; `None` when it has none.
    pub unit_index: Option<usize>,
}

fn unit_index(fmt: Fmt, tree: &Tree, leaves: &[Leaf]) -> HashMap<usize, usize> {
    let mode = if fmt == Fmt::Plist { policy::mode(tree) } else { Mode::Generic };
    let mut map = HashMap::new();
    for l in leaves {
        if policy::wanted(fmt == Fmt::Stringsdict, mode, l, &tree.nodes[l.node].text) {
            let k = map.len();
            map.insert(l.node, k);
        }
    }
    map
}

/// Every string value of a plist as a record (key = its path joined with `/`).
pub fn plist_records(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    let (tree, _) = read_any(Fmt::Plist, bytes, &ExtractOptions::default(), &Limits::default())?;
    let leaves = policy::walk(&tree, Limits::default().max_units)?;
    let idx = unit_index(Fmt::Plist, &tree, &leaves);
    Ok(leaves
        .iter()
        .map(|l| Record { key: l.path.clone(), value: tree.nodes[l.node].text.clone(), unit_index: idx.get(&l.node).copied(), ..Default::default() })
        .collect())
}

/// One record per stringsdict entry (a plural variable per record, device and width variants as forms).
pub fn stringsdict_records(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    let (tree, _) = read_any(Fmt::Stringsdict, bytes, &ExtractOptions::default(), &Limits::default())?;
    let leaves = policy::walk(&tree, Limits::default().max_units)?;
    let idx = unit_index(Fmt::Stringsdict, &tree, &leaves);
    let root = &tree.nodes[tree.root];
    let mut out = Vec::new();
    if root.kind != Kind::Dict {
        return Ok(out);
    }
    let skip = ["NSStringFormatSpecTypeKey", "NSStringFormatValueTypeKey"];
    for ek in &root.kids {
        let ekey = ek.key.clone().unwrap_or_default();
        let entry = &tree.nodes[ek.node];
        match entry.kind {
            Kind::Str => {
                out.push(Record { key: ekey, value: entry.text.clone(), unit_index: idx.get(&ek.node).copied(), ..Default::default() });
                continue;
            }
            Kind::Dict => {}
            _ => continue,
        }
        let (mut fmt_value, mut fmt_idx) = (String::new(), None);
        if let Some(p) = tree.find_key(ek.node, "NSStringLocalizedFormatKey") {
            let fk = &tree.nodes[entry.kids[p].node];
            if fk.kind == Kind::Str {
                fmt_value = fk.text.clone();
                fmt_idx = idx.get(&entry.kids[p].node).copied();
            }
        }
        struct Variant {
            name: String,
            forms: Vec<(String, String)>,
            first: Option<usize>,
            units: Vec<(String, usize)>,
        }
        let mut variants: Vec<Variant> = Vec::new();
        for vk in &entry.kids {
            let vname = vk.key.clone().unwrap_or_default();
            let var = &tree.nodes[vk.node];
            if vname == "NSStringLocalizedFormatKey" || var.kind != Kind::Dict {
                continue;
            }
            let mut forms = Vec::new();
            let mut units = Vec::new();
            for c in &var.kids {
                let k = c.key.clone().unwrap_or_default();
                let cn = &tree.nodes[c.node];
                if cn.kind == Kind::Str && !skip.contains(&k.as_str()) {
                    forms.push((k.clone(), cn.text.clone()));
                    if let Some(i) = idx.get(&c.node) {
                        units.push((k, *i));
                    }
                }
            }
            let first = var.kids.iter().find_map(|c| idx.get(&c.node).copied());
            variants.push(Variant { name: vname, forms, first, units });
        }
        if variants.is_empty() {
            out.push(Record { key: ekey, value: fmt_value, unit_index: fmt_idx, ..Default::default() });
            continue;
        }
        for (n, v) in variants.into_iter().enumerate() {
            out.push(Record {
                key: ekey.clone(),
                value: fmt_value.clone(),
                context: if n == 0 { String::new() } else { v.name },
                plural_forms: v.forms,
                plural_units: v.units,
                unit_index: fmt_idx.or(v.first),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
